//! The menu bar, which has two menus: File and Edit.

use egui::{Button, MenuBar, Ui};

use super::commands::Command;
use super::keyboard::{DESELECT_KEY, REDO_SHORTCUT, UNDO_SHORTCUT};

/// What decides which menu items can be used.
pub struct MenuAvailability {
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
}

/// Draws the menu bar and adds a command for the item chosen.
pub fn show(ui: &mut Ui, availability: &MenuAvailability, commands: &mut Vec<Command>) {
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
            if ui.add_enabled(idle, Button::new("Import")).clicked() {
                commands.push(Command::ImportFile);
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
        });
    });
}

/// The key that deselects, as a shortcut for the menu to show.
const DESELECT_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::NONE, DESELECT_KEY);
