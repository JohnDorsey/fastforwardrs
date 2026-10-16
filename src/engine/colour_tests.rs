// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour arithmetic of the shaders against its reference in Rust (`colour_space`): CM-08, "written once in Rust and once in WGSL, and checked against each other". A
//! piece of WGSL is run on the software graphics adapter over a row of colours, and what it makes is compared with what the Rust code makes of the same.

use super::device::EngineDevice;
use super::pipelines::{pipeline_of_two_modules, shader_module};
use super::shaders;
use super::testing::software_graphics;
use crate::colour_space::TransferFunction;

/// How many colours one run of a shader is given at most (a row of a texture; 4096 pixels of 16 bytes is a row of a multiple of 256 bytes, which the copy needs).
const ROW: u32 = 4096;

const THE_VERTEX_STAGE: &str = "
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Runs `shader`'s `fs_main` once for each of `inputs` on the card of `device`, and gives what it wrote for each. The shader has `vs_main` too (see
/// [`THE_VERTEX_STAGE`]) and reads the colour of its pixel at `@group(0) @binding(0)` (`texture_2d<f32>`, read with `textureLoad` at `position.xy`, which is the place of
/// the colour in the row) and, if `uniforms` is not empty, a uniform at `@group(0) @binding(1)`.
pub(super) fn run_on_the_card(
    device: &EngineDevice,
    shader: &str,
    uniforms: &[u8],
    inputs: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    assert!(inputs.len() <= ROW as usize, "one run is a row of {ROW} colours");
    let card = &device.device;
    let module = shader_module(card, "colour test", shader);
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }];
    if !uniforms.is_empty() {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    let bind_group_layout = card.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("colour test bindings"),
        entries: &entries,
    });
    let layout = card.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("colour test layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = pipeline_of_two_modules(
        card,
        "colour test",
        &module,
        &module,
        &layout,
        wgpu::TextureFormat::Rgba32Float,
        None,
    );

    let extent = wgpu::Extent3d {
        width: ROW,
        height: 1,
        depth_or_array_layers: 1,
    };
    let input_texture = card.create_texture(&wgpu::TextureDescriptor {
        label: Some("colour test inputs"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut padded = vec![[0.0_f32; 4]; ROW as usize];
    padded[..inputs.len()].copy_from_slice(inputs);
    let input_bytes: Vec<u8> = padded
        .iter()
        .flatten()
        .flat_map(|number| number.to_le_bytes())
        .collect();
    device.queue.write_texture(
        input_texture.as_image_copy(),
        &input_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16 * ROW),
            rows_per_image: Some(1),
        },
        extent,
    );
    let output_texture = card.create_texture(&wgpu::TextureDescriptor {
        label: Some("colour test outputs"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let uniform_buffer = (!uniforms.is_empty()).then(|| {
        let buffer = card.create_buffer(&wgpu::BufferDescriptor {
            label: Some("colour test uniforms"),
            size: uniforms.len().next_multiple_of(16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut bytes = uniforms.to_vec();
        bytes.resize(uniforms.len().next_multiple_of(16), 0);
        device.queue.write_buffer(&buffer, 0, &bytes);
        buffer
    });
    let input_view = input_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut bind_entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&input_view),
    }];
    if let Some(buffer) = &uniform_buffer {
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 1,
            resource: buffer.as_entire_binding(),
        });
    }
    let bind_group = card.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("colour test"),
        layout: &bind_group_layout,
        entries: &bind_entries,
    });
    let read_back = card.create_buffer(&wgpu::BufferDescriptor {
        label: Some("colour test read-back"),
        size: u64::from(16 * ROW),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let ((), mistake) = device.catching_errors(|| {
        let mut encoder = card.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("colour test"),
        });
        {
            let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("colour test"),
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            output_texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read_back,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(16 * ROW),
                    rows_per_image: Some(1),
                },
            },
            extent,
        );
        device.queue.submit([encoder.finish()]);
    });
    assert_eq!(mistake, None, "the card or the library found a mistake in the shader");

    let (sender, receiver) = std::sync::mpsc::channel();
    read_back.slice(..).map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    card.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(60)),
    })
    .expect("the card finishes");
    receiver
        .recv()
        .expect("the buffer is mapped")
        .expect("the buffer is mapped without a mistake");
    let bytes = read_back
        .slice(..)
        .get_mapped_range()
        .expect("the buffer is mapped")
        .to_vec();
    read_back.unmap();
    bytes
        .as_chunks::<16>()
        .0
        .iter()
        .take(inputs.len())
        .map(|pixel| {
            let number =
                |place: usize| f32::from_le_bytes(pixel[4 * place..4 * place + 4].try_into().expect("four bytes"));
            [number(0), number(1), number(2), number(3)]
        })
        .collect()
}

/// A shader that makes light of the colours it is given (or codes of them) with the curve whose number is in its uniform.
fn the_shader_of_a_curve() -> String {
    format!(
        "{}\n{THE_VERTEX_STAGE}
struct Which {{
    curve: u32,
    to_codes: u32,
}}
@group(0) @binding(0) var colours: texture_2d<f32>;
@group(0) @binding(1) var<uniform> which: Which;
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {{
    let colour = textureLoad(colours, vec2<i32>(position.xy), 0).rgb;
    if which.to_codes == 1u {{
        return vec4<f32>(encode_from_light(which.curve, colour), 1.0);
    }}
    return vec4<f32>(decode_to_light(which.curve, colour), 1.0);
}}
",
        shaders::COLOUR_CURVES
    )
}

fn the_uniform_of_a_curve(curve: TransferFunction, to_codes: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(curve.number_for_the_shaders().to_le_bytes());
    bytes.extend(u32::from(to_codes).to_le_bytes());
    bytes
}

/// 4096 colours, every channel on its own way across 0 to 1 (and a little past it at the ends, to see what is cut off).
fn colours_across_the_range() -> Vec<[f32; 4]> {
    (0..ROW)
        .map(|step| {
            let across = step as f32 / (ROW - 1) as f32;
            [across, 1.0 - across, across * across, 1.0]
        })
        .collect()
}

#[test]
fn the_shader_curves_are_the_reference_curves_to_the_precision_of_the_card() {
    let Some(graphics) = software_graphics() else { return };
    let inputs = colours_across_the_range();
    let shader = the_shader_of_a_curve();
    for curve in TransferFunction::ALL {
        for to_codes in [false, true] {
            let made = run_on_the_card(
                &graphics.device,
                &shader,
                &the_uniform_of_a_curve(curve, to_codes),
                &inputs,
            );
            let mut worst = 0.0_f64;
            for (input, output) in inputs.iter().zip(&made) {
                for channel in 0..3 {
                    let reference = if to_codes {
                        curve.encode_from_light(f64::from(input[channel]))
                    } else {
                        curve.decode_to_light(f64::from(input[channel]))
                    };
                    worst = worst.max((f64::from(output[channel]) - reference).abs());
                }
            }
            // A bound that was measured (see the line printed below with --nocapture) and written down with room to spare: 1e-5 is a thirtieth of a level of 8 bits.
            println!(
                "{curve:?} {}: the worst difference is {worst:e}",
                if to_codes { "encode" } else { "decode" }
            );
            assert!(
                worst < 1e-5,
                "{curve:?} {}: the card is {worst} from the reference",
                if to_codes { "encode" } else { "decode" }
            );
        }
    }
}

#[test]
fn the_shader_curves_cut_off_what_is_out_of_range_as_the_reference_does() {
    let Some(graphics) = software_graphics() else { return };
    // Codes below 0 and above 1, lights below 0 and above 1, and an exact 0 and 1: where the card's `pow` is asked for what is not defined if nothing cut it off.
    let inputs: Vec<[f32; 4]> = vec![
        [-0.5, 0.0, 1.0, 1.0],
        [1.5, 2.0, 100.0, 1.0],
        [-1e-9, 1e-9, 0.04045, 1.0],
        [0.0031308, 0.018, 0.081, 1.0],
        [-100.0, 0.5, 3.0, 1.0],
    ];
    let shader = the_shader_of_a_curve();
    for curve in TransferFunction::ALL {
        for to_codes in [false, true] {
            let made = run_on_the_card(
                &graphics.device,
                &shader,
                &the_uniform_of_a_curve(curve, to_codes),
                &inputs,
            );
            for (input, output) in inputs.iter().zip(&made) {
                for channel in 0..3 {
                    let reference = if to_codes {
                        curve.encode_from_light(f64::from(input[channel]))
                    } else {
                        curve.decode_to_light(f64::from(input[channel]))
                    };
                    let tolerance = 1e-5 * reference.abs().max(1.0);
                    assert!(
                        output[channel].is_finite() && (f64::from(output[channel]) - reference).abs() <= tolerance,
                        "{curve:?} {}: {} gives {} and the reference {reference}",
                        if to_codes { "encode" } else { "decode" },
                        input[channel],
                        output[channel]
                    );
                }
            }
        }
    }
}
