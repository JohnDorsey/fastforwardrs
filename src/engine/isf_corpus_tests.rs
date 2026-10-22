// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A folder of ISF shaders, each one read, compiled, installed in the engine and **drawn** by the program on the software adapter, with what became of each printed (phase 6.7, the
//! corpus): **run by hand**, with
//!
//! `FASTFORWARDRS_ISF_CORPUS=<folder> cargo test --lib isf_corpus -- --ignored --nocapture`
//!
//! It is how the shaders of the ISF bundles (`bundled-plugins/shaders.json`) were checked before they were put in the manifest, and how a shader that does not work is found: the line
//! of each says the stage it got to (`read`, `compile`, `structure`, `install`, `draw`) and why it stopped there. Only the files at the top of the folder are looked at.

use std::path::Path;

use super::effect_tests::{frame_of, whole_frame};
use super::foreign_effect_tests::call;
use super::oracle_tests::TestPicture;
use super::testing::with_software_graphics;
use super::{DEFAULT_MOST_PASSES_OF_A_LOOP, EffectState, FrameRenderer, guard_wgsl};
use crate::isf::{Shape, compile_in_a_worker, descriptor_of, numbers_of_the_values, read};
use crate::project::EffectKind;

/// How wide and how high the pictures that the shaders are drawn on are.
const SIZE: (u32, u32) = (48, 30);

/// What became of one shader: `Ok` with what its picture was like, or the stage it stopped at and why.
fn what_became_of(renderer: &mut FrameRenderer, number: usize, name: &str, text: &str) -> Result<String, String> {
    let file = read(text).map_err(|problem| format!("read: {problem}"))?;
    let kind = EffectKind::from_snapshot(descriptor_of(&file, name, text)).map_err(|why| format!("read: {why}"))?;
    let compiled = compile_in_a_worker(&file).map_err(|problems| {
        format!(
            "compile: {}",
            problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
        )
    })?;
    let structure = compiled.structure.clone().map_err(|problems| {
        format!(
            "structure: {}",
            problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
        )
    })?;
    structure.check().map_err(|why| format!("structure: {why}"))?;
    let key = format!("corpus:{number}");
    let guarded =
        guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP).map_err(|problem| format!("install: {problem}"))?;
    renderer
        .install_effect_with_structure(&key, &guarded, &structure)
        .map_err(|problem| format!("install: {problem}"))?;
    let shape = format!("{:?}", compiled.shape);
    let passes = structure.passes.len();
    if compiled.shape == Shape::Transition {
        // (A transition reads two pictures, and the engine does not give a shader two yet: it is only installed.)
        return Ok(format!("{shape}, {passes} passes, installed (not drawn: a transition)"));
    }
    let mut numbers = vec![1.0, 1.0 / 30.0, 30.0, 0.0];
    numbers.extend(numbers_of_the_values(&kind.default_values()));
    let picture = TestPicture::gradient(SIZE.0, SIZE.1);
    let mut last = None;
    for frame_number in 0..3_u64 {
        let mut effect = call(&key, &numbers);
        if structure.depends_on_earlier_frames() {
            effect.state = Some(EffectState {
                id: number as u64 + 1,
                frame: frame_number,
            });
        }
        numbers[0] = 1.0 + frame_number as f32 / 30.0;
        let (frame, statistics) = frame_of(renderer, SIZE, &[(1, &picture)], vec![whole_frame(1, vec![effect])]);
        if statistics.effects_not_run != 0 {
            return Err(format!(
                "draw: the engine did not run the effect ({})",
                statistics.effects_not_run
            ));
        }
        last = Some(frame);
    }
    let frame = last.expect("three frames were drawn");
    if std::env::var_os("FASTFORWARDRS_ISF_CORPUS_SHOW").is_some() {
        let mut distinct: Vec<[u8; 4]> = Vec::new();
        for pixel in frame.rgba_pixels.chunks_exact(4) {
            let pixel = [pixel[0], pixel[1], pixel[2], pixel[3]];
            if !distinct.contains(&pixel) && distinct.len() < 12 {
                distinct.push(pixel);
            }
        }
        println!("     numbers {numbers:?}");
        println!("     first colours {distinct:?}");
    }
    let count = (frame.rgba_pixels.len() / 4) as f64;
    let mut sums = [0.0_f64; 4];
    for pixel in frame.rgba_pixels.chunks_exact(4) {
        for (sum, value) in sums.iter_mut().zip(pixel) {
            *sum += f64::from(*value);
        }
    }
    Ok(format!(
        "{shape}, {passes} passes{}, mean rgba ({:.0}, {:.0}, {:.0}, {:.0})",
        if structure.depends_on_earlier_frames() {
            ", remembers"
        } else {
            ""
        },
        sums[0] / count,
        sums[1] / count,
        sums[2] / count,
        sums[3] / count
    ))
}

#[test]
#[ignore = "run by hand on a folder of shaders: see the top of the file"]
fn isf_corpus_every_shader_of_the_folder_is_read_compiled_and_drawn_and_what_became_of_each_is_printed() {
    let Some(folder) = std::env::var_os("FASTFORWARDRS_ISF_CORPUS") else {
        println!("FASTFORWARDRS_ISF_CORPUS names no folder: nothing was looked at");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(Path::new(&folder))
        .expect("the folder can be read")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ending| ending.eq_ignore_ascii_case("fs") || ending.eq_ignore_ascii_case("isf"))
        })
        .filter(|path| {
            // (Only the files with this in their names, if it is said: FASTFORWARDRS_ISF_CORPUS_ONLY.)
            std::env::var("FASTFORWARDRS_ISF_CORPUS_ONLY")
                .map_or(true, |only| path.to_string_lossy().contains(only.as_str()))
        })
        .collect();
    files.sort();
    let mut failures = Vec::new();
    with_software_graphics!(graphics, {
        for (number, path) in files.iter().enumerate() {
            // (A new renderer for each, so that no shader is held up by what another left.)
            let mut renderer = FrameRenderer::new(graphics.device.clone());
            let name = path
                .file_name()
                .expect("a file has a name")
                .to_string_lossy()
                .into_owned();
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    println!("FAIL {name} | read: {error}");
                    failures.push(name);
                    continue;
                }
            };
            match what_became_of(&mut renderer, number, &name, &text) {
                Ok(said) => println!("ok   {name} | {said}"),
                Err(why) => {
                    println!("FAIL {name} | {why}");
                    failures.push(name);
                }
            }
        }
    });
    println!("{} shaders, {} failed", files.len(), failures.len());
}
