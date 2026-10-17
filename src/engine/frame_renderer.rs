// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making one frame: everything of the frame is recorded into one batch of work for the card, the batch is submitted, and the pixels are taken back.
//! Pictures that are sent to the engine are turned into what the passes work on when they arrive, and are kept for the frames that follow.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

use thiserror::Error;

use super::colour::{DisplayDomainConversion, IngestConversion, MixingParts, OutputConversion};
use super::device::EngineDevice;
use super::effect_shaders::{check_that_it_is_an_effect, write_the_numbers_of_a_call};
use super::guard::GuardedShader;
use super::picture_cache::{DEFAULT_BUDGET_IN_BYTES, GpuPicture, PictureCache};
use super::pictures::{PictureData, PicturePixels};
use super::pipelines::{
    ACCUMULATION_FORMAT, CHROMA_FORMAT, ENCODED_FORMAT, LUMA_FORMAT, OUTPUT_FORMAT, PLANE_FORMAT, Pipelines,
    RGB_TO_YUV_UNIFORM_BYTES, SENT_PICTURE_FORMAT,
};
use super::placement::PlacementUniforms;
use super::plan::{EffectStep, EffectsThatRun, KindOfEffect, NUMBERS_OF_A_COLOUR_MATRIX, PassPlan, plan_frame};
use super::readback::{PendingReadback, ReadbackBuffers, ReadbackProblem};
use super::texture_pool::{PoolStatistics, TexturePool, TextureShape};
use super::uniform_arena::UniformArena;
use super::yuv::{ChromaReconstruction, YuvColour, chroma_size};
use super::{
    COLOUR_MATRIX_KEY, FrameDescription, FrameStatistics, LayerSource, PictureId, RenderedFrame, RenderedPlanes,
    UniformBytes, UniformSlot,
};

/// How many bytes of textures that are waiting to be used are kept from one frame to the next.
const IDLE_TEXTURE_BUDGET_IN_BYTES: u64 = 256 * 1024 * 1024;

/// How many read-back buffers are kept from one frame to the next. (The planes of the export take two for each frame, and the buffers of a frame that is on its way
/// are not waiting: the two that a frame finishing gives back are the two that the next frame begun takes.)
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
    /// Pictures that the frame uses were made for another working space than the one the frame is put together in: whoever sends the pictures and whoever describes the
    /// frame do not agree (they are made from the same setting, and a picture's id says which colours it was made with).
    #[error("{} of the pictures the frame uses were made for another working space than the frame is put together in", .0.len())]
    PicturesOfAnotherWorkingSpace(Vec<PictureId>),
    #[error("a picture that was sent cannot be used: {0}")]
    BadPicture(String),
    #[error("the shader of an effect cannot be used: {0}")]
    BadEffectShader(String),
}

/// What a frame is taken back as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOutput {
    /// The 8-bit frame that the preview shows: four bytes to the pixel.
    Rgba8,
    /// The planes of a `yuv420p` picture, made of the same finished frame the way the colour says (matrix and range): what the export hands to the encoder. The frame
    /// is read in half floats, as the card has it, and not as the 8 bits that the preview shows: the planes are rounded to 8 bits once.
    Planes(YuvColour),
}

/// A frame that has been submitted to the graphics card and not taken back yet. [`FrameRenderer::begin_frame`] makes one and [`FrameRenderer::finish_frame`]
/// waits for it, so that the next frame can be recorded while the card is still at work on this one.
pub struct PendingFrame {
    width_in_pixels: u32,
    height_in_pixels: u32,
    readbacks: PendingReadbacks,
    started: Instant,
    deadline: Instant,
    passes_recorded: usize,
    effects_not_run: usize,
}

enum PendingReadbacks {
    Rgba8(PendingReadback),
    /// The brightness plane, and the two colour differences side by side (blue, red, blue, red...).
    Planes {
        luma: PendingReadback,
        chroma: PendingReadback,
    },
}

/// A frame that has been taken back, as it was asked for.
#[derive(Debug)]
pub enum FinishedFrame {
    Rgba8(RenderedFrame),
    Planes(RenderedPlanes),
}

impl FinishedFrame {
    /// The frame, which was asked for as [`FrameOutput::Rgba8`].
    pub fn into_rgba8(self) -> RenderedFrame {
        match self {
            Self::Rgba8(frame) => frame,
            Self::Planes(_) => panic!("a frame that was asked for as planes is not an RGBA frame"),
        }
    }

    /// The planes, which were asked for as [`FrameOutput::Planes`].
    pub fn into_planes(self) -> RenderedPlanes {
        match self {
            Self::Planes(planes) => planes,
            Self::Rgba8(_) => panic!("a frame that was asked for as RGBA is not planes"),
        }
    }
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

    /// Lets go of these pictures (the ones that are not on the card are not missed). Their textures are kept to make the next pictures in.
    pub fn forget_pictures(&mut self, ids: impl IntoIterator<Item = PictureId>) {
        let forgotten = self.pictures.forget(ids);
        self.keep_the_textures_of(forgotten);
    }

    /// Gives the textures of pictures that were let go of to the pool, to be made into other pictures (a video that plays has a picture to send in every frame, and
    /// a texture of 16.6 megabytes at 1080p is not made again for each).
    fn keep_the_textures_of(&mut self, pictures: Vec<GpuPicture>) {
        for GpuPicture {
            texture,
            view,
            width_in_pixels,
            height_in_pixels,
            working: _,
        } in pictures
        {
            drop(view);
            self.pool
                .release(shape_of_a_picture(width_in_pixels, height_in_pixels), texture);
        }
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
        let converted_shape = shape_of_a_picture(width, height);
        let converted = self.pool.acquire(&self.device.device, converted_shape);
        let converted_view = converted.create_view(&wgpu::TextureViewDescriptor::default());
        // What the picture's numbers are turned into (nothing, in the compatibility mode), as the numbers of the pass that does it.
        let ingest = IngestConversion::of(data.colour, data.working);
        self.upload_uniforms.clear();
        let conversion_slot = match &data.pixels {
            PicturePixels::Rgba8 { .. } => self.upload_uniforms.add_slot(|writer| ingest.write(writer)),
            PicturePixels::Yuv420p8 { colour, .. } => {
                let chroma = ChromaReconstruction::of(data.working, data.chroma_siting);
                self.upload_uniforms
                    .add_slot(|writer| colour.conversion().write(writer, &ingest, chroma))
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
            let slot = conversion_slot;
            let buffer = self
                .upload_arena
                .upload(device, &self.device.queue, &self.upload_uniforms)
                .expect("a slot was written");
            let (pipeline, bind_group, dynamic_offsets) = match &data.pixels {
                PicturePixels::Yuv420p8 { .. } => {
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
                PicturePixels::Rgba8 { .. } => {
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("engine ingest pass"),
                        layout: &self.pipelines.ingest.bind_group_layout,
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
                        ],
                    });
                    (&self.pipelines.ingest.pipeline, bind_group, vec![slot.offset])
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
            drop(converted_view);
            self.pool.release(converted_shape, converted);
            return Err(RenderProblem::GraphicsMistake(mistake));
        }
        let let_go_of = self.pictures.insert(
            id,
            GpuPicture {
                texture: converted,
                view: converted_view,
                width_in_pixels: width,
                height_in_pixels: height,
                working: data.working,
            },
            width as usize * height as usize * BYTES_PER_PIXEL_ON_THE_CARD,
        );
        self.keep_the_textures_of(let_go_of);
        Ok(())
    }

    /// The frame that `description` describes, made within `timeout`, as the 8-bit RGBA frame that the preview shows.
    pub fn render(
        &mut self,
        description: &FrameDescription,
        timeout: Duration,
    ) -> Result<(RenderedFrame, FrameStatistics), RenderProblem> {
        let pending = self.begin_frame(description, FrameOutput::Rgba8, timeout)?;
        let (frame, statistics) = self.finish_frame(pending)?;
        Ok((frame.into_rgba8(), statistics))
    }

    /// The frame that `description` describes, made within `timeout`, as the planes of a `yuv420p` picture made of the finished frame (as the card has it, in half floats)
    /// the way `colour` says.
    pub fn render_planes(
        &mut self,
        description: &FrameDescription,
        colour: YuvColour,
        timeout: Duration,
    ) -> Result<(RenderedPlanes, FrameStatistics), RenderProblem> {
        let pending = self.begin_frame(description, FrameOutput::Planes(colour), timeout)?;
        let (planes, statistics) = self.finish_frame(pending)?;
        Ok((planes.into_planes(), statistics))
    }

    /// Records the frame that `description` describes, submits it to the card, and gives back what is needed to take its pixels when the card is done: the
    /// frame has `timeout` from now to be finished by [`Self::finish_frame`]. (Another frame can be begun before this one is finished.)
    pub fn begin_frame(
        &mut self,
        description: &FrameDescription,
        output_kind: FrameOutput,
        timeout: Duration,
    ) -> Result<PendingFrame, RenderProblem> {
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
        let made_for_another_working_space: Vec<PictureId> = pictures_used()
            .filter(|id| {
                self.pictures
                    .get(*id)
                    .is_some_and(|picture| picture.working != description.colour.working)
            })
            .collect();
        if !made_for_another_working_space.is_empty() {
            return Err(RenderProblem::PicturesOfAnotherWorkingSpace(
                made_for_another_working_space,
            ));
        }
        self.pictures.touch(pictures_used());

        let plan = plan_frame(description, &self.effects_that_run);
        let accumulation_shape = TextureShape {
            width_in_pixels: width,
            height_in_pixels: height,
            format: ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        };
        // The 8-bit frame is copied out for the preview; the export's passes that make the planes read the same numbers in half floats, as they are.
        let (output_format, output_usage) = match output_kind {
            FrameOutput::Rgba8 => (OUTPUT_FORMAT, wgpu::TextureUsages::COPY_SRC),
            FrameOutput::Planes(_) => (ENCODED_FORMAT, wgpu::TextureUsages::TEXTURE_BINDING),
        };
        let output_shape = TextureShape {
            width_in_pixels: width,
            height_in_pixels: height,
            format: output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | output_usage,
        };
        let (chroma_width, chroma_height) = chroma_size(width, height);
        let luma_shape = TextureShape {
            width_in_pixels: width,
            height_in_pixels: height,
            format: LUMA_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        };
        let chroma_shape = TextureShape {
            width_in_pixels: chroma_width,
            height_in_pixels: chroma_height,
            format: CHROMA_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        };
        let accumulation = self.pool.acquire(&self.device.device, accumulation_shape);
        let output = self.pool.acquire(&self.device.device, output_shape);
        let planes_textures = matches!(output_kind, FrameOutput::Planes(_)).then(|| {
            (
                self.pool.acquire(&self.device.device, luma_shape),
                self.pool.acquire(&self.device.device, chroma_shape),
            )
        });

        // What the effects of the layers make: a picture for each effect that runs, as big as the picture the effect is run on, from the pool and given back
        // after the frame. `effect_picture_numbers` says, for each layer that has effects, which of them are its own, in the order they are made in (the last
        // is the one the layer is put in its place from).
        let mut effect_pictures: Vec<(TextureShape, wgpu::Texture)> = Vec::new();
        let mut effect_picture_numbers: HashMap<usize, Vec<usize>> = HashMap::new();
        for (pass_number, pass) in plan.passes.iter().enumerate() {
            let PassPlan::Effects { layer, .. } = pass else {
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
            let numbers = plan.steps_of_effects[pass_number]
                .iter()
                .map(|_| {
                    effect_pictures.push((shape, self.pool.acquire(&self.device.device, shape)));
                    effect_pictures.len() - 1
                })
                .collect();
            effect_picture_numbers.insert(*layer, numbers);
        }
        self.uniforms.clear();

        let (readbacks, mistake) = self.device.catching_errors(|| {
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
            for (pass_number, pass) in plan.passes.iter().enumerate() {
                match pass {
                    PassPlan::Effects { layer, .. } => {
                        let effects = &description.layers[*layer].effects;
                        let domain = DisplayDomainConversion::of(description.colour.working);
                        effect_slots.push(
                            plan.steps_of_effects[pass_number]
                                .iter()
                                .map(|step| match *step {
                                    EffectStep::Call {
                                        call,
                                        converts_into_display,
                                        converts_out_of_display,
                                    } => {
                                        let effect = &effects[call];
                                        match self.effects_that_run.kind_of(&effect.key) {
                                            Some(KindOfEffect::Shader) => self.uniforms.add_slot(|writer| {
                                                write_the_numbers_of_a_call(writer, &effect.values);
                                            }),
                                            _ => {
                                                let (rows, offsets) = colour_matrix_numbers(&effect.values);
                                                let parts = MixingParts {
                                                    offsets,
                                                    converts_into_the_domain: converts_into_display,
                                                    converts_out_of_the_domain: converts_out_of_display,
                                                    mixes: true,
                                                };
                                                self.uniforms.add_slot(|writer| {
                                                    writer.mat3x3_from_rows(rows);
                                                    domain.write(writer);
                                                    parts.write(writer);
                                                })
                                            }
                                        }
                                    }
                                    // (A pass of the colour matrix that only converts: it mixes nothing and cuts nothing off.)
                                    EffectStep::Conversion { into_display } => {
                                        let parts = MixingParts {
                                            offsets: [0.0; 3],
                                            converts_into_the_domain: into_display,
                                            converts_out_of_the_domain: !into_display,
                                            mixes: false,
                                        };
                                        self.uniforms.add_slot(|writer| {
                                            writer.mat3x3_from_rows([
                                                [1.0, 0.0, 0.0],
                                                [0.0, 1.0, 0.0],
                                                [0.0, 0.0, 1.0],
                                            ]);
                                            domain.write(writer);
                                            parts.write(writer);
                                        })
                                    }
                                })
                                .collect(),
                        );
                    }
                    PassPlan::PlaceLayers { layers } => {
                        effect_slots.push(Vec::new());
                        for &number in layers {
                            let uniforms = PlacementUniforms::of_layer_in(
                                &description.layers[number],
                                (width, height),
                                description.colour.working,
                            );
                            slots.push((number, self.uniforms.add_slot(|writer| uniforms.write(writer))));
                        }
                    }
                    PassPlan::ClearAccumulation | PassPlan::Output => effect_slots.push(Vec::new()),
                }
            }
            // The numbers of the last pass (the display transform, or what the output is encoded as), and then, for the export, those that make its planes.
            let output_conversion = OutputConversion::of(description.colour);
            let output_slot = self.uniforms.add_slot(|writer| output_conversion.write(writer));
            let planes_slot = match output_kind {
                FrameOutput::Planes(colour) => Some(
                    self.uniforms
                        .add_slot(|writer| colour.forward_conversion().write(writer)),
                ),
                FrameOutput::Rgba8 => None,
            };
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
                    PassPlan::Effects { layer, .. } => {
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
                        for ((slot, &picture_number), step) in effect_slots[pass_number]
                            .iter()
                            .zip(&effect_picture_numbers[layer])
                            .zip(&plan.steps_of_effects[pass_number])
                        {
                            let kind = match *step {
                                EffectStep::Call { call, .. } => self
                                    .effects_that_run
                                    .kind_of(&description.layers[*layer].effects[call].key),
                                EffectStep::Conversion { .. } => Some(KindOfEffect::ColourMatrix),
                            };
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
                            let (pipeline, bind_group, label) = match kind {
                                Some(KindOfEffect::Shader) => (
                                    &self.shader_effects[&description.layers[*layer].effects[step_call(step)].key],
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
                        let uniform_buffer = uniform_buffer.expect("a frame with an output has uniforms");
                        let bind_group = self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("engine output pass"),
                            layout: &self.pipelines.output.bind_group_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                        buffer: uniform_buffer,
                                        offset: 0,
                                        size: NonZeroU64::new(u64::from(output_slot.size)),
                                    }),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::TextureView(&accumulation_view),
                                },
                            ],
                        });
                        let (label, pipeline) = match output_kind {
                            FrameOutput::Rgba8 => ("engine: the 8-bit frame", &self.pipelines.output.pipeline),
                            FrameOutput::Planes(_) => (
                                "engine: the frame, encoded, in half floats",
                                &self.pipelines.output.encoded_pipeline,
                            ),
                        };
                        let mut render_pass = begin_pass(
                            &mut encoder,
                            label,
                            &output_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        );
                        render_pass.set_pipeline(pipeline);
                        render_pass.set_bind_group(0, &bind_group, &[output_slot.offset]);
                        render_pass.draw(0..3, 0..1);
                    }
                }
            }
            match (&planes_textures, planes_slot) {
                (Some((luma, chroma)), Some(slot)) => {
                    let uniform_buffer = uniform_buffer.expect("a frame with planes has uniforms");
                    let bind_group = self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("engine planes pass"),
                        layout: &self.pipelines.planes.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: uniform_buffer,
                                    offset: 0,
                                    size: NonZeroU64::new(RGB_TO_YUV_UNIFORM_BYTES),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&output_view),
                            },
                        ],
                    });
                    for (label, texture, pipeline) in [
                        (
                            "engine: the brightness plane",
                            luma,
                            &self.pipelines.planes.luma_pipeline,
                        ),
                        (
                            "engine: the colour difference planes",
                            chroma,
                            &self.pipelines.planes.chroma_pipeline,
                        ),
                    ] {
                        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                        let mut render_pass =
                            begin_pass(&mut encoder, label, &view, wgpu::LoadOp::Clear(wgpu::Color::BLACK));
                        render_pass.set_pipeline(pipeline);
                        render_pass.set_bind_group(0, &bind_group, &[slot.offset]);
                        render_pass.draw(0..3, 0..1);
                    }
                    let readbacks = PendingReadbacks::Planes {
                        luma: self.readback.record_copy(&self.device.device, &mut encoder, luma),
                        chroma: self.readback.record_copy(&self.device.device, &mut encoder, chroma),
                    };
                    self.device.queue.submit([encoder.finish()]);
                    readbacks
                }
                _ => {
                    let readbacks =
                        PendingReadbacks::Rgba8(self.readback.record_copy(&self.device.device, &mut encoder, &output));
                    self.device.queue.submit([encoder.finish()]);
                    readbacks
                }
            }
        });
        self.pool.release(accumulation_shape, accumulation);
        self.pool.release(output_shape, output);
        if let Some((luma, chroma)) = planes_textures {
            self.pool.release(luma_shape, luma);
            self.pool.release(chroma_shape, chroma);
        }
        for (shape, texture) in effect_pictures {
            self.pool.release(shape, texture);
        }
        if let Some(mistake) = mistake {
            return Err(RenderProblem::GraphicsMistake(mistake));
        }
        let planes_passes = if matches!(output_kind, FrameOutput::Planes(_)) {
            2
        } else {
            0
        };
        Ok(PendingFrame {
            width_in_pixels: width,
            height_in_pixels: height,
            readbacks,
            started,
            deadline,
            passes_recorded: plan.passes_recorded() + planes_passes,
            effects_not_run: plan.effects_not_run,
        })
    }

    /// Waits for the card to finish `pending` (until the time it was begun with `timeout` after) and takes its pixels.
    pub fn finish_frame(&mut self, pending: PendingFrame) -> Result<(FinishedFrame, FrameStatistics), RenderProblem> {
        #[cfg(test)]
        if self.the_card_never_finishes {
            return Err(RenderProblem::TimedOut);
        }
        let PendingFrame {
            width_in_pixels: width,
            height_in_pixels: height,
            readbacks,
            started,
            deadline,
            passes_recorded,
            effects_not_run,
        } = pending;
        let waiting_since = Instant::now();
        let mut collect = |pending: PendingReadback| {
            self.readback
                .collect(&self.device.device, pending, deadline)
                .map_err(|problem| match problem {
                    ReadbackProblem::TimedOut => RenderProblem::TimedOut,
                    ReadbackProblem::Failed(message) => RenderProblem::ReadbackFailed(message),
                })
        };
        let finished = match readbacks {
            PendingReadbacks::Rgba8(readback) => FinishedFrame::Rgba8(RenderedFrame {
                width_in_pixels: width,
                height_in_pixels: height,
                rgba_pixels: collect(readback)?,
            }),
            PendingReadbacks::Planes { luma, chroma } => {
                let luma = collect(luma)?;
                let chroma = collect(chroma)?;
                FinishedFrame::Planes(RenderedPlanes {
                    width_in_pixels: width,
                    height_in_pixels: height,
                    planes: planes_from_the_textures(width, height, luma, &chroma),
                })
            }
        };
        let waited = waiting_since.elapsed();
        self.pool.end_frame(IDLE_TEXTURE_BUDGET_IN_BYTES);
        self.readback.end_frame(READBACK_BUFFERS_KEPT);
        Ok((
            finished,
            FrameStatistics {
                time_taken: started.elapsed(),
                time_waiting_for_the_graphics_card: waited,
                passes_recorded,
                came_from_the_cache: false,
                effects_not_run,
            },
        ))
    }
}

/// The planes of a `yuv420p` picture from what the card gave: the brightness plane as it is, and the colour differences, which came side by side (blue, red, blue,
/// red...), taken apart and put after it.
fn planes_from_the_textures(width: u32, height: u32, brightness: Vec<u8>, side_by_side: &[u8]) -> Vec<u8> {
    let (chroma_width, chroma_height) = chroma_size(width, height);
    let colour_difference_bytes = chroma_width as usize * chroma_height as usize;
    debug_assert_eq!(side_by_side.len(), 2 * colour_difference_bytes);
    let mut planes = brightness;
    planes.reserve(2 * colour_difference_bytes);
    planes.extend(side_by_side.iter().step_by(2));
    planes.extend(side_by_side.iter().skip(1).step_by(2));
    planes
}

/// The three rows of the matrix of an effect that mixes colours, from its nine numbers (the plan only lets one through that has nine).
/// The rows and the offsets that the nine or twelve numbers of a call of the colour matrix are (the offsets of one with nine are nothing).
fn colour_matrix_numbers(values: &[f32]) -> ([[f32; 3]; 3], [f32; 3]) {
    debug_assert!(NUMBERS_OF_A_COLOUR_MATRIX.contains(&values.len()));
    let rows = [
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ];
    let offsets = match values {
        [.., red, green, blue] if values.len() == 12 => [*red, *green, *blue],
        _ => [0.0; 3],
    };
    (rows, offsets)
}

/// The number of the effect that a step runs. (Only asked of the steps that call a shader.)
fn step_call(step: &EffectStep) -> usize {
    match step {
        EffectStep::Call { call, .. } => *call,
        EffectStep::Conversion { .. } => unreachable!("a conversion is not run by a shader"),
    }
}

/// The shape of the texture that a picture is kept in on the card: premultiplied, in half floats, to be read by the passes of a frame and drawn into by the pass
/// that makes it (and by the effects, which make pictures of the same shape).
fn shape_of_a_picture(width: u32, height: u32) -> TextureShape {
    TextureShape {
        width_in_pixels: width,
        height_in_pixels: height,
        format: ACCUMULATION_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    }
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
