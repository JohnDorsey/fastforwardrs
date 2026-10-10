//! How the video track is laid out on the screen: where each frame is, which thumbnail pictures
//! are visible, and where the view should scroll to. Plain numbers only; drawing is done elsewhere.
//!
//! The track is a strip of pictures taken from the video, one for every second of the finished
//! video, laid side by side with no gaps. Positions along it are measured in screen points from
//! the start of the strip ("content" positions); the view is a window onto the strip that starts
//! `scroll_offset` points along it.

use crate::project::{FrameRate, SourceFrameIndex, TimelineFrameIndex, VideoTrack};

/// The finer ticks of the ruler (one for each tenth of a second) are left out when they would be
/// closer together than this many points.
const SMALLEST_POINTS_BETWEEN_TENTH_OF_A_SECOND_TICKS: f64 = 8.0;

/// The thumbnail pictures are never made narrower than this (in points), whatever the shape of
/// the video, so that a very tall video does not make the strip tiny.
const NARROWEST_SECOND_IN_POINTS: f64 = 48.0;

/// ...and never wider than this, so that a very wide video does not make a single second take up
/// the whole screen.
const WIDEST_SECOND_IN_POINTS: f64 = 480.0;

/// One thumbnail picture on the strip, or the visible part of one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileLayout {
    pub clip_index: usize,
    /// Where the tile starts along the strip.
    pub left: f64,
    /// How much of the tile there is: a whole second's width, or less where a clip ends.
    pub width: f64,
    /// How much of the picture's width fits in the tile (1 for a whole tile, less when the clip
    /// ends in the middle of one): the picture is cut off rather than squeezed.
    pub fraction_of_picture_shown: f32,
    /// The source frame this tile's picture stands for.
    pub source_frame: SourceFrameIndex,
    /// Whether this tile is the first of its clip, so that its left edge is the clip's left edge.
    pub starts_clip: bool,
    /// Whether this tile is the last of its clip, so that its right edge is the clip's right edge.
    pub ends_clip: bool,
}

/// One mark of the ruler above the video track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerTick {
    /// Where the mark is along the strip, in points.
    pub x: f64,
    pub kind: RulerTickKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulerTickKind {
    /// A whole second of the finished video.
    Second,
    /// A tenth of a second between two seconds.
    Tenth,
}

/// The scale of the strip: how many points one second of the finished video takes up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineGeometry {
    frame_rate: FrameRate,
    points_per_second: f64,
}

impl TimelineGeometry {
    /// A strip whose pictures are `track_height` points tall and keep the shape of a video that is
    /// shown `display_width_in_pixels` by `display_height_in_pixels`. Each picture is one second
    /// wide, so the strip is that many points per second.
    pub fn for_video(
        frame_rate: FrameRate,
        display_width_in_pixels: u32,
        display_height_in_pixels: u32,
        track_height: f32,
    ) -> Self {
        let shape = f64::from(display_width_in_pixels.max(1)) / f64::from(display_height_in_pixels.max(1));
        let picture_width =
            (f64::from(track_height) * shape).clamp(NARROWEST_SECOND_IN_POINTS, WIDEST_SECOND_IN_POINTS);
        Self {
            frame_rate,
            points_per_second: picture_width,
        }
    }

    pub fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    /// Where frame number `frame_number` starts along the strip.
    pub fn x_of_frame(&self, frame_number: u64) -> f64 {
        self.frame_rate.seconds_at_frame(frame_number) * self.points_per_second
    }

    /// The whole length of the strip for a timeline of `total_frame_count` frames.
    pub fn content_width(&self, total_frame_count: u64) -> f64 {
        self.x_of_frame(total_frame_count)
    }

    /// The frame that is under position `x` along the strip, or `None` if `x` is before the start
    /// or past the end.
    pub fn frame_at_x(&self, x: f64, total_frame_count: u64) -> Option<TimelineFrameIndex> {
        if x.is_nan() || x < 0.0 {
            return None;
        }
        let frame_number = self.frame_rate.frame_at_seconds(x / self.points_per_second);
        (frame_number < total_frame_count).then_some(TimelineFrameIndex::new(frame_number))
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

    /// The marks of the ruler that are in the view `view_width` wide that starts `scroll_offset`
    /// along the strip, in order from left to right: one for each second of the finished video,
    /// and (when there is room) nine between each pair. They follow real time, not frames, so
    /// they stay true at frame rates such as 29.97 where a second is not a whole number of frames.
    pub fn ruler_ticks(&self, total_frame_count: u64, scroll_offset: f64, view_width: f64) -> Vec<RulerTick> {
        if total_frame_count == 0 {
            return Vec::new();
        }
        let points_per_tenth = self.points_per_second / 10.0;
        let has_tenths = points_per_tenth >= SMALLEST_POINTS_BETWEEN_TENTH_OF_A_SECOND_TICKS;
        let (spacing, ticks_per_second) = if has_tenths {
            (points_per_tenth, 10)
        } else {
            (self.points_per_second, 1)
        };

        let view_left = scroll_offset.max(0.0);
        let view_right = (scroll_offset + view_width).min(self.content_width(total_frame_count));
        if view_right < view_left {
            return Vec::new();
        }
        let first_tick_number = (view_left / spacing).ceil() as u64;
        let last_tick_number = (view_right / spacing).floor() as u64;
        (first_tick_number..=last_tick_number)
            .map(|tick_number| RulerTick {
                x: tick_number as f64 * spacing,
                kind: if tick_number % ticks_per_second == 0 {
                    RulerTickKind::Second
                } else {
                    RulerTickKind::Tenth
                },
            })
            .collect()
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

    /// The thumbnail tiles that are in the view `view_width` wide that starts `scroll_offset`
    /// along the strip. Every clip starts a fresh row of tiles at its left edge, each a second
    /// wide, with the last one cut short where the clip ends.
    pub fn visible_tiles(&self, track: &VideoTrack, scroll_offset: f64, view_width: f64) -> Vec<TileLayout> {
        let view_left = scroll_offset;
        let view_right = scroll_offset + view_width;
        let mut tiles = Vec::new();
        for placement in track.placements() {
            let clip_left = self.x_of_frame(placement.first_timeline_frame.value());
            let clip_right = self.x_of_frame(placement.end_timeline_frame.value());
            if clip_right <= view_left || clip_left >= view_right {
                continue;
            }
            let first_tile = ((view_left.max(clip_left) - clip_left) / self.points_per_second).floor() as u64;
            let last_tile = ((view_right.min(clip_right) - clip_left) / self.points_per_second).ceil() as u64;
            let clip_frame_count = placement.end_timeline_frame.value() - placement.first_timeline_frame.value();
            for tile_number in first_tile..last_tile {
                let left = clip_left + tile_number as f64 * self.points_per_second;
                let width = (left + self.points_per_second).min(clip_right) - left;
                if width <= 0.0 {
                    continue;
                }
                // The tile stands for the frame on screen when its second begins.
                let offset_in_clip = self
                    .frame_rate
                    .first_frame_in_whole_second(tile_number)
                    .min(clip_frame_count - 1);
                let Some(source_frame) = placement.clip.source_frame_at_timeline_offset(offset_in_clip) else {
                    continue;
                };
                tiles.push(TileLayout {
                    clip_index: placement.clip_index,
                    left,
                    width,
                    fraction_of_picture_shown: (width / self.points_per_second) as f32,
                    source_frame,
                    starts_clip: tile_number == 0,
                    ends_clip: left + self.points_per_second >= clip_right,
                });
            }
        }
        tiles
    }

    /// Where each boundary between two clips is along the strip, in order.
    pub fn clip_boundaries(&self, track: &VideoTrack) -> Vec<f64> {
        track
            .placements()
            .skip(1)
            .map(|placement| self.x_of_frame(placement.first_timeline_frame.value()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Clip, SourceVideoIdentifier, SpeedFactor};

    fn geometry_at(numerator: u32, denominator: u32, points_per_second: f64) -> TimelineGeometry {
        TimelineGeometry {
            frame_rate: FrameRate::new(numerator, denominator).unwrap(),
            points_per_second,
        }
    }

    fn clip(first: u64, end: u64, speed: u32) -> Clip {
        Clip::new(
            SourceVideoIdentifier::new(1),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::new(speed).unwrap(),
        )
        .unwrap()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-6, "{actual} is not {expected}");
    }

    /// How wide one second of the finished video is, which at 30 frames per second is where frame
    /// 30 starts.
    fn width_of_one_second(display_width: u32, display_height: u32, track_height: f32) -> f64 {
        let frame_rate = FrameRate::new(30, 1).unwrap();
        TimelineGeometry::for_video(frame_rate, display_width, display_height, track_height).x_of_frame(30)
    }

    #[test]
    fn pictures_keep_the_shape_of_the_video() {
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
    fn a_position_along_the_strip_finds_the_frame_under_it() {
        let geometry = geometry_at(30, 1, 120.0);
        let frame = |x: f64| geometry.frame_at_x(x, 300).map(TimelineFrameIndex::value);
        assert_eq!(frame(0.0), Some(0));
        assert_eq!(frame(3.9), Some(0));
        assert_eq!(frame(4.0), Some(1));
        assert_eq!(frame(1199.9), Some(299));
        assert_eq!(frame(1200.0), None, "the strip ends at 1200");
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
                        geometry.frame_at_x(x, 2000).map(TimelineFrameIndex::value),
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

    fn two_clip_track() -> VideoTrack {
        // Timeline at 30 fps: clip 0 is 100 frames (3.333 s), clip 1 is 200 source frames at 4x = 50 frames (1.667 s).
        VideoTrack::new(vec![clip(0, 100, 1), clip(100, 300, 4)])
    }

    #[test]
    fn each_clip_starts_a_fresh_row_of_tiles_with_the_last_tile_cut_short() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), 0.0, 10_000.0);

        // Clip 0 is 333.33 points wide: tiles of 100, 100, 100 and 33.33.
        let clip_zero: Vec<&TileLayout> = tiles.iter().filter(|tile| tile.clip_index == 0).collect();
        assert_eq!(clip_zero.len(), 4);
        let lefts: Vec<f64> = clip_zero.iter().map(|tile| tile.left).collect();
        assert_eq!(lefts, vec![0.0, 100.0, 200.0, 300.0]);
        assert_close(clip_zero[3].width, 33.333333);
        assert!((clip_zero[3].fraction_of_picture_shown - 1.0 / 3.0).abs() < 1e-3);
        assert_eq!(clip_zero[0].fraction_of_picture_shown, 1.0);
        let source_frames: Vec<u64> = clip_zero.iter().map(|tile| tile.source_frame.value()).collect();
        assert_eq!(
            source_frames,
            vec![0, 30, 60, 90],
            "one second further on in the video for each tile"
        );

        // Clip 1 starts at x = 333.33 and is 166.67 wide: a whole tile and two thirds of another.
        let clip_one: Vec<&TileLayout> = tiles.iter().filter(|tile| tile.clip_index == 1).collect();
        assert_eq!(clip_one.len(), 2);
        assert_close(clip_one[0].left, 333.333333);
        assert_close(clip_one[0].width, 100.0);
        assert_close(clip_one[1].width, 66.666667);
        // At 4x, each second of the finished video is 4 seconds of the source: frames 100 and 220.
        let source_frames: Vec<u64> = clip_one.iter().map(|tile| tile.source_frame.value()).collect();
        assert_eq!(source_frames, vec![100, 220]);
    }

    #[test]
    fn the_tiles_of_a_clip_fill_it_exactly_with_no_gaps_or_overlaps() {
        for (numerator, denominator) in [(30, 1), (30000, 1001)] {
            let geometry = geometry_at(numerator, denominator, 137.0);
            let track = VideoTrack::new(vec![clip(0, 250, 1), clip(250, 900, 3), clip(900, 913, 1)]);
            let tiles = geometry.visible_tiles(&track, 0.0, 1.0e6);
            for placement in track.placements() {
                let clip_tiles: Vec<&TileLayout> = tiles
                    .iter()
                    .filter(|tile| tile.clip_index == placement.clip_index)
                    .collect();
                let mut expected_left = geometry.x_of_frame(placement.first_timeline_frame.value());
                for tile in &clip_tiles {
                    assert_close(tile.left, expected_left);
                    expected_left += tile.width;
                }
                assert_close(expected_left, geometry.x_of_frame(placement.end_timeline_frame.value()));
            }
        }
    }

    #[test]
    fn only_the_tiles_inside_the_view_are_returned() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), 150.0, 200.0); // the view covers 150..350
        let lefts: Vec<f64> = tiles.iter().map(|tile| tile.left).collect();
        // Tiles starting at 100 (shown from 150), 200, 300 (to 333.33) of clip 0. Clip 1 starts at 333.33.
        assert_eq!(lefts.len(), 4, "{lefts:?}");
        assert_close(lefts[0], 100.0);
        assert_close(lefts[1], 200.0);
        assert_close(lefts[2], 300.0);
        assert_close(lefts[3], 333.333333);
    }

    #[test]
    fn a_view_beyond_the_strip_has_no_tiles_and_an_empty_track_has_none_either() {
        let geometry = geometry_at(30, 1, 100.0);
        assert!(geometry.visible_tiles(&two_clip_track(), 600.0, 400.0).is_empty());
        assert!(geometry.visible_tiles(&VideoTrack::default(), 0.0, 400.0).is_empty());
    }

    #[test]
    fn tile_pictures_never_come_from_beyond_the_end_of_their_clip() {
        let geometry = geometry_at(30, 1, 100.0);
        // A 10 frame clip: one cut-short tile whose picture is the clip's first frame.
        let track = VideoTrack::new(vec![clip(500, 510, 1)]);
        let tiles = geometry.visible_tiles(&track, 0.0, 1000.0);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].source_frame, SourceFrameIndex::new(500));
    }

    #[test]
    fn the_first_tile_of_a_clip_starts_it_and_the_last_ends_it() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), 0.0, 10_000.0);
        let flags = |clip_index: usize| -> Vec<(bool, bool)> {
            tiles
                .iter()
                .filter(|tile| tile.clip_index == clip_index)
                .map(|tile| (tile.starts_clip, tile.ends_clip))
                .collect()
        };
        // Clip 0 has four tiles, clip 1 has two.
        assert_eq!(
            flags(0),
            vec![(true, false), (false, false), (false, false), (false, true)]
        );
        assert_eq!(flags(1), vec![(true, false), (false, true)]);
    }

    #[test]
    fn a_clip_that_is_a_single_cut_short_tile_both_starts_and_ends() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&VideoTrack::new(vec![clip(0, 10, 1)]), 0.0, 1000.0);
        assert_eq!((tiles[0].starts_clip, tiles[0].ends_clip), (true, true));
    }

    #[test]
    fn scrolling_the_start_of_a_clip_out_of_view_does_not_change_which_tile_starts_it() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), 150.0, 200.0);
        // The first tile in view is the second of clip 0 (it begins at 100): it does not start the clip.
        assert!(!tiles[0].starts_clip);
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
    fn the_ruler_has_a_tick_for_every_second_and_finer_ones_between_when_there_is_room() {
        let geometry = geometry_at(30, 1, 100.0);
        // 5 seconds of timeline (150 frames) is 500 points: the view shows all of it.
        let ticks = geometry.ruler_ticks(150, 0.0, 1000.0);

        let seconds: Vec<f64> = ticks
            .iter()
            .filter(|tick| tick.kind == RulerTickKind::Second)
            .map(|tick| tick.x)
            .collect();
        assert_eq!(seconds.len(), 6, "0 s, 1 s, ... 5 s: {seconds:?}");
        for (second, x) in seconds.iter().enumerate() {
            assert_close(*x, second as f64 * 100.0);
        }
        let tenths: Vec<f64> = ticks
            .iter()
            .filter(|tick| tick.kind == RulerTickKind::Tenth)
            .map(|tick| tick.x)
            .collect();
        assert_eq!(tenths.len(), 5 * 9, "nine between each pair of seconds: {tenths:?}");
        assert_close(tenths[0], 10.0);
        assert_close(tenths[9], 110.0);
        assert!(ticks.windows(2).all(|pair| pair[0].x < pair[1].x), "in order");
    }

    #[test]
    fn the_ruler_leaves_out_the_finer_ticks_when_they_would_be_crowded() {
        // 48 points a second is the narrowest the strip gets: tenths would be 4.8 points apart.
        let geometry = geometry_at(30, 1, 48.0);
        let ticks = geometry.ruler_ticks(150, 0.0, 1000.0);
        assert!(ticks.iter().all(|tick| tick.kind == RulerTickKind::Second));
        assert_eq!(ticks.len(), 6);
    }

    #[test]
    fn the_ruler_only_has_ticks_inside_the_view_and_inside_the_strip() {
        let geometry = geometry_at(30, 1, 100.0);
        // 20 seconds of timeline; the view covers 450..850 along the strip.
        let ticks = geometry.ruler_ticks(600, 450.0, 400.0);
        assert!(ticks.iter().all(|tick| (450.0..=850.0).contains(&tick.x)), "{ticks:?}");
        let seconds: Vec<f64> = ticks
            .iter()
            .filter(|tick| tick.kind == RulerTickKind::Second)
            .map(|tick| tick.x)
            .collect();
        assert_eq!(seconds, vec![500.0, 600.0, 700.0, 800.0]);

        // Past the end of a 5 second strip there is nothing.
        assert!(geometry.ruler_ticks(150, 600.0, 400.0).is_empty());
        assert!(
            geometry.ruler_ticks(0, 0.0, 400.0).is_empty(),
            "an empty timeline has no ruler"
        );
    }

    #[test]
    fn ruler_ticks_follow_real_time_so_they_stay_true_at_ntsc_frame_rates() {
        // At 29.97 frames per second, frame 30 is at 1.001 s: the one second tick is a hair before it.
        let geometry = geometry_at(30000, 1001, 100.0);
        let ticks = geometry.ruler_ticks(300, 0.0, 10_000.0);
        let second_ticks: Vec<f64> = ticks
            .iter()
            .filter(|tick| tick.kind == RulerTickKind::Second)
            .map(|tick| tick.x)
            .collect();
        assert_close(second_ticks[1], 100.0);
        assert!(geometry.x_of_frame(30) > 100.0);
    }

    #[test]
    fn clip_boundaries_are_where_each_clip_after_the_first_begins() {
        let geometry = geometry_at(30, 1, 100.0);
        let boundaries = geometry.clip_boundaries(&two_clip_track());
        assert_eq!(boundaries.len(), 1);
        assert_close(boundaries[0], 333.333333);
        assert!(
            geometry
                .clip_boundaries(&VideoTrack::new(vec![clip(0, 10, 1)]))
                .is_empty()
        );
    }
}
