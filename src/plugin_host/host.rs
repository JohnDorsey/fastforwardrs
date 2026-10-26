// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor does to run a plugin: the [`PluginHost`] is the [`PluginRunner`] of the editor. **It is the one place that loads a library into the editor**, and it does so
//! only for a library in a folder that a person has said yes to (see `plugin_trust`), and after the breadcrumb says which library is about to be used.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ladspa_host::Library;

use super::effect::{Layout, PluginEffect, ProblemBoard};
use super::instance::{ClapCopy, ClapMaker, LadspaCopy, Lives, Lv2Copy, PluginInstance};
use crate::composition::LayerEffect;
use crate::media::export::SoundClipToExport;
use crate::plugin_scan::{Breadcrumb, dependency_search_folders};
use crate::plugin_trust::{FolderStanding, PluginCache, folder_of};
use crate::project::{FrameRate, InstalledPlugin, PluginAddress, PluginLibrary};
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
    /// The folder of the plugins that came with the program, and every folder in it, which are trusted without a question (not written down: it is the program's own).
    bundled: Vec<PathBuf>,
}

/// What is known of the plugins that are files in `folder` (as [`crate::plugin_trust::folder_key`] names it): they may be run (every folder is allowed, or a person said yes to it, or
/// it is the program's own bundled folder or in it), they may not, or nobody has said anything. **The one place that decides it.**
fn standing_in(trust: &Trust, folder: &Path) -> FolderStanding {
    if trust.everything_is_allowed
        || trust.approved.contains(folder)
        || trust
            .bundled
            .iter()
            // (Each is named as the system names it when it is looked at: a folder that was not there when it was said may be by now.)
            .any(|bundled| folder.starts_with(crate::plugin_trust::folder_key(bundled)))
    {
        FolderStanding::Approved
    } else if trust.declined.contains(folder) {
        FolderStanding::Declined
    } else {
        FolderStanding::NotAsked
    }
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
    clap_libraries: Mutex<HashMap<PathBuf, Result<clap_host::Library, String>>>,
    /// The LV2 libraries (the binaries of bundles), by the path of the file.
    lv2_libraries: Mutex<HashMap<PathBuf, Result<lv2_host::Library, String>>>,
    /// What the bundles said when they were last read (by the folder of the bundle): read again when the plugins are scanned again (see [`Self::forget_what_the_bundles_said`]).
    lv2_bundles: Mutex<HashMap<PathBuf, Result<std::sync::Arc<lv2_host::BundleDescription>, String>>>,
    /// The thread that does what CLAP asks to be done on the main thread, started when the first CLAP plugin is wanted.
    clap_worker: Mutex<Option<std::sync::Arc<clap_host::MainThread>>>,
    search: Mutex<Search>,
    breadcrumb: Mutex<Option<Breadcrumb>>,
    /// A number that is different whenever what may be run changed: what the sound is made of changes with it.
    generation: AtomicU64,
    /// What is wrong with the plugins that were wanted, by their keys (see [`Self::problem_with`]).
    problems: ProblemBoard,
    /// Which CLAP plugins are alive, and for which streams, and which streams are being stopped.
    lives: std::sync::Arc<Mutex<Lives>>,
    /// How long a plugin that is made waits for the plugins of the same library that are being let go of (see [`Self::the_clap_instances_of`]).
    longest_wait_for_what_is_being_let_go_of: std::time::Duration,
    /// The folders of the plugins that were written out of a project file and are trusted until the next project is opened (see [`Self::write_out_and_trust_an_embedded_plugin`]).
    embedded_folders: Mutex<Vec<PathBuf>>,
    /// Where the plugins of project files are written out, made when the first is. **The last field: the libraries above it are let go of before its files are removed** (a
    /// library that is loaded cannot be removed on Windows).
    embedded_cache: Mutex<Option<PluginCache>>,
}

/// How long a CLAP plugin that is made waits, at the most, for the plugins of its library that are being let go of by a stream that is being stopped.
pub const LONGEST_WAIT_FOR_WHAT_IS_BEING_LET_GO_OF: std::time::Duration = std::time::Duration::from_secs(2);

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
            clap_libraries: Mutex::new(HashMap::new()),
            lv2_libraries: Mutex::new(HashMap::new()),
            lv2_bundles: Mutex::new(HashMap::new()),
            clap_worker: Mutex::new(None),
            search: Mutex::new(Search::default()),
            breadcrumb: Mutex::new(None),
            generation: AtomicU64::new(0),
            problems: ProblemBoard::default(),
            lives: std::sync::Arc::default(),
            longest_wait_for_what_is_being_let_go_of: LONGEST_WAIT_FOR_WHAT_IS_BEING_LET_GO_OF,
            embedded_folders: Mutex::new(Vec::new()),
            embedded_cache: Mutex::new(None),
        }
    }

    /// Writes out the build for `platform` of the plugin library `library`, which is inside a project file, to a folder of its own in the temporary files (**the bytes are checked
    /// against the digest the project recorded for them first, and nothing is written if they are not that**), says that plugins may be run from that folder, and gives the file.
    ///
    /// **Call this only for a library that a person has said yes to** (the severe question of `plugin_trust`, asked every time a project is opened), or that the command line was
    /// told to allow: it is the one way that a plugin of a project file comes to be run, and it trusts what it writes. The trust is for the plugins of this build's folder alone, and
    /// lasts until [`Self::forget_the_embedded_plugins`] (another project is opened).
    pub fn write_out_and_trust_an_embedded_plugin(
        &self,
        library: &PluginLibrary,
        platform: &str,
    ) -> Result<PathBuf, String> {
        let build = library.build_for(platform).ok_or_else(|| {
            format!(
                "the plugin library {} has no build for {platform} (it has {})",
                library.name(),
                library.platforms().join(" and ")
            )
        })?;
        let path = {
            let mut cache = self
                .embedded_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if cache.is_none() {
                *cache = Some(
                    PluginCache::in_the_temporary_files()
                        .map_err(|error| format!("the folder to write plugins out to could not be made: {error}"))?,
                );
            }
            cache
                .as_ref()
                .expect("the cache was just made")
                .extract(library.name(), build)
                .map_err(|error| error.to_string())?
        };
        let folder = folder_of(&path);
        let mut folders = self
            .embedded_folders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !folders.contains(&folder) {
            folders.push(folder.clone());
        }
        drop(folders);
        self.approve_folder(&folder);
        Ok(path)
    }

    /// Takes back the trust that [`Self::write_out_and_trust_an_embedded_plugin`] gave (another project was opened, and an embedded plugin is asked about every time). The files
    /// stay where they were written; a plugin of one is not run again unless it is written out and trusted again. Whether any was trusted.
    pub fn forget_the_embedded_plugins(&self) -> bool {
        let folders = std::mem::take(
            &mut *self
                .embedded_folders
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        if folders.is_empty() {
            return false;
        }
        let mut trust = self.trust();
        for folder in &folders {
            trust.approved.remove(folder);
        }
        drop(trust);
        self.generation.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn trust(&self) -> std::sync::MutexGuard<'_, Trust> {
        self.trust.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Lets every plugin be run (the command line told `--allow-plugins`, which is the person saying yes to all).
    pub fn allow_everything(&self) {
        self.trust().everything_is_allowed = true;
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Says that the plugins that came with the program are in `folder` (as [`crate::plugin_trust::folder_key`] names it, or as it is given: it is made so) **and in the folders
    /// below it**: they are trusted without a question, as the program is (a release's own files are as trusted as the program; the licences and the source offer that go with them are
    /// in `docs/distribution-notices.md`). Not written to the file of approvals: it is the program's own, and a folder that is not this one is asked about as it always was.
    pub fn trust_bundled_folder(&self, folder: &Path) {
        let mut trust = self.trust();
        if !trust.bundled.iter().any(|known| known == folder) {
            trust.bundled.push(folder.to_owned());
        }
        drop(trust);
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
        standing_in(&self.trust(), folder)
    }

    /// The folders that plugins have been wanted from and nobody has answered for, once each; they are not given again.
    pub fn take_wanted(&self) -> Vec<Wanted> {
        std::mem::take(&mut self.trust().wanted)
    }

    /// Why the plugin whose effect has the key `key` did nothing the last time it was wanted in the sound (it could not be loaded, it is not what was scanned, it could not be
    /// made or run, it was too slow), if it did not: what the window of the effect says. Taken away when the plugin is wanted again and nothing is wrong.
    pub fn problem_with(&self, key: &str) -> Option<String> {
        self.problems
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    /// The same, waiting no longer than `longest` for the plugins that are being let go of (for a test, which does not want to wait two seconds).
    #[must_use]
    pub fn waiting_no_longer_than(mut self, longest: std::time::Duration) -> Self {
        self.longest_wait_for_what_is_being_let_go_of = longest;
        self
    }

    /// Says why the plugin whose effect has the key `key` does nothing (for the window of the effect). The host says it itself when a plugin that was wanted went wrong.
    pub fn note_a_problem_with(&self, key: &str, why: &str) {
        self.problems
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.to_owned(), why.to_owned());
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
        let clap_libraries = self
            .clap_libraries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        paths.extend(
            clap_libraries
                .iter()
                .filter(|(_, library)| library.is_ok())
                .map(|(path, _)| path.clone()),
        );
        let lv2_libraries = self
            .lv2_libraries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        paths.extend(
            lv2_libraries
                .iter()
                .filter(|(_, library)| library.is_ok())
                .map(|(path, _)| path.clone()),
        );
        paths.sort();
        paths
    }

    /// Whether plugins from `folder` may be run now: `Some(true)` yes, `Some(false)` declined, `None` nobody has answered (and the folder is then wanted).
    fn may_run_from(&self, folder: &Path, library_name: &str, plugin_name: &str) -> Option<bool> {
        let mut trust = self.trust();
        // (The same answer as [`Self::standing_of`] gives, which the window of an effect reads: what it says is what is done.)
        match standing_in(&trust, folder) {
            FolderStanding::Approved => return Some(true),
            FolderStanding::Declined => return Some(false),
            FolderStanding::NotAsked => {}
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
        self.before_loading(path);
        let loaded = Library::open(path).map_err(|error| error.to_string());
        libraries.insert(path.to_owned(), loaded.clone());
        loaded
    }

    /// The CLAP library at `path`, loaded (once: it stays loaded), or why it could not be.
    fn clap_library(&self, path: &Path) -> Result<clap_host::Library, String> {
        let mut libraries = self
            .clap_libraries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(known) = libraries.get(path) {
            return known.clone();
        }
        self.before_loading(path);
        let loaded = clap_host::Library::open(path).map_err(|error| error.to_string());
        libraries.insert(path.to_owned(), loaded.clone());
        loaded
    }

    /// The LV2 library at `path` (the binary of a bundle), loaded (once: it stays loaded), or why it could not be.
    fn lv2_library(&self, path: &Path) -> Result<lv2_host::Library, String> {
        let mut libraries = self
            .lv2_libraries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(known) = libraries.get(path) {
            return known.clone();
        }
        self.before_loading(path);
        let loaded = lv2_host::Library::open(path).map_err(|error| error.to_string());
        libraries.insert(path.to_owned(), loaded.clone());
        loaded
    }

    /// What the bundle in `folder` says (its Turtle is read once, and kept until [`Self::forget_what_the_bundles_said`]), or why it cannot be read.
    fn lv2_bundle(&self, folder: &Path) -> Result<std::sync::Arc<lv2_host::BundleDescription>, String> {
        let mut bundles = self
            .lv2_bundles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(known) = bundles.get(folder) {
            return known.clone();
        }
        let described = lv2_host::describe_bundle(folder)
            .map(std::sync::Arc::new)
            .map_err(|error| error.to_string());
        bundles.insert(folder.to_owned(), described.clone());
        described
    }

    /// Forgets what the bundles said: they are read again the next time a plugin of one is wanted. (For when the plugins have been scanned again: a bundle may have changed.)
    pub fn forget_what_the_bundles_said(&self) {
        self.lv2_bundles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// Before a library of either kind is loaded: where the libraries it needs are looked for, and the breadcrumb.
    fn before_loading(&self, path: &Path) {
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
    }

    /// The thread that does the main-thread calls of CLAP plugins (started the first time it is wanted).
    fn the_clap_worker(&self) -> std::sync::Arc<clap_host::MainThread> {
        self.clap_worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(clap_host::MainThread::start)
            .clone()
    }

    /// The environment variables that a **program of its own** that loads the library `library` (the scan program, the program that shows a plugin's window) is started with, so that
    /// it looks for the libraries that the plugin library needs where this host does: the folders a person said, and the parents of the library's folder if the setting says so,
    /// before what the variable has already. (Empty when there are none.)
    pub fn environment_for_a_program_that_loads(&self, library: &Path) -> Vec<(String, String)> {
        let search = self.search.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let folders = dependency_search_folders(library, &search.dependency_folders, search.parent_folders);
        if folders.is_empty() {
            return Vec::new();
        }
        let variable = crate::plugin_scan::library_search_variable();
        let existing = std::env::var_os(variable).map(|value| std::env::split_paths(&value).collect::<Vec<_>>());
        let all = folders.into_iter().chain(existing.into_iter().flatten());
        std::env::join_paths(all)
            .ok()
            .and_then(|value| value.into_string().ok())
            .map(|value| vec![(variable.to_owned(), value)])
            .unwrap_or_default()
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
        voice: &Voice<'_>,
    ) -> Option<Vec<Box<dyn PluginInstance>>> {
        let PluginAddress::Ladspa { index, .. } = plugin.address else {
            return None;
        };
        let mut instances = Vec::new();
        for _ in 0..layout.instances() {
            match library.instantiate(index, SAMPLE_RATE) {
                Ok(instance) => {
                    if !this_is_the_plugin_that_was_scanned(&instance, plugin) {
                        voice.problem(
                            format!(
                                "The sound plugin \u{ab}{}\u{bb} is not what it was when the plugins were scanned (the library {} has changed): scan again.",
                                plugin.descriptor.name,
                                plugin.library.display()
                            ),
                        );
                        return None;
                    }
                    instances.push(Box::new(LadspaCopy(instance)) as Box<dyn PluginInstance>);
                }
                Err(error) => {
                    voice.problem(
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

impl PluginHost {
    /// As many copies of the LV2 plugin `description` of `library` as the layout needs, each made at the rate of the sound engine and given its state if the project has one, or why
    /// not (said in the notes).
    fn the_lv2_instances_of(
        &self,
        plugin: &'static InstalledPlugin,
        description: &lv2_host::PluginDescription,
        layout: Layout,
        library: &lv2_host::Library,
        state: Option<&std::sync::Arc<[u8]>>,
        voice: &Voice<'_>,
    ) -> Option<Vec<Box<dyn PluginInstance>>> {
        if !this_lv2_plugin_is_the_one_that_was_scanned(description, plugin) {
            voice.problem(format!(
                "The sound plugin \u{ab}{}\u{bb} is not what it was when the plugins were scanned (the bundle {} has changed): scan again.",
                plugin.descriptor.name,
                plugin.library.display()
            ));
            return None;
        }
        let mut instances: Vec<Box<dyn PluginInstance>> = Vec::new();
        for _ in 0..layout.instances() {
            match lv2_host::Instance::new(
                library,
                description,
                f64::from(SAMPLE_RATE),
                super::instance::MOST_FRAMES_OF_AN_LV2_PLUGIN as u32,
            ) {
                Ok(mut instance) => {
                    // The state before the values: what the project says the values are is what they are, whatever the state says.
                    if let Some(bytes) = state
                        && instance.has_state()
                        && let Err(error) = instance.restore_state(bytes)
                    {
                        voice.problem(format!(
                            "The sound plugin \u{ab}{}\u{bb} did not take the state that the project has for it ({error}): it goes on with its values.",
                            plugin.descriptor.name
                        ));
                    }
                    instances.push(Box::new(Lv2Copy(instance)));
                }
                Err(error) => {
                    voice.problem(format!(
                        "The sound plugin \u{ab}{}\u{bb} could not be made: {error}. The sound goes through it unchanged.",
                        plugin.descriptor.name
                    ));
                    return None;
                }
            }
        }
        Some(instances)
    }

    /// As many copies of the CLAP plugin `id` of `library` as the layout needs, each activated at the rate of the sound engine, or why not (said in the notes).
    fn the_clap_instances_of(
        &self,
        plugin: &'static InstalledPlugin,
        id: &str,
        layout: Layout,
        library: &clap_host::Library,
        state: Option<&std::sync::Arc<[u8]>>,
        voice: &Voice<'_>,
    ) -> Option<Vec<Box<dyn PluginInstance>>> {
        let worker = self.the_clap_worker();
        let stream = stream_of(voice.notes);
        // A stream that is being stopped makes no more plugins (it is about to let go of those it has, and a plugin made now would be alive with the ones of the stream that took its
        // place): the sound of it is not wanted.
        if self
            .lives
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_stopping(stream)
        {
            return None;
        }
        self.wait_for_what_is_being_let_go_of(&plugin.library, stream);
        let mut instances: Vec<Box<dyn PluginInstance>> = Vec::new();
        for _ in 0..layout.instances() {
            // (Alive from the moment it begins to be made, so that a stream that begins to be made while it is is made to wait for it.)
            let alive = Lives::is_alive(&self.lives, &plugin.library, stream);
            let maker = ClapMaker {
                worker: std::sync::Arc::clone(&worker),
                library: library.clone(),
                id: id.to_owned(),
                state: state.cloned(),
            };
            match maker.make_and_say_if_the_state_was_not_taken() {
                Ok((instance, state_not_taken)) => {
                    if let Some(why) = state_not_taken {
                        voice.problem(format!(
                            "The sound plugin \u{ab}{}\u{bb} did not take the state that the project has for it ({why}): it goes on with its values.",
                            plugin.descriptor.name
                        ));
                    }
                    if !this_clap_plugin_is_the_one_that_was_scanned(&instance, plugin) {
                        voice.problem(
                            format!(
                                "The sound plugin \u{ab}{}\u{bb} is not what it was when the plugins were scanned (the library {} has changed): scan again.",
                                plugin.descriptor.name,
                                plugin.library.display()
                            ),
                        );
                        return None;
                    }
                    instances.push(Box::new(ClapCopy::new(instance, maker, alive)));
                }
                Err(error) => {
                    voice.problem(
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

/// What names a stream to the host: where the notes of its plugins are kept, which is one for each stream.
fn stream_of(notes: &Notes) -> usize {
    std::sync::Arc::as_ptr(notes) as *const () as usize
}

impl PluginHost {
    /// Waits, for no longer than [`Self::waiting_no_longer_than`] says, until no plugin of `library` is alive for a stream that is being stopped (other than `stream`, which is the
    /// one that wants the plugin). **A plugin may not allow two of itself to be alive at once**, whatever CLAP says: a reverb of the maintainer's began to roar, loud and fading as a
    /// reverb does, when a plugin was made while the one before it, of the stream of a seek, was still being let go of in the background; and it did so for 10% of the seeks. The
    /// stream that is being stopped lets go of its plugins within a few blocks, so this is a wait of milliseconds. Plugins of streams that are not being stopped (two clips that
    /// are heard together) are not waited for: they are meant to be alive together.
    fn wait_for_what_is_being_let_go_of(&self, library: &Path, stream: usize) {
        let deadline = std::time::Instant::now() + self.longest_wait_for_what_is_being_let_go_of;
        loop {
            let waiting = self
                .lives
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_something_of_a_stopping_stream_alive(library, stream);
            if !waiting || std::time::Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// Whether what the library gave for the CLAP plugin is the plugin that was scanned: the main ports have the channels that were scanned, and the plugin has every parameter that a
/// value of the effect is the value of (it may have more).
fn this_clap_plugin_is_the_one_that_was_scanned(instance: &clap_host::Instance, plugin: &InstalledPlugin) -> bool {
    let channels = |ports: &[clap_host::scan::AudioPort]| ports.first().map_or(0, |port| port.channel_count as usize);
    (channels(instance.audio_inputs()), channels(instance.audio_outputs()))
        == (plugin.audio_inputs, plugin.audio_outputs)
        && plugin
            .value_handles
            .iter()
            .all(|handle| instance.parameters().iter().any(|parameter| parameter.id == *handle))
}

/// Where a plugin says what is wrong with it: in the notes of the play, and on the board that the window reads.
struct Voice<'a> {
    notes: &'a Notes,
    board: &'a ProblemBoard,
    key: &'a str,
}

impl Voice<'_> {
    fn problem(&self, line: String) {
        self.board
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(self.key.to_owned(), line.clone());
        say(self.notes, line);
    }
}

fn say(notes: &Notes, line: String) {
    let mut notes = notes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !notes.contains(&line) {
        notes.push(line);
    }
}

/// Whether what the bundle says now of the LV2 plugin is the plugin that was scanned: the same audio ports and the same control inputs for the values.
fn this_lv2_plugin_is_the_one_that_was_scanned(
    description: &lv2_host::PluginDescription,
    plugin: &InstalledPlugin,
) -> bool {
    use lv2_host::{PortDirection, PortKind};
    let count = |direction: PortDirection| {
        description
            .ports
            .iter()
            .filter(|port| port.kind == PortKind::Audio && port.direction == direction)
            .count()
    };
    (count(PortDirection::Input), count(PortDirection::Output)) == (plugin.audio_inputs, plugin.audio_outputs)
        && plugin.value_handles.iter().all(|&index| {
            description
                .ports
                .get(index as usize)
                .is_some_and(|port| port.direction == PortDirection::Input && port.kind == PortKind::Control)
        })
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
        && plugin.value_handles.iter().all(|&port| {
            description
                .ports
                .get(port as usize)
                .is_some_and(|port| port.direction == PortDirection::Input && port.kind == PortKind::Control)
        })
}

impl PluginRunner for PluginHost {
    fn generation(&self) -> u64 {
        PluginHost::generation(self)
    }

    fn stream_is_stopping(&self, notes: &Notes) {
        self.lives
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stream_is_stopping(stream_of(notes), notes);
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
        let voice = Voice {
            notes,
            board: &self.problems,
            key: &plugin.descriptor.key,
        };
        // (What was wrong the last time is not known to be wrong now: it is said again if it is.)
        self.problems
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&*plugin.descriptor.key);
        let layout = Layout::of(plugin.audio_inputs, plugin.audio_outputs)?;
        let folder = folder_of(&plugin.library);
        let library_name = plugin
            .library
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        if !self.may_run_from(&folder, &library_name, &plugin.descriptor.name)? {
            return None;
        }
        let could_not_be_loaded = |error: String| {
            voice.problem(
                format!(
                    "The sound plugin \u{ab}{}\u{bb} could not be loaded from {}: {error}. The sound goes through it unchanged.",
                    plugin.descriptor.name,
                    plugin.library.display()
                ),
            );
        };
        let instances = match &plugin.address {
            PluginAddress::Ladspa { .. } => {
                let library = self
                    .library(&plugin.library)
                    .inspect_err(|error| could_not_be_loaded(error.clone()))
                    .ok()?;
                self.the_instances_of(plugin, layout, &library, &voice)?
            }
            PluginAddress::Lv2 { uri } => {
                let bundle = self
                    .lv2_bundle(&plugin.library)
                    .inspect_err(|error| could_not_be_loaded(error.clone()))
                    .ok()?;
                let Some(description) = bundle.plugins.iter().find(|candidate| candidate.uri == *uri) else {
                    could_not_be_loaded(format!("the bundle does not describe a plugin called {uri} any more"));
                    return None;
                };
                let library = self
                    .lv2_library(&description.binary)
                    .inspect_err(|error| could_not_be_loaded(error.clone()))
                    .ok()?;
                self.the_lv2_instances_of(plugin, description, layout, &library, effect.state.as_ref(), &voice)?
            }
            PluginAddress::Clap { id } => {
                let library = self
                    .clap_library(&plugin.library)
                    .inspect_err(|error| could_not_be_loaded(error.clone()))
                    .ok()?;
                self.the_clap_instances_of(plugin, id, layout, &library, effect.state.as_ref(), &voice)?
            }
        };
        PluginEffect::new(
            instances,
            plugin,
            effect.clone(),
            frame_rate,
            clip.first_timeline_frame,
            is_for_the_preview,
            std::sync::Arc::clone(notes),
        )
        .map(|made| {
            Box::new(made.reporting_problems_on(std::sync::Arc::clone(&self.problems))) as Box<dyn BlockProcessor>
        })
    }
}
