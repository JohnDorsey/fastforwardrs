// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The events of the music of a clip and the notes of a block that a chain is given, with chains made of processors that record what they are given.

use std::sync::{Arc, Mutex};

use super::*;
use crate::composition::{NoteConsumer, PlannedControl, PlannedNote};
use crate::midi_file::MidiControlKind;

fn note(start: u64, end: u64, key: u8) -> PlannedNote {
    PlannedNote {
        start,
        end,
        channel: 1,
        key,
        velocity: 127,
    }
}

fn music(notes: Vec<PlannedNote>, controls: Vec<PlannedControl>) -> Music {
    Music {
        notes,
        controls,
        consumers: vec![NoteConsumer::Clip],
    }
}

fn summary(events: &[TimedNote]) -> Vec<(u64, String)> {
    events
        .iter()
        .map(|event| {
            (
                event.time,
                match event.what {
                    NoteEventKind::On { key, id, .. } => format!("on {key} #{id}"),
                    NoteEventKind::Off { key, id, .. } => format!("off {key} #{id}"),
                    NoteEventKind::Controller { number, value, .. } => format!("controller {number}={value}"),
                    NoteEventKind::PitchBend { value, .. } => format!("wheel {value}"),
                    NoteEventKind::AllOff => "all off".to_owned(),
                },
            )
        })
        .collect()
}

#[test]
fn the_notes_of_a_clip_are_a_note_on_and_a_note_off_at_the_samples_of_the_sound_with_the_numbers_of_their_notes() {
    let mut next_id = 0;

    let events = events_of(
        &music(vec![note(10, 40, 60), note(20, 30, 64)], Vec::new()),
        1000,
        100,
        &mut next_id,
    );

    assert_eq!(
        summary(&events),
        vec![
            (1010, "on 60 #0".to_owned()),
            (1020, "on 64 #1".to_owned()),
            (1030, "off 64 #1".to_owned()),
            (1040, "off 60 #0".to_owned()),
        ]
    );
    assert_eq!(next_id, 2, "the next note is given the next number");
    let NoteEventKind::On { channel, velocity, .. } = events[0].what else {
        panic!("a note on");
    };
    assert_eq!((channel, velocity), (1, 1.0), "127 of 127 is a velocity of 1");
}

#[test]
fn a_velocity_is_the_number_the_file_has_over_127() {
    let mut next_id = 0;
    let mut soft = note(0, 10, 60);
    soft.velocity = 64;

    let events = events_of(&music(vec![soft], Vec::new()), 0, 100, &mut next_id);

    let NoteEventKind::On { velocity, .. } = events[0].what else {
        panic!("a note on");
    };
    assert_eq!(velocity, 64.0 / 127.0);
}

#[test]
fn a_note_that_begins_at_or_after_the_end_of_the_item_is_left_out_and_one_that_ends_after_it_ends_at_it() {
    let mut next_id = 0;

    let events = events_of(
        &music(
            vec![
                note(10, 500, 60),
                note(99, 100, 62),
                note(100, 200, 64),
                note(300, 400, 67),
            ],
            Vec::new(),
        ),
        0,
        100,
        &mut next_id,
    );

    assert_eq!(
        summary(&events),
        vec![
            (10, "on 60 #0".to_owned()),
            (99, "on 62 #1".to_owned()),
            (100, "off 60 #0".to_owned()),
            (100, "off 62 #1".to_owned()),
        ]
    );
    assert_eq!(next_id, 2, "a note that is left out has no number");
}

#[test]
fn a_note_of_no_length_is_a_sample_long() {
    let mut next_id = 0;

    let events = events_of(&music(vec![note(10, 10, 60)], Vec::new()), 0, 100, &mut next_id);

    assert_eq!(
        summary(&events),
        vec![(10, "on 60 #0".to_owned()), (11, "off 60 #0".to_owned())]
    );
}

#[test]
fn at_one_sample_the_note_offs_come_before_the_controllers_and_those_before_the_note_ons() {
    let mut next_id = 0;
    let controls = vec![
        PlannedControl {
            at: 50,
            channel: 1,
            what: MidiControlKind::Controller { number: 64, value: 0 },
        },
        PlannedControl {
            at: 50,
            channel: 1,
            what: MidiControlKind::PitchBend { value: 8192 },
        },
    ];

    // (A note that ends where another of the same key begins, with the pedal let up and the wheel put back at the same sample.)
    let events = events_of(
        &music(vec![note(50, 80, 60), note(0, 50, 60)], controls),
        0,
        100,
        &mut next_id,
    );

    let at_50: Vec<String> = summary(&events)
        .into_iter()
        .filter(|(time, _)| *time == 50)
        .map(|(_, what)| what)
        .collect();
    assert_eq!(
        at_50,
        vec!["off 60 #1", "controller 64=0", "wheel 8192", "on 60 #0"],
        "the key is let go, the pedal and the wheel are put back, and the key is pressed"
    );
}

#[test]
fn the_controllers_are_given_at_their_samples_and_one_at_the_end_of_the_item_is_given_at_the_end() {
    let mut next_id = 0;
    let controls = vec![
        PlannedControl {
            at: 30,
            channel: 2,
            what: MidiControlKind::Controller { number: 1, value: 40 },
        },
        PlannedControl {
            at: 400,
            channel: 2,
            what: MidiControlKind::PitchBend { value: 100 },
        },
    ];

    let events = events_of(&music(Vec::new(), controls), 1000, 100, &mut next_id);

    assert_eq!(
        summary(&events),
        vec![(1030, "controller 1=40".to_owned()), (1100, "wheel 100".to_owned())]
    );
}

#[test]
fn events_that_are_put_in_order_keep_the_order_they_had_where_they_are_at_one_sample_of_one_kind() {
    let on = |key: u8, id: u32| TimedNote {
        time: 5,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity: 1.0,
            id,
        },
    };
    let mut events = vec![
        on(64, 1),
        TimedNote {
            time: 2,
            what: NoteEventKind::AllOff,
        },
        on(60, 0),
        on(67, 2),
    ];

    put_in_order(&mut events);

    assert_eq!(
        summary(&events),
        vec![
            (2, "all off".to_owned()),
            (5, "on 64 #1".to_owned()),
            (5, "on 60 #0".to_owned()),
            (5, "on 67 #2".to_owned()),
        ]
    );
}

// --- giving a chain the notes of a block ---

/// What a processor was given, one entry for each call.
type Log = Arc<Mutex<Vec<Vec<(u32, String)>>>>;

struct Plays {
    plays: bool,
    passes_on: bool,
    latency: usize,
    log: Log,
}

impl BlockProcessor for Plays {
    fn process(&mut self, _left: &mut [f32], _right: &mut [f32]) {}

    fn reset(&mut self) {}

    fn latency(&self) -> usize {
        self.latency
    }

    fn plays_notes(&self) -> bool {
        self.plays
    }

    fn passes_notes_on(&self) -> bool {
        self.passes_on
    }

    fn give_notes(&mut self, notes: &[NoteEvent]) {
        self.log.lock().unwrap().push(
            notes
                .iter()
                .map(|note| {
                    (
                        note.time,
                        match note.what {
                            NoteEventKind::On { key, .. } => format!("on {key}"),
                            NoteEventKind::Off { key, .. } => format!("off {key}"),
                            _ => "other".to_owned(),
                        },
                    )
                })
                .collect(),
        );
    }
}

fn plays(plays: bool, latency: usize, log: &Log) -> Box<dyn BlockProcessor> {
    Box::new(Plays {
        plays,
        passes_on: false,
        latency,
        log: Arc::clone(log),
    })
}

/// A synthesizer whose switch is on: it plays the notes and passes them on.
fn passes_on(latency: usize, log: &Log) -> Box<dyn BlockProcessor> {
    Box::new(Plays {
        plays: true,
        passes_on: true,
        latency,
        log: Arc::clone(log),
    })
}

fn at(time: u64, key: u8) -> TimedNote {
    TimedNote {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity: 1.0,
            id: 0,
        },
    }
}

fn the_events() -> Vec<TimedNote> {
    vec![at(5, 1), at(10, 2), at(15, 3)]
}

#[test]
fn a_block_is_given_the_events_that_are_in_it_at_their_samples_of_the_block() {
    let log = Log::default();
    let mut chain = vec![plays(true, 0, &log)];

    give_the_notes_of_the_block(&mut chain, &the_events(), 8, 5);
    give_the_notes_of_the_block(&mut chain, &the_events(), 13, 5);
    give_the_notes_of_the_block(&mut chain, &the_events(), 18, 100);

    assert_eq!(
        *log.lock().unwrap(),
        vec![vec![(2, "on 2".to_owned())], vec![(2, "on 3".to_owned())], Vec::new(),],
        "an event at the end of a block is in the next, and the ones before the block are not given again"
    );
}

#[test]
fn the_events_of_blocks_that_follow_one_another_are_every_event_once() {
    let log = Log::default();
    let mut chain = vec![plays(true, 0, &log)];
    let events: Vec<TimedNote> = (0..40).map(|time| at(time, (time % 100) as u8)).collect();

    for block in 0..8 {
        give_the_notes_of_the_block(&mut chain, &events, block * 5, 5);
    }

    let given: Vec<u32> = log.lock().unwrap().iter().flatten().map(|(time, _)| *time).collect();
    assert_eq!(given.len(), 40);
    assert_eq!(given, (0..40).map(|time| time % 5).collect::<Vec<u32>>());
}

#[test]
fn the_notes_of_a_block_are_those_that_the_block_has_the_latency_before_the_synthesizer_earlier() {
    let log = Log::default();
    // (An effect that delays by 3 samples, then the synthesizer: its block that is 8 samples in is the sound's that is 5 in.)
    let mut chain = vec![plays(false, 3, &log), plays(true, 100, &log), plays(false, 7, &log)];

    give_the_notes_of_the_block(&mut chain, &the_events(), 8, 5);

    // (Samples 5 up to 10 of the sound: the event at 5 is at 0 of the block; the one at 10 is not in it. The latency of the synthesizer and of what is after it is not used here: it is how late
    // the sound comes out, which the graph makes up for by asking for the sound that much ahead.)
    assert_eq!(*log.lock().unwrap(), vec![vec![(0, "on 1".to_owned())]]);
}

#[test]
fn a_block_that_begins_before_the_start_of_the_sound_is_given_the_events_at_their_samples_of_it() {
    let log = Log::default();
    let mut chain = vec![plays(false, 6, &log), plays(true, 0, &log)];

    // (The sound's sample 2 is the 8th before it: the block begins at -4 and is 8 long; the event at 0 is at its 4th, and the one at 5 is not in it.)
    give_the_notes_of_the_block(&mut chain, &[at(0, 9), at(5, 1)], 2, 8);

    assert_eq!(*log.lock().unwrap(), vec![vec![(4, "on 9".to_owned())]]);
}

#[test]
fn only_the_first_processor_that_plays_notes_is_given_them_and_a_chain_that_has_none_is_given_nothing() {
    let (first, second, none) = (Log::default(), Log::default(), Log::default());
    let mut chain = vec![plays(false, 0, &none), plays(true, 0, &first), plays(true, 0, &second)];

    give_the_notes_of_the_block(&mut chain, &the_events(), 0, 100);
    let mut without = vec![plays(false, 0, &none)];
    give_the_notes_of_the_block(&mut without, &the_events(), 0, 100);

    assert_eq!(first.lock().unwrap().len(), 1);
    assert_eq!(first.lock().unwrap()[0].len(), 3);
    assert!(
        second.lock().unwrap().is_empty(),
        "the second synthesizer is not given any"
    );
    assert!(none.lock().unwrap().is_empty(), "neither is what does not play notes");
}

#[test]
fn a_synthesizer_is_given_a_block_with_no_events_in_it_and_the_stand_in_for_one_plays_notes() {
    let log = Log::default();
    let mut chain = vec![plays(true, 0, &log)];

    give_the_notes_of_the_block(&mut chain, &the_events(), 100, 10);

    assert_eq!(
        *log.lock().unwrap(),
        vec![Vec::new()],
        "it is told, that there are none"
    );
    assert!(TakesTheNotes::consuming().plays_notes());
}

#[test]
fn a_synthesizer_that_passes_notes_on_gives_them_to_the_next_one_too_each_at_the_time_that_makes_the_latency_before_it_right()
 {
    let (first, second) = (Log::default(), Log::default());
    // (An effect that delays by 3 samples, a synthesizer that passes the notes on and delays by 2, and another: the second has 5 samples of delay before it, the first 3.)
    let mut chain = vec![
        plays(false, 3, &Log::default()),
        passes_on(2, &first),
        plays(true, 7, &second),
    ];

    give_the_notes_of_the_block(&mut chain, &the_events(), 10, 7);

    // (The first has the sound's samples 7 to 14: the event at 10, at 3 of its block. The second has 5 to 12: the events at 5 and 10, at 0 and 5.)
    assert_eq!(*first.lock().unwrap(), vec![vec![(3, "on 2".to_owned())]]);
    assert_eq!(
        *second.lock().unwrap(),
        vec![vec![(0, "on 1".to_owned()), (5, "on 2".to_owned())]]
    );
}

#[test]
fn the_second_synthesizer_is_given_the_notes_by_its_own_place_in_the_chain_and_not_the_first_ones() {
    let (first, second) = (Log::default(), Log::default());
    // (The first synthesizer passes them on and delays by 4 samples; the second is after it.)
    let mut chain = vec![passes_on(4, &first), plays(true, 0, &second)];

    give_the_notes_of_the_block(&mut chain, &the_events(), 9, 5);

    // (The first has the sound from 9 up to 14 - its own latency is not a latency *before* it: 9 to 14, so the event at 10, at 1. The second is 4 samples earlier: 5 up to 10, the event at 5, at 0.)
    assert_eq!(*first.lock().unwrap(), vec![vec![(1, "on 2".to_owned())]]);
    assert_eq!(*second.lock().unwrap(), vec![vec![(0, "on 1".to_owned())]]);
}

#[test]
fn the_notes_stop_at_the_first_synthesizer_that_does_not_pass_them_on_however_many_come_after_it() {
    let (first, second, third) = (Log::default(), Log::default(), Log::default());
    let mut chain = vec![passes_on(0, &first), plays(true, 0, &second), passes_on(0, &third)];

    give_the_notes_of_the_block(&mut chain, &the_events(), 0, 100);

    assert_eq!(first.lock().unwrap().len(), 1);
    assert_eq!(
        second.lock().unwrap().len(),
        1,
        "it is given them, and does not pass them on"
    );
    assert!(third.lock().unwrap().is_empty());
}

#[test]
fn the_stand_in_for_a_synthesizer_passes_the_notes_on_if_it_is_made_to() {
    assert!(!TakesTheNotes::consuming().passes_notes_on());
    assert!(TakesTheNotes::passing_them_on().passes_notes_on());
    assert!(TakesTheNotes::consuming().plays_notes() && TakesTheNotes::passing_them_on().plays_notes());
}
