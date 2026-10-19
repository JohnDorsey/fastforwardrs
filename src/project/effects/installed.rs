// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound plugins that are installed on this computer, as types of effect (phase 7, piece 7.5; G10 of `docs/engine-phase-7-design.md`).
//!
//! A plugin is **found** by the scan of the folders of plugins (`plugin_scan`), which runs in a program of its own; what it finds is made into a descriptor for each plugin that
//! can be an effect (see `plugin_scan::effects`), and the list of them is given to [`set_installed_plugins`]. This module only keeps the list. It is a list for the whole
//! program, like the descriptors that files bring (see [`EffectKind::from_snapshot`](super::EffectKind::from_snapshot)): **a descriptor that is put in it is kept for the rest of
//! the run** (they are few and small), and the same descriptor put in twice is kept once, so that what is installed again after a new scan is the same kind of effect.
//!
//! **Whether a plugin is installed can change while the program runs** (the scan is in the background, and a folder can be added): an effect that a project has of a kind that
//! was made from the snapshot in the project file is *installed* as soon as the list has a plugin that can stand in for it, and *not installed* again if the plugin is taken out
//! of the list. Nothing is lost either way: a plugin that is not installed is a plugin that is not there, and what uses it passes the sound through and keeps its settings.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError, RwLock};

use super::EffectDescriptor;

/// A plugin that is installed, as a type of effect, and where and how to find it.
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledPlugin {
    /// The type of effect it is. Its values are the control inputs of the plugin, in the order of the ports.
    pub descriptor: EffectDescriptor,
    /// The library file the plugin is in.
    pub library: PathBuf,
    /// Which plugin of the library it is.
    pub plugin_index: usize,
    /// The number the maker gave it (kept; it is not the key, because plugins in the wild share them).
    pub unique_id: u64,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    /// For each value of the descriptor, in order, the number of the port of the plugin that it is the value of.
    pub value_ports: Vec<usize>,
}

/// The plugins that are installed now, in the order of the folders they were found in.
static INSTALLED: RwLock<Vec<&'static InstalledPlugin>> = RwLock::new(Vec::new());

/// Every plugin that was ever put in the list, each once.
static EVER_INSTALLED: Mutex<Vec<&'static InstalledPlugin>> = Mutex::new(Vec::new());

/// Makes `plugins` the plugins that are installed, in place of those that were. (A plugin that is the same as one that was put in before is that one: it is the same kind of
/// effect.)
pub fn set_installed_plugins(plugins: Vec<InstalledPlugin>) {
    let mut ever = EVER_INSTALLED.lock().unwrap_or_else(PoisonError::into_inner);
    let list: Vec<&'static InstalledPlugin> = plugins
        .into_iter()
        .map(|plugin| match ever.iter().find(|already| ***already == plugin) {
            Some(already) => *already,
            None => {
                let kept: &'static InstalledPlugin = Box::leak(Box::new(plugin));
                ever.push(kept);
                kept
            }
        })
        .collect();
    *INSTALLED.write().unwrap_or_else(PoisonError::into_inner) = list;
}

/// The plugins that are installed now.
pub fn installed_plugins() -> Vec<&'static InstalledPlugin> {
    INSTALLED.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// The installed plugin that can stand in for the type of effect `descriptor` describes (the same key and major version, and every value of `descriptor` is one of the plugin's
/// of the same kind), if there is one: the first in the list, which is the one of the folder that comes first.
pub fn installed_plugin_for(descriptor: &EffectDescriptor) -> Option<&'static InstalledPlugin> {
    INSTALLED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .copied()
        .find(|plugin| plugin.descriptor.can_stand_in_for(descriptor))
}
