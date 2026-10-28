// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the sound plan makes of the sound effects of clips and tracks (phase 7, piece 7.5): the plan carries them, with their values and the keyframes of them, for
//! whatever makes the sound to do (the sound engine, in piece 7.6).

use super::*;
use crate::composition::test_projects::{LONG_VIDEO, SOUND_FILE, media_clip, project_of};
use crate::project::tracks::Clip;
use crate::project::{AnimationTarget, EffectKind, InstalledPlugin, PropertyValue, set_installed_plugins};
use crate::test_support::{
    DeterministicRandomNumbers, an_installed_sound_plugin, make_a_keyframe, put_a_hue_shift_on, put_an_effect_on,
    the_installed_plugins_to_myself,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, frame(0), |_| true)
}

fn clips_of(plan: &SoundPlan) -> Vec<&SoundClip> {
    plan.strips
        .iter()
        .flat_map(|strip| &strip.items)
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

fn plugin(key: &str) -> InstalledPlugin {
    an_installed_sound_plugin(key, "Maker", &[("gain", 1.0, Some((0.0, 4.0))), ("mix", 0.5, None)])
}

fn kind_of(plugin: &InstalledPlugin) -> EffectKind {
    set_installed_plugins(vec![plugin.clone()]);
    EffectKind::installed_plugin_kinds()[0]
}

fn project_with_a_video_and_a_sound() -> Project {
    project_of(vec![
        vec![(0, Clip::Video(media_clip(LONG_VIDEO, 0, 100, 1)))],
        vec![(20, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ])
}

fn clip_id(project: &Project, track: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(0).unwrap().id
}

#[test]
fn the_sound_effects_of_a_clip_are_in_the_plan_with_their_kind_and_values_in_the_order_of_the_chain() {
    let _alone = the_installed_plugins_to_myself();
    let first = kind_of(&plugin("ladspa:a:first"));
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    let one = put_an_effect_on(
        &mut project,
        target,
        first,
        &[PropertyValue::Number(2.0), PropertyValue::Number(0.25)],
    );
    let two = put_an_effect_on(
        &mut project,
        target,
        first,
        &[PropertyValue::Number(3.0), PropertyValue::Number(0.75)],
    );

    let plan = plan(&project);
    let video = clips_of(&plan)[0];
    assert_eq!(video.effects.len(), 2);
    assert_eq!(video.effects[0].instance, one);
    assert_eq!(video.effects[1].instance, two);
    assert_eq!(video.effects[0].kind, first);
    assert_eq!(
        video.effects[0].values,
        [PropertyValue::Number(2.0), PropertyValue::Number(0.25)]
    );
    assert_eq!(
        video.effects[1].values,
        [PropertyValue::Number(3.0), PropertyValue::Number(0.75)]
    );
    assert_eq!(video.first_timeline_frame, 0);
    assert!(clips_of(&plan)[1].effects.is_empty(), "the other clip has none");
}

#[test]
fn a_clip_that_starts_later_says_when_and_a_sound_clip_can_have_an_effect_as_well_as_a_video() {
    let _alone = the_installed_plugins_to_myself();
    let kind = kind_of(&plugin("ladspa:a:on_a_sound"));
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 1));
    put_an_effect_on(
        &mut project,
        target,
        kind,
        &[PropertyValue::Number(1.0), PropertyValue::Number(0.5)],
    );
    let plan = plan(&project);
    let sound = clips_of(&plan)[1];
    assert_eq!(sound.first_timeline_frame, 20);
    assert_eq!(sound.effects.len(), 1);
    assert!(clips_of(&plan)[0].effects.is_empty());
}

#[test]
fn a_picture_effect_is_not_in_the_sound_plan() {
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    put_a_hue_shift_on(&mut project, target, 90.0);
    let plan = plan(&project);
    assert!(clips_of(&plan).iter().all(|clip| clip.effects.is_empty()));
}

#[test]
fn the_effects_of_the_track_are_the_instance_of_its_strip_that_its_clips_share_and_not_in_the_chain_of_a_clip() {
    let _alone = the_installed_plugins_to_myself();
    let kind = kind_of(&plugin("ladspa:a:on_a_track"));
    let mut project = project_with_a_video_and_a_sound();
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(4.0), PropertyValue::Number(0.5)],
    );
    let plan = plan(&project);
    let video = clips_of(&plan)[0];
    assert!(
        video.effects.is_empty(),
        "since phase 12 the effects of the track are not in the chain of its clips"
    );
    let buses = &plan.strips[0].buses;
    assert_eq!(buses.len(), 1);
    assert_eq!(buses[0].effects.len(), 1);
    assert_eq!(buses[0].effects[0].values[0], PropertyValue::Number(4.0));
    assert!(clips_of(&plan)[1].effects.is_empty(), "the other track is not affected");
    assert!(plan.strips[1].buses.is_empty());
}

#[test]
fn the_keyframes_of_a_value_are_in_the_plan_and_say_what_it_is_at_each_frame_of_the_timeline() {
    let _alone = the_installed_plugins_to_myself();
    let kind = kind_of(&plugin("ladspa:a:animated"));
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    let instance = put_an_effect_on(
        &mut project,
        target,
        kind,
        &[PropertyValue::Number(1.0), PropertyValue::Number(0.5)],
    );
    let gain = crate::project::PropertyId::effect_value(kind, instance, 0);
    make_a_keyframe(&mut project, target, gain, 10, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, target, gain, 30, PropertyValue::Number(4.0));

    let plan = plan(&project);
    let effect = &clips_of(&plan)[0].effects[0];
    assert!(effect.is_animated());
    let at = |frame: u64| effect.values_at(frame);
    assert_eq!(
        at(0)[0],
        PropertyValue::Number(0.0),
        "before the first keyframe it is as at that one"
    );
    assert_eq!(at(20)[0], PropertyValue::Number(2.0), "halfway between them");
    assert_eq!(
        at(60)[0],
        PropertyValue::Number(4.0),
        "after the last it is as at that one"
    );
    assert_eq!(
        at(20)[1],
        PropertyValue::Number(0.5),
        "a value without keyframes is what it is"
    );
}

#[test]
fn a_clip_that_is_cut_into_stretches_by_a_mute_keyframe_has_its_effects_in_each_heard_stretch_and_says_where_each_begins()
 {
    let _alone = the_installed_plugins_to_myself();
    let kind = kind_of(&plugin("ladspa:a:cut"));
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    put_an_effect_on(
        &mut project,
        target,
        kind,
        &[PropertyValue::Number(2.0), PropertyValue::Number(0.5)],
    );
    make_a_keyframe(
        &mut project,
        target,
        crate::project::PropertyId::Muted,
        0,
        PropertyValue::Flag(false),
    );
    make_a_keyframe(
        &mut project,
        target,
        crate::project::PropertyId::Muted,
        30,
        PropertyValue::Flag(true),
    );
    make_a_keyframe(
        &mut project,
        target,
        crate::project::PropertyId::Muted,
        60,
        PropertyValue::Flag(false),
    );
    let plan = plan(&project);
    let heard: Vec<(u64, usize)> = plan.strips[0]
        .items
        .iter()
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some((clip.first_timeline_frame, clip.effects.len())),
            SoundItem::Silence { .. } => None,
        })
        .collect();
    assert_eq!(heard, [(0, 1), (60, 1)]);
}

#[test]
fn an_effect_that_is_not_installed_is_in_the_plan_all_the_same_and_so_is_one_that_is_installed_later() {
    let _alone = the_installed_plugins_to_myself();
    let mine = plugin("ladspa:a:not_there_yet");
    let kind = kind_of(&mine);
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    put_an_effect_on(
        &mut project,
        target,
        kind,
        &[PropertyValue::Number(1.0), PropertyValue::Number(0.5)],
    );
    set_installed_plugins(Vec::new());
    let plan = plan(&project);
    let effect = &clips_of(&plan)[0].effects[0];
    assert!(!effect.kind.is_installed(), "the plugin is gone");
    assert_eq!(effect.values.len(), 2, "but the effect is kept whole, with its values");
}

#[test]
fn a_random_project_with_sound_effects_has_effects_in_the_plan_only_on_the_clips_that_have_them() {
    let _alone = the_installed_plugins_to_myself();
    let kind = kind_of(&plugin("ladspa:a:random"));
    let mut random = DeterministicRandomNumbers::with_seed(7);
    for _ in 0..20 {
        let mut project = project_with_a_video_and_a_sound();
        let with = usize::from(random.next_below(2) == 1);
        let target = AnimationTarget::Clip(clip_id(&project, with));
        let count = 1 + random.next_below(3) as usize;
        for _ in 0..count {
            put_an_effect_on(
                &mut project,
                target,
                kind,
                &[PropertyValue::Number(1.0), PropertyValue::Number(0.5)],
            );
        }
        let plan = plan(&project);
        let clips = clips_of(&plan);
        assert_eq!(clips[with].effects.len(), count);
        assert!(clips[1 - with].effects.is_empty());
    }
}

fn hash_of_the_sound_effects(project: &Project) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_the_sound_effects_into(project, &mut hasher);
    hasher.finish()
}

#[test]
fn what_is_heard_of_the_effects_changes_when_another_plugin_of_the_same_library_stands_in_for_the_kind() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    let gain = plugin("ladspa:a:first");
    let kind = kind_of(&gain);
    put_an_effect_on(
        &mut project,
        target,
        kind,
        &[PropertyValue::Number(2.0), PropertyValue::Number(0.25)],
    );
    let heard_first = hash_of_the_sound_effects(&project);

    // The same key, in the same library file, but the place of the plugin in it is another (the library was changed, and scanned again).
    let mut moved = gain.clone();
    moved.address = crate::project::PluginAddress::Ladspa {
        index: 1,
        unique_id: 900,
    };
    set_installed_plugins(vec![moved]);
    let heard_moved = hash_of_the_sound_effects(&project);
    // And a CLAP plugin with that key, in the same file.
    let mut clap = gain.clone();
    clap.address = crate::project::PluginAddress::Clap {
        id: "a.first".to_owned(),
    };
    set_installed_plugins(vec![clap]);
    let heard_clap = hash_of_the_sound_effects(&project);

    assert_ne!(heard_first, heard_moved);
    assert_ne!(heard_first, heard_clap);
    assert_ne!(heard_moved, heard_clap);
}

#[test]
fn the_state_that_a_plugin_saved_is_in_the_plan_and_what_is_heard_changes_when_it_does() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = project_with_a_video_and_a_sound();
    let target = AnimationTarget::Clip(clip_id(&project, 0));
    let kind = kind_of(&plugin("clap:a:first"));
    let instance = put_an_effect_on(
        &mut project,
        target,
        kind,
        &[PropertyValue::Number(2.0), PropertyValue::Number(0.25)],
    );
    assert_eq!(clips_of(&plan(&project))[0].effects[0].state, None);
    let heard_without = hash_of_the_sound_effects(&project);

    project
        .set_effect_state(target.into(), instance, Some(std::sync::Arc::from(b"one".as_slice())))
        .unwrap();
    let heard_one = hash_of_the_sound_effects(&project);
    project
        .set_effect_state(target.into(), instance, Some(std::sync::Arc::from(b"two".as_slice())))
        .unwrap();
    let heard_two = hash_of_the_sound_effects(&project);

    assert_eq!(
        clips_of(&plan(&project))[0].effects[0].state.as_deref(),
        Some(b"two".as_slice())
    );
    assert_ne!(heard_without, heard_one, "a state where there was none");
    assert_ne!(heard_one, heard_two, "another state");
    project
        .set_effect_state(target.into(), instance, Some(std::sync::Arc::from(b"one".as_slice())))
        .unwrap();
    assert_eq!(
        hash_of_the_sound_effects(&project),
        heard_one,
        "the same bytes are heard the same"
    );
}
