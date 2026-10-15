// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making one frame: everything of the frame is recorded into one batch of work for the card, the batch is submitted, and the pixels are taken back.
//! Pictures that are sent to the engine are turned into what the passes work on when they arrive, and are kept for the frames that follow.

use std::num::NonZeroU64;
use std::time::{Duration, Instant};

use thiserror::Error;

use super::device::EngineDevice;
use super::picture_cache::{DEFAULT_BUDGET_IN_BYTES, GpuPicture, PictureCache};
use super::pictures::{PictureData, PicturePixels};
use super::pipelines::{ACCUMULATION_FORMAT, OUTPUT_FORMAT, Pipelines, SENT_PICTURE_FORMAT};
use super::placement::PlacementUniforms;
use super::plan::{PassPlan, plan_frame};
use super::readback::{ReadbackBuffers, ReadbackProblem};
use super::texture_pool::{PoolStatistics, TexturePool, TextureShape};
use super::uniform_arena::UniformArena;
use super::{FrameDescription, FrameStatistics, LayerSource, PictureId, RenderedFrame, UniformBytes};

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
    /// A picture of one white pixel, which a layer that is a colour is made of.
    white_pixel: wgpu::TextureView,
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
        Self {
            uniforms: UniformBytes::new(device.uniform_alignment),
            white_pixel: white.create_view(&wgpu::TextureViewDescriptor::default()),
            device,
            pool: TexturePool::default(),
            readback: ReadbackBuffers::default(),
            pipelines,
            pictures: PictureCache::new(picture_budget_in_bytes),
            arena: UniformArena::default(),
        }
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
        let PicturePixels::Rgba8 { straight_alpha } = &data.pixels;
        let sent_shape = TextureShape {
            width_in_pixels: width,
            height_in_pixels: height,
            format: SENT_PICTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        };
        let sent = self.pool.acquire(&self.device.device, sent_shape);
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
        let ((), mistake) = self.device.catching_errors(|| {
            self.device.queue.write_texture(
                sent.as_image_copy(),
                straight_alpha,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            let bind_group = self.device.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("engine ingest pass"),
                layout: &self.pipelines.ingest.bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &sent.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                }],
            });
            let mut encoder = self
                .device
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("engine: take a picture"),
                });
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("engine: premultiply a picture"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &converted_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                render_pass.set_pipeline(&self.pipelines.ingest.pipeline);
                render_pass.set_bind_group(0, &bind_group, &[]);
                render_pass.draw(0..3, 0..1);
            }
            self.device.queue.submit([encoder.finish()]);
        });
        self.pool.release(sent_shape, sent);
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

        let plan = plan_frame(description);
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
        self.uniforms.clear();

        let (pending, mistake) = self.device.catching_errors(|| {
            let accumulation_view = accumulation.create_view(&wgpu::TextureViewDescriptor::default());
            let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

            // The uniforms of every layer, written to the card before anything is recorded that uses them.
            let mut slots = Vec::new();
            for pass in &plan.passes {
                if let PassPlan::PlaceLayers { layers } = pass {
                    for &number in layers {
                        let uniforms = PlacementUniforms::of_layer(&description.layers[number], (width, height));
                        slots.push((number, self.uniforms.add_slot(|writer| uniforms.write(writer))));
                    }
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
            for pass in &plan.passes {
                match pass {
                    PassPlan::ClearAccumulation => {
                        drop(begin_pass(
                            &mut encoder,
                            "engine: clear the picture to black",
                            &accumulation_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        ));
                    }
                    PassPlan::PlaceLayers { .. } => {
                        let uniform_buffer = uniform_buffer.expect("a frame with layers has uniforms");
                        let mut bind_groups = Vec::with_capacity(slots.len());
                        for &(number, slot) in &slots {
                            let picture_view = match description.layers[number].source {
                                LayerSource::Picture(id) => {
                                    &self
                                        .pictures
                                        .get(id)
                                        .expect("the pictures of the frame are on the card")
                                        .view
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
        if let Some(mistake) = mistake {
            return Err(RenderProblem::GraphicsMistake(mistake));
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
                passes_recorded: plan.passes.len(),
                came_from_the_cache: false,
                effects_not_run: 0,
            },
        ))
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
