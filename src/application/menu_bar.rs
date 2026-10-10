//! The menu bar, which has one menu: File.

use egui::{MenuBar, Ui};

use super::commands::Command;

/// What decides which menu items can be used.
pub struct MenuAvailability {
    /// Whether the project has anything that could be saved.
    pub can_save: bool,
    /// Whether a video can be imported (the project has none, or its video is missing).
    pub can_import: bool,
    /// Whether the project can be exported.
    pub can_export: bool,
    /// Whether something is going on (such as an export) that makes the other items wait.
    pub busy: bool,
}

/// Draws the menu bar and adds a command for the item chosen.
pub fn show(ui: &mut Ui, availability: &MenuAvailability, commands: &mut Vec<Command>) {
    MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            let idle = !availability.busy;
            if ui.add_enabled(idle, egui::Button::new("New project")).clicked() {
                commands.push(Command::NewProject);
            }
            if ui
                .add_enabled(idle && availability.can_save, egui::Button::new("Save project"))
                .clicked()
            {
                commands.push(Command::SaveProject);
            }
            if ui.add_enabled(idle, egui::Button::new("Load project")).clicked() {
                commands.push(Command::LoadProject);
            }
            let import = ui.add_enabled(idle && availability.can_import, egui::Button::new("Import"));
            if import.clicked() {
                commands.push(Command::ImportVideo);
            }
            if !availability.can_import {
                import.on_disabled_hover_text("This version works with one video per project.");
            }
            if ui
                .add_enabled(idle && availability.can_export, egui::Button::new("Export"))
                .clicked()
            {
                commands.push(Command::ExportVideo);
            }
        });
    });
}
