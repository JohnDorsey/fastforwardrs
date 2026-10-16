// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Video pictures in planes, sent to the engine and made red, green and blue by the graphics card: against the same arithmetic in plain Rust
//! (`yuv::reference_rgba`), for every matrix and range and for all sizes, odd ones too.

use std::time::Duration;

use super::testing::with_software_graphics;
use super::yuv::{chroma_size, reference_rgba};
use super::{
    FrameDescription, FrameRenderer, LayerDescription, LayerSource, PictureData, PictureId, PicturePixels,
    PictureProblem, YuvColour, YuvMatrix, YuvRange,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

pub(super) fn whole_frame_of(picture: u64, size: (u32, u32)) -> FrameDescription {
    FrameDescription {
        colour: Default::default(),
        render_size: size,
        layers: vec![LayerDescription {
            source: LayerSource::Picture(PictureId(picture)),
            corners: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            effects: Vec::new(),
        }],
    }
}

/// Numbers that look random and are the same every time.
pub(super) fn noise(count: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn planes_of_noise(width: u32, height: u32, seed: u64) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (chroma_width, chroma_height) = chroma_size(width, height);
    let chroma_bytes = (chroma_width * chroma_height) as usize;
    (
        noise((width * height) as usize, seed),
        noise(chroma_bytes, seed + 1000),
        noise(chroma_bytes, seed + 2000),
    )
}

/// The frame the engine makes of a video picture put on a frame of its own size, and the one the reference makes: how far apart they are.
fn engine_against_reference(
    renderer: &mut FrameRenderer,
    id: u64,
    width: u32,
    height: u32,
    colour: YuvColour,
    seed: u64,
) -> (Vec<u8>, Vec<u8>) {
    let (y, u, v) = planes_of_noise(width, height, seed);
    let reference = reference_rgba(width, height, &y, &u, &v, colour);
    renderer
        .upload_picture(
            PictureId(id),
            &PictureData::yuv420p8(width, height, y, u, v, colour).unwrap(),
        )
        .unwrap();
    let (frame, _) = renderer
        .render(&whole_frame_of(id, (width, height)), A_LONG_TIME)
        .unwrap();
    (frame.rgba_pixels, reference)
}

fn largest_difference(a: &[u8], b: &[u8]) -> u8 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0)
}

#[test]
fn pictures_of_every_size_odd_ones_too_are_what_the_plain_arithmetic_makes_of_them_within_a_level() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for (number, (width, height)) in [
            (1, 1),
            (2, 1),
            (1, 2),
            (2, 2),
            (3, 3),
            (5, 3),
            (7, 5),
            (16, 9),
            (33, 17),
            (64, 36),
            (65, 37),
        ]
        .into_iter()
        .enumerate()
        {
            let (engine, reference) = engine_against_reference(
                &mut renderer,
                number as u64 + 1,
                width,
                height,
                YuvColour::UNSPECIFIED,
                number as u64 + 7,
            );
            assert!(
                largest_difference(&engine, &reference) <= 1,
                "{width} by {height}: the engine is off by {}",
                largest_difference(&engine, &reference)
            );
        }
    });
}

#[test]
fn every_matrix_and_every_range_agrees_with_the_plain_arithmetic_and_they_are_not_the_same_as_each_other() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut pictures = Vec::new();
        let mut number = 0;
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709, YuvMatrix::Bt2020] {
            for range in [YuvRange::Limited, YuvRange::Full] {
                number += 1;
                let (engine, reference) =
                    engine_against_reference(&mut renderer, number, 24, 16, YuvColour { matrix, range }, 5);
                assert!(largest_difference(&engine, &reference) <= 1, "{matrix:?} {range:?}");
                pictures.push(engine);
            }
        }
        // (The same planes each time: what differs is the colours they stand for.)
        for first in 0..pictures.len() {
            for second in first + 1..pictures.len() {
                assert!(
                    largest_difference(&pictures[first], &pictures[second]) > 8,
                    "the pictures {first} and {second} are too much alike for another matrix or range"
                );
            }
        }
    });
}

#[test]
fn a_red_that_standard_definition_video_has_is_red() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Y 81, U 90, V 240 is pure red in BT.601 limited range (the numbers the standard gives).
        renderer
            .upload_picture(
                PictureId(1),
                &PictureData::yuv420p8(2, 2, vec![81; 4], vec![90], vec![240], YuvColour::UNSPECIFIED).unwrap(),
            )
            .unwrap();
        let (frame, _) = renderer.render(&whole_frame_of(1, (2, 2)), A_LONG_TIME).unwrap();
        for pixel in frame.rgba_pixels.as_chunks::<4>().0 {
            assert!(
                pixel[0] >= 254 && pixel[1] <= 2 && pixel[2] <= 2 && pixel[3] == 255,
                "{pixel:?}"
            );
        }
    });
}

#[test]
fn a_video_picture_is_placed_and_cut_off_like_any_other_picture() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A white picture of 8 by 8, put on the right half of a frame of 16 by 8: black on the left, white on the right (limited range white is 235).
        renderer
            .upload_picture(
                PictureId(1),
                &PictureData::yuv420p8(
                    8,
                    8,
                    vec![235; 64],
                    vec![128; 16],
                    vec![128; 16],
                    YuvColour::UNSPECIFIED,
                )
                .unwrap(),
            )
            .unwrap();
        let mut description = FrameDescription::black((16, 8));
        description.layers.push(LayerDescription {
            source: LayerSource::Picture(PictureId(1)),
            corners: [(0.5, 0.0), (1.0, 0.0), (1.0, 1.0), (0.5, 1.0)],
            effects: Vec::new(),
        });
        let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();
        assert_eq!(frame.pixel(3, 4), [0, 0, 0, 255]);
        let white = frame.pixel(12, 4);
        assert!(
            white[0] >= 254 && white[1] >= 254 && white[2] >= 254 && white[3] == 255,
            "{white:?}"
        );
    });
}

#[test]
fn a_video_picture_with_a_wrong_number_of_bytes_in_a_plane_is_refused_and_the_words_say_which_plane() {
    let colour = YuvColour::UNSPECIFIED;
    let problem = PictureData::yuv420p8(5, 3, vec![0; 15], vec![0; 5], vec![0; 6], colour).unwrap_err();
    // Chroma is 3 across and 2 down: 6 bytes. The blue plane has 5.
    assert_eq!(
        problem,
        PictureProblem::WrongNumberOfBytes {
            width: 5,
            height: 3,
            plane: "blue colour difference",
            expected: 6,
            actual: 5
        }
    );
    assert!(problem.to_string().contains("blue colour difference"), "{problem}");
    let problem = PictureData::yuv420p8(5, 3, vec![0; 14], vec![0; 6], vec![0; 6], colour).unwrap_err();
    assert!(
        matches!(
            problem,
            PictureProblem::WrongNumberOfBytes {
                plane: "brightness",
                expected: 15,
                actual: 14,
                ..
            }
        ),
        "{problem:?}"
    );
    let problem = PictureData::yuv420p8(5, 3, vec![0; 15], vec![0; 6], vec![0; 7], colour).unwrap_err();
    assert!(
        matches!(
            problem,
            PictureProblem::WrongNumberOfBytes {
                plane: "red colour difference",
                ..
            }
        ),
        "{problem:?}"
    );
    assert!(PictureData::yuv420p8(5, 3, vec![0; 15], vec![0; 6], vec![0; 6], colour).is_ok());
    assert!(matches!(
        PictureData::yuv420p8(0, 3, vec![], vec![], vec![], colour),
        Err(PictureProblem::Empty { .. })
    ));
}

#[test]
fn the_bytes_of_a_video_picture_count_for_what_they_take_up() {
    let picture = PictureData::yuv420p8(4, 4, vec![0; 16], vec![0; 4], vec![0; 4], YuvColour::UNSPECIFIED).unwrap();
    assert_eq!(picture.byte_count(), 24);
    assert!(matches!(picture.pixels, PicturePixels::Yuv420p8 { .. }));
}
