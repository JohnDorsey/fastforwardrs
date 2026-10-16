// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour differences of a video picture in planes, made smooth (phase 3, piece 3.7; decision C17 of `docs/engine-phase-3-design.md`): in linear light each pixel of
//! the picture gets its colour difference by blending the ones of the squares of four around it, by where the file says its colour differences sit; in the compatibility
//! mode every pixel of a square has the colour difference of the square, as ffmpeg's pictures had (and the tests of the engine against the old drawing need).

use std::time::Duration;

use super::linear_light_tests::{colour_of, the_whole_frame};
use super::testing::with_software_graphics;
use super::yuv::{chroma_size, reference_rgba, reference_rgba_smooth};
use super::yuv_tests::noise;
use super::{FrameRenderer, PictureColour, PictureData, WorkingSpace, YuvColour, YuvMatrix, YuvRange};
use crate::colour_space::{ChromaSiting, StandardPrimaries};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

const BT709_LIMITED: YuvColour = YuvColour {
    matrix: YuvMatrix::Bt709,
    range: YuvRange::Limited,
};

fn largest_difference(a: &[u8], b: &[u8]) -> u8 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0)
}

/// Planes of a picture: a brightness that does not change (so that only the colour differences make the picture), and colour differences `u` and `v`.
fn planes_with_colour_differences(width: u32, height: u32, u: Vec<u8>, v: Vec<u8>) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (vec![128; (width * height) as usize], u, v)
}

// --- the arithmetic on the processor ---

#[test]
fn the_places_the_colour_differences_sit_at_are_those_of_the_three_standards() {
    // In luma pixels from the centre of the first pixel of the square of four: left is level with the first column and half way down; centre is the middle of the square;
    // top left is on the first pixel.
    assert_eq!(ChromaSiting::Left.offsets_in_luma_pixels(), (0.0, 0.5));
    assert_eq!(ChromaSiting::Center.offsets_in_luma_pixels(), (0.5, 0.5));
    assert_eq!(ChromaSiting::TopLeft.offsets_in_luma_pixels(), (0.0, 0.0));
}

#[test]
fn what_ffprobe_calls_the_places_is_read_and_what_it_does_not_say_is_left() {
    for (name, expected) in [
        (Some("left"), ChromaSiting::Left),
        (Some("center"), ChromaSiting::Center),
        (Some("topleft"), ChromaSiting::TopLeft),
        (None, ChromaSiting::Left),
        (Some("unspecified"), ChromaSiting::Left),
        (Some(""), ChromaSiting::Left),
        (Some("top"), ChromaSiting::Left),
        (Some("bottomleft"), ChromaSiting::Left),
        (Some("something new"), ChromaSiting::Left),
    ] {
        assert_eq!(ChromaSiting::from_ffprobe(name), expected, "{name:?}");
    }
}

/// One row of a picture 4 pixels across (two colour differences) and 2 down (one): the blue difference changes from `low` to `high` along the row, and the red does not.
/// (The values are kept near the middle, so that the blue that they make is not cut off at 0 or 255, where it would not move with them.)
fn one_row_of_two_colour_differences(low: u8, high: u8, siting: ChromaSiting) -> Vec<u8> {
    let (y, u, v) = planes_with_colour_differences(4, 2, vec![low, high], vec![128, 128]);
    reference_rgba_smooth(4, 2, &y, &u, &v, BT709_LIMITED, siting)
}

/// The blue colour difference that a pixel of the row got, found from the blue of the colour it got: the blue is the one that moves with the difference.
fn blue_of(rgba: &[u8], across: usize) -> u8 {
    rgba[across * 4 + 2]
}

#[test]
fn a_pixel_that_is_level_with_a_colour_difference_has_it_and_the_one_between_two_has_the_middle_with_the_left_siting() {
    // Left: the colour differences are at pixels 0 and 2 (level with them). Pixel 1 is half way between: the middle.
    let smooth = one_row_of_two_colour_differences(100, 150, ChromaSiting::Left);
    let (low, high) = (blue_of(&smooth, 0), blue_of(&smooth, 2));
    assert!(
        low < high,
        "a premise: the two colour differences make two blues ({low}, {high})"
    );
    let middle = blue_of(&smooth, 1);
    assert!(
        middle.abs_diff(((u16::from(low) + u16::from(high)) / 2) as u8) <= 1,
        "{low} {middle} {high}"
    );
    // Pixel 3 is past the last colour difference: it has that one.
    assert_eq!(blue_of(&smooth, 3), high);
}

#[test]
fn with_the_centre_siting_the_colour_differences_are_between_the_pixels_and_the_first_and_last_pixels_have_the_nearest()
{
    // Centre: the colour differences are at 0.5 and 2.5. Pixel 0 and pixel 3 are beyond them (they take the nearest); pixels 1 and 2 are 3/4 and 1/4 of the way.
    let smooth = one_row_of_two_colour_differences(100, 150, ChromaSiting::Center);
    let (first, second, third, fourth) = (
        blue_of(&smooth, 0),
        blue_of(&smooth, 1),
        blue_of(&smooth, 2),
        blue_of(&smooth, 3),
    );
    let flat_low = blue_of(&one_row_of_two_colour_differences(100, 100, ChromaSiting::Center), 0);
    let flat_high = blue_of(&one_row_of_two_colour_differences(150, 150, ChromaSiting::Center), 0);
    assert_eq!(
        (first, fourth),
        (flat_low, flat_high),
        "beyond the outermost colour differences, the nearest one"
    );
    assert!(
        first < second && second < third && third < fourth,
        "{first} {second} {third} {fourth}"
    );
    // Pixel 1 is a quarter of the way from the low one to the high one, and pixel 2 three quarters (the blue moves along with the difference, in a straight line).
    let quarter = f32::from(flat_low) + 0.25 * f32::from(flat_high - flat_low);
    let three_quarters = f32::from(flat_low) + 0.75 * f32::from(flat_high - flat_low);
    assert!((f32::from(second) - quarter).abs() <= 1.0, "{second} {quarter}");
    assert!(
        (f32::from(third) - three_quarters).abs() <= 1.0,
        "{third} {three_quarters}"
    );
}

#[test]
fn vertically_the_left_and_centre_sitings_are_half_way_between_the_rows_and_the_top_left_one_is_on_the_first_row() {
    // 2 pixels across, 4 down: two colour differences, one above the other.
    let planes = |siting| {
        let (y, u, v) = planes_with_colour_differences(2, 4, vec![100, 150], vec![128, 128]);
        reference_rgba_smooth(2, 4, &y, &u, &v, BT709_LIMITED, siting)
    };
    let blue_at_row = |rgba: &[u8], row: usize| rgba[row * 2 * 4 + 2];
    let (left, centre, top_left) = (
        planes(ChromaSiting::Left),
        planes(ChromaSiting::Center),
        planes(ChromaSiting::TopLeft),
    );
    // Left and centre: the colour differences are at rows 0.5 and 2.5, so row 1 is 1/4 of the way and row 2 is 3/4 of the way.
    assert_eq!(
        (0..4).map(|row| blue_at_row(&left, row)).collect::<Vec<_>>(),
        (0..4).map(|row| blue_at_row(&centre, row)).collect::<Vec<_>>(),
        "the same vertically"
    );
    // Top left: the colour differences are at rows 0 and 2, so row 1 is half way, row 0 and row 2 have theirs exactly, and row 3 has the last.
    let rows: Vec<u8> = (0..4).map(|row| blue_at_row(&top_left, row)).collect();
    assert!(rows[0] < rows[1] && rows[1] < rows[2], "{rows:?}");
    assert_eq!(rows[3], rows[2]);
    let (low, high) = (f32::from(rows[0]), f32::from(rows[2]));
    assert!((f32::from(rows[1]) - (low + high) / 2.0).abs() <= 1.0, "{rows:?}");
}

#[test]
fn colour_differences_that_are_all_alike_make_the_same_picture_smooth_or_copied() {
    for siting in [ChromaSiting::Left, ChromaSiting::Center, ChromaSiting::TopLeft] {
        for (width, height) in [(1, 1), (2, 2), (3, 3), (7, 5), (16, 9)] {
            let (chroma_width, chroma_height) = chroma_size(width, height);
            let chroma_bytes = (chroma_width * chroma_height) as usize;
            let y = noise((width * height) as usize, 5);
            let (u, v) = (vec![90; chroma_bytes], vec![170; chroma_bytes]);
            let smooth = reference_rgba_smooth(width, height, &y, &u, &v, BT709_LIMITED, siting);
            let copied = reference_rgba(width, height, &y, &u, &v, BT709_LIMITED);
            assert_eq!(smooth, copied, "{siting:?} {width} by {height}");
        }
    }
}

// --- the graphics card ---

#[test]
fn in_linear_light_the_card_blends_the_colour_differences_as_the_arithmetic_on_the_processor_does_at_every_size_and_every_siting()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut number = 0;
        for siting in [ChromaSiting::Left, ChromaSiting::Center, ChromaSiting::TopLeft] {
            for (width, height) in [(1, 1), (2, 2), (3, 3), (4, 2), (2, 4), (7, 5), (16, 9), (33, 17)] {
                number += 1;
                let (chroma_width, chroma_height) = chroma_size(width, height);
                let chroma_bytes = (chroma_width * chroma_height) as usize;
                let y = noise((width * height) as usize, number);
                let u = noise(chroma_bytes, number + 1000);
                let v = noise(chroma_bytes, number + 2000);
                let reference = reference_rgba_smooth(width, height, &y, &u, &v, BT709_LIMITED, siting);

                let data = PictureData::yuv420p8(width, height, y, u, v, BT709_LIMITED)
                    .unwrap()
                    .in_colour(PictureColour::SRGB, LINEAR_REC709)
                    .with_chroma_siting(siting);
                renderer.upload_picture(super::PictureId(1), &data).unwrap();
                let mut description = super::FrameDescription::black((width, height));
                description.colour = colour_of(LINEAR_REC709);
                description.layers = vec![the_whole_frame(1)];
                let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();

                // (The card makes light of the codes and codes of the light again: half floats, so a level or two at the very darkest and lightest.)
                let off = largest_difference(&frame.rgba_pixels, &reference);
                assert!(
                    off <= 2,
                    "{siting:?} {width} by {height}: {off} levels from the arithmetic"
                );
            }
        }
    });
}

#[test]
fn in_the_compatibility_mode_the_card_copies_the_colour_differences_to_the_four_pixels_whatever_the_siting() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for siting in [ChromaSiting::Left, ChromaSiting::Center, ChromaSiting::TopLeft] {
            let (width, height) = (9_u32, 7_u32);
            let (chroma_width, chroma_height) = chroma_size(width, height);
            let chroma_bytes = (chroma_width * chroma_height) as usize;
            let y = noise((width * height) as usize, 3);
            let u = noise(chroma_bytes, 4);
            let v = noise(chroma_bytes, 5);
            let reference = reference_rgba(width, height, &y, &u, &v, BT709_LIMITED);
            let data = PictureData::yuv420p8(width, height, y, u, v, BT709_LIMITED)
                .unwrap()
                .with_chroma_siting(siting);
            renderer.upload_picture(super::PictureId(1), &data).unwrap();
            let mut description = super::FrameDescription::black((width, height));
            description.layers = vec![the_whole_frame(1)];
            let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();

            assert!(
                largest_difference(&frame.rgba_pixels, &reference) <= 1,
                "{siting:?}: the compatibility mode copies"
            );
        }
    });
}

#[test]
fn a_hard_edge_of_colour_is_smooth_in_linear_light_where_it_was_blocky() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 8 pixels across, 2 down: the colour difference jumps from 90 to 170 between the second and the third colour difference of the row.
        let (y, u, v) = planes_with_colour_differences(8, 2, vec![90, 90, 170, 170], vec![128; 4]);
        let mut blue_row = |working: WorkingSpace| -> Vec<u8> {
            let data = PictureData::yuv420p8(8, 2, y.clone(), u.clone(), v.clone(), BT709_LIMITED)
                .unwrap()
                .in_colour(PictureColour::SRGB, working)
                .with_chroma_siting(ChromaSiting::Center);
            renderer.upload_picture(super::PictureId(1), &data).unwrap();
            let mut description = super::FrameDescription::black((8, 2));
            description.colour = colour_of(working);
            description.layers = vec![the_whole_frame(1)];
            let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();
            (0..8).map(|across| frame.rgba_pixels[across * 4 + 2]).collect()
        };

        let blocky = blue_row(WorkingSpace::DisplayReferred);
        let smooth = blue_row(LINEAR_REC709);

        // Blocky: pairs of pixels alike, and one jump of the whole difference between the pairs.
        assert_eq!((blocky[0], blocky[2]), (blocky[1], blocky[3]));
        assert_eq!((blocky[4], blocky[6]), (blocky[5], blocky[7]));
        let jump = blocky[4].abs_diff(blocky[3]);
        assert!(jump > 100, "a premise: the edge is hard ({blocky:?})");
        // Smooth: the pixels on either side of the edge are in steps of about a half, and no step is the whole jump.
        let largest_step = smooth.windows(2).map(|pair| pair[0].abs_diff(pair[1])).max().unwrap();
        assert!(
            largest_step < jump / 2 + 3,
            "no step is more than about half the jump: {smooth:?} against {blocky:?}"
        );
        assert!(
            smooth[3] > blocky[3] && smooth[4] < blocky[4],
            "{smooth:?} against {blocky:?}: the two sides give way to each other"
        );
    });
}
