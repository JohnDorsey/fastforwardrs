// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making one frame: everything of the frame is recorded into one batch of work for the card, the batch is submitted, and the pixels are taken back.
//! Pictures that are sent to the engine are turned into what the passes work on when they arrive, and are kept for the frames that follow.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

use thiserror::Error;

use super::device::EngineDevice;
use super::effect_shaders::{check_that_it_is_an_effect, write_the_numbers_of_a_call};
use super::guard::GuardedShader;
use super::picture_cache::{DEFAULT_BUDGET_IN_BYTES, GpuPicture, PictureCache};
use super::pictures::{PictureData, PicturePixels};
use super::pipelines::{ACCUMULATION_FORMAT, OUTPUT_FORMAT, PLANE_FORMAT, Pipelines, SENT_PICTURE_FORMAT};
use super::placement::PlacementUniforms;
use super::plan::{EffectsThatRun, KindOfEffect, PassPlan, plan_frame};
use super::readback::{ReadbackBuffers, ReadbackProblem};
use super::texture_pool::{PoolStatistics, TexturePool, TextureShape};
use super::uniform_arena::UniformArena;
use super::yuv::chroma_size;
use super::{
    COLOUR_MATRIX_KEY, FrameDescription, FrameStatistics, LayerSource, PictureId, RenderedFrame, UniformBytes,
    UniformSlot,
};

/// How many bytes of textures that are waiting to be used are kept from one frame to the next.
const IDLE_TEXTURE_BUDGET_IN_BYTES: u64 = 256 * 1024 * 1024;

/// How many read-back buffers are kept from one frame to the next.
const READBACK_BUFFERS_KEPT: usize = 2;

/// How many bytes a pixel of a picture takes up on the card once it is half-float.
const BYTES_PER_PIXEL_ON_THE_CARD: usize = 8;

/// Why a frame could not be made, or a picture could not be taken.
#[derive(Debug, Error)]
pub enum RenderProblem {
    #[error(
        "a frame has to be at least one pixel across and down, and at most {largest} (this one is {width} by {height})"
    )]
    BadSize { width: u32, height: u32, largest: u32 },
    #[error("the graphics card, or the library, found a mistake in what it was asked to do: {0}")]
    GraphicsMistake(String),
    #[error("the graphics card did not finish the frame in time")]
    TimedOut,
    #[error("the pixels of the frame could not be got back from the graphics card: {0}")]
    ReadbackFailed(String),
    /// Pictures that the frame uses are not on the card (they were never sent, or were let go of to make room, or the device was replaced).
    #[error("{} of the pictures the frame uses are not on the graphics card", .0.len())]
    PicturesMissing(Vec<PictureId>),
    #[error("a picture that was sent cannot be used: {0}")]
    BadPicture(String),
    #[error("the shader of an effect cannot be used: {0}")]
    BadEffectShader(String),
}

/// Makes frames on a device. See the module documentation.
pub struct FrameRenderer {
    device: EngineDevice,
    pool: TexturePool,
    readback: ReadbackBuffers,
    pipelines: Pipelines,
    pictures: PictureCache,
    uniforms: UniformBytes,
    arena: UniformArena,
    /// The numbers that making a video picture red, green and blue is given (the frames have their own, in `uniforms`).
    upload_uniforms: UniformBytes,
    upload_arena: UniformArena,
    /// A picture of one white pixel, which a layer that is a colour is made of.
    white_pixel: wgpu::TextureView,
    /// The effects that were given to the engine as shaders, by their keys.
    shader_effects: HashMap<String, wgpu::RenderPipeline>,
    /// Which effects the engine can run: its own, and the keys of `shader_effects`.
    effects_that_run: EffectsThatRun,
    /// For the tests of what happens when a frame does not finish (which no card can be made to do on purpose, in no time, without spending the processor for
    /// as long as the program runs): the frames that are made say they did not finish.
    #[cfg(test)]
    the_card_never_finishes: bool,
}

impl FrameRenderer {
    pub fn new(device: EngineDevice) -> Self {
        Self::with_picture_budget(device, DEFAULT_BUDGET_IN_BYTES)
    }

    /// A renderer that keeps at most `picture_budget_in_bytes` of pictures on the card.
    pub fn with_picture_budget(device: EngineDevice, picture_budget_in_bytes: usize) -> Self {
        let pipelines = Pipelines::create(&device.device);
        let white = device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("engine: one white pixel"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SENT_PICTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        device.queue.write_texture(
            white.as_image_copy(),
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let alignment = device.uniform_alignment;
        Self {
            uniforms: UniformBytes::new(alignment),
            white_pixel: white.create_view(&wgpu::TextureViewDescriptor::default()),
            device,
            pool: TexturePool::default(),
            readback: ReadbackBuffers::default(),
            pipelines,
            pictures: PictureCache::new(picture_budget_in_bytes),
            arena: UniformArena::default(),
            upload_uniforms: UniformBytes::new(alignment),
            upload_arena: UniformArena::default(),
            shader_effects: HashMap::new(),
            effects_that_run: EffectsThatRun::built_in_only(),
            #[cfg(test)]
            the_card_never_finishes: false,
        }
    }

    /// From now on a frame is made and then said not to have finished in time, as a frame is that the card does not finish.
    #[cfg(test)]
    pub fn pretend_the_card_never_finishes(&mut self) {
        self.the_card_never_finishes = true;
    }

    /// Makes the effect under `key` an effect that frames can ask for, run by `shader`, which has been through the guard: its pipeline is made now, so that
    /// a shader the graphics library will not take is found out now and not in the middle of a frame. An effect that was under `key` is replaced.
    pub fn install_effect(&mut self, key: &str, shader: &GuardedShader) -> Result<(), RenderProblem> {
        if key == COLOUR_MATRIX_KEY || key.starts_with("engine:") {
            return Err(RenderProblem::BadEffectShader(format!(
                "the key {key} belongs to the engine's own effects"
            )));
        }
        check_that_it_is_an_effect(shader).map_err(RenderProblem::BadEffectShader)?;
        let (pipeline, mistake) = self.device.catching_errors(|| {
            self.pipelines
                .foreign_effect
                .pipeline_for(&self.device.device, key, shader)
        });
        if let Some(mistake) = mistake {
            return Err(RenderProblem::BadEffectShader(format!(
                "the graphics library would not take it: {mistake}"
            )));
        }
        self.shader_effects.insert(key.to_owned(), pipeline);
        self.effects_that_run.add_shader(key);
        Ok(())
    }

    /// The keys of the effects of `description` that are run by a shader that was given to the engine, once each, in order: what a frame that did not
    /// finish had in it that the engine did not write.
    pub fn shader_effects_in(&self, description: &FrameDescription) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for pass in &plan_frame(description, &self.effects_that_run).passes {
            if let PassPlan::Effects { layer, calls } = pass {
                for &call in calls {
                    let key = &description.layers[*layer].effects[call].key;
                    if self.effects_that_run.kind_of(key) == Some(KindOfEffect::Shader) && !keys.contains(key) {
                        keys.push(key.clone());
                    }
                }
            }
        }
        keys
    }

    pub fn device(&self) -> &EngineDevice {
        &self.device
    }

    pub fn pool_statistics(&self) -> PoolStatistics {
        self.pool.statistics()
    }

    /// How many read-back buffers are kept for the next frame.
    #[cfg(test)]
    pub fn readback_buffers_waiting(&self) -> usize {
        self.readback.idle_count()
    }

    /// How many pictures are on the card, and how many bytes they take up.
    pub fn pictures_on_the_card(&self) -> (usize, usize) {
        (self.pictures.len(), self.pictures.byte_count())
    }

    /// How big the picture `id` is, if it is on the card.
    pub fn picture_size(&self, id: PictureId) -> Option<(u32, u32)> {
        self.pictures
            .get(id)
            .map(|picture| (picture.width_in_pixels, picture.height_in_pixels))
    }

    /// Lets go of these pictures (the ones that are not on the card are not missed).
    pub fn forget_pictures(&mut self, ids: impl IntoIterator<Item = PictureId>) {
        self.pictures.forget(ids);
    }

    /// Takes a picture that was sent: makes it what the passes of a frame work on, and keeps it, under `id`, instead of what was kept under that id.
    pub fn upload_picture(&mut self, id: PictureId, data: &PictureData) -> Result<(), RenderProblem> {
        data.check()
            .map_err(|problem| RenderProblem::BadPicture(problem.to_string()))?;
        let (width, height) = (data.width_in_pixels, data.height_in_pixels);
        let largest = self.device.largest_texture_side;
        if width > largest || height > largest {
            return Err(RenderProblem::BadPicture(format!(
                "a picture {width} by {height} pixels is bigger than the graphics card takes ({largest})"
            )));
        }

        // The bytes go to textures first: one for the pixels of a picture of RGBA bytes, one for each plane of a video picture.
        let planes = planes_of(data);
        let staged: Vec<wgpu::Texture> = planes
            .iter()
            .map(|plane| self.pool.acquire(&self.device.device, plane.shape))
            .collect();
        let converted = self.device.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("engine: a picture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let converted_view = converted.create_view(&wgpu::TextureViewDescriptor::default());
        let conversion_slot = match &data.pixels {
            PicturePixels::Rgba8 { .. } => None,
            PicturePixels::Yuv420p8 { colour, .. } => {
                self.upload_uniforms.clear();
                Some(
                    self.upload_uniforms
                        .add_slot(|writer| colour.conversion().write(writer)),
                )
            }
        };

        let ((), mistake) = self.device.catching_errors(|| {
            for (plane, texture) in planes.iter().zip(&staged) {
                self.device.queue.write_texture(
                    texture.as_image_copy(),
                    plane.bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(plane.bytes_per_pixel * plane.shape.width_in_pixels),
                        rows_per_image: Some(plane.shape.height_in_pixels),
                    },
                    wgpu::Extent3d {
                        width: plane.shape.width_in_pixels,
                        height: plane.shape.height_in_pixels,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let views: Vec<wgpu::TextureView> = staged
                .iter()
                .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
                .collect();
            let device = &self.device.device;
            let (pipeline, bind_group, dynamic_offsets) = match (&data.pixels, conversion_slot) {
                (PicturePixels::Yuv420p8 { .. }, Some(slot)) => {
                    let buffer = self
                        .upload_arena
                        .upload(device, &self.device.queue, &self.upload_uniforms)
                        .expect("a slot was written");
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("engine yuv pass"),
                        layout: &self.pipelines.yuv.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer,
                                    offset: 0,
                                    size: NonZeroU64::new(u64::from(slot.size)),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&views[0]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&views[1]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&views[2]),
                            },
                        ],
                    });
                    (&self.pipelines.yuv.pipeline, bind_group, vec![slot.offset])
                }
                _ => {
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("engine ingest pass"),
                        layout: &self.pipelines.ingest.bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&views[0]),
                        }],
                    });
                    (&self.pipelines.ingest.pipeline, bind_group, Vec::new())
                }
            };
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("engine: take a picture"),
            });
            {
                let mut render_pass = begin_pass(
                    &mut encoder,
                    "engine: make a picture what the frames work on",
                    &converted_view,
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                );
                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(0, &bind_group, &dynamic_offsets);
                render_pass.draw(0..3, 0..1);
            }
            self.device.queue.submit([encoder.finish()]);
        });
        for (plane, texture) in planes.iter().zip(staged) {
            self.pool.release(plane.shape, texture);
        }
        if let Some(mistake) = mistake {
            return Err(RenderProblem::GraphicsMistake(mistake));
        }
        self.pictures.insert(
            id,
            GpuPicture {
                _texture: converted,
                view: converted_view,
                width_in_pixels: width,
                height_in_pixels: height,
            },
            width as usize * height as usize * BYTES_PER_PIXEL_ON_THE_CARD,
        );
        Ok(())
    }

    /// The frame that `description` describes, made within `timeout`.
    pub fn render(
        &mut self,
        description: &FrameDescription,
        timeout: Duration,
    ) -> Result<(RenderedFrame, FrameStatistics), RenderProblem> {
        let started = Instant::now();
        let deadline = started + timeout;
        let (width, height) = description.render_size;
        let largest = self.device.largest_texture_side;
        if width == 0 || height == 0 || width > largest || height > largest {
            return Err(RenderProblem::BadSize { width, height, largest });
        }
        let pictures_used = || {
            description.layers.iter().filter_map(|layer| match layer.source {
                LayerSource::Picture(id) => Some(id),
                LayerSource::Solid(_) => None,
            })
        };
        let missing = self.pictures.missing(pictures_used());
        if !missing.is_empty() {
            return Err(RenderProblem::PicturesMissing(missing));
        }
        self.pictures.touch(pictures_used());

        let plan = plan_frame(description, &self.effects_that_run);
        let accumulation_shape = TextureShape {
            width_in_pixels: width,
            height_in_pixels: height,
            format: ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        };
        let output_shape = TextureShape {
            width_in_pixels: width,
            height_in_pixels: height,
            format: OUTPUT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        };
        let accumulation = self.pool.acquire(&self.device.device, accumulation_shape);
        let output = self.pool.acquire(&self.device.device, output_shape);

        // What the effects of the layers make: a picture for each effect that runs, as big as the picture the effect is run on, from the pool and given back
        // after the frame. `effect_picture_numbers` says, for each layer that has effects, which of them are its own, in the order they are made in (the last
        // is the one the layer is put in its place from).
        let mut effect_pictures: Vec<(TextureShape, wgpu::Texture)> = Vec::new();
        let mut effect_picture_numbers: HashMap<usize, Vec<usize>> = HashMap::new();
        for pass in &plan.passes {
            let PassPlan::Effects { layer, calls } = pass else {
                continue;
            };
            let LayerSource::Picture(id) = description.layers[*layer].source else {
                continue;
            };
            let picture = self
                .pictures
                .get(id)
                .expect("the pictures of the frame are on the card");
            let shape = TextureShape {
                width_in_pixels: picture.width_in_pixels,
                height_in_pixels: picture.height_in_pixels,
                format: ACCUMULATION_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            };
            let numbers = calls
                .iter()
                .map(|_| {
                    effect_pictures.push((shape, self.pool.acquire(&self.device.device, shape)));
                    effect_pictures.len() - 1
                })
                .collect();
            effect_picture_numbers.insert(*layer, numbers);
        }
        self.uniforms.clear();

        let (pending, mistake) = self.device.catching_errors(|| {
            let accumulation_view = accumulation.create_view(&wgpu::TextureViewDescriptor::default());
            let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
            let effect_views: Vec<wgpu::TextureView> = effect_pictures
                .iter()
                .map(|(_, texture)| texture.create_view(&wgpu::TextureViewDescriptor::default()))
                .collect();

            // The uniforms of every pass of the frame that has some, written to the card before anything is recorded that uses them. For each pass of
            // the plan, in its order: the slots of the effects it runs (none for the others); and, for the layers that are put in their places, each one's.
            let mut effect_slots: Vec<Vec<UniformSlot>> = Vec::with_capacity(plan.passes.len());
            let mut slots = Vec::new();
            for pass in &plan.passes {
                match pass {
                    PassPlan::Effects { layer, calls } => {
                        let effects = &description.layers[*layer].effects;
                        effect_slots.push(
                            calls
                                .iter()
                                .map(|&call| {
                                    let effect = &effects[call];
                                    match self.effects_that_run.kind_of(&effect.key) {
                                        Some(KindOfEffect::Shader) => self.uniforms.add_slot(|writer| {
                                            write_the_numbers_of_a_call(writer, &effect.values);
                                        }),
                                        _ => {
                                            let rows = colour_matrix_rows(&effect.values);
                                            self.uniforms.add_slot(|writer| {
                                                writer.mat3x3_from_rows(rows);
                                            })
                                        }
                                    }
                                })
                                .collect(),
                        );
                    }
                    PassPlan::PlaceLayers { layers } => {
                        effect_slots.push(Vec::new());
                        for &number in layers {
                            let uniforms = PlacementUniforms::of_layer(&description.layers[number], (width, height));
                            slots.push((number, self.uniforms.add_slot(|writer| uniforms.write(writer))));
                        }
                    }
                    PassPlan::ClearAccumulation | PassPlan::Output => effect_slots.push(Vec::new()),
                }
            }
            let uniform_buffer = self
                .arena
                .upload(&self.device.device, &self.device.queue, &self.uniforms);

            let mut encoder = self
                .device
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("engine frame"),
                });
            for (pass_number, pass) in plan.passes.iter().enumerate() {
                match pass {
                    PassPlan::ClearAccumulation => {
                        drop(begin_pass(
                            &mut encoder,
                            "engine: clear the picture to black",
                            &accumulation_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        ));
                    }
                    PassPlan::Effects { layer, calls } => {
                        let uniform_buffer = uniform_buffer.expect("a frame with effects has uniforms");
                        let LayerSource::Picture(id) = description.layers[*layer].source else {
                            continue;
                        };
                        // Each effect works on the picture the one before it made (the first, on the layer's own).
                        let mut input_view = &self
                            .pictures
                            .get(id)
                            .expect("the pictures of the frame are on the card")
                            .view;
                        for ((slot, &picture_number), &call) in effect_slots[pass_number]
                            .iter()
                            .zip(&effect_picture_numbers[layer])
                            .zip(calls)
                        {
                            let effect = &description.layers[*layer].effects[call];
                            let uniform_binding = wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: uniform_buffer,
                                    offset: 0,
                                    size: NonZeroU64::new(u64::from(slot.size)),
                                }),
                            };
                            let picture_binding = wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(input_view),
                            };
                            // (The effect that mixes colours reads its picture with no sampler; one that a shader runs also has the sampler of the effects.)
                            let (pipeline, bind_group, label) = match self.effects_that_run.kind_of(&effect.key) {
                                Some(KindOfEffect::Shader) => (
                                    &self.shader_effects[&effect.key],
                                    self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                        label: Some("engine effect pass"),
                                        layout: &self.pipelines.foreign_effect.bind_group_layout,
                                        entries: &[
                                            uniform_binding,
                                            picture_binding,
                                            wgpu::BindGroupEntry {
                                                binding: 2,
                                                resource: wgpu::BindingResource::Sampler(
                                                    &self.pipelines.foreign_effect.sampler,
                                                ),
                                            },
                                        ],
                                    }),
                                    "engine: an effect that was given to the engine as a shader",
                                ),
                                _ => (
                                    &self.pipelines.colour_matrix.pipeline,
                                    self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                        label: Some("engine colour matrix pass"),
                                        layout: &self.pipelines.colour_matrix.bind_group_layout,
                                        entries: &[uniform_binding, picture_binding],
                                    }),
                                    "engine: mix the colours of a layer's picture",
                                ),
                            };
                            let mut render_pass = begin_pass(
                                &mut encoder,
                                label,
                                &effect_views[picture_number],
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            );
                            render_pass.set_pipeline(pipeline);
                            render_pass.set_bind_group(0, &bind_group, &[slot.offset]);
                            render_pass.draw(0..3, 0..1);
                            drop(render_pass);
                            input_view = &effect_views[picture_number];
                        }
                    }
                    PassPlan::PlaceLayers { .. } => {
                        let uniform_buffer = uniform_buffer.expect("a frame with layers has uniforms");
                        let mut bind_groups = Vec::with_capacity(slots.len());
                        for &(number, slot) in &slots {
                            let picture_view = match description.layers[number].source {
                                LayerSource::Picture(id) => {
                                    match effect_picture_numbers.get(&number).and_then(|numbers| numbers.last()) {
                                        Some(&last) => &effect_views[last],
                                        None => {
                                            &self
                                                .pictures
                                                .get(id)
                                                .expect("the pictures of the frame are on the card")
                                                .view
                                        }
                                    }
                                }
                                LayerSource::Solid(_) => &self.white_pixel,
                            };
                            bind_groups.push(self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("engine place pass"),
                                layout: &self.pipelines.place.bind_group_layout,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                            buffer: uniform_buffer,
                                            offset: 0,
                                            size: NonZeroU64::new(u64::from(slot.size)),
                                        }),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(picture_view),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(&self.pipelines.place.sampler),
                                    },
                                ],
                            }));
                        }
                        let mut render_pass = begin_pass(
                            &mut encoder,
                            "engine: put the layers in their places",
                            &accumulation_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        );
                        render_pass.set_pipeline(&self.pipelines.place.pipeline);
                        for (&(_, slot), bind_group) in slots.iter().zip(&bind_groups) {
                            render_pass.set_bind_group(0, bind_group, &[slot.offset]);
                            render_pass.draw(0..6, 0..1);
                        }
                    }
                    PassPlan::Output => {
                        let bind_group = self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("engine output pass"),
                            layout: &self.pipelines.output.bind_group_layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&accumulation_view),
                            }],
                        });
                        let mut render_pass = begin_pass(
                            &mut encoder,
                            "engine: the 8-bit frame",
                            &output_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        );
                        render_pass.set_pipeline(&self.pipelines.output.pipeline);
                        render_pass.set_bind_group(0, &bind_group, &[]);
                        render_pass.draw(0..3, 0..1);
                    }
                }
            }
            let pending = self.readback.record_copy(&self.device.device, &mut encoder, &output);
            self.device.queue.submit([encoder.finish()]);
            pending
        });
        self.pool.release(accumulation_shape, accumulation);
        self.pool.release(output_shape, output);
        for (shape, texture) in effect_pictures {
            self.pool.release(shape, texture);
        }
        if let Some(mistake) = mistake {
            return Err(RenderProblem::GraphicsMistake(mistake));
        }

        #[cfg(test)]
        if self.the_card_never_finishes {
            return Err(RenderProblem::TimedOut);
        }
        let waiting_since = Instant::now();
        let rgba_pixels =
            self.readback
                .collect(&self.device.device, pending, deadline)
                .map_err(|problem| match problem {
                    ReadbackProblem::TimedOut => RenderProblem::TimedOut,
                    ReadbackProblem::Failed(message) => RenderProblem::ReadbackFailed(message),
                })?;
        let waited = waiting_since.elapsed();
        self.pool.end_frame(IDLE_TEXTURE_BUDGET_IN_BYTES);
        self.readback.end_frame(READBACK_BUFFERS_KEPT);
        Ok((
            RenderedFrame {
                width_in_pixels: width,
                height_in_pixels: height,
                rgba_pixels,
            },
            FrameStatistics {
                time_taken: started.elapsed(),
                time_waiting_for_the_graphics_card: waited,
                passes_recorded: plan.passes_recorded(),
                came_from_the_cache: false,
                effects_not_run: plan.effects_not_run,
            },
        ))
    }
}

/// The three rows of the matrix of an effect that mixes colours, from its nine numbers (the plan only lets one through that has nine).
fn colour_matrix_rows(values: &[f32]) -> [[f32; 3]; 3] {
    debug_assert_eq!(values.len(), 9);
    [
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ]
}

/// One of the textures that the bytes of a picture are put in before the picture is made what the frames work on.
struct StagingPlane<'bytes> {
    shape: TextureShape,
    bytes: &'bytes [u8],
    bytes_per_pixel: u32,
}

/// The textures that the bytes of `data` go in: the pixels of a picture of RGBA bytes, or the three planes of a video picture.
fn planes_of(data: &PictureData) -> Vec<StagingPlane<'_>> {
    let (width, height) = (data.width_in_pixels, data.height_in_pixels);
    let shape = |width_in_pixels, height_in_pixels, format| TextureShape {
        width_in_pixels,
        height_in_pixels,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    };
    match &data.pixels {
        PicturePixels::Rgba8 { straight_alpha } => vec![StagingPlane {
            shape: shape(width, height, SENT_PICTURE_FORMAT),
            bytes: straight_alpha,
            bytes_per_pixel: 4,
        }],
        PicturePixels::Yuv420p8 { y, u, v, .. } => {
            let (chroma_width, chroma_height) = chroma_size(width, height);
            vec![
                StagingPlane {
                    shape: shape(width, height, PLANE_FORMAT),
                    bytes: y,
                    bytes_per_pixel: 1,
                },
                StagingPlane {
                    shape: shape(chroma_width, chroma_height, PLANE_FORMAT),
                    bytes: u,
                    bytes_per_pixel: 1,
                },
                StagingPlane {
                    shape: shape(chroma_width, chroma_height, PLANE_FORMAT),
                    bytes: v,
                    bytes_per_pixel: 1,
                },
            ]
        }
    }
}

/// A render pass that draws into `view`, beginning by `load`ing it as that says.
fn begin_pass<'encoder>(
    encoder: &'encoder mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'encoder> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
