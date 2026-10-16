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
//!
//! The export goes the other way: the finished frame (in half floats, not yet rounded to 8 bits) is made into the planes of `yuv420p` for the encoder, with the same matrix and range
//! ([`YuvColour::forward_conversion`], [`reference_yuv420p`]). The colour differences of a square of four pixels are the average of those of its four pixels.

use serde::{Deserialize, Serialize};

use super::SlotWriter;
use super::colour::IngestConversion;

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

    /// The numbers of the conversion the other way: from red, green and blue to brightness and colour differences, with the weights of the standard (`Y = Kr R + Kg G +
    /// Kb B`, `U = (B - Y) / (2 (1 - Kb))`, `V = (R - Y) / (2 (1 - Kr))`) and the scale and offsets of the range. It is the inverse of [`Self::conversion`].
    pub fn forward_conversion(self) -> RgbToYuvConversion {
        let (red_weight, blue_weight) = self.matrix.red_and_blue_weights();
        let green_weight = 1.0 - red_weight - blue_weight;
        // How much of 0 to 1 the 8-bit numbers of the brightness and of the colour differences span, divided by 255, and where they begin.
        let (brightness_scale, colour_scale, brightness_offset) = match self.range {
            YuvRange::Limited => (219.0 / 255.0, 224.0 / 255.0, 16.0 / 255.0),
            YuvRange::Full => (1.0, 1.0, 0.0),
        };
        let brightness = [red_weight, green_weight, blue_weight].map(|weight| weight * brightness_scale);
        let blue_difference = [
            -red_weight / (2.0 * (1.0 - blue_weight)),
            -green_weight / (2.0 * (1.0 - blue_weight)),
            0.5,
        ]
        .map(|number| number * colour_scale);
        let red_difference = [
            0.5,
            -green_weight / (2.0 * (1.0 - red_weight)),
            -blue_weight / (2.0 * (1.0 - red_weight)),
        ]
        .map(|number| number * colour_scale);
        RgbToYuvConversion {
            rows: [brightness, blue_difference, red_difference].map(|row| row.map(|number| number as f32)),
            offsets: [brightness_offset as f32, (128.0 / 255.0) as f32, (128.0 / 255.0) as f32],
        }
    }

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

/// The conversion the other way: `yuv = rows * rgb + offsets`, with red, green and blue from 0 to 1 and y, u and v as the 8-bit numbers divided by 255. Nothing is
/// cut off here: the 8-bit picture that the card writes does that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RgbToYuvConversion {
    pub rows: [[f32; 3]; 3],
    pub offsets: [f32; 3],
}

impl RgbToYuvConversion {
    /// Writes the numbers as the shaders' struct has them: three rows of a `vec4<f32>` each, the three numbers that multiply red, green and blue and then what is
    /// added.
    pub fn write(&self, writer: &mut SlotWriter) {
        for (row, offset) in self.rows.iter().zip(self.offsets) {
            writer.vec4([row[0], row[1], row[2], offset]);
        }
    }

    /// What one row makes of a colour, without its offset: the arithmetic the shaders do, in the same order, in `f32`.
    pub fn row_of(&self, row: usize, rgb: [f32; 3]) -> f32 {
        self.rows[row][0] * rgb[0] + self.rows[row][1] * rgb[1] + self.rows[row][2] * rgb[2]
    }

    /// The brightness and the two colour differences of one colour (as the 8-bit numbers divided by 255, not cut off).
    pub fn yuv_of(&self, rgb: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|row| self.row_of(row, rgb) + self.offsets[row])
    }
}

impl YuvConversion {
    /// Writes the numbers as the shader's struct has them: the matrix, what is taken off before it is used, and then, in the four bytes that the offsets leave over of
    /// their sixteen, the number of the curve of the codes, and the matrix that makes the light of the picture's primaries that of the working space (`ingest`: the
    /// same numbers as a picture of bytes is given).
    pub fn write(&self, writer: &mut SlotWriter, ingest: &IngestConversion) {
        writer
            .mat3x3_from_rows(self.rows)
            .vec3(self.offsets)
            .u32(ingest.curve)
            .mat3x3_from_rows(ingest.to_working);
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

/// The planes of the `yuv420p` picture that an RGBA picture is (the alpha is not looked at), made on the processor the way the graphics card makes them (it is what
/// the tests hold the card's to): the brightness of every pixel, and for every square of four the **average** of the colour differences of its four pixels (at an odd
/// edge the pixel that is there is counted again for the one that is not). The numbers are rounded to the nearest and cut off at the ends of the 8 bits. The planes are
/// returned in order, the brightness, the blue colour difference and the red one.
pub fn reference_yuv420p(width: u32, height: u32, rgba: &[u8], colour: YuvColour) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (width, height) = (width as usize, height as usize);
    let (chroma_width, chroma_height) = chroma_size(width as u32, height as u32);
    let conversion = colour.forward_conversion();
    let colour_at = |across: usize, down: usize| -> [f32; 3] {
        let first = (down.min(height - 1) * width + across.min(width - 1)) * 4;
        [rgba[first], rgba[first + 1], rgba[first + 2]].map(|byte| f32::from(byte) / 255.0)
    };
    let eight_bit = |value: f32| (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;

    let mut y = Vec::with_capacity(width * height);
    for down in 0..height {
        for across in 0..width {
            y.push(eight_bit(conversion.yuv_of(colour_at(across, down))[0]));
        }
    }
    let mut u = Vec::with_capacity(chroma_width as usize * chroma_height as usize);
    let mut v = Vec::with_capacity(u.capacity());
    for down in 0..chroma_height as usize {
        for across in 0..chroma_width as usize {
            let (mut blue_sum, mut red_sum) = (0.0f32, 0.0f32);
            for (step_across, step_down) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let rgb = colour_at(across * 2 + step_across, down * 2 + step_down);
                blue_sum += conversion.row_of(1, rgb);
                red_sum += conversion.row_of(2, rgb);
            }
            u.push(eight_bit(blue_sum / 4.0 + conversion.offsets[1]));
            v.push(eight_bit(red_sum / 4.0 + conversion.offsets[2]));
        }
    }
    (y, u, v)
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
        // (Rewritten on purpose in phase 3: the struct has the number of the curve, in the four bytes that the offsets leave over, and the matrix to the working space.)
        assert_eq!(
            members,
            vec![
                ("to_rgb".to_owned(), 0),
                ("offsets".to_owned(), 48),
                ("curve".to_owned(), 60),
                ("to_working".to_owned(), 64)
            ]
        );
        assert_eq!(u64::from(*span), YUV_UNIFORM_BYTES);

        let conversion = YuvConversion {
            rows: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
            offsets: [10.0, 11.0, 12.0],
        };
        let ingest = IngestConversion {
            to_working: [[13.0, 14.0, 15.0], [16.0, 17.0, 18.0], [19.0, 20.0, 21.0]],
            curve: 4,
        };
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| conversion.write(writer, &ingest));
        assert_eq!(u64::from(slot.size), YUV_UNIFORM_BYTES);
        let number = |offset: usize| f32::from_le_bytes(bytes.as_bytes()[offset..offset + 4].try_into().unwrap());
        // Column by column, 16 bytes each: the first column is the first number of each row.
        assert_eq!([number(0), number(4), number(8)], [1.0, 4.0, 7.0]);
        assert_eq!([number(48), number(52), number(56)], [10.0, 11.0, 12.0]);
        assert_eq!(
            u32::from_le_bytes(bytes.as_bytes()[60..64].try_into().unwrap()),
            4,
            "the curve, after the offsets"
        );
        assert_eq!(
            [number(64), number(68), number(72)],
            [13.0, 16.0, 19.0],
            "the first column of the matrix to the working space"
        );
        assert_eq!(
            [number(96), number(100), number(104)],
            [15.0, 18.0, 21.0],
            "and the last"
        );
    }

    /// The four numbers of each standard's matrix for the full range, as the standards give them (ITU-R BT.601, BT.709 and BT.2020, the colour differences
    /// that do not keep the brightness apart): how much of V makes red, of U makes blue, of U takes off green and of V takes off green.
    #[test]
    fn the_matrix_of_each_standard_has_the_numbers_that_the_standard_gives_for_the_full_range() {
        for (matrix, red_from_v, blue_from_u, green_from_u, green_from_v) in [
            (YuvMatrix::Bt601, 1.402, 1.772, -0.344_136, -0.714_136),
            (YuvMatrix::Bt709, 1.5748, 1.8556, -0.187_324, -0.468_124),
            (YuvMatrix::Bt2020, 1.4746, 1.8814, -0.164_553, -0.571_353),
        ] {
            let conversion = YuvColour {
                matrix,
                range: YuvRange::Full,
            }
            .conversion();
            close(conversion.rows[0][2], red_from_v);
            close(conversion.rows[2][1], blue_from_u);
            close(conversion.rows[1][1], green_from_u);
            close(conversion.rows[1][2], green_from_v);
            // (And the brightness is the brightness: each of red, green and blue has all of it, and no colour difference of its own in the red and the blue.)
            for row in conversion.rows {
                close(row[0], 1.0);
            }
            close(conversion.rows[0][1], 0.0);
            close(conversion.rows[2][2], 0.0);
        }
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

    /// Brightness and colour differences of the primary colours as the standards (and the books on video) give them, in the 8-bit numbers: BT.601 and BT.709 for the
    /// limited range (red 82, 90, 240 and green 145, 54, 34 and blue 41, 240, 110 in BT.601; 63, 102, 240 and 173, 42, 26 and 32, 240, 118 in BT.709), and JFIF's
    /// (BT.601, full range: red 76.2, 85.0, 255.5). (BT.2020's published numbers are those of the matrix the other way, which `the_matrix_of_each_standard_has_the_numbers_that_the_standard_gives_for_the_full_range`
    /// has, and this is its inverse.)
    #[test]
    fn the_numbers_the_standards_give_for_the_primary_colours_are_what_the_forward_conversion_makes() {
        let eight_bit =
            |colour: YuvColour, rgb: [f32; 3]| colour.forward_conversion().yuv_of(rgb).map(|number| number * 255.0);
        let limited = |matrix| YuvColour {
            matrix,
            range: YuvRange::Limited,
        };
        for (colour, rgb, expected) in [
            (limited(YuvMatrix::Bt601), [1.0, 0.0, 0.0], [81.48, 90.22, 240.0]),
            (limited(YuvMatrix::Bt601), [0.0, 1.0, 0.0], [144.55, 53.79, 34.21]),
            (limited(YuvMatrix::Bt601), [0.0, 0.0, 1.0], [40.97, 240.0, 109.79]),
            (limited(YuvMatrix::Bt709), [1.0, 0.0, 0.0], [62.56, 102.34, 240.0]),
            (limited(YuvMatrix::Bt709), [0.0, 1.0, 0.0], [172.64, 41.66, 26.28]),
            (limited(YuvMatrix::Bt709), [0.0, 0.0, 1.0], [31.81, 240.0, 117.73]),
            (FULL_601, [1.0, 0.0, 0.0], [76.245, 84.97, 255.5]),
            (FULL_601, [0.0, 0.0, 1.0], [29.07, 255.5, 107.265]),
        ] {
            let made = eight_bit(colour, rgb);
            for channel in 0..3 {
                assert!(
                    (made[channel] - expected[channel]).abs() < 0.1,
                    "{colour:?} {rgb:?}: {made:?} is not {expected:?}"
                );
            }
        }
    }

    #[test]
    fn black_and_white_are_the_ends_of_the_range_with_no_colour_in_every_matrix() {
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709, YuvMatrix::Bt2020] {
            for (range, black, white) in [(YuvRange::Limited, 16.0, 235.0), (YuvRange::Full, 0.0, 255.0)] {
                let colour = YuvColour { matrix, range };
                let eight_bit = |rgb: [f32; 3]| colour.forward_conversion().yuv_of(rgb).map(|number| number * 255.0);
                let (dark, light) = (eight_bit([0.0; 3]), eight_bit([1.0; 3]));
                for (made, expected) in dark.into_iter().zip([black, 128.0, 128.0]) {
                    assert!((made - expected).abs() < 1e-3, "{matrix:?} {range:?} black: {dark:?}");
                }
                for (made, expected) in light.into_iter().zip([white, 128.0, 128.0]) {
                    assert!((made - expected).abs() < 1e-3, "{matrix:?} {range:?} white: {light:?}");
                }
            }
        }
    }

    /// The conversion the other way is the inverse of this one, for every matrix and range: a colour made into brightness and colour differences and back is
    /// itself (the numbers are not rounded to 8 bits here, so only the float arithmetic is between them).
    #[test]
    fn the_forward_conversion_is_the_inverse_of_the_other_way_for_every_matrix_and_range() {
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709, YuvMatrix::Bt2020] {
            for range in [YuvRange::Limited, YuvRange::Full] {
                let colour = YuvColour { matrix, range };
                let (forward, backward) = (colour.forward_conversion(), colour.conversion());
                for red in 0..=4 {
                    for green in 0..=4 {
                        for blue in 0..=4 {
                            let rgb = [red, green, blue].map(|step| step as f32 / 4.0);
                            let [y, u, v] = forward.yuv_of(rgb);
                            let back = backward.rgb_of(y, u, v);
                            for channel in 0..3 {
                                assert!(
                                    (back[channel] - rgb[channel]).abs() < 2e-5,
                                    "{matrix:?} {range:?}: {rgb:?} came back as {back:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_forward_conversion_is_written_as_three_rows_of_four_numbers_the_row_and_then_what_is_added() {
        use crate::engine::UniformBytes;
        let conversion = RgbToYuvConversion {
            rows: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
            offsets: [10.0, 11.0, 12.0],
        };
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| conversion.write(writer));
        assert_eq!(slot.size, 48);
        let numbers: Vec<f32> = bytes.as_bytes()[..48]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect();
        assert_eq!(
            numbers,
            vec![1.0, 2.0, 3.0, 10.0, 4.0, 5.0, 6.0, 11.0, 7.0, 8.0, 9.0, 12.0]
        );
    }

    #[test]
    fn the_planes_of_a_flat_picture_are_flat_and_the_numbers_of_its_colour() {
        let (width, height) = (5, 3);
        let red = [255, 0, 0, 255].repeat((width * height) as usize);
        let (y, u, v) = reference_yuv420p(width, height, &red, LIMITED_601);
        assert_eq!((y.len(), u.len(), v.len()), (15, 6, 6));
        assert!(y.iter().all(|number| *number == 81), "{y:?}");
        assert!(u.iter().all(|number| *number == 90), "{u:?}");
        assert!(v.iter().all(|number| *number == 240), "{v:?}");
    }

    #[test]
    fn the_numbers_of_the_planes_are_rounded_to_the_nearest_and_not_cut_off_where_the_fraction_ends() {
        // Green in BT.601 limited range is 144.55, 53.79 and 34.21 (brightness, blue and red colour difference): 145, 54 and 34.
        let (y, u, v) = reference_yuv420p(2, 2, &[0, 255, 0, 255].repeat(4), LIMITED_601);
        assert_eq!((y[0], u[0], v[0]), (145, 54, 34));
    }

    #[test]
    fn the_colour_differences_of_a_square_of_four_are_the_average_of_those_of_its_four_pixels() {
        // Red, green, blue and white in the four corners of a square.
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let (y, u, v) = reference_yuv420p(2, 2, &rgba, FULL_601);
        let row = |rgb: [f32; 3], row: usize| FULL_601.forward_conversion().row_of(row, rgb);
        let colours = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 1.0]];
        let average = |number: usize| colours.iter().map(|rgb| row(*rgb, number)).sum::<f32>() / 4.0 + 128.0 / 255.0;
        assert_eq!(u, vec![(average(1) * 255.0 + 0.5) as u8]);
        assert_eq!(v, vec![(average(2) * 255.0 + 0.5) as u8]);
        // Each pixel has its own brightness.
        assert_eq!(y.len(), 4);
        assert_ne!(y[0], y[1]);
        assert_ne!(y[1], y[2]);
    }

    #[test]
    fn at_an_odd_edge_the_pixel_that_is_there_stands_for_the_one_that_is_not() {
        // 3 across and 1 down: the second colour difference of each plane is for the last pixel alone (counted for the missing pixel to its right, and for the
        // missing row below it).
        let rgba = [255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255];
        let (_, u, v) = reference_yuv420p(3, 1, &rgba, FULL_601);
        let alone = |rgba: &[u8]| reference_yuv420p(1, 1, rgba, FULL_601);
        let (_, blue_u, blue_v) = alone(&[0, 0, 255, 255]);
        assert_eq!(u.len(), 2);
        assert_eq!((u[1], v[1]), (blue_u[0], blue_v[0]));
        let (_, red_u, red_v) = alone(&[255, 0, 0, 255]);
        assert_eq!((u[0], v[0]), (red_u[0], red_v[0]));
    }

    #[test]
    fn colours_made_into_planes_and_back_and_into_planes_again_give_the_same_planes_within_a_level() {
        // A picture of flat squares of four (so that the colour differences of a square are one), of colours that are inside what red, green and blue can be.
        let (width, height) = (8usize, 6usize);
        let mut rgba = Vec::new();
        for down in 0..height {
            for across in 0..width {
                let square = (across / 2 + 4 * (down / 2)) as u32;
                rgba.extend([
                    (40 + 17 * square % 180) as u8,
                    (60 + 29 * square % 150) as u8,
                    (30 + 41 * square % 190) as u8,
                    255,
                ]);
            }
        }
        for colour in [LIMITED_601, FULL_601] {
            let (y, u, v) = reference_yuv420p(width as u32, height as u32, &rgba, colour);
            let back = reference_rgba(width as u32, height as u32, &y, &u, &v, colour);
            let (again_y, again_u, again_v) = reference_yuv420p(width as u32, height as u32, &back, colour);
            for (before, after) in [(&y, &again_y), (&u, &again_u), (&v, &again_v)] {
                assert!(
                    before.iter().zip(after.iter()).all(|(a, b)| a.abs_diff(*b) <= 1),
                    "{colour:?}: {before:?} against {after:?}"
                );
            }
            // And the colours are the ones that went in, within the few levels that a colour difference of 8 bits for a square costs.
            assert!(
                rgba.iter().zip(&back).all(|(a, b)| a.abs_diff(*b) <= 4),
                "{colour:?}: the colours did not come back"
            );
        }
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
