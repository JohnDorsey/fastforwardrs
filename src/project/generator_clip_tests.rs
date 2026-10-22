// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the generator clip in the project (phase 6, piece 6.5): making one (what it makes is its first effect), what it is (no media, somewhere on the picture, a length that is
//! set), that the effect that it makes its picture with is not taken off, switched off, copied or moved, and how it is cut, moved, kept at another frame rate and refused where a rule
//! of groups says so.

use super::tests::{frame, frame_rate};
use super::*;
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

const SOLID: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"colour\", \"TYPE\": \"color\", \"DEFAULT\": [0.2, 0.4, 0.9, 1.0] } ] }*/\nvoid main() { gl_FragColor = colour; }\n";
const A_FILTER: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn kind_of(name: &str, source: &str) -> EffectKind {
    let file = crate::isf::read(source).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, name, source)).unwrap()
}

fn a_generator() -> EffectKind {
    kind_of("solid.fs", SOLID)
}

/// A project with one track that has a text object from frame 10 to 40, and the track.
fn a_track_with_a_text_object() -> (Project, TrackId) {
    let mut project = Project::empty();
    let track = project.add_track_in_front();
    project
        .try_insert_text(track, frame(10), TextObject::default(), 30)
        .unwrap();
    (project, track)
}

fn make_a_generator(project: &mut Project, track: TrackId, first_frame: u64, wanted: u64) -> (ClipId, u64) {
    project
        .try_insert_generator_clip(track, frame(first_frame), a_generator(), wanted)
        .unwrap()
}

fn the_content_of(project: &Project, clip: ClipId) -> EffectInstanceId {
    project
        .placement_of(clip)
        .unwrap()
        .clip
        .as_generator()
        .unwrap()
        .content()
}

#[test]
fn a_generator_clip_is_put_on_a_track_where_there_is_room_for_as_long_as_there_is_room() {
    let (mut project, track) = a_track_with_a_text_object();

    let (clip, lasts) = make_a_generator(&mut project, track, 50, 150);

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
        project.try_insert_generator_clip(track, frame(20), a_generator(), 150),
        Err(EditError::NoRoomThere)
    ));
    let (_, before_the_text) = make_a_generator(&mut project, track, 0, 150);
    assert_eq!(before_the_text, 10, "it stops where the text object begins");
    assert!(matches!(
        project.try_insert_generator_clip(TrackId::new(99), frame(300), a_generator(), 10),
        Err(EditError::NoRoomThere)
    ));
}

#[test]
fn what_a_generator_makes_is_its_first_effect_which_is_of_the_kind_given_and_has_the_values_it_begins_with() {
    let (mut project, track) = a_track_with_a_text_object();

    let (clip, _) = make_a_generator(&mut project, track, 50, 30);

    let content = the_content_of(&project, clip);
    assert_eq!(content, EffectInstanceId::FIRST);
    let effects = project.effects_of(AnimationTarget::Clip(clip)).unwrap();
    assert_eq!(effects.len(), 1);
    let effect = effects.instance(content).unwrap();
    assert_eq!(effect.kind(), a_generator());
    assert_eq!(effect.values(), a_generator().default_values().as_slice());
}

#[test]
fn a_kind_that_a_clip_cannot_have_cannot_be_what_a_generator_makes_and_nothing_is_left_behind() {
    let (mut project, track) = a_track_with_a_text_object();
    let mut sound =
        crate::test_support::the_descriptor_of_an_effect_the_program_does_not_have("ladspa:1/a_sound_effect", &[]);
    sound.domain = crate::project::EffectDomain::Sound;
    let sound = EffectKind::from_snapshot(sound).unwrap();
    let before = project.clone();

    let refused = project.try_insert_generator_clip(track, frame(50), sound, 30);

    assert!(matches!(refused, Err(EditError::NoRoomThere)), "{refused:?}");
    assert_eq!(project, before, "no clip was put on the track");
}

#[test]
fn a_generator_clip_has_no_media_and_no_sound_and_is_the_whole_picture_until_it_is_put_somewhere() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 30);
    let found = project.placement_of(clip).unwrap().clip;

    assert!(!found.is_media());
    assert_eq!(found.source(), None);
    assert_eq!(found.placement(), Some(Placement::upright(CanvasArea::WHOLE_PICTURE)));
    assert_eq!(project.placement_on_the_picture(found), found.placement());
    assert_eq!(found.duration_in_frames_it_says(), Some(30));
    assert_eq!(
        found.timeline_frame_count(SpeedFactor::new(4).unwrap()),
        30,
        "the speed of its track does not change how long it lasts"
    );
}

#[test]
fn a_generator_clip_is_put_somewhere_on_the_picture_like_a_text_object_and_its_place_has_keyframes() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 30);
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(20.0));

    project.set_clip_placement(clip, somewhere).unwrap();

    assert_eq!(project.placement_of(clip).unwrap().clip.placement(), Some(somewhere));
    assert_eq!(project.placement_at(clip, frame(60)), Some(somewhere));
    for property in [PropertyId::Position, PropertyId::Size, PropertyId::Rotation] {
        assert!(property.applies_to_a_clip_of_kind(ClipKind::Generator), "{property:?}");
    }
    // It has no words, no letters, no colour and no loudness: those are for other clips.
    for property in [
        PropertyId::Text,
        PropertyId::FontFamily,
        PropertyId::FontSize,
        PropertyId::Colour,
        PropertyId::Gain,
        PropertyId::Muted,
    ] {
        assert!(!property.applies_to_a_clip_of_kind(ClipKind::Generator), "{property:?}");
    }
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        60,
        PropertyValue::Pair(0.7, 0.7),
    );
    assert!(project.is_animated(AnimationTarget::Clip(clip), PropertyId::Position));
}

#[test]
fn the_values_of_what_a_generator_makes_have_keyframes_like_the_values_of_any_effect() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 60);
    let target = AnimationTarget::Clip(clip);
    let colour = PropertyId::effect_value(a_generator(), the_content_of(&project, clip), 0);

    make_a_keyframe(
        &mut project,
        target,
        colour,
        50,
        PropertyValue::Colour(crate::project::TextColour::new(255, 0, 0, 255)),
    );
    make_a_keyframe(
        &mut project,
        target,
        colour,
        90,
        PropertyValue::Colour(crate::project::TextColour::new(0, 0, 255, 255)),
    );

    assert!(project.is_animated(target, colour));
    assert_eq!(
        project.property_value_at(target, colour, frame(70)),
        Some(PropertyValue::Colour(crate::project::TextColour::new(128, 0, 128, 255)))
    );
}

#[test]
fn what_a_generator_makes_is_not_taken_off_switched_off_copied_or_moved_and_the_other_effects_are() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 60);
    let content = the_content_of(&project, clip);
    let owner = Member::Clip(clip);
    let target = AnimationTarget::Clip(clip);
    let refused = |result: Result<(), EditError>| {
        assert!(
            matches!(result, Err(EditError::TheContentOfAGenerator { clip: named }) if named == clip),
            "{result:?}"
        );
    };
    let before = project.clone();

    refused(project.remove_effect(target, content));
    refused(project.set_effect_enabled(owner, content, false));
    refused(project.duplicate_effect(owner, content).map(|_| ()));
    refused(project.move_chain_item(owner, ChainItemKey::Effect(content), None));
    assert_eq!(project, before, "nothing was changed");

    // The effects that are put on it are its own, and are done after it: they are taken off, switched off, copied and moved.
    let turn = put_a_hue_shift_on(&mut project, target, 30.0);
    let blur = project.add_effect(target, EffectKind::BLUR).unwrap();
    project.set_effect_enabled(owner, turn, false).unwrap();
    project.set_effect_enabled(owner, turn, true).unwrap();
    let copy = project.duplicate_effect(owner, blur).unwrap();
    project
        .move_chain_item(owner, ChainItemKey::Effect(blur), Some(ChainItemKey::Effect(turn)))
        .unwrap();
    project.remove_effect(target, copy).unwrap();
    // ...but not in front of what it makes.
    refused(project.move_chain_item(owner, ChainItemKey::Effect(blur), Some(ChainItemKey::Effect(content))));
    let order: Vec<EffectInstanceId> = project
        .effects_of(target)
        .unwrap()
        .iter()
        .map(|effect| effect.instance())
        .collect();
    assert_eq!(order, [content, blur, turn], "what it makes stays the first");
}

#[test]
fn the_effects_applied_to_a_generator_begin_with_what_it_makes() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 60);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 30.0);

    let applied = project.effects_applied_to_the_clip(clip);

    assert_eq!(applied.len(), 2);
    assert_eq!(applied[0].effect.kind(), a_generator());
    assert_eq!(applied[1].effect.kind(), EffectKind::HUE_SHIFT);
}

#[test]
fn a_generator_clip_lasts_as_long_as_it_is_set_to_and_not_into_the_next_clip_and_is_cut_in_two_with_what_it_makes_in_both()
 {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 30);
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
        assert_eq!(generator.placement(), somewhere);
        assert_eq!(
            placement
                .effects
                .instance(generator.content())
                .map(|effect| effect.kind()),
            Some(a_generator()),
            "each half has what it makes"
        );
    }
}

#[test]
fn a_copy_of_a_generator_clip_makes_what_it_makes_in_the_same_place_with_the_same_effects() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 30);
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(20.0));
    project.set_clip_placement(clip, somewhere).unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 40.0);

    let copy = project.duplicate_clip(clip, ClipSide::Right).unwrap();

    let copied = project.placement_of(copy).unwrap();
    assert_eq!(copied.first_timeline_frame.value(), 80, "right after it");
    let generator = copied.clip.as_generator().expect("it is a generator too");
    assert_eq!(generator.placement(), somewhere);
    assert_eq!(project.effects_of(AnimationTarget::Clip(copy)).unwrap().len(), 2);
    assert_eq!(
        project.the_clip_whose_content_is(Member::Clip(copy), generator.content()),
        Some(copy),
        "and what it makes is protected there too"
    );
}

#[test]
fn a_generator_clip_is_counted_again_at_another_frame_rate_and_lasts_as_long() {
    let (mut project, track) = a_track_with_a_text_object();
    project.set_frame_rate(frame_rate(30));
    let (clip, _) = make_a_generator(&mut project, track, 50, 30);

    project.set_frame_rate(frame_rate(60));

    let placement = project.placement_of(clip).unwrap();
    assert_eq!(placement.clip.duration_in_frames_it_says(), Some(60));
    assert_eq!(placement.first_timeline_frame.value(), 100);
    assert_eq!(placement.effects.len(), 1, "what it makes is still there");
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

    let refused = project.try_insert_generator_clip(front, frame(30), a_generator(), 5);

    assert!(
        matches!(refused, Err(EditError::BreaksTheRulesOfGroups { .. })),
        "{refused:?}"
    );
    assert_eq!(project, before, "nothing was changed");
    assert!(
        project
            .try_insert_generator_clip(front, frame(50), a_generator(), 5)
            .is_ok(),
        "past the rectangle it is made"
    );
}

#[test]
fn a_filter_is_an_effect_of_a_generator_clip_as_of_any_clip_and_the_kinds_are_told_apart() {
    let (mut project, track) = a_track_with_a_text_object();
    let (clip, _) = make_a_generator(&mut project, track, 50, 30);
    let filter = kind_of("keep.fs", A_FILTER);

    assert!(project.add_effect(AnimationTarget::Clip(clip), filter).is_ok());
    assert!(a_generator().is_a_generator());
    assert!(!filter.is_a_generator());
    assert!(!EffectKind::HUE_SHIFT.is_a_generator());
}
