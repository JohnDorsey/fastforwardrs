//! Tests that use the real window: they send it the mouse and keyboard input a person would
//! and check what the project and the playback did. (The window and the ways of using it are in
//! [`super::test_window`].)

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::kittest::{NodeT, Queryable};

use super::SourceStatus;
use super::test_window::{WINDOW_SIZE, Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::{AfterConfirmation, Dialog};
use crate::project::{FrameRate, Project, SourceFrameIndex, SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex};

#[test]
fn the_file_menu_has_the_five_items_in_the_requested_order() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("File").click();
    window.settle();

    let items = ["New project", "Save project", "Load project", "Import", "Export"];
    let heights: Vec<f32> = items.iter().map(|item| window.rectangle_of(item).top()).collect();
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "the items are listed top to bottom in this order: {heights:?}"
    );
}

#[test]
fn the_menu_bar_has_the_file_and_edit_menus_in_that_order_and_no_others() {
    let window = Window::showing(project_of(300));
    assert!(window.rectangle_of("File").left() < window.rectangle_of("Edit").left());
    for other in ["View", "Help", "Window", "Tools"] {
        assert!(window.harness.query_by_label(other).is_none(), "{other}");
    }
}

#[test]
fn the_edit_menu_has_project_settings_then_global_settings() {
    let mut window = Window::showing(project_of(300));
    window.harness.get_by_label("Edit").click();
    window.settle();

    let items = ["Project settings", "Global settings"];
    let heights: Vec<f32> = items.iter().map(|item| window.rectangle_of(item).top()).collect();
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "listed top to bottom in this order: {heights:?}"
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
fn left_clicking_the_video_track_does_nothing() {
    let mut window = Window::showing(project_of(300));
    assert!(!window.application().has_unsaved_changes);

    window.click(window.middle_of_the_track_at(75) + vec2(1.0, 0.0));

    assert_eq!(window.clips(), vec![(0, 300, 1)], "cutting is done with the S key only");
    assert!(!window.application().has_unsaved_changes);
    assert_eq!(
        window.current_frame(),
        0,
        "and a click on the track does not move the playhead"
    );
}

#[test]
fn pressing_s_twice_at_the_same_frame_cuts_only_once() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    window.press_key(Key::S);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
}

#[test]
fn pressing_s_where_it_is_already_cut_does_nothing() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    window.application_mut().has_unsaved_changes = false;
    // Frame 75 is the first frame of the second clip, so cutting there again changes nothing.
    window.press_key(Key::S);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
    assert!(!window.application().has_unsaved_changes);
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
        !window.application().clock.is_playing(),
        "moving does not start playing"
    );
    assert!(
        !window.application().has_unsaved_changes,
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
        .clock
        .seek(started, TimelineFrameIndex::new(100), total);
    window.application_mut().clock.play(started, total);

    // Carried out at the very moment playing started, so that no time has passed to confuse the count.
    let context = egui::Context::default();
    window
        .application_mut()
        .execute(Command::MoveCurrentFrameBy(1), started, &context);
    let position = |window: &Window| window.application().clock.position(started, total).value();
    assert_eq!(position(&window), 101);
    assert!(window.application().clock.is_playing());
    window
        .application_mut()
        .execute(Command::MoveCurrentFrameBy(-5), started, &context);
    assert_eq!(position(&window), 96);
    assert!(window.application().clock.is_playing(), "still playing");
}

#[test]
fn the_arrow_keys_do_nothing_while_a_dialog_is_open() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    window.application_mut().dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.press_key(Key::ArrowRight);
    assert_eq!(window.current_frame(), 100);
}

#[test]
fn the_ruler_is_a_strip_of_the_same_width_directly_above_the_video_track() {
    let window = Window::showing(project_of(300));
    let ruler = window.rectangle_of("Ruler");
    let track = window.rectangle_of("Video track");
    assert_eq!(ruler.left(), track.left());
    assert_eq!(ruler.right(), track.right());
    assert_eq!(ruler.bottom(), track.top(), "no gap between them");
    assert!(
        ruler.height() > 8.0 && ruler.height() < 40.0,
        "a small strip: {}",
        ruler.height()
    );
}

#[test]
fn clicking_the_ruler_moves_the_playhead_to_that_frame() {
    let mut window = Window::showing(project_of(300));

    window.click(window.point_on_the_ruler_at(75));

    assert_eq!(window.current_frame(), 75);
    assert!(!window.application().has_unsaved_changes, "that is not an edit");
    assert_eq!(window.clips(), vec![(0, 300, 1)], "and it does not cut");
}

#[test]
fn clicking_the_ruler_while_playing_moves_the_playhead_and_playing_goes_on() {
    let mut window = Window::showing(project_of(300));
    let now = std::time::Instant::now();
    window.application_mut().clock.play(now, 300);

    window.click(window.point_on_the_ruler_at(150));

    assert!(window.application().clock.is_playing());
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
    let scroll_before = window.application().timeline_view.scroll_offset;
    assert!(
        scroll_before > 1000.0,
        "the strip follows the playhead to the middle: {scroll_before}"
    );

    // Click a place in view, some way to the right of the middle.
    let ruler = window.rectangle_of("Ruler");
    window.click(Pos2::new(ruler.center().x + 300.0, ruler.center().y));

    assert!(window.current_frame() > 450, "the playhead went where it was clicked");
    assert_eq!(
        window.application().timeline_view.scroll_offset,
        scroll_before,
        "the view did not scroll to put the new frame in the middle"
    );
}

#[test]
fn moving_the_playhead_any_other_way_still_scrolls_the_strip_to_follow_it() {
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(450);
    let scroll_before = window.application().timeline_view.scroll_offset;
    window.move_current_frame_to(800);
    assert!(window.application().timeline_view.scroll_offset > scroll_before + 500.0);
}

#[test]
fn the_s_key_cuts_at_the_current_frame_and_does_not_move_it() {
    let mut window = Window::showing(project_of(300));
    assert!(!window.application().has_unsaved_changes);
    window.move_current_frame_to(120);

    window.press_key(Key::S);

    assert_eq!(window.clips(), vec![(0, 120, 1), (120, 300, 1)]);
    assert!(
        window.application().has_unsaved_changes,
        "a cut is an edit that is not saved yet"
    );
    assert_eq!(window.current_frame(), 120, "cutting does not move the current frame");
}

#[test]
fn the_s_key_cuts_even_when_a_button_has_the_keyboard_focus() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(60);
    window.harness.get_by_label("File").focus();
    window.settle();

    window.press_key(Key::S);

    assert_eq!(window.clips(), vec![(0, 60, 1), (60, 300, 1)]);
}

#[test]
fn the_space_bar_no_longer_cuts() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(60);
    window.press_key(Key::Space);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}

#[test]
fn the_s_key_does_nothing_while_a_dialog_is_open() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(60);
    window.application_mut().dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.press_key(Key::S);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}

#[test]
fn the_s_key_does_nothing_with_no_video() {
    let mut window = Window::showing(Project::empty());
    window.press_key(Key::S);
    assert!(window.application().project.video_track().is_empty());
    assert!(!window.application().has_unsaved_changes);
}

fn project_cut_into_three() -> Window {
    let mut window = Window::showing(project_of(900));
    window
        .application_mut()
        .project
        .split_at(TimelineFrameIndex::new(300))
        .unwrap();
    window
        .application_mut()
        .project
        .split_at(TimelineFrameIndex::new(600))
        .unwrap();
    window.application_mut().has_unsaved_changes = false;
    window.settle();
    window
}

#[test]
fn dragging_the_right_mouse_button_up_on_a_clip_sets_how_fast_it_plays() {
    let mut window = project_cut_into_three();
    // The middle clip (frames 300..600) starts at 10 s: well inside the first screenful? 10 s is
    // 1849 points along the strip, so scroll first: put the current frame there.
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(450), 900);
    window.settle();
    let track = window.rectangle_of("Video track");
    let press = Pos2::new(track.center().x, track.center().y);

    // Dragging 200 points up is one tenfold on the slider (200 points per tenfold at most).
    window.drag(press, press - vec2(0.0, 200.0), PointerButton::Secondary);

    assert_eq!(window.clips(), vec![(0, 300, 1), (300, 600, 10), (600, 900, 1)]);
    assert!(window.application().has_unsaved_changes);
}

#[test]
fn a_shorter_drag_gives_a_smaller_speed_and_dragging_back_down_gives_normal_speed() {
    let mut window = project_cut_into_three();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(450), 900);
    window.settle();
    let track = window.rectangle_of("Video track");
    let press = track.center();

    // Halfway up a tenfold is the square root of ten: 3.16, which is the whole factor 3.
    window.drag(press, press - vec2(0.0, 100.0), PointerButton::Secondary);
    assert_eq!(window.clips()[1].2, 3);

    // The slider starts at the clip's own speed (3x, which is 95 points up a tenfold), so dragging
    // down by less than that only slows it a little...
    window.drag(press, press + vec2(0.0, 60.0), PointerButton::Secondary);
    assert_eq!(window.clips()[1].2, 2);

    // ...and dragging down past the bottom gives normal speed.
    window.drag(press, press + vec2(0.0, 150.0), PointerButton::Secondary);
    assert_eq!(window.clips()[1].2, 1);
}

#[test]
fn pressing_the_right_button_and_letting_go_without_moving_changes_nothing() {
    let mut window = project_cut_into_three();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(450), 900);
    window.settle();
    window
        .application_mut()
        .project
        .set_clip_speed(1, SpeedFactor::new(5).unwrap())
        .unwrap();
    window.application_mut().has_unsaved_changes = false;
    window.settle();
    let press = window.rectangle_of("Video track").center();

    window.drag(press, press, PointerButton::Secondary);

    assert_eq!(window.clips()[1].2, 5);
    assert!(!window.application().has_unsaved_changes);
}

#[test]
fn the_slider_starts_at_the_speed_the_clip_already_has() {
    let mut window = project_cut_into_three();
    window
        .application_mut()
        .project
        .set_clip_speed(1, SpeedFactor::new(10).unwrap())
        .unwrap();
    // The middle clip now takes the timeline from frame 300 to frame 330: go to the middle of it,
    // so that it is the clip in the middle of the strip.
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(315), 630);
    window.settle();
    let press = window.rectangle_of("Video track").center();

    // Another tenfold up from 10x is 100x.
    window.drag(press, press - vec2(0.0, 200.0), PointerButton::Secondary);

    assert_eq!(window.clips()[1].2, 100);
}

#[test]
fn the_arrow_keys_change_the_speed_by_one_while_the_slider_is_up() {
    let mut window = project_cut_into_three();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(450), 900);
    window.settle();
    let press = window.rectangle_of("Video track").center();
    let end = press - vec2(0.0, 200.0); // 10x

    window.drag_and_hold(press, end, PointerButton::Secondary);
    window.harness.key_press(Key::ArrowUp);
    window.harness.step();
    window.harness.key_press(Key::ArrowUp);
    window.harness.step();
    window.harness.key_press(Key::ArrowDown);
    window.harness.step();
    window.release(end, PointerButton::Secondary);
    window.settle();

    assert_eq!(window.clips()[1].2, 11, "10x, up twice and down once");
}

#[test]
fn speeding_up_an_earlier_clip_keeps_the_picture_that_was_on_screen() {
    let mut window = project_cut_into_three();
    let now = std::time::Instant::now();
    // The current frame is just inside the second clip: the strip scrolls so that the end of the
    // first clip is on screen too.
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(330), 900);
    window.settle();
    let source_frame_shown = |window: &Window| {
        let current = TimelineFrameIndex::new(window.current_frame());
        window
            .application()
            .project
            .video_track()
            .locate(current)
            .unwrap()
            .source_frame
    };
    assert_eq!(source_frame_shown(&window), SourceFrameIndex::new(330));

    // Speed the first clip up by pressing on its visible part, which is the left of the strip.
    let track = window.rectangle_of("Video track");
    let press = Pos2::new(track.left() + 100.0, track.center().y);
    window.drag(press, press - vec2(0.0, 200.0), PointerButton::Secondary);

    assert_eq!(window.clips()[0], (0, 300, 10));
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
        .clock
        .seek(now, TimelineFrameIndex::new(100), 900);

    window.harness.get_by_label("Play").click();
    window.harness.step();
    assert!(window.application().clock.is_playing());

    window.harness.get_by_label("Pause").click();
    window.harness.step();
    assert!(!window.application().clock.is_playing());
    assert!(window.current_frame() >= 100, "pausing stays where it is");

    window.harness.get_by_label("Play").click();
    window.harness.step();
    window.harness.get_by_label("Stop").click();
    window.harness.step();
    assert!(!window.application().clock.is_playing());
    assert_eq!(window.current_frame(), 0, "stop goes back to the start");
}

#[test]
fn the_buttons_do_nothing_when_there_is_no_video() {
    let mut window = Window::showing(Project::empty());
    window.harness.get_by_label("Play").click();
    window.harness.step();
    assert!(!window.application().clock.is_playing());
}

#[test]
fn the_warning_about_a_missing_video_is_in_the_middle_and_the_program_stays_usable() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().source_status = SourceStatus::Unavailable {
        reason: "there is no file there".to_owned(),
    };
    window.application_mut().missing_source_warning_open = true;
    window.settle();

    let warning = window.rectangle_of("Source video not found");
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
    assert!(window.harness.query_by_label("Choose the video").is_some());

    // Edits still work with the warning showing, and nothing is lost.
    window.cut_at_frame(75);
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)]);
}

#[test]
fn the_import_item_is_only_available_when_the_video_is_missing_or_there_is_none() {
    for (status, can_import) in [
        (SourceStatus::NoVideo, true),
        (
            SourceStatus::Unavailable {
                reason: "gone".to_owned(),
            },
            true,
        ),
        (
            SourceStatus::Available {
                path: PathBuf::from("holiday.mp4"),
            },
            false,
        ),
        (SourceStatus::Opening, false),
    ] {
        let mut window = Window::showing(project_of(300));
        window.application_mut().source_status = status.clone();
        window.application_mut().missing_source_warning_open = false;
        window.settle();
        window.harness.get_by_label("File").click();
        window.settle();
        let import = window.harness.get_by_label("Import");
        assert_eq!(!import.accesskit_node().is_disabled(), can_import, "{status:?}");
    }
}

#[test]
fn asking_for_a_new_project_with_unsaved_changes_asks_first_and_cancelling_keeps_everything() {
    let mut window = Window::showing(project_of(300));
    window.cut_at_frame(75);
    assert!(window.application().has_unsaved_changes);

    window
        .application_mut()
        .ask_before_discarding(AfterConfirmation::NewProject);
    window.settle();
    assert!(window.harness.query_by_label("Discard unsaved changes?").is_some());

    window.harness.get_by_label("Cancel").click();
    window.settle();
    assert!(window.harness.query_by_label("Discard unsaved changes?").is_none());
    assert_eq!(window.clips(), vec![(0, 75, 1), (75, 300, 1)], "nothing was discarded");
    assert!(window.application().has_unsaved_changes);
}

#[test]
fn a_click_outside_a_dialog_closes_it_and_the_dialog_blocks_the_track_while_it_is_up() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    // The speed slider of the track cannot be used behind the dialog.
    let on_the_track = window.middle_of_the_track_at(75);
    window.drag(on_the_track, on_the_track - vec2(0.0, 200.0), PointerButton::Secondary);
    assert_eq!(window.clips(), vec![(0, 300, 1)], "the drag did not reach the track");

    window.click(Pos2::new(5.0, 400.0));
    assert!(
        window.application().dialog.is_none(),
        "clicking outside a message closes it"
    );

    // With the dialog gone, the same drag sets the speed.
    window.drag(on_the_track, on_the_track - vec2(0.0, 200.0), PointerButton::Secondary);
    assert_eq!(window.clips(), vec![(0, 300, 10)]);
}

#[test]
fn the_ok_button_closes_a_message() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    assert!(window.harness.query_by_label("Read me").is_some());

    window.harness.get_by_label("OK").click();
    window.settle();

    assert!(window.application().dialog.is_none());
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

    assert!(window.application().dialog.is_none());
    assert!(window.application().has_unsaved_changes);
    assert_eq!(window.clips().len(), 2, "nothing was discarded");
}

fn window_inside_a_sped_up_clip() -> Window {
    let mut window = Window::showing(project_of(300));
    window
        .application_mut()
        .project
        .set_clip_speed(0, SpeedFactor::new(4).unwrap())
        .unwrap();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(10), 75);
    window.settle();
    window
}

#[test]
fn the_fast_forward_symbol_is_only_there_inside_sped_up_clips() {
    let mut window = Window::showing(project_of(300));
    assert!(
        window.harness.query_by_label("Fast-forward symbol").is_none(),
        "normal speed"
    );

    window
        .application_mut()
        .project
        .set_clip_speed(0, SpeedFactor::new(4).unwrap())
        .unwrap();
    let now = std::time::Instant::now();
    window
        .application_mut()
        .clock
        .seek(now, TimelineFrameIndex::new(10), 75);
    window.settle();
    assert!(window.harness.query_by_label("Fast-forward symbol").is_some(), "4x");
}

#[test]
fn dragging_the_symbol_moves_it_for_the_whole_project() {
    let mut window = window_inside_a_sped_up_clip();
    let before = window.application().project.speed_indicator_layout();
    let symbol = window.rectangle_of("Fast-forward symbol");

    window.drag(
        symbol.center(),
        symbol.center() + vec2(200.0, 100.0),
        PointerButton::Primary,
    );

    let after = window.application().project.speed_indicator_layout();
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
    assert!(window.application().has_unsaved_changes);
}

#[test]
fn dragging_the_corner_of_the_symbol_resizes_it() {
    let mut window = window_inside_a_sped_up_clip();
    let before = window.application().project.speed_indicator_layout();
    let symbol_before = window.rectangle_of("Fast-forward symbol");
    let corner = symbol_before.right_bottom() - vec2(4.0, 4.0);

    window.drag(corner, corner + vec2(60.0, 60.0), PointerButton::Primary);

    let after = window.application().project.speed_indicator_layout();
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
    let after = window.application().project.speed_indicator_layout();
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
        window.application().project.speed_indicator_layout(),
        SpeedIndicatorLayout::DEFAULT
    );
}

#[test]
fn the_wheel_over_the_strip_scrolls_it_without_moving_the_current_frame() {
    let mut window = Window::showing(project_of(900));
    let track = window.rectangle_of("Video track");
    window.move_pointer_to(track.center());
    let scroll_before = window.application().timeline_view.scroll_offset;

    window.harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -300.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    window.settle();

    assert!(
        window.application().timeline_view.scroll_offset > scroll_before,
        "the strip moved along"
    );
    assert_eq!(window.current_frame(), 0, "the current frame did not");
}
