// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fast-forward symbol (such as `▶▶ 4x`) that appears on accelerated clips.
//!
//! The same picture is drawn into the live preview and into the exported video: this module draws
//! it ([`render_speed_indicator`]) and decides where it goes ([`place_indicator`]), and both the
//! preview and the export use those two functions.

mod geometry;
mod render;

pub use geometry::{IndicatorPlacement, place_indicator};
pub use render::{IndicatorPicture, SMALLEST_HEIGHT_IN_PIXELS, indicator_aspect_ratio, render_speed_indicator};

use crate::project::{SpeedFactor, SpeedIndicatorLayout};

/// A symbol that is ready to be put onto a picture of a known size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionedIndicator {
    pub picture: IndicatorPicture,
    /// Where the picture's top-left corner goes on the video frame, in whole pixels.
    pub left_in_pixels: u32,
    pub top_in_pixels: u32,
}

/// Draws the symbol for `speed_factor` at the right size and works out its position on a video
/// frame that is `frame_width_in_pixels` by `frame_height_in_pixels`, for the export.
pub fn indicator_for_frame(
    speed_factor: SpeedFactor,
    layout: SpeedIndicatorLayout,
    frame_width_in_pixels: u32,
    frame_height_in_pixels: u32,
) -> PositionedIndicator {
    let placement = place_indicator(
        layout,
        frame_width_in_pixels as f32,
        frame_height_in_pixels as f32,
        indicator_aspect_ratio(speed_factor),
    );
    let mut height_in_pixels = placement.height.round() as u32;
    let mut picture = render_speed_indicator(speed_factor, height_in_pixels);
    // Rounding the height and then the width can leave the picture a pixel or two wider than the
    // frame when the symbol was shrunk to fit. Try slightly smaller sizes until it fits.
    while picture.width_in_pixels > frame_width_in_pixels && height_in_pixels > SMALLEST_HEIGHT_IN_PIXELS {
        height_in_pixels -= 1;
        picture = render_speed_indicator(speed_factor, height_in_pixels);
    }
    // Rounding the corner can still push the symbol one pixel past an edge.
    let largest_left = frame_width_in_pixels.saturating_sub(picture.width_in_pixels);
    let largest_top = frame_height_in_pixels.saturating_sub(picture.height_in_pixels);
    PositionedIndicator {
        left_in_pixels: (placement.left.round() as u32).min(largest_left),
        top_in_pixels: (placement.top.round() as u32).min(largest_top),
        picture,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_symbol_for_the_export_fits_inside_the_frame() {
        for (frame_width, frame_height) in [(1920, 1080), (1080, 1920), (320, 180), (640, 640)] {
            for (left, top, height) in [(0.0, 0.0, 0.08), (1.0, 1.0, 0.08), (0.97, 0.5, 0.3), (0.5, 0.99, 0.6)] {
                for factor in [2, 16, 250, 1000] {
                    let layout = SpeedIndicatorLayout::new(left, top, height);
                    let indicator =
                        indicator_for_frame(SpeedFactor::new(factor).unwrap(), layout, frame_width, frame_height);
                    assert!(
                        indicator.left_in_pixels + indicator.picture.width_in_pixels <= frame_width,
                        "{indicator:?}"
                    );
                    assert!(
                        indicator.top_in_pixels + indicator.picture.height_in_pixels <= frame_height,
                        "{indicator:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_symbol_for_the_export_has_the_height_the_layout_asks_for() {
        let layout = SpeedIndicatorLayout::new(0.1, 0.1, 0.1);
        let indicator = indicator_for_frame(SpeedFactor::new(4).unwrap(), layout, 1920, 1080);
        assert_eq!(indicator.picture.height_in_pixels, 108);
        assert_eq!((indicator.left_in_pixels, indicator.top_in_pixels), (192, 108));
    }
}
