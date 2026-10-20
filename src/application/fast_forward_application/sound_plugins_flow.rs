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
use crate::application::sound_plugins_window::{SoundPluginsCommand, SoundPluginsView};
use crate::plugin_host::PluginHost;
use crate::plugin_scan::ScanFailure;
use crate::plugin_scan::is_a_clap_file;
use crate::plugin_scan::{
    Breadcrumb, LibraryOutcome, LibraryReport, NotUsed, PluginScanner, PluginSearchSettings, ScanCache,
    folders_looked_in_by_default, plugins_found_in, recover, scanner_program_beside_this_program,
};
use crate::plugin_trust::{FolderStanding, folder_for_a_person, folder_of};
use crate::project::{EffectKind, set_installed_plugins};

/// What a scan found, for the window to show.
#[derive(Default)]
pub(super) struct ScanResult {
    pub(super) libraries: Vec<LibraryReport>,
    pub(super) not_used: Vec<NotUsed>,
    pub(super) effect_count: usize,
    pub(super) problems: Vec<String>,
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
    running: Option<Receiver<(Vec<LibraryReport>, Vec<String>)>>,
    /// A scan was asked for while another was going on: another is started when that one is taken.
    another_wanted: bool,
    pub(super) has_scanned: bool,
    pub(super) result: ScanResult,
}

impl PluginSearch {
    /// The folders that are looked in besides those a person added.
    fn other_folders(&self) -> Vec<PathBuf> {
        if !self.look_in_the_usual_places {
            return Vec::new();
        }
        folders_looked_in_by_default()
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
        if namespace != "ladspa" && namespace != "clap" {
            return None;
        }
        Some(self.why_a_plugin_is_not_installed(namespace, rest))
    }

    /// Why no plugin stands in for the effect whose key is `namespace:rest`.
    fn why_a_plugin_is_not_installed(&self, namespace: &str, rest: &str) -> String {
        if self.scanner_program.is_none() {
            return "The program that scans plugin libraries was not found beside the editor, so no plugins are looked for.".to_owned();
        }
        if self.running.is_some() || !self.has_scanned {
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
        let own_kind = |path: &Path| is_a_clap_file(path) == (namespace == "clap");
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
                format!("No library called {stem} was found in the folders that are looked in (see Sound plugins...).")
            }
            None => {
                "No plugin with this id was found in the folders that are looked in (see Sound plugins...).".to_owned()
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
        match scanner_program_beside_this_program() {
            Some(program) => {
                self.use_this_plugin_scanner(program, ScanCache::default_file(), true);
                if let Some(file) = Breadcrumb::default_file() {
                    self.keep_a_breadcrumb_of_the_plugins_in_use_in(file);
                }
                self.start_scanning_plugins();
            }
            None => {
                self.environment.plugin_search.result.problems.push(
                    "The program that scans plugin libraries (fastforwardrs-plugin-scan) was not found beside the editor, so no plugins are looked for.".to_owned(),
                );
            }
        }
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
                        "The last time the editor ran, it ended while it was using the sound plugin library {}{others}. That library will not be loaded again until the file changes; the Sound plugins window says so.",
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

    /// Starts a scan of the folders on a thread of its own, if there is a scan program and none is going on (if one is, another follows it).
    pub fn start_scanning_plugins(&mut self) {
        let search = &mut self.environment.plugin_search;
        if search.running.is_some() {
            search.another_wanted = true;
            return;
        }
        let Some(program) = search.scanner_program.clone() else {
            return;
        };
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
                let mut scanner =
                    PluginScanner::new(program, cache).looking_for_dependencies_in(dependency_folders, search_parents);
                let scan = scanner.scan_folders(&folders);
                let _ = sender.send((scan.libraries, scan.problems));
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
        let (libraries, problems) = match receiver.try_recv() {
            Ok(done) => done,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => (
                Vec::new(),
                vec!["The scan of the sound plugins ended without an answer.".to_owned()],
            ),
        };
        search.running = None;
        self.apply_what_a_scan_found(libraries, problems);
        let search = &mut self.environment.plugin_search;
        if std::mem::take(&mut search.another_wanted) {
            self.start_scanning_plugins();
        }
    }

    /// Makes the plugins that can be effects the effects that are installed, and keeps the rest for the window to show.
    fn apply_what_a_scan_found(&mut self, libraries: Vec<LibraryReport>, problems: Vec<String>) {
        let search = &mut self.environment.plugin_search;
        let found = plugins_found_in(&libraries);
        search.result = ScanResult {
            libraries,
            not_used: found.not_used,
            effect_count: found.effects.len(),
            problems,
        };
        search.has_scanned = true;
        set_installed_plugins(found.effects);
        // (Effects of plugins that were not installed before are now: their folders are asked about, and the sound is made with them if the answer is yes.)
        self.ask_about_the_folders_the_sound_needs();
    }

    /// For the tests: as if a scan had found these libraries.
    #[cfg(test)]
    pub(super) fn pretend_the_scan_found(&mut self, libraries: Vec<LibraryReport>) {
        self.apply_what_a_scan_found(libraries, Vec::new());
    }

    /// What the window of sound plugins shows.
    pub(super) fn show_the_window_of_sound_plugins(&mut self, context: &egui::Context, commands: &mut Vec<Command>) {
        let other_folders = self.environment.plugin_search.other_folders();
        let search = &self.environment.plugin_search;
        let view = SoundPluginsView {
            settings: &search.settings,
            other_folders: &other_folders,
            is_scanning: search.running.is_some(),
            has_scanned: search.has_scanned,
            libraries: &search.result.libraries,
            not_used: &search.result.not_used,
            effect_count: search.result.effect_count,
            problems: &search.result.problems,
        };
        crate::application::sound_plugins_window::show(
            context,
            &mut self.interface.sound_plugins_window_open,
            &view,
            commands,
        );
    }

    /// Does what the window of sound plugins asked for.
    pub(super) fn do_what_the_window_of_sound_plugins_asked(&mut self, command: SoundPluginsCommand) {
        match command {
            SoundPluginsCommand::Show => self.interface.sound_plugins_window_open = true,
            SoundPluginsCommand::AddFolder => {
                if let Some(folder) = self.ask_for_a_folder("Choose a folder with sound plugins") {
                    self.add_plugin_folder(folder);
                }
            }
            SoundPluginsCommand::RemoveFolder(number) => {
                let folders = &mut self.environment.plugin_search.settings.folders;
                if number < folders.len() {
                    folders.remove(number);
                    self.the_places_to_look_for_plugins_changed();
                }
            }
            SoundPluginsCommand::AddDependencyFolder => {
                if let Some(folder) = self.ask_for_a_folder("Choose a folder with libraries that plugins need") {
                    self.add_dependency_folder(folder);
                }
            }
            SoundPluginsCommand::RemoveDependencyFolder(number) => {
                let folders = &mut self.environment.plugin_search.settings.dependency_folders;
                if number < folders.len() {
                    folders.remove(number);
                    self.the_places_to_look_for_plugins_changed();
                }
            }
            SoundPluginsCommand::SetSearchParentFolders(on) => {
                if self.environment.plugin_search.settings.search_parent_folders != on {
                    self.environment.plugin_search.settings.search_parent_folders = on;
                    self.the_places_to_look_for_plugins_changed();
                }
            }
            SoundPluginsCommand::ScanAgain => self.start_scanning_plugins(),
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

    /// Whether the window of sound plugins is open.
    pub fn sound_plugins_window_is_open(&self) -> bool {
        self.interface.sound_plugins_window_open
    }
}
