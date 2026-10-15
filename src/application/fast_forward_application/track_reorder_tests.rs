// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of moving a track to another place in the list by dragging its title box: the row follows the pointer,
//! the other rows move out of its way, and where it is let go it is put.

use egui::{Key, PointerButton, Pos2, Shape, vec2};

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::theme;
use crate::project::TrackHeight;

/// A window with three tracks of the normal height: "track 2" in front, then "track 1", then "track 0", which has
/// the video.
fn window_with_three_tracks() -> Window {
    let mut window = Window::showing(project_of(300));
    window.do_this(Command::AddTrack);
    window.do_this(Command::AddTrack);
    window
}

fn titles(window: &Window) -> Vec<String> {
    (0..window.application().project().tracks().len())
        .map(|position| window.title_of_track(position))
        .collect()
}

/// How far the top of one row is from the top of the next when none of them is moving.
const ROW_PITCH: f32 = TrackHeight::DEFAULT_IN_POINTS + theme::SPACE_BETWEEN_TRACKS;

/// A point in the box of the track at `position` that is not on its title, in the margin at its left.
fn on_the_box(window: &Window, position: usize) -> Pos2 {
    let title = window.title_box_of_track(position);
    Pos2::new(title.left() - 4.0, window.strip_of_track(position).center().y)
}

fn on_the_title(window: &Window, position: usize) -> Pos2 {
    window.title_box_of_track(position).center()
}

/// Lets the window go on for a little over half a second, which is much longer than the rows take to settle.
fn let_the_rows_settle(window: &mut Window) {
    window.harness.run_steps(40);
}

fn top_of_strip(window: &Window, position: usize) -> f32 {
    window.strip_of_track(position).top()
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 1.0, "{what}: {actual} is not {expected}");
}

// --- the result ---

#[test]
fn dragging_the_box_of_a_track_down_over_the_next_one_and_letting_go_puts_it_after_that_one() {
    let mut window = window_with_three_tracks();
    assert_eq!(titles(&window), ["track 2", "track 1", "track 0"]);
    let from = on_the_box(&window, 0);

    window.drag(from, from + vec2(0.0, 80.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 1", "track 2", "track 0"]);
}

#[test]
fn a_track_can_be_dragged_up_and_over_more_than_one_track() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 2);

    window.drag(from, from - vec2(0.0, 2.0 * ROW_PITCH - 10.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 0", "track 2", "track 1"]);
}

#[test]
fn a_track_is_put_where_its_row_is_when_it_is_let_go_not_where_the_pointer_started_or_stopped() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 1);
    // Down over the last track and back up to a little above where it began: it goes back where it was.
    window.drag_and_hold(from, from + vec2(0.0, 1.5 * ROW_PITCH), PointerButton::Primary);
    window.move_pointer_to(from - vec2(0.0, 10.0));
    window.release(from - vec2(0.0, 10.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 2", "track 1", "track 0"]);
}

#[test]
fn a_little_dragging_that_does_not_take_a_row_past_half_of_the_next_changes_nothing() {
    let mut window = window_with_three_tracks();
    window.application_mut().note_everything_is_saved();
    let from = on_the_box(&window, 0);

    window.drag(from, from + vec2(0.0, ROW_PITCH / 2.0 - 8.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 2", "track 1", "track 0"]);
    assert!(!window.application().has_unsaved_changes(), "it is not an edit");
    assert_close(
        top_of_strip(&window, 0),
        top_of_strip(&window, 1) - ROW_PITCH,
        "the row went back to its place",
    );
    window.do_this(Command::Undo);
    assert_eq!(
        titles(&window),
        ["track 1", "track 0"],
        "what undoing takes back is the adding of the track made last: the drag was no step of the history"
    );
}

#[test]
fn a_track_dragged_above_all_the_others_or_below_all_of_them_goes_first_or_last() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 1);
    window.drag(from, from - vec2(0.0, 600.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert_eq!(titles(&window), ["track 1", "track 2", "track 0"], "first");

    let from = on_the_box(&window, 0);
    window.drag(from, from + vec2(0.0, 900.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert_eq!(titles(&window), ["track 2", "track 0", "track 1"], "last");
}

#[test]
fn moving_a_track_is_one_step_of_the_history_and_undoing_it_puts_the_tracks_back() {
    let mut window = window_with_three_tracks();
    window.application_mut().note_everything_is_saved();
    let from = on_the_box(&window, 0);

    window.drag(from, from + vec2(0.0, 2.0 * ROW_PITCH), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert_eq!(titles(&window), ["track 1", "track 0", "track 2"]);
    assert!(window.application().has_unsaved_changes());

    window.do_this(Command::Undo);
    assert_eq!(titles(&window), ["track 2", "track 1", "track 0"]);
    window.do_this(Command::Redo);
    assert_eq!(titles(&window), ["track 1", "track 0", "track 2"]);
}

#[test]
fn a_track_that_is_moved_keeps_what_is_selected_selected() {
    let mut window = window_with_three_tracks();
    window.select_the_track_at(0);
    let selected = window.application().selected_items();
    assert_eq!(selected.len(), 1);

    let from = on_the_box(&window, 0);
    window.drag(from, from + vec2(0.0, ROW_PITCH), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 1", "track 2", "track 0"]);
    assert_eq!(
        window.application().selected_items(),
        selected,
        "dragging a box selects nothing and loses nothing"
    );
}

#[test]
fn a_click_on_the_box_still_selects_the_track_and_is_not_a_drag() {
    let mut window = window_with_three_tracks();
    window.select_the_track_at(1);
    assert_eq!(titles(&window), ["track 2", "track 1", "track 0"]);
    assert_eq!(window.application().selected_items().len(), 1);
}

// --- while the box is dragged ---

#[test]
fn while_the_box_is_dragged_the_whole_row_goes_up_and_down_with_the_pointer() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 0);
    let strip_top = top_of_strip(&window, 0);
    let title_top = window.title_box_of_track(0).top();

    window.drag_and_hold(from, from + vec2(0.0, 40.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_close(
        top_of_strip(&window, 0),
        strip_top + 40.0,
        "the strip of the dragged track",
    );
    assert_close(window.title_box_of_track(0).top(), title_top + 40.0, "and its title");
    assert_eq!(
        titles(&window),
        ["track 2", "track 1", "track 0"],
        "the list is not changed until it is let go"
    );
    window.release(from + vec2(0.0, 40.0), PointerButton::Primary);
}

#[test]
fn the_row_keeps_the_place_under_the_pointer_where_it_was_taken_hold_of() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 0);
    let held_this_far_below_the_top = from.y - top_of_strip(&window, 0);

    window.drag_and_hold(from, from + vec2(0.0, 50.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    window.move_pointer_to(from + vec2(0.0, 17.0));
    let_the_rows_settle(&mut window);

    assert_close(
        from.y + 17.0 - top_of_strip(&window, 0),
        held_this_far_below_the_top,
        "how far the pointer is under the top of the row",
    );
    window.release(from + vec2(0.0, 17.0), PointerButton::Primary);
}

#[test]
fn a_row_the_dragged_one_has_gone_past_goes_back_to_where_the_dragged_one_started() {
    let mut window = window_with_three_tracks();
    let first_top = top_of_strip(&window, 0);
    let second_top = top_of_strip(&window, 1);
    let third_top = top_of_strip(&window, 2);
    let from = on_the_box(&window, 0);

    // Past the middle of the second row.
    window.drag_and_hold(from, from + vec2(0.0, ROW_PITCH / 2.0 + 15.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_close(
        top_of_strip(&window, 1),
        first_top,
        "the second track is where the first started",
    );
    assert_close(top_of_strip(&window, 2), third_top, "the third is where it was");
    assert_close(
        top_of_strip(&window, 0),
        first_top + ROW_PITCH / 2.0 + 15.0,
        "and the first is where the pointer has it",
    );
    assert!(second_top > first_top);
    window.release(from, PointerButton::Primary);
}

#[test]
fn a_row_the_dragged_one_has_come_back_from_goes_to_where_it_was() {
    let mut window = window_with_three_tracks();
    let second_top = top_of_strip(&window, 1);
    let from = on_the_box(&window, 0);

    window.drag_and_hold(from, from + vec2(0.0, ROW_PITCH), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert!(top_of_strip(&window, 1) < second_top - 50.0, "it went away");
    window.move_pointer_to(from + vec2(0.0, 5.0));
    let_the_rows_settle(&mut window);

    assert_close(top_of_strip(&window, 1), second_top, "and it came back");
    window.release(from, PointerButton::Primary);
}

#[test]
fn the_rows_make_way_in_a_short_animation_and_not_all_at_once() {
    let mut window = window_with_three_tracks();
    let first_top = top_of_strip(&window, 0);
    let second_top = top_of_strip(&window, 1);
    let from = on_the_box(&window, 0);
    window.drag_and_hold(from, from + vec2(0.0, 20.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    // The pointer jumps to well past the middle of the next row.
    window.move_pointer_to(from + vec2(0.0, ROW_PITCH / 2.0 + 30.0));
    let mut tops = vec![top_of_strip(&window, 1)];
    for _ in 0..40 {
        // (A frame that nothing happened in, so that if the window is to be drawn again at once it is for the rows.)
        window.harness.step();
        assert!(
            window.repaint_is_wanted_at_once(),
            "the window is drawn again at once, to show the rows moving"
        );
        tops.push(top_of_strip(&window, 1));
        if (tops.last().unwrap() - first_top).abs() < 0.5 {
            break;
        }
    }

    assert!(
        tops[0] > first_top + 30.0,
        "it has not got there in the frame the pointer jumped in: {tops:?}"
    );
    assert!(
        tops.windows(2).all(|pair| pair[1] <= pair[0]),
        "it only goes the one way: {tops:?}"
    );
    assert!(tops.iter().all(|top| *top <= second_top + 0.5), "{tops:?}");
    assert_close(*tops.last().unwrap(), first_top, "it gets there");
    assert!(
        (3..=25).contains(&tops.len()),
        "in a short while, which is not {} frames: {tops:?}",
        tops.len()
    );
    window.release(from, PointerButton::Primary);
}

#[test]
fn the_dragged_row_is_drawn_over_the_others_with_a_shadow_under_it() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 0);
    window.drag_and_hold(from, from + vec2(0.0, 60.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    let boxes: Vec<f32> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::TRACK_HEADER_BACKGROUND => Some(rectangle.rect.top()),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 3, "{boxes:?}");
    assert_close(
        *boxes.last().unwrap(),
        top_of_strip(&window, 0),
        "the box drawn last, over the others, is the dragged one",
    );
    let shadows_drawn = |window: &Window| {
        window
            .shapes_drawn()
            .into_iter()
            .filter(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.blur_width > 0.0))
            .count()
    };
    assert_eq!(shadows_drawn(&window), 1, "one shadow, for the one that is lifted");
    window.release(from, PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert_eq!(shadows_drawn(&window), 0, "and none when nothing is dragged");
}

#[test]
fn the_pointer_is_a_grabbing_hand_while_a_track_is_dragged_and_not_after() {
    let mut window = window_with_three_tracks();
    let from = on_the_box(&window, 0);
    window.drag_and_hold(from, from + vec2(0.0, 30.0), PointerButton::Primary);
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Grabbing
    );
    window.release(from + vec2(0.0, 30.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert_ne!(
        window.harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Grabbing
    );
}

// --- when it is let go ---

#[test]
fn when_the_track_is_let_go_it_settles_into_its_place_in_a_short_animation_and_nothing_moves_after_that() {
    let mut window = window_with_three_tracks();
    let first_top = top_of_strip(&window, 0);
    let from = on_the_box(&window, 0);
    window.drag_and_hold(from, from + vec2(0.0, 70.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    window.release(from + vec2(0.0, 70.0), PointerButton::Primary);

    // The track is now second in the list: its row goes the rest of the way to where the second row is.
    assert_eq!(titles(&window), ["track 1", "track 2", "track 0"]);
    let mut tops = vec![top_of_strip(&window, 1)];
    for _ in 0..40 {
        window.harness.step();
        tops.push(top_of_strip(&window, 1));
    }
    assert!(
        tops[0] < first_top + ROW_PITCH - 5.0,
        "it is not in its place the moment it is let go: {tops:?}"
    );
    assert!(
        tops.windows(2).all(|pair| pair[1] >= pair[0]),
        "it only goes the one way: {tops:?}"
    );
    assert_close(*tops.last().unwrap(), first_top + ROW_PITCH, "it gets to its place");
    assert!(
        !window.repaint_is_wanted_at_once(),
        "and the window is not drawn over and over when all is still"
    );
    for position in 0..3 {
        assert_close(
            top_of_strip(&window, position),
            first_top + position as f32 * ROW_PITCH,
            "every row is in its place",
        );
    }
}

// --- the title ---

#[test]
fn dragging_the_title_up_or_down_moves_the_track_too_and_ends_the_typing_in_it() {
    let mut window = window_with_three_tracks();
    let from = on_the_title(&window, 0);

    window.drag(from, from + vec2(2.0, 90.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 1", "track 2", "track 0"]);
    assert_eq!(
        window.harness.ctx.memory(|memory| memory.focused()),
        None,
        "the title is not being typed in: the keys are the window's"
    );
    window.press_key(Key::S);
    assert_eq!(
        titles(&window),
        ["track 1", "track 2", "track 0"],
        "S was not typed into the title"
    );
}

#[test]
fn dragging_along_the_title_selects_its_text_and_does_not_move_the_track() {
    let mut window = window_with_three_tracks();
    let title = window.title_box_of_track(0);
    let from = Pos2::new(title.left() + 10.0, title.center().y);

    window.drag(from, from + vec2(60.0, 3.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);

    assert_eq!(titles(&window), ["track 2", "track 1", "track 0"]);
    assert!(
        window.harness.ctx.memory(|memory| memory.focused()).is_some(),
        "the title is still being typed in"
    );
}

// --- more tracks than fit ---

#[test]
fn dragging_a_track_to_the_bottom_of_the_rows_scrolls_them_to_bring_the_others_into_view() {
    let mut window = window_with_three_tracks();
    window.do_this(Command::AddTrack);
    window.do_this(Command::AddTrack);
    window.do_this(Command::AddTrack);
    assert_eq!(window.application().project().tracks().len(), 6);
    let from = on_the_box(&window, 0);
    let bottom_of_the_rows = top_of_strip(&window, 2) + TrackHeight::DEFAULT_IN_POINTS;
    assert_eq!(window.application().interface.timeline_view.rows_scroll_offset, 0.0);

    window.drag_and_hold(
        from,
        Pos2::new(from.x, bottom_of_the_rows + 20.0),
        PointerButton::Primary,
    );
    window.harness.run_steps(60);

    assert!(
        window.application().interface.timeline_view.rows_scroll_offset > 100.0,
        "the rows scrolled down: {}",
        window.application().interface.timeline_view.rows_scroll_offset
    );
    window.release(Pos2::new(from.x, bottom_of_the_rows + 20.0), PointerButton::Primary);
    let_the_rows_settle(&mut window);
    assert_eq!(
        titles(&window).last().unwrap(),
        "track 5",
        "the track that was in front went all the way to the back, which was out of sight at first"
    );
}
