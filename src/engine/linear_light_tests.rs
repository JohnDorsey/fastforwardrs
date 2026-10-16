// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine in **linear light** (phase 3, piece 3.2; the design is `docs/engine-phase-3-design.md`), on the software adapter: what its passes make of pictures, colours and
//! effects when the working space is light, held to the arithmetic of the reference (`colour_space`) and to the properties that linear light is for.
//!
//! The compatibility mode (`WorkingSpace::DisplayReferred`) is what every other test of the engine runs in, so that they pass as they were written; here it is the thing
//! that linear light is compared with, in the places where they must agree (a picture that is only shown) and where they must not (a picture that is mixed).

use std::time::Duration;

use super::testing::with_software_graphics;
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameColour, FrameDescription, FrameRenderer, LayerDescription,
    LayerSource, OutputEncoding, PictureColour, PictureData, PictureId, RenderProblem, RenderedFrame, WorkingSpace,
    YuvColour, YuvMatrix, YuvRange,
};
use crate::colour_effects::ColourMatrix;
use crate::colour_space::{StandardPrimaries, TransferFunction, matrix};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);
const LINEAR_ACESCG: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::AcesCg);

/// A frame that is put together in `working` and shown on an sRGB display.
fn colour_of(working: WorkingSpace) -> FrameColour {
    FrameColour {
        working,
        output: OutputEncoding::SRGB,
    }
}

fn the_whole_frame(picture: u64) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: Vec::new(),
    }
}

fn a_colour(rgba: [u8; 4]) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Solid(rgba),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: Vec::new(),
    }
}

/// The sRGB picture `rgba` (4 bytes a pixel, `width` by `height`), made for `working`.
fn picture_for(working: WorkingSpace, width: u32, height: u32, rgba: Vec<u8>) -> PictureData {
    PictureData::rgba8(width, height, rgba)
        .unwrap()
        .in_colour(PictureColour::SRGB, working)
}

fn upload(renderer: &mut FrameRenderer, id: u64, picture: &PictureData) {
    renderer.upload_picture(PictureId(id), picture).unwrap();
}

fn render(
    renderer: &mut FrameRenderer,
    size: (u32, u32),
    colour: FrameColour,
    layers: Vec<LayerDescription>,
) -> RenderedFrame {
    let mut description = FrameDescription::black(size);
    description.colour = colour;
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap().0
}

/// How many levels (of 8 bits) the two pictures differ by at most, in any number.
fn the_largest_difference(a: &RenderedFrame, b: &RenderedFrame) -> u8 {
    assert_eq!(a.rgba_pixels.len(), b.rgba_pixels.len());
    a.rgba_pixels
        .iter()
        .zip(&b.rgba_pixels)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
}

/// Numbers that look random and are the same every time.
fn noise(count: usize, seed: u64) -> Vec<u8> {
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

/// All 256 codes, in three arrangements, as the pixels of a picture that is 256 across and 1 down, opaque.
fn every_code() -> Vec<u8> {
    (0..=255_u8)
        .flat_map(|code| [code, 255 - code, code.wrapping_mul(7), 255])
        .collect()
}

#[test]
fn a_picture_that_is_only_shown_comes_out_with_the_numbers_it_went_in_with_whatever_the_working_space() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pixels = every_code();
        let compatibility = {
            upload(
                &mut renderer,
                1,
                &picture_for(WorkingSpace::DisplayReferred, 256, 1, pixels.clone()),
            );
            render(
                &mut renderer,
                (256, 1),
                colour_of(WorkingSpace::DisplayReferred),
                vec![the_whole_frame(1)],
            )
        };
        assert_eq!(
            compatibility.rgba_pixels, pixels,
            "the compatibility mode shows what it is given"
        );
        for (working, most_levels_off) in [
            (LINEAR_REC709, 0),
            (LINEAR_ACESCG, 1),
            (WorkingSpace::Linear(StandardPrimaries::Rec2020), 1),
            (WorkingSpace::Linear(StandardPrimaries::DisplayP3), 1),
        ] {
            upload(&mut renderer, 2, &picture_for(working, 256, 1, pixels.clone()));
            let shown = render(&mut renderer, (256, 1), colour_of(working), vec![the_whole_frame(2)]);
            let off = the_largest_difference(&shown, &compatibility);
            assert!(
                off <= most_levels_off,
                "{working:?}: a picture that is only shown is {off} levels from what it was (at most {most_levels_off}), after being made light and back"
            );
        }
    });
}

#[test]
fn half_of_white_over_black_is_half_of_the_light_which_is_the_code_0_735_in_srgb_and_not_the_code_0_5() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // (A colour that is white and 128 of 255 opaque: its opacity is 0.50196, a hair above half.)
        let half = 128.0 / 255.0;
        let expected_in_srgb = (TransferFunction::Srgb.encode_from_light(half) * 255.0).round() as i32;
        assert_eq!(expected_in_srgb, 188, "a premise: the arithmetic of the standard");

        let in_light = render(
            &mut renderer,
            (4, 4),
            colour_of(LINEAR_REC709),
            vec![a_colour([255, 255, 255, 128])],
        );
        let in_codes = render(
            &mut renderer,
            (4, 4),
            colour_of(WorkingSpace::DisplayReferred),
            vec![a_colour([255, 255, 255, 128])],
        );
        assert!(
            (i32::from(in_light.pixel(1, 1)[0]) - expected_in_srgb).abs() <= 1,
            "in light: {:?}",
            in_light.pixel(1, 1)
        );
        assert_eq!(in_codes.pixel(1, 1)[0], 128, "in codes, half is the code 128");

        // The same white, in a picture: premultiplied in light when it is sent, so the half that it is laid over black is the half of the light too.
        let pixels = [255_u8, 255, 255, 128].repeat(4);
        upload(&mut renderer, 1, &picture_for(LINEAR_REC709, 2, 2, pixels));
        let picture = render(
            &mut renderer,
            (4, 4),
            colour_of(LINEAR_REC709),
            vec![the_whole_frame(1)],
        );
        assert!(
            (i32::from(picture.pixel(1, 1)[0]) - expected_in_srgb).abs() <= 1,
            "a picture: {:?}",
            picture.pixel(1, 1)
        );
    });
}

#[test]
fn a_checkerboard_shrunk_by_two_is_the_average_of_the_light_which_is_brighter_than_the_average_of_the_codes() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 8 by 8, a pixel black and the next white, as a chess board: the frame is half the size, so that every pixel of it is the mean of a square of four.
        let checkerboard: Vec<u8> = (0..64_u32)
            .flat_map(|index| {
                let level = if (index % 8 + index / 8) % 2 == 0 { 0 } else { 255 };
                [level, level, level, 255]
            })
            .collect();
        let expected_in_light = (TransferFunction::Srgb.encode_from_light(0.5) * 255.0).round() as i32;
        for (working, expected) in [(LINEAR_REC709, expected_in_light), (WorkingSpace::DisplayReferred, 128)] {
            upload(&mut renderer, 1, &picture_for(working, 8, 8, checkerboard.clone()));
            let shrunk = render(&mut renderer, (4, 4), colour_of(working), vec![the_whole_frame(1)]);
            for pixel in shrunk.rgba_pixels.as_chunks::<4>().0 {
                assert!(
                    (i32::from(pixel[0]) - expected).abs() <= 1,
                    "{working:?}: the mean of four pixels, two black and two white, is {} and should be {expected}",
                    pixel[0]
                );
            }
        }
        assert_eq!(expected_in_light, 188);
    });
}

#[test]
fn the_same_scene_in_two_working_spaces_is_the_same_picture_to_a_level() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A colour all over, and over it a picture of random colours and opacities, turned and made smaller: mixing that is linear in light, which the choice of primaries
        // cannot change (a matrix goes through a mix), so that the pictures are the same whatever the primaries are; it is the proof that every conversion in the chain
        // (the primaries at the way in, and at the way out) is the inverse of the other.
        let size = (48, 32);
        let mut random = noise(24 * 16 * 4, 5);
        for (number, pixel) in random.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            pixel[3] = [255, 128, 40, 255, 200][number % 5];
        }
        let turned_and_smaller = {
            // The corners of a rectangle 0.5 by 0.5 of the frame, turned by 30 degrees about the middle, as fractions of the frame.
            let (sine, cosine) = 30.0_f32.to_radians().sin_cos();
            let corner = |across: f32, down: f32| {
                // (The frame is 48 by 32, so a fraction of its width is not as long as the same fraction of its height: work in pixels.)
                let (x, y) = (across * 24.0, down * 16.0);
                (
                    (24.0 + x * cosine - y * sine) / 48.0,
                    (16.0 + x * sine + y * cosine) / 32.0,
                )
            };
            [
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ]
        };
        let scene = vec![
            a_colour([200, 90, 20, 255]),
            LayerDescription {
                source: LayerSource::Picture(PictureId(1)),
                corners: turned_and_smaller,
                effects: Vec::new(),
            },
        ];
        let mut made = Vec::new();
        for working in [
            LINEAR_REC709,
            LINEAR_ACESCG,
            WorkingSpace::Linear(StandardPrimaries::Rec2020),
        ] {
            upload(&mut renderer, 1, &picture_for(working, 24, 16, random.clone()));
            made.push(render(&mut renderer, size, colour_of(working), scene.clone()));
        }
        for other in &made[1..] {
            let off = the_largest_difference(&made[0], other);
            assert!(off <= 2, "two working spaces make pictures that are {off} levels apart");
        }
        // And the picture is not what it is in codes: the mixing at the edges is of light.
        upload(
            &mut renderer,
            1,
            &picture_for(WorkingSpace::DisplayReferred, 24, 16, random),
        );
        let in_codes = render(&mut renderer, size, colour_of(WorkingSpace::DisplayReferred), scene);
        assert!(
            the_largest_difference(&made[0], &in_codes) > 8,
            "linear light is not what the codes make: the edges and the see-through pixels differ"
        );
    });
}

#[test]
fn the_hue_shift_makes_the_colours_it_made_in_codes_in_any_working_space() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for degrees in [30.0, 120.0, 210.0] {
            let hue = EffectCall::colour_matrix(ColourMatrix::hue_rotation(degrees).rows());
            let layer = LayerDescription {
                effects: vec![hue],
                ..the_whole_frame(1)
            };
            let flat = [180, 90, 45, 255].repeat(16);
            upload(
                &mut renderer,
                1,
                &picture_for(WorkingSpace::DisplayReferred, 4, 4, flat.clone()),
            );
            let in_codes = render(
                &mut renderer,
                (4, 4),
                colour_of(WorkingSpace::DisplayReferred),
                vec![layer.clone()],
            );
            for working in [LINEAR_REC709, LINEAR_ACESCG] {
                upload(&mut renderer, 1, &picture_for(working, 4, 4, flat.clone()));
                let in_light = render(&mut renderer, (4, 4), colour_of(working), vec![layer.clone()]);
                assert!(
                    the_largest_difference(&in_light, &in_codes) <= 1,
                    "{working:?}, a turn of {degrees} degrees: {:?} where it was {:?}",
                    in_light.pixel(0, 0),
                    in_codes.pixel(0, 0)
                );
            }
        }
    });
}

#[test]
fn a_video_picture_in_planes_is_made_light_and_comes_out_as_the_numbers_its_matrix_makes() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for (matrix, range) in [
            (YuvMatrix::Bt709, YuvRange::Limited),
            (YuvMatrix::Bt601, YuvRange::Limited),
            (YuvMatrix::Bt709, YuvRange::Full),
        ] {
            let colour = YuvColour { matrix, range };
            // 8 by 4: a gradient of brightness and two colour differences that change along the picture, so that every part of the arithmetic is used.
            let (width, height) = (8_u32, 4_u32);
            let y: Vec<u8> = (0..width * height).map(|number| 20 + (number * 7) as u8).collect();
            let chroma_count = (width / 2 * (height / 2)) as usize;
            let u: Vec<u8> = (0..chroma_count).map(|number| 100 + (number * 9) as u8).collect();
            let v: Vec<u8> = (0..chroma_count).map(|number| 180 - (number * 11) as u8).collect();
            let compatibility = {
                let data = PictureData::yuv420p8(width, height, y.clone(), u.clone(), v.clone(), colour).unwrap();
                upload(&mut renderer, 1, &data);
                render(
                    &mut renderer,
                    (width, height),
                    colour_of(WorkingSpace::DisplayReferred),
                    vec![the_whole_frame(1)],
                )
            };
            for working in [LINEAR_REC709, LINEAR_ACESCG] {
                let data = PictureData::yuv420p8(width, height, y.clone(), u.clone(), v.clone(), colour)
                    .unwrap()
                    .in_colour(PictureColour::SRGB, working);
                upload(&mut renderer, 2, &data);
                let in_light = render(
                    &mut renderer,
                    (width, height),
                    colour_of(working),
                    vec![the_whole_frame(2)],
                );
                let off = the_largest_difference(&in_light, &compatibility);
                assert!(
                    off <= 1,
                    "{matrix:?} {range:?} in {working:?}: {off} levels from the codes that the matrix makes"
                );
            }
        }
    });
}

#[test]
fn a_colour_all_over_is_made_light_in_the_working_space_as_a_picture_is() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for colour in [
            [255, 0, 0, 255],
            [20, 200, 90, 255],
            [3, 3, 3, 255],
            [250, 250, 250, 255],
        ] {
            let in_codes = render(
                &mut renderer,
                (2, 2),
                colour_of(WorkingSpace::DisplayReferred),
                vec![a_colour(colour)],
            );
            for working in [LINEAR_REC709, LINEAR_ACESCG] {
                let in_light = render(&mut renderer, (2, 2), colour_of(working), vec![a_colour(colour)]);
                assert!(
                    the_largest_difference(&in_light, &in_codes) <= 1,
                    "{colour:?} in {working:?}: {:?} and not {:?}",
                    in_light.pixel(0, 0),
                    in_codes.pixel(0, 0)
                );
            }
        }
    });
}

#[test]
fn a_frame_does_not_use_pictures_that_were_made_for_another_working_space() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        upload(
            &mut renderer,
            1,
            &picture_for(WorkingSpace::DisplayReferred, 2, 2, [10, 20, 30, 255].repeat(4)),
        );
        let mut description = FrameDescription::black((4, 4));
        description.colour = colour_of(LINEAR_REC709);
        description.layers = vec![the_whole_frame(1)];
        match renderer.render(&description, A_LONG_TIME) {
            Err(RenderProblem::PicturesOfAnotherWorkingSpace(ids)) => assert_eq!(ids, vec![PictureId(1)]),
            other => panic!(
                "a refusal was expected: {:?}",
                other.map(|(frame, _)| frame.width_in_pixels)
            ),
        }
        // Sent again, made for the space the frame is in, it is used.
        upload(
            &mut renderer,
            1,
            &picture_for(LINEAR_REC709, 2, 2, [10, 20, 30, 255].repeat(4)),
        );
        assert!(renderer.render(&description, A_LONG_TIME).is_ok());
    });
}

#[test]
fn what_the_card_makes_of_a_picture_is_what_the_reference_arithmetic_makes_in_every_output_encoding() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let width = 64_u32;
        let rgba: Vec<u8> = noise(width as usize * 4, 21)
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect();
        let encodings = [
            OutputEncoding::SRGB,
            OutputEncoding {
                primaries: StandardPrimaries::Rec709,
                curve: TransferFunction::Gamma22,
            },
            OutputEncoding {
                primaries: StandardPrimaries::Rec709,
                curve: TransferFunction::Gamma24,
            },
            OutputEncoding {
                primaries: StandardPrimaries::DisplayP3,
                curve: TransferFunction::Srgb,
            },
        ];
        for working in [LINEAR_REC709, LINEAR_ACESCG] {
            upload(&mut renderer, 1, &picture_for(working, width, 1, rgba.clone()));
            for output in encodings {
                let shown = render(
                    &mut renderer,
                    (width, 1),
                    FrameColour { working, output },
                    vec![the_whole_frame(1)],
                );
                // What the arithmetic of the reference makes of each pixel: its sRGB codes made light, in the primaries of the output, and encoded.
                let to_output = StandardPrimaries::Rec709.conversion_to(output.primaries);
                for (number, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
                    let light = [0, 1, 2]
                        .map(|channel| TransferFunction::Srgb.decode_to_light(f64::from(pixel[channel]) / 255.0));
                    let expected = matrix::apply(&to_output, light)
                        .map(|light| (output.curve.encode_from_light(light).clamp(0.0, 1.0) * 255.0).round() as i32);
                    let made = shown.pixel(number as u32, 0);
                    for channel in 0..3 {
                        assert!(
                            (i32::from(made[channel]) - expected[channel]).abs() <= 1,
                            "{working:?} to {output:?}: the pixel {number} of {pixel:?} is {made:?} where the arithmetic says {expected:?}"
                        );
                    }
                }
            }
        }
    });
}
