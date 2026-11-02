// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A shader that reads masks (`docs/shader-masks-design.md`), **as the plan is made of it, with no graphics card**: the masks are groups that are put in their places before the effects
//! of the layer that reads them, each is made into the codes of a screen by a step of its own right before the call, and a call that does not have the masks its shader reads is not run.

use super::effect_tests::{matrix_effect, whole_frame};
use super::foreign_effect_tests::call;
use super::plan::{
    CallDomain, EffectStep, EffectToRun, EffectsThatRun, KindOfEffect, PassPlan, plan_frame, steps_of_a_run_of_effects,
};
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, GroupDescription, LayerDescription, LayerSource,
    PictureId, WorkingSpace,
};
use crate::colour_effects::ColourMatrix;

/// A shader that reads two masks.
const READING_TWO_MASKS: &str = "test:two_masks";
/// A shader that reads one mask and takes two passes.
const READING_ONE_MASK_IN_TWO_PASSES: &str = "test:one_mask";
/// A shader that reads no mask.
const READING_NO_MASK: &str = "test:no_mask";

fn known() -> EffectsThatRun {
    let mut known = EffectsThatRun::built_in_only();
    known.add_shader_with_masks(READING_TWO_MASKS, 1, 0, 2);
    known.add_shader_with_masks(READING_ONE_MASK_IN_TWO_PASSES, 2, 0, 1);
    known.add_shader(READING_NO_MASK);
    known
}

/// A group of the layers `layers`, as a mask is.
fn mask_of(layers: Vec<LayerDescription>) -> GroupDescription {
    GroupDescription { layers }
}

/// A call of `key` with these masks.
fn reading(key: &str, masks: Vec<GroupDescription>) -> EffectCall {
    EffectCall {
        masks,
        ..call(key, &[0.5])
    }
}

fn frame_of_layers(layers: Vec<LayerDescription>) -> FrameDescription {
    let mut description = FrameDescription::black((64, 36));
    description.layers = layers;
    description
}

fn frame_of_layers_in_light(layers: Vec<LayerDescription>) -> FrameDescription {
    let mut description = frame_of_layers(layers);
    description.colour.working = WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709);
    description
}

fn a_picture(picture: u64) -> LayerDescription {
    whole_frame(picture, Vec::new())
}

fn a_group(layers: Vec<LayerDescription>) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Group(GroupDescription { layers }),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: Vec::new(),
    }
}

fn the_call(call: usize) -> EffectStep {
    EffectStep::Call {
        call,
        converts_into_display: false,
        converts_out_of_display: false,
    }
}

fn the_mask(call: usize, mask: usize) -> EffectStep {
    EffectStep::Mask { call, mask }
}

const INTO_THE_CODES: EffectStep = EffectStep::Conversion { into_display: true };
const OUT_OF_THE_CODES: EffectStep = EffectStep::Conversion { into_display: false };

fn a_shader_reading(call: usize, masks: usize) -> EffectToRun {
    EffectToRun {
        call,
        domain: CallDomain::DisplayReferred,
        passes: 1,
        converts_itself: false,
        reads_the_backdrop: false,
        masks,
    }
}

// --- what the engine knows of a shader ---

#[test]
fn how_many_masks_a_shader_reads_is_what_it_was_given_with_and_none_for_anything_else() {
    let mut known = known();

    assert_eq!(known.masks_of(READING_TWO_MASKS), 2);
    assert_eq!(known.masks_of(READING_ONE_MASK_IN_TWO_PASSES), 1);
    assert_eq!(known.masks_of(READING_NO_MASK), 0);
    assert_eq!(known.masks_of("test:never_given"), 0);
    assert_eq!(known.masks_of(crate::engine::COLOUR_MATRIX_KEY), 0);
    assert_eq!(known.kind_of(READING_TWO_MASKS), Some(KindOfEffect::Shader));
    assert_eq!(known.passes_of(READING_ONE_MASK_IN_TWO_PASSES, KindOfEffect::Shader), 2);
    // The same key given again is the shader it is given as then.
    known.add_shader_with_images(READING_TWO_MASKS, 1, 0);
    assert_eq!(known.masks_of(READING_TWO_MASKS), 0);
    known.add_shader_with_masks(READING_TWO_MASKS, 3, 1, 4);
    assert_eq!(known.masks_of(READING_TWO_MASKS), 4);
    assert_eq!(known.imported_images_of(READING_TWO_MASKS), 1);
    assert_eq!(known.passes_of(READING_TWO_MASKS, KindOfEffect::Shader), 3);
    known.remove_shader(READING_TWO_MASKS);
    assert_eq!(known.masks_of(READING_TWO_MASKS), 0);
}

// --- the steps of one layer's effects ---

#[test]
fn each_mask_is_made_into_the_codes_of_a_screen_by_a_step_of_its_own_right_before_the_call_that_reads_it() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader_reading(0, 2)], false),
        vec![the_mask(0, 0), the_mask(0, 1), the_call(0)],
        "in the compatibility mode the numbers already are codes: the steps copy"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader_reading(0, 2)], true),
        vec![
            INTO_THE_CODES,
            the_mask(0, 0),
            the_mask(0, 1),
            the_call(0),
            OUT_OF_THE_CODES
        ],
        "in light the picture of the layer is made into codes and so are the masks, and the effect gives light back"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader_reading(0, 0)], true),
        vec![INTO_THE_CODES, the_call(0), OUT_OF_THE_CODES],
        "a shader that reads no mask has no such step"
    );
}

#[test]
fn the_steps_of_the_masks_come_after_the_step_of_the_backdrop_and_are_not_repeated_for_the_later_passes() {
    let two_passes_and_a_mask_and_the_backdrop = EffectToRun {
        passes: 2,
        reads_the_backdrop: true,
        ..a_shader_reading(0, 1)
    };

    assert_eq!(
        steps_of_a_run_of_effects(&[two_passes_and_a_mask_and_the_backdrop], false),
        vec![
            EffectStep::Backdrop { call: 0 },
            the_mask(0, 0),
            the_call(0),
            EffectStep::LaterPass { call: 0, pass: 1 },
        ],
        "the masks are made once, for the first pass, and the later pass reads the same ones"
    );
}

#[test]
fn the_effects_round_a_call_that_reads_masks_are_converted_once_as_they_are_for_any_neighbours_and_each_call_has_its_own_masks()
 {
    let a_matrix = |call| EffectToRun {
        call,
        domain: CallDomain::DisplayReferred,
        passes: 1,
        converts_itself: true,
        reads_the_backdrop: false,
        masks: 0,
    };

    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_shader_reading(1, 1), a_shader_reading(2, 2)], true),
        vec![
            EffectStep::Call {
                call: 0,
                converts_into_display: true,
                converts_out_of_display: false
            },
            the_mask(1, 0),
            the_call(1),
            the_mask(2, 0),
            the_mask(2, 1),
            the_call(2),
            OUT_OF_THE_CODES,
        ]
    );
}

// --- which calls run ---

#[test]
fn a_call_that_has_the_masks_its_shader_reads_is_run_and_one_that_has_another_number_of_them_is_left_out_and_counted() {
    let mask = || mask_of(vec![a_picture(7)]);
    let plan_of = |effect: EffectCall| plan_frame(&frame_of_layers(vec![whole_frame(1, vec![effect])]), &known());

    for (name, effect, runs) in [
        ("both masks", reading(READING_TWO_MASKS, vec![mask(), mask()]), true),
        ("one of two", reading(READING_TWO_MASKS, vec![mask()]), false),
        ("none of two", reading(READING_TWO_MASKS, vec![]), false),
        (
            "three of two",
            reading(READING_TWO_MASKS, vec![mask(), mask(), mask()]),
            false,
        ),
        (
            "the one it reads",
            reading(READING_ONE_MASK_IN_TWO_PASSES, vec![mask()]),
            true,
        ),
        (
            "one for a shader that reads none",
            reading(READING_NO_MASK, vec![mask()]),
            false,
        ),
        (
            "none for a shader that reads none",
            reading(READING_NO_MASK, vec![]),
            true,
        ),
    ] {
        let plan = plan_of(effect);
        assert_eq!(plan.effects_not_run, usize::from(!runs), "{name}");
        assert_eq!(
            plan.passes.iter().any(|pass| matches!(pass, PassPlan::Effects { .. })),
            runs,
            "{name}"
        );
    }
}

// --- where the masks are drawn ---

#[test]
fn a_mask_is_put_together_in_its_place_before_the_effects_of_the_layer_that_reads_it() {
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(
                READING_TWO_MASKS,
                vec![mask_of(vec![a_picture(7)]), mask_of(vec![a_picture(8), a_picture(9)])],
            )],
        )]),
        &known(),
    );

    // The layers are numbered container by container: the frame's layer is 0, the first mask's is 1, the second's are 2 and 3. The containers are drawn from the last to the first.
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::PlaceGroup {
                container: 2,
                layers: vec![2, 3]
            },
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![1]
            },
            PassPlan::Effects {
                layer: 0,
                calls: vec![0]
            },
            PassPlan::PlaceLayers { layers: vec![0] },
            PassPlan::Output,
        ]
    );
    assert_eq!(
        plan.steps_of_effects[2],
        vec![the_mask(0, 0), the_mask(0, 1), the_call(0)]
    );
    assert_eq!(plan.effects_not_run, 0);
    // One pass for each group, three for the steps, one that puts the layer in its place, one for the output.
    assert_eq!(plan.passes_recorded(), 2 + 3 + 1 + 1);
}

#[test]
fn the_effects_of_the_layers_of_a_mask_run_before_the_mask_is_put_together() {
    let hue = matrix_effect(ColourMatrix::hue_rotation(90.0));
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(
                READING_ONE_MASK_IN_TWO_PASSES,
                vec![mask_of(vec![whole_frame(7, vec![hue])])],
            )],
        )]),
        &known(),
    );

    assert_eq!(
        plan.passes,
        vec![
            PassPlan::Effects {
                layer: 1,
                calls: vec![0]
            },
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![1]
            },
            PassPlan::Effects {
                layer: 0,
                calls: vec![0]
            },
            PassPlan::PlaceLayers { layers: vec![0] },
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_mask_of_a_layer_in_a_group_is_put_together_before_the_group_is() {
    let plan = plan_frame(
        &frame_of_layers(vec![a_group(vec![whole_frame(
            1,
            vec![reading(
                READING_ONE_MASK_IN_TWO_PASSES,
                vec![mask_of(vec![a_picture(7)])],
            )],
        )])]),
        &known(),
    );

    // Containers: 0 is the frame (the group is layer 0), 1 is the group (its layer is 1), 2 is the mask (its layer is 2).
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::PlaceGroup {
                container: 2,
                layers: vec![2]
            },
            PassPlan::Effects {
                layer: 1,
                calls: vec![0]
            },
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![1]
            },
            PassPlan::PlaceLayers { layers: vec![0] },
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_mask_that_has_a_group_in_it_has_the_group_put_together_first() {
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(
                READING_ONE_MASK_IN_TWO_PASSES,
                vec![mask_of(vec![a_group(vec![a_picture(7)])])],
            )],
        )]),
        &known(),
    );

    // Containers: 0 the frame, 1 the mask (its layer, the group, is 1), 2 the group in the mask (its layer is 2).
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::PlaceGroup {
                container: 2,
                layers: vec![2]
            },
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![1]
            },
            PassPlan::Effects {
                layer: 0,
                calls: vec![0]
            },
            PassPlan::PlaceLayers { layers: vec![0] },
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_mask_with_no_layer_is_a_group_that_is_cleared_and_nothing_more() {
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(READING_ONE_MASK_IN_TWO_PASSES, vec![mask_of(vec![])])],
        )]),
        &known(),
    );

    assert_eq!(
        plan.passes[0],
        PassPlan::PlaceGroup {
            container: 1,
            layers: vec![]
        }
    );
    assert_eq!(plan.effects_not_run, 0);
}

#[test]
fn in_light_a_picture_of_codes_is_converted_into_and_out_of_round_the_call_that_reads_masks() {
    let plan = plan_frame(
        &frame_of_layers_in_light(vec![whole_frame(
            1,
            vec![reading(
                READING_TWO_MASKS,
                vec![mask_of(vec![a_picture(7)]), mask_of(vec![a_picture(8)])],
            )],
        )]),
        &known(),
    );
    let effects = plan
        .passes
        .iter()
        .position(|pass| matches!(pass, PassPlan::Effects { .. }))
        .expect("the call runs");

    assert_eq!(
        plan.steps_of_effects[effects],
        vec![
            INTO_THE_CODES,
            the_mask(0, 0),
            the_mask(0, 1),
            the_call(0),
            OUT_OF_THE_CODES
        ]
    );
}

#[test]
fn a_mask_is_not_a_picture_of_a_frame_the_picture_it_is_made_of_is() {
    let description = frame_of_layers(vec![whole_frame(
        1,
        vec![reading(
            READING_TWO_MASKS,
            vec![mask_of(vec![a_picture(7)]), mask_of(vec![a_picture(8)])],
        )],
    )]);

    assert_eq!(description.pictures_used(), [PictureId(1), PictureId(7), PictureId(8)]);
}

// --- a shader that reads what is behind its layer, in a mask (found by the maintainer's testing, 2026-11-02) ---

/// A shader that reads the backdrop, as a transition does.
const THE_WIPE: &str = "test:wipe";

fn known_with_a_wipe() -> EffectsThatRun {
    let mut known = known();
    known.add_shader_that_reads_the_backdrop(THE_WIPE, 1, 0);
    known
}

fn a_wipe() -> EffectCall {
    call(THE_WIPE, &[0.5])
}

#[test]
fn a_shader_that_reads_the_backdrop_runs_in_a_mask_and_reads_the_layers_behind_it_in_the_mask_which_begins_clear() {
    // The frame's layer is 0; the mask's are 1 (a picture) and 2 (a picture with the wipe on it).
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(
                READING_ONE_MASK_IN_TWO_PASSES,
                vec![mask_of(vec![a_picture(7), whole_frame(8, vec![a_wipe()])])],
            )],
        )]),
        &known_with_a_wipe(),
    );

    assert_eq!(plan.effects_not_run, 0);
    assert_eq!(
        plan.passes,
        vec![
            // (The layers behind the wipe, in the mask, clearing it first; then the wipe, which reads what is there; then the layers from it up, over what is there.)
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![1]
            },
            PassPlan::Effects {
                layer: 2,
                calls: vec![0]
            },
            PassPlan::PlaceGroupOver {
                container: 1,
                layers: vec![2]
            },
            PassPlan::Effects {
                layer: 0,
                calls: vec![0]
            },
            PassPlan::PlaceLayers { layers: vec![0] },
            PassPlan::Output,
        ]
    );
    assert_eq!(
        plan.steps_of_effects[1],
        vec![EffectStep::Backdrop { call: 0 }, the_call(0)],
        "the wipe reads the mask's picture so far"
    );
    // The two groups' passes, the wipe's two steps, the host's mask step and call (its second pass is a pass of the same call), the layer's place and the output.
    assert_eq!(plan.passes_recorded(), 1 + 2 + 1 + 3 + 1 + 1);
}

#[test]
fn a_shader_that_reads_the_backdrop_on_the_first_layer_of_a_mask_reads_a_picture_with_nothing_in_it() {
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(
                READING_ONE_MASK_IN_TWO_PASSES,
                vec![mask_of(vec![whole_frame(8, vec![a_wipe()])])],
            )],
        )]),
        &known_with_a_wipe(),
    );

    assert_eq!(plan.effects_not_run, 0);
    assert_eq!(
        &plan.passes[..3],
        [
            // (Nothing is behind it: the mask is cleared, and not put together of nothing.)
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![]
            },
            PassPlan::Effects {
                layer: 1,
                calls: vec![0]
            },
            PassPlan::PlaceGroupOver {
                container: 1,
                layers: vec![1]
            },
        ]
    );
}

#[test]
fn a_group_in_a_mask_is_still_a_group_where_a_shader_that_reads_the_backdrop_is_not_run_and_is_counted() {
    // (The wipe is in a group of the mask, whose picture begins clear as a group's does: the mask's own layers are the ones that can read their backdrop.)
    let plan = plan_frame(
        &frame_of_layers(vec![whole_frame(
            1,
            vec![reading(
                READING_ONE_MASK_IN_TWO_PASSES,
                vec![mask_of(vec![a_group(vec![
                    a_picture(7),
                    whole_frame(8, vec![a_wipe()]),
                ])])],
            )],
        )]),
        &known_with_a_wipe(),
    );

    assert_eq!(plan.effects_not_run, 1, "the wipe");
    assert!(
        !plan
            .passes
            .iter()
            .any(|pass| matches!(pass, PassPlan::PlaceGroupOver { .. })),
        "{:?}",
        plan.passes
    );
}
