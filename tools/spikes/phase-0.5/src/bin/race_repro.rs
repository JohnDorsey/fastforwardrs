//! Minimal reproduction: two draws, each with its own uniform buffer and its own target. Do both come out right?
use spikes::gpu::{VERTEX_WGSL, make_texture, read_rgba8, software_gpu};

const FRAGMENT_TEXTURE: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn main() -> @location(0) vec4<f32> { return vec4<f32>(textureSampleLevel(t, s, vec2<f32>(0.5, 0.5), 0.0).r, 0.0, 0.0, 1.0); }
"#;

const FRAGMENT: &str = r#"
struct U { value: f32 }
@group(0) @binding(0) var<uniform> u: U;
@fragment fn main() -> @location(0) vec4<f32> { return vec4<f32>(u.value, 0.0, 0.0, 1.0); }
"#;

fn run_textures(gpu: &spikes::gpu::Gpu, label: &str, mode: &str) {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let vertex = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(VERTEX_WGSL.into()),
    });
    let fragment = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(FRAGMENT_TEXTURE.into()),
    });
    let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
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
    let sampler = gpu.device.create_sampler(&Default::default());
    let sources = [
        spikes::gpu::upload_rgba8(gpu, &image::RgbaImage::from_pixel(1, 1, image::Rgba([64, 0, 0, 255]))),
        spikes::gpu::upload_rgba8(gpu, &image::RgbaImage::from_pixel(1, 1, image::Rgba([191, 0, 0, 255]))),
    ];
    let targets = [make_texture(gpu, 16, 16, format), make_texture(gpu, 16, 16, format)];
    let mut shared_encoder = gpu.device.create_command_encoder(&Default::default());
    let mut keep = Vec::new();
    for k in 0..2 {
        let source_view = sources[k].create_view(&Default::default());
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let view = targets[k].create_view(&Default::default());
        let mut own = gpu.device.create_command_encoder(&Default::default());
        let encoder = if mode.starts_with("one encoder") {
            &mut shared_encoder
        } else {
            &mut own
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        if mode.starts_with("one encoder") {
        } else if mode.contains("wait after each") {
            gpu.queue.submit([own.finish()]);
            gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        } else {
            gpu.queue.submit([own.finish()]);
        }
        keep.push((source_view, group));
    }
    if mode.starts_with("one encoder") {
        gpu.queue.submit([shared_encoder.finish()]);
    }
    let a = read_rgba8(gpu, &targets[0]).get_pixel(8, 8).0[0];
    let b = read_rgba8(gpu, &targets[1]).get_pixel(8, 8).0[0];
    println!(
        "{label}: TEXTURES: {mode}: target A red {a} (expected 64), target B red {b} (expected 191) -> {}",
        if a == 64 && b == 191 { "RIGHT" } else { "WRONG" }
    );
}

fn run(gpu: &spikes::gpu::Gpu, label: &str, mode: &str) {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let vertex = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(VERTEX_WGSL.into()),
    });
    let fragment = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(FRAGMENT.into()),
    });
    let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: mode.contains("dynamic"),
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
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
    let values = [0.25f32, 0.75f32];
    let targets = [make_texture(gpu, 16, 16, format), make_texture(gpu, 16, 16, format)];
    let mut shared_encoder = gpu.device.create_command_encoder(&Default::default());
    let mut keep = Vec::new();
    let shared = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 512,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for k in 0..2 {
        let own_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (buffer, offset) = if mode.contains("shared") {
            (shared.clone(), 256 * k as u64)
        } else {
            (own_buffer, 0)
        };
        gpu.queue
            .write_buffer(&buffer, offset, bytemuck::bytes_of(&[values[k], 0.0f32, 0.0, 0.0]));
        let binding = if mode.contains("dynamic") {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: std::num::NonZeroU64::new(16),
            })
        } else {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset,
                size: std::num::NonZeroU64::new(16),
            })
        };
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: binding,
            }],
        });
        let dynamic_offsets: Vec<u32> = if mode.contains("dynamic") {
            vec![offset as u32]
        } else {
            vec![]
        };
        let view = targets[k].create_view(&Default::default());
        let mut own = gpu.device.create_command_encoder(&Default::default());
        let encoder = if mode.starts_with("one encoder") {
            &mut shared_encoder
        } else {
            &mut own
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_bind_group(0, &group, &dynamic_offsets);
            pass.draw(0..3, 0..1);
        }
        if mode.starts_with("one encoder") {
        } else if mode.contains("wait after each") {
            gpu.queue.submit([own.finish()]);
            gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        } else {
            gpu.queue.submit([own.finish()]);
        }
        keep.push((buffer, group));
    }
    if mode.starts_with("one encoder") {
        gpu.queue.submit([shared_encoder.finish()]);
    }
    let a = read_rgba8(gpu, &targets[0]).get_pixel(8, 8).0[0];
    let b = read_rgba8(gpu, &targets[1]).get_pixel(8, 8).0[0];
    println!(
        "{label}: {mode}: target A red {a} (expected 64), target B red {b} (expected 191) -> {}",
        if a == 64 && b == 191 { "RIGHT" } else { "WRONG" }
    );
}

/// For THIS harmless repro only (two small triangles, no loops): any adapter whose backend and name match `SPIKE_REPRO_ADAPTER` ("Dx12:AMD"), else the software one.
fn gpu_for_the_repro() -> spikes::gpu::Gpu {
    let Ok(wanted) = std::env::var("SPIKE_REPRO_ADAPTER") else {
        return software_gpu();
    };
    let (backend, name) = wanted.split_once(':').unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
        .into_iter()
        .find(|a| format!("{:?}", a.get_info().backend) == backend && a.get_info().name.contains(name))
        .expect("no such adapter");
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    spikes::gpu::Gpu {
        device,
        queue,
        info,
        lost: Default::default(),
    }
}

fn main() {
    let gpu = gpu_for_the_repro();
    println!("adapter: {} {:?}", gpu.info.name, gpu.info.backend);
    for round in 0..3 {
        for mode in [
            "one encoder",
            "separate submits, no waiting",
            "separate submits, wait after each",
            "one encoder, shared buffer with two bind groups at offsets",
            "one encoder, shared buffer, dynamic offsets",
        ] {
            run(&gpu, &format!("round {round}"), mode);
            if !mode.contains("shared") {
                run_textures(&gpu, &format!("round {round}"), mode);
            }
        }
    }
}
