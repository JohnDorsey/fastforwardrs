// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of the plugins, once and waited for: what a program with no window does before it exports a project that has plugins on its clips (the editor scans on a thread of its
//! own and takes the result when the window is woken; see `sound_plugins_flow`).

use std::path::PathBuf;

use super::{
    PluginScanner, PluginSearchSettings, ScanCache, folders_looked_in_by_default, plugins_found_in,
    scanner_program_beside_this_program,
};
use crate::project::set_installed_plugins;

/// How plugins are looked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSearch {
    /// The program that scans the libraries.
    pub scanner_program: PathBuf,
    /// Where a person's folders are written down (`None`: the defaults).
    pub settings_file: Option<PathBuf>,
    /// Where what the scans found is kept (`None`: only while the scan runs).
    pub cache_file: Option<PathBuf>,
    /// Whether `LADSPA_PATH`, `CLAP_PATH` and the usual folders are looked in besides those a person added.
    pub look_in_the_usual_places: bool,
}

/// What a scan found, besides the plugins that are installed now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatTheScanFound {
    /// Where a person said to look (the libraries that plugin libraries need are looked for in the folders it names).
    pub settings: PluginSearchSettings,
    /// What went wrong with the scan as a whole, one line each.
    pub problems: Vec<String>,
    /// How many plugins can be effects now.
    pub effect_count: usize,
}

impl PluginSearch {
    /// The way a person's editor looks: the scan program beside this program, the folders and the cache beside the settings, the usual places. `None` if there is no scan
    /// program beside this program.
    pub fn the_usual() -> Option<Self> {
        Some(Self {
            scanner_program: scanner_program_beside_this_program()?,
            settings_file: PluginSearchSettings::default_file(),
            cache_file: ScanCache::default_file(),
            look_in_the_usual_places: true,
        })
    }

    /// The way a person's editor looks, but with the folders and the cache beside `settings_file` (the settings file that was asked for), and the scan program `scanner_program`.
    pub fn beside_the_settings_file(scanner_program: PathBuf, settings_file: Option<&std::path::Path>) -> Self {
        Self {
            scanner_program,
            settings_file: settings_file.and_then(PluginSearchSettings::file_beside),
            cache_file: settings_file.and_then(ScanCache::file_beside),
            look_in_the_usual_places: true,
        }
    }

    /// Scans the folders, **and waits for it**, and makes the plugins that can be effects the effects that are installed.
    pub fn scan_and_install(&self) -> WhatTheScanFound {
        let (settings, settings_problem) = match &self.settings_file {
            Some(file) => PluginSearchSettings::read(file),
            None => (PluginSearchSettings::default(), None),
        };
        let mut folders = settings.folders.clone();
        if self.look_in_the_usual_places {
            for folder in folders_looked_in_by_default() {
                if !folders.contains(&folder) {
                    folders.push(folder);
                }
            }
        }
        let cache = self.cache_file.clone().map_or_else(ScanCache::in_memory, ScanCache::at);
        let mut scanner = PluginScanner::new(self.scanner_program.clone(), cache)
            .looking_for_dependencies_in(settings.dependency_folders.clone(), settings.search_parent_folders);
        let scan = scanner.scan_folders(&folders);
        let found = plugins_found_in(&scan.libraries);
        let effect_count = found.effects.len();
        set_installed_plugins(found.effects);
        let mut problems = scan.problems;
        problems.extend(settings_problem);
        WhatTheScanFound {
            settings,
            problems,
            effect_count,
        }
    }
}
