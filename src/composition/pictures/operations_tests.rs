// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a layer asks of its picture (phase 4, piece 4.3): the operations of its effects at a frame, in the order they are applied in, with the colour mixes that are
//! neighbours made one and the effects that do nothing left out.

use super::tests::{frame, long_video};
use super::*;
use crate::colour_effects::ColourMatrix;
use crate::composition::test_projects::project_of;
use crate::project::{AnimationTarget, EffectKind, Member, PictureOperation, PropertyId, PropertyValue};
use crate::test_support::{make_a_keyframe, put_an_effect_on};

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

fn put_on(
    project: &mut Project,
    clip: ClipId,
    kind: EffectKind,
    values: &[PropertyValue],
) -> crate::project::EffectInstanceId {
    put_an_effect_on(project, AnimationTarget::Clip(clip), kind, values)
}

fn the_mix(operation: &PictureOperation) -> ColourMatrix {
    match operation {
        PictureOperation::Mix(matrix) => *matrix,
        other => panic!("{other:?} is not a mix of colours"),
    }
}

#[test]
fn a_layer_with_no_effects_asks_nothing_of_its_picture() {
    let (project, _) = a_video();
    assert_eq!(operations_at(&project, 50), Vec::new());
}

#[test]
fn every_effect_of_the_program_asks_nothing_of_the_picture_when_it_has_the_values_it_begins_with() {
    for kind in EffectKind::BUILT_IN {
        let (mut project, clip) = a_video();
        project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
        assert_eq!(operations_at(&project, 50), Vec::new(), "{kind}");
        // (An effect that needs a table and has none asks nothing at all.)
        let operation = kind.picture_operation(&kind.default_values());
        assert!(
            operation.as_ref().is_none_or(PictureOperation::changes_nothing),
            "{kind}: {operation:?}"
        );
    }
}

#[test]
fn each_effect_of_the_program_asks_the_operation_its_values_say() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Mix(ColourMatrix::saturation(0.5))]
    );

    let (mut project, clip) = a_video();
    put_on(
        &mut project,
        clip,
        EffectKind::BRIGHTNESS_AND_CONTRAST,
        &[number(0.25), number(1.5)],
    );
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Mix(ColourMatrix::brightness_and_contrast(0.25, 1.5))]
    );

    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(-1.5)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Exposure { stops: -1.5 }]
    );

    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(0.4)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Opacity { opacity: 0.4 }]
    );
}

#[test]
fn blur_and_crop_ask_the_operation_their_values_say_cut_off_where_the_effect_ends() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::BLUR, &[number(12.5)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Blur { radius: 12.5 }]
    );

    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::BLUR, &[number(-4.0)]);
    assert_eq!(
        operations_at(&project, 3),
        Vec::new(),
        "a blur of less than nothing is none"
    );

    let (mut project, clip) = a_video();
    put_on(
        &mut project,
        clip,
        EffectKind::CROP,
        &[number(0.1), number(0.2), number(0.3), number(0.4), number(0.05)],
    );
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Crop {
            left: 0.1,
            top: 0.2,
            right: 0.3,
            bottom: 0.4,
            feather: 0.05
        }]
    );

    let (mut project, clip) = a_video();
    put_on(
        &mut project,
        clip,
        EffectKind::CROP,
        &[number(-1.0), number(7.0), number(0.0), number(0.0), number(0.0)],
    );
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Crop {
            left: 0.0,
            top: 1.0,
            right: 0.0,
            bottom: 0.0,
            feather: 0.0
        }],
        "the fractions are cut off to nought to one"
    );
}

#[test]
fn a_blur_and_a_crop_end_a_run_of_colour_mixes_like_any_effect_that_is_not_a_mix() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    put_on(&mut project, clip, EffectKind::BLUR, &[number(3.0)]);
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    put_on(
        &mut project,
        clip,
        EffectKind::CROP,
        &[number(0.0), number(0.0), number(0.25), number(0.0), number(0.0)],
    );
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);

    let operations = operations_at(&project, 3);

    assert_eq!(operations.len(), 5, "{operations:?}");
    assert!(matches!(operations[1], PictureOperation::Blur { .. }));
    assert!(matches!(operations[3], PictureOperation::Crop { .. }));
}

#[test]
fn colour_mixes_that_are_neighbours_are_one_operation_in_the_order_they_are_applied_in() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::HUE_SHIFT, &[number(70.0)]);
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(1.5)]);
    put_on(
        &mut project,
        clip,
        EffectKind::BRIGHTNESS_AND_CONTRAST,
        &[number(0.1), number(1.2)],
    );

    let operations = operations_at(&project, 3);

    assert_eq!(operations.len(), 1, "three mixes make one pass: {operations:?}");
    assert_eq!(
        the_mix(&operations[0]),
        ColourMatrix::hue_rotation(70.0)
            .then(ColourMatrix::saturation(1.5))
            .then(ColourMatrix::brightness_and_contrast(0.1, 1.2))
    );
    // And the other way round is another matrix: the order is the order of the chain.
    let (mut other_way, clip) = a_video();
    put_on(
        &mut other_way,
        clip,
        EffectKind::BRIGHTNESS_AND_CONTRAST,
        &[number(0.1), number(1.2)],
    );
    put_on(&mut other_way, clip, EffectKind::SATURATION, &[number(1.5)]);
    put_on(&mut other_way, clip, EffectKind::HUE_SHIFT, &[number(70.0)]);
    assert_ne!(the_mix(&operations_at(&other_way, 3)[0]), the_mix(&operations[0]));
}

#[test]
fn an_effect_that_is_not_a_mix_ends_a_run_of_mixes_and_the_operations_keep_the_order_of_the_chain() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::HUE_SHIFT, &[number(30.0)]);
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(1.0)]);
    put_on(
        &mut project,
        clip,
        EffectKind::BRIGHTNESS_AND_CONTRAST,
        &[number(0.1), number(1.0)],
    );
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(0.5)]);
    put_on(&mut project, clip, EffectKind::HUE_SHIFT, &[number(20.0)]);

    let operations = operations_at(&project, 3);

    assert_eq!(
        operations,
        vec![
            PictureOperation::Mix(ColourMatrix::hue_rotation(30.0).then(ColourMatrix::saturation(0.5))),
            PictureOperation::Exposure { stops: 1.0 },
            PictureOperation::Mix(ColourMatrix::brightness_and_contrast(0.1, 1.0)),
            PictureOperation::Opacity { opacity: 0.5 },
            PictureOperation::Mix(ColourMatrix::hue_rotation(20.0)),
        ]
    );
}

#[test]
fn an_effect_that_does_nothing_is_left_out_and_does_not_keep_the_mixes_round_it_apart() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::HUE_SHIFT, &[number(30.0)]);
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(0.0)]);
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(1.0)]);
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(1.0)]);
    put_on(&mut project, clip, EffectKind::HUE_SHIFT, &[number(30.0)]);

    let operations = operations_at(&project, 3);

    assert_eq!(operations.len(), 1, "{operations:?}");
    assert_eq!(
        the_mix(&operations[0]),
        ColourMatrix::hue_rotation(30.0).then(ColourMatrix::hue_rotation(30.0))
    );
}

#[test]
fn two_mixes_that_undo_each_other_are_no_operation_at_all() {
    let (mut project, clip) = a_video();
    // (Brightness is the pair that undoes itself exactly: the matrices of two turns of the hue that are opposite are not quite each other's inverse, off by some
    // four ten-thousandths, which is as far as the matrix that CSS and SVG use goes.)
    put_on(
        &mut project,
        clip,
        EffectKind::BRIGHTNESS_AND_CONTRAST,
        &[number(0.3), number(1.0)],
    );
    put_on(
        &mut project,
        clip,
        EffectKind::BRIGHTNESS_AND_CONTRAST,
        &[number(-0.3), number(1.0)],
    );
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(1.0)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Exposure { stops: 1.0 }]
    );
}

#[test]
fn an_effect_that_is_switched_off_asks_nothing_and_is_asked_again_when_it_is_switched_on() {
    let (mut project, clip) = a_video();
    let exposure = put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(2.0)]);
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(0.5)]);

    project.set_effect_enabled(Member::Clip(clip), exposure, false).unwrap();
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Opacity { opacity: 0.5 }]
    );

    project.set_effect_enabled(Member::Clip(clip), exposure, true).unwrap();
    assert_eq!(operations_at(&project, 3).len(), 2);
}

#[test]
fn the_values_that_are_animated_are_what_the_keyframes_say_at_each_frame() {
    let (mut project, clip) = a_video();
    let exposure = put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(0.0)]);
    let stops = PropertyId::effect_value(EffectKind::EXPOSURE, exposure, 0);
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), stops, 10, number(0.0));
    make_a_keyframe(&mut project, AnimationTarget::Clip(clip), stops, 50, number(2.0));

    assert_eq!(
        operations_at(&project, 5),
        Vec::new(),
        "no exposure before the first keyframe"
    );
    assert_eq!(operations_at(&project, 10), Vec::new(), "and none at it");
    assert_eq!(
        operations_at(&project, 50),
        vec![PictureOperation::Exposure { stops: 2.0 }]
    );
    let [PictureOperation::Exposure { stops: between }] = operations_at(&project, 30)[..] else {
        panic!("{:?}", operations_at(&project, 30))
    };
    assert!((between - 1.0).abs() < 1e-4, "{between}");
}

#[test]
fn values_typed_past_what_makes_sense_are_cut_off_where_the_effect_ends() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(1000.0)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Exposure { stops: 12.0 }],
        "twelve stops is the most"
    );
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(-1000.0)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Exposure { stops: -12.0 }]
    );
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(7.0)]);
    assert_eq!(
        operations_at(&project, 3),
        Vec::new(),
        "more than all the opacity is all of it"
    );
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(-2.0)]);
    assert_eq!(
        operations_at(&project, 3),
        vec![PictureOperation::Opacity { opacity: 0.0 }]
    );
}

#[test]
fn the_colour_matrix_of_the_old_drawing_is_the_mixes_among_the_operations_and_only_them() {
    let (mut project, clip) = a_video();
    put_on(&mut project, clip, EffectKind::HUE_SHIFT, &[number(30.0)]);
    put_on(&mut project, clip, EffectKind::EXPOSURE, &[number(2.0)]);
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    put_on(&mut project, clip, EffectKind::OPACITY, &[number(0.5)]);
    let plan = super::tests::plan(&project);
    let layer = &plan.sections[0].layers[0];

    assert_eq!(
        layer.colour_matrix_at(frame(3)),
        ColourMatrix::hue_rotation(30.0).then(ColourMatrix::saturation(0.5)),
        "(the exposure and the opacity are left out of it)"
    );
    assert_eq!(layer.operations_at(frame(3)).len(), 4);
}

#[test]
fn the_operation_that_changes_nothing_is_the_engines_own_idea_of_it() {
    // The engine leaves a call out by the same limits (it knows nothing of these types); this is what keeps the two from drifting apart.
    use crate::engine::{exposure_changes_nothing, opacity_changes_nothing};
    for stops in [
        0.0, 1.0e-6, 5.0e-5, 5.5e-5, 5.6e-5, 5.7e-5, 1.0e-4, 1.0e-3, 0.1, -1.0e-6, -5.0e-5, -5.6e-5, -5.7e-5, -1.0e-4,
        -0.1, 1.0, -1.0,
    ] {
        assert_eq!(
            PictureOperation::Exposure { stops }.changes_nothing(),
            exposure_changes_nothing(stops),
            "{stops} stops"
        );
    }
    use crate::engine::{blur_changes_nothing, crop_changes_nothing};
    for radius in [-1.0, 0.0, 0.005, 0.0099, 0.01, 0.0101, 0.02, 1.0, 100.0] {
        assert_eq!(
            PictureOperation::Blur { radius }.changes_nothing(),
            blur_changes_nothing(radius),
            "{radius} points"
        );
    }
    for fraction in [0.0, 1.0e-6, 9.9e-5, 1.0e-4, 1.01e-4, 1.0e-3, 0.5, 1.0] {
        for place in 0..5 {
            let mut fractions = [0.0_f32; 5];
            fractions[place] = fraction;
            let [left, top, right, bottom, feather] = fractions;
            assert_eq!(
                PictureOperation::Crop {
                    left,
                    top,
                    right,
                    bottom,
                    feather
                }
                .changes_nothing(),
                crop_changes_nothing(&fractions),
                "{fractions:?}"
            );
        }
    }
    for opacity in [
        0.0, 0.5, 0.9, 0.999, 0.99995, 0.99996, 0.99997, 0.99999, 1.0, 1.0001, 7.0, -1.0,
    ] {
        assert_eq!(
            PictureOperation::Opacity { opacity }.changes_nothing(),
            opacity_changes_nothing(opacity),
            "{opacity} of the opacity"
        );
    }
}

// --- the look-up table ---

use crate::project::{LutSpace, SourceMediaLink};

const A_TABLE: &str = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

fn words(text: &str) -> PropertyValue {
    PropertyValue::Words(text.to_owned())
}

fn the_values_of_a_lut(table: &str, intensity: f32, input: &str, output: &str, way: &str) -> Vec<PropertyValue> {
    vec![words(table), number(intensity), words(input), words(output), words(way)]
}

fn a_project_with_a_table(contents: &str) -> (Project, ClipId) {
    let (mut project, clip) = a_video();
    project
        .add_resource(
            "grade.cube",
            crate::project::ResourceKind::Cube,
            std::sync::Arc::from(contents.as_bytes()),
            SourceMediaLink::from_absolute_path(std::path::PathBuf::from("/x/grade.cube")),
        )
        .unwrap();
    (project, clip)
}

#[test]
fn a_look_up_table_effect_whose_table_is_a_resource_that_is_a_table_asks_for_a_look_up_in_it_with_the_values_it_has() {
    let (mut project, clip) = a_project_with_a_table(A_TABLE);
    put_on(
        &mut project,
        clip,
        EffectKind::LUT,
        &the_values_of_a_lut("grade.cube", 0.75, "linear", "video", "trilinear"),
    );

    let operations = operations_at(&project, 3);

    let [PictureOperation::Lut(look)] = &operations[..] else {
        panic!("{operations:?}")
    };
    let resource = project.resources().get("grade.cube").unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&look.table, &resource.cube_table().unwrap()),
        "the table of the resource"
    );
    assert_eq!(
        (look.intensity, look.input, look.output, look.interpolation),
        (
            0.75,
            LutSpace::Linear,
            LutSpace::Video,
            crate::cube_table::Interpolation::Trilinear
        )
    );
}

#[test]
fn a_look_up_table_effect_with_no_table_that_can_be_used_asks_nothing_and_a_value_that_is_no_option_is_the_first_option()
 {
    for (name, contents) in [
        ("", Some(A_TABLE)),
        ("absent.cube", Some(A_TABLE)),
        ("grade.cube", Some("LUT_3D_SIZE two\n")),
        ("grade.cube", None),
    ] {
        let (mut project, clip) = match contents {
            Some(contents) => a_project_with_a_table(contents),
            None => {
                let (mut project, clip) = a_video();
                let mut resources = crate::project::Resources::default();
                resources
                    .add_as_it_was_saved(
                        "grade.cube",
                        crate::project::ResourceKind::Cube,
                        None,
                        SourceMediaLink::from_absolute_path(std::path::PathBuf::from("/gone")),
                    )
                    .unwrap();
                project = project.with_resources(resources);
                (project, clip)
            }
        };
        put_on(
            &mut project,
            clip,
            EffectKind::LUT,
            &the_values_of_a_lut(name, 1.0, "screen", "screen", "tetrahedral"),
        );
        assert_eq!(operations_at(&project, 3), Vec::new(), "{name:?} {contents:?}");
    }
    let (mut project, clip) = a_project_with_a_table(A_TABLE);
    put_on(
        &mut project,
        clip,
        EffectKind::LUT,
        &the_values_of_a_lut("grade.cube", 1.0, "nonsense", "", "somehow"),
    );
    let [PictureOperation::Lut(look)] = &operations_at(&project, 3)[..] else {
        panic!()
    };
    assert_eq!(
        (look.input, look.output, look.interpolation),
        (
            LutSpace::Screen,
            LutSpace::Screen,
            crate::cube_table::Interpolation::Tetrahedral
        )
    );
}

#[test]
fn a_look_up_table_of_none_of_the_table_is_left_out_and_one_ends_a_run_of_mixes_like_any_effect_that_is_not_a_mix() {
    let (mut project, clip) = a_project_with_a_table(A_TABLE);
    put_on(
        &mut project,
        clip,
        EffectKind::LUT,
        &the_values_of_a_lut("grade.cube", 0.0, "screen", "screen", "tetrahedral"),
    );
    assert_eq!(operations_at(&project, 3), Vec::new());

    let (mut project, clip) = a_project_with_a_table(A_TABLE);
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    put_on(
        &mut project,
        clip,
        EffectKind::LUT,
        &the_values_of_a_lut("grade.cube", 1.0, "screen", "screen", "tetrahedral"),
    );
    put_on(&mut project, clip, EffectKind::SATURATION, &[number(0.5)]);
    let operations = operations_at(&project, 3);
    assert_eq!(operations.len(), 3, "{operations:?}");
    assert!(matches!(operations[1], PictureOperation::Lut(_)));
}

#[test]
fn the_look_up_in_a_table_that_is_changed_to_another_is_another_table_and_the_old_one_is_not_asked_for() {
    let (mut project, clip) = a_project_with_a_table(A_TABLE);
    let other = A_TABLE.replace("1 1 1\n", "0.5 0.5 0.5\n");
    project
        .add_resource(
            "other.cube",
            crate::project::ResourceKind::Cube,
            std::sync::Arc::from(other.as_bytes()),
            SourceMediaLink::from_absolute_path(std::path::PathBuf::from("/x/other.cube")),
        )
        .unwrap();
    let instance = put_on(
        &mut project,
        clip,
        EffectKind::LUT,
        &the_values_of_a_lut("grade.cube", 1.0, "screen", "screen", "tetrahedral"),
    );
    let serial_of = |project: &Project| {
        let [PictureOperation::Lut(look)] = &operations_at(project, 3)[..] else {
            panic!()
        };
        look.table.serial_number()
    };
    let first = serial_of(&project);

    project
        .set_property_at(
            AnimationTarget::Clip(clip),
            PropertyId::effect_value(EffectKind::LUT, instance, 0),
            words("other.cube"),
            crate::project::TimelineFrameIndex::new(0),
        )
        .unwrap();

    assert_ne!(serial_of(&project), first);
}
