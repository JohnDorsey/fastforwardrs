// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of a bundle: its Turtle is read, its binary is loaded, and each of its plugins is made and run once on silence, saying what is being done as it goes. **It is meant
//! to be run in a program of its own** (`fastforwardrs-plugin-scan`), so that a library that crashes or never returns costs that program and not the editor: loading a library
//! runs code that nobody here wrote.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::description::{DescribeError, PluginDescription, PortDirection, PortKind, describe_bundle};
use crate::instance::Instance;
use crate::library::Library;

/// The sample rate that a plugin is made at to see whether it works.
pub const PROBE_SAMPLE_RATE: f64 = 48_000.0;

/// How many frames the block is that a plugin is run on to see whether it works.
pub const PROBE_BLOCK_FRAMES: u32 = 512;

/// A plugin of a bundle, with what came of making it and running it once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScannedPlugin {
    pub description: PluginDescription,
    /// Why it could not be made or run, or made a sound that is not a number, if it could not: such a plugin is not used.
    pub problem: Option<String>,
    /// How late its sound was after the one block (the latency port's value), in frames.
    pub latency: u32,
    /// Whether it has a state to save (`state:interface`).
    pub has_state: bool,
}

/// What the scan of a bundle found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleScan {
    pub plugins: Vec<ScannedPlugin>,
    /// What could not be understood of the bundle (its Turtle), one line for each.
    pub problems: Vec<String>,
}

/// Scans the bundle in the folder `bundle`, saying each step before it is done (`step`: the last of them says what was being done if the program crashes). An error means the
/// bundle could not be read at all (no manifest, or one that is not Turtle).
pub fn scan_bundle(bundle: &Path, step: &mut impl FnMut(&str)) -> Result<BundleScan, DescribeError> {
    step(&format!("reading the Turtle of {}", bundle.display()));
    let described = describe_bundle(bundle)?;
    let mut scan = BundleScan {
        plugins: Vec::new(),
        problems: described.problems,
    };
    // (A binary may have several plugins: it is loaded once, and what came of loading it is kept.)
    let mut libraries: HashMap<PathBuf, Result<Library, String>> = HashMap::new();
    for description in described.plugins {
        let library = libraries.entry(description.binary.clone()).or_insert_with(|| {
            step(&format!("loading {}", description.binary.display()));
            Library::open(&description.binary).map_err(|error| error.to_string())
        });
        let (problem, latency, has_state) = match library {
            Ok(library) => probe(library, &description, step),
            Err(reason) => (Some(reason.clone()), 0, false),
        };
        scan.plugins.push(ScannedPlugin {
            description,
            problem,
            latency,
            has_state,
        });
    }
    Ok(scan)
}

/// Whether every sample of the buffers is a number (not infinite, and not "not a number"): a plugin whose sound is not is not used.
pub(crate) fn is_made_of_numbers(buffers: &[Vec<f32>]) -> bool {
    buffers.iter().flatten().all(|sample| sample.is_finite())
}

/// Makes the plugin, runs it once on silence, and says what came of it: why not (if not), its latency, and whether it has a state.
fn probe(
    library: &Library,
    description: &PluginDescription,
    step: &mut impl FnMut(&str),
) -> (Option<String>, u32, bool) {
    step(&format!("making {}", description.uri));
    let mut instance = match Instance::new(library, description, PROBE_SAMPLE_RATE, PROBE_BLOCK_FRAMES) {
        Ok(instance) => instance,
        Err(error) => return (Some(error.to_string()), 0, false),
    };
    step(&format!("running {} on silence", description.uri));
    let frames = PROBE_BLOCK_FRAMES as usize;
    let inputs = description
        .ports
        .iter()
        .filter(|port| port.kind == PortKind::Audio && port.direction == PortDirection::Input)
        .count();
    let outputs = description
        .ports
        .iter()
        .filter(|port| port.kind == PortKind::Audio && port.direction == PortDirection::Output)
        .count();
    let mut input_buffers: Vec<Vec<f32>> = vec![vec![0.0; frames]; inputs];
    let mut output_buffers: Vec<Vec<f32>> = vec![vec![0.0; frames]; outputs];
    let mut input_slices: Vec<&mut [f32]> = input_buffers.iter_mut().map(Vec::as_mut_slice).collect();
    let mut output_slices: Vec<&mut [f32]> = output_buffers.iter_mut().map(Vec::as_mut_slice).collect();
    if let Err(error) = instance.run(&mut input_slices, &mut output_slices, frames) {
        return (Some(error.to_string()), 0, false);
    }
    let problem = (!is_made_of_numbers(&output_buffers))
        .then(|| "its sound is not made of numbers when it is given silence".to_owned());
    (problem, instance.latency(), instance.has_state())
}
