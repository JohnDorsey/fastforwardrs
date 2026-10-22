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
    let is_a_transition = compiled.shape == Shape::Transition;
    let mut values = kind.default_values();
    // (A transition is drawn over a picture of its own, the backdrop, as it is on a clip over another; its progress goes from 0 to 1 in the three frames. Its other values are what
    // they begin as.)
    let progress_place = kind.slot_of_the_value_with_id("progress");
    let picture = TestPicture::gradient(SIZE.0, SIZE.1);
    let backdrop = TestPicture::checkers(SIZE.0, SIZE.1, 6);
    let mut last = None;
    let mut numbers = Vec::new();
    let mut means: Vec<[f64; 4]> = Vec::new();
    for frame_number in 0..3_u64 {
        if is_a_transition && let Some(place) = progress_place {
            values[place] = crate::project::PropertyValue::Number(frame_number as f32 / 2.0);
        }
        numbers = vec![1.0 + frame_number as f32 / 30.0, 1.0 / 30.0, 30.0, 0.0];
        numbers.extend(numbers_of_the_values(&values));
        let mut effect = call(&key, &numbers);
        if structure.depends_on_earlier_frames() {
            effect.state = Some(EffectState {
                id: number as u64 + 1,
                frame: frame_number,
            });
        }
        let layers = if is_a_transition {
            vec![whole_frame(2, Vec::new()), whole_frame(1, vec![effect])]
        } else {
            vec![whole_frame(1, vec![effect])]
        };
        let (frame, statistics) = frame_of(renderer, SIZE, &[(1, &picture), (2, &backdrop)], layers);
        if statistics.effects_not_run != 0 {
            return Err(format!(
                "draw: the engine did not run the effect ({})",
                statistics.effects_not_run
            ));
        }
        means.push(mean_of(&frame));
        last = Some(frame);
    }
    let frame = last.expect("three frames were drawn");
    if std::env::var_os("FASTFORWARDRS_ISF_CORPUS_SHOW").is_some() {
        let mut distinct: Vec<[u8; 4]> = Vec::new();
        for pixel in frame.rgba_pixels.as_chunks::<4>().0 {
            if !distinct.contains(pixel) && distinct.len() < 12 {
                distinct.push(*pixel);
            }
        }
        println!("     numbers {numbers:?}");
        println!("     first colours {distinct:?}");
    }
    let in_words = |mean: &[f64; 4]| format!("({:.0}, {:.0}, {:.0}, {:.0})", mean[0], mean[1], mean[2], mean[3]);
    // A transition is one that begins as what is behind it and ends as the picture it brings (by the mean of the colours: a soft edge or a few pixels of a shape change it a little, and a shader that
    // reads the pictures wrongly, or whose values it begins with make it draw nothing, change it a lot).
    if is_a_transition {
        let far_from = |frame: &[f64; 4], picture: &TestPicture| {
            let wanted = mean_of_a_picture(picture);
            frame
                .iter()
                .zip(wanted)
                .map(|(got, wanted)| (got - wanted).abs())
                .fold(0.0, f64::max)
                > MOST_THE_MEAN_OF_A_TRANSITION_MAY_BE_OFF
        };
        if far_from(&means[0], &backdrop) {
            return Err(format!(
                "draw: at progress 0 the frame is not what is behind the layer (its mean is {}, and what is behind is {})",
                in_words(&means[0]),
                in_words(&mean_of_a_picture(&backdrop))
            ));
        }
        if far_from(&means[2], &picture) {
            return Err(format!(
                "draw: at progress 1 the frame is not the layer (its mean is {}, and the layer is {})",
                in_words(&means[2]),
                in_words(&mean_of_a_picture(&picture))
            ));
        }
    }
    // (A transition is said at its three progresses: 0 should be what is behind it, and 1 the picture it brings, whatever comes between.)
    let colours = if is_a_transition {
        format!(
            "mean rgba at progress 0 {}, 0.5 {}, 1 {}",
            in_words(&means[0]),
            in_words(&means[1]),
            in_words(&means[2])
        )
    } else {
        format!("mean rgba {}", in_words(&means[2]))
    };
    Ok(format!(
        "{shape}, {passes} passes{}, {colours}",
        if structure.depends_on_earlier_frames() {
            ", remembers"
        } else {
            ""
        }
    ))
}

/// How far, in levels of 8 bits, the mean of a frame of a transition may be from the mean of the picture that it begins as and ends as (the pictures are of 48 by 30 pixels, and the means
/// of the transitions of the Vidvox collection that work are off by one or two).
const MOST_THE_MEAN_OF_A_TRANSITION_MAY_BE_OFF: f64 = 6.0;

/// The mean of each of the four numbers of the pixels of `picture`.
fn mean_of_a_picture(picture: &TestPicture) -> [f64; 4] {
    let count = (picture.rgba.len() / 4) as f64;
    let mut sums = [0.0_f64; 4];
    for pixel in picture.rgba.as_chunks::<4>().0 {
        for (sum, value) in sums.iter_mut().zip(pixel) {
            *sum += f64::from(*value);
        }
    }
    sums.map(|sum| sum / count)
}

/// The mean of each of the four numbers of the pixels of `frame`.
fn mean_of(frame: &super::RenderedFrame) -> [f64; 4] {
    let count = (frame.rgba_pixels.len() / 4) as f64;
    let mut sums = [0.0_f64; 4];
    for pixel in frame.rgba_pixels.as_chunks::<4>().0 {
        for (sum, value) in sums.iter_mut().zip(pixel) {
            *sum += f64::from(*value);
        }
    }
    sums.map(|sum| sum / count)
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
