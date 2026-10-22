// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that use the real window: they send it the mouse and keyboard input a person would
//! and check what the project and the playback did. (The window and the ways of using it are in
//! [`super::test_window`].)

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{VIDEO, WINDOW_SIZE, Window, project_of, window_inside_a_sped_up_clip};
use super::{SourceFileState, SourceStatus};
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::application::preview_panel::{catcher_id, corner_handle_id, turn_place_id};
use crate::application::selection::SelectionChange;
use crate::application::timeline_panel::rows_area_id;
use crate::project::{
    Corner, FrameRate, Project, SourceFrameIndex, SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex,
};

#[test]
fn the_file_menu_has_its_items_in_the_requested_order() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("File").click();
    window.settle();

    let items = [
        "New project",
        "Save project",
        "Save project as",
        "Load project",
        "Embed and import",
        "Link and import",
        "Export",
    ];
    let heights: Vec<f32> = items.iter().map(|item| window.rectangle_of(item).top()).collect();
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "the items are listed top to bottom in this order: {heights:?}"
    );
}

#[test]
fn the_menu_bar_has_the_file_edit_and_view_menus_in_that_order_and_no_others() {
    // (The View menu was asked for on 2026-10-22, for the checkbox of the lead-up on a seek: it has that and nothing else.)
    let window = Window::showing(project_of(300));
    assert!(window.rectangle_of("File").left() < window.rectangle_of("Edit").left());
    assert!(window.rectangle_of("Edit").left() < window.rectangle_of("View").left());
    for other in ["Help", "Window", "Tools"] {
        assert!(window.harness.query_by_label(other).is_none(), "{other}");
    }
}

#[test]
fn the_edit_menu_has_deselect_then_project_settings_then_global_settings() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("Edit").click();
    window.settle();

    // (The item for Deselect has the key next to it, so it is found by what it starts with.)
    let heights = [
        window.harness.get_by_label_contains("Deselect").rect().top(),
        window.rectangle_of("Project settings").top(),
        window.rectangle_of("Global settings").top(),
    ];
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "listed top to bottom in this order: {heights:?}"
    );
}

#[test]
fn the_deselect_item_shows_the_key_that_does_the_same_and_is_only_available_when_a_clip_is_selected() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("Edit").click();
    window.settle();
    assert!(
        window.harness.query_by_label_contains("Esc").is_some(),
        "the key is shown next to the item"
    );
    assert!(
        window
            .harness
            .get_by_label_contains("Deselect")
            .accesskit_node()
            .is_disabled(),
        "nothing is selected: nothing to deselect"
    );

    // Close the menu, select the clip, and look again.
    window.press_key(Key::Escape);
    window.harness.get_by_label("Edit").click();
    window.settle();
    window.press_key(Key::Escape);
    window.click(window.middle_of_the_track_at(150));
    assert!(window.application().selected_clip().is_some());
    window.harness.get_by_label("Edit").click();
    window.settle();
    assert!(
        !window
            .harness
            .get_by_label_contains("Deselect")
            .accesskit_node()
            .is_disabled()
    );

    window.harness.get_by_label_contains("Deselect").click();
    window.settle();
    assert_eq!(
        window.application().selected_clip(),
        None,
        "choosing the item deselects"
    );
}

#[test]
fn the_play_buttons_and_the_time_are_there_with_stop_first_then_play_then_pause() {
    let window = Window::showing(project_of(300));
    for label in ["Stop", "Play", "Pause", "00:00:00.00"] {
        assert!(
            window.harness.query_by_label(label).is_some(),
            "{label} should be on the screen"
        );
    }
    let left_edges: Vec<f32> = ["Stop", "Play", "Pause", "00:00:00.00"]
        .iter()
        .map(|label| window.rectangle_of(label).left())
        .collect();
    assert!(
        left_edges.windows(2).all(|pair| pair[0] < pair[1]),
        "stop, play, pause, then the time, left to right: {left_edges:?}"
    );
}

#[test]
fn left_clicking_a_clip_selects_it_and_changes_nothing_else() {
    let mut window = Window::showing(project_of(300));
    assert!(!window.application().has_unsaved_changes());
    assert_eq!(window.application().selected_clip(), None);

    window.click(window.middle_of_the_track_at(75) + vec2(1.0, 0.0));

    assert_eq!(window.application().selected_clip(), Some(window.clip(0, 0)));
    assert_eq!(window.clips(), vec![(0, 300, 1)], "cutting is done with the S key only");
    assert!(!window.application().has_unsaved_changes(), "selecting is not an edit");
    assert_eq!(
        window.current_frame(),
        0,
        "and a click on the track does not move the playhead"
    );
}

#[test]
fn cutting_twice_at_the_same_frame_cuts_only_once() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    window.press_key_with(Modifiers::SHIFT, Key::S);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
}

#[test]
fn cutting_where_it_is_already_cut_does_nothing() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    window.application_mut().note_everything_is_saved();
    // Frame 75 is the first frame of the second clip, so cutting there again changes nothing, with Shift and S...
    window.press_key_with(Modifiers::SHIFT, Key::S);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
    assert!(!window.application().has_unsaved_changes());
    // ...or with S, of the clip that is selected and is there.
    window.click(window.middle_of_the_strip_at(0, 100));
    window.move_current_frame_to(75);
    window.press_key(Key::S);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn nothing_shows_where_a_click_on_the_track_would_have_cut() {
    let mut window = Window::showing(project_of(300));
    let on_the_track = window.middle_of_the_track_at(75);
    window.move_pointer_to(on_the_track);
    window.settle();
    // (The hover guide line and the crosshair cursor were only for left-click cutting, which is gone.)
    assert_eq!(
        window.harness.ctx.output(|output| output.cursor_icon),
        egui::CursorIcon::Default
    );
}

#[test]
fn dragging_with_the_left_button_does_not_cut() {
    let mut window = Window::showing(project_of(300));
    let from = window.middle_of_the_track_at(40);
    let to = window.middle_of_the_track_at(100);
    window.drag(from, to, PointerButton::Primary);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}

#[test]
fn the_arrow_keys_move_the_playhead_one_frame_at_a_time_while_paused() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);

    window.press_key(Key::ArrowRight);
    assert_eq!(window.current_frame(), 101);
    window.press_key(Key::ArrowRight);
    window.press_key(Key::ArrowRight);
    assert_eq!(window.current_frame(), 103);
    window.press_key(Key::ArrowLeft);
    assert_eq!(window.current_frame(), 102);
    assert!(
        !window.application().playback.clock.is_playing(),
        "moving does not start playing"
    );
    assert!(
        !window.application().has_unsaved_changes(),
        "moving the playhead is not an edit"
    );
}

#[test]
fn the_arrow_keys_stop_at_the_first_and_last_frames() {
    let mut window = Window::showing(project_of(300));
    window.press_key(Key::ArrowLeft);
    assert_eq!(window.current_frame(), 0);

    window.move_current_frame_to(299);
    window.press_key(Key::ArrowRight);
    assert_eq!(window.current_frame(), 299);
}

#[test]
fn holding_an_arrow_key_keeps_moving_and_each_repeat_counts() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    for repeat in [false, true, true, true] {
        window.harness.event(Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: Modifiers::NONE,
        });
    }
    window.harness.step();
    window.settle();
    assert_eq!(window.current_frame(), 104);
}

#[test]
fn the_arrow_keys_move_the_playhead_while_playing_and_playing_goes_on() {
    let mut window = Window::showing(project_of(300));
    let started = std::time::Instant::now();
    let total = 300;
    window
        .application_mut()
        .playback
        .clock
        .seek(started, TimelineFrameIndex::new(100), total);
    window.application_mut().playback.clock.play(started, total);

    // Carried out at the very moment playing started, so that no time has passed to confuse the count.
    let context = egui::Context::default();
    window
        .application_mut()
        .execute(Command::MoveCurrentFrameBy(1), started, &context);
    let position = |window: &Window| window.application().playback.clock.position(started, total).value();
    assert_eq!(position(&window), 101);
    assert!(window.application().playback.clock.is_playing());
    window
        .application_mut()
        .execute(Command::MoveCurrentFrameBy(-5), started, &context);
    assert_eq!(position(&window), 96);
    assert!(window.application().playback.clock.is_playing(), "still playing");
}

#[test]
fn the_arrow_keys_do_nothing_while_a_dialog_is_open() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.press_key(Key::ArrowRight);
    assert_eq!(window.current_frame(), 100);
}

#[test]
fn the_ruler_is_a_strip_of_the_same_width_directly_above_the_first_track() {
    let window = Window::showing(project_of(300));
    let ruler = window.rectangle_of("Ruler");
    let track = window.strip_of_track(0);
    assert_eq!(ruler.left(), track.left());
    assert_eq!(ruler.right(), track.right());
    assert_eq!(ruler.bottom(), track.top(), "no gap between them");
    assert_eq!(
        ruler.height(),
        crate::application::theme::TOP_ROW_HEIGHT,
        "as tall as the row above the tracks, which the button that adds a track is in too"
    );
    let button = window.rectangle_of(crate::application::timeline_panel::ADD_TRACK_LABEL);
    assert!(
        button.top() >= ruler.top() && button.bottom() <= ruler.bottom(),
        "the button is level with the ruler: {button:?} {ruler:?}"
    );
}

#[test]
fn clicking_the_ruler_moves_the_playhead_to_that_frame() {
    let mut window = Window::showing(project_of(300));

    window.click(window.point_on_the_ruler_at(75));

    assert_eq!(window.current_frame(), 75);
    assert!(!window.application().has_unsaved_changes(), "that is not an edit");
    assert_eq!(window.clips(), vec![(0, 300, 1)], "and it does not cut");
}

#[test]
fn clicking_the_ruler_while_playing_moves_the_playhead_and_playing_goes_on() {
    let mut window = Window::showing(project_of(300));
    let now = std::time::Instant::now();
    window.application_mut().playback.clock.play(now, 300);

    window.click(window.point_on_the_ruler_at(150));

    assert!(window.application().playback.clock.is_playing());
    assert!(
        (150..170).contains(&window.current_frame()),
        "playing carried on from frame 150: {}",
        window.current_frame()
    );
}

#[test]
fn dragging_along_the_ruler_moves_the_playhead_with_the_pointer() {
    let mut window = Window::showing(project_of(300));
    let from = window.point_on_the_ruler_at(30);
    let to = window.point_on_the_ruler_at(120);

    window.drag(from, to, PointerButton::Primary);

    assert_eq!(window.current_frame(), 120);
}

#[test]
fn clicking_the_ruler_past_either_end_of_the_strip_goes_to_the_first_or_last_frame() {
    // 120 frames is 4 seconds: 740 points of strip in a window about 1264 points wide.
    let mut window = Window::showing(project_of(120));
    let ruler = window.rectangle_of("Ruler");

    window.click(Pos2::new(ruler.right() - 20.0, ruler.center().y));
    assert_eq!(window.current_frame(), 119, "right of the strip is the last frame");

    window.click(Pos2::new(ruler.left() + 2.0, ruler.center().y));
    assert_eq!(window.current_frame(), 0);
}

#[test]
fn the_strip_stays_still_under_the_pointer_when_the_ruler_is_clicked() {
    // 30 seconds is about 5500 points of strip: far wider than the window, so it scrolls.
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(450);
    let scroll_before = window.application().interface.timeline_view.scroll_offset;
    assert!(
        scroll_before > 1000.0,
        "the strip follows the playhead to the middle: {scroll_before}"
    );

    // Click a place in view, some way to the right of the middle.
    let ruler = window.rectangle_of("Ruler");
    window.click(Pos2::new(ruler.center().x + 300.0, ruler.center().y));

    assert!(window.current_frame() > 450, "the playhead went where it was clicked");
    assert_eq!(
        window.application().interface.timeline_view.scroll_offset,
        scroll_before,
        "the view did not scroll to put the new frame in the middle"
    );
}

#[test]
fn moving_the_playhead_any_other_way_still_scrolls_the_strip_to_follow_it() {
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(450);
    let scroll_before = window.application().interface.timeline_view.scroll_offset;
    window.move_current_frame_to(800);
    assert!(window.application().interface.timeline_view.scroll_offset > scroll_before + 500.0);
}

#[test]
fn the_red_line_of_the_playhead_is_drawn_where_the_current_frame_starts() {
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(200);

    let drawn = window
        .x_of_the_drawn_playhead()
        .expect("the current frame is in view, so the playhead is drawn");

    // The line is put on a whole pixel, which moves it by half a point at the most.
    let starts = window.x_of_frame_on_the_strip(0, 200);
    assert!(
        (drawn - starts).abs() <= 0.5,
        "the line is at {drawn} and the frame starts at {starts}"
    );
}

#[test]
fn the_red_line_of_the_playhead_goes_along_with_the_strips_when_they_are_scrolled_and_is_not_drawn_out_of_view() {
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(450);
    let before = window.x_of_the_drawn_playhead().expect("in view to start with");

    window.application_mut().interface.timeline_view.scroll_offset += 300.0;
    window.settle();
    let after = window
        .x_of_the_drawn_playhead()
        .expect("still in view after scrolling by 300 points");
    assert!(
        ((before - after) - 300.0).abs() <= 1.0,
        "scrolled 300 points along, the line moved from {before} to {after}"
    );

    // As far along as the strips go, which leaves the middle of the video far behind.
    window.application_mut().interface.timeline_view.scroll_offset = 100_000.0;
    window.settle();
    assert_eq!(window.x_of_the_drawn_playhead(), None);
    assert_eq!(window.current_frame(), 450, "the current frame is where it was");
}

#[test]
fn the_s_key_cuts_the_selected_clip_at_the_current_frame_and_does_not_move_it() {
    let mut window = Window::showing(project_of(300));
    assert!(!window.application().has_unsaved_changes());
    window.click(window.middle_of_the_strip_at(0, 100));
    window.move_current_frame_to(120);

    window.press_key(Key::S);

    assert_eq!(window.clips(), vec![(0, 120, 1), (120, 300, 1)]);
    assert!(
        window.application().has_unsaved_changes(),
        "a cut is an edit that is not saved yet"
    );
    assert_eq!(window.current_frame(), 120, "cutting does not move the current frame");
}

#[test]
fn shift_and_the_s_key_cut_every_clip_at_the_current_frame_and_do_not_move_it() {
    let mut window = Window::showing(project_of(300));
    assert!(!window.application().has_unsaved_changes());
    window.move_current_frame_to(120);

    window.press_key_with(Modifiers::SHIFT, Key::S);

    assert_eq!(window.clips(), vec![(0, 120, 1), (120, 300, 1)]);
    assert!(window.application().has_unsaved_changes());
    assert_eq!(window.current_frame(), 120);
}

#[test]
fn the_s_key_alone_cuts_nothing_when_no_clip_is_selected() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(120);

    window.press_key(Key::S);

    assert_eq!(window.clips(), vec![(0, 300, 1)], "nothing is selected: nothing is cut");
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn the_tab_key_does_not_stop_where_there_is_nothing_to_do_by_keyboard() {
    // The preview and the empty room of the timeline take every click and drag, but cannot be seen to
    // have the focus, and Space (which plays) would be taken for a click on them as well.
    let mut window = Window::showing(project_of(300));
    let places_with_nothing_to_do = [catcher_id(), rows_area_id()];
    let mut places_it_stopped_at = Vec::new();
    for _ in 0..40 {
        window.press_key(Key::Tab);
        let focused = window.harness.ctx.memory(|memory| memory.focused());
        assert!(
            !focused.is_some_and(|id| places_with_nothing_to_do.contains(&id)),
            "the focus is at a place that has nothing to do for the keyboard: {focused:?}"
        );
        if let Some(id) = focused
            && !places_it_stopped_at.contains(&id)
        {
            places_it_stopped_at.push(id);
        }
    }
    assert!(
        places_it_stopped_at.len() >= 3,
        "(it did go to the places that do have something to do: {places_it_stopped_at:?})"
    );
}

#[test]
fn the_tab_key_does_not_stop_at_the_handles_of_the_selected_layer_either() {
    // The handles and the places to turn from take the mouse over everything, but have nothing to do for the keyboard.
    let mut window = Window::showing(project_of(300));
    let video = window.clip(0, 0);
    window.do_this(Command::select_clips(vec![video], SelectionChange::Replace));
    let places_with_nothing_to_do: Vec<egui::Id> = Corner::ALL
        .into_iter()
        .flat_map(|corner| [corner_handle_id(video, corner), turn_place_id(video, corner)])
        .collect();
    for _ in 0..40 {
        window.press_key(Key::Tab);
        let focused = window.harness.ctx.memory(|memory| memory.focused());
        assert!(
            !focused.is_some_and(|id| places_with_nothing_to_do.contains(&id)),
            "the focus is at a handle: {focused:?}"
        );
    }
}

#[test]
fn the_s_key_cuts_even_when_a_button_has_the_keyboard_focus() {
    for (modifiers, select_the_clip_first) in [(Modifiers::NONE, true), (Modifiers::SHIFT, false)] {
        let mut window = Window::showing(project_of(300));
        if select_the_clip_first {
            window.click(window.middle_of_the_strip_at(0, 100));
        }
        window.move_current_frame_to(60);
        window.harness.get_by_label("File").focus();
        window.settle();

        window.press_key_with(modifiers, Key::S);

        assert_eq!(window.clips(), vec![(0, 60, 1), (60, 300, 1)], "{modifiers:?}");
    }
}

#[test]
fn the_space_bar_does_not_cut() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(60);
    window.press_key(Key::Space);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}

#[test]
fn the_space_bar_starts_playing_when_paused_and_pauses_when_playing() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(60);
    assert!(!window.application().playback.clock.is_playing());

    window.press_key(Key::Space);
    assert!(window.application().playback.clock.is_playing(), "space plays");

    window.press_key(Key::Space);
    assert!(!window.application().playback.clock.is_playing(), "space pauses");
    assert!(
        window.current_frame() >= 60,
        "pausing keeps the current frame (it is where playing had got to)"
    );
}

#[test]
fn the_space_bar_plays_again_from_the_start_when_the_end_has_been_reached() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(299);
    window.press_key(Key::Space);
    assert!(window.application().playback.clock.is_playing());
    assert!(window.current_frame() < 299, "playing restarted from the first frame");
}

#[test]
fn the_space_bar_plays_even_when_a_button_has_the_keyboard_focus() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("File").focus();
    window.settle();
    window.press_key(Key::Space);
    assert!(window.application().playback.clock.is_playing());
}

#[test]
fn the_space_bar_does_nothing_while_a_dialog_is_open_or_with_nothing_to_play() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.press_key(Key::Space);
    assert!(!window.application().playback.clock.is_playing());

    let mut empty = Window::showing(Project::empty());
    empty.press_key(Key::Space);
    assert!(!empty.application().playback.clock.is_playing());
}

#[test]
fn the_s_key_does_nothing_while_a_dialog_is_open() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(60);
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.press_key_with(Modifiers::SHIFT, Key::S);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}

#[test]
fn the_s_key_does_nothing_with_no_video() {
    let mut window = Window::showing(Project::empty());
    window.press_key(Key::S);
    window.press_key_with(Modifiers::SHIFT, Key::S);
    assert!(window.application().document.project.has_no_clips());
    assert!(!window.application().has_unsaved_changes());
}

fn project_cut_into_three() -> Window {
    let mut window = Window::showing(project_of(900));
    window
        .application_mut()
        .document
        .project
        .split_at(TimelineFrameIndex::new(300), None);
    window
        .application_mut()
        .document
        .project
        .split_at(TimelineFrameIndex::new(600), None);
    window.application_mut().note_everything_is_saved();
    window.settle();
    window
}

#[test]
fn changing_the_speed_of_an_earlier_clip_keeps_the_picture_that_was_on_screen() {
    let mut window = project_cut_into_three();
    // The current frame is just inside the second clip.
    window.move_current_frame_to(330);
    let source_frame_shown = |window: &Window| {
        let current = TimelineFrameIndex::new(window.current_frame());
        let (_, placement_frame) = window
            .application()
            .document
            .project
            .tracks()
            .iter()
            .find_map(|track| {
                track
                    .placement_covering(current)
                    .map(|placement| (track.id(), placement))
            })
            .map(|(track, placement)| (track, placement.source_frame_at(current).unwrap()))
            .unwrap();
        placement_frame
    };
    assert_eq!(source_frame_shown(&window), SourceFrameIndex::new(330));

    let first_clip = window.clip(0, 0);
    window.do_this(Command::SetClipSpeed {
        clip: first_clip,
        speed_factor: SpeedFactor::new(10).unwrap(),
    });

    assert_eq!(window.clips()[0], (0, 300, 10));
    assert!(window.application().has_unsaved_changes());
    assert_eq!(
        source_frame_shown(&window),
        SourceFrameIndex::new(330),
        "the same picture is still on screen"
    );
    assert_eq!(
        window.current_frame(),
        30 + 30,
        "which is now 30 frames (the sped up clip) + 30 into the next one"
    );
}

#[test]
fn dragging_the_slider_along_the_bottom_moves_the_current_frame() {
    let mut window = Window::showing(project_of(901));
    let slider = window.rectangle_of("Position in the video");
    let middle = Pos2::new(slider.center().x, slider.center().y);
    let left_end = Pos2::new(slider.left() + 10.0, slider.center().y);

    window.drag(left_end, middle, PointerButton::Primary);

    let frame = window.current_frame();
    assert!(
        (440..=460).contains(&frame),
        "dragged to the middle of 901 frames: {frame}"
    );

    // The time shown follows (frame 450 of a 30 frames per second video is 15 s).
    let expected_time = crate::project::format_timecode(
        TimelineFrameIndex::new(frame),
        FrameRate::from_whole_frames_per_second(30).unwrap(),
    );
    assert!(
        window.harness.query_by_label(&expected_time).is_some(),
        "the time {expected_time} is shown"
    );
}

#[test]
fn clicking_the_slider_goes_straight_to_that_place() {
    let mut window = Window::showing(project_of(901));
    let slider = window.rectangle_of("Position in the video");
    window.click(Pos2::new(slider.right() - 9.0, slider.center().y));
    assert_eq!(window.current_frame(), 900, "the right end is the last frame");
}

#[test]
fn the_play_button_plays_and_the_pause_and_stop_buttons_stop_it() {
    let mut window = Window::showing(project_of(900));
    let now = std::time::Instant::now();
    window
        .application_mut()
        .playback
        .clock
        .seek(now, TimelineFrameIndex::new(100), 900);

    window.harness.get_by_label("Play").click();
    window.harness.step();
    assert!(window.application().playback.clock.is_playing());

    window.harness.get_by_label("Pause").click();
    window.harness.step();
    assert!(!window.application().playback.clock.is_playing());
    assert!(window.current_frame() >= 100, "pausing stays where it is");

    window.harness.get_by_label("Play").click();
    window.harness.step();
    window.harness.get_by_label("Stop").click();
    window.harness.step();
    assert!(!window.application().playback.clock.is_playing());
    assert_eq!(window.current_frame(), 0, "stop goes back to the start");
}

#[test]
fn the_buttons_do_nothing_when_there_is_no_video() {
    let mut window = Window::showing(Project::empty());
    window.harness.get_by_label("Play").click();
    window.harness.step();
    assert!(!window.application().playback.clock.is_playing());
}

#[test]
fn the_warning_about_a_missing_file_is_in_the_middle_and_the_program_stays_usable() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().sources.files.insert(
        VIDEO,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "there is no file there".to_owned(),
        }),
    );
    window.application_mut().interface.missing_sources_warning_open = true;
    window.settle();

    let warning = window.rectangle_of("Source files not found");
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]));
    assert!(
        (warning.center().x - screen.center().x).abs() < 120.0,
        "centred across: {warning:?}"
    );
    assert!(
        (warning.center().y - screen.center().y).abs() < 200.0,
        "centred down: {warning:?}"
    );
    assert!(window.harness.query_by_label_contains("must be chosen again").is_some());
    assert!(window.harness.query_by_label("Choose the file again").is_some());

    // Edits still work with the warning showing, and nothing is lost.
    window.cut_at_frame(75);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
}

#[test]
fn the_warning_lists_every_missing_file_with_where_it_was_and_why_it_cannot_be_used() {
    let mut project = project_of(300);
    project
        .import_source(super::test_window::sound_source(10), TimelineFrameIndex::new(0))
        .unwrap();
    let mut window = Window::showing(project);
    for (identifier, reason) in [
        (VIDEO, "there is no file there"),
        (super::test_window::SOUND, "it is not a sound"),
    ] {
        window.application_mut().sources.files.insert(
            identifier,
            SourceFileState::new(SourceStatus::Unavailable {
                reason: reason.to_owned(),
            }),
        );
    }
    window.application_mut().interface.missing_sources_warning_open = true;
    window.settle();

    assert!(window.harness.query_by_label_contains("holiday.mp4").is_some());
    assert!(window.harness.query_by_label_contains("song.mp3").is_some());
    assert!(window.harness.query_by_label("there is no file there").is_some());
    assert!(window.harness.query_by_label("it is not a sound").is_some());
    assert_eq!(window.harness.get_all_by_label("Choose the file again").count(), 2);
    assert!(window.application().missing_sources_warning_is_showing());
}

#[test]
fn the_warning_is_gone_when_the_file_is_there_and_does_not_come_back_by_itself() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().interface.missing_sources_warning_open = true;
    window.settle();
    assert!(
        window.harness.query_by_label("Source files not found").is_none(),
        "every file can be used: nothing to warn about"
    );
    assert!(!window.application().missing_sources_warning_is_showing());
}

#[test]
fn the_import_item_is_available_whatever_state_the_files_of_the_project_are_in() {
    for status in [
        SourceStatus::Unavailable {
            reason: "gone".to_owned(),
        },
        SourceStatus::Available {
            location: "holiday.mp4".into(),
        },
        SourceStatus::Opening,
    ] {
        let mut window = Window::showing(project_of(300));
        window
            .application_mut()
            .sources
            .files
            .insert(VIDEO, SourceFileState::new(status.clone()));
        window.application_mut().interface.missing_sources_warning_open = false;
        window.settle();
        window.harness.get_by_label("File").click();
        window.settle();
        for item in ["Embed and import", "Link and import"] {
            let import = window.harness.get_by_label(item);
            assert!(!import.accesskit_node().is_disabled(), "{item}: {status:?}");
        }
    }
}

#[test]
fn asking_for_a_new_project_with_unsaved_changes_asks_first_and_cancelling_keeps_everything() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    assert!(window.application().has_unsaved_changes());

    window
        .application_mut()
        .ask_before_discarding(AfterConfirmation::NewProject);
    window.settle();
    assert!(window.harness.query_by_label("Discard unsaved changes?").is_some());

    window.harness.get_by_label("Cancel").click();
    window.settle();
    assert!(window.harness.query_by_label("Discard unsaved changes?").is_none());
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)], "nothing was discarded");
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn a_click_outside_a_dialog_closes_it_and_the_dialog_blocks_the_track_while_it_is_up() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    // The clips of the track cannot be used behind the dialog.
    let on_the_track = window.middle_of_the_track_at(75);
    window.click(on_the_track);
    assert!(
        window.application().selected_clip().is_none(),
        "the click did not reach the clip"
    );

    window.click(Pos2::new(5.0, 400.0));
    assert!(
        window.application().interface.dialog.is_none(),
        "clicking outside a message closes it"
    );

    // With the dialog gone, the same click selects the clip.
    window.click(on_the_track);
    assert!(window.application().selected_clip().is_some());
}

#[test]
fn the_ok_button_closes_a_message() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    assert!(window.harness.query_by_label("Read me").is_some());

    window.harness.get_by_label("OK").click();
    window.settle();

    assert!(window.application().interface.dialog.is_none());
    assert!(window.harness.query_by_label("Read me").is_none());
}

#[test]
fn a_click_outside_the_discard_question_means_no_and_keeps_the_changes() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    window
        .application_mut()
        .ask_before_discarding(AfterConfirmation::NewProject);
    window.settle();

    window.click(Pos2::new(5.0, 400.0));

    assert!(window.application().interface.dialog.is_none());
    assert!(window.application().has_unsaved_changes());
    assert_eq!(window.clips().len(), 2, "nothing was discarded");
}

#[test]
fn the_fast_forward_symbol_is_only_there_inside_sped_up_clips() {
    let mut window = Window::showing(project_of(300));
    assert!(
        window.harness.query_by_label("Fast-forward symbol").is_none(),
        "normal speed"
    );

    let clip = window.clip(0, 0);
    window
        .application_mut()
        .document
        .project
        .set_clip_speed(clip, SpeedFactor::new(4).unwrap())
        .unwrap();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .playback
        .clock
        .seek(now, TimelineFrameIndex::new(10), 75);
    window.settle();
    assert!(window.harness.query_by_label("Fast-forward symbol").is_some(), "4x");
}

#[test]
fn dragging_the_symbol_moves_it_for_the_whole_project() {
    let mut window = window_inside_a_sped_up_clip();
    let before = window.application().document.project.speed_indicator_layout();
    let symbol = window.rectangle_of("Fast-forward symbol");

    window.drag(
        symbol.center(),
        symbol.center() + vec2(200.0, 100.0),
        PointerButton::Primary,
    );

    let after = window.application().document.project.speed_indicator_layout();
    assert!(
        after.left_fraction_of_frame_width() > before.left_fraction_of_frame_width() + 0.1,
        "{before:?} -> {after:?}"
    );
    assert!(
        after.top_fraction_of_frame_height() > before.top_fraction_of_frame_height() + 0.1,
        "{before:?} -> {after:?}"
    );
    assert_eq!(
        after.height_fraction_of_frame_height(),
        before.height_fraction_of_frame_height(),
        "moving does not resize"
    );
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn dragging_the_corner_of_the_symbol_resizes_it() {
    let mut window = window_inside_a_sped_up_clip();
    let before = window.application().document.project.speed_indicator_layout();
    let symbol_before = window.rectangle_of("Fast-forward symbol");
    let corner = symbol_before.right_bottom() - vec2(4.0, 4.0);

    window.drag(corner, corner + vec2(60.0, 60.0), PointerButton::Primary);

    let after = window.application().document.project.speed_indicator_layout();
    assert!(
        after.height_fraction_of_frame_height() > before.height_fraction_of_frame_height() * 1.5,
        "{before:?} -> {after:?}"
    );
    let symbol_after = window.rectangle_of("Fast-forward symbol");
    assert!(
        symbol_after.height() > symbol_before.height() * 1.5,
        "it is bigger on screen too"
    );
    assert!(
        (symbol_after.left_top() - symbol_before.left_top()).length() < 1.5,
        "it grows from its top-left corner, which stays where it was: {symbol_before:?} -> {symbol_after:?}"
    );
}

#[test]
fn the_symbol_cannot_be_dragged_out_of_the_picture() {
    let mut window = window_inside_a_sped_up_clip();
    let symbol = window.rectangle_of("Fast-forward symbol");
    window.drag(
        symbol.center(),
        symbol.center() + vec2(5000.0, 5000.0),
        PointerButton::Primary,
    );
    let after = window.application().document.project.speed_indicator_layout();
    assert!(after.left_fraction_of_frame_width() <= 1.0 && after.top_fraction_of_frame_height() <= 1.0);
    // And it is still entirely on the picture.
    let symbol = window.rectangle_of("Fast-forward symbol");
    assert!(
        symbol.right() <= WINDOW_SIZE[0] && symbol.bottom() <= WINDOW_SIZE[1],
        "{symbol:?}"
    );
}

#[test]
fn a_changed_symbol_layout_is_remembered_in_the_project() {
    let mut window = window_inside_a_sped_up_clip();
    let symbol = window.rectangle_of("Fast-forward symbol");
    window.drag(
        symbol.center(),
        symbol.center() + vec2(100.0, 0.0),
        PointerButton::Primary,
    );
    assert_ne!(
        window.application().document.project.speed_indicator_layout(),
        SpeedIndicatorLayout::DEFAULT
    );
}

#[test]
fn the_wheel_over_the_strip_scrolls_it_without_moving_the_current_frame() {
    let mut window = Window::showing(project_of(900));
    let track = window.strip_of_track(0);
    window.move_pointer_to(track.center());
    let scroll_before = window.application().interface.timeline_view.scroll_offset;

    window.harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -300.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    window.settle();

    assert!(
        window.application().interface.timeline_view.scroll_offset > scroll_before,
        "the strip moved along"
    );
    assert_eq!(window.current_frame(), 0, "the current frame did not");
}
