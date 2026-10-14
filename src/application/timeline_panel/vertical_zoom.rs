//! How far the rows of the timeline are zoomed up and down.
//!
//! The track of a row has a height of its own, which the user sets by dragging the boundary under its title box and which is kept
//! with the project. The vertical zoom is a different thing: how the timeline is looked at, like how far it is zoomed along and
//! scrolled, which is not kept with the project and is not an edit. Every row is drawn as tall as its track's height says times
//! the zoom, which makes all of them taller or shorter together and leaves the tracks as they are.

use crate::project::TrackHeight;

/// How much taller than their tracks' heights say the rows are drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::application) struct VerticalZoom {
    factor: f32,
}

impl VerticalZoom {
    /// The zoom a timeline starts at: rows are as tall as their tracks say.
    pub(in crate::application) const NORMAL: Self = Self { factor: 1.0 };

    /// The most the rows can be zoomed in: three times as tall as their tracks say.
    pub(in crate::application) const MOST_ZOOMED_IN: f32 = 3.0;

    /// A zoom of `factor` times, if that is a zoom at all (not a number, or not more than nothing, is the normal zoom).
    #[cfg(test)]
    pub(in crate::application) fn of(factor: f32) -> Self {
        if factor.is_finite() && factor > 0.0 {
            Self { factor }
        } else {
            Self::NORMAL
        }
    }

    /// How many times as tall as their tracks say the rows are.
    #[cfg(test)]
    pub(in crate::application) fn factor(self) -> f32 {
        self.factor
    }

    /// How tall the row of a track that has the height `track_height` is drawn, in points: as tall as the track's height says times
    /// the zoom, though never shorter than the least a track can be (what the title box at the left of the row needs).
    pub(in crate::application) fn height_of_a_row(self, track_height: TrackHeight) -> f32 {
        (track_height.points() * self.factor).max(TrackHeight::SMALLEST_IN_POINTS)
    }

    /// The height a track has to be given to be drawn `displayed` points tall, which is that divided by the zoom (as near to it as
    /// a track can be).
    pub(in crate::application) fn height_that_is_drawn_as(self, displayed: f32) -> TrackHeight {
        TrackHeight::from_points(displayed / self.factor)
    }

    /// The least the rows can be zoomed out, for tracks of which the tallest has a height of `tallest_track` points: as far as the
    /// rows of all of them are the least that rows can be, after which there is nothing more to zoom out (the zoom that did
    /// nothing, were it let go on, would have to be undone before it did anything again). It is never more than 1 (a track is never shorter
    /// than the least a row can be, and with no track at all the tallest is taken to be as short as that), so it is never more than
    /// [`Self::MOST_ZOOMED_IN`].
    fn most_zoomed_out_for(tallest_track: f32) -> f32 {
        TrackHeight::SMALLEST_IN_POINTS / tallest_track.max(TrackHeight::SMALLEST_IN_POINTS)
    }

    /// This zoom made `factor` times as much (less than 1 for zooming out), but not further in than [`Self::MOST_ZOOMED_IN`] and not
    /// further out than the tracks allow, the tallest of which has the height `tallest_track` points. (A zoom that is past a limit
    /// already, because the tracks have changed, is brought back to it by a change that goes further past it, and is changed by one
    /// that comes back.)
    pub(in crate::application) fn zoomed_by(self, factor: f32, tallest_track: f32) -> Self {
        if !(factor.is_finite() && factor > 0.0) {
            return self;
        }
        let least = Self::most_zoomed_out_for(tallest_track);
        Self {
            factor: (self.factor * factor).clamp(least, Self::MOST_ZOOMED_IN),
        }
    }
}

impl Default for VerticalZoom {
    fn default() -> Self {
        Self::NORMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: f32 = TrackHeight::DEFAULT_IN_POINTS;

    #[test]
    fn at_the_normal_zoom_a_row_is_as_tall_as_its_track_says() {
        for points in [44.0, 70.0, 104.0, 156.0] {
            assert_eq!(
                VerticalZoom::NORMAL.height_of_a_row(TrackHeight::from_points(points)),
                points
            );
        }
        assert_eq!(VerticalZoom::default(), VerticalZoom::NORMAL);
    }

    #[test]
    fn a_zoom_makes_every_row_as_many_times_as_tall_but_none_shorter_than_a_track_can_be() {
        let zoomed_in = VerticalZoom::of(2.0);
        assert_eq!(zoomed_in.height_of_a_row(TrackHeight::from_points(104.0)), 208.0);
        assert_eq!(zoomed_in.height_of_a_row(TrackHeight::from_points(60.0)), 120.0);
        let zoomed_out = VerticalZoom::of(0.5);
        assert_eq!(zoomed_out.height_of_a_row(TrackHeight::from_points(104.0)), 52.0);
        assert_eq!(
            zoomed_out.height_of_a_row(TrackHeight::from_points(60.0)),
            TrackHeight::SMALLEST_IN_POINTS,
            "30 is less than a row can be"
        );
        assert_eq!(
            zoomed_out.height_of_a_row(TrackHeight::from_points(44.0)),
            TrackHeight::SMALLEST_IN_POINTS
        );
    }

    #[test]
    fn the_height_that_makes_a_row_a_height_on_the_screen_is_that_divided_by_the_zoom_and_kept_within_what_a_track_can_be()
     {
        let zoomed_in = VerticalZoom::of(2.0);
        assert_eq!(zoomed_in.height_that_is_drawn_as(120.0).points(), 60.0);
        assert_eq!(
            zoomed_in.height_that_is_drawn_as(10_000.0).points(),
            TrackHeight::LARGEST_IN_POINTS
        );
        assert_eq!(
            zoomed_in.height_that_is_drawn_as(2.0).points(),
            TrackHeight::SMALLEST_IN_POINTS
        );
        // (And what it makes of a height is drawn as that, where the zoom allows it.)
        let height = zoomed_in.height_that_is_drawn_as(150.0);
        assert_eq!(zoomed_in.height_of_a_row(height), 150.0);
    }

    #[test]
    fn zooming_by_a_factor_multiplies_the_zoom_by_it_and_zooming_back_comes_back() {
        let zoom = VerticalZoom::NORMAL.zoomed_by(1.5, DEFAULT);
        assert_eq!(zoom.factor(), 1.5);
        let zoom = zoom.zoomed_by(2.0, DEFAULT);
        assert_eq!(zoom.factor(), 3.0);
        let back = VerticalZoom::NORMAL
            .zoomed_by(1.25, DEFAULT)
            .zoomed_by(1.0 / 1.25, DEFAULT);
        assert!((back.factor() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_rows_cannot_be_zoomed_in_past_three_times_as_tall() {
        let zoom = VerticalZoom::NORMAL.zoomed_by(1000.0, DEFAULT);
        assert_eq!(zoom.factor(), VerticalZoom::MOST_ZOOMED_IN);
        assert_eq!(zoom.zoomed_by(1.1, DEFAULT), zoom, "and nothing more is done there");
    }

    #[test]
    fn the_rows_cannot_be_zoomed_out_past_where_the_rows_of_all_the_tracks_are_the_least_that_a_row_can_be() {
        let least_for_new_tracks = TrackHeight::SMALLEST_IN_POINTS / DEFAULT;
        let zoom = VerticalZoom::NORMAL.zoomed_by(0.0001, DEFAULT);
        assert!((zoom.factor() - least_for_new_tracks).abs() < 1e-6, "{}", zoom.factor());
        assert_eq!(
            zoom.height_of_a_row(TrackHeight::from_points(DEFAULT)),
            TrackHeight::SMALLEST_IN_POINTS
        );

        // A track that is taller lets the rows be zoomed out further, until it is as short as the others.
        let for_tall_tracks = VerticalZoom::NORMAL.zoomed_by(0.0001, TrackHeight::LARGEST_IN_POINTS);
        assert!(for_tall_tracks.factor() < zoom.factor());
        assert!(
            (for_tall_tracks.height_of_a_row(TrackHeight::from_points(TrackHeight::LARGEST_IN_POINTS))
                - TrackHeight::SMALLEST_IN_POINTS)
                .abs()
                < 1e-3
        );

        // And when no track is taller than it has to be, there is no zooming out at all.
        let nothing_to_zoom_out = VerticalZoom::NORMAL.zoomed_by(0.5, TrackHeight::SMALLEST_IN_POINTS);
        assert_eq!(nothing_to_zoom_out, VerticalZoom::NORMAL);
    }

    #[test]
    fn with_no_track_at_all_there_is_nothing_to_zoom_out_either() {
        // (The tallest of no tracks is nothing, which is shorter than any track: not a limit that is further than where the zoom starts.)
        assert_eq!(VerticalZoom::NORMAL.zoomed_by(0.5, 0.0), VerticalZoom::NORMAL);
        assert_eq!(VerticalZoom::NORMAL.zoomed_by(0.5, f32::NAN), VerticalZoom::NORMAL);
        assert_eq!(VerticalZoom::NORMAL.zoomed_by(2.0, 0.0).factor(), 2.0);
    }

    #[test]
    fn a_zoom_that_the_tracks_have_left_past_a_limit_is_brought_back_to_it_by_the_next_change() {
        // Zoomed out as far as it goes for tall tracks, and then the tall tracks were made as short as the others.
        let far_out = VerticalZoom::NORMAL.zoomed_by(0.0001, TrackHeight::LARGEST_IN_POINTS);
        let after = far_out.zoomed_by(1.05, DEFAULT);
        assert!(
            (after.factor() - TrackHeight::SMALLEST_IN_POINTS / DEFAULT).abs() < 1e-6,
            "it did not creep up from way down: {}",
            after.factor()
        );
    }

    #[test]
    fn a_factor_that_is_not_a_number_or_not_more_than_nothing_changes_nothing() {
        let zoom = VerticalZoom::of(1.7);
        for factor in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
            assert_eq!(zoom.zoomed_by(factor, DEFAULT), zoom, "{factor}");
        }
        assert_eq!(VerticalZoom::of(f32::NAN), VerticalZoom::NORMAL);
        assert_eq!(VerticalZoom::of(-3.0), VerticalZoom::NORMAL);
    }
}
