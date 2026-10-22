// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu bar, which has three menus: File, Edit and View.

use egui::{Button, MenuBar, Ui};

use super::commands::Command;
use super::keyboard::{DESELECT_KEY, REDO_SHORTCUT, UNDO_SHORTCUT};
use super::sound_plugins_window::SoundPluginsCommand;

/// The label of the checkbox of the View menu.
pub const GENERATE_LEAD_UP_ON_SEEK_LABEL: &str = "Generate lead-up on seek";

/// What is said when the pointer rests on that checkbox.
const GENERATE_LEAD_UP_ON_SEEK_HELP: &str = "After you jump to another place in the timeline, make the frames that lead up to it in the background, so that an effect that remembers earlier frames (feedback, trails) shows what the exported video shows. Until it is done the picture is approximate. When this is off, the preview goes on from the nearest saved checkpoint instead, and says that the picture is approximate.";

/// What decides which menu items can be used, and what the checkboxes show.
pub struct MenuState {
    /// Whether the project has anything that could be saved.
    pub can_save: bool,
    /// Whether the project can be exported.
    pub can_export: bool,
    /// Whether a clip is selected, so that there is something to deselect.
    pub has_selection: bool,
    /// Whether there is an edit to undo, and one that was undone to redo.
    pub can_undo: bool,
    pub can_redo: bool,
    /// Whether something is going on (such as an export) that makes the other items wait.
    pub busy: bool,
    /// Whether the checkbox of the View menu is ticked: the global setting of the same name.
    pub generate_lead_up_on_seek: bool,
}

/// Draws the menu bar and adds a command for the item chosen.
pub fn show(ui: &mut Ui, availability: &MenuState, commands: &mut Vec<Command>) {
    let idle = !availability.busy;
    MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.add_enabled(idle, Button::new("New project")).clicked() {
                commands.push(Command::NewProject);
            }
            if ui
                .add_enabled(idle && availability.can_save, Button::new("Save project"))
                .clicked()
            {
                commands.push(Command::SaveProject);
            }
            if ui
                .add_enabled(idle && availability.can_save, Button::new("Save project as"))
                .clicked()
            {
                commands.push(Command::SaveProjectAs);
            }
            if ui.add_enabled(idle, Button::new("Load project")).clicked() {
                commands.push(Command::LoadProject);
            }
            if ui.add_enabled(idle, Button::new("Embed and import")).clicked() {
                commands.push(Command::EmbedAndImportFile);
            }
            if ui.add_enabled(idle, Button::new("Link and import")).clicked() {
                commands.push(Command::LinkAndImportFile);
            }
            if ui
                .add_enabled(idle && availability.can_export, Button::new("Export"))
                .clicked()
            {
                commands.push(Command::ExportVideo);
            }
        });
        ui.menu_button("Edit", |ui| {
            let undo = Button::new("Undo").shortcut_text(ui.ctx().format_shortcut(&UNDO_SHORTCUT));
            if ui.add_enabled(idle && availability.can_undo, undo).clicked() {
                commands.push(Command::Undo);
            }
            let redo = Button::new("Redo").shortcut_text(ui.ctx().format_shortcut(&REDO_SHORTCUT));
            if ui.add_enabled(idle && availability.can_redo, redo).clicked() {
                commands.push(Command::Redo);
            }
            ui.separator();
            let deselect = Button::new("Deselect").shortcut_text(ui.ctx().format_shortcut(&DESELECT_SHORTCUT));
            if ui.add_enabled(idle && availability.has_selection, deselect).clicked() {
                commands.push(Command::Deselect);
            }
            ui.separator();
            if ui.add_enabled(idle, Button::new("Project settings")).clicked() {
                commands.push(Command::ShowProjectSettings);
            }
            if ui.add_enabled(idle, Button::new("Global settings")).clicked() {
                commands.push(Command::ShowGlobalSettings);
            }
            if ui.add_enabled(idle, Button::new("Sound plugins...")).clicked() {
                commands.push(Command::SoundPlugins(SoundPluginsCommand::Show));
            }
        });
        ui.menu_button("View", |ui| {
            let mut generate_lead_up = availability.generate_lead_up_on_seek;
            let item = ui
                .checkbox(&mut generate_lead_up, GENERATE_LEAD_UP_ON_SEEK_LABEL)
                .on_hover_text(GENERATE_LEAD_UP_ON_SEEK_HELP);
            if item.changed() {
                commands.push(Command::SetGenerateLeadUpOnSeek(generate_lead_up));
            }
        });
    });
}

/// The key that deselects, as a shortcut for the menu to show.
const DESELECT_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::NONE, DESELECT_KEY);
