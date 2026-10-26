// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finds, describes and runs LV2 sound plugins, with a host of our own (no `lilv`). **This is a crate of fastforwardrs that has `unsafe` code in it** (the editor keeps
//! `#![forbid(unsafe_code)]`): a plugin is a library of native code that is loaded and called, and Rust cannot check any of it. The interface of this crate is safe, and each
//! `unsafe` block says why it is sound (`clippy::undocumented_unsafe_blocks` is on, and a test of this crate counts them: see `tests.rs`). The design is
//! `docs/engine-phase-11-lv2-design.md`.
//!
//! * [`description`] reads what a bundle says about its plugins (its Turtle), as safe values that can be sent between programs; **it loads nothing**.
//! * [`Library`] is a library that is loaded; [`Instance`] is a plugin that has been made from it, and can be given values and sound, and its state saved and restored.
//!
//! * [`scan_bundle`] reads a bundle, loads its binary, and makes and runs each of its plugins once on silence, saying what it is doing as it goes: **it is meant to be run in a
//!   program of its own** (`fastforwardrs-plugin-scan`), so that a library that crashes or never returns costs that program and not the editor.
//!
//! Nothing in this crate decides whether a library may be loaded. That is the editor's `plugin_trust` module.

#![deny(unsafe_code)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod abi;
pub mod description;
mod features;
mod instance;
mod library;
mod scan;
mod state;
mod worker;

pub use description::{
    AtomPortDescription, BundleDescription, DescribeError, PluginDescription, PortDescription, PortDirection, PortKind,
    ScalePoint, UiDescription, bundles_in, describe_bundle, usual_folders,
};

pub use instance::{Instance, InstantiateError, RunError};
pub use library::{Library, OpenError};
pub use scan::{BundleScan, PROBE_BLOCK_FRAMES, PROBE_SAMPLE_RATE, ScannedPlugin, scan_bundle};
pub use state::StateError;

#[cfg(test)]
mod description_tests;
#[cfg(test)]
mod tests;
