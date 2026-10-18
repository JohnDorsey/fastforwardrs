// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Conformance of the built-in effects (phase 4, piece 4.8): what every effect the program has does at **the ends of the ranges of its values**, and that **every number of every
//! effect follows its keyframes at every frame**. The other tests of this folder look at one effect at a time with the values their authors chose; these go through
//! [`EffectKind::BUILT_IN`] and the descriptors, so an effect that is added later is held to the same things without anyone remembering to.

use super::tests::{frame, long_video};
use super::*;
use crate::colour_effects::ColourMatrix;
use crate::composition::test_projects::project_of;
use crate::project::{AnimationTarget, EffectKind, PictureOperation, PropertyId, PropertyValue};
use crate::test_support::make_a_keyframe;

fn number(value: f32) -> PropertyValue {
    PropertyValue::Number(value)
}

fn a_video() -> (Project, ClipId) {
    let project = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).expect("a clip").id;
    (project, clip)
}

fn operations_at(project: &Project, timeline_frame: u64) -> Vec<PictureOperation> {
    let plan = super::tests::plan(project);
    plan.sections[0].layers[0].operations_at(frame(timeline_frame))
}

/// The values an effect begins with, but for the value in `slot`, which is `value`.
fn values_but_one(kind: EffectKind, slot: usize, value: f32) -> Vec<PropertyValue> {
    let mut values = kind.default_values();
    values[slot] = number(value);
    values
}

/// The operation that an effect asks for when its value in `slot` is `value` and the others are what it begins with, as the effect says it, or nothing where it changes nothing.
fn operation_with(kind: EffectKind, slot: usize, value: f32) -> Option<PictureOperation> {
    kind.picture_operation(&values_but_one(kind, slot, value))
        .filter(|operation| !operation.changes_nothing())
}

#[test]
fn at_both_ends_of_the_range_of_each_value_each_effect_does_what_the_arithmetic_of_the_effect_says() {
    let hue = EffectKind::HUE_SHIFT;
    assert_eq!(
        operation_with(hue, 0, -180.0),
        Some(PictureOperation::Mix(ColourMatrix::hue_rotation(-180.0)))
    );
    assert_eq!(
        operation_with(hue, 0, 180.0),
        Some(PictureOperation::Mix(ColourMatrix::hue_rotation(180.0)))
    );

    let saturation = EffectKind::SATURATION;
    assert_eq!(
        operation_with(saturation, 0, 0.0),
        Some(PictureOperation::Mix(ColourMatrix::saturation(0.0))),
        "none of the colour is a grey"
    );
    assert_eq!(
        operation_with(saturation, 0, 2.0),
        Some(PictureOperation::Mix(ColourMatrix::saturation(2.0)))
    );

    let light = EffectKind::BRIGHTNESS_AND_CONTRAST;
    assert_eq!(
        operation_with(light, 0, -1.0),
        Some(PictureOperation::Mix(ColourMatrix::brightness_and_contrast(-1.0, 1.0)))
    );
    assert_eq!(
        operation_with(light, 0, 1.0),
        Some(PictureOperation::Mix(ColourMatrix::brightness_and_contrast(1.0, 1.0)))
    );
    assert_eq!(
        operation_with(light, 1, 0.0),
        Some(PictureOperation::Mix(ColourMatrix::brightness_and_contrast(0.0, 0.0))),
        "no contrast is all middle grey"
    );
    assert_eq!(
        operation_with(light, 1, 2.0),
        Some(PictureOperation::Mix(ColourMatrix::brightness_and_contrast(0.0, 2.0)))
    );

    let exposure = EffectKind::EXPOSURE;
    assert_eq!(
        operation_with(exposure, 0, -5.0),
        Some(PictureOperation::Exposure { stops: -5.0 })
    );
    assert_eq!(
        operation_with(exposure, 0, 5.0),
        Some(PictureOperation::Exposure { stops: 5.0 })
    );

    let opacity = EffectKind::OPACITY;
    assert_eq!(
        operation_with(opacity, 0, 0.0),
        Some(PictureOperation::Opacity { opacity: 0.0 }),
        "none of the picture is left"
    );
    assert_eq!(operation_with(opacity, 0, 1.0), None, "all of it is nothing done");

    let blur = EffectKind::BLUR;
    assert_eq!(operation_with(blur, 0, 0.0), None, "a blur of nothing is none");
    assert_eq!(
        operation_with(blur, 0, 100.0),
        Some(PictureOperation::Blur { radius: 100.0 })
    );

    let crop = EffectKind::CROP;
    let edges = |left: f32, top: f32, right: f32, bottom: f32, feather: f32| {
        Some(PictureOperation::Crop {
            left,
            top,
            right,
            bottom,
            feather,
        })
    };
    for slot in 0..5 {
        assert_eq!(operation_with(crop, slot, 0.0), None, "cut nothing off at slot {slot}");
    }
    assert_eq!(operation_with(crop, 0, 1.0), edges(1.0, 0.0, 0.0, 0.0, 0.0), "the left");
    assert_eq!(operation_with(crop, 1, 1.0), edges(0.0, 1.0, 0.0, 0.0, 0.0), "the top");
    assert_eq!(
        operation_with(crop, 2, 1.0),
        edges(0.0, 0.0, 1.0, 0.0, 0.0),
        "the right"
    );
    assert_eq!(
        operation_with(crop, 3, 1.0),
        edges(0.0, 0.0, 0.0, 1.0, 0.0),
        "the bottom"
    );
    assert_eq!(
        operation_with(crop, 4, 0.5),
        edges(0.0, 0.0, 0.0, 0.0, 0.5),
        "the feather"
    );

    // (A look-up table with no table to look in asks for nothing, whatever its intensity is.)
    for intensity in [0.0, 0.5, 1.0] {
        assert_eq!(operation_with(EffectKind::LUT, 1, intensity), None);
    }
}

#[test]
fn the_range_of_every_number_of_every_effect_begins_at_the_default_or_has_it_inside_and_every_end_gives_a_finite_operation()
 {
    for kind in EffectKind::BUILT_IN {
        for (slot, parameter) in kind.parameters().iter().enumerate() {
            let Some((least, most)) = parameter.range else {
                continue;
            };
            let PropertyValue::Number(default) = parameter.default else {
                panic!("{kind} {slot}: a value with a range that is not a number");
            };
            assert!(least < most, "{kind} {slot}: {least} to {most}");
            assert!(
                (least..=most).contains(&default),
                "{kind} {slot}: the default {default} is not in {least} to {most}"
            );
            for end in [least, most] {
                let operation = kind.picture_operation(&values_but_one(kind, slot, end));
                // (The numbers of an operation are finite; a Debug text with `NaN` or `inf` in it is where one is not.)
                let said = format!("{operation:?}");
                assert!(
                    !said.contains("NaN") && !said.contains("inf"),
                    "{kind} {slot} at {end}: {said}"
                );
            }
        }
    }
}

#[test]
fn every_number_of_every_effect_follows_its_keyframes_at_every_frame() {
    for kind in EffectKind::BUILT_IN {
        for (slot, parameter) in kind.parameters().iter().enumerate() {
            let (PropertyValue::Number(_), Some((least, most))) = (&parameter.default, parameter.range) else {
                continue;
            };
            let (mut project, clip) = a_video();
            let target = AnimationTarget::Clip(clip);
            let instance = project.add_effect(target, kind).unwrap();
            let property = PropertyId::effect_value(kind, instance, u8::try_from(slot).unwrap());
            make_a_keyframe(&mut project, target, property, 10, number(least));
            make_a_keyframe(&mut project, target, property, 40, number(most));
            for timeline_frame in 0..60_u64 {
                // (Before the first keyframe the value is the first, and after the last it is the last; between them it goes in a straight line.)
                let along = (timeline_frame.clamp(10, 40) - 10) as f32 / 30.0;
                let value = least + (most - least) * along;
                let expected: Vec<PictureOperation> = operation_with(kind, slot, value).into_iter().collect();
                let asked = operations_at(&project, timeline_frame);
                assert_eq!(
                    format!("{asked:?}"),
                    format!("{expected:?}"),
                    "{kind}, value {slot}, frame {timeline_frame}"
                );
            }
        }
    }
}

#[test]
fn values_past_the_ends_of_what_makes_sense_are_cut_off_at_the_ends_the_effect_has() {
    let everything = |value: f32| vec![number(value); 5];
    assert_eq!(
        EffectKind::CROP.picture_operation(&everything(7.0)),
        Some(PictureOperation::Crop {
            left: 1.0,
            top: 1.0,
            right: 1.0,
            bottom: 1.0,
            feather: 1.0
        })
    );
    assert_eq!(
        EffectKind::CROP.picture_operation(&everything(-7.0)),
        Some(PictureOperation::Crop {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            feather: 0.0
        })
    );
    assert_eq!(
        EffectKind::OPACITY.picture_operation(&[number(7.0)]),
        Some(PictureOperation::Opacity { opacity: 1.0 })
    );
    assert_eq!(
        EffectKind::BLUR.picture_operation(&[number(-7.0)]),
        Some(PictureOperation::Blur { radius: 0.0 })
    );
}

#[test]
fn values_that_are_not_the_numbers_an_effect_has_are_nothing_done_for_every_effect() {
    for kind in EffectKind::BUILT_IN {
        for values in [Vec::new(), vec![PropertyValue::Words("not a number".to_owned())]] {
            if let Some(operation) = kind.picture_operation(&values) {
                assert!(operation.changes_nothing(), "{kind} with {values:?}: {operation:?}");
            }
        }
    }
}
