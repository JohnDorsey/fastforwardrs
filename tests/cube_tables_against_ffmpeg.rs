// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The look-up of the `.cube` host held to ffmpeg's (phase 4, piece 4.6, PH-04): the same table applied to the same 8-bit colours by this program's reference look-up
//! (`CubeTable::look_up`, which the card's pass is held to in the tests of the engine) and by ffmpeg's `lut3d` filter (`interp=tetrahedral`, `interp=trilinear`) and
//! `lut1d` filter, with the real ffmpeg.
//!
//! **What was measured** (ffmpeg 9.0.2 on Windows, 2026-10-17) is stated by the bound in each test.
//!
//! **Where this program and ffmpeg differ, on purpose.** A table with a `DOMAIN_MIN` and a `DOMAIN_MAX` that are not 0 and 1: this program maps the input colours from the domain to
//! the table, as the format's specification says (and OpenColorIO and DaVinci Resolve do); ffmpeg's `lut3d` was measured (on a table with the domain 0.1 to 0.9 in red and 0.2 to 0.8 in
//! blue) to give the colours as if the domain were 0 to 1, so the two are 86 levels apart at the worst colour. The tables here have the usual domain, where they agree; the domain is held
//! to the specification by the tests of the reader (`cube_table`).

mod support;

use std::path::Path;

use fastforwardrs::cube_table::{CubeTable, Interpolation, parse_cube};
use fastforwardrs::media::FfmpegToolchain;
use support::scratch_directory;
use support::yuv_videos::{ffmpeg, run};

const SIDE: usize = 64;

/// Every colour of a coarse grid of the cube of colours (so that the corners, the edges and the diagonal are all there), then noise: `SIDE * SIDE` pixels of `rgb24`.
fn colours_to_look_up() -> Vec<[u8; 3]> {
    let mut colours = Vec::with_capacity(SIDE * SIDE);
    let levels = [0_u8, 1, 17, 64, 100, 127, 128, 190, 254, 255];
    for red in levels {
        for green in levels {
            for blue in levels {
                colours.push([red, green, blue]);
            }
        }
    }
    let mut state = 0x2545_F491_u32;
    while colours.len() < SIDE * SIDE {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 8) as u8
        };
        colours.push([next(), next(), next()]);
    }
    colours.truncate(SIDE * SIDE);
    colours
}

/// The colours of `input` after ffmpeg has applied `filter` to them.
fn what_ffmpeg_makes_of(toolchain: &FfmpegToolchain, folder: &Path, input: &[[u8; 3]], filter: &str) -> Vec<[u8; 3]> {
    let raw_in = folder.join("in.raw");
    let raw_out = folder.join("out.raw");
    std::fs::write(&raw_in, input.iter().flatten().copied().collect::<Vec<u8>>()).unwrap();
    let mut command = ffmpeg(toolchain);
    command
        .args(["-f", "rawvideo", "-pixel_format", "rgb24", "-video_size"])
        .arg(format!("{SIDE}x{SIDE}"))
        .arg("-i")
        .arg(&raw_in)
        .arg("-vf")
        .arg(filter)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
        .arg(&raw_out);
    run(command);
    let bytes = std::fs::read(&raw_out).unwrap();
    bytes.as_chunks::<3>().0.to_vec()
}

/// What this program's look-up makes of `input`, as 8-bit colours.
fn what_the_reference_makes_of(table: &CubeTable, input: &[[u8; 3]], interpolation: Interpolation) -> Vec<[u8; 3]> {
    input
        .iter()
        .map(|colour| {
            let looked_up = table.look_up(colour.map(|channel| f32::from(channel) / 255.0), interpolation);
            looked_up.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
        })
        .collect()
}

fn the_largest_difference(a: &[[u8; 3]], b: &[[u8; 3]]) -> (u8, usize) {
    let mut largest = 0;
    let mut at = 0;
    for (number, (a, b)) in a.iter().zip(b).enumerate() {
        for place in 0..3 {
            let difference = a[place].abs_diff(b[place]);
            if difference > largest {
                largest = difference;
                at = number;
            }
        }
    }
    (largest, at)
}

/// The text of a 3-D table by formula.
fn text_of_a_3d_table(size: usize, header: &str, make: impl Fn(f32, f32, f32) -> [f32; 3]) -> String {
    let mut text = format!("{header}LUT_3D_SIZE {size}\n");
    let top = (size - 1) as f32;
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let [r, g, b] = make(red as f32 / top, green as f32 / top, blue as f32 / top);
                text.push_str(&format!("{r:.6} {g:.6} {b:.6}\n"));
            }
        }
    }
    text
}

/// Tables of several kinds: a curved one that is not near the identity, one that crosses the colours over, a small one and a big one, and one with a domain.
fn tables() -> Vec<(&'static str, String)> {
    vec![
        (
            "a warm curved grade, 17 points",
            text_of_a_3d_table(17, "", |r, g, b| {
                [
                    r.powf(0.8) * 1.05,
                    g.powf(1.1) * 0.95 + 0.02 * r,
                    (b * 0.9 + 0.1 * g).powf(1.3),
                ]
            }),
        ),
        (
            "channels crossed over, 9 points",
            text_of_a_3d_table(9, "", |r, g, b| {
                [g * 0.7 + b * 0.3, b * 0.5 + r * 0.5, r * 0.2 + g * 0.8]
            }),
        ),
        (
            "two points a side, a table of the corners of the cube",
            text_of_a_3d_table(2, "", |r, g, b| [r * 0.9 + 0.05, g * 0.2 + b * 0.7, (1.0 - r) * b]),
        ),
        (
            "the biggest, 65 points, a mix of curves",
            text_of_a_3d_table(65, "", |r, g, b| {
                [(r * 1.2 - 0.1).clamp(0.0, 1.0), g * g, 0.5 * (b + (r * g).sqrt())]
            }),
        ),
        (
            "values outside 0 to 1",
            text_of_a_3d_table(5, "", |r, g, b| [r * 1.3 - 0.1, g, (b * b) * 1.2]),
        ),
    ]
}

#[test]
fn a_3d_table_makes_the_colours_that_ffmpegs_tetrahedral_lut3d_makes_to_within_a_level() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let input = colours_to_look_up();
    for (name, text) in tables() {
        let table = parse_cube(&text).unwrap();
        let cube = folder.path().join("table.cube");
        std::fs::write(&cube, &text).unwrap();
        let filter = format!(
            "lut3d=file='{}':interp=tetrahedral",
            cube.to_str().unwrap().replace('\\', "/").replace(':', "\\:")
        );

        let theirs = what_ffmpeg_makes_of(&toolchain, folder.path(), &input, &filter);
        let ours = what_the_reference_makes_of(&table, &input, Interpolation::Tetrahedral);

        let (largest, at) = the_largest_difference(&theirs, &ours);
        assert!(
            largest <= 1,
            "{name}: the largest difference is {largest} levels, at the colour {:?}, where ffmpeg makes {:?} and this makes {:?}",
            input[at],
            theirs[at],
            ours[at]
        );
    }
}

#[test]
fn a_3d_table_makes_the_colours_that_ffmpegs_trilinear_lut3d_makes_to_within_a_level() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let input = colours_to_look_up();
    for (name, text) in tables() {
        let table = parse_cube(&text).unwrap();
        let cube = folder.path().join("table.cube");
        std::fs::write(&cube, &text).unwrap();
        let filter = format!(
            "lut3d=file='{}':interp=trilinear",
            cube.to_str().unwrap().replace('\\', "/").replace(':', "\\:")
        );

        let theirs = what_ffmpeg_makes_of(&toolchain, folder.path(), &input, &filter);
        let ours = what_the_reference_makes_of(&table, &input, Interpolation::Trilinear);

        let (largest, at) = the_largest_difference(&theirs, &ours);
        assert!(
            largest <= 1,
            "{name}: the largest difference is {largest} levels, at the colour {:?}, where ffmpeg makes {:?} and this makes {:?}",
            input[at],
            theirs[at],
            ours[at]
        );
    }
}

#[test]
fn a_1d_table_makes_the_colours_that_ffmpegs_lut1d_makes_to_within_a_level() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let input = colours_to_look_up();
    let mut text = String::from("LUT_1D_SIZE 33\n");
    for step in 0..33 {
        let x = step as f32 / 32.0;
        text.push_str(&format!("{:.6} {:.6} {:.6}\n", x.powf(0.8), x * x, 1.0 - x * 0.9));
    }
    let table = parse_cube(&text).unwrap();
    let cube = folder.path().join("curve.cube");
    std::fs::write(&cube, &text).unwrap();
    let filter = format!(
        "lut1d=file='{}':interp=linear",
        cube.to_str().unwrap().replace('\\', "/").replace(':', "\\:")
    );

    let theirs = what_ffmpeg_makes_of(&toolchain, folder.path(), &input, &filter);
    let ours = what_the_reference_makes_of(&table, &input, Interpolation::Tetrahedral);

    let (largest, at) = the_largest_difference(&theirs, &ours);
    assert!(
        largest <= 1,
        "the largest difference is {largest} levels, at the colour {:?}, where ffmpeg makes {:?} and this makes {:?}",
        input[at],
        theirs[at],
        ours[at]
    );
}
