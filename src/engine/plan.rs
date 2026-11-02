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
    /// Puts the layers in their places **over what is in the picture that the layers are put together in**, back to front, without clearing it first (phase 6.6): what comes after a layer that
    /// read what was behind it, which was made by the passes before. `layers` are the numbers of the layers in the description.
    PlaceLayersOver { layers: Vec<usize> },
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
                | PassPlan::PlaceLayersOver { .. }
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
    /// A pass of its own that makes **the picture that the layers behind this one make** (what the picture that the layers are put together in holds now: the *backdrop*) into a picture of
    /// display codes, for the call with this number among the layer's effects that reads it (phase 6.6). It is right before that call and **does not change the picture that the next
    /// effect works on**: the call reads this picture and the layer's own. (In the compatibility mode the numbers already are codes, and the pass copies.)
    Backdrop { call: usize },
    /// A pass of its own that makes the mask number `mask` of the call with this number among the layer's effects (the picture of a group that is drawn off the frame: see
    /// [`EffectCall::masks`]) into a picture of display codes, for the call to read (`docs/shader-masks-design.md`). It is right before the call, once for each mask, and **does not
    /// change the picture that the next effect works on**: the call reads this picture and the layer's own.
    Mask { call: usize, mask: usize },
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
    /// Whether the effect reads what is behind the layer (a shader that was given with a picture of that kind: see [`EffectsThatRun::reads_the_backdrop`]), so that a step makes
    /// the backdrop into codes before it. Only a shader does, and a shader works on the codes of a screen.
    pub reads_the_backdrop: bool,
    /// How many masks the effect reads (a shader that was given as one that does: see [`EffectsThatRun::masks_of`]), so that a step makes each into codes before it.
    pub masks: usize,
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
    // The call of an effect, and the second pass after it if it takes two. The effect that reads the backdrop has the step that makes it before the call, once, and not before its later passes.
    fn call(
        steps: &mut Vec<EffectStep>,
        effect: &EffectToRun,
        converts_into_display: bool,
        converts_out_of_display: bool,
    ) {
        if effect.reads_the_backdrop {
            steps.push(EffectStep::Backdrop { call: effect.call });
        }
        for mask in 0..effect.masks {
            steps.push(EffectStep::Mask {
                call: effect.call,
                mask,
            });
        }
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

/// What the plan needs to know of a shader that was given to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FactsOfAShader {
    /// How many passes of the card the shader takes.
    passes: usize,
    /// How many images it brings with it (which a call has to name the pictures of).
    imported_images: usize,
    /// Whether it reads what is behind its layer (phase 6.6).
    reads_the_backdrop: bool,
    /// How many masks it reads (which a call has to have the groups of).
    masks: usize,
}

/// The effects the engine can run: the ones it has of its own, and the ones that were given to it as shaders.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectsThatRun {
    /// The keys of the effects that shaders run, and what the plan needs to know of each.
    keys_of_shaders: HashMap<String, FactsOfAShader>,
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
        self.add_shader_with_images(key, passes, 0);
    }

    /// The effect under `key` is also run, by a shader that is run in `passes` passes (at least one) and brings `imported` images with it: a call that does not name that many
    /// pictures is not run.
    pub fn add_shader_with_images(&mut self, key: &str, passes: usize, imported: usize) {
        self.keys_of_shaders.insert(
            key.to_owned(),
            FactsOfAShader {
                passes: passes.max(1),
                imported_images: imported,
                reads_the_backdrop: false,
                masks: 0,
            },
        );
    }

    /// The effect under `key` is also run, by a shader that is run in `passes` passes (at least one), brings `imported` images with it, and **reads `masks` masks** (`docs/shader-masks-design.md`):
    /// a call that does not have that many groups for them is not run.
    pub fn add_shader_with_masks(&mut self, key: &str, passes: usize, imported: usize, masks: usize) {
        self.add_shader_with_images(key, passes, imported);
        self.set_the_masks_of(key, masks);
    }

    /// The shader that was given under `key` reads `masks` masks. (Nothing happens for a key that was not given.)
    pub fn set_the_masks_of(&mut self, key: &str, masks: usize) {
        if let Some(facts) = self.keys_of_shaders.get_mut(key) {
            facts.masks = masks;
        }
    }

    /// The effect under `key` is also run, by a shader that is run in `passes` passes (at least one), brings `imported` images with it, and **reads what is behind its layer** as well as the
    /// layer's own picture (phase 6.6: a transition). It is run only on a layer of the frame itself: a layer of a group is not behind anything that is opaque.
    pub fn add_shader_that_reads_the_backdrop(&mut self, key: &str, passes: usize, imported: usize) {
        self.add_shader_with_images(key, passes, imported);
        if let Some(facts) = self.keys_of_shaders.get_mut(key) {
            facts.reads_the_backdrop = true;
        }
    }

    /// Whether the shader under `key` reads what is behind its layer (nothing that is not a shader that was given does).
    pub fn reads_the_backdrop(&self, key: &str) -> bool {
        self.keys_of_shaders
            .get(key)
            .is_some_and(|facts| facts.reads_the_backdrop)
    }

    /// How many masks the shader under `key` reads (none for anything that is not a shader that was given).
    pub fn masks_of(&self, key: &str) -> usize {
        self.keys_of_shaders.get(key).map_or(0, |facts| facts.masks)
    }

    /// How many images the shader under `key` brings with it (none for anything that is not a shader that was given).
    pub fn imported_images_of(&self, key: &str) -> usize {
        self.keys_of_shaders.get(key).map_or(0, |facts| facts.imported_images)
    }

    /// How many passes of the card an effect takes: a shader, as many as it was given with; any other, as many as its kind takes.
    pub fn passes_of(&self, key: &str, kind: KindOfEffect) -> usize {
        match kind {
            KindOfEffect::Shader => self.keys_of_shaders.get(key).map_or(1, |facts| facts.passes),
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

/// Which effects of `layer` run, and how many cannot. A shader that reads what is behind its layer runs only `in_the_frame` (the layer is one of the frame's own: what is behind it is
/// opaque, as every frame is put together over opaque black); in a group the picture that the layers are put together in begins clear, and the effect is counted as one that is not run.
fn effects_of(layer: &LayerDescription, effects_that_run: &EffectsThatRun, in_the_frame: bool) -> (Vec<usize>, usize) {
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
                if effect.values.len() <= MOST_NUMBERS_OF_A_FOREIGN_EFFECT
                    && numbers_are_finite
                    && effect.images.len() == effects_that_run.imported_images_of(&effect.key)
                    && effect.masks.len() == effects_that_run.masks_of(&effect.key)
                    && (in_the_frame || !effects_that_run.reads_the_backdrop(&effect.key)) =>
            {
                calls.push(number);
            }
            _ => not_run += 1,
        }
    }
    (calls, not_run)
}

/// The pass that runs the effects of the layer `layer`, which is number `number` in the list of every layer and is one of the frame's own if `in_the_frame`, and the steps of it, if
/// any of its effects runs; and how many of its effects cannot.
fn effects_pass_of(
    number: usize,
    layer: &LayerDescription,
    effects_that_run: &EffectsThatRun,
    working_space_is_light: bool,
    in_the_frame: bool,
) -> (Option<(PassPlan, Vec<EffectStep>)>, usize) {
    let (calls, not_run) = effects_of(layer, effects_that_run, in_the_frame);
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
                reads_the_backdrop: kind == KindOfEffect::Shader
                    && effects_that_run.reads_the_backdrop(&call_itself.key),
                masks: if kind == KindOfEffect::Shader {
                    effects_that_run.masks_of(&call_itself.key)
                } else {
                    0
                },
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
        // The layers that have an effect that reads what is behind them, with the pass that runs their effects: it is not run with the others but when what is behind each of them has been put
        // together (see below).
        let mut reading_the_backdrop: Vec<(usize, PassPlan, Vec<EffectStep>)> = Vec::new();
        for number in numbers.clone() {
            let (pass, not_run) = effects_pass_of(
                number,
                flat.layers[number].layer,
                effects_that_run,
                working_space_is_light,
                container == 0,
            );
            effects_not_run += not_run;
            let Some((pass, steps)) = pass else { continue };
            if steps.iter().any(|step| matches!(step, EffectStep::Backdrop { .. })) {
                reading_the_backdrop.push((number, pass, steps));
            } else {
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
        } else if reading_the_backdrop.is_empty() {
            passes.push(PassPlan::PlaceLayers {
                layers: numbers.collect(),
            });
            steps_of_passes.push(Vec::new());
        } else {
            // The layers behind each layer that reads the backdrop are put in their places (the first of them clearing; if there are none, the picture is cleared) before the effects of the layer
            // run, and the layers from it up to the next one that reads the backdrop are put over what is there afterwards.
            let mut next_to_put = numbers.start;
            for (number, pass, steps) in reading_the_backdrop {
                if number > next_to_put {
                    let layers = (next_to_put..number).collect();
                    passes.push(if next_to_put == numbers.start {
                        PassPlan::PlaceLayers { layers }
                    } else {
                        PassPlan::PlaceLayersOver { layers }
                    });
                    steps_of_passes.push(Vec::new());
                } else if next_to_put == numbers.start {
                    passes.push(PassPlan::ClearAccumulation);
                    steps_of_passes.push(Vec::new());
                }
                passes.push(pass);
                steps_of_passes.push(steps);
                next_to_put = number;
            }
            passes.push(PassPlan::PlaceLayersOver {
                layers: (next_to_put..numbers.end).collect(),
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
mod tests;
