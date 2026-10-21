// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of what the picture plan makes of the effects of groups: each layer carries, among its effects, those of the tags its clip is in; and the clip groups and track groups that have
//! an effect that does something are **groups that make a picture of their own** (phase 5): a span of the section's layers, with the effects of the group, which are done to the
//! picture that the layers make together. (They were applied to each member as a placeholder until then.)

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

/// The groups of the first section, each as (the group, the layers it holds as clips, the values of its effects).
fn the_groups_of_the_first_section(
    plan: &PicturePlan,
) -> Vec<(crate::project::GroupId, Vec<ClipId>, Vec<PropertyValue>)> {
    let section = &plan.sections[0];
    section
        .groups
        .iter()
        .map(|span| {
            (
                match span.owner {
                    SpanOwner::Group(group) => group,
                    SpanOwner::Adjustment(clip) => panic!("an adjustment clip: {clip:?}"),
                },
                section.layers[span.first_layer..span.end_layer]
                    .iter()
                    .map(|layer| layer.clip)
                    .collect(),
                span.effects.iter().flat_map(|effect| effect.values.clone()).collect(),
            )
        })
        .collect()
}

// (Rewritten on purpose in phase 5: the effects of a clip group were in the effects of the layer of each member, after its own, as a placeholder. They are a group now: a span of the
// layers, with the effects done to the picture that the layers make together, and the layers' own effects are what they were without the group.)
#[test]
fn the_effects_of_a_clip_group_are_the_effects_of_a_group_of_the_layers_of_its_members_and_not_of_each_layer() {
    let (mut project, [first, second, third]) = three_texts();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(first), 30.0);
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    put_an_effect_of_a_group(&mut project, group, 90.0);

    let plan = plan(&project);

    assert_eq!(
        values_of_the_effects_of(&plan, first),
        [turn(30.0)],
        "its own, and nothing of the group's"
    );
    assert!(values_of_the_effects_of(&plan, second).is_empty());
    assert!(values_of_the_effects_of(&plan, third).is_empty());
    // (The layers are back to front: the third, which is on the back-most track, then the second and the first, which are the group's.)
    assert_eq!(
        plan.sections[0]
            .layers
            .iter()
            .map(|layer| layer.clip)
            .collect::<Vec<_>>(),
        [third, second, first]
    );
    assert_eq!(
        the_groups_of_the_first_section(&plan),
        [(group, vec![second, first], vec![turn(90.0)])]
    );
    assert_eq!(
        plan.sections[0].groups[0].operations_at(frame(0)),
        [PictureOperation::Mix(ColourMatrix::hue_rotation(90.0))]
    );
}

#[test]
fn a_track_group_with_a_clip_group_in_it_is_a_group_that_holds_a_group_and_each_has_its_own_effects() {
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
        the_groups_of_the_first_section(&plan),
        [
            (track_group, vec![second, first], vec![turn(45.0)]),
            (clip_group, vec![second, first], vec![turn(90.0)]),
        ],
        "the one that holds the other is first"
    );
    for clip in [first, second, third] {
        assert!(values_of_the_effects_of(&plan, clip).is_empty(), "{clip:?}");
    }
}

#[test]
fn a_group_with_no_effect_that_does_anything_is_not_a_group_of_the_plan_and_its_layers_are_where_they_were() {
    let (mut project, [first, second, _]) = three_texts();
    let plain = plan(&project);
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    assert!(plan(&project).sections[0].groups.is_empty(), "no effects: pass-through");

    put_an_effect_of_a_group(&mut project, group, 0.0);
    assert!(
        plan(&project).sections[0].groups.is_empty(),
        "a hue shift of nought degrees does nothing"
    );
    assert_eq!(plan(&project).sections[0].layers, plain.sections[0].layers);

    let second_effect = project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(
            group,
            PropertyId::effect_value(EffectKind::HUE_SHIFT, second_effect, 0),
            &turn(40.0),
        )
        .unwrap();
    assert_eq!(
        plan(&project).sections[0].groups.len(),
        1,
        "one that does something is enough"
    );
    project
        .set_effect_enabled(Member::Group(group), second_effect, false)
        .unwrap();
    assert!(
        plan(&project).sections[0].groups.is_empty(),
        "and when it is switched off the group is not one"
    );
}

#[test]
fn a_group_is_in_the_sections_where_its_clips_are_and_the_sections_with_different_groups_are_not_joined() {
    // The back track has a text object at frames 0 to 40; a text object of the front track, at frames 10 to 20, is in a clip group with it.
    let mut project = project_of(vec![vec![(10, text(10))], vec![(0, text(40))]]);
    let in_front = project.tracks()[0].placement_at_index(0).unwrap().id;
    let behind = project.tracks()[1].placement_at_index(0).unwrap().id;
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(in_front), Member::Clip(behind)])
        .unwrap();
    put_an_effect_of_a_group(&mut project, group, 90.0);

    let plan = plan(&project);

    let with_the_group: Vec<(u64, u64, usize)> = plan
        .sections
        .iter()
        .map(|section| {
            (
                section.first_timeline_frame,
                section.end_timeline_frame,
                section.groups.len(),
            )
        })
        .collect();
    assert_eq!(
        with_the_group,
        [(0, 10, 1), (10, 20, 1), (20, 40, 1)],
        "the group is in every section (its clips are), and what it holds is what is there"
    );
    assert_eq!(
        (
            plan.sections[0].groups[0].first_layer,
            plan.sections[0].groups[0].end_layer
        ),
        (0, 1)
    );
    assert_eq!(
        (
            plan.sections[1].groups[0].first_layer,
            plan.sections[1].groups[0].end_layer
        ),
        (0, 2)
    );
}

#[test]
fn what_is_behind_a_video_that_covers_the_picture_is_left_out_inside_a_group_and_a_group_that_keeps_it_opaque_covers_what_is_behind_it()
 {
    // Three tracks: a text object at the back, then a long video that covers the picture, then another text object in front; the last two are in a clip group.
    let mut project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, long_video(0, 40, 1))],
        vec![(0, text(40))],
    ]);
    let [front_text, video, back_text] =
        [0, 1, 2].map(|track| project.tracks()[track].placement_at_index(0).unwrap().id);
    let _ = back_text;
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(front_text), Member::Clip(video)])
        .unwrap();
    let clips_of = |plan: &PicturePlan| {
        plan.sections[0]
            .layers
            .iter()
            .map(|layer| layer.clip)
            .collect::<Vec<_>>()
    };

    // A hue shift keeps what is opaque opaque: the group covers the picture, and the text object at the very back is left out.
    put_an_effect_of_a_group(&mut project, group, 90.0);
    assert_eq!(
        clips_of(&plan(&project)),
        [video, front_text],
        "what is behind the group is left out"
    );

    // A blur does not: what is behind the group may show at its soft edges, so it is kept; what is behind the video inside the group has nothing there.
    let blur = project.add_effect_to_the_group(group, EffectKind::BLUR).unwrap();
    project
        .set_effect_value_of_the_group(group, PropertyId::effect_value(EffectKind::BLUR, blur, 0), &turn(12.0))
        .unwrap();
    project
        .remove_effect_from_the_group(group, crate::project::EffectInstanceId::FIRST)
        .unwrap();
    let kept = plan(&project);
    assert_eq!(
        clips_of(&kept),
        [back_text, video, front_text],
        "the group has a blur, which does not keep the edges opaque: what is behind the group is kept"
    );
    assert_eq!(
        (
            kept.sections[0].groups[0].first_layer,
            kept.sections[0].groups[0].end_layer
        ),
        (1, 3)
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

#[test]
fn a_group_covers_the_picture_only_if_every_one_of_its_effects_keeps_what_is_opaque_opaque() {
    // A long video over a text object, in a clip group with a hue shift and a blur: the blur may make the edges clear, so the group does not cover the picture and what is
    // behind it (a text object on the back-most track) is kept.
    let mut project = project_with_a_covering_video_in_a_group();
    let (group, back_text) = (project.0, project.2);
    let project = &mut project.1;
    put_an_effect_of_a_group(project, group, 90.0);
    let blur = project.add_effect_to_the_group(group, EffectKind::BLUR).unwrap();
    project
        .set_effect_value_of_the_group(group, PropertyId::effect_value(EffectKind::BLUR, blur, 0), &turn(12.0))
        .unwrap();
    let kept = plan(project);
    assert!(
        kept.sections[0].layers.iter().any(|layer| layer.clip == back_text),
        "a hue shift and a blur: the blur does not keep the edges opaque"
    );

    // Take the blur away and the hue shift alone does: the group covers the picture, and what is behind it is left out.
    project.remove_effect_from_the_group(group, blur).unwrap();
    let left_out = plan(project);
    assert!(left_out.sections[0].layers.iter().all(|layer| layer.clip != back_text));
}

/// Three tracks: a text object on the back-most one, a long video that covers the picture, and a text object in front, the last two in a clip group. Gives the group, the
/// project and the clip at the back.
fn project_with_a_covering_video_in_a_group() -> (crate::project::GroupId, Project, ClipId) {
    let mut project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, long_video(0, 40, 1))],
        vec![(0, text(40))],
    ]);
    let [front_text, video, back_text] =
        [0, 1, 2].map(|track| project.tracks()[track].placement_at_index(0).unwrap().id);
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(front_text), Member::Clip(video)])
        .unwrap();
    (group, project, back_text)
}

#[test]
fn two_things_that_cover_the_picture_in_one_container_leave_out_what_is_behind_the_front_most() {
    // Two long videos, the front one in a clip group with a text object: a hue shift on the group keeps it opaque, so it covers, and the video behind it is left out.
    let mut project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, long_video(0, 40, 1))],
        vec![(0, long_video(0, 40, 1))],
    ]);
    let [front_text, front_video, back_video] =
        [0, 1, 2].map(|track| project.tracks()[track].placement_at_index(0).unwrap().id);
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(front_text), Member::Clip(front_video)],
        )
        .unwrap();
    put_an_effect_of_a_group(&mut project, group, 90.0);

    let plan = plan(&project);

    let clips: Vec<ClipId> = plan.sections[0].layers.iter().map(|layer| layer.clip).collect();
    assert_eq!(
        clips,
        [front_video, front_text],
        "the video behind the group that covers the picture is not seen"
    );
    assert_ne!(back_video, front_video);
}

#[test]
fn a_track_group_in_a_track_group_is_the_outer_group_that_holds_the_inner_one() {
    let (mut project, [first, second, third]) = three_texts();
    let track_members = |range: std::ops::Range<usize>, project: &Project| -> Vec<Member> {
        project.tracks()[range]
            .iter()
            .map(|track| Member::Track(track.id()))
            .collect()
    };
    let inner = project
        .create_group(GroupKind::TrackGroup, &track_members(0..2, &project))
        .unwrap();
    let mut members = track_members(2..3, &project);
    members.push(Member::Group(inner));
    let outer = project.create_group(GroupKind::TrackGroup, &members).unwrap();
    put_an_effect_of_a_group(&mut project, inner, 45.0);
    put_an_effect_of_a_group(&mut project, outer, 90.0);

    let plan = plan(&project);

    assert_eq!(
        the_groups_of_the_first_section(&plan),
        [
            (outer, vec![third, second, first], vec![turn(90.0)]),
            (inner, vec![second, first], vec![turn(45.0)]),
        ],
        "the outer group, which holds the inner one, is first"
    );
}

#[test]
fn a_group_whose_effect_is_an_exposure_covers_the_picture_as_one_with_a_hue_shift_does() {
    let (group, mut project, back_text) = project_with_a_covering_video_in_a_group();
    project.add_effect_to_the_group(group, EffectKind::EXPOSURE).unwrap();
    project
        .set_effect_value_of_the_group(
            group,
            PropertyId::effect_value(EffectKind::EXPOSURE, crate::project::EffectInstanceId::FIRST, 0),
            &turn(1.0),
        )
        .unwrap();

    let plan = plan(&project);

    assert_eq!(plan.sections[0].groups.len(), 1, "the exposure does something");
    assert!(
        plan.sections[0].layers.iter().all(|layer| layer.clip != back_text),
        "an exposure keeps what is opaque opaque, so the group covers what is behind it"
    );
}
