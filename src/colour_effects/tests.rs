// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::DeterministicRandomNumbers;

pub(super) fn applied_to(matrix: ColourMatrix, red: u8, green: u8, blue: u8) -> [u8; 3] {
    let mut pixel = [red, green, blue, 99];
    matrix.apply_to_rgba_pixels(&mut pixel);
    assert_eq!(pixel[3], 99, "how clear it is is not changed");
    [pixel[0], pixel[1], pixel[2]]
}

#[test]
fn a_whole_number_of_turns_changes_nothing_exactly_and_so_does_no_turn() {
    for degrees in [0.0, 360.0, -360.0, 720.0, 1080.0] {
        let matrix = ColourMatrix::hue_rotation(degrees);
        assert_eq!(matrix, ColourMatrix::NOTHING_CHANGES, "{degrees} degrees");
        assert!(matrix.changes_nothing());
    }
    assert!(!ColourMatrix::hue_rotation(1.0).changes_nothing());
    assert!(!ColourMatrix::hue_rotation(-0.5).changes_nothing());
}

#[test]
fn a_turn_so_small_that_no_pixel_can_change_by_it_is_taken_for_no_turn() {
    assert!(ColourMatrix::hue_rotation(0.001).changes_nothing());
    assert!(!ColourMatrix::hue_rotation(0.5).changes_nothing());
}

#[test]
fn the_rows_of_a_turn_add_up_to_one_so_a_grey_stays_the_grey_it_was() {
    let mut random = DeterministicRandomNumbers::with_seed(5);
    for _ in 0..200 {
        let degrees = random.next_below(7200) as f32 / 10.0 - 360.0;
        let matrix = ColourMatrix::hue_rotation(degrees);
        for row in matrix.rows {
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{degrees}: {row:?}");
        }
        for grey in [0, 1, 77, 128, 254, 255] {
            let [red, green, blue] = applied_to(matrix, grey, grey, grey);
            assert!(
                [red, green, blue].iter().all(|channel| channel.abs_diff(grey) <= 1),
                "{degrees} degrees made grey {grey} into ({red}, {green}, {blue})"
            );
        }
    }
}

#[test]
fn a_third_of_a_turn_makes_red_green_and_green_blue_and_blue_red_which_is_the_way_round_the_wheel_of_colours() {
    let third = ColourMatrix::hue_rotation(120.0);
    let [red, green, blue] = applied_to(third, 255, 0, 0);
    assert!(green > red && green > blue, "red became ({red}, {green}, {blue})");
    let [red, green, blue] = applied_to(third, 0, 255, 0);
    assert!(blue > red && blue > green, "green became ({red}, {green}, {blue})");
    let [red, green, blue] = applied_to(third, 0, 0, 255);
    assert!(red > green && red > blue, "blue became ({red}, {green}, {blue})");
}

#[test]
fn half_a_turn_makes_a_colour_into_the_one_that_is_across_the_wheel_from_it() {
    let half = ColourMatrix::hue_rotation(180.0);
    // (Worked out by hand from the numbers of the matrix: -0.574 1.43 0.144 / 0.426 0.43 0.144 / 0.426 1.43 -0.856.)
    assert_eq!(applied_to(half, 200, 100, 50), [35, 135, 185]);
    // A colour that the turn takes outside what a colour can be is the least or the most there is.
    assert_eq!(applied_to(half, 255, 0, 0), [0, 109, 109]);
    assert_eq!(applied_to(half, 0, 255, 255), [255, 146, 146]);
}

#[test]
fn one_turn_after_another_is_the_turn_of_the_two_added_as_far_as_the_numbers_that_css_and_svg_use_go() {
    let mut random = DeterministicRandomNumbers::with_seed(9);
    for _ in 0..100 {
        let first = random.next_below(3600) as f32 / 10.0;
        let second = random.next_below(3600) as f32 / 10.0;
        let both = ColourMatrix::hue_rotation(first).then(ColourMatrix::hue_rotation(second));
        let together = ColourMatrix::hue_rotation(first + second);
        for (row_both, row_together) in both.rows.iter().zip(together.rows) {
            for (both, together) in row_both.iter().zip(row_together) {
                // (Those numbers are a few digits of what they would be to make turns add up exactly.)
                assert!(
                    (both - together).abs() < 5e-3,
                    "{first} and then {second}: {both:?} and {together:?}"
                );
            }
        }
    }
}

#[test]
fn one_matrix_after_another_does_the_first_matrix_first() {
    // (Rewritten on purpose in phase 4: a matrix has offsets too now, so these are made by `from_rows`, which gives them none.)
    let to_green_only = ColourMatrix::from_rows([[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]);
    let swap_red_and_green = ColourMatrix::from_rows([[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
    assert_eq!(
        applied_to(to_green_only.then(swap_red_and_green), 10, 20, 30),
        [20, 0, 0]
    );
    assert_eq!(
        applied_to(swap_red_and_green.then(to_green_only), 10, 20, 30),
        [0, 10, 0]
    );
    assert_eq!(ColourMatrix::NOTHING_CHANGES.then(to_green_only), to_green_only);
    assert_eq!(to_green_only.then(ColourMatrix::NOTHING_CHANGES), to_green_only);
}

#[test]
fn every_pixel_of_a_picture_is_changed_the_same_way_and_what_is_not_a_whole_pixel_at_the_end_is_left() {
    let half = ColourMatrix::hue_rotation(180.0);
    let mut pixels = vec![200, 100, 50, 77, 255, 0, 0, 255, 10, 20, 30, 0, 9, 9];
    half.apply_to_rgba_pixels(&mut pixels);
    assert_eq!(pixels, vec![35, 135, 185, 77, 0, 109, 109, 255, 27, 17, 7, 0, 9, 9]);
}

/// What the colours of a pixel become, worked out the plain way: each is the numbers of a row of the matrix times the red, the
/// green and the blue, added, to the nearest whole number (the least or the most there is if it comes out of the 0 to 255).
fn what_a_pixel_becomes_the_plain_way(matrix: &ColourMatrix, [red, green, blue]: [u8; 3]) -> [u8; 3] {
    let [red, green, blue] = [red, green, blue].map(f32::from);
    matrix.rows.map(|[of_red, of_green, of_blue]| {
        (of_red * red + of_green * green + of_blue * blue)
            .round()
            .clamp(0.0, 255.0) as u8
    })
}

#[test]
fn every_colour_of_a_pixel_becomes_the_nearest_whole_number_of_what_the_matrix_makes_of_it() {
    let mut random = DeterministicRandomNumbers::with_seed(21);
    // Pixels of any colour, and the eight corners of the cube of colours (where what comes out is furthest out of the 0 to 255).
    let mut pixels: Vec<u8> = (0..4 * 30_000).map(|_| random.next_below(256) as u8).collect();
    for corner in 0..8u8 {
        pixels.extend([
            (corner & 1) * 255,
            ((corner >> 1) & 1) * 255,
            ((corner >> 2) & 1) * 255,
            7,
        ]);
    }
    let mut angles = vec![
        1.0, 30.0, 60.0, 90.0, 120.0, 180.0, 240.0, 270.0, 300.0, 359.0, -45.0, 725.5,
    ];
    angles.extend((0..8).map(|_| random.next_below(7200) as f32 / 10.0 - 360.0));
    for degrees in angles {
        let matrix = ColourMatrix::hue_rotation(degrees);
        let mut changed = pixels.clone();

        matrix.apply_to_rgba_pixels(&mut changed);

        for (before, after) in pixels.as_chunks::<4>().0.iter().zip(changed.as_chunks::<4>().0) {
            let expected = what_a_pixel_becomes_the_plain_way(&matrix, [before[0], before[1], before[2]]);
            assert_eq!(
                [after[0], after[1], after[2]],
                expected,
                "{degrees} degrees, {before:?}"
            );
            assert_eq!(after[3], before[3], "how clear it is is not changed");
        }
    }
}

#[test]
fn a_picture_that_a_matrix_that_changes_nothing_is_applied_to_is_the_same_picture() {
    let mut pixels: Vec<u8> = (0..=255).collect();
    let before = pixels.clone();
    ColourMatrix::NOTHING_CHANGES.apply_to_rgba_pixels(&mut pixels);
    assert_eq!(pixels, before);
}
