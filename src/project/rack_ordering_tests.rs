// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of where a rack goes among the racks of the drawer it is put in: moving a rack up and down in its drawer, and putting it
//! at a place among the racks of another.

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, frames_in_the_rack, names_of_the_racks_in, put_a_rack_in,
};
use super::tests::frame;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// A video of 600 frames and a text object from frame 100 to 160, whose drawer has three racks: `a` (the loudness of the video),
/// `b` (whether the video is muted) and `c` (how far the text is turned), in that order, each with a keyframe at its frame 5; and
/// the video's own drawer, which has two racks: `x` (the loudness of the video) and `y` (how far the video is turned).
struct FiveRacks {
    project: Project,
    video: AnimationTarget,
    text: AnimationTarget,
    a: RackId,
    b: RackId,
    c: RackId,
    x: RackId,
    y: RackId,
}

fn five_racks() -> FiveRacks {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let (video, text) = (AnimationTarget::Clip(video), AnimationTarget::Clip(text));
    let number = |value: f32| PropertyValue::Number(value);
    let a = put_a_rack_in(&mut project, text, video, PropertyId::Gain, &[(5, number(0.0))]);
    let b = put_a_rack_in(
        &mut project,
        text,
        video,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    let c = put_a_rack_in(&mut project, text, text, PropertyId::Rotation, &[(5, number(10.0))]);
    let x = put_a_rack_in(&mut project, video, video, PropertyId::Gain, &[(300, number(-3.0))]);
    let y = put_a_rack_in(&mut project, video, video, PropertyId::Rotation, &[(310, number(20.0))]);
    FiveRacks {
        project,
        video,
        text,
        a,
        b,
        c,
        x,
        y,
    }
}

// --- inside a drawer ---

#[test]
fn a_rack_moved_in_its_drawer_to_a_slot_has_that_many_racks_above_it_not_counting_itself() {
    let FiveRacks {
        project, text, a, b, c, ..
    } = five_racks();
    let order_after_moving = |rack: RackId, slot: Option<usize>| -> Vec<RackId> {
        let mut project = project.clone();
        project
            .move_racks(&[rack], RackDestination::Drawer(text), 0, slot)
            .unwrap();
        names_of_the_racks_in(&project, text)
    };

    assert_eq!(order_after_moving(c, Some(0)), vec![c, a, b], "to the top");
    assert_eq!(order_after_moving(c, Some(1)), vec![a, c, b]);
    assert_eq!(order_after_moving(c, Some(2)), vec![a, b, c], "where it is");
    assert_eq!(order_after_moving(a, Some(1)), vec![b, a, c]);
    assert_eq!(
        order_after_moving(a, Some(2)),
        vec![b, c, a],
        "to the bottom: the other two are above it"
    );
    assert_eq!(order_after_moving(b, Some(1)), vec![a, b, c], "where it is");
    assert_eq!(
        order_after_moving(a, Some(99)),
        vec![b, c, a],
        "not further down than the bottom"
    );
    assert_eq!(
        order_after_moving(c, None),
        vec![a, b, c],
        "without a slot it keeps its place"
    );
    assert_eq!(order_after_moving(a, None), vec![a, b, c]);
}

#[test]
fn moving_a_rack_to_another_slot_of_its_drawer_changes_nothing_but_the_order() {
    let FiveRacks {
        mut project, text, c, ..
    } = five_racks();
    let turn_of_the_text_at_every_frame = |project: &Project| -> Vec<Option<PropertyValue>> {
        (100..160)
            .map(|at| project.property_value_at(text, PropertyId::Rotation, frame(at)))
            .collect()
    };
    let before = turn_of_the_text_at_every_frame(&project);

    project
        .move_racks(&[c], RackDestination::Drawer(text), 0, Some(0))
        .unwrap();

    assert_eq!(
        frames_in_the_rack(&project, c),
        vec![5],
        "its keyframes are where they were"
    );
    assert_eq!(turn_of_the_text_at_every_frame(&project), before);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_rack_can_be_shifted_in_time_and_put_at_another_slot_of_its_drawer_in_one_move() {
    let FiveRacks {
        mut project,
        text,
        a,
        b,
        c,
        ..
    } = five_racks();

    project
        .move_racks(&[a], RackDestination::Drawer(text), 7, Some(2))
        .unwrap();

    assert_eq!(names_of_the_racks_in(&project, text), vec![b, c, a]);
    assert_eq!(frames_in_the_rack(&project, a), vec![12]);
}

// --- into another drawer ---

#[test]
fn a_rack_put_in_another_drawer_at_a_slot_has_that_many_racks_above_it_and_goes_to_the_bottom_without_one() {
    let FiveRacks {
        project,
        video,
        text,
        a,
        b,
        c,
        x,
        y,
        ..
    } = five_racks();
    let order_after_moving = |slot: Option<usize>| -> (Vec<RackId>, Vec<RackId>) {
        let mut project = project.clone();
        project
            .move_racks(&[a], RackDestination::Drawer(video), 0, slot)
            .unwrap();
        (
            names_of_the_racks_in(&project, text),
            names_of_the_racks_in(&project, video),
        )
    };

    assert_eq!(order_after_moving(Some(0)).1, vec![a, x, y], "at the top");
    assert_eq!(order_after_moving(Some(1)).1, vec![x, a, y]);
    assert_eq!(order_after_moving(Some(2)).1, vec![x, y, a], "at the bottom");
    assert_eq!(
        order_after_moving(Some(40)).1,
        vec![x, y, a],
        "not further down than the bottom"
    );
    assert_eq!(
        order_after_moving(None).1,
        vec![x, y, a],
        "without a slot it goes under the others"
    );
    assert_eq!(
        order_after_moving(Some(0)).0,
        vec![b, c],
        "it is not in the drawer it came from any more, and the others are as they were"
    );
}

#[test]
fn a_rack_put_at_a_slot_of_a_drawer_that_has_a_rack_of_the_same_property_is_not_merged_with_it() {
    let FiveRacks {
        mut project,
        video,
        a,
        x,
        ..
    } = five_racks();

    project
        .move_racks(&[a], RackDestination::Drawer(video), 0, Some(1))
        .unwrap();

    let loudness_racks: Vec<RackId> = project
        .drawer_of(video)
        .unwrap()
        .racks_for(video, PropertyId::Gain)
        .map(Rack::id)
        .collect();
    assert_eq!(
        loudness_racks,
        vec![x, a],
        "two racks of the loudness of the video, one under the other"
    );
    assert_eq!(
        frames_in_the_rack(&project, a),
        vec![105],
        "the keyframe is where it was in time, counted from the start of the video"
    );
    assert_eq!(
        project.keyframes_of(video, PropertyId::Gain).len(),
        2,
        "each has its keyframe: nothing was merged"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn the_slot_is_not_looked_at_when_the_rack_goes_to_a_new_crate() {
    let FiveRacks {
        mut project, text, a, ..
    } = five_racks();
    let front = project.tracks()[0].id();

    project
        .move_racks(
            &[a],
            RackDestination::NewCrate {
                track: front,
                from_frame: 300,
                until_frame: 350,
            },
            200,
            Some(5),
        )
        .unwrap();

    let crate_id = project.tracks()[0].entries()[1].id();
    assert_eq!(
        names_of_the_racks_in(&project, AnimationTarget::Clip(crate_id)),
        vec![a]
    );
    assert_eq!(names_of_the_racks_in(&project, text).len(), 2);
}

#[test]
fn a_rack_that_cannot_go_where_it_is_asked_to_does_not_change_places_either() {
    let FiveRacks {
        mut project, text, a, ..
    } = five_racks();
    let before = project.clone();

    // The rack has its keyframe at 105 of the timeline: 5000 frames later there is no frame of the text for it.
    assert_eq!(
        project.move_racks(&[a], RackDestination::Drawer(text), 5_000, Some(2)),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before);
}

// --- whatever slot ---

#[test]
fn whatever_slot_a_rack_is_put_at_it_is_there_and_the_other_racks_keep_their_order() {
    let mut moves_made = 0;
    for seed in 1..=200u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let FiveRacks {
            mut project,
            video,
            text,
            ..
        } = five_racks();
        let owners = [video, text, AnimationTarget::Track(project.tracks()[0].id())];
        for step in 0..20 {
            let racks: Vec<RackId> = project.located_racks().map(|located| located.rack.id()).collect();
            let rack = racks[random.next_below(racks.len() as u64) as usize];
            let owner = owners[random.next_below(owners.len() as u64) as usize];
            let slot = match random.next_below(4) {
                0 => None,
                _ => Some(random.next_below(6) as usize),
            };
            let before = project.clone();
            let from = before.located_rack(rack).unwrap().owner;
            let context = format!("seed {seed}, step {step}: {rack} to {owner:?} at {slot:?}");

            let result = project.move_racks(&[rack], RackDestination::Drawer(owner), 0, slot);

            match result {
                Err(_) => assert_eq!(project, before, "{context}"),
                Ok(()) => {
                    moves_made += 1;
                    let others_before: Vec<RackId> = names_of_the_racks_in(&before, owner)
                        .into_iter()
                        .filter(|other| *other != rack)
                        .collect();
                    let now = names_of_the_racks_in(&project, owner);
                    let place = now.iter().position(|named| *named == rack).expect(&context);
                    let expected_place = match slot {
                        Some(slot) => slot.min(others_before.len()),
                        None if from == owner => before.drawer_of(owner).unwrap().position_of(rack).unwrap(),
                        None => others_before.len(),
                    };
                    assert_eq!(place, expected_place, "{context}");
                    let others_now: Vec<RackId> = now.iter().copied().filter(|other| *other != rack).collect();
                    assert_eq!(others_now, others_before, "the others keep their order: {context}");
                    if from != owner {
                        let left_behind: Vec<RackId> = names_of_the_racks_in(&before, from)
                            .into_iter()
                            .filter(|other| *other != rack)
                            .collect();
                        assert_eq!(names_of_the_racks_in(&project, from), left_behind, "{context}");
                    }
                }
            }
            assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
        }
    }
    assert!(moves_made > 500, "the test moves racks: {moves_made} moves were made");
}
