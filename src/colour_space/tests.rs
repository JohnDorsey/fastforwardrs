// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour conversions against numbers that were not made by the code: the published matrices and the values of the standards' formulas (worked out again in
//! Python, `scratchpad/colour_check.py`, from the primaries and from the formulas), and against themselves: every round trip, every white that stays white.

use super::matrix::{self, IDENTITY, Matrix3};
use super::{ACES_WHITE, Chromaticity, D65, Primaries, StandardPrimaries, TransferFunction, bradford_adaptation};

fn assert_matrix_close(actual: &Matrix3, expected: &Matrix3, tolerance: f64, what: &str) {
    for row in 0..3 {
        for column in 0..3 {
            assert!(
                (actual[row][column] - expected[row][column]).abs() <= tolerance,
                "{what}: [{row}][{column}] is {} and should be {} (to {tolerance})\nactual {actual:?}\nexpected {expected:?}",
                actual[row][column],
                expected[row][column]
            );
        }
    }
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} should be {expected} (to {tolerance})"
    );
}

// ---- the curves

#[test]
fn the_srgb_curve_gives_the_published_values() {
    // decode(0.5) = ((0.5 + 0.055) / 1.055) ^ 2.4; encode(0.5) = 1.055 * 0.5 ^ (1 / 2.4) - 0.055; 18 % grey is code 0.4614 (worked out in Python).
    assert_close(
        TransferFunction::Srgb.decode_to_light(0.5),
        0.214_041_140_482_232_55,
        1e-12,
        "decode 0.5",
    );
    assert_close(
        TransferFunction::Srgb.encode_from_light(0.5),
        0.735_356_983_052_449_5,
        1e-12,
        "encode 0.5",
    );
    assert_close(
        TransferFunction::Srgb.encode_from_light(0.18),
        0.461_356_129_500_441_64,
        1e-12,
        "18 % grey",
    );
    // The sentence of the gate of phase 3: 50 % of white over black is 0.5 in light and 0.735 in sRGB code.
    assert_eq!((TransferFunction::Srgb.encode_from_light(0.5) * 1000.0).round(), 735.0);
    // The straight piece near black: 12.92 times the light, and a code is 12.92 times less light.
    assert_close(
        TransferFunction::Srgb.encode_from_light(0.001),
        0.012_92,
        1e-12,
        "encode 0.001",
    );
    assert_close(
        TransferFunction::Srgb.decode_to_light(0.02),
        0.02 / 12.92,
        1e-12,
        "decode 0.02",
    );
}

#[test]
fn the_two_pieces_of_the_srgb_curve_meet_at_the_knee_as_well_as_the_standards_constants_let_them() {
    // (The constants of the standard, 0.04045, 0.0031308 and 12.92, are rounded: the pieces meet to 2e-9 in light on the decode side and 3e-8 on the encode side,
    // which is 1e-5 of a level of 8 bits.)
    let power_piece_of_decode = |code: f64| ((code + 0.055) / 1.055).powf(2.4);
    assert_close(
        power_piece_of_decode(0.04045),
        0.04045 / 12.92,
        1e-7,
        "the pieces of decode at the code 0.04045",
    );
    let power_piece_of_encode = |light: f64| 1.055 * light.powf(1.0 / 2.4) - 0.055;
    assert_close(
        power_piece_of_encode(0.003_130_8),
        12.92 * 0.003_130_8,
        1e-7,
        "the pieces of encode at the light 0.0031308",
    );
}

#[test]
fn the_rec709_camera_curve_gives_the_value_of_its_formula_and_its_two_pieces_meet() {
    let beta = 0.018_053_968_510_807;
    assert_close(
        TransferFunction::Rec709.encode_from_light(0.5),
        0.705_435_553_055_618_3,
        1e-12,
        "encode 0.5",
    );
    assert_close(
        TransferFunction::Rec709.encode_from_light(0.2),
        0.433_520_663_278_869_73,
        1e-12,
        "encode 0.2",
    );
    assert_close(TransferFunction::Rec709.encode_from_light(1.0), 1.0, 1e-12, "white");
    // Just below and just above the knee: the straight piece (4.5 times the light) and the power piece give the same code there, which they do not at 0.018.
    let below = TransferFunction::Rec709.encode_from_light(beta - 1e-12);
    let above = TransferFunction::Rec709.encode_from_light(beta + 1e-12);
    assert_close(below, 4.5 * beta, 1e-9, "below the knee");
    assert_close(above, 4.5 * beta, 1e-9, "above the knee");
    // The rounded constants of BT.709 give 0.081 at 0.018 where their power piece gives 0.08125: that is why they are not used.
    assert_close(
        TransferFunction::Rec709.encode_from_light(0.018),
        0.081,
        1e-12,
        "0.018 is below the knee of the precise constants",
    );
}

#[test]
fn the_power_curves_are_powers() {
    assert_close(
        TransferFunction::Gamma22.decode_to_light(0.5),
        0.217_637_640_824_031,
        1e-12,
        "2.2",
    );
    assert_close(
        TransferFunction::Gamma24.decode_to_light(0.5),
        0.189_464_570_813_799_78,
        1e-12,
        "2.4",
    );
    assert_close(
        TransferFunction::Gamma26.decode_to_light(0.5),
        0.164_938_488_846_618_77,
        1e-12,
        "2.6",
    );
    assert_close(
        TransferFunction::Gamma24.encode_from_light(0.5),
        0.749_153_538_438_340_8,
        1e-12,
        "encode with 2.4",
    );
    assert_close(
        TransferFunction::Gamma22.encode_from_light(0.5),
        0.729_740_052_840_723_1,
        1e-12,
        "encode with 2.2",
    );
}

#[test]
fn every_curve_goes_from_black_to_white_rises_and_is_undone_by_its_other_direction() {
    for curve in TransferFunction::ALL {
        assert_eq!(curve.decode_to_light(0.0), 0.0, "{curve:?}");
        assert_close(curve.decode_to_light(1.0), 1.0, 1e-12, &format!("{curve:?} decode 1"));
        assert_eq!(curve.encode_from_light(0.0), 0.0, "{curve:?}");
        assert_close(curve.encode_from_light(1.0), 1.0, 1e-12, &format!("{curve:?} encode 1"));
        let mut earlier = -1.0;
        for step in 0..=1000 {
            let code = f64::from(step) / 1000.0;
            let light = curve.decode_to_light(code);
            assert!(
                light > earlier || (step == 0 && light == 0.0),
                "{curve:?} does not rise at {code}"
            );
            earlier = light;
            assert_close(
                curve.encode_from_light(light),
                code,
                1e-7,
                &format!("{curve:?}: encode(decode({code}))"),
            );
        }
        for step in 0..=1000 {
            let light = f64::from(step) / 1000.0;
            assert_close(
                curve.decode_to_light(curve.encode_from_light(light)),
                light,
                1e-7,
                &format!("{curve:?}: decode(encode({light}))"),
            );
        }
    }
}

#[test]
fn a_code_or_a_light_that_is_out_of_range_gives_a_number_that_a_picture_can_have() {
    for curve in TransferFunction::ALL {
        assert_eq!(curve.decode_to_light(-0.5), 0.0, "{curve:?}: a code below 0 is black");
        assert_close(
            curve.decode_to_light(1.5),
            1.0,
            1e-12,
            &format!("{curve:?}: a code above 1 is white"),
        );
        assert_eq!(curve.decode_to_light(f64::NAN), 0.0, "{curve:?}: not a number is black");
        assert_eq!(
            curve.encode_from_light(-0.25),
            0.0,
            "{curve:?}: a light below 0 is the code 0"
        );
        assert_eq!(
            curve.encode_from_light(f64::NAN),
            0.0,
            "{curve:?}: not a number is the code 0"
        );
        assert!(
            curve.encode_from_light(2.0) > 1.0,
            "{curve:?}: a light brighter than white is a code above 1, for whoever makes the picture to cut off"
        );
    }
}

#[test]
fn the_numbers_for_the_shaders_are_the_places_in_the_list_and_not_the_same_for_two() {
    for (place, curve) in TransferFunction::ALL.into_iter().enumerate() {
        assert_eq!(curve.number_for_the_shaders() as usize, place, "{curve:?}");
    }
    for (place, primaries) in StandardPrimaries::ALL.into_iter().enumerate() {
        assert_eq!(primaries.number_for_the_shaders() as usize, place, "{primaries:?}");
    }
}

#[test]
fn every_curve_and_every_set_of_primaries_has_a_label_that_is_not_the_label_of_another() {
    let curve_labels: Vec<&str> = TransferFunction::ALL.iter().map(|curve| curve.label()).collect();
    let primaries_labels: Vec<&str> = StandardPrimaries::ALL
        .iter()
        .map(|primaries| primaries.label())
        .collect();
    for labels in [curve_labels, primaries_labels] {
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len(), "{labels:?}");
        assert!(labels.iter().all(|label| !label.is_empty()));
    }
}

// ---- the matrices

#[test]
fn the_matrix_of_rec709_to_xyz_is_the_one_of_iec_61966_2_1() {
    // As the standard prints it, to four decimals.
    let published = [
        [0.4124, 0.3576, 0.1805],
        [0.2126, 0.7152, 0.0722],
        [0.0193, 0.1192, 0.9505],
    ];
    assert_matrix_close(&StandardPrimaries::Rec709.rgb_to_xyz(), &published, 5e-5, "sRGB to XYZ");
}

#[test]
fn rec709_to_rec2020_is_the_matrix_of_bt_2087() {
    let published = [
        [0.6274, 0.3293, 0.0433],
        [0.0691, 0.9195, 0.0114],
        [0.0164, 0.0880, 0.8956],
    ];
    assert_matrix_close(
        &StandardPrimaries::Rec709.conversion_to(StandardPrimaries::Rec2020),
        &published,
        5e-5,
        "Rec.709 to Rec.2020",
    );
}

#[test]
fn rec709_to_acescg_and_to_aces2065_are_the_matrices_of_the_aces_documents_to_the_precision_they_have() {
    // The ACES documents round the primaries and the white; the ones made here are exact. The design of phase 3 (section 3.3) says by how much they differ.
    let acescg = [
        [0.613_132_422_4, 0.339_538_015_8, 0.047_416_696_0],
        [0.070_124_380_8, 0.916_394_011_3, 0.013_451_524_0],
        [0.020_587_657_5, 0.109_574_571_6, 0.869_785_404_0],
    ];
    assert_matrix_close(
        &StandardPrimaries::Rec709.conversion_to(StandardPrimaries::AcesCg),
        &acescg,
        1e-4,
        "Rec.709 to ACEScg",
    );
    let ap0 = [
        [0.439_701_0, 0.382_978_0, 0.177_335_0],
        [0.089_792_3, 0.813_439_0, 0.096_761_6],
        [0.017_544_0, 0.111_544_0, 0.870_704_0],
    ];
    assert_matrix_close(
        &StandardPrimaries::Rec709.conversion_to(StandardPrimaries::Aces2065),
        &ap0,
        3e-4,
        "Rec.709 to ACES2065-1",
    );
}

#[test]
fn the_conversions_that_are_not_published_here_are_what_an_independent_derivation_gives() {
    // Worked out in Python from the primaries (`scratchpad/colour_check.py`), not by this code.
    assert_matrix_close(
        &StandardPrimaries::Rec709.conversion_to(StandardPrimaries::DisplayP3),
        &[
            [0.822_461_968_7, 0.177_538_031_3, 0.0],
            [0.033_194_198_9, 0.966_805_801_1, 0.0],
            [0.017_082_630_7, 0.072_397_440_7, 0.910_519_928_6],
        ],
        1e-9,
        "Rec.709 to Display P3",
    );
    assert_matrix_close(
        &StandardPrimaries::Rec709.conversion_to(StandardPrimaries::Bt601Pal),
        &[
            [0.957_814_764_4, 0.042_185_235_6, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -0.011_934_121_9, 1.011_934_121_9],
        ],
        1e-9,
        "Rec.709 to Rec.601 625",
    );
    assert_matrix_close(
        &StandardPrimaries::Rec709.conversion_to(StandardPrimaries::Bt601Ntsc),
        &[
            [1.065_379_033_8, -0.055_400_872_8, -0.009_978_160_9],
            [-0.019_632_549_9, 1.036_363_094_6, -0.016_730_544_7],
            [0.001_632_051_1, 0.004_412_373_2, 0.993_955_575_8],
        ],
        1e-9,
        "Rec.709 to Rec.601 525",
    );
    assert_matrix_close(
        &StandardPrimaries::AcesCg.conversion_to(StandardPrimaries::Rec709),
        &[
            [1.705_050_992_7, -0.621_792_120_7, -0.083_258_872_0],
            [-0.130_256_417_5, 1.140_804_736_6, -0.010_548_319_1],
            [-0.024_003_356_8, -0.128_968_976_1, 1.152_972_332_9],
        ],
        1e-9,
        "ACEScg to Rec.709",
    );
}

#[test]
fn equal_amounts_of_red_green_and_blue_are_the_white_of_the_set_in_every_set() {
    for set in StandardPrimaries::ALL {
        let white = matrix::apply(&set.rgb_to_xyz(), [1.0, 1.0, 1.0]);
        let expected = set.primaries().white.xyz();
        for channel in 0..3 {
            assert_close(
                white[channel],
                expected[channel],
                1e-12,
                &format!("{set:?} white, channel {channel}"),
            );
        }
    }
}

#[test]
fn a_conversion_between_two_sets_keeps_white_white_and_the_other_way_undoes_it() {
    for from in StandardPrimaries::ALL {
        assert_matrix_close(
            &from.conversion_to(from),
            &IDENTITY,
            0.0,
            &format!("{from:?} to itself"),
        );
        for to in StandardPrimaries::ALL {
            let there = from.conversion_to(to);
            let white = matrix::apply(&there, [1.0, 1.0, 1.0]);
            for channel in white {
                assert_close(channel, 1.0, 1e-9, &format!("{from:?} to {to:?}: white"));
            }
            let back = to.conversion_to(from);
            assert_matrix_close(
                &matrix::multiply(&back, &there),
                &IDENTITY,
                1e-9,
                &format!("{from:?} to {to:?} and back"),
            );
        }
    }
}

#[test]
fn the_primaries_of_rec709_are_inside_rec2020_and_the_blue_of_aces2065_is_not_a_colour_and_still_makes_a_matrix() {
    let into_rec2020 = StandardPrimaries::Rec709.conversion_to(StandardPrimaries::Rec2020);
    for primary in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        let converted = matrix::apply(&into_rec2020, primary);
        assert!(
            converted.iter().all(|number| *number >= -1e-12),
            "{converted:?}: Rec.2020 holds Rec.709"
        );
    }
    // Rec.2020 is not inside Rec.709: its green has a negative red in it.
    let out_of_rec709 = matrix::apply(
        &StandardPrimaries::Rec2020.conversion_to(StandardPrimaries::Rec709),
        [0.0, 1.0, 0.0],
    );
    assert!(out_of_rec709.iter().any(|number| *number < -0.1), "{out_of_rec709:?}");
    // (The y of the blue primary of AP0 is below 0: it is not a colour that light can be, which is why AP0 is for exchange and not for compositing.)
    assert!(StandardPrimaries::Aces2065.primaries().blue.y < 0.0);
    assert!(StandardPrimaries::Aces2065.primaries().rgb_to_xyz().is_some());
}

#[test]
fn primaries_that_lie_on_a_line_make_no_matrix() {
    let on_a_line = Primaries {
        red: Chromaticity::new(0.2, 0.2),
        green: Chromaticity::new(0.3, 0.3),
        blue: Chromaticity::new(0.4, 0.4),
        white: D65,
    };
    assert_eq!(on_a_line.rgb_to_xyz(), None);
    assert_eq!(on_a_line.xyz_to_rgb(), None);
}

#[test]
fn the_bradford_adaptation_is_nothing_for_one_white_and_takes_one_white_to_the_other() {
    assert_eq!(bradford_adaptation(D65, D65), IDENTITY);
    let adaptation = bradford_adaptation(D65, ACES_WHITE);
    let adapted = matrix::apply(&adaptation, D65.xyz());
    for (channel, expected) in ACES_WHITE.xyz().into_iter().enumerate() {
        assert_close(adapted[channel], expected, 1e-12, &format!("white, channel {channel}"));
    }
    assert_matrix_close(
        &matrix::multiply(&bradford_adaptation(ACES_WHITE, D65), &adaptation),
        &IDENTITY,
        1e-12,
        "there and back",
    );
}

#[test]
fn the_matrix_functions_multiply_apply_and_undo() {
    let a = [[2.0, 0.0, 1.0], [1.0, 3.0, 2.0], [1.0, 1.0, 1.0]];
    let b = [[1.0, 2.0, 3.0], [0.0, 1.0, 4.0], [5.0, 6.0, 0.0]];
    // (Worked out by hand.)
    assert_eq!(
        matrix::multiply(&a, &b),
        [[7.0, 10.0, 6.0], [11.0, 17.0, 15.0], [6.0, 9.0, 7.0]]
    );
    assert_eq!(matrix::multiply(&a, &IDENTITY), a);
    assert_eq!(matrix::apply(&a, [1.0, 2.0, 3.0]), [5.0, 13.0, 6.0]);
    assert_matrix_close(
        &matrix::multiply(&b, &matrix::inverse(&b).unwrap()),
        &IDENTITY,
        1e-12,
        "b times its inverse",
    );
    assert_eq!(
        matrix::inverse(&[[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 1.0, 1.0]]),
        None,
        "two rows alike"
    );
    assert_eq!(matrix::inverse(&[[f64::NAN; 3]; 3]), None);
    assert_eq!(matrix::inverse(&[[0.0; 3]; 3]), None, "nothing");
    // Rows that are alike to the last digits: the determinant is rounding and not 0, and the "inverse" would be numbers of 1e15.
    assert_eq!(
        matrix::inverse(&[[0.1, 0.2, 0.3], [0.1, 0.2, 0.3 + 1e-15], [0.4, 0.5, 0.6]]),
        None,
        "rows alike to rounding"
    );
    // A matrix with small numbers all through it is still one that can be undone: what counts is the size of the determinant beside the size of the numbers.
    assert!(matrix::inverse(&[[1e-6, 0.0, 0.0], [0.0, 2e-6, 0.0], [0.0, 0.0, 3e-6]]).is_some());
    assert_eq!(
        matrix::to_f32(&IDENTITY),
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    );
}

#[test]
fn the_straight_piece_of_the_srgb_curve_is_used_up_to_and_at_its_knee_and_the_power_above_it() {
    // (IEC 61966-2-1: the straight piece for a code of 0.04045 or less and for a light of 0.0031308 or less. The two pieces meet there to within 3e-8, so only the exact
    // arithmetic of each piece can tell which of them is used: the pieces are written out here as the standard writes them.)
    let srgb = TransferFunction::Srgb;
    for code in [0.0, 0.01, 0.0402, 0.04045] {
        assert_eq!(srgb.decode_to_light(code), code / 12.92, "code {code}");
    }
    for code in [0.04046, 0.1, 0.5, 1.0] {
        assert_eq!(
            srgb.decode_to_light(code),
            ((code + 0.055) / 1.055).powf(2.4),
            "code {code}"
        );
    }
    for light in [0.0, 0.001, 0.0031, 0.0031308] {
        assert_eq!(srgb.encode_from_light(light), 12.92 * light, "light {light}");
    }
    for light in [0.0031309, 0.1, 0.5, 1.0] {
        assert_eq!(
            srgb.encode_from_light(light),
            1.055 * light.powf(1.0 / 2.4) - 0.055,
            "light {light}"
        );
    }
}
