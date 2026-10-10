//! The two little settings windows, opened from the Edit menu.
//!
//! * **Global settings** has a checkbox for each setting.
//! * **Project settings** has the same checkboxes, and next to each a "Use global setting" checkbox.
//!   While that is ticked the project follows the global setting: the checkbox beside it is
//!   greyed out and shows the global value that is being used, and nothing about it is saved in
//!   the project file.
//!
//! Both windows are made from the table of settings ([`BOOLEAN_SETTINGS`]), so a new setting
//! appears in both as soon as it has a row there. They do not change anything themselves: they
//! hand back the new values to be applied.

use egui::{Align2, Checkbox, Context, Grid, Id, Vec2, Window, vec2};

use super::commands::Command;
use crate::project::{BOOLEAN_SETTINGS, ProjectSettings, Settings};

/// What the checkbox next to each control in the project settings window says.
const USE_GLOBAL_SETTING_LABEL: &str = "Use global setting";

/// Which of the settings windows are open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsWindowsOpen {
    pub project: bool,
    pub global: bool,
}

/// Shows whichever settings windows are open, and adds a command for each change made in them.
pub fn show(
    context: &Context,
    open: &mut SettingsWindowsOpen,
    project_settings: ProjectSettings,
    global_settings: Settings,
    commands: &mut Vec<Command>,
) {
    if open.global
        && let Some(changed) = show_global_settings(context, &mut open.global, global_settings)
    {
        commands.push(Command::SetGlobalSettings(changed));
    }
    if open.project
        && let Some(changed) = show_project_settings(context, &mut open.project, project_settings, global_settings)
    {
        commands.push(Command::SetProjectSettings(changed));
    }
}

/// A window that can be moved but not resized or folded away, starting near the middle of the
/// window, `offset` points from the exact middle (so that the two windows, when both are opened,
/// do not start on top of each other).
fn small_window<'open>(title: &'static str, open: &'open mut bool, context: &Context, offset: Vec2) -> Window<'open> {
    Window::new(title)
        .id(Id::new(title))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(context.content_rect().center() + offset)
}

/// Shows the global settings window; returns the settings if they were changed in it.
fn show_global_settings(context: &Context, open: &mut bool, settings: Settings) -> Option<Settings> {
    let mut edited = settings;
    small_window("Global settings", open, context, vec2(150.0, 70.0)).show(context, |ui| {
        for setting in BOOLEAN_SETTINGS {
            ui.checkbox((setting.global)(&mut edited), setting.label);
        }
    });
    (edited != settings).then_some(edited)
}

/// Shows the project settings window; returns the project's settings if they were changed in it.
fn show_project_settings(
    context: &Context,
    open: &mut bool,
    settings: ProjectSettings,
    global_settings: Settings,
) -> Option<ProjectSettings> {
    let mut edited = settings;
    let mut global_values = global_settings;
    small_window("Project settings", open, context, vec2(-150.0, -70.0)).show(context, |ui| {
        Grid::new("project settings")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                for setting in BOOLEAN_SETTINGS {
                    let global_value = *(setting.global)(&mut global_values);
                    let own_value = (setting.project)(&mut edited);
                    let mut follows_global = own_value.is_none();
                    // While following, the control shows the global value (greyed out): that is the
                    // value in use. Once it is let go of, it starts from that same value.
                    let mut shown_value = own_value.unwrap_or(global_value);
                    ui.add_enabled(!follows_global, Checkbox::new(&mut shown_value, setting.label));
                    ui.checkbox(&mut follows_global, USE_GLOBAL_SETTING_LABEL);
                    *own_value = (!follows_global).then_some(shown_value);
                    ui.end_row();
                }
            });
    });
    (edited != settings).then_some(edited)
}
