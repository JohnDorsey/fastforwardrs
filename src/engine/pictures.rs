// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures that are sent to the engine, as bytes, before any frame refers to them by their [`super::PictureId`].

use thiserror::Error;

use super::yuv::{YuvColour, chroma_size};

/// How the pixels of a picture are laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PicturePixels {
    /// Four bytes to a pixel, red, green, blue and how opaque, row by row, the top row first. The colours are **not** multiplied by how opaque
    /// the pixel is (what a picture file and the text renderer give).
    Rgba8 { straight_alpha: Vec<u8> },
    /// A video picture in planes: a brightness for every pixel (`y`), and for every square of four pixels a colour difference toward blue (`u`) and one
    /// toward red (`v`), each plane row by row, the top row first. A picture that is not an even number of pixels across or down has one chroma pixel
    /// for the last single row or column. `colour` says what the numbers stand for.
    Yuv420p8 {
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
        colour: YuvColour,
    },
}

/// A picture the engine is sent: how big, and its pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PictureData {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub pixels: PicturePixels,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PictureProblem {
    #[error("a picture has to be at least one pixel across and down (this one is {width} by {height})")]
    Empty { width: u32, height: u32 },
    #[error("a picture {width} by {height} pixels has {expected} bytes in its {plane}, and these are {actual}")]
    WrongNumberOfBytes {
        width: u32,
        height: u32,
        /// Which part of the picture: `"pixels"`, or the plane `"brightness"`, `"blue colour difference"` or `"red colour difference"`.
        plane: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl PictureData {
    /// A picture of RGBA bytes with straight (not premultiplied) alpha.
    pub fn rgba8(width_in_pixels: u32, height_in_pixels: u32, straight_alpha: Vec<u8>) -> Result<Self, PictureProblem> {
        let data = Self {
            width_in_pixels,
            height_in_pixels,
            pixels: PicturePixels::Rgba8 { straight_alpha },
        };
        data.check()?;
        Ok(data)
    }

    /// A video picture in planes (see [`PicturePixels::Yuv420p8`]).
    pub fn yuv420p8(
        width_in_pixels: u32,
        height_in_pixels: u32,
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
        colour: YuvColour,
    ) -> Result<Self, PictureProblem> {
        let data = Self {
            width_in_pixels,
            height_in_pixels,
            pixels: PicturePixels::Yuv420p8 { y, u, v, colour },
        };
        data.check()?;
        Ok(data)
    }

    /// Whether the picture is what it says it is: not empty, and with as many bytes as it needs.
    pub fn check(&self) -> Result<(), PictureProblem> {
        let (width, height) = (self.width_in_pixels, self.height_in_pixels);
        if width == 0 || height == 0 {
            return Err(PictureProblem::Empty { width, height });
        }
        let wrong = |plane, expected: usize, actual: usize| {
            (expected != actual).then_some(PictureProblem::WrongNumberOfBytes {
                width,
                height,
                plane,
                expected,
                actual,
            })
        };
        let problem = match &self.pixels {
            PicturePixels::Rgba8 { straight_alpha } => {
                wrong("pixels", width as usize * height as usize * 4, straight_alpha.len())
            }
            PicturePixels::Yuv420p8 { y, u, v, .. } => {
                let (chroma_width, chroma_height) = chroma_size(width, height);
                let chroma_bytes = chroma_width as usize * chroma_height as usize;
                wrong("brightness", width as usize * height as usize, y.len())
                    .or_else(|| wrong("blue colour difference", chroma_bytes, u.len()))
                    .or_else(|| wrong("red colour difference", chroma_bytes, v.len()))
            }
        };
        problem.map_or(Ok(()), Err)
    }

    /// How many bytes the pixels take up.
    pub fn byte_count(&self) -> usize {
        match &self.pixels {
            PicturePixels::Rgba8 { straight_alpha } => straight_alpha.len(),
            PicturePixels::Yuv420p8 { y, u, v, .. } => y.len() + u.len() + v.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_with_the_right_number_of_bytes_is_accepted() {
        let picture = PictureData::rgba8(3, 2, vec![0; 24]).unwrap();
        assert_eq!((picture.width_in_pixels, picture.height_in_pixels), (3, 2));
        assert_eq!(picture.byte_count(), 24);
    }

    #[test]
    fn an_empty_picture_is_refused_whatever_bytes_it_has() {
        assert_eq!(
            PictureData::rgba8(0, 4, vec![]),
            Err(PictureProblem::Empty { width: 0, height: 4 })
        );
        assert_eq!(
            PictureData::rgba8(4, 0, vec![1, 2, 3, 4]),
            Err(PictureProblem::Empty { width: 4, height: 0 })
        );
    }

    #[test]
    fn a_picture_with_too_few_or_too_many_bytes_is_refused_and_the_words_say_how_many_it_needs() {
        let too_few = PictureData::rgba8(3, 2, vec![0; 23]).unwrap_err();
        assert_eq!(
            too_few,
            PictureProblem::WrongNumberOfBytes {
                width: 3,
                height: 2,
                plane: "pixels",
                expected: 24,
                actual: 23
            }
        );
        assert!(too_few.to_string().contains("24 bytes"), "{too_few}");
        assert!(PictureData::rgba8(3, 2, vec![0; 25]).is_err());
    }
}
