// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of the folders of plugins: each library goes to the scan program unless the cache already knows what the file is like. This runs on the thread it is called on,
//! which for the editor will be a background thread (the scan of a hundred libraries takes seconds and never blocks the window).

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

use super::cache::{CachedOutcome, FileFingerprint, ScanCache};
use super::child::{SCAN_TIME_LIMIT, scan_in_a_program};
use super::failure::ScanFailure;
use super::folders::{clap_libraries_in, dependency_search_folders, libraries_in, library_search_variable};
use ladspa_host::LibraryScan;

/// What the scan of a library came to.
#[derive(Clone, Debug, PartialEq)]
pub enum LibraryOutcome {
    Scanned(LibraryScan),
    /// A CLAP library was scanned.
    ScannedClap(clap_host::scan::LibraryScan),
    Failed(ScanFailure),
}

impl From<LibraryOutcome> for CachedOutcome {
    fn from(outcome: LibraryOutcome) -> Self {
        match outcome {
            LibraryOutcome::Scanned(scan) => Self::Scanned(scan),
            LibraryOutcome::ScannedClap(scan) => Self::ScannedClap(scan),
            LibraryOutcome::Failed(failure) => Self::Failed(failure),
        }
    }
}

impl From<CachedOutcome> for LibraryOutcome {
    fn from(outcome: CachedOutcome) -> Self {
        match outcome {
            CachedOutcome::Scanned(scan) => Self::Scanned(scan),
            CachedOutcome::ScannedClap(scan) => Self::ScannedClap(scan),
            CachedOutcome::Failed(failure) => Self::Failed(failure),
        }
    }
}

/// A library and what came of the scan of it.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryReport {
    pub path: PathBuf,
    pub outcome: LibraryOutcome,
    /// The scan program was not run: the library is the file that it was when it was scanned, and that scan was kept.
    pub from_cache: bool,
}

/// Why a library could not be scanned **for a reason that is not the library's** (it is not failed, and the next scan will try again).
#[derive(Debug, Error)]
pub enum ScannerProblem {
    #[error("the scan program {program} could not be run: {error}")]
    CouldNotRunTheScanner { program: PathBuf, error: io::Error },
    #[error("the library {path} could not be looked at: {error}")]
    CouldNotLookAtTheLibrary { path: PathBuf, error: io::Error },
}

/// What the scan of some folders found.
#[derive(Debug, Default)]
pub struct FolderScan {
    /// The libraries, folder by folder in the order of the folders, and in the folder in the order of the names of the files.
    pub libraries: Vec<LibraryReport>,
    /// What went wrong that was not the fault of a library: a folder that cannot be read, the scan program that cannot be run.
    pub problems: Vec<String>,
}

/// Scans libraries with a program of its own, and keeps what it finds.
#[derive(Debug)]
pub struct PluginScanner {
    program: PathBuf,
    time_limit: Duration,
    cache: ScanCache,
    environment: Vec<(String, String)>,
    /// The folders a person said have the libraries that plugin libraries need, and whether the parent folders of a plugin library are looked in too.
    dependency_folders: Vec<PathBuf>,
    search_parent_folders: bool,
}

impl PluginScanner {
    /// A scanner that runs `program` (see [`super::scanner_program_beside_this_program`]) and keeps what it finds in `cache`.
    pub fn new(program: PathBuf, cache: ScanCache) -> Self {
        Self {
            program,
            time_limit: SCAN_TIME_LIMIT,
            cache,
            environment: Vec::new(),
            dependency_folders: Vec::new(),
            search_parent_folders: false,
        }
        .looking_for_dependencies_in(Vec::new(), false)
    }

    /// The same, with another time limit for the scan of one library (for the tests, which do not want to wait five seconds).
    #[must_use]
    pub fn with_time_limit(mut self, time_limit: Duration) -> Self {
        self.time_limit = time_limit;
        self
    }

    /// The same, with these folders to look in for the libraries that a plugin library needs (a plugin library that needs one that is in none of them, or beside it, fails
    /// to load), and with `search_parent_folders` the folders above the plugin library too, after them. The scans that failed for want of a library are not believed any more
    /// if this is not what it was when they were made.
    #[must_use]
    pub fn looking_for_dependencies_in(mut self, folders: Vec<PathBuf>, search_parent_folders: bool) -> Self {
        let signature = format!(
            "{}|parents={search_parent_folders}",
            folders
                .iter()
                .map(|folder| folder.display().to_string())
                .collect::<Vec<_>>()
                .join("|")
        );
        self.cache.set_search_signature(&signature);
        self.dependency_folders = folders;
        self.search_parent_folders = search_parent_folders;
        self
    }

    /// The environment variables the scan program is started with for `library`: those that were asked for, and the variable that lists where libraries are looked for with
    /// the folders to look in put before what it has already.
    pub(super) fn environment_for(&self, library: &Path) -> Vec<(String, String)> {
        let mut environment = self.environment.clone();
        let folders = dependency_search_folders(library, &self.dependency_folders, self.search_parent_folders);
        if !folders.is_empty() {
            let variable = library_search_variable();
            let existing = std::env::var_os(variable).map(|value| std::env::split_paths(&value).collect::<Vec<_>>());
            let all = folders.into_iter().chain(existing.into_iter().flatten());
            if let Some(value) = std::env::join_paths(all)
                .ok()
                .and_then(|value| value.into_string().ok())
            {
                environment.retain(|(name, _)| name != variable);
                environment.push((variable.to_owned(), value));
            }
        }
        environment
    }

    /// The same, with an environment variable that the scan program is started with, besides those of the editor. (For the tests: the test plugins are asked to misbehave
    /// through one, and it is the scan program's alone, so that the tests that do it do not have to change the editor's own.)
    #[must_use]
    pub fn with_environment_variable(mut self, name: &str, value: &str) -> Self {
        self.environment.push((name.to_owned(), value.to_owned()));
        self
    }

    /// How long the scan of one library may take.
    pub fn time_limit(&self) -> Duration {
        self.time_limit
    }

    pub fn cache(&self) -> &ScanCache {
        &self.cache
    }

    /// What the scan of the library at `path` comes to: what the cache has if the file is the one that was scanned, and otherwise what the scan program says (which is then
    /// kept in the cache, but not written to its file: see [`Self::scan_folders`]).
    pub fn scan_library(&mut self, path: &Path) -> Result<LibraryReport, ScannerProblem> {
        let fingerprint = FileFingerprint::of(path).map_err(|error| ScannerProblem::CouldNotLookAtTheLibrary {
            path: path.to_owned(),
            error,
        })?;
        if let Some(known) = self.cache.lookup(path, &fingerprint) {
            return Ok(LibraryReport {
                path: path.to_owned(),
                outcome: known.clone().into(),
                from_cache: true,
            });
        }
        let outcome =
            scan_in_a_program(&self.program, path, self.time_limit, &self.environment_for(path)).map_err(|error| {
                ScannerProblem::CouldNotRunTheScanner {
                    program: self.program.clone(),
                    error,
                }
            })?;
        self.cache.record(path, fingerprint, outcome.clone().into());
        Ok(LibraryReport {
            path: path.to_owned(),
            outcome,
            from_cache: false,
        })
    }

    /// Scans every library in `folders` (each is looked in alone), and writes the cache. A library whose scan could not be run is a problem and not a failure.
    pub fn scan_folders(&mut self, folders: &[PathBuf]) -> FolderScan {
        let mut scan = FolderScan::default();
        let mut found = Vec::new();
        for folder in folders {
            // (The LADSPA libraries of the folder, and then the CLAP ones in it and below it.)
            let libraries = match libraries_in(folder).and_then(|mut found| {
                found.extend(clap_libraries_in(folder)?);
                Ok(found)
            }) {
                Ok(libraries) => libraries,
                Err(error) => {
                    scan.problems
                        .push(format!("the folder {} could not be read: {error}", folder.display()));
                    continue;
                }
            };
            for library in libraries {
                found.push(library.clone());
                match self.scan_library(&library) {
                    Ok(report) => scan.libraries.push(report),
                    Err(problem) => scan.problems.push(problem.to_string()),
                }
            }
        }
        // (A library that is not in any of the folders any more is not kept: if it comes back it is scanned again.)
        self.cache.forget_all_but(&found);
        self.cache.save();
        if let Some(problem) = self.cache.last_problem() {
            scan.problems.push(problem.to_owned());
        }
        scan
    }

    /// Fails the library at `path` as it is now, with `failure`, and writes the cache: it is not tried again until the file changes. (For a library that the editor itself found
    /// out is bad, after it had been scanned well: the breadcrumb of a run that ended badly.)
    pub fn fail_library(&mut self, path: &Path, failure: ScanFailure) {
        if let Ok(fingerprint) = FileFingerprint::of(path) {
            self.cache.record(path, fingerprint, CachedOutcome::Failed(failure));
            self.cache.save();
        }
    }
}
