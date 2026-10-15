// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the keys that take the playhead and the view about: Ctrl and an arrow go to the end of a clip, and F frames what is
//! selected in the timeline or, with the pointer over the preview, makes the picture of the preview fit its panel.

use egui::{Key, Modifiers, Rect, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::preview_panel::FIT_BUTTON_LABEL;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{AnimationTarget, PropertyId, TimelineFrameIndex};

/// A window with a video of 300 frames, cut at 100 and at 200 (the tracks: the sound in front and the video behind it), and a
/// sound of 5 seconds in front of it, which is cut at 100 too, so that clips begin and end at 0, 100, 150, 200 and 300 (which
/// is past the last frame, 299, there is to go to).
fn window_with_clips_ending_at_100_150_200_and_300() -> Window {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    window.cut_at_frame(100);
    window.cut_at_frame(200);
    window.move_current_frame_to(0);
    window
}

fn press_control_with(window: &mut Window, key: Key) {
    window.press_key_with(Modifiers::COMMAND, key);
}

// --- Ctrl and the arrows ---

#[test]
fn control_right_goes_to_the_nearest_end_of_any_clip_when_none_is_selected_and_control_left_goes_back_the_same_way() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    assert_eq!(window.application().selected_clips().len(), 0);

    let mut gone_to = Vec::new();
    for _ in 0..6 {
        press_control_with(&mut window, Key::ArrowRight);
        gone_to.push(window.current_frame());
    }
    assert_eq!(
        gone_to,
        [100, 150, 200, 299, 299, 299],
        "the sound ends at 150, the video at 300 (which is past the last frame, and the last frame is where it stops)"
    );

    let mut gone_back_to = Vec::new();
    for _ in 0..6 {
        press_control_with(&mut window, Key::ArrowLeft);
        gone_back_to.push(window.current_frame());
    }
    assert_eq!(gone_back_to, [200, 150, 100, 0, 0, 0]);
}

#[test]
fn with_clips_selected_control_and_an_arrow_go_to_the_ends_of_the_selected_clips_only() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    // The middle part of the video, from 100 to 200.
    window.click(window.middle_of_the_strip_at(1, 150));
    assert_eq!(window.application().selected_clips().len(), 1);

    let mut gone_to = Vec::new();
    for _ in 0..3 {
        press_control_with(&mut window, Key::ArrowRight);
        gone_to.push(window.current_frame());
    }
    assert_eq!(
        gone_to,
        [100, 200, 200],
        "not 150, where the sound ends: it is not selected"
    );

    window.move_current_frame_to(299);
    let mut gone_back_to = Vec::new();
    for _ in 0..3 {
        press_control_with(&mut window, Key::ArrowLeft);
        gone_back_to.push(window.current_frame());
    }
    assert_eq!(gone_back_to, [200, 100, 100]);
}

#[test]
fn with_two_clips_selected_it_goes_to_the_ends_of_both() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.click(window.middle_of_the_strip_at(1, 150)); // 100 to 200
    window.click_with_control(window.middle_of_the_strip_at(0, 120)); // the sound, 100 to 150
    assert_eq!(window.application().selected_clips().len(), 2);

    let mut gone_to = Vec::new();
    for _ in 0..4 {
        press_control_with(&mut window, Key::ArrowRight);
        gone_to.push(window.current_frame());
    }
    assert_eq!(gone_to, [100, 150, 200, 200]);
}

#[test]
fn a_clip_that_is_selected_but_has_nothing_to_go_to_in_that_direction_does_not_move_the_playhead_to_any_other() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.click(window.middle_of_the_strip_at(1, 50)); // 0 to 100
    window.move_current_frame_to(120);

    press_control_with(&mut window, Key::ArrowRight);
    assert_eq!(window.current_frame(), 120, "the selected clip ends before it");
    press_control_with(&mut window, Key::ArrowLeft);
    assert_eq!(window.current_frame(), 100);
}

#[test]
fn the_arrows_without_control_still_move_one_frame_and_control_and_an_arrow_move_the_view_with_the_playhead() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.press_key(Key::ArrowRight);
    assert_eq!(window.current_frame(), 1);
    press_control_with(&mut window, Key::ArrowRight);
    assert_eq!(window.current_frame(), 100);
    let strip = window.strip_of_track(1);
    let x_of_the_playhead = window
        .x_of_the_drawn_playhead()
        .expect("the playhead is in view: the view follows it");
    assert!(
        (x_of_the_playhead - strip.center().x).abs() < 3.0,
        "in the middle of the strips, as it always is when it is moved: {x_of_the_playhead} and {}",
        strip.center().x
    );
}

#[test]
fn control_and_an_arrow_do_nothing_with_nothing_on_the_timeline_and_while_a_text_is_typed_in() {
    let mut window = Window::showing(crate::project::Project::empty());
    press_control_with(&mut window, Key::ArrowRight);
    assert_eq!(window.current_frame(), 0);

    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.click(window.title_box_of_track(0).center());
    press_control_with(&mut window, Key::ArrowRight);
    assert_eq!(
        window.current_frame(),
        0,
        "the keys are the title's while it is typed in"
    );
}

// --- F in the timeline ---

/// How wide the strips are, which is the view that things are framed in.
fn width_of_the_view(window: &Window) -> f32 {
    window.strip_of_track(1).width()
}

/// Where the frames from `first` up to `end` are across the strips of the track at `position`, as a rectangle as tall as the strip.
fn where_the_frames_are(window: &Window, position: usize, first: u64, end: u64) -> Rect {
    let strip = window.strip_of_track(position);
    Rect::from_min_max(
        egui::pos2(window.x_of_frame_on_the_strip(position, first), strip.top()),
        egui::pos2(window.x_of_frame_on_the_strip(position, end), strip.bottom()),
    )
}

#[test]
fn f_makes_the_selected_clip_nearly_fill_the_width_of_the_timeline_with_the_middle_of_it_in_the_middle() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.click(window.middle_of_the_strip_at(1, 150)); // the middle of the video, 100 to 200
    window.move_pointer_to(window.strip_of_track(1).center());

    window.press_key(Key::F);

    let strip = window.strip_of_track(1);
    let clip = where_the_frames_are(&window, 1, 100, 200);
    assert!(
        (clip.width() / width_of_the_view(&window) - 0.92).abs() < 0.01,
        "the clip takes up nearly all of the width: {} of {}",
        clip.width(),
        width_of_the_view(&window)
    );
    assert!(
        (clip.center().x - strip.center().x).abs() < 1.0,
        "and is in the middle: {} and {}",
        clip.center().x,
        strip.center().x
    );
}

#[test]
fn f_frames_a_clip_at_the_start_of_the_timeline_as_near_the_middle_as_the_strips_can_be_scrolled() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.click(window.middle_of_the_strip_at(0, 60)); // the sound, 0 to 100
    window.move_pointer_to(window.strip_of_track(1).center());

    window.press_key(Key::F);

    let clip = where_the_frames_are(&window, 0, 0, 100);
    let strip = window.strip_of_track(0);
    assert!(
        (clip.width() / strip.width() - 0.92).abs() < 0.01,
        "it takes up nearly all of the width: {clip:?} in {strip:?}"
    );
    assert!(
        (clip.left() - strip.left()).abs() < 1.0,
        "and begins where the strips do, as they do not scroll back past their beginning: {clip:?} in {strip:?}"
    );
}

#[test]
fn f_frames_a_clip_that_is_far_along_the_timeline_by_scrolling_to_it() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    // The last part of the video is out of view to begin with.
    let last_part = window.application().project().tracks()[1]
        .placement_at_index(2)
        .unwrap()
        .id;
    window.do_this(Command::select_clips([last_part], SelectionChange::Replace));
    window.move_pointer_to(window.strip_of_track(1).center());

    window.press_key(Key::F);

    let clip = where_the_frames_are(&window, 1, 200, 300);
    let strip = window.strip_of_track(1);
    assert!(
        (clip.width() / strip.width() - 0.92).abs() < 0.01,
        "{clip:?} in {strip:?}"
    );
    assert!(
        clip.left() >= strip.left() && clip.right() <= strip.right(),
        "all of it is in view: {clip:?} in {strip:?}"
    );
}

#[test]
fn f_frames_all_of_what_is_selected_together() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.click(window.middle_of_the_strip_at(1, 50)); // the video from 0 to 100
    window.click_with_control(window.middle_of_the_strip_at(0, 120)); // the sound from 100 to 150
    window.move_pointer_to(window.strip_of_track(1).center());

    window.press_key(Key::F);

    let all = where_the_frames_are(&window, 1, 0, 150);
    assert!(
        (all.width() / width_of_the_view(&window) - 0.92).abs() < 0.01,
        "from the start of the one to the end of the other: {all:?}"
    );
}

#[test]
fn f_frames_a_keyframe_by_a_second_of_the_timeline_and_a_rack_by_the_frames_its_keyframes_take_up() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    let video = window.clip(1, 1); // 100 to 200
    let target = AnimationTarget::Clip(video);
    let keyframes: Vec<_> = [120, 150, 180]
        .into_iter()
        .map(|frame| {
            window
                .application_mut()
                .document
                .project
                .add_keyframe(target, PropertyId::Gain, TimelineFrameIndex::new(frame))
                .unwrap()
                .unwrap()
        })
        .collect();
    window.settle();
    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Keyframe(keyframes[1].keyframe)],
        change: SelectionChange::Replace,
    });
    window.move_pointer_to(window.strip_of_track(1).center());

    window.press_key(Key::F);

    let one_second = where_the_frames_are(&window, 1, 150, 180);
    assert!(
        (one_second.width() / width_of_the_view(&window) - 0.92).abs() < 0.01,
        "a keyframe is framed by a second round it, not by the frame it is on: {one_second:?}"
    );
    let strip = window.strip_of_track(1);
    let middle_of_the_keyframe =
        (window.x_of_frame_on_the_strip(1, 150) + window.x_of_frame_on_the_strip(1, 151)) / 2.0;
    assert!(
        (middle_of_the_keyframe - strip.center().x).abs() < 2.0,
        "it is in the middle: {middle_of_the_keyframe} and {}",
        strip.center().x
    );

    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Rack(keyframes[0].rack)],
        change: SelectionChange::Replace,
    });
    window.press_key(Key::F);
    let rack = where_the_frames_are(&window, 1, 120, 181);
    assert!(
        (rack.width() / width_of_the_view(&window) - 0.92).abs() < 0.01,
        "the rack: from its first keyframe to just after its last: {rack:?}"
    );
}

#[test]
fn f_with_nothing_selected_leaves_the_timeline_as_it_is_and_so_does_a_track_that_is_selected() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.move_pointer_to(window.strip_of_track(1).center());
    let before = (
        window.application().interface.timeline_view.scroll_offset,
        window.x_of_frame_on_the_strip(1, 100),
        window.x_of_frame_on_the_strip(1, 200),
    );

    window.press_key(Key::F);
    assert_eq!(
        (
            window.application().interface.timeline_view.scroll_offset,
            window.x_of_frame_on_the_strip(1, 100),
            window.x_of_frame_on_the_strip(1, 200)
        ),
        before
    );

    let track = window.application().project().tracks()[1].id();
    window.do_this(Command::select_track(track, SelectionChange::Replace));
    window.press_key(Key::F);
    assert_eq!(
        window.x_of_frame_on_the_strip(1, 100),
        before.1,
        "a track takes up no frames to frame"
    );
}

#[test]
fn the_timeline_stays_framed_until_the_playhead_moves_and_then_follows_it_again() {
    let mut window = window_with_clips_ending_at_100_150_200_and_300();
    window.move_current_frame_to(150);
    window.click(window.middle_of_the_strip_at(1, 150));
    window.move_pointer_to(window.strip_of_track(1).center());
    window.press_key(Key::F);
    let framed = (
        window.x_of_frame_on_the_strip(1, 100),
        window.x_of_frame_on_the_strip(1, 200),
    );

    window.harness.run_steps(10);
    assert_eq!(
        (
            window.x_of_frame_on_the_strip(1, 100),
            window.x_of_frame_on_the_strip(1, 200)
        ),
        framed,
        "it does not slide back to follow the playhead by itself"
    );

    window.press_key(Key::ArrowRight);
    let strip = window.strip_of_track(1);
    let playhead = window.x_of_the_drawn_playhead().expect("the playhead is in view");
    assert!(
        (playhead - strip.center().x).abs() < 3.0,
        "moving the playhead takes the view to it, as it always does"
    );
}

// --- F over the preview ---

fn zoom_the_preview_in(window: &mut Window) {
    let middle = window.the_preview_panel().center();
    window.scroll_the_wheel(middle, vec2(0.0, 200.0), Modifiers::NONE);
}

fn the_picture_of_the_preview_is_zoomed(window: &Window) -> bool {
    window.harness.query_by_label(FIT_BUTTON_LABEL).is_some()
}

#[test]
fn f_with_the_pointer_over_the_preview_makes_the_picture_fit_the_panel_like_the_button_does() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    zoom_the_preview_in(&mut window);
    assert!(the_picture_of_the_preview_is_zoomed(&window), "the setup: it is zoomed");
    let timeline_before = window.application().interface.timeline_view.scroll_offset;
    window.click(window.middle_of_the_strip_at(0, 60)); // something is selected: it is not what F is about here
    window.move_pointer_to(window.the_preview_panel().center());

    window.press_key(Key::F);

    assert!(
        !the_picture_of_the_preview_is_zoomed(&window),
        "the button is gone: it fits"
    );
    assert_eq!(
        window.application().interface.timeline_view.scroll_offset,
        timeline_before,
        "the timeline was left alone"
    );
}

#[test]
fn f_with_the_pointer_elsewhere_leaves_the_picture_of_the_preview_as_it_is() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    zoom_the_preview_in(&mut window);
    window.click(window.middle_of_the_strip_at(0, 60));
    window.move_pointer_to(window.strip_of_track(1).center());

    window.press_key(Key::F);

    assert!(the_picture_of_the_preview_is_zoomed(&window), "still zoomed");
}

#[test]
fn f_over_the_preview_works_with_nothing_selected_and_when_the_picture_already_fits() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    window.move_pointer_to(window.the_preview_panel().center());
    window.press_key(Key::F);
    assert!(
        !the_picture_of_the_preview_is_zoomed(&window),
        "it fits, and nothing was made of it"
    );

    zoom_the_preview_in(&mut window);
    window.press_key(Key::F);
    assert!(!the_picture_of_the_preview_is_zoomed(&window));
}
