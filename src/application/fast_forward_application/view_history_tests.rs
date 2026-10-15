// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of going back and forward through the views the timeline has had, with the Back and Forward buttons of a mouse.
//!
//! A view is how far the timeline is zoomed and scrolled, along and up and down. Each thing the user does to it (a turn of the wheel,
//! or turns that come soon after one another; a zoom; a press of the F key that frames what is selected) is a step.

use egui::{Key, Modifiers, PointerButton, Pos2, vec2};

use super::test_window::{Window, button_pressed_at, button_released_at, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::selection::SelectionChange;
use crate::test_support::check_every_case_at_once;

const ONE_NOTCH: f32 = 100.0;

const CTRL_AND_ALT: Modifiers = Modifiers {
    alt: true,
    ..Modifiers::COMMAND
};

/// A window showing a 30 second video, which is far wider than the window at the normal zoom.
fn window_with_a_long_video() -> Window {
    Window::showing(project_of(900))
}

/// Presses and lets go of a button of the mouse, with the pointer over the window at `position`.
fn press_the_mouse_button(window: &mut Window, button: PointerButton, position: Pos2) {
    window.move_pointer_to(position);
    window.press(position, button);
    window.release(position, button);
    window.settle();
}

/// The button of the mouse that goes back: the first of the extra buttons, which most mice that have two have under the thumb, the one nearer
/// the user. (Written out here and not taken from the program, so that which buttons the program takes is what is checked.)
const THE_BUTTON_THAT_GOES_BACK: PointerButton = PointerButton::Extra1;

/// The button of the mouse that goes forward: the second of the extra buttons.
const THE_BUTTON_THAT_GOES_FORWARD: PointerButton = PointerButton::Extra2;

fn go_back(window: &mut Window) {
    let position = window.middle_of_the_strip_at(0, 100);
    press_the_mouse_button(window, THE_BUTTON_THAT_GOES_BACK, position);
}

fn go_forward(window: &mut Window) {
    let position = window.middle_of_the_strip_at(0, 100);
    press_the_mouse_button(window, THE_BUTTON_THAT_GOES_FORWARD, position);
}

/// Leaves the window alone for long enough that what is done to it next is not more of what was done before.
fn pause(window: &mut Window) {
    window.harness.run_steps(60);
}

/// Turns the wheel a notch at a time, `notches` of them, with a tenth of a second between one and the next, as a hand turns it
/// when it rolls it, and lets the smooth scrolling that follows play out. (Scrolls the strips along.)
fn roll_the_wheel_along(window: &mut Window, notches: usize) {
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    window.move_pointer_to(over_a_strip);
    for _ in 0..notches {
        window.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -ONE_NOTCH),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        window.harness.run_steps(6);
    }
    window.harness.run_steps(40);
    window.settle();
}

/// Turns the wheel to scroll the strips along (a notch is as far as a notch scrolls them).
fn scroll_along(window: &mut Window, notches: f32) {
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    window.scroll_the_wheel(over_a_strip, vec2(0.0, -notches * ONE_NOTCH), Modifiers::NONE);
}

fn zoom_along(window: &mut Window, notches: f32) {
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    window.scroll_the_wheel(over_a_strip, vec2(0.0, notches * ONE_NOTCH), Modifiers::COMMAND);
}

fn zoom_up_and_down(window: &mut Window, notches: f32) {
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    window.scroll_the_wheel(over_a_strip, vec2(0.0, notches * ONE_NOTCH), CTRL_AND_ALT);
}

fn scroll_the_rows(window: &mut Window, notches: f32) {
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    window.scroll_the_wheel(over_a_strip, vec2(0.0, -notches * ONE_NOTCH), Modifiers::ALT);
}

fn scrolled_along(window: &Window) -> f64 {
    window.application().timeline_view.scroll_offset
}

fn width_of_a_second(window: &Window) -> f32 {
    window.x_of_frame_on_the_strip(0, 60) - window.x_of_frame_on_the_strip(0, 30)
}

fn height_of_the_rows(window: &Window) -> f32 {
    window.strip_of_track(0).height()
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!((actual - expected).abs() < 0.5, "{what}: {actual} is not {expected}");
}

// --- going back and forward ---

#[test]
fn the_back_button_goes_back_to_the_view_from_before_a_scroll_and_the_forward_button_goes_on_to_it_again() {
    let mut window = window_with_a_long_video();
    let at_the_start = scrolled_along(&window);

    scroll_along(&mut window, 2.0);
    let scrolled = scrolled_along(&window);
    assert!(
        scrolled > at_the_start + 100.0,
        "the setup: it scrolled: {at_the_start} then {scrolled}"
    );

    go_back(&mut window);
    assert_close(scrolled_along(&window), at_the_start, "back");

    go_forward(&mut window);
    assert_close(scrolled_along(&window), scrolled, "forward again");
}

#[test]
fn turns_of_the_wheel_that_come_soon_after_one_another_are_one_step_and_turns_after_a_pause_are_steps_of_their_own() {
    let mut window = window_with_a_long_video();
    let at_the_start = scrolled_along(&window);

    // Two turns, one after the other, and then a pause, and a third.
    roll_the_wheel_along(&mut window, 2);
    let after_two_turns = scrolled_along(&window);
    pause(&mut window);
    scroll_along(&mut window, 1.0);
    let after_three = scrolled_along(&window);
    assert!(
        after_two_turns > at_the_start + 100.0 && after_three > after_two_turns + 50.0,
        "the setup"
    );

    go_back(&mut window);
    assert_close(
        scrolled_along(&window),
        after_two_turns,
        "the third turn is undone alone",
    );
    go_back(&mut window);
    assert_close(scrolled_along(&window), at_the_start, "and the two before it, together");
    go_back(&mut window);
    assert_close(
        scrolled_along(&window),
        at_the_start,
        "and there is nothing before that",
    );
}

#[test]
fn zooming_and_scrolling_are_steps_of_their_own_even_when_one_comes_straight_after_the_other() {
    let mut window = window_with_a_long_video();
    let (at_the_start, second_at_the_start) = (scrolled_along(&window), width_of_a_second(&window));

    zoom_along(&mut window, 1.0);
    let (zoomed, second_zoomed) = (scrolled_along(&window), width_of_a_second(&window));
    scroll_along(&mut window, 1.0);
    let scrolled = scrolled_along(&window);
    assert!(second_zoomed > second_at_the_start * 1.5, "the setup: it zoomed");
    assert!(scrolled > zoomed + 50.0, "the setup: it scrolled");

    go_back(&mut window);
    assert_close(
        scrolled_along(&window),
        zoomed,
        "back from the scroll to the zoomed view",
    );
    assert_close(
        f64::from(width_of_a_second(&window)),
        f64::from(second_zoomed),
        "as zoomed as it was",
    );
    go_back(&mut window);
    assert_close(scrolled_along(&window), at_the_start, "and back from the zoom");
    assert_close(
        f64::from(width_of_a_second(&window)),
        f64::from(second_at_the_start),
        "to the zoom it began with",
    );
}

#[test]
fn zooming_the_rows_up_and_down_and_scrolling_them_are_in_the_history_too() {
    let mut window = project_with_enough_tracks_to_scroll();
    let (height, top) = (height_of_the_rows(&window), window.strip_of_track(0).top());

    zoom_up_and_down(&mut window, 0.5);
    let zoomed_height = height_of_the_rows(&window);
    assert!(zoomed_height > height + 10.0, "the setup: the rows are taller");
    // (Zooming keeps the part of the rows that is under the pointer where it is, which scrolls them a little.)
    let scrolled_by_the_zoom = window.application().timeline_view.rows_scroll_offset;
    pause(&mut window);
    scroll_the_rows(&mut window, 1.0);
    let scrolled_by = window.application().timeline_view.rows_scroll_offset;
    assert!(
        scrolled_by > scrolled_by_the_zoom + 20.0,
        "the setup: they scrolled by {scrolled_by}"
    );

    go_back(&mut window);
    assert_close(
        f64::from(window.application().timeline_view.rows_scroll_offset),
        f64::from(scrolled_by_the_zoom),
        "the rows are scrolled back",
    );
    assert_close(
        f64::from(height_of_the_rows(&window)),
        f64::from(zoomed_height),
        "and are as zoomed as they were",
    );
    go_back(&mut window);
    assert_close(
        f64::from(height_of_the_rows(&window)),
        f64::from(height),
        "and then the zoom goes back",
    );
    assert_close(
        f64::from(window.strip_of_track(0).top()),
        f64::from(top),
        "to where they were",
    );
    go_forward(&mut window);
    go_forward(&mut window);
    assert_close(
        f64::from(window.application().timeline_view.rows_scroll_offset),
        f64::from(scrolled_by),
        "and forward again",
    );
}

/// A window with five tracks, of which only three show at once: the rows can be scrolled.
fn project_with_enough_tracks_to_scroll() -> Window {
    let mut window = Window::showing(project_with_a_video_and_a_sound(900, 20));
    for _ in 0..3 {
        window.do_this(Command::AddTrack);
    }
    window
}

#[test]
fn the_key_that_frames_what_is_selected_is_a_step_every_time_it_is_pressed() {
    let mut window = window_with_a_long_video();
    let clip = window.clip(0, 0);
    window.do_this(Command::select_clips(vec![clip], SelectionChange::Replace));
    let (at_the_start, second_at_the_start) = (scrolled_along(&window), width_of_a_second(&window));

    window.press_key(Key::F);
    let (framed, second_framed) = (scrolled_along(&window), width_of_a_second(&window));
    assert!(
        (second_framed - second_at_the_start).abs() > 1.0 || (framed - at_the_start).abs() > 1.0,
        "the setup: the view changed: {second_at_the_start} to {second_framed}"
    );

    go_back(&mut window);

    assert_close(scrolled_along(&window), at_the_start, "back from framing");
    assert_close(
        f64::from(width_of_a_second(&window)),
        f64::from(second_at_the_start),
        "to the zoom it had",
    );
    go_forward(&mut window);
    assert_close(scrolled_along(&window), framed, "forward to the framing");
}

#[test]
fn framing_what_is_already_framed_changes_nothing_and_is_not_a_step() {
    let mut window = window_with_a_long_video();
    let clip = window.clip(0, 0);
    window.do_this(Command::select_clips(vec![clip], SelectionChange::Replace));
    let second_at_the_start = width_of_a_second(&window);
    window.press_key(Key::F);
    assert!(
        (width_of_a_second(&window) - second_at_the_start).abs() > 1.0,
        "the setup: the first framing changed the view (it zoomed it)"
    );
    assert_eq!(
        window.application().timeline_view.view_history_steps_back(),
        1,
        "the setup: it is a step"
    );

    window.press_key(Key::F);

    assert_eq!(
        window.application().timeline_view.view_history_steps_back(),
        1,
        "the second framing changed nothing, so it is not a step"
    );
    go_back(&mut window);
    assert_close(
        f64::from(width_of_a_second(&window)),
        f64::from(second_at_the_start),
        "going back once goes back from the first framing",
    );
}

#[test]
fn two_framings_one_after_the_other_are_two_steps_though_the_time_between_them_is_short() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(900, 20));
    let (video, sound) = (window.clip(0, 0), window.clip(1, 0));
    window.do_this(Command::select_clips(vec![video], SelectionChange::Replace));
    window.press_key(Key::F);
    let after_the_first = (scrolled_along(&window), width_of_a_second(&window));
    window.do_this(Command::select_clips(vec![sound], SelectionChange::Replace));

    window.press_key(Key::F);

    let after_the_second = (scrolled_along(&window), width_of_a_second(&window));
    assert_ne!(
        after_the_first, after_the_second,
        "the setup: the second framing is another view"
    );
    assert_eq!(
        window.application().timeline_view.view_history_steps_back(),
        2,
        "each framing is a step, whatever the time between them"
    );
}

// --- what is one action and what is two ---

/// The four things the wheel does to how the timeline is looked at, with the keys held that make it do them. Two scrollings, or two zoomings, that
/// come soon after one another are one action; a scrolling and a zooming are two.
#[derive(Clone, Copy, Debug)]
enum WhatTheWheelDoes {
    ScrollsTheStrips,
    ScrollsTheRows,
    ZoomsTheStrips,
    ZoomsTheRows,
}

impl WhatTheWheelDoes {
    const ALL: [Self; 4] = [
        Self::ScrollsTheStrips,
        Self::ScrollsTheRows,
        Self::ZoomsTheStrips,
        Self::ZoomsTheRows,
    ];

    fn is_a_zoom(self) -> bool {
        matches!(self, Self::ZoomsTheStrips | Self::ZoomsTheRows)
    }

    /// Turns the wheel half a notch with the keys held that make it do this, and leaves the window alone for a tenth of a second after
    /// (far less than the pause that ends an action; the helpers that scroll and zoom, which let a turn of the wheel play out, take longer).
    fn turn_the_wheel(self, window: &mut Window) {
        let (delta, modifiers) = match self {
            Self::ScrollsTheStrips => (-ONE_NOTCH / 2.0, Modifiers::NONE),
            Self::ScrollsTheRows => (-ONE_NOTCH / 2.0, Modifiers::ALT),
            Self::ZoomsTheStrips => (ONE_NOTCH / 2.0, Modifiers::COMMAND),
            Self::ZoomsTheRows => (ONE_NOTCH / 2.0, CTRL_AND_ALT),
        };
        let over_a_strip = window.middle_of_the_strip_at(0, 100);
        window.move_pointer_to(over_a_strip);
        window.harness.event(egui::Event::ModifiersChanged(modifiers));
        window.harness.step();
        window.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, delta),
            phase: egui::TouchPhase::Move,
            modifiers,
        });
        window.harness.run_steps(6);
        window.harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        window.harness.step();
    }
}

/// What the timeline is looked at as, in the numbers that the four things change.
fn how_the_timeline_is_looked_at(window: &Window) -> (f64, f32, f32, f32) {
    let view = &window.application().timeline_view;
    (
        view.scroll_offset,
        width_of_a_second(window),
        view.rows_scroll_offset,
        view.vertical_zoom.factor(),
    )
}

#[test]
fn what_the_wheel_does_is_one_action_with_what_it_did_a_moment_before_if_both_are_scrolling_or_both_are_zooming_and_two_if_not()
 {
    let pairs: Vec<(WhatTheWheelDoes, WhatTheWheelDoes)> = WhatTheWheelDoes::ALL
        .into_iter()
        .flat_map(|first| WhatTheWheelDoes::ALL.into_iter().map(move |second| (first, second)))
        .collect();

    check_every_case_at_once(&pairs, |&(first, second)| {
        let mut window = project_with_enough_tracks_to_scroll();
        let before = how_the_timeline_is_looked_at(&window);

        first.turn_the_wheel(&mut window);
        let between = how_the_timeline_is_looked_at(&window);
        second.turn_the_wheel(&mut window);
        let after = how_the_timeline_is_looked_at(&window);
        pause(&mut window);

        assert_ne!(before, between, "the setup: {first:?} changed how it is looked at");
        assert_ne!(between, after, "the setup: {second:?} changed it again");
        assert_eq!(
            window.application().timeline_view.view_history_steps_back(),
            if first.is_a_zoom() == second.is_a_zoom() { 1 } else { 2 },
            "{first:?} and then {second:?} a moment later"
        );
    });
}

#[test]
fn a_zoom_and_a_scroll_in_one_frame_are_a_zoom_and_the_scrolling_that_goes_on_in_the_frames_after_it_is_another_action()
{
    let mut window = window_with_a_long_video();
    let over_a_strip = window.middle_of_the_strip_at(0, 100);
    window.move_pointer_to(over_a_strip);
    let before = how_the_timeline_is_looked_at(&window);

    // (A pinch on a trackpad that goes on while two fingers scroll: both come in the same frame. The scrolling goes on for a few frames.)
    window.all_in_one_frame([
        egui::Event::Zoom(1.2),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -ONE_NOTCH),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        },
    ]);
    let after_the_frame = how_the_timeline_is_looked_at(&window);
    assert!(after_the_frame.1 > before.1 + 1.0, "the setup: it zoomed");
    pause(&mut window);
    assert!(
        how_the_timeline_is_looked_at(&window).0 > after_the_frame.0 + 1.0,
        "the setup: and the scrolling went on after that frame"
    );

    assert_eq!(
        window.application().timeline_view.view_history_steps_back(),
        2,
        "the frame that had both was a zooming, and the scrolling that went on after it is another action; if the frame had been a scrolling \
         it would have been one action with it"
    );
}

#[test]
fn going_back_in_the_very_frame_that_a_frame_is_picked_on_the_ruler_in_keeps_the_view_that_it_goes_back_to() {
    let mut window = window_with_a_long_video();
    let at_the_start = scrolled_along(&window);
    scroll_along(&mut window, 2.0);
    let scrolled = scrolled_along(&window);
    assert!(
        scrolled > at_the_start + 100.0,
        "the setup: the view is not where it was"
    );
    pause(&mut window);
    let on_the_ruler = window.point_on_the_ruler_at(120);
    window.move_pointer_to(on_the_ruler);

    // (A click on the ruler, and the Back button of the mouse, in one frame: the frame that is picked is where the current frame goes, and
    // the view is not made to follow it, since the pointer is on it where it is; nor is the view that was gone back to.)
    window.all_in_one_frame([
        button_pressed_at(on_the_ruler, PointerButton::Primary),
        button_released_at(on_the_ruler, PointerButton::Primary),
        button_pressed_at(on_the_ruler, THE_BUTTON_THAT_GOES_BACK),
        button_released_at(on_the_ruler, THE_BUTTON_THAT_GOES_BACK),
    ]);
    pause(&mut window);

    assert_eq!(
        window.application().current_frame().value(),
        120,
        "the setup: the frame on the ruler was picked"
    );
    assert_close(
        scrolled_along(&window),
        at_the_start,
        &format!(
            "the view is the one that was gone back to, and does not move to the frame that was picked (it was scrolled to {scrolled})"
        ),
    );
}

// --- what is not a step ---

#[test]
fn what_the_timeline_does_by_itself_is_not_a_step_following_the_current_frame_is_not_something_to_go_back_through() {
    let mut window = window_with_a_long_video();
    let at_the_start = scrolled_along(&window);

    // The current frame goes far on, and the view follows it (which is what it does while a video plays).
    window.move_current_frame_to(800);
    let following = scrolled_along(&window);
    assert!(
        following > at_the_start + 500.0,
        "the setup: the view followed the current frame"
    );

    go_back(&mut window);

    assert_close(
        scrolled_along(&window),
        following,
        "there is no step to go back through",
    );
}

#[test]
fn a_wheel_that_is_turned_where_it_does_nothing_is_not_a_step() {
    let mut window = window_with_a_long_video();
    // Over the preview the wheel does not scroll the timeline.
    let over_the_preview = window.the_preview_panel().center();
    window.scroll_the_wheel(over_the_preview, vec2(0.0, -ONE_NOTCH), Modifiers::NONE);
    // And at the start of the timeline, scrolling back further goes nowhere.
    scroll_along(&mut window, -3.0);
    assert_close(scrolled_along(&window), 0.0, "the setup: it is at the start");

    assert_eq!(window.application().timeline_view.view_history_steps_back(), 0);
}

// --- what a new step does to what was gone back from ---

#[test]
fn a_new_step_after_going_back_leaves_the_views_that_were_gone_back_from_behind() {
    let mut window = window_with_a_long_video();
    scroll_along(&mut window, 2.0);
    let scrolled = scrolled_along(&window);
    go_back(&mut window);
    assert_eq!(
        window.application().timeline_view.view_history_steps_forward(),
        1,
        "the setup: there is a view to go forward to"
    );
    pause(&mut window);

    zoom_along(&mut window, 0.5);
    let zoomed = scrolled_along(&window);
    go_forward(&mut window);

    assert_eq!(
        window.application().timeline_view.view_history_steps_forward(),
        0,
        "that is gone"
    );
    assert_close(scrolled_along(&window), zoomed, "and going forward does nothing");
    assert!(
        zoomed < scrolled,
        "(and the view that was gone back from is not where it is now)"
    );
}

// --- the buttons ---

#[test]
fn the_buttons_work_wherever_the_pointer_is_in_the_window() {
    for (place, over) in [
        ("the preview", 0),
        ("the properties panel", 1),
        ("the timeline panel", 2),
        ("a title box", 3),
    ] {
        let mut window = window_with_a_long_video();
        scroll_along(&mut window, 2.0);
        let position = match over {
            0 => window.the_preview_panel().center(),
            1 => window.the_properties_panel().center(),
            2 => window.the_timeline_panel().right_top() + vec2(-3.0, 3.0),
            _ => window.title_box_of_track(0).center(),
        };

        press_the_mouse_button(&mut window, THE_BUTTON_THAT_GOES_BACK, position);

        assert_close(scrolled_along(&window), 0.0, &format!("the Back button over {place}"));
        press_the_mouse_button(&mut window, THE_BUTTON_THAT_GOES_FORWARD, position);
        assert!(scrolled_along(&window) > 100.0, "the Forward button over {place}");
    }
}

#[test]
fn a_button_that_is_held_down_goes_one_step_and_not_a_step_for_every_frame_it_is_held() {
    let mut window = window_with_a_long_video();
    scroll_along(&mut window, 1.0);
    pause(&mut window);
    scroll_along(&mut window, 1.0);
    let second = scrolled_along(&window);
    pause(&mut window);
    scroll_along(&mut window, 1.0);
    assert!(
        scrolled_along(&window) > second + 50.0,
        "the setup: three steps, and the view is a step on from the second"
    );
    let position = window.middle_of_the_strip_at(0, 100);
    window.move_pointer_to(position);

    window.press(position, THE_BUTTON_THAT_GOES_BACK);
    window.harness.run_steps(10);
    window.release(position, THE_BUTTON_THAT_GOES_BACK);
    window.settle();

    assert_close(
        scrolled_along(&window),
        second,
        "one step back from the third, for all the frames the button was held",
    );
}

#[test]
fn the_buttons_do_nothing_while_a_dialog_has_to_be_dealt_with() {
    let mut window = window_with_a_long_video();
    scroll_along(&mut window, 2.0);
    let scrolled = scrolled_along(&window);
    window.application_mut().dialog = Some(Dialog::Message {
        title: "Something".to_owned(),
        text: "Something happened.".to_owned(),
    });
    window.settle();

    let position = window.the_preview_panel().center();
    press_the_mouse_button(&mut window, THE_BUTTON_THAT_GOES_BACK, position);

    assert_close(scrolled_along(&window), scrolled, "the view stays where it is");
}

#[test]
fn the_other_buttons_of_the_mouse_do_not_go_back_or_forward() {
    let mut window = window_with_a_long_video();
    scroll_along(&mut window, 2.0);
    let scrolled = scrolled_along(&window);
    let position = window.the_preview_panel().center();

    press_the_mouse_button(&mut window, PointerButton::Middle, position);

    assert_close(scrolled_along(&window), scrolled, "the middle button does not go back");
}

#[test]
fn going_back_and_forward_are_commands_too_and_do_nothing_when_there_is_nowhere_to_go() {
    let mut window = window_with_a_long_video();
    let at_the_start = scrolled_along(&window);

    window.do_this(Command::GoBackInTheTimelineViewHistory);
    window.do_this(Command::GoForwardInTheTimelineViewHistory);

    assert_close(scrolled_along(&window), at_the_start, "nowhere to go");
    scroll_along(&mut window, 2.0);
    let scrolled = scrolled_along(&window);
    window.do_this(Command::GoBackInTheTimelineViewHistory);
    assert_close(scrolled_along(&window), at_the_start, "back");
    window.do_this(Command::GoForwardInTheTimelineViewHistory);
    assert_close(scrolled_along(&window), scrolled, "and forward");
    assert!(!window.application().can_undo(), "none of it is an edit");
    assert!(!window.application().has_unsaved_changes(), "or something to save");
}

#[test]
fn a_different_project_is_looked_at_with_no_history() {
    let mut window = window_with_a_long_video();
    scroll_along(&mut window, 2.0);

    window.show_another_project(project_of(900));
    go_back(&mut window);

    assert_close(scrolled_along(&window), 0.0, "there is nothing to go back to");
}
