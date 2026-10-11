//! The stretch of the strips that is in view, and where on the screen it is.
//!
//! Along the strips, positions are measured in points from the start of the first frame (see
//! [`TimelineGeometry`]). A view shows the stretch that starts `scroll_offset` points along, at the
//! left edge of its rectangle. The ruler, the strip of each track and the playhead each have one
//! (they share the scale, the scroll offset and the left edge; only the heights differ), so that
//! "where on the screen is this?" and "what is under the pointer?" are answered in one place
//! instead of by a formula written out wherever it is needed.

use egui::Rect;

use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::TimelineFrameIndex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StripView {
    /// The scale of the strips.
    pub geometry: TimelineGeometry,
    /// How far along the strips the left edge of the view is, in points.
    pub scroll_offset: f64,
    /// Where the view is on the screen.
    pub rectangle: Rect,
}

impl StripView {
    pub fn new(geometry: TimelineGeometry, scroll_offset: f64, rectangle: Rect) -> Self {
        Self {
            geometry,
            scroll_offset,
            rectangle,
        }
    }

    /// The same view of the strips, shown in a different part of the screen (the strip of another
    /// track, say): the scale and the scroll offset are the same, the rectangle is `rectangle`.
    pub fn shown_in(&self, rectangle: Rect) -> Self {
        Self { rectangle, ..*self }
    }

    /// How much of the strips the view shows along them, in points.
    pub fn width(&self) -> f64 {
        f64::from(self.rectangle.width())
    }

    /// Where on the screen position `x` along the strips is. This is outside the view's rectangle
    /// for a position that has been scrolled out of view.
    pub fn screen_x_of(&self, x: f64) -> f32 {
        self.rectangle.left() + (x - self.scroll_offset) as f32
    }

    /// Where on the screen frame number `frame_number` starts.
    pub fn screen_x_of_frame(&self, frame_number: u64) -> f32 {
        self.screen_x_of(self.geometry.x_of_frame(frame_number))
    }

    /// The position along the strips of the place on the screen at `screen_x`.
    pub fn x_at(&self, screen_x: f32) -> f64 {
        f64::from(screen_x - self.rectangle.left()) + self.scroll_offset
    }

    /// The frame that is under the place on the screen at `screen_x` (see
    /// [`TimelineGeometry::frame_at_x`]).
    pub fn frame_at(&self, screen_x: f32) -> Option<TimelineFrameIndex> {
        self.geometry.frame_at_x(self.x_at(screen_x))
    }

    /// The frame the playhead goes to when the ruler is clicked at `screen_x` (see
    /// [`TimelineGeometry::frame_picked_at_x`]).
    pub fn frame_picked_at(&self, screen_x: f32, total_frame_count: u64) -> Option<TimelineFrameIndex> {
        self.geometry.frame_picked_at_x(self.x_at(screen_x), total_frame_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::FrameRate;
    use egui::{pos2, vec2};

    /// 120 points a second at 30 frames a second, so a frame is 4 points wide. (A square picture
    /// makes a second as wide as the strips are tall, which keeps all the numbers exact.)
    fn geometry() -> TimelineGeometry {
        TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 100, 100, 120.0)
    }

    /// A view 600 points wide whose left edge is at 100 on the screen and 250 points along the strips.
    fn view() -> StripView {
        StripView::new(
            geometry(),
            250.0,
            Rect::from_min_size(pos2(100.0, 40.0), vec2(600.0, 104.0)),
        )
    }

    #[test]
    fn the_position_at_the_left_edge_of_the_view_is_at_the_left_edge_of_its_rectangle() {
        assert_eq!(view().screen_x_of(250.0), 100.0);
    }

    #[test]
    fn positions_further_along_are_further_right_by_the_same_distance() {
        assert_eq!(view().screen_x_of(400.0), 250.0);
        assert_eq!(view().screen_x_of(850.0), 700.0, "the right edge of the view");
    }

    #[test]
    fn positions_that_have_been_scrolled_out_of_view_are_off_the_left_edge_or_the_right_edge() {
        assert_eq!(view().screen_x_of(0.0), -150.0);
        assert!(view().screen_x_of(900.0) > view().rectangle.right());
    }

    #[test]
    fn a_frame_is_where_the_geometry_says_it_is_along_the_strips_seen_through_the_view() {
        // Frame 90 starts 3 seconds in, which is 360 points along the strips.
        assert_eq!(view().screen_x_of_frame(90), 100.0 + (360.0 - 250.0));
        assert_eq!(view().screen_x_of_frame(0), 100.0 - 250.0);
    }

    #[test]
    fn the_place_on_the_screen_at_a_position_is_that_position_again() {
        let view = view();
        for x in [0.0, 1.0, 250.0, 333.25, 850.0, 4000.0] {
            let found = view.x_at(view.screen_x_of(x));
            assert!((found - x).abs() < 1e-3, "{x} became {found}");
        }
    }

    #[test]
    fn the_frame_under_a_place_on_the_screen_is_the_frame_that_starts_there_or_before() {
        let view = view();
        assert_eq!(
            view.frame_at(view.screen_x_of_frame(90)),
            Some(TimelineFrameIndex::new(90))
        );
        assert_eq!(
            view.frame_at(view.screen_x_of_frame(90) + 3.5),
            Some(TimelineFrameIndex::new(90)),
            "a frame is 4 points wide"
        );
        assert_eq!(
            view.frame_at(view.screen_x_of_frame(91)),
            Some(TimelineFrameIndex::new(91))
        );
    }

    #[test]
    fn there_is_no_frame_under_a_place_that_is_before_the_start_of_the_strips() {
        // The strips start 250 points to the left of the view.
        assert_eq!(view().frame_at(100.0 - 251.0), None);
    }

    #[test]
    fn picking_on_the_ruler_beyond_either_end_gives_the_first_or_the_last_frame() {
        let view = view();
        assert_eq!(view.frame_picked_at(-5000.0, 300), Some(TimelineFrameIndex::new(0)));
        assert_eq!(view.frame_picked_at(5000.0, 300), Some(TimelineFrameIndex::new(299)));
        assert_eq!(
            view.frame_picked_at(100.0, 0),
            None,
            "an empty timeline has nothing to pick"
        );
    }

    #[test]
    fn the_same_view_shown_in_another_part_of_the_screen_keeps_the_scale_and_the_scroll_offset() {
        let elsewhere = Rect::from_min_size(pos2(100.0, 400.0), vec2(600.0, 30.0));
        let other = view().shown_in(elsewhere);
        assert_eq!(other.rectangle, elsewhere);
        assert_eq!(other.screen_x_of(400.0), view().screen_x_of(400.0));
        assert_eq!(other.width(), 600.0);
    }

    #[test]
    fn a_view_on_the_strips_is_as_wide_as_its_rectangle() {
        assert_eq!(view().width(), 600.0);
    }
}
