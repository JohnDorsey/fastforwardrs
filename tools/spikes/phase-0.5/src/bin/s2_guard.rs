//! S2, part 2: the loop cap in the rewriting step. Software adapter only; run it through run_guard.ps1 (kills the process by id at a hard limit).
use std::collections::HashMap;
use std::time::Instant;

use image::{Rgba, RgbaImage};
use spikes::gpu::{read_rgba8, software_gpu, upload_rgba8};
use spikes::isf;

const HEADER: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 1.0 } ] }*/\n";

fn main() {
    let name = std::env::args().nth(1).expect("usage: s2_guard <shader> <cap | none>");
    let cap_text = std::env::args().nth(2).expect("usage: s2_guard <shader> <cap | none>");
    let cap = if cap_text == "none" {
        None
    } else {
        Some(cap_text.parse::<u32>().unwrap())
    };
    let code = match name.as_str() {
        "infinite_while" => "void main() { float s = isf_FragNormCoord.x; while (true) { s = fract(s * 1.0001 + 0.37); if (amount > 1000.0) break; } gl_FragColor = vec4(s, 0.5, 0.25, 1.0); }".to_string(),
        "huge_for" => "void main() { float s = isf_FragNormCoord.x; for (int i = 0; i < 2000000000; i++) { s = fract(s * 1.0001 + 0.37); } gl_FragColor = vec4(s, 0.5, 0.25, 1.0); }".to_string(),
        "nested_1000_x_1000" => "void main() { float s = isf_FragNormCoord.x; for (int i = 0; i < 1000; i++) { for (int j = 0; j < 1000; j++) { s = fract(s * 1.0001 + 0.37); } } gl_FragColor = vec4(s, 0.5, 0.25, 1.0); }".to_string(),
        "gaussian_blur" => std::fs::read_to_string("shaders/gaussian_blur.fs").unwrap(),
        "kaleidoscope" => std::fs::read_to_string("shaders/kaleidoscope.fs").unwrap(),
        "plasma" => std::fs::read_to_string("shaders/plasma.fs").unwrap(),
        other => panic!("unknown shader {other}"),
    };
    let source = if code.starts_with("/*") {
        code
    } else {
        format!("{HEADER}{code}")
    };
    let parsed = isf::parse(&source).unwrap();
    let started = Instant::now();
    let compiled = isf::compile_with_loop_cap(&parsed, cap).unwrap_or_else(|problems| panic!("{problems:?}"));
    let loops_in_wgsl = compiled.wgsl.matches("loop {").count();
    let counters = compiled.wgsl.matches("_isf_loop_counter").count();
    println!(
        "compiled in {:?}: {loops_in_wgsl} loops in the WGSL, {counters} mentions of the counter, cap {cap:?}",
        started.elapsed()
    );
    std::fs::write(format!("out_s2_guard_{name}_{cap_text}.wgsl"), &compiled.wgsl).unwrap();
    let gpu = software_gpu();
    let picture = RgbaImage::from_fn(256, 192, |x, y| {
        Rgba([(x % 256) as u8, (y * 255 / 191) as u8, 100, 255])
    });
    let texture = upload_rgba8(&gpu, &picture);
    let mut images: HashMap<String, &wgpu::Texture> = HashMap::new();
    images.insert("inputImage".into(), &texture);
    let started = Instant::now();
    let output = isf::render(&gpu, &parsed, &compiled, &HashMap::new(), &images, (256, 192), 1.0, 0).unwrap();
    let result = read_rgba8(&gpu, &output);
    println!(
        "drawn and read back in {:?}: pixels (10,10) {:?}, (128,96) {:?}, (250,180) {:?}",
        started.elapsed(),
        result.get_pixel(10, 10).0,
        result.get_pixel(128, 96).0,
        result.get_pixel(250, 180).0
    );
    result.save(format!("out_s2_guard_{name}_{cap_text}.png")).unwrap();
}
