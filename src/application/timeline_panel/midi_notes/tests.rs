// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which bars the notes of a MIDI clip are drawn as: where each is along the clip and how long, how high for its key, which are left out, and how the many that are in one place become one.

use egui::{Rect, pos2};

use super::*;

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn note(start_ms: u64, end_ms: u64, key: u8) -> MidiNote {
    MidiNote {
        start_us: start_ms * 1000,
        end_us: end_ms * 1000,
        channel: 0,
        key,
        velocity: 100,
    }
}

/// A clip of 3 seconds (90 frames, from the start of its file), 300 points wide from x = 100, and 60 points tall from y = 10.
fn the_clip() -> Rect {
    Rect::from_min_max(pos2(100.0, 10.0), pos2(400.0, 70.0))
}

/// The lowest and the highest key of `notes`, as a track says them.
fn range_of(notes: &[MidiNote]) -> (u8, u8) {
    (
        notes.iter().map(|note| note.key).min().unwrap_or(60),
        notes.iter().map(|note| note.key).max().unwrap_or(60),
    )
}

fn bars(notes: &[MidiNote]) -> Vec<Rect> {
    note_bars(notes, range_of(notes), thirty(), 0, 90, the_clip(), the_clip())
}

#[test]
fn a_note_is_as_far_along_the_clip_and_as_long_as_it_is_in_time() {
    // (100 points a second: from 0.5 s to 1.25 s is from 150 to 225.)
    let drawn = bars(&[note(500, 1250, 60)]);

    assert_eq!(drawn.len(), 1);
    assert!((drawn[0].left() - 150.0).abs() < 1e-3, "{:?}", drawn[0]);
    assert!((drawn[0].right() - 225.0).abs() < 1e-3, "{:?}", drawn[0]);
}

#[test]
fn a_note_that_goes_on_past_the_end_of_the_clip_is_cut_at_it_and_one_that_begins_at_or_after_it_is_left_out() {
    let drawn = bars(&[note(2500, 9000, 60), note(3000, 3500, 62), note(4000, 5000, 64)]);

    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].right(), 400.0);
}

#[test]
fn a_clip_that_begins_later_in_its_file_has_its_notes_counted_from_there_and_a_note_that_began_before_it_is_not_drawn()
{
    // (The clip plays frames 30 to 90 of the file, which is 2 seconds in 300 points: 150 points a second.)
    let notes = [note(500, 1500, 60), note(1000, 1400, 62), note(1500, 2000, 64)];

    let drawn = note_bars(&notes, range_of(&notes), thirty(), 30, 90, the_clip(), the_clip());

    // (The first began at 0.5 s, before the clip, and is not played: not drawn. The second begins where the clip does; the third 0.5 s after: 75 points.)
    assert_eq!(drawn.len(), 2);
    assert!((drawn[0].left() - 100.0).abs() < 1e-3, "{:?}", drawn[0]);
    assert!((drawn[1].left() - 175.0).abs() < 1e-3, "{:?}", drawn[1]);
    assert!((drawn[1].right() - 250.0).abs() < 1e-3);
}

#[test]
fn a_higher_key_is_higher_on_the_clip_and_all_of_them_are_inside_it() {
    let notes = [note(0, 500, 40), note(500, 1000, 64), note(1000, 1500, 88)];

    let drawn = bars(&notes);

    assert_eq!(drawn.len(), 3);
    assert!(
        drawn[0].top() > drawn[1].top() && drawn[1].top() > drawn[2].top(),
        "{drawn:?}"
    );
    for bar in &drawn {
        assert!(
            bar.top() >= the_clip().top() && bar.bottom() <= the_clip().bottom(),
            "{bar:?}"
        );
    }
    // (The lowest key is at the bottom of what is shown and the highest at the top: the track's own range fills the clip, which is 52 points between its margins.)
    assert!(
        (drawn[2].top() - (the_clip().top() + 4.0)).abs() < 1e-3,
        "{:?}",
        drawn[2]
    );
    assert!(
        (drawn[0].bottom() - (the_clip().bottom() - 4.0)).abs() < 1e-3,
        "{:?}",
        drawn[0]
    );
}

#[test]
fn a_key_outside_the_range_that_is_given_is_drawn_at_the_edge_of_it() {
    let notes = [
        note(0, 500, 0),
        note(500, 1000, 127),
        note(1000, 1500, 40),
        note(1500, 2000, 80),
    ];

    let drawn = note_bars(&notes, (40, 80), thirty(), 0, 90, the_clip(), the_clip());

    assert_eq!(drawn[0].top(), drawn[2].top(), "below it: as the lowest");
    assert_eq!(drawn[1].top(), drawn[3].top(), "above it: as the highest");
}

#[test]
fn a_track_of_one_key_or_of_a_few_is_not_as_tall_as_the_clip_and_the_bars_are_not_taller_than_they_need_to_be() {
    let one_key = bars(&[note(0, 500, 60)]);
    let two_keys = bars(&[note(0, 500, 60), note(500, 1000, 61)]);

    assert!(
        one_key[0].height() <= 8.0 && one_key[0].height() >= 1.5,
        "{:?}",
        one_key[0]
    );
    assert!(
        one_key[0].center().y > the_clip().top() + 8.0 && one_key[0].center().y < the_clip().bottom() - 8.0,
        "in the middle, not at an edge: {:?}",
        one_key[0]
    );
    assert!(two_keys[0].top() > two_keys[1].top());
    assert!(
        two_keys[0].top() - two_keys[1].top() < 10.0,
        "next to each other: {two_keys:?}"
    );
}

#[test]
fn a_note_that_is_shorter_than_a_point_is_still_a_bar() {
    let drawn = bars(&[note(500, 501, 60)]);

    assert!(drawn[0].width() >= 1.5, "{:?}", drawn[0]);
    assert!(drawn[0].height() >= 1.5);
}

#[test]
fn only_the_notes_in_the_part_of_the_clip_that_is_in_view_are_drawn() {
    let notes = [note(100, 400, 60), note(1000, 1500, 62), note(2500, 2900, 64)];
    // (Only from x = 190 to x = 300: 0.9 s to 2 s.)
    let in_view = Rect::from_min_max(pos2(190.0, 10.0), pos2(300.0, 70.0));

    let drawn = note_bars(&notes, range_of(&notes), thirty(), 0, 90, the_clip(), in_view);

    assert_eq!(drawn.len(), 1);
    assert!((drawn[0].left() - 200.0).abs() < 1e-3, "{:?}", drawn[0]);
}

#[test]
fn a_note_that_begins_before_the_view_and_goes_into_it_is_drawn() {
    let notes = [note(100, 2500, 60)];
    let in_view = Rect::from_min_max(pos2(250.0, 10.0), pos2(400.0, 70.0));

    let drawn = note_bars(&notes, range_of(&notes), thirty(), 0, 90, the_clip(), in_view);

    assert_eq!(drawn.len(), 1, "it is in view where it goes on");
}

#[test]
fn notes_that_would_be_on_top_of_each_other_are_one_bar() {
    // (A thousand notes of one key in 5 thousandths of a second: half a point of the clip.)
    let many: Vec<MidiNote> = (0..1000)
        .map(|n| MidiNote {
            start_us: 500_000 + n * 5,
            end_us: 500_000 + n * 5 + 1,
            channel: 0,
            key: 60,
            velocity: 100,
        })
        .collect();

    let drawn = bars(&many);

    assert_eq!(drawn.len(), 1);
    let other_keys = bars(&[note(500, 501, 60), note(500, 501, 61)]);
    assert_eq!(other_keys.len(), 2, "another key is another bar");
    let other_columns = bars(&[note(500, 501, 60), note(600, 601, 60)]);
    assert_eq!(other_columns.len(), 2, "another column of pixels is another bar");
}

#[test]
fn a_clip_of_no_length_or_no_width_has_no_bars() {
    let notes = [note(0, 500, 60)];

    assert!(note_bars(&notes, range_of(&notes), thirty(), 10, 10, the_clip(), the_clip()).is_empty());
    let nothing = Rect::from_min_max(pos2(100.0, 10.0), pos2(100.0, 70.0));
    assert!(note_bars(&notes, range_of(&notes), thirty(), 0, 90, nothing, nothing).is_empty());
}

#[test]
fn the_frame_rate_of_the_project_says_how_many_frames_a_second_of_the_file_is() {
    // (At 24 frames a second the clip of 72 frames is the 3 seconds: the same bars.)
    let notes = [note(500, 1250, 60)];

    let at_24 = note_bars(
        &notes,
        range_of(&notes),
        FrameRate::new(24, 1).unwrap(),
        0,
        72,
        the_clip(),
        the_clip(),
    );

    assert!((at_24[0].left() - 150.0).abs() < 1e-3);
    assert!((at_24[0].right() - 225.0).abs() < 1e-3);
}

#[test]
fn a_bar_is_at_most_eight_points_tall_however_tall_the_clip_is() {
    let tall = Rect::from_min_max(pos2(100.0, 10.0), pos2(400.0, 410.0));
    let notes = [note(0, 500, 60)];

    let drawn = note_bars(&notes, (60, 60), thirty(), 0, 90, tall, tall);

    assert_eq!(drawn[0].height(), 8.0);
}
