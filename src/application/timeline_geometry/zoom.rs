// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Zooming the strips in and out: how far that can go, and what it does to where they are scrolled to.
//!
//! The zoom is how many points of the screen one second of the finished video takes up. Zooming
//! with the pointer over the strips keeps the moment under the pointer where it is on the screen,
//! which is what makes it easy to zoom in on the thing that is being pointed at.

use super::TimelineGeometry;

/// The least zoomed in the strips can be, in points per second: a video an hour long is then
/// 7200 points, which is a few screens.
const SMALLEST_POINTS_PER_SECOND: f64 = 2.0;

/// The most zoomed in the strips can be, in points per frame: a frame is then wide enough to take
/// aim at with a mouse, and no wider.
const LARGEST_POINTS_PER_FRAME: f64 = 40.0;

/// How much of the width of the view frames that are framed take up: nearly all of it, with a little room at either end.
const SHARE_OF_THE_VIEW_THAT_WHAT_IS_FRAMED_TAKES_UP: f64 = 0.92;

/// The least time that is framed, in seconds: one keyframe, or a few frames, are not zoomed in on as far as the strips can be.
const LEAST_SECONDS_FRAMED: f64 = 1.0;

impl TimelineGeometry {
    /// The most points per second the strips can be zoomed in to at this geometry's frame rate.
    fn largest_points_per_second(&self) -> f64 {
        (LARGEST_POINTS_PER_FRAME * self.frame_rate.frames_per_second()).max(SMALLEST_POINTS_PER_SECOND)
    }

    /// The same geometry at `points_per_second`, or as near to that as is allowed. (Something that
    /// is not a number is the least zoomed in.)
    #[must_use]
    pub fn with_points_per_second(self, points_per_second: f64) -> Self {
        let points_per_second = if points_per_second.is_nan() {
            SMALLEST_POINTS_PER_SECOND
        } else {
            points_per_second.clamp(SMALLEST_POINTS_PER_SECOND, self.largest_points_per_second())
        };
        Self {
            points_per_second,
            ..self
        }
    }

    pub fn points_per_second(&self) -> f64 {
        self.points_per_second
    }

    /// This geometry zoomed so that the frames from `first_frame` up to (not including) `end_frame` nearly fill a view `view_width`
    /// points wide (at the most as far in as makes [`LEAST_SECONDS_FRAMED`] seconds fill it, and at the most as far out as the
    /// strips go), and the scroll offset that puts the middle of them in the middle of the view. (The offset is not kept inside
    /// the strips: see [`Self::clamp_scroll_offset`].)
    #[must_use]
    pub fn framing(self, first_frame: u64, end_frame: u64, view_width: f64) -> (Self, f64) {
        let first_second = self.frame_rate.seconds_at_frame(first_frame);
        let end_second = self.frame_rate.seconds_at_frame(end_frame.max(first_frame));
        let middle_second = (first_second + end_second) / 2.0;
        let seconds_framed = (end_second - first_second).max(LEAST_SECONDS_FRAMED);
        let framed =
            self.with_points_per_second(view_width * SHARE_OF_THE_VIEW_THAT_WHAT_IS_FRAMED_TAKES_UP / seconds_framed);
        (framed, middle_second * framed.points_per_second - view_width / 2.0)
    }

    /// This geometry zoomed by `factor` (above 1 zooms in, below 1 out), and the scroll offset that
    /// goes with it: the view starts `scroll_offset` points along the strips before, and the place
    /// `anchor_x_in_view` points from its left edge is held still, so that the moment that is under
    /// it before is under it after.
    #[must_use]
    pub fn zoomed_about(self, scroll_offset: f64, factor: f64, anchor_x_in_view: f64) -> (Self, f64) {
        let zoomed = self.with_points_per_second(self.points_per_second * factor);
        let seconds_under_the_anchor = (scroll_offset + anchor_x_in_view) / self.points_per_second;
        (
            zoomed,
            seconds_under_the_anchor * zoomed.points_per_second - anchor_x_in_view,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{assert_close, geometry_at};

    #[test]
    fn the_most_zoomed_in_is_a_frame_forty_points_wide_whatever_the_frame_rate() {
        for (numerator, denominator, largest) in [(30, 1, 1200.0), (60, 1, 2400.0), (25, 1, 1000.0)] {
            let geometry = geometry_at(numerator, denominator, 100.0).with_points_per_second(1.0e9);
            assert_close(geometry.points_per_second(), largest);
            assert_close(geometry.x_of_frame(1), 40.0);
        }
    }

    #[test]
    fn a_scale_outside_the_limits_is_brought_back_inside_and_one_inside_is_left_alone() {
        let geometry = geometry_at(30, 1, 100.0);
        assert_close(geometry.with_points_per_second(185.0).points_per_second(), 185.0);
        assert_close(geometry.with_points_per_second(0.01).points_per_second(), 2.0);
        assert_close(geometry.with_points_per_second(-5.0).points_per_second(), 2.0);
        assert_close(
            geometry.with_points_per_second(f64::INFINITY).points_per_second(),
            1200.0,
        );
        assert_close(
            geometry.with_points_per_second(f64::NEG_INFINITY).points_per_second(),
            2.0,
        );
        assert_close(geometry.with_points_per_second(f64::NAN).points_per_second(), 2.0);
        assert_eq!(
            geometry.with_points_per_second(185.0),
            geometry_at(30, 1, 185.0),
            "the frame rate stays as it was"
        );
    }

    #[test]
    fn zooming_keeps_the_moment_under_the_anchor_where_it_is_on_the_screen() {
        let before = geometry_at(30, 1, 185.0);
        let (scroll_offset, anchor) = (900.0, 300.0);
        let seconds_under_the_anchor = (scroll_offset + anchor) / before.points_per_second();
        for factor in [0.1, 0.5, 0.9, 1.0, 1.7, 3.0, 6.0] {
            let (after, after_scroll_offset) = before.zoomed_about(scroll_offset, factor, anchor);
            assert_close(
                (after_scroll_offset + anchor) / after.points_per_second(),
                seconds_under_the_anchor,
            );
        }
    }

    #[test]
    fn zooming_in_makes_a_second_wider_and_zooming_out_narrower_and_a_factor_of_one_changes_nothing() {
        let before = geometry_at(30, 1, 185.0);
        assert!(before.zoomed_about(500.0, 2.0, 100.0).0.points_per_second() > 185.0);
        assert!(before.zoomed_about(500.0, 0.5, 100.0).0.points_per_second() < 185.0);
        let (unchanged, scroll_offset) = before.zoomed_about(500.0, 1.0, 100.0);
        assert_close(unchanged.points_per_second(), 185.0);
        assert_close(scroll_offset, 500.0);
    }

    #[test]
    fn framing_frames_makes_them_nearly_fill_the_view_with_the_middle_of_them_in_the_middle_of_it() {
        let (framed, scroll_offset) = geometry_at(30, 1, 185.0).framing(300, 600, 1000.0);

        // Ten seconds, from the tenth to the twentieth, in 92 % of a thousand points.
        assert_close(framed.points_per_second(), 92.0);
        assert_close(framed.x_of_frame(600) - framed.x_of_frame(300), 920.0);
        // (The fifteenth second is the middle of them, and the middle of the view is 500 points from its edge.)
        assert_close(scroll_offset + 500.0, 15.0 * 92.0);
        // (What is framed begins a few points in from the edge of the view, and ends as many from the other.)
        assert_close(framed.x_of_frame(300) - scroll_offset, 40.0);
        assert_close(scroll_offset + 1000.0 - framed.x_of_frame(600), 40.0);
    }

    #[test]
    fn framing_a_few_frames_does_not_zoom_in_further_than_a_second_across_the_view() {
        let (one_frame, offset_of_one_frame) = geometry_at(30, 1, 185.0).framing(150, 151, 1000.0);
        assert_close(one_frame.points_per_second(), 920.0);
        let (middle_of_the_frame, view_middle) = (150.5 / 30.0 * 920.0, offset_of_one_frame + 500.0);
        assert_close(view_middle, middle_of_the_frame);
        let (empty, _) = geometry_at(30, 1, 185.0).framing(150, 150, 1000.0);
        assert_close(empty.points_per_second(), 920.0);
        let (backwards, _) = geometry_at(30, 1, 185.0).framing(150, 100, 1000.0);
        assert_close(backwards.points_per_second(), 920.0);
    }

    #[test]
    fn framing_more_than_the_strips_can_show_is_as_zoomed_out_as_they_go_and_a_few_frames_no_more_zoomed_in_than_they_go()
     {
        let (a_lot, _) = geometry_at(30, 1, 185.0).framing(0, 10_000_000, 1000.0);
        assert_close(a_lot.points_per_second(), 2.0);
        let (narrow_view, _) = geometry_at(30, 1, 185.0).framing(0, 3, 1.0e6);
        assert_close(narrow_view.points_per_second(), 1200.0);
    }

    #[test]
    fn framing_keeps_the_frame_rate_of_the_geometry() {
        let (framed, _) = geometry_at(25, 1, 185.0).framing(0, 250, 1000.0);
        assert_close(framed.points_per_second(), 92.0);
        assert_close(framed.x_of_frame(25), 92.0);
    }

    #[test]
    fn zooming_in_and_out_by_the_same_factor_gets_back_to_where_it_began() {
        let before = geometry_at(30, 1, 185.0);
        let (there, there_scroll_offset) = before.zoomed_about(1234.5, 2.5, 400.0);
        let (back, back_scroll_offset) = there.zoomed_about(there_scroll_offset, 1.0 / 2.5, 400.0);
        assert_close(back.points_per_second(), 185.0);
        assert_close(back_scroll_offset, 1234.5);
    }

    #[test]
    fn zooming_further_than_the_limit_stops_at_the_limit_and_still_keeps_the_anchor_still() {
        let before = geometry_at(30, 1, 1000.0);
        let (scroll_offset, anchor) = (3000.0, 250.0);
        let (after, after_scroll_offset) = before.zoomed_about(scroll_offset, 100.0, anchor);
        assert_close(after.points_per_second(), 1200.0);
        assert_close(
            (after_scroll_offset + anchor) / after.points_per_second(),
            (scroll_offset + anchor) / before.points_per_second(),
        );
        let (out, _) = before.zoomed_about(scroll_offset, 1.0e-9, anchor);
        assert_close(out.points_per_second(), 2.0);
    }
}
