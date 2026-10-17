// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The numbers that the engine's own effects (exposure, opacity, blur, crop) give their passes, worked out from the numbers of a call and the size of the picture they work
//! on. There is no graphics card in it, so that what a call comes to can be looked at and tested without one.

use super::plan::KindOfEffect;

/// The widest a blur is: how many pixels each side of a pixel are averaged, at the most. (A blur of 100 points of a picture 1080 high is 100 pixels of standard deviation
/// in it, and three of those each side is 300.)
pub const MOST_PIXELS_EACH_SIDE_OF_A_BLUR: i32 = 300;

/// The factors that the premultiplied red, green, blue and opacity of a picture are multiplied by, for an exposure of `value` stops or an opacity of `value`.
pub fn gain_factors(kind: KindOfEffect, value: f32) -> [f32; 4] {
    match kind {
        KindOfEffect::Exposure => {
            let gain = value.exp2();
            [gain, gain, gain, 1.0]
        }
        _ => {
            let opacity = value.clamp(0.0, 1.0);
            [opacity; 4]
        }
    }
}

/// The standard deviation of the Gaussian of a blur of `radius_in_points` (points of a picture 1080 high), in pixels of a picture that is `picture_height_in_pixels`
/// high, and how many pixels each side of a pixel are averaged (three standard deviations, at least one and at most [`MOST_PIXELS_EACH_SIDE_OF_A_BLUR`]).
pub fn blur_numbers(radius_in_points: f32, picture_height_in_pixels: u32) -> (f32, i32) {
    let sigma = (radius_in_points * picture_height_in_pixels as f32 / 1080.0).max(0.01);
    let half_width = ((3.0 * sigma).ceil() as i32).clamp(1, MOST_PIXELS_EACH_SIDE_OF_A_BLUR);
    (sigma, half_width)
}

/// What the pass of a crop is given for a picture `width` by `height` pixels: the rectangle that is left (the x of its left edge, the y of its top edge, the x of its right
/// edge and the y of its bottom edge, in pixels) and how wide the step across each edge is (the feather, which is a fraction of the height, and one pixel at the least).
pub fn crop_numbers(values: &[f32], (width, height): (u32, u32)) -> ([f32; 4], f32) {
    let [left, top, right, bottom, feather] = [0, 1, 2, 3, 4].map(|place| values[place].clamp(0.0, 1.0));
    let (width, height) = (width as f32, height as f32);
    (
        [
            left * width,
            top * height,
            (1.0 - right) * width,
            (1.0 - bottom) * height,
        ],
        (feather * height).max(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exposure_is_a_gain_of_two_to_the_stops_on_the_colours_and_not_on_the_opacity() {
        assert_eq!(gain_factors(KindOfEffect::Exposure, 0.0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(gain_factors(KindOfEffect::Exposure, 1.0), [2.0, 2.0, 2.0, 1.0]);
        assert_eq!(gain_factors(KindOfEffect::Exposure, -2.0), [0.25, 0.25, 0.25, 1.0]);
        assert_eq!(gain_factors(KindOfEffect::Exposure, 3.0), [8.0, 8.0, 8.0, 1.0]);
    }

    #[test]
    fn an_opacity_is_the_same_factor_on_all_four_numbers_cut_off_at_nought_and_one() {
        assert_eq!(gain_factors(KindOfEffect::Opacity, 0.25), [0.25; 4]);
        assert_eq!(gain_factors(KindOfEffect::Opacity, 1.0), [1.0; 4]);
        assert_eq!(gain_factors(KindOfEffect::Opacity, 7.0), [1.0; 4]);
        assert_eq!(gain_factors(KindOfEffect::Opacity, -7.0), [0.0; 4]);
    }

    #[test]
    fn the_radius_of_a_blur_is_in_points_of_a_picture_1080_high_and_scales_with_the_height_of_the_picture_it_is_done_to()
     {
        assert_eq!(blur_numbers(10.0, 1080), (10.0, 30));
        assert_eq!(blur_numbers(10.0, 540), (5.0, 15));
        assert_eq!(blur_numbers(10.0, 2160), (20.0, 60));
        assert_eq!(blur_numbers(10.0, 720).0, 10.0 * 720.0 / 1080.0);
        // Three standard deviations each side, rounded up, and one at the least.
        assert_eq!(blur_numbers(1.0, 1080), (1.0, 3));
        assert_eq!(blur_numbers(0.4, 1080).1, 2, "1.2 pixels each side, rounded up");
        assert_eq!(blur_numbers(0.01, 1080).1, 1);
        assert_eq!(
            blur_numbers(0.0, 1080).1,
            1,
            "(a blur of nothing is not run, but the numbers are still sane)"
        );
    }

    #[test]
    fn a_blur_is_never_wider_than_the_most_there_is() {
        assert_eq!(blur_numbers(100.0, 1080).1, 300);
        assert_eq!(blur_numbers(100.0, 2160).1, MOST_PIXELS_EACH_SIDE_OF_A_BLUR);
        assert_eq!(blur_numbers(1000.0, 1080).1, MOST_PIXELS_EACH_SIDE_OF_A_BLUR);
    }

    #[test]
    fn the_edges_of_a_crop_are_the_fractions_of_the_size_cut_from_each_side_and_the_feather_is_a_fraction_of_the_height()
     {
        assert_eq!(
            crop_numbers(&[0.25, 0.1, 0.5, 0.0, 0.0], (320, 180)),
            ([80.0, 18.0, 160.0, 180.0], 1.0)
        );
        assert_eq!(
            crop_numbers(&[0.0, 0.0, 0.0, 0.0, 0.1], (320, 180)),
            ([0.0, 0.0, 320.0, 180.0], 18.0)
        );
        // Fractions outside nought to one are cut off to it.
        assert_eq!(
            crop_numbers(&[-1.0, 2.0, 0.5, 0.0, 9.0], (100, 100)),
            ([0.0, 100.0, 50.0, 100.0], 100.0)
        );
    }
}
