// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A `.clap` file that has been loaded: what is in it.

use std::path::{Path, PathBuf};

use clack_host::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a file could not be opened as a CLAP library.
#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenError {
    /// The system's loader refused the file, or the library said no when it was initialised (`clap_entry.init` is false).
    #[error("{0}")]
    CouldNotBeLoaded(String),
    /// It loaded, and has no plugin factory: it is a library, and not a CLAP one.
    #[error("{0}")]
    NotAClapLibrary(String),
}

/// What a plugin says of itself in its descriptor.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDescription {
    /// A reverse-domain string that is unique in the world (`com.example.reverb`).
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: String,
    /// The words that say what it is (`audio-effect`, `instrument`, `stereo`...).
    pub features: Vec<String>,
}

/// What a library has in it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryDescription {
    pub plugins: Vec<PluginDescription>,
    /// Plugins whose descriptors could not be used, one line each.
    pub problems: Vec<String>,
}

/// A CLAP library that is loaded. It stays loaded for as long as any copy of it, or any instance made from it, exists.
#[derive(Clone)]
pub struct Library {
    pub(crate) entry: PluginEntry,
    path: PathBuf,
}

impl std::fmt::Debug for Library {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Library").field("path", &self.path).finish()
    }
}

impl Library {
    /// Loads the file at `path` (the bundle on macOS) and initialises it.
    #[allow(unsafe_code)]
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        // SAFETY: loading a library runs its code, which is what a plugin is; the caller (the scan program, or the editor after a person said yes to the folder) has
        // already decided that this file may be run. Nothing else is promised by the call: `clack` calls `clap_entry.init` the way the contract says.
        let entry =
            unsafe { PluginEntry::load(path) }.map_err(|error| OpenError::CouldNotBeLoaded(error.to_string()))?;
        if entry.get_plugin_factory().is_none() {
            return Err(OpenError::NotAClapLibrary(
                "the library has no plugin factory (it is a library, and not a CLAP one)".to_owned(),
            ));
        }
        Ok(Self {
            entry,
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the descriptors of the plugins in the library say.
    pub fn describe(&self) -> LibraryDescription {
        let mut described = LibraryDescription::default();
        let Some(factory) = self.entry.get_plugin_factory() else {
            return described;
        };
        for (index, descriptor) in factory.plugin_descriptors().enumerate() {
            let text = |value: Option<&std::ffi::CStr>| {
                value
                    .map(|text| text.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let id = descriptor.id().map(|id| id.to_string_lossy().into_owned());
            let name = text(descriptor.name());
            if let Some(problem) = problem_with_descriptor(index, id.as_deref(), &name) {
                described.problems.push(problem);
                continue;
            }
            let id = id.unwrap_or_default();
            described.plugins.push(PluginDescription {
                id,
                name,
                vendor: text(descriptor.vendor()),
                version: text(descriptor.version()),
                description: text(descriptor.description()),
                features: descriptor
                    .features()
                    .map(|feature| feature.to_string_lossy().into_owned())
                    .collect(),
            });
        }
        described
    }
}

/// What is wrong with a descriptor that has this id and name (the plugin number `index` of its library), if anything: a plugin without both cannot be shown to anyone or told apart.
pub(crate) fn problem_with_descriptor(index: usize, id: Option<&str>, name: &str) -> Option<String> {
    let Some(id) = id else {
        return Some(format!("plugin number {index} has no id"));
    };
    if id.trim().is_empty() {
        return Some(format!("plugin number {index} has a blank id"));
    }
    if name.trim().is_empty() {
        return Some(format!("the plugin {id} has a blank name"));
    }
    None
}
