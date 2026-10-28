// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the notes that a host gives to an instrument (phase 13, piece 13.1): the bytes of the sequence that is written, and the test instrument of `crates/lv2-test-plugins` that reads it. What
//! the instrument makes is plain arithmetic (see [`lv2_test_plugins::INSTRUMENT`]).

use lv2_test_plugins::{GAIN, INSTRUMENT};

use crate::notes::{midi_bytes_of, sequence_of};
use crate::tests::{Bundle, the_bundle};
use crate::{Instance, NoteEvent, NoteEventKind, RunError};

fn on(time: u32, channel: u8, key: u8, velocity: f32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On { channel, key, velocity },
    }
}

fn off(time: u32, channel: u8, key: u8) -> NoteEvent {
    // (A velocity that is not 0, so that a note off given as a note on is not taken for one.)
    NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel,
            key,
            velocity: 0.5,
        },
    }
}

fn something(time: u32, what: NoteEventKind) -> NoteEvent {
    NoteEvent { time, what }
}

// --- the bytes ---

#[test]
fn the_sequence_that_is_written_has_the_header_and_each_event_with_its_time_its_size_its_type_and_its_body_padded_to_eight_bytes()
 {
    let bytes = sequence_of(&[on(2, 0, 60, 1.0), off(5, 1, 61)], 7, 9, 1000);

    let mut wanted = Vec::new();
    wanted.extend_from_slice(&(8_u32 + 48).to_ne_bytes());
    wanted.extend_from_slice(&7_u32.to_ne_bytes());
    wanted.extend_from_slice(&[0; 8]);
    for (time, body) in [(2_i64, [0x90, 60, 127]), (5, [0x81, 61, 64])] {
        wanted.extend_from_slice(&time.to_ne_bytes());
        wanted.extend_from_slice(&3_u32.to_ne_bytes());
        wanted.extend_from_slice(&9_u32.to_ne_bytes());
        wanted.extend_from_slice(&body);
        wanted.extend_from_slice(&[0; 5]);
    }
    assert_eq!(bytes, wanted);
    assert_eq!(bytes.len() % 8, 0, "the sequence is a whole number of 8-byte words");
}

#[test]
fn no_notes_make_an_empty_sequence() {
    let bytes = sequence_of(&[], 7, 9, 1000);

    let mut wanted = Vec::new();
    wanted.extend_from_slice(&8_u32.to_ne_bytes());
    wanted.extend_from_slice(&7_u32.to_ne_bytes());
    wanted.extend_from_slice(&[0; 8]);
    assert_eq!(bytes, wanted);
}

#[test]
fn the_events_that_do_not_fit_in_the_room_are_left_out_and_the_ones_before_them_are_kept_whole() {
    let notes = [on(0, 0, 60, 1.0), on(1, 0, 61, 1.0), on(2, 0, 62, 1.0)];

    // (16 bytes of header and 24 for each event.)
    assert_eq!(sequence_of(&notes, 7, 9, 16 + 48).len(), 16 + 48);
    assert_eq!(
        sequence_of(&notes, 7, 9, 16 + 48 + 23).len(),
        16 + 48,
        "23 bytes more are not enough for an event"
    );
    assert_eq!(sequence_of(&notes, 7, 9, 16 + 24).len(), 16 + 24);
    let none = sequence_of(&notes, 7, 9, 16 + 23);
    assert_eq!(none.len(), 16);
    assert_eq!(
        u32::from_ne_bytes(none[0..4].try_into().unwrap()),
        8,
        "the size of what is written is what is there"
    );
    let two = sequence_of(&notes, 7, 9, 16 + 48);
    assert_eq!(u32::from_ne_bytes(two[0..4].try_into().unwrap()), 8 + 48);
}

#[test]
fn all_off_is_one_event_for_each_channel() {
    let bytes = sequence_of(&[something(3, NoteEventKind::AllOff)], 7, 9, 10_000);

    assert_eq!(bytes.len(), 16 + 16 * 24);
    for channel in 0..16_usize {
        let event = &bytes[16 + channel * 24..16 + (channel + 1) * 24];
        assert_eq!(i64::from_ne_bytes(event[0..8].try_into().unwrap()), 3);
        assert_eq!(&event[16..19], &[0xB0 | channel as u8, 120, 0]);
    }
}

#[test]
fn midi_bytes_are_masked_clamped_and_never_a_note_on_of_no_velocity() {
    let bytes = |what| midi_bytes_of(what);

    assert_eq!(
        bytes(NoteEventKind::On {
            channel: 17,
            key: 130,
            velocity: 0.0
        }),
        vec![[0x91, 2, 1]]
    );
    assert_eq!(
        bytes(NoteEventKind::On {
            channel: 0,
            key: 60,
            velocity: 2.0
        }),
        vec![[0x90, 60, 127]]
    );
    assert_eq!(
        bytes(NoteEventKind::Off {
            channel: 3,
            key: 60,
            velocity: 0.0
        }),
        vec![[0x83, 60, 0]]
    );
    assert_eq!(
        bytes(NoteEventKind::Off {
            channel: 3,
            key: 60,
            velocity: 1.0
        }),
        vec![[0x83, 60, 127]]
    );
    assert_eq!(
        bytes(NoteEventKind::Controller {
            channel: 2,
            number: 64,
            value: 200
        }),
        vec![[0xB2, 64, 72]]
    );
    assert_eq!(
        bytes(NoteEventKind::PitchBend {
            channel: 0,
            value: 8192
        }),
        vec![[0xE0, 0, 64]]
    );
    assert_eq!(
        bytes(NoteEventKind::PitchBend {
            channel: 0,
            value: 16383
        }),
        vec![[0xE0, 127, 127]]
    );
    assert_eq!(
        bytes(NoteEventKind::PitchBend {
            channel: 0,
            value: 40_000
        }),
        vec![[0xE0, 127, 127]]
    );
    assert_eq!(
        bytes(NoteEventKind::PitchBend { channel: 5, value: 1 }),
        vec![[0xE5, 1, 0]]
    );
    assert_eq!(bytes(NoteEventKind::AllOff).len(), 16);
}

// --- the instrument ---

fn instrument(bundle: &Bundle) -> Instance {
    bundle.make(INSTRUMENT)
}

/// Runs the instrument for `frames` frames with `notes`, and gives its left channel, after checking that both are the same.
fn run_instrument(instance: &mut Instance, frames: usize, notes: &[NoteEvent]) -> Vec<f32> {
    let (mut left, mut right) = (vec![9.0_f32; frames], vec![9.0_f32; frames]);
    instance
        .run_with_notes(&mut [], &mut [&mut left, &mut right], frames, notes)
        .expect("the block is run");
    assert_eq!(left, right);
    left
}

fn close(heard: &[f32], wanted: &[f32]) {
    assert_eq!(heard.len(), wanted.len());
    for (moment, (heard, wanted)) in heard.iter().zip(wanted).enumerate() {
        assert!(
            (heard - wanted).abs() < 1e-6,
            "sample {moment}: {heard} and not {wanted}\n{heard:?}"
        );
    }
}

fn stretch(value: f32, count: usize) -> Vec<f32> {
    vec![value; count]
}

#[test]
fn an_instrument_says_that_it_takes_midi_and_an_effect_does_not() {
    let bundle = the_bundle();

    assert!(instrument(&bundle).takes_midi());
    assert!(!bundle.make(GAIN).takes_midi());
    assert_eq!(instrument(&bundle).audio_port_counts(), (0, 2));
}

#[test]
fn notes_make_sound_from_their_frame_to_their_note_off_and_the_sound_is_exact() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);
    let seen = bundle.port(INSTRUMENT, "events_seen");

    // (Key 50 at full velocity is 0.5; key 20 at 64 of 127 is 0.2 * 64 / 127.)
    let heard = run_instrument(
        &mut instrument,
        16,
        &[on(4, 0, 50, 1.0), on(8, 0, 20, 0.5), off(12, 0, 50), off(14, 0, 20)],
    );

    let half = 64.0_f32 / 127.0;
    let mut wanted = stretch(0.0, 4);
    wanted.extend(stretch(0.5, 4));
    wanted.extend(stretch(0.5 + 0.2 * half, 4));
    wanted.extend(stretch(0.2 * half, 2));
    wanted.extend(stretch(0.0, 2));
    close(&heard, &wanted);
    assert_eq!(instrument.control(seen), Some(4.0));
}

#[test]
fn a_note_goes_on_sounding_in_the_next_block_which_is_given_an_empty_sequence() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);
    let seen = bundle.port(INSTRUMENT, "events_seen");

    let first = run_instrument(&mut instrument, 8, &[on(6, 0, 40, 1.0)]);
    let second = run_instrument(&mut instrument, 8, &[]);
    let seen_in_the_second = instrument.control(seen);
    let third = run_instrument(&mut instrument, 8, &[off(3, 0, 40)]);

    close(&first, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.4, 0.4]);
    close(&second, &stretch(0.4, 8));
    assert_eq!(
        seen_in_the_second,
        Some(0.0),
        "the notes of the block before are not there again"
    );
    close(&third, &[0.4, 0.4, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_note_on_with_no_velocity_is_the_quietest_note_and_not_a_note_off() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);

    let heard = run_instrument(&mut instrument, 4, &[on(0, 0, 50, 0.0)]);

    close(&heard, &stretch(0.5 / 127.0, 4));
}

#[test]
fn a_note_off_ends_the_note_of_its_own_channel_and_key_and_no_other() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);

    let heard = run_instrument(
        &mut instrument,
        8,
        &[
            on(0, 0, 50, 1.0),
            on(0, 1, 50, 1.0),
            off(2, 2, 50),
            off(2, 0, 51),
            off(4, 1, 50),
        ],
    );

    let mut wanted = stretch(1.0, 4);
    wanted.extend(stretch(0.5, 4));
    close(&heard, &wanted);
}

#[test]
fn the_pitch_wheel_and_the_pedal_are_heard_and_the_wheel_has_fourteen_bits() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);

    let bent = run_instrument(
        &mut instrument,
        8,
        &[something(
            2,
            NoteEventKind::PitchBend {
                channel: 0,
                value: 16383,
            },
        )],
    );
    let centred = run_instrument(
        &mut instrument,
        4,
        &[something(
            0,
            NoteEventKind::PitchBend {
                channel: 0,
                value: 8192,
            },
        )],
    );
    let pedal = run_instrument(
        &mut instrument,
        4,
        &[
            on(0, 0, 40, 1.0),
            something(
                2,
                NoteEventKind::Controller {
                    channel: 0,
                    number: 64,
                    value: 127,
                },
            ),
        ],
    );

    let top = (16383.0_f32 - 8192.0) / 8192.0 * 0.5;
    close(&bent, &[0.0, 0.0, top, top, top, top, top, top]);
    close(&centred, &stretch(0.0, 4));
    close(&pedal, &[0.4, 0.4, 0.65, 0.65]);
}

#[test]
fn all_off_silences_the_notes_of_every_channel() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);

    let heard = run_instrument(
        &mut instrument,
        8,
        &[
            on(0, 0, 50, 1.0),
            on(0, 5, 20, 1.0),
            on(0, 15, 10, 1.0),
            something(4, NoteEventKind::AllOff),
        ],
    );
    let after = run_instrument(&mut instrument, 4, &[]);

    let mut wanted = stretch(0.8, 4);
    wanted.extend(stretch(0.0, 4));
    close(&heard, &wanted);
    close(&after, &stretch(0.0, 4));
}

#[test]
fn a_note_that_is_at_or_after_the_end_of_the_block_is_refused_and_nothing_is_run() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);
    let (mut left, mut right) = (vec![9.0_f32; 8], vec![9.0_f32; 8]);

    let result = instrument.run_with_notes(&mut [], &mut [&mut left, &mut right], 8, &[on(8, 0, 50, 1.0)]);

    assert_eq!(result, Err(RunError::NoteOutsideTheBlock { time: 8, frames: 8 }));
    assert_eq!(left, stretch(9.0, 8), "the block was not run");
    let last = run_instrument(&mut instrument, 8, &[on(7, 0, 50, 1.0)]);
    close(&last, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5]);
}

#[test]
fn more_notes_than_the_port_has_room_for_are_left_out_and_the_ones_before_them_are_heard() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);
    let seen = bundle.port(INSTRUMENT, "events_seen");
    // (The buffer of an atom port has 64 KiB: the header, and 24 bytes for each event.)
    let room = (64 * 1024 - 16) / 24;
    let notes: Vec<NoteEvent> = (0..room + 100)
        .map(|number| on(0, (number % 16) as u8, 1, 0.5))
        .collect();

    let heard = run_instrument(&mut instrument, 4, &notes);

    assert_eq!(instrument.control(seen), Some(room as f32));
    assert!(
        (heard[0] - room as f32 * (64.0 / 127.0) * 0.01).abs() < 0.01,
        "{}",
        heard[0]
    );
}

#[test]
fn a_plugin_with_no_midi_input_is_given_no_notes_and_runs_as_it_did() {
    let bundle = the_bundle();
    let mut gain = bundle.make(GAIN);
    let mut input = vec![1.0_f32; 8];
    let mut output = vec![0.0_f32; 8];

    gain.run_with_notes(
        &mut [&mut input],
        &mut [&mut output],
        8,
        &[on(2, 0, 50, 1.0), off(4, 0, 50)],
    )
    .expect("the block is run");

    assert_eq!(output, stretch(1.0, 8));
}

#[test]
fn run_without_notes_gives_no_events_so_that_a_held_note_goes_on() {
    let bundle = the_bundle();
    let mut instrument = instrument(&bundle);
    let first = run_instrument(&mut instrument, 4, &[on(0, 0, 40, 1.0)]);
    let (mut left, mut right) = (vec![9.0_f32; 8], vec![9.0_f32; 8]);

    instrument.run(&mut [], &mut [&mut left, &mut right], 8).expect("run");

    close(&first, &stretch(0.4, 4));
    close(&left, &stretch(0.4, 8));
}
