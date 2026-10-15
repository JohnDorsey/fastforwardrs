// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the fast-forward symbol (">> 4x") sits on the picture and how big it is.

/// The position and size of the fast-forward symbol, shared by the whole project: every
/// accelerated clip shows the symbol in this one place, in the live preview and in the export.
///
/// Everything is a fraction of the video picture rather than a number of pixels, so the symbol
/// lands in the same place whatever size the picture is shown or exported at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeedIndicatorLayout {
    left_fraction_of_frame_width: f32,
    top_fraction_of_frame_height: f32,
    height_fraction_of_frame_height: f32,
}

impl SpeedIndicatorLayout {
    /// Near the top-left corner, about 8% of the picture's height.
    pub const DEFAULT: SpeedIndicatorLayout = SpeedIndicatorLayout {
        left_fraction_of_frame_width: 0.03,
        top_fraction_of_frame_height: 0.04,
        height_fraction_of_frame_height: 0.08,
    };

    /// The symbol is never made smaller than this fraction of the picture's height.
    pub const SMALLEST_HEIGHT_FRACTION: f32 = 0.03;
    /// The symbol is never made larger than this fraction of the picture's height.
    pub const LARGEST_HEIGHT_FRACTION: f32 = 0.60;

    /// Creates a layout. Values that are out of range (or not numbers) are corrected rather than
    /// rejected, so a layout read from a hand-edited project file can never break the editor.
    pub fn new(
        left_fraction_of_frame_width: f32,
        top_fraction_of_frame_height: f32,
        height_fraction_of_frame_height: f32,
    ) -> Self {
        Self {
            left_fraction_of_frame_width: sanitised(
                left_fraction_of_frame_width,
                0.0,
                1.0,
                Self::DEFAULT.left_fraction_of_frame_width,
            ),
            top_fraction_of_frame_height: sanitised(
                top_fraction_of_frame_height,
                0.0,
                1.0,
                Self::DEFAULT.top_fraction_of_frame_height,
            ),
            height_fraction_of_frame_height: sanitised(
                height_fraction_of_frame_height,
                Self::SMALLEST_HEIGHT_FRACTION,
                Self::LARGEST_HEIGHT_FRACTION,
                Self::DEFAULT.height_fraction_of_frame_height,
            ),
        }
    }

    /// How far the symbol's left edge is from the picture's left edge, as a fraction of the
    /// picture's width.
    pub fn left_fraction_of_frame_width(&self) -> f32 {
        self.left_fraction_of_frame_width
    }

    /// How far the symbol's top edge is from the picture's top edge, as a fraction of the
    /// picture's height.
    pub fn top_fraction_of_frame_height(&self) -> f32 {
        self.top_fraction_of_frame_height
    }

    /// The symbol's height as a fraction of the picture's height. Its width follows from its
    /// shape, which depends on the speed factor being shown.
    pub fn height_fraction_of_frame_height(&self) -> f32 {
        self.height_fraction_of_frame_height
    }
}

impl Default for SpeedIndicatorLayout {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn sanitised(value: f32, smallest: f32, largest: f32, fallback: f32) -> f32 {
    if value.is_nan() {
        fallback
    } else {
        value.clamp(smallest, largest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_range_values_are_kept_exactly() {
        let layout = SpeedIndicatorLayout::new(0.25, 0.5, 0.125);
        assert_eq!(layout.left_fraction_of_frame_width(), 0.25);
        assert_eq!(layout.top_fraction_of_frame_height(), 0.5);
        assert_eq!(layout.height_fraction_of_frame_height(), 0.125);
    }

    #[test]
    fn out_of_range_values_are_pulled_into_range() {
        let layout = SpeedIndicatorLayout::new(-3.0, 7.0, 99.0);
        assert_eq!(layout.left_fraction_of_frame_width(), 0.0);
        assert_eq!(layout.top_fraction_of_frame_height(), 1.0);
        assert_eq!(
            layout.height_fraction_of_frame_height(),
            SpeedIndicatorLayout::LARGEST_HEIGHT_FRACTION
        );

        let tiny = SpeedIndicatorLayout::new(0.5, 0.5, 0.0);
        assert_eq!(
            tiny.height_fraction_of_frame_height(),
            SpeedIndicatorLayout::SMALLEST_HEIGHT_FRACTION
        );
    }

    #[test]
    fn values_that_are_not_numbers_fall_back_to_the_defaults() {
        let layout = SpeedIndicatorLayout::new(f32::NAN, f32::NAN, f32::NAN);
        assert_eq!(layout, SpeedIndicatorLayout::DEFAULT);
    }

    #[test]
    fn infinite_values_are_clamped_not_rejected() {
        let layout = SpeedIndicatorLayout::new(f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY);
        assert_eq!(layout.left_fraction_of_frame_width(), 1.0);
        assert_eq!(layout.top_fraction_of_frame_height(), 0.0);
        assert_eq!(
            layout.height_fraction_of_frame_height(),
            SpeedIndicatorLayout::LARGEST_HEIGHT_FRACTION
        );
    }

    #[test]
    fn the_default_layout_is_itself_valid() {
        let default = SpeedIndicatorLayout::DEFAULT;
        assert_eq!(
            SpeedIndicatorLayout::new(
                default.left_fraction_of_frame_width(),
                default.top_fraction_of_frame_height(),
                default.height_fraction_of_frame_height()
            ),
            default
        );
    }
}
