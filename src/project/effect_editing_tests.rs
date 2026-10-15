// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of putting effects on the clips and the tracks of a project and taking them off, of their values and keyframes, and of what
//! happens to them when a clip is cut, copied, moved or put back as it was.

use super::racks_tests::{assert_the_racks_are_sound, db, keyframe_frames, put_a_rack_in};
use super::tests::{audio_source, frame, picture_source, video_source};
use super::*;
use crate::test_support::make_a_keyframe;

const HUE: PropertyId = PropertyId::HueShift;

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

fn clip(id: ClipId) -> AnimationTarget {
    AnimationTarget::Clip(id)
}

fn track(id: TrackId) -> AnimationTarget {
    AnimationTarget::Track(id)
}

/// A project with one of each of the things there are to put effects on, each on a track of its own: a video of 600 frames, a sound of
/// 600 frames, a picture of 150 frames, and in front of them all a text object of 100 frames. Nothing has an effect.
struct OneOfEach {
    project: Project,
    video: ClipId,
    sound: ClipId,
    picture: ClipId,
    text: ClipId,
    video_track: TrackId,
    sound_track: TrackId,
    picture_track: TrackId,
}

/// A keyframe crate on `track`, beginning at frame 300, that is not empty (which a crate cannot stay): it has a rack that lowers the loudness
/// of `video`.
fn a_crate_holding_a_rack(project: &mut Project, track: TrackId, video: ClipId) -> ClipId {
    let a_crate = project.insert_crate(track, frame(300), 50).unwrap();
    put_a_rack_in(project, clip(a_crate), clip(video), PropertyId::Gain, &[(5, db(-6.0))]);
    a_crate
}

fn one_of_each() -> OneOfEach {
    let mut project = Project::empty();
    let video = project.import_source(video_source(1, 30, 600), frame(0)).unwrap();
    let sound = project.import_source(audio_source(2, 20), frame(0)).unwrap();
    let picture = project.import_source(picture_source(3, 640, 480), frame(0)).unwrap();
    let text_track = project.add_track_in_front();
    let (text, lasts) = project
        .insert_text(text_track, frame(0), TextObject::default(), 100)
        .unwrap();
    assert_eq!(lasts, 100);
    let made = OneOfEach {
        video: video.clip.unwrap(),
        sound: sound.clip.unwrap(),
        picture: picture.clip.unwrap(),
        text,
        video_track: video.track,
        sound_track: sound.track,
        picture_track: picture.track,
        project,
    };
    assert_eq!(
        made.project.placement_of(made.video).unwrap().timeline_frame_count(),
        600
    );
    assert_eq!(
        made.project.placement_of(made.sound).unwrap().timeline_frame_count(),
        600
    );
    assert_eq!(
        made.project.placement_of(made.picture).unwrap().timeline_frame_count(),
        150
    );
    made
}

/// Where the hue shift of `target` is at `at`, which is not animated for it to be anything but a turn that is what its keyframes say.
fn hue_at(project: &Project, target: AnimationTarget, at: u64) -> f32 {
    match project.property_value_at(target, HUE, frame(at)) {
        Some(PropertyValue::Number(degrees)) => degrees,
        other => panic!("not a hue shift: {other:?}"),
    }
}

fn racks_of_the_hue_shift_of(project: &Project, target: AnimationTarget) -> usize {
    project
        .located_racks()
        .filter(|located| located.rack.target() == target && located.rack.property() == HUE)
        .count()
}

// --- putting an effect on ---

#[test]
fn what_is_seen_and_each_track_can_have_a_hue_shift_which_begins_as_a_turn_of_nothing() {
    let OneOfEach {
        mut project,
        video,
        picture,
        text,
        video_track,
        sound_track,
        ..
    } = one_of_each();

    for target in [
        clip(video),
        clip(picture),
        clip(text),
        track(video_track),
        track(sound_track),
    ] {
        assert!(!project.has_the_effect(target, EffectKind::HueShift), "{target}");
        assert!(project.effects_of(target).unwrap().is_empty(), "{target}");
        assert_eq!(
            project.property_value_at(target, HUE, frame(10)),
            None,
            "{target}: a value of an effect is not a property of what does not have it"
        );

        project.add_effect(target, EffectKind::HueShift).unwrap();

        assert!(project.has_the_effect(target, EffectKind::HueShift), "{target}");
        assert_eq!(project.effects_of(target).unwrap().len(), 1, "{target}");
        assert_eq!(
            project.property_value_at(target, HUE, frame(10)),
            Some(turn(0.0)),
            "{target}"
        );
        assert!(!project.is_animated(target, HUE), "{target}");
    }
    assert_the_racks_are_sound(&project);
}

#[test]
fn an_effect_put_on_one_clip_is_not_on_any_other_clip_or_track() {
    let OneOfEach {
        mut project,
        video,
        picture,
        text,
        video_track,
        ..
    } = one_of_each();

    project.add_effect(clip(picture), EffectKind::HueShift).unwrap();

    for other in [clip(video), clip(text), track(video_track)] {
        assert!(!project.has_the_effect(other, EffectKind::HueShift), "{other}");
    }
}

#[test]
fn a_sound_and_a_keyframe_crate_cannot_have_an_effect_and_the_error_says_which_clip_it_was() {
    let OneOfEach {
        mut project,
        video,
        sound,
        picture_track,
        ..
    } = one_of_each();
    let a_crate = a_crate_holding_a_rack(&mut project, picture_track, video);

    for not_seen in [sound, a_crate] {
        assert_eq!(
            project.add_effect(clip(not_seen), EffectKind::HueShift),
            Err(EditError::Clip(ClipEditError::NotAnEffectOfThisClip {
                clip: not_seen,
                kind: EffectKind::HueShift
            })),
            "{not_seen}"
        );
        assert!(
            !project.has_the_effect(clip(not_seen), EffectKind::HueShift),
            "{not_seen}"
        );
        assert_eq!(
            project.property_value_at(clip(not_seen), HUE, frame(0)),
            None,
            "{not_seen}"
        );
    }
}

#[test]
fn an_effect_cannot_be_put_on_twice_and_the_values_it_has_are_not_what_the_second_time_would_begin_with() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    project.set_property_at(clip(video), HUE, turn(45.0), frame(0)).unwrap();

    assert_eq!(
        project.add_effect(clip(video), EffectKind::HueShift),
        Err(EditError::AlreadyHasThatEffect {
            target: clip(video),
            kind: EffectKind::HueShift
        })
    );

    assert_eq!(hue_at(&project, clip(video), 0), 45.0, "it is as it was");
    assert_eq!(project.effects_of(clip(video)).unwrap().len(), 1);
}

#[test]
fn an_effect_that_is_not_on_cannot_be_taken_off_and_neither_can_one_of_what_is_not_there() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    assert_eq!(
        project.remove_effect(clip(video), EffectKind::HueShift),
        Err(EditError::DoesNotHaveThatEffect {
            target: clip(video),
            kind: EffectKind::HueShift
        })
    );

    let no_such_clip = ClipId::new(TrackId::new(99), 0);
    let no_such_track = TrackId::new(99);
    assert_eq!(
        project.add_effect(clip(no_such_clip), EffectKind::HueShift),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: no_such_clip }))
    );
    assert_eq!(
        project.remove_effect(clip(no_such_clip), EffectKind::HueShift),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: no_such_clip }))
    );
    assert_eq!(
        project.add_effect(track(no_such_track), EffectKind::HueShift),
        Err(EditError::UnknownTrack { track: no_such_track })
    );
    assert_eq!(
        project.remove_effect(track(no_such_track), EffectKind::HueShift),
        Err(EditError::UnknownTrack { track: no_such_track })
    );
    assert_eq!(project.effects_of(clip(no_such_clip)), None);
    assert_eq!(project.effects_of(track(no_such_track)), None);
    assert!(!project.has_the_effect(clip(no_such_clip), EffectKind::HueShift));
}

/// What ties the effects to the table of them: whatever is in it can be put on exactly what its row says can have it, and its values
/// are then properties of that thing, each as it begins, and not before.
#[test]
fn every_effect_in_the_table_goes_on_what_it_says_can_have_it_and_its_values_are_the_properties_of_that_thing_from_then_on()
 {
    let mut base = one_of_each();
    let a_crate = a_crate_holding_a_rack(&mut base.project, base.picture_track, base.video);
    for kind in EffectKind::ALL {
        let cases = [
            (clip(base.video), kind.applies_to_a_clip_of_kind(ClipKind::Video)),
            (clip(base.sound), kind.applies_to_a_clip_of_kind(ClipKind::Audio)),
            (clip(base.picture), kind.applies_to_a_clip_of_kind(ClipKind::Image)),
            (clip(base.text), kind.applies_to_a_clip_of_kind(ClipKind::Text)),
            (clip(a_crate), kind.applies_to_a_clip_of_kind(ClipKind::Crate)),
            (track(base.video_track), kind.applies_to_a_track()),
        ];
        for (target, can_have_it) in cases {
            let mut project = base.project.clone();
            for property in kind.properties() {
                assert_eq!(
                    project.property_value_at(target, *property, frame(5)),
                    None,
                    "{kind:?} {target}"
                );
            }

            let result = project.add_effect(target, kind);

            assert_eq!(result.is_ok(), can_have_it, "{kind:?} on {target}: {result:?}");
            for (property, value) in kind.properties().iter().zip(kind.default_values()) {
                assert_eq!(
                    project.property_value_at(target, *property, frame(5)),
                    can_have_it.then_some(value),
                    "{property:?} of {target} with {kind:?} put on"
                );
            }
            assert_the_racks_are_sound(&project);
        }
    }
}

// --- values ---

#[test]
fn the_value_of_an_effect_is_changed_as_the_property_it_is_and_only_where_the_effect_is_on() {
    let OneOfEach {
        mut project,
        video,
        picture,
        video_track,
        ..
    } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    project.add_effect(track(video_track), EffectKind::HueShift).unwrap();

    let new_keyframe = project
        .set_property_at(clip(video), HUE, turn(120.0), frame(7))
        .unwrap();
    project
        .set_property_at(track(video_track), HUE, turn(-60.0), frame(7))
        .unwrap();

    assert_eq!(new_keyframe, None, "it is not animated, so no keyframe was made");
    assert_eq!(hue_at(&project, clip(video), 0), 120.0);
    assert_eq!(hue_at(&project, clip(video), 599), 120.0);
    assert_eq!(hue_at(&project, track(video_track), 300), -60.0);

    // Not where the effect is not on, and not a value of another kind.
    assert_eq!(
        project.set_property_at(clip(picture), HUE, turn(1.0), frame(0)),
        Err(EditError::Clip(ClipEditError::NotAPropertyOfThisClip {
            clip: picture,
            property: HUE
        }))
    );
    assert_eq!(
        project.set_property_at(clip(video), HUE, PropertyValue::Flag(true), frame(0)),
        Err(EditError::Clip(ClipEditError::NotAPropertyOfThisClip {
            clip: video,
            property: HUE
        }))
    );
    let other_track = project
        .tracks()
        .iter()
        .map(Track::id)
        .find(|id| *id != video_track)
        .unwrap();
    assert_eq!(
        project.set_property_at(track(other_track), HUE, turn(1.0), frame(0)),
        Err(EditError::NotAPropertyOfTheTrack {
            track: other_track,
            property: HUE
        })
    );
    assert_eq!(hue_at(&project, clip(video), 0), 120.0, "nothing was changed by those");
}

#[test]
fn the_value_of_an_effect_with_keyframes_goes_from_one_to_the_next_even_by_more_than_a_turn() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(-90.0));
    make_a_keyframe(&mut project, clip(video), HUE, 160, turn(450.0));

    assert!(project.is_animated(clip(video), HUE));
    assert_eq!(keyframe_frames(&project, clip(video), HUE), vec![100, 160]);
    assert_eq!(hue_at(&project, clip(video), 100), -90.0);
    assert_eq!(
        hue_at(&project, clip(video), 130),
        180.0,
        "half way from -90 to 450 is a half turn"
    );
    assert_eq!(hue_at(&project, clip(video), 160), 450.0);
    assert_eq!(
        hue_at(&project, clip(video), 0),
        -90.0,
        "before the first it is what the first says"
    );
    assert_eq!(
        hue_at(&project, clip(video), 599),
        450.0,
        "and after the last, what the last says"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn when_the_last_keyframe_of_an_effect_is_taken_away_it_stays_what_that_keyframe_said() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(30.0));

    project.remove_keyframe(clip(video), HUE, frame(100)).unwrap();

    assert!(!project.is_animated(clip(video), HUE));
    assert_eq!(hue_at(&project, clip(video), 0), 30.0);
    assert!(
        project.has_the_effect(clip(video), EffectKind::HueShift),
        "the effect is still on"
    );
}

#[test]
fn a_keyframe_cannot_be_made_for_the_value_of_an_effect_that_is_not_on() {
    let OneOfEach {
        mut project,
        video,
        video_track,
        ..
    } = one_of_each();

    assert!(project.add_keyframe(clip(video), HUE, frame(10)).is_err());
    assert!(project.add_keyframe(track(video_track), HUE, frame(10)).is_err());

    assert_eq!(project.keyframe_count(), 0);
    assert_the_racks_are_sound(&project);
}

// --- taking an effect off ---

#[test]
fn an_effect_that_is_taken_off_takes_the_keyframes_of_its_values_with_it_wherever_they_are_kept_and_nothing_else() {
    let OneOfEach {
        mut project,
        video,
        text,
        picture_track,
        ..
    } = one_of_each();
    for target in [video, text] {
        project.add_effect(clip(target), EffectKind::HueShift).unwrap();
    }
    // In the drawer of the video itself, and the loudness of the video, which is not the effect's.
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(0.0));
    make_a_keyframe(&mut project, clip(video), HUE, 200, turn(120.0));
    make_a_keyframe(&mut project, clip(video), PropertyId::Gain, 50, db(-3.0));
    // In the drawer of the text, which is of the video's hue shift, and its own.
    put_a_rack_in(
        &mut project,
        clip(text),
        clip(video),
        HUE,
        &[(30, turn(10.0)), (60, turn(20.0))],
    );
    make_a_keyframe(&mut project, clip(text), HUE, 10, turn(5.0));
    // In a keyframe crate that has nothing else, which goes when its one rack does.
    let a_crate = project.insert_crate(picture_track, frame(300), 50).unwrap();
    put_a_rack_in(&mut project, clip(a_crate), clip(video), HUE, &[(5, turn(1.0))]);
    assert_eq!(racks_of_the_hue_shift_of(&project, clip(video)), 3);
    assert_the_racks_are_sound(&project);

    project.remove_effect(clip(video), EffectKind::HueShift).unwrap();

    assert!(!project.has_the_effect(clip(video), EffectKind::HueShift));
    assert_eq!(
        racks_of_the_hue_shift_of(&project, clip(video)),
        0,
        "wherever they were"
    );
    assert!(project.placement_of(a_crate).is_none(), "the crate had no other rack");
    assert_eq!(
        keyframe_frames(&project, clip(video), PropertyId::Gain),
        vec![50],
        "its loudness is not an effect's"
    );
    assert!(project.has_the_effect(clip(text), EffectKind::HueShift));
    assert_eq!(
        keyframe_frames(&project, clip(text), HUE),
        vec![10],
        "the text has its own, which are not the video's"
    );
    assert_eq!(project.property_value_at(clip(video), HUE, frame(100)), None);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_crate_that_holds_racks_of_other_things_too_stays_when_the_racks_of_an_effect_are_taken_from_it() {
    let OneOfEach {
        mut project,
        video,
        picture_track,
        ..
    } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    let a_crate = project.insert_crate(picture_track, frame(300), 50).unwrap();
    put_a_rack_in(&mut project, clip(a_crate), clip(video), HUE, &[(5, turn(1.0))]);
    put_a_rack_in(
        &mut project,
        clip(a_crate),
        clip(video),
        PropertyId::Gain,
        &[(5, db(-6.0))],
    );

    project.remove_effect(clip(video), EffectKind::HueShift).unwrap();

    let crate_placement = project.placement_of(a_crate).expect("the crate is still there");
    assert_eq!(crate_placement.drawer.len(), 1);
    assert_eq!(crate_placement.drawer.racks()[0].property(), PropertyId::Gain);
    assert_the_racks_are_sound(&project);
}

#[test]
fn an_effect_put_on_again_after_it_was_taken_off_begins_again_with_nothing_of_what_it_had() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    project.set_property_at(clip(video), HUE, turn(77.0), frame(0)).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(10.0));
    make_a_keyframe(&mut project, clip(video), HUE, 200, turn(20.0));

    project.remove_effect(clip(video), EffectKind::HueShift).unwrap();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();

    assert_eq!(hue_at(&project, clip(video), 150), 0.0);
    assert!(!project.is_animated(clip(video), HUE));
    assert_eq!(project.keyframe_count(), 0);
}

#[test]
fn the_effect_of_a_track_is_taken_off_with_the_keyframes_of_its_values_and_not_those_of_its_clips() {
    let OneOfEach {
        mut project,
        video,
        video_track,
        ..
    } = one_of_each();
    project.add_effect(track(video_track), EffectKind::HueShift).unwrap();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, track(video_track), HUE, 40, turn(10.0));
    make_a_keyframe(&mut project, clip(video), HUE, 40, turn(20.0));

    project.remove_effect(track(video_track), EffectKind::HueShift).unwrap();

    assert!(
        !project.has_the_effect(track(video_track), EffectKind::HueShift),
        "the track has not got it any more"
    );
    assert!(project.effects_of(track(video_track)).unwrap().is_empty());
    assert_eq!(project.property_value_at(track(video_track), HUE, frame(40)), None);
    assert_eq!(racks_of_the_hue_shift_of(&project, track(video_track)), 0);
    assert!(
        project.has_the_effect(clip(video), EffectKind::HueShift),
        "the clip has still got its own"
    );
    assert_eq!(keyframe_frames(&project, clip(video), HUE), vec![40]);
    assert_the_racks_are_sound(&project);

    // Put on again it begins again, as it does for a clip.
    project.add_effect(track(video_track), EffectKind::HueShift).unwrap();
    assert_eq!(hue_at(&project, track(video_track), 40), 0.0);
    assert!(!project.is_animated(track(video_track), HUE));
}

// --- what is kept ---

#[test]
fn a_cut_clip_has_the_effect_in_both_halves_and_what_its_keyframes_said_goes_on_across_the_cut() {
    let OneOfEach {
        mut project,
        video,
        video_track,
        ..
    } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(0.0));
    make_a_keyframe(&mut project, clip(video), HUE, 300, turn(180.0));

    let cut = project.split_at(frame(200), None);

    let outcome = cut
        .iter()
        .find(|(track, _)| *track == video_track)
        .map(|(_, outcome)| *outcome)
        .unwrap();
    let (first, second) = (outcome.first_half, outcome.second_half);
    for half in [first, second] {
        assert!(project.has_the_effect(clip(half), EffectKind::HueShift), "{half}");
    }
    let near = |left: f32, right: f32| (left - right).abs() < 0.01;
    assert!(near(hue_at(&project, clip(first), 100), 0.0));
    assert!(
        near(hue_at(&project, clip(first), 199), 89.1),
        "{}",
        hue_at(&project, clip(first), 199)
    );
    assert!(
        near(hue_at(&project, clip(second), 200), 90.0),
        "{}",
        hue_at(&project, clip(second), 200)
    );
    assert!(near(hue_at(&project, clip(second), 300), 180.0));
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_cut_clip_whose_effect_is_not_animated_has_it_with_the_same_value_in_both_halves() {
    let OneOfEach {
        mut project,
        video,
        video_track,
        ..
    } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    project.set_property_at(clip(video), HUE, turn(75.0), frame(0)).unwrap();

    let cut = project.split_at(frame(250), None);

    let outcome = cut
        .iter()
        .find(|(track, _)| *track == video_track)
        .map(|(_, outcome)| *outcome)
        .unwrap();
    assert_eq!(hue_at(&project, clip(outcome.first_half), 10), 75.0);
    assert_eq!(hue_at(&project, clip(outcome.second_half), 400), 75.0);
}

#[test]
fn a_copy_of_a_clip_has_its_effects_and_keyframes_of_its_own_in_which_changing_one_does_not_change_the_other() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(10.0));
    make_a_keyframe(&mut project, clip(video), HUE, 200, turn(60.0));

    let copy = project.duplicate_clip(video, ClipSide::Right).unwrap();

    assert!(project.has_the_effect(clip(copy), EffectKind::HueShift));
    assert_eq!(
        keyframe_frames(&project, clip(copy), HUE),
        vec![700, 800],
        "the copy starts at frame 600"
    );
    assert_eq!(hue_at(&project, clip(copy), 700), 10.0);
    assert_eq!(hue_at(&project, clip(copy), 750), 35.0);
    project
        .set_property_at(clip(copy), HUE, turn(-5.0), frame(700))
        .unwrap();
    assert_eq!(hue_at(&project, clip(copy), 700), -5.0);
    assert_eq!(hue_at(&project, clip(video), 100), 10.0, "the clip is as it was");
    assert_eq!(keyframe_frames(&project, clip(video), HUE), vec![100, 200]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_copy_of_a_track_has_its_effects_and_those_of_its_clips_with_their_keyframes_made_again() {
    let OneOfEach {
        mut project,
        video,
        video_track,
        ..
    } = one_of_each();
    project.add_effect(track(video_track), EffectKind::HueShift).unwrap();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    project
        .set_property_at(track(video_track), HUE, turn(15.0), frame(0))
        .unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(10.0));
    make_a_keyframe(&mut project, track(video_track), HUE, 50, turn(1.0));

    let copy = project.duplicate_track(video_track, TrackSide::Above).unwrap();

    let copied_clip = project.track(copy).unwrap().entries()[0].id();
    assert!(project.has_the_effect(track(copy), EffectKind::HueShift));
    assert!(project.has_the_effect(clip(copied_clip), EffectKind::HueShift));
    assert_eq!(keyframe_frames(&project, track(copy), HUE), vec![50]);
    assert_eq!(keyframe_frames(&project, clip(copied_clip), HUE), vec![100]);
    project.remove_effect(track(copy), EffectKind::HueShift).unwrap();
    assert_eq!(
        keyframe_frames(&project, track(video_track), HUE),
        vec![50],
        "the track is as it was"
    );
    assert_eq!(keyframe_frames(&project, clip(video), HUE), vec![100]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_clip_moved_to_another_track_takes_its_effects_and_their_keyframes_with_it() {
    let OneOfEach {
        mut project,
        video,
        picture,
        video_track,
        ..
    } = one_of_each();
    for target in [picture, video] {
        project.add_effect(clip(target), EffectKind::HueShift).unwrap();
    }
    project.set_property_at(clip(video), HUE, turn(33.0), frame(0)).unwrap();
    make_a_keyframe(&mut project, clip(picture), HUE, 10, turn(10.0));
    make_a_keyframe(&mut project, clip(picture), HUE, 90, turn(50.0));

    project
        .move_clips(&[ClipMove {
            clip: picture,
            to_track: video_track,
            start: frame(700),
        }])
        .unwrap();

    assert_eq!(
        project.track_holding(picture).unwrap().id(),
        video_track,
        "it is on the other track now"
    );
    assert!(project.has_the_effect(clip(picture), EffectKind::HueShift));
    assert_eq!(
        keyframe_frames(&project, clip(picture), HUE),
        vec![710, 790],
        "10 and 90 frames into the clip"
    );
    assert_eq!(hue_at(&project, clip(picture), 750), 30.0);
    assert_eq!(
        hue_at(&project, clip(video), 5),
        33.0,
        "the clip that was there is as it was"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn what_an_edit_did_with_effects_is_put_back_as_it_was_by_restoring_an_earlier_project() {
    let OneOfEach { mut project, video, .. } = one_of_each();
    let before = project.clone();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(10.0));
    let with_the_effect = project.clone();

    project.restore(before.clone());
    assert!(!project.has_the_effect(clip(video), EffectKind::HueShift));
    assert_eq!(project.keyframe_count(), 0);

    project.restore(with_the_effect);
    assert!(project.has_the_effect(clip(video), EffectKind::HueShift));
    assert_eq!(keyframe_frames(&project, clip(video), HUE), vec![100]);
}

// --- what the check of the racks finds ---

#[test]
fn a_rack_of_the_values_of_an_effect_is_a_problem_when_what_it_animates_does_not_have_the_effect() {
    let OneOfEach {
        mut project,
        video,
        text,
        ..
    } = one_of_each();
    put_a_rack_in(&mut project, clip(text), clip(video), HUE, &[(5, turn(1.0))]);

    let problems = project.problems_with_the_racks();

    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("HueShift"), "{problems:?}");
    assert!(problems[0].contains("does not have it"), "{problems:?}");

    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    assert_the_racks_are_sound(&project);
}

#[test]
fn the_effects_of_a_clip_stay_on_it_when_its_speed_or_the_speed_of_its_track_changes_and_the_keyframes_of_their_values_go_along_with_it()
 {
    let OneOfEach {
        mut project,
        video,
        video_track,
        ..
    } = one_of_each();
    project.add_effect(clip(video), EffectKind::HueShift).unwrap();
    project.add_effect(track(video_track), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(video), HUE, 100, turn(0.0));
    make_a_keyframe(&mut project, clip(video), HUE, 300, turn(180.0));
    make_a_keyframe(&mut project, track(video_track), HUE, 100, turn(10.0));

    project.set_clip_speed(video, SpeedFactor::new(2).unwrap()).unwrap();

    assert!(project.has_the_effect(clip(video), EffectKind::HueShift));
    assert_eq!(
        keyframe_frames(&project, clip(video), HUE),
        vec![50, 150],
        "half as far in, as the clip is half as long"
    );
    assert_eq!(hue_at(&project, clip(video), 100), 90.0);
    assert_eq!(
        keyframe_frames(&project, track(video_track), HUE),
        vec![100],
        "the keyframes of the track are counted from the start of the timeline, and stay"
    );

    project
        .set_track_speed(video_track, SpeedFactor::new(2).unwrap())
        .unwrap();

    assert!(project.has_the_effect(clip(video), EffectKind::HueShift));
    assert!(project.has_the_effect(track(video_track), EffectKind::HueShift));
    assert_eq!(keyframe_frames(&project, clip(video), HUE), vec![25, 75]);
    assert_eq!(hue_at(&project, clip(video), 50), 90.0);
    assert_the_racks_are_sound(&project);
}

#[test]
fn the_effects_of_a_picture_stay_on_it_when_it_is_shown_for_less_time_and_its_keyframes_are_pulled_in() {
    let OneOfEach {
        mut project, picture, ..
    } = one_of_each();
    project.add_effect(clip(picture), EffectKind::HueShift).unwrap();
    make_a_keyframe(&mut project, clip(picture), HUE, 20, turn(0.0));
    make_a_keyframe(&mut project, clip(picture), HUE, 140, turn(120.0));

    project.set_clip_duration(picture, 100).unwrap();

    assert!(project.has_the_effect(clip(picture), EffectKind::HueShift));
    let frames = keyframe_frames(&project, clip(picture), HUE);
    assert_eq!(
        frames.len(),
        2,
        "no keyframe is lost while there is a frame for it: {frames:?}"
    );
    assert!(frames.iter().all(|frame| *frame < 100), "{frames:?}");
    assert_the_racks_are_sound(&project);
}

// --- whatever is done ---

#[test]
fn whatever_effects_are_put_on_and_taken_off_and_whatever_is_done_to_the_clips_and_tracks_the_racks_stay_sound() {
    use crate::composition::test_projects::random_project_with_effects;
    use crate::test_support::DeterministicRandomNumbers;

    let (mut done, mut effects_put_on, mut effects_taken_off) = (0, 0, 0);
    for seed in 1..=80u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 51_000);
        let mut project = random_project_with_effects(&mut random);
        for step in 0..25 {
            let context = format!("seed {seed}, step {step}");
            let clips: Vec<ClipId> = project
                .tracks()
                .iter()
                .flat_map(|track| track.entries().iter().map(|entry| entry.id()))
                .collect();
            let tracks: Vec<TrackId> = project.tracks().iter().map(Track::id).collect();
            let a_clip =
                |random: &mut DeterministicRandomNumbers| clips[random.next_below(clips.len() as u64) as usize];
            let a_track =
                |random: &mut DeterministicRandomNumbers| tracks[random.next_below(tracks.len() as u64) as usize];
            let a_target = |random: &mut DeterministicRandomNumbers| {
                if clips.is_empty() || random.next_below(3) == 0 {
                    track(a_track(random))
                } else {
                    clip(a_clip(random))
                }
            };
            let total = project.total_frame_count().max(1);
            let was_done = match random.next_below(14) {
                0 | 1 => {
                    let target = a_target(&mut random);
                    let result = project.add_effect(target, EffectKind::HueShift).is_ok();
                    effects_put_on += usize::from(result);
                    result
                }
                2 => {
                    let target = a_target(&mut random);
                    let result = project.remove_effect(target, EffectKind::HueShift).is_ok();
                    effects_taken_off += usize::from(result);
                    result
                }
                3 => !project.split_at(frame(random.next_below(total)), None).is_empty(),
                4 if !clips.is_empty() => {
                    let side = [ClipSide::Left, ClipSide::Right][random.next_below(2) as usize];
                    project.duplicate_clip(a_clip(&mut random), side).is_ok()
                }
                5 => {
                    let side = [TrackSide::Above, TrackSide::Below][random.next_below(2) as usize];
                    project.duplicate_track(a_track(&mut random), side).is_ok()
                }
                6 if !clips.is_empty() => project.delete_clip(a_clip(&mut random)).is_ok(),
                7 if !clips.is_empty() => project
                    .set_clip_speed(
                        a_clip(&mut random),
                        SpeedFactor::new(1 + random.next_below(4) as u32).unwrap(),
                    )
                    .is_ok(),
                8 => project
                    .set_track_speed(
                        a_track(&mut random),
                        SpeedFactor::new(1 + random.next_below(3) as u32).unwrap(),
                    )
                    .is_ok(),
                9 if !clips.is_empty() => project
                    .set_clip_duration(a_clip(&mut random), 1 + random.next_below(80))
                    .is_ok(),
                10 if !clips.is_empty() => {
                    let moved = ClipMove {
                        clip: a_clip(&mut random),
                        to_track: a_track(&mut random),
                        start: frame(random.next_below(total)),
                    };
                    project.move_clips(&[moved]).is_ok()
                }
                11 | 12 => {
                    // A rack goes to the drawer of a clip or a track, where it is kept apart from what it is of.
                    let racks: Vec<RackId> = project.located_racks().map(|located| located.rack.id()).collect();
                    if racks.is_empty() {
                        false
                    } else {
                        let rack = racks[random.next_below(racks.len() as u64) as usize];
                        let owner = a_target(&mut random);
                        project
                            .move_racks(&[rack], RackDestination::Drawer(owner), 0, None)
                            .is_ok()
                    }
                }
                _ => {
                    // A keyframe of a hue shift, if what is chosen has one.
                    let target = a_target(&mut random);
                    let at = frame(random.next_below(total));
                    project.add_keyframe(target, HUE, at).is_ok_and(|made| made.is_some())
                }
            };
            done += usize::from(was_done);

            assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
            for placement in project.tracks().iter().flat_map(Track::placements) {
                if matches!(placement.clip.kind(), ClipKind::Audio | ClipKind::Crate) {
                    assert!(
                        placement.effects.is_empty(),
                        "{context}: a {:?} has an effect",
                        placement.clip.kind()
                    );
                }
            }
        }
    }
    // (So that what is done cannot stop doing anything, or never reach effects, without this noticing.)
    assert!(done > 800, "only {done} of the 2000 steps did anything");
    assert!(effects_put_on > 60, "only {effects_put_on} effects were put on");
    assert!(
        effects_taken_off > 40,
        "only {effects_taken_off} effects were taken off"
    );
}
