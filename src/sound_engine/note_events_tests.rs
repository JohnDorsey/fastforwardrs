// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes of the sound engine, and what they are made into for each of the host crates (phase 13, piece 13.2).

use super::{NoteEvent, NoteEventKind};

fn note(time: u32, what: NoteEventKind) -> NoteEvent {
    NoteEvent { time, what }
}

#[test]
fn a_note_that_is_moved_earlier_keeps_what_it_says_and_one_that_would_be_before_the_start_is_gone() {
    let on = note(
        9,
        NoteEventKind::On {
            channel: 3,
            key: 60,
            velocity: 0.5,
            id: 7,
        },
    );

    assert_eq!(on.earlier_by(0), Some(on));
    assert_eq!(on.earlier_by(4), Some(note(5, on.what)));
    assert_eq!(
        on.earlier_by(9),
        Some(note(0, on.what)),
        "a note at the start of the piece is at its first sample"
    );
    assert_eq!(on.earlier_by(10), None, "a note before the piece is not in it");
    assert_eq!(on.earlier_by(1_000_000), None);
}

#[test]
fn every_note_is_made_into_the_note_of_clap_with_everything_it_says() {
    let cases = [
        (
            NoteEventKind::On {
                channel: 3,
                key: 60,
                velocity: 0.5,
                id: 7,
            },
            clap_host::NoteEventKind::On {
                channel: 3,
                key: 60,
                velocity: 0.5,
                id: 7,
            },
        ),
        (
            NoteEventKind::Off {
                channel: 4,
                key: 61,
                velocity: 0.25,
                id: 8,
            },
            clap_host::NoteEventKind::Off {
                channel: 4,
                key: 61,
                velocity: 0.25,
                id: 8,
            },
        ),
        (
            NoteEventKind::Controller {
                channel: 5,
                number: 64,
                value: 127,
            },
            clap_host::NoteEventKind::Controller {
                channel: 5,
                number: 64,
                value: 127,
            },
        ),
        (
            NoteEventKind::PitchBend {
                channel: 6,
                value: 12_345,
            },
            clap_host::NoteEventKind::PitchBend {
                channel: 6,
                value: 12_345,
            },
        ),
        (NoteEventKind::AllOff, clap_host::NoteEventKind::AllOff),
    ];
    for (ours, theirs) in cases {
        let made: clap_host::NoteEvent = note(11, ours).into();
        assert_eq!(made, clap_host::NoteEvent { time: 11, what: theirs }, "{ours:?}");
    }
}

#[test]
fn every_note_is_made_into_the_note_of_lv2_with_everything_it_says_but_the_id_which_midi_does_not_have() {
    let cases = [
        (
            NoteEventKind::On {
                channel: 3,
                key: 60,
                velocity: 0.5,
                id: 7,
            },
            lv2_host::NoteEventKind::On {
                channel: 3,
                key: 60,
                velocity: 0.5,
            },
        ),
        (
            NoteEventKind::Off {
                channel: 4,
                key: 61,
                velocity: 0.25,
                id: 8,
            },
            lv2_host::NoteEventKind::Off {
                channel: 4,
                key: 61,
                velocity: 0.25,
            },
        ),
        (
            NoteEventKind::Controller {
                channel: 5,
                number: 64,
                value: 127,
            },
            lv2_host::NoteEventKind::Controller {
                channel: 5,
                number: 64,
                value: 127,
            },
        ),
        (
            NoteEventKind::PitchBend {
                channel: 6,
                value: 12_345,
            },
            lv2_host::NoteEventKind::PitchBend {
                channel: 6,
                value: 12_345,
            },
        ),
        (NoteEventKind::AllOff, lv2_host::NoteEventKind::AllOff),
    ];
    for (ours, theirs) in cases {
        let made: lv2_host::NoteEvent = note(11, ours).into();
        assert_eq!(made, lv2_host::NoteEvent { time: 11, what: theirs }, "{ours:?}");
    }
}
