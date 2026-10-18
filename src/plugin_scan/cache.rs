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
    /// What the file at `path` is like now.
    pub fn of(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
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

/// What a scan came to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CachedOutcome {
    Scanned(LibraryScan),
    Failed(ScanFailure),
}

#[derive(Serialize, Deserialize)]
struct CacheDocument {
    /// (`SCAN_FORMAT` when it was written: a file with another one is not believed.)
    format: u32,
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
        crate::settings_file::default_settings_file_path()
            .and_then(|settings| Some(settings.parent()?.join(CACHE_FILE_NAME)))
    }

    /// What the scan of the library at `path` came to, **if the file is still the one that was scanned** (it has the size and the time that it had).
    pub fn lookup(&self, path: &Path, now: &FileFingerprint) -> Option<&CachedOutcome> {
        let (then, outcome) = self.entries.get(path)?;
        (then == now).then_some(outcome)
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
