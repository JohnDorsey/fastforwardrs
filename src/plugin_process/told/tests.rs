// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn on(channel: u8, key: u8, velocity: f32, id: u32) -> NoteEvent {
    NoteEvent {
        time: 40,
        what: NoteEventKind::On {
            channel,
            key,
            velocity,
            id,
        },
    }
}

fn off(channel: u8, key: u8, id: u32) -> NoteEvent {
    NoteEvent {
        time: 50,
        what: NoteEventKind::Off {
            channel,
            key,
            velocity: 0.5,
            id,
        },
    }
}

fn controller(channel: u8, number: u8, value: u8) -> NoteEvent {
    NoteEvent {
        time: 7,
        what: NoteEventKind::Controller { channel, number, value },
    }
}

fn wheel(channel: u8, value: u16) -> NoteEvent {
    NoteEvent {
        time: 9,
        what: NoteEventKind::PitchBend { channel, value },
    }
}

/// The notes that would be played again, as `(channel, key, velocity, id)`.
fn the_notes_played_again(told: &WhatTheProgramWasTold) -> Vec<(u8, u8, f32, u32)> {
    told.events_to_give_again()
        .into_iter()
        .filter_map(|event| match event.what {
            NoteEventKind::On {
                channel,
                key,
                velocity,
                id,
            } => Some((channel, key, velocity, id)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_program_that_was_told_nothing_has_nothing_to_be_told_again() {
    let told = WhatTheProgramWasTold::default();

    assert!(told.values_to_set_again().is_empty());
    assert!(told.events_to_give_again().is_empty());
}

#[test]
fn a_note_is_held_from_its_note_on_to_its_note_off_and_the_one_with_the_id_is_the_one_that_goes() {
    let mut told = WhatTheProgramWasTold::default();

    told.after_notes(&[on(0, 60, 0.5, 1), on(0, 60, 0.75, 2), on(2, 64, 1.0, 3)]);
    assert_eq!(
        the_notes_played_again(&told),
        vec![(0, 60, 0.5, 1), (0, 60, 0.75, 2), (2, 64, 1.0, 3)],
        "three held, in the order they began"
    );
    told.after_notes(&[off(0, 60, 2)]);

    assert_eq!(the_notes_played_again(&told), vec![(0, 60, 0.5, 1), (2, 64, 1.0, 3)]);
}

#[test]
fn a_note_off_with_an_id_nobody_has_lets_go_of_the_first_note_of_its_key_and_channel_and_a_note_that_is_not_held_changes_nothing()
 {
    let mut told = WhatTheProgramWasTold::default();
    told.after_notes(&[on(0, 60, 0.5, 60), on(1, 60, 0.5, 60)]);

    told.after_notes(&[off(0, 60, 999)]);
    assert_eq!(
        the_notes_played_again(&told),
        vec![(1, 60, 0.5, 60)],
        "the one of channel 0 went"
    );
    told.after_notes(&[off(0, 72, 1), off(3, 60, 60)]);

    assert_eq!(the_notes_played_again(&told), vec![(1, 60, 0.5, 60)]);
}

#[test]
fn a_note_on_again_for_a_note_that_is_held_is_one_note_that_began_last_and_not_two() {
    let mut told = WhatTheProgramWasTold::default();
    told.after_notes(&[on(0, 60, 0.5, 1), on(0, 62, 0.5, 2), on(0, 60, 0.9, 1)]);

    assert_eq!(the_notes_played_again(&told), vec![(0, 62, 0.5, 2), (0, 60, 0.9, 1)]);
}

#[test]
fn the_order_of_the_notes_of_a_run_is_what_decides_a_note_that_is_let_go_of_in_the_run_it_began_in_is_not_held() {
    let mut told = WhatTheProgramWasTold::default();

    told.after_notes(&[on(0, 60, 0.5, 1), off(0, 60, 1), off(0, 62, 2), on(0, 62, 0.5, 2)]);

    assert_eq!(
        the_notes_played_again(&told),
        vec![(0, 62, 0.5, 2)],
        "the first was let go of; the second was let go of before it began, which was nothing"
    );
}

#[test]
fn every_note_off_lets_go_of_every_note_and_the_midi_ones_let_go_of_the_notes_of_their_own_channel_alone() {
    let mut told = WhatTheProgramWasTold::default();
    told.after_notes(&[on(0, 60, 0.5, 1), on(1, 61, 0.5, 2)]);
    told.after_notes(&[NoteEvent {
        time: 0,
        what: NoteEventKind::AllOff,
    }]);
    assert!(the_notes_played_again(&told).is_empty());

    for number in [120, 123] {
        told.after_notes(&[on(0, 60, 0.5, 1), on(1, 61, 0.5, 2)]);
        told.after_notes(&[controller(0, number, 0)]);
        assert_eq!(
            the_notes_played_again(&told),
            vec![(1, 61, 0.5, 2)],
            "controller {number}"
        );
        told.after_notes(&[controller(1, number, 127)]);
        assert!(the_notes_played_again(&told).is_empty(), "controller {number}");
    }
    assert!(
        told.events_to_give_again().is_empty(),
        "the controllers that let go of notes are not kept as controllers"
    );
}

#[test]
fn a_controller_and_a_wheel_are_given_again_where_they_were_last_put_before_the_notes() {
    let mut told = WhatTheProgramWasTold::default();

    told.after_notes(&[
        on(0, 60, 0.5, 1),
        controller(0, 64, 127),
        wheel(0, 9000),
        controller(0, 64, 0),
        controller(3, 1, 20),
        wheel(0, 12000),
    ]);

    let again = told.events_to_give_again();
    assert_eq!(
        again.iter().map(|event| event.what).collect::<Vec<_>>(),
        vec![
            NoteEventKind::Controller {
                channel: 0,
                number: 64,
                value: 0
            },
            NoteEventKind::Controller {
                channel: 3,
                number: 1,
                value: 20
            },
            NoteEventKind::PitchBend {
                channel: 0,
                value: 12000
            },
            NoteEventKind::On {
                channel: 0,
                key: 60,
                velocity: 0.5,
                id: 1
            },
        ]
    );
    assert!(
        again.iter().all(|event| event.time == 0),
        "all of them at the first sample of the run"
    );
}

#[test]
fn the_last_value_of_each_parameter_is_set_again_at_the_first_sample_in_the_order_of_the_parameters() {
    let mut told = WhatTheProgramWasTold::default();

    told.after_commands(&[
        Command::SetValue {
            handle: 9,
            value: 0.1,
            at_sample: 300,
        },
        Command::SetValue {
            handle: 2,
            value: 0.5,
            at_sample: 0,
        },
        Command::SetValue {
            handle: 9,
            value: 0.7,
            at_sample: 20,
        },
    ]);

    assert_eq!(
        told.values_to_set_again(),
        vec![
            Command::SetValue {
                handle: 2,
                value: 0.5,
                at_sample: 0
            },
            Command::SetValue {
                handle: 9,
                value: 0.7,
                at_sample: 0
            },
        ]
    );
}

#[test]
fn a_reset_lets_go_of_the_notes_the_controllers_and_the_wheels_and_keeps_the_values() {
    let mut told = WhatTheProgramWasTold::default();
    told.after_commands(&[Command::SetValue {
        handle: 1,
        value: 2.0,
        at_sample: 0,
    }]);
    told.after_notes(&[on(0, 60, 0.5, 1), controller(0, 64, 127), wheel(0, 9000)]);

    told.after_commands(&[Command::Reset]);

    assert!(told.events_to_give_again().is_empty());
    assert_eq!(told.values_to_set_again().len(), 1, "the value is kept");
}

#[test]
fn commands_in_order_a_reset_and_then_a_value_keep_the_value_and_a_value_and_then_a_reset_keep_it_too() {
    let value = Command::SetValue {
        handle: 4,
        value: 1.5,
        at_sample: 3,
    };
    for commands in [[Command::Reset, value], [value, Command::Reset]] {
        let mut told = WhatTheProgramWasTold::default();
        told.after_notes(&[on(0, 60, 0.5, 1)]);

        told.after_commands(&commands);

        assert_eq!(told.values_to_set_again().len(), 1);
        assert!(the_notes_played_again(&told).is_empty());
    }
}

#[test]
fn only_so_many_notes_are_kept_as_held_and_the_oldest_is_the_one_let_go_of() {
    let mut told = WhatTheProgramWasTold::default();
    let notes: Vec<NoteEvent> = (0..(MOST_NOTES_HELD as u32 + 5))
        .map(|id| on(0, (id % 128) as u8, 0.5, id))
        .collect();

    told.after_notes(&notes);

    let held = the_notes_played_again(&told);
    assert_eq!(held.len(), MOST_NOTES_HELD);
    assert_eq!(held[0].3, 5, "the first five were let go of");
    assert_eq!(held.last().unwrap().3, MOST_NOTES_HELD as u32 + 4);
}

#[test]
fn a_copy_of_what_was_told_is_what_was_told_and_what_it_is_told_next_does_not_change_the_one_it_was_made_from() {
    let mut told = WhatTheProgramWasTold::default();
    told.after_notes(&[on(0, 60, 0.5, 1)]);
    let mut later = told.clone();

    later.after_commands(&[Command::Reset]);

    assert_eq!(the_notes_played_again(&told).len(), 1);
    assert!(the_notes_played_again(&later).is_empty());
}
