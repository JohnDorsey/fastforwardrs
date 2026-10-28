// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the notes of a MIDI clip are in a stretch of it, in samples: exact numbers at 30 frames a second (1600 samples a frame, 48000 a second) and at 29.97.

use super::*;
use crate::midi_file::MidiNote;

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn note(start_us: u64, end_us: u64) -> MidiNote {
    MidiNote {
        start_us,
        end_us,
        channel: 0,
        key: 60,
        velocity: 100,
    }
}

fn track_of(notes: Vec<MidiNote>, controls: Vec<crate::midi_file::MidiControl>) -> MidiTrack {
    MidiTrack {
        name: String::new(),
        notes,
        controls,
        end_us: 0,
    }
}

fn controller(time_us: u64, channel: u8, number: u8, value: u8) -> crate::midi_file::MidiControl {
    crate::midi_file::MidiControl {
        time_us,
        channel,
        what: MidiControlKind::Controller { number, value },
    }
}

fn wheel(time_us: u64, channel: u8, value: u16) -> crate::midi_file::MidiControl {
    crate::midi_file::MidiControl {
        time_us,
        channel,
        what: MidiControlKind::PitchBend { value },
    }
}

/// A stretch of the clip that begins where the clip does, from source frame `first` to `end`, at 30 frames a second.
fn whole(first: u64, end: u64, speed: u32) -> StretchOfMusic {
    StretchOfMusic {
        frame_rate: thirty(),
        clip_first_source_frame: first,
        first_source_frame: first,
        end_source_frame: end,
        speed,
    }
}

fn the_notes(notes: Vec<MidiNote>, stretch: StretchOfMusic) -> Vec<(u64, u64)> {
    music_of_a_stretch(&track_of(notes, Vec::new()), stretch)
        .0
        .iter()
        .map(|note| (note.start, note.end))
        .collect()
}

#[test]
fn a_note_is_where_its_time_says_in_samples_from_the_start_of_the_stretch() {
    let notes = vec![note(0, 500_000), note(1_000_000, 1_500_000)];

    assert_eq!(the_notes(notes, whole(0, 90, 1)), vec![(0, 24_000), (48_000, 72_000)]);
}

#[test]
fn at_twice_the_speed_the_notes_are_twice_as_near_each_other_and_as_short() {
    let notes = vec![note(0, 500_000), note(1_000_000, 1_500_000)];

    assert_eq!(
        the_notes(notes.clone(), whole(0, 90, 2)),
        vec![(0, 12_000), (24_000, 36_000)]
    );
    assert_eq!(the_notes(notes, whole(0, 90, 3)), vec![(0, 8_000), (16_000, 24_000)]);
}

#[test]
fn a_stretch_that_begins_later_counts_from_where_it_begins_and_a_note_that_begins_with_it_begins_at_sample_0() {
    let notes = vec![note(1_000_000, 1_500_000), note(1_250_000, 1_750_000)];

    // (From the first second, which is source frame 30.)
    let stretch = StretchOfMusic {
        first_source_frame: 30,
        ..whole(0, 90, 1)
    };

    assert_eq!(the_notes(notes, stretch), vec![(0, 24_000), (12_000, 36_000)]);
}

#[test]
fn a_note_that_began_in_the_clip_before_the_stretch_and_is_still_sounding_is_begun_where_the_stretch_begins_as_hard_as_it_was()
 {
    let mut hard = note(500_000, 1_500_000);
    hard.velocity = 117;
    hard.channel = 3;
    hard.key = 72;
    let notes = vec![hard, note(500_000, 800_000), note(500_000, 1_000_000)];
    let stretch = StretchOfMusic {
        first_source_frame: 30,
        ..whole(0, 90, 1)
    };

    let (planned, _) = music_of_a_stretch(&track_of(notes, Vec::new()), stretch);

    assert_eq!(
        planned,
        vec![PlannedNote {
            start: 0,
            end: 24_000,
            channel: 3,
            key: 72,
            velocity: 117
        }],
        "the one that ended before the stretch, and the one that ends just where it begins, are not played"
    );
}

#[test]
fn a_note_that_began_before_the_clip_is_not_played_though_it_is_sounding_where_the_clip_begins() {
    // (The clip begins at source frame 15, half a second in; the stretch is the whole of the clip.)
    let notes = vec![note(0, 2_000_000), note(499_999, 900_000), note(500_000, 900_000)];

    let planned = the_notes(notes, whole(15, 90, 1));

    assert_eq!(
        planned,
        vec![(0, 19_200)],
        "only the one that begins at the clip's first frame"
    );
}

#[test]
fn a_note_that_goes_on_past_the_stretch_has_its_note_off_at_the_end_of_it_and_one_that_begins_after_it_is_not_in_it() {
    let notes = vec![
        note(500_000, 2_000_000),
        note(1_000_000, 1_200_000),
        note(999_999, 1_500_000),
    ];

    let planned = the_notes(notes, whole(0, 30, 1));

    assert_eq!(
        planned,
        vec![(24_000, 48_000), (47_999, 48_000)],
        "a second is 48000 samples; the note that begins at 1.0 s is in the next stretch, and the one that begins a microsecond before is the last sample of this"
    );
}

#[test]
fn a_note_of_no_length_is_a_sample_long_and_times_are_rounded_down() {
    let notes = vec![note(1, 20_833), note(500_000, 500_000), note(30_000_000, 30_000_000)];

    let planned = the_notes(notes, whole(0, 1000, 1));

    // (One microsecond is 0.048 of a sample; 20 833 are 999.98.)
    assert_eq!(planned, vec![(0, 999), (24_000, 24_001), (1_440_000, 1_440_001)]);
}

#[test]
fn a_note_that_begins_in_the_last_sample_of_a_stretch_is_one_sample_long_there() {
    // (The stretch is a frame, which is 1600 samples; a note that begins at the frame's last microsecond rounds down to its last sample.)
    let notes = vec![note(33_333, 33_334)];

    let planned = the_notes(notes, whole(0, 1, 1));

    assert_eq!(planned, vec![(1599, 1600)]);
}

#[test]
fn the_end_of_a_stretch_that_is_cut_by_speed_is_where_the_last_whole_sample_of_its_music_is() {
    // (Ten frames at three times the speed: 10 / 30 / 3 of a second is 5333.33 samples; a note that goes past the end ends at the 5333rd.)
    let notes = vec![note(0, 10_000_000)];

    assert_eq!(the_notes(notes, whole(0, 10, 3)), vec![(0, 5_333)]);
}

#[test]
fn at_29_97_frames_a_second_a_frame_is_longer_than_at_30_and_the_notes_are_where_the_time_says() {
    let ntsc = FrameRate::new(30_000, 1001).unwrap();
    // (Source frame 30 is 1.001 seconds in. A note at 1 001 000 microseconds is at its first sample; one at 1 501 000 is half a second after it.)
    let stretch = StretchOfMusic {
        frame_rate: ntsc,
        clip_first_source_frame: 0,
        first_source_frame: 30,
        end_source_frame: 90,
        speed: 1,
    };
    let notes = vec![note(1_001_000, 1_251_000), note(1_501_000, 2_001_000)];

    assert_eq!(the_notes(notes, stretch), vec![(0, 12_000), (24_000, 48_000)]);
}

#[test]
fn what_is_outside_the_stretch_is_not_in_it() {
    let notes = vec![note(0, 500_000), note(5_000_000, 5_500_000)];

    assert_eq!(the_notes(notes, whole(30, 60, 1)), Vec::new());
}

#[test]
fn notes_keep_their_order_their_channels_and_their_keys() {
    let mut first = note(0, 100_000);
    first.key = 40;
    let mut second = note(0, 200_000);
    second.key = 64;
    second.channel = 9;

    let (planned, _) = music_of_a_stretch(&track_of(vec![first, second], Vec::new()), whole(0, 30, 1));

    assert_eq!(
        planned.iter().map(|note| (note.channel, note.key)).collect::<Vec<_>>(),
        vec![(0, 40), (9, 64)]
    );
}

// --- the pedal, the wheel and the other controllers ---

#[test]
fn controllers_are_where_their_times_say_and_a_pedal_that_is_down_and_a_wheel_that_is_bent_are_put_back_at_the_end() {
    let controls = vec![
        controller(500_000, 0, 64, 127),
        wheel(700_000, 1, 12_000),
        controller(800_000, 2, 1, 33),
    ];

    let (_, planned) = music_of_a_stretch(&track_of(Vec::new(), controls), whole(0, 90, 1));

    // (Ninety frames of 1600 samples are 144 000: the end of the stretch.)
    assert_eq!(
        planned,
        vec![
            PlannedControl {
                at: 24_000,
                channel: 0,
                what: MidiControlKind::Controller { number: 64, value: 127 }
            },
            PlannedControl {
                at: 33_600,
                channel: 1,
                what: MidiControlKind::PitchBend { value: 12_000 }
            },
            PlannedControl {
                at: 38_400,
                channel: 2,
                what: MidiControlKind::Controller { number: 1, value: 33 }
            },
            PlannedControl {
                at: 144_000,
                channel: 0,
                what: MidiControlKind::Controller { number: 64, value: 0 }
            },
            PlannedControl {
                at: 144_000,
                channel: 1,
                what: MidiControlKind::PitchBend { value: 8_192 }
            },
        ],
        "the modulation wheel is left where it was put"
    );
}

#[test]
fn a_pedal_that_was_let_up_and_a_wheel_that_was_put_back_are_not_put_back_again() {
    let controls = vec![
        controller(100_000, 0, 64, 127),
        controller(200_000, 0, 64, 0),
        wheel(300_000, 0, 9_000),
        wheel(400_000, 0, 8_192),
        controller(500_000, 0, 64, 63),
    ];

    let (_, planned) = music_of_a_stretch(&track_of(Vec::new(), controls), whole(0, 90, 1));

    assert_eq!(planned.len(), 5, "no more than the five that the file has: {planned:?}");
    assert!(planned.iter().all(|control| control.at < 144_000));
}

#[test]
fn the_last_of_each_controller_before_the_stretch_in_the_clip_is_given_where_the_stretch_begins_and_those_before_the_clip_are_not()
 {
    let controls = vec![
        controller(100_000, 0, 1, 10),
        // (A controller that has no later value: if the clip's start were not looked at, it would be given.)
        controller(150_000, 0, 11, 5),
        controller(400_000, 0, 1, 50),
        wheel(500_000, 0, 12_000),
        controller(600_000, 1, 64, 127),
        // (At the very start of the stretch: it is where the stretch begins, as an event of the stretch.)
        controller(1_000_000, 0, 7, 99),
        controller(1_250_000, 0, 7, 20),
    ];
    // (The clip begins at source frame 6, 0.2 seconds in: the first is before it.)
    let stretch = StretchOfMusic {
        clip_first_source_frame: 6,
        first_source_frame: 30,
        ..whole(0, 90, 1)
    };

    let (_, planned) = music_of_a_stretch(&track_of(Vec::new(), controls), stretch);

    let at = |sample: u64| -> Vec<_> {
        planned
            .iter()
            .filter(|control| control.at == sample)
            .map(|control| (control.channel, control.what))
            .collect()
    };
    assert_eq!(
        at(0),
        vec![
            (0, MidiControlKind::Controller { number: 1, value: 50 }),
            (0, MidiControlKind::PitchBend { value: 12_000 }),
            (1, MidiControlKind::Controller { number: 64, value: 127 }),
            (0, MidiControlKind::Controller { number: 7, value: 99 }),
        ],
        "the last value of the modulation wheel, the wheel, the pedal, and then what the stretch itself says at its start"
    );
    assert_eq!(
        at(12_000),
        vec![(0, MidiControlKind::Controller { number: 7, value: 20 })]
    );
    // (The pedal that was down before the stretch and was never let up is let up at its end; so is the wheel that is bent.)
    assert!(planned.contains(&PlannedControl {
        at: 96_000,
        channel: 1,
        what: MidiControlKind::Controller { number: 64, value: 0 }
    }));
    assert!(planned.contains(&PlannedControl {
        at: 96_000,
        channel: 0,
        what: MidiControlKind::PitchBend { value: 8_192 }
    }));
    assert!(
        !planned.iter().any(|control| matches!(
            control.what,
            MidiControlKind::Controller {
                number: 1 | 11,
                value: 10 | 5
            }
        )),
        "a value before the clip is not given"
    );
}

#[test]
fn controllers_are_in_order_of_their_times() {
    // (Given out of order: what is planned is in order whatever order the track has them in.)
    let controls = vec![
        controller(300_000, 0, 1, 3),
        controller(100_000, 0, 1, 1),
        controller(200_000, 0, 1, 2),
    ];

    let (_, planned) = music_of_a_stretch(&track_of(Vec::new(), controls), whole(0, 90, 1));

    assert_eq!(
        planned.iter().map(|control| control.at).collect::<Vec<_>>(),
        vec![4_800, 9_600, 14_400]
    );
}

#[test]
fn a_controller_at_the_very_end_of_the_stretch_is_not_in_it() {
    // (3 seconds is the end of ninety frames.)
    let controls = vec![controller(2_999_999, 0, 1, 1), controller(3_000_000, 0, 1, 2)];

    let (_, planned) = music_of_a_stretch(&track_of(Vec::new(), controls), whole(0, 90, 1));

    assert_eq!(planned.len(), 1, "{planned:?}");
    assert_eq!(planned[0].what, MidiControlKind::Controller { number: 1, value: 1 });
}

#[test]
fn the_pedal_and_the_wheel_of_one_channel_are_both_kept_from_before_the_stretch() {
    let controls = vec![controller(100_000, 0, 64, 127), wheel(200_000, 0, 12_000)];
    let stretch = StretchOfMusic {
        first_source_frame: 30,
        ..whole(0, 90, 1)
    };

    let (_, planned) = music_of_a_stretch(&track_of(Vec::new(), controls), stretch);

    let at_the_start: Vec<_> = planned
        .iter()
        .filter(|control| control.at == 0)
        .map(|control| control.what)
        .collect();
    assert_eq!(
        at_the_start,
        vec![
            MidiControlKind::Controller { number: 64, value: 127 },
            MidiControlKind::PitchBend { value: 12_000 }
        ],
        "the wheel does not take the place of the pedal"
    );
}

#[test]
fn a_pedal_is_down_from_the_value_64_and_a_pedal_at_63_is_not() {
    for (value, put_back) in [(63, false), (64, true), (65, true)] {
        let (_, planned) = music_of_a_stretch(
            &track_of(Vec::new(), vec![controller(100_000, 0, 64, value)]),
            whole(0, 90, 1),
        );

        assert_eq!(
            planned.len(),
            if put_back { 2 } else { 1 },
            "the value {value}: {planned:?}"
        );
    }
}
