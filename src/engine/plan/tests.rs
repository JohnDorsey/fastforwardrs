// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::engine::{BLUR_KEY, CROP_KEY};
use crate::engine::{CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, LayerDescription, LayerSource, PictureId};

/// What was planned before effects could be given as shaders: the effects the engine has of its own, and no others.
fn plan_of_the_built_in_effects(description: &FrameDescription) -> FramePlan {
    plan_frame(description, &EffectsThatRun::built_in_only())
}

fn layer(picture: u64) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: Vec::new(),
    }
}

fn layer_with(picture: u64, effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        effects,
        ..layer(picture)
    }
}

fn description_of(layers: Vec<LayerDescription>) -> FrameDescription {
    let mut description = FrameDescription::black((64, 36));
    description.layers = layers;
    description
}

/// A matrix that makes a colour redder: it changes things.
fn redder() -> EffectCall {
    EffectCall::colour_matrix([[1.0, 0.2, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
}

fn greyer() -> EffectCall {
    EffectCall::colour_matrix([[0.5, 0.25, 0.25], [0.25, 0.5, 0.25], [0.25, 0.25, 0.5]])
}

fn nothing_changes() -> EffectCall {
    EffectCall::colour_matrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
}

#[test]
fn a_frame_with_nothing_in_it_is_cleared_and_then_made_into_the_output() {
    let plan = plan_of_the_built_in_effects(&FrameDescription::black((64, 36)));
    assert_eq!(plan.passes, vec![PassPlan::ClearAccumulation, PassPlan::Output]);
    assert_eq!(plan.effects_not_run, 0);
    assert_eq!(plan.passes_recorded(), 2);
}

#[test]
fn layers_are_put_in_their_places_back_to_front_in_one_pass_that_also_clears_and_then_the_output_is_made() {
    let plan = plan_of_the_built_in_effects(&description_of(vec![layer(1), layer(2), layer(3)]));
    assert_eq!(
        plan.passes,
        vec![PassPlan::PlaceLayers { layers: vec![0, 1, 2] }, PassPlan::Output]
    );
    assert_eq!(plan.passes_recorded(), 2);
}

#[test]
fn the_effects_of_a_layer_run_before_the_layers_are_put_in_their_places_and_in_the_order_they_were_asked_for() {
    let plan = plan_of_the_built_in_effects(&description_of(vec![
        layer(1),
        layer_with(2, vec![redder(), greyer()]),
        layer(3),
        layer_with(4, vec![greyer()]),
    ]));
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::Effects {
                layer: 1,
                calls: vec![0, 1]
            },
            PassPlan::Effects {
                layer: 3,
                calls: vec![0]
            },
            PassPlan::PlaceLayers {
                layers: vec![0, 1, 2, 3]
            },
            PassPlan::Output,
        ]
    );
    assert_eq!(plan.effects_not_run, 0);
    assert_eq!(plan.passes_recorded(), 3 + 1 + 1);
}

#[test]
fn an_effect_that_changes_nothing_is_left_out_and_is_not_counted_as_one_that_could_not_run() {
    let plan = plan_of_the_built_in_effects(&description_of(vec![layer_with(
        1,
        vec![nothing_changes(), redder(), nothing_changes(), greyer()],
    )]));
    assert_eq!(
        plan.passes[0],
        PassPlan::Effects {
            layer: 0,
            calls: vec![1, 3]
        }
    );
    assert_eq!(plan.effects_not_run, 0);

    // A layer whose effects all change nothing is put in its place straight from its picture.
    let plan = plan_of_the_built_in_effects(&description_of(vec![layer_with(1, vec![nothing_changes()])]));
    assert_eq!(
        plan.passes,
        vec![PassPlan::PlaceLayers { layers: vec![0] }, PassPlan::Output]
    );
}

#[test]
fn a_matrix_is_taken_for_the_one_that_changes_nothing_when_no_number_of_it_is_a_hundredth_of_a_step_from_it() {
    let with_a_number_off_by = |difference: f32| {
        let mut values = vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        values[3] += difference;
        values
    };
    let hundredth_of_a_step = 1.0 / 256.0 / 100.0;
    assert!(colour_matrix_changes_nothing(&with_a_number_off_by(0.0)));
    assert!(colour_matrix_changes_nothing(&with_a_number_off_by(
        hundredth_of_a_step * 0.9
    )));
    assert!(colour_matrix_changes_nothing(&with_a_number_off_by(
        -hundredth_of_a_step * 0.9
    )));
    assert!(!colour_matrix_changes_nothing(&with_a_number_off_by(
        hundredth_of_a_step * 1.1
    )));
    assert!(!colour_matrix_changes_nothing(&with_a_number_off_by(
        -hundredth_of_a_step * 1.1
    )));
}

#[test]
fn an_effect_the_engine_does_not_have_or_cannot_use_is_left_out_and_counted() {
    let unknown = EffectCall {
        key: "someone:else".to_owned(),
        values: vec![1.0, 2.0],
        table: None,
        state: None,
        images: Vec::new(),
    };
    let too_few_numbers = EffectCall {
        key: COLOUR_MATRIX_KEY.to_owned(),
        values: vec![1.0; 8],
        table: None,
        state: None,
        images: Vec::new(),
    };
    let too_many_numbers = EffectCall {
        key: COLOUR_MATRIX_KEY.to_owned(),
        values: vec![1.0; 10],
        table: None,
        state: None,
        images: Vec::new(),
    };
    let not_a_number = EffectCall {
        key: COLOUR_MATRIX_KEY.to_owned(),
        values: vec![f32::NAN, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        table: None,
        state: None,
        images: Vec::new(),
    };
    let infinite = EffectCall {
        key: COLOUR_MATRIX_KEY.to_owned(),
        values: vec![f32::INFINITY, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        table: None,
        state: None,
        images: Vec::new(),
    };
    let plan = plan_of_the_built_in_effects(&description_of(vec![
        layer_with(1, vec![unknown, redder(), too_few_numbers]),
        layer_with(2, vec![too_many_numbers, not_a_number, infinite]),
    ]));
    assert_eq!(plan.effects_not_run, 5);
    assert_eq!(
        plan.passes,
        vec![
            PassPlan::Effects {
                layer: 0,
                calls: vec![1]
            },
            PassPlan::PlaceLayers { layers: vec![0, 1] },
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_layer_that_is_only_a_colour_has_no_picture_for_an_effect_to_work_on_so_its_effects_are_counted_as_not_run() {
    let solid = LayerDescription {
        source: LayerSource::Solid([10, 20, 30, 255]),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: vec![redder(), greyer()],
    };
    let plan = plan_of_the_built_in_effects(&description_of(vec![solid]));
    assert_eq!(plan.effects_not_run, 2);
    assert_eq!(
        plan.passes,
        vec![PassPlan::PlaceLayers { layers: vec![0] }, PassPlan::Output]
    );
}

#[test]
fn the_passes_a_frame_takes_are_one_for_each_effect_and_the_two_that_every_frame_has() {
    let plan = plan_of_the_built_in_effects(&description_of(vec![
        layer_with(1, vec![redder(), greyer(), redder()]),
        layer_with(2, vec![greyer()]),
    ]));
    assert_eq!(plan.passes_recorded(), 4 + 2);
    assert_eq!(
        plan_of_the_built_in_effects(&FrameDescription::black((8, 8))).passes_recorded(),
        2
    );
}

/// The numbers that say a matrix changes nothing are the window's own (`ColourMatrix::changes_nothing`): a matrix the window would not apply is one the
/// engine does not run, and the other way round. They are written twice (the engine knows nothing of the window's types), and this is what keeps
/// them from drifting apart.
#[test]
fn the_engine_and_the_window_agree_on_which_matrices_change_nothing() {
    use crate::colour_effects::ColourMatrix;
    let numbers_of = |matrix: ColourMatrix| matrix.bit_pattern().map(f32::from_bits);
    let nothing = numbers_of(ColourMatrix::NOTHING_CHANGES);
    let mut matrices = vec![
        ColourMatrix::NOTHING_CHANGES,
        ColourMatrix::hue_rotation(0.0),
        ColourMatrix::hue_rotation(360.0),
        ColourMatrix::hue_rotation(0.001),
        ColourMatrix::hue_rotation(0.01),
        ColourMatrix::hue_rotation(0.1),
        ColourMatrix::hue_rotation(1.0),
        ColourMatrix::hue_rotation(120.0),
        ColourMatrix::hue_rotation(10.0).then(ColourMatrix::hue_rotation(-10.0)),
    ];
    // And ones that are one number off from the one that changes nothing by amounts on both sides of the limit. (Rewritten on purpose in phase 4: the twelve numbers
    // are the nine of the matrix and its three offsets, which the window's matrix has now.)
    for place in 0..12 {
        for difference in [0.0, 1e-5, 3.0e-5, 3.8e-5, 3.9e-5, 4.0e-5, 4.1e-5, 1e-4, 1e-3, 0.1] {
            for sign in [1.0, -1.0] {
                let mut numbers = nothing;
                numbers[place] += sign * difference;
                matrices.push(matrix_of(numbers));
            }
        }
    }
    for matrix in matrices {
        assert_eq!(
            colour_matrix_changes_nothing(&numbers_of(matrix)),
            matrix.changes_nothing(),
            "{matrix:?}"
        );
    }
}

/// A `ColourMatrix` with these twelve numbers: nine, row by row, and then the three offsets.
fn matrix_of(numbers: [f32; 12]) -> crate::colour_effects::ColourMatrix {
    crate::colour_effects::ColourMatrix::from_rows_and_offsets(
        [
            [numbers[0], numbers[1], numbers[2]],
            [numbers[3], numbers[4], numbers[5]],
            [numbers[6], numbers[7], numbers[8]],
        ],
        [numbers[9], numbers[10], numbers[11]],
    )
}

fn a_matrix(call: usize) -> EffectToRun {
    EffectToRun {
        call,
        domain: CallDomain::DisplayReferred,
        passes: 1,
        converts_itself: true,
        reads_the_backdrop: false,
    }
}

fn a_shader(call: usize) -> EffectToRun {
    EffectToRun {
        call,
        domain: CallDomain::DisplayReferred,
        passes: 1,
        converts_itself: false,
        reads_the_backdrop: false,
    }
}

fn a_pass_of_light(call: usize) -> EffectToRun {
    EffectToRun {
        call,
        domain: CallDomain::SceneLinear,
        passes: 1,
        converts_itself: false,
        reads_the_backdrop: false,
    }
}

/// An effect of light that takes two passes of the card, as the blur does.
fn a_blur(call: usize) -> EffectToRun {
    EffectToRun {
        passes: 2,
        ..a_pass_of_light(call)
    }
}

// (Rewritten on purpose in phase 6.4: the second pass of an effect is `LaterPass` number 1.)
fn second_pass_of(call: usize) -> EffectStep {
    EffectStep::LaterPass { call, pass: 1 }
}

#[test]
fn an_effect_that_takes_two_passes_has_its_second_pass_right_after_its_first_wherever_it_is() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_blur(1), a_matrix(2)], true),
        vec![
            call_of(0, true, true),
            call_of(1, false, false),
            second_pass_of(1),
            call_of(2, true, true)
        ]
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader(0), a_blur(1), a_blur(2)], true),
        vec![
            INTO_THE_CODES,
            call_of(0, false, false),
            OUT_OF_THE_CODES,
            call_of(1, false, false),
            second_pass_of(1),
            call_of(2, false, false),
            second_pass_of(2)
        ],
        "(a shader's conversion out of the codes is a pass of its own, before the blur; two blurs need none between them)"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_blur(0), a_matrix(1)], false),
        vec![call_of(0, false, false), second_pass_of(0), call_of(1, true, true)],
        "in the compatibility mode too"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_blur(0)], true),
        vec![call_of(0, false, false), second_pass_of(0)],
        "a blur of light needs no conversion"
    );
}

#[test]
fn a_blur_is_given_one_number_and_a_crop_five_and_the_passes_of_a_blur_are_two() {
    let mut known = EffectsThatRun::built_in_only();
    known.add_shader("test:shader");
    let blur = |numbers: &[f32]| EffectCall {
        key: BLUR_KEY.to_owned(),
        values: numbers.to_vec(),
        table: None,
        state: None,
        images: Vec::new(),
    };
    let crop = |numbers: &[f32]| EffectCall {
        key: CROP_KEY.to_owned(),
        values: numbers.to_vec(),
        table: None,
        state: None,
        images: Vec::new(),
    };
    let plan_of = |effects: Vec<EffectCall>| plan_frame(&description_of(vec![layer_with(1, effects)]), &known);
    // (A layer, its effects, the place and the output: two passes besides the effects.)
    let runs = |effects: Vec<EffectCall>| plan_of(effects).passes.len() == 3;

    assert!(runs(vec![blur(&[4.0])]));
    assert_eq!(
        plan_of(vec![blur(&[4.0])]).passes_recorded(),
        2 + 2,
        "a blur takes two passes"
    );
    assert_eq!(plan_of(vec![blur(&[4.0]), blur(&[1.0])]).passes_recorded(), 2 + 4);
    assert!(!runs(vec![blur(&[0.0])]), "a blur of nothing is not run");
    assert!(!runs(vec![blur(&[0.005])]));
    assert!(runs(vec![blur(&[0.02])]));
    assert!(!runs(vec![blur(&[-3.0])]), "and a blur of less than nothing is none");
    assert_eq!(
        plan_of(vec![blur(&[0.0])]).effects_not_run,
        0,
        "it did what it was asked to"
    );
    for wrong in [blur(&[]), blur(&[1.0, 2.0]), blur(&[f32::NAN]), blur(&[f32::INFINITY])] {
        let plan = plan_of(vec![wrong.clone()]);
        assert_eq!((plan.passes.len(), plan.effects_not_run), (2, 1), "{wrong:?}");
    }

    assert!(runs(vec![crop(&[0.1, 0.0, 0.0, 0.0, 0.0])]));
    assert!(
        runs(vec![crop(&[0.0, 0.0, 0.0, 0.0, 0.05])]),
        "a feather alone is a soft edge round the picture"
    );
    assert_eq!(plan_of(vec![crop(&[0.1, 0.0, 0.0, 0.0, 0.0])]).passes_recorded(), 2 + 1);
    assert!(!runs(vec![crop(&[0.0; 5])]));
    assert!(!runs(vec![crop(&[1.0e-5, 0.0, -1.0, 0.0, 0.0])]));
    assert_eq!(plan_of(vec![crop(&[0.0; 5])]).effects_not_run, 0);
    for wrong in [
        crop(&[0.1; 4]),
        crop(&[0.1; 6]),
        crop(&[]),
        crop(&[0.1, 0.1, 0.1, f32::NAN, 0.0]),
    ] {
        let plan = plan_of(vec![wrong.clone()]);
        assert_eq!((plan.passes.len(), plan.effects_not_run), (2, 1), "{wrong:?}");
    }
}

fn call_of(call: usize, converts_into_display: bool, converts_out_of_display: bool) -> EffectStep {
    EffectStep::Call {
        call,
        converts_into_display,
        converts_out_of_display,
    }
}

const INTO_THE_CODES: EffectStep = EffectStep::Conversion { into_display: true };
const OUT_OF_THE_CODES: EffectStep = EffectStep::Conversion { into_display: false };

#[test]
fn a_run_of_no_effects_has_no_steps_and_a_matrix_alone_converts_in_its_own_pass() {
    assert_eq!(steps_of_a_run_of_effects(&[], true), vec![]);
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(3)], true),
        vec![call_of(3, true, true)]
    );
}

#[test]
fn neighbours_of_the_display_referred_domain_are_converted_into_it_once_and_out_of_it_once() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_matrix(1), a_matrix(2)], true),
        vec![
            call_of(0, true, false),
            call_of(1, false, false),
            call_of(2, false, true)
        ],
        "three matrices: the first converts in, the last converts out, the one between does neither"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_shader(1), a_matrix(2)], true),
        vec![
            call_of(0, true, false),
            call_of(1, false, false),
            call_of(2, false, true)
        ],
        "a shader between two matrices: no pass of its own for a conversion"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader(0), a_shader(1)], true),
        vec![
            INTO_THE_CODES,
            call_of(0, false, false),
            call_of(1, false, false),
            OUT_OF_THE_CODES
        ],
        "two shaders: a pass that converts before them and one after them"
    );
}

#[test]
fn a_shader_at_the_edge_of_a_run_gets_a_pass_that_converts_and_a_matrix_there_converts_itself() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader(0)], true),
        vec![INTO_THE_CODES, call_of(0, false, false), OUT_OF_THE_CODES]
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_shader(1)], true),
        vec![call_of(0, true, false), call_of(1, false, false), OUT_OF_THE_CODES]
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader(0), a_matrix(1)], true),
        vec![INTO_THE_CODES, call_of(0, false, false), call_of(1, false, true)]
    );
}

#[test]
fn an_effect_of_the_other_domain_ends_a_run_and_the_next_display_referred_effect_begins_another() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_pass_of_light(1), a_matrix(2)], true),
        vec![call_of(0, true, true), call_of(1, false, false), call_of(2, true, true)]
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader(0), a_pass_of_light(1), a_shader(2)], true),
        vec![
            INTO_THE_CODES,
            call_of(0, false, false),
            OUT_OF_THE_CODES,
            call_of(1, false, false),
            INTO_THE_CODES,
            call_of(2, false, false),
            OUT_OF_THE_CODES
        ]
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_pass_of_light(0), a_pass_of_light(1)], true),
        vec![call_of(0, false, false), call_of(1, false, false)],
        "effects of light are not converted at all"
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_pass_of_light(0), a_matrix(1), a_pass_of_light(2)], true),
        vec![
            call_of(0, false, false),
            call_of(1, true, true),
            call_of(2, false, false)
        ]
    );
}

#[test]
fn in_the_compatibility_mode_nothing_is_converted_and_no_pass_is_added() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_shader(1), a_pass_of_light(2), a_shader(3)], false),
        vec![
            call_of(0, true, true),
            call_of(1, false, false),
            call_of(2, false, false),
            call_of(3, false, false)
        ],
        "(the colour matrix keeps the conversions it always had, which do nothing there)"
    );
}

fn in_light() -> FrameDescription {
    let mut description = FrameDescription::black((64, 36));
    description.colour.working = WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709);
    description
}

#[test]
fn a_plan_in_light_has_the_conversions_of_a_shader_and_counts_their_passes() {
    let mut known = EffectsThatRun::built_in_only();
    known.add_shader("test:shader");
    let shader = EffectCall {
        key: "test:shader".to_owned(),
        values: vec![1.0],
        table: None,
        state: None,
        images: Vec::new(),
    };
    let mut description = in_light();
    description.layers = vec![
        layer_with(1, vec![redder(), shader.clone(), greyer()]),
        layer_with(2, vec![shader]),
    ];
    let plan = plan_frame(&description, &known);
    assert_eq!(
        plan.steps_of_effects,
        vec![
            vec![
                call_of(0, true, false),
                call_of(1, false, false),
                call_of(2, false, true)
            ],
            vec![INTO_THE_CODES, call_of(0, false, false), OUT_OF_THE_CODES],
            vec![],
            vec![]
        ]
    );
    assert_eq!(
        plan.passes_recorded(),
        3 + 3 + 1 + 1,
        "the effects, the conversions, the layers and the output"
    );
    // The same layers in the compatibility mode need no conversion.
    description.colour = Default::default();
    let plan = plan_frame(&description, &known);
    assert_eq!(plan.passes_recorded(), 3 + 1 + 1 + 1);
}

#[test]
fn a_colour_matrix_is_given_nine_numbers_or_twelve_and_no_other_number() {
    let nine = [1.0, 0.2, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let call = |numbers: &[f32]| EffectCall {
        key: COLOUR_MATRIX_KEY.to_owned(),
        values: numbers.to_vec(),
        table: None,
        state: None,
        images: Vec::new(),
    };
    let with = |offsets: [f32; 3]| [nine.as_slice(), offsets.as_slice()].concat();
    for (numbers, runs) in [
        (nine.to_vec(), true),
        (with([0.1, 0.0, 0.0]), true),
        (nine[..8].to_vec(), false),
        ([nine.as_slice(), &[0.0, 0.0]].concat(), false),
        ([nine.as_slice(), &[0.0; 4]].concat(), false),
        (Vec::new(), false),
    ] {
        let plan = plan_of_the_built_in_effects(&description_of(vec![layer_with(1, vec![call(&numbers)])]));
        assert_eq!(
            plan.passes.len() == 3,
            runs,
            "{} numbers: {:?}",
            numbers.len(),
            plan.passes
        );
        assert_eq!(plan.effects_not_run, usize::from(!runs), "{} numbers", numbers.len());
    }
}

/// An effect run by a shader that takes `passes` passes of the card.
fn a_shader_of_passes(call: usize, passes: usize) -> EffectToRun {
    EffectToRun {
        passes,
        ..a_shader(call)
    }
}

fn later_pass_of(call: usize, pass: usize) -> EffectStep {
    EffectStep::LaterPass { call, pass }
}

#[test]
fn a_shader_that_takes_several_passes_has_them_one_after_another_numbered_from_the_second() {
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader_of_passes(0, 3)], false),
        vec![call_of(0, false, false), later_pass_of(0, 1), later_pass_of(0, 2)]
    );
    assert_eq!(
        steps_of_a_run_of_effects(&[a_matrix(0), a_shader_of_passes(1, 4), a_matrix(2)], true),
        vec![
            call_of(0, true, false),
            call_of(1, false, false),
            later_pass_of(1, 1),
            later_pass_of(1, 2),
            later_pass_of(1, 3),
            call_of(2, false, true),
        ],
        "(the conversions are done once round the passes of a shader, and not between them)"
    );
    // A shader of one pass has nothing after its first, and a shader of none is taken for one.
    assert_eq!(
        steps_of_a_run_of_effects(&[a_shader_of_passes(0, 1)], false),
        vec![call_of(0, false, false)]
    );
}

#[test]
fn a_shader_given_with_several_passes_takes_them_in_a_plan_and_counts_them_and_one_given_with_none_takes_one() {
    let mut known = EffectsThatRun::built_in_only();
    known.add_shader_with_passes("test:passes", 3);
    known.add_shader_with_passes("test:none", 0);
    known.add_shader("test:plain");
    let shader = |key: &str| EffectCall {
        key: key.to_owned(),
        values: vec![1.0],
        table: None,
        state: None,
        images: Vec::new(),
    };
    assert_eq!(known.passes_of("test:passes", KindOfEffect::Shader), 3);
    assert_eq!(known.passes_of("test:none", KindOfEffect::Shader), 1);
    assert_eq!(known.passes_of("test:plain", KindOfEffect::Shader), 1);
    assert_eq!(
        known.passes_of("test:never_given", KindOfEffect::Shader),
        1,
        "a shader that was not given is taken for one pass"
    );
    assert_eq!(
        known.passes_of(BLUR_KEY, KindOfEffect::Blur),
        2,
        "the blur's passes are the blur's"
    );

    let plan = plan_frame(
        &description_of(vec![layer_with(1, vec![shader("test:plain"), shader("test:passes")])]),
        &known,
    );
    assert_eq!(
        plan.steps_of_effects[0],
        vec![
            call_of(0, false, false),
            call_of(1, false, false),
            later_pass_of(1, 1),
            later_pass_of(1, 2)
        ]
    );
    assert_eq!(
        plan.passes_recorded(),
        4 + 1 + 1,
        "the passes, the layer and the output"
    );
    // The same key given again with another number of passes replaces the first.
    known.add_shader_with_passes("test:passes", 2);
    assert_eq!(known.passes_of("test:passes", KindOfEffect::Shader), 2);
    // An effect that is taken away takes no passes (and is not run).
    known.remove_shader("test:passes");
    assert_eq!(known.kind_of("test:passes"), None);
}

#[test]
fn a_colour_matrix_of_twelve_numbers_that_changes_nothing_is_left_out_and_not_counted_and_the_offsets_count() {
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let call = |offsets: [f32; 3]| EffectCall {
        key: COLOUR_MATRIX_KEY.to_owned(),
        values: [identity.as_slice(), offsets.as_slice()].concat(),
        table: None,
        state: None,
        images: Vec::new(),
    };
    let plan = plan_of_the_built_in_effects(&description_of(vec![layer_with(1, vec![call([0.0; 3])])]));
    assert_eq!(
        plan.passes.len(),
        2,
        "no offset and the matrix that changes nothing: nothing is run"
    );
    assert_eq!(plan.effects_not_run, 0);
    for place in 0..3 {
        let mut offsets = [0.0; 3];
        offsets[place] = 0.01;
        let plan = plan_of_the_built_in_effects(&description_of(vec![layer_with(1, vec![call(offsets)])]));
        assert_eq!(plan.passes.len(), 3, "an offset of {offsets:?} is a change");
    }
    // Less than a hundredth of a step of 256 is no change at all.
    let plan = plan_of_the_built_in_effects(&description_of(vec![layer_with(1, vec![call([1.0e-5, 0.0, -1.0e-5])])]));
    assert_eq!(plan.passes.len(), 2);
}
