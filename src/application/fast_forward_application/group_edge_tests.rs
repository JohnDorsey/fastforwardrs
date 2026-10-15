// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of moving the edges of a clip group with the pointer, in the window: the pointer over an edge, the edge that goes along with the pointer
//! where it can be put and goes quickly to the nearest place where it cannot, and the group that takes in clips and lets go of them when the edge
//! is let go.

use egui::{CursorIcon, PointerButton, Pos2, vec2};

use super::clip_drag_tests::{in_frame, project_of_tracks};
use super::group_outline_tests::{
    bounds_of, colour_of_group, outlines_drawn_in, window_with_a_clip_group_of_the_first_two_clips,
};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project::{ClipId, GroupId, GroupKind, Member};

/// The window of the first two clips of three in a clip group: the clips are at frames 20 to 50 and 65 to 95, the group's rectangle is from
/// 20 to 95, and the third clip, which is not in it, is at 110 to 140. Gives the window, the group and the clips.
fn window_with_a_clip_group_and_a_clip_beside_it() -> (Window, GroupId, [ClipId; 3]) {
    let (window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let clips = [window.clip(0, 0), window.clip(0, 1), window.clip(0, 2)];
    (window, group, clips)
}

/// Lets the frames go by that it takes an edge to get where it is going (it goes there quickly, in a fraction of a second).
fn let_the_edge_get_there(window: &mut Window) {
    window.harness.run_steps(60);
}

fn cursor_shown(window: &Window) -> CursorIcon {
    window.harness.output().platform_output.cursor_icon
}

/// Where the left and the right of the outline of the group are on the screen, in the last frame.
fn edges_of_the_outline(window: &Window, group: GroupId) -> (f32, f32) {
    let outlines = outlines_drawn_in(window, colour_of_group(window, group));
    let bounds = bounds_of(&outlines[0]);
    (bounds.min.x, bounds.max.x)
}

/// A point on the right edge of the outline of the group, half way up it.
fn on_the_right_edge(window: &Window, group: GroupId) -> Pos2 {
    let (_, right) = edges_of_the_outline(window, group);
    Pos2::new(right, window.strip_of_track(0).center().y)
}

fn on_the_left_edge(window: &Window, group: GroupId) -> Pos2 {
    let (left, _) = edges_of_the_outline(window, group);
    Pos2::new(left, window.strip_of_track(0).center().y)
}

fn members_of(window: &Window, group: GroupId) -> Vec<Member> {
    window.application().project().group(group).unwrap().members().to_vec()
}

#[test]
fn the_pointer_is_a_double_arrow_over_the_left_and_the_right_edge_of_a_clip_group_and_nowhere_else() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();

    window.move_pointer_to(on_the_right_edge(&window, group));
    assert_eq!(cursor_shown(&window), CursorIcon::ResizeHorizontal);
    window.move_pointer_to(on_the_left_edge(&window, group));
    assert_eq!(cursor_shown(&window), CursorIcon::ResizeHorizontal);

    window.move_pointer_to(in_frame(&window, 0, 35));
    assert_ne!(
        cursor_shown(&window),
        CursorIcon::ResizeHorizontal,
        "over a clip in the middle of the group"
    );
    window.move_pointer_to(in_frame(&window, 0, 125));
    assert_ne!(
        cursor_shown(&window),
        CursorIcon::ResizeHorizontal,
        "over a clip that is not in it"
    );
}

#[test]
fn an_edge_that_is_dragged_over_a_clip_takes_it_in_when_it_is_let_go_and_nothing_is_changed_before() {
    let (mut window, group, [first, second, third]) = window_with_a_clip_group_and_a_clip_beside_it();
    let before = window.application().project().clone();
    let from = on_the_right_edge(&window, group);

    // To the end of the third clip, at 140.
    window.drag_and_hold(from, in_frame(&window, 0, 140), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    assert_eq!(
        window.application().project(),
        &before,
        "the group is not changed while the edge is held"
    );
    let (left, right) = edges_of_the_outline(&window, group);
    assert!(
        right > window.x_of_frame_on_the_strip(0, 139),
        "the outline goes as far as the edge was taken: {right}"
    );
    assert!(
        (left - (window.x_of_frame_on_the_strip(0, 20) - 3.0)).abs() < 0.5,
        "and the other edge is where it was: {left}"
    );
    assert_eq!(cursor_shown(&window), CursorIcon::ResizeHorizontal);

    window.release(in_frame(&window, 0, 140), PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    assert_eq!(
        members_of(&window, group),
        [Member::Clip(first), Member::Clip(second), Member::Clip(third)]
    );
    let (_, right) = edges_of_the_outline(&window, group);
    assert!(
        (right - (window.x_of_frame_on_the_strip(0, 140) + 3.0)).abs() < 0.5,
        "the outline is round the three clips: {right}"
    );
    window.do_this(Command::Undo);
    assert_eq!(window.application().project(), &before, "one step takes it back");
}

#[test]
fn an_edge_that_is_let_go_where_there_is_nothing_to_take_in_goes_back_to_fit_what_is_in_the_group() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let before = window.application().project().clone();
    let (_, right_before) = edges_of_the_outline(&window, group);
    let from = on_the_right_edge(&window, group);

    // To frame 105, between the clips.
    window.drag_and_hold(from, in_frame(&window, 0, 105), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    let (_, held) = edges_of_the_outline(&window, group);
    assert!(
        (held - window.x_of_frame_on_the_strip(0, 105)).abs() < 8.0,
        "it is where the pointer is, where that is a place for it: {held}"
    );

    window.release(in_frame(&window, 0, 105), PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    assert_eq!(
        window.application().project(),
        &before,
        "nothing was taken in, and nothing is changed"
    );
    let (_, right) = edges_of_the_outline(&window, group);
    assert!(
        (right - right_before).abs() < 0.5,
        "it shrank to fit the clips: {right} and {right_before}"
    );
}

#[test]
fn an_edge_that_is_dragged_across_a_clip_is_held_at_the_nearest_place_where_it_does_not_cut_across_it() {
    let (mut window, group, [_, _, third]) = window_with_a_clip_group_and_a_clip_beside_it();
    let from = on_the_right_edge(&window, group);

    // To frame 118, in the middle of the third clip (from 110 to 140): the nearest places are 110 and 140, and 110 is nearer.
    window.drag_and_hold(from, in_frame(&window, 0, 118), PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    let (_, shown) = edges_of_the_outline(&window, group);
    let (frame_110, frame_140) = (
        window.x_of_frame_on_the_strip(0, 110),
        window.x_of_frame_on_the_strip(0, 140),
    );
    assert!(
        (shown - (frame_110 + 3.0)).abs() < 1.0,
        "held at the start of the clip, which is the nearer: {shown} and {frame_110} (the end is at {frame_140})"
    );
    // Further in, it is nearer the end of the clip.
    window.move_pointer_to(in_frame(&window, 0, 132));
    let_the_edge_get_there(&mut window);
    let (_, shown) = edges_of_the_outline(&window, group);
    assert!((shown - (frame_140 + 3.0)).abs() < 1.0, "{shown} and {frame_140}");

    window.release(in_frame(&window, 0, 132), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    assert!(
        members_of(&window, group).contains(&Member::Clip(third)),
        "it was let go at the end of the clip: the clip is taken in"
    );
}

#[test]
fn the_left_edge_that_is_dragged_in_past_a_clip_lets_it_go() {
    let (mut window, group, [first, second, _]) = window_with_a_clip_group_and_a_clip_beside_it();
    let from = on_the_left_edge(&window, group);

    // To frame 58, between the first clip (20 to 50) and the second (65 to 95).
    window.drag(from, in_frame(&window, 0, 58), PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    assert_eq!(members_of(&window, group), [Member::Clip(second)]);
    assert!(
        window
            .application()
            .project()
            .shape_group_of(Member::Clip(first))
            .is_none(),
        "the first clip is out of the group"
    );
    let (left, _) = edges_of_the_outline(&window, group);
    assert!(
        (left - (window.x_of_frame_on_the_strip(0, 65) - 3.0)).abs() < 0.5,
        "the outline fits the clip that is left: {left}"
    );
}

#[test]
fn a_group_cannot_be_left_with_nothing_in_it_by_its_edge_and_the_edge_goes_back() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let before = window.application().project().clone();
    let from = on_the_right_edge(&window, group);

    // To frame 10, before everything that is in it: the nearest place that is allowed is where it was, or the clip first in it.
    window.drag(from, in_frame(&window, 0, 10), PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    assert_eq!(window.application().project().groups().len(), 1);
    assert!(
        !members_of(&window, group).is_empty(),
        "there is a clip in it: {:?} (it was {:?})",
        members_of(&window, group),
        before.group(group).unwrap().members()
    );
    assert_eq!(window.application().project().problems_with_the_groups(), vec![]);
}

#[test]
fn an_edge_that_is_let_go_where_it_was_taken_hold_of_is_not_an_edit_and_is_not_a_step_to_undo() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let before = window.application().project().clone();
    assert!(!window.application().can_undo(), "the setup: nothing has been done yet");
    let from = on_the_right_edge(&window, group);

    // Out over the third clip, and back to where it was, and let go.
    window.drag_and_hold(from, in_frame(&window, 0, 125), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    window.move_pointer_to(from);
    let_the_edge_get_there(&mut window);
    window.release(from, PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    assert_eq!(window.application().project(), &before);
    assert!(
        !window.application().can_undo(),
        "the edge was let go where the group has it, which asks nothing of the group"
    );
}

#[test]
fn an_edge_that_is_let_go_goes_to_where_the_group_has_it_by_way_of_the_places_between_and_not_at_once() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let from = on_the_right_edge(&window, group);
    let (_, where_the_group_has_it) = edges_of_the_outline(&window, group);

    // Let go between the second and third clips (the group ends at the second): the edge is shown there while it is held, and goes back.
    window.drag_and_hold(from, in_frame(&window, 0, 105), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    let (_, held_at) = edges_of_the_outline(&window, group);
    assert!(
        held_at > where_the_group_has_it + 20.0,
        "the setup: it is held further along: {held_at}"
    );
    window.release(in_frame(&window, 0, 105), PointerButton::Primary);

    let mut shown_after_each_frame = Vec::new();
    for _ in 0..25 {
        window.harness.step();
        shown_after_each_frame.push(edges_of_the_outline(&window, group).1);
    }

    assert!(
        shown_after_each_frame
            .iter()
            .all(|x| *x >= where_the_group_has_it - 0.5 && *x <= held_at + 0.5),
        "it goes from where it was held to where the group has it, and not past either: {shown_after_each_frame:?}"
    );
    assert!(
        shown_after_each_frame.windows(2).all(|pair| pair[1] <= pair[0] + 0.01),
        "and only towards it: {shown_after_each_frame:?}"
    );
    let on_the_way = shown_after_each_frame
        .iter()
        .filter(|x| **x > where_the_group_has_it + 1.0 && **x < held_at - 1.0)
        .count();
    assert!(
        on_the_way >= 2,
        "it is seen at places between, and does not jump: {shown_after_each_frame:?}"
    );
    assert!(
        (shown_after_each_frame.last().unwrap() - where_the_group_has_it).abs() < 0.5,
        "and it gets there: {shown_after_each_frame:?}"
    );
}

#[test]
fn a_click_on_an_edge_changes_nothing() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let before = window.application().project().clone();

    window.click(on_the_right_edge(&window, group));
    let_the_edge_get_there(&mut window);

    assert_eq!(window.application().project(), &before);
}

/// Whether the pointer is a double arrow when it is at `position`.
fn is_a_double_arrow_at(window: &mut Window, position: Pos2) -> bool {
    window.move_pointer_to(position);
    cursor_shown(window) == CursorIcon::ResizeHorizontal
}

#[test]
fn an_edge_is_taken_hold_of_in_a_stripe_three_points_to_each_side_of_it_that_stops_short_of_the_corners() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let outline = bounds_of(&outlines_drawn_in(&window, colour_of_group(&window, group))[0]);

    for edge in [outline.min.x, outline.max.x] {
        let middle = outline.center().y;
        for (how_far_out, is_in_the_stripe) in [(-4.0, false), (-2.0, true), (2.0, true), (4.0, false)] {
            assert_eq!(
                is_a_double_arrow_at(&mut window, Pos2::new(edge + how_far_out, middle)),
                is_in_the_stripe,
                "{how_far_out} points out from the edge at {edge}"
            );
        }
        for (how_far_from_the_end, is_in_the_stripe) in [(4.0, false), (8.0, false), (12.0, true)] {
            assert_eq!(
                is_a_double_arrow_at(&mut window, Pos2::new(edge, outline.min.y + how_far_from_the_end)),
                is_in_the_stripe,
                "{how_far_from_the_end} points under the top of the edge at {edge}"
            );
            assert_eq!(
                is_a_double_arrow_at(&mut window, Pos2::new(edge, outline.max.y - how_far_from_the_end)),
                is_in_the_stripe,
                "{how_far_from_the_end} points over the bottom of the edge at {edge}"
            );
        }
    }
}

#[test]
fn an_edge_that_is_scrolled_out_of_the_strips_has_no_stripe_over_the_title_boxes() {
    // A long timeline, scrolled until the first edge of the group (at frame 20) is 50 points before the strips, over the title boxes.
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30), (15, 30), (15, 30), (700, 30)]]));
    let clips = [window.clip(0, 0), window.clip(0, 1)];
    let group = window
        .application_mut()
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();
    let points_to_frame_20 = window.x_of_frame_on_the_strip(0, 20) - window.x_of_frame_on_the_strip(0, 0);
    window.application_mut().timeline_view.scroll_offset = f64::from(points_to_frame_20 + 50.0);
    window.settle();

    let the_edge_that_can_be_seen = on_the_right_edge(&window, group);
    assert!(
        is_a_double_arrow_at(&mut window, the_edge_that_can_be_seen),
        "the setup: the edge that can be seen can be taken hold of"
    );
    let (left, _) = edges_of_the_outline(&window, group);
    assert!(
        left < window.strip_of_track(0).left() - 40.0,
        "the setup: the other is far out: {left}"
    );

    let over_the_title_box = Pos2::new(left, window.strip_of_track(0).center().y);
    assert!(
        window.box_of_track(0).contains(over_the_title_box),
        "the setup: that is over the title box"
    );
    assert!(!is_a_double_arrow_at(&mut window, over_the_title_box));
}

#[test]
fn an_edge_that_is_held_when_a_message_comes_up_is_let_go_of_and_nothing_is_changed_and_none_can_be_taken_hold_of_meanwhile()
 {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let before = window.application().project().clone();
    let (_, right_before) = edges_of_the_outline(&window, group);
    let from = on_the_right_edge(&window, group);
    window.drag_and_hold(from, in_frame(&window, 0, 140), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    let (_, held_at) = edges_of_the_outline(&window, group);
    assert!(
        held_at > right_before + 20.0,
        "the setup: the edge is held out at the clip: {held_at}"
    );

    window.application_mut().dialog = Some(Dialog::Message {
        title: "Something".to_owned(),
        text: "Something happened.".to_owned(),
    });
    let_the_edge_get_there(&mut window);
    let (_, right) = edges_of_the_outline(&window, group);
    assert!(
        (right - right_before).abs() < 0.5,
        "the edge is where the group has it: {right} and {right_before}"
    );
    window.release(in_frame(&window, 0, 140), PointerButton::Primary);
    let_the_edge_get_there(&mut window);
    assert_eq!(window.application().project(), &before, "nothing was done to the group");

    let the_edge = on_the_right_edge(&window, group);
    assert!(
        !is_a_double_arrow_at(&mut window, the_edge),
        "not over an edge while the message is up"
    );
}

#[test]
fn an_edge_follows_the_pointer_exactly_where_it_can_be_put_and_is_not_put_between_a_place_and_what_is_not_one() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let from = on_the_right_edge(&window, group);
    let points_to_a_frame = window.x_of_frame_on_the_strip(0, 1) - window.x_of_frame_on_the_strip(0, 0);
    assert!(
        0.4 * points_to_a_frame > 0.5,
        "the setup: a part of a frame can be seen"
    );

    // To the middle of frame 100, a place for the edge (any from 95, where the group ends, to 110, where the next clip begins): at once,
    // wherever in the frame the pointer is. (The edge is where the pointer is, which is not where a whole frame is.)
    window.drag_and_hold(from, in_frame(&window, 0, 100), PointerButton::Primary);
    let (_, shown) = edges_of_the_outline(&window, group);
    assert!(
        (shown - (in_frame(&window, 0, 100).x + 3.0)).abs() < 0.3,
        "{shown} and the pointer at {} ({points_to_a_frame} points to a frame)",
        in_frame(&window, 0, 100).x
    );

    // A little way into frame 110 and the clip that begins there is cut across by an edge that is where the pointer is: it is put at the
    // frame that the clip begins at.
    let a_little_way_into_the_frame =
        Pos2::new(window.x_of_frame_on_the_strip(0, 110) + 0.4 * points_to_a_frame, from.y);
    window.move_pointer_to(a_little_way_into_the_frame);
    window.move_pointer_to(a_little_way_into_the_frame);
    let (_, shown) = edges_of_the_outline(&window, group);
    assert!(
        (shown - (window.x_of_frame_on_the_strip(0, 110) + 3.0)).abs() < 0.3,
        "at the place and not into the clip: {shown} and the place at {}",
        window.x_of_frame_on_the_strip(0, 110)
    );
    window.release(a_little_way_into_the_frame, PointerButton::Primary);
}

#[test]
fn an_edge_can_be_taken_hold_of_again_when_the_one_that_was_let_go_has_got_to_where_the_group_has_it() {
    let (mut window, group, [first, second, third]) = window_with_a_clip_group_and_a_clip_beside_it();
    window.drag(
        on_the_right_edge(&window, group),
        in_frame(&window, 0, 140),
        PointerButton::Primary,
    );
    let_the_edge_get_there(&mut window);
    assert_eq!(
        members_of(&window, group),
        [Member::Clip(first), Member::Clip(second), Member::Clip(third)],
        "the setup: the group has all three"
    );

    // Back in, between the second and the third.
    window.drag(
        on_the_right_edge(&window, group),
        in_frame(&window, 0, 100),
        PointerButton::Primary,
    );
    let_the_edge_get_there(&mut window);

    assert_eq!(members_of(&window, group), [Member::Clip(first), Member::Clip(second)]);
}

#[test]
fn the_window_asks_for_nothing_more_while_an_edge_is_held_still_at_the_first_frame_with_the_pointer_far_before_it() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let from = on_the_left_edge(&window, group);
    // Over the title boxes, which are before the first frame of the timeline: the left edge is held at the first frame, the nearest place that it
    // can be put at.
    let over_the_title_box = Pos2::new(window.title_box_of_track(0).center().x, from.y);

    window.drag_and_hold(from, over_the_title_box, PointerButton::Primary);
    let_the_edge_get_there(&mut window);

    let (left, _) = edges_of_the_outline(&window, group);
    assert!(
        (left - (window.x_of_frame_on_the_strip(0, 0) - 3.0)).abs() < 0.5,
        "the setup: the edge is at the first frame: {left}"
    );
    assert!(
        !window.repaint_is_wanted_at_once(),
        "it is where it is going to be, so nothing is moving, and the window is not drawn again and again"
    );
    window.release(over_the_title_box, PointerButton::Primary);
}

#[test]
fn an_edge_that_is_taken_hold_of_is_where_it_was_in_every_frame_after_and_does_not_fly_in_from_the_other_edge() {
    let (mut window, group, _) = window_with_a_clip_group_and_a_clip_beside_it();
    let from = on_the_right_edge(&window, group);
    let (_, right_before) = edges_of_the_outline(&window, group);

    window.move_pointer_to(from);
    window.press(from, PointerButton::Primary);
    // The pointer goes on along a point at a time (the edge is taken hold of when it has gone a few points), and the edge goes with it: in no
    // frame is it back towards the other edge.
    for step in 1..=24 {
        window.move_pointer_to(from + vec2(step as f32, 0.0));
        let (_, right) = edges_of_the_outline(&window, group);
        assert!(
            right >= right_before - 0.5,
            "after {step} points the edge is shown at {right}, and it was at {right_before}"
        );
    }
    window.release(from + vec2(24.0, 0.0), PointerButton::Primary);
}
