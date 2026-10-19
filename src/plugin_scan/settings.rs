// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a person has said that plugins are (G16 of `docs/engine-phase-7-design.md`), kept in a file of its own beside the settings (`plugin_search.json`): the folders to look
//! for plugin libraries in, the folders that have the libraries that plugin libraries need (on Windows a plugin often needs a DLL that is not beside it: the DLLs of LMMS are
//! in the folder above its `plugins`), and whether the folders above a plugin library are looked in for those too when they are not in the folders just named.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::atomic_file::write_file_atomically;

/// What the file is called, beside the settings file.
const FILE_NAME: &str = "plugin_search.json";

/// Where plugins and the libraries they need are looked for, besides the usual places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSearchSettings {
    /// The folders a person added to look for plugin libraries in.
    pub folders: Vec<PathBuf>,
    /// The folders a person said have libraries that plugin libraries need.
    pub dependency_folders: Vec<PathBuf>,
    /// Whether the folders above a plugin library are looked in for the libraries it needs, after those just named. (On unless a person turns it off.)
    pub search_parent_folders: bool,
}

impl Default for PluginSearchSettings {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            dependency_folders: Vec::new(),
            search_parent_folders: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Document {
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    dependency_folders: Vec<String>,
    #[serde(default = "yes")]
    search_parent_folders: bool,
}

fn yes() -> bool {
    true
}

impl PluginSearchSettings {
    /// Where the file is kept: beside the settings file. `None` if this system has no settings folder.
    pub fn default_file() -> Option<PathBuf> {
        crate::settings_file::default_settings_file_path().and_then(|settings| Self::file_beside(&settings))
    }

    /// Where the file is kept when the settings file is `settings_file`: beside it.
    pub fn file_beside(settings_file: &std::path::Path) -> Option<PathBuf> {
        Some(settings_file.parent()?.join(FILE_NAME))
    }

    /// What is in the file at `file`: nothing there (or no file) is the defaults; a file that cannot be read is the defaults and the problem is given, so that the person is told
    /// and the file is left alone until something is changed.
    pub fn read(file: &std::path::Path) -> (Self, Option<String>) {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return (Self::default(), None),
            Err(error) => {
                return (
                    Self::default(),
                    Some(format!("{} cannot be read: {error}", file.display())),
                );
            }
        };
        match serde_json::from_str::<Document>(&text) {
            Ok(document) => (
                Self {
                    folders: document.folders.into_iter().map(PathBuf::from).collect(),
                    dependency_folders: document.dependency_folders.into_iter().map(PathBuf::from).collect(),
                    search_parent_folders: document.search_parent_folders,
                },
                None,
            ),
            Err(error) => (
                Self::default(),
                Some(format!("{} cannot be read: {error}", file.display())),
            ),
        }
    }

    /// Writes the file at `file`.
    pub fn write(&self, file: &std::path::Path) -> io::Result<()> {
        let document = Document {
            folders: self
                .folders
                .iter()
                .filter_map(|folder| folder.to_str().map(str::to_owned))
                .collect(),
            dependency_folders: self
                .dependency_folders
                .iter()
                .filter_map(|folder| folder.to_str().map(str::to_owned))
                .collect(),
            search_parent_folders: self.search_parent_folders,
        };
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder)?;
        }
        write_file_atomically(
            file,
            &serde_json::to_string_pretty(&document).map_err(io::Error::other)?,
        )
    }
}
