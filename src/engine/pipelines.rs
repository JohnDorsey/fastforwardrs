// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pipelines of the passes, made once when the device is made. Their bind group layouts are **explicit** (made here, not worked out by wgpu
//! from the shader): with an automatic layout wgpu leaves out the bindings that a shader does not use, and then a bind group has to be made
//! differently for each shader (found in phase 0.5).

use std::num::NonZeroU64;

use super::effect_shaders::ForeignEffectPass;
use super::shaders;

/// The format of the picture that the layers are put together in, and of the pictures of the layers: premultiplied, in half floats.
pub const ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The format a picture is sent in, before it is made premultiplied half-float.
pub const SENT_PICTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The format of a finished frame that is given back as a picture: the 8 bits that the preview shows.
pub const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The format of a finished frame that the export makes the planes of: the same numbers, in half floats, so that the brightness and the colour differences are made of
/// the picture as it is and are rounded to 8 bits once, at the end, and not twice (decision C13 of `docs/engine-phase-3-design.md`).
pub const ENCODED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// How many bytes the uniforms of the pass that puts a layer in its place take up (see `place.wgsl`).
pub const PLACEMENT_UNIFORM_BYTES: u64 = 64;

/// How many bytes the uniforms of the pass that makes a video picture red, green and blue (and the numbers of the working space) take up (see `yuv.wgsl`): the matrix, the
/// offsets with the number of the curve in the last four of their sixteen bytes, and the matrix to the working space.
pub const YUV_UNIFORM_BYTES: u64 = 128;

/// How many bytes the uniforms of the pass that makes a picture of bytes the numbers of the working space take up (see `ingest.wgsl`): a 3 by 3 matrix, three columns of 16
/// bytes, and the number of the curve, in a struct that is a multiple of 16.
pub const INGEST_UNIFORM_BYTES: u64 = 64;

/// How many bytes the uniforms of the last pass take up (see `output.wgsl`): the same as the pass before it.
pub const OUTPUT_UNIFORM_BYTES: u64 = 64;

/// How many bytes the uniforms of the effect that mixes colours take up (see `colour_matrix.wgsl`): three 3 by 3 matrices, three columns of 16 bytes each, the number
/// of the curve, the three offsets and the number that says which parts are left out, in a struct that is a multiple of 16.
pub const COLOUR_MATRIX_UNIFORM_BYTES: u64 = 176;

/// The format of the planes of a video picture, which are sent as they are.
pub const PLANE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// The format of the brightness plane that the export makes of a finished frame.
pub const LUMA_FORMAT: wgpu::TextureFormat = PLANE_FORMAT;

/// The format of the texture that the two colour difference planes of a finished frame are made side by side in (blue in the red channel, red in the green one).
pub const CHROMA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;

/// How many bytes the uniforms of the passes that make the planes of a finished frame take up (see `rgb_to_luma.wgsl`): three rows of a `vec4<f32>`.
pub const RGB_TO_YUV_UNIFORM_BYTES: u64 = 48;

/// The last pass: the accumulation picture to the finished frame, as 8 bits (for the preview) or as half floats (for the export's planes): the same shader, the same
/// bindings, two targets.
pub struct OutputPass {
    pub pipeline: wgpu::RenderPipeline,
    pub encoded_pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// The pass that makes a picture that was sent as bytes into one the passes of a frame work on.
pub struct IngestPass {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// The pass that makes a video picture in planes red, green and blue.
pub struct YuvPass {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// The two passes that make the planes of `yuv420p` of a finished frame, for the export: they read the same things (the numbers of the conversion, and the frame in
/// half floats) and write other textures.
pub struct PlanesPass {
    pub luma_pipeline: wgpu::RenderPipeline,
    pub chroma_pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// The effect that mixes the colours of a layer's picture: a picture in, a picture of the same size out.
pub struct ColourMatrixPass {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// The pass that puts a layer's picture in its place on the frame.
pub struct PlacePass {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// How layers' pictures are read: smoothly between their pixels, and the colour of the edge pixel beyond their edges.
    pub sampler: wgpu::Sampler,
}

/// Every pipeline of the engine.
pub struct Pipelines {
    pub output: OutputPass,
    pub ingest: IngestPass,
    pub yuv: YuvPass,
    pub planes: PlanesPass,
    pub colour_matrix: ColourMatrixPass,
    pub place: PlacePass,
    /// What the effects that others wrote are made from (the pipeline of each is made when its shader is given to the engine).
    pub foreign_effect: ForeignEffectPass,
}

impl Pipelines {
    pub fn create(device: &wgpu::Device) -> Self {
        Self {
            output: output_pass(device),
            ingest: ingest_pass(device),
            yuv: yuv_pass(device),
            planes: planes_pass(device),
            colour_matrix: colour_matrix_pass(device),
            place: place_pass(device),
            foreign_effect: ForeignEffectPass::create(device),
        }
    }
}

pub(super) fn shader_module(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

/// A bind group layout entry for the uniforms of a pass (`bytes` of them at a dynamic offset: see `uniform_bytes`), which only the fragment stage reads.
fn dynamic_uniform_entry(binding: u32, bytes: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: NonZeroU64::new(bytes),
        },
        count: None,
    }
}

/// A bind group layout entry for a texture that is read with `textureLoad` only.
fn loaded_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// A pipeline that draws with the shader's `vs_main` and `fs_main` into `format`, with `blend`.
fn pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    pipeline_of_two_modules(device, label, module, module, layout, format, blend)
}

/// A pipeline that draws with the `vs_main` of one shader and the `fs_main` of another (they may be the same) into `format`, with `blend`.
pub(super) fn pipeline_of_two_modules(
    device: &wgpu::Device,
    label: &str,
    vertex_module: &wgpu::ShaderModule,
    fragment_module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: vertex_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: fragment_module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
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

fn output_pass(device: &wgpu::Device) -> OutputPass {
    let module = shader_module(device, "engine output pass", shaders::OUTPUT);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine output pass bindings"),
        entries: &[dynamic_uniform_entry(0, OUTPUT_UNIFORM_BYTES), loaded_texture_entry(1)],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("engine output pass layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    OutputPass {
        pipeline: pipeline(device, "engine output pass", &module, &layout, OUTPUT_FORMAT, None),
        encoded_pipeline: pipeline(device, "engine encode pass", &module, &layout, ENCODED_FORMAT, None),
        bind_group_layout,
    }
}

fn ingest_pass(device: &wgpu::Device) -> IngestPass {
    let module = shader_module(device, "engine ingest pass", shaders::INGEST);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine ingest pass bindings"),
        entries: &[dynamic_uniform_entry(0, INGEST_UNIFORM_BYTES), loaded_texture_entry(1)],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("engine ingest pass layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    IngestPass {
        pipeline: pipeline(
            device,
            "engine ingest pass",
            &module,
            &layout,
            ACCUMULATION_FORMAT,
            None,
        ),
        bind_group_layout,
    }
}

fn yuv_pass(device: &wgpu::Device) -> YuvPass {
    let module = shader_module(device, "engine yuv pass", shaders::YUV);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine yuv pass bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(YUV_UNIFORM_BYTES),
                },
                count: None,
            },
            loaded_texture_entry(1),
            loaded_texture_entry(2),
            loaded_texture_entry(3),
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("engine yuv pass layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    YuvPass {
        pipeline: pipeline(device, "engine yuv pass", &module, &layout, ACCUMULATION_FORMAT, None),
        bind_group_layout,
    }
}

fn planes_pass(device: &wgpu::Device) -> PlanesPass {
    let luma_module = shader_module(device, "engine brightness plane pass", shaders::RGB_TO_LUMA);
    let chroma_module = shader_module(device, "engine colour difference planes pass", shaders::RGB_TO_CHROMA);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine planes pass bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(RGB_TO_YUV_UNIFORM_BYTES),
                },
                count: None,
            },
            loaded_texture_entry(1),
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("engine planes pass layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    PlanesPass {
        luma_pipeline: pipeline(
            device,
            "engine brightness plane pass",
            &luma_module,
            &layout,
            LUMA_FORMAT,
            None,
        ),
        chroma_pipeline: pipeline(
            device,
            "engine colour difference planes pass",
            &chroma_module,
            &layout,
            CHROMA_FORMAT,
            None,
        ),
        bind_group_layout,
    }
}

fn colour_matrix_pass(device: &wgpu::Device) -> ColourMatrixPass {
    let module = shader_module(device, "engine colour matrix pass", shaders::COLOUR_MATRIX);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine colour matrix pass bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(COLOUR_MATRIX_UNIFORM_BYTES),
                },
                count: None,
            },
            loaded_texture_entry(1),
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("engine colour matrix pass layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    ColourMatrixPass {
        pipeline: pipeline(
            device,
            "engine colour matrix pass",
            &module,
            &layout,
            ACCUMULATION_FORMAT,
            None,
        ),
        bind_group_layout,
    }
}

/// How a layer is laid over what is under it: its colours are premultiplied already, so they are added to what shows through; the same as the
/// window's own renderer does, so that the same layers over the same picture come to the same numbers.
const PREMULTIPLIED_OVER: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

fn place_pass(device: &wgpu::Device) -> PlacePass {
    let module = shader_module(device, "engine place pass", shaders::PLACE);
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("engine place pass bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    // (Dynamic: every layer's uniforms are in one buffer, each at an offset of its own. See `uniform_bytes`.)
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(PLACEMENT_UNIFORM_BYTES),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("engine place pass layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("engine layer sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    PlacePass {
        pipeline: pipeline(
            device,
            "engine place pass",
            &module,
            &layout,
            ACCUMULATION_FORMAT,
            Some(PREMULTIPLIED_OVER),
        ),
        bind_group_layout,
        sampler,
    }
}
