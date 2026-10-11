//! How the tracks are laid out on the screen: where each frame is, which thumbnail pictures and
//! which stretches of a sound's waveform are visible, and where the view should scroll to. Plain
//! numbers only; drawing is done elsewhere.
//!
//! Every track is a strip as long as the finished video, and they all have the same scale, so
//! that a frame is at the same distance along each of them. Positions along a strip are measured
//! in screen points from the start of the strip ("content" positions); the view is a window onto
//! the strip that starts `scroll_offset` points along it.
//!
//! This file has the scale itself and what follows from it for the whole strip: where frames are,
//! what is under a position, and how far to scroll. The rest is in files of its own: the marks of the
//! ruler (`ruler`), the row of thumbnail pictures that is a video clip (`tiles`), and the bars of a
//! sound's waveform (`waveform`), and zooming (`zoom`).

mod ruler;
#[cfg(test)]
mod test_support;
mod tiles;
mod waveform;
mod zoom;

pub use ruler::{RulerTickKind, time_label};
pub use tiles::{TileLayout, tile_width_for};
pub use waveform::WaveformColumn;

use crate::project::{ClipPlacement, FrameRate, TimelineFrameIndex, Track};

/// The thumbnail pictures are never made narrower than this (in points), whatever the shape of
/// the video, so that a very tall video does not make the strip tiny.
const NARROWEST_SECOND_IN_POINTS: f64 = 48.0;

/// ...and never wider than this, so that a very wide video does not make a single second take up
/// the whole screen.
const WIDEST_SECOND_IN_POINTS: f64 = 480.0;

/// The scale of the strips: how many points one second of the finished video takes up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineGeometry {
    frame_rate: FrameRate,
    points_per_second: f64,
}

impl TimelineGeometry {
    /// Strips whose pictures are `track_height` points tall and keep the shape of a finished
    /// picture that is `picture_width_in_pixels` by `picture_height_in_pixels`. Each picture is one
    /// second wide, so the strips are that many points per second.
    pub fn for_picture(
        frame_rate: FrameRate,
        picture_width_in_pixels: u32,
        picture_height_in_pixels: u32,
        track_height: f32,
    ) -> Self {
        let shape = f64::from(picture_width_in_pixels.max(1)) / f64::from(picture_height_in_pixels.max(1));
        let picture_width =
            (f64::from(track_height) * shape).clamp(NARROWEST_SECOND_IN_POINTS, WIDEST_SECOND_IN_POINTS);
        Self {
            frame_rate,
            points_per_second: picture_width,
        }
    }

    /// Where frame number `frame_number` starts along the strip.
    pub fn x_of_frame(&self, frame_number: u64) -> f64 {
        self.frame_rate.seconds_at_frame(frame_number) * self.points_per_second
    }

    /// The whole length of the strip for a timeline of `total_frame_count` frames.
    pub fn content_width(&self, total_frame_count: u64) -> f64 {
        self.x_of_frame(total_frame_count)
    }

    /// The frame that is under position `x` along the strip, wherever that is: past the end of the
    /// timeline too, where the strips are empty. `None` only for a position before the start (or
    /// that is not a number).
    pub fn frame_at_x(&self, x: f64) -> Option<TimelineFrameIndex> {
        if x.is_nan() || x < 0.0 {
            return None;
        }
        Some(TimelineFrameIndex::new(
            self.frame_rate.frame_at_seconds(x / self.points_per_second),
        ))
    }

    /// The frame the playhead goes to when the ruler is clicked at position `x` along the strip.
    /// Unlike [`Self::frame_at_x`] this has an answer for every position: left of the strip is the
    /// first frame and right of it the last. Only an empty timeline (or `x` not being a number)
    /// has none.
    pub fn frame_picked_at_x(&self, x: f64, total_frame_count: u64) -> Option<TimelineFrameIndex> {
        if x.is_nan() || total_frame_count == 0 {
            return None;
        }
        let last_frame = total_frame_count - 1;
        let frame_number = if x < 0.0 {
            0
        } else {
            self.frame_rate
                .frame_at_seconds(x / self.points_per_second)
                .min(last_frame)
        };
        Some(TimelineFrameIndex::new(frame_number))
    }

    /// How far to scroll so that `frame` is in the middle of a view `view_width` wide, without
    /// scrolling past either end of the strip.
    pub fn scroll_offset_to_centre(&self, frame: TimelineFrameIndex, view_width: f64, total_frame_count: u64) -> f64 {
        let wanted = self.x_of_frame(frame.value()) - view_width / 2.0;
        self.clamp_scroll_offset(wanted, view_width, total_frame_count)
    }

    /// Keeps a scroll offset inside the strip: not before its start, and not so far that the end
    /// of the strip moves left of the view's right edge. A strip shorter than the view stays at 0.
    pub fn clamp_scroll_offset(&self, scroll_offset: f64, view_width: f64, total_frame_count: u64) -> f64 {
        let furthest = (self.content_width(total_frame_count) - view_width).max(0.0);
        scroll_offset.clamp(0.0, furthest)
    }

    /// Where each boundary between two clips that touch is along the strip, in order. (Clips that
    /// have empty time between them have no boundary: the empty time is between them.)
    pub fn clip_boundaries(&self, track: &Track) -> Vec<f64> {
        let placements: Vec<ClipPlacement<'_>> = track.placements().collect();
        placements
            .windows(2)
            .filter(|pair| pair[0].end_timeline_frame == pair[1].first_timeline_frame)
            .map(|pair| self.x_of_frame(pair[1].first_timeline_frame.value()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{assert_close, clip, geometry_at, track_of, two_clip_track};
    use super::*;
    use crate::project::TrackId;

    /// How wide one second of the finished video is, which at 30 frames per second is where frame
    /// 30 starts.
    fn width_of_one_second(picture_width: u32, picture_height: u32, track_height: f32) -> f64 {
        let frame_rate = FrameRate::new(30, 1).unwrap();
        TimelineGeometry::for_picture(frame_rate, picture_width, picture_height, track_height).x_of_frame(30)
    }

    #[test]
    fn pictures_keep_the_shape_of_the_finished_picture() {
        assert_close(width_of_one_second(1920, 1080, 90.0), 160.0);
        assert_close(width_of_one_second(1280, 720, 108.0), 192.0);
        assert_close(width_of_one_second(1000, 1000, 100.0), 100.0);
    }

    #[test]
    fn pictures_of_very_tall_or_very_wide_videos_stay_a_sensible_width() {
        // A phone held upright: 9:16 at 100 points tall is 56 wide: fine. 1:10 would be 10: too narrow.
        assert_close(width_of_one_second(1080, 1920, 100.0), 56.25);
        assert_close(width_of_one_second(100, 1000, 100.0), 48.0);
        assert_close(width_of_one_second(5000, 500, 100.0), 480.0);
    }

    #[test]
    fn frames_sit_at_their_share_of_a_second_along_the_strip() {
        let geometry = geometry_at(30, 1, 120.0);
        assert_close(geometry.x_of_frame(0), 0.0);
        assert_close(geometry.x_of_frame(15), 60.0);
        assert_close(geometry.x_of_frame(30), 120.0);
        assert_close(geometry.content_width(300), 1200.0);
    }

    #[test]
    fn a_position_along_the_strip_finds_the_frame_under_it_even_past_the_end_of_the_timeline() {
        // The strips are empty past the end of the timeline, and can be clicked there.
        let geometry = geometry_at(30, 1, 120.0);
        let frame = |x: f64| geometry.frame_at_x(x).map(TimelineFrameIndex::value);
        assert_eq!(frame(0.0), Some(0));
        assert_eq!(frame(3.9), Some(0));
        assert_eq!(frame(4.0), Some(1));
        assert_eq!(frame(1199.9), Some(299));
        assert_eq!(frame(1200.0), Some(300), "where a timeline of 300 frames has ended");
        assert_eq!(frame(120_000.0), Some(30_000));
        assert_eq!(frame(-0.1), None);
        assert_eq!(frame(f64::NAN), None);
    }

    #[test]
    fn every_frame_is_found_again_from_anywhere_inside_it() {
        for (numerator, denominator) in [(30, 1), (30000, 1001), (24000, 1001), (60, 1)] {
            let geometry = geometry_at(numerator, denominator, 173.0);
            for frame_number in 0..2000u64 {
                let start = geometry.x_of_frame(frame_number);
                let end = geometry.x_of_frame(frame_number + 1);
                for x in [start + 0.001, (start + end) / 2.0, end - 0.001] {
                    assert_eq!(
                        geometry.frame_at_x(x).map(TimelineFrameIndex::value),
                        Some(frame_number),
                        "x = {x} at {numerator}/{denominator}"
                    );
                }
            }
        }
    }

    #[test]
    fn scrolling_centres_a_frame_but_never_past_either_end() {
        let geometry = geometry_at(30, 1, 100.0);
        // 600 frames = 20 s = 2000 points; a view of 800 points.
        let centred = |frame: u64| geometry.scroll_offset_to_centre(TimelineFrameIndex::new(frame), 800.0, 600);
        assert_close(centred(300), 600.0); // frame 300 is at x = 1000; minus half the view
        assert_close(centred(0), 0.0);
        assert_close(centred(100), 0.0); // x = 333, minus 400, is before the start
        assert_close(centred(599), 1200.0); // the furthest the view can go: 2000 - 800
    }

    #[test]
    fn a_strip_shorter_than_the_view_never_scrolls() {
        let geometry = geometry_at(30, 1, 100.0);
        assert_close(
            geometry.scroll_offset_to_centre(TimelineFrameIndex::new(50), 5000.0, 90),
            0.0,
        );
        assert_close(geometry.clamp_scroll_offset(300.0, 5000.0, 90), 0.0);
    }

    #[test]
    fn a_scroll_offset_is_pulled_back_into_the_strip() {
        let geometry = geometry_at(30, 1, 100.0);
        assert_close(geometry.clamp_scroll_offset(-50.0, 800.0, 600), 0.0);
        assert_close(geometry.clamp_scroll_offset(700.0, 800.0, 600), 700.0);
        assert_close(geometry.clamp_scroll_offset(5000.0, 800.0, 600), 1200.0);
    }

    #[test]
    fn clicking_anywhere_along_the_ruler_picks_a_frame_and_the_ends_pick_the_first_and_last() {
        let geometry = geometry_at(30, 1, 120.0);
        let picked = |x: f64| geometry.frame_picked_at_x(x, 300).map(TimelineFrameIndex::value);
        assert_eq!(picked(0.0), Some(0));
        assert_eq!(picked(3.9), Some(0));
        assert_eq!(picked(4.0), Some(1));
        assert_eq!(picked(1199.9), Some(299));
        assert_eq!(picked(-80.0), Some(0), "to the left of the strip is its first frame");
        assert_eq!(picked(1200.0), Some(299), "to the right of the strip is its last frame");
        assert_eq!(picked(50_000.0), Some(299));
        assert_eq!(picked(f64::NAN), None);
        assert_eq!(geometry.frame_picked_at_x(10.0, 0), None, "an empty timeline has none");
    }

    #[test]
    fn clip_boundaries_are_where_each_clip_that_touches_the_one_before_it_begins() {
        let geometry = geometry_at(30, 1, 100.0);
        let boundaries = geometry.clip_boundaries(&two_clip_track());
        assert_eq!(boundaries.len(), 1);
        assert_close(boundaries[0], 333.333333);
        assert!(geometry.clip_boundaries(&track_of(vec![clip(0, 10, 1)])).is_empty());
        assert!(geometry.clip_boundaries(&track_of(vec![])).is_empty());
    }

    #[test]
    fn clips_with_empty_time_between_them_have_no_boundary() {
        let geometry = geometry_at(30, 1, 100.0);
        // Clip 0 is frames 0..30, then 15 empty frames, then clip 1 is frames 45..75, touching clip 2 (75..105).
        let track = Track::new(TrackId::new(0), "track 0").with_clips([
            (0, clip(0, 30, 1)),
            (15, clip(30, 60, 1)),
            (0, clip(60, 90, 1)),
        ]);
        let boundaries = geometry.clip_boundaries(&track);
        assert_eq!(boundaries.len(), 1, "only where clips touch: {boundaries:?}");
        assert_close(boundaries[0], 250.0); // frame 75 at 100 points a second
    }
}
