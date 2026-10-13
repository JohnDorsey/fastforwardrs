//! Tests of dragging racks of keyframes along the timeline together: several racks that are selected together, and racks that are
//! selected with clips or with keyframes. (A rack that is dragged on its own goes to other drawers and other places in the drawer
//! it is in, which is in `rack_drag_tests`.)
//!
//! The window of these tests shows the first 170 frames or so of the timeline, and the pointer cannot go further than that, so
//! the drags here are within them: where something has to be stopped by the end of a drawer, it is put near the end of the 300
//! frames of the video first.

use egui::{PointerButton, Pos2, vec2};

use super::keyframe_drag_tests::{
    frame_of, keyframe_at, make_a_keyframe, points_per_frame, selected_keyframes, where_it_is,
};
use super::rack_mouse_tests::{
    Racks, on_the_rack, select_racks, selected_racks, the_rack_of, window_with_racks_of_the_video,
};
use super::rack_tests::racks_drawn;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{AnimationTarget, ClipId, KeyframeMove, PropertyId, RackDestination, RackId};

/// The frames of the timeline that the keyframes of `rack` are at.
fn frames_of(window: &Window, rack: RackId) -> Vec<u64> {
    let located = window
        .application()
        .project()
        .located_rack(rack)
        .expect("the rack is there");
    located
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| located.timeline_frame_of(keyframe.frame))
        .collect()
}

/// Takes hold of what is at `from` and drags it `frames` frames of the timeline to the right (to the left, if negative), the pointer
/// going straight along.
fn drag_sideways_by(window: &mut Window, from: Pos2, frames: f32) {
    let points = frames * points_per_frame(window);
    window.drag(from, from + vec2(points, 0.0), PointerButton::Primary);
}

/// How many frames the keyframes of `rack` have gone since they were at the frames `before`: the same for every one of them, or the
/// rack was not moved as a whole.
fn how_far_they_went(window: &Window, rack: RackId, before: &[u64]) -> i64 {
    let after = frames_of(window, rack);
    assert_eq!(after.len(), before.len(), "no keyframe was lost or made");
    let shifts: Vec<i64> = after
        .iter()
        .zip(before)
        .map(|(after, before)| *after as i64 - *before as i64)
        .collect();
    assert!(
        shifts.windows(2).all(|pair| pair[0] == pair[1]),
        "all the keyframes of a rack went as far as one another: {before:?} to {after:?}"
    );
    shifts[0]
}

fn racks_of_the_drawer_of(window: &Window, clip: ClipId) -> Vec<RackId> {
    window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(clip))
        .expect("the clip has a drawer")
        .racks()
        .iter()
        .map(|rack| rack.id())
        .collect()
}

/// The frame of the timeline that `clip` begins at.
fn start_of(window: &Window, clip: ClipId) -> u64 {
    window
        .application()
        .project()
        .placement_of(clip)
        .expect("the clip is there")
        .first_timeline_frame
        .value()
}

/// How many frames into the clip whose drawer `rack` is in the first keyframe of it is.
fn how_far_into_the_clip(window: &Window, rack: RackId) -> u64 {
    window
        .application()
        .project()
        .located_rack(rack)
        .expect("the rack is there")
        .rack
        .first_frame()
        .expect("it has keyframes")
}

/// Puts the only keyframe of `rack` at `frame` of the drawer of the video (which begins where the timeline does), so that it has
/// as little room to go on as is wanted.
fn put_the_keyframe_of_the_rack_at(window: &mut Window, rack: RackId, frame: u64) {
    let keyframe = window
        .application()
        .project()
        .located_rack(rack)
        .unwrap()
        .rack
        .keyframes()[0]
        .id();
    window.do_this(Command::MoveKeyframes(vec![KeyframeMove {
        keyframe,
        to_rack: rack,
        to_frame: frame,
    }]));
    window.harness.run_steps(5);
}

fn select(window: &mut Window, items: Vec<SelectionItem>) {
    window.do_this(Command::SelectItems {
        items,
        change: SelectionChange::Replace,
    });
    window.harness.run_steps(5);
}

// --- racks that are selected together ---

#[test]
fn racks_that_are_selected_together_are_dragged_sideways_together_by_the_same_number_of_frames() {
    let Racks {
        mut window,
        upper,
        lower,
        turn,
        ..
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper, turn]);
    let (upper_before, lower_before, turn_before) = (
        frames_of(&window, upper),
        frames_of(&window, lower),
        frames_of(&window, turn),
    );

    let from = on_the_rack(&window, 0, 10);
    drag_sideways_by(&mut window, from, 30.0);

    let upper_went = how_far_they_went(&window, upper, &upper_before);
    let turn_went = how_far_they_went(&window, turn, &turn_before);
    assert!(
        (27..=30).contains(&upper_went),
        "about as far as the pointer went: {upper_went}"
    );
    assert_eq!(upper_went, turn_went, "the two went together");
    assert_eq!(
        frames_of(&window, lower),
        lower_before,
        "and the one that was not selected stayed"
    );
    assert_eq!(selected_racks(&window), vec![upper, turn], "both are still selected");
}

#[test]
fn the_racks_can_be_taken_hold_of_by_any_of_them_and_go_back_the_other_way_too() {
    let Racks {
        mut window,
        upper,
        turn,
        ..
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper, turn]);
    let (upper_before, turn_before) = (frames_of(&window, upper), frames_of(&window, turn));

    // By the third rack, which is the last of the two, and to the left.
    let from = on_the_rack(&window, 2, 160);
    drag_sideways_by(&mut window, from, -40.0);

    let turn_went = how_far_they_went(&window, turn, &turn_before);
    let upper_went = how_far_they_went(&window, upper, &upper_before);
    assert!((-41..=-39).contains(&turn_went), "{turn_went}");
    assert_eq!(upper_went, turn_went);
}

#[test]
fn racks_dragged_together_stop_together_when_the_first_to_come_to_the_end_of_its_drawer_does() {
    let Racks {
        mut window,
        upper,
        lower,
        turn,
        ..
    } = window_with_racks_of_the_video();
    // The drawer of the video has 300 frames: the keyframe of the turn, at 250, can go on 49 frames, and the keyframes of the upper
    // rack, at 100 and 120, 179. The pointer goes further than the first of those.
    put_the_keyframe_of_the_rack_at(&mut window, turn, 250);
    select_racks(&mut window, &[upper, turn]);

    let from = on_the_rack(&window, 0, 10);
    drag_sideways_by(&mut window, from, 120.0);

    assert_eq!(
        frames_of(&window, turn),
        vec![299],
        "the one that has the least room is at the end"
    );
    assert_eq!(
        frames_of(&window, upper),
        vec![149, 169],
        "and the other went as far as that and no further, so that they are where they were in relation to one another"
    );
    assert_eq!(
        frames_of(&window, lower),
        vec![200],
        "the one that was not selected stayed"
    );
}

#[test]
fn racks_dragged_together_are_not_taken_to_another_place_by_the_pointer_going_up_or_down() {
    let Racks {
        mut window,
        video,
        upper,
        lower,
        turn,
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper, turn]);
    let from = on_the_rack(&window, 0, 160);

    // Up, over the text track in front and the strip of the video, and then down, below the racks.
    window.drag(from, from + vec2(0.0, -150.0), PointerButton::Primary);
    window.drag(from, from + vec2(0.0, 120.0), PointerButton::Primary);

    assert_eq!(
        racks_of_the_drawer_of(&window, video),
        vec![upper, lower, turn],
        "the same racks in the same order in the same drawer"
    );
    assert_eq!(selected_racks(&window), vec![upper, turn]);
}

#[test]
fn dragging_racks_together_is_one_step_of_the_history() {
    let Racks {
        mut window,
        upper,
        turn,
        ..
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper, turn]);
    let (upper_before, turn_before) = (frames_of(&window, upper), frames_of(&window, turn));

    let from = on_the_rack(&window, 0, 10);
    drag_sideways_by(&mut window, from, 30.0);
    assert_ne!(frames_of(&window, upper), upper_before);
    window.do_this(Command::Undo);

    assert_eq!(frames_of(&window, upper), upper_before);
    assert_eq!(frames_of(&window, turn), turn_before);
}

// --- racks and clips that are selected together ---

#[test]
fn a_clip_and_a_rack_that_are_selected_together_are_dragged_sideways_together() {
    let Racks { mut window, lower, .. } = window_with_racks_of_the_video();
    // The text object in front of the video, which lasts from frame 40 to 100, and a rack in the drawer of the video.
    let text = window.application().project().tracks()[0].entries()[0].id();
    select(&mut window, vec![SelectionItem::Clip(text), SelectionItem::Rack(lower)]);
    let (lower_before, text_before) = (frames_of(&window, lower), start_of(&window, text));

    // The text object is taken hold of in the middle of it.
    let from = window.middle_of_the_strip_at(0, 70);
    drag_sideways_by(&mut window, from, 25.0);

    let text_went = start_of(&window, text) as i64 - text_before as i64;
    assert!(
        (22..=25).contains(&text_went),
        "about as far as the pointer went: {text_went}"
    );
    assert_eq!(
        how_far_they_went(&window, lower, &lower_before),
        text_went,
        "the clip and the rack went together"
    );
}

#[test]
fn a_rack_selected_with_a_clip_can_be_what_is_taken_hold_of_and_the_clip_goes_with_it() {
    let Racks { mut window, lower, .. } = window_with_racks_of_the_video();
    let text = window.application().project().tracks()[0].entries()[0].id();
    select(&mut window, vec![SelectionItem::Clip(text), SelectionItem::Rack(lower)]);
    let (lower_before, text_before) = (frames_of(&window, lower), start_of(&window, text));

    let from = on_the_rack(&window, 1, 10);
    drag_sideways_by(&mut window, from, 25.0);

    let lower_went = how_far_they_went(&window, lower, &lower_before);
    assert!((22..=25).contains(&lower_went), "{lower_went}");
    assert_eq!(start_of(&window, text) as i64 - text_before as i64, lower_went);
}

#[test]
fn a_clip_that_stops_against_the_start_of_the_timeline_stops_the_rack_that_goes_with_it() {
    let Racks { mut window, lower, .. } = window_with_racks_of_the_video();
    let text = window.application().project().tracks()[0].entries()[0].id();
    select(&mut window, vec![SelectionItem::Clip(text), SelectionItem::Rack(lower)]);
    let (lower_before, text_before) = (frames_of(&window, lower), start_of(&window, text));
    assert_eq!(text_before, 40);

    // Back further than the text can go: it begins at 40.
    let from = window.middle_of_the_strip_at(0, 80);
    drag_sideways_by(&mut window, from, -60.0);

    assert_eq!(start_of(&window, text), 0, "the text is at the start of the timeline");
    assert_eq!(
        how_far_they_went(&window, lower, &lower_before),
        -40,
        "and the rack went as far as it did and no further"
    );
}

#[test]
fn a_clip_that_stops_against_the_start_of_the_timeline_stops_the_rack_that_is_taken_hold_of_just_the_same() {
    let Racks { mut window, lower, .. } = window_with_racks_of_the_video();
    let text = window.application().project().tracks()[0].entries()[0].id();
    select(&mut window, vec![SelectionItem::Clip(text), SelectionItem::Rack(lower)]);
    let (lower_before, text_before) = (frames_of(&window, lower), start_of(&window, text));
    assert_eq!(text_before, 40);
    assert_eq!(
        lower_before,
        vec![200],
        "the rack has room to go back more than the text has"
    );

    // Taken hold of by the rack, and back further than the text can go.
    let from = on_the_rack(&window, 1, 90);
    drag_sideways_by(&mut window, from, -60.0);

    assert_eq!(start_of(&window, text), 0, "the text is at the start of the timeline");
    assert_eq!(
        how_far_they_went(&window, lower, &lower_before),
        -40,
        "and the rack went as far as the text did and no further, though it was the rack that was taken hold of"
    );
}

#[test]
fn a_rack_that_stops_against_the_end_of_its_drawer_stops_the_clip_that_goes_with_it() {
    let Racks { mut window, lower, .. } = window_with_racks_of_the_video();
    let text = window.application().project().tracks()[0].entries()[0].id();
    put_the_keyframe_of_the_rack_at(&mut window, lower, 250);
    select(&mut window, vec![SelectionItem::Clip(text), SelectionItem::Rack(lower)]);

    // The keyframe of the rack is at 250 and the drawer has 300 frames, so it can go on 49: the text has more room than that.
    let from = window.middle_of_the_strip_at(0, 50);
    drag_sideways_by(&mut window, from, 100.0);

    assert_eq!(frames_of(&window, lower), vec![299]);
    assert_eq!(
        start_of(&window, text),
        40 + 49,
        "the text is as far as the rack, not as far as the pointer"
    );
}

#[test]
fn a_rack_in_the_drawer_of_a_clip_that_is_dragged_goes_with_the_clip_and_is_not_moved_in_the_drawer_as_well() {
    let Racks { mut window, lower, .. } = window_with_racks_of_the_video();
    // The text object has a rack of its own, in its own drawer; it is selected with the text and with a rack of the video.
    let text = window.application().project().tracks()[0].entries()[0].id();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 60);
    let rack_of_the_text = window
        .application()
        .project()
        .locate_keyframe(keyframe)
        .unwrap()
        .rack
        .rack
        .id();
    select(
        &mut window,
        vec![
            SelectionItem::Clip(text),
            SelectionItem::Rack(rack_of_the_text),
            SelectionItem::Rack(lower),
        ],
    );
    let in_the_text_before = how_far_into_the_clip(&window, rack_of_the_text);
    let (lower_before, text_before) = (frames_of(&window, lower), start_of(&window, text));

    let from = window.middle_of_the_strip_at(0, 80);
    drag_sideways_by(&mut window, from, 25.0);

    let text_went = start_of(&window, text) as i64 - text_before as i64;
    assert!(text_went > 20, "{text_went}");
    assert_eq!(
        how_far_into_the_clip(&window, rack_of_the_text),
        in_the_text_before,
        "the rack in the drawer of the text keeps its place in it: it went with the text, and was not moved in it as well"
    );
    assert_eq!(
        how_far_they_went(&window, lower, &lower_before),
        text_went,
        "the other rack went as far as the text"
    );
}

// --- what is taken hold of, and what is not selected ---

#[test]
fn a_rack_that_is_alone_is_dragged_as_before_and_goes_up_and_down_between_the_racks() {
    let Racks {
        mut window,
        video,
        upper,
        lower,
        turn,
    } = window_with_racks_of_the_video();
    let from = on_the_rack(&window, 0, 160);
    let third = racks_drawn(&window)[2].center().y;

    // Past the middle of the last rack: it is the last.
    window.drag(from, Pos2::new(from.x, third + 3.0), PointerButton::Primary);

    assert_eq!(
        racks_of_the_drawer_of(&window, video),
        vec![lower, turn, upper],
        "the first rack was dragged to the bottom, as one rack can be"
    );
}

#[test]
fn taking_hold_of_a_rack_that_is_not_selected_selects_only_it_and_drags_only_it() {
    let Racks {
        mut window,
        upper,
        lower,
        turn,
        ..
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper, turn]);
    let (upper_before, turn_before, lower_before) = (
        frames_of(&window, upper),
        frames_of(&window, turn),
        frames_of(&window, lower),
    );

    let from = on_the_rack(&window, 1, 10);
    drag_sideways_by(&mut window, from, 20.0);

    assert_eq!(
        selected_racks(&window),
        vec![lower],
        "only the rack that was taken hold of is selected"
    );
    assert_eq!(frames_of(&window, upper), upper_before);
    assert_eq!(frames_of(&window, turn), turn_before);
    let went = how_far_they_went(&window, lower, &lower_before);
    assert!(went > 15, "it went {went} frames");
}

#[test]
fn taking_hold_of_a_rack_that_is_not_selected_with_control_held_adds_it_and_drags_all_that_are_selected() {
    let Racks {
        mut window,
        upper,
        lower,
        turn,
        ..
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper]);
    let (upper_before, lower_before, turn_before) = (
        frames_of(&window, upper),
        frames_of(&window, lower),
        frames_of(&window, turn),
    );

    let from = on_the_rack(&window, 2, 10);
    window.hold_control();
    drag_sideways_by(&mut window, from, 20.0);
    window.let_go_of_control();

    assert_eq!(selected_racks(&window), vec![upper, turn]);
    let (upper_went, turn_went) = (
        how_far_they_went(&window, upper, &upper_before),
        how_far_they_went(&window, turn, &turn_before),
    );
    assert!(
        upper_went > 15 && upper_went == turn_went,
        "the two went together: {upper_went}, {turn_went}"
    );
    assert_eq!(frames_of(&window, lower), lower_before);
}

#[test]
fn a_selected_keyframe_and_a_selected_rack_are_dragged_together_by_either() {
    let Racks {
        mut window,
        video,
        lower,
        turn,
        ..
    } = window_with_racks_of_the_video();
    let keyframe_of_the_turn = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Rotation, 150);
    select(
        &mut window,
        vec![
            SelectionItem::Rack(lower),
            SelectionItem::Keyframe(keyframe_of_the_turn),
        ],
    );
    let (lower_before, turn_before) = (frames_of(&window, lower), frames_of(&window, turn));

    // By the keyframe.
    let from = where_it_is(&window, keyframe_of_the_turn);
    drag_sideways_by(&mut window, from, 20.0);

    let turn_went = how_far_they_went(&window, turn, &turn_before);
    assert!(turn_went > 15, "{turn_went}");
    assert_eq!(how_far_they_went(&window, lower, &lower_before), turn_went);
    assert_eq!(selected_racks(&window), vec![lower]);
    assert_eq!(selected_keyframes(&window), vec![keyframe_of_the_turn]);

    // And by the rack, back the other way.
    let (lower_before, turn_before) = (frames_of(&window, lower), frames_of(&window, turn));
    let from = on_the_rack(&window, 1, 10);
    drag_sideways_by(&mut window, from, -10.0);
    let lower_went = how_far_they_went(&window, lower, &lower_before);
    assert!(lower_went < -7, "{lower_went}");
    assert_eq!(how_far_they_went(&window, turn, &turn_before), lower_went);
}

#[test]
fn a_track_that_is_selected_with_a_rack_does_not_stop_the_rack_from_being_dragged_to_other_places_on_its_own() {
    let Racks {
        mut window,
        video,
        upper,
        lower,
        turn,
    } = window_with_racks_of_the_video();
    let track = window.application().project().tracks()[0].id();
    select(
        &mut window,
        vec![SelectionItem::Rack(lower), SelectionItem::Track(track)],
    );
    let from = on_the_rack(&window, 1, 10);
    let last = racks_drawn(&window)[2].center().y;

    // Past the middle of the last rack, as a rack that is alone is taken to be the last.
    window.drag(from, Pos2::new(from.x, last + 3.0), PointerButton::Primary);

    assert_eq!(
        racks_of_the_drawer_of(&window, video),
        vec![upper, turn, lower],
        "nothing of the track is dragged, so the rack is dragged on its own"
    );
}

#[test]
fn racks_in_the_drawer_of_a_clip_that_begins_late_go_along_with_the_pointer_as_far_as_it_goes() {
    let Racks { mut window, .. } = window_with_racks_of_the_video();
    // The text object begins at frame 40, so what is in its drawer is counted from there.
    let text = window.application().project().tracks()[0].entries()[0].id();
    let target = AnimationTarget::Clip(text);
    let first = make_a_keyframe(&mut window, target, PropertyId::Rotation, 60);
    let second = make_a_keyframe(&mut window, target, PropertyId::Size, 70);
    let (first_rack, second_rack) = {
        let project = window.application().project();
        (
            project.locate_keyframe(first).unwrap().rack.rack.id(),
            project.locate_keyframe(second).unwrap().rack.rack.id(),
        )
    };
    select_racks(&mut window, &[first_rack, second_rack]);
    let (first_before, second_before) = (frames_of(&window, first_rack), frames_of(&window, second_rack));
    assert_eq!((first_before.clone(), second_before.clone()), (vec![60], vec![70]));

    let from = Pos2::new(
        window.x_of_frame_on_the_strip(0, 90),
        racks_drawn(&window)[0].center().y,
    );
    drag_sideways_by(&mut window, from, 8.0);

    let went = (
        how_far_they_went(&window, first_rack, &first_before),
        how_far_they_went(&window, second_rack, &second_before),
    );
    assert!((7..=8).contains(&went.0), "as far as the pointer went: {went:?}");
    assert_eq!(went.0, went.1, "and both together");
}

// --- what a rack that is dragged with other things keeps them from doing ---

#[test]
fn a_keyframe_selected_with_a_rack_is_not_taken_to_another_rack_because_the_rack_stays_whole() {
    let Racks {
        mut window,
        video,
        upper,
        lower,
        turn,
    } = window_with_racks_of_the_video();
    let keyframe = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 120);
    select(
        &mut window,
        vec![SelectionItem::Keyframe(keyframe), SelectionItem::Rack(turn)],
    );
    let from = where_it_is(&window, keyframe);
    let turn_before = frames_of(&window, turn);

    // Down onto the rack under the one it is in, where it would go if it were alone (and there is a test that says it does), and a
    // little to the right, to see that they all go along.
    let to = Pos2::new(
        from.x + 10.0 * points_per_frame(&window),
        racks_drawn(&window)[1].center().y,
    );
    window.drag(from, to, PointerButton::Primary);

    let rack_it_is_in = window
        .application()
        .project()
        .locate_keyframe(keyframe)
        .unwrap()
        .rack
        .rack
        .id();
    assert_eq!(
        rack_it_is_in, upper,
        "it is in the rack it was in: a rack that is dragged goes whole, in its own place"
    );
    assert_eq!(
        frames_of(&window, lower),
        vec![200],
        "and the racks that are not selected are as they were"
    );
    let turn_went = how_far_they_went(&window, turn, &turn_before);
    assert!(turn_went > 7, "they were dragged along the timeline: {turn_went}");
    assert_eq!(
        frame_of(&window, keyframe) as i64 - 120,
        turn_went,
        "as far as one another"
    );
}

/// The window of [`window_with_racks_of_the_video`], but with a third rack of the loudness of the video, and none of how far it
/// is turned: three racks of one property one under another (the keyframes of the first at 100 and 120, of the second at 200 and
/// of the third at 60), so that the one in the middle has a rack to go to in either direction.
struct ThreeRacksOfTheLoudness {
    window: Window,
    video: ClipId,
    first: RackId,
    second: RackId,
    third: RackId,
}

fn window_with_three_racks_of_the_loudness_of_the_video() -> ThreeRacksOfTheLoudness {
    let Racks {
        mut window,
        video,
        upper: first,
        lower: second,
        turn,
    } = window_with_racks_of_the_video();
    let target = AnimationTarget::Clip(video);
    window.do_this(Command::DeleteRacks(vec![turn]));
    // The two racks wait in the drawer of the track in front while the third is made, so that it is a rack of its own.
    let in_front = AnimationTarget::Track(window.application().project().tracks()[0].id());
    for rack in [first, second] {
        window.do_this(Command::MoveRack {
            rack,
            to: RackDestination::Drawer(in_front),
            shift_in_frames: 0,
            slot: None,
        });
    }
    make_a_keyframe(&mut window, target, PropertyId::Gain, 60);
    let third = the_rack_of(&window, video, PropertyId::Gain);
    for (slot, rack) in [(0, first), (1, second)] {
        window.do_this(Command::MoveRack {
            rack,
            to: RackDestination::Drawer(target),
            shift_in_frames: 0,
            slot: Some(slot),
        });
    }
    window.harness.run_steps(40);
    assert_eq!(
        racks_of_the_drawer_of(&window, video),
        vec![first, second, third],
        "the setup: one under another"
    );
    ThreeRacksOfTheLoudness {
        window,
        video,
        first,
        second,
        third,
    }
}

#[test]
fn a_rack_that_is_dragged_with_a_keyframe_is_not_taken_down_to_the_rack_under_it_when_the_keyframe_is_taken_down() {
    let ThreeRacksOfTheLoudness {
        mut window,
        video,
        first,
        second,
        third,
    } = window_with_three_racks_of_the_loudness_of_the_video();
    let keyframe = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 120);
    select(
        &mut window,
        vec![SelectionItem::Keyframe(keyframe), SelectionItem::Rack(second)],
    );
    let (second_before, third_before) = (frames_of(&window, second), frames_of(&window, third));
    let from = where_it_is(&window, keyframe);

    // Down onto the second rack, where the keyframe would go if it were alone, and a little to the right. The second rack is
    // dragged whole, in its own place, so nothing is taken down: and it has a rack under it that it could have been taken to.
    let to = Pos2::new(
        from.x + 10.0 * points_per_frame(&window),
        racks_drawn(&window)[1].center().y,
    );
    window.drag(from, to, PointerButton::Primary);

    let rack_it_is_in = window
        .application()
        .project()
        .locate_keyframe(keyframe)
        .unwrap()
        .rack
        .rack
        .id();
    assert_eq!(rack_it_is_in, first, "the keyframe is in the rack it was in");
    assert_eq!(
        frames_of(&window, third),
        third_before,
        "the rack under the one that is selected got nothing from it"
    );
    let second_went = how_far_they_went(&window, second, &second_before);
    assert!(
        second_went > 7,
        "the rack was dragged along the timeline: {second_went}"
    );
    assert_eq!(
        frame_of(&window, keyframe) as i64 - 120,
        second_went,
        "as far as the keyframe"
    );
    assert_eq!(
        racks_of_the_drawer_of(&window, video),
        vec![first, second, third],
        "the racks are in the places they were in"
    );
}
