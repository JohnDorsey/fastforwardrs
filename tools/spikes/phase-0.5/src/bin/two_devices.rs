// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 1 design check: can a second device be made from the adapter that the window's device came from, and do both draw? (A harmless clear of a
//! small texture and a read back: no shader. It is run on every adapter, the real ones included, because the editor will do exactly this on the
//! window's own adapter.)
use std::time::Instant;

use spikes::gpu::{make_texture, read_rgba8};

fn main() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all())) {
        let info = adapter.get_info();
        print!("{:?} | {} | {:?}: ", info.backend, info.name, info.device_type);
        let started = Instant::now();
        let first = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()));
        let first_time = started.elapsed();
        let started = Instant::now();
        let second = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()));
        let second_time = started.elapsed();
        let (Ok((device_a, queue_a)), Ok((device_b, queue_b))) = (first, second) else {
            println!("the second device could not be made");
            continue;
        };
        let draw = |device: &wgpu::Device, queue: &wgpu::Queue, red: f64| {
            let gpu = spikes::gpu::Gpu {
                device: device.clone(),
                queue: queue.clone(),
                info: info.clone(),
                lost: Default::default(),
            };
            let texture = make_texture(&gpu, 8, 8, wgpu::TextureFormat::Rgba8Unorm);
            let view = texture.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: red, g: 0.0, b: 0.0, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            queue.submit([encoder.finish()]);
            read_rgba8(&gpu, &texture).get_pixel(3, 3).0
        };
        let a = draw(&device_a, &queue_a, 0.25);
        let b = draw(&device_b, &queue_b, 0.75);
        let a_again = draw(&device_a, &queue_a, 0.5);
        println!(
            "two devices from one adapter work: first {first_time:?}, second {second_time:?}; device A drew {a:?}, device B drew {b:?}, device A again {a_again:?}"
        );
    }
}
