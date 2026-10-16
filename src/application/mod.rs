// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user interface: an egui application that ties the project, the media tools and the live
//! preview together.
//!
//! The parts that decide things (where tiles go, what a slider position means, how an edit moves
//! the current frame) are plain functions without any drawing in them, so they can be tested.
//! The parts that draw are kept thin.

mod archive_cache;
mod background;
mod clip_moving;
mod colour_of_a_file_window;
pub mod command_line;
mod commands;
mod dialogs;
mod duration_control;
mod editing;
mod fast_forward_application;
mod file_chooser;
mod gesture_frame;
mod group_messages;
mod history;
mod keyboard;
mod keyframe_moving;
mod menu_bar;
mod mouse_buttons;
mod naming;
mod opening;
mod plus_button;
mod preview_panel;
mod properties_panel;
mod scrub_bar;
mod scrub_slider;
mod selection;
mod settings_windows;
mod start_time_hold;
mod textures;
mod theme;
mod timeline_geometry;
mod timeline_panel;
mod transport_bar;

pub use fast_forward_application::FastForwardApplication;

/// The size the window starts at, in points.
const STARTING_WINDOW_SIZE: [f32; 2] = [1280.0, 820.0];

/// The smallest the window can be made, in points.
const SMALLEST_WINDOW_SIZE: [f32; 2] = [760.0, 560.0];

/// Opens the window and runs the editor until the window is closed. If `project_to_open` is given,
/// that project file is loaded at the start.
pub fn run(project_to_open: Option<std::path::PathBuf>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("fastforwardrs")
            .with_inner_size(STARTING_WINDOW_SIZE)
            .with_min_inner_size(SMALLEST_WINDOW_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        "fastforwardrs",
        options,
        Box::new(move |creation_context| Ok(Box::new(FastForwardApplication::new(creation_context, project_to_open)))),
    )
}
