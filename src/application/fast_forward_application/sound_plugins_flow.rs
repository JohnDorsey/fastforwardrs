// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the sound plugins that are installed, for the editor (phase 7, piece 7.5; the scan is in `plugin_scan`, and its program is a program of its own).
//!
//! The scan runs **on a thread of its own**, and never blocks the window: the thread runs the scan program on every plugin library in the folders (the libraries that did not
//! change since the last scan are not scanned again), and when it has finished the window is woken and takes the result: the plugins that can be effects become the effects that
//! are installed (see `project::set_installed_plugins`), so that the menu of the plus button offers them and the effects of a project that are of them work. A scan is started
//! when the editor starts (if it can find its scan program), and when a person asks for it or changes where to look.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::application::plugin_manager_bundles::{BundleKind, BundlesView};
use crate::application::plugin_manager_window::{PluginManagerCommand, PluginManagerView};
use crate::plugin_host::PluginHost;
use crate::plugin_scan::ScanFailure;
use crate::plugin_scan::{
    Breadcrumb, LibraryOutcome, LibraryReport, NotUsed, PluginScanner, PluginSearchSettings, ScanCache,
    bundled_plugins_folder, folders_looked_in_by_default, plugins_found_in, recover, scan_a_library_alone,
    scanner_program_beside_this_program,
};
use crate::plugin_scan::{is_a_clap_file, is_an_lv2_bundle};
use crate::plugin_trust::{FolderStanding, folder_for_a_person, folder_of};
use crate::project::{EffectKind, set_installed_plugins, set_installed_shaders};
use crate::shader_files::{
    ShaderScan, scan_folders as scan_the_folders_for_shaders, usual_folders as usual_folders_of_shaders,
};

/// What a scan found, for the window to show.
#[derive(Default)]
pub(super) struct ScanResult {
    pub(super) libraries: Vec<LibraryReport>,
    pub(super) not_used: Vec<NotUsed>,
    pub(super) effect_count: usize,
    pub(super) problems: Vec<String>,
    /// The shaders that were found in the folders, each with what became of it (the usable ones are the installed shaders).
    pub(super) shaders: ShaderScan,
}

/// What the thread of a scan gives back: the libraries that were found (and what went wrong that was not a library's doing) and the shaders.
struct ScanDone {
    libraries: Vec<LibraryReport>,
    problems: Vec<String>,
    shaders: ShaderScan,
}

/// What the editor knows of the plugins that are installed.
#[derive(Default)]
pub(super) struct PluginSearch {
    /// Where a person said to look.
    pub(super) settings: PluginSearchSettings,
    settings_file: Option<PathBuf>,
    /// The scan program. `None` where it cannot be found: nothing is scanned, and the window says so.
    scanner_program: Option<PathBuf>,
    /// Where what the scans found is kept. `None` keeps it only while the program runs.
    cache_file: Option<PathBuf>,
    /// Whether the folders of `LADSPA_PATH` and `CLAP_PATH` and the usual ones are looked in. (They are, for the program a person uses; the tests ask for it where they mean to.)
    look_in_the_usual_places: bool,
    /// The scan that is going on, if one is.
    running: Option<Receiver<ScanDone>>,
    /// Whether the editor looked for its scan program and did not find it (the window says so, with the problems of the scans).
    scanner_missing: bool,
    /// Whether this editor put shaders in the list of the installed ones (so that it takes them out again when they are gone, and never writes the list when it has put nothing in it:
    /// the list is one for the whole program).
    shaders_installed: bool,
    /// A scan was asked for while another was going on: another is started when that one is taken.
    another_wanted: bool,
    pub(super) has_scanned: bool,
    pub(super) result: ScanResult,
    /// What the scans of the plugin libraries that were written out of the project file that is open found (the ones a person said yes to: see `plugin_flow`). They are installed
    /// after the plugins of the folders, which win where a key is the same, and are forgotten when another project is opened.
    pub(super) embedded: Vec<LibraryReport>,
    /// The scans of those that are going on, each with the file it is of.
    embedded_running: Vec<Receiver<(PathBuf, Result<LibraryReport, String>)>>,
    /// The plugin libraries inside the project that is open that are not loaded (nobody has said yes to them, or the person said no, or they could not be written out): a plugin
    /// that is not installed may be in one of them, and the window of the effect says so.
    pub(super) embedded_not_loaded: Vec<String>,
    /// The *Bundles* tab: the plugins that can be fetched from their authors.
    pub(super) bundles: super::plugin_bundles_flow::Bundles,
}

impl PluginSearch {
    /// What the window says went wrong that was not a library's doing: that there is no scan program (if there is not), and what the scans said.
    fn problems_to_show(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.scanner_missing {
            problems.push(
                "The program that scans plugin libraries (fastforwardrs-plugin-scan) was not found beside the editor, so no plugins are looked for (shaders are).".to_owned(),
            );
        }
        problems.extend(self.result.problems.iter().cloned());
        problems
    }

    /// The folders that are looked in besides those a person added.
    fn other_folders(&self) -> Vec<PathBuf> {
        if !self.look_in_the_usual_places {
            return Vec::new();
        }
        folders_looked_in_by_default()
    }

    /// The folders that are looked in for shaders besides those a person added: the usual places of ISF, and the folder that the *Bundles* tab fetches shaders into.
    fn other_folders_for_shaders(&self) -> Vec<PathBuf> {
        let mut folders = Vec::new();
        if self.look_in_the_usual_places {
            folders.extend(usual_folders_of_shaders());
        }
        // (The folder that the tab fetches into is looked in when the usual places are, and when the tab was told another folder to fetch into, as the tests do.)
        let fetched = match &self.bundles.shader_folder {
            Some(folder) => Some(folder.clone()),
            None if self.look_in_the_usual_places => crate::plugin_bundles::fetched_shaders_folder(),
            None => None,
        };
        folders.extend(fetched);
        folders
    }

    /// The folders that shaders are looked for in: a person's, then the usual places of ISF and the folder of the shaders that were fetched.
    fn folders_to_scan_for_shaders(&self) -> Vec<PathBuf> {
        let mut all = self.settings.folders.clone();
        for folder in self.other_folders_for_shaders() {
            if !all.contains(&folder) {
                all.push(folder);
            }
        }
        all
    }

    /// All the folders, in the order of who wins: a person's, then `LADSPA_PATH` and the usual ones of LADSPA, then `CLAP_PATH` and the usual ones of CLAP.
    fn folders_to_scan(&self) -> Vec<PathBuf> {
        let mut all = self.settings.folders.clone();
        for folder in self.other_folders() {
            if !all.contains(&folder) {
                all.push(folder);
            }
        }
        all
    }
}

impl PluginSearch {
    /// Why the effect of the kind `kind` does nothing at all now, if it does not, for a sound plugin (a key of a plugin: `ladspa:` or `clap:`): it is not installed (the scan is not done, did not find it, found its library
    /// failed or found it unfit to be an effect), or it is installed and its folder is not trusted, or it was wanted in the sound and went wrong. `None` for a kind that is not a
    /// sound plugin and for one that works. **A plugin that is not run is never silent about it.**
    pub(super) fn why_a_plugin_effect_does_nothing(&self, kind: EffectKind, host: &PluginHost) -> Option<String> {
        if let Some(plugin) = kind.installed_plugin() {
            let folder = folder_of(&plugin.library);
            let folder_for_a_person = folder_for_a_person(&folder);
            return match host.standing_of(&folder) {
                FolderStanding::NotAsked => Some(format!(
                    "You have not said whether to trust the folder {folder_for_a_person}, so the plugin is not run."
                )),
                FolderStanding::Declined => Some(format!(
                    "You did not trust the folder {folder_for_a_person}, so the plugin is not run."
                )),
                FolderStanding::Approved => host.problem_with(kind.key()),
            };
        }
        let key = kind.key();
        let (namespace, rest) = key.split_once(':')?;
        if namespace != "ladspa" && namespace != "clap" && namespace != "lv2" {
            return None;
        }
        Some(self.why_a_plugin_is_not_installed(namespace, rest))
    }

    /// Why no plugin stands in for the effect whose key is `namespace:rest`.
    fn why_a_plugin_is_not_installed(&self, namespace: &str, rest: &str) -> String {
        let why = self.why_a_plugin_is_not_in_the_folders(namespace, rest);
        if self.embedded_not_loaded.is_empty() {
            return why;
        }
        format!(
            "{why}\n\nThe project has plugin libraries inside it that are not loaded ({}): the plugin may be in one of them. They are asked about when the project is opened, and a plugin inside a project is loaded only if you hold down \u{ab}Load it\u{bb}.",
            self.embedded_not_loaded.join(", ")
        )
    }

    /// Why no plugin stands in for the effect whose key is `namespace:rest`, of the plugins in the folders that are looked in.
    fn why_a_plugin_is_not_in_the_folders(&self, namespace: &str, rest: &str) -> String {
        if self.scanner_program.is_none() {
            return "The program that scans plugin libraries was not found beside the editor, so no plugins are looked for.".to_owned();
        }
        if self.running.is_some() || !self.has_scanned || !self.embedded_running.is_empty() {
            return "The plugins are still being looked for.".to_owned();
        }
        let named = |path: &Path| {
            path.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        };
        let stem_of = |path: &Path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().replace(':', "_"))
                .unwrap_or_default()
        };
        // (A LADSPA key is `ladspa:` the file without its ending, a colon and the label; a CLAP key is `clap:` and the id, which names no file.)
        let (library_stem, label) = match namespace {
            "ladspa" => match rest.split_once(':') {
                Some((stem, label)) => (Some(stem), label),
                None => (None, rest),
            },
            _ => (None, rest),
        };
        let in_the_library = |path: &Path| library_stem.is_none_or(|stem| stem_of(path) == stem);
        let own_kind = |path: &Path| match namespace {
            "lv2" => is_an_lv2_bundle(path),
            "clap" => is_a_clap_file(path),
            _ => !is_a_clap_file(path) && !is_an_lv2_bundle(path),
        };
        // The plugin was found, and cannot be an effect.
        if let Some(not_used) = self.result.not_used.iter().find(|not_used| {
            not_used.label.replace(':', "_") == label
                && in_the_library(&not_used.library)
                && own_kind(&not_used.library)
        }) {
            return format!(
                "The plugin \u{ab}{}\u{bb} in {} was found, and cannot be put on a clip: {}",
                not_used.name,
                named(&not_used.library),
                not_used.reason
            );
        }
        // Its library failed (it crashed the scan, took too long, ended the editor badly, could not be loaded...).
        let failed: Vec<(&LibraryReport, &ScanFailure)> = self
            .result
            .libraries
            .iter()
            .filter(|report| in_the_library(&report.path) && own_kind(&report.path))
            .filter_map(|report| match &report.outcome {
                LibraryOutcome::Failed(failure) => Some((report, failure)),
                _ => None,
            })
            .collect();
        if library_stem.is_some()
            && let Some((report, failure)) = failed.first()
        {
            return format!("Its library {} failed: {failure}.", named(&report.path));
        }
        // The library is there and has no such plugin.
        if library_stem.is_some()
            && let Some(report) = self
                .result
                .libraries
                .iter()
                .find(|report| in_the_library(&report.path) && own_kind(&report.path))
        {
            return format!(
                "The library {} is there, and has no plugin called {label}.",
                named(&report.path)
            );
        }
        if !failed.is_empty() {
            let mut which: Vec<String> = failed
                .iter()
                .take(3)
                .map(|(report, failure)| format!("{} ({failure})", named(&report.path)))
                .collect();
            if failed.len() > 3 {
                which.push(format!("and {} more", failed.len() - 3));
            }
            return format!(
                "No plugin like it was found, and {} failed in the last scan, which may be where it is: {}.",
                if failed.len() == 1 {
                    "a library"
                } else {
                    "some libraries"
                },
                which.join(", ")
            );
        }
        match library_stem {
            Some(stem) => {
                format!("No library called {stem} was found in the folders that are looked in (see Plugin manager...).")
            }
            None => {
                "No plugin with this id was found in the folders that are looked in (see Plugin manager...).".to_owned()
            }
        }
    }
}

impl FastForwardApplication {
    /// Keeps where a person said to look for plugins in the file at `file` (and reads what is in it).
    pub fn keep_plugin_search_settings_in(&mut self, file: PathBuf) {
        let (settings, problem) = PluginSearchSettings::read(&file);
        self.environment.plugin_search.settings = settings;
        self.environment.plugin_search.settings_file = Some(file);
        self.tell_the_host_where_to_look_for_libraries();
        if let Some(problem) = problem {
            self.add_to_message(
                "Where to look for sound plugins could not be read",
                format!("{problem}\n\nThe usual places are being used until you change them."),
            );
        }
    }

    /// Uses the program `program` to scan plugin libraries, and keeps what the scans found in `cache_file` (or only while the program runs). The usual places for plugins are
    /// looked in as well as those a person added if `look_in_the_usual_places`.
    pub fn use_this_plugin_scanner(
        &mut self,
        program: PathBuf,
        cache_file: Option<PathBuf>,
        look_in_the_usual_places: bool,
    ) {
        let search = &mut self.environment.plugin_search;
        search.scanner_program = Some(program);
        search.cache_file = cache_file;
        search.look_in_the_usual_places = look_in_the_usual_places;
    }

    /// For the editor that a person uses: the scan program beside this program, the cache beside the settings, the usual places; and a scan now.
    pub(super) fn find_plugins_the_way_a_person_uses_the_editor(&mut self) {
        if let Some(file) = PluginSearchSettings::default_file() {
            self.keep_plugin_search_settings_in(file);
        }
        // (The folder that the Bundles tab fetches into is trusted before it is there: what is put in it is checked against the checksums of the program.)
        for folder in bundled_plugins_folder()
            .into_iter()
            .chain(crate::plugin_bundles::fetched_plugins_folder())
        {
            self.trust_the_plugins_bundled_in(&folder);
        }
        match scanner_program_beside_this_program() {
            Some(program) => {
                self.use_this_plugin_scanner(program, ScanCache::default_file(), true);
                if let Some(file) = Breadcrumb::default_file() {
                    self.keep_a_breadcrumb_of_the_plugins_in_use_in(file);
                }
                self.start_scanning_plugins();
            }
            None => {
                // (Said by the window, whatever the scans find: see `PluginSearch::problems_to_show`.)
                self.environment.plugin_search.scanner_missing = true;
                // (The shaders are looked for all the same: a shader is read by the editor itself, and needs no scan program.)
                self.environment.plugin_search.look_in_the_usual_places = true;
                self.start_scanning_plugins();
            }
        }
    }

    /// Says that the plugins that came with the program are in `folder`: they are trusted without the question that other folders are asked (they are the program's own files; the
    /// licences and the source of each are with them). The folder is looked in by the scan that is made of the usual places.
    pub fn trust_the_plugins_bundled_in(&mut self, folder: &Path) {
        self.environment.plugin_host.trust_bundled_folder(folder);
    }

    /// Looks at what the last run left in `file`: if it ended while it was using a plugin library, that library is failed (it is not loaded again until it changes) and a message
    /// says so; then the libraries that this run uses are written there, each before it is loaded, until the editor ends well. (Needs the scan program, which is the one that keeps
    /// what is known of libraries: call it after [`Self::use_this_plugin_scanner`].)
    pub fn keep_a_breadcrumb_of_the_plugins_in_use_in(&mut self, file: PathBuf) {
        let search = &self.environment.plugin_search;
        if let Some(program) = search.scanner_program.clone() {
            let cache = search
                .cache_file
                .clone()
                .map_or_else(ScanCache::in_memory, ScanCache::at);
            let mut scanner = PluginScanner::new(program, cache);
            if let Some(recovery) = recover(&file, &mut scanner) {
                let others = if recovery.also_in_use.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (it was using {} other librar{} too, which are named in the file of scans)",
                        recovery.also_in_use.len(),
                        if recovery.also_in_use.len() == 1 { "y" } else { "ies" }
                    )
                };
                self.add_to_message(
                    "The editor ended badly",
                    format!(
                        "The last time the editor ran, it ended while it was using the sound plugin library {}{others}. That library will not be loaded again until the file changes; the Plugin manager says so.",
                        recovery.failed.display()
                    ),
                );
            }
        }
        self.environment.plugin_host.keep_a_breadcrumb(Breadcrumb::at(file));
    }

    /// Tells what runs the plugins where the libraries that plugin libraries need are looked for.
    fn tell_the_host_where_to_look_for_libraries(&self) {
        let settings = &self.environment.plugin_search.settings;
        self.environment
            .plugin_host
            .look_for_dependencies_in(settings.dependency_folders.clone(), settings.search_parent_folders);
    }

    /// Starts a scan of the folders on a thread of its own, if there is something to scan with and none is going on (if one is, another follows it): the plugin libraries, with the
    /// scan program if there is one, and the shaders, which need no program (a shader is read, and compiled by the host in this process).
    pub fn start_scanning_plugins(&mut self) {
        let search = &mut self.environment.plugin_search;
        if search.running.is_some() {
            search.another_wanted = true;
            return;
        }
        let program = search.scanner_program.clone();
        let shader_folders = search.folders_to_scan_for_shaders();
        if program.is_none() && shader_folders.is_empty() {
            // (Nothing to look with and nothing to look in: the shaders that the folders that were taken away had are gone.)
            self.install_the_shaders_that_were_found(ShaderScan::default());
            return;
        }
        let folders = search.folders_to_scan();
        let dependency_folders = search.settings.dependency_folders.clone();
        let search_parents = search.settings.search_parent_folders;
        let cache = search
            .cache_file
            .clone()
            .map_or_else(ScanCache::in_memory, ScanCache::at);
        let wake_up = self.environment.wake_up.clone();
        let (sender, receiver) = mpsc::channel();
        let started = thread::Builder::new()
            .name("scanning plugins".to_owned())
            .spawn(move || {
                let (libraries, problems) = match program {
                    Some(program) => {
                        let mut scanner = PluginScanner::new(program, cache)
                            .looking_for_dependencies_in(dependency_folders, search_parents);
                        let scan = scanner.scan_folders(&folders);
                        (scan.libraries, scan.problems)
                    }
                    None => (Vec::new(), Vec::new()),
                };
                let shaders = scan_the_folders_for_shaders(&shader_folders);
                let _ = sender.send(ScanDone {
                    libraries,
                    problems,
                    shaders,
                });
                wake_up();
            });
        match started {
            Ok(_) => search.running = Some(receiver),
            Err(error) => search
                .result
                .problems
                .push(format!("The scan of the sound plugins could not be started: {error}")),
        }
    }

    /// Whether a scan of the plugins is going on.
    pub fn is_scanning_plugins(&self) -> bool {
        self.environment.plugin_search.running.is_some()
    }

    /// Takes what a scan found if it has finished: the plugins that can be effects are the effects that are installed from now on.
    pub(super) fn take_what_the_plugin_scan_found(&mut self) {
        let search = &mut self.environment.plugin_search;
        let Some(receiver) = &search.running else {
            return;
        };
        let done = match receiver.try_recv() {
            Ok(done) => done,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => ScanDone {
                libraries: Vec::new(),
                problems: vec!["The scan of the plugins ended without an answer.".to_owned()],
                shaders: ShaderScan::default(),
            },
        };
        search.running = None;
        self.apply_what_a_scan_found(done.libraries, done.problems, done.shaders);
        let search = &mut self.environment.plugin_search;
        if std::mem::take(&mut search.another_wanted) {
            self.start_scanning_plugins();
        }
    }

    /// Makes the plugins that can be effects, and the shaders that can be used, the effects that are installed, and keeps the rest for the window to show.
    fn apply_what_a_scan_found(&mut self, libraries: Vec<LibraryReport>, problems: Vec<String>, shaders: ShaderScan) {
        let search = &mut self.environment.plugin_search;
        search.result.libraries = libraries;
        search.result.problems = problems.into_iter().chain(shaders.problems.iter().cloned()).collect();
        search.has_scanned = true;
        self.install_the_shaders_that_were_found(shaders);
        self.install_the_plugins_that_were_found();
    }

    /// Makes the shaders of `scan` that can be used the shaders that are installed (a shader is data: it is installed by being found, and the same one found again is the kind of effect
    /// that it was), and keeps the scan for the window. **Nothing is written to the list of the installed ones by an editor that put nothing in it and has nothing to put.**
    fn install_the_shaders_that_were_found(&mut self, scan: ShaderScan) {
        let search = &mut self.environment.plugin_search;
        let kinds = scan.usable_kinds();
        if search.shaders_installed || !kinds.is_empty() {
            search.shaders_installed = !kinds.is_empty();
            set_installed_shaders(kinds);
        }
        search.result.shaders = scan;
    }

    /// Makes the plugins that the scans found, of the folders and of the libraries written out of the project file (those last: a plugin of a folder wins where a key is the same),
    /// the plugins that are installed.
    fn install_the_plugins_that_were_found(&mut self) {
        let search = &mut self.environment.plugin_search;
        let reports: Vec<LibraryReport> = search
            .result
            .libraries
            .iter()
            .chain(&search.embedded)
            .cloned()
            .collect();
        let found = plugins_found_in(&reports);
        search.result.not_used = found.not_used;
        search.result.effect_count = found.effects.len();
        // (What the host read of the bundles before is what they said then: a bundle may have changed, and the scan that has just ended is what says.)
        self.environment.plugin_host.forget_what_the_bundles_said();
        set_installed_plugins(found.effects);
        // (Effects of plugins that were not installed before are now: their folders are asked about, and the sound is made with them if the answer is yes.)
        self.ask_about_the_folders_the_sound_needs();
    }

    /// Starts the scan of the plugin library at `file`, which was written out of the project file after a person said yes to it, on a thread of its own (nothing is kept of it: the
    /// plugins it has are installed until another project is opened). Where there is no scan program nothing is scanned: the plugin manager says so.
    pub(super) fn start_scanning_an_embedded_plugin(&mut self, file: PathBuf) {
        let search = &mut self.environment.plugin_search;
        let Some(program) = search.scanner_program.clone() else {
            return;
        };
        let dependency_folders = search.settings.dependency_folders.clone();
        let search_parents = search.settings.search_parent_folders;
        let wake_up = self.environment.wake_up.clone();
        let (sender, receiver) = mpsc::channel();
        let started = thread::Builder::new()
            .name("scanning a plugin of the project".to_owned())
            .spawn(move || {
                let report = scan_a_library_alone(program, &file, dependency_folders, search_parents)
                    .map_err(|problem| problem.to_string());
                let _ = sender.send((file, report));
                wake_up();
            });
        match started {
            Ok(_) => search.embedded_running.push(receiver),
            Err(error) => search.result.problems.push(format!(
                "The scan of a plugin inside the project could not be started: {error}"
            )),
        }
    }

    /// Takes what the scans of the plugins written out of the project file found, if they have finished: the plugins that can be effects are installed from then on.
    pub(super) fn take_what_the_scans_of_embedded_plugins_found(&mut self) {
        let search = &mut self.environment.plugin_search;
        if search.embedded_running.is_empty() {
            return;
        }
        let mut done = Vec::new();
        search.embedded_running.retain(|receiver| match receiver.try_recv() {
            Ok(answer) => {
                done.push(answer);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        if done.is_empty() {
            return;
        }
        let mut said = Vec::new();
        for (file, answer) in done {
            let name = file
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            match answer {
                Ok(report) => {
                    if let LibraryOutcome::Failed(failure) = &report.outcome {
                        said.push(format!(
                            "\u{ab}{name}\u{bb}, inside the project, could not be used: {failure}. What uses it passes the sound through unchanged."
                        ));
                    }
                    search.embedded.retain(|known| known.path != file);
                    search.embedded.push(report);
                }
                Err(problem) => said.push(format!(
                    "\u{ab}{name}\u{bb}, inside the project, could not be looked at: {problem}"
                )),
            }
        }
        self.install_the_plugins_that_were_found();
        for line in said {
            self.add_to_message("A plugin inside the project", line);
        }
    }

    /// Lets go of the plugins that were written out of the project file that was open (another is being opened, and a plugin inside a project is asked about every time): they are
    /// not trusted any more and are not installed.
    pub(super) fn forget_the_embedded_plugins(&mut self) {
        self.environment.plugin_host.forget_the_embedded_plugins();
        let search = &mut self.environment.plugin_search;
        search.embedded_running.clear();
        search.embedded_not_loaded.clear();
        if !std::mem::take(&mut search.embedded).is_empty() {
            self.install_the_plugins_that_were_found();
        }
    }

    /// For the tests: as if the editor had looked for its scan program and not found it.
    #[cfg(test)]
    pub(super) fn pretend_the_scan_program_is_missing(&mut self) {
        self.environment.plugin_search.scanner_missing = true;
    }

    /// For the tests: as if a scan had found these libraries.
    #[cfg(test)]
    pub(super) fn pretend_the_scan_found(&mut self, libraries: Vec<LibraryReport>) {
        self.apply_what_a_scan_found(libraries, Vec::new(), ShaderScan::default());
    }

    /// What the plugin manager shows.
    pub(super) fn show_the_plugin_manager(&mut self, context: &egui::Context, commands: &mut Vec<Command>) {
        let mut other_folders = self.environment.plugin_search.other_folders();
        for folder in self.environment.plugin_search.other_folders_for_shaders() {
            if !other_folders.contains(&folder) {
                other_folders.push(folder);
            }
        }
        let bundled_folder = bundled_plugins_folder();
        let fetched_shaders_folder = crate::plugin_bundles::fetched_shaders_folder();
        let (rows, has_builds, total_bytes, hosts) = self.what_the_bundles_tab_shows(BundleKind::SoundPlugins);
        let (shader_rows, shaders_have_builds, shader_bytes, shader_hosts) =
            self.what_the_bundles_tab_shows(BundleKind::Shaders);
        let fetched_folder = self.folder_of_the_bundles();
        let shader_folder = self.folder_of_the_shader_bundles();
        let search = &self.environment.plugin_search;
        let problems = search.problems_to_show();
        let bundles = BundlesView {
            kind: BundleKind::SoundPlugins,
            rows: &rows,
            has_builds,
            total_bytes,
            hosts: &hosts,
            folder: fetched_folder.as_deref(),
            state: super::plugin_bundles_flow::state_of(&search.bundles.download),
        };
        let shader_bundles = BundlesView {
            kind: BundleKind::Shaders,
            rows: &shader_rows,
            has_builds: shaders_have_builds,
            total_bytes: shader_bytes,
            hosts: &shader_hosts,
            folder: shader_folder.as_deref(),
            state: super::plugin_bundles_flow::state_of(&search.bundles.shader_download),
        };
        let view = PluginManagerView {
            settings: &search.settings,
            other_folders: &other_folders,
            bundled_folder: bundled_folder.as_deref(),
            fetched_folder: fetched_folder.as_deref(),
            fetched_shaders_folder: fetched_shaders_folder.as_deref(),
            tab: self.interface.plugin_manager_tab,
            bundles,
            shader_bundles,
            is_scanning: search.running.is_some(),
            has_scanned: search.has_scanned,
            libraries: &search.result.libraries,
            not_used: &search.result.not_used,
            effect_count: search.result.effect_count,
            problems: &problems,
            shaders: &search.result.shaders,
        };
        crate::application::plugin_manager_window::show(
            context,
            &mut self.interface.plugin_manager_open,
            &view,
            commands,
        );
    }

    /// Does what the plugin manager asked for.
    pub(super) fn do_what_the_plugin_manager_asked(&mut self, command: PluginManagerCommand) {
        match command {
            PluginManagerCommand::Show => self.interface.plugin_manager_open = true,
            PluginManagerCommand::AddFolder => {
                if let Some(folder) = self.ask_for_a_folder("Choose a folder with plugins or shaders") {
                    self.add_plugin_folder(folder);
                }
            }
            PluginManagerCommand::RemoveFolder(number) => {
                let folders = &mut self.environment.plugin_search.settings.folders;
                if number < folders.len() {
                    folders.remove(number);
                    self.the_places_to_look_for_plugins_changed();
                }
            }
            PluginManagerCommand::AddDependencyFolder => {
                if let Some(folder) = self.ask_for_a_folder("Choose a folder with libraries that plugins need") {
                    self.add_dependency_folder(folder);
                }
            }
            PluginManagerCommand::RemoveDependencyFolder(number) => {
                let folders = &mut self.environment.plugin_search.settings.dependency_folders;
                if number < folders.len() {
                    folders.remove(number);
                    self.the_places_to_look_for_plugins_changed();
                }
            }
            PluginManagerCommand::SetSearchParentFolders(on) => {
                if self.environment.plugin_search.settings.search_parent_folders != on {
                    self.environment.plugin_search.settings.search_parent_folders = on;
                    self.the_places_to_look_for_plugins_changed();
                }
            }
            PluginManagerCommand::ScanAgain => self.start_scanning_plugins(),
            PluginManagerCommand::SelectTab(tab) => self.interface.plugin_manager_tab = tab,
            PluginManagerCommand::AskToDownloadBundles(kind) => self.ask_whether_to_download_the_bundles(kind),
            PluginManagerCommand::CancelAskingToDownloadBundles(kind) => self.do_not_download_the_bundles(kind),
            PluginManagerCommand::DownloadBundles(kind) => self.download_the_bundles(kind),
            PluginManagerCommand::CancelBundleDownload(kind) => self.cancel_the_download_of_the_bundles(kind),
            PluginManagerCommand::DismissBundleMessage(kind) => self.dismiss_what_the_download_said(kind),
        }
    }

    /// Adds `folder` to those plugins are looked for in (what choosing a folder with the button of the window does), and looks again.
    pub fn add_plugin_folder(&mut self, folder: PathBuf) {
        let folders = &mut self.environment.plugin_search.settings.folders;
        if !folders.contains(&folder) {
            folders.push(folder);
            self.the_places_to_look_for_plugins_changed();
        }
    }

    /// Adds `folder` to those the libraries that plugins need are looked for in, and looks again.
    pub fn add_dependency_folder(&mut self, folder: PathBuf) {
        let folders = &mut self.environment.plugin_search.settings.dependency_folders;
        if !folders.contains(&folder) {
            folders.push(folder);
            self.the_places_to_look_for_plugins_changed();
        }
    }

    fn ask_for_a_folder(&mut self, title: &str) -> Option<PathBuf> {
        self.pause_playback_for_dialog();
        self.environment
            .file_chooser
            .choose_a_folder(&crate::application::file_chooser::FileQuestion {
                title,
                suggested_file_name: None,
                starting_directory: None,
                kinds: &[],
            })
    }

    /// Writes where to look to its file and looks again.
    fn the_places_to_look_for_plugins_changed(&mut self) {
        self.tell_the_host_where_to_look_for_libraries();
        if let Some(file) = self.environment.plugin_search.settings_file.clone()
            && let Err(error) = self.environment.plugin_search.settings.write(&file)
        {
            self.show_message(
                "Where to look for sound plugins could not be saved",
                format!("{}: {error}", file.display()),
            );
        }
        self.start_scanning_plugins();
    }

    /// Where a person said to look for plugins.
    pub fn plugin_search_settings(&self) -> &PluginSearchSettings {
        &self.environment.plugin_search.settings
    }

    /// What the last scan of the plugins found.
    pub fn plugin_libraries_found(&self) -> &[LibraryReport] {
        &self.environment.plugin_search.result.libraries
    }

    /// The problems of the last scan that were not a library's.
    pub fn plugin_scan_problems(&self) -> &[String] {
        &self.environment.plugin_search.result.problems
    }

    /// Whether the plugin manager is open.
    pub fn plugin_manager_is_open(&self) -> bool {
        self.interface.plugin_manager_open
    }
}
