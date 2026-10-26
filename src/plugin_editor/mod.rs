// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin editor windows (phase 10; the design is `docs/engine-phase-10-editors-design.md`).
//!
//! * [`protocol`] is what the editor and the editor-window program say to each other (P4).
//! * [`sessions`] is the editor's side: the windows that are open, each a process of that program.
//! * [`values`] says what a plugin's number is as a value of an effect and back (one place, for the sound and for the window).
//! * [`program`] is the inside of that program, `fastforwardrs-plugin-editor` (P1): the plugin's window lives in a process of its own, so that a plugin whose window crashes cannot
//!   take the editor and the project with it.

pub mod program;
pub mod protocol;
pub mod sessions;
pub mod values;
