// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of copying keyframes to the frames after them or before them.

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, db, frames_in_the_rack, names_of_the_keyframes_in,
    put_a_rack_in,
};
use super::tests::frame;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// A video of 600 frames and a text object from frame 100 to 160, and racks of the loudness of the video: `in_the_text` in the
/// drawer of the text with keyframes at its frames 10, 15 and 30 (values -10, -15 and -30 decibels), `in_the_video` in the drawer
/// of the video with keyframes at 50 and 300 (values -5 and -3), and `on_the_track` in the drawer of the front track, with a
/// keyframe at 100 (value -1).
struct Racks {
    project: Project,
    in_the_text: RackId,
    in_the_video: RackId,
    on_the_track: RackId,
}

fn racks() -> Racks {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = AnimationTarget::Track(project.tracks()[0].id());
    let in_the_text = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-10.0)), (15, db(-15.0)), (30, db(-30.0))],
    );
    let in_the_video = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(50, db(-5.0)), (300, db(-3.0))],
    );
    let on_the_track = put_a_rack_in(&mut project, front, big, PropertyId::Gain, &[(100, db(-1.0))]);
    Racks {
        project,
        in_the_text,
        in_the_video,
        on_the_track,
    }
}

fn values_in_the_rack(project: &Project, rack: RackId) -> Vec<PropertyValue> {
    project
        .located_rack(rack)
        .unwrap()
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| keyframe.value.clone())
        .collect()
}

// --- copying later and earlier ---

#[test]
fn a_keyframe_is_copied_to_a_frame_later_that_is_the_gap_after_it_with_a_name_and_a_value() {
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);

    let copies = project
        .duplicate_keyframes(&[names[0]], TimeDirection::Later, 3)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 13, 15, 30]);
    assert_eq!(copies.len(), 1);
    assert!(!names.contains(&copies[0]), "the copy is a keyframe of its own");
    let copy = project.locate_keyframe(copies[0]).unwrap();
    assert_eq!(copy.rack.rack.id(), in_the_text, "in the rack the keyframe is in");
    assert_eq!(copy.keyframe.frame, 13);
    assert_eq!(copy.keyframe.value, db(-10.0), "with its value");
    assert_eq!(
        project.locate_keyframe(names[0]).unwrap().keyframe.frame,
        10,
        "the keyframe that was copied has not moved"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_is_copied_to_a_frame_earlier_in_the_same_way() {
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);

    let copies = project
        .duplicate_keyframes(&[names[2]], TimeDirection::Earlier, 4)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 15, 26, 30]);
    assert_eq!(project.locate_keyframe(copies[0]).unwrap().keyframe.value, db(-30.0));
}

#[test]
fn the_gap_is_at_least_one_frame_so_that_a_copy_is_never_at_the_frame_of_the_keyframe_it_is_a_copy_of() {
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);

    project
        .duplicate_keyframes(&[names[2]], TimeDirection::Later, 0)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 15, 30, 31]);
}

#[test]
fn a_group_of_keyframes_is_copied_as_a_whole_with_the_same_distances_between_the_copies_as_between_the_keyframes() {
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);

    // The first and the last of the keyframes are 5 frames apart, and the gap is 2: the copies are 7 frames after them.
    let copies = project
        .duplicate_keyframes(&[names[0], names[1]], TimeDirection::Later, 2)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 15, 17, 22, 30]);
    assert_eq!(copies.len(), 2);
    assert_eq!(
        project.locate_keyframe(copies[0]).unwrap().keyframe.frame,
        17,
        "the copy of the first"
    );
    assert_eq!(
        project.locate_keyframe(copies[1]).unwrap().keyframe.frame,
        22,
        "the copy of the second"
    );
    assert_eq!(
        values_in_the_rack(&project, in_the_text),
        vec![db(-10.0), db(-15.0), db(-10.0), db(-15.0), db(-30.0)]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_group_copied_earlier_has_its_copies_a_gap_before_the_first_of_the_group() {
    let Racks {
        mut project,
        in_the_video,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_video);

    // The keyframes at 50 and 300 are 250 apart: the copies are 250 + 10 frames earlier, at -210 (which is no frame) and 40.
    let before = project.clone();
    assert_eq!(
        project.duplicate_keyframes(&names, TimeDirection::Earlier, 10),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before, "the copy that could be made was not made either");
    // The copy of the second only: 250 is not looked at, as there is one keyframe: it is 10 earlier.
    let copies = project
        .duplicate_keyframes(&names[1..], TimeDirection::Earlier, 10)
        .unwrap();
    assert_eq!(project.locate_keyframe(copies[0]).unwrap().keyframe.frame, 290);
}

#[test]
fn keyframes_of_several_racks_are_copied_each_in_its_own_rack_by_the_same_distance() {
    let Racks {
        mut project,
        in_the_video,
        on_the_track,
        ..
    } = racks();
    let from_the_video = names_of_the_keyframes_in(&project, in_the_video)[0];
    let from_the_track = names_of_the_keyframes_in(&project, on_the_track)[0];

    // At 50 and at 100 of the timeline: 50 apart, and the gap is 10, so the copies are 60 frames later.
    let copies = project
        .duplicate_keyframes(&[from_the_track, from_the_video], TimeDirection::Later, 10)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![50, 110, 300]);
    assert_eq!(frames_in_the_rack(&project, on_the_track), vec![100, 160]);
    assert_eq!(copies.len(), 2);
    assert_eq!(
        project.locate_keyframe(copies[0]).unwrap().rack.rack.id(),
        on_the_track,
        "the names are given in the order of the keyframes that were named"
    );
    assert_eq!(project.locate_keyframe(copies[1]).unwrap().rack.rack.id(), in_the_video);
}

#[test]
fn a_keyframe_named_twice_is_copied_once_and_nothing_to_copy_is_nothing_done() {
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);
    let before = project.clone();

    let copies = project
        .duplicate_keyframes(&[names[0], names[0]], TimeDirection::Later, 3)
        .unwrap();
    assert_eq!(copies.len(), 1);
    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 13, 15, 30]);

    let mut untouched = before.clone();
    assert_eq!(untouched.duplicate_keyframes(&[], TimeDirection::Later, 3), Ok(vec![]));
    assert_eq!(untouched, before, "nothing was changed");
}

// --- what cannot be copied ---

#[test]
fn copies_that_would_not_all_have_a_frame_or_a_frame_of_their_own_are_not_made_and_nothing_changes() {
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);
    let before = project.clone();

    // The drawer of the text has the frames 0 to 59: a copy at 70 is not in it, and one before the first is not either.
    assert_eq!(
        project.duplicate_keyframes(&[names[2]], TimeDirection::Later, 40),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(
        project.duplicate_keyframes(&[names[0]], TimeDirection::Earlier, 11),
        Err(EditError::NoRoomForTheKeyframes)
    );
    // A copy at 15 would be on the keyframe that is there.
    assert_eq!(
        project.duplicate_keyframes(&[names[0]], TimeDirection::Later, 5),
        Err(EditError::Rack(RackEditError::FramesWouldCoincide))
    );
    // Two keyframes that are 15 frames apart and a gap of 31: the copies are 46 frames later, at 61 and 76, which is past the end.
    assert_eq!(
        project.duplicate_keyframes(&[names[1], names[2]], TimeDirection::Later, 31),
        Err(EditError::NoRoomForTheKeyframes)
    );
    let nobody = KeyframeId::new(in_the_text, 99);
    assert_eq!(
        project.duplicate_keyframes(&[names[0], nobody], TimeDirection::Later, 3),
        Err(EditError::Rack(RackEditError::NoSuchKeyframe))
    );
    assert_eq!(project, before);
}

#[test]
fn a_copy_at_the_last_frame_of_a_drawer_is_made_and_a_copy_at_the_frame_after_it_is_not() {
    // The drawer of the text has the frames 0 to 59, and its last keyframe is at 30.
    let Racks {
        mut project,
        in_the_text,
        ..
    } = racks();
    let names = names_of_the_keyframes_in(&project, in_the_text);
    let before = project.clone();

    assert_eq!(
        project.duplicate_keyframes(&[names[2]], TimeDirection::Later, 30),
        Err(EditError::NoRoomForTheKeyframes),
        "a copy at 60 is past the last frame"
    );
    assert_eq!(project, before);

    project
        .duplicate_keyframes(&[names[2]], TimeDirection::Later, 29)
        .expect("a copy at 59 is at the last frame");
    assert_eq!(frames_in_the_rack(&project, in_the_text), vec![10, 15, 30, 59]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn asking_whether_keyframes_can_be_copied_gives_the_answer_that_copying_them_does_and_changes_nothing() {
    let Racks {
        mut project,
        in_the_text,
        in_the_video,
        ..
    } = racks();
    let mut keyframes = names_of_the_keyframes_in(&project, in_the_text);
    keyframes.extend(names_of_the_keyframes_in(&project, in_the_video));
    let before = project.clone();

    for subset in [&keyframes[..1], &keyframes[2..4], &keyframes[..], &keyframes[3..]] {
        for towards in [TimeDirection::Earlier, TimeDirection::Later] {
            for gap in [0, 1, 7, 40] {
                let asked = project.check_the_duplicating_of_keyframes(subset, towards, gap);
                assert_eq!(project, before, "asking changes nothing");
                let done = project.duplicate_keyframes(subset, towards, gap);
                assert_eq!(asked, done.map(|_| ()), "{subset:?} {towards:?} {gap}");
                project = before.clone();
            }
        }
    }
}

// --- crates and whatever is copied ---

/// A video of 600 frames, a text object from frame 100 to 160 and, in front of the video as the text is, a keyframe crate with a rack of
/// the loudness of the video, which has keyframes at the frames 310 and 320 of the timeline (the crate lasts from one to the other, and
/// one frame more: it begins at 310 and ends at 321). Another text object is at frame 340, which is where the crate can last until.
/// The project, the video, the crate and its rack.
fn a_crate_with_a_text_object_after_it_and_one_before() -> (Project, ClipId, ClipId, RackId) {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 50).unwrap();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-1.0)), (20, db(-2.0))],
    );
    project
        .insert_text(front, frame(340), TextObject::default(), 20)
        .unwrap();
    assert_eq!(first_and_end_of_the_crate_clip(&project, crate_id), (310, 321));
    (project, video, crate_id, rack)
}

fn first_and_end_of_the_crate_clip(project: &Project, crate_id: ClipId) -> (u64, u64) {
    let placement = project.placement_of(crate_id).unwrap();
    (
        placement.first_timeline_frame.value(),
        placement.end_timeline_frame.value(),
    )
}

#[test]
fn the_copies_of_keyframes_after_those_of_a_crate_make_it_longer_where_it_has_room_and_are_not_made_where_it_has_not() {
    let (mut project, _, crate_id, rack) = a_crate_with_a_text_object_after_it_and_one_before();
    let names = names_of_the_keyframes_in(&project, rack);

    project.duplicate_keyframes(&names, TimeDirection::Later, 5).unwrap();
    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 10, 15, 25]);
    assert_eq!(
        first_and_end_of_the_crate_clip(&project, crate_id),
        (310, 336),
        "the copies, at the frames 325 and 335, are in the crate, which lasts until the last"
    );

    // Another copy of all four would be 30 frames later, at 340, 350, 355 and 365, and the text object is at 340.
    let names = names_of_the_keyframes_in(&project, rack);
    let before = project.clone();
    assert_eq!(
        project.duplicate_keyframes(&names, TimeDirection::Later, 5),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before, "nothing is changed");

    // One more frame of room is enough for a copy of the first keyframe alone: 28 frames after it, at 338, in the last frame the crate has.
    project
        .duplicate_keyframes(&names[..1], TimeDirection::Later, 28)
        .unwrap();
    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 10, 15, 25, 28]);
    assert_eq!(first_and_end_of_the_crate_clip(&project, crate_id), (310, 339));
    assert_the_racks_are_sound(&project);
}

#[test]
fn the_copies_of_keyframes_before_those_of_a_crate_make_it_begin_earlier_and_what_is_in_it_stays_at_the_frames_it_was_at()
 {
    let (mut project, video, crate_id, rack) = a_crate_with_a_text_object_after_it_and_one_before();
    let video = AnimationTarget::Clip(video);
    let names = names_of_the_keyframes_in(&project, rack);

    project
        .duplicate_keyframes(&names[..1], TimeDirection::Earlier, 4)
        .unwrap();

    assert_eq!(first_and_end_of_the_crate_clip(&project, crate_id), (306, 321));
    assert_eq!(
        frames_in_the_rack(&project, rack),
        vec![0, 4, 14],
        "counted from where the crate begins now"
    );
    assert_eq!(
        project
            .keyframes_of(video, PropertyId::Gain)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect::<Vec<_>>(),
        vec![306, 310, 320],
        "the keyframes that were there are where they were, and the copy is 4 frames before them"
    );

    // A copy 150 frames before the first would be at frame 156, which is in the text object that is from 100 to 160.
    let names = names_of_the_keyframes_in(&project, rack);
    let before = project.clone();
    assert_eq!(
        project.duplicate_keyframes(&names[..1], TimeDirection::Earlier, 150),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before, "nothing is changed");
    assert_the_racks_are_sound(&project);
}

#[test]
fn whatever_keyframes_are_copied_the_copies_are_where_they_should_be_and_the_project_stays_sound() {
    let mut copied = 0;
    for seed in 1..=200u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let Racks { mut project, .. } = racks();
        let all: Vec<KeyframeId> = project
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
        let mut chosen: Vec<KeyframeId> = all.iter().copied().filter(|_| random.next_below(3) == 0).collect();
        if chosen.is_empty() {
            chosen.push(all[random.next_below(all.len() as u64) as usize]);
        }
        let towards = if random.next_below(2) == 0 {
            TimeDirection::Earlier
        } else {
            TimeDirection::Later
        };
        let gap = random.next_below(40);
        let before = project.clone();
        let context = format!("seed {seed}: {chosen:?} {towards:?} {gap}");

        let asked = project.check_the_duplicating_of_keyframes(&chosen, towards, gap);
        let result = project.duplicate_keyframes(&chosen, towards, gap);

        assert_eq!(asked, result.clone().map(|_| ()), "{context}");
        match result {
            Err(_) => assert_eq!(project, before, "{context}"),
            Ok(copies) => {
                copied += 1;
                assert_eq!(copies.len(), chosen.len(), "{context}");
                let timeline_frames: Vec<u64> = chosen
                    .iter()
                    .map(|keyframe| before.timeline_frame_of_keyframe(*keyframe).unwrap())
                    .collect();
                let distance =
                    timeline_frames.iter().max().unwrap() - timeline_frames.iter().min().unwrap() + gap.max(1);
                for (original, copy) in chosen.iter().zip(&copies) {
                    let original_before = before.locate_keyframe(*original).unwrap();
                    let copy_now = project.locate_keyframe(*copy).expect(&context);
                    assert_eq!(copy_now.rack.rack.id(), original_before.rack.rack.id(), "{context}");
                    assert_eq!(copy_now.keyframe.value, original_before.keyframe.value, "{context}");
                    let expected = match towards {
                        TimeDirection::Later => original_before.timeline_frame() + distance,
                        TimeDirection::Earlier => original_before.timeline_frame() - distance,
                    };
                    assert_eq!(copy_now.timeline_frame(), expected, "{context}");
                    assert!(!all.contains(copy), "a copy is a new keyframe: {context}");
                    assert_eq!(
                        project.locate_keyframe(*original).unwrap().keyframe,
                        original_before.keyframe,
                        "the keyframe that was copied is as it was: {context}"
                    );
                }
                let count_after: usize = project.located_racks().map(|located| located.rack.len()).sum();
                assert_eq!(count_after, all.len() + chosen.len(), "{context}");
            }
        }
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
    }
    assert!(copied > 50, "the test copies keyframes: {copied} copies were made");
}
