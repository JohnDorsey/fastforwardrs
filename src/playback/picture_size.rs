// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How big the pictures of the live preview are made.

/// The largest width the preview's pictures are made at.
pub const LARGEST_PREVIEW_WIDTH_IN_PIXELS: u32 = 1280;

/// The largest height the preview's pictures are made at.
pub const LARGEST_PREVIEW_HEIGHT_IN_PIXELS: u32 = 720;

/// The size, in pixels, of the pictures that are decoded for the live preview of a video that is
/// shown `display_width_in_pixels` by `display_height_in_pixels`.
///
/// Pictures are made no larger than 1280 by 720, however large the video is or the window gets:
/// larger would only make decoding slower, and the picture is stretched to fit the window by the
/// graphics card anyway. Making them a fixed size, not one that follows the window, also means
/// resizing the window never makes the preview start decoding again. The shape of the video is
/// kept, and a small video is never made larger.
pub fn preview_picture_size(display_width_in_pixels: u32, display_height_in_pixels: u32) -> (u32, u32) {
    let width = display_width_in_pixels.max(1);
    let height = display_height_in_pixels.max(1);
    let shrink = (f64::from(LARGEST_PREVIEW_WIDTH_IN_PIXELS) / f64::from(width))
        .min(f64::from(LARGEST_PREVIEW_HEIGHT_IN_PIXELS) / f64::from(height))
        .min(1.0);
    (
        ((f64::from(width) * shrink).round() as u32).max(1),
        ((f64::from(height) * shrink).round() as u32).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_videos_keep_their_size() {
        assert_eq!(preview_picture_size(640, 360), (640, 360));
        assert_eq!(preview_picture_size(1280, 720), (1280, 720));
        assert_eq!(preview_picture_size(320, 180), (320, 180));
    }

    #[test]
    fn large_videos_are_shrunk_to_fit_keeping_their_shape() {
        assert_eq!(preview_picture_size(1920, 1080), (1280, 720));
        assert_eq!(preview_picture_size(3840, 2160), (1280, 720));
        assert_eq!(preview_picture_size(2560, 1440), (1280, 720));
    }

    #[test]
    fn tall_videos_are_limited_by_their_height() {
        // A 1080x1920 phone video: 720 tall, so 405 wide.
        assert_eq!(preview_picture_size(1080, 1920), (405, 720));
    }

    #[test]
    fn very_wide_videos_are_limited_by_their_width() {
        assert_eq!(preview_picture_size(5120, 1080), (1280, 270));
    }

    #[test]
    fn the_shape_is_kept_to_within_a_pixel() {
        for (width, height) in [
            (1920, 1080),
            (1366, 768),
            (1440, 1080),
            (1000, 3000),
            (4000, 1234),
            (720, 576),
        ] {
            let (shown_width, shown_height) = preview_picture_size(width, height);
            let original_shape = f64::from(width) / f64::from(height);
            let shown_shape = f64::from(shown_width) / f64::from(shown_height);
            assert!(
                (original_shape - shown_shape).abs() / original_shape < 0.01,
                "{width}x{height} became {shown_width}x{shown_height}"
            );
            assert!(shown_width <= 1280 && shown_height <= 720);
        }
    }

    #[test]
    fn degenerate_sizes_still_give_a_usable_picture() {
        assert_eq!(preview_picture_size(0, 0), (1, 1));
        assert_eq!(preview_picture_size(1, 100_000), (1, 720));
    }
}
