// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of sound plugins, opened from the Edit menu (G17 of `docs/engine-phase-7-design.md`): where plugins are looked for (the folders a person adds; those of
//! `LADSPA_PATH`, `CLAP_PATH` and the usual ones are shown and cannot be removed), where the libraries that plugins need are looked for, and what the scan found and what failed. It is a
//! window that is opened, not a control that is always there. It changes nothing itself: it hands back commands.

use std::path::{Path, PathBuf};

use egui::{Align2, Context, Id, RichText, ScrollArea, Ui, Window, vec2};

use super::commands::Command;
use crate::plugin_scan::{LibraryOutcome, LibraryReport, NotUsed, PluginSearchSettings};

/// The title of the window, and what its buttons and the checkbox say.
pub const TITLE: &str = "Sound plugins";
pub const ADD_FOLDER_LABEL: &str = "Add folder...";
pub const REMOVE_LABEL: &str = "Remove";
pub const SCAN_AGAIN_LABEL: &str = "Scan again";
pub const SEARCH_PARENT_FOLDERS_LABEL: &str = "Also look in the folders above each plugin library";
pub const FOLDERS_HEADING: &str = "Folders to look for plugins in";
pub const DEPENDENCY_FOLDERS_HEADING: &str = "Folders with libraries that plugins need";
pub const SCANNING_NOTE: &str = "Scanning...";

/// How wide the window can grow, in points (a long path is cut short, and says all of itself when the pointer is on it).
const MOST_WIDTH: f32 = 720.0;

/// What the window can ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundPluginsCommand {
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
}

/// What the window shows.
pub struct SoundPluginsView<'a> {
    pub settings: &'a PluginSearchSettings,
    /// The folders that are looked in besides those a person added: those of `LADSPA_PATH` and `CLAP_PATH`, and the usual ones.
    pub other_folders: &'a [PathBuf],
    pub is_scanning: bool,
    pub has_scanned: bool,
    pub libraries: &'a [LibraryReport],
    pub not_used: &'a [NotUsed],
    /// How many plugins are effects that can be put on a clip.
    pub effect_count: usize,
    /// What went wrong that was not a library's doing.
    pub problems: &'a [String],
}

/// Shows the window if it is `open`, and adds the commands for what is done in it.
pub fn show(context: &Context, open: &mut bool, view: &SoundPluginsView<'_>, commands: &mut Vec<Command>) {
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

fn show_contents(ui: &mut Ui, view: &SoundPluginsView<'_>, commands: &mut Vec<Command>) {
    ui.label(RichText::new(FOLDERS_HEADING).strong());
    show_folders(
        ui,
        "plugin folders",
        &view.settings.folders,
        view.other_folders,
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
        commands,
        false,
    );
    let mut search_parents = view.settings.search_parent_folders;
    if ui.checkbox(&mut search_parents, SEARCH_PARENT_FOLDERS_LABEL).changed() {
        commands.push(Command::SoundPlugins(SoundPluginsCommand::SetSearchParentFolders(
            search_parents,
        )));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!view.is_scanning, egui::Button::new(SCAN_AGAIN_LABEL))
            .clicked()
        {
            commands.push(Command::SoundPlugins(SoundPluginsCommand::ScanAgain));
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
}

/// What the scan came to, in a line.
pub fn summary(view: &SoundPluginsView<'_>) -> String {
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
    let libraries = if view.libraries.len() == 1 {
        "1 library".to_owned()
    } else {
        format!("{} libraries", view.libraries.len())
    };
    if failed == 0 {
        format!("{libraries} found; {effects}.")
    } else {
        format!("{libraries} found, {failed} of them failed; {effects}.")
    }
}

fn show_folders(
    ui: &mut Ui,
    salt: &str,
    folders: &[PathBuf],
    other_folders: &[PathBuf],
    commands: &mut Vec<Command>,
    for_plugins: bool,
) {
    ui.push_id(salt, |ui| {
        for (number, folder) in folders.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(REMOVE_LABEL).clicked() {
                    commands.push(Command::SoundPlugins(if for_plugins {
                        SoundPluginsCommand::RemoveFolder(number)
                    } else {
                        SoundPluginsCommand::RemoveDependencyFolder(number)
                    }));
                }
                ui.add(egui::Label::new(folder.display().to_string()).truncate())
                    .on_hover_text(folder.display().to_string());
            });
        }
        for folder in other_folders {
            ui.add(egui::Label::new(RichText::new(format!("{} (usual)", folder.display())).weak()).truncate())
                .on_hover_text(folder.display().to_string());
        }
        if ui.button(ADD_FOLDER_LABEL).clicked() {
            commands.push(Command::SoundPlugins(if for_plugins {
                SoundPluginsCommand::AddFolder
            } else {
                SoundPluginsCommand::AddDependencyFolder
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

fn show_libraries(ui: &mut Ui, view: &SoundPluginsView<'_>) {
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
