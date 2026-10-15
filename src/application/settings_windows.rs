// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two little settings windows, opened from the Edit menu.
//!
//! * **Global settings** has a control for each setting: a checkbox, or a drop-down list.
//! * **Project settings** has the same controls, and next to each a "Use global setting" checkbox.
//!   While that is ticked the project follows the global setting: the control beside it is greyed
//!   out and shows the global value that is being used, and nothing about it is saved in the
//!   project file.
//!
//! Both windows are made from the table of settings ([`SETTINGS`]), so a new setting appears in
//! both as soon as it has a row there. They do not change anything themselves: they hand back the
//! new values to be applied.

use egui::{Align2, ComboBox, Context, Grid, Id, Ui, Vec2, Window, vec2};

use super::commands::Command;
use crate::project::{AnySetting, FrameRate, ProjectSettings, SETTINGS, SettingControlKind, Settings};

/// What the checkbox next to each control in the project settings window says.
const USE_GLOBAL_SETTING_LABEL: &str = "Use global setting";

/// What the drop-down list of frame rates in the project settings window is called.
pub const FRAME_RATE_LABEL: &str = "Frame rate";

/// The frame rates the drop-down list offers, as numerators and denominators (a project can have
/// another one, because it follows its first video, and then that one is listed too).
const USUAL_FRAME_RATES: [(u32, u32); 11] = [
    (12, 1),
    (15, 1),
    (24000, 1001),
    (24, 1),
    (25, 1),
    (30000, 1001),
    (30, 1),
    (48, 1),
    (50, 1),
    (60000, 1001),
    (60, 1),
];

/// How tall the list of frame rates can be before it scrolls, in points: tall enough for all of them.
const MOST_HEIGHT_OF_THE_LIST_OF_FRAME_RATES: f32 = 360.0;

/// A frame rate as the drop-down list says it: "30 fps", or "29.97 fps" for the one that is not a whole
/// number of frames a second.
pub fn frame_rate_label(frame_rate: FrameRate) -> String {
    if frame_rate.denominator() == 1 {
        return format!("{} fps", frame_rate.numerator());
    }
    let with_decimals = format!("{:.3}", frame_rate.frames_per_second());
    format!("{} fps", with_decimals.trim_end_matches('0').trim_end_matches('.'))
}

/// Which of the settings windows are open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsWindowsOpen {
    pub project: bool,
    pub global: bool,
}

/// Shows whichever settings windows are open, and adds a command for each change made in them.
/// `frame_rate` is the frame rate of the project.
pub fn show(
    context: &Context,
    open: &mut SettingsWindowsOpen,
    project: (ProjectSettings, FrameRate),
    global_settings: Settings,
    commands: &mut Vec<Command>,
) {
    if open.global
        && let Some(changed) = show_global_settings(context, &mut open.global, global_settings)
    {
        commands.push(Command::SetGlobalSettings(changed));
    }
    if open.project {
        show_project_settings(context, &mut open.project, project, global_settings, commands);
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

/// Shows the control for `setting` with the value `option` (a number in the setting's list of
/// values), greyed out if it is not `enabled`. Returns the value after whatever the user did.
fn show_control(ui: &mut Ui, setting: &dyn AnySetting, option: usize, enabled: bool) -> usize {
    let mut chosen = option;
    ui.add_enabled_ui(enabled, |ui| match setting.control() {
        SettingControlKind::Checkbox => {
            let mut ticked = option != 0;
            ui.checkbox(&mut ticked, setting.label());
            chosen = usize::from(ticked);
        }
        SettingControlKind::Dropdown => {
            let option_labels = setting.option_labels();
            ui.horizontal(|ui| {
                ui.label(setting.label());
                ComboBox::from_id_salt(setting.label())
                    .selected_text(option_labels.get(option).copied().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (number, label) in option_labels.iter().enumerate() {
                            ui.selectable_value(&mut chosen, number, *label);
                        }
                    });
            });
        }
    });
    chosen
}

/// Shows the global settings window; returns the settings if they were changed in it.
fn show_global_settings(context: &Context, open: &mut bool, settings: Settings) -> Option<Settings> {
    let mut edited = settings;
    small_window("Global settings", open, context, vec2(150.0, 70.0)).show(context, |ui| {
        for setting in SETTINGS {
            let option = setting.global_option(edited);
            let chosen = show_control(ui, setting, option, true);
            setting.set_global_option(&mut edited, chosen);
        }
    });
    (edited != settings).then_some(edited)
}

/// The drop-down list of frame rates, with `frame_rate` chosen. Returns the one chosen, which is a
/// different one if the user picked another.
fn show_frame_rate_control(ui: &mut Ui, frame_rate: FrameRate) -> FrameRate {
    let mut chosen = frame_rate;
    let mut offered: Vec<FrameRate> = USUAL_FRAME_RATES
        .iter()
        .filter_map(|(numerator, denominator)| FrameRate::new(*numerator, *denominator).ok())
        .collect();
    if !offered.contains(&frame_rate) {
        // One that is not usual (a video of that rate was the first) is there too, in its place in the order.
        offered.push(frame_rate);
        offered.sort_by(|first, second| first.frames_per_second().total_cmp(&second.frames_per_second()));
    }
    ui.horizontal(|ui| {
        ui.label(FRAME_RATE_LABEL);
        ComboBox::from_id_salt("project frame rate")
            .selected_text(frame_rate_label(frame_rate))
            .height(MOST_HEIGHT_OF_THE_LIST_OF_FRAME_RATES)
            .show_ui(ui, |ui| {
                for rate in offered {
                    ui.selectable_value(&mut chosen, rate, frame_rate_label(rate));
                }
            });
    });
    chosen
}

/// Shows the project settings window, and adds a command for each change made in it.
fn show_project_settings(
    context: &Context,
    open: &mut bool,
    (settings, frame_rate): (ProjectSettings, FrameRate),
    global_settings: Settings,
    commands: &mut Vec<Command>,
) {
    let mut edited = settings;
    small_window("Project settings", open, context, vec2(-150.0, -70.0)).show(context, |ui| {
        let chosen_frame_rate = show_frame_rate_control(ui, frame_rate);
        if chosen_frame_rate != frame_rate {
            commands.push(Command::SetProjectFrameRate(chosen_frame_rate));
        }
        ui.add_space(6.0);
        Grid::new("project settings")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                for setting in SETTINGS {
                    let own_option = setting.project_option(edited);
                    let mut follows_global = own_option.is_none();
                    // While following, the control shows the global value (greyed out): that is the
                    // value in use. Once it is let go of, it starts from that same value.
                    let shown_option = own_option.unwrap_or_else(|| setting.global_option(global_settings));
                    let chosen = show_control(ui, setting, shown_option, !follows_global);
                    ui.checkbox(&mut follows_global, USE_GLOBAL_SETTING_LABEL);
                    setting.set_project_option(&mut edited, (!follows_global).then_some(chosen));
                    ui.end_row();
                }
            });
    });
    if edited != settings {
        commands.push(Command::SetProjectSettings(edited));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_rate_is_said_as_the_number_of_frames_a_second_in_as_few_figures_as_it_takes() {
        let rate = |numerator, denominator| FrameRate::new(numerator, denominator).unwrap();
        assert_eq!(frame_rate_label(rate(30, 1)), "30 fps");
        assert_eq!(frame_rate_label(rate(24, 1)), "24 fps");
        assert_eq!(frame_rate_label(rate(30000, 1001)), "29.97 fps");
        assert_eq!(frame_rate_label(rate(24000, 1001)), "23.976 fps");
        assert_eq!(frame_rate_label(rate(60000, 1001)), "59.94 fps");
        assert_eq!(frame_rate_label(rate(25, 3)), "8.333 fps");
        assert_eq!(frame_rate_label(rate(1, 1)), "1 fps");
    }

    #[test]
    fn the_usual_frame_rates_are_all_valid_and_in_order_and_each_has_a_label_of_its_own() {
        let rates: Vec<FrameRate> = USUAL_FRAME_RATES
            .iter()
            .map(|(numerator, denominator)| FrameRate::new(*numerator, *denominator).unwrap())
            .collect();
        assert!(
            rates
                .windows(2)
                .all(|pair| pair[0].frames_per_second() < pair[1].frames_per_second())
        );
        let mut labels: Vec<String> = rates.iter().map(|rate| frame_rate_label(*rate)).collect();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), rates.len());
    }
}
