// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the engine gives back: a finished frame as plain pixels, and what it cost.

use std::time::Duration;

/// A finished frame: four bytes (red, green, blue, and 255, which is how opaque it is) for each pixel, row by row, the top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedFrame {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub rgba_pixels: Vec<u8>,
}

impl RenderedFrame {
    /// How many bytes the frame takes up.
    pub fn byte_count(&self) -> usize {
        self.rgba_pixels.len()
    }

    /// Whether every pixel of the frame is `colour`.
    pub fn every_pixel_is(&self, colour: [u8; 4]) -> bool {
        self.rgba_pixels.as_chunks::<4>().0.iter().all(|pixel| *pixel == colour)
    }

    /// The four bytes of the pixel `across` and `down` from the top left corner.
    pub fn pixel(&self, across: u32, down: u32) -> [u8; 4] {
        let first = (down as usize * self.width_in_pixels as usize + across as usize) * 4;
        [
            self.rgba_pixels[first],
            self.rgba_pixels[first + 1],
            self.rgba_pixels[first + 2],
            self.rgba_pixels[first + 3],
        ]
    }
}

/// A finished frame as the planes of a `yuv420p` picture, which is what the export hands to the encoder: the brightness of every pixel, row by row, the top row first,
/// and then the blue colour difference and the red one of every square of four pixels (at an odd edge, one for the single pixels that are left over), each plane after
/// the one before. One piece of bytes, exactly as ffmpeg reads raw `yuv420p` video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedPlanes {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub planes: Vec<u8>,
}

impl RenderedPlanes {
    /// How many bytes the planes take up.
    pub fn byte_count(&self) -> usize {
        self.planes.len()
    }

    fn brightness_bytes(&self) -> usize {
        self.width_in_pixels as usize * self.height_in_pixels as usize
    }

    fn colour_difference_bytes(&self) -> usize {
        let (width, height) = super::yuv::chroma_size(self.width_in_pixels, self.height_in_pixels);
        width as usize * height as usize
    }

    /// The brightness of every pixel.
    pub fn luma(&self) -> &[u8] {
        &self.planes[..self.brightness_bytes()]
    }

    /// The blue colour difference of every square of four pixels.
    pub fn blue_difference(&self) -> &[u8] {
        let first = self.brightness_bytes();
        &self.planes[first..first + self.colour_difference_bytes()]
    }

    /// The red colour difference of every square of four pixels.
    pub fn red_difference(&self) -> &[u8] {
        let first = self.brightness_bytes() + self.colour_difference_bytes();
        &self.planes[first..first + self.colour_difference_bytes()]
    }
}

/// What making a frame came to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStatistics {
    /// From the engine's taking the request up to having the frame in hand.
    pub time_taken: Duration,
    /// The part of that spent waiting for the graphics card to finish and for its pixels.
    pub time_waiting_for_the_graphics_card: Duration,
    /// How many passes were recorded (0 for a frame that came from the cache).
    pub passes_recorded: usize,
    /// Whether the frame was in the cache of finished frames, so that nothing was drawn.
    pub came_from_the_cache: bool,
    /// How many effects were left out of the frame because the engine does not have an effect with the key that was asked for.
    pub effects_not_run: usize,
    /// Whether the picture is not exact: an effect that depends on earlier frames went on from a state that is not the one of the frame before (the preview jumped, or skipped a frame), or from
    /// one that was made from such a state not long ago (EN-14). Never for the export, which makes every frame in order.
    pub approximate: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_is_found_by_its_row_and_its_place_in_the_row() {
        let frame = RenderedFrame {
            width_in_pixels: 3,
            height_in_pixels: 2,
            rgba_pixels: (0u8..24).collect(),
        };
        assert_eq!(frame.pixel(0, 0), [0, 1, 2, 3]);
        assert_eq!(frame.pixel(2, 0), [8, 9, 10, 11]);
        assert_eq!(frame.pixel(0, 1), [12, 13, 14, 15]);
        assert_eq!(frame.pixel(2, 1), [20, 21, 22, 23]);
        assert_eq!(frame.byte_count(), 24);
    }

    #[test]
    fn a_frame_is_all_one_colour_only_if_every_pixel_is() {
        let mut frame = RenderedFrame {
            width_in_pixels: 2,
            height_in_pixels: 2,
            rgba_pixels: [[1, 2, 3, 255]; 4].concat(),
        };
        assert!(frame.every_pixel_is([1, 2, 3, 255]));
        assert!(!frame.every_pixel_is([1, 2, 3, 254]));
        frame.rgba_pixels[15] = 0;
        assert!(!frame.every_pixel_is([1, 2, 3, 255]), "the last pixel is another one");
    }
}
