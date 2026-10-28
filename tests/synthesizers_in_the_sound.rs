// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Synthesizers as effects of the sound (phase 13, piece 13.2), with the real instruments of the CLAP library and of the LV2 bundle that are written for the tests (see the head of
//! `crates/clap-test-plugins/src/lib.rs`: what they make is plain arithmetic): **the notes that are given to the effect are played at their samples, the sound that reaches it passes by
//! and what it makes is added to it**, a note goes on sounding in the next block, a reset lets go of the voices, and a value of the plugin is heard.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use clap_test_plugins::{INSTRUMENT, INSTRUMENT_MIDI, library_file_for_tests};
use fastforwardrs::composition::LayerEffect;
use fastforwardrs::media::export::{LoudnessToExport, SoundClipToExport};
use fastforwardrs::plugin_host::PluginHost;
use fastforwardrs::plugin_scan::{effect_of_a_clap_plugin, effect_of_an_lv2_plugin, key_of_an_lv2_plugin};
use fastforwardrs::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, PropertyValue, SpeedFactor, set_installed_plugins,
};
use fastforwardrs::sound_engine::{BlockProcessor, NoteEvent, NoteEventKind, Notes, PluginRunner};

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn the_clap_kind_of(id: &str) -> EffectKind {
    let path = library_file_for_tests();
    let scan = clap_host::scan::scan_library(&path, |_| {}).expect("the test library is scanned");
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_a_clap_plugin(&path, plugin).ok())
            .collect(),
    );
    let key = format!("clap:{id}");
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {id}"))
}

fn the_lv2_kind_of(folder: &Path, uri: &str) -> EffectKind {
    let bundle = lv2_test_plugins::write_bundle(folder);
    let scan = lv2_host::scan_bundle(&bundle, &mut |_| {}).expect("the bundle is scanned");
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_an_lv2_plugin(&bundle, plugin).ok())
            .collect(),
    );
    let key = key_of_an_lv2_plugin(uri);
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {uri}"))
}

fn an_effect(kind: EffectKind, values: Vec<PropertyValue>) -> LayerEffect {
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind,
        values,
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        shared_by: None,
    }
}

fn the_processor_of(effect: &LayerEffect) -> Box<dyn BlockProcessor> {
    let host = PluginHost::new();
    host.allow_everything();
    let clip = SoundClipToExport {
        source_input: 0,
        first_source_frame: 0,
        end_source_frame: 1000,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: 1000,
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame: 0,
        frames_of_the_clip_before: 0,
        effects: vec![effect.clone()],
        sent_to: None,
        tail_routes: Vec::new(),
        music: None,
    };
    host.processor_for(effect, &clip, thirty(), &Notes::default(), false)
        .expect("the plugin is run")
}

fn on(time: u32, channel: u8, key: u8, velocity: f32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel,
            key,
            velocity,
            id: u32::from(key),
        },
    }
}

fn off(time: u32, channel: u8, key: u8) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel,
            key,
            velocity: 0.5,
            id: u32::from(key),
        },
    }
}

/// Gives the processor `notes` and a block of `frames` samples of the sound `(left, right)` that passes by, and gives what comes out.
fn block(
    processor: &mut dyn BlockProcessor,
    notes: &[NoteEvent],
    frames: usize,
    passing: (f32, f32),
) -> (Vec<f32>, Vec<f32>) {
    let (mut left, mut right) = (vec![passing.0; frames], vec![passing.1; frames]);
    processor.give_notes(notes);
    processor.process(&mut left, &mut right);
    (left, right)
}

fn close(heard: &[f32], wanted: &[f32]) {
    assert_eq!(heard.len(), wanted.len());
    for (moment, (heard, wanted)) in heard.iter().zip(wanted).enumerate() {
        assert!(
            (heard - wanted).abs() < 1e-5,
            "sample {moment}: {heard} and not {wanted}\n{heard:?}"
        );
    }
}

fn stretches(parts: &[(f32, usize)]) -> Vec<f32> {
    parts.iter().flat_map(|&(value, count)| vec![value; count]).collect()
}

/// What the sound that passes by is, and what is added to it.
fn passing_by(passing: f32, added: Vec<f32>) -> Vec<f32> {
    added.into_iter().map(|made| passing + made).collect()
}

#[test]
fn a_clap_instrument_plays_the_notes_it_is_given_at_their_samples_and_the_sound_that_reaches_it_passes_by() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(INSTRUMENT);
    assert!(kind.makes_sound_from_notes());
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(1.0)]));

    // (Key 50 at full velocity is 0.5: from sample 4 to sample 12.)
    let (left, right) = block(&mut *processor, &[on(4, 0, 50, 1.0), off(12, 0, 50)], 16, (0.25, -0.25));

    let made = stretches(&[(0.0, 4), (0.5, 8), (0.0, 4)]);
    close(&left, &passing_by(0.25, made.clone()));
    close(&right, &passing_by(-0.25, made));
}

#[test]
fn a_value_of_a_synthesizer_is_heard() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(INSTRUMENT);
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(2.0)]));

    let (left, _) = block(&mut *processor, &[on(0, 0, 50, 1.0)], 8, (0.0, 0.0));

    close(&left, &stretches(&[(1.0, 8)]));
}

#[test]
fn a_note_goes_on_sounding_in_the_next_block_and_ends_with_its_note_off() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(INSTRUMENT);
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(1.0)]));

    let (first, _) = block(&mut *processor, &[on(6, 0, 40, 1.0)], 8, (0.0, 0.0));
    let (second, _) = block(&mut *processor, &[], 8, (0.0, 0.0));
    let (third, _) = block(&mut *processor, &[off(3, 0, 40)], 8, (0.0, 0.0));

    close(&first, &stretches(&[(0.0, 6), (0.4, 2)]));
    close(&second, &stretches(&[(0.4, 8)]));
    close(&third, &stretches(&[(0.4, 3), (0.0, 5)]));
}

#[test]
fn a_reset_lets_go_of_the_voices_so_that_a_seek_does_not_leave_a_note_sounding() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(INSTRUMENT);
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(1.0)]));
    let (sounding, _) = block(&mut *processor, &[on(0, 0, 40, 1.0)], 4, (0.0, 0.0));

    processor.reset();
    let (after, _) = block(&mut *processor, &[], 4, (0.0, 0.0));

    close(&sounding, &stretches(&[(0.4, 4)]));
    close(&after, &stretches(&[(0.0, 4)]));
}

#[test]
fn all_off_is_given_as_the_event_of_clap_and_silences_the_instrument_that_reads_only_those() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(INSTRUMENT);
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(1.0)]));
    let all_off = NoteEvent {
        time: 4,
        what: NoteEventKind::AllOff,
    };

    // (This instrument is deaf to MIDI's all sound off: only CLAP's choke silences it.)
    let (heard, _) = block(
        &mut *processor,
        &[on(0, 0, 50, 1.0), on(0, 7, 20, 1.0), all_off],
        8,
        (0.0, 0.0),
    );

    close(&heard, &stretches(&[(0.7, 4), (0.0, 4)]));
}

#[test]
fn an_instrument_that_takes_only_midi_is_given_midi_with_the_velocity_in_seven_bits_and_hears_the_wheel_and_the_pedal()
{
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(INSTRUMENT_MIDI);
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(1.0)]));
    let wheel = NoteEvent {
        time: 4,
        what: NoteEventKind::PitchBend {
            channel: 0,
            value: 16383,
        },
    };

    let (heard, _) = block(
        &mut *processor,
        &[on(0, 0, 20, 0.5), wheel, off(8, 0, 20)],
        12,
        (0.0, 0.0),
    );

    let note = 0.2 * (64.0 / 127.0);
    let bent = (16383.0 - 8192.0) / 8192.0 * 0.5;
    close(&heard, &stretches(&[(note, 4), (note + bent, 4), (bent, 4)]));
}

#[test]
fn an_lv2_instrument_plays_the_notes_it_is_given_at_their_samples_and_the_sound_that_reaches_it_passes_by() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let kind = the_lv2_kind_of(folder.path(), lv2_test_plugins::INSTRUMENT);
    assert!(kind.makes_sound_from_notes());
    let mut processor = the_processor_of(&an_effect(kind, Vec::new()));

    let (left, right) = block(
        &mut *processor,
        &[on(4, 0, 50, 1.0), on(8, 0, 20, 0.5), off(12, 0, 50), off(14, 0, 20)],
        16,
        (0.25, -0.25),
    );

    let half = 64.0_f32 / 127.0;
    let made = stretches(&[(0.0, 4), (0.5, 4), (0.5 + 0.2 * half, 4), (0.2 * half, 2), (0.0, 2)]);
    close(&left, &passing_by(0.25, made.clone()));
    close(&right, &passing_by(-0.25, made));
}

#[test]
fn an_lv2_instrument_goes_on_in_the_next_block_and_a_reset_lets_go_of_it() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let kind = the_lv2_kind_of(folder.path(), lv2_test_plugins::INSTRUMENT);
    let mut processor = the_processor_of(&an_effect(kind, Vec::new()));

    let (first, _) = block(&mut *processor, &[on(6, 0, 40, 1.0)], 8, (0.0, 0.0));
    let (second, _) = block(&mut *processor, &[], 8, (0.0, 0.0));
    processor.reset();
    let (after, _) = block(&mut *processor, &[], 8, (0.0, 0.0));

    close(&first, &stretches(&[(0.0, 6), (0.4, 2)]));
    close(&second, &stretches(&[(0.4, 8)]));
    close(&after, &stretches(&[(0.0, 8)]));
}

#[test]
fn notes_given_to_an_effect_that_is_not_a_synthesizer_do_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of("org.fastforwardrs.test.gain");
    assert!(!kind.makes_sound_from_notes());
    let mut processor = the_processor_of(&an_effect(kind, vec![PropertyValue::Number(2.0)]));

    let (left, right) = block(&mut *processor, &[on(2, 0, 50, 1.0)], 8, (0.25, -0.25));

    close(&left, &stretches(&[(0.5, 8)]));
    close(&right, &stretches(&[(-0.5, 8)]));
}
