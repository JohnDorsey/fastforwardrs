// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of a library: what it is made of, and whether each of its plugins can be made and run. **It runs the code of the library**, so it is meant for a program of its own.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::description::{PluginDescription, PortDirection, PortKind};
use crate::library::{Library, OpenError};

/// The sample rate that a plugin is made at when it is tried.
pub const PROBE_SAMPLE_RATE: u32 = 48_000;

/// How many moments of silence a plugin is run on when it is tried.
pub const PROBE_BLOCK_MOMENTS: usize = 512;

/// A plugin that was found, and whether it could be made and run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScannedPlugin {
    pub description: PluginDescription,
    /// Why the plugin cannot be used, if it cannot be made or run.
    pub problem: Option<String>,
}

/// What the scan of a library found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryScan {
    pub plugins: Vec<ScannedPlugin>,
    /// Plugins whose descriptors could not be used, one line each.
    pub problems: Vec<String>,
}

/// Loads the library at `path`, describes it, and makes each plugin once and runs it on a block of silence. `step` is told what is about to be done, before it is done, so
/// that a program that is watching can say what it was doing when the library crashed it or took for ever.
pub fn scan_library(path: &Path, mut step: impl FnMut(&str)) -> Result<LibraryScan, OpenError> {
    step("loading the library");
    let library = Library::open(path)?;
    step("reading what the plugins say of themselves");
    let described = library.describe();
    let mut plugins = Vec::new();
    for description in described.plugins {
        let problem = try_the_plugin(&library, &description, &mut step);
        plugins.push(ScannedPlugin { description, problem });
    }
    Ok(LibraryScan {
        plugins,
        problems: described.problems,
    })
}

/// Makes the plugin and runs it once on silence, with its controls at their defaults. Gives why it could not, if it could not.
fn try_the_plugin(library: &Library, description: &PluginDescription, step: &mut impl FnMut(&str)) -> Option<String> {
    step(&format!(
        "making plugin number {} ({}) at {PROBE_SAMPLE_RATE} Hz",
        description.index, description.label
    ));
    let mut instance = match library.instantiate(description.index, PROBE_SAMPLE_RATE) {
        Ok(instance) => instance,
        Err(error) => return Some(error.to_string()),
    };
    step(&format!(
        "running plugin number {} ({}) on {PROBE_BLOCK_MOMENTS} moments of silence",
        description.index, description.label
    ));
    let (inputs, outputs) = description.audio_port_counts();
    debug_assert_eq!(
        description.ports_of(PortDirection::Input, PortKind::Audio).count(),
        inputs
    );
    let mut input_buffers = vec![vec![0.0_f32; PROBE_BLOCK_MOMENTS]; inputs];
    let mut output_buffers = vec![vec![0.0_f32; PROBE_BLOCK_MOMENTS]; outputs];
    let mut input_slices: Vec<&mut [f32]> = input_buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let mut output_slices: Vec<&mut [f32]> = output_buffers.iter_mut().map(Vec::as_mut_slice).collect();
    if let Err(error) = instance.run(&mut input_slices, &mut output_slices, PROBE_BLOCK_MOMENTS) {
        return Some(error.to_string());
    }
    step(&format!(
        "stopping plugin number {} ({})",
        description.index, description.label
    ));
    instance.deactivate();
    None
}
