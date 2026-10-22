// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Effects that someone else wrote: what a shader of one has to be, and how the engine runs it.
//!
//! The engine has an effect of its own (`engine:colour_matrix`, which mixes the colours of a picture). Others are given to it as **shaders**: the program
//! says "the effect under this key is this shader" ([`Engine::compile_effect_shader`](super::Engine::compile_effect_shader)), the shader goes through the
//! guard (`guard`: it is read, checked, and every loop of it is given a limit), and frames can then ask for the effect by its key like any other
//! (an [`EffectCall`](super::EffectCall) with the key and some numbers).
//!
//! **What the shader of an effect is** (this is the first form of it, the one phase 1 needs to prove the guard on; the host of phase 5 gives it more):
//!
//! * It is a WGSL module with a fragment entry point named `fs_main` that takes `@builtin(position) position: vec4<f32>` (the pixel it is working on, in
//!   pixels, from the top left) and gives `@location(0) vec4<f32>`: the new colour of the pixel, **premultiplied** (its red, green and blue are already
//!   multiplied by its opacity). The vertex stage is the engine's own (`shaders/full_screen_triangle.wgsl`): one triangle that covers the picture. It is run
//!   on a picture the size of the layer's own picture, before the layer is put in its place.
//! * It may use these three resources, and no others, all in `@group(0)`: `@binding(0)` a `var<uniform>` of [`FOREIGN_EFFECT_UNIFORM_BYTES`] bytes
//!   that holds the numbers of the call, one `f32` after another from the first byte, zeros after them up to the [`MOST_NUMBERS_OF_A_FOREIGN_EFFECT`], and then
//!   [`NUMBERS_THE_ENGINE_ADDS`] of the engine's own: **the width and the height, in pixels, of the picture the effect works on**, and two zeros (phase 6: an ISF shader's `RENDERSIZE`); `@binding(1)` a `texture_2d<f32>`, the picture
//!   the effect works on (premultiplied, half-float); `@binding(2)` a filtering `sampler` that smooths between the pixels of the picture and uses
//!   the pixel at the edge for what is beyond it.
//! * A call gives it at most [`MOST_NUMBERS_OF_A_FOREIGN_EFFECT`] numbers, all of them finite; a call that gives more, or one that is not a number, is not run (and is
//!   counted, as an effect the engine has not got is).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::guard::GuardedShader;
use super::pipelines::{ACCUMULATION_FORMAT, pipeline_of_two_modules, shader_module};
use super::shader_structure::{MOST_IMAGES_OF_A_SHADER, ShaderStructure, TargetFormat};
use super::shaders;
use super::uniform_bytes::SlotWriter;

/// What is on the shelf for an effect: the WGSL of its shader and what the shader is made of besides its code (see [`ShaderStructure`]).
type OnTheShelf = (Arc<str>, Arc<ShaderStructure>);

/// The shaders that were put on the shelf (see [`publish_effect_shader`]), by the key of the effect they run.
static SHELF: Mutex<Option<HashMap<String, OnTheShelf>>> = Mutex::new(None);

/// Puts the WGSL `source` of the effect under `key` on the shelf, for **the engine to take when a frame asks for the effect**: whoever describes a frame does not need a
/// handle of the engine to give it the shader (phase 6: the host of ISF shaders makes the WGSL and publishes it; the window's engine and the export's engine each take what they
/// are asked for). A key that is on the shelf already has its shader replaced. The shader is **one pass that reads one picture** (see [`publish_effect_shader_with`] for one that is
/// not).
pub fn publish_effect_shader(key: &str, source: &str) {
    publish_effect_shader_with(key, source, ShaderStructure::one_pass_of_one_picture());
}

/// The same, for a shader that is made of what `structure` says: the pictures it reads, the buffers it draws into, and its passes.
pub fn publish_effect_shader_with(key: &str, source: &str, structure: ShaderStructure) {
    SHELF
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(key.to_owned(), (Arc::from(source), Arc::new(structure)));
}

/// The shader that is on the shelf for the effect under `key`, if there is one.
pub fn published_effect_shader(key: &str) -> Option<Arc<str>> {
    published_effect_shader_with_structure(key).map(|(source, _)| source)
}

/// The shader that is on the shelf for the effect under `key`, and what it is made of, if there is one.
pub fn published_effect_shader_with_structure(key: &str) -> Option<(Arc<str>, Arc<ShaderStructure>)> {
    SHELF
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|shelf| shelf.get(key).cloned())
}

/// The name of the fragment entry point every effect shader has.
pub const FOREIGN_EFFECT_ENTRY_POINT: &str = "fs_main";

/// How many numbers a call of an effect that others wrote can give its shader.
pub const MOST_NUMBERS_OF_A_FOREIGN_EFFECT: usize = 64;

/// How many numbers the engine puts after the ones of a call: the width and the height of the picture the effect works on, in pixels, and two zeros.
pub const NUMBERS_THE_ENGINE_ADDS: usize = 4;

/// How many bytes of uniforms a pass of such an effect is given: room for the most numbers there can be and the ones the engine adds. (A shader that wants more is
/// refused when the graphics library is given it.)
pub const FOREIGN_EFFECT_UNIFORM_BYTES: u64 =
    ((MOST_NUMBERS_OF_A_FOREIGN_EFFECT + NUMBERS_THE_ENGINE_ADDS) * std::mem::size_of::<f32>()) as u64;

/// The format of a buffer that is not `float`: 8 bits of each colour.
pub const EIGHT_BIT_BUFFER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The format that a pass draws into, for the kind of target that the structure of a shader says.
pub fn texture_format_of(format: TargetFormat) -> wgpu::TextureFormat {
    match format {
        TargetFormat::Half => ACCUMULATION_FORMAT,
        TargetFormat::EightBit => EIGHT_BIT_BUFFER_FORMAT,
    }
}

/// The numbers a pass of a call is given: those of the call, with the number of the pass in the place that the structure says (`PASSINDEX`), if it says one. (A shader of phase 1
/// owns all its numbers.)
pub fn numbers_of_a_pass(values: &[f32], structure: &ShaderStructure, pass: usize) -> Vec<f32> {
    let mut numbers = values.to_vec();
    if let Some(place) = structure.pass_index_number {
        if numbers.len() <= place {
            numbers.resize(place + 1, 0.0);
        }
        numbers[place] = pass as f32;
    }
    numbers
}

/// The bindings of a pass for shaders that read some number of pictures, and the layout of the pipeline that is made from them.
struct LayoutForPictures {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
}

/// What the pipelines of all the effects that others wrote are made from: the same vertex stage, the same sampler, and for each number of pictures that a shader reads the same
/// bindings (the numbers at 0, then a texture and a sampler for each picture: the kth at `1 + 2k` and `2 + 2k`).
pub struct ForeignEffectPass {
    /// The bindings for no picture, one, two... up to [`MOST_IMAGES_OF_A_SHADER`].
    layouts: Vec<LayoutForPictures>,
    vertex_stage: wgpu::ShaderModule,
    /// How the pictures an effect works on are read: smoothly between their pixels, and the pixel at the edge beyond them.
    pub sampler: wgpu::Sampler,
}

impl ForeignEffectPass {
    pub fn create(device: &wgpu::Device) -> Self {
        let layouts = (0..=MOST_IMAGES_OF_A_SHADER)
            .map(|images| {
                let mut entries = vec![wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // (This is what makes a shader that wants more bytes than a call gives it one the library refuses when it is given it,
                        // and not a mistake of every frame it is in.)
                        min_binding_size: std::num::NonZeroU64::new(FOREIGN_EFFECT_UNIFORM_BYTES),
                    },
                    count: None,
                }];
                for image in 0..images as u32 {
                    entries.push(wgpu::BindGroupLayoutEntry {
                        binding: 1 + 2 * image,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    });
                    entries.push(wgpu::BindGroupLayoutEntry {
                        binding: 2 + 2 * image,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    });
                }
                let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("engine foreign effect bindings"),
                    entries: &entries,
                });
                let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("engine foreign effect layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });
                LayoutForPictures {
                    bind_group_layout,
                    pipeline_layout,
                }
            })
            .collect();
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("engine foreign effect sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Self {
            vertex_stage: shader_module(device, "engine full screen triangle", shaders::FULL_SCREEN_TRIANGLE),
            layouts,
            sampler,
        }
    }

    /// The bindings of a pass of a shader that reads `images` pictures (at most [`MOST_IMAGES_OF_A_SHADER`]).
    pub fn bind_group_layout_for(&self, images: usize) -> &wgpu::BindGroupLayout {
        &self.layouts[images].bind_group_layout
    }

    /// The pipeline that runs `shader` (which has been through the guard), which reads `images` pictures, drawing into a texture of `format`. Call it where the errors of the
    /// graphics library are caught: a shader that the library will not take makes an error that is not in the return value.
    pub fn pipeline_for(
        &self,
        device: &wgpu::Device,
        key: &str,
        shader: &GuardedShader,
        images: usize,
        format: TargetFormat,
    ) -> wgpu::RenderPipeline {
        let fragment_stage = shader_module(device, &format!("effect {key}"), &shader.wgsl);
        pipeline_of_two_modules(
            device,
            &format!("effect {key}"),
            &self.vertex_stage,
            &fragment_stage,
            &self.layouts[images].pipeline_layout,
            texture_format_of(format),
            None,
        )
    }
}

/// Whether `shader` has what an effect has to have, in words if it has not.
pub fn check_that_it_is_an_effect(shader: &GuardedShader) -> Result<(), String> {
    if shader.has_entry_point(FOREIGN_EFFECT_ENTRY_POINT, naga::ShaderStage::Fragment) {
        Ok(())
    } else {
        Err(format!(
            "the shader of an effect has to have a fragment entry point named {FOREIGN_EFFECT_ENTRY_POINT} (this one has {})",
            if shader.entry_points.is_empty() {
                "no entry point at all".to_owned()
            } else {
                shader
                    .entry_points
                    .iter()
                    .map(|(name, stage)| format!("{name} ({stage:?})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ))
    }
}

/// Writes the numbers of a call as the shader reads them: one `f32` after another, zeros up to the most there can be, and then the engine's own: the size of the picture
/// the effect works on, in pixels, and two zeros.
pub fn write_the_numbers_of_a_call(writer: &mut SlotWriter, values: &[f32], picture_size_in_pixels: (u32, u32)) {
    for value in values {
        writer.f32(*value);
    }
    while writer.len() < MOST_NUMBERS_OF_A_FOREIGN_EFFECT * std::mem::size_of::<f32>() {
        writer.f32(0.0);
    }
    writer.f32(picture_size_in_pixels.0 as f32);
    writer.f32(picture_size_in_pixels.1 as f32);
    writer.f32(0.0);
    writer.f32(0.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::UniformBytes;

    // (Rewritten on purpose in phase 6: the room is the most numbers a call has, and then the four of the engine's: the size of the picture and two zeros. The numbers of a
    // call are where they were.)
    #[test]
    fn the_numbers_of_a_call_are_written_one_after_another_zeros_fill_the_room_and_the_engines_own_come_last() {
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| write_the_numbers_of_a_call(writer, &[1.5, -2.0, 3.25], (640, 360)));
        assert_eq!(u64::from(slot.size), FOREIGN_EFFECT_UNIFORM_BYTES);
        let numbers: Vec<f32> = bytes.as_bytes()[slot.offset as usize..(slot.offset + slot.size) as usize]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|four| f32::from_le_bytes(*four))
            .collect();
        assert_eq!(
            numbers.len(),
            MOST_NUMBERS_OF_A_FOREIGN_EFFECT + NUMBERS_THE_ENGINE_ADDS
        );
        assert_eq!(numbers[..3], [1.5, -2.0, 3.25]);
        assert!(
            numbers[3..MOST_NUMBERS_OF_A_FOREIGN_EFFECT]
                .iter()
                .all(|number| *number == 0.0)
        );
        assert_eq!(numbers[MOST_NUMBERS_OF_A_FOREIGN_EFFECT..], [640.0, 360.0, 0.0, 0.0]);
    }

    #[test]
    fn a_call_with_all_the_numbers_it_can_have_fills_the_room_exactly() {
        let values: Vec<f32> = (0..MOST_NUMBERS_OF_A_FOREIGN_EFFECT)
            .map(|number| number as f32)
            .collect();
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| write_the_numbers_of_a_call(writer, &values, (2, 3)));
        assert_eq!(u64::from(slot.size), FOREIGN_EFFECT_UNIFORM_BYTES);
        let written: Vec<f32> = bytes.as_bytes()[..slot.size as usize]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|four| f32::from_le_bytes(*four))
            .collect();
        assert_eq!(written[..MOST_NUMBERS_OF_A_FOREIGN_EFFECT], values);
        assert_eq!(written[MOST_NUMBERS_OF_A_FOREIGN_EFFECT..], [2.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn a_call_with_no_numbers_is_all_zeros_but_the_size_of_the_picture() {
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| write_the_numbers_of_a_call(writer, &[], (0, 0)));
        assert_eq!(u64::from(slot.size), FOREIGN_EFFECT_UNIFORM_BYTES);
        assert!(bytes.as_bytes().iter().all(|byte| *byte == 0));
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| write_the_numbers_of_a_call(writer, &[], (7, 9)));
        assert_eq!(
            slot.size as usize,
            4 * (MOST_NUMBERS_OF_A_FOREIGN_EFFECT + NUMBERS_THE_ENGINE_ADDS)
        );
        let (before, engines) = bytes.as_bytes()[..slot.size as usize].split_at(4 * MOST_NUMBERS_OF_A_FOREIGN_EFFECT);
        assert!(before.iter().all(|byte| *byte == 0));
        let engines: Vec<f32> = engines
            .as_chunks::<4>()
            .0
            .iter()
            .map(|four| f32::from_le_bytes(*four))
            .collect();
        assert_eq!(engines, [7.0, 9.0, 0.0, 0.0]);
    }

    #[test]
    fn the_number_of_the_pass_is_put_where_the_structure_says_and_nowhere_when_it_says_nowhere() {
        let mut structure = ShaderStructure::one_pass_of_one_picture();
        // A shader of phase 1 owns all its numbers.
        assert_eq!(
            numbers_of_a_pass(&[1.0, 2.0, 3.0, 4.0, 5.0], &structure, 2),
            vec![1.0, 2.0, 3.0, 4.0, 5.0]
        );
        structure.pass_index_number = Some(3);
        assert_eq!(
            numbers_of_a_pass(&[1.0, 2.0, 3.0, 4.0, 5.0], &structure, 2),
            vec![1.0, 2.0, 3.0, 2.0, 5.0]
        );
        assert_eq!(numbers_of_a_pass(&[1.0, 2.0, 3.0, 4.0, 5.0], &structure, 0)[3], 0.0);
        // A call that has fewer numbers is made as long as it has to be.
        assert_eq!(numbers_of_a_pass(&[], &structure, 5), vec![0.0, 0.0, 0.0, 5.0]);
        assert_eq!(numbers_of_a_pass(&[7.0, 8.0], &structure, 1), vec![7.0, 8.0, 0.0, 1.0]);
        structure.pass_index_number = Some(0);
        assert_eq!(numbers_of_a_pass(&[9.0, 8.0], &structure, 3), vec![3.0, 8.0]);
    }

    #[test]
    fn a_pass_draws_into_half_floats_or_into_eight_bits_as_the_structure_says() {
        assert_eq!(texture_format_of(TargetFormat::Half), ACCUMULATION_FORMAT);
        assert_eq!(
            texture_format_of(TargetFormat::EightBit),
            wgpu::TextureFormat::Rgba8Unorm
        );
        assert_eq!(EIGHT_BIT_BUFFER_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
    }

    fn shader_with(entry_points: Vec<(&str, naga::ShaderStage)>) -> GuardedShader {
        GuardedShader {
            wgsl: String::new(),
            loops_capped: 0,
            entry_points: entry_points
                .into_iter()
                .map(|(name, stage)| (name.to_owned(), stage))
                .collect(),
        }
    }

    #[test]
    fn a_shader_is_an_effect_if_it_has_a_fragment_entry_point_of_the_right_name_and_says_what_it_has_if_not() {
        assert!(
            check_that_it_is_an_effect(&shader_with(vec![
                ("other", naga::ShaderStage::Compute),
                ("fs_main", naga::ShaderStage::Fragment)
            ]))
            .is_ok()
        );
        let nothing = check_that_it_is_an_effect(&shader_with(vec![])).unwrap_err();
        assert!(
            nothing.contains("fs_main") && nothing.contains("no entry point at all"),
            "{nothing}"
        );
        // The right name for the wrong kind, and the right kind for another name, are not what is wanted.
        let wrong_kind =
            check_that_it_is_an_effect(&shader_with(vec![("fs_main", naga::ShaderStage::Vertex)])).unwrap_err();
        assert!(wrong_kind.contains("fs_main (Vertex)"), "{wrong_kind}");
        let wrong_name = check_that_it_is_an_effect(&shader_with(vec![
            ("main", naga::ShaderStage::Fragment),
            ("vs_main", naga::ShaderStage::Vertex),
        ]))
        .unwrap_err();
        assert!(wrong_name.contains("main (Fragment), vs_main (Vertex)"), "{wrong_name}");
    }
}
