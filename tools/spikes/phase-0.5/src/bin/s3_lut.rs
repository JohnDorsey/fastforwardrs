//! S3: a `.cube` table in a shader (tetrahedral and trilinear, written by hand in WGSL) against a CPU float reference and against ffmpeg's `lut3d`.
use std::io::Write;
use std::process::Command;

use image::{Rgb, RgbImage, Rgba, RgbaImage};
use spikes::gpu::{VERTEX_WGSL, make_texture, read_rgba8, software_gpu, upload_rgba8};

// ---------- the table ----------

struct Cube {
    size: usize,
    domain_min: [f64; 3],
    domain_max: [f64; 3],
    /// Red index fastest, as in the file.
    data: Vec<[f32; 3]>,
}

/// A deliberately bumpy table (so that interpolation matters): cross-channel terms and a ripple in red.
fn make_cube_text(size: usize, domain_min: [f64; 3], domain_max: [f64; 3]) -> String {
    let mut text = String::new();
    text.push_str("# written by the S3 spike\nTITLE \"spike table\"\n");
    text.push_str(&format!("LUT_3D_SIZE {size}\n"));
    text.push_str(&format!(
        "DOMAIN_MIN {} {} {}\n",
        domain_min[0], domain_min[1], domain_min[2]
    ));
    text.push_str(&format!(
        "DOMAIN_MAX {} {} {}\n",
        domain_max[0], domain_max[1], domain_max[2]
    ));
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let (x, y, z) = (
                    r as f64 / (size - 1) as f64,
                    g as f64 / (size - 1) as f64,
                    b as f64 / (size - 1) as f64,
                );
                let out_r = (0.5 + 0.5 * (x * 7.0 + y * 3.0).sin()) * 0.8 + 0.2 * z;
                let out_g = y.powf(1.4) * 0.9 + 0.1 * x * z;
                let out_b = z * (1.0 - 0.4 * x) + 0.3 * (1.0 - y) * z + 0.05;
                text.push_str(&format!(
                    "{:.6} {:.6} {:.6}\n",
                    out_r.clamp(0.0, 1.0),
                    out_g.clamp(0.0, 1.0),
                    out_b.clamp(0.0, 1.0)
                ));
            }
        }
    }
    text
}

fn parse_cube(text: &str) -> Cube {
    let mut size = 0;
    let mut domain_min = [0.0; 3];
    let mut domain_max = [1.0; 3];
    let mut data = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("TITLE") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "LUT_3D_SIZE" => size = parts[1].parse().unwrap(),
            "DOMAIN_MIN" => {
                domain_min = [
                    parts[1].parse().unwrap(),
                    parts[2].parse().unwrap(),
                    parts[3].parse().unwrap(),
                ]
            }
            "DOMAIN_MAX" => {
                domain_max = [
                    parts[1].parse().unwrap(),
                    parts[2].parse().unwrap(),
                    parts[3].parse().unwrap(),
                ]
            }
            _ => data.push([
                parts[0].parse().unwrap(),
                parts[1].parse().unwrap(),
                parts[2].parse().unwrap(),
            ]),
        }
    }
    assert_eq!(data.len(), size * size * size);
    Cube {
        size,
        domain_min,
        domain_max,
        data,
    }
}

// ---------- the CPU reference (f64) ----------

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn cpu_lookup(cube: &Cube, input: [f64; 3], tetrahedral: bool) -> [f64; 3] {
    let n = cube.size;
    let mut prev = [0usize; 3];
    let mut next = [0usize; 3];
    let mut d = [0.0f64; 3];
    for k in 0..3 {
        let normalised = ((input[k] - cube.domain_min[k]) / (cube.domain_max[k] - cube.domain_min[k])).clamp(0.0, 1.0);
        let scaled = normalised * (n - 1) as f64;
        prev[k] = (scaled.floor() as usize).min(n - 1);
        next[k] = (prev[k] + 1).min(n - 1);
        d[k] = scaled - prev[k] as f64;
    }
    let at = |r: usize, g: usize, b: usize| -> [f64; 3] {
        let v = cube.data[r + n * (g + n * b)];
        [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
    };
    let c000 = at(prev[0], prev[1], prev[2]);
    let c001 = at(prev[0], prev[1], next[2]);
    let c010 = at(prev[0], next[1], prev[2]);
    let c011 = at(prev[0], next[1], next[2]);
    let c100 = at(next[0], prev[1], prev[2]);
    let c101 = at(next[0], prev[1], next[2]);
    let c110 = at(next[0], next[1], prev[2]);
    let c111 = at(next[0], next[1], next[2]);
    if !tetrahedral {
        let c00 = lerp(c000, c100, d[0]);
        let c10 = lerp(c010, c110, d[0]);
        let c01 = lerp(c001, c101, d[0]);
        let c11 = lerp(c011, c111, d[0]);
        return lerp(lerp(c00, c10, d[1]), lerp(c01, c11, d[1]), d[2]);
    }
    let (r, g, b) = (d[0], d[1], d[2]);
    let terms: [([f64; 3], [f64; 3], f64); 3] = if r > g {
        if g > b {
            [(c000, c100, r), (c100, c110, g), (c110, c111, b)]
        } else if r > b {
            [(c000, c100, r), (c100, c101, b), (c101, c111, g)]
        } else {
            [(c000, c001, b), (c001, c101, r), (c101, c111, g)]
        }
    } else if b > g {
        [(c000, c001, b), (c001, c011, g), (c011, c111, r)]
    } else if b > r {
        [(c000, c010, g), (c010, c011, b), (c011, c111, r)]
    } else {
        [(c000, c010, g), (c010, c110, r), (c110, c111, b)]
    };
    let mut out = c000;
    for (from, to, t) in terms {
        for k in 0..3 {
            out[k] += (to[k] - from[k]) * t;
        }
    }
    out
}

// ---------- the shader ----------

const LUT_FRAGMENT: &str = r#"
struct Params {
    size: f32,
    mode: f32,            // 0 = tetrahedral, 1 = trilinear by hand, 2 = trilinear by the sampler's hardware
    domain_min: vec4<f32>,
    domain_max: vec4<f32>,
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var lut: texture_3d<f32>;
@group(0) @binding(2) var picture: texture_2d<f32>;
@group(0) @binding(3) var lut_filtered: texture_3d<f32>;
@group(0) @binding(4) var lut_sampler: sampler;

fn corner(r: i32, g: i32, b: i32) -> vec3<f32> {
    return textureLoad(lut, vec3<i32>(r, g, b), 0).rgb;
}

@fragment fn main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let source = textureLoad(picture, vec2<i32>(position.xy), 0);
    let n = i32(params.size);
    let normalised = clamp((source.rgb - params.domain_min.rgb) / (params.domain_max.rgb - params.domain_min.rgb), vec3<f32>(0.0), vec3<f32>(1.0));
    if (params.mode > 1.5) {
        // The sampler's own trilinear filtering: texel centres sit at (i + 0.5) / n.
        let coord = (normalised * f32(n - 1) + vec3<f32>(0.5)) / f32(n);
        return vec4<f32>(textureSampleLevel(lut_filtered, lut_sampler, coord, 0.0).rgb, source.a);
    }
    let scaled = normalised * f32(n - 1);
    let prev = min(vec3<i32>(floor(scaled)), vec3<i32>(n - 1));
    let next = min(prev + vec3<i32>(1), vec3<i32>(n - 1));
    let d = scaled - vec3<f32>(prev);
    let c000 = corner(prev.x, prev.y, prev.z);
    let c001 = corner(prev.x, prev.y, next.z);
    let c010 = corner(prev.x, next.y, prev.z);
    let c011 = corner(prev.x, next.y, next.z);
    let c100 = corner(next.x, prev.y, prev.z);
    let c101 = corner(next.x, prev.y, next.z);
    let c110 = corner(next.x, next.y, prev.z);
    let c111 = corner(next.x, next.y, next.z);
    var out: vec3<f32>;
    if (params.mode > 0.5) {
        let c00 = mix(c000, c100, d.x);
        let c10 = mix(c010, c110, d.x);
        let c01 = mix(c001, c101, d.x);
        let c11 = mix(c011, c111, d.x);
        out = mix(mix(c00, c10, d.y), mix(c01, c11, d.y), d.z);
    } else {
        if (d.x > d.y) {
            if (d.y > d.z) {
                out = c000 + (c100 - c000) * d.x + (c110 - c100) * d.y + (c111 - c110) * d.z;
            } else if (d.x > d.z) {
                out = c000 + (c100 - c000) * d.x + (c101 - c100) * d.z + (c111 - c101) * d.y;
            } else {
                out = c000 + (c001 - c000) * d.z + (c101 - c001) * d.x + (c111 - c101) * d.y;
            }
        } else {
            if (d.z > d.y) {
                out = c000 + (c001 - c000) * d.z + (c011 - c001) * d.y + (c111 - c011) * d.x;
            } else if (d.z > d.x) {
                out = c000 + (c010 - c000) * d.y + (c011 - c010) * d.z + (c111 - c011) * d.x;
            } else {
                out = c000 + (c010 - c000) * d.y + (c110 - c010) * d.x + (c111 - c110) * d.z;
            }
        }
    }
    return vec4<f32>(out, source.a);
}
"#;

fn half_bits(value: f32) -> u16 {
    half::f16::from_f32(value).to_bits()
}

fn make_lut_texture(gpu: &spikes::gpu::Gpu, cube: &Cube, format: wgpu::TextureFormat) -> wgpu::Texture {
    let n = cube.size as u32;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lut"),
        size: wgpu::Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: n,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: Vec<u8> = match format {
        wgpu::TextureFormat::Rgba32Float => cube
            .data
            .iter()
            .flat_map(|v| [v[0], v[1], v[2], 1.0f32])
            .flat_map(f32::to_le_bytes)
            .collect(),
        wgpu::TextureFormat::Rgba16Float => cube
            .data
            .iter()
            .flat_map(|v| [half_bits(v[0]), half_bits(v[1]), half_bits(v[2]), half_bits(1.0)])
            .flat_map(u16::to_le_bytes)
            .collect(),
        _ => unreachable!(),
    };
    let bytes_per_pixel = if format == wgpu::TextureFormat::Rgba32Float {
        16
    } else {
        8
    };
    gpu.queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_pixel * n),
            rows_per_image: Some(n),
        },
        wgpu::Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: n,
        },
    );
    texture
}

/// Runs the table over the picture on the GPU; `mode` 0 tetrahedral, 1 trilinear by hand, 2 hardware trilinear (16-bit float table). Returns the 8-bit picture and the float one.
fn run_gpu(gpu: &spikes::gpu::Gpu, cube: &Cube, picture: &RgbaImage, mode: f32) -> (RgbaImage, Vec<f32>) {
    let (width, height) = picture.dimensions();
    let lut32 = make_lut_texture(gpu, cube, wgpu::TextureFormat::Rgba32Float);
    let lut16 = make_lut_texture(gpu, cube, wgpu::TextureFormat::Rgba16Float);
    let input = upload_rgba8(gpu, picture);
    let vertex = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(VERTEX_WGSL.into()),
    });
    let fragment = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(LUT_FRAGMENT.into()),
    });
    let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let mut params = vec![0u8; 64];
    let floats: [f32; 12] = [
        cube.size as f32,
        mode,
        0.0,
        0.0,
        cube.domain_min[0] as f32,
        cube.domain_min[1] as f32,
        cube.domain_min[2] as f32,
        0.0,
        cube.domain_max[0] as f32,
        cube.domain_max[1] as f32,
        cube.domain_max[2] as f32,
        0.0,
    ];
    for (k, value) in floats.iter().enumerate() {
        params[4 * k..4 * k + 4].copy_from_slice(&value.to_le_bytes());
    }
    let params_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue.write_buffer(&params_buffer, 0, &params[..48]);
    let lut32_view = lut32.create_view(&Default::default());
    let lut16_view = lut16.create_view(&Default::default());
    let input_view = input.create_view(&Default::default());
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params_buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(48),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&lut32_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&input_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&lut16_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut results: Vec<(RgbaImage, Vec<f32>)> = Vec::new();
    for format in [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Rgba32Float] {
        let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let target = make_texture(gpu, width, height, format);
        let target_view = target.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
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
            pass.set_bind_group(0, &group, &[0]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        if format == wgpu::TextureFormat::Rgba8Unorm {
            results.push((read_rgba8(gpu, &target), vec![]));
        } else {
            results.push((RgbaImage::new(1, 1), read_rgba32f(gpu, &target)));
        }
    }
    let float_picture = results.pop().unwrap().1;
    (results.pop().unwrap().0, float_picture)
}

fn read_rgba32f(gpu: &spikes::gpu::Gpu, texture: &wgpu::Texture) -> Vec<f32> {
    let (width, height) = (texture.width(), texture.height());
    let padded = (16 * width).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(padded * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = slice.get_mapped_range().unwrap();
    let mut out = Vec::new();
    for row in 0..height {
        let start = (row * padded) as usize;
        for chunk in data[start..start + (16 * width) as usize].chunks_exact(4) {
            out.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }
    }
    out
}

// ---------- the picture and ffmpeg ----------

fn test_picture() -> RgbImage {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 24) as u8
    };
    RgbImage::from_fn(256, 256, |x, y| match y {
        0 => Rgb([x as u8, x as u8, x as u8]),
        1 => Rgb([x as u8, 0, 0]),
        2 => Rgb([0, x as u8, 0]),
        3 => Rgb([0, 0, x as u8]),
        4 => Rgb([x as u8, 255 - x as u8, 128]),
        _ => Rgb([next(), next(), next()]),
    })
}

fn ffmpeg_lut(folder: &std::path::Path, input: &str, cube: &str, interp: &str, output: &str) {
    let status = Command::new("ffmpeg")
        .current_dir(folder)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            input,
            "-vf",
            &format!("format=rgb24,lut3d=file={cube}:interp={interp}"),
            "-pix_fmt",
            "rgb24",
            output,
        ])
        .status()
        .expect("ffmpeg did not start");
    assert!(status.success(), "ffmpeg failed");
}

struct Stats {
    largest: u8,
    mean_absolute: f64,
    mean_signed: f64,
    over_one: usize,
    total: usize,
}

fn compare_8bit(a: &[u8], b: &[u8]) -> Stats {
    let mut stats = Stats {
        largest: 0,
        mean_absolute: 0.0,
        mean_signed: 0.0,
        over_one: 0,
        total: a.len(),
    };
    for (x, y) in a.iter().zip(b) {
        let d = i32::from(*x) - i32::from(*y);
        stats.largest = stats.largest.max(d.unsigned_abs() as u8);
        stats.mean_absolute += f64::from(d.abs());
        stats.mean_signed += f64::from(d);
        if d.abs() > 1 {
            stats.over_one += 1;
        }
    }
    stats.mean_absolute /= a.len() as f64;
    stats.mean_signed /= a.len() as f64;
    stats
}

fn describe(label: &str, stats: &Stats) {
    println!(
        "    {label}: largest {} of 255, mean |d| {:.4}, mean signed {:+.4}, {} of {} values differ by more than 1",
        stats.largest, stats.mean_absolute, stats.mean_signed, stats.over_one, stats.total
    );
}

fn main() {
    let folder = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: s3_lut <work folder>"));
    std::fs::create_dir_all(&folder).unwrap();
    let gpu = software_gpu();
    println!("adapter: {} ({:?})", gpu.info.name, gpu.info.backend);
    let rgb = test_picture();
    rgb.save(folder.join("input.png")).unwrap();
    let rgba = RgbaImage::from_fn(256, 256, |x, y| {
        let p = rgb.get_pixel(x, y).0;
        Rgba([p[0], p[1], p[2], 255])
    });

    for (name, size, domain_min, domain_max) in [
        ("lut17", 17usize, [0.0; 3], [1.0; 3]),
        ("lut33", 33, [0.0; 3], [1.0; 3]),
        ("lut17_domain", 17, [0.05, 0.0, 0.1], [0.95, 1.0, 0.9]),
    ] {
        println!("=== {name}: {size}^3 points, domain {domain_min:?} to {domain_max:?}");
        let text = make_cube_text(size, domain_min, domain_max);
        let cube_file = format!("{name}.cube");
        std::fs::File::create(folder.join(&cube_file))
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        let cube = parse_cube(&text);

        // CPU reference at float precision, as 8-bit by rounding to nearest and by truncation.
        let mut reference = [vec![0f64; 256 * 256 * 3], vec![0f64; 256 * 256 * 3]];
        for (k, pixel) in rgb.pixels().enumerate() {
            let input = [
                f64::from(pixel.0[0]) / 255.0,
                f64::from(pixel.0[1]) / 255.0,
                f64::from(pixel.0[2]) / 255.0,
            ];
            for (mode, tetra) in [(0, true), (1, false)] {
                let out = cpu_lookup(&cube, input, tetra);
                reference[mode][3 * k..3 * k + 3].copy_from_slice(&out);
            }
        }
        let reference_8bit = |mode: usize, truncate: bool| -> Vec<u8> {
            reference[mode]
                .iter()
                .map(|v| {
                    if truncate {
                        (v * 255.0).clamp(0.0, 255.0) as u8
                    } else {
                        (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
                    }
                })
                .collect()
        };

        for (mode_label, mode_index, gpu_mode, interp) in [
            ("tetrahedral", 0usize, 0.0f32, "tetrahedral"),
            ("trilinear", 1, 1.0, "trilinear"),
        ] {
            println!("  -- {mode_label}");
            ffmpeg_lut(
                &folder,
                "input.png",
                &cube_file,
                interp,
                &format!("{name}_{interp}_ffmpeg.png"),
            );
            let ffmpeg_picture = image::open(folder.join(format!("{name}_{interp}_ffmpeg.png")))
                .unwrap()
                .to_rgb8();
            let (gpu_picture, gpu_float) = run_gpu(&gpu, &cube, &rgba, gpu_mode);
            let gpu_rgb: Vec<u8> = gpu_picture.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2]]).collect();
            let ffmpeg_rgb = ffmpeg_picture.as_raw().clone();
            // float comparison GPU vs CPU
            let mut worst_float = 0f64;
            for k in 0..256 * 256 {
                for c in 0..3 {
                    worst_float =
                        worst_float.max((f64::from(gpu_float[4 * k + c]) - reference[mode_index][3 * k + c]).abs());
                }
            }
            println!(
                "    GPU (32-bit float target) vs CPU f64 reference: largest difference {worst_float:.2e} (= {:.4} of one 8-bit step)",
                worst_float * 255.0
            );
            describe(
                "GPU (8-bit target) vs CPU reference rounded to nearest",
                &compare_8bit(&gpu_rgb, &reference_8bit(mode_index, false)),
            );
            describe(
                "ffmpeg vs CPU reference rounded to nearest",
                &compare_8bit(&ffmpeg_rgb, &reference_8bit(mode_index, false)),
            );
            describe(
                "ffmpeg vs CPU reference TRUNCATED",
                &compare_8bit(&ffmpeg_rgb, &reference_8bit(mode_index, true)),
            );
            describe("GPU (8-bit target) vs ffmpeg", &compare_8bit(&gpu_rgb, &ffmpeg_rgb));
            if mode_index == 1 {
                let (hardware, _) = run_gpu(&gpu, &cube, &rgba, 2.0);
                let hardware_rgb: Vec<u8> = hardware.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2]]).collect();
                describe(
                    "hardware trilinear (16-bit float table, the sampler's own filtering) vs ffmpeg trilinear",
                    &compare_8bit(&hardware_rgb, &ffmpeg_rgb),
                );
                describe(
                    "hardware trilinear vs CPU trilinear reference",
                    &compare_8bit(&hardware_rgb, &reference_8bit(1, false)),
                );
            }
        }
        if domain_min != [0.0; 3] {
            // Hypothesis: ffmpeg's lut3d does not subtract DOMAIN_MIN and clips its scale to at most 1, so for this table it behaves as if the domain were 0 to 1.
            let plain = Cube {
                size: cube.size,
                domain_min: [0.0; 3],
                domain_max: [1.0; 3],
                data: cube.data.clone(),
            };
            let mut as_if_plain = Vec::new();
            for pixel in rgb.pixels() {
                let input = [
                    f64::from(pixel.0[0]) / 255.0,
                    f64::from(pixel.0[1]) / 255.0,
                    f64::from(pixel.0[2]) / 255.0,
                ];
                for v in cpu_lookup(&plain, input, true) {
                    as_if_plain.push((v * 255.0).clamp(0.0, 255.0) as u8);
                }
            }
            let ffmpeg_picture = image::open(folder.join(format!("{name}_tetrahedral_ffmpeg.png")))
                .unwrap()
                .to_rgb8();
            describe(
                "ffmpeg tetrahedral vs CPU tetrahedral with the domain IGNORED (truncated)",
                &compare_8bit(ffmpeg_picture.as_raw(), &as_if_plain),
            );
        }
        // How different are the two interpolations from each other on this table (so that "matches" means something)?
        describe(
            "CPU tetrahedral vs CPU trilinear (what a wrong choice of interpolation would cost)",
            &compare_8bit(&reference_8bit(0, false), &reference_8bit(1, false)),
        );
    }
}
