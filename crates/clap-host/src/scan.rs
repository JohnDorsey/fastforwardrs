// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of a library: what it is made of, and whether each of its plugins can be made and run. **It runs the code of the library**, so it is meant for a program of its own.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::instance::{Instance, ParameterChange};
use crate::library::{Library, OpenError, PluginDescription};
use crate::main_thread::MainThread;
use crate::start_check::StartCheck;

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

/// The `CLAP_PARAM_*` flags that the program looks at (`ext/params.h`).
const IS_STEPPED: u32 = 1 << 0;
const IS_HIDDEN: u32 = 1 << 2;
const IS_READONLY: u32 = 1 << 3;
const IS_BYPASS: u32 = 1 << 4;
const IS_ENUM: u32 = 1 << 16;

impl Parameter {
    /// A whole number (the plugin truncates what it is given).
    pub fn is_stepped(&self) -> bool {
        self.flags & IS_STEPPED != 0
    }

    /// Not to be shown to a person.
    pub fn is_hidden(&self) -> bool {
        self.flags & IS_HIDDEN != 0
    }

    /// Cannot be set: the plugin gives it.
    pub fn is_read_only(&self) -> bool {
        self.flags & IS_READONLY != 0
    }

    /// The plugin's own bypass: a stepped value, 0 off and 1 on.
    pub fn is_bypass(&self) -> bool {
        self.flags & IS_BYPASS != 0
    }

    /// Stepped, and every whole value from the minimum to the maximum has a text.
    pub fn is_enum(&self) -> bool {
        self.flags & IS_ENUM != 0
    }
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
    /// Whether the plugin's sound at its start depends on memory it did not write (see [`crate::start_check`]). A scan from before the check has [`StartCheck::NotChecked`].
    #[serde(default)]
    pub start: StartCheck,
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
pub fn scan_library(path: &Path, step: impl FnMut(&str)) -> Result<LibraryScan, OpenError> {
    scan_library_checking_starts_for(path, crate::start_check::LONGEST_FOR_A_LIBRARY, step)
}

/// The same, checking the start of the plugins (see [`crate::start_check`]) for `longest` all together: the plugins after the time is up are not checked.
pub fn scan_library_checking_starts_for(
    path: &Path,
    longest: std::time::Duration,
    mut step: impl FnMut(&str),
) -> Result<LibraryScan, OpenError> {
    step("loading the library");
    let library = Library::open(path)?;
    step("reading what the plugins say of themselves");
    let described = library.describe();
    let worker = MainThread::start();
    let mut plugins = Vec::new();
    let began = std::time::Instant::now();
    for description in described.plugins {
        // (The check of a plugin's start takes about half a second: a library of hundreds of plugins is checked for as long as it is given.)
        let check_the_start = began.elapsed() < longest;
        plugins.push(probe(&worker, &library, description, check_the_start, &mut step));
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
    check_the_start: bool,
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
        start: StartCheck::NotChecked,
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
    if scanned.problem.is_none() && check_the_start {
        step(&format!(
            "making the plugin {} again in memory that holds other bytes, to see whether its start depends on them",
            scanned.description.id
        ));
        scanned.start =
            crate::start_check::check_the_start(worker, library, &scanned.description.id, main_in, main_out);
    }
    scanned
}
