// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The clip groups and track groups that have sound effects (phase 12, piece 12.3; docs/engine-phase-12-design.md, sections 2 and 8): which of them are groups of the sound plan, where the sum of
//! each goes, which clips are sent to which, and what a group's edge does to the runs of a track.

use std::hash::{DefaultHasher, Hasher};

use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::project::tracks::Clip;
use crate::project::{
    EffectKind, GroupKind, InstalledPlugin, Member, PropertyValue, TailLength, TrackSharing, set_installed_plugins,
};
use crate::test_support::{
    an_installed_sound_plugin, put_an_effect_on, put_an_effect_on_a_group, the_installed_plugins_to_myself,
};

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

/// A sound plugin that is installed, as a kind of effect.
fn a_kind() -> EffectKind {
    let plugin: InstalledPlugin =
        an_installed_sound_plugin("ladspa:a:groups", "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]);
    set_installed_plugins(vec![plugin]);
    EffectKind::installed_plugin_kinds()[0]
}

/// Track 0 has three sound clips of 30 frames one after the other (0 to 90); track 1 has one of 60 frames (0 to 60); track 2 has one of 90 frames (0 to 90).
fn three_tracks() -> Project {
    project_of(vec![
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 90, 1)))],
    ])
}

fn clip_at(project: &Project, track: usize, index: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(index).unwrap().id
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

/// Where each clip of each strip is sent to, strip by strip.
fn sent_to_by_strip(plan: &SoundPlan) -> Vec<Vec<Option<usize>>> {
    plan.strips
        .iter()
        .map(|strip| clips_of(strip).iter().map(|clip| clip.sent_to).collect())
        .collect()
}

#[test]
fn where_no_group_has_a_sound_effect_the_plan_has_no_groups_and_every_clip_goes_to_the_sum_of_all_the_sound() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = three_tracks();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    project
        .add_effect_to_the_group(project.groups()[0].id(), EffectKind::HUE_SHIFT)
        .expect("a picture effect on a group");

    let plan = plan(&project);

    assert_eq!(
        plan.groups,
        Vec::new(),
        "a group with a picture effect and no sound effect is pass-through in the sound"
    );
    assert!(sent_to_by_strip(&plan).iter().flatten().all(Option::is_none));
}

#[test]
fn a_clip_group_that_has_a_sound_effect_is_a_group_of_the_plan_and_its_clips_are_sent_to_it_and_the_others_are_not() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);

    let plan = plan(&project);

    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].group, group);
    assert_eq!(
        plan.groups[0].parent, None,
        "the sum of the group goes into the sum of all the sound"
    );
    assert_eq!(plan.groups[0].effects.len(), 1);
    assert_eq!(plan.groups[0].effects[0].values[0], PropertyValue::Number(3.0));
    assert_eq!(
        sent_to_by_strip(&plan),
        [vec![Some(0), Some(0), None], vec![None], vec![None]],
        "the two clips of the group, and none of the others"
    );
    assert!(
        plan.strips
            .iter()
            .flat_map(clips_of)
            .all(|clip| clip.effects.is_empty()),
        "the effect of the group is not in the chain of any clip"
    );
}

#[test]
fn a_sound_effect_that_is_switched_off_makes_no_group_of_the_plan() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let instance = put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    assert_eq!(plan(&project).groups.len(), 1, "a premise");

    project
        .set_effect_enabled(Member::Group(group), instance, false)
        .unwrap();

    let plan = plan(&project);
    assert_eq!(plan.groups, Vec::new());
    assert!(sent_to_by_strip(&plan).iter().flatten().all(Option::is_none));
}

#[test]
fn a_track_group_sends_all_the_clips_of_its_tracks_to_it_and_a_clip_group_in_it_is_sent_to_it_too() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let (track_0, track_1) = (project.tracks()[0].id(), project.tracks()[1].id());
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let track_group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track_0), Member::Track(track_1)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, clip_group, kind, &[PropertyValue::Number(1.0)]);
    put_an_effect_on_a_group(&mut project, track_group, kind, &[PropertyValue::Number(2.0)]);

    let plan = plan(&project);

    let place_of = |group| plan.groups.iter().position(|node| node.group == group).unwrap();
    let (inner, outer) = (place_of(clip_group), place_of(track_group));
    assert_eq!(
        plan.groups[inner].parent,
        Some(outer),
        "the clip group's sum goes into the track group's"
    );
    assert_eq!(plan.groups[outer].parent, None);
    assert_eq!(
        sent_to_by_strip(&plan),
        [
            vec![Some(inner), Some(inner), Some(outer)],
            vec![Some(outer)],
            vec![None],
        ],
        "a clip goes to the nearest group it is inside; a track that is in no group goes to the sum"
    );
}

#[test]
fn a_clip_group_in_a_clip_group_is_sent_to_the_one_round_it_and_a_group_with_no_sound_effect_between_them_is_passed_over()
 {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let (first, second, other) = (
        clip_at(&project, 0, 0),
        clip_at(&project, 0, 1),
        clip_at(&project, 1, 0),
    );
    let inner = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    // (A clip group that has only a picture effect, round the first: the sound goes through it as if it was not there.)
    let middle = project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), Member::Clip(other)])
        .unwrap();
    project.add_effect_to_the_group(middle, EffectKind::HUE_SHIFT).unwrap();
    let third = clip_at(&project, 0, 2);
    let outer = project
        .create_group(GroupKind::ClipGroup, &[Member::Group(middle), Member::Clip(third)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, inner, kind, &[PropertyValue::Number(1.0)]);
    put_an_effect_on_a_group(&mut project, outer, kind, &[PropertyValue::Number(2.0)]);

    let plan = plan(&project);

    assert_eq!(plan.groups.len(), 2, "the middle one is not a group of the sound");
    let place_of = |group| plan.groups.iter().position(|node| node.group == group).unwrap();
    assert_eq!(plan.groups[place_of(inner)].parent, Some(place_of(outer)));
    assert_eq!(plan.groups[place_of(outer)].parent, None);
    assert_eq!(
        sent_to_by_strip(&plan),
        [
            vec![Some(place_of(inner)), Some(place_of(inner)), Some(place_of(outer))],
            vec![Some(place_of(outer))],
            vec![None],
        ]
    );
}

#[test]
fn the_edge_of_a_group_ends_a_run_of_the_effects_of_the_track_and_clips_that_go_to_the_same_place_are_one() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let ranges = |project: &Project| -> Vec<(usize, usize)> {
        plan(project).strips[0]
            .buses
            .iter()
            .map(|bus| (bus.first_item, bus.last_item))
            .collect()
    };
    assert_eq!(
        ranges(&project),
        [(0, 2)],
        "a premise: three clips in a row, one instance of the track's effects"
    );

    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    assert_eq!(
        ranges(&project),
        [(0, 2)],
        "a group that has no sound effect is not an edge"
    );
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);

    assert_eq!(
        ranges(&project),
        [(0, 1), (2, 2)],
        "the two clips of the group are an instance of the track's effects, and the clip outside it another: a tail does not cross the edge"
    );
}

#[test]
fn with_one_copy_of_the_effects_of_the_track_for_each_clip_a_group_changes_no_run_because_there_is_none() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
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
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);

    let plan = plan(&project);

    assert!(plan.strips[0].buses.is_empty());
    assert!(clips_of(&plan.strips[0]).iter().all(|clip| clip.effects.len() == 1));
}

#[test]
fn what_the_effects_of_a_group_ring_on_for_is_in_the_lead_in_of_a_plan_that_begins_after_the_clips() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let instance = put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    let lead_in = |project: &Project| plan_sound(project, TimelineFrameIndex::new(60), |_| true).lead_in_frames;

    project
        .set_effect_tail_length(Member::Group(group), instance, TailLength::Milliseconds(1_000))
        .unwrap();
    assert_eq!(lead_in(&project), 30, "a second is thirty frames");

    project
        .set_effect_tail_length(Member::Group(group), instance, TailLength::None)
        .unwrap();
    assert_eq!(lead_in(&project), 0, "no tail, no lead-in");
}

#[test]
fn a_change_of_what_a_group_does_to_the_sound_is_a_change_of_the_hash_of_the_sound_effects() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = three_tracks();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let instance = put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    let hash_of = |project: &Project| {
        let mut hasher = DefaultHasher::new();
        hash_the_sound_effects_into(project, &mut hasher);
        hasher.finish()
    };
    let before = hash_of(&project);

    project
        .set_effect_value_of_the_group(
            group,
            PropertyId::effect_value(kind, instance, 0),
            &PropertyValue::Number(1.0),
        )
        .unwrap();
    let with_another_value = hash_of(&project);
    assert_ne!(before, with_another_value);

    project
        .set_effect_enabled(Member::Group(group), instance, false)
        .unwrap();
    assert_ne!(with_another_value, hash_of(&project), "switching it off is heard");
}
