// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's blur and crop (phase 4, piece 4.4), on the software adapter: the blur against a Gaussian worked out on the processor, in light where it is made for, and at two
//! sizes; the crop against the edge where its fractions say it is, to the pixel, and against its soft edge's arithmetic.
//!
//! **What was measured** (on the software adapter of Windows, 2026-10-17) is in the bounds below: they are the tightest round numbers the measurements fit in.

use std::time::Duration;

use super::effect_tests::{flat, frame_of, noisy, render};
use super::linear_light_tests::{colour_of, picture_for, the_whole_frame, upload};
use super::oracle_tests::TestPicture;
use super::testing::with_software_graphics;
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, FrameRenderer, FrameStatistics, LayerDescription,
    LayerSource, RenderedFrame, WorkingSpace,
};
use crate::colour_space::{StandardPrimaries, TransferFunction};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

/// The Gaussian blur of an opaque picture, worked out the plain way in numbers: along the rows and then along the columns, the weights `exp(-offset^2 / 2 sigma^2)` for the
/// offsets of three standard deviations (rounded up) each side, divided by their sum, and the pixel at the edge used for what is beyond it. The result is in numbers of 0 to
/// 255, not rounded.
fn the_blur_of(picture: &TestPicture, sigma: f64) -> Vec<[f64; 3]> {
    let (width, height) = (picture.width as i64, picture.height as i64);
    let half_width = (3.0 * sigma).ceil().max(1.0) as i64;
    let weights: Vec<f64> = (-half_width..=half_width)
        .map(|offset| (-0.5 * (offset * offset) as f64 / (sigma * sigma)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let at = |across: i64, down: i64| {
        let across = across.clamp(0, width - 1);
        let down = down.clamp(0, height - 1);
        let first = ((down * width + across) * 4) as usize;
        [0, 1, 2].map(|place| f64::from(picture.rgba[first + place]))
    };
    let mut along_rows = vec![[0.0; 3]; (width * height) as usize];
    for down in 0..height {
        for across in 0..width {
            let mut sum = [0.0; 3];
            for (weight, offset) in weights.iter().zip(-half_width..=half_width) {
                let colour = at(across + offset, down);
                for place in 0..3 {
                    sum[place] += weight * colour[place];
                }
            }
            along_rows[(down * width + across) as usize] = sum.map(|number| number / total);
        }
    }
    let mut result = vec![[0.0; 3]; (width * height) as usize];
    for down in 0..height {
        for across in 0..width {
            let mut sum = [0.0; 3];
            for (weight, offset) in weights.iter().zip(-half_width..=half_width) {
                let colour = along_rows[((down + offset).clamp(0, height - 1) * width + across) as usize];
                for place in 0..3 {
                    sum[place] += weight * colour[place];
                }
            }
            result[(down * width + across) as usize] = sum.map(|number| number / total);
        }
    }
    result
}

fn the_largest_difference_from(frame: &RenderedFrame, wanted: &[[f64; 3]]) -> f64 {
    let mut largest: f64 = 0.0;
    for (pixel, wanted) in frame.rgba_pixels.as_chunks::<4>().0.iter().zip(wanted) {
        for place in 0..3 {
            largest = largest.max((f64::from(pixel[place]) - wanted[place]).abs());
        }
    }
    largest
}

fn render_in(
    renderer: &mut FrameRenderer,
    size: (u32, u32),
    working: WorkingSpace,
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, FrameStatistics) {
    let mut description = FrameDescription::black(size);
    description.colour = colour_of(working);
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap()
}

/// A picture `height` pixels high is blurred by `sigma` pixels when the radius is `sigma * 1080 / height` points.
fn points_for(sigma: f32, height: u32) -> f32 {
    sigma * 1080.0 / height as f32
}

// ---- the blur ----

#[test]
fn a_blur_is_the_gaussian_of_the_radius_to_within_a_level_whatever_the_picture() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = [
            ("a gradient", TestPicture::gradient(40, 30)),
            ("noise", noisy(40, 30, 3, false)),
            ("squares", TestPicture::checkers(40, 30, 5)),
        ];
        for (name, picture) in &pictures {
            for sigma in [0.5_f32, 1.0, 2.0, 3.5, 6.0] {
                let (blurred, statistics) = frame_of(
                    &mut renderer,
                    (40, 30),
                    &[(1, picture)],
                    vec![LayerDescription {
                        effects: vec![EffectCall::blur(points_for(sigma, 30))],
                        ..the_whole_frame(1)
                    }],
                );
                assert_eq!(statistics.effects_not_run, 0, "{name}, sigma {sigma}");
                assert_eq!(
                    statistics.passes_recorded,
                    2 + 2,
                    "two passes, along the rows and along the columns"
                );
                let largest = the_largest_difference_from(&blurred, &the_blur_of(picture, f64::from(sigma)));
                assert!(
                    largest <= 1.0,
                    "{name}, sigma {sigma}: the largest difference is {largest}"
                );
            }
        }
    });
}

#[test]
fn a_blur_of_a_picture_that_is_all_one_colour_is_that_picture_for_any_radius_even_where_the_kernel_is_cut_off() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for colour in [
            [200, 100, 50, 255],
            [3, 250, 17, 255],
            [0, 0, 0, 255],
            [255, 255, 255, 255],
        ] {
            for points in [1.0_f32, 20.0, 100.0, 1000.0] {
                let (blurred, _) = frame_of(
                    &mut renderer,
                    (16, 12),
                    &[(1, &flat(16, 12, colour))],
                    vec![LayerDescription {
                        effects: vec![EffectCall::blur(points)],
                        ..the_whole_frame(1)
                    }],
                );
                assert!(
                    blurred
                        .rgba_pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|pixel| (0..4).all(|place| pixel[place].abs_diff(colour[place]) <= 1)),
                    "{colour:?}, {points} points: {:?}",
                    blurred.pixel(5, 5)
                );
            }
        }
    });
}

#[test]
fn a_blur_does_not_darken_the_edge_of_the_picture_the_pixel_at_the_edge_is_used_for_what_is_beyond_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A white picture with a black column down the middle: the white at the left edge stays about as white as it was far from the black.
        let mut picture = flat(33, 8, [255, 255, 255, 255]);
        for down in 0..8 {
            let first = (down * 33 + 16) * 4;
            picture.rgba[first..first + 3].copy_from_slice(&[0, 0, 0]);
        }
        let (blurred, _) = frame_of(
            &mut renderer,
            (33, 8),
            &[(1, &picture)],
            vec![LayerDescription {
                effects: vec![EffectCall::blur(points_for(2.0, 8))],
                ..the_whole_frame(1)
            }],
        );
        assert!(
            the_largest_difference_from(&blurred, &the_blur_of(&picture, 2.0)) <= 1.0,
            "{:?}",
            blurred.pixel(0, 4)
        );
        assert_eq!(
            blurred.pixel(0, 4),
            [255, 255, 255, 255],
            "far enough from the black to be untouched"
        );
        // (At the middle of the black column, the Gaussian has 1 / (2 * sqrt(2 pi)) = 0.2 of its weight on it, so the white is left with 0.8.)
        assert!(blurred.pixel(16, 4)[0].abs_diff(204) <= 2, "{:?}", blurred.pixel(16, 4));
    });
}

#[test]
fn a_blur_in_light_averages_the_light_so_squares_of_black_and_white_come_out_brighter_than_the_codes_would_make_them() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let squares = TestPicture {
            width: 36,
            height: 36,
            rgba: (0..36 * 36)
                .flat_map(|number| {
                    if (number % 36 + number / 36) % 2 == 0 {
                        [0, 0, 0, 255]
                    } else {
                        [255, 255, 255, 255]
                    }
                })
                .collect(),
        };
        let effects = || vec![EffectCall::blur(points_for(5.0, 36))];
        // In the codes of the compatibility mode, black and white average to the middle of the steps.
        let (in_codes, _) = frame_of(
            &mut renderer,
            (36, 36),
            &[(1, &squares)],
            vec![LayerDescription {
                effects: effects(),
                ..the_whole_frame(1)
            }],
        );
        let middle = in_codes.pixel(18, 18);
        assert!(middle[..3].iter().all(|colour| colour.abs_diff(128) <= 3), "{middle:?}");
        // In light, they average to half of the light, which is the code that is 188 of the 255.
        upload(
            &mut renderer,
            1,
            &picture_for(LINEAR_REC709, 36, 36, squares.rgba.clone()),
        );
        let (in_light, _) = render_in(
            &mut renderer,
            (36, 36),
            LINEAR_REC709,
            vec![LayerDescription {
                effects: effects(),
                ..the_whole_frame(1)
            }],
        );
        let wanted = TransferFunction::Srgb.encode_from_light(0.5) * 255.0;
        let middle = in_light.pixel(18, 18);
        assert!(
            middle[..3]
                .iter()
                .all(|colour| (f64::from(*colour) - wanted).abs() <= 3.0),
            "{middle:?} where about {wanted:.1} was wanted"
        );
    });
}

#[test]
fn the_radius_is_in_points_of_a_picture_1080_high_so_a_picture_twice_as_high_is_blurred_twice_as_many_pixels_and_looks_alike()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A step from black to white along a row, 36 pixels high; and the same picture made twice as big (every pixel four).
        let small = TestPicture {
            width: 48,
            height: 36,
            rgba: (0..36)
                .flat_map(|_| {
                    (0..48).flat_map(|across| {
                        if across < 24 {
                            [0, 0, 0, 255]
                        } else {
                            [255, 255, 255, 255]
                        }
                    })
                })
                .collect(),
        };
        let big = TestPicture {
            width: 96,
            height: 72,
            rgba: (0..72)
                .flat_map(|_| {
                    (0..96).flat_map(|across| {
                        if across < 48 {
                            [0, 0, 0, 255]
                        } else {
                            [255, 255, 255, 255]
                        }
                    })
                })
                .collect(),
        };
        let blur = |renderer: &mut FrameRenderer, picture: &TestPicture| {
            frame_of(
                renderer,
                (picture.width, picture.height),
                &[(1, picture)],
                vec![LayerDescription {
                    effects: vec![EffectCall::blur(60.0)],
                    ..the_whole_frame(1)
                }],
            )
            .0
        };
        let (small_blurred, big_blurred) = (blur(&mut renderer, &small), blur(&mut renderer, &big));
        // 60 points is 2 pixels of the small picture and 4 of the big one. The big one, averaged in twos, is the small one.
        assert!(
            the_largest_difference_from(&small_blurred, &the_blur_of(&small, 2.0)) <= 1.0,
            "the small one"
        );
        assert!(
            the_largest_difference_from(&big_blurred, &the_blur_of(&big, 4.0)) <= 1.0,
            "the big one"
        );
        for across in 0..48 {
            let small_value = f64::from(small_blurred.pixel(across, 18)[0]);
            let big_value = (f64::from(big_blurred.pixel(across * 2, 36)[0])
                + f64::from(big_blurred.pixel(across * 2 + 1, 36)[0]))
                / 2.0;
            assert!(
                (small_value - big_value).abs() <= 8.0,
                "column {across}: {small_value} where the big picture has {big_value}"
            );
        }
    });
}

#[test]
fn a_blur_leaves_how_opaque_the_picture_is_where_it_is_and_averages_what_is_see_through_with_what_is_not() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Over white, so that what the picture lets through shows: half of the picture is clear, and the other half is red.
        let mut picture = flat(40, 8, [255, 0, 0, 255]);
        for down in 0..8 {
            for across in 0..20 {
                let first = (down * 40 + across) * 4;
                picture.rgba[first..first + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        upload(&mut renderer, 1, &picture.as_picture_data());
        let white = LayerDescription {
            source: LayerSource::Solid([255, 255, 255, 255]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        };
        let blurred_layer = |points: f32| LayerDescription {
            effects: vec![EffectCall::blur(points)],
            ..the_whole_frame(1)
        };
        let (plain, _) = render(&mut renderer, (40, 8), vec![white.clone(), the_whole_frame(1)]);
        let (blurred, _) = render(&mut renderer, (40, 8), vec![white, blurred_layer(points_for(3.0, 8))]);
        assert_eq!(
            plain.pixel(5, 4),
            [255, 255, 255, 255],
            "(clear: the white shows through)"
        );
        assert_eq!(plain.pixel(35, 4), [255, 0, 0, 255]);
        // At the middle the picture is half red and half clear: half of the red over the white. Far from it, as it was.
        let middle = blurred.pixel(20, 4);
        assert!(middle[0] >= 250 && (110..=145).contains(&middle[1]), "{middle:?}");
        assert_eq!(blurred.pixel(0, 4), [255, 255, 255, 255]);
        assert_eq!(blurred.pixel(39, 4), [255, 0, 0, 255]);
    });
}

#[test]
fn a_blur_much_wider_than_the_picture_still_runs_and_makes_one_colour_of_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = TestPicture::gradient(20, 20);
        let (blurred, statistics) = frame_of(
            &mut renderer,
            (20, 20),
            &[(1, &picture)],
            vec![LayerDescription {
                effects: vec![EffectCall::blur(100.0)],
                ..the_whole_frame(1)
            }],
        );
        assert_eq!(statistics.effects_not_run, 0);
        // (Sigma is 1.85 pixels here; the picture is 20 wide, so it is only softened. A radius of a thousand points would be a hundred pixels, and the kernel is cut off at 300.)
        let (wide, _) = frame_of(
            &mut renderer,
            (20, 20),
            &[(1, &picture)],
            vec![LayerDescription {
                effects: vec![EffectCall::blur(100_000.0)],
                ..the_whole_frame(1)
            }],
        );
        let mean = |frame: &RenderedFrame, place: usize| {
            frame
                .rgba_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| f64::from(pixel[place]))
                .sum::<f64>()
                / 400.0
        };
        let first = wide.pixel(0, 0);
        assert!(
            wide.rgba_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| (0..3).all(|place| pixel[place].abs_diff(first[place]) <= 8)),
            "every pixel is about {first:?} (the pixels at the edges are what is beyond them, so not exactly)"
        );
        for place in 0..2 {
            assert!(
                (f64::from(first[place]) - mean(&blurred, place)).abs() <= 12.0,
                "place {place}: {first:?} where the mean is {}",
                mean(&blurred, place)
            );
        }
    });
}

// ---- the crop ----

fn crop_over_white(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    colour: [u8; 4],
    crop: &[f32; 5],
    size: (u32, u32),
) -> RenderedFrame {
    upload(
        renderer,
        1,
        &picture_for(working, size.0, size.1, colour.repeat((size.0 * size.1) as usize)),
    );
    let white = LayerDescription {
        source: LayerSource::Solid([255, 255, 255, 255]),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: Vec::new(),
    };
    let cropped = LayerDescription {
        effects: vec![EffectCall::crop(crop[0], crop[1], crop[2], crop[3], crop[4])],
        ..the_whole_frame(1)
    };
    render_in(renderer, size, working, vec![white, cropped]).0
}

#[test]
fn the_edges_of_a_crop_are_where_the_fractions_say_to_the_pixel_and_what_is_outside_shows_what_is_below() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let colour = [200, 20, 60, 255];
        // 32 by 18: a quarter is 8 columns, a half of the height is 9 rows, a quarter on the right leaves the columns up to 24.
        let frame = crop_over_white(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            colour,
            &[0.25, 0.5, 0.25, 0.0, 0.0],
            (32, 18),
        );
        for down in 0..18 {
            for across in 0..32 {
                let inside = (8..24).contains(&across) && (9..18).contains(&down);
                let wanted = if inside { colour } else { [255, 255, 255, 255] };
                assert_eq!(frame.pixel(across, down), wanted, "column {across}, row {down}");
            }
        }
    });
}

#[test]
fn a_crop_of_nothing_is_not_run_and_a_crop_of_everything_shows_only_what_is_below() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let colour = [10, 120, 230, 255];
        let nothing = crop_over_white(&mut renderer, WorkingSpace::DisplayReferred, colour, &[0.0; 5], (16, 9));
        assert!(nothing.every_pixel_is(colour));
        // More is cut from the left and the right together than there is: nothing is left.
        let everything = crop_over_white(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            colour,
            &[0.6, 0.0, 0.6, 0.0, 0.0],
            (16, 9),
        );
        assert!(everything.every_pixel_is([255, 255, 255, 255]));
        let from_the_top_and_the_bottom = crop_over_white(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            colour,
            &[0.0, 0.6, 0.0, 0.6, 0.0],
            (16, 9),
        );
        assert!(from_the_top_and_the_bottom.every_pixel_is([255, 255, 255, 255]));
    });
}

#[test]
fn an_edge_that_is_not_between_two_pixels_leaves_the_pixel_it_is_in_partly_clear_by_the_smooth_step() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A fraction of 0.3 of 32 columns is 9.6: the centre of column 9 is 0.1 outside the edge, and the step is a pixel wide, so 0.4 of the way up it; column 10 is all inside.
        let colour = [0, 0, 0, 255];
        let frame = crop_over_white(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            colour,
            &[0.3, 0.0, 0.0, 0.0, 0.0],
            (32, 9),
        );
        let covered = 0.4_f32 * 0.4 * (3.0 - 2.0 * 0.4);
        let wanted = 255.0 * (1.0 - covered);
        assert!(
            (f32::from(frame.pixel(9, 4)[0]) - wanted).abs() <= 1.0,
            "{:?} where {wanted:.1} was wanted",
            frame.pixel(9, 4)
        );
        assert_eq!(frame.pixel(8, 4), [255, 255, 255, 255]);
        assert_eq!(frame.pixel(10, 4), [0, 0, 0, 255]);
    });
}

#[test]
fn the_feather_is_a_soft_edge_as_wide_as_its_fraction_of_the_height_that_is_half_inside_the_edge_and_half_outside() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 32 by 18, a half cut from the left (the edge is at column 16) and a feather of a half of the height (9 pixels): the step is from 11.5 to 20.5.
        let frame = crop_over_white(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            [0, 0, 0, 255],
            &[0.5, 0.0, 0.0, 0.0, 0.5],
            (32, 18),
        );
        // (Every edge of the rectangle that is left has the soft step, the picture's own edges too: the feather of a crop that cuts nothing is a soft edge round the picture.
        // The top and the bottom are too far from the middle row to matter here; the right edge, at column 32, is not far from the last columns.)
        let smooth_step = |from_the_edge: f32| {
            let t = ((from_the_edge + 4.5) / 9.0).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        for across in 0..32_u32 {
            let centre = across as f32 + 0.5;
            let covered = smooth_step(centre - 16.0) * smooth_step(32.0 - centre);
            let wanted = 255.0 * (1.0 - covered);
            let found = f32::from(frame.pixel(across, 9)[0]);
            assert!(
                (found - wanted).abs() <= 1.5,
                "column {across}: {found} where the smooth step has {wanted:.1}"
            );
        }
        // Halfway, on the edge, a little over half: the step is 0.5 of the way up at the edge itself.
        let found = f32::from(frame.pixel(16, 9)[0]);
        assert!((100.0..150.0).contains(&found), "{found}");
    });
}

#[test]
fn a_crop_in_light_blends_its_edge_with_what_is_below_in_light() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Black over white, with the edge in the middle of a column: half of that pixel is the black, and half of the light is the white's.
        let frame = crop_over_white(
            &mut renderer,
            LINEAR_REC709,
            [0, 0, 0, 255],
            &[0.28125, 0.0, 0.0, 0.0, 0.0],
            (32, 9),
        );
        // 0.28125 * 32 = 9.0: on a boundary. A fraction of 0.296875 is 9.5: the centre of column 9 is 0.0 from the edge, so half covered by the step.
        assert_eq!(frame.pixel(8, 4), [255, 255, 255, 255]);
        let frame = crop_over_white(
            &mut renderer,
            LINEAR_REC709,
            [0, 0, 0, 255],
            &[0.296875, 0.0, 0.0, 0.0, 0.0],
            (32, 9),
        );
        let wanted = TransferFunction::Srgb.encode_from_light(0.5) * 255.0;
        let found = f64::from(frame.pixel(9, 4)[0]);
        assert!(
            (found - wanted).abs() <= 2.0,
            "{found} where half of the light of white is the code {wanted:.1}"
        );
    });
}

#[test]
fn a_crop_and_a_blur_in_a_row_are_done_in_the_order_they_were_asked_for() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = flat(32, 18, [200, 20, 60, 255]);
        let layers = |effects: Vec<EffectCall>| {
            vec![
                LayerDescription {
                    source: LayerSource::Solid([255, 255, 255, 255]),
                    corners: CORNERS_OF_THE_WHOLE_PICTURE,
                    effects: Vec::new(),
                },
                LayerDescription {
                    effects,
                    ..the_whole_frame(1)
                },
            ]
        };
        let crop = || EffectCall::crop(0.25, 0.0, 0.25, 0.0, 0.0);
        let blur = || EffectCall::blur(points_for(2.0, 18));
        let (crop_then_blur, statistics) =
            frame_of(&mut renderer, (32, 18), &[(1, &picture)], layers(vec![crop(), blur()]));
        assert_eq!(
            statistics.passes_recorded,
            2 + 1 + 2,
            "the layers and the output; the crop; the two of the blur"
        );
        let (blur_then_crop, _) = render(&mut renderer, (32, 18), layers(vec![blur(), crop()]));
        // The cropped edge is softened when the blur comes after the crop and is sharp when it comes before: the column right inside the edge differs.
        assert_ne!(crop_then_blur.pixel(8, 9), blur_then_crop.pixel(8, 9));
        assert_eq!(
            blur_then_crop.pixel(8, 9),
            [200, 20, 60, 255],
            "(a flat picture is not changed by a blur, and then the crop is sharp)"
        );
        assert_ne!(
            crop_then_blur.pixel(8, 9),
            [200, 20, 60, 255],
            "(the blur takes the cut edge into account)"
        );
    });
}
