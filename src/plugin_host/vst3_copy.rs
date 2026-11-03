// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A copy of a VST3 plugin, as the effect runs it (phase 15, piece 15.3, `docs/engine-phase-15-vst3-design.md`). **A VST3 plugin is never run in the editor's process**: the copy is made
//! in the program that runs plugins (`plugin_process`), which gives the editor a `RemoteCopy` in its place. This is the copy the program holds, and the copy a test makes in its own
//! process to see that the program's sound is the same.

use std::sync::Arc;

use super::instance::{BetweenBlocks, MOST_RESTARTS_OF_A_COPY, PluginInstance};
use crate::sound_engine::{NoteEvent, SAMPLE_RATE};

/// How many frames a VST3 plugin is set up for at most, and so how many it is given in one call (a block of the sound engine is no more than this; a longer stretch is given in pieces).
pub const MOST_FRAMES_OF_A_VST3_PLUGIN: usize = crate::sound_engine::BLOCK_SAMPLES;

/// What makes a VST3 plugin again, when it asks to be made again.
pub struct Vst3Maker {
    pub module: vst3_host::Module,
    pub class_id: vst3_host::ClassId,
    /// What the plugin saved of what is not one of its values, if the project has it: each copy is given it as soon as it is made, before it is given any value.
    pub state: Option<Arc<[u8]>>,
}

impl Vst3Maker {
    /// A copy of the plugin, set up at the rate of the sound engine and given its state if there is one. A state that the plugin cannot take is not a reason to go without it: the copy
    /// is made and the second thing that comes is what is said of it. (An instrument that is given a state first runs a silent block, so that the state is applied before the first
    /// note: see `vst3_host::Instance::new`.)
    pub fn make_and_say_if_the_state_was_not_taken(
        &self,
    ) -> Result<(vst3_host::Instance, Option<String>), vst3_host::InstantiateError> {
        vst3_host::Instance::new(
            &self.module,
            self.class_id,
            f64::from(SAMPLE_RATE),
            MOST_FRAMES_OF_A_VST3_PLUGIN,
            self.state.as_deref(),
        )
    }

    /// The same, when what is said of a state that was not taken is of no use (a copy that is made again).
    pub fn make(&self) -> Result<vst3_host::Instance, vst3_host::InstantiateError> {
        self.make_and_say_if_the_state_was_not_taken()
            .map(|(instance, _)| instance)
    }
}

/// A copy of a VST3 plugin.
pub struct Vst3Copy {
    instance: vst3_host::Instance,
    maker: Vst3Maker,
    restarts: usize,
    /// Whether the person was told that some controllers or pitch wheel positions of the notes had no parameter to go to.
    said_that_events_were_left_out: bool,
}

impl Vst3Copy {
    pub fn new(instance: vst3_host::Instance, maker: Vst3Maker) -> Self {
        Self {
            instance,
            maker,
            restarts: 0,
            said_that_events_were_left_out: false,
        }
    }
}

impl PluginInstance for Vst3Copy {
    fn set_value(&mut self, handle: u32, value: f32, at_sample: usize) {
        self.instance.set_value(handle, f64::from(value), at_sample);
    }

    fn takes_values_at_samples(&self) -> bool {
        true
    }

    fn most_samples_at_once(&self) -> Option<usize> {
        Some(MOST_FRAMES_OF_A_VST3_PLUGIN)
    }

    fn run(&mut self, inputs: &mut [&mut [f32]], outputs: &mut [&mut [f32]], frames: usize) -> Result<(), String> {
        self.instance
            .run(inputs, outputs, frames)
            .map_err(|error| error.to_string())
    }

    fn run_with_notes(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
        notes: &[NoteEvent],
    ) -> Result<(), String> {
        let notes: Vec<vst3_host::NoteEvent> = notes.iter().map(|note| (*note).into()).collect();
        self.instance
            .run_with_notes(inputs, outputs, frames, &notes)
            .map_err(|error| error.to_string())
    }

    fn reset(&mut self) {
        self.instance.reset();
    }

    fn latency(&self) -> usize {
        self.instance.latency() as usize
    }

    fn between_blocks(&mut self) -> BetweenBlocks {
        // (What the plugin edited by itself, with no window to do it, is given to its next block by the instance, and is not a person's edit: it is not told.)
        let latency_before = self.instance.latency();
        let asked = self.instance.between_blocks();
        let mut said: Vec<String> = Vec::new();
        if self.instance.take_the_count_of_events_left_out() > 0 && !self.said_that_events_were_left_out {
            self.said_that_events_were_left_out = true;
            said.push(
                "it has no parameter for some of the controllers and pitch wheel positions of the notes it is played, which are left out"
                    .to_owned(),
            );
        }
        let mut was_restarted = false;
        if asked.wants_to_be_made_again {
            if self.restarts < MOST_RESTARTS_OF_A_COPY {
                self.restarts += 1;
                match self.maker.make() {
                    Ok(again) => {
                        let latency_changed = again.latency() != self.instance.latency();
                        self.instance = again;
                        was_restarted = true;
                        if latency_changed {
                            said.push(format!(
                                "its latency changed when it was made again, and {NOT_MADE_UP_FOR}"
                            ));
                        }
                    }
                    Err(error) => said.push(format!("it asked to be made again and could not be: {error}")),
                }
            } else if self.restarts == MOST_RESTARTS_OF_A_COPY {
                self.restarts += 1;
                said.push(format!(
                    "it asked to be made again more than {MOST_RESTARTS_OF_A_COPY} times, and is not made again any more (it goes on as it is)"
                ));
            }
        } else if asked.latency_changed && self.instance.latency() != latency_before {
            said.push(format!("its latency changed, and {NOT_MADE_UP_FOR}"));
        }
        BetweenBlocks {
            was_restarted,
            note: (!said.is_empty()).then(|| said.join("; ")),
        }
    }
}

const NOT_MADE_UP_FOR: &str = "the sound is not made up for the new latency until the next play";
