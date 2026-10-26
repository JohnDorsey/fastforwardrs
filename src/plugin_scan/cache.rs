// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the scans found, kept in a file beside the settings (`plugin_scan_cache.json`), so that the libraries do not have to be loaded again at every start of the program,
//! and so that **a library that failed is not tried again until it changes**. A library has changed when its size or its time of last change is not what it was when it was
//! scanned. (The size and the time are read from the directory; reading every library to hash it at every start would cost more than scanning the ones that did not change.)

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use ladspa_host::LibraryScan;
use serde::{Deserialize, Serialize};

use super::failure::ScanFailure;
use super::protocol::SCAN_FORMAT;
use crate::atomic_file::write_file_atomically;

/// What the file of scans is called, beside the settings file.
const CACHE_FILE_NAME: &str = "plugin_scan_cache.json";

/// What a file was like when it was scanned: if it is not like this any more, the scan is not believed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub size: u64,
    pub modified_seconds: u64,
    pub modified_nanoseconds: u32,
}

impl FileFingerprint {
    /// What the file at `path` is like now. **For a bundle (a CLAP library on macOS is a folder) it is the file inside that is looked at**: the time of a folder does not change
    /// when a file deep in it does.
    pub fn of(path: &Path) -> io::Result<Self> {
        // (An LV2 bundle is a folder of many files, and any may change: its size is theirs together and its time the latest of theirs.)
        if path.join("manifest.ttl").is_file() {
            return folder_fingerprint(path);
        }
        let metadata = fs::metadata(bundle_executable(path).as_deref().unwrap_or(path))?;
        let since_epoch = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .unwrap_or_default();
        Ok(Self {
            size: metadata.len(),
            modified_seconds: since_epoch.as_secs(),
            modified_nanoseconds: since_epoch.subsec_nanos(),
        })
    }
}

/// How many files of a folder are looked at to tell whether it has changed, and how many folders down: what is past is not looked at (a bundle that has more is a bundle of data).
const MOST_FILES_OF_A_BUNDLE_LOOKED_AT: usize = 5000;
const MOST_FOLDERS_DOWN_IN_A_BUNDLE: usize = 4;

/// What a folder is like: the sizes of its files added up, and the time of the latest change of any (and of the folders), down to a few folders and a few thousand files.
fn folder_fingerprint(folder: &Path) -> io::Result<FileFingerprint> {
    fn walk(
        folder: &Path,
        down: usize,
        looked_at: &mut usize,
        size: &mut u64,
        latest: &mut std::time::SystemTime,
    ) -> io::Result<()> {
        for entry in fs::read_dir(folder)? {
            let Ok(entry) = entry else { continue };
            let Ok(metadata) = entry.metadata() else { continue };
            *looked_at += 1;
            if *looked_at > MOST_FILES_OF_A_BUNDLE_LOOKED_AT {
                return Ok(());
            }
            if let Ok(modified) = metadata.modified() {
                *latest = (*latest).max(modified);
            }
            if metadata.is_dir() {
                if down > 0 {
                    let _ = walk(&entry.path(), down - 1, looked_at, size, latest);
                }
            } else {
                *size += metadata.len();
            }
        }
        Ok(())
    }
    let (mut size, mut latest, mut looked_at) = (0_u64, UNIX_EPOCH, 0_usize);
    walk(
        folder,
        MOST_FOLDERS_DOWN_IN_A_BUNDLE,
        &mut looked_at,
        &mut size,
        &mut latest,
    )?;
    let since_epoch = latest.duration_since(UNIX_EPOCH).unwrap_or_default();
    Ok(FileFingerprint {
        size,
        modified_seconds: since_epoch.as_secs(),
        modified_nanoseconds: since_epoch.subsec_nanos(),
    })
}

/// The file that is the library inside the bundle `path` (`Name.clap/Contents/MacOS/<the one file there>`), if `path` is a folder that has one.
fn bundle_executable(path: &Path) -> Option<PathBuf> {
    let mut files = fs::read_dir(path.join("Contents").join("MacOS"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|file| file.is_file());
    let first = files.next()?;
    // (A bundle with more than one file there is not one whose library can be told: the folder itself is looked at.)
    files.next().is_none().then_some(first)
}

/// What a scan came to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CachedOutcome {
    Scanned(LibraryScan),
    /// A CLAP library that was scanned.
    ScannedClap(clap_host::scan::LibraryScan),
    /// An LV2 bundle that was scanned.
    ScannedLv2(lv2_host::BundleScan),
    Failed(ScanFailure),
}

#[derive(Serialize, Deserialize)]
struct CacheDocument {
    /// (`SCAN_FORMAT` when it was written: a file with another one is not believed.)
    format: u32,
    /// Where the libraries that plugin libraries need were looked for when the scans were made (see [`ScanCache::set_search_signature`]).
    #[serde(default)]
    search: String,
    #[serde(default)]
    libraries: Vec<LibraryDocument>,
}

#[derive(Serialize, Deserialize)]
struct LibraryDocument {
    path: String,
    #[serde(flatten)]
    fingerprint: FileFingerprint,
    outcome: CachedOutcome,
}

/// The scans that were made, by the path of the library. It lives in a file (or only in memory, for a computer with no settings folder and for the tests); a file that cannot
/// be read is an empty cache, so that the worst that can happen is that the libraries are scanned again, and **a file that cannot be written is said**
/// ([`Self::last_problem`]).
#[derive(Debug, Default)]
pub struct ScanCache {
    file: Option<PathBuf>,
    entries: BTreeMap<PathBuf, (FileFingerprint, CachedOutcome)>,
    search_signature: String,
    last_problem: Option<String>,
}

impl ScanCache {
    /// A cache that is not kept anywhere.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// The cache that is kept in `file`, with what is in it now.
    pub fn at(file: PathBuf) -> Self {
        let mut cache = Self {
            file: Some(file.clone()),
            ..Self::default()
        };
        match fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<CacheDocument>(&text) {
                // (A file of another format is from another version of the scan: its answers mean something else.)
                Ok(document) if document.format == SCAN_FORMAT => {
                    cache.search_signature = document.search;
                    for library in document.libraries {
                        cache
                            .entries
                            .insert(PathBuf::from(library.path), (library.fingerprint, library.outcome));
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    cache.last_problem = Some(format!(
                        "the file of what the scans of plugin libraries found, {}, cannot be read ({error}), so the libraries will be scanned again",
                        file.display()
                    ));
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                cache.last_problem = Some(format!(
                    "the file of what the scans of plugin libraries found, {}, cannot be read ({error}), so the libraries will be scanned again",
                    file.display()
                ));
            }
        }
        cache
    }

    /// Where the cache is kept: beside the settings file. `None` if this system has no settings folder.
    pub fn default_file() -> Option<PathBuf> {
        crate::settings_file::default_settings_file_path().and_then(|settings| Self::file_beside(&settings))
    }

    /// Where the cache is kept when the settings file is `settings_file`: beside it.
    pub fn file_beside(settings_file: &Path) -> Option<PathBuf> {
        Some(settings_file.parent()?.join(CACHE_FILE_NAME))
    }

    /// What the scan of the library at `path` came to, **if the file is still the one that was scanned** (it has the size and the time that it had).
    pub fn lookup(&self, path: &Path, now: &FileFingerprint) -> Option<&CachedOutcome> {
        let (then, outcome) = self.entries.get(path)?;
        (then == now).then_some(outcome)
    }

    /// Says where the libraries that plugin libraries need are looked for now (a text that is different when the folders or the choice to look in the parent folders are). **The
    /// scans that failed because a library could not be loaded or was not a plugin library are not believed any more if it is not what it was when they were made**: the
    /// library that was not found may be found now. (The scans that went well, and those that failed in the library's own doing, a crash or a wrong answer, stay.)
    pub fn set_search_signature(&mut self, signature: &str) {
        if self.search_signature == signature {
            return;
        }
        self.entries.retain(|_, (_, outcome)| {
            !matches!(
                outcome,
                CachedOutcome::Failed(
                    ScanFailure::CouldNotBeLoaded(_)
                        | ScanFailure::NotALadspaLibrary(_)
                        | ScanFailure::NotAClapLibrary(_)
                )
            )
        });
        self.search_signature = signature.to_owned();
    }

    /// Remembers what the scan of the library at `path`, which was like `fingerprint`, came to. (In memory: [`Self::save`] writes the file.)
    pub fn record(&mut self, path: &Path, fingerprint: FileFingerprint, outcome: CachedOutcome) {
        self.entries.insert(path.to_owned(), (fingerprint, outcome));
    }

    /// Forgets the libraries whose paths are not in `kept` (they are gone, or no folder that is looked in has them any more).
    pub fn forget_all_but(&mut self, kept: &[PathBuf]) {
        self.entries.retain(|path, _| kept.contains(path));
    }

    /// How many scans are kept.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Writes the file, if the cache has one. A failure is kept for [`Self::last_problem`] and does not stop anything (the scans are still in memory).
    pub fn save(&mut self) {
        let Some(file) = &self.file else { return };
        let document = CacheDocument {
            format: SCAN_FORMAT,
            search: self.search_signature.clone(),
            libraries: self
                .entries
                .iter()
                .filter_map(|(path, (fingerprint, outcome))| {
                    Some(LibraryDocument {
                        path: path.to_str()?.to_owned(),
                        fingerprint: *fingerprint,
                        outcome: outcome.clone(),
                    })
                })
                .collect(),
        };
        let written = serde_json::to_string_pretty(&document)
            .map_err(io::Error::other)
            .and_then(|text| {
                if let Some(folder) = file.parent() {
                    fs::create_dir_all(folder)?;
                }
                write_file_atomically(file, &text)
            });
        self.last_problem = written.err().map(|error| {
            format!(
                "the file of what the scans of plugin libraries found, {}, cannot be written ({error}), so the libraries will be scanned again next time",
                file.display()
            )
        });
    }

    /// What went wrong with the file the last time it was read or written, if anything did.
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }
}
