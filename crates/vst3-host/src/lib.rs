// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finds, describes and runs VST3 sound plugins, with a host of our own. **This is a crate of fastforwardrs that has `unsafe` code in it** (the editor keeps
//! `#![forbid(unsafe_code)]`): a plugin is a library of native code that is loaded and called, and Rust cannot check any of it. The interface of this crate is safe, and each
//! `unsafe` block says why it is sound (`clippy::undocumented_unsafe_blocks` is on, and a test of this crate counts them: see `tests.rs`). The design is
//! `docs/engine-phase-15-vst3-design.md`; the bindings are the `vst3` crate's, made from the headers of Steinberg's SDK.
//!
//! * [`Module`] is a module (a `.vst3` folder or file) that is loaded; it lists its classes and makes them.
//! * [`Instance`] is a plugin that has been made from it, with its component and controller connected: it can be given values, sound and notes, and its state saved and restored.
//! * A plugin's window (`IPlugView`) is put in a window of the caller with [`Instance::open_view`], told its size, and watched with [`Instance::view_events`].
//! * [`ClassId`] is the id of a class in a form that is the same on every system.
//! * [`scan_module`] loads a module, and makes and runs each of its plugins once on silence, saying what it is doing as it goes: **it is meant to be run in a program of its
//!   own** (`fastforwardrs-plugin-scan`), so that a library that crashes or never returns costs that program and not the editor.
//!
//! Nothing in this crate decides whether a library may be loaded. That is the editor's `plugin_trust` module. And nothing in it runs a plugin in the editor's process: the
//! editor makes copies only in a program of its own (`fastforwardrs-plugin-process`).

#![deny(unsafe_code)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod classid;
mod description;
mod instance;
mod module;
mod notes;
mod objects;
mod scan;
mod state;
mod view;

pub use classid::ClassId;
pub use description::{BusDescription, ClassDescription, ClassFacts, FactoryDescription, ParameterDescription};
pub use instance::{BetweenBlocks, Instance, InstantiateError, RunError};
pub use module::{AUDIO_MODULE_CLASS, Module, OpenError, library_file_of};
pub use notes::{NoteEvent, NoteEventKind};
pub use scan::{ModuleScan, PROBE_BLOCK_FRAMES, PROBE_SAMPLE_RATE, ScannedClass, scan_module};
pub use state::{StateError, decode_state, encode_state};
pub use view::{ViewError, ViewEvent, ViewSize};

#[cfg(test)]
mod message_tests;
#[cfg(test)]
mod run_loop_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_tests;
