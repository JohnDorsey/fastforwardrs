// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MIDI clip in the project (phase 13, piece 13.3): putting one on a track, what it is (music that comes from a file, with a speed, a volume and a mute switch, and nothing on the picture),
//! how it is edited, cut and counted again at another frame rate, the resource that it plays, and the effects it can have.

use std::path::PathBuf;

use super::tests::{frame, frame_rate};
use super::*;
use crate::test_support::{a_midi_file, an_installed_sound_plugin, make_a_keyframe, the_installed_plugins_to_myself};

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn a_midi_clip(first: u64, end: u64) -> MidiClip {
    MidiClip::new(
        "tune.mid",
        0,
        SourceFrameIndex::new(first),
        SourceFrameIndex::new(end),
        SpeedFactor::NORMAL,
    )
    .unwrap()
}

/// A project at 30 frames a second with one track that has a MIDI clip of 90 frames from frame 10, and the file it plays as a resource.
fn a_track_with_a_midi_clip() -> (Project, TrackId, ClipId) {
    let mut project = Project::empty().with_frame_rate(Some(frame_rate(30)));
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            a_midi_file(),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();
    let track = project.add_track_in_front();
    let clip = project
        .try_insert_midi_clip(track, frame(10), a_midi_clip(0, 90))
        .unwrap();
    (project, track, clip)
}

fn the_midi_clip(project: &Project, clip: ClipId) -> MidiClip {
    project
        .placement_of(clip)
        .unwrap()
        .clip
        .as_midi()
        .expect("it is a MIDI clip")
        .clone()
}

fn the_frames_of(project: &Project, clip: ClipId) -> (u64, u64) {
    let placement = project.placement_of(clip).unwrap();
    (
        placement.first_timeline_frame.value(),
        placement.end_timeline_frame.value(),
    )
}

#[test]
fn a_midi_clip_is_put_on_a_track_where_there_is_room_for_all_of_it_and_lasts_as_long_as_its_music_says() {
    let (mut project, track, clip) = a_track_with_a_midi_clip();

    let placement = project.placement_of(clip).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Midi);
    assert_eq!(the_frames_of(&project, clip), (10, 100));
    assert_eq!(placement.timeline_frame_count(), 90);

    assert!(matches!(
        project.try_insert_midi_clip(track, frame(50), a_midi_clip(0, 10)),
        Err(EditError::NoRoomThere)
    ));
    assert!(
        matches!(
            project.try_insert_midi_clip(track, frame(0), a_midi_clip(0, 11)),
            Err(EditError::NoRoomThere)
        ),
        "ten frames fit before the clip and eleven do not"
    );
    assert!(
        project
            .try_insert_midi_clip(track, frame(0), a_midi_clip(0, 10))
            .is_ok()
    );
    assert!(matches!(
        project.try_insert_midi_clip(TrackId::new(99), frame(300), a_midi_clip(0, 10)),
        Err(EditError::NoRoomThere)
    ));
}

#[test]
fn a_midi_clip_is_a_clip_of_a_file_that_is_not_media_has_no_source_file_and_is_nowhere_on_the_picture() {
    let (project, _, clip) = a_track_with_a_midi_clip();

    let placement = project.placement_of(clip).unwrap();

    assert!(
        placement.clip.is_media(),
        "it has a speed, so the clips after it are carried along when its length changes"
    );
    assert!(placement.clip.as_media().is_none());
    assert_eq!(
        placement.clip.source(),
        None,
        "its file is a resource and not a source of video or sound"
    );
    assert_eq!(placement.clip.placement(), None);
    assert_eq!(
        placement
            .clip
            .with_placement(Placement::upright(CanvasArea::WHOLE_PICTURE)),
        None
    );
    assert_eq!(project.placement_on_the_picture(placement.clip), None);
    assert_eq!(placement.clip.content(), None);
    assert_eq!(placement.clip.duration_in_frames_it_says(), None);
}

#[test]
fn the_speed_of_a_midi_clip_changes_how_long_it_lasts_and_carries_the_clips_that_touch_it_along() {
    let (mut project, track, first) = a_track_with_a_midi_clip();
    let second = project
        .try_insert_midi_clip(track, frame(100), a_midi_clip(100, 160))
        .unwrap();
    assert_eq!(the_frames_of(&project, second), (100, 160));

    project.set_clip_speed(first, speed(3)).unwrap();

    assert_eq!(the_midi_clip(&project, first).speed(), speed(3));
    assert_eq!(
        the_frames_of(&project, first),
        (10, 40),
        "ninety frames at three times the speed are thirty"
    );
    assert_eq!(
        the_frames_of(&project, second),
        (40, 100),
        "the clip that touched it is carried along"
    );
}

#[test]
fn the_speed_of_the_track_counts_too_and_it_is_the_effective_speed_that_says_how_long_a_clip_lasts() {
    let (mut project, track, clip) = a_track_with_a_midi_clip();
    project.set_clip_speed(clip, speed(3)).unwrap();

    project.set_track_speed(track, speed(2)).unwrap();

    let placement = project.placement_of(clip).unwrap();
    assert_eq!(placement.effective_speed, speed(6));
    assert_eq!(placement.timeline_frame_count(), 15);
}

#[test]
fn the_volume_and_the_mute_switch_of_a_midi_clip_are_changed_like_those_of_a_sound_clip_and_have_keyframes() {
    let (mut project, _, clip) = a_track_with_a_midi_clip();
    let target = AnimationTarget::Clip(clip);

    project.set_clip_gain(clip, Gain::from_decibels(-6.0)).unwrap();
    project.set_clip_muted(clip, true).unwrap();

    let midi = the_midi_clip(&project, clip);
    assert_eq!(midi.gain(), Gain::from_decibels(-6.0));
    assert!(midi.is_muted());
    assert_eq!(
        project.property_value_at(target, PropertyId::Gain, frame(20)),
        Some(PropertyValue::Number(-6.0))
    );
    assert_eq!(
        project.property_value_at(target, PropertyId::Muted, frame(20)),
        Some(PropertyValue::Flag(true))
    );
    make_a_keyframe(&mut project, target, PropertyId::Gain, 20, PropertyValue::Number(-12.0));
    assert_eq!(
        project.property_value_at(target, PropertyId::Gain, frame(20)),
        Some(PropertyValue::Number(-12.0)),
        "a keyframe says what the volume is"
    );
    assert_eq!(
        the_midi_clip(&project, clip).gain(),
        Gain::from_decibels(-6.0),
        "the clip says what it was"
    );
}

#[test]
fn a_midi_clip_has_a_volume_and_a_mute_switch_and_nothing_that_is_seen() {
    for property in [PropertyId::Gain, PropertyId::Muted] {
        assert!(property.applies_to_a_clip_of_kind(ClipKind::Midi), "{property:?}");
    }
    for property in [PropertyId::Position, PropertyId::Size, PropertyId::Rotation] {
        assert!(!property.applies_to_a_clip_of_kind(ClipKind::Midi), "{property:?}");
    }
    let (project, _, clip) = a_track_with_a_midi_clip();
    assert_eq!(
        project.property_value_at(AnimationTarget::Clip(clip), PropertyId::Position, frame(20)),
        None
    );
}

#[test]
fn cutting_a_midi_clip_gives_two_that_play_the_same_track_of_the_same_file_and_together_the_same_music() {
    let (mut project, track, clip) = a_track_with_a_midi_clip();
    project.set_clip_gain(clip, Gain::from_decibels(-3.0)).unwrap();

    let cut = project.split_at(frame(40), None);

    assert_eq!(cut.len(), 1);
    let SplitOutcome {
        first_half,
        second_half,
    } = cut[0].1;
    assert_eq!(cut[0].0, track);
    let (first, second) = (
        the_midi_clip(&project, first_half),
        the_midi_clip(&project, second_half),
    );
    assert_eq!(
        (first.first_source_frame().value(), first.end_source_frame().value()),
        (0, 30)
    );
    assert_eq!(
        (second.first_source_frame().value(), second.end_source_frame().value()),
        (30, 90)
    );
    for half in [&first, &second] {
        assert_eq!((half.resource(), half.midi_track()), ("tune.mid", 0));
        assert_eq!(half.gain(), Gain::from_decibels(-3.0));
    }
    assert_eq!(the_frames_of(&project, first_half), (10, 40));
    assert_eq!(the_frames_of(&project, second_half), (40, 100));
}

#[test]
fn counted_again_at_another_frame_rate_a_midi_clip_covers_the_same_music_for_as_long() {
    let (mut project, _, clip) = a_track_with_a_midi_clip();

    project.set_frame_rate(frame_rate(60));

    let midi = the_midi_clip(&project, clip);
    assert_eq!(
        (midi.first_source_frame().value(), midi.end_source_frame().value()),
        (0, 180)
    );
    assert_eq!(the_frames_of(&project, clip), (20, 200));
}

// --- the file it plays is a resource ---

#[test]
fn the_files_that_midi_clips_play_are_resources_that_the_project_uses_and_saves_and_the_others_are_not() {
    let (mut project, track, _) = a_track_with_a_midi_clip();
    let spare = project
        .add_resource(
            "spare.mid",
            ResourceKind::Midi,
            std::sync::Arc::from(&b"another"[..]),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/spare.mid")),
        )
        .unwrap();

    let used = project.resources_in_use();
    let saved: Vec<String> = project
        .resources_to_save()
        .iter()
        .map(|resource| resource.name().to_owned())
        .collect();

    assert_eq!(used.into_iter().collect::<Vec<_>>(), vec!["tune.mid".to_owned()]);
    assert_eq!(
        saved,
        vec!["tune.mid".to_owned()],
        "{spare} is kept in memory and not saved"
    );
    let other = MidiClip::new(
        "spare.mid",
        1,
        SourceFrameIndex::new(0),
        SourceFrameIndex::new(5),
        SpeedFactor::NORMAL,
    )
    .unwrap();
    project.try_insert_midi_clip(track, frame(200), other).unwrap();
    let now: Vec<String> = project.resources_in_use().into_iter().collect();
    assert_eq!(now, vec!["spare.mid".to_owned(), "tune.mid".to_owned()]);
}

#[test]
fn a_midi_clip_whose_file_is_not_in_the_project_or_is_missing_is_kept_and_its_file_is_said_to_be_missing() {
    let mut project = Project::empty().with_frame_rate(Some(frame_rate(30)));
    let track = project.add_track_in_front();
    project
        .try_insert_midi_clip(track, frame(0), a_midi_clip(0, 30))
        .unwrap();
    assert_eq!(
        project.resources_in_use_that_are_missing(),
        vec!["tune.mid".to_owned()],
        "the project has no such file"
    );

    let mut with_it_missing = Resources::default();
    with_it_missing
        .add_as_it_was_saved(
            "tune.mid",
            ResourceKind::Midi,
            None,
            SourceMediaLink::from_absolute_path(PathBuf::from("/gone/tune.mid")),
        )
        .unwrap();
    let project = project.with_resources(with_it_missing);
    assert_eq!(
        project.resources_in_use_that_are_missing(),
        vec!["tune.mid".to_owned()],
        "the file could not be read"
    );
    assert_eq!(project.tracks()[0].entries().len(), 1, "the clip is kept");
}

// --- effects ---

#[test]
fn a_midi_clip_can_have_sound_effects_and_synthesizers_and_not_picture_effects() {
    let _alone = the_installed_plugins_to_myself();
    let mut synthesizer = an_installed_sound_plugin("clap:com.example.synth", "A Maker", &[("gain", 1.0, None)]);
    synthesizer.descriptor.makes_sound_from_notes = true;
    synthesizer.audio_inputs = 0;
    synthesizer.audio_outputs = 2;
    let reverb = an_installed_sound_plugin("clap:com.example.reverb", "A Maker", &[("mix", 0.5, None)]);
    set_installed_plugins(vec![synthesizer, reverb]);
    let kinds = EffectKind::installed_plugin_kinds();
    let (synthesizer, reverb) = (kinds[0], kinds[1]);
    let (mut project, _, clip) = a_track_with_a_midi_clip();
    let target = AnimationTarget::Clip(clip);

    assert!(project.add_effect(target, synthesizer).is_ok());
    assert!(project.add_effect(target, reverb).is_ok());
    assert!(
        project.add_effect(target, EffectKind::HUE_SHIFT).is_err(),
        "a picture effect is not for what makes no picture"
    );
    assert!(project.add_effect(target, EffectKind::BLUR).is_err());

    let chain = project.effects_of(target).unwrap();
    assert_eq!(chain.iter().count(), 2);
    assert!(synthesizer.applies_to_a_clip_of_kind(ClipKind::Midi));
    assert!(!EffectKind::HUE_SHIFT.applies_to_a_clip_of_kind(ClipKind::Midi));
}
