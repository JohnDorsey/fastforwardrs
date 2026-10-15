//! S1: sample ISF shaders through the rewriting step and naga to pictures, on the software adapter only.
use std::collections::HashMap;
use std::time::Instant;

use image::{Rgba, RgbaImage};
use serde_json::{Value, json};
use spikes::gpu::{read_rgba8, software_gpu, upload_rgba8};
use spikes::isf;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 192;

/// A test picture in DISPLAY orientation (row 0 on top): a gradient, a red square at the top left, a blue one at the bottom right, a white diagonal and a checker.
fn test_picture(variant: u32) -> RgbaImage {
    RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let (u, v_down) = (x as f32 / WIDTH as f32, y as f32 / HEIGHT as f32);
        let mut c = [u, 1.0 - v_down, 0.35 + 0.3 * (variant as f32), 1.0];
        if variant == 1 {
            c = [1.0 - u, v_down, 0.2, 1.0];
        }
        if x < 40 && y < 40 {
            c = [1.0, 0.0, 0.0, 1.0];
        }
        if x >= WIDTH - 40 && y >= HEIGHT - 40 {
            c = [0.0, 0.0, 1.0, 1.0];
        }
        if (x as i32 - y as i32 * 4 / 3).abs() < 3 {
            c = [1.0, 1.0, 1.0, 1.0];
        }
        if (100..140).contains(&x) && (60..100).contains(&y) && ((x / 8 + y / 8) % 2 == 0) {
            c = [0.1, 0.1, 0.1, 1.0];
        }
        Rgba(c.map(|channel| (channel.clamp(0.0, 1.0) * 255.0 + 0.5) as u8))
    })
}

/// Flips the rows: the pictures inside the renderer are in GL orientation (row 0 is the bottom row).
fn flip(image: &RgbaImage) -> RgbaImage {
    image::imageops::flip_vertical(image)
}

fn main() {
    let folder = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: s1_isf <output folder> <shader folder>"),
    );
    let shaders = std::path::PathBuf::from(
        std::env::args()
            .nth(2)
            .expect("usage: s1_isf <output folder> <shader folder>"),
    );
    std::fs::create_dir_all(&folder).unwrap();
    let gpu = software_gpu();
    println!(
        "adapter: {} ({:?}, {:?}, driver {} {})",
        gpu.info.name, gpu.info.backend, gpu.info.device_type, gpu.info.driver, gpu.info.driver_info
    );

    let picture_a = test_picture(0);
    let picture_b = test_picture(1);
    picture_a.save(folder.join("input_a.png")).unwrap();
    picture_b.save(folder.join("input_b.png")).unwrap();
    let texture_a = upload_rgba8(&gpu, &flip(&picture_a));
    let texture_b = upload_rgba8(&gpu, &flip(&picture_b));

    let mut names: Vec<_> = std::fs::read_dir(&shaders)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "fs"))
        .collect();
    names.sort();
    let mut sheet_entries: Vec<(String, RgbaImage)> = vec![
        ("input_a".to_string(), picture_a.clone()),
        ("input_b".to_string(), picture_b.clone()),
    ];
    for path in &names {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).unwrap();
        let parsed = match isf::parse(&source) {
            Ok(parsed) => parsed,
            Err(error) => {
                println!("[{name}] HEADER ERROR: {error}");
                continue;
            }
        };
        let started = Instant::now();
        let compiled = match isf::compile(&parsed) {
            Ok(compiled) => compiled,
            Err(problems) => {
                println!(
                    "[{name}] shape {:?}: COMPILE FAILED ({:?}):",
                    parsed.shape(),
                    started.elapsed()
                );
                for problem in problems {
                    println!(
                        "    line {:?} column {:?}: {}",
                        problem.line, problem.column, problem.message
                    );
                }
                continue;
            }
        };
        let compile_time = started.elapsed();
        std::fs::write(folder.join(format!("{name}.glsl")), &compiled.rewritten.glsl).unwrap();
        std::fs::write(folder.join(format!("{name}.wgsl")), &compiled.wgsl).unwrap();
        let mut images: HashMap<String, &wgpu::Texture> = HashMap::new();
        match parsed.shape() {
            isf::EffectShape::Transition => {
                images.insert("startImage".into(), &texture_a);
                images.insert("endImage".into(), &texture_b);
            }
            _ => {
                images.insert("inputImage".into(), &texture_a);
            }
        }
        let mut overrides: HashMap<String, Value> = HashMap::new();
        if parsed.shape() == isf::EffectShape::Transition {
            overrides.insert("progress".into(), json!(0.5));
        }
        let started = Instant::now();
        match isf::render(&gpu, &parsed, &compiled, &overrides, &images, (WIDTH, HEIGHT), 1.25, 0) {
            Ok(texture) => {
                let picture = flip(&read_rgba8(&gpu, &texture));
                picture.save(folder.join(format!("{name}.png"))).unwrap();
                println!(
                    "[{name}] shape {:?}: ok, compiled in {:?}, drawn+read in {:?}; notes: {:?}",
                    parsed.shape(),
                    compile_time,
                    started.elapsed(),
                    compiled.rewritten.notes
                );
                sheet_entries.push((name, picture));
            }
            Err(error) => println!("[{name}] RENDER ERROR: {error}"),
        }
    }

    // A contact sheet of everything, 4 per row.
    let columns = 4u32;
    let rows = (sheet_entries.len() as u32).div_ceil(columns);
    let mut sheet = RgbaImage::from_pixel(
        columns * (WIDTH + 8) + 8,
        rows * (HEIGHT + 8) + 8,
        Rgba([40, 40, 40, 255]),
    );
    for (k, (_, picture)) in sheet_entries.iter().enumerate() {
        let (cx, cy) = (k as u32 % columns, k as u32 / columns);
        image::imageops::replace(
            &mut sheet,
            picture,
            i64::from(8 + cx * (WIDTH + 8)),
            i64::from(8 + cy * (HEIGHT + 8)),
        );
    }
    sheet.save(folder.join("contact_sheet.png")).unwrap();
    println!(
        "contact sheet order: {:?}",
        sheet_entries.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>()
    );

    // ---- Checks of VALUES, not looks ----
    let run = |source: &str, overrides: HashMap<String, Value>, input: &wgpu::Texture| -> RgbaImage {
        let parsed = isf::parse(source).unwrap();
        let compiled = isf::compile(&parsed).unwrap_or_else(|problems| panic!("{problems:?}"));
        let mut images: HashMap<String, &wgpu::Texture> = HashMap::new();
        images.insert("inputImage".into(), input);
        let texture = isf::render(&gpu, &parsed, &compiled, &overrides, &images, (WIDTH, HEIGHT), 0.0, 0).unwrap();
        flip(&read_rgba8(&gpu, &texture))
    };
    let largest_difference = |a: &RgbaImage, b: &RgbaImage| -> (u8, f64) {
        let mut worst = 0u8;
        let mut total = 0u64;
        for (p, q) in a.pixels().zip(b.pixels()) {
            for k in 0..4 {
                let d = p.0[k].abs_diff(q.0[k]);
                worst = worst.max(d);
                total += u64::from(d);
            }
        }
        (worst, total as f64 / f64::from(WIDTH * HEIGHT * 4))
    };

    // 1. A pass-through filter gives the input back: orientation and sampling agree.
    let passthrough = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_NORM_PIXEL(inputImage); }\n";
    let out = run(passthrough, HashMap::new(), &texture_a);
    let (worst, mean) = largest_difference(&out, &picture_a);
    println!("CHECK pass-through (IMG_THIS_NORM_PIXEL) vs input: largest difference {worst} of 255, mean {mean:.4}");

    // 2. The orientation of isf_FragNormCoord and gl_FragCoord in the picture the user sees: y is UP (GL), so the TOP row is green = 1.
    let orientation = "/*{ \"INPUTS\": [] }*/\nvoid main() { gl_FragColor = vec4(isf_FragNormCoord.x, isf_FragNormCoord.y, gl_FragCoord.y / RENDERSIZE.y, 1.0); }\n";
    let out = run(orientation, HashMap::new(), &texture_a);
    let top_left = out.get_pixel(0, 0).0;
    let bottom_left = out.get_pixel(0, HEIGHT - 1).0;
    let top_right = out.get_pixel(WIDTH - 1, 0).0;
    println!(
        "CHECK orientation of isf_FragNormCoord / gl_FragCoord in the displayed picture: top-left {top_left:?}, bottom-left {bottom_left:?}, top-right {top_right:?} (GL: red grows to the right, green and blue grow UP)"
    );

    // 3. color_controls against a CPU reference, default and non-default values (this checks the uniform packing, bool as int, a colour and the maths).
    let color_controls = std::fs::read_to_string(shaders.join("color_controls.fs")).unwrap();
    for (label, values) in [
        ("defaults", json!({})),
        (
            "brightness 0.1, contrast 1.5, saturation 0.3",
            json!({"brightness": 0.1, "contrast": 1.5, "saturation": 0.3}),
        ),
        (
            "invert, tint (1, 0.5, 0.25, 0.5)",
            json!({"invert": true, "tint": [1.0, 0.5, 0.25, 0.5]}),
        ),
    ] {
        let overrides: HashMap<String, Value> = values
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let get = |name: &str, default: f64| values.get(name).and_then(Value::as_f64).unwrap_or(default) as f32;
        let (brightness, contrast, saturation) = (get("brightness", 0.0), get("contrast", 1.0), get("saturation", 1.0));
        let invert = values.get("invert").and_then(Value::as_bool).unwrap_or(false);
        let tint: Vec<f32> = values
            .get("tint")
            .map(|t| {
                t.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect()
            })
            .unwrap_or(vec![1.0; 4]);
        let expected = RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
            let p = picture_a.get_pixel(x, y).0;
            let mut rgb = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
            for c in &mut rgb {
                *c += brightness;
                *c = (*c - 0.5) * contrast + 0.5;
            }
            let grey = rgb[0] * 0.2125 + rgb[1] * 0.7154 + rgb[2] * 0.0721;
            for c in &mut rgb {
                *c = grey + (*c - grey) * saturation;
                if invert {
                    *c = 1.0 - *c;
                }
            }
            for (k, c) in rgb.iter_mut().enumerate() {
                *c *= tint[k];
            }
            let alpha = p[3] as f32 / 255.0 * tint[3];
            Rgba([rgb[0], rgb[1], rgb[2], alpha].map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8))
        });
        let out = run(&color_controls, overrides, &texture_a);
        let (worst, mean) = largest_difference(&out, &expected);
        println!("CHECK color_controls vs CPU, {label}: largest difference {worst} of 255, mean {mean:.4}");
    }

    // 4. Errors: where does a mistake in a file show up?
    let bad = std::fs::read_to_string(shaders.join("bad_syntax.fs")).unwrap();
    match isf::parse(&bad).and_then(|p| isf::compile(&p).map(|_| ()).map_err(|problems| format!("{problems:?}"))) {
        Ok(()) => println!("CHECK bad_syntax.fs: compiled?!"),
        Err(problems) => println!("CHECK bad_syntax.fs: {problems}"),
    }
    for (number, line) in bad.lines().enumerate() {
        println!("    file line {}: {}", number + 1, line);
    }
}
