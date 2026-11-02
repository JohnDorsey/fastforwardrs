// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of a module: it is loaded, and each of its plugins is made and run once on silence, saying what is being done as it goes. **It is meant to be run in a program of its
//! own** (`fastforwardrs-plugin-scan`), so that a library that crashes or never returns costs that program and not the editor: loading a library runs code that nobody here wrote.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::description::{ClassDescription, ClassFacts, FactoryDescription};
use crate::instance::Instance;
use crate::module::{Module, OpenError};

/// The sample rate that a plugin is made at to see whether it works.
pub const PROBE_SAMPLE_RATE: f64 = 48_000.0;

/// How many frames the block is that a plugin is run on to see whether it works.
pub const PROBE_BLOCK_FRAMES: usize = 512;

/// A plugin class of a module, with what came of making it and running it once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScannedClass {
    pub class: ClassDescription,
    /// What the class is, if it could be made.
    pub facts: Option<ClassFacts>,
    /// Why it could not be made or run, or made a sound that is not a number, if it could not: such a plugin is not used.
    pub problem: Option<String>,
    /// Whether the plugin has a state to save.
    pub has_state: bool,
}

/// What the scan of a module found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModuleScan {
    pub factory: FactoryDescription,
    /// The classes that are plugins (the others are not listed), in the factory's order.
    pub classes: Vec<ScannedClass>,
}

/// Whether every sample of the buffers is a number (not infinite, and not "not a number"): a plugin whose sound is not is not used.
pub(crate) fn is_made_of_numbers(buffers: &[Vec<f32>]) -> bool {
    buffers.iter().flatten().all(|sample| sample.is_finite())
}

/// Scans the module at `path`, saying each step before it is done (`step`: the last of them says what was being done if the program crashes). An error means the module could not
/// be opened at all.
pub fn scan_module(path: &Path, step: &mut impl FnMut(&str)) -> Result<ModuleScan, OpenError> {
    step(&format!("loading {}", path.display()));
    let module = Module::open(path)?;
    step("reading the classes");
    let mut scan = ModuleScan {
        factory: module.factory_description(),
        classes: Vec::new(),
    };
    for class in module.classes().into_iter().filter(ClassDescription::is_a_plugin) {
        step(&format!("making {} ({})", class.name, class.id));
        let (facts, problem, has_state) = probe(&module, &class, step);
        scan.classes.push(ScannedClass {
            class,
            facts,
            problem,
            has_state,
        });
    }
    Ok(scan)
}

/// Makes the plugin, runs it once on silence, and says what came of it: what it is, why not (if not), and whether it has a state.
fn probe(
    module: &Module,
    class: &ClassDescription,
    step: &mut impl FnMut(&str),
) -> (Option<ClassFacts>, Option<String>, bool) {
    let mut instance = match Instance::new(module, class.id, PROBE_SAMPLE_RATE, PROBE_BLOCK_FRAMES, None) {
        Ok((instance, _)) => instance,
        Err(error) => return (None, Some(error.to_string()), false),
    };
    step(&format!("running {} on silence", class.name));
    let facts = instance.facts().clone();
    let (input_channels, output_channels) = instance.channels();
    let mut inputs: Vec<Vec<f32>> = vec![vec![0.0; PROBE_BLOCK_FRAMES]; input_channels];
    let mut outputs: Vec<Vec<f32>> = vec![vec![0.0; PROBE_BLOCK_FRAMES]; output_channels];
    let mut input_slices: Vec<&mut [f32]> = inputs.iter_mut().map(Vec::as_mut_slice).collect();
    let mut output_slices: Vec<&mut [f32]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
    if let Err(error) = instance.run(&mut input_slices, &mut output_slices, PROBE_BLOCK_FRAMES) {
        return (Some(facts), Some(error.to_string()), false);
    }
    let problem =
        (!is_made_of_numbers(&outputs)).then(|| "its sound is not made of numbers when it is given silence".to_owned());
    (Some(facts), problem, instance.save_state().is_ok())
}
