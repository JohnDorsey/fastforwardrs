// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the sound plugins that are installed, without ever loading one in the editor (phase 7, piece 7.4; the design is G3, G4 and G16 of `docs/engine-phase-7-design.md`).
//!
//! * [`folders`] says where to look (the folders a person added, `LADSPA_PATH`, the usual ones of the system) and which files in a folder are libraries.
//! * A library is **scanned by a program of its own**, `fastforwardrs-plugin-scan` ([`child`], with the words it and the editor use for each other in [`protocol`]): the program
//!   loads the library, reads what its plugins say, makes each one and runs it once on silence. A library that crashes it, never returns (it is stopped after thirty seconds),
//!   says nothing, or says something that is not the answer is **failed, with the reason** ([`ScanFailure`]), and nothing is lost but the program.
//! * What was found, and what failed, is kept in a file ([`cache`]) and **not tried again until the library's size or time changes**, so that a library that crashes the
//!   scanner does not do it at every start of the editor.
//! * A **breadcrumb** ([`breadcrumb`]) is written before a library is used for the first time in a session and removed when the editor ends well; one that is found at the start
//!   says which library the editor was using when it ended badly, and that library is failed like the others.
//!
//! Nothing here loads a library into the editor (the editor's crate has no `unsafe` code, and the only way to load one is `ladspa_host::Library::open`, which only the scan
//! program and, later, the part that runs plugins after a person has said yes call).

pub mod breadcrumb;
pub mod cache;
pub mod child;
pub mod effects;
pub mod failure;
pub mod folders;
pub mod program;
pub mod protocol;
pub mod scanner;
pub mod search;
pub mod settings;

pub use breadcrumb::{Breadcrumb, Recovery, recover};
pub use cache::{CachedOutcome, FileFingerprint, ScanCache};
pub use child::{SCAN_TIME_LIMIT, scanner_program_beside_this_program};
pub use effects::{
    NotUsed, PluginsFound, effect_of_a_clap_plugin, effect_of_a_plugin, key_of, key_of_a_clap_plugin, plugins_found_in,
    pre_roll_of_a_clap_plugin,
};
pub use failure::ScanFailure;
pub use folders::{
    BUNDLED_PLUGINS_VARIABLE, bundled_plugins_folder, bundled_plugins_folder_with, clap_folders, clap_libraries_in,
    dependency_search_folders, folders_looked_in_by_default, folders_looked_in_with, folders_to_scan, is_a_clap_file,
    libraries_in, library_file_extension, library_file_extensions, library_search_variable, usual_folders,
};
pub use ladspa_host::{LibraryScan, ScannedPlugin};
pub use scanner::{FolderScan, LibraryOutcome, LibraryReport, PluginScanner, ScannerProblem};
pub use search::{PluginSearch, WhatTheScanFound};
pub use settings::PluginSearchSettings;

#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod tests;
