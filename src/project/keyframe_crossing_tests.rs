// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of keyframes that go from one rack to another: which racks a keyframe can go to, what it keeps (its name, its value, its
//! place in time), what happens to the racks and crates it leaves, and how far keyframes that are dragged to other racks can go.

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, db, frames_in_the_rack, gain_at,
    names_of_the_keyframes_in, names_of_the_racks_in, put_a_rack_in, where_the_video_has_keyframes,
};
use super::tests::frame;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// A video of 600 frames and a text object from frame 100 to 160, and two racks of the loudness of the video: one in the drawer of
/// the text with keyframes at 10 and 40 of it (110 and 140 of the timeline) and one in the drawer of the video with keyframes at
/// 50 and 300.
struct TwoRacksOfTheLoudness {
    project: Project,
    video: ClipId,
    text: ClipId,
    in_the_text: RackId,
    in_the_video: RackId,
}

fn two_racks_of_the_loudness() -> TwoRacksOfTheLoudness {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let in_the_text = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let in_the_video = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(50, db(-6.0)), (300, db(-3.0))],
    );
    TwoRacksOfTheLoudness {
        project,
        video,
        text,
        in_the_text,
        in_the_video,
    }
}

fn move_to(keyframe: KeyframeId, to_rack: RackId, to_frame: u64) -> KeyframeMove {
    KeyframeMove {
        keyframe,
        to_rack,
        to_frame: i64::try_from(to_frame).unwrap(),
    }
}

fn going_to(keyframe: KeyframeId, rack: RackId) -> KeyframeDestination {
    KeyframeDestination { keyframe, rack }
}

fn the_loudness_of_the_video_at_every_frame(project: &Project, video: ClipId) -> Vec<f32> {
    (0..620)
        .map(|at| gain_at(project, AnimationTarget::Clip(video), at))
        .collect()
}

// --- going to another rack ---

#[test]
fn a_keyframe_moved_to_another_rack_of_the_same_property_keeps_its_name_and_its_value_and_its_place_in_time() {
    let TwoRacksOfTheLoudness {
        mut project,
        video,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let travelling = names_of_the_keyframes_in(&project, in_the_text)[0];
    let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);

    // It is at frame 110 of the timeline, which is frame 110 of the drawer of the video.
    project
        .move_keyframes(&[move_to(travelling, in_the_video, 110)])
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![40]);
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![50, 110, 300]);
    let located = project.locate_keyframe(travelling).unwrap();
    assert_eq!(
        located.rack.rack.id(),
        in_the_video,
        "it is in the other rack, and is found by the name it was made with"
    );
    assert_eq!(located.keyframe.value, db(-20.0));
    assert_eq!(located.timeline_frame(), 110);
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![50, 110, 140, 300]
    );
    assert_eq!(
        the_loudness_of_the_video_at_every_frame(&project, video),
        curve_before,
        "the video is as loud as it was at every frame"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_moved_to_a_rack_in_a_drawer_that_starts_later_is_counted_from_the_start_of_that_drawer() {
    let TwoRacksOfTheLoudness {
        mut project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let travelling = names_of_the_keyframes_in(&project, in_the_video)[0];

    // The keyframe at 50 of the timeline goes to 120 of it: 20 frames into the text, which starts at 100.
    project.move_keyframes(&[move_to(travelling, in_the_text, 20)]).unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 20, 40]);
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![300]);
    assert_eq!(project.timeline_frame_of_keyframe(travelling), Some(120));
}

#[test]
fn moving_a_keyframe_to_the_rack_it_is_in_is_moving_it_in_time_and_two_of_them_can_change_places() {
    let TwoRacksOfTheLoudness {
        mut project,
        in_the_text,
        ..
    } = two_racks_of_the_loudness();
    let names = names_of_the_keyframes_in(&project, in_the_text);

    project
        .move_keyframes(&[move_to(names[0], in_the_text, 40), move_to(names[1], in_the_text, 10)])
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 40]);
    assert_eq!(project.timeline_frame_of_keyframe(names[0]), Some(140));
    assert_eq!(project.timeline_frame_of_keyframe(names[1]), Some(110));
    assert_eq!(
        project.locate_keyframe(names[0]).unwrap().keyframe.value,
        db(-20.0),
        "each keeps its own value: they changed places"
    );
}

#[test]
fn two_keyframes_that_change_racks_with_each_other_can_take_each_others_frames() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let in_the_text = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let in_the_video = put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(110, db(-6.0))]);
    let from_the_text = names_of_the_keyframes_in(&project, in_the_text)[0];
    let from_the_video = names_of_the_keyframes_in(&project, in_the_video)[0];

    assert_eq!(
        project.move_keyframes(&[move_to(from_the_text, in_the_video, 110)]),
        Err(EditError::Rack(RackEditError::FramesWouldCoincide)),
        "one of them alone would be where the other still is"
    );
    project
        .move_keyframes(&[
            move_to(from_the_text, in_the_video, 110),
            move_to(from_the_video, in_the_text, 10),
        ])
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10]);
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![110]);
    let now_in_the_video = project.locate_keyframe(from_the_text).unwrap();
    assert_eq!(now_in_the_video.rack.rack.id(), in_the_video);
    assert_eq!(now_in_the_video.keyframe.value, db(-20.0), "the values went with them");
    assert_eq!(
        project.locate_keyframe(from_the_video).unwrap().rack.rack.id(),
        in_the_text
    );
    assert_the_racks_are_sound(&project);
}

// --- where a keyframe cannot go ---

#[test]
fn a_keyframe_cannot_go_to_a_rack_of_another_property_or_of_the_same_property_of_another_thing() {
    let TwoRacksOfTheLoudness {
        mut project,
        video,
        text,
        in_the_text,
        ..
    } = two_racks_of_the_loudness();
    let big = AnimationTarget::Clip(video);
    let small = AnimationTarget::Clip(text);
    let switch = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    // The turn of the video and the turn of the text are the same property of two things.
    let turn_of_the_video = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Rotation,
        &[(5, PropertyValue::Number(10.0))],
    );
    let turn_of_the_text = put_a_rack_in(
        &mut project,
        small,
        small,
        PropertyId::Rotation,
        &[(7, PropertyValue::Number(20.0))],
    );
    let loud = names_of_the_keyframes_in(&project, in_the_text)[0];
    let turning = names_of_the_keyframes_in(&project, turn_of_the_video)[0];
    let before = project.clone();

    for (keyframe, rack, frame) in [(loud, switch, 11), (turning, turn_of_the_text, 20)] {
        let moves = [move_to(keyframe, rack, frame)];
        let refused = Err(EditError::KeyframeCannotGoToThatRack { keyframe, rack });
        assert_eq!(project.check_the_move_of_keyframes(&moves), refused);
        assert_eq!(project.move_keyframes(&moves), refused);
        assert_eq!(project, before);
    }
}

#[test]
fn a_keyframe_cannot_go_to_a_frame_its_new_drawer_does_not_have_or_one_that_has_a_keyframe_or_a_rack_that_is_not_there()
{
    let TwoRacksOfTheLoudness {
        mut project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let travelling = names_of_the_keyframes_in(&project, in_the_video)[0];
    let before = project.clone();

    // The text lasts 60 frames, from 100: the frames of its drawer are 0 to 59.
    assert_eq!(
        project.move_keyframes(&[move_to(travelling, in_the_text, 60)]),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(
        project.move_keyframes(&[move_to(travelling, in_the_text, 40)]),
        Err(EditError::Rack(RackEditError::FramesWouldCoincide)),
        "the rack has a keyframe at 40"
    );
    let no_rack = RackId::new(TrackId::new(50), 0);
    assert_eq!(
        project.move_keyframes(&[move_to(travelling, no_rack, 1)]),
        Err(EditError::UnknownRack { rack: no_rack })
    );
    let nobody = KeyframeId::new(in_the_video, 77);
    assert_eq!(
        project.move_keyframes(&[move_to(nobody, in_the_text, 1)]),
        Err(EditError::Rack(RackEditError::NoSuchKeyframe))
    );
    assert_eq!(
        project.move_keyframes(&[move_to(travelling, in_the_text, 1), move_to(travelling, in_the_text, 2)]),
        Err(EditError::Rack(RackEditError::NoSuchKeyframe)),
        "one keyframe is not moved to two places"
    );
    assert_eq!(project, before);
}

#[test]
fn keyframes_that_go_to_other_racks_all_go_or_none_does() {
    let TwoRacksOfTheLoudness {
        mut project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let from_the_text = names_of_the_keyframes_in(&project, in_the_text)[0];
    let from_the_video = names_of_the_keyframes_in(&project, in_the_video)[0];
    let before = project.clone();

    let one_too_late = project.move_keyframes(&[
        move_to(from_the_text, in_the_video, 120),
        move_to(from_the_video, in_the_text, 90),
    ]);
    assert_eq!(one_too_late, Err(EditError::NoRoomForTheKeyframes));
    assert_eq!(project, before, "the first was not moved either");

    project
        .move_keyframes(&[
            move_to(from_the_text, in_the_video, 120),
            move_to(from_the_video, in_the_text, 5),
        ])
        .unwrap();
    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![5, 40]);
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![120, 300]);
}

// --- what is left behind ---

#[test]
fn a_rack_that_loses_its_last_keyframe_to_another_rack_goes_and_so_does_a_crate_that_is_left_with_no_rack() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 100).unwrap();
    let in_the_crate = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let in_the_video = put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(50, db(-6.0))]);
    let travelling = names_of_the_keyframes_in(&project, in_the_crate)[0];

    project
        .move_keyframes(&[move_to(travelling, in_the_video, 310)])
        .unwrap();

    assert!(
        project.located_rack(in_the_crate).is_none(),
        "the rack has no keyframe left"
    );
    assert!(
        project.placement_of(crate_id).is_none(),
        "and the crate has no rack left"
    );
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![50, 310]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_that_has_another_rack_stays_when_one_of_its_racks_loses_all_its_keyframes() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 100).unwrap();
    let inside = AnimationTarget::Clip(crate_id);
    let loudness = put_a_rack_in(&mut project, inside, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let switch = put_a_rack_in(
        &mut project,
        inside,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    let in_the_video = put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(50, db(-6.0))]);
    let travelling = names_of_the_keyframes_in(&project, loudness)[0];

    project
        .move_keyframes(&[move_to(travelling, in_the_video, 310)])
        .unwrap();

    assert_eq!(names_of_the_racks_in(&project, inside), vec![switch]);
    assert!(project.placement_of(crate_id).is_some());
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_rack_that_is_emptied_by_keyframes_that_leave_and_filled_by_others_that_arrive_stays() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let in_the_text = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let in_the_video = put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(300, db(-6.0))]);
    let from_the_text = names_of_the_keyframes_in(&project, in_the_text)[0];
    let from_the_video = names_of_the_keyframes_in(&project, in_the_video)[0];

    project
        .move_keyframes(&[
            move_to(from_the_text, in_the_video, 110),
            move_to(from_the_video, in_the_text, 20),
        ])
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![20]);
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![110]);
    assert_the_racks_are_sound(&project);
}

// --- how far dragged keyframes can go ---

#[test]
fn keyframes_dragged_to_another_rack_stop_at_the_ends_of_its_drawer_and_at_its_keyframes_that_are_not_moving() {
    let TwoRacksOfTheLoudness {
        project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let first_in_the_text = names_of_the_keyframes_in(&project, in_the_text)[0];

    // To the rack in the video, which has keyframes at 50 and 300: from the frame after 50 to the frame before 300.
    let to_the_video = [going_to(first_in_the_text, in_the_video)];
    assert_eq!(project.allowed_shift_of_keyframes(&to_the_video, 1_000), 189);
    assert_eq!(project.allowed_shift_of_keyframes(&to_the_video, -1_000), -59);
    assert_eq!(project.allowed_shift_of_keyframes(&to_the_video, 12), 12);
    // Staying where it is, in the drawer of the text, which has the frames 100 to 159: the keyframe at 140 is in the way.
    let staying = [going_to(first_in_the_text, in_the_text)];
    assert_eq!(project.allowed_shift_of_keyframes(&staying, 1_000), 29);
    assert_eq!(project.allowed_shift_of_keyframes(&staying, -1_000), -10);
}

#[test]
fn keyframes_that_are_dragged_to_other_racks_together_are_stopped_by_whichever_is_stopped_first_and_not_by_each_other()
{
    let TwoRacksOfTheLoudness {
        project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let names = names_of_the_keyframes_in(&project, in_the_text);

    // The keyframes at 110 and 140 both go to the rack in the video, which has keyframes at 50 and 300.
    let both = [going_to(names[0], in_the_video), going_to(names[1], in_the_video)];
    assert_eq!(
        project.allowed_shift_of_keyframes(&both, 1_000),
        159,
        "the one at 140 is stopped before 300"
    );
    assert_eq!(
        project.allowed_shift_of_keyframes(&both, -1_000),
        -59,
        "the one at 110 is stopped after 50"
    );
    // One goes and one stays: the one that stays in the drawer of the text is in the way of nothing that is moving, and what
    // moves is not stopped by the one that is moving out of its way.
    let one_goes = [going_to(names[0], in_the_video), going_to(names[1], in_the_text)];
    assert_eq!(project.allowed_shift_of_keyframes(&one_goes, 1_000), 19);
    assert_eq!(project.allowed_shift_of_keyframes(&one_goes, -1_000), -40);
}

#[test]
fn a_keyframe_that_is_at_the_frame_of_the_keyframe_that_is_dragged_is_not_in_its_way_either_way() {
    let TwoRacksOfTheLoudness {
        mut project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let first_in_the_text = names_of_the_keyframes_in(&project, in_the_text)[0]; // at 110
    // The rack in the video gets a keyframe at 110 too. (The keyframe cannot be put there, but it is not stopped by it from
    // going to the frames on either side: those are for whoever asks to have checked.)
    let in_the_way_of_nobody = names_of_the_keyframes_in(&project, in_the_video)[0];
    project
        .move_keyframes(&[move_to(in_the_way_of_nobody, in_the_video, 110)])
        .unwrap();

    let to_the_video = [going_to(first_in_the_text, in_the_video)];
    assert_eq!(project.allowed_shift_of_keyframes(&to_the_video, 12), 12);
    assert_eq!(project.allowed_shift_of_keyframes(&to_the_video, -12), -12);
    assert_eq!(
        project.allowed_shift_of_keyframes(&to_the_video, 1_000),
        189,
        "the keyframe at 300 is still in the way after it"
    );
    assert_eq!(
        project.check_the_move_of_keyframes(&project.moves_of_keyframes_by(&to_the_video, 0)),
        Err(EditError::Rack(RackEditError::FramesWouldCoincide)),
        "though it cannot stay there"
    );
}

#[test]
fn the_moves_that_put_keyframes_in_other_racks_count_their_frames_from_the_drawers_they_go_to() {
    let TwoRacksOfTheLoudness {
        project,
        in_the_text,
        in_the_video,
        ..
    } = two_racks_of_the_loudness();
    let first_in_the_text = names_of_the_keyframes_in(&project, in_the_text)[0];
    let first_in_the_video = names_of_the_keyframes_in(&project, in_the_video)[0];

    assert_eq!(
        project.moves_of_keyframes_by(&[going_to(first_in_the_text, in_the_video)], 25),
        vec![move_to(first_in_the_text, in_the_video, 135)],
        "110 + 25, in a drawer that starts at 0"
    );
    assert_eq!(
        project.moves_of_keyframes_by(&[going_to(first_in_the_video, in_the_text)], 70),
        vec![move_to(first_in_the_video, in_the_text, 20)],
        "50 + 70 = 120 of the timeline, which is 20 frames into the text"
    );
    let before_the_text = project.moves_of_keyframes_by(&[going_to(first_in_the_video, in_the_text)], 10);
    assert_eq!(
        before_the_text,
        vec![KeyframeMove {
            keyframe: first_in_the_video,
            to_rack: in_the_text,
            to_frame: -40,
        }],
        "60 of the timeline is 40 frames before the text starts: it is said so, as a frame of the drawer that is before its first"
    );
    assert_eq!(
        project.check_the_move_of_keyframes(&before_the_text),
        Err(EditError::NoRoomForTheKeyframes),
        "and the drawer of a text has no room for it"
    );
    assert_eq!(
        project.moves_of_keyframes_by(&[going_to(KeyframeId::new(in_the_video, 77), in_the_text)], 0),
        vec![],
        "a keyframe that is not there is left out"
    );
}

// --- whatever is moved ---

/// A project with a video, two text objects and a crate on the front track, and racks of the loudness of the video in the drawers
/// of all of them and of the front track, each with keyframes at random frames its drawer has.
fn a_project_with_racks_of_the_loudness_everywhere(random: &mut DeterministicRandomNumbers) -> (Project, Vec<RackId>) {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let other_text = project
        .insert_text(front, frame(300), TextObject::default(), 100)
        .map(|(clip, _)| clip)
        .unwrap();
    let crate_id = project.insert_crate(front, frame(450), 80).unwrap();
    let owners = [
        big,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(other_text),
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Track(front),
    ];
    let mut racks = Vec::new();
    for owner in owners {
        for _ in 0..1 + random.next_below(2) {
            let extent = project.extent_of_the_drawer_of(owner).unwrap();
            let length = extent.room.end_frame.map_or(600, |end| end - extent.start);
            let mut frames: Vec<u64> = (0..1 + random.next_below(3))
                .map(|_| random.next_below(length))
                .collect();
            frames.sort_unstable();
            frames.dedup();
            let keyframes: Vec<(u64, PropertyValue)> = frames
                .iter()
                .map(|frame| (*frame, db(-(random.next_below(300) as f32) / 10.0)))
                .collect();
            racks.push(put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &keyframes));
        }
    }
    (project, racks)
}

#[test]
fn whatever_keyframes_are_moved_where_the_project_stays_sound_and_a_move_that_cannot_be_made_changes_nothing() {
    let mut moves_made = 0;
    for seed in 1..=150u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, _) = a_project_with_racks_of_the_loudness_everywhere(&mut random);
        assert_the_racks_are_sound(&project);

        for step in 0..30 {
            let all_keyframes: Vec<KeyframeId> = project
                .located_racks()
                .flat_map(|located| {
                    located
                        .rack
                        .keyframes()
                        .iter()
                        .map(RackKeyframe::id)
                        .collect::<Vec<_>>()
                })
                .collect();
            let mut moving: Vec<KeyframeId> = (0..1 + random.next_below(3))
                .map(|_| all_keyframes[random.next_below(all_keyframes.len() as u64) as usize])
                .collect();
            moving.sort();
            moving.dedup();
            // (Racks that have lost all their keyframes are gone: the others are the places to move them to.)
            let racks_there_are: Vec<LocatedRack<'_>> = project.located_racks().collect();
            let moves: Vec<KeyframeMove> = moving
                .iter()
                .map(|keyframe| {
                    let to = racks_there_are[random.next_below(racks_there_are.len() as u64) as usize];
                    // (Anywhere that its drawer has room for: a keyframe crate has room before where it begins too, which is what
                    // a frame that is less than 0 is, in the frames of the drawer.)
                    let first = i64::try_from(to.room.first_frame).unwrap() - i64::try_from(to.start).unwrap();
                    let end =
                        i64::try_from(to.room.end_frame.unwrap_or(600)).unwrap() - i64::try_from(to.start).unwrap();
                    KeyframeMove {
                        keyframe: *keyframe,
                        to_rack: to.rack.id(),
                        to_frame: first
                            + i64::try_from(random.next_below(u64::try_from(end - first).unwrap())).unwrap(),
                    }
                })
                .collect();
            let before = project.clone();
            let context = format!("seed {seed}, step {step}: {moves:?}");

            let asked = project.check_the_move_of_keyframes(&moves);
            let result = project.move_keyframes(&moves);

            assert_eq!(asked, result, "{context}");
            match result {
                Err(_) => assert_eq!(project, before, "{context}"),
                Ok(()) => {
                    moves_made += 1;
                    for moved in &moves {
                        let located = project.locate_keyframe(moved.keyframe).expect(&context);
                        assert_eq!(located.rack.rack.id(), moved.to_rack, "{context}");
                        // (Where it is is the frame of the timeline it was told to go to: a crate that was made to begin earlier is
                        // counted from an earlier frame afterwards.)
                        let wanted = before.located_rack(moved.to_rack).unwrap().start as i64 + moved.to_frame;
                        assert_eq!(located.timeline_frame() as i64, wanted, "{context}");
                        let value_before = before.locate_keyframe(moved.keyframe).unwrap().keyframe.value.clone();
                        assert_eq!(located.keyframe.value, value_before, "{context}");
                    }
                    let moving_set: Vec<KeyframeId> = moves.iter().map(|moved| moved.keyframe).collect();
                    for keyframe in all_keyframes.iter().filter(|keyframe| !moving_set.contains(keyframe)) {
                        // (A keyframe crate that was made longer, at its start, or shorter, counts its frames from another frame
                        // afterwards: it is where it was in the timeline, in the rack it was in.)
                        let (then, now) = (
                            before.locate_keyframe(*keyframe).unwrap(),
                            project.locate_keyframe(*keyframe).expect(&context),
                        );
                        assert_eq!(
                            (now.rack.rack.id(), now.timeline_frame(), &now.keyframe.value),
                            (then.rack.rack.id(), then.timeline_frame(), &then.keyframe.value),
                            "a keyframe that was not moved is where it was: {context}"
                        );
                    }
                    let count_after: usize = project.located_racks().map(|located| located.rack.len()).sum();
                    assert_eq!(
                        count_after,
                        all_keyframes.len(),
                        "no keyframe is lost or made: {context}"
                    );
                }
            }
            assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
        }
    }
    assert!(
        moves_made > 200,
        "the test moves keyframes: {moves_made} moves were made"
    );
}

#[test]
fn what_is_dragged_to_other_racks_can_always_be_moved_by_the_shift_that_is_allowed_and_not_by_one_frame_more() {
    let mut cases_tried = 0;
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (project, racks) = a_project_with_racks_of_the_loudness_everywhere(&mut random);
        let all_keyframes: Vec<KeyframeId> = project
            .located_racks()
            .flat_map(|located| {
                located
                    .rack
                    .keyframes()
                    .iter()
                    .map(RackKeyframe::id)
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut moving: Vec<KeyframeId> = (0..1 + random.next_below(4))
            .map(|_| all_keyframes[random.next_below(all_keyframes.len() as u64) as usize])
            .collect();
        moving.sort();
        moving.dedup();
        // Each goes to the rack it is in or, a third of the time, to another.
        let destinations: Vec<KeyframeDestination> = moving
            .iter()
            .map(|keyframe| {
                let own = project.locate_keyframe(*keyframe).unwrap().rack.rack.id();
                let rack = if random.next_below(3) == 0 {
                    racks[random.next_below(racks.len() as u64) as usize]
                } else {
                    own
                };
                going_to(*keyframe, rack)
            })
            .collect();
        // (What is dragged is somewhere that it fits, which is where the keyframes are when it is begun.)
        let at_the_start = project.moves_of_keyframes_by(&destinations, 0);
        if at_the_start.len() != destinations.len() || project.check_the_move_of_keyframes(&at_the_start).is_err() {
            continue;
        }
        cases_tried += 1;
        let wanted = random.next_below(401) as i64 - 200;

        let allowed = project.allowed_shift_of_keyframes(&destinations, wanted);

        let context = format!("seed {seed}: {destinations:?}, {wanted} wanted, {allowed} allowed");
        assert!(allowed.abs() <= wanted.abs() && allowed * wanted >= 0, "{context}");
        let moves = project.moves_of_keyframes_by(&destinations, allowed);
        assert_eq!(
            moves.len(),
            destinations.len(),
            "{context}: all of them have a frame to go to"
        );
        assert_eq!(project.check_the_move_of_keyframes(&moves), Ok(()), "{context}");
        if allowed != wanted {
            let one_more = project.moves_of_keyframes_by(&destinations, allowed + wanted.signum());
            assert!(
                one_more.len() < destinations.len() || project.check_the_move_of_keyframes(&one_more).is_err(),
                "{context}: one frame more could have been done"
            );
        }
    }
    assert!(
        cases_tried > 100,
        "only {cases_tried} cases had somewhere to be dragged from"
    );
}
