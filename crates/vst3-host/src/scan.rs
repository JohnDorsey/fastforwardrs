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
    /// Whether the plugin has a window that this system can put in a parent (its controller makes one). **A window is made to find out, and let go of at once.**
    #[serde(default)]
    pub has_a_window: bool,
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
        let probed = probe(&module, &class, step);
        scan.classes.push(ScannedClass {
            class,
            facts: probed.facts,
            problem: probed.problem,
            has_state: probed.has_state,
            has_a_window: probed.has_a_window,
        });
    }
    Ok(scan)
}

/// What making a plugin and running it once came to.
#[derive(Default)]
struct Probed {
    facts: Option<ClassFacts>,
    problem: Option<String>,
    has_state: bool,
    has_a_window: bool,
}

/// Makes the plugin, runs it once on silence, and says what came of it: what it is, why not (if not), whether it has a state and whether it has a window.
fn probe(module: &Module, class: &ClassDescription, step: &mut impl FnMut(&str)) -> Probed {
    let mut instance = match Instance::new(module, class.id, PROBE_SAMPLE_RATE, PROBE_BLOCK_FRAMES, None) {
        Ok((instance, _)) => instance,
        Err(error) => {
            return Probed {
                problem: Some(error.to_string()),
                ..Probed::default()
            };
        }
    };
    step(&format!("running {} on silence", class.name));
    let facts = instance.facts().clone();
    let (input_channels, output_channels) = instance.channels();
    let mut inputs: Vec<Vec<f32>> = vec![vec![0.0; PROBE_BLOCK_FRAMES]; input_channels];
    let mut outputs: Vec<Vec<f32>> = vec![vec![0.0; PROBE_BLOCK_FRAMES]; output_channels];
    let mut input_slices: Vec<&mut [f32]> = inputs.iter_mut().map(Vec::as_mut_slice).collect();
    let mut output_slices: Vec<&mut [f32]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
    if let Err(error) = instance.run(&mut input_slices, &mut output_slices, PROBE_BLOCK_FRAMES) {
        return Probed {
            facts: Some(facts),
            problem: Some(error.to_string()),
            ..Probed::default()
        };
    }
    let problem =
        (!is_made_of_numbers(&outputs)).then(|| "its sound is not made of numbers when it is given silence".to_owned());
    let has_state = instance.save_state().is_ok();
    step(&format!("looking for the window of {}", class.name));
    let has_a_window = problem.is_none() && instance.has_a_view();
    Probed {
        facts: Some(facts),
        problem,
        has_state,
        has_a_window,
    }
}
