// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the racks of a project say together, wherever they are kept: the keyframes of a clip that are in the drawer of
//! another clip, or of a track, and what happens to them when clips are moved, cut, shortened and deleted.

use super::tests::{first_clip_location, frame, project_with_a_video};
use super::*;
use crate::test_support::DeterministicRandomNumbers;

pub(super) fn db(decibels: f32) -> PropertyValue {
    PropertyValue::Number(decibels)
}

/// A project with a video of 600 frames on its track, and in front of it a track with a text object from frame 100 to 160.
pub(super) fn a_big_video_and_a_small_text() -> (Project, ClipId, ClipId) {
    let mut project = project_with_a_video(600);
    let video = first_clip_location(&project, 0);
    let in_front = project.add_track_in_front();
    let (text, lasts) = project
        .insert_text(in_front, frame(100), TextObject::default(), 60)
        .unwrap();
    assert_eq!(lasts, 60);
    (project, video, text)
}

/// Puts a rack in the drawer of `owner`, of the keyframes of `property` of `target` at these frames (counted from the start of the
/// drawer), without anything else being done but making a keyframe crate as long as its keyframes need (and so counting the frames
/// of the racks in it from where it begins then).
pub(super) fn put_a_rack_in(
    project: &mut Project,
    owner: AnimationTarget,
    target: AnimationTarget,
    property: PropertyId,
    keyframes: &[(u64, PropertyValue)],
) -> RackId {
    let position = match owner {
        AnimationTarget::Clip(clip) => project.track_position_holding(clip).unwrap(),
        AnimationTarget::Track(track) => project.track_position(track).unwrap(),
    };
    let id = project.tracks[position].make_a_rack_id();
    let mut rack = Rack::new(id, target, property);
    for (frame, value) in keyframes {
        rack.set(*frame, value.clone()).unwrap();
    }
    project.drawer_mut(owner).unwrap().push(rack);
    // (A keyframe crate lasts exactly as long as its keyframes need, from the first to the last.)
    project
        .fit_the_crate(owner)
        .expect("a crate has room for the keyframes that are put in it");
    id
}

pub(super) fn gain_at(project: &Project, target: AnimationTarget, at: u64) -> f32 {
    match project.property_value_at(target, PropertyId::Gain, frame(at)) {
        Some(PropertyValue::Number(decibels)) => decibels,
        other => panic!("not a gain: {other:?}"),
    }
}

/// The frames the keyframes of the rack are at, counted from the start of the drawer it is in.
pub(super) fn frames_in_the_rack(project: &Project, rack: RackId) -> Vec<u64> {
    project
        .located_rack(rack)
        .expect("the rack is in the project")
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect()
}

/// The names of the keyframes in the rack, the earliest first.
pub(super) fn names_of_the_keyframes_in(project: &Project, rack: RackId) -> Vec<KeyframeId> {
    let rack = project.located_rack(rack).expect("the rack is in the project").rack;
    rack.keyframes().iter().map(RackKeyframe::id).collect()
}

/// The names of the racks in the drawer of `owner`, in the order they are in.
pub(super) fn names_of_the_racks_in(project: &Project, owner: AnimationTarget) -> Vec<RackId> {
    project
        .drawer_of(owner)
        .expect("the drawer is in the project")
        .racks()
        .iter()
        .map(Rack::id)
        .collect()
}

/// The frames of the timeline that the keyframes of `property` of the clip `video` are at, from all the racks that have some.
pub(super) fn where_the_video_has_keyframes(project: &Project, video: ClipId, property: PropertyId) -> Vec<u64> {
    keyframe_frames(project, AnimationTarget::Clip(video), property)
}

pub(super) fn keyframe_frames(project: &Project, target: AnimationTarget, property: PropertyId) -> Vec<u64> {
    project
        .keyframes_of(target, property)
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect()
}

pub(super) fn assert_the_racks_are_sound(project: &Project) {
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
}

// --- what a property is when its keyframes are kept elsewhere ---

#[test]
fn keyframes_kept_in_the_drawer_of_another_clip_animate_the_clip_they_are_of_at_frames_of_the_timeline() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    // The drawer of the text starts at frame 100: its keyframes are at frames 110 and 140.
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );

    assert!(project.is_animated(big, PropertyId::Gain));
    assert!(!project.is_animated(AnimationTarget::Clip(text), PropertyId::Gain));
    assert_eq!(gain_at(&project, big, 110), -20.0);
    assert_eq!(gain_at(&project, big, 125), -10.0);
    assert_eq!(gain_at(&project, big, 140), 0.0);
    assert_eq!(
        gain_at(&project, big, 0),
        -20.0,
        "before the first it is what the first says"
    );
    assert_eq!(
        gain_at(&project, big, 500),
        0.0,
        "and after the last, what the last says"
    );
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![110, 140]);
    assert_eq!(
        project.keyframe_before(big, PropertyId::Gain, frame(125)),
        Some(frame(110))
    );
    assert_eq!(
        project.keyframe_after(big, PropertyId::Gain, frame(125)),
        Some(frame(140))
    );
    assert!(project.has_keyframe_at(big, PropertyId::Gain, frame(110)));
    assert!(!project.has_keyframe_at(big, PropertyId::Gain, frame(111)));
    assert_the_racks_are_sound(&project);
}

#[test]
fn racks_in_the_drawers_of_several_clips_and_of_a_track_are_one_curve() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let (second_text, _) = project
        .insert_text(front, frame(300), TextObject::default(), 60)
        .unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (10, db(-12.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(second_text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (10, db(-12.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Track(front),
        big,
        PropertyId::Gain,
        &[(200, db(6.0))],
    );

    assert_eq!(
        keyframe_frames(&project, big, PropertyId::Gain),
        vec![100, 110, 200, 300, 310]
    );
    // From the last keyframe of the first dip (-12 dB at frame 110) to the one the track has (6 dB at frame 200).
    assert!((gain_at(&project, big, 150) + 4.0).abs() < 1e-4);
    assert!((gain_at(&project, big, 155) + 3.0).abs() < 1e-4);
    assert!(
        (gain_at(&project, big, 250) - 3.0).abs() < 1e-4,
        "half way from 6 dB to the 0 dB at frame 300"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn where_two_keyframes_of_a_property_are_at_one_frame_the_one_in_the_rack_that_comes_later_is_the_one_that_counts() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    // (The track in front is first, so the drawer of the video, which is on the track at the back, comes later.)
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(-10.0))],
    );
    put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(100, db(-5.0))]);

    assert_eq!(gain_at(&project, big, 100), -5.0);
    let location = project.locate_the_keyframe_at(big, PropertyId::Gain, 100).unwrap();
    assert_eq!(location.owner, big, "in the drawer of the video itself");
    assert_eq!(
        keyframe_frames(&project, big, PropertyId::Gain),
        vec![100],
        "they are one keyframe"
    );
}

#[test]
fn the_resolved_animations_say_what_the_animation_of_each_thing_is() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (30, db(-9.0))],
    );
    put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );

    let resolved = project.resolved_animations();

    assert_eq!(resolved.of(big), &project.animation_of(big));
    assert!(resolved.of(big).is_animated(PropertyId::Gain) && resolved.of(big).is_animated(PropertyId::Muted));
    assert_eq!(
        resolved.of(AnimationTarget::Clip(text)),
        &Animation::default(),
        "nothing animates the text"
    );
    assert_eq!(resolved.of(big).value_at(PropertyId::Gain, 115), Some(db(-4.5)));
}

// --- changing and making and taking away keyframes that are kept elsewhere ---

#[test]
fn changing_a_property_at_a_frame_that_has_its_keyframe_kept_elsewhere_changes_it_there() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    let made = project
        .set_property_at(big, PropertyId::Gain, db(-3.0), frame(110))
        .unwrap();

    assert_eq!(made, None, "no keyframe was made");
    assert_eq!(gain_at(&project, big, 110), -3.0);
    assert!(
        project.placement_of(video).unwrap().drawer.is_empty(),
        "the clip's own drawer has nothing in it"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_made_for_a_clip_whose_other_keyframes_are_kept_elsewhere_goes_in_its_own_drawer_and_the_result_says_so() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    let made = project
        .add_keyframe(big, PropertyId::Gain, frame(300))
        .unwrap()
        .unwrap();

    assert_eq!(made.owner, big);
    let own = project
        .placement_of(video)
        .unwrap()
        .drawer
        .first_rack_for(big, PropertyId::Gain)
        .unwrap();
    assert_eq!(own.id(), made.rack);
    assert_eq!(own.keyframe(made.keyframe).unwrap().frame, 300);
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![110, 300]);
    assert_eq!(
        project.add_keyframe(big, PropertyId::Gain, frame(300)).unwrap(),
        None,
        "one is there already, and nothing changes"
    );
    assert_eq!(
        project.add_keyframe(big, PropertyId::Gain, frame(110)).unwrap(),
        None,
        "and one that is kept elsewhere counts"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn taking_away_a_keyframe_that_is_kept_elsewhere_takes_it_from_there_and_the_rack_goes_when_it_has_none_left() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(
        &mut project,
        owner,
        big,
        PropertyId::Gain,
        &[(10, db(-12.0)), (40, db(-6.0))],
    );

    project.remove_keyframe(big, PropertyId::Gain, frame(110)).unwrap();
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![140]);
    assert_eq!(project.drawer_of(owner).unwrap().len(), 1);

    project.remove_keyframe(big, PropertyId::Gain, frame(140)).unwrap();
    assert!(project.drawer_of(owner).unwrap().is_empty(), "the rack has gone");
    assert!(!project.is_animated(big, PropertyId::Gain));
    assert_eq!(
        project.placement_of(video).unwrap().clip.as_media().unwrap().gain(),
        Gain::from_decibels(-6.0),
        "the video stays as the last keyframe said"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_at_a_frame_that_two_racks_have_is_taken_from_both_and_does_not_show_again() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(-10.0)), (30, db(0.0))],
    );
    put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(100, db(-5.0))]);

    project.remove_keyframe(big, PropertyId::Gain, frame(100)).unwrap();

    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![130]);
    assert!(project.placement_of(video).unwrap().drawer.is_empty());
    assert_the_racks_are_sound(&project);
}

// --- moving, deleting ---

#[test]
fn the_keyframes_in_the_drawer_of_a_clip_move_with_it_whatever_they_are_of() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    project
        .move_clips(&[ClipMove {
            clip: text,
            to_track: front,
            start: frame(250),
        }])
        .unwrap();

    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![260]);
    assert_eq!(gain_at(&project, big, 100), -20.0, "still the same before the first");
    assert_the_racks_are_sound(&project);
}

#[test]
fn deleting_a_clip_takes_the_racks_that_animate_it_from_the_drawers_of_other_clips_and_a_clip_that_holds_racks_takes_them_with_it()
 {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    put_a_rack_in(
        &mut project,
        big,
        owner,
        PropertyId::Rotation,
        &[(5, PropertyValue::Number(30.0))],
    );
    assert_the_racks_are_sound(&project);

    project.delete_clip(video).unwrap();

    assert!(
        project.drawer_of(owner).unwrap().is_empty(),
        "a rack of the loudness of a clip that is gone has nothing to do"
    );
    assert_the_racks_are_sound(&project);

    // The text is animated by a rack in the drawer of another text, which goes when that is deleted.
    let front = project.tracks()[0].id();
    let (other, _) = project
        .insert_text(front, frame(300), TextObject::default(), 30)
        .unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(other),
        owner,
        PropertyId::Rotation,
        &[(2, PropertyValue::Number(10.0))],
    );
    assert!(project.is_animated(owner, PropertyId::Rotation));
    project.delete_clip(other).unwrap();
    assert!(
        !project.is_animated(owner, PropertyId::Rotation),
        "the keyframes went with the drawer they were in"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn deleting_a_track_takes_the_racks_that_animate_it_and_its_clips_from_the_drawers_that_are_left() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let back = project.tracks()[1].id();
    // The text on the track in front holds racks of the loudness of the video and of its track, which are at the back.
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Track(back),
        PropertyId::Gain,
        &[(5, db(-1.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Track(front),
        AnimationTarget::Track(back),
        PropertyId::Muted,
        &[(3, PropertyValue::Flag(true))],
    );
    assert_the_racks_are_sound(&project);

    project.delete_track(back).unwrap();

    assert!(project.drawer_of(AnimationTarget::Clip(text)).unwrap().is_empty());
    assert!(project.drawer_of(AnimationTarget::Track(front)).unwrap().is_empty());
    assert_the_racks_are_sound(&project);
}

// --- cutting ---

#[test]
fn a_clip_that_is_cut_goes_on_being_animated_in_both_halves_by_the_keyframes_that_are_kept_elsewhere() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (40, db(-12.0))],
    );

    let cut = project.split_at(frame(120), Some(&[video]));

    let second_half = cut[0].1.second_half;
    assert_eq!(gain_at(&project, big, 110), -3.0);
    assert_eq!(gain_at(&project, big, 119), -5.7, "the first half is as the whole was");
    assert!((gain_at(&project, AnimationTarget::Clip(second_half), 120) + 6.0).abs() < 1e-4);
    assert!(
        (gain_at(&project, AnimationTarget::Clip(second_half), 130) + 9.0).abs() < 1e-4,
        "and so is the second"
    );
    assert_eq!(
        project.drawer_of(AnimationTarget::Clip(text)).unwrap().len(),
        2,
        "the drawer of the text has a rack for each half"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn cutting_a_clip_that_holds_racks_of_other_clips_divides_them_and_leaves_what_they_animate_as_it_was() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (20, db(-12.0)), (50, db(-3.0))],
    );
    let before: Vec<f32> = (0..600).map(|at| gain_at(&project, big, at)).collect();

    // (Only the text is cut: the video is not at the frame the cut is made at.)
    let cut = project.split_at(frame(130), Some(&[text]));

    assert_eq!(cut.len(), 1);
    let after: Vec<f32> = (0..600).map(|at| gain_at(&project, big, at)).collect();
    assert_eq!(before, after);
    let racks: Vec<usize> = project.located_racks().map(|located| located.rack.len()).collect();
    assert_eq!(
        racks,
        vec![2, 1],
        "keyframes before and after the cut, in the drawer of each half"
    );
    assert_the_racks_are_sound(&project);
}

/// Whatever racks animate a clip, and wherever they are, cutting at any frame leaves every frame of it with what it had.
#[test]
fn whatever_racks_there_are_a_cut_leaves_every_frame_of_the_clips_with_the_value_the_whole_had() {
    for seed in 1..=150u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, video, text) = a_big_video_and_a_small_text();
        let big = AnimationTarget::Clip(video);
        let front = project.tracks()[0].id();
        let mut random_keyframes = |limit: u64| -> Vec<(u64, PropertyValue)> {
            (0..1 + random.next_below(4))
                .map(|_| {
                    (
                        random.next_below(limit),
                        db(random.next_below(300) as f32 / 10.0 - 15.0),
                    )
                })
                .collect()
        };
        // In its own drawer, in the drawer of the text, and in the drawer of the track in front.
        let own = random_keyframes(600);
        let held_by_the_text = random_keyframes(60);
        let held_by_the_track = random_keyframes(600);
        put_a_rack_in(&mut project, big, big, PropertyId::Gain, &own);
        put_a_rack_in(
            &mut project,
            AnimationTarget::Clip(text),
            big,
            PropertyId::Gain,
            &held_by_the_text,
        );
        put_a_rack_in(
            &mut project,
            AnimationTarget::Track(front),
            big,
            PropertyId::Gain,
            &held_by_the_track,
        );
        let before: Vec<f32> = (0..600).map(|at| gain_at(&project, big, at)).collect();
        let cut_at = 1 + random.next_below(599);

        let cut = project.split_at(frame(cut_at), None);

        let (first_half, second_half) = cut
            .iter()
            .map(|(_, outcome)| (outcome.first_half, outcome.second_half))
            .find(|(first, _)| *first == video)
            .expect("the video is cut");
        for (at, expected) in before.iter().enumerate() {
            let half = if (at as u64) < cut_at { first_half } else { second_half };
            let now = gain_at(&project, AnimationTarget::Clip(half), at as u64);
            assert!(
                (now - expected).abs() < 1e-3,
                "seed {seed}, cut at {cut_at}: frame {at} is {now} dB and was {expected}"
            );
        }
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "seed {seed}");
    }
}

// --- making a clip shorter ---

#[test]
fn making_a_clip_shorter_brings_in_the_keyframes_it_holds_that_it_has_no_frame_for_and_the_clip_they_animate_goes_on_having_the_last()
 {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (50, db(0.0))],
    );

    let lasts = project.set_clip_duration(text, 30).unwrap();

    assert_eq!(lasts, 30);
    // The keyframe at 10 stays, and the one at 50, which the clip has no frame for any more, is at its last frame, with its value.
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![110, 129]);
    assert_eq!(frames_in_the_rack(&project, rack), vec![10, 29]);
    assert_eq!(gain_at(&project, big, 129), 0.0, "the value the keyframe had");
    assert_eq!(gain_at(&project, big, 500), 0.0, "the video goes on having the last");
    assert_the_racks_are_sound(&project);
}

/// The bug of this round, at the level of the project: a clip that is made shorter lost the keyframes it had no frame for, with
/// room for them left.
#[test]
fn making_a_clip_shorter_loses_none_of_the_keyframes_it_holds_while_it_has_as_many_frames_as_keyframes() {
    let (mut project, _, text) = a_big_video_and_a_small_text();
    let mine = AnimationTarget::Clip(text);
    let keyframes: Vec<(u64, PropertyValue)> = (0..6).map(|number| (number * 10, db(number as f32))).collect();
    let rack = put_a_rack_in(&mut project, mine, mine, PropertyId::Rotation, &keyframes);
    let names_before = names_of_the_keyframes_in(&project, rack);

    for new_length in [50, 20, 6] {
        project.set_clip_duration(text, new_length).unwrap();
        assert_eq!(
            names_of_the_keyframes_in(&project, rack),
            names_before,
            "shortened to {new_length} frames, which has a frame for each of the six"
        );
        assert!(
            frames_in_the_rack(&project, rack)
                .iter()
                .all(|frame| *frame < new_length)
        );
    }
    assert_eq!(
        frames_in_the_rack(&project, rack),
        vec![0, 1, 2, 3, 4, 5],
        "at every one of the six frames"
    );
    assert_eq!(project.keyframe_count(), 6);
    assert_the_racks_are_sound(&project);

    project.set_clip_duration(text, 5).unwrap();
    assert_eq!(
        project.keyframe_count(),
        5,
        "with one frame less than keyframes, one is lost and the rest have a frame each"
    );
    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 1, 2, 3, 4]);
    assert_the_racks_are_sound(&project);
}

/// Whatever keyframes a clip holds, making it shorter loses only those it has no frame for: those of its drawer that are still
/// in it are as many as it has frames for, the earliest of them, in their order, and what is kept elsewhere is not touched.
#[test]
fn making_a_clip_shorter_keeps_as_many_of_its_keyframes_as_it_has_frames_for_in_their_order_and_leaves_what_is_kept_elsewhere()
 {
    let (mut losses, mut rounds_with_a_crowd) = (0, 0);
    for seed in 1..=100u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, _, text) = a_big_video_and_a_small_text();
        let mine = AnimationTarget::Clip(text);
        let front = project.tracks()[0].id();
        let mut random_keyframes = |limit: u64| -> Vec<(u64, PropertyValue)> {
            (0..1 + random.next_below(8))
                .map(|_| {
                    (
                        random.next_below(limit),
                        PropertyValue::Number(random.next_below(200) as f32 / 10.0),
                    )
                })
                .collect()
        };
        let own = random_keyframes(60);
        let held_by_the_track = random_keyframes(400);
        let own_rack = put_a_rack_in(&mut project, mine, mine, PropertyId::Rotation, &own);
        let track_rack = put_a_rack_in(
            &mut project,
            AnimationTarget::Track(front),
            mine,
            PropertyId::Rotation,
            &held_by_the_track,
        );
        let names_before = names_of_the_keyframes_in(&project, own_rack);
        let track_rack_before = project.located_rack(track_rack).unwrap().rack.clone();
        let new_length = 1 + random.next_below(59);

        project.set_clip_duration(text, new_length).unwrap();

        let kept = names_before.len().min(new_length as usize);
        assert_eq!(
            names_of_the_keyframes_in(&project, own_rack),
            names_before[..kept],
            "seed {seed}, shortened to {new_length}: the earliest {kept} of {} are kept, in their order",
            names_before.len()
        );
        assert!(
            frames_in_the_rack(&project, own_rack)
                .iter()
                .all(|frame| *frame < new_length),
            "seed {seed}"
        );
        assert_eq!(
            project.located_rack(track_rack).unwrap().rack,
            &track_rack_before,
            "seed {seed}: the keyframes that the track holds are not in the clip, and are as they were"
        );
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "seed {seed}");
        losses += usize::from(names_before.len() > kept);
        rounds_with_a_crowd += usize::from(own.len() > 1 && new_length < 8);
    }
    assert!(losses > 3, "some of the rounds lose keyframes: {losses}");
    assert!(
        rounds_with_a_crowd > 5,
        "some of the rounds make the clip very short: {rounds_with_a_crowd}"
    );
}

// --- how long things last ---

/// The frames of the keyframes of each rack in the drawer of `clip`.
fn frames_of_the_racks_in_the_drawer_of(project: &Project, clip: ClipId) -> Vec<Vec<u64>> {
    project
        .drawer_of(AnimationTarget::Clip(clip))
        .map(|drawer| {
            drawer
                .racks()
                .iter()
                .map(|rack| rack.keyframes().iter().map(|keyframe| keyframe.frame).collect())
                .collect()
        })
        .unwrap_or_default()
}

/// How many keyframes there would be fewer of if each of those of the racks `racks` were put at the frame that is as far through
/// what lasts `to` frames as it was through what lasted `from`, and of two that land on one frame the later was kept: the way it was
/// done before keyframes were put at frames of their own.
fn keyframes_that_the_old_way_of_stretching_would_have_merged(racks: &[Vec<u64>], from: u64, to: u64) -> usize {
    racks
        .iter()
        .map(|frames| {
            let mut stretched: Vec<u64> = frames
                .iter()
                .map(|frame| ((*frame as f64 * to as f64 / from as f64).round() as u64).min(to - 1))
                .collect();
            stretched.dedup();
            frames.len() - stretched.len()
        })
        .sum()
}

/// The bug of this round, at the level of the project: speeding a clip up, or a track, or counting the project again at another
/// frame rate, or putting a clip on a faster track, or shortening a clip lost keyframes that were close together, though the clips
/// had a frame for each of their keyframes. Whatever is done, none is lost while every clip has at least as many frames as it has
/// keyframes.
#[test]
fn whatever_is_done_to_how_long_things_last_no_keyframe_is_lost_while_every_clip_has_a_frame_for_each_of_its_keyframes()
{
    let (mut would_have_been_merged, mut lengths_changed) = (0, 0);
    for seed in 1..=60u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, video, text) = a_big_video_and_a_small_text();
        let front = project.tracks()[0].id();
        let back = project.tracks()[1].id();
        project.set_track_speed(front, SpeedFactor::new(2).unwrap()).unwrap();
        // The keyframes are all in a narrow stretch of what has them, so that making it shorter crowds them.
        let keyframes_in = |random: &mut DeterministicRandomNumbers, limit: u64| -> Vec<(u64, PropertyValue)> {
            let mut frames = std::collections::BTreeSet::new();
            while frames.len() < 6 {
                frames.insert(random.next_below(limit));
            }
            frames
                .into_iter()
                .enumerate()
                .map(|(number, frame)| (frame, db(number as f32)))
                .collect()
        };
        let of_the_video = keyframes_in(&mut random, 40);
        let of_the_text = keyframes_in(&mut random, 20);
        put_a_rack_in(
            &mut project,
            AnimationTarget::Clip(video),
            AnimationTarget::Clip(video),
            PropertyId::Gain,
            &of_the_video,
        );
        put_a_rack_in(
            &mut project,
            AnimationTarget::Clip(text),
            AnimationTarget::Clip(text),
            PropertyId::Rotation,
            &of_the_text,
        );
        let keyframes_to_keep = project.keyframe_count();
        assert_eq!(keyframes_to_keep, 12, "seed {seed}");

        for step in 0..10 {
            let length_of = |project: &Project, clip| project.placement_of(clip).unwrap().timeline_frame_count();
            let (video_before, text_before) = (length_of(&project, video), length_of(&project, text));
            let racks_before = [
                frames_of_the_racks_in_the_drawer_of(&project, video),
                frames_of_the_racks_in_the_drawer_of(&project, text),
            ];
            let context = format!("seed {seed}, step {step}");
            let operation = random.next_below(5);
            match operation {
                0 => project
                    .set_clip_speed(video, SpeedFactor::new(1 + random.next_below(4) as u32).unwrap())
                    .unwrap(),
                1 => project
                    .set_track_speed(back, SpeedFactor::new(1 + random.next_below(3) as u32).unwrap())
                    .unwrap(),
                2 => {
                    let rates = [24, 25, 30, 50, 60];
                    project.set_frame_rate(FrameRate::new(rates[random.next_below(5) as usize], 1).unwrap());
                }
                3 => {
                    project.set_clip_duration(text, 12 + random.next_below(89)).unwrap();
                }
                _ => {
                    let to_track = if random.next_below(2) == 0 { front } else { back };
                    // (A move that cannot be made, because something is in the way, is not made.)
                    let _ = project.move_clips(&[ClipMove {
                        clip: video,
                        to_track,
                        start: frame(300 + random.next_below(3) * 200),
                    }]);
                }
            }
            let (video_after, text_after) = (length_of(&project, video), length_of(&project, text));
            for (racks, from, to) in [
                (&racks_before[0], video_before, video_after),
                (&racks_before[1], text_before, text_after),
            ] {
                if from != to {
                    lengths_changed += 1;
                    // (A clip that is made shorter by being asked to last less loses what is cut off, not what lands on one frame.)
                    if operation != 3 {
                        would_have_been_merged +=
                            keyframes_that_the_old_way_of_stretching_would_have_merged(racks, from, to);
                    }
                }
            }
            for (clip, frames) in [(video, video_after), (text, text_after)] {
                let keyframes = frames_of_the_racks_in_the_drawer_of(&project, clip)
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>();
                assert!(
                    frames >= keyframes as u64,
                    "{context}: {clip} has {frames} frames for {keyframes} keyframes"
                );
            }
            assert_eq!(
                project.keyframe_count(),
                keyframes_to_keep,
                "{context}: a keyframe was lost"
            );
            assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
        }
    }
    assert!(
        lengths_changed > 300,
        "the lengths of clips are changed a lot: {lengths_changed}"
    );
    assert!(
        would_have_been_merged > 0,
        "in some of those the old way would have merged keyframes, or this does not test what it is for"
    );
}

// --- the checks themselves ---

#[test]
fn a_project_whose_racks_are_in_order_has_no_problems_and_what_is_wrong_is_said() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    assert_the_racks_are_sound(&project);

    // A keyframe beyond the end of the drawer it is in, a rack of something that is not there, a property that clip has not.
    put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Muted,
        &[(61, PropertyValue::Flag(true))],
    );
    put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(ClipId::new(TrackId::new(77), 0)),
        PropertyId::Gain,
        &[(1, db(0.0))],
    );
    put_a_rack_in(&mut project, owner, owner, PropertyId::Gain, &[(1, db(0.0))]);
    let problems = project.problems_with_the_racks();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("a keyframe at frame 61 of the drawer it is in, which has no room for it"))
    );
    assert!(problems.iter().any(|problem| problem.contains("is not there")));
    assert!(problems.iter().any(|problem| problem.contains("does not have it")));
}

/// The racks of a text object's drawer that two racks of the loudness of the video are put in, at frames 5 and 9 of the first.
fn a_text_with_a_rack_of_the_loudness_of_the_video() -> (Project, AnimationTarget, RackId) {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let owner = AnimationTarget::Clip(text);
    let rack = put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(5, db(-1.0)), (9, db(-2.0))],
    );
    (project, owner, rack)
}

#[test]
fn a_keyframe_that_is_in_two_racks_under_one_name_is_found() {
    let (mut project, owner, first) = a_text_with_a_rack_of_the_loudness_of_the_video();
    let the_keyframe = project.located_rack(first).unwrap().rack.keyframes()[0].clone();
    let target = project.located_rack(first).unwrap().rack.target();
    let mut second = Rack::new(RackId::new(project.tracks()[0].id(), 99), target, PropertyId::Gain);
    second.put_in(the_keyframe.clone()).unwrap();
    project.drawer_mut(owner).unwrap().push(second);

    let problems = project.problems_with_the_racks();

    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains(&format!("there is more than one {}", the_keyframe.id())),
        "{problems:?}"
    );
}

#[test]
fn keyframes_that_are_not_in_the_order_of_their_frames_or_are_two_to_a_frame_are_found() {
    for (name, frames_of_the_keyframes) in [("backwards", [9, 5]), ("two to one frame", [5, 5])] {
        let (mut project, owner, rack) = a_text_with_a_rack_of_the_loudness_of_the_video();
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{name}");
        let mut keyframes = project.located_rack(rack).unwrap().rack.keyframes().to_vec();
        for (keyframe, frame) in keyframes.iter_mut().zip(frames_of_the_keyframes) {
            keyframe.frame = frame;
        }
        let unsound = project
            .located_rack(rack)
            .unwrap()
            .rack
            .with_keyframes_in_any_order(keyframes);
        *project.drawer_mut(owner).unwrap().rack_mut(rack).unwrap() = unsound;

        let problems = project.problems_with_the_racks();

        assert_eq!(problems.len(), 1, "{name}: {problems:?}");
        assert!(
            problems[0].contains("not in the order of their frames, each at a frame of its own"),
            "{name}: {problems:?}"
        );
    }
}
