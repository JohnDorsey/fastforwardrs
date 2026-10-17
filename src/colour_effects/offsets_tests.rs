// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The offsets of a colour matrix, and the matrices of saturation and of brightness and contrast (phase 4, piece 4.3).

use super::tests::applied_to;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// What a colour becomes with the matrix, worked out the plain way in numbers of 0 to 1 that are not rounded and not cut off.
fn what_a_colour_becomes_without_cutting_off(matrix: &ColourMatrix, colour: [f32; 3]) -> [f32; 3] {
    let mut made = [0.0; 3];
    for (place, made) in made.iter_mut().enumerate() {
        *made = (0..3).map(|from| matrix.rows[place][from] * colour[from]).sum::<f32>() + matrix.offsets[place];
    }
    made
}

fn brightness_of(colour: [f32; 3]) -> f32 {
    0.2126 * colour[0] + 0.7152 * colour[1] + 0.0722 * colour[2]
}

#[test]
fn a_saturation_of_one_changes_nothing_exactly_and_of_nothing_makes_every_colour_the_grey_it_is_as_bright_as() {
    assert_eq!(ColourMatrix::saturation(1.0), ColourMatrix::NOTHING_CHANGES);
    let grey = ColourMatrix::saturation(0.0);
    for colour in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [200, 100, 50], [10, 250, 130]] {
        let [red, green, blue] = applied_to(grey, colour[0], colour[1], colour[2]);
        let brightness = brightness_of(colour.map(f32::from));
        assert!(
            red == green && green == blue && (f32::from(red) - brightness).abs() <= 0.51,
            "{colour:?} became ({red}, {green}, {blue}), where its brightness is {brightness}"
        );
    }
    // The brightest red there is, for instance, is as bright as 0.2126 of white: 54 of 255.
    assert_eq!(applied_to(grey, 255, 0, 0), [54, 54, 54]);
}

#[test]
fn a_saturation_keeps_every_grey_and_how_bright_every_colour_is_and_scales_how_far_it_is_from_its_grey() {
    let mut random = DeterministicRandomNumbers::with_seed(8);
    for amount in [0.0, 0.25, 0.5, 1.0, 1.5, 2.0] {
        let matrix = ColourMatrix::saturation(amount);
        for row in matrix.rows {
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{amount}: {row:?}");
        }
        assert_eq!(matrix.offsets, [0.0; 3]);
        for _ in 0..100 {
            let colour = [0, 1, 2].map(|_| random.next_below(1001) as f32 / 1000.0);
            let made = what_a_colour_becomes_without_cutting_off(&matrix, colour);
            assert!(
                (brightness_of(made) - brightness_of(colour)).abs() < 1e-4,
                "{amount}: {colour:?} became {made:?}"
            );
            // How far the colour is from the grey of its brightness is `amount` times as far as it was.
            let grey = brightness_of(colour);
            for place in 0..3 {
                assert!(
                    ((made[place] - grey) - amount * (colour[place] - grey)).abs() < 1e-4,
                    "{amount}: {colour:?} became {made:?}"
                );
            }
        }
    }
    // A colour that is made more colourful than there is room for is cut off.
    let [red, green, blue] = applied_to(ColourMatrix::saturation(2.0), 120, 80, 80);
    assert!(red > 120 && green < 80 && blue < 80, "({red}, {green}, {blue})");
    assert_eq!(applied_to(ColourMatrix::saturation(2.0), 255, 0, 0), [255, 0, 0]);
}

#[test]
fn a_brightness_of_nothing_and_a_contrast_of_one_change_nothing_exactly() {
    assert_eq!(
        ColourMatrix::brightness_and_contrast(0.0, 1.0),
        ColourMatrix::NOTHING_CHANGES
    );
    assert!(ColourMatrix::brightness_and_contrast(0.0, 1.0).changes_nothing());
    assert!(!ColourMatrix::brightness_and_contrast(0.01, 1.0).changes_nothing());
    assert!(!ColourMatrix::brightness_and_contrast(0.0, 1.01).changes_nothing());
    assert!(!ColourMatrix::brightness_and_contrast(0.0, 0.99).changes_nothing());
}

#[test]
fn a_colour_becomes_its_distance_from_the_middle_grey_times_the_contrast_and_the_brightness_more() {
    let mut random = DeterministicRandomNumbers::with_seed(9);
    for (brightness, contrast) in [
        (0.0, 2.0),
        (0.0, 0.5),
        (0.0, 0.0),
        (0.2, 1.0),
        (-0.3, 1.0),
        (0.1, 1.5),
        (-0.1, 0.25),
    ] {
        let matrix = ColourMatrix::brightness_and_contrast(brightness, contrast);
        for _ in 0..100 {
            let colour = [0, 1, 2].map(|_| random.next_below(1001) as f32 / 1000.0);
            let made = what_a_colour_becomes_without_cutting_off(&matrix, colour);
            for place in 0..3 {
                let wanted = (colour[place] - 0.5) * contrast + 0.5 + brightness;
                assert!(
                    (made[place] - wanted).abs() < 1e-5,
                    "brightness {brightness}, contrast {contrast}: {colour:?} became {made:?}"
                );
            }
        }
    }
}

#[test]
fn the_pixels_of_a_picture_are_changed_by_the_offsets_and_cut_off_after_them() {
    // The grey of 128 is half a step above the middle of the 255 steps, so a contrast of 3 makes it three halves of a step above (128 * 3 - 255 = 129), a tenth of the
    // steps brighter is 25.5 more, and what is out of the range is the least or the most there is.
    assert_eq!(
        applied_to(ColourMatrix::brightness_and_contrast(0.0, 3.0), 128, 128, 128),
        [129, 129, 129]
    );
    assert_eq!(
        applied_to(ColourMatrix::brightness_and_contrast(0.1, 1.0), 100, 100, 100),
        [126, 126, 126]
    );
    assert_eq!(
        applied_to(ColourMatrix::brightness_and_contrast(0.0, 2.0), 63, 192, 128),
        [0, 255, 129]
    );
    assert_eq!(
        applied_to(ColourMatrix::brightness_and_contrast(0.0, 0.0), 10, 200, 255),
        [128, 128, 128]
    );
    assert_eq!(
        applied_to(ColourMatrix::brightness_and_contrast(1.0, 1.0), 10, 200, 255),
        [255, 255, 255]
    );
    assert_eq!(
        applied_to(ColourMatrix::brightness_and_contrast(-1.0, 1.0), 10, 200, 255),
        [0, 0, 0]
    );
}

#[test]
fn one_matrix_with_offsets_after_another_is_the_two_done_one_after_the_other_without_cutting_off_between() {
    let mut random = DeterministicRandomNumbers::with_seed(10);
    let first = ColourMatrix::brightness_and_contrast(0.1, 1.5);
    let second = ColourMatrix::saturation(0.5).then(ColourMatrix::brightness_and_contrast(-0.2, 0.75));
    for (before, after) in [(first, second), (second, first), (first, first)] {
        let both = before.then(after);
        for _ in 0..100 {
            let colour = [0, 1, 2].map(|_| random.next_below(1001) as f32 / 1000.0);
            let one_by_one = what_a_colour_becomes_without_cutting_off(
                &after,
                what_a_colour_becomes_without_cutting_off(&before, colour),
            );
            let together = what_a_colour_becomes_without_cutting_off(&both, colour);
            for place in 0..3 {
                assert!(
                    (one_by_one[place] - together[place]).abs() < 1e-4,
                    "{colour:?}: {one_by_one:?} and {together:?}"
                );
            }
        }
    }
    assert_eq!(first.then(ColourMatrix::NOTHING_CHANGES), first);
    assert_eq!(ColourMatrix::NOTHING_CHANGES.then(first), first);
}

#[test]
fn the_offsets_are_a_part_of_what_tells_one_matrix_from_another() {
    let moved = ColourMatrix::brightness_and_contrast(0.1, 1.0);
    assert_eq!(moved.offsets(), [0.1; 3]);
    assert_ne!(moved.bit_pattern(), ColourMatrix::NOTHING_CHANGES.bit_pattern());
    assert_eq!(moved.bit_pattern()[9..], [0.1_f32.to_bits(); 3]);
    assert_eq!(
        moved.bit_pattern()[..9],
        ColourMatrix::NOTHING_CHANGES.bit_pattern()[..9]
    );
    let different_offset = ColourMatrix::brightness_and_contrast(0.2, 1.0);
    assert_ne!(moved.bit_pattern(), different_offset.bit_pattern());
}
