// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the notes that a host gives to an instrument (phase 13, piece 13.1), with the two test instruments of `crates/clap-test-plugins`: one takes the notes of CLAP and MIDI, the other MIDI
//! only. What they make is plain arithmetic (see the documentation of [`clap_test_plugins::INSTRUMENT`]): while notes are held every sample is the sum of `velocity * key / 100`.

use clap_test_plugins::{GAIN, INSTRUMENT, INSTRUMENT_MIDI, INSTRUMENT_WITH_A_SIDE_CHAIN};

use crate::scan::{NOTE_DIALECT_CLAP, NOTE_DIALECT_MIDI, scan_library};
use crate::{Instance, Library, MainThread, NoteEvent, NoteEventKind, ParameterChange};

fn made(id: &str) -> Instance {
    let library = Library::open(&clap_test_plugins::library_file_for_tests()).expect("the test library is opened");
    Instance::new(&MainThread::start(), &library, id, 48_000.0, 512).expect("the plugin is made")
}

fn on(time: u32, key: u8, velocity: f32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity,
            id: u32::from(key),
        },
    }
}

fn off(time: u32, key: u8) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel: 0,
            key,
            // (A velocity that is not 0, so that a note off given as a note on is not taken for one by a plugin that follows the rule that a note on of velocity 0 is a note off.)
            velocity: 0.5,
            id: u32::from(key),
        },
    }
}

fn something(time: u32, what: NoteEventKind) -> NoteEvent {
    NoteEvent { time, what }
}

/// Runs an instrument for `frames` samples with `notes` and gives its left channel, after checking that both channels are the same.
fn run_instrument(instance: &mut Instance, frames: usize, notes: &[NoteEvent]) -> Vec<f32> {
    let (mut left, mut right) = (vec![9.0_f32; frames], vec![9.0_f32; frames]);
    instance
        .run_with_notes(&mut [], &mut [&mut left, &mut right], &[], notes)
        .expect("the block is run");
    assert_eq!(left, right, "an instrument makes the same sound in both channels");
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
fn the_scan_says_where_a_plugin_takes_notes_and_in_what_kind_of_event() {
    let scan = scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned");
    let plugin = |id: &str| scan.plugins.iter().find(|plugin| plugin.description.id == id).unwrap();

    let clap = &plugin(INSTRUMENT).note_inputs;
    assert_eq!(clap.len(), 1);
    assert_eq!(clap[0].name, "Notes");
    assert_eq!(clap[0].supported_dialects, NOTE_DIALECT_CLAP | NOTE_DIALECT_MIDI);
    assert_eq!(clap[0].preferred_dialect, NOTE_DIALECT_CLAP);
    assert!(clap[0].takes_clap_notes() && clap[0].takes_midi());

    let midi = &plugin(INSTRUMENT_MIDI).note_inputs;
    assert_eq!(midi.len(), 1);
    assert_eq!(midi[0].supported_dialects, NOTE_DIALECT_MIDI);
    assert_eq!(midi[0].preferred_dialect, NOTE_DIALECT_MIDI);
    assert!(!midi[0].takes_clap_notes() && midi[0].takes_midi());

    assert!(plugin(GAIN).note_inputs.is_empty(), "an effect has no note port");
    assert_eq!(plugin(INSTRUMENT).problem, None);
    assert_eq!(plugin(INSTRUMENT_MIDI).problem, None);
}

#[test]
fn a_scan_that_was_made_before_note_ports_were_read_is_read_as_one_with_none() {
    let scan = scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned");
    let mut as_text: serde_json::Value = serde_json::to_value(&scan).unwrap();
    for plugin in as_text["plugins"].as_array_mut().unwrap() {
        assert!(plugin.as_object_mut().unwrap().remove("note_inputs").is_some());
    }

    let old: crate::scan::LibraryScan = serde_json::from_value(as_text).expect("an older scan is read");

    assert!(old.plugins.iter().all(|plugin| plugin.note_inputs.is_empty()));
}

#[test]
fn notes_in_the_events_of_clap_make_sound_from_their_time_to_their_note_off() {
    let mut instrument = made(INSTRUMENT);

    // (Key 50 at full velocity is 0.5; key 20 at half velocity is 0.1.)
    let heard = run_instrument(
        &mut instrument,
        16,
        &[on(4, 50, 1.0), on(8, 20, 0.5), off(12, 50), off(14, 20)],
    );

    let mut wanted = stretch(0.0, 4);
    wanted.extend(stretch(0.5, 4));
    wanted.extend(stretch(0.6, 4));
    wanted.extend(stretch(0.1, 2));
    wanted.extend(stretch(0.0, 2));
    close(&heard, &wanted);
}

#[test]
fn a_note_goes_on_sounding_in_the_next_block_until_its_note_off() {
    let mut instrument = made(INSTRUMENT);

    let first = run_instrument(&mut instrument, 8, &[on(6, 40, 1.0)]);
    let second = run_instrument(&mut instrument, 8, &[]);
    let third = run_instrument(&mut instrument, 8, &[off(3, 40)]);

    close(&first, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.4, 0.4]);
    close(&second, &stretch(0.4, 8));
    close(&third, &[0.4, 0.4, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_note_on_with_no_velocity_in_the_events_of_clap_is_a_note_that_makes_no_sound_and_is_still_held() {
    let mut instrument = made(INSTRUMENT);

    // (In CLAP a velocity of 0 is a note on and not a note off: the note is held, and silent, and a note off ends it.)
    let heard = run_instrument(&mut instrument, 4, &[on(0, 50, 0.0)]);
    let later = run_instrument(&mut instrument, 4, &[off(0, 50), on(1, 30, 1.0)]);

    close(&heard, &stretch(0.0, 4));
    close(&later, &[0.0, 0.3, 0.3, 0.3]);
}

#[test]
fn notes_are_given_as_midi_to_a_plugin_that_takes_only_midi_with_the_velocity_in_seven_bits() {
    let mut instrument = made(INSTRUMENT_MIDI);

    // (A velocity of 1.0 is 127 of 127; one of 0.5 is 64 of 127, which is 0.5039...)
    let heard = run_instrument(
        &mut instrument,
        12,
        &[on(2, 50, 1.0), on(4, 20, 0.5), off(8, 50), off(10, 20)],
    );

    let half = 64.0_f32 / 127.0;
    let mut wanted = stretch(0.0, 2);
    wanted.extend(stretch(0.5, 2));
    wanted.extend(stretch(0.5 + 0.2 * half, 4));
    wanted.extend(stretch(0.2 * half, 2));
    wanted.extend(stretch(0.0, 2));
    close(&heard, &wanted);
}

#[test]
fn a_note_on_of_no_velocity_is_given_to_midi_as_the_least_there_is_and_is_not_taken_for_a_note_off() {
    let mut instrument = made(INSTRUMENT_MIDI);

    // (In MIDI a note on of velocity 0 is a note off, so the host never gives one: the quietest note on is 1 of 127.)
    let heard = run_instrument(&mut instrument, 4, &[on(0, 50, 0.0)]);

    close(&heard, &stretch(0.5 / 127.0, 4));
}

#[test]
fn the_pitch_wheel_and_a_controller_are_given_to_a_plugin_that_takes_midi_and_the_wheel_has_fourteen_bits() {
    let mut instrument = made(INSTRUMENT_MIDI);

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
            on(0, 40, 1.0),
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
fn an_instrument_whose_port_takes_the_events_of_clap_is_given_those_and_does_not_hear_a_controller() {
    let mut instrument = made(INSTRUMENT);

    // (This instrument does not read controllers or the wheel, and takes notes in CLAP's events: the notes sound, the rest is not heard.)
    let heard = run_instrument(
        &mut instrument,
        4,
        &[
            on(0, 40, 1.0),
            something(
                1,
                NoteEventKind::PitchBend {
                    channel: 0,
                    value: 16383,
                },
            ),
            something(
                1,
                NoteEventKind::Controller {
                    channel: 0,
                    number: 64,
                    value: 127,
                },
            ),
        ],
    );

    close(&heard, &stretch(0.4, 4));
}

#[test]
fn all_off_silences_every_note_at_once_in_either_kind_of_event() {
    for id in [INSTRUMENT, INSTRUMENT_MIDI] {
        let mut instrument = made(id);

        let heard = run_instrument(
            &mut instrument,
            8,
            &[
                on(0, 50, 1.0),
                on(0, 20, 1.0),
                something(
                    0,
                    NoteEventKind::On {
                        channel: 5,
                        key: 10,
                        velocity: 1.0,
                        id: 99,
                    },
                ),
                something(4, NoteEventKind::AllOff),
            ],
        );
        let after = run_instrument(&mut instrument, 4, &[]);

        // (Notes on channel 0 and on channel 5: all of them are silenced.)
        let mut wanted = stretch(0.8, 4);
        wanted.extend(stretch(0.0, 4));
        close(&heard, &wanted);
        close(&after, &stretch(0.0, 4));
    }
}

#[test]
fn a_note_is_ended_by_the_note_off_of_its_own_channel_and_key_and_by_no_other() {
    let mut instrument = made(INSTRUMENT);
    let on_channel = |channel: u8, key: u8| NoteEvent {
        time: 0,
        what: NoteEventKind::On {
            channel,
            key,
            velocity: 1.0,
            id: u32::from(channel) * 128 + u32::from(key),
        },
    };
    let off_channel = |time: u32, channel: u8, key: u8| NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel,
            key,
            velocity: 0.0,
            id: u32::from(channel) * 128 + u32::from(key),
        },
    };

    // (Key 50 on channels 0 and 1: 0.5 each. A note off on channel 2, or for key 51, ends nothing; one for channel 1 ends only that one.)
    let heard = run_instrument(
        &mut instrument,
        8,
        &[
            on_channel(0, 50),
            on_channel(1, 50),
            off_channel(2, 2, 50),
            off_channel(2, 0, 51),
            off_channel(4, 1, 50),
        ],
    );

    let mut wanted = stretch(1.0, 4);
    wanted.extend(stretch(0.5, 4));
    close(&heard, &wanted);
}

#[test]
fn a_plugin_with_no_note_port_is_given_no_notes_and_its_parameters_are_still_set_between_them() {
    let mut gain = made(GAIN);
    let ones = vec![1.0_f32; 16];
    let (mut left, mut right) = (ones.clone(), ones.clone());
    let (mut out_left, mut out_right) = (vec![0.0_f32; 16], vec![0.0_f32; 16]);
    let parameter_id = gain.parameters()[0].id;

    // (The gain is 2 from sample 8; notes before and after it, and at the same sample, are not given to a plugin that has no note port.)
    gain.run_with_notes(
        &mut [&mut left, &mut right],
        &mut [&mut out_left, &mut out_right],
        &[ParameterChange {
            time: 8,
            id: parameter_id,
            value: 2.0,
        }],
        &[on(2, 50, 1.0), on(8, 50, 1.0), off(12, 50)],
    )
    .expect("the block is run");

    let mut wanted = stretch(1.0, 8);
    wanted.extend(stretch(2.0, 8));
    assert_eq!(out_left, wanted);
    assert_eq!(out_right, wanted);
}

#[test]
fn run_without_notes_gives_a_plugin_no_events_so_that_a_note_it_holds_goes_on() {
    let mut instrument = made(INSTRUMENT);
    let first = run_instrument(&mut instrument, 4, &[on(0, 40, 1.0)]);
    let (mut left, mut right) = (vec![9.0_f32; 8], vec![9.0_f32; 8]);

    instrument.run(&mut [], &mut [&mut left, &mut right], &[]).expect("run");

    close(&first, &stretch(0.4, 4));
    close(&left, &stretch(0.4, 8));
}

#[test]
fn notes_and_parameter_changes_are_given_in_order_of_time_and_the_change_comes_first_at_the_same_sample() {
    let mut instrument = made(INSTRUMENT);
    let gain = instrument.parameters()[0].id;
    let (mut left, mut right) = (vec![9.0_f32; 14], vec![9.0_f32; 14]);

    // (The gain is a half from sample 2, and 2 from sample 8; a note on key 50 from sample 6 to sample 10, and another at sample 12 where the gain changes to 1.)
    instrument
        .run_with_notes(
            &mut [],
            &mut [&mut left, &mut right],
            &[
                ParameterChange {
                    time: 2,
                    id: gain,
                    value: 0.5,
                },
                ParameterChange {
                    time: 8,
                    id: gain,
                    value: 2.0,
                },
                ParameterChange {
                    time: 12,
                    id: gain,
                    value: 1.0,
                },
            ],
            &[on(6, 50, 1.0), off(10, 50), on(12, 40, 1.0)],
        )
        .expect("the block is run");

    let mut wanted = stretch(0.0, 6);
    wanted.extend(stretch(0.25, 2));
    wanted.extend(stretch(1.0, 2));
    wanted.extend(stretch(0.0, 2));
    wanted.extend(stretch(0.4, 2));
    close(&left, &wanted);
}

#[test]
fn a_gain_set_before_the_notes_is_heard_in_them() {
    let mut instrument = made(INSTRUMENT_MIDI);
    let gain = instrument.parameters()[0].id;
    let (mut left, mut right) = (vec![9.0_f32; 4], vec![9.0_f32; 4]);

    instrument
        .run_with_notes(
            &mut [],
            &mut [&mut left, &mut right],
            &[ParameterChange {
                time: 0,
                id: gain,
                value: 2.0,
            }],
            &[on(0, 40, 1.0)],
        )
        .expect("the block is run");

    close(&left, &stretch(0.8, 4));
}

#[test]
fn an_instrument_whose_only_audio_input_is_a_side_chain_has_no_main_input_and_is_run_with_the_side_chain_silent() {
    // (This is how Surge XT is: one stereo input that says it is not the main one. The sound of the test instrument has a thousand added to it if the host gives that port anything but two channels.)
    let scan = scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned");
    let plugin = scan
        .plugins
        .iter()
        .find(|plugin| plugin.description.id == INSTRUMENT_WITH_A_SIDE_CHAIN)
        .unwrap();
    assert_eq!(plugin.audio_inputs.len(), 1);
    assert!(!plugin.audio_inputs[0].is_main);
    assert_eq!(plugin.audio_inputs[0].channel_count, 2);
    assert!(
        plugin.main_audio_input().is_none(),
        "the only input is not the main one"
    );
    assert_eq!(plugin.problem, None);

    let mut instrument = made(INSTRUMENT_WITH_A_SIDE_CHAIN);
    assert!(instrument.main_audio_input().is_none());
    let heard = run_instrument(&mut instrument, 16, &[on(4, 50, 1.0), off(12, 50)]);

    let mut wanted = stretch(0.0, 4);
    wanted.extend(stretch(0.5, 8));
    wanted.extend(stretch(0.0, 4));
    close(&heard, &wanted);
}

#[test]
fn the_main_input_of_a_plugin_is_its_first_port_if_that_is_flagged_main_or_if_it_takes_no_notes() {
    let scan = scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned");
    let plugin = |id: &str| scan.plugins.iter().find(|plugin| plugin.description.id == id).unwrap();

    assert!(plugin(GAIN).main_audio_input().is_some_and(|port| port.is_main));
    assert!(
        plugin(INSTRUMENT).main_audio_input().is_none(),
        "no input at all: no main one"
    );
    let mut unflagged = plugin(GAIN).clone();
    unflagged.audio_inputs[0].is_main = false;
    assert!(
        unflagged.main_audio_input().is_some(),
        "an effect that does not say which port is main still has its first port as the main one"
    );
    unflagged.note_inputs = plugin(INSTRUMENT).note_inputs.clone();
    assert!(
        unflagged.main_audio_input().is_none(),
        "but one that takes notes and does not flag its first input as main is an instrument with a side chain"
    );
}

#[test]
fn the_scan_says_what_value_each_parameter_has_in_a_new_plugin_whatever_default_the_plugin_says() {
    let scan = scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned");
    let gain_of = |id: &str| {
        scan.plugins
            .iter()
            .find(|plugin| plugin.description.id == id)
            .unwrap()
            .parameters[0]
            .clone()
    };

    // (A plugin that says what it starts at: the two are the same.)
    let honest = gain_of(INSTRUMENT);
    assert_eq!((honest.default, honest.initial_value), (1.0, Some(1.0)));
    // (The instrument made like Surge XT says 0 and starts at 1.)
    let like_surge = gain_of(INSTRUMENT_WITH_A_SIDE_CHAIN);
    assert_eq!((like_surge.default, like_surge.initial_value), (0.0, Some(1.0)));
}

#[test]
fn a_scan_that_was_made_before_the_starting_value_was_read_is_read_as_one_that_does_not_say() {
    let scan = scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned");
    let mut as_text: serde_json::Value = serde_json::to_value(&scan).unwrap();
    for plugin in as_text["plugins"].as_array_mut().unwrap() {
        for parameter in plugin["parameters"].as_array_mut().unwrap() {
            assert!(parameter.as_object_mut().unwrap().remove("initial_value").is_some());
        }
    }

    let old: crate::scan::LibraryScan = serde_json::from_value(as_text).expect("an older scan is read");

    assert!(
        old.plugins
            .iter()
            .flat_map(|plugin| &plugin.parameters)
            .all(|parameter| parameter.initial_value.is_none())
    );
}
