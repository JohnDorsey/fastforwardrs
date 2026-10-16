// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the picture plan makes of the effects of groups: each layer carries, among its effects, those of the tags its clip is in,
//! and (as a placeholder, see `project::groups::effects_of_groups`) those of the clip groups and track groups that the clip is inside.

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::colour_effects::ColourMatrix;
use crate::composition::test_projects::project_of;
use crate::project::{Badge, ChainItemKey, EffectKind, GroupKind};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

const HUE: PropertyId = crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT;

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

/// Three tracks with a text object on each at frames 0 to 40; the first track is in front. (`clips[n]` is the clip on track `n`.)
fn three_texts() -> (Project, [ClipId; 3]) {
    let project = project_of(vec![vec![(0, text(40))], vec![(0, text(40))], vec![(0, text(40))]]);
    let clips = [0, 1, 2].map(|track| project.tracks()[track].placement_at_index(0).unwrap().id);
    (project, clips)
}

/// The values of the effects of the layer of `clip` in the first section, in the order they are applied in.
fn values_of_the_effects_of(plan: &PicturePlan, clip: ClipId) -> Vec<PropertyValue> {
    plan.sections[0]
        .layers
        .iter()
        .find(|layer| layer.clip == clip)
        .expect("the clip is seen in the first section")
        .effects
        .iter()
        .flat_map(|effect| effect.values.clone())
        .collect()
}

fn put_an_effect_of_a_group(project: &mut Project, group: crate::project::GroupId, degrees: f32) {
    project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(group, HUE, &turn(degrees))
        .unwrap();
}

#[test]
fn the_layer_of_each_clip_that_is_in_a_tag_has_the_effects_of_the_tag_and_no_other_layer_does() {
    let (mut project, [first, second, third]) = three_texts();
    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(third)])
        .unwrap();
    put_an_effect_of_a_group(&mut project, tag, 60.0);

    let plan = plan(&project);
    assert_eq!(values_of_the_effects_of(&plan, first), [turn(60.0)]);
    assert_eq!(values_of_the_effects_of(&plan, third), [turn(60.0)]);
    assert!(
        values_of_the_effects_of(&plan, second).is_empty(),
        "between them, and not in the tag"
    );
}

#[test]
fn the_effects_of_a_tag_are_applied_where_its_badge_is_in_the_chain_of_the_clip_and_the_colours_change_in_that_order() {
    let (mut project, [first, ..]) = three_texts();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(first), 30.0);
    let front_track = project.tracks()[0].id();
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(front_track), 15.0);
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(first)]).unwrap();
    put_an_effect_of_a_group(&mut project, tag, 60.0);

    assert_eq!(
        values_of_the_effects_of(&plan(&project), first),
        [turn(30.0), turn(60.0), turn(15.0)],
        "its own, the tag's (its badge came after the effect), and then those of its track"
    );
    project
        .move_chain_item(
            Member::Clip(first),
            ChainItemKey::Badge(Badge::Tag(tag)),
            Some(ChainItemKey::Effect(crate::project::EffectInstanceId::FIRST)),
        )
        .unwrap();
    let plan = plan(&project);
    assert_eq!(
        values_of_the_effects_of(&plan, first),
        [turn(60.0), turn(30.0), turn(15.0)]
    );
    let layer = plan.sections[0]
        .layers
        .iter()
        .find(|layer| layer.clip == first)
        .unwrap();
    assert_eq!(
        layer.colour_matrix(),
        ColourMatrix::hue_rotation(60.0)
            .then(ColourMatrix::hue_rotation(30.0))
            .then(ColourMatrix::hue_rotation(15.0))
    );
}

#[test]
fn the_effects_of_a_clip_group_are_applied_to_the_layer_of_each_member_after_its_own() {
    let (mut project, [first, second, third]) = three_texts();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(first), 30.0);
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    put_an_effect_of_a_group(&mut project, group, 90.0);

    let plan = plan(&project);
    assert_eq!(values_of_the_effects_of(&plan, first), [turn(30.0), turn(90.0)]);
    assert_eq!(values_of_the_effects_of(&plan, second), [turn(90.0)]);
    assert!(values_of_the_effects_of(&plan, third).is_empty());
    let layer = plan.sections[0]
        .layers
        .iter()
        .find(|layer| layer.clip == first)
        .unwrap();
    assert_eq!(
        layer.colour_matrix(),
        ColourMatrix::hue_rotation(30.0).then(ColourMatrix::hue_rotation(90.0))
    );
}

#[test]
fn the_effects_of_a_track_group_are_applied_to_every_clip_on_its_tracks_after_those_of_the_clip_groups() {
    let (mut project, [first, second, third]) = three_texts();
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let tracks: Vec<Member> = project.tracks()[..2]
        .iter()
        .map(|track| Member::Track(track.id()))
        .collect();
    let track_group = project.create_group(GroupKind::TrackGroup, &tracks).unwrap();
    put_an_effect_of_a_group(&mut project, track_group, 45.0);
    put_an_effect_of_a_group(&mut project, clip_group, 90.0);

    let plan = plan(&project);
    assert_eq!(
        values_of_the_effects_of(&plan, first),
        [turn(90.0), turn(45.0)],
        "the nearest group first"
    );
    assert_eq!(values_of_the_effects_of(&plan, second), [turn(90.0), turn(45.0)]);
    assert!(
        values_of_the_effects_of(&plan, third).is_empty(),
        "on a track that is not in the group"
    );
}

#[test]
fn the_keyframes_of_a_clips_effect_stay_with_the_clips_effect_and_the_effect_of_a_group_has_none() {
    let mut project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), HUE, 50, turn(100.0));
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap();
    put_an_effect_of_a_group(&mut project, tag, 20.0);

    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];
    assert_eq!(layer.effects.len(), 2);
    assert!(
        layer.effects[0].is_animated(),
        "the clip's own is animated by the clip's keyframes"
    );
    assert!(!layer.effects[1].is_animated(), "the tag's is not animated");
    assert!(layer.has_keyframes());
    assert_eq!(layer.effects[1].values_at(70), [turn(20.0)]);
    assert_eq!(layer.effects[0].values_at(25), [turn(50.0)]);
    assert_eq!(
        layer.colour_matrix_at(frame(25)),
        ColourMatrix::hue_rotation(50.0).then(ColourMatrix::hue_rotation(20.0))
    );
}

#[test]
fn a_clip_that_is_cut_into_sections_by_what_is_over_it_has_the_effects_of_its_groups_in_each_of_them() {
    // The front track has a text object at frames 10 to 20, which cuts what is behind it into three sections.
    let mut project = project_of(vec![vec![(10, text(10))], vec![(0, text(40))]]);
    let behind = project.tracks()[1].placement_at_index(0).unwrap().id;
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(behind)]).unwrap();
    put_an_effect_of_a_group(&mut project, tag, 70.0);

    let plan = plan(&project);
    assert!(plan.sections.len() >= 3, "{}", plan.sections.len());
    for section in &plan.sections {
        let layer = section
            .layers
            .iter()
            .find(|layer| layer.clip == behind)
            .expect("it is seen throughout");
        assert_eq!(layer.effects.len(), 1);
        assert_eq!(layer.effects[0].values, [turn(70.0)]);
    }
}

#[test]
fn a_project_with_groups_that_have_no_effects_makes_the_plan_that_it_would_without_them() {
    let (mut project, [first, second, third]) = three_texts();
    let without = plan(&project);
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(third)])
        .unwrap();
    let tracks: Vec<Member> = project.tracks().iter().map(|track| Member::Track(track.id())).collect();
    project.create_group(GroupKind::TrackGroup, &tracks).unwrap();
    assert_eq!(plan(&project), without);
}
