// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the effects that change the colours of a picture do to its pixels, and how ffmpeg is asked to do the same, so that the
//! preview and the export agree.
//!
//! Each of them is a [`ColourMatrix`]: every pixel's red, green and blue are mixed to make the red, green and blue it becomes,
//! the same way for every pixel of the picture. (How clear a pixel is does not change.) What the matrix is, for one of the
//! effects, is worked out by the effect (see [`crate::project::EffectKind`]); this is the arithmetic that they have in common.
//! It has no other parts of the program in it, so that it can be tested alone.

/// How the red, green and blue of a pixel are mixed to make the red, green and blue of the pixel it becomes: each row says how
/// much of the red, the green and the blue the new red (the first row), green (the second) and blue (the third) is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColourMatrix {
    rows: [[f32; 3]; 3],
}

/// How different a number of a matrix can be from what it is in the matrix that changes nothing, for it to be taken for
/// the one that changes nothing: a change that is less than a hundredth of a step of the 256 steps of a colour changes no pixel.
const NEAR_ENOUGH_TO_CHANGE_NOTHING: f32 = 1.0 / 256.0 / 100.0;

/// How many decimals the numbers of a matrix have when ffmpeg is asked to use it.
const DECIMALS_FOR_FFMPEG: usize = 6;

impl ColourMatrix {
    /// The matrix that changes nothing.
    pub const NOTHING_CHANGES: Self = Self {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    /// The matrix with these rows (how much of the red, the green and the blue the new red is made of, then the new green, then the new blue). For the
    /// tests, which make matrices that no effect makes.
    #[cfg(test)]
    pub fn from_rows(rows: [[f32; 3]; 3]) -> Self {
        Self { rows }
    }

    /// The rows of the matrix: how much of the red, the green and the blue the new red (the first row), green (the second) and blue (the third) is
    /// made of.
    pub fn rows(&self) -> [[f32; 3]; 3] {
        self.rows
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
        }
    }

    /// The matrix that does what this one does and then what `next` does.
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
        Self { rows }
    }

    /// What tells this matrix from every other, as a number for each of its nine places: matrices that are the same have the
    /// same pattern. (There is no `Hash` for a matrix, which is made of numbers with fractions.)
    pub fn bit_pattern(&self) -> [u32; 9] {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.rows;
        [a, b, c, d, e, f, g, h, i].map(f32::to_bits)
    }

    /// Whether this matrix changes no pixel, or none that can be told from the pixel it was (a change that is far less than the
    /// least that a colour can change by).
    pub fn changes_nothing(&self) -> bool {
        self.rows.iter().zip(Self::NOTHING_CHANGES.rows).all(|(row, nothing)| {
            row.iter()
                .zip(nothing)
                .all(|(number, nothing)| (number - nothing).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING)
        })
    }

    /// Changes the colour of every pixel of the picture `rgba_pixels`, which has four bytes for each pixel (red, green, blue and
    /// how clear it is, which is not changed), as this matrix says. A colour that comes out less than 0 or more than 255 is
    /// the least or the most there is.
    pub fn apply_to_rgba_pixels(&self, rgba_pixels: &mut [u8]) {
        let [[rr, rg, rb], [gr, gg, gb], [br, bg, bb]] = self.rows;
        // The nearest whole number, and the least or the most there is for what is out of the 0 to 255: half is added and what comes
        // after the point is cut off, which making the number a `u8` does, and does the 0 and the 255 too. (`f32::round` and
        // `clamp` do the same, but the first is a call to a library for each of the three colours of every pixel: measured, a picture
        // of 1280 by 720 took some 40 milliseconds, which is more than the whole of a frame of a video that plays at 30 a second,
        // and this takes 2.)
        let channel = |mixed: f32| (mixed + 0.5) as u8;
        for pixel in rgba_pixels.as_chunks_mut::<4>().0 {
            let (red, green, blue) = (f32::from(pixel[0]), f32::from(pixel[1]), f32::from(pixel[2]));
            pixel[0] = channel(rr * red + rg * green + rb * blue);
            pixel[1] = channel(gr * red + gg * green + gb * blue);
            pixel[2] = channel(br * red + bg * green + bb * blue);
        }
    }

    /// The filter that asks ffmpeg to do what this matrix does (`colorchannelmixer`: each of its settings is how much of one
    /// colour a colour of the new pixel is made of, named by the two: `rg` is how much of the green makes the new red).
    pub fn ffmpeg_filter(&self) -> String {
        let [[rr, rg, rb], [gr, gg, gb], [br, bg, bb]] = self.rows;
        let number = |number: f32| {
            let text = format!("{number:.DECIMALS_FOR_FFMPEG$}");
            let trimmed = text.trim_end_matches('0').trim_end_matches('.');
            // (Not `-0`, which a number that rounds to nothing from below would be.)
            if trimmed == "-0" || trimmed.is_empty() {
                "0".to_owned()
            } else {
                trimmed.to_owned()
            }
        };
        format!(
            "colorchannelmixer=rr={}:rg={}:rb={}:gr={}:gg={}:gb={}:br={}:bg={}:bb={}",
            number(rr),
            number(rg),
            number(rb),
            number(gr),
            number(gg),
            number(gb),
            number(br),
            number(bg),
            number(bb)
        )
    }
}

#[cfg(test)]
mod tests;
