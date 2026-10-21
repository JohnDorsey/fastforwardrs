// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the adjustment clip in the project (phase 5, piece 5.3): making one, what it is (no media, no place on the picture, a length that is set), what can be put on it, and
//! how it is cut, moved, kept at another frame rate and refused where a rule of groups says so.

use super::tests::{frame, frame_rate};
use super::*;
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

/// A project with one track that has a text object from frame 10 to 40, and the track.
fn a_track_with_a_text_object() -> (Project, TrackId) {
    let mut project = Project::empty();
    let track = project.add_track_in_front();
    project
        .try_insert_text(track, frame(10), TextObject::default(), 30)
        .unwrap();
    (project, track)
}

#[test]
fn an_adjustment_clip_is_put_on_a_track_where_there_is_room_for_as_long_as_there_is_room() {
    let (mut project, track) = a_track_with_a_text_object();

    let (clip, lasts) = project.try_insert_adjustment_clip(track, frame(50), 150).unwrap();

    assert_eq!(lasts, 150, "there is room");
    let placement = project.placement_of(clip).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Adjustment);
    assert_eq!(
        (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        ),
        (50, 200)
    );

    // Where the text object is, there is no room; before it, there is room for as long as it is to the text object.
    assert!(matches!(
        project.try_insert_adjustment_clip(track, frame(20), 150),
        Err(EditError::NoRoomThere)
    ));
    let (_, before_the_text) = project.try_insert_adjustment_clip(track, frame(0), 150).unwrap();
    assert_eq!(before_the_text, 10, "it stops where the text object begins");
    assert!(matches!(
        project.try_insert_adjustment_clip(TrackId::new(99), frame(300), 10),
        Err(EditError::NoRoomThere)
    ));
}

#[test]
fn an_adjustment_clip_has_no_media_no_sound_and_no_place_on_the_picture() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = project.try_insert_adjustment_clip(track, frame(50), 30).unwrap();
    let clip_found = project.placement_of(clip).unwrap().clip;

    assert!(!clip_found.is_media());
    assert_eq!(clip_found.source(), None);
    assert_eq!(clip_found.placement(), None);
    assert_eq!(
        project.placement_on_the_picture(clip_found),
        None,
        "it is the whole picture and has no place of its own"
    );
    assert_eq!(clip_found.duration_in_frames_it_says(), Some(30));
    assert_eq!(
        clip_found.timeline_frame_count(SpeedFactor::new(4).unwrap()),
        30,
        "the speed of its track does not change how long it lasts"
    );
    assert!(
        project
            .set_clip_placement(clip, Placement::upright(CanvasArea::WHOLE_PICTURE))
            .is_err()
    );
}

#[test]
fn only_a_picture_effect_can_be_put_on_an_adjustment_clip() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = project.try_insert_adjustment_clip(track, frame(50), 30).unwrap();

    let effect = project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::BLUR)
        .unwrap();
    assert_eq!(project.effects_of(AnimationTarget::Clip(clip)).unwrap().len(), 2);

    // A sound effect (a made-up one: the program has no sound effect of its own) is not for a clip that has nothing to hear.
    let mut sound =
        crate::test_support::the_descriptor_of_an_effect_the_program_does_not_have("ladspa:1/a_sound_effect", &[]);
    sound.domain = crate::project::EffectDomain::Sound;
    let sound = EffectKind::from_snapshot(sound).unwrap();
    assert!(!sound.applies_to_a_clip_of_kind(ClipKind::Adjustment));
    assert!(project.add_effect(AnimationTarget::Clip(clip), sound).is_err());

    // The values of an effect are animated like those of any clip's.
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::effect_value(EffectKind::HUE_SHIFT, effect, 0),
        50,
        PropertyValue::Number(0.0),
    );
    assert!(project.is_animated(
        AnimationTarget::Clip(clip),
        PropertyId::effect_value(EffectKind::HUE_SHIFT, effect, 0)
    ));
    // There is nothing of an adjustment clip that is on the picture to animate: where it is has no keyframes.
    assert!(
        !PropertyId::Position.applies_to_a_clip_of_kind(ClipKind::Adjustment)
            && !PropertyId::Size.applies_to_a_clip_of_kind(ClipKind::Adjustment)
            && !PropertyId::Rotation.applies_to_a_clip_of_kind(ClipKind::Adjustment)
    );
}

#[test]
fn an_adjustment_clip_lasts_as_long_as_it_is_set_to_and_not_into_the_next_clip_and_is_cut_in_two() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = project.try_insert_adjustment_clip(track, frame(50), 30).unwrap();

    assert_eq!(project.set_clip_duration(clip, 100).unwrap(), 100);
    assert_eq!(
        project.set_clip_duration(clip, 0).unwrap(),
        1,
        "it lasts at least a frame"
    );
    assert_eq!(project.set_clip_duration(clip, 100).unwrap(), 100);

    let cut = project.split_at(frame(80), Some(&[clip]));
    assert_eq!(cut.len(), 1);
    let track_now = &project.tracks()[0];
    let durations: Vec<(u64, u64)> = track_now
        .placements()
        .map(|placement| {
            (
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value(),
            )
        })
        .collect();
    assert_eq!(
        durations,
        [(10, 40), (50, 80), (80, 150)],
        "two adjustment clips that last as long as the one did"
    );
    assert!(
        track_now
            .placements()
            .skip(1)
            .all(|placement| placement.clip.kind() == ClipKind::Adjustment)
    );
}

#[test]
fn an_adjustment_clip_is_counted_again_at_another_frame_rate_and_lasts_as_long() {
    let (mut project, track) = a_track_with_a_text_object();
    project.set_frame_rate(frame_rate(30));
    let (clip, _) = project.try_insert_adjustment_clip(track, frame(50), 30).unwrap();

    project.set_frame_rate(frame_rate(60));

    let placement = project.placement_of(clip).unwrap();
    assert_eq!(
        placement.clip.duration_in_frames_it_says(),
        Some(60),
        "a second is 60 frames now"
    );
    assert_eq!(placement.first_timeline_frame.value(), 100);
}

#[test]
fn an_adjustment_clip_is_not_put_where_it_would_be_inside_the_rectangle_of_a_clip_group() {
    // Two text objects on two tracks, in a clip group: the rectangle is frames 10 to 40 across both tracks.
    let mut project = Project::empty();
    let back = project.add_track_in_front();
    let front = project.add_track_in_front();
    let (a, _) = project
        .try_insert_text(back, frame(10), TextObject::default(), 30)
        .unwrap();
    let (b, _) = project
        .try_insert_text(front, frame(20), TextObject::default(), 10)
        .unwrap();
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();

    let refused = project.try_insert_adjustment_clip(front, frame(30), 5);

    assert!(
        matches!(refused, Err(EditError::BreaksTheRulesOfGroups { .. })),
        "{refused:?}"
    );
    assert_eq!(project, before, "nothing was changed");
    assert!(
        project.try_insert_adjustment_clip(front, frame(50), 5).is_ok(),
        "past the rectangle it is made"
    );
}

#[test]
fn the_effects_of_an_adjustment_clip_are_among_those_applied_to_it_and_to_no_other_clip() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = project.try_insert_adjustment_clip(track, frame(50), 30).unwrap();
    let text = project.tracks()[0].placement_at_index(0).unwrap().id;
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 40.0);

    assert_eq!(project.effects_applied_to_the_clip(clip).len(), 1);
    assert!(project.effects_applied_to_the_clip(text).is_empty());
}
