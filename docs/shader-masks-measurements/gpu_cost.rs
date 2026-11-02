//! PROOF OF CONCEPT (scratch copy only): what a mask costs on a real graphics adapter.
//!
//! usage: gpu_cost <part of the adapter's name> <width>x<height> [loops] [flat]
//!
//! Part 1 runs the WGSL that the program's own compiler made (E:/a/poc_out/loop{K}_{u,e,o,v}.wgsl): `u` the input from the uniforms (what a slider is now), `e` the input read from
//! a mask picture at every use, `o` read once per pixel before the shader's main, `v` the mask read at a place that changes in every round (a read for every sample).
//! Part 2 sends the planes of a video picture to the card, and draws them into a texture (what a track that is a mask has to do every frame, on top of the decoding).

use std::time::{Duration, Instant};

const OUT: &str = "E:/a/poc_out";

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

struct Card {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Card {
    fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        });
    }
}

fn texture(card: &Card, width: u32, height: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    card.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn fill(card: &Card, texture: &wgpu::Texture, width: u32, height: u32, bytes_per_pixel: u32, pattern: impl Fn(u32, u32, u32) -> u8) {
    let mut bytes = vec![0_u8; (width * height * bytes_per_pixel) as usize];
    for y in 0..height {
        for x in 0..width {
            for channel in 0..bytes_per_pixel {
                bytes[((y * width + x) * bytes_per_pixel + channel) as usize] = pattern(x, y, channel);
            }
        }
    }
    write(card, texture, width, height, bytes_per_pixel, &bytes);
}

fn write(card: &Card, texture: &wgpu::Texture, width: u32, height: u32, bytes_per_pixel: u32, bytes: &[u8]) {
    card.queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * bytes_per_pixel),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

const VERTEX: &str = "
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(corners[index], 0.0, 1.0);
}
";

fn pipeline(card: &Card, layout: &wgpu::PipelineLayout, fragment_source: &str) -> wgpu::RenderPipeline {
    let vertex = card.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(VERTEX.into()),
    });
    let fragment = card.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(fragment_source.into()),
    });
    card.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &vertex,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fragment,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// How long `draws` full-screen draws of `pipeline` take on the card, from the submit to the card being done.
fn batch(card: &Card, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup, target: &wgpu::TextureView, draws: u32) -> Duration {
    let mut encoder = card.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        for _ in 0..draws {
            pass.draw(0..3, 0..1);
        }
    }
    let commands = encoder.finish();
    let started = Instant::now();
    card.queue.submit([commands]);
    card.wait();
    started.elapsed()
}

/// The pixels that one draw of `pipeline` makes, as bytes (rows of `width * 4`), for checking that the variants make the same picture.
fn pixels_of(card: &Card, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup, target: &wgpu::Texture, target_view: &wgpu::TextureView, width: u32, height: u32) -> Vec<u8> {
    batch(card, pipeline, group, target_view, 1);
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = card.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = card.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    card.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    card.wait();
    let mapped = buffer.slice(..).get_mapped_range().expect("the buffer is mapped");
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        pixels.extend_from_slice(&mapped[(y * row) as usize..(y * row + width * 4) as usize]);
    }
    pixels
}

fn milliseconds(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let wanted = arguments.get(1).expect("part of the name of an adapter");
    let size: Vec<u32> = arguments
        .get(2)
        .expect("WxH")
        .split('x')
        .map(|part| part.parse().unwrap())
        .collect();
    let (width, height) = (size[0], size[1]);
    let loops: Vec<String> = arguments
        .get(3)
        .map(|list| list.split(',').map(str::to_string).collect())
        .unwrap_or_else(|| vec!["8".to_string(), "32".to_string(), "128".to_string()]);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
        .into_iter()
        .find(|adapter| adapter.get_info().name.contains(wanted.as_str()))
        .expect("no such adapter");
    let information = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let card = Card { device, queue };
    println!("adapter: {} ({:?}, {:?}), frames of {width} by {height}", information.name, information.backend, information.device_type);

    // --- the textures: a picture with detail, a mask that changes smoothly across, and where the drawing goes ---
    let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
    let picture = texture(&card, width, height, wgpu::TextureFormat::Rgba8Unorm, usage);
    fill(&card, &picture, width, height, 4, |x, y, channel| {
        (x.wrapping_mul(2654435761).wrapping_add(y.wrapping_mul(40503)).wrapping_add(channel * 977) >> 11) as u8
    });
    // (The control: a mask that has the same value everywhere, and the uniform given that value, so that every shader reads the same places of the picture and only the reading of the mask differs.)
    let flat = arguments.get(4).is_some_and(|word| word == "flat");
    let mask = texture(&card, width, height, wgpu::TextureFormat::Rgba8Unorm, usage);
    fill(&card, &mask, width, height, 4, |x, y, channel| {
        if channel == 3 {
            255
        } else if flat {
            128
        } else {
            ((x * 255 / width) / 2 + (y * 255 / height) / 2) as u8
        }
    });
    println!("the mask is {}", if flat { "flat (128 of 255 everywhere), and the uniform says the same" } else { "a smooth ramp across the picture" });
    let target = texture(
        &card,
        width,
        height,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let target_view = target.create_view(&Default::default());
    let sampler = card.device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let uniforms = card.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 17 * 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut numbers = [0.0_f32; 68];
    numbers[4] = if flat { 128.0 / 255.0 } else { 0.5 }; // the first input, `amount`
    numbers[64] = width as f32;
    numbers[65] = height as f32;
    let bytes: Vec<u8> = numbers.iter().flat_map(|number| number.to_le_bytes()).collect();
    card.queue.write_buffer(&uniforms, 0, &bytes);
    card.wait();

    // --- part 1: the shaders ---
    let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    };
    let layout_group = card.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture_entry(1),
            sampler_entry(2),
            texture_entry(3),
            sampler_entry(4),
        ],
    });
    let layout = card.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout_group)],
        immediate_size: 0,
    });
    let picture_view = picture.create_view(&Default::default());
    let mask_view = mask.create_view(&Default::default());
    let group = card.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout_group,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&picture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&mask_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    println!("\nPart 1: milliseconds for one full-screen pass of a shader with a loop of K rounds (median of 7 batches)");
    println!("  u = the input from the uniforms, e = the mask read at every use, o = the mask read once per pixel, v = the mask read at a place that changes in every round");
    println!(
        "{:>6} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9}   {:>8} {:>8} {:>8} {:>8}",
        "K", "draws", "u", "e", "o", "v", "u again", "e/u", "o/u", "v/u", "u2/u"
    );
    for k in &loops {
        let modes = ["u", "e", "o", "v", "u"];
        let pipelines: Vec<wgpu::RenderPipeline> = modes
            .iter()
            .map(|mode| {
                let name = match k.strip_prefix('b') {
                    Some(number) => format!("branch{number}"),
                    None => format!("loop{k}"),
                };
                let source = std::fs::read_to_string(format!("{OUT}/{name}_{mode}.wgsl")).unwrap();
                pipeline(&card, &layout, &source)
            })
            .collect();
        // Do the variants make the same picture? (Compared with `u`: the greatest difference of a byte, and how many bytes differ by more than one.)
        let reference = pixels_of(&card, &pipelines[0], &group, &target, &target_view, width, height);
        for (mode, pipeline) in modes.iter().zip(&pipelines) {
            let made = pixels_of(&card, pipeline, &group, &target, &target_view, width, height);
            let greatest = made.iter().zip(&reference).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            let differing = made.iter().zip(&reference).filter(|(a, b)| a.abs_diff(**b) > 1).count();
            let mean = made.iter().map(|byte| *byte as f64).sum::<f64>() / made.len() as f64;
            println!("    K={k} {mode}: mean byte {mean:.2}, greatest difference from u {greatest}, bytes that differ by more than 1: {differing}");
        }
        // How many draws make a batch of about 40 ms for `u`.
        let mut draws = 1_u32;
        batch(&card, &pipelines[0], &group, &target_view, 1);
        while draws < 8192 && batch(&card, &pipelines[0], &group, &target_view, draws) < Duration::from_millis(40) {
            draws *= 2;
        }
        // The variants take turns, round after round, in an order that is a turn further each round, so that what the card does as it warms up or cools does not fall on one of them.
        for pipeline in &pipelines {
            batch(&card, pipeline, &group, &target_view, draws);
        }
        let mut times: Vec<Vec<Duration>> = vec![Vec::new(); pipelines.len()];
        for round in 0..9 {
            for turn in 0..pipelines.len() {
                let which = (turn + round) % pipelines.len();
                times[which].push(batch(&card, &pipelines[which], &group, &target_view, draws));
            }
        }
        let per_pass: Vec<f64> = times.into_iter().map(|t| milliseconds(median(t)) / draws as f64).collect();
        println!(
            "{k:>6} {draws:>8} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>9.4}   {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            per_pass[0],
            per_pass[1],
            per_pass[2],
            per_pass[3],
            per_pass[4],
            per_pass[1] / per_pass[0],
            per_pass[2] / per_pass[0],
            per_pass[3] / per_pass[0],
            per_pass[4] / per_pass[0]
        );
    }

    // --- part 2: a video picture to the card, and into a texture ---
    let planes = [(width, height), (width.div_ceil(2), height.div_ceil(2)), (width.div_ceil(2), height.div_ceil(2))];
    let plane_textures: Vec<wgpu::Texture> = planes
        .iter()
        .map(|(plane_width, plane_height)| texture(&card, *plane_width, *plane_height, wgpu::TextureFormat::R8Unorm, usage))
        .collect();
    let plane_bytes: Vec<Vec<u8>> = planes
        .iter()
        .map(|(plane_width, plane_height)| (0..plane_width * plane_height).map(|index| (index.wrapping_mul(2654435761) >> 24) as u8).collect())
        .collect();
    let rgba_bytes: Vec<u8> = (0..width * height * 4).map(|index| (index.wrapping_mul(2654435761) >> 24) as u8).collect();
    let megabytes = |count: usize| count as f64 / 1_048_576.0;
    let yuv_size: usize = plane_bytes.iter().map(Vec::len).sum();
    println!("\nPart 2: sending one picture of video to the card");
    println!("  planes of 8 bits (what the decoder gives): {:.2} MB; the same as RGBA: {:.2} MB", megabytes(yuv_size), megabytes(rgba_bytes.len()));
    let send_yuv = || {
        for ((texture, bytes), (plane_width, plane_height)) in plane_textures.iter().zip(&plane_bytes).zip(&planes) {
            write(&card, texture, *plane_width, *plane_height, 1, bytes);
        }
    };
    let send_rgba = || write(&card, &picture, width, height, 4, &rgba_bytes);
    for (name, send) in [("planes", &send_yuv as &dyn Fn()), ("RGBA", &send_rgba as &dyn Fn())] {
        for _ in 0..5 {
            send();
            card.queue.submit([]);
            card.wait();
        }
        // One at a time: sent, and waited for.
        let one_at_a_time: Vec<Duration> = (0..31)
            .map(|_| {
                let started = Instant::now();
                send();
                card.queue.submit([]);
                card.wait();
                started.elapsed()
            })
            .collect();
        // A run of 60, one right after another, waited for at the end (how video plays).
        let started = Instant::now();
        for _ in 0..60 {
            send();
            card.queue.submit([]);
        }
        card.wait();
        let run = started.elapsed() / 60;
        println!(
            "  {name:>6}: {:.2} ms each when sent one at a time and waited for, {:.2} ms each in a run of 60",
            milliseconds(median(one_at_a_time)),
            milliseconds(run)
        );
    }

    // The planes drawn into a texture as colours: what a track that is a mask has to be made of each frame.
    let convert_layout_group = card.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[texture_entry(0), texture_entry(1), texture_entry(2), sampler_entry(3)],
    });
    let convert_layout = card.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&convert_layout_group)],
        immediate_size: 0,
    });
    let views: Vec<wgpu::TextureView> = plane_textures.iter().map(|texture| texture.create_view(&Default::default())).collect();
    let convert_group = card.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &convert_layout_group,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&views[0]),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&views[1]),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&views[2]),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let convert = pipeline(
        &card,
        &convert_layout,
        "
@group(0) @binding(0) var plane_y: texture_2d<f32>;
@group(0) @binding(1) var plane_u: texture_2d<f32>;
@group(0) @binding(2) var plane_v: texture_2d<f32>;
@group(0) @binding(3) var plane_sampler: sampler;
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(textureDimensions(plane_y));
    let y = (textureSampleLevel(plane_y, plane_sampler, uv, 0.0).r - 16.0 / 255.0) * (255.0 / 219.0);
    let u = textureSampleLevel(plane_u, plane_sampler, uv, 0.0).r - 0.5;
    let v = textureSampleLevel(plane_v, plane_sampler, uv, 0.0).r - 0.5;
    return vec4<f32>(y + 1.5748 * v, y - 0.1873 * u - 0.4681 * v, y + 1.8556 * u, 1.0);
}
",
    );
    let mut draws = 1_u32;
    batch(&card, &convert, &convert_group, &target_view, 1);
    while draws < 8192 && batch(&card, &convert, &convert_group, &target_view, draws) < Duration::from_millis(40) {
        draws *= 2;
    }
    let times: Vec<Duration> = (0..7).map(|_| batch(&card, &convert, &convert_group, &target_view, draws)).collect();
    println!(
        "  drawing the planes into a texture as colours: {:.3} ms for each pass ({draws} passes in a batch)",
        milliseconds(median(times)) / draws as f64
    );
    println!(
        "  the memory of one more texture of this size: {:.1} MB as 8 bits of each colour, {:.1} MB as half floats",
        megabytes((width * height * 4) as usize),
        megabytes((width * height * 8) as usize)
    );
}
