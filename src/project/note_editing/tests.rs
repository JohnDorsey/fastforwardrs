// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch "pass notes on" of a synthesizer (phase 13, piece 13.5): off when it is put on, set on a clip, a track and a group, refused for what is not a synthesizer, and kept by what copies it.

use std::path::PathBuf;

use super::*;
use crate::project::tests::{frame, frame_rate};
use crate::project::{
    AnimationTarget, ChainItemKey, ClipId, Effect, EffectKind, GroupKind, InstalledPlugin, PropertyValue, TailLength,
    TrackId, set_installed_plugins,
};
use crate::project::{MidiClip, ResourceKind, SourceFrameIndex, SourceMediaLink, SpeedFactor};
use crate::test_support::{
    a_midi_file, an_installed_sound_plugin, an_installed_synthesizer, the_installed_plugins_to_myself,
};

/// The kinds of effect that are installed: a synthesizer, and an effect that changes sound.
fn the_kinds() -> (EffectKind, EffectKind) {
    let plugins: Vec<InstalledPlugin> = vec![
        an_installed_synthesizer("clap:test:synth"),
        an_installed_sound_plugin("clap:test:effect", "Maker", &[("mix", 0.5, None)]),
    ];
    set_installed_plugins(plugins);
    let kinds = EffectKind::installed_plugin_kinds();
    (kinds[0], kinds[1])
}

/// Two tracks, each with a MIDI clip of 10 frames at the start (clips that can have sound effects), and the file they play.
struct Laid {
    project: Project,
    tracks: [TrackId; 2],
    clips: [ClipId; 2],
}

fn lay_out() -> Laid {
    let mut project = Project::empty().with_frame_rate(Some(frame_rate(30)));
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            a_midi_file(),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();
    let tracks = [project.add_track_in_front(), project.add_track_in_front()];
    let clips = tracks.map(|track| {
        project
            .try_insert_midi_clip(
                track,
                frame(0),
                MidiClip::new(
                    "tune.mid",
                    0,
                    SourceFrameIndex::new(0),
                    SourceFrameIndex::new(10),
                    SpeedFactor::NORMAL,
                )
                .unwrap(),
            )
            .unwrap()
    });
    Laid { project, tracks, clips }
}

fn passes_notes_on(project: &Project, owner: Member, instance: EffectInstanceId) -> bool {
    project
        .chain_of_the_member(owner)
        .and_then(|chain| chain.instance(instance))
        .expect("the effect is there")
        .passes_notes_on()
}

#[test]
fn a_synthesizer_is_put_on_consuming_the_notes_and_the_switch_is_set_on_a_clip_a_track_and_a_group() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut laid = lay_out();
    let clip = Member::Clip(laid.clips[0]);
    let track = Member::Track(laid.tracks[0]);
    let group = laid
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(laid.clips[0]), Member::Clip(laid.clips[1])],
        )
        .unwrap();
    let on_the_clip = laid
        .project
        .add_effect(AnimationTarget::Clip(laid.clips[0]), synthesizer)
        .unwrap();
    let on_the_track = laid
        .project
        .add_effect(AnimationTarget::Track(laid.tracks[0]), synthesizer)
        .unwrap();
    let on_the_group = laid.project.add_effect_to_the_group(group, synthesizer).unwrap();
    let owners = [
        (clip, on_the_clip),
        (track, on_the_track),
        (Member::Group(group), on_the_group),
    ];
    for (owner, instance) in owners {
        assert!(
            !passes_notes_on(&laid.project, owner, instance),
            "off when put on: {owner:?}"
        );
    }

    for (owner, instance) in owners {
        laid.project.set_effect_passes_notes_on(owner, instance, true).unwrap();

        assert!(passes_notes_on(&laid.project, owner, instance), "on: {owner:?}");
        for (other_owner, other_instance) in owners.into_iter().filter(|other| other.0 != owner) {
            let was_switched_before =
                owners.iter().position(|o| o.0 == other_owner) < owners.iter().position(|o| o.0 == owner);
            assert_eq!(
                passes_notes_on(&laid.project, other_owner, other_instance),
                was_switched_before,
                "only the one that was set is changed"
            );
        }
    }
    laid.project
        .set_effect_passes_notes_on(clip, on_the_clip, false)
        .unwrap();
    assert!(!passes_notes_on(&laid.project, clip, on_the_clip), "and off again");
}

#[test]
fn what_is_not_a_synthesizer_or_is_not_there_cannot_be_set() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, effect) = the_kinds();
    let mut laid = lay_out();
    let clip = Member::Clip(laid.clips[0]);
    let target = AnimationTarget::Clip(laid.clips[0]);
    let plain = laid.project.add_effect(target, effect).unwrap();
    let synth = laid.project.add_effect(target, synthesizer).unwrap();
    let before = laid.project.clone();

    assert_eq!(
        laid.project.set_effect_passes_notes_on(clip, plain, true),
        Err(EditError::NotASynthesizer {
            owner: clip,
            instance: plain
        })
    );
    assert_eq!(
        laid.project
            .set_effect_passes_notes_on(clip, EffectInstanceId::new(99), true),
        Err(EditError::NotInTheChain {
            owner: clip,
            item: ChainItemKey::Effect(EffectInstanceId::new(99))
        })
    );
    assert!(
        laid.project
            .set_effect_passes_notes_on(Member::Track(TrackId::new(12_345)), synth, true)
            .is_err()
    );
    assert_eq!(laid.project, before, "nothing was changed");
}

#[test]
fn a_copy_of_the_effect_keeps_the_switch() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut laid = lay_out();
    let clip = Member::Clip(laid.clips[0]);
    let instance = laid
        .project
        .add_effect(AnimationTarget::Clip(laid.clips[0]), synthesizer)
        .unwrap();
    laid.project.set_effect_passes_notes_on(clip, instance, true).unwrap();

    let copy = laid.project.duplicate_effect(clip, instance).unwrap();

    assert!(passes_notes_on(&laid.project, clip, copy), "a copy of the effect");
    assert!(passes_notes_on(&laid.project, clip, instance));
}

#[test]
fn the_switch_does_not_change_the_values_of_the_effect_or_anything_else_about_it() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let effect = Effect::new(synthesizer).with_tail_length(TailLength::Milliseconds(300));

    let switched = effect.clone().with_passes_notes_on(true);

    assert_eq!(switched.values(), effect.values());
    assert_eq!(switched.tail_length(), effect.tail_length());
    assert_eq!(
        (switched.kind(), switched.instance()),
        (effect.kind(), effect.instance())
    );
    assert!(switched.passes_notes_on() && !effect.passes_notes_on());
    assert_ne!(switched, effect, "two effects that differ by the switch are not equal");
    let other_value = switched
        .with_value(switched.property(0).unwrap(), &PropertyValue::Number(2.0))
        .unwrap();
    assert!(
        other_value.passes_notes_on(),
        "a value that is set does not undo the switch"
    );
}
