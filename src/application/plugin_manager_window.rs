// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin manager, opened from the Edit menu (G17 of `docs/engine-phase-7-design.md`, and phase 6.8): where plugins and shaders are looked for (the folders a person adds; those of
//! `LADSPA_PATH`, `CLAP_PATH` and the usual ones, and those of ISF, are shown and cannot be removed), where the libraries that plugins need are looked for, and what the scan found and
//! what failed: the sound plugins (LADSPA and CLAP) by library, and the shaders (ISF), each with whether it can be used and why not. It is a window that is opened, not a control that is
//! always there. It changes nothing itself: it hands back commands.

use std::path::{Path, PathBuf};

use egui::{Align2, Context, Id, RichText, ScrollArea, Ui, Window, vec2};

use super::commands::Command;
use super::plugin_manager_bundles::{BundleKind, BundlesView, show as show_bundles};
use crate::isf::Shape;
use crate::plugin_scan::{LibraryOutcome, LibraryReport, NotUsed, PluginSearchSettings};
use crate::shader_files::{FoundShader, ShaderScan};

/// The title of the window, and what its buttons and the checkbox say.
pub const TITLE: &str = "Plugin manager";
pub const ADD_FOLDER_LABEL: &str = "Add folder...";
pub const REMOVE_LABEL: &str = "Remove";
pub const SCAN_AGAIN_LABEL: &str = "Scan again";
pub const SEARCH_PARENT_FOLDERS_LABEL: &str = "Also look in the folders above each plugin library";
pub const FOLDERS_HEADING: &str = "Folders to look for plugins and shaders in";
pub const DEPENDENCY_FOLDERS_HEADING: &str = "Folders with libraries that plugins need";
pub const SCANNING_NOTE: &str = "Scanning...";

/// How wide the window can grow, in points (a long path is cut short, and says all of itself when the pointer is on it).
const MOST_WIDTH: f32 = 720.0;

/// What a folder of plugins that came with the program is called in the list of folders (the others that are looked in are "usual").
pub const BUNDLED_LABEL: &str = "bundled with the program";

/// What the folder that the *Bundles* tab fetches into is called in that list.
pub const FETCHED_LABEL: &str = "fetched with Bundles";

/// What the window can ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginManagerCommand {
    /// Open the window.
    Show,
    /// Ask which folder to add to those plugins are looked for in.
    AddFolder,
    RemoveFolder(usize),
    /// Ask which folder to add to those the libraries that plugins need are looked for in.
    AddDependencyFolder,
    RemoveDependencyFolder(usize),
    SetSearchParentFolders(bool),
    /// Scan the folders again (the libraries that did not change are not scanned again, and those that failed for want of a library are, if the places to look for them changed).
    ScanAgain,
    /// Show the other tab.
    SelectTab(PluginManagerTab),
    /// The button of a section of the *Bundles* tab was pressed: the question that comes before the download is shown.
    AskToDownloadBundles(BundleKind),
    CancelAskingToDownloadBundles(BundleKind),
    /// The question was answered yes: the download begins.
    DownloadBundles(BundleKind),
    /// A download that is going on is stopped.
    CancelBundleDownload(BundleKind),
    /// What the last download said is taken away.
    DismissBundleMessage(BundleKind),
}

/// The two tabs of the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluginManagerTab {
    /// Where plugins are looked for, and what the scan found.
    #[default]
    Plugins,
    /// The plugins that can be fetched from their authors.
    Bundles,
}

impl PluginManagerTab {
    /// What the tab is called.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Plugins => "Plugins",
            Self::Bundles => "Bundles",
        }
    }
}

/// What the window shows.
pub struct PluginManagerView<'a> {
    pub settings: &'a PluginSearchSettings,
    /// The folders that are looked in besides those a person added: those of `LADSPA_PATH` and `CLAP_PATH`, and the usual ones.
    pub other_folders: &'a [PathBuf],
    /// Which of those is the folder of the plugins that came with the program, if one is (it is said so, and not called usual).
    pub bundled_folder: Option<&'a Path>,
    /// The folder that the *Bundles* tab fetches into (it is one of those, where it is there).
    pub fetched_folder: Option<&'a Path>,
    /// The folder that the *Bundles* tab fetches shaders into (it is one of those too).
    pub fetched_shaders_folder: Option<&'a Path>,
    pub tab: PluginManagerTab,
    /// The section of the *Bundles* tab for the sound plugins, and the one for the shaders (which is not shown if the program has no shaders to fetch).
    pub bundles: BundlesView<'a>,
    pub shader_bundles: BundlesView<'a>,
    pub is_scanning: bool,
    pub has_scanned: bool,
    pub libraries: &'a [LibraryReport],
    pub not_used: &'a [NotUsed],
    /// How many plugins are effects that can be put on a clip.
    pub effect_count: usize,
    /// What went wrong that was not a library's doing.
    pub problems: &'a [String],
    /// The shaders that were found, with what became of each.
    pub shaders: &'a ShaderScan,
}

/// Shows the window if it is `open`, and adds the commands for what is done in it.
pub fn show(context: &Context, open: &mut bool, view: &PluginManagerView<'_>, commands: &mut Vec<Command>) {
    if !*open {
        return;
    }
    Window::new(TITLE)
        .id(Id::new(TITLE))
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(520.0, 420.0))
        .max_width(MOST_WIDTH)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(context.content_rect().center())
        .show(context, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| show_contents(ui, view, commands));
        });
}

fn show_contents(ui: &mut Ui, view: &PluginManagerView<'_>, commands: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        for tab in [PluginManagerTab::Plugins, PluginManagerTab::Bundles] {
            if ui.selectable_label(view.tab == tab, tab.label()).clicked() && view.tab != tab {
                commands.push(Command::PluginManager(PluginManagerCommand::SelectTab(tab)));
            }
        }
    });
    ui.separator();
    match view.tab {
        PluginManagerTab::Plugins => show_the_plugins_tab(ui, view, commands),
        PluginManagerTab::Bundles => {
            show_bundles(ui, &view.bundles, commands);
            if !view.shader_bundles.rows.is_empty() {
                ui.add_space(10.0);
                ui.separator();
                show_bundles(ui, &view.shader_bundles, commands);
            }
        }
    }
}

fn show_the_plugins_tab(ui: &mut Ui, view: &PluginManagerView<'_>, commands: &mut Vec<Command>) {
    ui.label(RichText::new(FOLDERS_HEADING).strong());
    show_folders(
        ui,
        "plugin folders",
        &view.settings.folders,
        view.other_folders,
        [view.bundled_folder, view.fetched_folder, view.fetched_shaders_folder],
        commands,
        true,
    );
    ui.add_space(8.0);
    ui.label(RichText::new(DEPENDENCY_FOLDERS_HEADING).strong());
    ui.label(
        RichText::new("A plugin that needs a library that is not beside it fails to load; name the folders that have such libraries here.")
            .weak()
            .small(),
    );
    show_folders(
        ui,
        "dependency folders",
        &view.settings.dependency_folders,
        &[],
        [None, None, None],
        commands,
        false,
    );
    let mut search_parents = view.settings.search_parent_folders;
    if ui.checkbox(&mut search_parents, SEARCH_PARENT_FOLDERS_LABEL).changed() {
        commands.push(Command::PluginManager(PluginManagerCommand::SetSearchParentFolders(
            search_parents,
        )));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!view.is_scanning, egui::Button::new(SCAN_AGAIN_LABEL))
            .clicked()
        {
            commands.push(Command::PluginManager(PluginManagerCommand::ScanAgain));
        }
        if view.is_scanning {
            ui.label(SCANNING_NOTE);
        } else if view.has_scanned {
            ui.label(summary(view));
        }
    });
    for problem in view.problems {
        ui.colored_label(ui.visuals().warn_fg_color, problem);
    }
    ui.add_space(4.0);
    ui.separator();
    show_libraries(ui, view);
    show_shaders(ui, view);
}

/// What the scan came to, in a line.
pub fn summary(view: &PluginManagerView<'_>) -> String {
    let failed = view
        .libraries
        .iter()
        .filter(|library| matches!(library.outcome, LibraryOutcome::Failed(_)))
        .count();
    let effects = if view.effect_count == 1 {
        "1 plugin can be put on a clip".to_owned()
    } else {
        format!("{} plugins can be put on a clip", view.effect_count)
    };
    let shaders = summary_of_the_shaders(view.shaders);
    let libraries = if view.libraries.len() == 1 {
        "1 library".to_owned()
    } else {
        format!("{} libraries", view.libraries.len())
    };
    if failed == 0 {
        format!("{libraries} found; {effects}{shaders}.")
    } else {
        format!("{libraries} found, {failed} of them failed; {effects}{shaders}.")
    }
}

/// What the scan came to of shaders, to follow the rest of the line: nothing if no shader was found, and otherwise `; 212 shaders can be used, 9 cannot`.
fn summary_of_the_shaders(shaders: &ShaderScan) -> String {
    if shaders.shaders.is_empty() {
        return String::new();
    }
    let usable = shaders.usable_count();
    let unusable = shaders.unusable_count();
    let can_be_used = format!("{usable} shader{} can be used", if usable == 1 { "" } else { "s" });
    if unusable == 0 {
        format!("; {can_be_used}")
    } else {
        format!("; {can_be_used}, {unusable} cannot")
    }
}

fn show_folders(
    ui: &mut Ui,
    salt: &str,
    folders: &[PathBuf],
    other_folders: &[PathBuf],
    [bundled, fetched, fetched_shaders]: [Option<&Path>; 3],
    commands: &mut Vec<Command>,
    for_plugins: bool,
) {
    ui.push_id(salt, |ui| {
        for (number, folder) in folders.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(REMOVE_LABEL).clicked() {
                    commands.push(Command::PluginManager(if for_plugins {
                        PluginManagerCommand::RemoveFolder(number)
                    } else {
                        PluginManagerCommand::RemoveDependencyFolder(number)
                    }));
                }
                ui.add(egui::Label::new(folder.display().to_string()).truncate())
                    .on_hover_text(folder.display().to_string());
            });
        }
        for folder in other_folders {
            let kind = if bundled == Some(folder.as_path()) {
                BUNDLED_LABEL
            } else if fetched == Some(folder.as_path()) || fetched_shaders == Some(folder.as_path()) {
                FETCHED_LABEL
            } else {
                "usual"
            };
            ui.add(egui::Label::new(RichText::new(format!("{} ({kind})", folder.display())).weak()).truncate())
                .on_hover_text(folder.display().to_string());
        }
        if ui.button(ADD_FOLDER_LABEL).clicked() {
            commands.push(Command::PluginManager(if for_plugins {
                PluginManagerCommand::AddFolder
            } else {
                PluginManagerCommand::AddDependencyFolder
            }));
        }
    });
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// What a shader is for, as the lists say it.
pub fn word_for_a_shape(shape: Shape) -> &'static str {
    match shape {
        Shape::Filter => "filter",
        Shape::Generator => "generator",
        Shape::Transition => "transition",
    }
}

/// The heading of the list of the shaders that can be used.
pub fn heading_of_the_shaders_that_can_be_used(count: usize) -> String {
    format!("Shaders that can be used ({count})")
}

/// The heading of the list of the shaders that cannot be used.
pub fn heading_of_the_shaders_that_cannot_be_used(count: usize) -> String {
    format!("Shaders that cannot be used ({count})")
}

/// What a line of the list of the shaders that can be used says: its name, what it is for, and its file.
pub fn line_of_a_shader_that_can_be_used(shader: &FoundShader) -> String {
    let name = shader.kind.map_or_else(
        || shader.file_name.clone(),
        crate::application::naming::what_an_effect_is_called,
    );
    match shader.shape {
        Some(shape) => format!("{name} ({}): {}", word_for_a_shape(shape), shader.file_name),
        None => format!("{name}: {}", shader.file_name),
    }
}

/// What a line of the list of the shaders that cannot be used says: its file, and why.
pub fn line_of_a_shader_that_cannot_be_used(shader: &FoundShader) -> String {
    format!(
        "{}: {}",
        shader.file_name,
        shader.why_not_usable.as_deref().unwrap_or("it cannot be used")
    )
}

/// The shaders that were found: the ones that can be used (each once, by what it is), then the ones that cannot, with why. Nothing is shown before a shader is found.
fn show_shaders(ui: &mut Ui, view: &PluginManagerView<'_>) {
    if view.shaders.shaders.is_empty() {
        return;
    }
    let mut usable: Vec<&FoundShader> = Vec::new();
    for shader in &view.shaders.shaders {
        if shader.can_be_used() && !usable.iter().any(|known| known.kind == shader.kind) {
            usable.push(shader);
        }
    }
    let unusable: Vec<&FoundShader> = view
        .shaders
        .shaders
        .iter()
        .filter(|shader| !shader.can_be_used())
        .collect();
    ui.add_space(6.0);
    if !usable.is_empty() {
        egui::CollapsingHeader::new(heading_of_the_shaders_that_can_be_used(usable.len()))
            .id_salt("shaders that can be used")
            .show(ui, |ui| {
                for shader in usable {
                    ui.label(line_of_a_shader_that_can_be_used(shader))
                        .on_hover_text(shader.path.display().to_string());
                }
            });
    }
    if !unusable.is_empty() {
        egui::CollapsingHeader::new(heading_of_the_shaders_that_cannot_be_used(unusable.len()))
            .id_salt("shaders that cannot be used")
            .default_open(true)
            .show(ui, |ui| {
                for shader in unusable {
                    ui.colored_label(ui.visuals().warn_fg_color, line_of_a_shader_that_cannot_be_used(shader))
                        .on_hover_text(shader.path.display().to_string());
                }
            });
    }
}

fn show_libraries(ui: &mut Ui, view: &PluginManagerView<'_>) {
    for library in view.libraries {
        let name = file_name(&library.path);
        match &library.outcome {
            LibraryOutcome::Scanned(scan) => {
                let count = scan.plugins.len();
                ui.label(format!("{name}: {count} plugin{}", if count == 1 { "" } else { "s" }))
                    .on_hover_text(library.path.display().to_string());
            }
            LibraryOutcome::ScannedClap(scan) => {
                let count = scan.plugins.len();
                ui.label(format!("{name}: {count} plugin{}", if count == 1 { "" } else { "s" }))
                    .on_hover_text(library.path.display().to_string());
                // (A plugin whose start depends on memory it did not write is told of here: a person is told, and not blamed.)
                for plugin in &scan.plugins {
                    if let Some(warning) = plugin.start.warning() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("{} is unreliable at its start: {warning}.", plugin.description.name),
                        );
                    }
                }
            }
            LibraryOutcome::ScannedLv2(scan) => {
                let count = scan.plugins.len();
                ui.label(format!(
                    "{name}: {count} plugin{} (LV2)",
                    if count == 1 { "" } else { "s" }
                ))
                .on_hover_text(library.path.display().to_string());
            }
            LibraryOutcome::Failed(failure) => {
                ui.colored_label(ui.visuals().warn_fg_color, format!("{name}: {failure}"))
                    .on_hover_text(library.path.display().to_string());
            }
        }
    }
    if !view.not_used.is_empty() {
        ui.add_space(6.0);
        egui::CollapsingHeader::new(format!(
            "Plugins that cannot be put on a clip ({})",
            view.not_used.len()
        ))
        .id_salt("plugins not used")
        .show(ui, |ui| {
            for not_used in view.not_used {
                ui.label(format!(
                    "{} ({}): {}",
                    not_used.name,
                    file_name(&not_used.library),
                    not_used.reason
                ));
            }
        });
    }
}
