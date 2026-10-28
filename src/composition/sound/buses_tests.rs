// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The instances of the sound effects of a track that clips share (phase 12, piece 12.2; docs/engine-phase-12-design.md, sections 2 and 5): what the plan says of them, and which clips
//! have the effects of their track in their own chain instead.

use std::hash::{DefaultHasher, Hasher};

use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::media::export::sound_graph_specification;
use crate::project::tracks::Clip;
use crate::project::{
    Badge, ChainItemKey, EffectDomain, EffectKind, InstalledPlugin, Member, PropertyValue, SourceMediaIdentifier,
    SpedUpClipSound, TrackSharing, set_installed_plugins,
};
use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

/// Two sound plugins, as installed kinds of effect: the first and the second.
fn two_kinds() -> (EffectKind, EffectKind) {
    let plugin =
        |key: &str| -> InstalledPlugin { an_installed_sound_plugin(key, "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]) };
    set_installed_plugins(vec![plugin("ladspa:a:first"), plugin("ladspa:a:second")]);
    let kinds = EffectKind::installed_plugin_kinds();
    (kinds[0], kinds[1])
}

/// A track of sound clips, one after another with the gaps before them that are given, and a track of one sound clip.
fn a_track_of_clips_with_gaps(gaps: &[u64]) -> Project {
    let clips: Vec<(u64, Clip)> = gaps
        .iter()
        .map(|gap| (*gap, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))))
        .collect();
    project_of(vec![clips, vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))]])
}

fn clip_at(project: &Project, track: usize, index: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(index).unwrap().id
}

fn values_of(effects: &[LayerEffect]) -> Vec<PropertyValue> {
    effects.iter().map(|effect| effect.values[0].clone()).collect()
}

fn clips_of(strip: &SoundStrip) -> Vec<&SoundClip> {
    strip
        .items
        .iter()
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

#[test]
fn the_effects_of_a_track_are_one_instance_that_the_clips_that_follow_one_another_share_and_are_not_in_the_chain_of_any_clip()
 {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );

    let plan = plan(&project);

    let strip = &plan.strips[0];
    assert_eq!(strip.items.len(), 3);
    assert_eq!(strip.buses.len(), 1, "three clips in a row: one instance");
    assert_eq!((strip.buses[0].first_item, strip.buses[0].last_item), (0, 2));
    assert_eq!(values_of(&strip.buses[0].effects), [PropertyValue::Number(2.0)]);
    assert!(
        clips_of(strip).iter().all(|clip| clip.effects.is_empty()),
        "the clips do not have them in their chain"
    );
    assert!(plan.strips[1].buses.is_empty(), "the other track has no effects");
}

#[test]
fn a_gap_between_two_clips_ends_a_run_and_the_next_clip_has_an_instance_that_begins_as_it_was_made() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 15, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );

    let plan = plan(&project);

    // The items: clip, clip, silence, clip, clip.
    let strip = &plan.strips[0];
    assert_eq!(strip.items.len(), 5);
    let ranges: Vec<(usize, usize)> = strip.buses.iter().map(|bus| (bus.first_item, bus.last_item)).collect();
    assert_eq!(ranges, [(0, 1), (3, 4)]);
}

#[test]
fn with_one_copy_of_the_effects_for_each_clip_the_effects_are_in_the_chain_of_each_clip_as_they_were_before() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_track_sharing(track, EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();

    let plan = plan(&project);

    let strip = &plan.strips[0];
    assert!(strip.buses.is_empty());
    for clip in clips_of(strip) {
        assert_eq!(values_of(&clip.effects), [PropertyValue::Number(2.0)]);
    }
}

#[test]
fn the_sharing_of_the_picture_effects_of_a_track_changes_nothing_in_the_sound() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let before = plan(&project);

    project
        .set_track_sharing(track, EffectDomain::Picture, TrackSharing::PerClip)
        .unwrap();

    assert_eq!(plan(&project), before);
}

#[test]
fn the_effects_of_the_clip_and_of_its_tags_stay_in_the_chain_of_the_clip_and_those_of_the_track_that_come_after_them_are_the_instance()
 {
    let _alone = the_installed_plugins_to_myself();
    let (own_kind, track_kind) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    for index in 0..2 {
        let clip = clip_at(&project, 0, index);
        put_an_effect_on(
            &mut project,
            AnimationTarget::Clip(clip),
            own_kind,
            &[PropertyValue::Number(3.0)],
        );
    }
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        track_kind,
        &[PropertyValue::Number(2.0)],
    );

    let plan = plan(&project);

    let strip = &plan.strips[0];
    assert_eq!(strip.buses.len(), 1);
    assert_eq!(values_of(&strip.buses[0].effects), [PropertyValue::Number(2.0)]);
    for clip in clips_of(strip) {
        assert_eq!(values_of(&clip.effects), [PropertyValue::Number(3.0)], "the clip's own");
    }
}

#[test]
fn a_clip_that_has_the_badge_of_its_track_in_the_middle_of_its_chain_has_its_own_copy_there_and_does_not_share() {
    let _alone = the_installed_plugins_to_myself();
    let (own_kind, track_kind) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0, 0]);
    let track = project.tracks()[0].id();
    let middle = clip_at(&project, 0, 1);
    let own = put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(middle),
        own_kind,
        &[PropertyValue::Number(3.0)],
    );
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        track_kind,
        &[PropertyValue::Number(2.0)],
    );
    // The badge of the track goes before the clip's own effect: the track's effects are done first, and then the clip's.
    project
        .move_chain_item(
            Member::Clip(middle),
            ChainItemKey::Badge(Badge::Track),
            Some(ChainItemKey::Effect(own)),
        )
        .unwrap();

    let plan = plan(&project);

    let strip = &plan.strips[0];
    let clips = clips_of(strip);
    assert_eq!(
        values_of(&clips[1].effects),
        [PropertyValue::Number(2.0), PropertyValue::Number(3.0)],
        "the clip with the badge in the middle has the track's effects where it is, and then its own"
    );
    assert!(clips[0].effects.is_empty() && clips[2].effects.is_empty());
    let ranges: Vec<(usize, usize)> = strip.buses.iter().map(|bus| (bus.first_item, bus.last_item)).collect();
    assert_eq!(
        ranges,
        [(0, 0), (2, 2)],
        "the clips on either side do not share it with it: they are two runs"
    );
}

#[test]
fn the_instances_are_in_the_specification_that_the_sound_is_made_from() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let plan = plan(&project);
    let sources: Vec<SourceMediaIdentifier> = crate::media::export::sources_of_sound(&plan).into_iter().collect();

    let specification = sound_graph_specification(&plan, &sources, SpedUpClipSound::MaintainPitch);

    let bus = &specification.strips[0].buses;
    assert_eq!(bus.len(), 1);
    assert_eq!((bus[0].first_item, bus[0].last_item), (0, 1));
    assert_eq!(values_of(&bus[0].effects), [PropertyValue::Number(2.0)]);
}

#[test]
fn a_change_of_how_much_of_the_track_one_copy_of_its_sound_effects_covers_is_a_change_of_what_is_heard() {
    let _alone = the_installed_plugins_to_myself();
    let (kind, _) = two_kinds();
    let mut project = a_track_of_clips_with_gaps(&[0, 0]);
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let hash = |project: &Project| {
        let mut hasher = DefaultHasher::new();
        hash_the_sound_effects_into(project, &mut hasher);
        hasher.finish()
    };
    let as_it_is = hash(&project);

    project
        .set_track_sharing(track, EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();

    assert_ne!(hash(&project), as_it_is);
    project
        .set_track_sharing(track, EffectDomain::Picture, TrackSharing::PerTrackIndivisible)
        .unwrap();
    let with_the_sound_alone = hash(&project);
    project
        .set_track_sharing(track, EffectDomain::Picture, TrackSharing::PerRun)
        .unwrap();
    assert_eq!(
        hash(&project),
        with_the_sound_alone,
        "the sharing of the picture effects is not heard"
    );
}
