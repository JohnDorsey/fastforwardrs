// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The *Bundles* tab of the plugin manager: the plugins that the program can fetch from their authors' own release pages, who made each and under which licence, whether each is
//! fetched already, and a button that fetches them all after a question. It changes nothing itself: it hands back commands (`PluginManagerCommand`).

use std::path::Path;

use egui::{ProgressBar, RichText, Ui};

use super::commands::Command;
use super::plugin_manager_window::PluginManagerCommand;
use crate::plugin_bundles::Progress;

/// What the tab says.
pub const INTRO: &str = "Plugins that other people made. They are fetched from the release pages of their authors, and nothing is downloaded until you ask.";
pub const NO_BUILDS_NOTE: &str = "There are no builds of these plugins for this computer yet.";
pub const ASK_YES_LABEL: &str = "Download";
pub const ASK_NO_LABEL: &str = "Cancel";
pub const CANCEL_LABEL: &str = "Cancel";
pub const OK_LABEL: &str = "OK";
pub const FETCHED_NOTE: &str = "fetched";
pub const NOT_FETCHED_NOTE: &str = "not fetched yet";
pub const NO_BUILD_FOR_THIS_COMPUTER_NOTE: &str = "no build for this computer";

/// One set of plugins, as the tab shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleRow {
    pub name: String,
    pub version: String,
    pub plugins: Vec<String>,
    pub authors: String,
    pub licence: String,
    /// Where the corresponding source is.
    pub source: String,
    /// Whether there is a build of it for this computer (a set that has none is listed, and is not fetched).
    pub has_build: bool,
    pub is_fetched: bool,
}

/// What is going on with the download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleState<'a> {
    Idle,
    /// The question that comes before the download is showing.
    Asking,
    Running(&'a Progress),
    /// What the last download said, which stays until it is dismissed.
    Done(&'a str),
    Failed(&'a str),
}

/// What the tab shows.
pub struct BundlesView<'a> {
    pub rows: &'a [BundleRow],
    /// Whether there are builds for this computer.
    pub has_builds: bool,
    /// How many bytes the download is.
    pub total_bytes: u64,
    /// The addresses of the sites it comes from, for the question.
    pub hosts: &'a str,
    /// Where the plugins are put.
    pub folder: Option<&'a Path>,
    pub state: BundleState<'a>,
}

/// The size of a download, for a person: `143 MB`.
pub fn size_in_words(bytes: u64) -> String {
    let megabytes = (bytes as f64 / 1_000_000.0).max(0.1);
    if megabytes >= 10.0 {
        format!("{megabytes:.0} MB")
    } else {
        format!("{megabytes:.1} MB")
    }
}

/// What the button that begins the download says.
pub fn download_button_label(view: &BundlesView<'_>) -> String {
    format!("Download all ({})...", size_in_words(view.total_bytes))
}

/// What the question says.
pub fn question(view: &BundlesView<'_>) -> String {
    let count: usize = view
        .rows
        .iter()
        .filter(|row| row.has_build)
        .map(|row| row.plugins.len())
        .sum();
    let folder = view.folder.map_or_else(
        || "the folder of the program's data".to_owned(),
        |folder| folder.display().to_string(),
    );
    format!(
        "Download {count} plugin{} ({}) from {} and keep {} in {folder}? Each file is checked against a checksum that is kept in this program, and is not used if it is different. \
         The licence of each is listed here and is kept with it.",
        if count == 1 { "" } else { "s" },
        size_in_words(view.total_bytes),
        view.hosts,
        if count == 1 { "it" } else { "them" },
    )
}

pub fn show(ui: &mut Ui, view: &BundlesView<'_>, commands: &mut Vec<Command>) {
    ui.label(RichText::new(INTRO).weak().small());
    ui.add_space(6.0);
    // (What can be done comes before the list, so that the button, the question and the progress are in view without scrolling.)
    show_the_download(ui, view, commands);
    ui.add_space(6.0);
    for row in view.rows {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {}", row.name, row.version)).strong());
                let note = if !row.has_build {
                    NO_BUILD_FOR_THIS_COMPUTER_NOTE
                } else if row.is_fetched {
                    FETCHED_NOTE
                } else {
                    NOT_FETCHED_NOTE
                };
                ui.label(RichText::new(note).weak());
            });
            ui.label(format!("{} ({})", row.plugins.join(", "), row.licence));
            ui.label(
                RichText::new(format!("By {}. Source: {}", row.authors, row.source))
                    .weak()
                    .small(),
            );
        });
    }
}

fn show_the_download(ui: &mut Ui, view: &BundlesView<'_>, commands: &mut Vec<Command>) {
    if !view.has_builds {
        ui.colored_label(ui.visuals().warn_fg_color, NO_BUILDS_NOTE);
        return;
    }
    match &view.state {
        BundleState::Idle => {
            if ui.button(download_button_label(view)).clicked() {
                commands.push(Command::PluginManager(PluginManagerCommand::AskToDownloadBundles));
            }
        }
        BundleState::Asking => {
            ui.label(question(view));
            ui.horizontal(|ui| {
                if ui.button(ASK_YES_LABEL).clicked() {
                    commands.push(Command::PluginManager(PluginManagerCommand::DownloadBundles));
                }
                if ui.button(ASK_NO_LABEL).clicked() {
                    commands.push(Command::PluginManager(
                        PluginManagerCommand::CancelAskingToDownloadBundles,
                    ));
                }
            });
        }
        BundleState::Running(progress) => {
            let fraction = if progress.bytes_total == 0 {
                0.0
            } else {
                (progress.bytes_done as f32 / progress.bytes_total as f32).clamp(0.0, 1.0)
            };
            ui.add(ProgressBar::new(fraction).text(progress.step.clone()));
            if ui.button(CANCEL_LABEL).clicked() {
                commands.push(Command::PluginManager(PluginManagerCommand::CancelBundleDownload));
            }
        }
        BundleState::Done(message) | BundleState::Failed(message) => {
            if matches!(view.state, BundleState::Failed(_)) {
                ui.colored_label(ui.visuals().warn_fg_color, *message);
            } else {
                ui.label(*message);
            }
            if ui.button(OK_LABEL).clicked() {
                commands.push(Command::PluginManager(PluginManagerCommand::DismissBundleMessage));
            }
        }
    }
}
