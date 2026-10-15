//! S1: an ISF file -> a small rewriting step -> naga (GLSL in, WGSL out) -> wgpu on a software adapter -> a picture. Throwaway spike code.
use std::collections::HashMap;

use naga::front::glsl::{Frontend, Options};
use serde_json::Value;

use crate::gpu::{Gpu, VERTEX_WGSL, make_texture};

#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    Float,
    Bool,
    Long,
    Color,
    Point2D,
    Image,
    Event,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct IsfInput {
    pub name: String,
    pub kind: InputKind,
    pub default: Option<Value>,
}

#[derive(Debug)]
pub struct Isf {
    pub header: Value,
    pub inputs: Vec<IsfInput>,
    pub passes: Vec<Value>,
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub enum EffectShape {
    Generator,
    Filter,
    Transition,
}

#[derive(Debug)]
pub struct Problem {
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
}

/// Reads the JSON header (the first `/*{ ... }*/` comment) and keeps the rest. The body keeps its line count (the comment becomes empty lines), so that a line
/// number from the compiler is a line number of the file.
pub fn parse(source: &str) -> Result<Isf, String> {
    let start = source.find("/*").ok_or("no header comment")?;
    let end = source[start..].find("*/").ok_or("the header comment is not closed")? + start;
    let json_text = source[start + 2..end].trim();
    let header: Value = serde_json::from_str(json_text).map_err(|error| format!("the header is not JSON: {error}"))?;
    let mut inputs = Vec::new();
    for input in header.get("INPUTS").and_then(Value::as_array).into_iter().flatten() {
        let name = input
            .get("NAME")
            .and_then(Value::as_str)
            .ok_or("an input has no NAME")?
            .to_string();
        let kind = match input.get("TYPE").and_then(Value::as_str).unwrap_or("") {
            "float" => InputKind::Float,
            "bool" => InputKind::Bool,
            "long" => InputKind::Long,
            "color" => InputKind::Color,
            "point2D" => InputKind::Point2D,
            "image" => InputKind::Image,
            "event" => InputKind::Event,
            other => InputKind::Other(other.to_string()),
        };
        inputs.push(IsfInput {
            name,
            kind,
            default: input.get("DEFAULT").cloned(),
        });
    }
    let passes = header
        .get("PASSES")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let blanked: String = source[start..end + 2]
        .chars()
        .map(|c| if c == '\n' { '\n' } else { ' ' })
        .collect();
    let body = format!("{}{}{}", &source[..start], blanked, &source[end + 2..]);
    Ok(Isf {
        header,
        inputs,
        passes,
        body,
    })
}

impl Isf {
    pub fn shape(&self) -> EffectShape {
        let has = |name: &str| {
            self.inputs
                .iter()
                .any(|input| input.name == name && input.kind == InputKind::Image)
        };
        if has("startImage") && has("endImage") {
            EffectShape::Transition
        } else if has("inputImage") {
            EffectShape::Filter
        } else {
            EffectShape::Generator
        }
    }

    /// Every name that is an image in the shader: the image inputs and the targets of the passes.
    pub fn image_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .inputs
            .iter()
            .filter(|input| input.kind == InputKind::Image)
            .map(|input| input.name.clone())
            .collect();
        for pass in &self.passes {
            if let Some(target) = pass.get("TARGET").and_then(Value::as_str) {
                if !names.iter().any(|name| name == target) {
                    names.push(target.to_string());
                }
            }
        }
        names
    }
}

const IMAGE_CALLS: [&str; 7] = [
    "IMG_PIXEL",
    "IMG_NORM_PIXEL",
    "IMG_THIS_PIXEL",
    "IMG_THIS_NORM_PIXEL",
    "IMG_SIZE",
    "texture2D",
    "texture2DRect",
];

fn is_identifier_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `IMG_NORM_PIXEL(inputImage, uv)` becomes `_isf_IMG_NORM_PIXEL_inputImage(uv)` (naga's preprocessor has no `##`, so the macros of ISF cannot be written
/// as macros). Anything else about the call is left as the author wrote it.
fn rewrite_image_calls(body: &str, image_names: &[String]) -> (String, usize) {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len() + 256);
    let mut rewritten = 0;
    let mut i = 0;
    while i < chars.len() {
        if is_identifier_character(chars[i]) && (i == 0 || !is_identifier_character(chars[i - 1])) {
            let mut j = i;
            while j < chars.len() && is_identifier_character(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            if IMAGE_CALLS.contains(&word.as_str()) {
                let mut k = j;
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                if k < chars.len() && chars[k] == '(' {
                    k += 1;
                    while k < chars.len() && chars[k].is_whitespace() {
                        k += 1;
                    }
                    let name_start = k;
                    while k < chars.len() && is_identifier_character(chars[k]) {
                        k += 1;
                    }
                    let image: String = chars[name_start..k].iter().collect();
                    if image_names.contains(&image) {
                        let mut after = k;
                        while after < chars.len() && chars[after].is_whitespace() {
                            after += 1;
                        }
                        if after < chars.len() && (chars[after] == ',' || chars[after] == ')') {
                            out.push_str(&format!("_isf_{word}_{image}("));
                            i = if chars[after] == ',' { after + 1 } else { after };
                            rewritten += 1;
                            continue;
                        }
                    }
                }
            }
            out.push_str(&word);
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, rewritten)
}

fn replace_word(text: &str, word: &str, with: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let target: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let boundary_before = i == 0 || !is_identifier_character(chars[i - 1]);
        if boundary_before
            && chars[i..].starts_with(&target)
            && chars.get(i + target.len()).is_none_or(|c| !is_identifier_character(*c))
        {
            out.push_str(with);
            i += target.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub struct Rewritten {
    pub glsl: String,
    /// Lines the generated prelude takes: a compiler line number minus this is a line of the ISF file.
    pub prelude_lines: u32,
    pub notes: Vec<String>,
}

pub fn rewrite(isf: &Isf) -> Rewritten {
    let image_names = isf.image_names();
    let mut notes = Vec::new();
    let mut prelude = String::new();
    prelude.push_str("#version 450\n");
    prelude.push_str("layout(set = 0, binding = 0) uniform IsfUniforms {\n    vec2 RENDERSIZE;\n    float TIME;\n    float TIMEDELTA;\n    int PASSINDEX;\n    int FRAMEINDEX;\n    vec4 DATE;\n");
    let mut bool_macros = String::new();
    for input in &isf.inputs {
        match input.kind {
            InputKind::Float => prelude.push_str(&format!("    float {};\n", input.name)),
            InputKind::Long => prelude.push_str(&format!("    int {};\n", input.name)),
            InputKind::Color => prelude.push_str(&format!("    vec4 {};\n", input.name)),
            InputKind::Point2D => prelude.push_str(&format!("    vec2 {};\n", input.name)),
            InputKind::Bool | InputKind::Event => {
                prelude.push_str(&format!("    int _isf_b_{};\n", input.name));
                bool_macros.push_str(&format!("#define {0} (_isf_b_{0} != 0)\n", input.name));
            }
            InputKind::Image => {}
            InputKind::Other(ref other) => notes.push(format!("input {} of kind {other} is not supported", input.name)),
        }
    }
    prelude.push_str("};\n");
    prelude.push_str(&bool_macros);
    for (k, name) in image_names.iter().enumerate() {
        prelude.push_str(&format!(
            "layout(set = 0, binding = {}) uniform texture2D {name}_tex;\n",
            1 + 2 * k
        ));
        prelude.push_str(&format!(
            "layout(set = 0, binding = {}) uniform sampler {name}_samp;\n",
            2 + 2 * k
        ));
    }
    prelude.push_str("vec2 isf_FragNormCoord;\n#define vv_FragNormCoord isf_FragNormCoord\nvec4 _isf_color;\n#define gl_FragColor _isf_color\n");
    for name in &image_names {
        // textureLod, never texture: the derivative-taking one must be in uniform control flow, and ISF shaders sample inside branches all the time.
        prelude.push_str(&format!(
            "vec4 _isf_IMG_NORM_PIXEL_{n}(vec2 c) {{ return textureLod(sampler2D({n}_tex, {n}_samp), c, 0.0); }}\n\
             vec2 _isf_IMG_SIZE_{n}() {{ return vec2(textureSize(sampler2D({n}_tex, {n}_samp), 0)); }}\n\
             vec4 _isf_IMG_PIXEL_{n}(vec2 c) {{ return _isf_IMG_NORM_PIXEL_{n}(c / _isf_IMG_SIZE_{n}()); }}\n\
             vec4 _isf_IMG_THIS_PIXEL_{n}() {{ return _isf_IMG_NORM_PIXEL_{n}(isf_FragNormCoord); }}\n\
             vec4 _isf_IMG_THIS_NORM_PIXEL_{n}() {{ return _isf_IMG_NORM_PIXEL_{n}(isf_FragNormCoord); }}\n\
             vec4 _isf_texture2D_{n}(vec2 c) {{ return _isf_IMG_NORM_PIXEL_{n}(c); }}\n\
             vec4 _isf_texture2DRect_{n}(vec2 c) {{ return _isf_IMG_PIXEL_{n}(c); }}\n",
            n = name
        ));
    }
    let prelude_lines = prelude.matches('\n').count() as u32;

    let (mut body, count) = rewrite_image_calls(&isf.body, &image_names);
    if count > 0 {
        notes.push(format!("{count} image calls rewritten"));
    }
    // Lines that Vulkan-style GLSL cannot have.
    let mut lines: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#version") {
            notes.push("a #version line was removed".to_string());
            lines.push(String::new());
        } else if trimmed.starts_with("varying ") || trimmed.starts_with("uniform ") {
            // A varying of the author's own (no vertex shader gives it a value here) or a uniform declaration that the header already has.
            notes.push(format!(
                "`{}` was turned into a plain global",
                trimmed.split(';').next().unwrap_or("")
            ));
            lines.push(line.replacen("varying ", "", 1).replacen("uniform ", "", 1));
        } else {
            lines.push(line.to_string());
        }
    }
    body = lines.join("\n");
    body = replace_word(&body, "main", "_isf_user_main");
    let mut glsl = prelude;
    glsl.push_str(&body);
    glsl.push_str("\nlayout(location = 0) out vec4 _isf_out;\nvoid main() {\n    isf_FragNormCoord = gl_FragCoord.xy / RENDERSIZE;\n    _isf_color = vec4(0.0);\n    _isf_user_main();\n    _isf_out = _isf_color;\n}\n");
    Rewritten {
        glsl,
        prelude_lines,
        notes,
    }
}

#[derive(Debug, Clone)]
pub struct Member {
    pub name: String,
    pub offset: u32,
    pub width: u32,
}

pub struct Compiled {
    pub rewritten: Rewritten,
    pub wgsl: String,
    pub members: Vec<Member>,
    pub uniform_size: u32,
}

pub fn compile(isf: &Isf) -> Result<Compiled, Vec<Problem>> {
    compile_with_loop_cap(isf, None)
}

/// `loop_cap`: every loop of the shader leaves after that many passes (see guard.rs).
pub fn compile_with_loop_cap(isf: &Isf, loop_cap: Option<u32>) -> Result<Compiled, Vec<Problem>> {
    let rewritten = rewrite(isf);
    let mut frontend = Frontend::default();
    let mut module = match frontend.parse(&Options::from(naga::ShaderStage::Fragment), &rewritten.glsl) {
        Ok(module) => module,
        Err(errors) => {
            return Err(errors
                .errors
                .iter()
                .map(|error| {
                    let location = error.location(&rewritten.glsl);
                    Problem {
                        line: location.map(|l| l.line_number.saturating_sub(rewritten.prelude_lines)),
                        column: location.map(|l| l.line_position),
                        message: error.kind.to_string(),
                    }
                })
                .collect());
        }
    };
    if let Some(cap) = loop_cap {
        crate::guard::cap_loops(&mut module, cap);
    }
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .map_err(|error| {
            let mut cause = error.as_inner().to_string();
            let mut source = std::error::Error::source(error.as_inner());
            while let Some(next) = source {
                cause = format!("{cause}: {next}");
                source = next.source();
            }
            let mut problems: Vec<Problem> = error
                .spans()
                .map(|(span, label)| {
                    let location = span.location(&rewritten.glsl);
                    Problem {
                        line: Some(location.line_number.saturating_sub(rewritten.prelude_lines)),
                        column: Some(location.line_position),
                        message: format!("{cause} ({label})"),
                    }
                })
                .collect();
            if problems.is_empty() {
                problems.push(Problem {
                    line: None,
                    column: None,
                    message: format!("validation: {}", error.as_inner()),
                });
            }
            problems
        })?;
    let mut members = Vec::new();
    let mut uniform_size = 0;
    for (_, global) in module.global_variables.iter() {
        if global.space == naga::AddressSpace::Uniform {
            if let naga::TypeInner::Struct {
                members: struct_members,
                span,
            } = &module.types[global.ty].inner
            {
                uniform_size = *span;
                for member in struct_members {
                    let width = match &module.types[member.ty].inner {
                        naga::TypeInner::Scalar(_) => 4,
                        naga::TypeInner::Vector { size, .. } => 4 * *size as u32,
                        other => panic!("unexpected uniform member type {other:?}"),
                    };
                    members.push(Member {
                        name: member.name.clone().unwrap_or_default(),
                        offset: member.offset,
                        width,
                    });
                }
            }
        }
    }
    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty()).map_err(|error| {
            vec![Problem {
                line: None,
                column: None,
                message: format!("wgsl writer: {error}"),
            }]
        })?;
    Ok(Compiled {
        rewritten,
        wgsl,
        members,
        uniform_size,
    })
}

fn numbers(value: &Value) -> Vec<f32> {
    match value {
        Value::Array(items) => items.iter().filter_map(Value::as_f64).map(|x| x as f32).collect(),
        Value::Number(number) => vec![number.as_f64().unwrap() as f32],
        Value::Bool(flag) => vec![if *flag { 1.0 } else { 0.0 }],
        _ => vec![],
    }
}

/// Packs the values of one pass into the uniform block, by the offsets naga gave the members.
pub fn pack_uniforms(
    isf: &Isf,
    compiled: &Compiled,
    overrides: &HashMap<String, Value>,
    size: (u32, u32),
    time: f32,
    pass_index: i32,
    frame_index: i32,
) -> Vec<u8> {
    let mut bytes = vec![0u8; compiled.uniform_size.max(16) as usize];
    let mut put_floats = |name: &str, values: &[f32]| {
        if let Some(member) = compiled.members.iter().find(|member| member.name == name) {
            for (k, value) in values.iter().take((member.width / 4) as usize).enumerate() {
                let at = member.offset as usize + 4 * k;
                bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    };
    put_floats("RENDERSIZE", &[size.0 as f32, size.1 as f32]);
    put_floats("TIME", &[time]);
    put_floats("TIMEDELTA", &[1.0 / 30.0]);
    let mut ints: Vec<(String, i32)> = vec![("PASSINDEX".into(), pass_index), ("FRAMEINDEX".into(), frame_index)];
    for input in &isf.inputs {
        let value = overrides.get(&input.name).or(input.default.as_ref());
        let Some(value) = value else { continue };
        match input.kind {
            InputKind::Float | InputKind::Color | InputKind::Point2D => put_floats(&input.name, &numbers(value)),
            InputKind::Long => ints.push((
                input.name.clone(),
                numbers(value).first().copied().unwrap_or(0.0) as i32,
            )),
            InputKind::Bool | InputKind::Event => ints.push((
                format!("_isf_b_{}", input.name),
                numbers(value).first().copied().unwrap_or(0.0) as i32,
            )),
            _ => {}
        }
    }
    for (name, value) in ints {
        if let Some(member) = compiled.members.iter().find(|member| member.name == name) {
            let at = member.offset as usize;
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

/// Renders every pass of a shader and returns the texture of the last one. `images` gives the textures of the image inputs; pictures are in GL orientation
/// (row 0 is the BOTTOM row) the whole way, so nothing in the shader needs flipping.
pub fn render(
    gpu: &Gpu,
    isf: &Isf,
    compiled: &Compiled,
    overrides: &HashMap<String, Value>,
    images: &HashMap<String, &wgpu::Texture>,
    size: (u32, u32),
    time: f32,
    frame_index: i32,
) -> Result<wgpu::Texture, String> {
    render_with_buffers(gpu, isf, compiled, overrides, images, size, time, frame_index).map(|(output, _)| output)
}

#[allow(clippy::type_complexity)]
pub fn render_with_buffers(
    gpu: &Gpu,
    isf: &Isf,
    compiled: &Compiled,
    overrides: &HashMap<String, Value>,
    images: &HashMap<String, &wgpu::Texture>,
    size: (u32, u32),
    time: f32,
    frame_index: i32,
) -> Result<(wgpu::Texture, HashMap<String, wgpu::Texture>), String> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let vertex = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vertex"),
        source: wgpu::ShaderSource::Wgsl(VERTEX_WGSL.into()),
    });
    let fragment = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("isf"),
        source: wgpu::ShaderSource::Wgsl(compiled.wgsl.as_str().into()),
    });
    // An EXPLICIT layout: with `layout: None` wgpu leaves out the bindings that the shader does not use, and the bind group then has to be made differently
    // for every shader.
    let image_count = isf.image_names().len() as u32;
    let mut layout_entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: None,
        },
        count: None,
    }];
    for k in 0..image_count {
        layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 1 + 2 * k,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 2 + 2 * k,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
    }
    let bind_group_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("isf bindings"),
        entries: &layout_entries,
    });
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("isf pipeline"),
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
    if let Some(error) = pollster::block_on(scope.pop()) {
        return Err(format!("pipeline: {error}"));
    }
    let image_names = isf.image_names();
    let dummy = make_texture(gpu, 1, 1, format);
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // The textures behind the targets of the passes (named buffers).
    let mut targets: HashMap<String, wgpu::Texture> = HashMap::new();
    for pass in &isf.passes {
        if let Some(name) = pass.get("TARGET").and_then(Value::as_str) {
            targets
                .entry(name.to_string())
                .or_insert_with(|| make_texture(gpu, size.0, size.1, format));
        }
    }
    let pass_count = isf.passes.len().max(1);
    let output = make_texture(gpu, size.0, size.1, format);
    // ONE buffer for the uniforms of every pass, each pass at its own 256-byte offset, bound with a DYNAMIC offset. (On WARP, wgpu 30.0.1 gives two draws that use
    // different statically bound uniform buffers the data of the LAST one: dynamic offsets are root descriptors there and are right: see race_repro.rs.)
    let uniform_stride = 256u64;
    let uniform_arena = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("isf uniforms"),
        size: uniform_stride * pass_count as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for pass_index in 0..pass_count {
        let target_name = isf
            .passes
            .get(pass_index)
            .and_then(|pass| pass.get("TARGET"))
            .and_then(Value::as_str);
        let is_last = pass_index + 1 == pass_count;
        let target_texture: &wgpu::Texture = match (target_name, is_last) {
            (Some(name), false) => &targets[name],
            _ => &output,
        };
        let uniform_bytes = pack_uniforms(isf, compiled, overrides, size, time, pass_index as i32, frame_index);
        assert!(
            uniform_bytes.len() as u64 <= uniform_stride,
            "the uniforms of a shader are larger than {uniform_stride} bytes"
        );
        gpu.queue
            .write_buffer(&uniform_arena, uniform_stride * pass_index as u64, &uniform_bytes);
        let views: Vec<wgpu::TextureView> = image_names
            .iter()
            .map(|name| {
                let texture: &wgpu::Texture = if Some(name.as_str()) == target_name && !is_last {
                    &dummy
                } else if let Some(texture) = targets.get(name) {
                    texture
                } else if let Some(texture) = images.get(name) {
                    texture
                } else {
                    &dummy
                };
                texture.create_view(&Default::default())
            })
            .collect();
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &uniform_arena,
                offset: 0,
                size: std::num::NonZeroU64::new(uniform_bytes.len().max(16) as u64),
            }),
        }];
        for (k, view) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 1 + 2 * k as u32,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 2 + 2 * k as u32,
                resource: wgpu::BindingResource::Sampler(&sampler),
            });
        }
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &entries,
        });
        let target_view = target_texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            render_pass.set_pipeline(&pipeline);
            render_pass.set_bind_group(0, &bind_group, &[(uniform_stride * pass_index as u64) as u32]);
            render_pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(format!("pass {pass_index}: {error}"));
        }
    }
    Ok((output, targets))
}
