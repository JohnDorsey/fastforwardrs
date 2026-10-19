// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor does to run a plugin: the [`PluginHost`] is the [`PluginRunner`] of the editor. **It is the one place that loads a library into the editor**, and it does so
//! only for a library in a folder that a person has said yes to (see `plugin_trust`), and after the breadcrumb says which library is about to be used.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ladspa_host::Library;

use super::effect::{Layout, PluginEffect};
use crate::composition::LayerEffect;
use crate::media::export::SoundClipToExport;
use crate::plugin_scan::{Breadcrumb, dependency_search_folders};
use crate::plugin_trust::{FolderStanding, folder_of};
use crate::project::{FrameRate, InstalledPlugin};
use crate::sound_engine::{BlockProcessor, Notes, PluginRunner, SAMPLE_RATE};

/// A folder that plugins are wanted from and nobody has said anything about yet: the editor asks, and the sound is made again when it has been answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub folder: PathBuf,
    /// The file names of the libraries of the folder that are wanted, each once, in the order they were first wanted.
    pub libraries: Vec<String>,
    /// The names of the plugins of the folder that are wanted, each once, in the order they were first wanted.
    pub plugins: Vec<String>,
}

#[derive(Default)]
struct Trust {
    /// Whether every folder is allowed: the command line that is told `--allow-plugins` cannot ask.
    everything_is_allowed: bool,
    approved: HashSet<PathBuf>,
    declined: HashSet<PathBuf>,
    wanted: Vec<Wanted>,
}

/// Where the libraries that plugin libraries need are looked for, besides beside the plugin library.
struct Search {
    dependency_folders: Vec<PathBuf>,
    /// Whether the folders above a plugin library are looked in too. (On, as the setting of a person is until they turn it off: the host is right before the setting is read.)
    parent_folders: bool,
    /// What the search path was before the editor changed it for a library.
    original: Option<std::ffi::OsString>,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            dependency_folders: Vec::new(),
            parent_folders: true,
            original: None,
        }
    }
}

/// Loads the plugins of the folders that may be run from, and makes the effects of the sound from them.
pub struct PluginHost {
    trust: Mutex<Trust>,
    libraries: Mutex<HashMap<PathBuf, Result<Library, String>>>,
    search: Mutex<Search>,
    breadcrumb: Mutex<Option<Breadcrumb>>,
    /// A number that is different whenever what may be run changed: what the sound is made of changes with it.
    generation: AtomicU64,
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            trust: Mutex::new(Trust::default()),
            libraries: Mutex::new(HashMap::new()),
            search: Mutex::new(Search::default()),
            breadcrumb: Mutex::new(None),
            generation: AtomicU64::new(0),
        }
    }

    fn trust(&self) -> std::sync::MutexGuard<'_, Trust> {
        self.trust.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Lets every plugin be run (the command line told `--allow-plugins`, which is the person saying yes to all).
    pub fn allow_everything(&self) {
        self.trust().everything_is_allowed = true;
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Says which folders have been approved (those that are written down), replacing what was said.
    pub fn set_approved_folders(&self, folders: impl IntoIterator<Item = PathBuf>) {
        let mut trust = self.trust();
        let approved: HashSet<PathBuf> = folders.into_iter().collect();
        if trust.approved != approved {
            trust.approved = approved;
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Says that plugins may be run from `folder` (as [`crate::plugin_trust::folder_key`] names it).
    pub fn approve_folder(&self, folder: &Path) {
        let mut trust = self.trust();
        trust.declined.remove(folder);
        trust.wanted.retain(|wanted| wanted.folder != folder);
        trust.approved.insert(folder.to_owned());
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Says that plugins may not be run from `folder` for as long as the project is open.
    pub fn decline_folder(&self, folder: &Path) {
        let mut trust = self.trust();
        trust.wanted.retain(|wanted| wanted.folder != folder);
        trust.declined.insert(folder.to_owned());
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Forgets what was declined (another project was opened).
    pub fn forget_what_was_declined(&self) {
        let mut trust = self.trust();
        if !trust.declined.is_empty() {
            trust.declined.clear();
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// What the host does about plugins that are files in `folder` (as [`crate::plugin_trust::folder_key`] names it), without asking for any: they are run, or they are not and
    /// the person said no, or nobody has said anything.
    pub fn standing_of(&self, folder: &Path) -> FolderStanding {
        let trust = self.trust();
        if trust.everything_is_allowed || trust.approved.contains(folder) {
            FolderStanding::Approved
        } else if trust.declined.contains(folder) {
            FolderStanding::Declined
        } else {
            FolderStanding::NotAsked
        }
    }

    /// The folders that plugins have been wanted from and nobody has answered for, once each; they are not given again.
    pub fn take_wanted(&self) -> Vec<Wanted> {
        std::mem::take(&mut self.trust().wanted)
    }

    /// A number that is different whenever what may be run changed.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Says where the libraries that plugin libraries need are looked for (the folders a person named, and whether the folders above a plugin library are looked in too).
    pub fn look_for_dependencies_in(&self, dependency_folders: Vec<PathBuf>, parent_folders: bool) {
        let mut search = self.search.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        search.dependency_folders = dependency_folders;
        search.parent_folders = parent_folders;
    }

    /// Where the libraries that plugin libraries need are looked for, and whether the folders above a plugin library are looked in too (see [`Self::look_for_dependencies_in`]).
    pub fn where_dependencies_are_looked_for(&self) -> (Vec<PathBuf>, bool) {
        let search = self.search.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        (search.dependency_folders.clone(), search.parent_folders)
    }

    /// Keeps the breadcrumb: a library is written in it before it is used for the first time.
    pub fn keep_a_breadcrumb(&self, breadcrumb: Breadcrumb) {
        *self
            .breadcrumb
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(breadcrumb);
    }

    /// The editor is ending well: the breadcrumb is taken away.
    pub fn ended_well(&self) {
        if let Some(breadcrumb) = self
            .breadcrumb
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
        {
            breadcrumb.ended_well();
        }
    }

    /// The libraries that are loaded now.
    pub fn loaded_libraries(&self) -> Vec<PathBuf> {
        let libraries = self.libraries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut paths: Vec<PathBuf> = libraries
            .iter()
            .filter(|(_, library)| library.is_ok())
            .map(|(path, _)| path.clone())
            .collect();
        paths.sort();
        paths
    }

    /// Whether plugins from `folder` may be run now: `Some(true)` yes, `Some(false)` declined, `None` nobody has answered (and the folder is then wanted).
    fn may_run_from(&self, folder: &Path, library_name: &str, plugin_name: &str) -> Option<bool> {
        let mut trust = self.trust();
        if trust.everything_is_allowed || trust.approved.contains(folder) {
            return Some(true);
        }
        if trust.declined.contains(folder) {
            return Some(false);
        }
        match trust.wanted.iter_mut().find(|wanted| wanted.folder == folder) {
            Some(wanted) => {
                if !wanted.libraries.iter().any(|name| name == library_name) {
                    wanted.libraries.push(library_name.to_owned());
                }
                if !wanted.plugins.iter().any(|name| name == plugin_name) {
                    wanted.plugins.push(plugin_name.to_owned());
                }
            }
            None => trust.wanted.push(Wanted {
                folder: folder.to_owned(),
                libraries: vec![library_name.to_owned()],
                plugins: vec![plugin_name.to_owned()],
            }),
        }
        None
    }

    /// The library at `path`, loaded (once: it stays loaded), or why it could not be.
    fn library(&self, path: &Path) -> Result<Library, String> {
        let mut libraries = self.libraries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(known) = libraries.get(path) {
            return known.clone();
        }
        self.prepare_the_search_for(path);
        if let Some(breadcrumb) = self
            .breadcrumb
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
        {
            // (A breadcrumb that cannot be written is not a reason to leave the plugin out: it only helps after a crash.)
            let _ = breadcrumb.note_use_of(path);
        }
        let loaded = Library::open(path).map_err(|error| error.to_string());
        libraries.insert(path.to_owned(), loaded.clone());
        loaded
    }

    /// Before a library is loaded: where the libraries that it needs are looked for, where that can be done after the program has started (on Windows: before the others of the
    /// search path). On the other systems the loader reads its search path when the program starts, and a plugin that needs a library that is not beside it has to be
    /// loaded by a program that was started with the folder in `LD_LIBRARY_PATH` (the scan has the folders: it is started with them).
    fn prepare_the_search_for(&self, library: &Path) {
        let mut search = self.search.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let folders = dependency_search_folders(library, &search.dependency_folders, search.parent_folders);
        if folders.is_empty() {
            return;
        }
        let original = search
            .original
            .get_or_insert_with(|| std::env::var_os(crate::plugin_scan::library_search_variable()).unwrap_or_default())
            .clone();
        ladspa_host::put_folders_before_the_search_path(&folders, &original);
    }

    fn the_instances_of(
        &self,
        plugin: &'static InstalledPlugin,
        layout: Layout,
        library: &Library,
        notes: &Notes,
    ) -> Option<Vec<ladspa_host::Instance>> {
        let mut instances = Vec::new();
        for _ in 0..layout.instances() {
            match library.instantiate(plugin.plugin_index, SAMPLE_RATE) {
                Ok(instance) => {
                    if !this_is_the_plugin_that_was_scanned(&instance, plugin) {
                        say(
                            notes,
                            format!(
                                "The sound plugin \u{ab}{}\u{bb} is not what it was when the plugins were scanned (the library {} has changed): scan again.",
                                plugin.descriptor.name,
                                plugin.library.display()
                            ),
                        );
                        return None;
                    }
                    instances.push(instance);
                }
                Err(error) => {
                    say(
                        notes,
                        format!(
                            "The sound plugin \u{ab}{}\u{bb} could not be made: {error}. The sound goes through it unchanged.",
                            plugin.descriptor.name
                        ),
                    );
                    return None;
                }
            }
        }
        Some(instances)
    }
}

fn say(notes: &Notes, line: String) {
    let mut notes = notes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !notes.contains(&line) {
        notes.push(line);
    }
}

/// Whether what the library gave for the plugin is the plugin that was scanned: the same label and ports of the same kinds, so that no value is given to a port that is
/// not the one it was made for.
fn this_is_the_plugin_that_was_scanned(instance: &ladspa_host::Instance, plugin: &InstalledPlugin) -> bool {
    use ladspa_host::{PortDirection, PortKind};
    let description = instance.description();
    let (inputs, outputs) = description.audio_port_counts();
    let key_label = plugin.descriptor.key.rsplit(':').next().unwrap_or_default();
    description.label.replace(':', "_") == key_label
        && (inputs, outputs) == (plugin.audio_inputs, plugin.audio_outputs)
        && plugin.value_ports.iter().all(|&port| {
            description
                .ports
                .get(port)
                .is_some_and(|port| port.direction == PortDirection::Input && port.kind == PortKind::Control)
        })
}

impl PluginRunner for PluginHost {
    fn generation(&self) -> u64 {
        PluginHost::generation(self)
    }

    fn processor_for(
        &self,
        effect: &LayerEffect,
        clip: &SoundClipToExport,
        frame_rate: FrameRate,
        notes: &Notes,
        is_for_the_preview: bool,
    ) -> Option<Box<dyn BlockProcessor>> {
        let plugin = effect.kind.installed_plugin()?;
        let layout = Layout::of(plugin.audio_inputs, plugin.audio_outputs)?;
        let folder = folder_of(&plugin.library);
        let library_name = plugin
            .library
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        if !self.may_run_from(&folder, &library_name, &plugin.descriptor.name)? {
            return None;
        }
        let library = match self.library(&plugin.library) {
            Ok(library) => library,
            Err(error) => {
                say(
                    notes,
                    format!(
                        "The sound plugin \u{ab}{}\u{bb} could not be loaded from {}: {error}. The sound goes through it unchanged.",
                        plugin.descriptor.name,
                        plugin.library.display()
                    ),
                );
                return None;
            }
        };
        let instances = self.the_instances_of(plugin, layout, &library, notes)?;
        PluginEffect::new(
            instances,
            plugin,
            effect.clone(),
            frame_rate,
            clip.first_timeline_frame,
            is_for_the_preview,
            std::sync::Arc::clone(notes),
        )
        .map(|made| Box::new(made) as Box<dyn BlockProcessor>)
    }
}
