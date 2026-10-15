//! Probe of what naga's GLSL front end accepts, one small shader at a time, and what WGSL it writes. (No GPU.)
use naga::ShaderStage;
use naga::front::glsl::{Frontend, Options};

fn try_glsl(name: &str, source: &str, show_wgsl: bool) {
    let mut frontend = Frontend::default();
    match frontend.parse(&Options::from(ShaderStage::Fragment), source) {
        Err(errors) => {
            for error in &errors.errors {
                let location = error.location(source);
                println!(
                    "[FAIL parse] {name}: line {:?}: {}",
                    location.map(|l| (l.line_number, l.line_position)),
                    error.kind
                );
            }
        }
        Ok(module) => {
            let info =
                naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                    .validate(&module);
            match info {
                Err(error) => println!("[FAIL validate] {name}: {}", error.as_inner()),
                Ok(info) => {
                    println!("[ok] {name}");
                    if show_wgsl {
                        println!(
                            "{}",
                            naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
                                .unwrap()
                        );
                    }
                }
            }
        }
    }
}

const HEAD: &str = r#"#version 450
layout(set = 0, binding = 0) uniform U { vec2 RENDERSIZE; float TIME; int invert_raw; };
layout(set = 0, binding = 1) uniform texture2D inputImage_tex;
layout(set = 0, binding = 2) uniform sampler inputImage_samp;
layout(location = 0) out vec4 _isf_out;
"#;

fn main() {
    let show = std::env::args().any(|a| a == "--wgsl");
    try_glsl(
        "combined sampler2D global",
        "#version 450\nlayout(set=0,binding=1) uniform sampler2D img;\nlayout(location=0) out vec4 o;\nvoid main(){ o = texture(img, vec2(0.5)); }\n",
        false,
    );
    try_glsl(
        "token pasting ## in a function-like macro",
        &format!(
            "{HEAD}#define IMG(img, c) texture(sampler2D(img##_tex, img##_samp), c)\nvoid main() {{ _isf_out = IMG(inputImage, vec2(0.5)); }}\n"
        ),
        show,
    );
    try_glsl(
        "nested macros",
        &format!(
            "{HEAD}#define N(img, c) texture(sampler2D(img##_tex, img##_samp), c)\n#define P(img, c) N(img, (c) / vec2(textureSize(sampler2D(img##_tex, img##_samp), 0)))\nvoid main() {{ _isf_out = P(inputImage, vec2(3.0, 4.0)); }}\n"
        ),
        show,
    );
    try_glsl(
        "unnamed uniform block members used as plain names + macro bool",
        &format!(
            "{HEAD}#define invert (invert_raw != 0)\nvoid main() {{ vec4 c = vec4(1.0); if (invert) c = 1.0 - c; _isf_out = c * TIME; }}\n"
        ),
        false,
    );
    try_glsl(
        "macro named gl_FragColor",
        &format!(
            "{HEAD}vec4 _c;\n#define gl_FragColor _c\nvoid main() {{ gl_FragColor = vec4(1.0); _isf_out = _c; }}\n"
        ),
        false,
    );
    try_glsl(
        "int literal times float (TIME * 2)",
        &format!("{HEAD}void main() {{ float t = TIME * 2; _isf_out = vec4(t); }}\n"),
        false,
    );
    try_glsl(
        "float f = 1; (int to float initialiser)",
        &format!("{HEAD}void main() {{ float f = 1; _isf_out = vec4(f); }}\n"),
        false,
    );
    try_glsl(
        "pow(x, 2) (int literal as a float argument)",
        &format!("{HEAD}void main() {{ _isf_out = vec4(pow(TIME, 2)); }}\n"),
        false,
    );
    try_glsl(
        "precision highp float;",
        &format!("{HEAD}precision highp float;\nvoid main() {{ _isf_out = vec4(1.0); }}\n"),
        false,
    );
    try_glsl(
        "const array global with constructor",
        &format!(
            "{HEAD}const float w[3] = float[3](0.25, 0.5, 0.25);\nvoid main() {{ float s = 0.0; for (int i = 0; i < 3; i++) s += w[i]; _isf_out = vec4(s); }}\n"
        ),
        false,
    );
    try_glsl(
        "old array initialiser style: float w[3] = float[](...)",
        &format!("{HEAD}void main() {{ float w[3] = float[](0.25, 0.5, 0.25); _isf_out = vec4(w[1]); }}\n"),
        false,
    );
    try_glsl(
        "global with non-const initialiser",
        &format!("{HEAD}vec2 half_size = RENDERSIZE / 2.0;\nvoid main() {{ _isf_out = vec4(half_size, 0.0, 1.0); }}\n"),
        false,
    );
    try_glsl(
        "varying keyword",
        &format!("{HEAD}varying vec2 uv;\nvoid main() {{ _isf_out = vec4(uv, 0.0, 1.0); }}\n"),
        false,
    );
    try_glsl(
        "out parameter and user function",
        &format!(
            "{HEAD}void two(in float a, out float b) {{ b = a * 2.0; }}\nvoid main() {{ float b; two(TIME, b); _isf_out = vec4(b); }}\n"
        ),
        false,
    );
    try_glsl(
        "mat2 and rotation",
        &format!(
            "{HEAD}void main() {{ float a = TIME; mat2 m = mat2(cos(a), -sin(a), sin(a), cos(a)); vec2 p = m * vec2(1.0, 0.0); _isf_out = vec4(p, 0.0, 1.0); }}\n"
        ),
        false,
    );
    try_glsl(
        "atan(y, x), mod, fract, smoothstep, mix with bool-free math",
        &format!(
            "{HEAD}void main() {{ vec2 p = gl_FragCoord.xy / RENDERSIZE - 0.5; float a = atan(p.y, p.x); float r = length(p); _isf_out = vec4(mod(a, 1.0), fract(r * 4.0), smoothstep(0.1, 0.4, r), 1.0); }}\n"
        ),
        false,
    );
    try_glsl(
        "for loop with float counter",
        &format!(
            "{HEAD}void main() {{ float s = 0.0; for (float i = 0.0; i < 8.0; i += 1.0) s += sin(i * TIME); _isf_out = vec4(s); }}\n"
        ),
        false,
    );
    try_glsl(
        "while loop with break",
        &format!(
            "{HEAD}void main() {{ float s = 0.0; int i = 0; while (true) {{ s += 0.1; i++; if (i > 10) break; }} _isf_out = vec4(s); }}\n"
        ),
        false,
    );
    try_glsl(
        "switch statement",
        &format!(
            "{HEAD}void main() {{ int k = invert_raw; vec4 c = vec4(0.0); switch (k) {{ case 0: c = vec4(1.0); break; case 1: c = vec4(0.5); break; default: break; }} _isf_out = c; }}\n"
        ),
        false,
    );
    try_glsl(
        "discard",
        &format!("{HEAD}void main() {{ if (TIME > 100.0) discard; _isf_out = vec4(1.0); }}\n"),
        false,
    );
    try_glsl(
        "ternary operator",
        &format!("{HEAD}void main() {{ float a = TIME > 1.0 ? 1.0 : 0.0; _isf_out = vec4(a); }}\n"),
        false,
    );
    try_glsl(
        "ivec2 / int arithmetic / int division and modulo",
        &format!(
            "{HEAD}void main() {{ ivec2 c = ivec2(gl_FragCoord.xy); int k = (c.x + c.y) % 2; int d = c.x / 3; _isf_out = vec4(float(k), float(d), 0.0, 1.0); }}\n"
        ),
        false,
    );
    try_glsl(
        "swizzle assignment on out colour (rgb = ...)",
        &format!("{HEAD}void main() {{ _isf_out = vec4(0.0); _isf_out.rgb = vec3(1.0); _isf_out.a = 1.0; }}\n"),
        false,
    );
    try_glsl(
        "texelFetch",
        &format!(
            "{HEAD}void main() {{ _isf_out = texelFetch(sampler2D(inputImage_tex, inputImage_samp), ivec2(1, 2), 0); }}\n"
        ),
        false,
    );
    try_glsl(
        "textureLod",
        &format!(
            "{HEAD}void main() {{ _isf_out = textureLod(sampler2D(inputImage_tex, inputImage_samp), vec2(0.5), 0.0); }}\n"
        ),
        false,
    );
    try_glsl(
        "dFdx / dFdy / fwidth",
        &format!("{HEAD}void main() {{ _isf_out = vec4(fwidth(TIME)); }}\n"),
        false,
    );
    try_glsl(
        "mod() with int literal argument",
        &format!("{HEAD}void main() {{ _isf_out = vec4(mod(TIME, 2)); }}\n"),
        false,
    );
    try_glsl(
        "vec3(1) integer constructor argument",
        &format!("{HEAD}void main() {{ _isf_out = vec4(vec3(1), 1); }}\n"),
        false,
    );
    try_glsl(
        "min(x, 1) / max(x, 0) / clamp with ints",
        &format!("{HEAD}void main() {{ _isf_out = vec4(clamp(TIME, 0, 1), min(TIME, 1), max(TIME, 0), 1.0); }}\n"),
        false,
    );
    try_glsl(
        "bool uniform in the block (not allowed in Vulkan GLSL)",
        "#version 450\nlayout(set=0,binding=0) uniform U { bool flag; };\nlayout(location=0) out vec4 o;\nvoid main(){ o = vec4(flag ? 1.0 : 0.0); }\n",
        false,
    );
    try_glsl(
        "struct",
        &format!(
            "{HEAD}struct P {{ vec2 a; float b; }};\nvoid main() {{ P p = P(vec2(1.0), 2.0); _isf_out = vec4(p.a, p.b, 1.0); }}\n"
        ),
        false,
    );
    try_glsl(
        "#define constant and function-like macro",
        &format!(
            "{HEAD}#define PI 3.14159265\n#define SQ(x) ((x)*(x))\nvoid main() {{ _isf_out = vec4(SQ(sin(PI * TIME))); }}\n"
        ),
        false,
    );
    try_glsl(
        "#if / #ifdef",
        &format!(
            "{HEAD}#define FAST 1\nvoid main() {{\n#ifdef FAST\n _isf_out = vec4(1.0);\n#else\n _isf_out = vec4(0.0);\n#endif\n}}\n"
        ),
        false,
    );
    try_glsl(
        "hash noise with fract(sin(dot()))",
        &format!(
            "{HEAD}float rand(vec2 co) {{ return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453); }}\nvoid main() {{ _isf_out = vec4(rand(gl_FragCoord.xy)); }}\n"
        ),
        false,
    );
    try_glsl("texture() inside a branch on a NON-uniform value (derivative-taking sample, as in ISF files that write texture2D inside an if)", &format!("{HEAD}void main() {{ vec4 c = vec4(0.0); if (gl_FragCoord.x > 10.0) {{ c = texture(sampler2D(inputImage_tex, inputImage_samp), vec2(0.5)); }} _isf_out = c; }}
"), false);
    try_glsl("textureLod() inside the same branch", &format!("{HEAD}void main() {{ vec4 c = vec4(0.0); if (gl_FragCoord.x > 10.0) {{ c = textureLod(sampler2D(inputImage_tex, inputImage_samp), vec2(0.5), 0.0); }} _isf_out = c; }}
"), false);
    try_glsl(
        "recursion (not legal in GLSL)",
        &format!(
            "{HEAD}float f(float x) {{ return x > 0.0 ? f(x - 1.0) : 0.0; }}\nvoid main() {{ _isf_out = vec4(f(TIME)); }}\n"
        ),
        false,
    );
    try_glsl(
        "infinite loop",
        &format!("{HEAD}void main() {{ float s = 0.0; while (true) {{ s += 1.0; }} _isf_out = vec4(s); }}\n"),
        false,
    );
}
