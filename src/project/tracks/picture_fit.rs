//! Fitting a video of one shape into a picture of another.
//!
//! The finished picture has the shape of the project's first video. A video of another shape is
//! made as big as it can be while fitting inside, keeping its shape, and put in the middle; what
//! is not covered shows what is behind it (black, if there is nothing). The live preview and the
//! export both work this out here, so that they place the video in exactly the same way. Until a
//! video is moved, resized or turned, this is where it is (see [`fitted_placement`]).

use super::placement::{Angle, CanvasArea, Placement};

/// Where a video goes in the picture, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureFit {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    /// How far the video's left edge is from the picture's left edge.
    pub left_in_pixels: u32,
    pub top_in_pixels: u32,
}

impl PictureFit {
    /// Whether the video fills the whole of a picture `picture_width` by `picture_height` pixels,
    /// so that nothing has to be put round it.
    pub fn fills(&self, picture_width_in_pixels: u32, picture_height_in_pixels: u32) -> bool {
        self.width_in_pixels == picture_width_in_pixels && self.height_in_pixels == picture_height_in_pixels
    }
}

/// How a video that is shown `video_width` by `video_height` pixels fits in a picture
/// `picture_width` by `picture_height` pixels. If the two have the same shape the video simply fills
/// the picture. Otherwise it is made as large as it can be inside the picture (with whole, even
/// numbers of pixels, which is what video wants) and put in the middle.
pub fn fit_inside_picture(
    video_width_in_pixels: u32,
    video_height_in_pixels: u32,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> PictureFit {
    let (video_width, video_height) = (
        u64::from(video_width_in_pixels.max(1)),
        u64::from(video_height_in_pixels.max(1)),
    );
    let (picture_width, picture_height) = (
        u64::from(picture_width_in_pixels.max(1)),
        u64::from(picture_height_in_pixels.max(1)),
    );
    let fills = PictureFit {
        width_in_pixels: picture_width_in_pixels,
        height_in_pixels: picture_height_in_pixels,
        left_in_pixels: 0,
        top_in_pixels: 0,
    };
    // The same shape, if width / height is the same for both: compared without any rounding.
    if video_width * picture_height == video_height * picture_width {
        return fills;
    }
    let (fitted_width, fitted_height) = if video_width * picture_height > video_height * picture_width {
        // Wider than the picture: as wide as the picture.
        (
            picture_width,
            (video_height * picture_width + video_width / 2) / video_width,
        )
    } else {
        // Taller than the picture: as tall as the picture.
        (
            (video_width * picture_height + video_height / 2) / video_height,
            picture_height,
        )
    };
    let fitted_width = even(fitted_width).min(picture_width).max(1);
    let fitted_height = even(fitted_height).min(picture_height).max(1);
    PictureFit {
        width_in_pixels: u32::try_from(fitted_width).unwrap_or(u32::MAX),
        height_in_pixels: u32::try_from(fitted_height).unwrap_or(u32::MAX),
        left_in_pixels: u32::try_from(((picture_width - fitted_width) / 2) & !1).unwrap_or(0),
        top_in_pixels: u32::try_from(((picture_height - fitted_height) / 2) & !1).unwrap_or(0),
    }
}

/// The largest even number that is not above `size` (but not less than 2 when `size` is, so that
/// nothing is made empty).
fn even(size: u64) -> u64 {
    if size < 2 { size } else { size & !1 }
}

/// Where a video that is shown `video_size` pixels (width, height) is on a picture `picture_size`
/// pixels, as long as it has not been moved: the fit (see [`fit_inside_picture`]) as fractions of
/// the picture, not turned. At the size of the picture the place comes out in the very same
/// pixels as the fit, and at any other size it is the same place.
pub fn fitted_placement(video_size: (u32, u32), picture_size: (u32, u32)) -> Placement {
    let fit = fit_inside_picture(video_size.0, video_size.1, picture_size.0, picture_size.1);
    if fit.fills(picture_size.0, picture_size.1) {
        return Placement::upright(CanvasArea::new(0.0, 0.0, 1.0, 1.0));
    }
    let (picture_width, picture_height) = (picture_size.0.max(1) as f32, picture_size.1.max(1) as f32);
    Placement::new(
        CanvasArea::new(
            fit.left_in_pixels as f32 / picture_width,
            fit.top_in_pixels as f32 / picture_height,
            fit.width_in_pixels as f32 / picture_width,
            fit.height_in_pixels as f32 / picture_height,
        ),
        Angle::ZERO,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(video: (u32, u32), picture: (u32, u32)) -> PictureFit {
        fit_inside_picture(video.0, video.1, picture.0, picture.1)
    }

    #[test]
    fn a_video_of_the_same_shape_fills_the_picture_whatever_its_size() {
        for (video, picture) in [
            ((1920, 1080), (1920, 1080)),
            ((3840, 2160), (1280, 720)),
            ((640, 360), (1920, 1080)),
            ((1000, 1000), (50, 50)),
        ] {
            let fitted = fit(video, picture);
            assert!(
                fitted.fills(picture.0, picture.1),
                "{video:?} in {picture:?}: {fitted:?}"
            );
            assert_eq!((fitted.left_in_pixels, fitted.top_in_pixels), (0, 0));
        }
    }

    #[test]
    fn a_tall_video_in_a_wide_picture_has_bars_at_the_sides() {
        // 9:16 in 1920x1080: as tall as the picture, 607.5 wide, which is made 608 (the nearest even number).
        let fitted = fit((1080, 1920), (1920, 1080));
        assert_eq!(fitted.height_in_pixels, 1080);
        assert_eq!(fitted.width_in_pixels, 608);
        assert_eq!(fitted.top_in_pixels, 0);
        assert_eq!(fitted.left_in_pixels, (1920 - 608) / 2);
        assert!(!fitted.fills(1920, 1080));
    }

    #[test]
    fn a_wide_video_in_a_tall_picture_has_bars_above_and_below() {
        let fitted = fit((1920, 1080), (1080, 1920));
        assert_eq!(fitted.width_in_pixels, 1080);
        assert_eq!(fitted.height_in_pixels, 608);
        assert_eq!(fitted.left_in_pixels, 0);
        assert_eq!(fitted.top_in_pixels, (1920 - 608) / 2);
    }

    #[test]
    fn a_video_that_is_nearly_the_shape_of_the_picture_still_gets_thin_bars() {
        let fitted = fit((1280, 720), (1280, 722));
        assert_eq!((fitted.width_in_pixels, fitted.height_in_pixels), (1280, 720));
        assert_eq!(
            (fitted.left_in_pixels, fitted.top_in_pixels),
            (0, 0),
            "a place of one pixel is made none: video wants even places"
        );
    }

    #[test]
    fn the_fitted_video_is_always_inside_the_picture_with_even_sizes_and_even_places() {
        for video in [
            (1920, 1080),
            (1080, 1920),
            (1000, 1000),
            (3, 1000),
            (1000, 3),
            (1, 1),
            (4096, 2160),
        ] {
            for picture in [(1920, 1080), (1080, 1920), (640, 360), (1000, 1000), (2, 2)] {
                let fitted = fit(video, picture);
                assert!(
                    fitted.width_in_pixels >= 1 && fitted.height_in_pixels >= 1,
                    "{video:?} {picture:?}"
                );
                assert!(
                    fitted.left_in_pixels + fitted.width_in_pixels <= picture.0,
                    "{video:?} {picture:?} {fitted:?}"
                );
                assert!(
                    fitted.top_in_pixels + fitted.height_in_pixels <= picture.1,
                    "{video:?} {picture:?} {fitted:?}"
                );
                if !fitted.fills(picture.0, picture.1) && fitted.width_in_pixels > 1 {
                    assert_eq!(fitted.width_in_pixels % 2, 0, "{video:?} {picture:?} {fitted:?}");
                }
                assert_eq!(fitted.left_in_pixels % 2, 0, "{video:?} {picture:?} {fitted:?}");
                assert_eq!(fitted.top_in_pixels % 2, 0, "{video:?} {picture:?} {fitted:?}");
            }
        }
    }

    #[test]
    fn the_fitted_video_keeps_the_shape_of_the_video_to_within_a_pixel_or_two() {
        for video in [(1920u32, 1080u32), (1080, 1920), (1000, 700)] {
            let fitted = fit(video, (1280, 720));
            let video_shape = f64::from(video.0) / f64::from(video.1);
            let fitted_shape = f64::from(fitted.width_in_pixels) / f64::from(fitted.height_in_pixels);
            assert!(
                (video_shape - fitted_shape).abs() / video_shape < 0.01,
                "{video:?}: {fitted:?}"
            );
        }
    }

    #[test]
    fn a_video_of_the_shape_of_the_picture_is_placed_over_all_of_it_and_not_turned() {
        for (video, picture) in [
            ((1920, 1080), (1920, 1080)),
            ((3840, 2160), (1280, 720)),
            ((500, 500), (64, 64)),
        ] {
            assert_eq!(
                fitted_placement(video, picture),
                Placement::upright(CanvasArea::new(0.0, 0.0, 1.0, 1.0)),
                "{video:?} in {picture:?}"
            );
        }
    }

    #[test]
    fn a_video_of_another_shape_is_placed_in_the_middle_where_the_fit_puts_it_as_fractions_of_the_picture() {
        // 9:16 in 1920x1080: 608 wide and the whole height, 656 pixels from the left.
        let placement = fitted_placement((1080, 1920), (1920, 1080));
        assert_eq!(placement.rotation, Angle::ZERO);
        assert!((placement.area.left() - 656.0 / 1920.0).abs() < 1e-6);
        assert!((placement.area.width() - 608.0 / 1920.0).abs() < 1e-6);
        assert_eq!((placement.area.top(), placement.area.height()), (0.0, 1.0));
    }

    /// The export lays the video over the picture at the pixels of the place rounded, and the
    /// preview draws it at the fractions: they must be the pixels of the fit, whatever the sizes.
    #[test]
    fn the_place_is_the_pixels_of_the_fit_once_it_is_rounded_to_pixels_again() {
        for video in [
            (1920, 1080),
            (1080, 1920),
            (1000, 1000),
            (1280, 720),
            (720, 1280),
            (4096, 2160),
            (100, 400),
        ] {
            for picture in [
                (1920, 1080),
                (1080, 1920),
                (640, 360),
                (1000, 1000),
                (3840, 2160),
                (1280, 722),
            ] {
                let fit = fit(video, picture);
                let placement = fitted_placement(video, picture);
                let in_pixels = |fraction: f32, size: u32| (fraction * size as f32).round() as u32;
                assert_eq!(
                    (
                        in_pixels(placement.area.left(), picture.0),
                        in_pixels(placement.area.top(), picture.1),
                        in_pixels(placement.area.width(), picture.0),
                        in_pixels(placement.area.height(), picture.1),
                    ),
                    (
                        fit.left_in_pixels,
                        fit.top_in_pixels,
                        fit.width_in_pixels,
                        fit.height_in_pixels
                    ),
                    "{video:?} in {picture:?}"
                );
            }
        }
    }

    /// A place is never narrower than a fiftieth of the picture (see [`CanvasArea`]), so a video that
    /// is far thinner than that is placed that wide, which shows it stretched. Nobody films that.
    #[test]
    fn a_video_a_thousand_times_as_tall_as_it_is_wide_is_placed_no_thinner_than_a_place_can_be() {
        let placement = fitted_placement((3, 1000), (1920, 1080));
        assert_eq!(placement.area.width(), CanvasArea::SMALLEST_SIDE_FRACTION);
        assert_eq!(placement.area.height(), 1.0);
    }

    #[test]
    fn a_size_of_zero_never_makes_the_fit_panic_or_divide_by_zero() {
        // (Videos and pictures are never that small; this is only about not falling over.)
        assert_eq!(
            fit((0, 0), (0, 0)),
            PictureFit {
                width_in_pixels: 0,
                height_in_pixels: 0,
                left_in_pixels: 0,
                top_in_pixels: 0
            },
            "the same shape (as far as it can be told): the video fills the picture"
        );
        let fitted = fit((0, 100), (100, 0));
        assert!(
            fitted.width_in_pixels >= 1 && fitted.height_in_pixels >= 1,
            "{fitted:?}"
        );
    }
}
