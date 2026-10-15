// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of editing something that has keyframes while the playhead moves: a gesture makes at most one keyframe of what it
//! changes, however many frames of the window it goes on for, and an edit of what has keyframes stops the video that is playing,
//! as the keyframe is made where the video is stopped.
//!
//! (There was a bug: a value that was dragged while the video played made a keyframe at every frame it went over: 88 of them in
//! a row, the movement of the hand written down. The first tests here are what it was.)

use std::time::{Duration, Instant};

use egui::PointerButton;

use super::keyframe_tests::{keyframes_of, window_with_a_video};
use super::preview_layer_tests::{middle_of_the_layer, select};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::gesture_frame::GestureFrame;
use crate::project::{
    AnimationTarget, CanvasArea, ClipId, Gain, Placement, PropertyId, PropertyValue, TimelineFrameIndex,
};

const FRAMES_OF_THE_WINDOW_PER_SECOND: f64 = 30.0;

/// A window with a video of 300 frames whose position has keyframes at the frames 10 and 50, with the playhead at 20 and nothing
/// to undo.
fn window_with_a_position_that_has_two_keyframes() -> (Window, ClipId) {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    for (frame, across) in [(10, 0.5), (50, 0.7)] {
        window
            .application_mut()
            .document
            .project
            .add_keyframe(target, PropertyId::Position, TimelineFrameIndex::new(frame))
            .unwrap();
        window
            .application_mut()
            .document
            .project
            .set_property_at(
                target,
                PropertyId::Position,
                PropertyValue::Pair(across, 0.5),
                TimelineFrameIndex::new(frame),
            )
            .unwrap();
    }
    window.application_mut().note_everything_is_saved();
    window.application_mut().document.history.reset();
    window.move_current_frame_to(20);
    (window, clip)
}

/// The command that puts the video `step` hundredths of the picture further to the right than it was at the beginning.
fn put_it_to_the_right_by(clip: ClipId, step: u32) -> Command {
    Command::SetClipPlacement {
        clip,
        placement: Placement::upright(CanvasArea::new(step as f32 * 0.01, 0.0, 1.0, 1.0)),
    }
}

/// Carries out `commands` as the frame of the window that is `seconds` after `start` does.
fn carry_out_at(window: &mut Window, start: Instant, seconds: f64, commands: Vec<Command>) {
    window.application_mut().execute_all(
        commands,
        start + Duration::from_secs_f64(seconds),
        &egui::Context::default(),
    );
}

fn paused_frame(window: &Window) -> u64 {
    assert!(!window.application().is_playing(), "it is paused");
    window.current_frame()
}

// --- the bug: a drag while the video plays ---

#[test]
fn dragging_a_value_that_has_keyframes_while_the_video_plays_makes_one_keyframe_and_not_one_at_every_frame_it_goes_over()
 {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    carry_out_at(&mut window, start, 0.0, vec![Command::Play]);
    assert!(window.application().is_playing());

    // About three seconds of dragging, a frame of the window each thirtieth of a second, each moving it a little.
    for step in 1..=88 {
        carry_out_at(
            &mut window,
            start,
            step as f64 / FRAMES_OF_THE_WINDOW_PER_SECOND,
            vec![put_it_to_the_right_by(clip, step)],
        );
    }

    let stopped_at = paused_frame(&window);
    assert!(
        (20..=23).contains(&stopped_at),
        "the video was stopped by the first change, which was a moment after the start: it is at {stopped_at}"
    );
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![10, stopped_at, 50],
        "one keyframe was made, where the video was stopped, and none of the 87 others"
    );
}

#[test]
fn the_whole_drag_is_one_step_of_the_history() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    carry_out_at(&mut window, start, 0.0, vec![Command::Play]);
    for step in 1..=30 {
        carry_out_at(
            &mut window,
            start,
            step as f64 / FRAMES_OF_THE_WINDOW_PER_SECOND,
            vec![put_it_to_the_right_by(clip, step)],
        );
    }

    window.do_this(Command::Undo);

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![10, 50],
        "one undo takes back the whole drag"
    );
}

#[test]
fn the_keyframe_that_the_drag_made_has_the_last_value_that_it_gave() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    carry_out_at(&mut window, start, 0.0, vec![Command::Play]);
    for step in 1..=10 {
        carry_out_at(
            &mut window,
            start,
            step as f64 / FRAMES_OF_THE_WINDOW_PER_SECOND,
            vec![put_it_to_the_right_by(clip, step)],
        );
    }

    let frame = paused_frame(&window);
    let value = window.application().project().property_value_at(
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        TimelineFrameIndex::new(frame),
    );
    let Some(PropertyValue::Pair(across, down)) = value else {
        panic!("the position is a pair: {value:?}");
    };
    assert!(
        (across - 0.6).abs() < 1e-4 && (down - 0.5).abs() < 1e-4,
        "{across}, {down}"
    );
}

// --- what stops the video and what does not ---

#[test]
fn an_edit_of_a_value_that_has_no_keyframes_does_not_stop_the_video() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    carry_out_at(&mut window, start, 0.0, vec![Command::Play]);

    // The loudness has no keyframes: it is changed for the whole clip, which is what a slider that is dragged during playback is for.
    for step in 1..=20 {
        carry_out_at(
            &mut window,
            start,
            step as f64 / FRAMES_OF_THE_WINDOW_PER_SECOND,
            vec![Command::SetClipGain {
                clip,
                gain: Gain::from_decibels(-(step as f32) * 0.25),
            }],
        );
    }

    assert!(window.application().is_playing(), "the video plays on");
    assert!(
        keyframes_of(&window, clip, PropertyId::Gain).is_empty(),
        "and nothing was written down"
    );
    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![10, 50]);
}

#[test]
fn the_stopwatch_and_the_other_commands_that_name_the_frame_they_are_for_do_not_stop_the_video() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    carry_out_at(&mut window, start, 0.0, vec![Command::Play]);

    carry_out_at(
        &mut window,
        start,
        0.2,
        vec![Command::AddKeyframe {
            target: AnimationTarget::Clip(clip),
            property: PropertyId::Gain,
            frame: TimelineFrameIndex::new(30),
        }],
    );

    assert!(window.application().is_playing());
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![30]);
}

#[test]
fn an_edit_of_a_value_that_has_keyframes_while_the_video_is_paused_is_made_at_the_playhead_as_always() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    window.move_current_frame_to(30);

    for step in 1..=5 {
        carry_out_at(
            &mut window,
            start,
            step as f64 * 0.01,
            vec![put_it_to_the_right_by(clip, step)],
        );
    }

    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![10, 30, 50]);
    assert!(!window.application().is_playing());
}

#[test]
fn an_edit_made_later_at_another_frame_is_another_keyframe_and_another_step_of_the_history() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    let start = Instant::now();
    window.move_current_frame_to(30);
    carry_out_at(&mut window, start, 0.0, vec![put_it_to_the_right_by(clip, 3)]);
    window.move_current_frame_to(40);

    // (A moment later: not a second, so that it is the same sort of edit a gesture is, and not another by waiting.)
    carry_out_at(&mut window, start, 0.1, vec![put_it_to_the_right_by(clip, 4)]);

    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![10, 30, 40, 50]);
    window.do_this(Command::Undo);
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![10, 30, 50],
        "what was done at the other frame is a step of its own"
    );
}

// --- a drag with the mouse, in which the playhead is moved from outside ---

#[test]
fn a_drag_of_the_video_on_the_preview_keeps_to_the_frame_it_began_at_when_the_playhead_moves_during_it() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    select(&mut window, clip);
    let total_frames = window.application().project().total_frame_count();
    let from = middle_of_the_layer(&window);

    window.move_pointer_to(from);
    window.press(from, PointerButton::Primary);
    for step in 1..=12 {
        // The playhead goes on (as it does while the video plays, or a key that moves it is held), a frame in each step.
        window
            .application_mut()
            .playback
            .clock
            .seek(Instant::now(), TimelineFrameIndex::new(20 + step), total_frames);
        window.move_pointer_to(from + egui::vec2(3.0 * step as f32, 1.0 * step as f32));
    }
    window.release(from + egui::vec2(36.0, 12.0), PointerButton::Primary);

    let frames = keyframes_of(&window, clip, PropertyId::Position);
    assert_eq!(
        frames.len(),
        3,
        "the two there were, and one for the whole drag, and not one for each of the twelve frames the playhead went over: {frames:?}"
    );
    assert!(frames.contains(&10) && frames.contains(&50), "{frames:?}");
}

#[test]
fn a_drag_that_is_made_again_after_the_playhead_was_moved_makes_its_keyframe_where_the_playhead_is_then() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    select(&mut window, clip);
    let from = middle_of_the_layer(&window);
    window.drag(from, from + egui::vec2(30.0, 0.0), PointerButton::Primary);
    let first = keyframes_of(&window, clip, PropertyId::Position);
    assert_eq!(first, vec![10, 20, 50]);

    window.move_current_frame_to(35);
    let from = middle_of_the_layer(&window);
    window.drag(from, from + egui::vec2(30.0, 0.0), PointerButton::Primary);

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![10, 20, 35, 50],
        "another press, another gesture, at the frame the playhead is at: not the one the first began at"
    );
}

#[test]
fn what_a_drag_goes_on_with_after_an_undo_in_the_middle_of_it_is_made_where_the_playhead_is_then() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    select(&mut window, clip);
    let total_frames = window.application().project().total_frame_count();
    let from = middle_of_the_layer(&window);

    window.move_pointer_to(from);
    window.press(from, PointerButton::Primary);
    window.move_pointer_to(from + egui::vec2(6.0, 2.0));
    window.move_pointer_to(from + egui::vec2(12.0, 4.0));
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![10, 20, 50],
        "the drag made its keyframe where the playhead was when it began"
    );

    // The edit is undone while the button is still down, and the playhead moves on before the hand does.
    window.do_this_and_draw_nothing(Command::Undo);
    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![10, 50]);
    window
        .application_mut()
        .playback
        .clock
        .seek(Instant::now(), TimelineFrameIndex::new(40), total_frames);
    window.move_pointer_to(from + egui::vec2(18.0, 6.0));
    window.release(from + egui::vec2(18.0, 6.0), PointerButton::Primary);

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![10, 40, 50],
        "what the drag went on with is not more of the gesture that was undone: it is made where the playhead is, and does not bring back the keyframe that was taken away"
    );
}

#[test]
fn replacing_the_project_lets_go_of_the_gesture_that_was_going_on() {
    let (mut window, clip) = window_with_a_position_that_has_two_keyframes();
    select(&mut window, clip);
    let from = middle_of_the_layer(&window);
    window.move_pointer_to(from);
    window.press(from, PointerButton::Primary);
    window.move_pointer_to(from + egui::vec2(6.0, 2.0));
    window.move_pointer_to(from + egui::vec2(12.0, 4.0));
    assert_ne!(
        window.application().interface.gesture_frame,
        GestureFrame::default(),
        "the setup: the drag is a gesture that keeps to a frame"
    );

    // (No window of the program lets a project be loaded while a button is down, but nothing that is kept of one project is for the
    // next, however it comes.)
    window.application_mut().start_empty_project();

    assert_eq!(window.application().interface.gesture_frame, GestureFrame::default());
}
