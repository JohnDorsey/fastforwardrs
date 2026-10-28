// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loads and runs CLAP sound plugins (phase 7, piece 7.8; the design is `docs/engine-phase-7-clap-design.md`).
//!
//! **The only crate that names `clack-host`** (C1): the rest of the program is given what `ladspa-host` gives it for LADSPA plugins: a [`Library`] that can be loaded and
//! described, and an [`Instance`] that can be run. The code here has no `unsafe` of its own except the one call that loads a file, which runs the code of the library.
//!
//! **The threads of the contract are kept** (C8): CLAP asks for some calls on *the main thread* (making a plugin, `init`, `activate`, `deactivate`, `destroy`, `on_main_thread`)
//! and others on *an audio thread* (`start_processing`, `process`, `stop_processing`, `reset`). A [`MainThread`] is a thread of its own that does the first kind, for as many
//! instances as it is given; the thread that calls [`Instance::run`] is the audio thread. The host says so to the plugin through `thread-check`.

#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(unsafe_code)]

pub mod editor;
mod host;
mod instance;
mod library;
mod main_thread;
pub mod scan;
pub mod start_check;

pub use editor::{Editor, EditorError, EditorEvent, ParameterEdit, WindowSize};
pub use instance::{Instance, InstanceError, NoteEvent, NoteEventKind, ParameterChange, ProcessOutcome, RunError};
pub use library::{Library, LibraryDescription, OpenError, PluginDescription};
pub use main_thread::MainThread;

#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod note_tests;
#[cfg(test)]
mod tests;
