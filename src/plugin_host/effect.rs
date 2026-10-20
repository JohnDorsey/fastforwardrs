// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin as an effect of the sound of a clip: what changes the sound a block at a time (G5, G10, G11, G18 of `docs/engine-phase-7-design.md`).
//!
//! * **The channel layout** decides how the sound goes through the plugin: a plugin of one input and one output is made **twice** (a copy for each channel, with the same
//!   values), two and two once, one and two once (it is given the mix of the two channels and its two outputs are the channels), two and one once (its output is both channels).
//! * **The values change at the frame the keyframes say** (G11): a value that is animated is looked at every [`SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED`] samples (a little less than a
//!   thousandth of a second), and one that is not at the start of each block. They are what the keyframes say at the frame the sample is in, **kept to the range the plugin
//!   says** (a plugin is not given a value that its hint says it does not take, though a person can type one past the slider).
//! * **A plugin that is too slow is left out** (G18), but only in the live preview: when it takes longer to make a block than the block lasts for [`SECONDS_TOO_SLOW`] seconds
//!   of sound in a row, the sound goes through it unchanged for the rest of the play, and a note says so. The export never leaves one out.
//! * What a plugin gives that is not a number (NaN, infinity) is silence: a plugin that goes wrong does not send that to the speakers.

use std::time::Instant;

use super::instance::{PluginInstance, places_to_look_in};
use crate::composition::LayerEffect;
use crate::project::{FrameRate, InstalledPlugin, PropertyValue};
use crate::sound_engine::{BlockProcessor, Notes, SAMPLE_RATE};

/// How many samples apart the values of an effect that is animated are looked at (32 is 0.67 milliseconds at 48000 a second).
pub const SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED: usize = 32;

/// How many times the loudest sound there can be (1.0) a sample may be before it is taken for a plugin gone wrong and made silent: 60 decibels over. (A plugin may be louder than
/// 1.0, as a later one may bring it back; a sample beyond this would be heard as a crash by anyone, whatever follows it.)
pub const MOST_A_PLUGIN_MAY_GIVE: f32 = 1000.0;

/// How many seconds of sound a plugin may be slower than the sound, one after another, before it is left out of the preview.
pub const SECONDS_TOO_SLOW: f64 = 2.0;

/// What is wrong with the plugins that were wanted in the sound, by their keys: what a window shows of an effect that does nothing (a plugin that could not be loaded, that is
/// not what was scanned, that could not be run, that was too slow). An entry is taken away when the plugin is wanted again and nothing is wrong.
pub type ProblemBoard = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>;

/// How a plugin is given the sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One audio input and one output: two copies, one for each channel.
    EachChannelOnItsOwn,
    /// Two and two.
    Stereo,
    /// One input (the mix of the channels) and two outputs.
    MonoToStereo,
    /// Two inputs and one output (which is both channels).
    StereoToMono,
}

impl Layout {
    /// The layout of a plugin that has `inputs` audio inputs and `outputs` audio outputs, if it is one of the four.
    pub fn of(inputs: usize, outputs: usize) -> Option<Self> {
        match (inputs, outputs) {
            (1, 1) => Some(Self::EachChannelOnItsOwn),
            (2, 2) => Some(Self::Stereo),
            (1, 2) => Some(Self::MonoToStereo),
            (2, 1) => Some(Self::StereoToMono),
            _ => None,
        }
    }

    /// How many instances of the plugin the layout needs.
    pub fn instances(self) -> usize {
        match self {
            Self::EachChannelOnItsOwn => 2,
            Self::Stereo | Self::MonoToStereo | Self::StereoToMono => 1,
        }
    }
}

/// Whether a plugin has been too slow for too long: it is told how long each block was and how long the plugin took.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlownessGuard {
    seconds_too_slow_in_a_row: f64,
    /// How many seconds of sound, one after another, it may be too slow for.
    patience: f64,
}

impl Default for SlownessGuard {
    fn default() -> Self {
        Self::with_patience(SECONDS_TOO_SLOW)
    }
}

impl SlownessGuard {
    /// A guard that gives up on a plugin that is too slow for `seconds` of sound in a row.
    pub fn with_patience(seconds: f64) -> Self {
        Self {
            seconds_too_slow_in_a_row: 0.0,
            patience: seconds,
        }
    }

    /// Takes in a block of `block_seconds` of sound that took `took_seconds` to make; says whether the plugin is to be left out now.
    pub fn is_it_too_slow(&mut self, block_seconds: f64, took_seconds: f64) -> bool {
        if took_seconds > block_seconds {
            self.seconds_too_slow_in_a_row += block_seconds;
        } else {
            self.seconds_too_slow_in_a_row = 0.0;
        }
        self.seconds_too_slow_in_a_row >= self.patience
    }
}

/// The frame of the timeline that the sample `sample` of a clip that begins at the frame `first_frame` is in.
pub fn frame_of_a_sample(frame_rate: FrameRate, first_frame: u64, sample: u64) -> u64 {
    let frames = u128::from(sample) * u128::from(frame_rate.numerator())
        / (u128::from(frame_rate.denominator()) * u128::from(SAMPLE_RATE));
    first_frame + u64::try_from(frames).unwrap_or(u64::MAX)
}

/// A plugin that is an effect of the sound of a clip.
pub struct PluginEffect {
    instances: Vec<Box<dyn PluginInstance>>,
    layout: Layout,
    plugin: &'static InstalledPlugin,
    effect: LayerEffect,
    frame_rate: FrameRate,
    first_timeline_frame: u64,
    /// How many samples of the clip have gone through.
    samples_done: u64,
    /// How many of the samples that went through before the first sample of the clip were from before the clip (see `BlockProcessor::about_to_be_given_sound_from_before_the_clip`).
    samples_before_the_clip: u64,
    /// Whether the values are looked at often, because some of them have keyframes.
    animated: bool,
    /// The values as they were last given to the plugin, by their places, so that a value that has not changed is not given again.
    given: Vec<f32>,
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
    /// `Some` where a plugin that is too slow is left out (the preview).
    guard: Option<SlownessGuard>,
    left_out: bool,
    /// Whether a note has been made of sound that was not fit to be heard (it is said once).
    has_said_it_gave_too_much: bool,
    notes: Notes,
    /// Where a plugin that is left out says why, for the window.
    problems: Option<ProblemBoard>,
}

impl PluginEffect {
    /// The effect `effect` of the plugin `plugin`, made of `instances` (as many as the layout needs, all of the plugin), on a clip that begins at `first_timeline_frame`.
    pub fn new(
        instances: Vec<Box<dyn PluginInstance>>,
        plugin: &'static InstalledPlugin,
        effect: LayerEffect,
        frame_rate: FrameRate,
        first_timeline_frame: u64,
        leave_out_one_that_is_too_slow: bool,
        notes: Notes,
    ) -> Option<Self> {
        let layout = Layout::of(plugin.audio_inputs, plugin.audio_outputs)?;
        if instances.len() != layout.instances() {
            return None;
        }
        let animated = !effect.animation.is_empty();
        Some(Self {
            instances,
            layout,
            plugin,
            effect,
            frame_rate,
            first_timeline_frame,
            samples_done: 0,
            samples_before_the_clip: 0,
            animated,
            given: Vec::new(),
            input: [Vec::new(), Vec::new()],
            output: [Vec::new(), Vec::new()],
            guard: leave_out_one_that_is_too_slow.then(SlownessGuard::default),
            left_out: false,
            has_said_it_gave_too_much: false,
            notes,
            problems: None,
        })
    }

    /// The same, left out for being too slow after `seconds` of sound in a row and not [`SECONDS_TOO_SLOW`] (for a test that cannot wait for two). Does nothing to an effect that
    /// is not guarded (the export).
    #[must_use]
    pub fn with_patience(mut self, seconds: f64) -> Self {
        if self.guard.is_some() {
            self.guard = Some(SlownessGuard::with_patience(seconds));
        }
        self
    }

    /// The same, saying on `board` why it does nothing when it is left out.
    #[must_use]
    pub fn reporting_problems_on(mut self, board: ProblemBoard) -> Self {
        self.problems = Some(board);
        self
    }

    /// Says on the board, if there is one, why this plugin does nothing now.
    fn put_on_the_board(&self, line: &str) {
        if let Some(board) = &self.problems {
            board
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(self.plugin.descriptor.key.to_string(), line.to_owned());
        }
    }

    /// Whether the sound goes through the plugin unchanged because it was too slow.
    pub fn is_left_out(&self) -> bool {
        self.left_out
    }

    fn say(&self, line: String) {
        self.notes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(line);
    }

    /// Gives the plugin the values the keyframes say at `timeline_frame`, those that have changed, to be applied from sample `at_sample` of the next run.
    fn give_the_values_at(&mut self, timeline_frame: u64, at_sample: usize) {
        let values = self.effect.values_at(timeline_frame);
        let wanted: Vec<f32> = values
            .iter()
            .enumerate()
            .map(|(slot, value)| {
                let number = match value {
                    PropertyValue::Number(number) => *number,
                    PropertyValue::Flag(on) => f32::from(u8::from(*on)),
                    // (The id of an option of a choice is the number the plugin gives it.)
                    PropertyValue::Words(words) => words.trim().parse::<f32>().unwrap_or(0.0),
                    PropertyValue::Pair(..) | PropertyValue::Colour(_) => 0.0,
                };
                // (A value that is typed past the range is kept to it when the plugin is run.)
                match self
                    .plugin
                    .descriptor
                    .parameters
                    .get(slot)
                    .and_then(|parameter| parameter.range)
                {
                    Some((least, most)) if number.is_finite() => number.clamp(least, most),
                    _ => number,
                }
            })
            .collect();
        if wanted == self.given {
            return;
        }
        for (slot, value) in wanted.iter().enumerate() {
            if self.given.get(slot) == Some(value) {
                continue;
            }
            let Some(&port) = self.plugin.value_handles.get(slot) else {
                continue;
            };
            for instance in &mut self.instances {
                instance.set_value(port, *value, at_sample);
            }
        }
        self.given = wanted;
    }

    /// Runs the plugin on the sound in `left` and `right` (as long as each other), which are the sound as it is until it is changed.
    fn run_on(&mut self, left: &mut [f32], right: &mut [f32]) {
        let length = left.len();
        for buffer in self.input.iter_mut().chain(self.output.iter_mut()) {
            buffer.clear();
            buffer.resize(length, 0.0);
        }
        let [input_left, input_right] = &mut self.input;
        let [output_left, output_right] = &mut self.output;
        let result = match self.layout {
            Layout::EachChannelOnItsOwn => {
                input_left.copy_from_slice(left);
                input_right.copy_from_slice(right);
                let (first, rest) = self.instances.split_at_mut(1);
                first[0]
                    .run(
                        &mut [input_left.as_mut_slice()],
                        &mut [output_left.as_mut_slice()],
                        length,
                    )
                    .and_then(|()| {
                        rest[0].run(
                            &mut [input_right.as_mut_slice()],
                            &mut [output_right.as_mut_slice()],
                            length,
                        )
                    })
            }
            Layout::Stereo => {
                input_left.copy_from_slice(left);
                input_right.copy_from_slice(right);
                self.instances[0].run(
                    &mut [input_left.as_mut_slice(), input_right.as_mut_slice()],
                    &mut [output_left.as_mut_slice(), output_right.as_mut_slice()],
                    length,
                )
            }
            Layout::MonoToStereo => {
                for (mixed, (l, r)) in input_left.iter_mut().zip(left.iter().zip(right.iter())) {
                    *mixed = (l + r) * 0.5;
                }
                self.instances[0].run(
                    &mut [input_left.as_mut_slice()],
                    &mut [output_left.as_mut_slice(), output_right.as_mut_slice()],
                    length,
                )
            }
            Layout::StereoToMono => {
                input_left.copy_from_slice(left);
                input_right.copy_from_slice(right);
                let result = self.instances[0].run(
                    &mut [input_left.as_mut_slice(), input_right.as_mut_slice()],
                    &mut [output_left.as_mut_slice()],
                    length,
                );
                output_right.copy_from_slice(output_left);
                result
            }
        };
        if let Err(error) = result {
            self.left_out = true;
            let line = format!(
                "The sound plugin \u{ab}{}\u{bb} could not be run ({error}), so the sound goes through it unchanged.",
                self.plugin.descriptor.name
            );
            self.put_on_the_board(&line);
            self.say(line);
            return;
        }
        // (A sample that is not a number, or is louder than any sound should be, is silence in its place.)
        let is_fit = |sample: f32| sample.is_finite() && sample.abs() <= MOST_A_PLUGIN_MAY_GIVE;
        let mut was_unfit = false;
        for (sound, made) in left
            .iter_mut()
            .zip(output_left.iter())
            .chain(right.iter_mut().zip(output_right.iter()))
        {
            was_unfit |= !is_fit(*made);
            *sound = if is_fit(*made) { *made } else { 0.0 };
        }
        if was_unfit && !self.has_said_it_gave_too_much {
            self.has_said_it_gave_too_much = true;
            self.say(format!(
                "The sound plugin \u{ab}{}\u{bb} gave sound that is not a number, or more than {MOST_A_PLUGIN_MAY_GIVE} times as loud as the loudest sound there can be, and that sound was made silent.",
                self.plugin.descriptor.name
            ));
        }
    }
}

impl PluginEffect {
    /// The number of the sample of the clip that the sample `at` of the block being made is: the samples from before the clip are the first moment of it.
    fn sample_of_the_clip(&self, at: u64) -> u64 {
        (self.samples_done + at).saturating_sub(self.samples_before_the_clip)
    }

    /// Things a plugin has asked of the host that are done between blocks: a call on its main thread, being made again (it then has to be given its values again).
    fn do_what_the_plugins_asked_between_blocks(&mut self) {
        for number in 0..self.instances.len() {
            let done = self.instances[number].between_blocks();
            if done.was_restarted {
                self.given.clear();
            }
            if let Some(note) = done.note {
                self.say(format!(
                    "The sound plugin \u{ab}{}\u{bb}: {note}.",
                    self.plugin.descriptor.name
                ));
            }
        }
    }

    /// Runs the block with the values given at the samples where the keyframes say, in one run (or several, if the block is longer than the plugin takes): for a plugin that
    /// is told when a value changes.
    fn process_with_values_at_samples(&mut self, left: &mut [f32], right: &mut [f32], total: usize) {
        let most = self.instances[0].most_samples_at_once().unwrap_or(total).max(1);
        let mut piece_start = 0;
        while piece_start < total && !self.left_out {
            let piece_end = (piece_start + most).min(total);
            let first_sample = self.sample_of_the_clip(piece_start as u64);
            for place in places_to_look_in(first_sample, piece_end - piece_start, self.animated) {
                let frame = frame_of_a_sample(self.frame_rate, self.first_timeline_frame, first_sample + place as u64);
                self.give_the_values_at(frame, place);
            }
            self.run_on(&mut left[piece_start..piece_end], &mut right[piece_start..piece_end]);
            piece_start = piece_end;
        }
    }
}

impl BlockProcessor for PluginEffect {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let total = left.len().min(right.len());
        if total == 0 || self.left_out {
            self.samples_done += total as u64;
            return;
        }
        let began = Instant::now();
        self.do_what_the_plugins_asked_between_blocks();
        if self.instances[0].takes_values_at_samples() {
            self.process_with_values_at_samples(left, right, total);
        } else {
            let step = if self.animated {
                SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED
            } else {
                total
            };
            let mut at = 0;
            while at < total && !self.left_out {
                let end = (at + step).min(total);
                let frame = frame_of_a_sample(
                    self.frame_rate,
                    self.first_timeline_frame,
                    self.sample_of_the_clip(at as u64),
                );
                self.give_the_values_at(frame, 0);
                self.run_on(&mut left[at..end], &mut right[at..end]);
                at = end;
            }
        }
        self.samples_done += total as u64;
        if let Some(guard) = &mut self.guard {
            let block_seconds = total as f64 / f64::from(SAMPLE_RATE);
            let patience = guard.patience;
            if !self.left_out && guard.is_it_too_slow(block_seconds, began.elapsed().as_secs_f64()) {
                self.left_out = true;
                let name = self.plugin.descriptor.name.to_string();
                let line = format!(
                    "The sound plugin \u{ab}{name}\u{bb} was too slow to keep up with the sound for {patience} seconds, so the sound goes through it unchanged for the rest of this play."
                );
                self.put_on_the_board(&line);
                self.say(line);
            }
        }
    }

    fn reset(&mut self) {
        for instance in &mut self.instances {
            instance.reset();
        }
        self.given.clear();
    }

    fn about_to_be_given_sound_from_before_the_clip(&mut self, samples: usize) {
        self.samples_before_the_clip = samples as u64;
    }

    fn latency(&self) -> usize {
        self.instances
            .iter()
            .map(|instance| instance.latency())
            .max()
            .unwrap_or(0)
    }
}
