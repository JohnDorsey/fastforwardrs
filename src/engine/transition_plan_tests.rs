// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A shader that reads what is behind its layer (a transition: `startImage` is the backdrop and `endImage` the layer's own picture; phase 6.6, the design is
//! `docs/engine-phase-6-6-design.md`, T1 to T5), **as the plan is made of it, with no graphics card**: when the layers behind it have to be put in their places, where the picture
//! that it reads is made, what a layer that reads it keeps in its one pass of effects, and where it is not run.

use super::effect_tests::{matrix_effect, whole_frame};
use super::foreign_effect_tests::call;
use super::plan::{
    CallDomain, EffectStep, EffectToRun, EffectsThatRun, KindOfEffect, PassPlan, plan_frame, steps_of_a_run_of_effects,
};
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, GroupDescription, LayerDescription, LayerSource,
    WorkingSpace,
};
use crate::colour_effects::ColourMatrix;

/// A shader that reads the backdrop, as a transition does.
const THE_WIPE: &str = "test:wipe";
/// A shader that reads only its own picture, as a filter does.
const A_PLAIN_SHADER: &str = "test:plain";

fn known() -> EffectsThatRun {
    let mut known = EffectsThatRun::built_in_only();
    known.add_shader_that_reads_the_backdrop(THE_WIPE, 1, 0);
    known.add_shader(A_PLAIN_SHADER);
    known
}

fn wipe() -> EffectCall {
    call(THE_WIPE, &[0.5])
}

fn plain() -> EffectCall {
    call(A_PLAIN_SHADER, &[1.0])
}

fn hue() -> EffectCall {
    matrix_effect(ColourMatrix::hue_rotation(90.0))
}

/// A frame (in the compatibility mode) of these layers, the one at the back first.
fn frame_of_layers(layers: Vec<LayerDescription>) -> FrameDescription {
    let mut description = FrameDescription::black((64, 36));
    description.layers = layers;
    description
}

/// The same in light, where the shaders are converted to and from the codes of a screen.
fn frame_of_layers_in_light(layers: Vec<LayerDescription>) -> FrameDescription {
    let mut description = frame_of_layers(layers);
    description.colour.working = WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709);
    description
}

fn a_group(layers: Vec<LayerDescription>, effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Group(GroupDescription { layers }),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects,
    }
}

fn effects_of(layer: usize, calls: &[usize]) -> PassPlan {
    PassPlan::Effects {
        layer,
        calls: calls.to_vec(),
    }
}

fn place(layers: &[usize]) -> PassPlan {
    PassPlan::PlaceLayers {
        layers: layers.to_vec(),
    }
}

fn place_over(layers: &[usize]) -> PassPlan {
    PassPlan::PlaceLayersOver {
        layers: layers.to_vec(),
    }
}

fn the_call(call: usize) -> EffectStep {
    EffectStep::Call {
        call,
        converts_into_display: false,
        converts_out_of_display: false,
    }
}

const BACKDROP_OF_0: EffectStep = EffectStep::Backdrop { call: 0 };
const INTO_THE_CODES: EffectStep = EffectStep::Conversion { into_display: true };
const OUT_OF_THE_CODES: EffectStep = EffectStep::Conversion { into_display: false };

fn the_steps_that_are_backdrops(steps: &[Vec<EffectStep>]) -> usize {
    steps
        .iter()
        .flatten()
        .filter(|step| matches!(step, EffectStep::Backdrop { .. }))
        .count()
}

// --- what the engine knows of a shader ---

#[test]
fn a_shader_is_said_to_read_the_backdrop_only_if_it_was_given_as_one_that_does() {
    let mut known = known();
    assert!(known.reads_the_backdrop(THE_WIPE));
    assert!(!known.reads_the_backdrop(A_PLAIN_SHADER));
    assert!(!known.reads_the_backdrop("test:never_given"));
    assert!(
        !known.reads_the_backdrop(crate::engine::COLOUR_MATRIX_KEY),
        "the engine's own effects read their own picture"
    );
    assert_eq!(known.kind_of(THE_WIPE), Some(KindOfEffect::Shader));
    // The same key given again as another kind of shader is that one, and one that is taken away is not run at all.
    known.add_shader_with_images(THE_WIPE, 1, 0);
    assert!(!known.reads_the_backdrop(THE_WIPE));
    known.add_shader_that_reads_the_backdrop(THE_WIPE, 2, 1);
    assert!(known.reads_the_backdrop(THE_WIPE));
    assert_eq!(known.passes_of(THE_WIPE, KindOfEffect::Shader), 2);
    assert_eq!(known.imported_images_of(THE_WIPE), 1);
    known.remove_shader(THE_WIPE);
    assert!(!known.reads_the_backdrop(THE_WIPE));
    assert_eq!(known.kind_of(THE_WIPE), None);
}

// --- the steps of one layer's effects ---

fn a_wipe_to_run(call: usize) -> EffectToRun {
    EffectToRun {
        call,
        domain: CallDomain::DisplayReferred,
        passes: 1,
        converts_itself: false,
        reads_the_backdrop: true,
    }
}

fn a_matrix_to_run(call: usize) -> EffectToRun {
    EffectToRun {
        call,
        domain: CallDomain::DisplayReferred,
        passes: 1,
        converts_itself: true,
        reads_the_backdrop: false,
    }
}

#[test]
fn the_backdrop_is_made_into_the_codes_of_a_screen_by_a_step_of_its_own_right_before_the_call_that_reads_it() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_wipe_to_run(0)], true),
        vec![INTO_THE_CODES, BACKDROP_OF_0, the_call(0), OUT_OF_THE_CODES],
        "in light the picture of the layer is made into codes and so is the backdrop, and the effect gives light back"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_wipe_to_run(0)], false),
        vec![BACKDROP_OF_0, the_call(0)],
        "in the compatibility mode the numbers already are codes: the step copies"
    );
}

#[test]
fn the_effects_round_a_call_that_reads_the_backdrop_are_converted_once_as_they_are_for_any_neighbours() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix_to_run(0), a_wipe_to_run(1), a_matrix_to_run(2)], true),
        vec![
            EffectStep::Call {
                call: 0,
                converts_into_display: true,
                converts_out_of_display: false
            },
            EffectStep::Backdrop { call: 1 },
            the_call(1),
            EffectStep::Call {
                call: 2,
                converts_into_display: false,
                converts_out_of_display: true
            },
        ],
        "a matrix converts into the codes and the one after the wipe out of them: the wipe has no conversion of its own, and the backdrop has its step"
    );
}

#[test]
fn a_call_that_reads_the_backdrop_and_takes_several_passes_has_the_backdrop_made_once_before_its_first() {
    let wipe_of_three_passes = EffectToRun {
        passes: 3,
        ..a_wipe_to_run(0)
    };
    assert_eq!(
        steps_of_a_run_of_effects(&[wipe_of_three_passes], false),
        vec![
            BACKDROP_OF_0,
            the_call(0),
            EffectStep::LaterPass { call: 0, pass: 1 },
            EffectStep::LaterPass { call: 0, pass: 2 },
        ]
    );
}

#[test]
fn two_calls_that_read_the_backdrop_each_have_the_step_that_makes_it() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_wipe_to_run(0), a_wipe_to_run(1)], false),
        vec![
            BACKDROP_OF_0,
            the_call(0),
            EffectStep::Backdrop { call: 1 },
            the_call(1)
        ]
    );
}

// --- the plan of a frame ---

#[test]
fn a_frame_in_which_no_layer_reads_the_backdrop_is_planned_as_it_always_was_whatever_shaders_the_engine_has() {
    let description = frame_of_layers(vec![
        whole_frame(1, Vec::new()),
        whole_frame(2, vec![hue(), plain()]),
        whole_frame(3, Vec::new()),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![effects_of(1, &[0, 1]), place(&[0, 1, 2]), PassPlan::Output]
    );
    assert_eq!(the_steps_that_are_backdrops(&plan.steps_of_effects), 0);
    assert_eq!(plan.effects_not_run, 0);
}

#[test]
fn a_layer_that_reads_the_backdrop_has_the_layers_behind_it_put_in_their_places_first_and_the_rest_over_them() {
    let description = frame_of_layers(vec![
        whole_frame(1, Vec::new()),
        whole_frame(2, Vec::new()),
        whole_frame(3, vec![wipe()]),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![place(&[0, 1]), effects_of(2, &[0]), place_over(&[2]), PassPlan::Output]
    );
    assert_eq!(
        plan.steps_of_effects,
        vec![vec![], vec![BACKDROP_OF_0, the_call(0)], vec![], vec![]]
    );
    assert_eq!(plan.effects_not_run, 0);
    assert_eq!(
        plan.passes_recorded(),
        1 + 2 + 1 + 1,
        "the layers behind, the backdrop and the call, the layer over them, and the output"
    );
}

#[test]
fn the_effects_of_the_layers_that_do_not_read_the_backdrop_are_run_first_where_they_always_were() {
    let description = frame_of_layers(vec![
        whole_frame(1, vec![hue()]),
        whole_frame(2, vec![wipe()]),
        whole_frame(3, vec![hue()]),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            effects_of(0, &[0]),
            effects_of(2, &[0]),
            place(&[0]),
            effects_of(1, &[0]),
            place_over(&[1, 2]),
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_layer_at_the_very_back_that_reads_the_backdrop_reads_the_opaque_black_that_every_frame_is_put_together_over() {
    let description = frame_of_layers(vec![
        whole_frame(1, vec![wipe()]),
        whole_frame(2, Vec::new()),
        whole_frame(3, Vec::new()),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::ClearAccumulation,
            effects_of(0, &[0]),
            place_over(&[0, 1, 2]),
            PassPlan::Output
        ]
    );
}

#[test]
fn each_layer_that_reads_the_backdrop_reads_what_the_layers_up_to_it_make_and_the_rest_are_put_over_it_in_turn() {
    let description = frame_of_layers(vec![
        whole_frame(1, Vec::new()),
        whole_frame(2, vec![wipe()]),
        whole_frame(3, Vec::new()),
        whole_frame(4, vec![wipe()]),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            place(&[0]),
            effects_of(1, &[0]),
            place_over(&[1, 2]),
            effects_of(3, &[0]),
            place_over(&[3]),
            PassPlan::Output,
        ]
    );
    assert_eq!(the_steps_that_are_backdrops(&plan.steps_of_effects), 2);
}

#[test]
fn two_layers_in_a_row_that_read_the_backdrop_have_nothing_but_the_first_put_between_them() {
    let description = frame_of_layers(vec![
        whole_frame(1, Vec::new()),
        whole_frame(2, vec![wipe()]),
        whole_frame(3, vec![wipe()]),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            place(&[0]),
            effects_of(1, &[0]),
            place_over(&[1]),
            effects_of(2, &[0]),
            place_over(&[2]),
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_layer_that_reads_the_backdrop_keeps_one_pass_of_effects_with_the_effects_before_and_after_the_one_that_reads_it() {
    let description = frame_of_layers_in_light(vec![
        whole_frame(1, Vec::new()),
        whole_frame(2, vec![hue(), wipe(), hue()]),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            place(&[0]),
            effects_of(1, &[0, 1, 2]),
            place_over(&[1]),
            PassPlan::Output
        ]
    );
    assert_eq!(
        plan.steps_of_effects[1],
        vec![
            EffectStep::Call {
                call: 0,
                converts_into_display: true,
                converts_out_of_display: false
            },
            EffectStep::Backdrop { call: 1 },
            the_call(1),
            EffectStep::Call {
                call: 2,
                converts_into_display: false,
                converts_out_of_display: true
            },
        ]
    );
}

#[test]
fn in_light_the_picture_of_the_layer_and_the_backdrop_are_both_made_into_codes_and_the_effect_gives_light_back() {
    let description = frame_of_layers_in_light(vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe()])]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.steps_of_effects[1],
        vec![INTO_THE_CODES, BACKDROP_OF_0, the_call(0), OUT_OF_THE_CODES]
    );
    assert_eq!(plan.passes_recorded(), 1 + 4 + 1 + 1);
}

#[test]
fn a_layer_that_is_a_group_in_the_frame_reads_the_backdrop_with_the_picture_of_the_group_as_its_own() {
    // (Container 1 is the group: the frame's layers are 0 and 1, the group's own layer is 2.)
    let description = frame_of_layers(vec![
        whole_frame(1, Vec::new()),
        a_group(vec![whole_frame(2, Vec::new())], vec![wipe()]),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![2]
            },
            place(&[0]),
            effects_of(1, &[0]),
            place_over(&[1]),
            PassPlan::Output,
        ]
    );
    assert_eq!(plan.effects_not_run, 0);
}

#[test]
fn a_shader_that_reads_the_backdrop_is_not_run_in_a_group_where_the_backdrop_is_not_opaque_and_is_counted() {
    // (The layer that reads the backdrop is in the group, whose picture begins clear.)
    let description = frame_of_layers(vec![
        whole_frame(1, Vec::new()),
        a_group(
            vec![whole_frame(2, Vec::new()), whole_frame(3, vec![wipe(), hue()])],
            Vec::new(),
        ),
    ]);
    let plan = plan_frame(&description, &known());
    assert_eq!(plan.effects_not_run, 1, "the wipe, and not the hue shift after it");
    assert_eq!(
        plan.passes,
        vec![
            effects_of(3, &[1]),
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![2, 3]
            },
            place(&[0, 1]),
            PassPlan::Output,
        ],
        "the rest of the layer's effects run, and the frame is planned as if there were no wipe"
    );
    assert_eq!(the_steps_that_are_backdrops(&plan.steps_of_effects), 0);
}

#[test]
fn a_shader_that_reads_the_backdrop_that_cannot_run_for_another_reason_is_counted_and_does_not_move_the_layers() {
    let not_a_number = EffectCall {
        values: vec![f32::NAN],
        ..wipe()
    };
    let wanting_an_image = EffectCall {
        images: vec![crate::engine::PictureId(9)],
        ..wipe()
    };
    for (wrong, why) in [
        (not_a_number, "a number that is not one"),
        (wanting_an_image, "an image that the shader did not bring"),
    ] {
        let description = frame_of_layers(vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wrong])]);
        let plan = plan_frame(&description, &known());
        assert_eq!(plan.passes, vec![place(&[0, 1]), PassPlan::Output], "{why}");
        assert_eq!(plan.effects_not_run, 1, "{why}");
    }
}

#[test]
fn a_layer_that_is_only_a_colour_has_nothing_for_a_shader_that_reads_the_backdrop_to_work_on() {
    let solid = LayerDescription {
        source: LayerSource::Solid([10, 20, 30, 255]),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: vec![wipe()],
    };
    let description = frame_of_layers(vec![whole_frame(1, Vec::new()), solid]);
    let plan = plan_frame(&description, &known());
    assert_eq!(plan.passes, vec![place(&[0, 1]), PassPlan::Output]);
    assert_eq!(plan.effects_not_run, 1);
}

#[test]
fn a_frame_that_has_no_layer_but_one_that_reads_the_backdrop_is_the_opaque_black_with_the_layer_over_it() {
    let description = frame_of_layers(vec![whole_frame(1, vec![wipe()])]);
    let plan = plan_frame(&description, &known());
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::ClearAccumulation,
            effects_of(0, &[0]),
            place_over(&[0]),
            PassPlan::Output
        ]
    );
}
