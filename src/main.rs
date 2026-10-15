// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

// Release builds are windowed programs: no console window opens behind the editor on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    // A project file given on the command line (as when one is double-clicked) is opened at the start.
    let project_to_open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    fastforwardrs::application::run(project_to_open)
}
