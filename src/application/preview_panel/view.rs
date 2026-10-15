// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the preview is zoomed, and which part of the picture it shows.
//!
//! The picture is drawn as big as it can be in the panel to begin with (it *fits*), and can be zoomed out, until it is half as big
//! as that, and in, until each pixel of it is wide enough to be seen by itself ([`MOST_POINTS_PER_PIXEL`]). Zooming goes about a
//! point (the pointer), which stays over the same part of the picture. A picture that is bigger than the panel can be moved about
//! in it, and a picture that is not is kept in the middle of it: there is never more of the panel without the picture than there has
//! to be.
//!
//! Everything here is arithmetic on rectangles, so that it can be tested without a window.

use egui::{Pos2, Rect, Vec2, vec2};

use super::fit_inside;
use crate::application::textures::PictureFilter;
use crate::application::theme;

/// How small the picture can be zoomed out to, as a fraction of the size it has when it fits the panel: half.
pub const SMALLEST_ZOOM: f32 = 0.5;

/// How big a pixel of the picture is, at the most, in points of the screen: wide enough for every pixel to be seen by itself.
pub const MOST_POINTS_PER_PIXEL: f32 = 8.0;

/// A zoom that is this near to the one at which the picture fits is the one at which it fits, which a zoom that has gone in and
/// out again by the same amounts comes to, but for a rounding error.
const NEAR_ENOUGH_TO_FIT: f32 = 0.002;

/// A pixel of the picture that is this many pixels of the screen wide or more is shown by itself, as a block, and not blended into
/// those next to it.
const PIXELS_OF_THE_SCREEN_FOR_A_PIXEL_TO_SHOW_AS_A_BLOCK: f32 = 2.0;

/// How big the picture is drawn, and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewView {
    /// How many times as wide and as tall the picture is drawn as when it fits the panel.
    zoom: f32,
    /// How far the middle of the picture is from the middle of the panel, in points.
    shift: Vec2,
}

impl Default for PreviewView {
    fn default() -> Self {
        Self::FITTING
    }
}

impl PreviewView {
    /// The picture as big as it can be in the panel, in the middle of it.
    pub const FITTING: Self = Self {
        zoom: 1.0,
        shift: Vec2::ZERO,
    };

    /// Whether the picture fits the panel exactly, as big as it can be and in the middle: the one view that has no button to bring
    /// it back to.
    pub fn is_fitting(&self) -> bool {
        *self == Self::FITTING
    }

    /// How many times as wide as it is when it fits the panel the picture is drawn.
    #[cfg(test)]
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Where the picture of `picture_size` pixels is drawn in `panel`.
    pub fn picture_rectangle(&self, panel: Rect, picture_size: (u32, u32)) -> Rect {
        let fitting = fitting_rectangle(panel, picture_size);
        Rect::from_center_size(panel.center() + self.shift, fitting.size() * self.zoom)
    }

    /// How wide a pixel of the picture is drawn, in points.
    pub fn points_per_pixel(&self, panel: Rect, picture_size: (u32, u32)) -> f32 {
        self.picture_rectangle(panel, picture_size).width() / picture_size.0.max(1) as f32
    }

    /// How the pictures are put on the screen at this zoom: blended where a pixel of the picture is a few pixels of the screen at
    /// the most, and as blocks where it is more, so that the pixels are seen. `pixels_per_point` is how many pixels of the screen
    /// there are to a point.
    pub fn filter(&self, panel: Rect, picture_size: (u32, u32), pixels_per_point: f32) -> PictureFilter {
        let pixels_of_the_screen = self.points_per_pixel(panel, picture_size) * pixels_per_point;
        if pixels_of_the_screen >= PIXELS_OF_THE_SCREEN_FOR_A_PIXEL_TO_SHOW_AS_A_BLOCK {
            PictureFilter::Pixels
        } else {
            PictureFilter::Smooth
        }
    }

    /// The view zoomed by `factor` (more than 1 zooms in) about `about`, a point of the screen, which stays over the same point of
    /// the picture. The zoom stops where the picture is half as big as when it fits (it stops there, and does not go on past it),
    /// or where a pixel of it is [`MOST_POINTS_PER_PIXEL`] wide, and the picture is kept where it can be.
    #[must_use]
    pub fn zoomed_by(self, factor: f32, about: Pos2, panel: Rect, picture_size: (u32, u32)) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return self;
        }
        let zoom = (self.zoom * factor).clamp(SMALLEST_ZOOM, most_zoom(panel, picture_size));
        let how_much_bigger = zoom / self.zoom;
        let middle = about + (panel.center() + self.shift - about) * how_much_bigger;
        Self {
            zoom,
            shift: middle - panel.center(),
        }
        .kept_where_it_can_be(panel, picture_size)
    }

    /// The view moved by `by` points: the picture goes that way (as far as it can: see [`Self::kept_where_it_can_be`]).
    #[must_use]
    pub fn shifted_by(self, by: Vec2, panel: Rect, picture_size: (u32, u32)) -> Self {
        Self {
            shift: self.shift + by,
            ..self
        }
        .kept_where_it_can_be(panel, picture_size)
    }

    /// The view as it can be in this panel, which is what it is if the panel has not changed since the view was made: a zoom that is
    /// further than the limits for the panel and the picture is brought back to them, a picture that is bigger than the panel
    /// is kept covering all of it, and one that is not is kept in the middle of it.
    #[must_use]
    pub fn kept_where_it_can_be(self, panel: Rect, picture_size: (u32, u32)) -> Self {
        let zoom = self.zoom.clamp(SMALLEST_ZOOM, most_zoom(panel, picture_size));
        let zoom = if (zoom - 1.0).abs() < NEAR_ENOUGH_TO_FIT {
            1.0
        } else {
            zoom
        };
        let drawn = fitting_rectangle(panel, picture_size).size() * zoom;
        // (There is room to move it by half of how much bigger it is than the panel, either way, if it is bigger.)
        let furthest = vec2(
            ((drawn.x - panel.width()) / 2.0).max(0.0),
            ((drawn.y - panel.height()) / 2.0).max(0.0),
        );
        Self {
            zoom,
            shift: vec2(
                self.shift.x.clamp(-furthest.x, furthest.x),
                self.shift.y.clamp(-furthest.y, furthest.y),
            ),
        }
    }
}

/// The rectangle the picture is drawn in when it fits the panel, which leaves a margin of its own round it.
fn fitting_rectangle(panel: Rect, picture_size: (u32, u32)) -> Rect {
    fit_inside(panel.shrink(theme::SPACING), picture_size.0, picture_size.1)
}

/// The furthest the picture can be zoomed in, in this panel: until a pixel of it is [`MOST_POINTS_PER_PIXEL`] wide, if it can be
/// zoomed in at all (a picture that is so small that its pixels are wider than that when it fits is not zoomed in).
fn most_zoom(panel: Rect, picture_size: (u32, u32)) -> f32 {
    let points_per_pixel_when_fitting = fitting_rectangle(panel, picture_size).width() / picture_size.0.max(1) as f32;
    (MOST_POINTS_PER_PIXEL / points_per_pixel_when_fitting).max(1.0)
}

#[cfg(test)]
mod tests;
