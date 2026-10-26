// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin editor windows (phase 10; the design is `docs/engine-phase-10-editors-design.md`).
//!
//! * [`protocol`] is what the editor and the editor-window program say to each other (P4).
//! * [`program`] is the inside of that program, `fastforwardrs-plugin-editor` (P1): the plugin's window lives in a process of its own, so that a plugin whose window crashes cannot
//!   take the editor and the project with it.

pub mod program;
pub mod protocol;
