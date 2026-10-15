// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the fast-forward symbol goes inside a picture. The live preview and the export both use
//! this, so the symbol lands in the same place in both.

use crate::project::SpeedIndicatorLayout;

/// A rectangle inside a picture, measured in that picture's pixels from its top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorPlacement {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// Works out where the symbol goes in a picture that is `frame_width` by `frame_height` pixels.
///
/// `aspect_ratio` is the symbol's width divided by its height, which depends on how many digits
/// the speed factor has. The symbol's top-left corner is where the layout says, but it is never
/// allowed to stick out of the picture: it is pulled back inside if the corner is too near the
/// right or bottom edge (which happens when a wider symbol, such as ">> 100x", replaces a narrower
/// one), and shrunk if it would be wider than the whole picture.
pub fn place_indicator(
    layout: SpeedIndicatorLayout,
    frame_width: f32,
    frame_height: f32,
    aspect_ratio: f32,
) -> IndicatorPlacement {
    let mut height = frame_height * layout.height_fraction_of_frame_height();
    let mut width = height * aspect_ratio;
    if width > frame_width {
        width = frame_width;
        height = width / aspect_ratio;
    }
    let left = (layout.left_fraction_of_frame_width() * frame_width).clamp(0.0, frame_width - width);
    let top = (layout.top_fraction_of_frame_height() * frame_height).clamp(0.0, frame_height - height);
    IndicatorPlacement {
        left,
        top,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f32 = 3.0;

    #[test]
    fn the_symbol_is_placed_at_the_layout_position_with_the_layout_height() {
        let layout = SpeedIndicatorLayout::new(0.25, 0.5, 0.1);
        let placement = place_indicator(layout, 1000.0, 500.0, WIDE);
        assert_eq!(
            placement,
            IndicatorPlacement {
                left: 250.0,
                top: 250.0,
                width: 150.0,
                height: 50.0
            }
        );
    }

    #[test]
    fn the_same_layout_lands_in_the_same_relative_place_at_any_picture_size() {
        let layout = SpeedIndicatorLayout::new(0.2, 0.3, 0.1);
        let small = place_indicator(layout, 640.0, 360.0, WIDE);
        let large = place_indicator(layout, 1920.0, 1080.0, WIDE);
        let scale = 3.0;
        assert!((large.left - small.left * scale).abs() < 1e-3);
        assert!((large.top - small.top * scale).abs() < 1e-3);
        assert!((large.width - small.width * scale).abs() < 1e-3);
        assert!((large.height - small.height * scale).abs() < 1e-3);
    }

    #[test]
    fn a_symbol_near_the_right_edge_is_pulled_back_inside_the_picture() {
        let layout = SpeedIndicatorLayout::new(0.95, 0.0, 0.1);
        let placement = place_indicator(layout, 1000.0, 500.0, WIDE);
        assert_eq!(placement.width, 150.0);
        assert_eq!(
            placement.left, 850.0,
            "right edge of the symbol sits on the right edge of the picture"
        );
    }

    #[test]
    fn a_symbol_near_the_bottom_edge_is_pulled_back_inside_the_picture() {
        let layout = SpeedIndicatorLayout::new(0.0, 1.0, 0.1);
        let placement = place_indicator(layout, 1000.0, 500.0, WIDE);
        assert_eq!(placement.top, 450.0);
    }

    #[test]
    fn a_symbol_wider_than_the_picture_is_shrunk_to_fit_keeping_its_shape() {
        // A tall, narrow picture and a big symbol: 0.5 * 400 = 200 high, 600 wide, in a 100 wide picture.
        let layout = SpeedIndicatorLayout::new(0.5, 0.5, 0.5);
        let placement = place_indicator(layout, 100.0, 400.0, WIDE);
        assert_eq!(placement.width, 100.0);
        assert!((placement.height - 100.0 / WIDE).abs() < 1e-4);
        assert_eq!(placement.left, 0.0);
    }

    #[test]
    fn the_placement_always_stays_inside_the_picture() {
        for left in [0.0, 0.3, 0.9, 1.0] {
            for top in [0.0, 0.4, 0.95, 1.0] {
                for height in [0.03, 0.2, 0.6] {
                    for (frame_width, frame_height) in [(1920.0, 1080.0), (1080.0, 1920.0), (200.0, 200.0)] {
                        let layout = SpeedIndicatorLayout::new(left, top, height);
                        let placement = place_indicator(layout, frame_width, frame_height, 3.4);
                        assert!(
                            placement.left >= 0.0 && placement.top >= 0.0,
                            "{layout:?} {placement:?}"
                        );
                        assert!(
                            placement.left + placement.width <= frame_width + 1e-3,
                            "{layout:?} {placement:?}"
                        );
                        assert!(
                            placement.top + placement.height <= frame_height + 1e-3,
                            "{layout:?} {placement:?}"
                        );
                    }
                }
            }
        }
    }
}
