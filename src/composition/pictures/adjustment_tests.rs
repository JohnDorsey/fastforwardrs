// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of an adjustment clip (phase 5, piece 5.3; design H7): **it is not a layer, it is a group** made of everything behind it in its container, with its
//! effects. At the frame (and not inside a group) that is over opaque black.

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{AdjustmentClip, GroupKind};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

fn an_adjustment_clip(duration: u64) -> Clip {
    Clip::Adjustment(AdjustmentClip::new(duration).unwrap())
}

/// The clips of the tracks of a project that `project_of` made, front track first: the first clip of each.
fn first_clips(project: &Project, tracks: usize) -> Vec<ClipId> {
    (0..tracks)
        .map(|track| project.tracks()[track].placement_at_index(0).unwrap().id)
        .collect()
}

fn clips_of_the_layers(plan: &PicturePlan, section: usize) -> Vec<ClipId> {
    plan.sections[section].layers.iter().map(|layer| layer.clip).collect()
}

#[test]
fn an_adjustment_clip_is_a_group_of_everything_behind_it_with_its_effects_over_black_and_is_not_a_layer() {
    let mut project = project_of(vec![
        vec![(0, an_adjustment_clip(40))],
        vec![(0, text(40))],
        vec![(0, text(40))],
    ]);
    let [adjustment, middle, back] = first_clips(&project, 3)[..] else {
        panic!()
    };
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 40.0);

    let plan = plan(&project);

    assert_eq!(
        clips_of_the_layers(&plan, 0),
        [back, middle],
        "the adjustment clip is not one of the layers"
    );
    assert_eq!(plan.sections[0].groups.len(), 1);
    let span = &plan.sections[0].groups[0];
    assert_eq!(span.owner, SpanOwner::Adjustment(adjustment));
    assert_eq!(
        (span.first_layer, span.end_layer),
        (0, 2),
        "everything that is behind it"
    );
    assert!(span.over_black, "at the frame, what is behind it is over black");
    assert_eq!(
        span.operations_at(frame(0)),
        [PictureOperation::Mix(ColourMatrix::hue_rotation(40.0))]
    );
    assert_eq!(
        span.effects
            .iter()
            .flat_map(|effect| effect.values.clone())
            .collect::<Vec<_>>(),
        [turn(40.0)]
    );
}

#[test]
fn what_is_in_front_of_an_adjustment_clip_is_not_in_its_group() {
    let mut project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, an_adjustment_clip(40))],
        vec![(0, text(40))],
    ]);
    let [in_front, adjustment, behind] = first_clips(&project, 3)[..] else {
        panic!()
    };
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 40.0);

    let plan = plan(&project);

    assert_eq!(clips_of_the_layers(&plan, 0), [behind, in_front]);
    let span = &plan.sections[0].groups[0];
    assert_eq!((span.first_layer, span.end_layer), (0, 1), "only what is behind it");
}

#[test]
fn an_adjustment_clip_with_no_effect_or_with_nothing_behind_it_does_nothing() {
    // No effect: the layers are those there would be without it.
    let project = project_of(vec![vec![(0, an_adjustment_clip(40))], vec![(0, text(40))]]);
    let [_, behind] = first_clips(&project, 2)[..] else {
        panic!()
    };
    let without_effect = plan(&project);
    assert_eq!(clips_of_the_layers(&without_effect, 0), [behind]);
    assert!(without_effect.sections[0].groups.is_empty());

    // Nothing behind it: nothing to change.
    let mut alone = project_of(vec![vec![(0, an_adjustment_clip(40))]]);
    let adjustment = first_clips(&alone, 1)[0];
    put_a_hue_shift_on(&mut alone, AnimationTarget::Clip(adjustment), 40.0);
    let plan = plan(&alone);
    assert!(
        plan.sections
            .iter()
            .all(|section| section.layers.is_empty() && section.groups.is_empty())
    );
}

#[test]
fn an_adjustment_clip_is_in_the_sections_where_it_is_and_the_sections_round_it_are_not_joined_to_those() {
    let mut project = project_of(vec![vec![(10, an_adjustment_clip(10))], vec![(0, text(40))]]);
    let adjustment = first_clips(&project, 2)[0];
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 40.0);

    let plan = plan(&project);

    let seen: Vec<(u64, u64, usize)> = plan
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
    assert_eq!(seen, [(0, 10, 0), (10, 20, 1), (20, 40, 0)]);
}

#[test]
fn an_adjustment_clip_in_a_clip_group_that_has_an_effect_adjusts_only_what_is_behind_it_in_the_group_and_not_over_black()
 {
    // The front track has an adjustment clip and the middle track a text object, in a clip group that has an effect; the back track has a text object that is not in it.
    let mut project = project_of(vec![
        vec![(0, an_adjustment_clip(40))],
        vec![(0, text(40))],
        vec![(0, text(40))],
    ]);
    let [adjustment, in_the_group, outside] = first_clips(&project, 3)[..] else {
        panic!()
    };
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(adjustment), Member::Clip(in_the_group)],
        )
        .unwrap();
    project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(group, crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT, &turn(90.0))
        .unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 40.0);

    let plan = plan(&project);

    assert_eq!(clips_of_the_layers(&plan, 0), [outside, in_the_group]);
    let spans: Vec<(SpanOwner, usize, usize, bool)> = plan.sections[0]
        .groups
        .iter()
        .map(|span| (span.owner, span.first_layer, span.end_layer, span.over_black))
        .collect();
    assert_eq!(
        spans,
        [
            (SpanOwner::Group(group), 1, 2, false),
            (SpanOwner::Adjustment(adjustment), 1, 2, false)
        ],
        "the text outside the group is behind the group and is not adjusted, and what is behind the adjustment inside the group is not over black"
    );
}

#[test]
fn an_adjustment_clip_that_keeps_what_is_opaque_opaque_over_a_video_that_covers_the_picture_leaves_out_what_is_behind_the_video()
 {
    let mut project = project_of(vec![
        vec![(0, an_adjustment_clip(40))],
        vec![(0, long_video(0, 40, 1))],
        vec![(0, text(40))],
    ]);
    let [adjustment, video, _behind] = first_clips(&project, 3)[..] else {
        panic!()
    };
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 40.0);

    let plan = plan(&project);

    assert_eq!(
        clips_of_the_layers(&plan, 0),
        [video],
        "the text object behind the video is not seen"
    );
    assert_eq!(plan.sections[0].groups.len(), 1);
}

#[test]
fn the_values_of_the_effects_of_an_adjustment_clip_can_have_keyframes_and_the_group_has_them_at_every_frame() {
    let mut project = project_of(vec![vec![(0, an_adjustment_clip(100))], vec![(0, text(100))]]);
    let adjustment = first_clips(&project, 2)[0];
    let effect = put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 0.0);
    let hue = PropertyId::effect_value(EffectKind::HUE_SHIFT, effect, 0);
    make_a_keyframe(&mut project, AnimationTarget::Clip(adjustment), hue, 0, turn(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(adjustment), hue, 50, turn(100.0));

    let plan = plan(&project);

    let span = &plan.sections[0].groups[0];
    assert!(span.effects[0].is_animated());
    assert_eq!(
        span.operations_at(frame(0)),
        Vec::<PictureOperation>::new(),
        "a hue shift of nought does nothing at the first frame, and the group is still there"
    );
    assert_eq!(
        span.operations_at(frame(25)),
        [PictureOperation::Mix(ColourMatrix::hue_rotation(50.0))]
    );
    assert_eq!(
        span.operations_at(frame(75)),
        [PictureOperation::Mix(ColourMatrix::hue_rotation(100.0))],
        "after the last keyframe it stays"
    );
}
