// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of making tracks taller and shorter by dragging the boundary under their title boxes.

use egui::{CursorIcon, Key, Modifiers, PointerButton, Pos2, vec2};

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::theme;
use crate::application::timeline_panel::HEIGHT_THE_BOX_NEEDS;
use crate::project::TrackHeight;

/// A window with a sound on the track in front and a video on the one behind it.
fn window_with_two_tracks() -> Window {
    Window::showing(project_with_a_video_and_a_sound(300, 10))
}

/// Where the boundary under the title box of the track at `position` is: in the middle of the gap
/// between that box and the one under it, below the title (which is clear of it at any height).
fn boundary_under(window: &Window, position: usize) -> Pos2 {
    Pos2::new(
        window.title_box_of_track(position).center().x,
        window.strip_of_track(position).bottom() + theme::SPACE_BETWEEN_TRACKS / 2.0,
    )
}

fn height_of(window: &Window, position: usize) -> f32 {
    window.application().project().tracks()[position].height().points()
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 0.5, "{what}: {actual} is not {expected}");
}

fn set_height(window: &mut Window, position: usize, points: f32) {
    let track = window.application().project().tracks()[position].id();
    window.do_this(Command::SetTrackHeight {
        track,
        height: TrackHeight::from_points(points),
    });
}

// --- the limits ---

#[test]
fn the_least_a_track_can_be_is_just_taller_than_what_its_dots_and_title_need_and_the_most_is_half_as_much_again() {
    let needs = HEIGHT_THE_BOX_NEEDS;
    let least = TrackHeight::SMALLEST_IN_POINTS;
    assert!(
        least > needs,
        "the dots and the title fit: {least} is more than {needs}"
    );
    assert!(
        least <= needs + 4.0,
        "and only just: {least} is not within 4 points of {needs}"
    );
    assert_eq!(TrackHeight::LARGEST_IN_POINTS, 1.5 * TrackHeight::DEFAULT_IN_POINTS);
}

#[test]
fn at_the_least_height_the_title_is_inside_the_title_box() {
    let mut window = window_with_two_tracks();
    set_height(&mut window, 0, 0.0);
    let row = window.strip_of_track(0); // the title box has the top and the bottom of its strip
    assert_close(row.height(), TrackHeight::SMALLEST_IN_POINTS, "the height of the row");

    let title = window.title_box_of_track(0);
    assert!(
        title.top() >= row.top() && title.bottom() <= row.bottom(),
        "the title ({title:?}) is inside the box ({row:?})"
    );
}

#[test]
fn at_the_greatest_height_a_row_is_half_as_tall_again_as_a_new_one() {
    let mut window = window_with_two_tracks();
    let normal = window.strip_of_track(0).height();
    set_height(&mut window, 0, 10_000.0);
    assert_close(window.strip_of_track(0).height(), 1.5 * normal, "the tallest row");
}

// --- dragging the boundary ---

#[test]
fn dragging_the_boundary_under_a_title_box_makes_that_track_taller_and_no_other() {
    let mut window = window_with_two_tracks();
    let from = boundary_under(&window, 0);

    window.drag(from, from + vec2(0.0, 30.0), PointerButton::Primary);

    assert_close(height_of(&window, 0), 104.0 + 30.0, "the track that was dragged");
    assert_close(height_of(&window, 1), 104.0, "the one behind it");
    assert_close(
        window.strip_of_track(0).height(),
        134.0,
        "its row, and the clips on it, are as tall",
    );
    assert_close(window.strip_of_track(1).height(), 104.0, "the row behind it");
    assert!(window.application().can_undo(), "it is an edit");
}

#[test]
fn dragging_it_up_makes_the_track_shorter() {
    let mut window = window_with_two_tracks();
    let from = boundary_under(&window, 0);
    window.drag(from, from - vec2(0.0, 40.0), PointerButton::Primary);
    assert_close(height_of(&window, 0), 64.0, "the track");
}

#[test]
fn the_boundary_stays_under_the_pointer_while_it_is_dragged() {
    // The window at the bottom of the screen grows upward when the rows get taller, so if it
    // did that while the boundary is dragged, the boundary would stay where it is and the pointer
    // would go on without it.
    let mut window = window_with_two_tracks();
    let top_before = window.strip_of_track(0).top();
    let from = boundary_under(&window, 0);

    window.drag_and_hold(from, from + vec2(0.0, 30.0), PointerButton::Primary);
    // (What the last move of the pointer asked for is drawn in the frame after it.)
    window.harness.run_steps(2);

    let after = boundary_under(&window, 0);
    assert_close(after.y, from.y + 30.0, "the boundary has gone as far as the pointer");
    assert_close(
        window.strip_of_track(0).top(),
        top_before,
        "and the top of the row has not moved",
    );
    assert_close(height_of(&window, 0), 134.0, "while the button is still down");
    window.release(from + vec2(0.0, 30.0), PointerButton::Primary);
}

#[test]
fn a_track_cannot_be_dragged_shorter_than_the_least_or_taller_than_the_most() {
    let mut window = window_with_two_tracks();
    let from = boundary_under(&window, 0);
    window.drag(from, from + vec2(0.0, 80.0), PointerButton::Primary);
    assert_eq!(
        height_of(&window, 0),
        TrackHeight::LARGEST_IN_POINTS,
        "dragged far down"
    );

    let from = boundary_under(&window, 0);
    window.drag(from, from - vec2(0.0, 150.0), PointerButton::Primary);
    assert_eq!(height_of(&window, 0), TrackHeight::SMALLEST_IN_POINTS, "dragged far up");
}

#[test]
fn the_boundary_under_the_last_track_can_be_dragged_too() {
    let mut window = window_with_two_tracks();
    let from = boundary_under(&window, 1);

    window.drag(from, from + vec2(0.0, 20.0), PointerButton::Primary);

    assert_close(height_of(&window, 1), 124.0, "the last track");
    assert_close(height_of(&window, 0), 104.0, "the one in front of it");
}

#[test]
fn the_boundary_under_the_last_track_can_be_dragged_when_the_rows_scroll_and_are_at_their_end() {
    let mut window = window_with_two_tracks();
    for _ in 0..3 {
        window.do_this(Command::AddTrack);
    }
    let last = window.application().project().tracks().len() - 1;
    assert_eq!(last, 4, "five tracks, of which three are shown at once");
    // Scroll the rows right to the end.
    let over_the_title_boxes = window.title_box_of_track(0).center();
    window.scroll_the_wheel(over_the_title_boxes, vec2(0.0, -100_000.0), Modifiers::ALT);

    let from = boundary_under(&window, last);
    window.drag(from, from + vec2(0.0, 12.0), PointerButton::Primary);

    assert_close(
        height_of(&window, last),
        116.0,
        "the last track, at the very end of the rows",
    );
}

#[test]
fn one_drag_is_one_step_of_the_history_which_undoing_takes_back_whole() {
    let mut window = window_with_two_tracks();
    window.application_mut().note_everything_is_saved();
    let from = boundary_under(&window, 0);
    window.drag(from, from + vec2(0.0, 30.0), PointerButton::Primary);
    assert_close(height_of(&window, 0), 134.0, "dragged");

    window.harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    window.harness.step();
    window.settle();

    assert_close(
        height_of(&window, 0),
        104.0,
        "back to what it was before the drag, not to a part of it",
    );
    assert!(!window.application().can_undo(), "and that was the one step");
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn the_boundary_has_a_resizing_pointer_over_it_and_the_ordinary_one_elsewhere() {
    let mut window = window_with_two_tracks();

    window.move_pointer_to(boundary_under(&window, 0));
    window.harness.step();
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeVertical
    );

    window.move_pointer_to(window.middle_of_the_track_at(10));
    window.harness.step();
    assert_ne!(
        window.harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeVertical
    );
}

#[test]
fn rows_of_other_heights_keep_their_clips_where_they_are_along_the_timeline() {
    let mut window = Window::showing(project_of(300));
    let x_before = window.x_of_frame_on_the_strip(0, 100);
    set_height(&mut window, 0, 40.0);
    assert_close(window.x_of_frame_on_the_strip(0, 100), x_before, "frame 100");
}

#[test]
fn a_shorter_track_makes_the_whole_timeline_shorter() {
    let mut window = Window::showing(project_of(300));
    let ruler_before = window.rectangle_of("Ruler").top();
    set_height(&mut window, 0, 50.0);
    // The timeline is at the bottom of the window and everything above it makes way.
    assert_close(
        window.rectangle_of("Ruler").top(),
        ruler_before + 54.0,
        "the ruler comes down as the row gets shorter",
    );
}

#[test]
fn rows_never_take_up_more_of_the_window_than_three_rows_of_the_normal_height() {
    let mut window = window_with_two_tracks();
    window.do_this(Command::AddTrack);
    window.do_this(Command::AddTrack);
    for position in 0..4 {
        set_height(&mut window, position, 10_000.0);
    }
    let first = window.strip_of_track(0);
    let bottom_of_the_rows = window.the_bottom_of_what_can_be_seen_of_the_rows();
    let three_normal_rows = 3.0 * TrackHeight::DEFAULT_IN_POINTS + 2.0 * theme::SPACE_BETWEEN_TRACKS;
    assert!(
        bottom_of_the_rows - first.top() < three_normal_rows + 80.0,
        "{} points from the top of the rows to where they end at the bottom",
        bottom_of_the_rows - first.top()
    );
}
