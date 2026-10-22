// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What each step of the effects of a layer draws into, and what it reads, worked out before anything is drawn (phase 6.4; see `docs/engine-phase-6-4-design.md`, K3 and K7).
//!
//! A step of a shader (its first pass, or a later one) draws into a texture of its own: the buffer that the pass says, as big as its expression says, or the picture the effect
//! makes. Every other step draws into a picture as big as the one the effect works on, as it always did. This is a function of its arguments and has no graphics card in it.

use std::sync::Arc;

use super::EffectCall;
use super::effect_shaders::texture_format_of;
use super::plan::EffectStep;
use super::shader_structure::{ImageSource, ShaderStructure, TargetFormat};
use super::texture_pool::TextureShape;

/// What a step that runs a pass of a shader reads and draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderStep {
    /// The number of the pass (the first is 0).
    pub pass: usize,
    /// The place, among the steps of the layer's effects, of the step of the first pass of this call: the pass `p` of the call is the step `first_step + p`.
    pub first_step: usize,
    /// The place, among the steps of the layer's effects, of the step that made the backdrop into codes for this call (phase 6.6), if the shader reads the backdrop and the plan put such
    /// a step before the call.
    pub backdrop_step: Option<usize>,
    /// How many pictures the shader reads (the bindings the step needs).
    pub images: usize,
    /// The format of what the step draws into.
    pub target_format: TargetFormat,
    /// The buffer the step draws into, if it draws into one.
    pub target_buffer: Option<usize>,
    /// What each of the pictures of the shader reads in this pass.
    pub reads: Vec<ImageSource>,
}

/// What one step of the effects of a layer draws into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepPlan {
    /// The texture the step draws into.
    pub shape: TextureShape,
    /// How many pixels across and down the step draws (`RENDERSIZE` for a shader).
    pub size: (u32, u32),
    /// Which pass of a shader the step is, if it is one.
    pub shader: Option<ShaderStep>,
}

/// What each of `steps` draws and reads. `effects` are the effects of the layer, which the steps are numbered among; `structure_of` says what a shader that was given to the engine is
/// made of; `base_shape` is the shape of the picture that the effects work on (what a step that is not a pass of a shader draws into); `picture_size` is its size; `frame_shape` is the
/// shape of the picture that the layers are put together in (what the step that makes the backdrop into codes draws into: the backdrop is the size of the frame, whatever the picture
/// of the layer is); `largest_side` is the most pixels across that the card allows a texture to be.
pub fn plan_the_steps(
    steps: &[EffectStep],
    effects: &[EffectCall],
    structure_of: impl Fn(&str) -> Option<Arc<ShaderStructure>>,
    base_shape: TextureShape,
    picture_size: (u32, u32),
    frame_shape: TextureShape,
    largest_side: u32,
) -> Vec<StepPlan> {
    let mut first_step_of_the_call: Vec<(usize, usize)> = Vec::new();
    let mut backdrop_step_of_the_call: Vec<(usize, usize)> = Vec::new();
    steps
        .iter()
        .enumerate()
        .map(|(place, step)| {
            let (call, pass) = match *step {
                EffectStep::Conversion { .. } => {
                    return StepPlan {
                        shape: base_shape,
                        size: picture_size,
                        shader: None,
                    };
                }
                EffectStep::Backdrop { call } => {
                    backdrop_step_of_the_call.push((call, place));
                    return StepPlan {
                        shape: frame_shape,
                        size: (frame_shape.width_in_pixels, frame_shape.height_in_pixels),
                        shader: None,
                    };
                }
                EffectStep::Call { call, .. } => {
                    first_step_of_the_call.push((call, place));
                    (call, 0)
                }
                EffectStep::LaterPass { call, pass } => (call, pass),
            };
            let Some(structure) = structure_of(&effects[call].key) else {
                return StepPlan {
                    shape: base_shape,
                    size: picture_size,
                    shader: None,
                };
            };
            let facts = structure.facts_of_the_passes(picture_size, &effects[call].values, largest_side);
            let facts = facts[pass.min(facts.len() - 1)];
            let first_step = first_step_of_the_call
                .iter()
                .rev()
                .find(|(of, _)| *of == call)
                .map_or(place, |(_, first)| *first);
            StepPlan {
                shape: TextureShape {
                    width_in_pixels: facts.size.0,
                    height_in_pixels: facts.size.1,
                    format: texture_format_of(facts.format),
                    usage: base_shape.usage,
                },
                size: facts.size,
                shader: Some(ShaderStep {
                    pass,
                    first_step,
                    backdrop_step: backdrop_step_of_the_call
                        .iter()
                        .rev()
                        .find(|(of, _)| *of == call)
                        .map(|(_, backdrop)| *backdrop),
                    images: structure.images.len(),
                    target_format: facts.format,
                    target_buffer: facts.target,
                    reads: structure.what_each_image_reads(pass),
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
