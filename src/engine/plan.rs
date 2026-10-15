// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a frame is made of, worked out from its description before anything is drawn: which passes, in which order.
//!
//! It is a function of the description alone (no graphics card in it), so that what the engine will do for a frame can be looked at and tested
//! without one: that a layer with no effects is put in its place straight from its picture, that the effects of a layer are run before it is put
//! in its place and in the order they were asked for, that the first thing drawn clears the picture, and so on.
//!
//! **Which effects run.** An effect that the engine does not have (its key is not one the engine knows: [`EffectsThatRun`] says which it knows), or that is
//! given numbers it cannot use, is left out and counted (`FramePlan::effects_not_run`), as an effect that is not installed does nothing. So is an effect on a
//! layer that is only a colour, which has no picture to do anything to. An effect that changes nothing is left out too, and not counted: it has done what it was
//! asked to.

use std::collections::HashSet;

use super::effect_shaders::MOST_NUMBERS_OF_A_FOREIGN_EFFECT;
use super::{COLOUR_MATRIX_KEY, FrameDescription, LayerDescription, LayerSource};

/// How far a number of a colour matrix may be from what it is in the matrix that changes nothing, for the matrix to be taken for that one: a change
/// of less than a hundredth of a step of the 256 steps of a colour changes no pixel. (It is the same number that the window's own colour matrix uses;
/// a test holds the two to each other.)
pub const NEAR_ENOUGH_TO_CHANGE_NOTHING: f32 = 1.0 / 256.0 / 100.0;

/// One step of making a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassPlan {
    /// Clears the picture that the layers are put together in to opaque black, when there are no layers to draw over it.
    ClearAccumulation,
    /// Runs some of the effects of a layer, one after another, the first on the layer's own picture and each of the others on the picture the one
    /// before made. `calls` are the numbers of the layer's effects that run (the others are left out), in the order they run in. It comes before the
    /// layers are put in their places, and the layer is then put in its place from the picture the last of them made.
    Effects { layer: usize, calls: Vec<usize> },
    /// Clears the picture that the layers are put together in to opaque black and then puts the layers in their places on it, back to front. `layers`
    /// are the numbers of the layers in the description.
    PlaceLayers { layers: Vec<usize> },
    /// Turns the picture that was put together into the 8-bit frame.
    Output,
}

/// Every step of a frame, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub passes: Vec<PassPlan>,
    /// How many effects of the description are left out because the engine cannot run them (not counting those that change nothing).
    pub effects_not_run: usize,
}

impl FramePlan {
    /// How many passes of the graphics card the frame takes: one for each effect that runs, one that puts the layers together (or clears the
    /// picture) and one that makes the 8-bit frame.
    pub fn passes_recorded(&self) -> usize {
        self.passes
            .iter()
            .map(|pass| match pass {
                PassPlan::Effects { calls, .. } => calls.len(),
                PassPlan::ClearAccumulation | PassPlan::PlaceLayers { .. } | PassPlan::Output => 1,
            })
            .sum()
    }
}

/// What an effect is run by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindOfEffect {
    /// The engine's own effect that mixes colours with a matrix of nine numbers.
    ColourMatrix,
    /// A shader that was given to the engine (see `effect_shaders`), which is given as many numbers as the call has, up to a limit.
    Shader,
}

/// The effects the engine can run: the ones it has of its own, and the ones that were given to it as shaders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectsThatRun {
    keys_of_shaders: HashSet<String>,
}

impl EffectsThatRun {
    /// The effects the engine has of its own, and no others.
    pub fn built_in_only() -> Self {
        Self::default()
    }

    /// The effect under `key` is also run, by a shader. (A key that is the engine's own stays the engine's own.)
    pub fn add_shader(&mut self, key: &str) {
        self.keys_of_shaders.insert(key.to_owned());
    }

    /// The effect under `key` is not run any more.
    pub fn remove_shader(&mut self, key: &str) {
        self.keys_of_shaders.remove(key);
    }

    /// How the effect under `key` is run, if the engine can run it.
    pub fn kind_of(&self, key: &str) -> Option<KindOfEffect> {
        if key == COLOUR_MATRIX_KEY {
            Some(KindOfEffect::ColourMatrix)
        } else if self.keys_of_shaders.contains(key) {
            Some(KindOfEffect::Shader)
        } else {
            None
        }
    }
}

/// Whether nine numbers (a matrix, row by row) are one that changes no colour, or none that can be told from what it was.
pub fn colour_matrix_changes_nothing(values: &[f32]) -> bool {
    const NOTHING_CHANGES: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    values
        .iter()
        .zip(NOTHING_CHANGES)
        .all(|(number, nothing)| (number - nothing).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING)
}

/// Which effects of `layer` run, and how many cannot.
fn effects_of(layer: &LayerDescription, effects_that_run: &EffectsThatRun) -> (Vec<usize>, usize) {
    if matches!(layer.source, LayerSource::Solid(_)) {
        return (Vec::new(), layer.effects.len());
    }
    let (mut calls, mut not_run) = (Vec::new(), 0);
    for (number, effect) in layer.effects.iter().enumerate() {
        let numbers_are_finite = effect.values.iter().all(|value| value.is_finite());
        match effects_that_run.kind_of(&effect.key) {
            Some(KindOfEffect::ColourMatrix) if effect.values.len() == 9 && numbers_are_finite => {
                if !colour_matrix_changes_nothing(&effect.values) {
                    calls.push(number);
                }
            }
            Some(KindOfEffect::Shader)
                if effect.values.len() <= MOST_NUMBERS_OF_A_FOREIGN_EFFECT && numbers_are_finite =>
            {
                calls.push(number);
            }
            _ => not_run += 1,
        }
    }
    (calls, not_run)
}

/// The steps that make the frame `description` describes, with the effects `effects_that_run` says the engine can run.
pub fn plan_frame(description: &FrameDescription, effects_that_run: &EffectsThatRun) -> FramePlan {
    let mut passes = Vec::new();
    let mut effects_not_run = 0;
    for (number, layer) in description.layers.iter().enumerate() {
        let (calls, not_run) = effects_of(layer, effects_that_run);
        effects_not_run += not_run;
        if !calls.is_empty() {
            passes.push(PassPlan::Effects { layer: number, calls });
        }
    }
    if description.layers.is_empty() {
        passes.push(PassPlan::ClearAccumulation);
    } else {
        passes.push(PassPlan::PlaceLayers {
            layers: (0..description.layers.len()).collect(),
        });
    }
    passes.push(PassPlan::Output);
    FramePlan {
        passes,
        effects_not_run,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        };
        let too_few_numbers = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![1.0; 8],
        };
        let too_many_numbers = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![1.0; 10],
        };
        let not_a_number = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![f32::NAN, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        };
        let infinite = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![f32::INFINITY, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
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
        // And ones that are one number off from the one that changes nothing by amounts on both sides of the limit.
        for place in 0..9 {
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

    /// A `ColourMatrix` with these nine numbers, row by row.
    fn matrix_of(numbers: [f32; 9]) -> crate::colour_effects::ColourMatrix {
        crate::colour_effects::ColourMatrix::from_rows([
            [numbers[0], numbers[1], numbers[2]],
            [numbers[3], numbers[4], numbers[5]],
            [numbers[6], numbers[7], numbers[8]],
        ])
    }
}
