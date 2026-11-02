// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the eye in the box of a track: after the title, on its line; open while the track is seen and closed while it is hidden; a click on it is one step of the history that changes
//! nothing but whether the track is seen, and does not select the track or begin to type in its title.

use super::test_window::{Window, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::timeline_panel::{HIDE_TRACK_LABEL, SHOW_TRACK_LABEL};

fn window_with_two_tracks() -> Window {
    Window::showing(project_with_a_video_and_a_sound(300, 5))
}

fn is_hidden(window: &Window, position: usize) -> bool {
    window.application().project().tracks()[position].is_hidden()
}

#[test]
fn every_track_has_an_open_eye_after_its_title_on_the_same_line_and_inside_its_box() {
    let window = window_with_two_tracks();

    for position in 0..2 {
        let (eye, title, track_box) = (
            window.eye_of_track(position),
            window.title_box_of_track(position),
            window.box_of_track(position),
        );
        assert!(eye.left() >= title.right(), "after the title: {eye:?} and {title:?}");
        assert!(
            (eye.center().y - title.center().y).abs() < 1.0,
            "on the line of the title: {eye:?} and {title:?}"
        );
        assert!(
            track_box.contains_rect(eye),
            "inside the box of the track: {eye:?} in {track_box:?}"
        );
        assert!(title.width() > 80.0, "the title keeps room for a name: {title:?}");
        assert!(
            eye.width() >= 16.0 && eye.height() >= 16.0,
            "big enough to hit: {eye:?}"
        );
    }
    assert_eq!(
        window.how_many_are_called(HIDE_TRACK_LABEL),
        2,
        "open, and so it offers to hide"
    );
    assert_eq!(window.how_many_are_called(SHOW_TRACK_LABEL), 0);
}

#[test]
fn a_click_on_the_eye_hides_the_track_and_the_eye_closes_and_another_click_shows_it_again() {
    let mut window = window_with_two_tracks();

    window.click(window.eye_of_track(1).center());

    assert!(is_hidden(&window, 1));
    assert!(!is_hidden(&window, 0), "the other track is as it was");
    assert_eq!(window.how_many_are_called(SHOW_TRACK_LABEL), 1, "one eye is closed");
    assert_eq!(window.how_many_are_called(HIDE_TRACK_LABEL), 1, "and the other is open");
    // (The same eye, in the same place: it is found again by where the track is.)
    window.click(window.eye_of_track(1).center());

    assert!(!is_hidden(&window, 1));
    assert_eq!(window.how_many_are_called(HIDE_TRACK_LABEL), 2);
}

#[test]
fn hiding_a_track_and_showing_it_are_each_a_step_of_the_history() {
    let mut window = window_with_two_tracks();
    window.application_mut().note_everything_is_saved();

    window.click(window.eye_of_track(0).center());
    assert!(is_hidden(&window, 0));
    assert!(window.application().can_undo());

    window.do_this(Command::Undo);
    assert!(!is_hidden(&window, 0), "undone");
    assert!(!window.application().can_undo(), "it was one step");

    window.do_this(Command::Redo);
    assert!(is_hidden(&window, 0), "done again");
}

#[test]
fn a_click_on_the_eye_does_not_select_the_track_or_begin_to_type_in_its_title() {
    let mut window = window_with_two_tracks();
    assert!(window.application().selected_items().is_empty());

    window.click(window.eye_of_track(1).center());

    assert!(
        window.application().selected_items().is_empty(),
        "{:?}",
        window.application().selected_items()
    );
    assert!(!window.text_field_showing(&window.title_of_track(1)).is_focused());
}

#[test]
fn hiding_a_track_and_showing_it_again_with_the_eye_leaves_the_project_exactly_as_it_was() {
    let mut window = window_with_two_tracks();
    let before = window.application().project().clone();

    window.click(window.eye_of_track(1).center());
    window.click(window.eye_of_track(1).center());

    assert_eq!(window.application().project(), &before, "nothing else was changed");
}
