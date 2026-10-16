// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The planes of the export in linear light (phase 3, piece 3.5; decision C13 of `docs/engine-phase-3-design.md`): they are made from the **finished picture as the card
//! has it** (encoded, in half floats), and not from the 8-bit picture that the preview shows. A picture that is rounded to 8 bits and then turned into brightness and colour
//! differences is rounded twice; this is the test that it is rounded once.

use std::time::Duration;

use super::linear_light_tests::{a_colour, colour_of, noise, picture_for, the_whole_frame, upload};
use super::testing::with_software_graphics;
use super::yuv::reference_yuv420p;
use super::{
    FrameColour, FrameDescription, FrameRenderer, LayerDescription, OutputEncoding, WorkingSpace, YuvColour, YuvMatrix,
    YuvRange,
};
use crate::colour_space::{StandardPrimaries, TransferFunction};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const BT709_LIMITED: YuvColour = YuvColour {
    matrix: YuvMatrix::Bt709,
    range: YuvRange::Limited,
};

/// The red, green and blue of a layer of a colour with `opacity`, over black, as codes of `curve` (the arithmetic of the standards, in f64).
fn encoded_colour_of_a_layer(code: [u8; 3], opacity: f64, curve: TransferFunction) -> [f64; 3] {
    code.map(|code| {
        let light = TransferFunction::Srgb.decode_to_light(f64::from(code) / 255.0) * opacity;
        curve.encode_from_light(light).clamp(0.0, 1.0)
    })
}

/// The brightness and the two colour differences, as the levels that BT.709 makes in the limited range of three numbers `red`, `green`, `blue` between 0 and 1 (not rounded).
fn levels_of_bt709_limited([red, green, blue]: [f64; 3]) -> [f64; 3] {
    const RED_PART: f64 = 0.2126;
    const BLUE_PART: f64 = 0.0722;
    let brightness = RED_PART * red + (1.0 - RED_PART - BLUE_PART) * green + BLUE_PART * blue;
    let blue_difference = (blue - brightness) / (2.0 * (1.0 - BLUE_PART));
    let red_difference = (red - brightness) / (2.0 * (1.0 - RED_PART));
    [
        16.0 + 219.0 * brightness,
        128.0 + 224.0 * blue_difference,
        128.0 + 224.0 * red_difference,
    ]
}

/// How far `level` is from the nearest place at which rounding it changes its mind (a half): 0 is the worst, 0.5 the best.
fn distance_from_a_rounding_boundary(level: f64) -> f64 {
    let fraction = level - level.floor();
    (fraction - 0.5).abs().min(0.5 - (fraction - 0.5).abs())
}

fn rounded(levels: [f64; 3]) -> [u8; 3] {
    levels.map(|level| level.round() as u8)
}

/// A frame of one layer, a colour with `opacity` as 128 of 255, put together in linear light and encoded as `curve`.
fn a_flat_frame_encoded_as(curve: TransferFunction, layer: LayerDescription) -> FrameDescription {
    let mut description = FrameDescription::black((6, 4));
    description.colour = FrameColour {
        working: WorkingSpace::Linear(StandardPrimaries::Rec709),
        output: OutputEncoding {
            primaries: StandardPrimaries::Rec709,
            curve,
        },
    };
    description.layers = vec![layer];
    description
}

#[test]
fn the_planes_are_made_of_the_finished_picture_as_it_is_and_not_of_its_8_bit_rounding() {
    const OPACITY: u8 = 128;
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut tried_where_the_two_ways_differ = 0;
        for curve in [
            TransferFunction::Srgb,
            TransferFunction::Gamma22,
            TransferFunction::Gamma24,
        ] {
            for red in (30..=250).step_by(23) {
                for green in (20..=240).step_by(29) {
                    for blue in (10..=250).step_by(37) {
                        let colour = [red, green, blue];
                        let exact = levels_of_bt709_limited(encoded_colour_of_a_layer(
                            colour,
                            f64::from(OPACITY) / 255.0,
                            curve,
                        ));
                        // (A picture rounded to 8 bits first: what the planes used to be made of.)
                        let rounded_first = levels_of_bt709_limited(
                            encoded_colour_of_a_layer(colour, f64::from(OPACITY) / 255.0, curve)
                                .map(|value| (value * 255.0).round() / 255.0),
                        );
                        // Only where the exact level is away from a rounding boundary by more than the card can be off (its half floats are a twentieth of a level coarse in
                        // brightness, and the rest of its arithmetic is far finer) can the card be held to it exactly.
                        if exact
                            .iter()
                            .any(|&level| distance_from_a_rounding_boundary(level) < 0.13)
                        {
                            continue;
                        }
                        let wanted = rounded(exact);
                        if wanted != rounded(rounded_first) {
                            tried_where_the_two_ways_differ += 1;
                        }
                        let description = a_flat_frame_encoded_as(curve, a_colour([red, green, blue, OPACITY]));
                        let (planes, _) = renderer
                            .render_planes(&description, BT709_LIMITED, A_LONG_TIME)
                            .unwrap();
                        for (name, made, wanted) in [
                            ("brightness", planes.luma(), wanted[0]),
                            ("blue colour difference", planes.blue_difference(), wanted[1]),
                            ("red colour difference", planes.red_difference(), wanted[2]),
                        ] {
                            assert!(
                                made.iter().all(|&level| level == wanted),
                                "{curve:?}, {colour:?} at {OPACITY}/255: the {name} is {:?} and the arithmetic says {wanted} (rounded first it would be {:?})",
                                &made[..made.len().min(3)],
                                rounded(rounded_first)
                            );
                        }
                    }
                }
            }
        }
        assert!(
            tried_where_the_two_ways_differ >= 20,
            "a premise: the colours tried are ones where rounding to 8 bits first makes other levels ({tried_where_the_two_ways_differ} of them)"
        );
    });
}

#[test]
fn the_planes_of_a_frame_that_is_mixed_in_light_are_what_the_plain_arithmetic_makes_of_the_8_bit_frame_within_a_level()
{
    // The same frame is asked for twice, as the picture that the preview shows and as planes; the planes are held to the arithmetic of `reference_yuv420p` on the first.
    // (Two things that round are one level apart at most when the one before the other moves by less than a level: the brightness and the colour differences of 8-bit
    // codes are within half a level of those of the exact numbers, whatever the picture.) Opaque and see-through noise, over each other, at odd sizes.
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let working = WorkingSpace::Linear(StandardPrimaries::Rec709);
        for (number, (width, height)) in [(2, 2), (3, 3), (7, 5), (33, 17), (64, 36)].into_iter().enumerate() {
            let pixel_count = (width * height) as usize;
            let mut opaque = noise(pixel_count * 4, number as u64 + 11);
            for pixel in opaque.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
            // (Alpha 1 to 255, and the colour of a see-through pixel is any code: the picture is straight, not premultiplied.)
            let mut see_through = noise(pixel_count * 4, number as u64 + 101);
            for pixel in see_through.as_chunks_mut::<4>().0 {
                pixel[3] = pixel[3].max(1);
            }
            upload(&mut renderer, 1, &picture_for(working, width, height, opaque));
            upload(&mut renderer, 2, &picture_for(working, width, height, see_through));
            let mut description = FrameDescription::black((width, height));
            description.colour = colour_of(working);
            description.layers = vec![the_whole_frame(1), the_whole_frame(2)];
            let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();
            for colour in [
                BT709_LIMITED,
                YuvColour {
                    matrix: YuvMatrix::Bt601,
                    range: YuvRange::Full,
                },
            ] {
                let (planes, _) = renderer.render_planes(&description, colour, A_LONG_TIME).unwrap();
                let (y, u, v) = reference_yuv420p(width, height, &frame.rgba_pixels, colour);
                for (name, made, reference) in [
                    ("brightness", planes.luma(), &y),
                    ("blue colour difference", planes.blue_difference(), &u),
                    ("red colour difference", planes.red_difference(), &v),
                ] {
                    let largest = made
                        .iter()
                        .zip(reference)
                        .map(|(made, reference)| made.abs_diff(*reference))
                        .max()
                        .unwrap();
                    assert!(
                        largest <= 1,
                        "{width} by {height}, {colour:?}: the {name} is off by {largest}"
                    );
                }
            }
        }
    });
}
