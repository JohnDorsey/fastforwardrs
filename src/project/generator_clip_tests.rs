// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the generator clip in the project (phase 5, piece 5.4): making one, what it is (no media, a colour that is a property like a text object's, a place on the picture,
//! a length that is set), what can be put on it, and how it is cut, moved, kept at another frame rate and refused where a rule of groups says so.

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

fn make_a_solid_colour(project: &mut Project, track: TrackId, first_frame: u64, wanted: u64) -> (ClipId, u64) {
    project
        .try_insert_generator_clip(track, frame(first_frame), GeneratorKind::SolidColour, wanted)
        .unwrap()
}

const RED: TextColour = TextColour::new(255, 0, 0, 255);
const BLUE_AND_HALF_CLEAR: TextColour = TextColour::new(0, 0, 255, 128);

#[test]
fn a_generator_clip_is_put_on_a_track_where_there_is_room_for_as_long_as_there_is_room() {
    let (mut project, track) = a_track_with_a_text_object();

    let (clip, lasts) = make_a_solid_colour(&mut project, track, 50, 150);

    assert_eq!(lasts, 150, "there is room");
    let placement = project.placement_of(clip).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Generator);
    assert_eq!(
        (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        ),
        (50, 200)
    );

    assert!(matches!(
        project.try_insert_generator_clip(track, frame(20), GeneratorKind::SolidColour, 150),
        Err(EditError::NoRoomThere)
    ));
    let (_, before_the_text) = make_a_solid_colour(&mut project, track, 0, 150);
    assert_eq!(before_the_text, 10, "it stops where the text object begins");
    assert!(matches!(
        project.try_insert_generator_clip(TrackId::new(99), frame(300), GeneratorKind::SolidColour, 10),
        Err(EditError::NoRoomThere)
    ));
}

#[test]
fn a_generator_clip_has_no_media_and_no_sound_and_is_the_whole_picture_until_it_is_put_somewhere() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    let found = project.placement_of(clip).unwrap().clip;

    assert!(!found.is_media());
    assert_eq!(found.source(), None);
    assert_eq!(found.placement(), Some(Placement::upright(CanvasArea::WHOLE_PICTURE)));
    assert_eq!(
        project.placement_on_the_picture(found),
        Some(Placement::upright(CanvasArea::WHOLE_PICTURE))
    );
    assert_eq!(found.duration_in_frames_it_says(), Some(30));
    assert_eq!(
        found.timeline_frame_count(SpeedFactor::new(4).unwrap()),
        30,
        "the speed of its track does not change how long it lasts"
    );
    assert_eq!(found.as_generator().unwrap().kind(), GeneratorKind::SolidColour);
}

#[test]
fn a_generator_clip_is_put_somewhere_on_the_picture_like_a_text_object_and_nothing_else_about_it_changes() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    project.set_generator_colour(clip, RED).unwrap();
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(20.0));

    project.set_clip_placement(clip, somewhere).unwrap();

    let found = project.placement_of(clip).unwrap().clip;
    assert_eq!(found.placement(), Some(somewhere));
    assert_eq!(found.as_generator().unwrap().colour, RED);
    assert_eq!(found.duration_in_frames_it_says(), Some(30));
    assert_eq!(project.placement_at(clip, frame(60)), Some(somewhere));
}

#[test]
fn the_colour_of_a_generator_clip_is_a_property_that_has_a_value_and_can_be_changed_and_animated() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 60);
    let target = AnimationTarget::Clip(clip);
    assert_eq!(
        project.property_value_at(target, PropertyId::Colour, frame(70)),
        Some(PropertyValue::Colour(TextColour::new(128, 128, 128, 255))),
        "it begins as a middle grey"
    );

    // Changed while it has no keyframes: what the clip says.
    project.set_generator_colour(clip, RED).unwrap();
    assert_eq!(project.generator_colour_at(clip, frame(70)), Some(RED));

    // Animated: red at the start, blue and half clear 30 frames later, and between them the colour changes evenly in each of its parts.
    project.add_keyframe(target, PropertyId::Colour, frame(50)).unwrap();
    project.add_keyframe(target, PropertyId::Colour, frame(80)).unwrap();
    project
        .set_property_at(target, PropertyId::Colour, PropertyValue::Colour(RED), frame(50))
        .unwrap();
    project
        .set_property_at(
            target,
            PropertyId::Colour,
            PropertyValue::Colour(BLUE_AND_HALF_CLEAR),
            frame(80),
        )
        .unwrap();
    assert_eq!(project.generator_colour_at(clip, frame(50)), Some(RED));
    assert_eq!(project.generator_colour_at(clip, frame(80)), Some(BLUE_AND_HALF_CLEAR));
    assert_eq!(
        project.generator_colour_at(clip, frame(65)),
        Some(TextColour::new(128, 0, 128, 192)),
        "half way"
    );
    assert_eq!(
        project.generator_colour_at(clip, frame(0)),
        Some(RED),
        "a frame before it is taken for its first"
    );
    assert_eq!(
        project.generator_colour_at(clip, frame(500)),
        Some(BLUE_AND_HALF_CLEAR),
        "and one after it for its last"
    );
    assert_eq!(
        project.property_value_at(target, PropertyId::Colour, frame(65)),
        Some(PropertyValue::Colour(TextColour::new(128, 0, 128, 192)))
    );
}

#[test]
fn only_a_text_object_and_a_generator_clip_have_a_colour_and_only_those_that_are_on_the_picture_have_a_place() {
    for kind in [ClipKind::Text, ClipKind::Generator] {
        assert!(PropertyId::Colour.applies_to_a_clip_of_kind(kind), "{kind:?}");
    }
    for kind in [
        ClipKind::Video,
        ClipKind::Audio,
        ClipKind::Image,
        ClipKind::Crate,
        ClipKind::Adjustment,
    ] {
        assert!(!PropertyId::Colour.applies_to_a_clip_of_kind(kind), "{kind:?}");
    }
    for property in [PropertyId::Position, PropertyId::Size, PropertyId::Rotation] {
        assert!(property.applies_to_a_clip_of_kind(ClipKind::Generator), "{property:?}");
    }
    // The words and the letters of a text object are not those of a generator.
    for property in [PropertyId::Text, PropertyId::FontFamily, PropertyId::FontSize] {
        assert!(!property.applies_to_a_clip_of_kind(ClipKind::Generator), "{property:?}");
    }
    for property in [PropertyId::Gain, PropertyId::Muted] {
        assert!(!property.applies_to_a_clip_of_kind(ClipKind::Generator), "{property:?}");
    }
}

#[test]
fn the_colour_of_a_clip_that_is_not_a_generator_cannot_be_set_as_one() {
    let (mut project, track) = a_track_with_a_text_object();
    let text = project.tracks()[0].placement_at_index(0).unwrap().id;
    let _ = track;

    let refused = project.set_generator_colour(text, RED);

    assert!(
        matches!(
            refused,
            Err(EditError::Clip(ClipEditError::NotAGeneratorClip { clip })) if clip == text
        ),
        "{refused:?}"
    );
}

#[test]
fn a_picture_effect_can_be_put_on_a_generator_clip_and_a_sound_effect_cannot() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    let effect = project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::BLUR)
        .unwrap();
    assert_eq!(project.effects_of(AnimationTarget::Clip(clip)).unwrap().len(), 2);

    let mut sound =
        crate::test_support::the_descriptor_of_an_effect_the_program_does_not_have("ladspa:1/a_sound_effect", &[]);
    sound.domain = crate::project::EffectDomain::Sound;
    let sound = EffectKind::from_snapshot(sound).unwrap();
    assert!(!sound.applies_to_a_clip_of_kind(ClipKind::Generator));
    assert!(project.add_effect(AnimationTarget::Clip(clip), sound).is_err());

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
}

#[test]
fn a_generator_clip_lasts_as_long_as_it_is_set_to_and_not_into_the_next_clip_and_is_cut_in_two_with_the_colour_and_place_kept()
 {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    project.set_generator_colour(clip, RED).unwrap();
    let somewhere = Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5));
    project.set_clip_placement(clip, somewhere).unwrap();

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
    let times: Vec<(u64, u64)> = track_now
        .placements()
        .map(|placement| {
            (
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value(),
            )
        })
        .collect();
    assert_eq!(times, [(10, 40), (50, 80), (80, 150)]);
    for placement in track_now.placements().skip(1) {
        let generator = placement.clip.as_generator().expect("two generators");
        assert_eq!((generator.colour, generator.placement()), (RED, somewhere));
    }
}

#[test]
fn a_generator_clip_is_counted_again_at_another_frame_rate_and_lasts_as_long() {
    let (mut project, track) = a_track_with_a_text_object();
    project.set_frame_rate(frame_rate(30));
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    project.set_generator_colour(clip, RED).unwrap();

    project.set_frame_rate(frame_rate(60));

    let placement = project.placement_of(clip).unwrap();
    assert_eq!(placement.clip.duration_in_frames_it_says(), Some(60));
    assert_eq!(placement.first_timeline_frame.value(), 100);
    assert_eq!(placement.clip.as_generator().unwrap().colour, RED);
}

#[test]
fn a_generator_clip_is_not_put_where_it_would_be_inside_the_rectangle_of_a_clip_group() {
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

    let refused = project.try_insert_generator_clip(front, frame(30), GeneratorKind::SolidColour, 5);

    assert!(
        matches!(refused, Err(EditError::BreaksTheRulesOfGroups { .. })),
        "{refused:?}"
    );
    assert_eq!(project, before, "nothing was changed");
    assert!(
        project
            .try_insert_generator_clip(front, frame(50), GeneratorKind::SolidColour, 5)
            .is_ok(),
        "past the rectangle it is made"
    );
}

#[test]
fn the_effects_of_a_generator_clip_are_among_those_applied_to_it_and_to_no_other_clip() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    let text = project.tracks()[0].placement_at_index(0).unwrap().id;
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 40.0);

    assert_eq!(project.effects_applied_to_the_clip(clip).len(), 1);
    assert!(project.effects_applied_to_the_clip(text).is_empty());
}

#[test]
fn a_copy_of_a_generator_clip_has_its_colour_and_place_and_effects_and_the_keyframes_of_its_colour() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(20.0));
    project.set_generator_colour(clip, RED).unwrap();
    project.set_clip_placement(clip, somewhere).unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 40.0);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Colour,
        50,
        PropertyValue::Colour(BLUE_AND_HALF_CLEAR),
    );

    let copy = project.duplicate_clip(clip, ClipSide::Right).unwrap();

    let copied = project.placement_of(copy).unwrap();
    assert_eq!(copied.first_timeline_frame.value(), 80, "right after it");
    let generator = copied.clip.as_generator().expect("it is a generator too");
    assert_eq!((generator.colour, generator.placement()), (RED, somewhere));
    assert_eq!(project.effects_of(AnimationTarget::Clip(copy)).unwrap().len(), 1);
    assert!(
        project.is_animated(AnimationTarget::Clip(copy), PropertyId::Colour),
        "the keyframes of its colour are the copy's own"
    );
    assert_eq!(
        project.generator_colour_at(copy, frame(80)),
        Some(BLUE_AND_HALF_CLEAR),
        "its first frame is as the first frame of the clip was"
    );
}

#[test]
fn the_colour_of_a_generator_clip_that_has_no_keyframes_is_changed_as_any_property_is_at_a_frame() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_solid_colour(&mut project, track, 50, 30);
    let target = AnimationTarget::Clip(clip);

    let made = project
        .set_property_at(target, PropertyId::Colour, PropertyValue::Colour(RED), frame(60))
        .unwrap();

    assert!(made.is_none(), "no keyframe is made when there are none");
    assert!(!project.is_animated(target, PropertyId::Colour));
    assert_eq!(project.generator_colour_at(clip, frame(50)), Some(RED));
    assert_eq!(project.generator_colour_at(clip, frame(79)), Some(RED));
    // (A colour is not a property of a clip that is not a text or a generator.)
    let text = project.tracks()[0].placement_at_index(0).unwrap().id;
    assert!(
        project
            .set_property_at(
                AnimationTarget::Clip(text),
                PropertyId::Gain,
                PropertyValue::Number(0.0),
                frame(20)
            )
            .is_err()
    );
}
