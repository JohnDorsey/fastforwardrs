// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning the planes of a video picture (brightness, and two colour differences) into red, green and blue: which numbers, and a plain Rust version of
//! the conversion that the tests hold the graphics card's against (CM-08: fixed-function conversions are written once in Rust and once in WGSL, and
//! checked against each other).
//!
//! A picture in `yuv420p` has a brightness (Y) for every pixel and two colour differences (U, the blue one, and V, the red one) for every square of
//! four. The numbers are 8-bit. In *limited* range ("TV" range, which nearly all video is) black is 16 and white 235 for Y, and the colour
//! differences go from 16 to 240 around 128; in *full* range everything uses 0 to 255. How the colour differences make red, green and blue depends on
//! the *matrix*, which a video file says (BT.601 for old and standard-definition video, BT.709 for most high-definition video, BT.2020 for the newest).
//! A file that does not say gets BT.601, because that is what ffmpeg has always done with it, and the pictures of this program must not change by
//! the move to the graphics card: that is the compatibility mode (CM-20). What a better guess for HD video would be is for phase 3.

use serde::{Deserialize, Serialize};

use super::SlotWriter;

/// Which matrix turns the colour differences into red, green and blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YuvMatrix {
    /// ITU-R BT.601 (and what a file that does not say gets).
    Bt601,
    /// ITU-R BT.709.
    Bt709,
    /// ITU-R BT.2020, the one that does not keep the brightness apart (`bt2020nc`).
    Bt2020,
}

impl YuvMatrix {
    /// How much of the brightness of a colour is red and how much is blue (the green is what is left): the two numbers the matrix is made from.
    fn red_and_blue_weights(self) -> (f64, f64) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }
}

/// How the 8-bit numbers of the planes stand for brightness and colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YuvRange {
    /// Brightness from 16 to 235, colour differences from 16 to 240 (what nearly all video is).
    Limited,
    /// Everything from 0 to 255.
    Full,
}

/// A picture's way of standing for colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct YuvColour {
    pub matrix: YuvMatrix,
    pub range: YuvRange,
}

impl YuvColour {
    /// What a video that says nothing about its colours gets: BT.601, limited range (the compatibility mode).
    pub const UNSPECIFIED: YuvColour = YuvColour {
        matrix: YuvMatrix::Bt601,
        range: YuvRange::Limited,
    };

    /// The numbers of the conversion.
    pub fn conversion(self) -> YuvConversion {
        let (red_weight, blue_weight) = self.matrix.red_and_blue_weights();
        let green_weight = 1.0 - red_weight - blue_weight;
        // Brightness plus how much of the red colour difference makes red, and likewise for blue and, from both, for green.
        let red_from_v = 2.0 * (1.0 - red_weight);
        let blue_from_u = 2.0 * (1.0 - blue_weight);
        let green_from_u = -2.0 * blue_weight * (1.0 - blue_weight) / green_weight;
        let green_from_v = -2.0 * red_weight * (1.0 - red_weight) / green_weight;
        // What the numbers are multiplied by first to make the range 0 to 1 for brightness and -0.5 to 0.5 for the colour differences, and what is taken
        // off them before.
        let (brightness_scale, colour_scale, brightness_offset) = match self.range {
            YuvRange::Limited => (255.0 / 219.0, 255.0 / 224.0, 16.0 / 255.0),
            YuvRange::Full => (1.0, 1.0, 0.0),
        };
        let rows = [
            [brightness_scale, 0.0, red_from_v * colour_scale],
            [
                brightness_scale,
                green_from_u * colour_scale,
                green_from_v * colour_scale,
            ],
            [brightness_scale, blue_from_u * colour_scale, 0.0],
        ]
        .map(|row| row.map(|number| number as f32));
        YuvConversion {
            rows,
            offsets: [brightness_offset as f32, (128.0 / 255.0) as f32, (128.0 / 255.0) as f32],
        }
    }
}

/// `rgb = rows * (yuv - offsets)`, with y, u and v as the 8-bit numbers divided by 255, and the result cut off at 0 and 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YuvConversion {
    pub rows: [[f32; 3]; 3],
    pub offsets: [f32; 3],
}

impl YuvConversion {
    /// Writes the numbers as the shader's struct has them: the matrix, and then what is taken off before it is used.
    pub fn write(&self, writer: &mut SlotWriter) {
        writer.mat3x3_from_rows(self.rows).vec3(self.offsets);
    }

    /// The red, green and blue (0 to 1, cut off) of one pixel's 8-bit brightness and colour differences. This is the arithmetic the shader does, in the
    /// same order, in `f32`.
    pub fn rgb_of(&self, y: f32, u: f32, v: f32) -> [f32; 3] {
        let shifted = [y - self.offsets[0], u - self.offsets[1], v - self.offsets[2]];
        self.rows
            .map(|row| (row[0] * shifted[0] + row[1] * shifted[1] + row[2] * shifted[2]).clamp(0.0, 1.0))
    }
}

/// How many pixels across and down a chroma plane of a `yuv420p` picture `width` by `height` is: one for every square of four, and the ones at the
/// edge of an odd size have one for the last single row or column.
pub fn chroma_size(width: u32, height: u32) -> (u32, u32) {
    (width.div_ceil(2), height.div_ceil(2))
}

/// The picture a `yuv420p` picture is, in RGBA bytes (opaque), made on the processor the way the graphics card makes it (it is what the tests hold the
/// card's to, and what stands in where a picture in planes has to be had as RGBA): brightness at every pixel, and the colour differences of the square of
/// four that the pixel is in, each of them copied to the (up to) four pixels it stands for and not blended with those of the squares beside it.
///
/// That is what ffmpeg does when it makes RGBA of a `yuv420p` picture of the size the picture has (and of half that size, and of any size when the picture
/// is turned into its own size by it first), and the pictures of this program must not change by the move to the graphics card (CM-20): measured with
/// ffmpeg against hard-edged colours, copying agrees to within 3 levels where a smooth blend is off by up to 82 (phase 1.3d, `docs/engine-phase-1-design.md`).
/// A smooth blend would be the better picture of a smooth colour; it is for the phases that make the quality better.
pub fn reference_rgba(width: u32, height: u32, y: &[u8], u: &[u8], v: &[u8], colour: YuvColour) -> Vec<u8> {
    let (chroma_width, _) = chroma_size(width, height);
    let conversion = colour.conversion();
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for down in 0..height as usize {
        for across in 0..width as usize {
            let chroma_index = (down / 2) * chroma_width as usize + across / 2;
            let rgb = conversion.rgb_of(
                f32::from(y[down * width as usize + across]) / 255.0,
                f32::from(u[chroma_index]) / 255.0,
                f32::from(v[chroma_index]) / 255.0,
            );
            rgba.extend(rgb.map(|channel| (channel * 255.0 + 0.5) as u8));
            rgba.push(255);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITED_601: YuvColour = YuvColour {
        matrix: YuvMatrix::Bt601,
        range: YuvRange::Limited,
    };

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} is not {expected}");
    }

    /// The numbers that libswscale uses for these (`ff_yuv2rgb_coeffs`: 104597, 132201, 25675, 53279 over 65536 for BT.601 and 117504, 138453, 13954,
    /// 34903 for BT.709, for the limited range), which are the four of the matrix: how much of V makes red, of U makes blue, of U takes off green and of
    /// V takes off green, with the scale of the limited range in them.
    #[test]
    fn the_numbers_are_those_that_libswscale_has_for_the_limited_range() {
        let conversion = LIMITED_601.conversion();
        close(conversion.rows[0][2], 104_597.0 / 65_536.0);
        close(conversion.rows[2][1], 132_201.0 / 65_536.0);
        close(conversion.rows[1][1], -25_675.0 / 65_536.0);
        close(conversion.rows[1][2], -53_279.0 / 65_536.0);
        close(conversion.rows[0][0], 255.0 / 219.0);

        let conversion = YuvColour {
            matrix: YuvMatrix::Bt709,
            range: YuvRange::Limited,
        }
        .conversion();
        close(conversion.rows[0][2], 117_504.0 / 65_536.0);
        close(conversion.rows[2][1], 138_453.0 / 65_536.0);
        close(conversion.rows[1][1], -13_954.0 / 65_536.0);
        close(conversion.rows[1][2], -34_903.0 / 65_536.0);
    }

    #[test]
    fn the_range_decides_what_is_taken_off_and_how_much_the_numbers_are_stretched() {
        let limited = LIMITED_601.conversion();
        close(limited.offsets[0], 16.0 / 255.0);
        close(limited.offsets[1], 128.0 / 255.0);
        close(limited.rows[0][0], 255.0 / 219.0);
        close(limited.rows[0][2], 1.402 * 255.0 / 224.0);
        let full = YuvColour {
            matrix: YuvMatrix::Bt601,
            range: YuvRange::Full,
        }
        .conversion();
        close(full.offsets[0], 0.0);
        close(full.offsets[2], 128.0 / 255.0);
        close(full.rows[0][0], 1.0);
        close(full.rows[0][2], 1.402);
        close(full.rows[2][1], 1.772);
    }

    #[test]
    fn black_and_white_and_grey_are_what_they_should_be_in_each_range() {
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709, YuvMatrix::Bt2020] {
            let limited = YuvColour {
                matrix,
                range: YuvRange::Limited,
            }
            .conversion();
            assert_eq!(
                limited.rgb_of(16.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0),
                [0.0; 3],
                "{matrix:?}: black"
            );
            for channel in limited.rgb_of(235.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0) {
                close(channel, 1.0);
            }
            for channel in limited.rgb_of(125.5 / 255.0, 128.0 / 255.0, 128.0 / 255.0) {
                close(channel, 0.5);
            }
            let full = YuvColour {
                matrix,
                range: YuvRange::Full,
            }
            .conversion();
            assert_eq!(full.rgb_of(0.0, 128.0 / 255.0, 128.0 / 255.0), [0.0; 3]);
            for channel in full.rgb_of(1.0, 128.0 / 255.0, 128.0 / 255.0) {
                close(channel, 1.0);
            }
        }
    }

    /// Every matrix is the inverse of the one that makes brightness and colour differences from red, green and blue (which is the definition of
    /// the matrix): the 8-bit numbers of the primary colours, made the forward way, come back to the colour to within a level.
    #[test]
    fn colours_made_into_brightness_and_colour_differences_the_forward_way_come_back_to_themselves() {
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709, YuvMatrix::Bt2020] {
            for range in [YuvRange::Limited, YuvRange::Full] {
                let conversion = YuvColour { matrix, range }.conversion();
                let (red_weight, blue_weight) = matrix.red_and_blue_weights();
                for rgb in [
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [1.0, 1.0, 0.0],
                    [0.2, 0.5, 0.9],
                    [0.0, 0.0, 0.0],
                    [1.0, 1.0, 1.0],
                ] {
                    let brightness =
                        red_weight * rgb[0] + (1.0 - red_weight - blue_weight) * rgb[1] + blue_weight * rgb[2];
                    let blue_difference = (rgb[2] - brightness) / (2.0 * (1.0 - blue_weight));
                    let red_difference = (rgb[0] - brightness) / (2.0 * (1.0 - red_weight));
                    let eight_bit = |value: f64, low: f64, span: f64| ((low + value * span).round()) / 255.0;
                    let (y, u, v) = match range {
                        YuvRange::Limited => (
                            eight_bit(brightness, 16.0, 219.0),
                            eight_bit(blue_difference, 128.0, 224.0),
                            eight_bit(red_difference, 128.0, 224.0),
                        ),
                        YuvRange::Full => (
                            eight_bit(brightness, 0.0, 255.0),
                            eight_bit(blue_difference, 128.0, 255.0),
                            eight_bit(red_difference, 128.0, 255.0),
                        ),
                    };
                    let back = conversion.rgb_of(y as f32, u as f32, v as f32);
                    for channel in 0..3 {
                        assert!(
                            (f64::from(back[channel]) - rgb[channel]).abs() < 3.0 / 255.0,
                            "{matrix:?} {range:?}: {rgb:?} came back as {back:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_shader_has_the_members_where_the_writer_puts_the_numbers_and_the_struct_is_as_big_as_the_pipeline_says() {
        use crate::engine::pipelines::YUV_UNIFORM_BYTES;
        use crate::engine::{UniformBytes, shaders};
        let module = naga::front::wgsl::parse_str(shaders::YUV).unwrap();
        let (_, global) = module
            .global_variables
            .iter()
            .find(|(_, global)| global.name.as_deref() == Some("conversion"))
            .unwrap();
        let naga::TypeInner::Struct { members, span } = &module.types[global.ty].inner else {
            panic!("not a struct")
        };
        let members: Vec<(String, u32)> = members
            .iter()
            .map(|member| (member.name.clone().unwrap(), member.offset))
            .collect();
        assert_eq!(members, vec![("to_rgb".to_owned(), 0), ("offsets".to_owned(), 48)]);
        assert_eq!(u64::from(*span), YUV_UNIFORM_BYTES);

        let conversion = YuvConversion {
            rows: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
            offsets: [10.0, 11.0, 12.0],
        };
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| conversion.write(writer));
        assert_eq!(u64::from(slot.size), YUV_UNIFORM_BYTES);
        let number = |offset: usize| f32::from_le_bytes(bytes.as_bytes()[offset..offset + 4].try_into().unwrap());
        // Column by column, 16 bytes each: the first column is the first number of each row.
        assert_eq!([number(0), number(4), number(8)], [1.0, 4.0, 7.0]);
        assert_eq!([number(48), number(52), number(56)], [10.0, 11.0, 12.0]);
    }

    #[test]
    fn what_comes_out_is_cut_off_at_the_ends_of_the_range() {
        // A middling brightness with the least possible blue colour difference and the most possible red one makes more red than there is room for
        // (1.3, worked out by hand) and less blue than none (-0.5): both are cut off.
        let conversion = LIMITED_601.conversion();
        let rgb = conversion.rgb_of(0.5, 0.0, 1.0);
        assert!(rgb.iter().all(|channel| (0.0..=1.0).contains(channel)), "{rgb:?}");
        assert_eq!(rgb[0], 1.0);
        assert_eq!(rgb[2], 0.0);
        // And a brightness of 255 is brighter than white in the limited range: cut off at 1.
        assert_eq!(conversion.rgb_of(1.0, 128.0 / 255.0, 128.0 / 255.0), [1.0; 3]);
    }

    #[test]
    fn a_chroma_plane_has_one_pixel_for_every_square_of_four_and_one_for_what_is_left_over_at_an_odd_edge() {
        assert_eq!(chroma_size(8, 6), (4, 3));
        assert_eq!(chroma_size(7, 5), (4, 3));
        assert_eq!(chroma_size(1, 1), (1, 1));
        assert_eq!(chroma_size(2, 1), (1, 1));
        assert_eq!(chroma_size(1, 100), (1, 50));
    }

    #[test]
    fn the_reference_picture_of_flat_planes_is_one_colour_everywhere_and_the_one_the_conversion_gives() {
        let (width, height) = (5, 3);
        let (chroma_width, chroma_height) = chroma_size(width, height);
        let y = vec![81; (width * height) as usize];
        let u = vec![90; (chroma_width * chroma_height) as usize];
        let v = vec![240; (chroma_width * chroma_height) as usize];
        let rgba = reference_rgba(width, height, &y, &u, &v, LIMITED_601);
        // Y 81, U 90, V 240 is red in BT.601 limited range.
        for pixel in rgba.as_chunks::<4>().0 {
            assert!(
                pixel[0] >= 254 && pixel[1] <= 1 && pixel[2] <= 1 && pixel[3] == 255,
                "{pixel:?}"
            );
        }
    }

    const FULL_601: YuvColour = YuvColour {
        matrix: YuvMatrix::Bt601,
        range: YuvRange::Full,
    };

    fn blue_of(rgba: &[u8]) -> Vec<u8> {
        rgba.as_chunks::<4>().0.iter().map(|pixel| pixel[2]).collect()
    }

    #[test]
    fn the_reference_copies_the_colour_of_a_chroma_pixel_to_the_pixels_it_stands_for_and_does_not_blend() {
        // Two chroma pixels across (U = 100 and 200) for four pixels of picture across: each is for two of them.
        let rgba = reference_rgba(4, 1, &[128; 4], &[100, 200], &[128, 128], FULL_601);
        let blue = blue_of(&rgba);
        let alone = |u: u8| reference_rgba(1, 1, &[128], &[u], &[128], FULL_601)[2];
        assert_eq!(blue, vec![alone(100), alone(100), alone(200), alone(200)]);
        assert_ne!(
            alone(100),
            alone(200),
            "the two colours are not the same, or this shows nothing"
        );
    }

    #[test]
    fn the_reference_gives_the_pixels_of_an_odd_edge_the_colour_of_the_last_single_chroma_pixel_and_goes_down_by_rows_of_squares()
     {
        // 3 across and 3 down: chroma is 2 across and 2 down. Pixel (x, y) takes the chroma pixel (x / 2, y / 2).
        let u = [10, 20, 30, 40];
        let v = [128; 4];
        let rgba = reference_rgba(3, 3, &[128; 9], &u, &v, FULL_601);
        let blue = blue_of(&rgba);
        let alone = |u: u8| reference_rgba(1, 1, &[128], &[u], &[128], FULL_601)[2];
        let expected: Vec<u8> = [[10, 10, 20], [10, 10, 20], [30, 30, 40]]
            .into_iter()
            .flatten()
            .map(alone)
            .collect();
        assert_eq!(blue, expected);
    }
}
