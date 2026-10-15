// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making one frame: everything of the frame is recorded into one batch of work for the card, the batch is submitted, and the pixels are taken back.

use std::time::{Duration, Instant};

use thiserror::Error;

use super::device::EngineDevice;
use super::pipelines::{ACCUMULATION_FORMAT, OUTPUT_FORMAT, Pipelines};
use super::plan::{PassPlan, plan_frame};
use super::readback::{ReadbackBuffers, ReadbackProblem};
use super::texture_pool::{PoolStatistics, TexturePool, TextureShape};
use super::{FrameDescription, FrameStatistics, RenderedFrame};

/// How many bytes of textures that are waiting to be used are kept from one frame to the next.
const IDLE_TEXTURE_BUDGET_IN_BYTES: u64 = 256 * 1024 * 1024;

/// How many read-back buffers are kept from one frame to the next.
const READBACK_BUFFERS_KEPT: usize = 2;

/// Why a frame could not be made.
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
}

/// Makes frames on a device. See the module documentation.
pub struct FrameRenderer {
    device: EngineDevice,
    pool: TexturePool,
    readback: ReadbackBuffers,
    pipelines: Pipelines,
}

impl FrameRenderer {
    pub fn new(device: EngineDevice) -> Self {
        let pipelines = Pipelines::create(&device.device);
        Self {
            device,
            pool: TexturePool::default(),
            readback: ReadbackBuffers::default(),
            pipelines,
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

        let (pending, mistake) = self.device.catching_errors(|| {
            let accumulation_view = accumulation.create_view(&wgpu::TextureViewDescriptor::default());
            let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("engine frame"),
                });
            for pass in &plan.passes {
                match pass {
                    PassPlan::ClearAccumulation => {
                        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("engine: clear the picture to black"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &accumulation_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        }));
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
                        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("engine: the 8-bit frame"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &output_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
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
