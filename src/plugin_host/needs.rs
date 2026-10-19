// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the sound of a project needs from the folders that plugins are in: asked before the sound is made, by the export and by the command line, so that a person is asked
//! (or the command line refuses) before anything is run, and not after.

use std::collections::BTreeMap;

use super::host::Wanted;
use crate::plugin_trust::folder_of;
use crate::project::{EffectDomain, Project};

/// The folders that the plugins whose effects are applied to the clips of `project` are in, each once, in the order of the folders, with the names of the plugins of it that are
/// used (each once, in the order of their names). An effect that is a plugin that is not installed is not here: nothing runs it.
pub fn folders_the_sound_of_a_project_needs(project: &Project) -> Vec<Wanted> {
    let mut folders: BTreeMap<std::path::PathBuf, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for applied in project.effects_applied_to_every_clip().values().flatten() {
        let kind = applied.effect.kind();
        if kind.domain() != EffectDomain::Sound {
            continue;
        }
        let Some(plugin) = kind.installed_plugin() else {
            continue;
        };
        let (libraries, plugins) = folders.entry(folder_of(&plugin.library)).or_default();
        let library = plugin
            .library
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        if !libraries.contains(&library) {
            libraries.push(library);
        }
        let name = plugin.descriptor.name.to_string();
        if !plugins.contains(&name) {
            plugins.push(name);
        }
    }
    folders
        .into_iter()
        .map(|(folder, (mut libraries, mut plugins))| {
            libraries.sort();
            plugins.sort();
            Wanted {
                folder,
                libraries,
                plugins,
            }
        })
        .collect()
}
