// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A finished frame made into the planes of `yuv420p` by the graphics card, for the encoder of the export: against the same arithmetic in plain Rust
//! (`yuv::reference_yuv420p`) for every matrix and range and for all sizes, odd ones too; against the numbers the standards give; and what they are made from
//! (the 8-bit frame that the preview shows).

use std::time::Duration;

use super::testing::with_software_graphics;
use super::yuv::{chroma_size, reference_yuv420p};
use super::yuv_tests::{noise, whole_frame_of};
use super::{
    FrameDescription, FrameRenderer, LayerDescription, LayerSource, PictureData, PictureId, RenderProblem,
    RenderedPlanes, YuvColour, YuvMatrix, YuvRange,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const EVERY_COLOUR: [YuvColour; 6] = [
    YuvColour {
        matrix: YuvMatrix::Bt601,
        range: YuvRange::Limited,
    },
    YuvColour {
        matrix: YuvMatrix::Bt601,
        range: YuvRange::Full,
    },
    YuvColour {
        matrix: YuvMatrix::Bt709,
        range: YuvRange::Limited,
    },
    YuvColour {
        matrix: YuvMatrix::Bt709,
        range: YuvRange::Full,
    },
    YuvColour {
        matrix: YuvMatrix::Bt2020,
        range: YuvRange::Limited,
    },
    YuvColour {
        matrix: YuvMatrix::Bt2020,
        range: YuvRange::Full,
    },
];

fn largest_difference(a: &[u8], b: &[u8]) -> u8 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0)
}

/// A picture of RGBA bytes that look random, opaque, uploaded as picture 1.
fn upload_noise(renderer: &mut FrameRenderer, width: u32, height: u32, seed: u64) {
    let mut bytes = noise((width * height * 4) as usize, seed);
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    renderer
        .upload_picture(PictureId(1), &PictureData::rgba8(width, height, bytes).unwrap())
        .unwrap();
}

#[test]
fn the_planes_of_a_frame_are_what_the_plain_arithmetic_makes_of_its_8_bit_pixels_within_a_level() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for (number, (width, height)) in [
            (1, 1),
            (2, 2),
            (3, 3),
            (2, 1),
            (1, 2),
            (7, 5),
            (16, 9),
            (64, 36),
            (65, 37),
            (257, 3),
            (1000, 4),
        ]
        .into_iter()
        .enumerate()
        {
            upload_noise(&mut renderer, width, height, number as u64 + 3);
            let description = whole_frame_of(1, (width, height));
            let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();
            for colour in EVERY_COLOUR {
                let (planes, _) = renderer.render_planes(&description, colour, A_LONG_TIME).unwrap();
                let (y, u, v) = reference_yuv420p(width, height, &frame.rgba_pixels, colour);
                assert_eq!((planes.width_in_pixels, planes.height_in_pixels), (width, height));
                assert_eq!(planes.luma().len(), y.len(), "{width} by {height}");
                assert_eq!(planes.blue_difference().len(), u.len(), "{width} by {height}");
                assert_eq!(planes.red_difference().len(), v.len(), "{width} by {height}");
                for (name, made, reference) in [
                    ("brightness", planes.luma(), &y),
                    ("blue colour difference", planes.blue_difference(), &u),
                    ("red colour difference", planes.red_difference(), &v),
                ] {
                    assert!(
                        largest_difference(made, reference) <= 1,
                        "{width} by {height}, {colour:?}: the {name} is off by {}",
                        largest_difference(made, reference)
                    );
                }
            }
        }
    });
}

#[test]
fn the_planes_are_laid_one_after_the_other_in_the_order_the_encoder_reads_them() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for (width, height) in [(8, 6), (7, 5), (2, 2)] {
            let (chroma_width, chroma_height) = chroma_size(width, height);
            let (planes, statistics) = renderer
                .render_planes(
                    &FrameDescription::black((width, height)),
                    YuvColour::UNSPECIFIED,
                    A_LONG_TIME,
                )
                .unwrap();
            // (The two passes of a black frame, and the two that make the planes.)
            assert_eq!(statistics.passes_recorded, 4);
            assert_eq!(
                planes.planes.len(),
                (width * height + 2 * chroma_width * chroma_height) as usize
            );
            assert_eq!(planes.byte_count(), planes.planes.len());
            // (A black frame: brightness 16 everywhere, and no colour, which is 128, in both of the other planes.)
            assert!(planes.luma().iter().all(|number| *number == 16), "{planes:?}");
            assert!(planes.blue_difference().iter().all(|number| *number == 128));
            assert!(planes.red_difference().iter().all(|number| *number == 128));
            assert_eq!(
                planes.planes[..(width * height) as usize],
                *planes.luma(),
                "the brightness is first"
            );
        }
    });
}

#[test]
fn the_top_of_the_frame_is_the_first_row_of_every_plane_and_the_left_the_first_of_each_row() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 4 by 4: white at the top left pixel, a red square of four pixels at the bottom right, black elsewhere.
        let mut bytes = Vec::new();
        for down in 0..4 {
            for across in 0..4 {
                bytes.extend(match (across, down) {
                    (0, 0) => [255, 255, 255, 255],
                    (2 | 3, 2 | 3) => [255, 0, 0, 255],
                    _ => [0, 0, 0, 255],
                });
            }
        }
        renderer
            .upload_picture(PictureId(1), &PictureData::rgba8(4, 4, bytes).unwrap())
            .unwrap();
        let (planes, _) = renderer
            .render_planes(&whole_frame_of(1, (4, 4)), YuvColour::UNSPECIFIED, A_LONG_TIME)
            .unwrap();
        let y = planes.luma();
        assert!(y[0] >= 234, "white at the top left: {y:?}");
        assert!(y[1] <= 17 && y[4] <= 17, "black beside it: {y:?}");
        assert!(y[10] >= 80 && y[10] <= 83, "red at (2, 2): {y:?}");
        assert!(y[15] >= 80 && y[15] <= 83, "red at (3, 3): {y:?}");
        // The colour differences: 2 by 2. Only the bottom right square has any (red: blue difference 90, red difference 240).
        let (u, v) = (planes.blue_difference(), planes.red_difference());
        assert_eq!(u.len(), 4);
        assert!(
            u[0].abs_diff(128) <= 1 && v[0].abs_diff(128) <= 1,
            "the top left square is grey"
        );
        assert!(
            u[3].abs_diff(90) <= 1 && v[3].abs_diff(240) <= 1,
            "the bottom right square is red: {u:?} {v:?}"
        );
    });
}

/// A frame of one flat colour, as the planes the standards give it (BT.601 and BT.709, limited range).
#[test]
fn flat_colours_are_the_numbers_that_the_standards_give() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let flat = |colour: [u8; 4]| FrameDescription {
            render_size: (8, 8),
            layers: vec![LayerDescription {
                source: LayerSource::Solid(colour),
                corners: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
                effects: Vec::new(),
            }],
        };
        let bt601 = YuvColour::UNSPECIFIED;
        let bt709 = YuvColour {
            matrix: YuvMatrix::Bt709,
            range: YuvRange::Limited,
        };
        for (colour, rgba, expected) in [
            (bt601, [255, 255, 255, 255], [235, 128, 128]),
            (bt601, [0, 0, 0, 255], [16, 128, 128]),
            (bt601, [255, 0, 0, 255], [81, 90, 240]),
            (bt601, [0, 255, 0, 255], [145, 54, 34]),
            (bt601, [0, 0, 255, 255], [41, 240, 110]),
            (bt709, [255, 0, 0, 255], [63, 102, 240]),
            (bt709, [0, 255, 0, 255], [173, 42, 26]),
            (bt709, [0, 0, 255, 255], [32, 240, 118]),
        ] {
            let (planes, _) = renderer.render_planes(&flat(rgba), colour, A_LONG_TIME).unwrap();
            let made = [
                planes.luma()[0],
                planes.blue_difference()[0],
                planes.red_difference()[0],
            ];
            for channel in 0..3 {
                assert!(
                    made[channel].abs_diff(expected[channel]) <= 1,
                    "{colour:?} {rgba:?}: {made:?} is not {expected:?}"
                );
            }
        }
    });
}

#[test]
fn a_frame_made_into_planes_and_the_same_frame_made_as_rgba_are_the_same_picture_whatever_comes_between() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        upload_noise(&mut renderer, 40, 24, 11);
        let description = whole_frame_of(1, (40, 24));
        let (first, _) = renderer.render(&description, A_LONG_TIME).unwrap();
        for _ in 0..3 {
            renderer
                .render_planes(&description, YuvColour::UNSPECIFIED, A_LONG_TIME)
                .unwrap();
        }
        let (again, _) = renderer.render(&description, A_LONG_TIME).unwrap();
        assert_eq!(first, again, "making planes in between did not change the RGBA frame");
    });
}

#[test]
fn making_many_frames_of_planes_does_not_make_more_textures_or_buffers_than_it_needs() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        upload_noise(&mut renderer, 64, 36, 5);
        let description = whole_frame_of(1, (64, 36));
        renderer
            .render_planes(&description, YuvColour::UNSPECIFIED, A_LONG_TIME)
            .unwrap();
        let after_one = renderer.pool_statistics();
        let buffers_after_one = renderer.readback_buffers_waiting();
        for _ in 0..10 {
            renderer
                .render_planes(&description, YuvColour::UNSPECIFIED, A_LONG_TIME)
                .unwrap();
        }
        assert_eq!(
            renderer.pool_statistics().made,
            after_one.made,
            "no texture was made again"
        );
        assert!(renderer.pool_statistics().reused > after_one.reused);
        assert_eq!(renderer.readback_buffers_waiting(), buffers_after_one);
    });
}

#[test]
fn a_frame_of_no_size_or_too_big_is_refused_as_a_frame_of_rgba_is() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for size in [(0, 4), (4, 0), (graphics.device.largest_texture_side + 1, 2)] {
            let problem = renderer
                .render_planes(&FrameDescription::black(size), YuvColour::UNSPECIFIED, A_LONG_TIME)
                .unwrap_err();
            assert!(
                matches!(problem, RenderProblem::BadSize { .. }),
                "{size:?}: {problem:?}"
            );
        }
    });
}

#[test]
fn rendered_planes_know_where_each_plane_is() {
    let planes = RenderedPlanes {
        width_in_pixels: 3,
        height_in_pixels: 3,
        // Brightness 3 by 3, then the two colour differences, 2 by 2 each.
        planes: (0u8..17).collect(),
    };
    assert_eq!(planes.luma(), &(0u8..9).collect::<Vec<_>>()[..]);
    assert_eq!(planes.blue_difference(), &[9, 10, 11, 12]);
    assert_eq!(planes.red_difference(), &[13, 14, 15, 16]);
}
