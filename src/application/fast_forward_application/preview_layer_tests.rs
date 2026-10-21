// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what is done to videos and text on the preview with the mouse: picking one, moving it,
//! resizing it by a corner and turning it by dragging just outside a corner.

use egui::epaint::ColorMode;
use egui::{Color32, CursorIcon, Event, PointerButton, Pos2, Shape, Vec2, pos2, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{
    Window, button_pressed_at, button_released_at, portrait_project_of, project_of, window_inside_a_sped_up_clip,
};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::preview_panel::{corner_handle_label, text_object_label, turn_handle_label, video_label};
use crate::application::selection::SelectionChange;
use crate::application::theme;
use crate::project::{Angle, CanvasArea, ClipId, Corner, Placement, TextObject, TimelineFrameIndex};

/// A window with a video of 300 frames on its one track and, over it for 200 frames from the start, a
/// text object on a track in front, which is selected.
fn window_with_a_text_object_selected() -> (Window, ClipId) {
    let mut project = project_of(300);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), TextObject::default(), 200)
        .unwrap();
    let mut window = Window::showing(project);
    select(&mut window, text);
    (window, text)
}

/// Like [`window_with_a_text_object_selected`], with the text in the middle of the picture, so that
/// the corners of it stay inside the preview when it is turned (a corner that is outside the
/// preview, where the timeline is, cannot be reached with the pointer).
fn window_with_a_text_object_in_the_middle_selected() -> (Window, ClipId) {
    let mut project = project_of(300);
    let front = project.add_track_in_front();
    let in_the_middle = TextObject {
        area: CanvasArea::new(0.3, 0.4, 0.4, 0.2),
        ..TextObject::default()
    };
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), in_the_middle, 200)
        .unwrap();
    let mut window = Window::showing(project);
    select(&mut window, text);
    (window, text)
}

/// A window with a video of 300 frames on its one track, which is selected.
fn window_with_the_video_selected() -> (Window, ClipId) {
    let project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let mut window = Window::showing(project);
    select(&mut window, video);
    (window, video)
}

pub(super) fn select(window: &mut Window, clip: ClipId) {
    window.do_this(Command::select_clips(vec![clip], SelectionChange::Replace));
}

fn placement_of(window: &Window, clip: ClipId) -> Placement {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .placement()
        .expect("a clip that is on the picture")
}

/// Where the corner handle of the selected layer is on the screen.
fn corner_position(window: &Window, corner: Corner) -> Pos2 {
    window.rectangle_of(&corner_handle_label(corner)).center()
}

/// The middle of the selected layer on the screen.
pub(super) fn middle_of_the_layer(window: &Window) -> Pos2 {
    let corners = Corner::ALL.map(|corner| corner_position(window, corner).to_vec2());
    (corners.iter().fold(Vec2::ZERO, |sum, corner| sum + *corner) / 4.0).to_pos2()
}

/// A point `distance` points away from `corner` of the selected layer, on the line from its middle through the corner.
fn just_outside(window: &Window, corner: Corner, distance: f32) -> Pos2 {
    let corner_position = corner_position(window, corner);
    let outwards = (corner_position - middle_of_the_layer(window)).normalized();
    corner_position + outwards * distance
}

/// `point` turned clockwise (as it is seen on the screen) by `degrees` about `middle`.
fn turned_about(point: Pos2, middle: Pos2, degrees: f32) -> Pos2 {
    let (sine, cosine) = degrees.to_radians().sin_cos();
    let offset = point - middle;
    middle + vec2(offset.x * cosine - offset.y * sine, offset.x * sine + offset.y * cosine)
}

fn assert_degrees(actual: Angle, expected: f32, context: &str) {
    assert!(
        (actual.degrees() - expected).abs() < 0.5,
        "{context}: turned {} degrees, not {expected}",
        actual.degrees()
    );
}

// --- turning ---

#[test]
fn dragging_round_from_just_outside_a_corner_turns_text_clockwise_as_far_as_the_pointer_goes_round_its_middle() {
    for corner in Corner::ALL {
        let (mut window, text) = window_with_a_text_object_selected();
        let before = placement_of(&window, text);
        let middle = middle_of_the_layer(&window);
        let start = just_outside(&window, corner, 18.0);

        window.drag(start, turned_about(start, middle, 30.0), PointerButton::Primary);

        let after = placement_of(&window, text);
        assert_degrees(after.rotation, 30.0, &format!("{corner:?}"));
        assert_eq!(
            after.area, before.area,
            "{corner:?}: turning does not move or resize it"
        );
        assert!(window.application().has_unsaved_changes());
    }
}

#[test]
fn dragging_the_other_way_round_turns_it_counter_clockwise_and_it_can_go_on_turning_from_there() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.drag(start, turned_about(start, middle, -40.0), PointerButton::Primary);
    assert_degrees(placement_of(&window, text).rotation, -40.0, "once");

    // The corner has gone round with the text: it is grabbed where it is now.
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    let middle = middle_of_the_layer(&window);
    window.drag(start, turned_about(start, middle, 100.0), PointerButton::Primary);
    assert_degrees(placement_of(&window, text).rotation, 60.0, "and again");
}

#[test]
fn it_turns_from_where_it_was_grabbed_and_not_from_where_the_pointer_was_when_the_drag_began() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopRight, 18.0);
    // The pointer goes a long way round, past the other side, and comes back to a quarter turn.
    window.drag_and_hold(start, turned_about(start, middle, 20.0), PointerButton::Primary);
    window.move_pointer_to(turned_about(start, middle, 170.0));
    window.move_pointer_to(turned_about(start, middle, 90.0));
    window.release(turned_about(start, middle, 90.0), PointerButton::Primary);
    assert_degrees(placement_of(&window, text).rotation, 90.0, "a quarter turn clockwise");
}

#[test]
fn going_on_round_turns_it_past_half_a_turn_and_past_a_whole_one() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopRight, 18.0);
    // The pointer goes round in steps of 70 degrees, each of which is the way round it is going.
    window.drag_and_hold(start, turned_about(start, middle, 70.0), PointerButton::Primary);
    for degrees in [140.0, 210.0, 280.0, 350.0, 420.0] {
        window.move_pointer_to(turned_about(start, middle, degrees));
    }
    window.release(turned_about(start, middle, 420.0), PointerButton::Primary);

    assert_degrees(
        placement_of(&window, text).rotation,
        420.0,
        "a whole turn and a sixth clockwise",
    );
}

#[test]
fn going_round_the_other_way_for_more_than_a_whole_turn_turns_it_that_way() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopRight, 18.0);
    window.drag_and_hold(start, turned_about(start, middle, -70.0), PointerButton::Primary);
    for degrees in [-140.0, -210.0, -280.0, -350.0, -420.0] {
        window.move_pointer_to(turned_about(start, middle, degrees));
    }
    window.release(turned_about(start, middle, -420.0), PointerButton::Primary);

    assert_degrees(
        placement_of(&window, text).rotation,
        -420.0,
        "a whole turn and a sixth back",
    );
}

#[test]
fn a_turn_with_shift_held_is_made_a_whole_number_of_fifteen_degrees() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomLeft, 18.0);
    window.hold_shift();
    window.drag(start, turned_about(start, middle, 37.0), PointerButton::Primary);
    window.let_go_of_shift();
    assert_eq!(
        placement_of(&window, text).rotation,
        Angle::from_degrees(30.0),
        "37 is nearer 30 than 45"
    );
}

#[test]
fn turning_is_one_step_of_the_history_and_undoing_it_puts_the_text_upright_again() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.drag(start, turned_about(start, middle, 50.0), PointerButton::Primary);
    assert_degrees(placement_of(&window, text).rotation, 50.0, "turned");

    window.do_this(Command::Undo);

    assert_eq!(
        placement_of(&window, text).rotation,
        Angle::ZERO,
        "one undo is enough for the whole drag"
    );
    assert!(!window.application().can_undo(), "and there was nothing before it");
}

#[test]
fn a_press_just_inside_a_corner_moves_the_text_and_does_not_turn_it() {
    let (mut window, text) = window_with_a_text_object_selected();
    let before = placement_of(&window, text);
    let corner = corner_position(&window, Corner::BottomRight);
    // Inside the box, beyond the little square that resizes.
    let inside = corner + vec2(-16.0, -12.0);

    window.drag(inside, inside + vec2(-40.0, -30.0), PointerButton::Primary);

    let after = placement_of(&window, text);
    assert_eq!(after.rotation, Angle::ZERO);
    assert!(
        after.area.left() < before.area.left() && after.area.top() < before.area.top(),
        "{before:?} then {after:?}"
    );
    assert_eq!(
        (after.area.width(), after.area.height()),
        (before.area.width(), before.area.height())
    );
}

#[test]
fn the_place_to_turn_from_that_is_named_at_each_corner_is_the_place_a_press_turns_from_and_is_round_the_corner() {
    let (window, _) = window_with_a_text_object_in_the_middle_selected();
    for corner in Corner::ALL {
        let place = window.rectangle_of(&turn_handle_label(corner));
        let at = corner_position(&window, corner);

        // (A press turns the layer from where it is as near the corner as 34 points: the place that is named for screen readers, and
        // that the widget is made as, is the square round that.)
        assert!(
            (place.center() - at).length() < 0.6,
            "{corner:?}: the place is round the corner: {place:?} and {at:?}"
        );
        assert!(
            (place.width() - 68.0).abs() < 0.6 && (place.height() - 68.0).abs() < 0.6,
            "{corner:?}: it is twice 34 points wide and tall: {place:?}"
        );
    }
}

#[test]
fn a_press_far_from_the_text_turns_nothing() {
    let (mut window, text) = window_with_a_text_object_selected();
    let before = placement_of(&window, text);
    let far = just_outside(&window, Corner::TopLeft, 80.0);
    window.drag(far, far + vec2(30.0, 60.0), PointerButton::Primary);
    assert_eq!(placement_of(&window, text), before);
}

#[test]
fn only_the_selected_layer_can_be_turned_from_outside_its_corners() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.do_this(Command::Deselect);
    assert!(
        window
            .harness
            .query_by_label(&turn_handle_label(Corner::BottomRight))
            .is_none()
    );

    window.drag(start, turned_about(start, middle, 30.0), PointerButton::Primary);

    assert_eq!(placement_of(&window, text).rotation, Angle::ZERO);
}

#[test]
fn a_turned_text_object_is_grabbed_where_it_is_and_not_in_the_corners_of_the_rectangle_round_it() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.drag(start, turned_about(start, middle, 45.0), PointerButton::Primary);
    assert_degrees(placement_of(&window, text).rotation, 45.0, "turned an eighth");
    window.do_this(Command::Deselect);
    let bounds = window.rectangle_of(&text_object_label(text));
    let area_before = placement_of(&window, text).area;

    // A corner of the rectangle that the turned text fits in is not on the text, which, turned an
    // eighth of a turn, runs from the top left to the bottom right: the other two corners are far from it.
    let bounds_corner = bounds.right_top() + vec2(-3.0, 3.0);
    window.click(bounds_corner);
    assert_ne!(
        window.application().selected_clip(),
        Some(text),
        "that is not the text (it is the video behind it that was picked)"
    );
    // The middle of it is.
    window.click(middle);
    assert_eq!(window.application().selected_clip(), Some(text));
    assert_eq!(placement_of(&window, text).area, area_before);
}

// --- resizing a turned one ---

#[test]
fn dragging_a_corner_of_a_turned_text_resizes_it_with_the_corner_across_staying_where_it_is_on_the_screen() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.drag(start, turned_about(start, middle, 20.0), PointerButton::Primary);
    let across = corner_position(&window, Corner::TopLeft);
    let corner = corner_position(&window, Corner::BottomRight);
    let before = placement_of(&window, text);

    // Along the box, towards the other corner: makes it shorter.
    window.drag(corner, corner + (across - corner) * 0.25, PointerButton::Primary);

    let after = placement_of(&window, text);
    assert_eq!(after.rotation, before.rotation, "still turned the same");
    assert!(
        after.area.width() < before.area.width() - 0.02,
        "{before:?} then {after:?}"
    );
    let still_there = corner_position(&window, Corner::TopLeft);
    assert!(still_there.distance(across) < 1.5, "{across:?} became {still_there:?}");
}

// --- videos ---

#[test]
fn clicking_a_video_on_the_preview_selects_its_clip_and_shows_its_corners() {
    let project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let mut window = Window::showing(project);
    assert_eq!(window.application().selected_clip(), None);
    let picture = window.rectangle_of(&video_label(video));

    window.click(picture.center());

    assert_eq!(window.application().selected_clip(), Some(video));
    for corner in Corner::ALL {
        assert!(
            window.harness.query_by_label(&corner_handle_label(corner)).is_some(),
            "{corner:?}"
        );
    }
}

#[test]
fn a_video_that_fills_the_picture_can_be_moved_out_past_the_edges_until_its_middle_is_at_them() {
    let (mut window, video) = window_with_the_video_selected();
    assert_eq!(
        placement_of_the_video_or_none(&window, video),
        None,
        "no place of its own yet"
    );
    let middle = middle_of_the_layer(&window);

    window.drag(middle, middle + vec2(25.0, 25.0), PointerButton::Primary);

    let moved = placement_of(&window, video);
    assert!(moved.area.left() > 0.01 && moved.area.top() > 0.01, "{moved:?}");
    assert_eq!(
        (moved.area.width(), moved.area.height()),
        (1.0, 1.0),
        "it is as big as it was"
    );

    // Taken as far as it will go to the bottom right, its middle is at that corner of the picture.
    let middle = middle_of_the_layer(&window);
    window.drag(middle, middle + vec2(4000.0, 4000.0), PointerButton::Primary);
    let at_the_far_corner = placement_of(&window, video);
    assert!(
        (at_the_far_corner.area.left() - 0.5).abs() < 1e-3,
        "{at_the_far_corner:?}"
    );
    assert!(
        (at_the_far_corner.area.top() - 0.5).abs() < 1e-3,
        "{at_the_far_corner:?}"
    );

    // And the other way, as far as the top left corner.
    let middle = middle_of_the_layer(&window);
    window.drag(middle, middle - vec2(4000.0, 4000.0), PointerButton::Primary);
    let at_the_near_corner = placement_of(&window, video);
    assert!(
        (at_the_near_corner.area.left() + 0.5).abs() < 1e-3,
        "{at_the_near_corner:?}"
    );
    assert!(
        (at_the_near_corner.area.top() + 0.5).abs() < 1e-3,
        "{at_the_near_corner:?}"
    );
    assert_eq!(at_the_near_corner.rotation, Angle::ZERO);
}

#[test]
fn a_video_that_fills_the_picture_can_be_made_smaller_by_a_corner_and_keeps_its_shape() {
    let (mut window, video) = window_with_the_video_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let top_left = corner_position(&window, Corner::TopLeft);

    window.drag(corner, corner + (top_left - corner) * 0.5, PointerButton::Primary);

    let after = placement_of(&window, video);
    assert!((after.area.width() - 0.5).abs() < 0.02, "{after:?}");
    assert!(
        (after.area.height() - 0.5).abs() < 0.02,
        "the shape of the video is kept: {after:?}"
    );
    assert!(
        after.area.left().abs() < 1e-3 && after.area.top().abs() < 1e-3,
        "the corner across stays: {after:?}"
    );
    assert_eq!(after.rotation, Angle::ZERO);
}

fn placement_of_the_video_or_none(window: &Window, video: ClipId) -> Option<Placement> {
    window
        .application()
        .project()
        .placement_of(video)
        .unwrap()
        .clip
        .placement()
}

#[test]
fn a_video_that_was_made_smaller_can_be_moved_and_then_turned() {
    let (mut window, video) = window_with_the_video_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let top_left = corner_position(&window, Corner::TopLeft);
    window.drag(corner, corner + (top_left - corner) * 0.5, PointerButton::Primary);
    let before = placement_of(&window, video);

    let middle = middle_of_the_layer(&window);
    window.drag(middle, middle + vec2(60.0, 40.0), PointerButton::Primary);
    let moved = placement_of(&window, video);
    assert!(moved.area.left() > before.area.left() + 0.02 && moved.area.top() > before.area.top() + 0.02);
    assert_eq!(
        (moved.area.width(), moved.area.height()),
        (before.area.width(), before.area.height())
    );

    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopRight, 18.0);
    window.drag(start, turned_about(start, middle, -25.0), PointerButton::Primary);
    assert_degrees(
        placement_of(&window, video).rotation,
        -25.0,
        "a video is turned like text",
    );
}

#[test]
fn a_video_that_is_turned_keeps_its_shape_when_a_corner_is_dragged() {
    let (mut window, video) = window_with_the_video_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let top_left = corner_position(&window, Corner::TopLeft);
    window.drag(corner, corner + (top_left - corner) * 0.5, PointerButton::Primary);
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.drag(start, turned_about(start, middle, 30.0), PointerButton::Primary);
    let before = placement_of(&window, video);

    // Drag a corner along but not along the sides: a text would take that shape, a video does not.
    let corner = corner_position(&window, Corner::BottomRight);
    let across = corner_position(&window, Corner::TopLeft);
    let off_the_diagonal = corner + (across - corner) * 0.3 + vec2(0.0, 25.0);
    window.drag(corner, off_the_diagonal, PointerButton::Primary);

    let after = placement_of(&window, video);
    let shape = |place: Placement| place.area.width() / place.area.height();
    assert!(
        (shape(after) - shape(before)).abs() < 0.01 * shape(before),
        "{before:?} then {after:?}: the shape is the same"
    );
    assert_eq!(after.rotation, before.rotation);
    assert!(after.area.width() < before.area.width() - 0.01, "and it is smaller");
}

#[test]
fn text_on_the_preview_takes_any_shape_that_a_corner_is_dragged_to() {
    let (mut window, text) = window_with_a_text_object_selected();
    let before = placement_of(&window, text);
    let corner = corner_position(&window, Corner::BottomRight);

    window.drag(corner, corner + vec2(-60.0, 0.0), PointerButton::Primary);

    let after = placement_of(&window, text);
    assert!(after.area.width() < before.area.width() - 0.02);
    assert!(
        (after.area.height() - before.area.height()).abs() < 1e-3,
        "only one way changed: {after:?}"
    );
}

#[test]
fn a_video_that_fills_the_picture_has_no_outline_or_hand_when_the_pointer_is_over_it_until_it_is_picked() {
    let project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let mut window = Window::showing(project);
    let picture = window.rectangle_of(&video_label(video));
    window.move_pointer_to(picture.center());
    window.harness.step();
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Default,
        "nothing to grab: it is all of the picture"
    );
    assert_eq!(window.application().selected_clip(), None);

    window.click(picture.center());
    window.move_pointer_to(picture.center() + vec2(1.0, 1.0));
    window.harness.step();
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Grab,
        "once it is selected it can be grabbed"
    );
}

#[test]
fn the_symbol_can_still_be_grabbed_over_a_video_that_fills_the_picture() {
    // The symbol is over the front-most video, so the video does not take the pointer from it.
    let mut project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    project
        .set_clip_speed(video, crate::project::SpeedFactor::new(4).unwrap())
        .unwrap();
    let mut window = Window::showing(project);
    window.move_current_frame_to(10);
    let symbol = window.rectangle_of("Fast-forward symbol");
    let before = window.application().project().speed_indicator_layout();

    window.drag(
        symbol.center(),
        symbol.center() + vec2(-40.0, -10.0),
        PointerButton::Primary,
    );

    let after = window.application().project().speed_indicator_layout();
    assert!(
        after.left_fraction_of_frame_width() < before.left_fraction_of_frame_width() - 0.01,
        "{before:?} then {after:?}"
    );
    assert_eq!(
        window.application().selected_clip(),
        None,
        "the video was not picked by it"
    );
}

#[test]
fn text_in_front_of_a_video_is_picked_by_a_click_on_it_and_the_video_by_a_click_beside_it() {
    let (mut window, text) = window_with_a_text_object_selected();
    window.do_this(Command::Deselect);
    let text_body = window.rectangle_of(&text_object_label(text));
    window.click(text_body.center());
    assert_eq!(window.application().selected_clip(), Some(text));

    let video = window.application().project().tracks()[1].entries()[0].id();
    let beside_the_text = window.rectangle_of(&video_label(video)).center() - vec2(0.0, 100.0);
    window.click(beside_the_text);
    assert_eq!(window.application().selected_clip(), Some(video));
}

#[test]
fn a_layer_grabbed_by_its_edge_is_dragged_even_when_the_pointer_goes_into_the_layer_in_front_that_touches_it() {
    // Two text objects: one at the left, and in front of it another that begins where it ends.
    let mut project = project_of(300);
    let behind_track = project.add_track_in_front();
    let at_the_left = TextObject {
        area: CanvasArea::new(0.05, 0.4, 0.3, 0.2),
        ..TextObject::default()
    };
    let (behind, _) = project
        .insert_text(behind_track, TimelineFrameIndex::new(0), at_the_left, 200)
        .unwrap();
    let in_front_track = project.add_track_in_front();
    let touching_it = TextObject {
        area: CanvasArea::new(0.35, 0.3, 0.3, 0.4),
        ..TextObject::default()
    };
    let (in_front, _) = project
        .insert_text(in_front_track, TimelineFrameIndex::new(0), touching_it, 200)
        .unwrap();
    let mut window = Window::showing(project);
    let behind_rectangle = window.rectangle_of(&text_object_label(behind));
    let picture_width = behind_rectangle.width() / 0.3;
    let behind_before = placement_of(&window, behind);
    let in_front_before = placement_of(&window, in_front);
    let grabbed_at = pos2(behind_rectangle.right() - 2.0, behind_rectangle.center().y);
    // A little way in is less than it takes for the pointer to have gone far enough to be dragging.
    let a_little_way_into_the_one_in_front = grabbed_at + vec2(4.0, 0.0);
    let far_on = a_little_way_into_the_one_in_front + vec2(0.5 * picture_width, 0.0);

    window.move_pointer_to(grabbed_at);
    window.press(grabbed_at, PointerButton::Primary);
    window.move_pointer_to(a_little_way_into_the_one_in_front);
    window.move_pointer_to(far_on);
    window.release(far_on, PointerButton::Primary);
    window.settle();

    let behind_after = placement_of(&window, behind);
    assert!(
        behind_after.area.left() > behind_before.area.left() + 0.4,
        "it went with the pointer: {behind_before:?} then {behind_after:?}"
    );
    assert_eq!(
        placement_of(&window, in_front),
        in_front_before,
        "and the one in front, which the pointer went into, was not moved"
    );
    assert_eq!(window.application().selected_clip(), Some(behind));
}

// --- reaching out of the picture, and over the other panels ---
//
// The box of the selected video (its outline, its corners, the places to turn it from) reaches out of the picture, and is drawn and
// taken hold of wherever it is over the preview, but it stops at the edges of the preview: it is not drawn over the other panels,
// and what is in them works as it does without it.

/// A window with the video selected and put where `area` is, not turned.
fn window_with_the_video_put_at(area: CanvasArea) -> (Window, ClipId) {
    let (mut window, video) = window_with_the_video_selected();
    window.do_this(Command::SetClipPlacement {
        clip: video,
        placement: Placement::upright(area),
    });
    (window, video)
}

/// A box so far before the picture (which fills the width of the preview) that its left corners are over the
/// properties panel.
fn far_before_the_picture() -> CanvasArea {
    CanvasArea::new(-0.2, 0.3, 0.6, 0.4)
}

/// A window with the video selected and put so that its bottom left corner is over an empty place of the properties panel, which is
/// not over the picture, and is not over anything that the properties panel has to be used with: the video, and the place.
/// (That the place is empty is checked by dragging from it with nothing selected, which changes nothing.)
fn window_with_a_corner_over_an_empty_place_of_the_properties_panel() -> (Window, ClipId, Pos2) {
    let (mut window, video) = window_with_the_video_selected();
    let properties = window.the_properties_panel();
    let place = pos2(properties.center().x, properties.bottom() - 40.0);
    let picture = picture_rectangle_on_the_screen(&window);
    window.do_this(Command::SetClipPlacement {
        clip: video,
        placement: Placement::upright(CanvasArea::new(
            (place.x - picture.left()) / picture.width(),
            (place.y - picture.top()) / picture.height() - 0.4,
            0.8,
            0.4,
        )),
    });
    assert!(
        properties.contains(place) && corner_position(&window, Corner::BottomLeft).distance(place) < 2.0,
        "the setup: the corner is at the place, over the properties panel: {place:?}"
    );

    window.do_this(Command::Deselect);
    let project_before = window.application().project().clone();
    window.drag(place, place + vec2(60.0, -30.0), PointerButton::Primary);
    assert!(
        *window.application().project() == project_before,
        "the setup: nothing of the properties panel is at the place, which a drag from it would have changed"
    );
    select(&mut window, video);
    (window, video, place)
}

/// What the speed of the clip is.
fn speed_of(window: &Window, clip: ClipId) -> crate::project::SpeedFactor {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .as_media()
        .unwrap()
        .speed()
}

/// Whether the sound of the clip is switched off.
fn is_muted(window: &Window, clip: ClipId) -> bool {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .as_media()
        .unwrap()
        .is_muted()
}

#[test]
fn a_corner_of_the_selected_video_that_is_over_the_properties_panel_is_not_there_to_be_taken_hold_of() {
    let (mut window, video, handle) = window_with_a_corner_over_an_empty_place_of_the_properties_panel();
    let before = placement_of(&window, video);

    // Towards the middle of the video: would make it smaller, if the corner were taken hold of.
    window.drag(handle, handle + vec2(60.0, -30.0), PointerButton::Primary);

    assert_eq!(placement_of(&window, video), before, "the video is as it was");
}

#[test]
fn a_control_of_the_properties_panel_that_a_corner_of_the_selected_video_is_over_takes_the_press_and_the_video_is_not_resized()
 {
    let (mut window, video) = window_with_the_video_selected();
    let speed_slider = window.rectangle_of("Speed").center();
    let picture = picture_rectangle_on_the_screen(&window);
    // The video is put so that its top left corner is right on the speed slider.
    let area = CanvasArea::new(
        (speed_slider.x - picture.left()) / picture.width(),
        (speed_slider.y - picture.top()) / picture.height(),
        0.6,
        0.4,
    );
    window.do_this(Command::SetClipPlacement {
        clip: video,
        placement: Placement::upright(area),
    });
    let handle = corner_position(&window, Corner::TopLeft);
    assert!(
        handle.distance(speed_slider) < 2.0,
        "the setup: the corner is on the slider: {handle:?}, {speed_slider:?}"
    );
    let before = placement_of(&window, video);
    let speed_before = speed_of(&window, video);

    window.drag(handle, handle + vec2(20.0, 20.0), PointerButton::Primary);

    assert_ne!(speed_of(&window, video), speed_before, "the slider took the press");
    assert_eq!(placement_of(&window, video), before, "and the corner over it did not");
}

#[test]
fn a_place_to_turn_from_that_is_over_the_properties_panel_is_not_there_to_turn_the_selected_video_from() {
    let (mut window, video, _handle) = window_with_a_corner_over_an_empty_place_of_the_properties_panel();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomLeft, 18.0);
    assert!(
        window.the_properties_panel().contains(start),
        "the setup: it is over the properties panel: {start:?}"
    );
    let before = placement_of(&window, video);

    window.drag(start, turned_about(start, middle, 20.0), PointerButton::Primary);

    assert_eq!(placement_of(&window, video), before, "the video is as it was");
}

#[test]
fn the_box_of_the_selected_video_is_cut_off_where_the_preview_ends_and_none_of_it_is_drawn_over_the_other_panels() {
    let (window, _video) = window_with_the_video_put_at(far_before_the_picture());
    let handle = corner_position(&window, Corner::TopLeft);
    let (properties, preview, timeline) = (
        window.the_properties_panel(),
        window.the_preview_panel(),
        window.the_timeline_panel(),
    );
    assert!(
        properties.contains(handle),
        "the setup: the corner is over the properties panel: {handle:?}"
    );
    let drawn = window.clipped_shapes_drawn();

    // What is drawn of the box is what is of the colour of its outline, and what is seen of it is where it is not cut off.
    let parts_of_the_box: Vec<(egui::Rect, egui::Rect)> = drawn
        .iter()
        .filter(|(_, shape)| match shape {
            Shape::Path(path) => path.closed && path.stroke.color == ColorMode::Solid(theme::SELECTION_OUTLINE),
            Shape::Rect(square) => square.fill == theme::SELECTION_OUTLINE,
            _ => false,
        })
        .map(|(cut_off_at, shape)| (*cut_off_at, cut_off_at.intersect(shape.visual_bounding_rect())))
        .collect();
    assert_eq!(
        parts_of_the_box.len(),
        5,
        "the outline and the four squares at the corners"
    );
    for (cut_off_at, seen) in parts_of_the_box {
        assert!(
            preview.expand(0.5).contains_rect(cut_off_at),
            "it is cut off at the edge of the preview: {cut_off_at:?}, the preview is {preview:?}"
        );
        for other in [properties, timeline] {
            assert!(
                !seen.intersects(other),
                "none of it is seen over another panel: {seen:?} is over {other:?}"
            );
        }
    }
}

#[test]
fn what_is_in_the_properties_panel_still_works_under_the_box_of_the_selected_video_and_under_a_corner_of_it() {
    let (mut window, video) = window_with_the_video_put_at(far_before_the_picture());
    let mute = window.rectangle_of("Mute").center();
    // (A corner is put right over the box that is ticked: a box that is wide enough to be put so far before the picture, which
    // keeps the middle of a box on it.)
    let picture = picture_rectangle_on_the_screen(&window);
    window.do_this(Command::SetClipPlacement {
        clip: video,
        placement: Placement::upright(CanvasArea::new(
            (mute.x - picture.left()) / picture.width(),
            (mute.y - picture.top()) / picture.height(),
            0.8,
            0.4,
        )),
    });
    assert!(
        corner_position(&window, Corner::TopLeft).distance(mute) < 2.0,
        "the setup: a corner is over the box that is ticked: {:?}, {mute:?}",
        corner_position(&window, Corner::TopLeft),
    );
    assert!(!is_muted(&window, video));

    window.click(mute);

    assert!(is_muted(&window, video), "the box was ticked");
}

#[test]
fn a_selected_video_is_moved_by_a_point_of_it_that_is_near_a_corner_but_not_on_the_handle() {
    // That point is in the square round the corner that takes the mouse to turn the layer, which is over
    // the preview as a whole, so that what moves the layer is not the widget that the press came to.
    let (mut window, video) = window_with_the_video_put_at(CanvasArea::new(0.2, 0.3, 0.5, 0.4));
    let near_the_corner = corner_position(&window, Corner::TopLeft) + vec2(20.0, 20.0);
    let before = placement_of(&window, video);

    window.drag(
        near_the_corner,
        near_the_corner + vec2(30.0, 20.0),
        PointerButton::Primary,
    );

    let after = placement_of(&window, video);
    assert!(
        after.area.left() > before.area.left() + 0.02 && after.area.top() > before.area.top() + 0.02,
        "{before:?} then {after:?}"
    );
    assert_eq!(
        (after.area.width(), after.area.height()),
        (before.area.width(), before.area.height())
    );
}

#[test]
fn what_is_drawn_of_a_video_that_reaches_out_of_the_picture_is_cut_off_at_the_edge_of_the_picture() {
    let (window, _video) = window_with_the_video_put_at(far_before_the_picture());
    let corners = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomRight,
        Corner::BottomLeft,
    ]
    .map(|corner| corner_position(&window, corner));
    let drawn = window.clipped_shapes_drawn();
    // (The video has no picture yet in the test window: it is drawn as a plain box.)
    let (cut_off_at, _) = drawn
        .iter()
        .find(|(_, shape)| {
            matches!(shape, Shape::Path(path)
                if path.closed
                    && path.fill == theme::TRACK_BACKGROUND
                    && path.points.len() == 4
                    && path.points.iter().zip(corners).all(|(point, corner)| point.distance(corner) < 1.0))
        })
        .expect("the video is drawn");
    assert_eq!(*cut_off_at, picture_rectangle_on_the_screen(&window));
}

/// A window with a video that stands up (so that the preview has room at both sides of the picture), which is
/// selected, and put where `area` is, not turned.
fn window_with_a_standing_video_put_at(area: CanvasArea) -> (Window, ClipId) {
    let project = portrait_project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let mut window = Window::showing(project);
    select(&mut window, video);
    window.do_this(Command::SetClipPlacement {
        clip: video,
        placement: Placement::upright(area),
    });
    (window, video)
}

/// A point on the left side of the box of the selected layer, `inwards` points in from it.
fn on_the_left_side(window: &Window, inwards: f32) -> Pos2 {
    let top_left = corner_position(window, Corner::TopLeft);
    let bottom_left = corner_position(window, Corner::BottomLeft);
    pos2(top_left.x + inwards, (top_left.y + bottom_left.y) / 2.0)
}

#[test]
fn the_selected_video_is_moved_by_taking_hold_of_it_where_it_is_before_the_picture_but_over_the_preview() {
    let (mut window, video) = window_with_a_standing_video_put_at(CanvasArea::new(-0.5, 0.3, 0.8, 0.4));
    let before_the_picture = on_the_left_side(&window, 40.0);
    assert!(
        before_the_picture.x < picture_rectangle_on_the_screen(&window).left()
            && window.the_preview_panel().contains(before_the_picture),
        "the setup: the point is before the picture, over the preview: {before_the_picture:?}"
    );
    let before = placement_of(&window, video);

    window.drag(
        before_the_picture,
        before_the_picture + vec2(30.0, 0.0),
        PointerButton::Primary,
    );

    let after = placement_of(&window, video);
    assert!(
        after.area.left() > before.area.left() + 0.02,
        "{before:?} then {after:?}"
    );
    assert_eq!(
        (after.area.top(), after.area.width()),
        (before.area.top(), before.area.width())
    );
}

#[test]
fn a_video_that_is_not_selected_is_not_taken_hold_of_where_it_is_before_the_picture() {
    let (mut window, video) = window_with_a_standing_video_put_at(CanvasArea::new(-0.5, 0.3, 0.8, 0.4));
    let before_the_picture = on_the_left_side(&window, 40.0);
    window.do_this(Command::Deselect);
    let before = placement_of(&window, video);

    window.drag(
        before_the_picture,
        before_the_picture + vec2(30.0, 0.0),
        PointerButton::Primary,
    );

    assert_eq!(placement_of(&window, video), before, "it did not move");
    assert_eq!(window.application().selected_clip(), None, "and it was not picked");
}

#[test]
fn a_corner_of_the_selected_video_that_is_over_the_preview_but_not_over_the_picture_is_taken_hold_of_there() {
    let (mut window, video) = window_with_a_standing_video_put_at(CanvasArea::new(-0.5, 0.3, 0.8, 0.4));
    let handle = corner_position(&window, Corner::TopLeft);
    assert!(
        handle.x < picture_rectangle_on_the_screen(&window).left() && window.the_preview_panel().contains(handle),
        "the setup: {handle:?}"
    );
    let before = placement_of(&window, video);

    window.drag(handle, handle + vec2(40.0, 40.0), PointerButton::Primary);

    let after = placement_of(&window, video);
    assert!(
        after.area.width() < before.area.width() - 0.02,
        "{before:?} then {after:?}"
    );
}

#[test]
fn a_corner_over_the_preview_but_not_the_picture_pressed_in_the_frame_of_the_move_that_brought_the_pointer_to_it_is_taken_hold_of()
 {
    let (mut window, video) = window_with_a_standing_video_put_at(CanvasArea::new(-0.5, 0.3, 0.8, 0.4));
    let handle = corner_position(&window, Corner::TopLeft);
    assert!(
        handle.x < picture_rectangle_on_the_screen(&window).left() && window.the_preview_panel().contains(handle),
        "the setup: {handle:?}"
    );
    let before = placement_of(&window, video);
    point_at(&mut window, handle + vec2(100.0, 100.0));

    window.all_in_one_frame([
        Event::PointerMoved(handle),
        button_pressed_at(handle, PointerButton::Primary),
    ]);
    for step in 1..=6 {
        window.move_pointer_to(handle + vec2(10.0 * step as f32, 5.0 * step as f32));
    }
    window.release(handle + vec2(60.0, 30.0), PointerButton::Primary);
    window.settle();

    let after = placement_of(&window, video);
    assert!(
        after.area.width() < before.area.width() - 0.02,
        "the corner was taken hold of: {before:?} then {after:?}"
    );
}

#[test]
fn a_place_to_turn_from_that_is_over_the_preview_but_not_the_picture_turns_the_selected_video() {
    let (mut window, video) = window_with_a_standing_video_put_at(CanvasArea::new(-0.5, 0.3, 0.8, 0.4));
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopLeft, 18.0);
    assert!(
        start.x < picture_rectangle_on_the_screen(&window).left() && window.the_preview_panel().contains(start),
        "the setup: it is before the picture, over the preview: {start:?}"
    );

    window.drag(start, turned_about(start, middle, 20.0), PointerButton::Primary);

    assert_degrees(
        placement_of(&window, video).rotation,
        20.0,
        "turned from a place that is over the preview and not over the picture",
    );
}

#[test]
fn the_box_of_the_selected_video_is_drawn_whole_where_it_reaches_out_of_the_picture_over_the_preview() {
    let (window, _video) = window_with_a_standing_video_put_at(CanvasArea::new(-0.5, 0.3, 0.8, 0.4));
    let handle = corner_position(&window, Corner::TopLeft);
    let preview = window.the_preview_panel();
    assert!(
        handle.x < picture_rectangle_on_the_screen(&window).left() && preview.contains(handle),
        "the setup: the corner is before the picture, over the preview: {handle:?}"
    );

    let drawn = window.clipped_shapes_drawn();
    let (cut_off_at, square) = drawn
        .iter()
        .find(|(_, shape)| {
            matches!(shape, Shape::Rect(square) if square.fill == theme::SELECTION_OUTLINE && square.rect.center().distance(handle) < 1.0)
        })
        .expect("the square at the corner is drawn");
    assert!(
        cut_off_at.contains_rect(square.visual_bounding_rect()),
        "the square at the corner that is before the picture is drawn whole: {cut_off_at:?}"
    );
    let outline = drawn
        .iter()
        .find(|(_, shape)| {
            matches!(shape, Shape::Path(path) if path.closed && path.stroke.color == ColorMode::Solid(theme::SELECTION_OUTLINE))
        })
        .expect("the outline of the selected video is drawn");
    assert!(
        outline.0.contains(handle),
        "and so is the outline: it is not cut off at the edge of the picture: {:?}",
        outline.0
    );
}

/// The picture on the screen: the black rectangle that the layers are drawn on.
pub(super) fn picture_rectangle_on_the_screen(window: &Window) -> egui::Rect {
    window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == Color32::BLACK => Some(rectangle.rect),
            _ => None,
        })
        .expect("the picture is drawn")
}

// --- what the pointer shows it can take hold of ---

fn cursor_shown(window: &Window) -> CursorIcon {
    window.harness.output().platform_output.cursor_icon
}

/// Moves the pointer to `position` and lets one more frame go by.
fn point_at(window: &mut Window, position: Pos2) {
    window.move_pointer_to(position);
    window.harness.step();
}

/// How many closed lines of `colour` were drawn in the last frame: the outlines of layers.
fn outlines_drawn_in(window: &Window, colour: Color32) -> usize {
    window
        .shapes_drawn()
        .into_iter()
        .filter(
            |shape| matches!(shape, Shape::Path(path) if path.closed && path.stroke.color == ColorMode::Solid(colour)),
        )
        .count()
}

#[test]
fn the_pointer_is_a_hand_over_a_layer_a_resize_arrow_over_a_corner_a_cross_just_outside_it_and_an_arrow_when_over_nothing()
 {
    let (mut window, _text) = window_with_a_text_object_in_the_middle_selected();
    let middle = middle_of_the_layer(&window);
    let corner = corner_position(&window, Corner::BottomRight);
    let outside = just_outside(&window, Corner::BottomRight, 18.0);
    let far_from_the_text = middle + vec2(0.0, -120.0);

    point_at(&mut window, middle);
    assert_eq!(cursor_shown(&window), CursorIcon::Grab, "over the text");
    point_at(&mut window, corner);
    // (A box four times as wide as it is tall has a corner that is nearer straight across from its middle
    // than diagonal, so the arrow is the one that points across.)
    assert_eq!(cursor_shown(&window), CursorIcon::ResizeHorizontal, "over its corner");
    point_at(&mut window, outside);
    assert_eq!(cursor_shown(&window), CursorIcon::Crosshair, "just outside its corner");
    point_at(&mut window, far_from_the_text);
    assert_eq!(
        cursor_shown(&window),
        CursorIcon::Default,
        "over the video, which is all of the picture and not picked"
    );
}

#[test]
fn while_something_is_dragged_the_pointer_is_a_closed_hand_or_the_arrow_of_the_corner_or_a_cross() {
    let (mut window, _text) = window_with_a_text_object_in_the_middle_selected();
    let middle = middle_of_the_layer(&window);
    window.drag_and_hold(middle, middle + vec2(30.0, 0.0), PointerButton::Primary);
    assert_eq!(cursor_shown(&window), CursorIcon::Grabbing, "moving");
    window.release(middle + vec2(30.0, 0.0), PointerButton::Primary);

    let corner = corner_position(&window, Corner::BottomRight);
    window.drag_and_hold(corner, corner + vec2(-20.0, -10.0), PointerButton::Primary);
    assert_eq!(cursor_shown(&window), CursorIcon::ResizeHorizontal, "resizing");
    window.release(corner + vec2(-20.0, -10.0), PointerButton::Primary);

    let middle = middle_of_the_layer(&window);
    let outside = just_outside(&window, Corner::TopRight, 18.0);
    window.drag_and_hold(outside, turned_about(outside, middle, 20.0), PointerButton::Primary);
    assert_eq!(cursor_shown(&window), CursorIcon::Crosshair, "turning");
}

#[test]
fn a_layer_the_pointer_is_over_is_outlined_unless_it_is_all_of_the_picture_and_not_picked() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let outline = theme::INDICATOR_OUTLINE;

    window.do_this(Command::Deselect);
    let middle = window.rectangle_of(&text_object_label(text)).center();
    point_at(&mut window, middle);
    assert_eq!(outlines_drawn_in(&window, outline), 1, "the text is outlined");

    point_at(&mut window, middle + vec2(0.0, -120.0));
    assert_eq!(
        outlines_drawn_in(&window, outline),
        0,
        "the video that is all of the picture is not, until it is picked"
    );

    select(&mut window, text);
    point_at(&mut window, middle);
    assert_eq!(
        outlines_drawn_in(&window, outline),
        1,
        "and the picked text is outlined as it is selected as well as hovered"
    );
}

#[test]
fn a_ring_is_drawn_round_the_corner_that_turns_the_layer_when_the_pointer_is_over_the_place_to_turn_from() {
    let (mut window, _text) = window_with_a_text_object_in_the_middle_selected();
    let circles = |window: &Window| {
        window
            .shapes_drawn()
            .into_iter()
            .filter(|shape| matches!(shape, Shape::Circle(_)))
            .count()
    };
    let middle = middle_of_the_layer(&window);
    let outside = just_outside(&window, Corner::BottomLeft, 18.0);
    point_at(&mut window, middle);
    let without_the_ring = circles(&window);
    point_at(&mut window, outside);
    assert_eq!(circles(&window), without_the_ring + 1);
}

// --- what the preview takes from the mouse and what it leaves alone ---

#[test]
fn a_layer_that_is_dragged_goes_exactly_as_far_as_the_pointer_does() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let picture = window.rectangle_of(&video_label(window.clip(1, 0)));
    let before = placement_of(&window, text);
    let middle = middle_of_the_layer(&window);

    window.drag(middle, middle + vec2(80.0, 30.0), PointerButton::Primary);

    let after = placement_of(&window, text);
    assert!(
        (after.area.left() - before.area.left() - 80.0 / picture.width()).abs() < 0.002,
        "{before:?} then {after:?}"
    );
    assert!(
        (after.area.top() - before.area.top() - 30.0 / picture.height()).abs() < 0.002,
        "{before:?} then {after:?}"
    );
}

#[test]
fn nothing_on_the_preview_reacts_to_the_pointer_while_a_dialog_is_over_it() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    window.do_this(Command::Deselect);
    let middle = window.rectangle_of(&text_object_label(text)).center();
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    point_at(&mut window, middle);
    assert_eq!(cursor_shown(&window), CursorIcon::Default);
    assert_eq!(outlines_drawn_in(&window, theme::INDICATOR_OUTLINE), 0);
    window.click(middle);
    assert_eq!(window.application().selected_clip(), None);
}

#[test]
fn only_the_left_button_picks_and_drags_layers() {
    let (mut window, text) = window_with_a_text_object_selected();
    window.do_this(Command::Deselect);
    let middle = window.rectangle_of(&text_object_label(text)).center();
    let before = placement_of(&window, text);

    window.drag(middle, middle + vec2(80.0, 0.0), PointerButton::Secondary);
    window.right_click(middle);

    assert_eq!(placement_of(&window, text), before, "the text did not move");
    assert_eq!(window.application().selected_clip(), None, "and was not picked");
}

#[test]
fn a_layer_at_the_edge_of_the_picture_is_turned_from_outside_the_picture() {
    let mut project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let front = project.add_track_in_front();
    let at_the_left_edge = TextObject {
        area: CanvasArea::new(0.0, 0.4, 0.2, 0.2),
        ..TextObject::default()
    };
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), at_the_left_edge, 200)
        .unwrap();
    let mut window = Window::showing(project);
    select(&mut window, text);
    let picture = window.rectangle_of(&video_label(video));
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopLeft, 18.0);
    assert!(
        start.x < picture.left(),
        "the place to turn it from is beside the picture"
    );

    window.drag(start, turned_about(start, middle, 40.0), PointerButton::Primary);

    assert_degrees(
        placement_of(&window, text).rotation,
        40.0,
        "it was turned from where there is no picture",
    );
}

#[test]
fn a_press_or_a_click_that_is_beside_the_picture_changes_nothing() {
    let (mut window, video) = window_with_the_video_selected();
    let before = window.application().project().clone();
    let picture = window.rectangle_of(&video_label(video));
    let beside_the_picture = pos2(picture.left() - 100.0, picture.center().y);

    window.click(beside_the_picture);
    window.drag(
        beside_the_picture,
        beside_the_picture + vec2(50.0, 30.0),
        PointerButton::Primary,
    );

    assert_eq!(window.application().project(), &before);
    assert_eq!(
        window.application().selected_clip(),
        Some(video),
        "the video is still picked"
    );
}

// --- input that comes in one frame, as a tap or a quick click gives it ---

/// A window with the text in front of the video, nothing selected and the pointer over the video
/// above the text. It gives the text and the middle of it.
fn window_with_the_pointer_over_the_video_above_the_text() -> (Window, ClipId, Pos2) {
    let (mut window, text) = window_with_a_text_object_selected();
    window.do_this(Command::Deselect);
    let text_rectangle = window.rectangle_of(&text_object_label(text));
    let above_the_text = text_rectangle.center() + vec2(0.0, -60.0);
    assert!(!text_rectangle.contains(above_the_text));
    point_at(&mut window, above_the_text);
    (window, text, text_rectangle.center())
}

#[test]
fn a_click_in_the_frame_of_the_move_that_brought_the_pointer_to_the_text_picks_the_text() {
    let (mut window, text, middle_of_the_text) = window_with_the_pointer_over_the_video_above_the_text();

    window.all_in_one_frame([
        Event::PointerMoved(middle_of_the_text),
        button_pressed_at(middle_of_the_text, PointerButton::Primary),
        button_released_at(middle_of_the_text, PointerButton::Primary),
    ]);
    window.settle();

    assert_eq!(window.application().selected_clip(), Some(text));
}

#[test]
fn a_press_in_the_frame_of_the_move_that_brought_the_pointer_to_the_text_drags_the_text() {
    let (mut window, text, middle_of_the_text) = window_with_the_pointer_over_the_video_above_the_text();
    let before = placement_of(&window, text);

    window.all_in_one_frame([
        Event::PointerMoved(middle_of_the_text),
        button_pressed_at(middle_of_the_text, PointerButton::Primary),
    ]);
    for step in 1..=6 {
        window.move_pointer_to(middle_of_the_text + vec2(10.0 * step as f32, 0.0));
    }
    window.release(middle_of_the_text + vec2(60.0, 0.0), PointerButton::Primary);
    window.settle();

    assert!(
        placement_of(&window, text).area.left() > before.area.left() + 0.03,
        "it went to the right with the pointer"
    );
    assert_eq!(window.application().selected_clip(), Some(text));
}

#[test]
fn a_tap_on_the_text_by_a_finger_that_was_not_over_the_window_before_picks_the_text() {
    let (mut window, text, middle_of_the_text) = window_with_the_pointer_over_the_video_above_the_text();
    window.harness.event(Event::PointerGone);
    window.harness.step();

    window.all_in_one_frame([
        Event::PointerMoved(middle_of_the_text),
        button_pressed_at(middle_of_the_text, PointerButton::Primary),
        button_released_at(middle_of_the_text, PointerButton::Primary),
    ]);
    window.settle();

    assert_eq!(window.application().selected_clip(), Some(text));
}

#[test]
fn a_corner_pressed_in_the_frame_of_the_move_that_brought_the_pointer_to_it_is_dragged_to_resize() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let before = placement_of(&window, text);
    let over_the_video = middle_of_the_layer(&window) + vec2(0.0, -120.0);
    point_at(&mut window, over_the_video);

    window.all_in_one_frame([
        Event::PointerMoved(corner),
        button_pressed_at(corner, PointerButton::Primary),
    ]);
    for step in 1..=6 {
        window.move_pointer_to(corner + vec2(-10.0 * step as f32, 0.0));
    }
    window.release(corner + vec2(-60.0, 0.0), PointerButton::Primary);
    window.settle();

    assert!(placement_of(&window, text).area.width() < before.area.width() - 0.03);
}

#[test]
fn a_place_to_turn_from_pressed_in_the_frame_of_the_move_that_brought_the_pointer_to_it_turns_the_layer() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::TopRight, 18.0);
    point_at(&mut window, middle + vec2(0.0, -120.0));

    window.all_in_one_frame([
        Event::PointerMoved(start),
        button_pressed_at(start, PointerButton::Primary),
    ]);
    for step in 1..=6 {
        window.move_pointer_to(turned_about(start, middle, 5.0 * step as f32));
    }
    window.release(turned_about(start, middle, 30.0), PointerButton::Primary);
    window.settle();

    assert_degrees(placement_of(&window, text).rotation, 30.0, "turned with the pointer");
}

#[test]
fn the_symbol_pressed_in_the_frame_of_the_move_that_brought_the_pointer_to_it_is_dragged() {
    let mut window = window_inside_a_sped_up_clip();
    let symbol = window.rectangle_of("Fast-forward symbol");
    let before = window.application().project().speed_indicator_layout();
    point_at(&mut window, symbol.center() + vec2(0.0, 150.0));

    window.all_in_one_frame([
        Event::PointerMoved(symbol.center()),
        button_pressed_at(symbol.center(), PointerButton::Primary),
    ]);
    for step in 1..=6 {
        window.move_pointer_to(symbol.center() + vec2(10.0 * step as f32, 0.0));
    }
    window.release(symbol.center() + vec2(60.0, 0.0), PointerButton::Primary);
    window.settle();

    let after = window.application().project().speed_indicator_layout();
    assert!(
        after.left_fraction_of_frame_width() > before.left_fraction_of_frame_width() + 0.02,
        "{before:?} then {after:?}"
    );
    assert_eq!(
        window.application().selected_clip(),
        None,
        "and the video was not picked"
    );
}

#[test]
fn the_area_of_a_placement_is_never_bigger_than_the_picture_and_has_its_middle_on_it_whatever_is_done_to_it() {
    let (mut window, text) = window_with_a_text_object_selected();
    let middle = middle_of_the_layer(&window);
    let start = just_outside(&window, Corner::BottomRight, 18.0);
    window.drag(start, turned_about(start, middle, 77.0), PointerButton::Primary);
    let corner = corner_position(&window, Corner::BottomRight);
    window.drag(corner, corner + vec2(3000.0, 3000.0), PointerButton::Primary);
    let area = placement_of(&window, text).area;
    assert!(area.width() <= 1.0 + 1e-4 && area.height() <= 1.0 + 1e-4, "{area:?}");
    let (middle_across, middle_down) = (area.left() + area.width() / 2.0, area.top() + area.height() / 2.0);
    assert!(
        (-1e-4..=1.0 + 1e-4).contains(&middle_across) && (-1e-4..=1.0 + 1e-4).contains(&middle_down),
        "{area:?}"
    );
}

// --- held to the edges of the picture with Shift ---

#[derive(Clone, Copy, Debug)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

const EVERY_EDGE: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];

/// The picture as it is on the screen.
fn the_picture_on_the_screen(window: &Window) -> egui::Rect {
    window.rectangle_of(&video_label(window.clip(1, 0)))
}

/// The left, top, right and bottom sides of the rectangle that has all of `clip` in it, turned as it is, as fractions of the
/// picture.
fn sides_of_the_layer(window: &Window, clip: ClipId) -> (f32, f32, f32, f32) {
    let place = placement_of(window, clip);
    let (width, height) = window.application().project().picture_size();
    let corners = Corner::ALL.map(|corner| place.corner_position(corner, (width as f32, height as f32)));
    (
        corners.iter().map(|corner| corner.0).fold(f32::INFINITY, f32::min),
        corners.iter().map(|corner| corner.1).fold(f32::INFINITY, f32::min),
        corners.iter().map(|corner| corner.0).fold(f32::NEG_INFINITY, f32::max),
        corners.iter().map(|corner| corner.1).fold(f32::NEG_INFINITY, f32::max),
    )
}

/// How many points the side of `clip` that faces `edge` is from that edge of the picture (negative if it is past it).
fn points_from_the_edge(window: &Window, clip: ClipId, edge: Edge) -> f32 {
    let picture = the_picture_on_the_screen(window);
    let (left, top, right, bottom) = sides_of_the_layer(window, clip);
    match edge {
        Edge::Left => left * picture.width(),
        Edge::Right => (1.0 - right) * picture.width(),
        Edge::Top => top * picture.height(),
        Edge::Bottom => (1.0 - bottom) * picture.height(),
    }
}

/// The way the pointer goes to take a layer towards `edge`.
fn towards(edge: Edge) -> Vec2 {
    match edge {
        Edge::Left => vec2(-1.0, 0.0),
        Edge::Right => vec2(1.0, 0.0),
        Edge::Top => vec2(0.0, -1.0),
        Edge::Bottom => vec2(0.0, 1.0),
    }
}

/// Takes hold of the selected layer in the middle and goes towards `edge` with it until its side is about 40 points from it
/// (which is not near enough to be held), and then in one move until it is `points_away` from it. The button is left down. Gives
/// where the pointer is, and the way it goes to take the layer towards the edge.
fn take_the_layer_to_within(window: &mut Window, clip: ClipId, edge: Edge, points_away: f32) -> (Pos2, Vec2) {
    let direction = towards(edge);
    let middle = middle_of_the_layer(window);
    let first_stop = middle + direction * (points_from_the_edge(window, clip, edge) - 40.0);
    window.drag_and_hold(middle, first_stop, PointerButton::Primary);
    let pointer = first_stop + direction * (points_from_the_edge(window, clip, edge) - points_away);
    window.move_pointer_to(pointer);
    (pointer, direction)
}

#[test]
fn a_layer_that_is_dragged_with_shift_held_is_held_to_the_edge_of_the_picture_it_comes_near() {
    for edge in EVERY_EDGE {
        let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
        window.hold_shift();
        let (pointer, _) = take_the_layer_to_within(&mut window, text, edge, 6.0);
        window.release(pointer, PointerButton::Primary);
        window.let_go_of_shift();

        assert!(
            points_from_the_edge(&window, text, edge).abs() < 0.01,
            "{edge:?}: the pointer was 6 points from the edge, and the layer is held to it: {}",
            points_from_the_edge(&window, text, edge)
        );
        assert_eq!(
            placement_of(&window, text).rotation,
            Angle::ZERO,
            "only where it is changes"
        );
    }
}

#[test]
fn a_layer_is_held_from_as_many_points_away_at_the_top_and_the_bottom_as_at_the_sides() {
    // (The picture is wider than it is tall, so a distance that was counted in fractions of the width for the sides that face the
    // top and the bottom, or of the height for the others, would be a different number of points there.)
    for edge in EVERY_EDGE {
        let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
        window.hold_shift();
        let (pointer, _) = take_the_layer_to_within(&mut window, text, edge, 11.0);
        window.release(pointer, PointerButton::Primary);
        window.let_go_of_shift();
        assert!(
            points_from_the_edge(&window, text, edge).abs() < 0.01,
            "{edge:?}: held from 11 points away: {}",
            points_from_the_edge(&window, text, edge)
        );

        let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
        window.hold_shift();
        let (pointer, _) = take_the_layer_to_within(&mut window, text, edge, 13.0);
        window.release(pointer, PointerButton::Primary);
        window.let_go_of_shift();
        let away = points_from_the_edge(&window, text, edge);
        assert!(
            (away - 13.0).abs() < 0.3,
            "{edge:?}: not held from 13 points away: {away}"
        );
    }
}

#[test]
fn without_shift_a_layer_is_not_held_to_the_edge_however_near_it_comes() {
    for edge in EVERY_EDGE {
        let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
        let (pointer, _) = take_the_layer_to_within(&mut window, text, edge, 6.0);
        window.release(pointer, PointerButton::Primary);

        let away = points_from_the_edge(&window, text, edge);
        assert!(
            (away - 6.0).abs() < 0.3,
            "{edge:?}: {away} points from the edge, where the pointer took it"
        );
    }
}

#[test]
fn a_layer_that_is_further_from_the_edge_than_the_distance_it_is_held_from_goes_where_the_pointer_takes_it() {
    for edge in EVERY_EDGE {
        let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
        window.hold_shift();
        let (pointer, _) = take_the_layer_to_within(&mut window, text, edge, 20.0);
        window.release(pointer, PointerButton::Primary);
        window.let_go_of_shift();

        let away = points_from_the_edge(&window, text, edge);
        assert!((away - 20.0).abs() < 0.3, "{edge:?}: {away} points from the edge");
    }
}

#[test]
fn a_layer_is_let_go_of_by_the_edge_when_the_pointer_has_gone_far_enough_from_where_it_would_hold_it_however_slowly() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    window.hold_shift();
    let (pointer, direction) = take_the_layer_to_within(&mut window, text, Edge::Left, 6.0);
    assert!(points_from_the_edge(&window, text, Edge::Left).abs() < 0.01, "held");

    // Back from the edge a point or two at a time: it stays held while the pointer is within the distance, and then it follows.
    let mut pointer = pointer;
    for (steps_back, held) in [(1.0, true), (2.0, true), (2.0, true), (3.0, false)] {
        pointer -= direction * steps_back;
        window.move_pointer_to(pointer);
        let away = points_from_the_edge(&window, text, Edge::Left);
        if held {
            assert!(
                away.abs() < 0.01,
                "the pointer is {} points from where it holds it: {away}",
                6.0 + steps_back
            );
        }
    }
    let away = points_from_the_edge(&window, text, Edge::Left);
    assert!(
        (away - 14.0).abs() < 0.3,
        "the pointer has gone back 8 points from where it was 6 from the edge: it is 14 from it, and not held: {away}"
    );
    window.release(pointer, PointerButton::Primary);
    window.let_go_of_shift();
}

#[test]
fn letting_go_of_shift_while_dragging_puts_the_layer_where_the_pointer_has_it() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    window.hold_shift();
    let (pointer, direction) = take_the_layer_to_within(&mut window, text, Edge::Right, 6.0);
    assert!(points_from_the_edge(&window, text, Edge::Right).abs() < 0.01, "held");

    window.let_go_of_shift();
    window.move_pointer_to(pointer - direction * 1.0);

    let away = points_from_the_edge(&window, text, Edge::Right);
    assert!((away - 7.0).abs() < 0.3, "not held: {away} points from the edge");
    window.release(pointer - direction * 1.0, PointerButton::Primary);
}

#[test]
fn a_turned_layer_is_held_by_the_corner_that_comes_nearest_to_the_edge() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let turned = Placement::new(CanvasArea::new(0.3, 0.4, 0.4, 0.2), Angle::from_degrees(40.0));
    window.do_this(Command::SetClipPlacement {
        clip: text,
        placement: turned,
    });
    window.hold_shift();
    let (pointer, _) = take_the_layer_to_within(&mut window, text, Edge::Left, 6.0);
    window.release(pointer, PointerButton::Primary);
    window.let_go_of_shift();

    assert!(
        points_from_the_edge(&window, text, Edge::Left).abs() < 0.01,
        "its leftmost corner is at the edge: {}",
        points_from_the_edge(&window, text, Edge::Left)
    );
    assert_degrees(placement_of(&window, text).rotation, 40.0, "still turned the same");
}

#[test]
fn being_held_to_an_edge_is_one_step_of_the_history_and_undoing_it_puts_the_layer_back() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let before = placement_of(&window, text);
    window.application_mut().note_everything_is_saved();
    window.hold_shift();
    let (pointer, _) = take_the_layer_to_within(&mut window, text, Edge::Top, 6.0);
    window.release(pointer, PointerButton::Primary);
    window.let_go_of_shift();
    assert!(window.application().has_unsaved_changes());

    window.do_this(Command::Undo);

    assert_eq!(placement_of(&window, text), before, "one undo is enough");
}

#[test]
fn a_corner_that_is_dragged_with_shift_held_is_not_held_to_the_edge_of_the_picture() {
    // (Shift has a meaning of its own when a layer is turned, which is whole steps of 15 degrees, and none when it is resized:
    // only moving a layer is held to the edges.)
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let picture = the_picture_on_the_screen(&window);
    let corner = corner_position(&window, Corner::BottomRight);
    window.hold_shift();
    window.drag(corner, pos2(picture.right() - 6.0, corner.y), PointerButton::Primary);
    window.let_go_of_shift();

    let away = points_from_the_edge(&window, text, Edge::Right);
    assert!(
        (away - 6.0).abs() < 0.5,
        "{away} points from the edge: the corner is where the pointer put it"
    );
}

// --- a solid colour (a generator clip) ---

/// A window with a video of 300 frames on its one track and, over it from the start, a solid colour on a track in front of it that is put at `area`, which is selected.
fn window_with_a_solid_colour_at(area: CanvasArea) -> (Window, ClipId) {
    let mut project = project_of(300);
    let front = project.add_track_in_front();
    let (solid, _) = project
        .try_insert_generator_clip(
            front,
            TimelineFrameIndex::new(0),
            crate::project::GeneratorKind::SolidColour,
            200,
        )
        .unwrap();
    project.set_clip_placement(solid, Placement::upright(area)).unwrap();
    let mut window = Window::showing(project);
    select(&mut window, solid);
    (window, solid)
}

#[test]
fn a_solid_colour_on_the_preview_takes_any_shape_that_a_corner_is_dragged_to() {
    let (mut window, solid) = window_with_a_solid_colour_at(CanvasArea::new(0.3, 0.4, 0.4, 0.2));
    let before = placement_of(&window, solid);
    let corner = corner_position(&window, Corner::BottomRight);

    window.drag(corner, corner + vec2(-40.0, 0.0), PointerButton::Primary);

    let after = placement_of(&window, solid);
    assert!(
        after.area.width() < before.area.width() - 0.02,
        "{before:?} then {after:?}"
    );
    assert!(
        (after.area.height() - before.area.height()).abs() < 1e-3,
        "only one way changed: {after:?}"
    );
}

#[test]
fn a_solid_colour_on_the_preview_is_moved_by_dragging_it() {
    let (mut window, solid) = window_with_a_solid_colour_at(CanvasArea::new(0.3, 0.4, 0.4, 0.2));
    let before = placement_of(&window, solid);
    let middle = middle_of_the_layer(&window);

    window.drag(middle, middle + vec2(30.0, 0.0), PointerButton::Primary);

    let after = placement_of(&window, solid);
    assert!(
        after.area.left() > before.area.left() + 0.01,
        "{before:?} then {after:?}"
    );
    assert_eq!(after.area.width(), before.area.width(), "it keeps its size");
}

#[test]
fn a_solid_colour_that_fills_the_picture_has_no_hand_when_the_pointer_is_over_it_until_it_is_picked() {
    let (mut window, solid) = window_with_a_solid_colour_at(CanvasArea::WHOLE_PICTURE);
    window.do_this(Command::Deselect);
    let picture = window.rectangle_of(&crate::application::preview_panel::solid_colour_label(solid));
    window.move_pointer_to(picture.center());
    window.harness.step();
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Default,
        "nothing to grab: it is all of the picture"
    );
    assert_eq!(window.application().selected_clip(), None);

    window.click(picture.center());
    assert_eq!(window.application().selected_clip(), Some(solid), "a click picks it");
}
