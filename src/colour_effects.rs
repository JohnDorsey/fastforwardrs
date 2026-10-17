// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the effects that change the colours of a picture do to its pixels, and how ffmpeg is asked to do the same, so that the
//! preview and the export agree.
//!
//! Each of them is a [`ColourMatrix`]: every pixel's red, green and blue are mixed to make the red, green and blue it becomes,
//! the same way for every pixel of the picture, and three numbers (offsets) are added to what the mixing makes, which brightness and contrast
//! need. (How clear a pixel is does not change.) What the matrix is, for one of the
//! effects, is worked out by the effect (see [`crate::project::EffectKind`]); this is the arithmetic that they have in common.
//! It has no other parts of the program in it, so that it can be tested alone.

/// How the red, green and blue of a pixel are mixed to make the red, green and blue of the pixel it becomes: each row says how
/// much of the red, the green and the blue the new red (the first row), green (the second) and blue (the third) is made of; and the
/// three offsets are added to the new red, green and blue (as fractions of the 255 steps of a colour), after the mixing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColourMatrix {
    rows: [[f32; 3]; 3],
    offsets: [f32; 3],
}

/// The weights of the red, the green and the blue in how bright a colour is (Rec. 709).
const LUMA_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// How different a number of a matrix can be from what it is in the matrix that changes nothing, for it to be taken for
/// the one that changes nothing: a change that is less than a hundredth of a step of the 256 steps of a colour changes no pixel.
const NEAR_ENOUGH_TO_CHANGE_NOTHING: f32 = 1.0 / 256.0 / 100.0;

impl ColourMatrix {
    /// The matrix that changes nothing.
    pub const NOTHING_CHANGES: Self = Self {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        offsets: [0.0; 3],
    };

    /// The matrix with these rows (how much of the red, the green and the blue the new red is made of, then the new green, then the new blue). For the
    /// tests, which make matrices that no effect makes.
    #[cfg(test)]
    pub fn from_rows(rows: [[f32; 3]; 3]) -> Self {
        Self {
            rows,
            offsets: [0.0; 3],
        }
    }

    /// The matrix with these rows and offsets. For the tests, which make matrices that no effect makes.
    #[cfg(test)]
    pub fn from_rows_and_offsets(rows: [[f32; 3]; 3], offsets: [f32; 3]) -> Self {
        Self { rows, offsets }
    }

    /// The rows of the matrix: how much of the red, the green and the blue the new red (the first row), green (the second) and blue (the third) is
    /// made of.
    pub fn rows(&self) -> [[f32; 3]; 3] {
        self.rows
    }

    /// The numbers that are added to the new red, green and blue after the mixing, as fractions of a whole colour (0 to 1).
    pub fn offsets(&self) -> [f32; 3] {
        self.offsets
    }

    /// The matrix that makes the colours of a picture as colourful as `amount` says: 1 is as they are, 0 is the shades of grey they are as bright as (by the weights of
    /// Rec. 709), and more than 1 is more colourful than they are (a colour that comes out of the range of colours is cut off). A grey stays the grey it was for any amount.
    pub fn saturation(amount: f32) -> Self {
        let mut rows = [[0.0; 3]; 3];
        for (row, made) in rows.iter_mut().enumerate() {
            for (column, number) in made.iter_mut().enumerate() {
                *number = (1.0 - amount) * LUMA_WEIGHTS[column] + if row == column { amount } else { 0.0 };
            }
        }
        Self {
            rows,
            offsets: [0.0; 3],
        }
    }

    /// The matrix that makes the colours of a picture brighter by `brightness` (a fraction of a whole colour: 0.1 is a tenth of the steps more) and further from the
    /// middle grey, or nearer to it, by `contrast` (1 is as they are, 2 is twice as far, 0 is all the middle grey): a colour `x` becomes `(x - 0.5) * contrast + 0.5 +
    /// brightness`, cut off at 0 and 1. (The codes of a screen are what it is made for.)
    pub fn brightness_and_contrast(brightness: f32, contrast: f32) -> Self {
        let offset = 0.5 - 0.5 * contrast + brightness;
        Self {
            rows: [[contrast, 0.0, 0.0], [0.0, contrast, 0.0], [0.0, 0.0, contrast]],
            offsets: [offset; 3],
        }
    }

    /// The matrix that turns the hues of the colours of a picture round the wheel of colours by `degrees` (clockwise, as the wheel
    /// is usually drawn: red turned by 120 degrees is green), as the hue rotation of CSS and SVG does, keeping how bright
    /// a colour is as far as that can be done. A whole number of turns changes nothing, exactly.
    pub fn hue_rotation(degrees: f32) -> Self {
        let turned = f64::from(degrees).rem_euclid(360.0);
        if turned == 0.0 {
            return Self::NOTHING_CHANGES;
        }
        let (sine, cosine) = turned.to_radians().sin_cos();
        let rows = [
            [
                0.213 + cosine * 0.787 - sine * 0.213,
                0.715 - cosine * 0.715 - sine * 0.715,
                0.072 - cosine * 0.072 + sine * 0.928,
            ],
            [
                0.213 - cosine * 0.213 + sine * 0.143,
                0.715 + cosine * 0.285 + sine * 0.140,
                0.072 - cosine * 0.072 - sine * 0.283,
            ],
            [
                0.213 - cosine * 0.213 - sine * 0.787,
                0.715 - cosine * 0.715 + sine * 0.715,
                0.072 + cosine * 0.928 + sine * 0.072,
            ],
        ];
        Self {
            rows: rows.map(|row| row.map(|number| number as f32)),
            offsets: [0.0; 3],
        }
    }

    /// The matrix that does what this one does and then what `next` does. (The colours are not cut off between the two, as they are not when the matrices are mixed
    /// in one pass: a colour is cut off once, at the end.)
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        let mut rows = [[0.0; 3]; 3];
        for (row, made) in rows.iter_mut().enumerate() {
            for (column, number) in made.iter_mut().enumerate() {
                *number = (0..3)
                    .map(|between| next.rows[row][between] * self.rows[between][column])
                    .sum();
            }
        }
        let mut offsets = next.offsets;
        for (row, offset) in offsets.iter_mut().enumerate() {
            *offset += (0..3)
                .map(|between| next.rows[row][between] * self.offsets[between])
                .sum::<f32>();
        }
        Self { rows, offsets }
    }

    /// What tells this matrix from every other, as a number for each of its nine places and three offsets: matrices that are the same have the
    /// same pattern. (There is no `Hash` for a matrix, which is made of numbers with fractions.)
    pub fn bit_pattern(&self) -> [u32; 12] {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.rows;
        let [x, y, z] = self.offsets;
        [a, b, c, d, e, f, g, h, i, x, y, z].map(f32::to_bits)
    }

    /// Whether this matrix changes no pixel, or none that can be told from the pixel it was (a change that is far less than the
    /// least that a colour can change by).
    pub fn changes_nothing(&self) -> bool {
        self.rows.iter().zip(Self::NOTHING_CHANGES.rows).all(|(row, nothing)| {
            row.iter()
                .zip(nothing)
                .all(|(number, nothing)| (number - nothing).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING)
        }) && self
            .offsets
            .iter()
            .all(|offset| offset.abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING)
    }

    /// Changes the colour of every pixel of the picture `rgba_pixels`, which has four bytes for each pixel (red, green, blue and
    /// how clear it is, which is not changed), as this matrix says. A colour that comes out less than 0 or more than 255 is
    /// the least or the most there is.
    pub fn apply_to_rgba_pixels(&self, rgba_pixels: &mut [u8]) {
        let [[rr, rg, rb], [gr, gg, gb], [br, bg, bb]] = self.rows;
        let [red_offset, green_offset, blue_offset] = self.offsets.map(|offset| offset * 255.0);
        // The nearest whole number, and the least or the most there is for what is out of the 0 to 255: half is added and what comes
        // after the point is cut off, which making the number a `u8` does, and does the 0 and the 255 too. (`f32::round` and
        // `clamp` do the same, but the first is a call to a library for each of the three colours of every pixel: measured, a picture
        // of 1280 by 720 took some 40 milliseconds, which is more than the whole of a frame of a video that plays at 30 a second,
        // and this takes 2.)
        let channel = |mixed: f32| (mixed + 0.5) as u8;
        for pixel in rgba_pixels.as_chunks_mut::<4>().0 {
            let (red, green, blue) = (f32::from(pixel[0]), f32::from(pixel[1]), f32::from(pixel[2]));
            pixel[0] = channel(rr * red + rg * green + rb * blue + red_offset);
            pixel[1] = channel(gr * red + gg * green + gb * blue + green_offset);
            pixel[2] = channel(br * red + bg * green + bb * blue + blue_offset);
        }
    }
}

#[cfg(test)]
mod offsets_tests;
#[cfg(test)]
mod tests;
