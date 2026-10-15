// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of zooming the rows of the timeline up and down with Ctrl and Alt and the mouse wheel.
//!
//! The zoom is how the timeline is looked at (not an edit, and not kept with the project): every row is as many times as tall as
//! its track says, and the tracks keep the heights they have.

use egui::{Event, Modifiers, MouseWheelUnit, Pos2, Shape, TouchPhase, vec2};

use super::rack_tests::racks_drawn;
use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::theme;
use crate::project::{AnimationTarget, PropertyId, PropertyValue, TrackHeight};
use crate::test_support::{RACKS_IN_THE_CRATE, make_a_keyframe, project_with_a_crate_that_has_a_lot_of_racks};

/// How much a turn of the wheel by this many points makes the zoom change (egui makes a zoom of `exp(delta / 200)` of a turn).
const ONE_NOTCH: f32 = 100.0;

/// What a notch makes of a height: `exp(100 / 200)`.
const ZOOM_OF_ONE_NOTCH: f32 = 1.648_721_3;

const CTRL_AND_ALT: Modifiers = Modifiers {
    alt: true,
    ..Modifiers::COMMAND
};

/// A window with a sound on the track in front and a video on the one behind it.
fn window_with_two_tracks() -> Window {
    Window::showing(project_with_a_video_and_a_sound(300, 10))
}

fn zoom_up_and_down(window: &mut Window, position: Pos2, notches: f32) {
    window.scroll_the_wheel(position, vec2(0.0, notches * ONE_NOTCH), CTRL_AND_ALT);
}

fn row_height(window: &Window, position: usize) -> f32 {
    window.strip_of_track(position).height()
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 0.6, "{what}: {actual} is not {expected}");
}

fn width_of_a_second(window: &Window) -> f32 {
    window.x_of_frame_on_the_strip(0, 60) - window.x_of_frame_on_the_strip(0, 30)
}

fn set_height(window: &mut Window, position: usize, points: f32) {
    let track = window.application().project().tracks()[position].id();
    window.do_this(Command::SetTrackHeight {
        track,
        height: TrackHeight::from_points(points),
    });
}

// --- zooming ---

#[test]
fn turning_the_wheel_with_control_and_alt_held_makes_every_row_taller_and_turning_it_back_makes_them_as_they_were() {
    let mut window = window_with_two_tracks();
    let normal = row_height(&window, 0);
    assert_close(normal, 104.0, "the setup: a new track");
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, 1.0);

    for position in 0..2 {
        assert_close(
            row_height(&window, position),
            normal * ZOOM_OF_ONE_NOTCH,
            &format!("the row of track {position}"),
        );
    }

    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    zoom_up_and_down(&mut window, over_a_strip, -1.0);
    for position in 0..2 {
        assert_close(row_height(&window, position), normal, &format!("track {position} back"));
    }
}

#[test]
fn the_wheel_the_other_way_makes_the_rows_shorter_and_they_stop_at_the_least_that_a_row_can_be() {
    let mut window = window_with_two_tracks();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, -1.0);
    assert_close(
        row_height(&window, 0),
        104.0 / ZOOM_OF_ONE_NOTCH,
        "one notch makes a row that much shorter, which is more than the least",
    );

    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    zoom_up_and_down(&mut window, over_a_strip, -40.0);
    assert_close(
        row_height(&window, 0),
        TrackHeight::SMALLEST_IN_POINTS,
        "and no row is shorter than the least that a track can be",
    );
}

#[test]
fn the_rows_stop_at_three_times_as_tall_as_their_tracks_say() {
    let mut window = window_with_two_tracks();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, 40.0);

    assert_close(row_height(&window, 0), 3.0 * 104.0, "the most it zooms in");
}

#[test]
fn zooming_all_the_way_out_does_not_make_a_dead_place_that_the_wheel_has_to_be_turned_through_before_the_rows_grow_again()
 {
    let mut window = window_with_two_tracks();
    set_height(&mut window, 0, 156.0);
    let over_a_strip = window.middle_of_the_strip_at(1, 100);
    zoom_up_and_down(&mut window, over_a_strip, -40.0);
    assert_close(
        row_height(&window, 0),
        TrackHeight::SMALLEST_IN_POINTS,
        "the tallest track is as short as can be",
    );
    assert_close(
        row_height(&window, 1),
        TrackHeight::SMALLEST_IN_POINTS,
        "and so is the other",
    );

    let over_a_strip = window.middle_of_the_strip_at(1, 100);
    zoom_up_and_down(&mut window, over_a_strip, 0.5);

    assert!(
        row_height(&window, 0) > TrackHeight::SMALLEST_IN_POINTS + 5.0,
        "a little of the wheel the other way makes the rows taller at once: {}",
        row_height(&window, 0)
    );
}

#[test]
fn rows_of_different_heights_are_all_as_many_times_as_tall() {
    let mut window = window_with_two_tracks();
    set_height(&mut window, 0, 60.0);
    set_height(&mut window, 1, 130.0);
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, 1.0);

    assert_close(row_height(&window, 0), 60.0 * ZOOM_OF_ONE_NOTCH, "the shorter");
    assert_close(row_height(&window, 1), 130.0 * ZOOM_OF_ONE_NOTCH, "the taller");
}

#[test]
fn the_zoom_works_from_the_ruler_the_strips_and_the_title_boxes_but_not_from_the_other_panels() {
    for (place, zooms) in [
        ("the ruler", true),
        ("a strip", true),
        ("a title box", true),
        ("the preview", false),
        ("the properties panel", false),
    ] {
        let mut window = window_with_two_tracks();
        let position = match place {
            "the ruler" => window.rectangle_of("Ruler").center(),
            "a strip" => window.middle_of_the_strip_at(0, 100),
            "a title box" => window.title_box_of_track(0).center(),
            "the preview" => window.the_preview_panel().center(),
            _ => window.the_properties_panel().center(),
        };

        zoom_up_and_down(&mut window, position, 1.0);

        let changed = (row_height(&window, 0) - 104.0).abs() > 1.0;
        assert_eq!(changed, zooms, "the wheel with Ctrl and Alt held over {place}");
    }
}

// --- what it is not ---

#[test]
fn the_wheel_with_control_and_alt_zooms_up_and_down_alone_and_does_not_zoom_or_scroll_along_the_strips() {
    let mut window = Window::showing(project_of(900));
    let second = width_of_a_second(&window);
    let scroll_before = window.application().timeline_view.scroll_offset;
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, 1.0);

    assert!(
        row_height(&window, 0) > 104.0 + 20.0,
        "the setup: it zoomed up and down"
    );
    assert_close(width_of_a_second(&window), second, "a second is as wide as it was");
    assert_eq!(
        window.application().timeline_view.scroll_offset,
        scroll_before,
        "and the strips were not scrolled along"
    );
}

#[test]
fn the_wheel_with_control_alone_still_zooms_along_the_strips_and_does_not_change_the_rows() {
    let mut window = Window::showing(project_of(900));
    let second = width_of_a_second(&window);
    let height = row_height(&window, 0);
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    window.scroll_the_wheel(over_a_strip, vec2(0.0, ONE_NOTCH), Modifiers::COMMAND);

    assert!(width_of_a_second(&window) > second * 1.5, "the strips zoomed along");
    assert_close(row_height(&window, 0), height, "and the rows did not change");
}

#[test]
fn the_wheel_with_control_and_shift_still_zooms_along_the_strips_only() {
    let mut window = Window::showing(project_of(900));
    let second = width_of_a_second(&window);
    let height = row_height(&window, 0);
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    window.scroll_the_wheel(
        over_a_strip,
        vec2(0.0, ONE_NOTCH),
        Modifiers::COMMAND | Modifiers::SHIFT,
    );

    assert!(width_of_a_second(&window) > second * 1.5, "the strips zoomed along");
    assert_close(row_height(&window, 0), height, "and the rows did not change");
}

#[test]
fn the_wheel_with_alt_alone_still_scrolls_the_rows_and_does_not_zoom_them() {
    let mut window = window_with_two_tracks();
    for _ in 0..3 {
        window.do_this(Command::AddTrack);
    }
    let height = row_height(&window, 0);
    let top = window.strip_of_track(0).top();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    window.scroll_the_wheel(over_a_strip, vec2(0.0, -ONE_NOTCH), Modifiers::ALT);

    assert_close(row_height(&window, 0), height, "the rows are as tall as they were");
    assert!(window.strip_of_track(0).top() < top - 20.0, "and they scrolled");
}

#[test]
fn the_zoom_is_not_an_edit_it_changes_no_track_and_is_not_one_of_the_unsaved_changes() {
    let mut window = window_with_two_tracks();
    let heights_before: Vec<f32> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.height().points())
        .collect();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, 1.0);

    let heights_after: Vec<f32> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.height().points())
        .collect();
    assert_eq!(heights_after, heights_before, "the tracks are as tall as they were");
    assert!(!window.application().can_undo(), "there is nothing to undo");
    assert!(!window.application().has_unsaved_changes(), "and nothing to save");
}

#[test]
fn a_different_project_is_looked_at_without_the_zoom() {
    let mut window = window_with_two_tracks();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    zoom_up_and_down(&mut window, over_a_strip, 1.0);
    assert!(row_height(&window, 0) > 150.0, "the setup");

    window.show_another_project(project_of(300));

    assert_close(row_height(&window, 0), 104.0, "the rows of the new project");
}

// --- what is under the pointer ---

/// How far down the strip of the track at `position` the point `y` is, as a fraction of its height.
fn how_far_down_the_row(window: &Window, position: usize, y: f32) -> f32 {
    let strip = window.strip_of_track(position);
    (y - strip.top()) / strip.height()
}

#[test]
fn zooming_keeps_the_part_of_the_row_that_is_under_the_pointer_under_it() {
    let mut window = window_with_two_tracks();
    for _ in 0..3 {
        window.do_this(Command::AddTrack);
    }
    // Five tracks, of which three show: the rows scroll. The pointer is a quarter of the way down the third.
    let third = window.strip_of_track(2);
    let pointer = Pos2::new(third.center().x, third.top() + 0.25 * third.height());
    assert!(
        window.the_bottom_of_what_can_be_seen_of_the_rows() > pointer.y,
        "the setup: the pointer is over the rows"
    );

    zoom_up_and_down(&mut window, pointer, 1.0);

    assert!(row_height(&window, 2) > 150.0, "the setup: the rows are taller");
    assert_close(
        how_far_down_the_row(&window, 2, pointer.y),
        0.25,
        "the point of the third row that was under the pointer is under it (as a fraction of the way down the row)",
    );

    zoom_up_and_down(&mut window, pointer, -1.0);
    assert_close(
        how_far_down_the_row(&window, 2, pointer.y),
        0.25,
        "and again as the rows go back",
    );
}

#[test]
fn zooming_keeps_the_part_of_a_drawer_that_is_under_the_pointer_the_same_distance_under_the_strip_it_hangs_from() {
    let mut window = window_with_two_tracks();
    for _ in 0..3 {
        window.do_this(Command::AddTrack);
    }
    let track = window.application().project().tracks()[2].id();
    window
        .application_mut()
        .timeline_view
        .open_the_drawer_of(crate::project::AnimationTarget::Track(track));
    // (The drawer slides open in a fifth of a second, and the rows are scrolled to show all of it as it does: nothing is moving
    // when it has.)
    window.harness.run_steps(40);
    let strip = window.strip_of_track(2);
    let in_the_drawer = Pos2::new(strip.center().x, strip.bottom() + 20.0);
    assert!(
        window.the_bottom_of_what_can_be_seen_of_the_rows() > in_the_drawer.y,
        "the setup: the pointer is over the rows"
    );

    zoom_up_and_down(&mut window, in_the_drawer, 1.0);

    assert_close(
        in_the_drawer.y - window.strip_of_track(2).bottom(),
        20.0,
        "the pointer is still 20 points under the strip, in the drawer that is as tall as it was",
    );
}

/// A window with five tracks, of which three show at once: the rows scroll.
fn window_with_five_tracks() -> Window {
    let mut window = window_with_two_tracks();
    for _ in 0..3 {
        window.do_this(Command::AddTrack);
    }
    window
}

/// Scrolls the rows (with the wheel and Alt, the pointer over the first of them) further down than they go.
fn scroll_the_rows_to_the_end(window: &mut Window) {
    let over_the_first_row = window.middle_of_the_strip_at(0, 100);
    scroll_the_rows_further_down_than_they_go_with_the_pointer_at(window, over_the_first_row);
}

/// Scrolls the rows (with the wheel and Alt) further down than they go, with the pointer at `pointer`, which has to be over the timeline:
/// (the first row is not where it was when the rows are scrolled down, so the pointer is not always over it).
fn scroll_the_rows_further_down_than_they_go_with_the_pointer_at(window: &mut Window, pointer: Pos2) {
    window.scroll_the_wheel(pointer, vec2(0.0, -20.0 * ONE_NOTCH), Modifiers::ALT);
}

fn rows_scroll_offset(window: &Window) -> f32 {
    window.application().timeline_view.rows_scroll_offset
}

/// A point over the rows, a little under the ruler: near the top of what can be seen of them.
fn near_the_top_of_the_rows(window: &Window) -> Pos2 {
    let ruler = window.rectangle_of("Ruler");
    Pos2::new(ruler.center().x, ruler.bottom() + 12.0)
}

#[test]
fn zooming_out_with_the_rows_scrolled_to_the_end_leaves_them_scrolled_as_far_as_they_go() {
    let mut window = window_with_five_tracks();
    let pointer = near_the_top_of_the_rows(&window);
    zoom_up_and_down(&mut window, pointer, 1.0);
    scroll_the_rows_to_the_end(&mut window);
    let before = rows_scroll_offset(&window);
    assert!(
        before > 100.0,
        "the setup: the rows are scrolled a good way down: {before}"
    );

    // (With the pointer near the top, keeping the part of the rows under it where it is would take the rows further than they go.)
    zoom_up_and_down(&mut window, pointer, -0.5);
    let after_the_zoom = rows_scroll_offset(&window);
    assert!(
        after_the_zoom < before - 20.0,
        "the setup: the rows are shorter now: {after_the_zoom}"
    );
    // (With the pointer where it is, which is over the rows: the first of them is scrolled out of sight.)
    scroll_the_rows_further_down_than_they_go_with_the_pointer_at(&mut window, pointer);

    assert_close(
        after_the_zoom,
        rows_scroll_offset(&window),
        "the rows were already as far down as they go, to the end of the room under the last row",
    );
}

#[test]
fn zooming_in_with_the_rows_scrolled_to_the_end_keeps_the_part_of_them_that_is_under_the_pointer_under_it() {
    let mut window = window_with_five_tracks();
    scroll_the_rows_to_the_end(&mut window);
    let pointer = near_the_top_of_the_rows(&window);
    let track_under = |window: &Window| {
        (0..5)
            .find(|position| window.strip_of_track(*position).y_range().contains(pointer.y))
            .expect("the pointer is over one of the rows")
    };
    let (track, fraction) = (
        track_under(&window),
        how_far_down_the_row(&window, track_under(&window), pointer.y),
    );

    zoom_up_and_down(&mut window, pointer, 1.0);

    assert_eq!(track_under(&window), track, "the same track is under the pointer");
    assert_close(
        how_far_down_the_row(&window, track, pointer.y),
        fraction,
        "and the same part of its row",
    );
}

#[test]
fn the_racks_in_the_drawer_of_a_track_are_under_the_racks_of_a_crate_that_reaches_below_its_strip_when_the_rows_are_zoomed_down()
 {
    // A track with a keyframe crate that has more racks than fit in its strip (so that they reach down below it, and the more so the shorter
    // the strip is drawn), and the drawer of the track itself open, with a rack in it: how loud the track is.
    let (mut project, crate_track) = project_with_a_crate_that_has_a_lot_of_racks();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(crate_track),
        PropertyId::Gain,
        10,
        PropertyValue::Number(-3.0),
    );
    let mut window = Window::showing(project);
    window
        .application_mut()
        .timeline_view
        .open_the_drawer_of(AnimationTarget::Track(crate_track));
    window.settle();
    let over_the_rows = near_the_top_of_the_rows(&window);

    zoom_up_and_down(&mut window, over_the_rows, -1.0);
    window.settle();

    let position = window
        .application()
        .project()
        .tracks()
        .iter()
        .position(|track| track.id() == crate_track)
        .unwrap();
    assert!(
        window.strip_of_track(position).height() < 80.0,
        "the setup: the strip is drawn shorter than the 104 points that the track says: {}",
        window.strip_of_track(position).height()
    );
    let racks = racks_drawn(&window);
    assert_eq!(
        racks.len(),
        RACKS_IN_THE_CRATE + 1,
        "the racks of the crate, and the rack of the track"
    );
    let (in_the_crate, in_the_drawer_of_the_track) = racks.split_at(RACKS_IN_THE_CRATE);
    assert!(
        in_the_drawer_of_the_track[0].top() >= in_the_crate[RACKS_IN_THE_CRATE - 1].bottom(),
        "the rack of the track is under the last rack of the crate, and not over it: {:?} and {:?}",
        in_the_drawer_of_the_track[0],
        in_the_crate[RACKS_IN_THE_CRATE - 1]
    );

    // The bodies of the crate and of the drawer, which are drawn under the strip, are not cut off at the bottom by the room they are drawn in.
    let bodies: Vec<_> = window
        .clipped_shapes_drawn()
        .into_iter()
        .filter_map(|(cut_off_at, shape)| match shape {
            Shape::Rect(rectangle)
                if rectangle.fill == theme::CRATE_CLIP_BACKGROUND || rectangle.fill == theme::DRAWER_BACKGROUND =>
            {
                Some((cut_off_at, rectangle.rect, rectangle.fill))
            }
            _ => None,
        })
        .collect();
    assert!(
        bodies.len() >= 2,
        "the setup: the body of the crate and the body of the drawer: {bodies:?}"
    );
    for (cut_off_at, body, fill) in &bodies {
        assert!(
            body.bottom() <= cut_off_at.bottom() + 0.5,
            "the body {body:?} ({fill:?}) is cut off at {cut_off_at:?}"
        );
    }
    // The drawer of the track hangs from the end of what the crate reaches down to: it does not begin before that.
    let lowest_of_the_crate = bodies
        .iter()
        .filter(|(_, _, fill)| *fill == theme::CRATE_CLIP_BACKGROUND)
        .map(|(_, body, _)| body.bottom())
        .fold(f32::MIN, f32::max);
    let drawer_of_the_track = bodies
        .iter()
        .find(|(_, _, fill)| *fill == theme::DRAWER_BACKGROUND)
        .expect("the drawer of the track is drawn")
        .1;
    assert!(
        drawer_of_the_track.top() >= lowest_of_the_crate - 0.5,
        "the drawer of the track {drawer_of_the_track:?} begins where the crate has reached down to: {lowest_of_the_crate}"
    );
}

// --- the layout ---

#[test]
fn the_rows_are_laid_out_as_tall_as_the_area_they_are_in_finds_them_when_they_are_zoomed() {
    // The panel is made as tall as the rows are going to be from what the layout of them says, so the layout and what the area
    // finds in it must not drift apart when the rows are zoomed: this is what tells it if they do.
    let mut window = window_with_two_tracks();
    let rows = |window: &Window| {
        window
            .application()
            .timeline_view
            .rows_in_the_last_frame
            .expect("the rows have been drawn")
    };
    let at_the_start = rows(&window);
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, 0.5);

    let now = rows(&window);
    assert!(
        now.content_height > at_the_start.content_height + 20.0,
        "the setup: the rows grew"
    );
    assert!(
        ((now.content_height - at_the_start.content_height) - (now.height_laid_out - at_the_start.height_laid_out))
            .abs()
            < 0.1,
        "what is in the area changed by {} and the layout by {}",
        now.content_height - at_the_start.content_height,
        now.height_laid_out - at_the_start.height_laid_out
    );
}

/// Where the wheel is turned with Ctrl and Alt held, a frame at a time, which is what a window gets: one event, and then the smooth
/// zooming that follows in each frame. Calls `after_each_frame` after each frame.
fn zoom_a_frame_at_a_time(window: &mut Window, position: Pos2, delta: f32, mut after_each_frame: impl FnMut(&Window)) {
    window.move_pointer_to(position);
    window.harness.event(Event::ModifiersChanged(CTRL_AND_ALT));
    window.harness.step();
    window.harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, delta),
        phase: TouchPhase::Move,
        modifiers: CTRL_AND_ALT,
    });
    for _ in 0..31 {
        window.harness.step();
        after_each_frame(window);
    }
    window.harness.event(Event::ModifiersChanged(Modifiers::NONE));
    window.harness.step();
    window.settle();
}

#[test]
fn while_the_rows_zoom_the_bottom_of_the_last_row_stays_where_it_is_on_the_screen_in_every_frame() {
    // (The panel is as tall as what was in it in the frame before, unless it is made as tall as the zoom is going to make the
    // rows, and then everything in it is drawn lower than it is going to be, by as much as the rows have grown in that frame, and
    // goes back up in the next: see `drawer_motion_tests`.)
    let mut window = window_with_two_tracks();
    let bottom_before = window.strip_of_track(1).bottom();
    let top_before = window.strip_of_track(0).top();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    let mut bottoms = Vec::new();
    let mut tops = Vec::new();
    zoom_a_frame_at_a_time(&mut window, over_a_strip, ONE_NOTCH * 0.5, |window| {
        bottoms.push(window.strip_of_track(1).bottom());
        tops.push(window.strip_of_track(0).top());
    });

    assert!(
        tops.last().unwrap() < &(top_before - 20.0),
        "the setup: the panel grew upward as the rows grew: {top_before} then {:?}",
        tops.last()
    );
    for (frame, bottom) in bottoms.iter().enumerate() {
        assert_close(
            *bottom,
            bottom_before,
            &format!("the bottom of the last row in frame {frame}"),
        );
    }
}

// --- dragging the boundary under a title box ---

#[test]
fn a_track_that_is_made_taller_with_the_rows_zoomed_is_given_the_height_that_is_drawn_as_tall_as_the_pointer_makes_it()
{
    let mut window = window_with_two_tracks();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    zoom_up_and_down(&mut window, over_a_strip, 0.5);
    let zoomed = row_height(&window, 0);
    assert!(zoomed > 130.0, "the setup: {zoomed}");
    let from = Pos2::new(
        window.title_box_of_track(0).center().x,
        window.strip_of_track(0).bottom() + 3.0,
    );

    window.drag(from, from + vec2(0.0, 30.0), egui::PointerButton::Primary);

    assert_close(
        row_height(&window, 0),
        zoomed + 30.0,
        "the row has gone as far as the pointer",
    );
    let track_height = window.application().project().tracks()[0].height().points();
    assert_close(
        track_height,
        104.0 + 30.0 / (zoomed / 104.0),
        "and the track is given the height that is drawn that tall (the move of the pointer divided by the zoom)",
    );
    assert_close(row_height(&window, 1), zoomed, "the other row did not change");
}

#[test]
fn the_wheel_with_control_and_alt_held_does_nothing_while_a_boundary_is_being_dragged() {
    let mut window = window_with_two_tracks();
    let from = Pos2::new(
        window.title_box_of_track(0).center().x,
        window.strip_of_track(0).bottom() + 3.0,
    );
    window.drag_and_hold(from, from + vec2(0.0, 10.0), egui::PointerButton::Primary);
    window.harness.run_steps(2);
    let height = row_height(&window, 0);
    assert_close(height, 114.0, "the setup: the row is as tall as the pointer made it");

    // (The pointer stays where it has hold of the boundary, and the wheel is turned there.)
    window.harness.event(Event::ModifiersChanged(CTRL_AND_ALT));
    window.harness.step();
    window.harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, ONE_NOTCH),
        phase: TouchPhase::Move,
        modifiers: CTRL_AND_ALT,
    });
    for _ in 0..20 {
        window.harness.step();
    }

    assert_close(
        row_height(&window, 0),
        height,
        "the row is as tall as the pointer made it, with nothing zoomed",
    );
    window.harness.event(Event::ModifiersChanged(Modifiers::NONE));
    window.release(from + vec2(0.0, 10.0), egui::PointerButton::Primary);
    window.harness.run_steps(40);

    // (What is drawn while a boundary is held is what the pointer has made of it, so a zoom would not show until it is let go.)
    assert_eq!(
        window.application().timeline_view.vertical_zoom.factor(),
        1.0,
        "the rows were not zoomed by the wheel while the boundary was held"
    );
    assert_close(
        row_height(&window, 0),
        height,
        "and they are as tall as the pointer made them when it is let go",
    );
}

#[test]
fn the_wheel_with_control_and_alt_held_does_nothing_while_a_track_is_being_moved_in_the_list() {
    let mut window = window_with_two_tracks();
    let height = row_height(&window, 0);
    let title_box = window.title_box_of_track(0);
    let from = Pos2::new(title_box.left() - 4.0, window.strip_of_track(0).center().y);
    window.drag_and_hold(from, from + vec2(0.0, 20.0), egui::PointerButton::Primary);
    window.harness.run_steps(2);

    window.harness.event(Event::ModifiersChanged(CTRL_AND_ALT));
    window.harness.step();
    window.harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, ONE_NOTCH),
        phase: TouchPhase::Move,
        modifiers: CTRL_AND_ALT,
    });
    for _ in 0..20 {
        window.harness.step();
    }
    window.harness.event(Event::ModifiersChanged(Modifiers::NONE));
    window.release(from + vec2(0.0, 20.0), egui::PointerButton::Primary);
    window.harness.run_steps(40);

    assert_eq!(
        window.application().timeline_view.vertical_zoom.factor(),
        1.0,
        "the rows were not zoomed by the wheel while a track was moved"
    );
    assert_close(row_height(&window, 0), height, "they are as tall as they were");
}

#[test]
fn a_track_that_has_a_row_of_the_least_height_is_drawn_so_and_not_cut_by_the_zoom() {
    let mut window = window_with_two_tracks();
    set_height(&mut window, 0, 0.0);
    let over_a_strip = window.middle_of_the_strip_at(0, 100);

    zoom_up_and_down(&mut window, over_a_strip, -0.5);

    assert_close(
        row_height(&window, 0),
        TrackHeight::SMALLEST_IN_POINTS,
        "a row that cannot be any shorter",
    );
    let title = window.title_box_of_track(0);
    let row = window.strip_of_track(0);
    assert!(
        title.top() >= row.top() && title.bottom() <= row.bottom(),
        "the title is inside the box of the row: {title:?}, {row:?}"
    );
}
