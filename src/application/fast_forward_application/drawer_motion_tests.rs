// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what moves on the screen, frame by frame, while a drawer slides open or shut.
//!
//! The timeline panel is as tall as what is in it, and grows upward from the bottom of the window, so a drawer that pushes the
//! rows under it down makes it taller as it opens, and the row of the buttons and the slider, which is the top of the panel, goes
//! up with it. (There was a bug: the panel was as tall as the rows were in the frame before, so in each frame of the sliding
//! everything in it was drawn lower than it was going to be, by as much as the drawer had grown in that frame, and went back up
//! in the next: the track under the drawer jumped down and crept back. The tests here are what it was.)

use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_with_a_video_and_a_sound};
use crate::application::timeline_panel::drawer_button_label;

/// How far a thing that is meant to stay where it is on the screen may be from where it is when the sliding is over, in points: the
/// edges of the panel are put on whole points, so a thing that is placed from them can be half a point either way.
const THE_ROUNDING_OF_THE_PANEL: f32 = 0.55;

/// Where the things that matter are, in one frame of the window.
#[derive(Clone, Copy, Debug)]
struct Places {
    /// The top of the strip of the track that has the drawer, which is the first.
    first_strip_top: f32,
    /// The top of the strip of the track under it.
    second_strip_top: f32,
    /// The top of the slider, which is on the row at the top of the timeline panel.
    slider_top: f32,
    /// The bottom of the timeline panel.
    panel_bottom: f32,
    /// The top of the timeline panel, which is also the bottom of the panel above it.
    panel_top: f32,
}

impl Places {
    /// How far under the top of the panel the top of the slider is: the row it is on is the first thing in the panel, so this
    /// does not change as the panel grows.
    fn slider_below_the_top_of_the_panel(&self) -> f32 {
        self.slider_top - self.panel_top
    }
}

fn places(window: &Window) -> Places {
    let panel = window.the_timeline_panel();
    Places {
        first_strip_top: window.strip_of_track(0).top(),
        second_strip_top: window.strip_of_track(1).top(),
        slider_top: window.rectangle_of("Position in the video").top(),
        panel_bottom: panel.bottom(),
        panel_top: panel.top(),
    }
}

/// A window with two tracks, the drawer of the first of which can be opened, and nothing moving.
pub(super) fn window_with_two_tracks() -> Window {
    Window::showing(project_with_a_video_and_a_sound(60, 2))
}

/// Presses the button of the drawer of the first clip of the first track, as a person does: the pointer goes over the clip, which
/// makes the button come, and then over the button.
pub(super) fn press_the_button_of_the_drawer_of_the_first_track(window: &mut Window) {
    let clip = window.clip(0, 0);
    let strip = window.strip_of_track(0);
    window.move_pointer_to(egui::pos2(strip.center().x, strip.top() + 20.0));
    window.harness.run_steps(2);
    let button = window
        .harness
        .get_by_label(&drawer_button_label(&window.title_of_track(0), clip))
        .rect();
    window.move_pointer_to(button.center());
    window.press(button.center(), egui::PointerButton::Primary);
    window.release(button.center(), egui::PointerButton::Primary);
}

/// The places in each of the frames of the window from now until `frames` have gone, the first being the one after the button was
/// pressed.
fn the_places_in_the_frames_that_follow(window: &mut Window, frames: usize) -> Vec<Places> {
    (0..frames)
        .map(|_| {
            let places = places(window);
            window.harness.step();
            places
        })
        .collect()
}

fn assert_stays_within(places: &[Places], what: &str, wherever: impl Fn(&Places) -> f32, final_place: f32) {
    for (frame, places) in places.iter().enumerate() {
        let off = wherever(places) - final_place;
        assert!(
            off.abs() <= THE_ROUNDING_OF_THE_PANEL,
            "{what} is {off:+.2} points from where it ends up in frame {frame} of the sliding"
        );
    }
}

#[test]
fn the_track_under_a_drawer_that_opens_stays_where_it_is_on_the_screen_in_every_frame_of_the_opening() {
    let mut window = window_with_two_tracks();
    let before = places(&window);
    press_the_button_of_the_drawer_of_the_first_track(&mut window);

    let during = the_places_in_the_frames_that_follow(&mut window, 24);

    let after = *during.last().unwrap();
    assert!(
        after.first_strip_top < before.first_strip_top - 10.0,
        "the setup: the timeline grew upward by what the drawer needs: {before:?} then {after:?}"
    );
    assert_stays_within(
        &during,
        "the track under the drawer",
        |places| places.second_strip_top,
        before.second_strip_top,
    );
}

#[test]
fn the_slider_stays_the_same_distance_under_the_top_of_the_panel_and_the_bottom_of_the_panel_stays_where_it_is_while_a_drawer_opens()
 {
    let mut window = window_with_two_tracks();
    let before = places(&window);
    press_the_button_of_the_drawer_of_the_first_track(&mut window);

    let during = the_places_in_the_frames_that_follow(&mut window, 24);

    let after = *during.last().unwrap();
    assert!(
        after.slider_top < before.slider_top - 10.0,
        "the setup: the slider went up with the top of the panel: {before:?} then {after:?}"
    );
    assert_stays_within(
        &during,
        "the slider under the top of the panel",
        Places::slider_below_the_top_of_the_panel,
        before.slider_below_the_top_of_the_panel(),
    );
    assert_stays_within(
        &during,
        "the bottom of the panel",
        |places| places.panel_bottom,
        before.panel_bottom,
    );
}

#[test]
fn the_top_of_the_timeline_goes_up_as_the_drawer_opens_by_as_much_as_the_drawer_pushes_the_row_under_it_down() {
    let mut window = window_with_two_tracks();
    let before = places(&window);
    press_the_button_of_the_drawer_of_the_first_track(&mut window);

    let during = the_places_in_the_frames_that_follow(&mut window, 24);

    // The strip of the track that has the drawer is as far from the track under it as the drawer has opened, in every frame:
    // what the panel does is what the rows do, and not a frame behind them.
    let settled = *during.last().unwrap();
    let whole_distance = settled.second_strip_top - settled.first_strip_top;
    let distance_before = before.second_strip_top - before.first_strip_top;
    assert!(
        whole_distance > distance_before + 10.0,
        "the drawer pushes: {distance_before} to {whole_distance}"
    );
    for (frame, places) in during.iter().enumerate() {
        let panel_grew_by = before.panel_top - places.panel_top;
        let rows_were_pushed_by = (places.second_strip_top - places.first_strip_top) - distance_before;
        assert!(
            (panel_grew_by - rows_were_pushed_by).abs() <= THE_ROUNDING_OF_THE_PANEL,
            "frame {frame}: the panel grew by {panel_grew_by:.2} and the rows were pushed apart by {rows_were_pushed_by:.2}"
        );
    }
}

#[test]
fn the_track_under_a_drawer_that_shuts_stays_where_it_is_on_the_screen_in_every_frame_of_the_shutting() {
    let mut window = window_with_two_tracks();
    press_the_button_of_the_drawer_of_the_first_track(&mut window);
    window.settle();
    let open = places(&window);
    // (The timeline grew upward: the button is where it is now.)
    press_the_button_of_the_drawer_of_the_first_track(&mut window);

    let during = the_places_in_the_frames_that_follow(&mut window, 24);

    let after = *during.last().unwrap();
    assert!(
        after.first_strip_top > open.first_strip_top + 10.0,
        "the setup: the timeline shrank back down: {open:?} then {after:?}"
    );
    assert_stays_within(
        &during,
        "the track under the drawer",
        |places| places.second_strip_top,
        open.second_strip_top,
    );
    assert_stays_within(
        &during,
        "the slider under the top of the panel",
        Places::slider_below_the_top_of_the_panel,
        open.slider_below_the_top_of_the_panel(),
    );
}

#[test]
fn a_track_that_is_added_is_there_at_once_and_so_is_the_top_of_the_panel_with_the_slider_on_it() {
    let mut window = window_with_two_tracks();
    let before = places(&window);

    window.do_this_and_draw_nothing(crate::application::commands::Command::AddTrack);
    window.harness.step();
    let during = the_places_in_the_frames_that_follow(&mut window, 12);

    // (The panel is at its tallest with three tracks, so the new track makes it taller by as much as it can be, up to there.)
    let after = *during.last().unwrap();
    assert!(
        after.slider_top < before.slider_top - 50.0,
        "the setup: the panel grew, and the slider on its top went up: {before:?} then {after:?}"
    );
    assert_stays_within(&during, "the slider", |places| places.slider_top, after.slider_top);
    assert_stays_within(
        &during,
        "the bottom of the panel",
        |places| places.panel_bottom,
        before.panel_bottom,
    );
}

#[test]
fn the_height_the_rows_are_laid_out_to_be_changes_exactly_as_the_height_of_what_is_in_the_area_does() {
    // The panel is made as tall as the rows are going to be from what the layout of them says, so the layout and what the area finds
    // in it must not drift apart: this is what tells it if they do.
    let mut window = window_with_two_tracks();
    let rows_in_the_last_frame = |window: &Window| {
        window
            .application()
            .interface
            .timeline_view
            .rows_in_the_last_frame
            .expect("the rows have been drawn")
    };
    let at_the_start = rows_in_the_last_frame(&window);
    press_the_button_of_the_drawer_of_the_first_track(&mut window);

    let mut most_it_changed_by = 0.0_f32;
    for frame in 0..24 {
        let now = rows_in_the_last_frame(&window);
        let content_changed_by = now.content_height - at_the_start.content_height;
        let layout_changed_by = now.height_laid_out - at_the_start.height_laid_out;
        // (egui puts the sizes of what is laid out on the pixels of the screen, so they are a fraction of a point off.)
        assert!(
            (content_changed_by - layout_changed_by).abs() < 0.1,
            "frame {frame}: what is in the area changed by {content_changed_by} and the layout by {layout_changed_by}"
        );
        most_it_changed_by = most_it_changed_by.max(content_changed_by);
        window.harness.step();
    }
    assert!(
        most_it_changed_by > 10.0,
        "the setup: the rows grew: {most_it_changed_by}"
    );
}

#[test]
fn a_track_that_is_taken_away_is_gone_at_once_and_so_is_the_top_of_the_panel_with_the_slider_on_it() {
    let mut window = window_with_two_tracks();
    window.do_this(crate::application::commands::Command::AddTrack);
    let before = places(&window);
    let the_track_in_front = window.application().project().tracks()[0].id();

    window.do_this_and_draw_nothing(crate::application::commands::Command::DeleteTrack(the_track_in_front));
    window.harness.step();
    let during = the_places_in_the_frames_that_follow(&mut window, 12);

    assert_eq!(
        window.application().project().tracks().len(),
        2,
        "the setup: the track is gone"
    );
    assert!(
        during.last().unwrap().first_strip_top > before.first_strip_top + 10.0,
        "the setup: the timeline shrank: {before:?} then {:?}",
        during.last().unwrap()
    );
    let after = *during.last().unwrap();
    assert!(
        after.slider_top > before.slider_top + 50.0,
        "the setup: the slider on the top of the panel went down: {before:?} then {after:?}"
    );
    assert_stays_within(&during, "the slider", |places| places.slider_top, after.slider_top);
    assert_stays_within(
        &during,
        "the bottom of the panel",
        |places| places.panel_bottom,
        before.panel_bottom,
    );
}
