// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loads LADSPA sound plugins. **This is the only crate of fastforwardrs that has `unsafe` code in it** (the editor keeps `#![forbid(unsafe_code)]`): a plugin is a library of
//! native code that is loaded and called, and Rust cannot check any of it. The interface of this crate is safe, and each `unsafe` block says why it is sound
//! (`clippy::undocumented_unsafe_blocks` is on, and a test of this crate counts them: see `tests.rs`). The design is G1 to G4 of `docs/engine-phase-7-design.md`.
//!
//! * [`description`] is what a plugin says about itself (its ports and the hints about them), as safe values that can be sent between programs.
//! * [`Library`] is a library that is loaded; [`Instance`] is a plugin that has been made from it, and can be given values and sound.
//! * [`scan_library`] loads a library, describes it, and makes and runs each of its plugins once on silence, saying what it is doing as it goes: **it is meant to be run in a
//!   program of its own** (`fastforwardrs-plugin-scan`), so that a library that crashes or never returns costs that program and not the editor.
//!
//! Nothing in this crate decides whether a library may be loaded. That is the editor's `plugin_trust` module, and nothing but it is to call [`Library::open`].

#![deny(unsafe_code)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod abi;
pub mod description;
mod instance;
mod library;
mod process;
mod scan;

pub use description::{
    DefaultHint, PluginDescription, PluginProperties, PortDescription, PortDirection, PortHint, PortKind,
};
pub use instance::{ControlError, Instance, InstantiateError, RunError};
pub use library::{Library, LibraryDescription, OpenError};
pub use process::prepare_to_crash_quietly;
pub use scan::{LibraryScan, PROBE_BLOCK_MOMENTS, PROBE_SAMPLE_RATE, ScannedPlugin, scan_library};

#[cfg(test)]
mod tests;
