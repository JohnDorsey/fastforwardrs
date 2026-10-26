// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of an effect that is a shader (phase 6, piece 6.3): an operation with the numbers of the call, the first four of which are the shader's own
//! (`TIME`, `TIMEDELTA`, `FRAMEINDEX`, `PASSINDEX`) and the clock of which is **the clip's, never the wall clock**; one that cannot be run does nothing.

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{EffectKind, GroupKind};
use crate::test_support::make_a_keyframe;

const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [1.0, 0.0, 0.0, 1.0] }, { \"NAME\": \"flip\", \"TYPE\": \"bool\", \"DEFAULT\": true }, { \"NAME\": \"mode\", \"TYPE\": \"long\", \"VALUES\": [3, 7], \"DEFAULT\": 7 }, { \"NAME\": \"spot\", \"TYPE\": \"point2D\", \"DEFAULT\": [10, 20] } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

// (A transition: the kind of shader that this version cannot run. It was a generator before phase 6, which is run now.)
const A_TRANSITION: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"startImage\", \"TYPE\": \"image\" }, { \"NAME\": \"endImage\", \"TYPE\": \"image\" }, { \"NAME\": \"progress\", \"TYPE\": \"float\" } ] }*/\nvoid main() { gl_FragColor = mix(IMG_THIS_PIXEL(startImage), IMG_THIS_PIXEL(endImage), progress); }\n";

/// A filter that wants a second picture of its own, which this version cannot give a shader: the example of one that cannot be run. (Rewritten on purpose in phase 6.6: it was the transition.)
const A_SHADER_THAT_CANNOT_BE_RUN: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"maskImage\", \"TYPE\": \"image\" }, { \"NAME\": \"progress\", \"TYPE\": \"float\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage) * IMG_THIS_PIXEL(maskImage) * progress; }\n";

fn kind_of(name: &str, source: &str) -> EffectKind {
    let file = crate::isf::read(source).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, name, source)).unwrap()
}

fn the_shader_operation(operations: &[PictureOperation]) -> (String, Vec<f32>) {
    match operations {
        [PictureOperation::Shader { key, numbers, .. }] => (key.clone(), numbers.clone()),
        other => panic!("one operation of a shader was expected: {other:?}"),
    }
}

#[test]
fn a_shader_is_an_operation_with_the_four_numbers_of_the_shader_and_then_those_of_its_inputs() {
    let mut project = project_of(vec![vec![(10, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let kind = kind_of("tint.fs", TINT);
    project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    let (key, numbers) = the_shader_operation(&layer.operations_at(frame(10)));

    assert_eq!(key, kind.key());
    let frames_per_second = project.frame_rate().frames_per_second() as f32;
    // The clip begins at frame 10: TIME is 0, TIMEDELTA the length of a frame, FRAMEINDEX 0 and PASSINDEX 0; then amount, the four of the tint, flip, mode and the two of the spot.
    assert_eq!(numbers[0], 0.0);
    assert!((numbers[1] - 1.0 / frames_per_second).abs() < 1e-6);
    assert_eq!(numbers[2], 0.0);
    assert_eq!(numbers[3], 0.0);
    assert_eq!(numbers[4..], [0.25, 1.0, 0.0, 0.0, 1.0, 1.0, 7.0, 10.0, 20.0]);
}

#[test]
fn time_is_the_time_since_the_clip_began_whatever_the_frame_of_the_timeline_is() {
    let mut project = project_of(vec![vec![(10, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project
        .add_effect(AnimationTarget::Clip(clip), kind_of("tint.fs", TINT))
        .unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();
    let frames_per_second = project.frame_rate().frames_per_second();

    for since in [0u64, 1, 15, 59] {
        let (_, numbers) = the_shader_operation(&layer.operations_at(frame(10 + since)));
        assert!(
            (f64::from(numbers[0]) - since as f64 / frames_per_second).abs() < 1e-5,
            "{since} frames in: {numbers:?}"
        );
        assert_eq!(numbers[2], since as f32, "FRAMEINDEX");
    }
    assert_eq!(layer.effects[0].clock.first_frame, 10);
    assert_eq!(layer.effects[0].clock.frames_per_second, frames_per_second);
}

#[test]
fn the_clock_of_an_effect_of_a_track_begins_with_the_timeline() {
    let mut project = project_of(vec![vec![(10, long_video(0, 60, 1))]]);
    let track = project.tracks()[0].id();
    project
        .add_effect(AnimationTarget::Track(track), kind_of("tint.fs", TINT))
        .unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    assert_eq!(layer.effects[0].clock.first_frame, 0);
    let (_, numbers) = the_shader_operation(&layer.operations_at(frame(40)));
    assert_eq!(numbers[2], 40.0);
}

#[test]
fn the_values_of_a_shader_follow_their_keyframes_and_the_clock_goes_on() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let target = AnimationTarget::Clip(clip);
    let kind = kind_of("tint.fs", TINT);
    let instance = project.add_effect(target, kind).unwrap();
    let amount = PropertyId::effect_value(kind, instance, 0);
    make_a_keyframe(&mut project, target, amount, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, target, amount, 40, PropertyValue::Number(1.0));
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    let at = |timeline_frame: u64| the_shader_operation(&layer.operations_at(frame(timeline_frame))).1;

    assert_eq!(at(0)[4], 0.0);
    assert_eq!(at(20)[4], 0.5);
    assert_eq!(at(40)[4], 1.0);
    assert_eq!(at(20)[2], 20.0, "and FRAMEINDEX is the frame");
}

#[test]
fn a_shader_that_cannot_be_run_does_nothing_and_is_not_an_operation() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let cannot_be_run = kind_of("mask.fs", A_SHADER_THAT_CANNOT_BE_RUN);
    assert!(!cannot_be_run.is_installed());
    project.add_effect(AnimationTarget::Clip(clip), cannot_be_run).unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    assert!(layer.operations_at(frame(5)).is_empty());
}

#[test]
fn neighbouring_shaders_and_colour_mixes_stay_what_they_are_and_in_their_order() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let target = AnimationTarget::Clip(clip);
    let kind = kind_of("tint.fs", TINT);
    crate::test_support::put_a_hue_shift_on(&mut project, target, 40.0);
    project.add_effect(target, kind).unwrap();
    project.add_effect(target, kind).unwrap();
    crate::test_support::put_a_hue_shift_on(&mut project, target, 20.0);
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    let operations = layer.operations_at(frame(5));

    let shape: Vec<&str> = operations
        .iter()
        .map(|operation| match operation {
            PictureOperation::Mix(_) => "mix",
            PictureOperation::Shader { .. } => "shader",
            _ => "other",
        })
        .collect();
    assert_eq!(
        shape,
        ["mix", "shader", "shader", "mix"],
        "two shaders are two passes, and the mixes either side of them are not joined"
    );
}

#[test]
fn a_shader_on_a_group_or_a_tag_has_its_clock_from_the_start_of_the_timeline_and_the_clip_that_is_in_a_tag_has_its_own()
{
    let mut project = project_of(vec![vec![(5, long_video(0, 60, 1))], vec![(5, text(60))]]);
    let first = project.tracks()[0].placement_at_index(0).unwrap().id;
    let second = project.tracks()[1].placement_at_index(0).unwrap().id;
    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    project.add_effect_to_the_group(tag, kind_of("tint.fs", TINT)).unwrap();
    let plan = plan(&project);

    for layer in plan.sections.iter().flat_map(|section| &section.layers) {
        assert_eq!(layer.effects.len(), 1, "the effect of the tag is done to each member");
        assert_eq!(layer.effects[0].clock.first_frame, 0);
    }
}

#[test]
fn a_shader_that_is_not_installed_is_no_operation_even_when_it_is_in_the_effects_of_a_layer() {
    // (The plan leaves such an effect out of a layer, so this is the check that holds if one gets there: a layer of effects that are given by hand.)
    let cannot_be_run = kind_of("mask.fs", A_SHADER_THAT_CANNOT_BE_RUN);
    let effect = LayerEffect {
        state: None,
        clock: EffectClock::default(),
        instance: crate::project::EffectInstanceId::FIRST,
        kind: cannot_be_run,
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
    };

    assert_eq!(
        operations_of_effects(&[effect], frame(3), 0),
        Vec::<PictureOperation>::new()
    );
}

#[test]
fn the_operation_of_a_transition_says_that_it_reads_what_is_behind_the_clip_and_that_of_a_filter_does_not() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let target = AnimationTarget::Clip(clip);
    project.add_effect(target, kind_of("tint.fs", TINT)).unwrap();
    project
        .add_effect(target, kind_of("transition.fs", A_TRANSITION))
        .unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    let reads: Vec<bool> = layer
        .operations_at(frame(5))
        .iter()
        .map(|operation| match operation {
            PictureOperation::Shader { reads_the_backdrop, .. } => *reads_the_backdrop,
            other => panic!("a shader was expected: {other:?}"),
        })
        .collect();
    assert_eq!(reads, [false, true]);
}
