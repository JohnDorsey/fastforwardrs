//! Tests of the effects of a track and of the clips on it: which can have which, putting them on and taking them off, their values,
//! and that they stay with a clip when it is cut, copied and moved.

use super::tests::{THE_TRACK, clip_id, frame, picture, sound, text, track_of, video};
use super::*;
use crate::project::tracks::ids::TrackId;
use crate::project::{Effect, EffectKind, Effects, PropertyId, PropertyValue};

const HUE: PropertyId = PropertyId::HueShift;

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

/// A track with, one after the other and with no empty frames between them, a video of 100 frames (clip 0), a sound of 50 (clip 1), a
/// picture of 40 (clip 2) and a text object of 30 (clip 3).
fn track_with_a_clip_of_each_kind() -> (Track, [ClipId; 4]) {
    let track = track_of(vec![
        (0, video(0, 100, 1)),
        (0, sound(0, 50, 1)),
        (0, picture(40)),
        (0, text(30)),
    ]);
    (track, [clip_id(0), clip_id(1), clip_id(2), clip_id(3)])
}

fn effects_of(track: &Track, clip: ClipId) -> &Effects {
    track.placement_of(clip).expect("the clip is on the track").effects
}

fn the_hue_shift_of(track: &Track, clip: ClipId) -> Option<PropertyValue> {
    effects_of(track, clip).value_of(HUE)
}

// --- putting them on and taking them off ---

#[test]
fn a_track_and_its_clips_have_no_effects_when_they_are_made() {
    let (track, clips) = track_with_a_clip_of_each_kind();
    assert!(track.effects().is_empty());
    for clip in clips {
        assert!(effects_of(&track, clip).is_empty(), "{clip}");
        assert!(track.placement_of(clip).unwrap().effects.is_empty());
    }
}

#[test]
fn a_track_has_an_effect_once_and_has_it_as_it_begins_and_takes_it_off() {
    let mut track = Track::new(THE_TRACK, "track 0");

    assert!(track.add_effect(EffectKind::HueShift));
    assert!(track.effects().contains(EffectKind::HueShift));
    assert_eq!(track.effects().value_of(HUE), Some(turn(0.0)));
    assert!(!track.add_effect(EffectKind::HueShift), "it has it already");
    assert_eq!(track.effects().len(), 1);

    assert!(track.remove_effect(EffectKind::HueShift));
    assert!(track.effects().is_empty());
    assert!(!track.remove_effect(EffectKind::HueShift), "it did not have it");
}

#[test]
fn a_clip_that_is_seen_can_have_an_effect_and_a_sound_cannot_and_nothing_is_changed_when_it_cannot() {
    let (mut track, [video_clip, sound_clip, picture_clip, text_clip]) = track_with_a_clip_of_each_kind();

    for seen in [video_clip, picture_clip, text_clip] {
        assert_eq!(
            track.add_effect_to_the_clip(seen, EffectKind::HueShift),
            Ok(true),
            "{seen}"
        );
        assert_eq!(the_hue_shift_of(&track, seen), Some(turn(0.0)), "{seen}");
        assert_eq!(
            track.add_effect_to_the_clip(seen, EffectKind::HueShift),
            Ok(false),
            "{seen} has it already"
        );
    }

    assert_eq!(
        track.add_effect_to_the_clip(sound_clip, EffectKind::HueShift),
        Err(ClipEditError::NotAnEffectOfThisClip {
            clip: sound_clip,
            kind: EffectKind::HueShift
        })
    );
    assert!(effects_of(&track, sound_clip).is_empty());
    assert!(track.effects().is_empty(), "the track has none of its own");
}

#[test]
fn an_effect_is_taken_off_one_clip_and_the_others_keep_theirs() {
    let (mut track, [video_clip, _, picture_clip, text_clip]) = track_with_a_clip_of_each_kind();
    for clip in [video_clip, picture_clip, text_clip] {
        track.add_effect_to_the_clip(clip, EffectKind::HueShift).unwrap();
    }

    assert_eq!(
        track.remove_effect_from_the_clip(picture_clip, EffectKind::HueShift),
        Ok(true)
    );
    assert_eq!(
        track.remove_effect_from_the_clip(picture_clip, EffectKind::HueShift),
        Ok(false),
        "it was not there to take off"
    );

    assert!(effects_of(&track, picture_clip).is_empty());
    assert!(effects_of(&track, video_clip).contains(EffectKind::HueShift));
    assert!(effects_of(&track, text_clip).contains(EffectKind::HueShift));
}

#[test]
fn a_clip_that_is_not_there_is_named_in_the_error_of_each_edit_of_effects() {
    let (mut track, _) = track_with_a_clip_of_each_kind();
    let not_there = ClipId::new(TrackId::new(7), 3);
    let error = ClipEditError::UnknownClip { clip: not_there };

    assert_eq!(
        track.add_effect_to_the_clip(not_there, EffectKind::HueShift),
        Err(error)
    );
    assert_eq!(
        track.remove_effect_from_the_clip(not_there, EffectKind::HueShift),
        Err(error)
    );
    assert_eq!(
        track.set_effect_value_of_the_clip(not_there, HUE, &turn(10.0)),
        Err(error)
    );
}

// --- values ---

#[test]
fn the_value_of_the_effect_of_a_track_is_changed_when_it_has_the_effect_and_is_the_kind_of_value_it_has() {
    let mut track = Track::new(THE_TRACK, "track 0");
    assert!(!track.set_effect_value(HUE, &turn(30.0)), "it does not have the effect");

    track.add_effect(EffectKind::HueShift);
    assert!(track.set_effect_value(HUE, &turn(30.0)));
    assert_eq!(track.effects().value_of(HUE), Some(turn(30.0)));

    assert!(
        !track.set_effect_value(HUE, &PropertyValue::Flag(true)),
        "not that kind of value"
    );
    assert!(
        !track.set_effect_value(PropertyId::Gain, &turn(1.0)),
        "not the value of an effect"
    );
    assert_eq!(track.effects().value_of(HUE), Some(turn(30.0)), "as it was");
}

#[test]
fn the_value_of_the_effect_of_a_clip_is_changed_for_that_clip_only_and_a_wrong_change_is_an_error_that_changes_nothing()
{
    let (mut track, [video_clip, _, picture_clip, _]) = track_with_a_clip_of_each_kind();
    track.add_effect_to_the_clip(video_clip, EffectKind::HueShift).unwrap();
    track
        .add_effect_to_the_clip(picture_clip, EffectKind::HueShift)
        .unwrap();

    track
        .set_effect_value_of_the_clip(video_clip, HUE, &turn(-45.0))
        .unwrap();

    assert_eq!(the_hue_shift_of(&track, video_clip), Some(turn(-45.0)));
    assert_eq!(
        the_hue_shift_of(&track, picture_clip),
        Some(turn(0.0)),
        "the other one is as it was"
    );
    let wrong = ClipEditError::NotAPropertyOfThisClip {
        clip: video_clip,
        property: HUE,
    };
    assert_eq!(
        track.set_effect_value_of_the_clip(video_clip, HUE, &PropertyValue::Flag(true)),
        Err(wrong)
    );
    assert_eq!(
        track.set_effect_value_of_the_clip(video_clip, PropertyId::Rotation, &turn(5.0)),
        Err(ClipEditError::NotAPropertyOfThisClip {
            clip: video_clip,
            property: PropertyId::Rotation
        }),
        "not the value of an effect"
    );
    assert_eq!(the_hue_shift_of(&track, video_clip), Some(turn(-45.0)));

    let text_clip_without_it = clip_id(3);
    assert_eq!(
        track.set_effect_value_of_the_clip(text_clip_without_it, HUE, &turn(5.0)),
        Err(ClipEditError::NotAPropertyOfThisClip {
            clip: text_clip_without_it,
            property: HUE
        }),
        "it does not have the effect"
    );
}

#[test]
fn keyframes_of_the_value_of_an_effect_are_only_made_for_what_has_the_effect() {
    let (mut track, [video_clip, _, picture_clip, _]) = track_with_a_clip_of_each_kind();
    track.add_effect_to_the_clip(video_clip, EffectKind::HueShift).unwrap();

    track.set_clip_keyframe(video_clip, HUE, 10, turn(90.0)).unwrap();
    assert_eq!(
        track.set_clip_keyframe(picture_clip, HUE, 10, turn(90.0)),
        Err(ClipEditError::NotAPropertyOfThisClip {
            clip: picture_clip,
            property: HUE
        }),
        "the picture does not have the effect"
    );
    assert!(track.placement_of(picture_clip).unwrap().drawer.is_empty());

    assert!(
        !track.set_own_keyframe(HUE, frame(5), turn(90.0)),
        "the track does not have the effect"
    );
    assert!(track.drawer().is_empty());
    track.add_effect(EffectKind::HueShift);
    assert!(track.set_own_keyframe(HUE, frame(5), turn(90.0)));
    assert_eq!(track.drawer().len(), 1);
}

// --- making a track from its parts ---

#[test]
fn a_track_made_from_its_parts_has_the_effects_it_is_given_as_they_are_and_so_has_each_clip() {
    let (track, [video_clip, sound_clip, picture_clip, text_clip]) = track_with_a_clip_of_each_kind();
    let turned_by = |degrees: f32| {
        let mut effects = Effects::none();
        effects.put_on(Effect::with_values(EffectKind::HueShift, vec![turn(degrees)]).unwrap());
        effects
    };

    let track = track.with_effects(turned_by(15.0)).with_effects_on_its_clips([
        turned_by(30.0),
        Effects::none(),
        turned_by(60.0),
    ]);

    assert_eq!(track.effects().value_of(HUE), Some(turn(15.0)));
    assert_eq!(the_hue_shift_of(&track, video_clip), Some(turn(30.0)));
    assert_eq!(the_hue_shift_of(&track, sound_clip), None);
    assert_eq!(the_hue_shift_of(&track, picture_clip), Some(turn(60.0)));
    assert_eq!(
        the_hue_shift_of(&track, text_clip),
        None,
        "there were not as many sets as there are clips: the rest have none"
    );
}

// --- what is kept when a clip is cut, copied and moved ---

#[test]
fn both_halves_of_a_cut_clip_have_the_effects_and_the_values_the_clip_had() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    track.add_effect_to_the_clip(clip_id(0), EffectKind::HueShift).unwrap();
    track
        .set_effect_value_of_the_clip(clip_id(0), HUE, &turn(75.0))
        .unwrap();

    let cut = track.split_at(frame(40)).unwrap();

    for half in [cut.first_half, cut.second_half] {
        assert_eq!(the_hue_shift_of(&track, half), Some(turn(75.0)), "{half}");
    }
    assert_ne!(cut.first_half, cut.second_half);
    assert!(track.effects().is_empty(), "the track got none by it");
}

#[test]
fn a_copy_of_a_clip_has_its_effects_and_changing_the_copy_does_not_change_the_clip() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    track.add_effect_to_the_clip(clip_id(0), EffectKind::HueShift).unwrap();
    track
        .set_effect_value_of_the_clip(clip_id(0), HUE, &turn(75.0))
        .unwrap();

    let copy = track.insert_a_copy_of_the_clip_at(clip_id(0), frame(100)).unwrap();

    assert_eq!(the_hue_shift_of(&track, copy), Some(turn(75.0)));
    track.set_effect_value_of_the_clip(copy, HUE, &turn(-10.0)).unwrap();
    assert_eq!(
        the_hue_shift_of(&track, clip_id(0)),
        Some(turn(75.0)),
        "the clip is as it was"
    );
    assert_eq!(the_hue_shift_of(&track, copy), Some(turn(-10.0)));
}

#[test]
fn a_copy_of_a_track_has_its_effects_and_those_of_each_of_its_clips_which_it_changes_alone() {
    let (mut track, [video_clip, sound_clip, ..]) = track_with_a_clip_of_each_kind();
    track.add_effect(EffectKind::HueShift);
    track.set_effect_value(HUE, &turn(15.0));
    track.add_effect_to_the_clip(video_clip, EffectKind::HueShift).unwrap();
    track
        .set_effect_value_of_the_clip(video_clip, HUE, &turn(30.0))
        .unwrap();

    let mut copy = track.copy_as(TrackId::new(2), "a copy".to_owned());

    assert_eq!(copy.effects().value_of(HUE), Some(turn(15.0)));
    let copied_video = copy.entries()[0].id();
    let copied_sound = copy.entries()[1].id();
    assert_eq!(the_hue_shift_of(&copy, copied_video), Some(turn(30.0)));
    assert_eq!(the_hue_shift_of(&copy, copied_sound), None);
    copy.set_effect_value(HUE, &turn(99.0));
    copy.set_effect_value_of_the_clip(copied_video, HUE, &turn(98.0))
        .unwrap();
    assert_eq!(
        track.effects().value_of(HUE),
        Some(turn(15.0)),
        "the track is as it was"
    );
    assert_eq!(the_hue_shift_of(&track, video_clip), Some(turn(30.0)));
    assert!(effects_of(&track, sound_clip).is_empty());
}

#[test]
fn clips_that_are_moved_along_a_track_keep_their_effects_each_with_its_own() {
    let (mut track, [video_clip, sound_clip, picture_clip, text_clip]) = track_with_a_clip_of_each_kind();
    track.add_effect_to_the_clip(video_clip, EffectKind::HueShift).unwrap();
    track
        .set_effect_value_of_the_clip(video_clip, HUE, &turn(30.0))
        .unwrap();
    track.add_effect_to_the_clip(text_clip, EffectKind::HueShift).unwrap();
    track
        .set_effect_value_of_the_clip(text_clip, HUE, &turn(-30.0))
        .unwrap();

    // The text object goes to the front of the track, before everything, and the picture is moved away to the end.
    track
        .move_clips(&[(text_clip, 0), (video_clip, 30), (sound_clip, 130), (picture_clip, 180)])
        .unwrap();

    let order: Vec<ClipId> = track.entries().iter().map(TrackEntry::id).collect();
    assert_eq!(order, vec![text_clip, video_clip, sound_clip, picture_clip]);
    assert_eq!(the_hue_shift_of(&track, text_clip), Some(turn(-30.0)));
    assert_eq!(the_hue_shift_of(&track, video_clip), Some(turn(30.0)));
    assert_eq!(the_hue_shift_of(&track, sound_clip), None);
    assert_eq!(the_hue_shift_of(&track, picture_clip), None);
}

#[test]
fn a_clip_made_by_putting_one_in_a_track_begins_with_no_effects() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    let inserted = track.insert_clip_at(frame(200), picture(10)).unwrap();
    assert!(effects_of(&track, inserted).is_empty());
}
