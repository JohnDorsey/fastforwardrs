// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of a library: what it is made of, and whether each of its plugins can be made and run. **It runs the code of the library**, so it is meant for a program of its own.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::instance::{Instance, ParameterChange};
use crate::library::{Library, OpenError, PluginDescription};
use crate::main_thread::MainThread;

/// The sample rate that a plugin is made at when it is tried.
pub const PROBE_SAMPLE_RATE: f64 = 48_000.0;

/// How many frames the plugin is run on when it is tried.
pub const PROBE_BLOCK_FRAMES: usize = 512;

/// An audio port of a plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPort {
    pub id: u32,
    pub name: String,
    pub channel_count: u32,
    /// `mono`, `stereo`... (`None`: it says nothing).
    pub port_type: Option<String>,
    pub is_main: bool,
}

/// A parameter of a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// The id the plugin gave it, which never changes.
    pub id: u32,
    pub name: String,
    /// Where it is in a tree of parameters (`filter/envelope`).
    pub module: String,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
    /// The `CLAP_PARAM_*` flags.
    pub flags: u32,
    /// For an enum: the text of each value from the minimum up.
    pub texts: Vec<String>,
}

/// A plugin that was found, and whether it could be made and run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScannedPlugin {
    pub description: PluginDescription,
    pub audio_inputs: Vec<AudioPort>,
    pub audio_outputs: Vec<AudioPort>,
    pub parameters: Vec<Parameter>,
    pub latency: u32,
    /// `Some` if the plugin says how long its tail is.
    pub tail: Option<u32>,
    pub has_state: bool,
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

/// Loads the library at `path`, describes it, and makes each plugin once and runs it on a block of silence. `step` is told what is about to be done, before it is done.
pub fn scan_library(path: &Path, mut step: impl FnMut(&str)) -> Result<LibraryScan, OpenError> {
    step("loading the library");
    let library = Library::open(path)?;
    step("reading what the plugins say of themselves");
    let described = library.describe();
    let worker = MainThread::start();
    let mut plugins = Vec::new();
    for description in described.plugins {
        plugins.push(probe(&worker, &library, description, &mut step));
    }
    Ok(LibraryScan {
        plugins,
        problems: described.problems,
    })
}

fn probe(
    worker: &Arc<MainThread>,
    library: &Library,
    description: PluginDescription,
    step: &mut impl FnMut(&str),
) -> ScannedPlugin {
    step(&format!(
        "making the plugin {} at {PROBE_SAMPLE_RATE} Hz",
        description.id
    ));
    let mut scanned = ScannedPlugin {
        description,
        audio_inputs: Vec::new(),
        audio_outputs: Vec::new(),
        parameters: Vec::new(),
        latency: 0,
        tail: None,
        has_state: false,
        problem: None,
    };
    let mut instance = match Instance::new(
        worker,
        library,
        &scanned.description.id,
        PROBE_SAMPLE_RATE,
        PROBE_BLOCK_FRAMES as u32,
    ) {
        Ok(instance) => instance,
        Err(error) => {
            scanned.problem = Some(error.to_string());
            return scanned;
        }
    };
    scanned.audio_inputs = instance.audio_inputs().to_vec();
    scanned.audio_outputs = instance.audio_outputs().to_vec();
    scanned.parameters = instance.parameters().to_vec();
    scanned.latency = instance.latency();
    scanned.has_state = instance.has_state();
    step(&format!(
        "running the plugin {} on {PROBE_BLOCK_FRAMES} frames of silence",
        scanned.description.id
    ));
    let main_in = scanned
        .audio_inputs
        .first()
        .map_or(0, |port| port.channel_count as usize);
    let main_out = scanned
        .audio_outputs
        .first()
        .map_or(0, |port| port.channel_count as usize);
    let mut input = vec![vec![0.0_f32; PROBE_BLOCK_FRAMES]; main_in];
    let mut output = vec![vec![0.0_f32; PROBE_BLOCK_FRAMES]; main_out];
    {
        let mut input_slices: Vec<&mut [f32]> = input.iter_mut().map(Vec::as_mut_slice).collect();
        let mut output_slices: Vec<&mut [f32]> = output.iter_mut().map(Vec::as_mut_slice).collect();
        let no_changes: [ParameterChange; 0] = [];
        if let Err(error) = instance.run(&mut input_slices, &mut output_slices, &no_changes) {
            scanned.problem = Some(error.to_string());
        }
    }
    scanned.tail = instance.tail();
    step(&format!("stopping the plugin {}", scanned.description.id));
    drop(instance);
    scanned
}
