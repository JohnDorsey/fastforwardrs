// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The breadcrumb (G4 of the design): **plugins run in the editor's own process** (all but the instruments, since phase 14, which run in a program of their own and are not written here:
//! the editor never loads them), so a plugin that crashes while it runs takes the editor with it, and nothing can be done about that while it happens. What is done is this: before a library is used for the first time in a session its path is written to a file (and flushed to the disk), and when
//! the editor ends well the file is removed. If the next start finds the file, the last run ended badly, and the libraries in the file are the ones it was using. The library that
//! was started last is **failed** (like one that crashed the scan: not loaded again until it changes), and the others are named.
//!
//! Nothing writes a breadcrumb yet in piece 7.4: plugins are not run in the editor until piece 7.6. What is here is the whole of the mechanism, tested.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::failure::ScanFailure;
use super::scanner::PluginScanner;
use crate::atomic_file::write_file_atomically;

/// What the file is called, beside the settings file.
const BREADCRUMB_FILE_NAME: &str = "plugins_in_use.json";

#[derive(Serialize, Deserialize, Default)]
struct BreadcrumbDocument {
    /// The libraries that were used this session, in the order they were first used.
    #[serde(default)]
    libraries: Vec<String>,
}

/// The file that says which libraries this session is using.
#[derive(Debug)]
pub struct Breadcrumb {
    file: PathBuf,
    used: Vec<PathBuf>,
}

/// What a run that ended badly was doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovery {
    /// The library that was started last, which is now failed.
    pub failed: PathBuf,
    /// The others that were in use.
    pub also_in_use: Vec<PathBuf>,
}

impl Breadcrumb {
    /// A breadcrumb that is written to `file`. (This does not touch an old file: read what the last run left with [`recover`] first.)
    pub fn at(file: PathBuf) -> Self {
        Self { file, used: Vec::new() }
    }

    /// Where the breadcrumb is kept: beside the settings file. `None` if this system has no settings folder.
    pub fn default_file() -> Option<PathBuf> {
        crate::settings_file::default_settings_file_path()
            .and_then(|settings| Some(settings.parent()?.join(BREADCRUMB_FILE_NAME)))
    }

    /// Says that the library at `library` is about to be used for the first time in this session. **It returns when the file has been written and flushed**, which is the whole
    /// of what it is for: call it before the library is loaded. A library that was said before is not said again. An error means that the file could not be written; the
    /// caller decides whether to go on without it.
    pub fn note_use_of(&mut self, library: &Path) -> io::Result<()> {
        if self.used.iter().any(|used| used == library) {
            return Ok(());
        }
        let mut libraries = self.used.clone();
        libraries.push(library.to_owned());
        let document = BreadcrumbDocument {
            libraries: libraries
                .iter()
                .filter_map(|path| path.to_str().map(str::to_owned))
                .collect(),
        };
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder)?;
        }
        write_file_atomically(&self.file, &serde_json::to_string(&document).map_err(io::Error::other)?)?;
        self.used = libraries;
        Ok(())
    }

    /// The editor is ending well: the file is removed. (This is not done by dropping the breadcrumb: a program that panics drops what it has, and has not ended well.)
    pub fn ended_well(&mut self) {
        self.used.clear();
        let _ = fs::remove_file(&self.file);
    }

    /// The libraries this session has said it is using.
    pub fn used(&self) -> &[PathBuf] {
        &self.used
    }
}

/// Looks at what the last run left at `file`: if it left a breadcrumb, the library that it started last is failed in `scanner`, the file is removed, and what was done is given.
/// Nothing was left: `None`. (A file that cannot be understood is removed and says nothing: a person cannot do anything with "something is wrong with a file that is only
/// there to help".)
pub fn recover(file: &Path, scanner: &mut PluginScanner) -> Option<Recovery> {
    let text = fs::read_to_string(file).ok()?;
    let _ = fs::remove_file(file);
    let document: BreadcrumbDocument = serde_json::from_str(&text).ok()?;
    let mut libraries: Vec<PathBuf> = document.libraries.into_iter().map(PathBuf::from).collect();
    let failed = libraries.pop()?;
    scanner.fail_library(
        &failed,
        ScanFailure::WasInUseWhenTheProgramEndedBadly {
            others: libraries.len(),
        },
    );
    Some(Recovery {
        failed,
        also_in_use: libraries,
    })
}
