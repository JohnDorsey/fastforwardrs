//! PROOF OF CONCEPT (scratch copy only): compiles looped shaders in four ways with the real rewriter and compiler, writes the WGSL for the GPU harness, and times the compile.

use std::sync::atomic::Ordering;
use std::time::Instant;

use super::compile::compile;
use super::reader::read;
use super::rewrite::{POC_DRIVEN, POC_ONCE};

const OUT: &str = "E:/a/poc_out";

/// A loop of `k` reads of the picture whose offsets and weights use `amount` three times in every round.
fn loop_shader(k: u32, with_mask_input: bool) -> String {
    let mask_input = if with_mask_input {
        ", { \"NAME\": \"poc_mask\", \"TYPE\": \"image\" }"
    } else {
        ""
    };
    // (With the mask as an input of its own, the weight is read from it at a place that changes in every round: a read for every sample, which cannot be made once.)
    let amount = if with_mask_input {
        "(0.5 + IMG_NORM_PIXEL(poc_mask, uv + vec2((f - 0.5) * 0.01, 0.0)).r)"
    } else {
        "amount"
    };
    format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}{mask_input}, \
         {{ \"NAME\": \"amount\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5 }} ] }}*/\n\
         void main() {{\n\
             vec2 uv = isf_FragNormCoord;\n\
             vec4 sum = vec4(0.0);\n\
             float total = 0.0;\n\
             for (int i = 0; i < {k}; i++) {{\n\
                 float f = float(i) / float({k});\n\
                 float w = 1.0 + {amount} * f;\n\
                 sum += IMG_NORM_PIXEL(inputImage, uv + vec2((f - 0.5) * 0.04 * {amount}, (f - 0.5) * 0.01)) * w;\n\
                 total += w;\n\
             }}\n\
             gl_FragColor = sum / total;\n\
         }}\n"
    )
}

/// A loop in which the input is used inside branches that depend on the picture (so a compiler cannot take the read of a mask out of the loop without making it before the branch).
fn branchy_shader(k: u32, with_mask_input: bool) -> String {
    let mask_input = if with_mask_input {
        ", { \"NAME\": \"poc_mask\", \"TYPE\": \"image\" }"
    } else {
        ""
    };
    let amount = if with_mask_input {
        "(0.5 + IMG_NORM_PIXEL(poc_mask, uv + vec2((f - 0.5) * 0.01, 0.0)).r)"
    } else {
        "amount"
    };
    format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}{mask_input},          {{ \"NAME\": \"amount\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5 }} ] }}*/
         void main() {{
             vec2 uv = isf_FragNormCoord;
             vec4 sum = vec4(0.0);
             for (int i = 0; i < {k}; i++) {{
                 float f = float(i) / float({k});
                 vec4 s = IMG_NORM_PIXEL(inputImage, uv + vec2((f - 0.5) * 0.04 * {amount}, 0.0));
                 if (s.r > 0.5) {{
                     sum += s * {amount} * ({amount} + f);
                 }} else if (s.g > 0.5) {{
                     sum += s.gbra * (1.0 - {amount}) * (0.2 + {amount});
                 }} else {{
                     sum += s * 0.25;
                 }}
             }}
             gl_FragColor = sum / float({k});
         }}
"
    )
}

fn compiled_in(mode: &str, source: &str, driven: &str) -> super::compile::CompiledShader {
    *POC_DRIVEN.lock().unwrap() = driven.to_string();
    POC_ONCE.store(mode == "o", Ordering::SeqCst);
    let file = read(source).unwrap_or_else(|problem| panic!("{problem:?}"));
    let shader = compile(&file).unwrap_or_else(|problems| panic!("{mode}: {problems:?}"));
    POC_DRIVEN.lock().unwrap().clear();
    POC_ONCE.store(false, Ordering::SeqCst);
    shader
}

#[test]
fn poc_write_the_shaders_and_time_the_compile() {
    std::fs::create_dir_all(OUT).unwrap();
    let mut summary = String::new();
    for k in [8_u32, 32, 128] {
        for (mode, source, driven) in [
            ("u", loop_shader(k, false), ""),
            ("e", loop_shader(k, false), "amount"),
            ("o", loop_shader(k, false), "amount"),
            ("v", loop_shader(k, true), ""),
        ] {
            let shader = compiled_in(mode, &source, driven);
            std::fs::write(format!("{OUT}/loop{k}_{mode}.wgsl"), &shader.wgsl).unwrap();
            summary.push_str(&format!(
                "loop{k}_{mode}: images {:?}, {} bytes of wgsl, {} reads of the mask in the text\n",
                shader.image_names,
                shader.wgsl.len(),
                shader.wgsl.matches("poc_mask_tex").count().saturating_sub(1)
            ));
        }
    }
    for k in [32_u32] {
        for (mode, source, driven) in [
            ("u", branchy_shader(k, false), ""),
            ("e", branchy_shader(k, false), "amount"),
            ("o", branchy_shader(k, false), "amount"),
            ("v", branchy_shader(k, true), ""),
        ] {
            let shader = compiled_in(mode, &source, driven);
            std::fs::write(format!("{OUT}/branch{k}_{mode}.wgsl"), &shader.wgsl).unwrap();
            summary.push_str(&format!(
                "branch{k}_{mode}: images {:?}, {} bytes of wgsl
",
                shader.image_names,
                shader.wgsl.len()
            ));
        }
    }
    // The compile, timed: what a click on a button would wait for (the rewrite, naga, and the check).
    for (mode, driven) in [("u", ""), ("e", "amount"), ("o", "amount")] {
        let source = loop_shader(32, false);
        compiled_in(mode, &source, driven);
        let started = Instant::now();
        for _ in 0..40 {
            compiled_in(mode, &source, driven);
        }
        summary.push_str(&format!(
            "compile of loop32 in mode {mode}: {:.2} ms each\n",
            started.elapsed().as_secs_f64() * 1000.0 / 40.0
        ));
    }
    std::fs::write(format!("{OUT}/summary.txt"), &summary).unwrap();
    println!("{summary}");
}
