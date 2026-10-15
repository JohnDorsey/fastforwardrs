// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the tracks: their title boxes, the round button that adds one, the order they are in,
//! and deleting one. (Selecting a track is in `track_selection_tests`, and the settings of a track, which are
//! in the properties panel, in `properties_panel_tests`.)

use egui::{Key, Modifiers, Pos2, Rect};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::timeline_panel::ADD_TRACK_LABEL;

fn add_a_track(window: &mut Window) {
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    window.click(button.center());
}

// --- the list of tracks ---

#[test]
fn a_new_project_has_one_track_called_track_0_with_a_title_box_at_its_left() {
    let window = Window::showing(project_of(300));
    assert_eq!(window.application().project().tracks().len(), 1);
    assert_eq!(window.title_of_track(0), "track 0");
    let title_box = window.title_box_of_track(0);
    let strip = window.strip_of_track(0);
    assert!(title_box.right() < strip.left(), "at the left of the strip");
    assert!(
        title_box.center().y > strip.top() && title_box.center().y < strip.bottom(),
        "level with the strip"
    );
    assert_eq!(
        window.text_field_showing("track 0").rect(),
        title_box,
        "the title is shown in the title box"
    );
}

#[test]
fn the_big_round_button_is_above_the_title_box_of_the_top_track_and_big_and_round() {
    let window = Window::showing(project_of(300));
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    let title_box = window.title_box_of_track(0);
    let strip = window.strip_of_track(0);
    assert!(
        button.bottom() <= title_box.top(),
        "above the top track's title box: {button:?} {title_box:?}"
    );
    assert!(
        (button.center().x - title_box.center().x).abs() < 40.0 && button.center().x < strip.left(),
        "over the title boxes, at the left of the strips: {button:?}"
    );
    assert!(button.width() >= 32.0, "big: {}", button.width());
    assert!((button.width() - button.height()).abs() < 0.5, "round: {button:?}");
    assert!(
        window.rectangle_of("Ruler").bottom() <= strip.top(),
        "and it is in the row of the ruler, which is over the strips"
    );
}

#[test]
fn pressing_the_button_adds_a_track_in_front_of_the_others_with_the_next_title() {
    let mut window = Window::showing(project_of(300));
    let old_track = window.application().project().tracks()[0].id();
    add_a_track(&mut window);

    let tracks = window.application().project().tracks();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title(), "track 1", "the new track is in front, at the top");
    assert_eq!(tracks[1].id(), old_track);
    assert!(tracks[0].is_empty());
    assert!(window.application().has_unsaved_changes());

    add_a_track(&mut window);
    let titles: Vec<String> = (0..3).map(|position| window.title_of_track(position)).collect();
    assert_eq!(titles, ["track 2", "track 1", "track 0"], "titles go on counting");
}

#[test]
fn tracks_higher_in_the_list_are_higher_on_the_screen() {
    let mut window = Window::showing(project_of(300));
    add_a_track(&mut window);
    add_a_track(&mut window);
    let tops: Vec<f32> = (0..3).map(|position| window.strip_of_track(position).top()).collect();
    assert!(
        tops.windows(2).all(|pair| pair[0] < pair[1]),
        "front to back, top to bottom: {tops:?}"
    );
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    assert!(
        button.bottom() <= window.title_box_of_track(0).top(),
        "and the button is over the first, which is at the top"
    );
}

#[test]
fn every_strip_has_the_same_scale_and_the_ruler_is_over_all_of_them() {
    let mut window = Window::showing(project_of(300));
    add_a_track(&mut window);
    let ruler = window.rectangle_of("Ruler");
    for position in 0..2 {
        let strip = window.strip_of_track(position);
        assert_eq!(
            (strip.left(), strip.right()),
            (ruler.left(), ruler.right()),
            "track {position}"
        );
    }
    assert_eq!(
        window.strip_of_track(0).bottom().round() + 6.0,
        window.strip_of_track(1).top().round()
    );
}

#[test]
fn with_more_tracks_than_fit_the_rows_scroll_and_the_button_can_still_be_pressed() {
    let mut window = Window::showing(project_of(300));
    for _ in 0..6 {
        add_a_track(&mut window);
    }
    assert_eq!(window.application().project().tracks().len(), 7);
    let screen = Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(super::test_window::WINDOW_SIZE[0], super::test_window::WINDOW_SIZE[1]),
    );
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    assert!(screen.contains_rect(button), "the button is on the screen: {button:?}");
    // The rows are in a scrolling area of a limited height, so that the preview keeps its room.
    let first = window.strip_of_track(0);
    let ruler = window.rectangle_of("Ruler");
    assert_eq!(first.top(), ruler.bottom());
    assert!(
        window.the_bottom_of_what_can_be_seen_of_the_rows() - first.top()
            < 4.0 * crate::project::TrackHeight::DEFAULT_IN_POINTS,
        "at most a few tracks are shown at once"
    );
    add_a_track(&mut window);
    assert_eq!(window.application().project().tracks().len(), 8);
    assert_eq!(
        window.rectangle_of(ADD_TRACK_LABEL),
        button,
        "the button stays where it is, above the rows that scroll"
    );
}

#[test]
fn alt_and_the_wheel_scroll_the_rows_up_and_down_and_the_wheel_alone_scrolls_the_strips_along() {
    let mut window = Window::showing(project_of(900));
    for _ in 0..6 {
        add_a_track(&mut window);
    }
    let strip_top_before = window.strip_of_track(0).top();
    let horizontal_before = window.application().timeline_view.scroll_offset;
    let over_the_strips = window.strip_of_track(0).center();

    window.scroll_the_wheel(over_the_strips, egui::vec2(0.0, -150.0), Modifiers::ALT);

    assert!(
        window.strip_of_track(0).top() < strip_top_before - 50.0,
        "the rows moved up: {} then {}",
        strip_top_before,
        window.strip_of_track(0).top()
    );
    assert_eq!(
        window.application().timeline_view.scroll_offset,
        horizontal_before,
        "and the strips did not scroll along"
    );

    // Back up again, with the pointer over the title boxes this time (the first row has been scrolled
    // out of sight, so it is the column of boxes just under the ruler that is used).
    let over_the_title_boxes = Pos2::new(
        window.rectangle_of(ADD_TRACK_LABEL).center().x,
        window.rectangle_of("Ruler").bottom() + 40.0,
    );
    window.scroll_the_wheel(over_the_title_boxes, egui::vec2(0.0, 400.0), Modifiers::ALT);
    assert_eq!(window.strip_of_track(0).top(), strip_top_before, "all the way back");

    // The wheel by itself still scrolls along and not up and down.
    window.scroll_the_wheel(over_the_strips, egui::vec2(0.0, -150.0), Modifiers::NONE);
    assert!(window.application().timeline_view.scroll_offset > horizontal_before);
    assert_eq!(
        window.strip_of_track(0).top(),
        strip_top_before,
        "the rows did not move"
    );
}

#[test]
fn rows_that_are_scrolled_down_with_alt_and_the_wheel_stop_at_the_last_one() {
    let mut window = Window::showing(project_of(300));
    for _ in 0..6 {
        add_a_track(&mut window);
    }
    let over_the_strips = window.strip_of_track(0).center();
    window.scroll_the_wheel(over_the_strips, egui::vec2(0.0, -100_000.0), Modifiers::ALT);
    let last_strip = window.strip_of_track(6);
    let bottom_of_the_view = window.the_bottom_of_what_can_be_seen_of_the_rows();
    assert!(
        last_strip.bottom() <= bottom_of_the_view + 1.0,
        "the last row is fully shown: {last_strip:?} in a view that ends at {bottom_of_the_view}"
    );
    assert!(
        bottom_of_the_view - last_strip.bottom() < 20.0,
        "and the rows did not scroll on past it into empty room: {last_strip:?} in a view that ends at {bottom_of_the_view}"
    );
}

// --- the title ---

#[test]
fn typing_in_the_title_box_renames_the_track() {
    let mut window = Window::showing(project_of(300));
    window.replace_text_in_the_field_showing("track 0", "Music");
    assert_eq!(window.title_of_track(0), "Music");
    assert!(window.application().has_unsaved_changes());
    // The other things that are called by the title are called by the new one.
    assert!(window.harness.query_by_label("Strip of Music").is_some());
}

#[test]
fn the_keys_of_the_keyboard_are_not_shortcuts_while_the_title_is_being_typed() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    window.click(window.title_box_of_track(0).center());

    for key in [Key::Space, Key::S, Key::Delete, Key::ArrowLeft, Key::Backspace] {
        window.press_key(key);
    }
    window.press_key_with(Modifiers::SHIFT, Key::S);

    assert!(!window.application().is_playing(), "space did not play");
    assert_eq!(window.clips(), vec![(0, 300, 1)], "S did not cut");
    assert_eq!(window.current_frame(), 100, "the arrow did not move the playhead");
}

#[test]
fn clicking_somewhere_else_ends_the_typing_and_the_keys_are_shortcuts_again() {
    let mut window = Window::showing(project_of(300));
    window.click(window.title_box_of_track(0).center());
    // A click on the preview (empty space) takes the keyboard away from the title.
    window.click(Pos2::new(640.0, 120.0));
    window.press_key(Key::Space);
    assert!(window.application().is_playing(), "space plays again");
}

// --- deleting a track ---

const DELETE_TRACK: &str = "Delete track";

fn right_click_the_title_box_of_track(window: &mut Window, position: usize) {
    let title_box = window.title_box_of_track(position);
    window.right_click(title_box.center());
}

/// A window with a video of 300 frames on the track behind and a sound file (shorter) on the one in front.
fn window_with_a_video_and_a_sound() -> Window {
    Window::showing(project_with_a_video_and_a_sound(300, 5))
}

#[test]
fn right_clicking_the_title_box_of_a_track_offers_to_delete_it_and_choosing_that_takes_it_and_its_clips_away() {
    let mut window = window_with_a_video_and_a_sound();
    let sound_track = window.application().project().tracks()[0].id();
    assert_eq!(window.application().project().tracks().len(), 2);
    window.application_mut().note_everything_is_saved();

    right_click_the_title_box_of_track(&mut window, 0);
    assert!(
        window.harness.query_by_label(DELETE_TRACK).is_some(),
        "the menu offers it"
    );
    assert_eq!(
        window.application().project().tracks().len(),
        2,
        "opening the menu changes nothing"
    );
    window.harness.get_by_label(DELETE_TRACK).click();
    window.settle();

    let tracks = window.application().project().tracks();
    assert_eq!(tracks.len(), 1);
    assert_ne!(
        tracks[0].id(),
        sound_track,
        "the one in front is gone, the video track is left"
    );
    assert!(window.application().has_unsaved_changes());
    assert!(window.harness.query_by_label(DELETE_TRACK).is_none(), "the menu closed");
}

#[test]
fn the_box_beside_the_title_has_the_same_menu() {
    let mut window = window_with_a_video_and_a_sound();
    let title_box = window.title_box_of_track(0);
    // Beside the title there is only the box itself.
    window.right_click(Pos2::new(title_box.left() - 4.0, title_box.center().y));
    assert!(window.harness.query_by_label(DELETE_TRACK).is_some());
}

#[test]
fn the_only_track_cannot_be_deleted_so_the_menu_item_is_there_but_cannot_be_used() {
    let mut window = Window::showing(project_of(300));
    right_click_the_title_box_of_track(&mut window, 0);
    let item = window.harness.get_by_label(DELETE_TRACK);
    assert!(item.accesskit_node().is_disabled(), "disabled");
    window.harness.get_by_label(DELETE_TRACK).click();
    window.settle();
    assert_eq!(window.application().project().tracks().len(), 1);
}

#[test]
fn deleting_a_track_is_one_step_of_the_history_and_undoing_it_brings_back_the_track_and_its_clips() {
    let mut window = window_with_a_video_and_a_sound();
    let before = window.application().project().clone();

    right_click_the_title_box_of_track(&mut window, 0);
    window.harness.get_by_label(DELETE_TRACK).click();
    window.settle();
    assert_eq!(window.application().project().tracks().len(), 1);

    window.do_this(crate::application::commands::Command::Undo);
    assert_eq!(window.application().project(), &before, "everything is as it was");
    window.do_this(crate::application::commands::Command::Redo);
    assert_eq!(window.application().project().tracks().len(), 1);
}

#[test]
fn what_was_selected_on_a_deleted_track_is_not_selected_any_more_and_the_playhead_stays_in_range() {
    let mut window = window_with_a_video_and_a_sound();
    // The sound is 5 seconds, the video 10: the playhead at the end of the video is past what is left.
    let sound = window.clip(0, 0);
    let video = window.clip(1, 0);
    window.do_this(crate::application::commands::Command::select_clips(
        vec![sound, video],
        crate::application::selection::SelectionChange::Replace,
    ));
    assert_eq!(window.application().selected_clips().len(), 2);

    right_click_the_title_box_of_track(&mut window, 0);
    window.harness.get_by_label(DELETE_TRACK).click();
    window.settle();

    assert_eq!(
        window.application().selected_clips(),
        vec![video],
        "only what is still there"
    );
}
