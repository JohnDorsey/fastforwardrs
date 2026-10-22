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
//!
//! **Domains.** Every effect works in one of two colour domains (CM-10): the *display-referred* one, the codes of a screen (sRGB codes, Rec.709 primaries: what the hue shift
//! is made for, and what every effect that someone else wrote expects), or the *scene-linear* one, the light of the working space (what a blur or an exposure is made
//! for). The layer's picture is light, so a display-referred effect is run on codes that were made from it and gives light back; **two neighbours of one domain are
//! converted once and not twice** ([`steps_of_a_run_of_effects`]). The engine's colour matrix converts round itself, in its own pass, when it is the first or the last of
//! a run in the display-referred domain; a shader cannot, so a pass of its own does it (an [`EffectStep::Conversion`]). In the compatibility mode (the numbers already are
//! codes) there is nothing to convert, and there are no such passes.

use std::collections::{HashMap, HashSet};

use super::effect_shaders::MOST_NUMBERS_OF_A_FOREIGN_EFFECT;
use super::{
    BLUR_KEY, COLOUR_MATRIX_KEY, CROP_KEY, EXPOSURE_KEY, EffectCall, FrameDescription, LUT_KEY, LayerDescription,
    LayerSource, LutInterpolation, LutSpace, OPACITY_KEY, TableId, WorkingSpace,
};

/// How far a number of a colour matrix may be from what it is in the matrix that changes nothing, for the matrix to be taken for that one: a change
/// of less than a hundredth of a step of the 256 steps of a colour changes no pixel. (It is the same number that the window's own colour matrix uses;
/// a test holds the two to each other.)
pub const NEAR_ENOUGH_TO_CHANGE_NOTHING: f32 = 1.0 / 256.0 / 100.0;

/// How many numbers the engine's colour matrix is given: the nine of the matrix, row by row, or those and three offsets that are added to the mixed colour (F3).
pub const NUMBERS_OF_A_COLOUR_MATRIX: [usize; 2] = [9, 12];

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
    /// Clears the picture of the group that is container number `container` (see `FlatLayers`) to **clear** and puts the layers of the group in their places on it, back to front.
    /// `layers` are their numbers in the list of every layer (the frame's first): those of a group are numbered after those of the frame. It comes before the layer that is
    /// made of the group has its effects run, and before the layers of the container that it is in are put in their places.
    PlaceGroup { container: usize, layers: Vec<usize> },
    /// Turns the picture that was put together into the 8-bit frame.
    Output,
}

/// Every step of a frame, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub passes: Vec<PassPlan>,
    /// What each pass of `passes` that runs effects does, in order (the same length as `passes`; empty for the other passes): the calls of the pass, with the conversions
    /// between the colour domains that they need.
    pub steps_of_effects: Vec<Vec<EffectStep>>,
    /// How many effects of the description are left out because the engine cannot run them (not counting those that change nothing).
    pub effects_not_run: usize,
}

impl FramePlan {
    /// How many passes of the graphics card the frame takes: one for each effect that runs, one for each conversion between the colour domains that a pass cannot do
    /// itself, one that puts the layers together (or clears the picture) and one that makes the 8-bit frame.
    pub fn passes_recorded(&self) -> usize {
        self.passes
            .iter()
            .zip(&self.steps_of_effects)
            .map(|(pass, steps)| match pass {
                PassPlan::Effects { .. } => steps.len(),
                PassPlan::ClearAccumulation
                | PassPlan::PlaceLayers { .. }
                | PassPlan::PlaceGroup { .. }
                | PassPlan::Output => 1,
            })
            .sum()
    }
}

/// The colour domain an effect works in (F4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallDomain {
    /// The codes of a screen: sRGB codes of Rec.709 primaries.
    DisplayReferred,
    /// The light of the working space.
    SceneLinear,
}

/// One thing that is done to the picture of a layer in a pass that runs effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectStep {
    /// Runs the effect with this number among the layer's effects. For the engine's colour matrix the flags say whether its pass also converts the light to the codes
    /// before the mixing (`converts_into_display`) and the codes to light after it (`converts_out_of_display`); for a shader they are always false.
    Call {
        call: usize,
        converts_into_display: bool,
        converts_out_of_display: bool,
    },
    /// A pass after the first of the effect with this number among the layer's effects, for an effect that takes more than one: `pass` is its number (the first is 0, which is the
    /// `Call`). The blur takes two (along the rows, and then along the columns); a shader takes as many as its structure has (see [`ShaderStructure`](super::ShaderStructure)).
    LaterPass { call: usize, pass: usize },
    /// A pass of its own that only converts, light to display codes (`into_display`) or display codes to light.
    Conversion { into_display: bool },
}

/// What [`steps_of_a_run_of_effects`] needs to know of an effect that runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectToRun {
    /// The number of the effect among the layer's effects.
    pub call: usize,
    pub domain: CallDomain,
    /// How many passes of the card the effect takes (one; two for the blur; as many as the structure of a shader says).
    pub passes: usize,
    /// Whether the effect's own pass can do the conversion before it (when it is the first of its domain) and after it (when it is the last): the engine's colour matrix can.
    pub converts_itself: bool,
}

/// The steps that run `effects`, in order, on a picture of light (`working_space_is_light`) or of codes already (the compatibility mode), and give light back.
///
/// In the compatibility mode there is nothing to convert: the engine's colour matrix keeps the conversions that it has always had (which do nothing there). In light, an
/// effect of the display-referred domain after one of the other domain (or the first of all) is converted into, and one of the other domain after it (or the last of all)
/// is converted out of; **neighbours of one domain are not converted between**.
pub fn steps_of_a_run_of_effects(effects: &[EffectToRun], working_space_is_light: bool) -> Vec<EffectStep> {
    // Takes the picture out of the display-referred domain after the last step, by the pass of that step if it can, or by a pass of its own.
    fn leave(steps: &mut Vec<EffectStep>, effects: &[EffectToRun]) {
        match steps.last_mut() {
            Some(EffectStep::Call {
                call,
                converts_out_of_display,
                ..
            }) if effects
                .iter()
                .any(|effect| effect.call == *call && effect.converts_itself) =>
            {
                *converts_out_of_display = true;
            }
            _ => steps.push(EffectStep::Conversion { into_display: false }),
        }
    }
    // The call of an effect, and the second pass after it if it takes two.
    fn call(
        steps: &mut Vec<EffectStep>,
        effect: &EffectToRun,
        converts_into_display: bool,
        converts_out_of_display: bool,
    ) {
        steps.push(EffectStep::Call {
            call: effect.call,
            converts_into_display,
            converts_out_of_display,
        });
        for pass in 1..effect.passes {
            steps.push(EffectStep::LaterPass {
                call: effect.call,
                pass,
            });
        }
    }
    let mut steps: Vec<EffectStep> = Vec::with_capacity(effects.len());
    let mut in_display_codes = false;
    for effect in effects {
        if !working_space_is_light {
            call(&mut steps, effect, effect.converts_itself, effect.converts_itself);
            continue;
        }
        match effect.domain {
            CallDomain::DisplayReferred if !in_display_codes => {
                in_display_codes = true;
                if effect.converts_itself {
                    call(&mut steps, effect, true, false);
                } else {
                    steps.push(EffectStep::Conversion { into_display: true });
                    call(&mut steps, effect, false, false);
                }
            }
            CallDomain::DisplayReferred => call(&mut steps, effect, false, false),
            CallDomain::SceneLinear => {
                if in_display_codes {
                    leave(&mut steps, effects);
                    in_display_codes = false;
                }
                call(&mut steps, effect, false, false);
            }
        }
    }
    if in_display_codes {
        leave(&mut steps, effects);
    }
    steps
}

/// What an effect is run by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindOfEffect {
    /// The engine's own effect that mixes colours with a matrix of nine numbers, or twelve with three offsets.
    ColourMatrix,
    /// The engine's own effect that makes the light brighter or darker, by the stops that its one number says.
    Exposure,
    /// The engine's own effect that makes the picture more or less see-through, by the fraction that its one number says.
    Opacity,
    /// The engine's own Gaussian blur, by the radius that its one number says, in two passes.
    Blur,
    /// The engine's own crop, with the five numbers that [`CROP_KEY`] says.
    Crop,
    /// The engine's own look-up table effect, with the four numbers and the table that [`LUT_KEY`] says.
    Lut,
    /// A shader that was given to the engine (see `effect_shaders`), which is given as many numbers as the call has, up to a limit.
    Shader,
}

impl KindOfEffect {
    /// The colour domain the effect works in: the codes of a screen, for the engine's colour matrix (its matrices are made for them) and for every shader (what shader
    /// files are written for); the light of the working space, for exposure and opacity.
    pub fn domain(self) -> CallDomain {
        match self {
            Self::ColourMatrix | Self::Shader => CallDomain::DisplayReferred,
            Self::Exposure | Self::Opacity | Self::Blur | Self::Crop => CallDomain::SceneLinear,
            // (The domain of a look-up table depends on the call: see [`Self::domain_of_a_call`].)
            Self::Lut => CallDomain::SceneLinear,
        }
    }

    /// The colour domain the effect works in for this call. Only a look-up table differs from [`Self::domain`]: **a table of the codes of a screen, of which the whole is
    /// wanted, works in the display-referred domain** (neighbours of that domain are not converted between), and any other call of it is of the scene-linear domain and
    /// converts into and out of its spaces itself.
    pub fn domain_of_a_call(self, call: &EffectCall) -> CallDomain {
        match self {
            Self::Lut if lut_works_on_the_codes_of_a_screen(&call.values) => CallDomain::DisplayReferred,
            _ => self.domain(),
        }
    }

    /// Whether the pass of the effect for this call can convert into the display-referred domain and out of it itself (see [`EffectToRun::converts_itself`]).
    pub fn converts_itself_for_a_call(self, call: &EffectCall) -> bool {
        match self {
            Self::ColourMatrix => true,
            Self::Lut => lut_works_on_the_codes_of_a_screen(&call.values),
            Self::Shader | Self::Exposure | Self::Opacity | Self::Blur | Self::Crop => false,
        }
    }

    /// How many passes of the card an effect of this kind takes (a shader is one pass here; [`EffectsThatRun::passes_of`] says how many a given shader takes).
    pub fn passes(self) -> usize {
        match self {
            Self::Blur => 2,
            Self::ColourMatrix | Self::Shader | Self::Exposure | Self::Opacity | Self::Crop | Self::Lut => 1,
        }
    }
}

/// The effects the engine can run: the ones it has of its own, and the ones that were given to it as shaders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectsThatRun {
    /// The keys of the effects that shaders run, and how many passes each takes.
    keys_of_shaders: HashMap<String, usize>,
    /// The tables that the card has, which a look-up table effect needs one of.
    tables: HashSet<TableId>,
}

impl EffectsThatRun {
    /// The effects the engine has of its own, and no others.
    pub fn built_in_only() -> Self {
        Self::default()
    }

    /// The effect under `key` is also run, by a shader of one pass. (A key that is the engine's own stays the engine's own.)
    pub fn add_shader(&mut self, key: &str) {
        self.add_shader_with_passes(key, 1);
    }

    /// The effect under `key` is also run, by a shader that is run in `passes` passes (at least one).
    pub fn add_shader_with_passes(&mut self, key: &str, passes: usize) {
        self.keys_of_shaders.insert(key.to_owned(), passes.max(1));
    }

    /// How many passes of the card an effect takes: a shader, as many as it was given with; any other, as many as its kind takes.
    pub fn passes_of(&self, key: &str, kind: KindOfEffect) -> usize {
        match kind {
            KindOfEffect::Shader => self.keys_of_shaders.get(key).copied().unwrap_or(1),
            _ => kind.passes(),
        }
    }

    /// The effect under `key` is not run any more.
    pub fn remove_shader(&mut self, key: &str) {
        self.keys_of_shaders.remove(key);
    }

    /// These tables are the ones the engine has, and no others.
    pub fn set_tables(&mut self, tables: impl IntoIterator<Item = TableId>) {
        self.tables = tables.into_iter().collect();
    }

    /// Whether the engine has the table.
    pub fn has_the_table(&self, table: TableId) -> bool {
        self.tables.contains(&table)
    }

    /// How the effect under `key` is run, if the engine can run it.
    pub fn kind_of(&self, key: &str) -> Option<KindOfEffect> {
        if key == COLOUR_MATRIX_KEY {
            Some(KindOfEffect::ColourMatrix)
        } else if key == EXPOSURE_KEY {
            Some(KindOfEffect::Exposure)
        } else if key == OPACITY_KEY {
            Some(KindOfEffect::Opacity)
        } else if key == BLUR_KEY {
            Some(KindOfEffect::Blur)
        } else if key == CROP_KEY {
            Some(KindOfEffect::Crop)
        } else if key == LUT_KEY {
            Some(KindOfEffect::Lut)
        } else if self.keys_of_shaders.contains_key(key) {
            Some(KindOfEffect::Shader)
        } else {
            None
        }
    }
}

/// Whether the nine numbers of a matrix (row by row), and the three offsets that may follow them, are of one that changes no colour, or none that can be told from what it
/// was.
pub fn colour_matrix_changes_nothing(values: &[f32]) -> bool {
    const NOTHING_CHANGES: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
    values
        .iter()
        .zip(NOTHING_CHANGES)
        .all(|(number, nothing)| (number - nothing).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING)
}

/// Whether an exposure of this many stops is none, or none that can be told from none: the light is multiplied by a number that is less than a hundredth of a step of the
/// 256 steps of a colour from one.
pub fn exposure_changes_nothing(stops: f32) -> bool {
    (stops.exp2() - 1.0).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING
}

/// Whether an opacity is the whole opacity, or less than a hundredth of a step of the 256 steps of a colour from it. (More than the whole is the whole.)
pub fn opacity_changes_nothing(opacity: f32) -> bool {
    1.0 - opacity < NEAR_ENOUGH_TO_CHANGE_NOTHING
}

/// Whether a blur of this many points of a picture 1080 high is none, or none that can be told from none: the Gaussian is narrower than a hundredth of a pixel of a picture
/// that high. (A blur of less than nothing is none.)
pub fn blur_changes_nothing(radius: f32) -> bool {
    radius < 0.01
}

/// Whether the five numbers of a crop (the fractions cut off from the left, the top, the right and the bottom, and the width of the soft edge) are those of a crop that
/// changes nothing: no edge is cut and the soft edge has no width, or less than a ten-thousandth of the picture across for every one of them.
pub fn crop_changes_nothing(values: &[f32]) -> bool {
    values.iter().all(|value| *value < 1.0e-4)
}

/// Whether the four numbers of a look-up table effect say a table of the codes of a screen coming out as the codes of a screen, of which the whole is wanted: the one that
/// works in the display-referred domain.
fn lut_works_on_the_codes_of_a_screen(values: &[f32]) -> bool {
    matches!(values, [intensity, input, output, _]
        if *intensity >= 1.0 && *input == LutSpace::Screen.number() && *output == LutSpace::Screen.number())
}

/// Whether a look-up table effect with these numbers changes nothing: none of the table is wanted.
pub fn lut_changes_nothing(values: &[f32]) -> bool {
    values
        .first()
        .is_some_and(|intensity| *intensity < NEAR_ENOUGH_TO_CHANGE_NOTHING)
}

/// Which effects of `layer` run, and how many cannot.
fn effects_of(layer: &LayerDescription, effects_that_run: &EffectsThatRun) -> (Vec<usize>, usize) {
    // (The picture of a group is a picture for the effects to work on; a colour has none.)
    if matches!(layer.source, LayerSource::Solid(_)) {
        return (Vec::new(), layer.effects.len());
    }
    let (mut calls, mut not_run) = (Vec::new(), 0);
    for (number, effect) in layer.effects.iter().enumerate() {
        let numbers_are_finite = effect.values.iter().all(|value| value.is_finite());
        match effects_that_run.kind_of(&effect.key) {
            Some(KindOfEffect::ColourMatrix)
                if NUMBERS_OF_A_COLOUR_MATRIX.contains(&effect.values.len()) && numbers_are_finite =>
            {
                if !colour_matrix_changes_nothing(&effect.values) {
                    calls.push(number);
                }
            }
            Some(KindOfEffect::Exposure) if effect.values.len() == 1 && numbers_are_finite => {
                if !exposure_changes_nothing(effect.values[0]) {
                    calls.push(number);
                }
            }
            Some(KindOfEffect::Opacity) if effect.values.len() == 1 && numbers_are_finite => {
                if !opacity_changes_nothing(effect.values[0]) {
                    calls.push(number);
                }
            }
            Some(KindOfEffect::Lut)
                if effect.values.len() == 4
                    && numbers_are_finite
                    && LutSpace::from_number(effect.values[1]).is_some()
                    && LutSpace::from_number(effect.values[2]).is_some()
                    && LutInterpolation::from_number(effect.values[3]).is_some()
                    && effect.table.is_some_and(|table| effects_that_run.has_the_table(table)) =>
            {
                if !lut_changes_nothing(&effect.values) {
                    calls.push(number);
                }
            }
            Some(KindOfEffect::Blur) if effect.values.len() == 1 && numbers_are_finite => {
                if !blur_changes_nothing(effect.values[0]) {
                    calls.push(number);
                }
            }
            Some(KindOfEffect::Crop) if effect.values.len() == 5 && numbers_are_finite => {
                if !crop_changes_nothing(&effect.values) {
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

/// The pass that runs the effects of the layer `layer`, which is number `number` in the list of every layer, and the steps of it, if any of its effects runs; and how many of its
/// effects cannot.
fn effects_pass_of(
    number: usize,
    layer: &LayerDescription,
    effects_that_run: &EffectsThatRun,
    working_space_is_light: bool,
) -> (Option<(PassPlan, Vec<EffectStep>)>, usize) {
    let (calls, not_run) = effects_of(layer, effects_that_run);
    if calls.is_empty() {
        return (None, not_run);
    }
    let to_run: Vec<EffectToRun> = calls
        .iter()
        .map(|&call| {
            let kind = effects_that_run
                .kind_of(&layer.effects[call].key)
                .expect("a call that runs is of a kind that the engine can run");
            let call_itself = &layer.effects[call];
            EffectToRun {
                call,
                domain: kind.domain_of_a_call(call_itself),
                passes: effects_that_run.passes_of(&call_itself.key, kind),
                converts_itself: kind.converts_itself_for_a_call(call_itself),
            }
        })
        .collect();
    let steps = steps_of_a_run_of_effects(&to_run, working_space_is_light);
    (Some((PassPlan::Effects { layer: number, calls }, steps)), not_run)
}

/// The steps that make the frame `description` describes, with the effects `effects_that_run` says the engine can run.
///
/// **The groups come first, the last one first** (see [`FlatLayers`](super::FlatLayers)): for each, the effects of its layers and then the pass that puts its layers together in its picture; so that the
/// picture of a group is made before the effects of the layer that it is run, and before the layers of the container it is in are put in their places. The frame's own layers come
/// last, as they did when there were no groups.
pub fn plan_frame(description: &FrameDescription, effects_that_run: &EffectsThatRun) -> FramePlan {
    let flat = description.flatten();
    let mut passes: Vec<PassPlan> = Vec::new();
    let mut steps_of_passes: Vec<Vec<EffectStep>> = Vec::new();
    let mut effects_not_run = 0;
    let working_space_is_light = matches!(description.colour.working, WorkingSpace::Linear(_));
    for container in (0..flat.containers.len()).rev() {
        let numbers = flat.containers[container].clone();
        for number in numbers.clone() {
            let (pass, not_run) = effects_pass_of(
                number,
                flat.layers[number].layer,
                effects_that_run,
                working_space_is_light,
            );
            effects_not_run += not_run;
            if let Some((pass, steps)) = pass {
                passes.push(pass);
                steps_of_passes.push(steps);
            }
        }
        // (Every pass that does not run effects has an empty list of steps.)
        if container > 0 {
            passes.push(PassPlan::PlaceGroup {
                container,
                layers: numbers.collect(),
            });
            steps_of_passes.push(Vec::new());
        } else if numbers.is_empty() {
            passes.push(PassPlan::ClearAccumulation);
            steps_of_passes.push(Vec::new());
        } else {
            passes.push(PassPlan::PlaceLayers {
                layers: numbers.collect(),
            });
            steps_of_passes.push(Vec::new());
        }
    }
    passes.push(PassPlan::Output);
    steps_of_passes.push(Vec::new());
    FramePlan {
        passes,
        steps_of_effects: steps_of_passes,
        effects_not_run,
    }
}

#[cfg(test)]
mod tests {
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
        };
        let too_few_numbers = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![1.0; 8],
            table: None,
        };
        let too_many_numbers = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![1.0; 10],
            table: None,
        };
        let not_a_number = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![f32::NAN, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            table: None,
        };
        let infinite = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![f32::INFINITY, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            table: None,
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
        }
    }

    fn a_shader(call: usize) -> EffectToRun {
        EffectToRun {
            call,
            domain: CallDomain::DisplayReferred,
            passes: 1,
            converts_itself: false,
        }
    }

    fn a_pass_of_light(call: usize) -> EffectToRun {
        EffectToRun {
            call,
            domain: CallDomain::SceneLinear,
            passes: 1,
            converts_itself: false,
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
        };
        let crop = |numbers: &[f32]| EffectCall {
            key: CROP_KEY.to_owned(),
            values: numbers.to_vec(),
            table: None,
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
        let plan =
            plan_of_the_built_in_effects(&description_of(vec![layer_with(1, vec![call([1.0e-5, 0.0, -1.0e-5])])]));
        assert_eq!(plan.passes.len(), 2);
    }
}
