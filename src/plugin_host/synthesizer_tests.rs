// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A synthesizer as an effect of the sound (phase 13, piece 13.2): the sound that reaches it passes by, what it makes is added to it, and the notes that reach it in a block are given to it at
//! the samples they are at, in pieces if the block is longer than the plugin takes. A plugin instance that is written here records what it is given, so that each case is exactly what is tried.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use super::*;
use crate::composition::LayerEffect;
use crate::project::{
    Animation, EffectDescriptor, EffectDomain, EffectInstanceId, EffectKind, FrameRate, InstalledPlugin, PluginAddress,
    UnknownFields,
};
use crate::sound_engine::{BlockProcessor, NoteEvent, NoteEventKind, Notes};

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

/// What a plugin instance was given, one entry for each run.
type Calls = Arc<Mutex<Vec<Call>>>;

#[derive(Clone, Debug, PartialEq)]
struct Call {
    frames: usize,
    inputs: usize,
    outputs: usize,
    notes: Vec<NoteEvent>,
    with_notes: bool,
}

/// A plugin instance that fills output channel `n` with `levels[n]`, and records what it was given.
struct Recorder {
    levels: Vec<f32>,
    most: Option<usize>,
    calls: Calls,
    fails: bool,
}

impl PluginInstance for Recorder {
    fn set_value(&mut self, _handle: u32, _value: f32, _at_sample: usize) {}

    fn takes_values_at_samples(&self) -> bool {
        true
    }

    fn most_samples_at_once(&self) -> Option<usize> {
        self.most
    }

    fn run(&mut self, inputs: &mut [&mut [f32]], outputs: &mut [&mut [f32]], frames: usize) -> Result<(), String> {
        self.record(inputs, outputs, frames, &[], false)
    }

    fn run_with_notes(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
        notes: &[NoteEvent],
    ) -> Result<(), String> {
        self.record(inputs, outputs, frames, notes, true)
    }

    fn reset(&mut self) {}

    fn latency(&self) -> usize {
        0
    }
}

impl Recorder {
    fn record(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
        notes: &[NoteEvent],
        with_notes: bool,
    ) -> Result<(), String> {
        self.calls.lock().unwrap().push(Call {
            frames,
            inputs: inputs.len(),
            outputs: outputs.len(),
            notes: notes.to_vec(),
            with_notes,
        });
        if self.fails {
            return Err("it says that processing failed".to_owned());
        }
        for (number, output) in outputs.iter_mut().enumerate() {
            output.fill(self.levels[number]);
        }
        Ok(())
    }
}

/// An installed plugin with `inputs` audio inputs and `outputs` audio outputs, which is a synthesizer if it has no input. (Kept for the rest of the run, as they all are.)
fn a_plugin(inputs: usize, outputs: usize) -> &'static InstalledPlugin {
    let descriptor = EffectDescriptor {
        shader: None,
        key: Cow::Borrowed("clap:com.example.recorder"),
        version: (1, 0),
        name: Cow::Borrowed("recorder"),
        description: Cow::Borrowed(""),
        category: Cow::Borrowed("Test"),
        domain: EffectDomain::Sound,
        changes_each_pixel_by_itself: false,
        makes_sound_from_notes: inputs == 0,
        parameters: Cow::Owned(Vec::new()),
        unknown_fields: UnknownFields::none(),
    };
    Box::leak(Box::new(InstalledPlugin {
        descriptor,
        library: "/clap/recorder.clap".into(),
        address: PluginAddress::Clap {
            id: "com.example.recorder".to_owned(),
        },
        audio_inputs: inputs,
        audio_outputs: outputs,
        value_handles: Vec::new(),
        pre_roll_samples: 0,
        has_a_window: false,
        start_warning: None,
        presets: Vec::new(),
    }))
}

fn the_effect_of(plugin: &'static InstalledPlugin) -> LayerEffect {
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind: EffectKind::from_snapshot(plugin.descriptor.clone()).expect("the descriptor is good"),
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        shared_by: None,
    }
}

/// An effect of a plugin with these ports, made of one recorder (two for a plugin of one input and one output), whose output channels are `levels`.
fn an_effect_of(inputs: usize, outputs: usize, levels: &[f32], most: Option<usize>) -> (PluginEffect, Calls, Notes) {
    let plugin = a_plugin(inputs, outputs);
    let calls = Calls::default();
    let layout = Layout::of(inputs, outputs).unwrap();
    let instances = (0..layout.instances())
        .map(|_| {
            Box::new(Recorder {
                levels: levels.to_vec(),
                most,
                calls: Arc::clone(&calls),
                fails: false,
            }) as Box<dyn PluginInstance>
        })
        .collect();
    let notes = Notes::default();
    let effect = PluginEffect::new(
        instances,
        plugin,
        the_effect_of(plugin),
        thirty(),
        0,
        false,
        Arc::clone(&notes),
    )
    .expect("the effect is made");
    (effect, calls, notes)
}

fn on(time: u32, key: u8) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity: 1.0,
            id: u32::from(key),
        },
    }
}

fn the_sound(left: f32, right: f32, count: usize) -> (Vec<f32>, Vec<f32>) {
    (vec![left; count], vec![right; count])
}

#[test]
fn a_synthesizer_of_one_output_is_added_to_both_channels_and_the_sound_that_reaches_it_passes_by() {
    let (mut effect, calls, _) = an_effect_of(0, 1, &[0.5], None);
    let (mut left, mut right) = the_sound(0.25, -0.25, 64);

    effect.process(&mut left, &mut right);

    assert_eq!(left, vec![0.75; 64]);
    assert_eq!(right, vec![0.25; 64]);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "one instance, one run");
    assert_eq!((calls[0].inputs, calls[0].outputs, calls[0].frames), (0, 1, 64));
}

#[test]
fn a_synthesizer_of_two_outputs_adds_each_to_its_channel() {
    let (mut effect, calls, _) = an_effect_of(0, 2, &[0.5, 0.125], None);
    let (mut left, mut right) = the_sound(0.25, -0.25, 64);

    effect.process(&mut left, &mut right);

    assert_eq!(left, vec![0.75; 64]);
    assert_eq!(right, vec![-0.125; 64]);
    assert_eq!(calls.lock().unwrap()[0].outputs, 2);
}

#[test]
fn what_a_synthesizer_makes_is_added_to_silence_and_to_the_sound_of_a_block_of_any_length() {
    let (mut effect, _, _) = an_effect_of(0, 2, &[0.5, 0.5], None);
    for length in [1, 7, 512, 1000] {
        let (mut left, mut right) = the_sound(0.0, 0.0, length);
        effect.process(&mut left, &mut right);
        assert_eq!(left, vec![0.5; length]);
        assert_eq!(right, vec![0.5; length]);
    }
}

#[test]
fn the_notes_of_a_block_are_given_to_a_synthesizer_at_their_samples_and_are_not_kept_for_the_next_block() {
    let (mut effect, calls, _) = an_effect_of(0, 2, &[0.5, 0.5], None);
    let notes = [on(3, 60), on(40, 61)];

    effect.give_notes(&notes);
    let (mut left, mut right) = the_sound(0.0, 0.0, 64);
    effect.process(&mut left, &mut right);
    let (mut left, mut right) = the_sound(0.0, 0.0, 64);
    effect.process(&mut left, &mut right);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].notes, notes);
    assert!(calls[0].with_notes);
    assert_eq!(calls[1].notes, Vec::new(), "the notes were of the first block only");
    assert!(
        calls[1].with_notes,
        "a synthesizer is run with notes, though there are none"
    );
}

#[test]
fn a_block_longer_than_the_plugin_takes_is_given_in_pieces_and_each_piece_has_the_notes_that_are_in_it_counted_from_its_start()
 {
    let (mut effect, calls, _) = an_effect_of(0, 2, &[0.5, 0.5], Some(16));

    effect.give_notes(&[on(3, 60), on(16, 61), on(20, 62), on(39, 63)]);
    let (mut left, mut right) = the_sound(0.0, 0.0, 40);
    effect.process(&mut left, &mut right);

    let calls = calls.lock().unwrap();
    let pieces: Vec<(usize, Vec<(u32, u8)>)> = calls
        .iter()
        .map(|call| {
            (
                call.frames,
                call.notes
                    .iter()
                    .map(|note| match note.what {
                        NoteEventKind::On { key, .. } => (note.time, key),
                        other => panic!("{other:?}"),
                    })
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        pieces,
        vec![(16, vec![(3, 60)]), (16, vec![(0, 61), (4, 62)]), (8, vec![(7, 63)]),]
    );
    assert_eq!(left, vec![0.5; 40]);
}

#[test]
fn a_note_that_is_at_or_after_the_end_of_the_block_is_not_given_to_any_piece() {
    let (mut effect, calls, _) = an_effect_of(0, 2, &[0.5, 0.5], Some(16));

    effect.give_notes(&[on(32, 60), on(100, 61)]);
    let (mut left, mut right) = the_sound(0.0, 0.0, 32);
    effect.process(&mut left, &mut right);

    assert!(calls.lock().unwrap().iter().all(|call| call.notes.is_empty()));
}

#[test]
fn an_effect_that_changes_the_sound_does_not_hear_the_notes_and_is_run_as_it_always_was() {
    let (mut effect, calls, _) = an_effect_of(2, 2, &[0.5, 0.125], None);

    effect.give_notes(&[on(3, 60)]);
    let (mut left, mut right) = the_sound(0.25, -0.25, 16);
    effect.process(&mut left, &mut right);

    assert_eq!(left, vec![0.5; 16], "what it made replaces the sound");
    assert_eq!(right, vec![0.125; 16]);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(!calls[0].with_notes, "it was run, and not run with notes");
    assert_eq!(calls[0].notes, Vec::new());
}

#[test]
fn notes_given_to_an_effect_that_is_not_a_synthesizer_are_not_kept_for_a_later_synthesizer_block() {
    let (mut effect, calls, _) = an_effect_of(1, 1, &[0.5], None);

    effect.give_notes(&[on(0, 60)]);
    let (mut left, mut right) = the_sound(0.25, 0.25, 8);
    effect.process(&mut left, &mut right);

    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| call.notes.is_empty() && !call.with_notes)
    );
}

#[test]
fn what_a_synthesizer_makes_that_is_not_a_number_is_silence_added_and_the_sound_that_passes_by_is_kept() {
    let (mut effect, _, notes) = an_effect_of(0, 2, &[f32::NAN, 5000.0], None);
    let (mut left, mut right) = the_sound(0.25, -0.25, 8);

    effect.process(&mut left, &mut right);

    assert_eq!(left, vec![0.25; 8]);
    assert_eq!(right, vec![-0.25; 8]);
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("not a number"), "{said:?}");
}

#[test]
fn a_synthesizer_that_fails_is_left_out_and_the_sound_goes_by_unchanged_and_its_notes_are_let_go() {
    let plugin = a_plugin(0, 2);
    let calls = Calls::default();
    let notes = Notes::default();
    let mut effect = PluginEffect::new(
        vec![Box::new(Recorder {
            levels: vec![0.5, 0.5],
            most: None,
            calls: Arc::clone(&calls),
            fails: true,
        })],
        plugin,
        the_effect_of(plugin),
        thirty(),
        0,
        false,
        Arc::clone(&notes),
    )
    .unwrap();

    effect.give_notes(&[on(0, 60)]);
    let (mut left, mut right) = the_sound(0.25, -0.25, 8);
    effect.process(&mut left, &mut right);
    effect.give_notes(&[on(0, 61)]);
    let (mut again_left, mut again_right) = the_sound(0.25, -0.25, 8);
    effect.process(&mut again_left, &mut again_right);

    assert!(effect.is_left_out());
    assert_eq!((left, right), the_sound(0.25, -0.25, 8));
    assert_eq!((again_left, again_right), the_sound(0.25, -0.25, 8));
    assert_eq!(calls.lock().unwrap().len(), 1, "it was not run again");
    assert_eq!(notes.lock().unwrap().len(), 1, "it is said once");
}
