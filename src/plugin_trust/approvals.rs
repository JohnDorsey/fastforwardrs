// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The folders that a person has said plugins may be run from: **remembered by the folder** (SF-05, changed on 2026-10-19 at the maintainer's word: *"I don't want individual
//! security questions for every locally installed plugin; ask once for each new folder a plugin or library comes from"*). Before that a linked plugin was remembered by its path and
//! a hash of the file, and a file that changed was asked about again; **now a folder that has been said yes to is a folder whose plugins are run without asking**, whatever is put in
//! it later, so a person who says yes to a folder says yes to what is in it and to what comes to be in it.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file::write_file_atomically;

/// What the file of approvals is called, beside the settings file.
const APPROVALS_FILE_NAME: &str = "plugin_approvals.json";

/// What a folder is called in the file and in the sets: the folder as the system names it (links followed, every part spelled as the disk spells it), so that the same folder
/// reached in two ways is one folder. A folder that cannot be looked at is the folder as it was given, made absolute if it can be.
pub fn folder_key(folder: &Path) -> PathBuf {
    fs::canonicalize(folder)
        .or_else(|_| std::path::absolute(folder))
        .unwrap_or_else(|_| folder.to_owned())
}

/// The folder that the file `file` is in, as [`folder_key`] names it.
pub fn folder_of(file: &Path) -> PathBuf {
    let absolute = fs::canonicalize(file)
        .or_else(|_| std::path::absolute(file))
        .unwrap_or_else(|_| file.to_owned());
    absolute.parent().map_or_else(|| absolute.clone(), Path::to_path_buf)
}

/// A folder as a person should read it: the way Windows names a path that is not limited in length (`\\?\C:\...`) is not.
pub fn folder_for_a_person(folder: &Path) -> String {
    let text = folder.display().to_string();
    text.strip_prefix(r"\\?\").map_or(text.clone(), str::to_owned)
}

#[derive(Serialize, Deserialize, Default)]
struct ApprovalsDocument {
    /// (The file of the first version of this had `approved`, plugin files by path and hash. Those are not read: a person is asked about the folder instead.)
    #[serde(default)]
    approved_folders: Vec<String>,
}

/// The folders that have been approved. It lives in a file (or only in memory, for a computer with no settings folder, and for the tests); a file that cannot be read is an
/// empty list, so that the worst that can happen to a person is that they are asked again, and **a file that cannot be written is said** ([`Self::last_problem`]).
#[derive(Debug, Default)]
pub struct ApprovalStore {
    file: Option<PathBuf>,
    approved: HashSet<PathBuf>,
    last_problem: Option<String>,
}

impl ApprovalStore {
    /// A store that is not kept anywhere.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// The store that is kept in `file`, with what is in it now.
    pub fn at(file: PathBuf) -> Self {
        let mut store = Self {
            file: Some(file.clone()),
            ..Self::default()
        };
        let problem = |error: &dyn std::fmt::Display| {
            format!(
                "the file of the folders that plugins may be run from, {}, cannot be read ({error}), so they will be asked about again",
                file.display()
            )
        };
        match fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<ApprovalsDocument>(&text) {
                Ok(document) => {
                    store
                        .approved
                        .extend(document.approved_folders.into_iter().map(PathBuf::from));
                }
                Err(error) => store.last_problem = Some(problem(&error)),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => store.last_problem = Some(problem(&error)),
        }
        store
    }

    /// Where the file of approvals is kept: beside the settings file. `None` if this system has no settings folder.
    pub fn default_file() -> Option<PathBuf> {
        crate::settings_file::default_settings_file_path()
            .and_then(|settings| Some(settings.parent()?.join(APPROVALS_FILE_NAME)))
    }

    /// Whether plugins may be run from `folder` (a folder that was said yes to; not the folders in it, which are others).
    pub fn is_folder_approved(&self, folder: &Path) -> bool {
        self.approved.contains(&folder_key(folder))
    }

    /// The folders that have been approved, in the order of their names.
    pub fn approved_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = self.approved.iter().cloned().collect();
        folders.sort();
        folders
    }

    /// How many folders have been approved.
    pub fn len(&self) -> usize {
        self.approved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.approved.is_empty()
    }

    /// Remembers that plugins may be run from `folder`, and writes the file again. (If it cannot be written the approval holds until the program ends, and
    /// [`Self::last_problem`] says so.)
    pub fn approve_folder(&mut self, folder: &Path) {
        self.approved.insert(folder_key(folder));
        self.save();
    }

    /// What last went wrong with the file, if anything did.
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    fn save(&mut self) {
        let Some(file) = self.file.clone() else {
            return;
        };
        let mut approved_folders: Vec<String> = self
            .approved
            .iter()
            .filter_map(|folder| folder.to_str().map(str::to_owned))
            .collect();
        approved_folders.sort();
        let text = match serde_json::to_string_pretty(&ApprovalsDocument { approved_folders }) {
            Ok(text) => text,
            Err(error) => {
                self.last_problem = Some(format!("the approvals of plugins could not be written: {error}"));
                return;
            }
        };
        let written = file
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| write_file_atomically(&file, &text));
        self.last_problem = written.err().map(|error| {
            format!(
                "the approvals of plugins could not be written to {} ({error}), so they are kept only until the program ends",
                file.display()
            )
        });
    }
}
