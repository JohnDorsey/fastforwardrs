// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of dragging a rack of keyframes: into another drawer, onto an empty part of a track (where it is in a keyframe crate),
//! sideways in its drawer, and what is said when it cannot go where it is over.

use egui::{CursorIcon, PointerButton, Pos2, Rect, Shape, vec2};

use super::keyframe_drag_tests::{
    frame_of, keyframe_at, make_a_keyframe, points_per_frame, selected_keyframes, where_it_is,
    window_with_a_video_and_a_text,
};
use super::rack_tests::racks_drawn;
use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::theme;
use crate::application::timeline_panel::{NOT_ENOUGH_SPACE_MESSAGE, drawer_button_label};
use crate::project::{
    AnimationTarget, Clip, ClipId, GroupKind, Member, PropertyId, RackDestination, RackId, TextObject,
    TimelineFrameIndex,
};
use crate::test_support::{COLOUR_OF_A_TEXT, SIZE_OF_A_TEXT};

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 1.5, "{what}: {actual} is not {expected}");
}

/// Makes a rack of the loudness of the video in its own drawer, with keyframes at these frames.
fn rack_of_the_loudness_of(window: &mut Window, video: ClipId, frames: &[u64]) -> RackId {
    for at in frames {
        make_a_keyframe(window, AnimationTarget::Clip(video), PropertyId::Gain, *at);
    }
    window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(video))
        .unwrap()
        .first_rack_for(AnimationTarget::Clip(video), PropertyId::Gain)
        .unwrap()
        .id()
}

fn frames_of_the_keyframes_of_the_loudness_of(window: &Window, video: ClipId) -> Vec<u64> {
    window
        .application()
        .project()
        .keyframes_of(AnimationTarget::Clip(video), PropertyId::Gain)
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect()
}

/// The point on the screen that is `frame_number` along the strips, at height `y`.
fn at_frame(window: &Window, frame_number: u64, y: f32) -> Pos2 {
    Pos2::new(window.x_of_frame_on_the_strip(0, frame_number), y)
}

fn the_words_drawn_by_the_pointer(window: &Window) -> Option<(String, Pos2)> {
    window.shapes_drawn().into_iter().find_map(|shape| match shape {
        Shape::Text(text) if text.galley.text() == NOT_ENOUGH_SPACE_MESSAGE => {
            Some((text.galley.text().to_owned(), text.pos))
        }
        _ => None,
    })
}

fn cursor_shown(window: &Window) -> CursorIcon {
    window.harness.output().platform_output.cursor_icon
}

fn racks_in(window: &Window, owner: AnimationTarget) -> usize {
    window
        .application()
        .project()
        .drawer_of(owner)
        .map_or(0, |drawer| drawer.len())
}

// --- into another drawer ---

#[test]
fn a_rack_dragged_into_the_drawer_of_another_clip_goes_there_and_its_keyframes_are_where_they_were_in_time() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    // The drawer of the text, which is the one that is higher on the screen, is where the rack is dragged to: to the room under
    // the rack that is in it, straight up from where the rack is taken hold of.
    let racks = racks_drawn(&window);
    let (in_the_text, in_the_video) = (racks[0], racks[1]);
    let from = at_frame(&window, 70, in_the_video.center().y);
    let to = at_frame(&window, 70, in_the_text.bottom() + 10.0);

    window.drag(from, to, PointerButton::Primary);

    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(text)),
        2,
        "the rack is in the drawer of the text"
    );
    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(video)),
        0,
        "and not in the drawer of the video"
    );
    assert_eq!(
        frames_of_the_keyframes_of_the_loudness_of(&window, video),
        vec![60, 80],
        "the video has the keyframes it had: they are at the same frames of the timeline"
    );
    assert_eq!(
        racks_drawn(&window).len(),
        2,
        "both racks are drawn in the drawer of the text"
    );
}

#[test]
fn a_rack_dragged_sideways_as_it_is_dragged_into_another_drawer_has_its_keyframes_that_much_later_or_earlier() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 70]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    let racks = racks_drawn(&window);
    let (in_the_text, in_the_video) = (racks[0], racks[1]);
    let from = at_frame(&window, 65, in_the_video.center().y);
    let to = at_frame(&window, 75, in_the_text.bottom() + 10.0);

    window.drag(from, to, PointerButton::Primary);

    let frames = frames_of_the_keyframes_of_the_loudness_of(&window, video);
    assert_eq!(frames[1] - frames[0], 10, "the distance between them is what it was");
    assert!(
        (68..=70).contains(&frames[0]),
        "10 frames later, or one less for the first move of the pointer: {frames:?}"
    );
}

#[test]
fn a_rack_that_is_dragged_further_along_than_the_drawer_it_is_over_goes_stops_at_the_end_of_it() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 70]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    let racks = racks_drawn(&window);
    let (in_the_text, in_the_video) = (racks[0], racks[1]);
    // The drawer of the text lasts to frame 99; the pointer is let go over it, but the rack was taken far to the left.
    let from = at_frame(&window, 20, in_the_video.center().y);
    let to = at_frame(&window, 95, in_the_text.bottom() + 10.0);

    window.drag(from, to, PointerButton::Primary);

    assert_eq!(
        frames_of_the_keyframes_of_the_loudness_of(&window, video),
        vec![89, 99],
        "the last at the last frame"
    );
}

#[test]
fn dragging_a_rack_up_and_down_between_two_drawers_is_one_step_of_the_history() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    window.application_mut().note_everything_is_saved();
    let racks = racks_drawn(&window);
    let (in_the_text, in_the_video) = (racks[0], racks[1]);
    let from = at_frame(&window, 70, in_the_video.center().y);

    window.drag(
        from,
        at_frame(&window, 70, in_the_text.bottom() + 10.0),
        PointerButton::Primary,
    );
    assert_eq!(racks_in(&window, AnimationTarget::Clip(text)), 2);
    assert!(window.application().has_unsaved_changes());
    window.do_this(Command::Undo);

    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(text)),
        1,
        "the rack is back where it was"
    );
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 1);
    assert_eq!(frames_of_the_keyframes_of_the_loudness_of(&window, video), vec![60, 80]);
}

// --- sideways in its drawer ---

#[test]
fn a_rack_dragged_sideways_in_its_drawer_moves_its_keyframes_together_and_not_out_of_it() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let rack = rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    let on_the_rack = racks_drawn(&window)[0];
    let per_frame = points_per_frame(&window);
    let from = at_frame(&window, 70, on_the_rack.center().y);

    window.drag(from, from + vec2(30.0 * per_frame, 0.0), PointerButton::Primary);

    let frames = frames_of_the_keyframes_of_the_loudness_of(&window, video);
    assert_eq!(frames[1] - frames[0], 20, "the same distance apart");
    assert!(
        (88..=90).contains(&frames[0]),
        "30 frames later, less the first move of the pointer: {frames:?}"
    );
    assert_eq!(
        window.application().project().located_rack(rack).unwrap().owner,
        AnimationTarget::Clip(video),
        "and in its own drawer"
    );
}

// --- onto a track ---

#[test]
fn a_rack_dragged_onto_an_empty_part_of_a_track_is_in_a_new_crate_that_lasts_as_long_as_its_keyframes_and_is_where_they_were_dropped()
 {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    let on_the_rack = racks_drawn(&window)[0];
    let in_front = window.strip_of_track(0);
    // The text is from frame 40 to frame 100 on the track in front, and the video lasts 300 frames: the empty stretch of the
    // track in front is from 100 to the end of the video. The rack is taken at frame 100 (it is as long as its drawer, and can be
    // taken anywhere along it) and put down 60 frames later.
    let from = at_frame(&window, 100, on_the_rack.center().y);
    let to = at_frame(&window, 160, in_front.center().y);

    window.drag(from, to, PointerButton::Primary);

    let track = &window.application().project().tracks()[0];
    assert_eq!(track.entries().len(), 2, "the text and one crate");
    let crate_placement = track.placement_at_index(1).unwrap();
    assert!(matches!(crate_placement.clip, Clip::Crate(_)));
    assert!(
        (118..=120).contains(&crate_placement.first_timeline_frame.value()),
        "60 frames later than the first keyframe was, less the first move of the pointer: {}",
        crate_placement.first_timeline_frame.value()
    );
    assert_eq!(
        crate_placement.timeline_frame_count(),
        21,
        "from the first keyframe to the last, and not through the empty stretch"
    );
    assert_eq!(crate_placement.drawer.len(), 1, "with the rack in it");
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 0);
    let frames = frames_of_the_keyframes_of_the_loudness_of(&window, video);
    assert_eq!(
        (frames[0], frames[1] - frames[0]),
        (crate_placement.first_timeline_frame.value(), 20),
        "the keyframes are the first and the last frames of the crate, as far apart as they were"
    );
}

#[test]
fn a_rack_dragged_onto_a_track_at_the_frames_of_a_clip_is_put_where_the_empty_stretch_begins_and_not_refused() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    let on_the_rack = racks_drawn(&window)[0];
    let in_front = window.strip_of_track(0);
    // The text is from frame 40 to frame 100 on the track in front: the keyframes, at 60 and 80, are where it is, so they go as far as
    // the empty stretch after it, and the first is at its first frame.
    let from = at_frame(&window, 150, on_the_rack.center().y);
    let to = at_frame(&window, 150, in_front.center().y);

    window.drag(from, to, PointerButton::Primary);

    let track = &window.application().project().tracks()[0];
    let crate_placement = track.placement_at_index(1).unwrap();
    assert_eq!(
        crate_placement.first_timeline_frame.value(),
        100,
        "from where the text ends"
    );
    assert_eq!(crate_placement.end_timeline_frame.value(), 121);
    assert_eq!(
        frames_of_the_keyframes_of_the_loudness_of(&window, video),
        vec![100, 120]
    );
}

#[test]
fn a_crate_is_a_box_with_its_racks_in_the_middle_of_it_up_and_down_and_as_long_as_it_is() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    let on_the_rack = racks_drawn(&window)[0];
    let in_front = window.strip_of_track(0);
    window.drag(
        at_frame(&window, 150, on_the_rack.center().y),
        at_frame(&window, 150, in_front.center().y),
        PointerButton::Primary,
    );

    let boxes: Vec<Rect> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::CRATE_CLIP_BACKGROUND => Some(rectangle.rect),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 1, "the crate is drawn");
    let rack = racks_drawn(&window)[0];
    assert_close(
        rack.center().y,
        boxes[0].center().y,
        "the rack is in the middle of the crate up and down",
    );
    assert_close(rack.left(), boxes[0].left(), "and as long as the crate");
    assert_close(
        rack.right(),
        boxes[0].right().min(window.strip_of_track(0).right()),
        "to its end",
    );
}

#[test]
fn a_rack_dragged_out_of_a_crate_into_a_drawer_leaves_the_crate_with_nothing_and_it_goes() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    let on_the_rack = racks_drawn(&window)[0];
    let in_front = window.strip_of_track(0);
    window.drag(
        at_frame(&window, 150, on_the_rack.center().y),
        at_frame(&window, 150, in_front.center().y),
        PointerButton::Primary,
    );
    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        2,
        "the crate is there"
    );
    // (It is from frame 100 to frame 120, as long as the keyframes need, and the rack in it is as long as it is.)
    let in_the_crate = racks_drawn(&window)[0];
    let video_strip = window.strip_of_track(1);

    // Down into the drawer of the video, which is still open and has nothing in it.
    window.drag(
        at_frame(&window, 110, in_the_crate.center().y),
        at_frame(&window, 110, video_strip.bottom() + 12.0),
        PointerButton::Primary,
    );

    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        1,
        "the crate has gone"
    );
    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(video)),
        1,
        "and the rack is in the drawer of the video"
    );
}

// --- when it cannot go ---

#[test]
fn a_rack_whose_keyframes_are_further_apart_than_a_drawer_is_long_says_so_by_the_pointer_over_it_and_stays_where_it_was()
 {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[10, 100]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    let racks = racks_drawn(&window);
    let (in_the_text, in_the_video) = (racks[0], racks[1]);
    let before = window.application().project().clone();
    let from = at_frame(&window, 60, in_the_video.center().y);
    let to = at_frame(&window, 60, in_the_text.bottom() + 10.0);

    window.drag_and_hold(from, to, PointerButton::Primary);

    let (words, at) = the_words_drawn_by_the_pointer(&window).expect("it says why");
    assert_eq!(words, "not enough space here");
    assert!(
        at.distance(to) < 60.0,
        "by the pointer: the words at {at:?}, the pointer at {to:?}"
    );
    assert_eq!(cursor_shown(&window), CursorIcon::NotAllowed);
    assert_eq!(window.application().project(), &before, "nothing was moved");
    window.release(to, PointerButton::Primary);
    window.settle();
    assert_eq!(
        window.application().project(),
        &before,
        "and nothing was when it was let go there"
    );
    assert!(
        the_words_drawn_by_the_pointer(&window).is_none(),
        "the words are gone with the drag"
    );
}

#[test]
fn a_rack_that_cannot_go_where_the_pointer_is_for_a_reason_that_is_not_space_does_not_say_there_is_not_enough() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    // The clip group has the text, from 40 to 100 on the track in front, and the video, so its rectangle goes from frame 0 to 300 across both
    // tracks: the empty stretch of the track in front after the text is inside it, and a crate there would be a clip that is not in the group.
    window.do_this(Command::CreateGroup {
        kind: GroupKind::ClipGroup,
        members: vec![Member::Clip(text), Member::Clip(video)],
    });
    let before = window.application().project().clone();
    let on_the_rack = racks_drawn(&window)[0];
    let in_front = window.strip_of_track(0);
    let from = at_frame(&window, 150, on_the_rack.center().y);
    let to = at_frame(&window, 150, in_front.center().y);

    window.drag_and_hold(from, to, PointerButton::Primary);

    assert_eq!(
        cursor_shown(&window),
        CursorIcon::NotAllowed,
        "the rack cannot go there"
    );
    assert!(
        the_words_drawn_by_the_pointer(&window).is_none(),
        "and the words about space are not there: it is not for want of space"
    );
    window.release(to, PointerButton::Primary);
    window.settle();
    assert_eq!(window.application().project(), &before, "nothing was moved");
}

#[test]
fn the_words_go_when_the_pointer_comes_to_a_place_where_the_rack_fits() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[10, 100]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    let racks = racks_drawn(&window);
    let (in_the_text, in_the_video) = (racks[0], racks[1]);
    let from = at_frame(&window, 60, in_the_video.center().y);
    window.drag_and_hold(
        from,
        at_frame(&window, 60, in_the_text.bottom() + 10.0),
        PointerButton::Primary,
    );
    assert!(the_words_drawn_by_the_pointer(&window).is_some());

    // Over the drawer of the video, which is as long as the video, it fits.
    window.move_pointer_to(at_frame(&window, 60, in_the_video.bottom() + 12.0));
    window.move_pointer_to(at_frame(&window, 60, in_the_video.bottom() + 12.0));

    assert!(the_words_drawn_by_the_pointer(&window).is_none());
    assert_ne!(cursor_shown(&window), CursorIcon::NotAllowed);
    window.release(
        at_frame(&window, 60, in_the_video.bottom() + 12.0),
        PointerButton::Primary,
    );
}

#[test]
fn a_rack_can_be_dragged_into_a_drawer_that_has_a_rack_for_the_same_thing_already_and_the_two_are_not_merged() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    let first = rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    // A rack of the loudness of the video goes in the drawer of the text, and a new keyframe makes another in the drawer of the
    // video: that is dragged into the drawer of the text, which has a rack of the loudness of the video already.
    window.do_this(Command::MoveRacks {
        racks: vec![first],
        to: RackDestination::Drawer(AnimationTarget::Clip(text)),
        shift_in_frames: 0,
        slot: None,
    });
    let in_the_drawer_of_the_video = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 200);
    let second = window
        .application()
        .project()
        .locate_keyframe(in_the_drawer_of_the_video)
        .unwrap()
        .rack
        .rack
        .id();
    window.harness.run_steps(40);
    let racks = racks_drawn(&window);
    let in_the_video = *racks.last().unwrap();
    let in_the_text = racks[0];
    // (Straight up, so that the rack is not moved along its own drawer on the way.)
    let from = at_frame(&window, 70, in_the_video.center().y);
    let to = at_frame(&window, 70, in_the_text.top() + 5.0);

    window.drag_and_hold(from, to, PointerButton::Primary);

    assert!(
        the_words_drawn_by_the_pointer(&window).is_none(),
        "it is not a matter of room"
    );
    window.release(to, PointerButton::Primary);
    window.settle();
    let project = window.application().project();
    let video_target = AnimationTarget::Clip(video);
    let drawer_of_the_text = project.drawer_of(AnimationTarget::Clip(text)).unwrap();
    assert_eq!(
        drawer_of_the_text.len(),
        3,
        "the rack of the rotation of the text and the two of the loudness"
    );
    let mut loudness_racks: Vec<RackId> = drawer_of_the_text
        .racks_for(video_target, PropertyId::Gain)
        .map(|rack| rack.id())
        .collect();
    loudness_racks.sort();
    assert_eq!(loudness_racks, vec![first, second], "the two are not one rack");
    assert_eq!(project.drawer_of(video_target).unwrap().len(), 0);
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
}

// --- up and down in a drawer ---

/// A window in which the drawer of the video has three racks, in this order from the top: its loudness, whether it is muted, and
/// how far it is turned. Gives the window and the racks in that order.
fn window_with_three_racks_in_the_drawer_of_the_video() -> (Window, ClipId, [RackId; 3]) {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    for property in [PropertyId::Gain, PropertyId::Muted, PropertyId::Rotation] {
        make_a_keyframe(&mut window, target, property, 100);
    }
    let racks = window
        .application()
        .project()
        .drawer_of(target)
        .unwrap()
        .racks()
        .iter()
        .map(|rack| rack.id())
        .collect::<Vec<_>>();
    window.application_mut().note_everything_is_saved();
    window.harness.run_steps(40);
    (window, video, [racks[0], racks[1], racks[2]])
}

fn racks_in_the_drawer_of(window: &Window, video: ClipId) -> Vec<RackId> {
    window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(video))
        .unwrap()
        .racks()
        .iter()
        .map(|rack| rack.id())
        .collect()
}

#[test]
fn a_rack_dragged_up_past_the_middle_of_the_rack_above_it_changes_places_with_it_and_not_before() {
    let (mut window, video, [loudness, muted, turn]) = window_with_three_racks_in_the_drawer_of_the_video();
    let rectangles = racks_drawn(&window);
    let (from, near_the_one_above) = (
        at_frame(&window, 150, rectangles[2].center().y),
        at_frame(&window, 150, rectangles[1].bottom() - 1.0),
    );

    // Up to just short of the middle of the rack above: it is still below it.
    window.drag_and_hold(from, near_the_one_above, PointerButton::Primary);
    assert_eq!(racks_in_the_drawer_of(&window, video), vec![loudness, muted, turn]);
    // And on past the middle of it.
    window.move_pointer_to(at_frame(&window, 150, rectangles[1].center().y - 1.0));
    assert_eq!(racks_in_the_drawer_of(&window, video), vec![loudness, turn, muted]);
    window.release(
        at_frame(&window, 150, rectangles[1].center().y - 1.0),
        PointerButton::Primary,
    );
}

#[test]
fn a_rack_dragged_to_the_top_of_its_drawer_is_the_first_and_a_rack_dragged_to_the_bottom_is_the_last() {
    let (mut window, video, [loudness, muted, turn]) = window_with_three_racks_in_the_drawer_of_the_video();
    let rectangles = racks_drawn(&window);

    window.drag(
        at_frame(&window, 150, rectangles[2].center().y),
        at_frame(&window, 150, rectangles[0].top() + 1.0),
        PointerButton::Primary,
    );
    assert_eq!(racks_in_the_drawer_of(&window, video), vec![turn, loudness, muted]);

    let rectangles = racks_drawn(&window);
    window.drag(
        at_frame(&window, 150, rectangles[0].center().y),
        at_frame(&window, 150, rectangles[2].bottom() + 4.0),
        PointerButton::Primary,
    );
    assert_eq!(racks_in_the_drawer_of(&window, video), vec![loudness, muted, turn]);
}

#[test]
fn changing_places_in_the_drawer_takes_nothing_but_the_order_and_a_whole_drag_is_one_step_of_the_history() {
    let (mut window, video, [loudness, muted, turn]) = window_with_three_racks_in_the_drawer_of_the_video();
    let rectangles = racks_drawn(&window);
    let before = window.application().project().clone();

    window.drag(
        at_frame(&window, 150, rectangles[0].center().y),
        at_frame(&window, 150, rectangles[2].center().y + 3.0),
        PointerButton::Primary,
    );

    assert_eq!(racks_in_the_drawer_of(&window, video), vec![muted, turn, loudness]);
    let after = window.application().project();
    for rack in [loudness, muted, turn] {
        assert_eq!(
            after.located_rack(rack).unwrap().rack,
            before.located_rack(rack).unwrap().rack,
            "{rack} has the keyframes it had, where it had them"
        );
    }
    assert!(window.application().has_unsaved_changes());
    window.do_this(Command::Undo);
    assert_eq!(
        racks_in_the_drawer_of(&window, video),
        vec![loudness, muted, turn],
        "one undo is enough"
    );
}

#[test]
fn a_rack_that_the_pointer_moves_a_little_in_its_own_place_does_not_change_places_or_the_project() {
    let (mut window, video, racks) = window_with_three_racks_in_the_drawer_of_the_video();
    let rectangles = racks_drawn(&window);
    let before = window.application().project().clone();

    window.drag(
        at_frame(&window, 150, rectangles[1].center().y - 2.0),
        at_frame(&window, 150, rectangles[1].center().y + 3.0),
        PointerButton::Primary,
    );

    assert_eq!(racks_in_the_drawer_of(&window, video), racks.to_vec());
    assert_eq!(window.application().project(), &before);
}

#[test]
fn a_rack_cannot_be_dragged_onto_a_place_that_is_not_a_drawer_or_an_empty_part_of_a_track() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    let on_the_rack = racks_drawn(&window)[0];
    let before = window.application().project().clone();
    let on_the_video = window.middle_of_the_strip_at(1, 150);

    window.drag_and_hold(
        at_frame(&window, 150, on_the_rack.center().y),
        on_the_video,
        PointerButton::Primary,
    );

    assert_eq!(cursor_shown(&window), CursorIcon::NotAllowed, "over a clip");
    window.release(on_the_video, PointerButton::Primary);
    window.settle();
    assert_eq!(window.application().project(), &before);
}

#[test]
fn clicking_a_rack_selects_it_and_does_not_change_the_project() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let rack = rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    window.application_mut().note_everything_is_saved();
    let on_the_rack = racks_drawn(&window)[0];

    window.click(at_frame(&window, 70, on_the_rack.center().y));

    assert_eq!(
        window.application().selected_items(),
        vec![crate::application::selection::SelectionItem::Rack(rack)]
    );
    assert!(!window.application().has_unsaved_changes());
}

// --- onto the drawer of a track, and crates with many racks ---

#[test]
fn a_rack_dragged_into_the_drawer_of_a_track_goes_there_and_counts_from_the_start_of_the_timeline() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    // The drawer of the track in front is opened with the button on its box.
    let button = {
        use egui_kittest::kittest::Queryable;
        window
            .harness
            .get_by_label(&crate::application::timeline_panel::track_drawer_button_label(
                &window.title_of_track(0),
            ))
            .rect()
            .center()
    };
    window.click(button);
    window.harness.run_steps(40);
    let on_the_rack = racks_drawn(&window)[0];
    let in_front = window.strip_of_track(0);
    let from = at_frame(&window, 70, on_the_rack.center().y);
    // Into the room of the drawer of the track, which is under its strip.
    let to = at_frame(&window, 70, in_front.bottom() + 14.0);

    window.drag(from, to, PointerButton::Primary);

    let track = window.application().project().tracks()[0].id();
    assert_eq!(racks_in(&window, AnimationTarget::Track(track)), 1);
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 0);
    assert_eq!(frames_of_the_keyframes_of_the_loudness_of(&window, video), vec![60, 80]);
}

#[test]
fn a_crate_with_more_racks_than_fit_in_the_height_of_the_track_reaches_below_it_and_has_them_all_in_it() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    let mut racks = Vec::new();
    for property in [
        PropertyId::Gain,
        PropertyId::Muted,
        PropertyId::Position,
        PropertyId::Size,
        PropertyId::Rotation,
    ] {
        make_a_keyframe(&mut window, target, property, 150);
        racks.push(
            window
                .application()
                .project()
                .drawer_of(target)
                .unwrap()
                .first_rack_for(target, property)
                .unwrap()
                .id(),
        );
    }
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    racks.push(
        window
            .application()
            .project()
            .drawer_of(AnimationTarget::Clip(text))
            .unwrap()
            .first_rack_for(AnimationTarget::Clip(text), PropertyId::Rotation)
            .unwrap()
            .id(),
    );
    let front = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveRacks {
        racks: vec![racks[0]],
        to: RackDestination::NewCrate {
            track: front,
            from_frame: 100,
            until_frame: 300,
        },
        shift_in_frames: 0,
        slot: None,
    });
    let crate_clip = window.application().project().tracks()[0]
        .placement_at_index(1)
        .unwrap()
        .id;
    for (index, rack) in racks.iter().enumerate().skip(1) {
        window.do_this(Command::MoveRacks {
            racks: vec![*rack],
            to: RackDestination::Drawer(AnimationTarget::Clip(crate_clip)),
            shift_in_frames: if index == 5 { 100 } else { 0 },
            slot: None,
        });
    }
    window.harness.run_steps(40);
    assert_eq!(racks_in(&window, AnimationTarget::Clip(crate_clip)), 6);

    let strip = window.strip_of_track(0);
    let crate_boxes: Vec<Rect> = window
        .clipped_shapes_drawn()
        .into_iter()
        .filter_map(|(clip_rectangle, shape)| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::CRATE_CLIP_BACKGROUND => {
                Some(rectangle.rect.intersect(clip_rectangle))
            }
            _ => None,
        })
        .collect();
    let bottom_of_the_crate = crate_boxes
        .iter()
        .map(|rectangle| rectangle.bottom())
        .fold(f32::MIN, f32::max);
    let all_the_racks = racks_drawn(&window);
    assert_eq!(all_the_racks.len(), 6, "all of them are drawn");
    assert!(
        bottom_of_the_crate > strip.bottom() + 10.0,
        "it reaches below the strip: to {bottom_of_the_crate}"
    );
    for rack in &all_the_racks {
        assert!(
            rack.top() >= strip.top() && rack.bottom() <= bottom_of_the_crate - theme::CRATE_MARGIN + 1.0,
            "{rack:?} is in the crate, to {bottom_of_the_crate}"
        );
    }
    for pair in all_the_racks.windows(2) {
        assert_close(
            pair[1].top() - pair[0].bottom(),
            theme::SPACE_BETWEEN_RACKS,
            "one under another",
        );
    }
}

// --- in the drawer of a clip that begins late ---

#[test]
fn a_rack_in_the_drawer_of_a_clip_that_begins_late_is_dragged_sideways_as_far_as_the_pointer_goes() {
    let (mut window, _, text) = window_with_a_video_and_a_text();
    // The text is from frame 40: its drawer begins there, and the frames of its racks are counted from there.
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 60);
    let on_the_rack = racks_drawn(&window)[0];
    let per_frame = points_per_frame(&window);
    let from = at_frame(&window, 75, on_the_rack.center().y);

    window.drag(from, from + vec2(10.0 * per_frame, 0.0), PointerButton::Primary);

    let frames: Vec<u64> = window
        .application()
        .project()
        .keyframes_of(AnimationTarget::Clip(text), PropertyId::Rotation)
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(frames[1] - frames[0], 10, "the same distance apart");
    assert!(
        (59..=60).contains(&frames[0]),
        "10 frames later, less the first move of the pointer: {frames:?}"
    );
}

// --- a drawer that is still sliding open ---

#[test]
fn a_rack_is_put_in_a_drawer_that_has_finished_sliding_open_and_not_before() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    assert_eq!(racks_drawn(&window).len(), 1);
    let from = at_frame(&window, 70, racks_drawn(&window)[0].center().y);
    // Under the strip of the text, which is where its drawer, shut now and with nothing in it, will be as it slides open. (The
    // timeline grows upward as the drawer pushes the row under it down, so where that is moves: the pointer follows it.)
    let under_the_text = |window: &Window| at_frame(window, 70, window.strip_of_track(0).bottom() + 12.0);
    window.drag_and_hold(from, under_the_text(&window), PointerButton::Primary);
    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(video)),
        1,
        "there is no drawer there for the rack to go in, as yet"
    );

    window
        .application_mut()
        .interface
        .timeline_view
        .open_the_drawer_of(AnimationTarget::Clip(text));
    for _ in 0..3 {
        window.move_pointer_to(under_the_text(&window));
    }
    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(text)),
        0,
        "a drawer that has begun to open, and has the pointer over what can be seen of it, is not open: the rack is not put in"
    );
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 1);

    window.harness.run_steps(30);
    let under_the_text = under_the_text(&window);
    window.move_pointer_to(under_the_text);
    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(text)),
        1,
        "when it has opened all the way it takes the rack"
    );
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 0);
    window.release(under_the_text, PointerButton::Primary);
}

#[test]
fn the_keyframes_in_a_drawer_that_is_still_sliding_open_cannot_be_taken_hold_of() {
    let (mut window, _, text) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    let per_frame = points_per_frame(&window);
    // Shut the drawer, and open it again: it slides.
    let button = window.rectangle_of(&drawer_button_label(&window.title_of_track(0), text));
    window.click(button.center());
    window.harness.run_steps(30);
    assert_eq!(racks_drawn(&window).len(), 0, "shut");
    window
        .application_mut()
        .interface
        .timeline_view
        .open_the_drawer_of(AnimationTarget::Clip(text));
    window.harness.run_steps(4);
    assert_eq!(racks_drawn(&window).len(), 1, "the rack can be seen, as it comes out");

    let from = where_it_is(&window, keyframe);
    window.drag(from, from + vec2(20.0 * per_frame, 0.0), PointerButton::Primary);

    assert_eq!(frame_of(&window, keyframe), 50, "the keyframe was not moved");
    assert_eq!(selected_keyframes(&window), vec![], "nor selected");

    // Once the drawer is open the same drag moves it.
    window.harness.run_steps(30);
    let from = where_it_is(&window, keyframe);
    window.drag(from, from + vec2(20.0 * per_frame, 0.0), PointerButton::Primary);
    assert!(frame_of(&window, keyframe) >= 68, "{}", frame_of(&window, keyframe));
}

// --- while a message is up ---

#[test]
fn nothing_in_a_drawer_can_be_taken_hold_of_while_a_message_is_up() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let rack = rack_of_the_loudness_of(&mut window, video, &[100, 120]);
    let keyframe =
        super::keyframe_drag_tests::keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    let per_frame = points_per_frame(&window);
    let on_the_keyframe = where_it_is(&window, keyframe);
    let on_the_rack = at_frame(&window, 140, racks_drawn(&window)[0].center().y);
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    window.drag(
        on_the_keyframe,
        on_the_keyframe + vec2(20.0 * per_frame, 0.0),
        PointerButton::Primary,
    );
    window.drag(on_the_rack, on_the_rack + vec2(0.0, -150.0), PointerButton::Primary);

    assert_eq!(
        frames_of_the_keyframes_of_the_loudness_of(&window, video),
        vec![100, 120]
    );
    assert_eq!(
        window.application().project().located_rack(rack).unwrap().owner,
        AnimationTarget::Clip(video),
        "the rack is where it was"
    );
    assert_eq!(selected_keyframes(&window), vec![]);

    // With the message gone, the same drag moves the keyframe.
    window.application_mut().interface.dialog = None;
    window.settle();
    window.drag(
        on_the_keyframe,
        on_the_keyframe + vec2(20.0 * per_frame, 0.0),
        PointerButton::Primary,
    );
    assert!(frames_of_the_keyframes_of_the_loudness_of(&window, video)[0] >= 118);
}

// --- drawers over one another ---

#[test]
fn a_rack_let_go_where_two_open_drawers_overlap_goes_in_the_one_that_is_on_top() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[60, 80]);
    make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Muted, 70);
    // Five racks in the drawer of the text, which is on the track in front: it is so tall that it reaches down over the room
    // of the drawer of the video, which is under the track behind (and over the first of its two racks).
    for property in [
        PropertyId::Position,
        PropertyId::Size,
        PropertyId::Rotation,
        SIZE_OF_A_TEXT,
        COLOUR_OF_A_TEXT,
    ] {
        make_a_keyframe(&mut window, AnimationTarget::Clip(text), property, 50);
    }
    // The keyframes opened the drawer of the text so that it pushes the row under it down. Shut and opened again with Shift held
    // it lies over the row under it instead, and so over the drawer of the video that hangs from that.
    let button_of_the_text = |window: &Window| {
        window
            .rectangle_of(&drawer_button_label(&window.title_of_track(0), text))
            .center()
    };
    window.click(button_of_the_text(&window));
    window.harness.run_steps(40);
    window.click_with_shift(button_of_the_text(&window));
    window.harness.run_steps(40);
    let racks = racks_drawn(&window);
    assert_eq!(
        racks.len(),
        7,
        "five in the drawer of the text and two in that of the video"
    );
    let (last_of_the_text, first_in_the_video, second_in_the_video) = (racks[4], racks[5], racks[6]);
    // The drawer of the text goes on below its last rack by as much as the corners of a clip are round, and the drawer of the video
    // begins a little over its rack: between those two, the drawers are over one another.
    let in_both = last_of_the_text.bottom() + 19.0;
    assert!(
        in_both < last_of_the_text.bottom() + f32::from(theme::CLIP_CORNER_RADIUS)
            && in_both > first_in_the_video.top() - theme::SPACE_ABOVE_THE_RACKS,
        "the drawers do overlap there: {last_of_the_text:?} {first_in_the_video:?}"
    );
    // The second rack of the video is taken hold of: it is below where the drawer of the text ends, and the first is partly under it.
    assert!(second_in_the_video.top() > last_of_the_text.bottom() + f32::from(theme::CLIP_CORNER_RADIUS));
    let from = at_frame(&window, 40, second_in_the_video.center().y);
    let to = at_frame(&window, 40, in_both);

    window.drag(from, to, PointerButton::Primary);

    assert_eq!(
        racks_in(&window, AnimationTarget::Clip(text)),
        6,
        "the drawer of the text, which is over the other, has the rack"
    );
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 1);
}

// --- over what there is no crate to be made of ---

#[test]
fn a_rack_cannot_be_put_on_a_track_after_the_end_of_everything_which_is_not_a_place_for_a_crate_and_says_nothing() {
    let mut project = project_of(60);
    let in_front = project.add_track_in_front();
    // The text goes on to frame 69: the finished video is 70 frames long, and the track in front has nothing after it.
    project
        .insert_text(in_front, TimelineFrameIndex::new(50), TextObject::default(), 20)
        .unwrap();
    let mut window = Window::showing(project);
    let video = window.clip(1, 0);
    rack_of_the_loudness_of(&mut window, video, &[20, 40]);
    let from = at_frame(&window, 30, racks_drawn(&window)[0].center().y);
    let after_the_end = window.middle_of_the_strip_at(0, 120);

    window.drag_and_hold(from, after_the_end, PointerButton::Primary);

    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        1,
        "no crate was made"
    );
    assert_eq!(racks_in(&window, AnimationTarget::Clip(video)), 1);
    assert_eq!(cursor_shown(&window), CursorIcon::NotAllowed);
    assert_eq!(
        the_words_drawn_by_the_pointer(&window),
        None,
        "it is not said that there is no room: there is no place there"
    );
    window.release(after_the_end, PointerButton::Primary);
}

// --- a drag that is going on when a message comes up ---

#[test]
fn a_drag_of_a_keyframe_that_is_going_on_when_a_message_comes_up_does_not_go_on() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[100, 140]);
    let keyframe = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    let per_frame = points_per_frame(&window);
    let from = where_it_is(&window, keyframe);
    window.drag_and_hold(from, from + vec2(10.0 * per_frame, 0.0), PointerButton::Primary);
    let went_to = frame_of(&window, keyframe);
    assert!(went_to >= 108, "it went along with the pointer: {went_to}");

    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.move_pointer_to(from + vec2(25.0 * per_frame, 0.0));
    window.move_pointer_to(from + vec2(30.0 * per_frame, 0.0));

    assert_eq!(
        frame_of(&window, keyframe),
        went_to,
        "the drag is over: the keyframe is where it was when the message came up"
    );
}

#[test]
fn a_drag_of_a_rack_that_is_going_on_when_a_message_comes_up_does_not_go_on() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    rack_of_the_loudness_of(&mut window, video, &[100, 120]);
    let per_frame = points_per_frame(&window);
    // (Taken hold of at frame 130, so that going on to the right stays over the strip, which ends at about frame 172.)
    let from = at_frame(&window, 130, racks_drawn(&window)[0].center().y);
    window.drag_and_hold(from, from + vec2(10.0 * per_frame, 0.0), PointerButton::Primary);
    let went_to = frames_of_the_keyframes_of_the_loudness_of(&window, video);
    assert!(went_to[0] >= 108, "it went along with the pointer: {went_to:?}");

    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.move_pointer_to(from + vec2(20.0 * per_frame, 0.0));
    window.move_pointer_to(from + vec2(25.0 * per_frame, 0.0));

    assert_eq!(
        frames_of_the_keyframes_of_the_loudness_of(&window, video),
        went_to,
        "the drag is over: the rack is where it was when the message came up"
    );
}
