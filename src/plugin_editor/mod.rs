// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin editor windows (phase 10; the design is `docs/engine-phase-10-editors-design.md`).
//!
//! * [`protocol`] is what the editor and the editor-window program say to each other (P4).
//! * [`sessions`] is the editor's side: the windows that are open, each a process of that program.
//! * [`values`] says what a plugin's number is as a value of an effect and back (one place, for the sound and for the window).
//! * [`backends`] is what the program does with a plugin's window, once, and how for each kind of plugin (CLAP, LV2).
//! * [`program`] is the inside of that program, `fastforwardrs-plugin-editor` (P1): the plugin's window lives in a process of its own, so that a plugin whose window crashes cannot
//!   take the editor and the project with it.

pub mod backends;
pub mod program;
pub mod protocol;
pub mod sessions;
pub mod values;

/// The kind of window of this system that a plugin is asked for (the words that CLAP uses, which LV2's classes of interface are matched with: `win32`, `cocoa`, `x11`).
pub fn the_kind_of_window() -> &'static str {
    if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "macos") {
        "cocoa"
    } else {
        "x11"
    }
}
