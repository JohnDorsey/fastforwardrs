//! The row of thumbnail pictures that a video clip is made of.
//!
//! A video clip on a track is a row of pictures taken from the video, laid side by side with no
//! gaps, starting at the clip's left edge. Each is as wide as a picture of the video is on a row of
//! the height of the track, so that it is neither stretched nor cropped, and stands for the moment
//! at its left edge. So the pictures are further apart in time the further the timeline is zoomed
//! out and the taller the track is, and nearer to one another the other way: the finer pictures
//! that this needs (see [`ThumbnailKey`]) are what each tile says it wants.

use super::TimelineGeometry;
use crate::media::{ThumbnailKey, thumbnail_level_for_spacing};
use crate::project::{Clip, ClipId, SourceFrameIndex, SourceMediaIdentifier, Track};

/// A picture is never made narrower than this, in points, however small the track and the video
/// are, so that a view never has a flood of tiles.
const NARROWEST_TILE: f64 = 4.0;

/// The shapes of picture that a tile keeps as they are: a video narrower than 9 to 20 or wider than
/// 9 to 2 is drawn in a tile of that shape (and stretched), so that a very tall video does not
/// make a sliver and a very wide one does not make a tile as wide as the window.
const NARROWEST_PICTURE_SHAPE: f64 = 0.45;
const WIDEST_PICTURE_SHAPE: f64 = 4.5;

/// How wide a tile is, in points, on a row `row_height` points tall, for a video whose pictures
/// are `picture_width_in_pixels` by `picture_height_in_pixels`: as wide as the picture is at that height.
pub fn tile_width_for(row_height: f32, picture_width_in_pixels: u32, picture_height_in_pixels: u32) -> f64 {
    let shape = f64::from(picture_width_in_pixels.max(1)) / f64::from(picture_height_in_pixels.max(1));
    f64::from(row_height) * shape.clamp(NARROWEST_PICTURE_SHAPE, WIDEST_PICTURE_SHAPE)
}

/// One thumbnail picture on the strip, or the visible part of one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileLayout {
    pub clip: ClipId,
    /// The video the picture of the tile is taken from.
    pub source: SourceMediaIdentifier,
    /// Where the tile starts along the strip.
    pub left: f64,
    /// How much of the tile there is: all of its width, or less where a clip ends.
    pub width: f64,
    /// How much of the picture's width fits in the tile (1 for a whole tile, less when the clip
    /// ends in the middle of one): the picture is cut off rather than squeezed.
    pub fraction_of_picture_shown: f32,
    /// The source frame this tile's picture stands for: the one that is on screen at its left edge.
    pub source_frame: SourceFrameIndex,
    /// The picture to show: of the level that is fine enough for how far apart the tiles are in
    /// time, and the nearest of that level to the frame the tile stands for.
    pub picture: ThumbnailKey,
    /// Whether this tile is the first of its clip, so that its left edge is the clip's left edge.
    pub starts_clip: bool,
    /// Whether this tile is the last of its clip, so that its right edge is the clip's right edge.
    pub ends_clip: bool,
}

impl TimelineGeometry {
    /// The thumbnail tiles of the video clips of `track` that are in the view `view_width` wide
    /// that starts `scroll_offset` along the strip. Every clip starts a fresh row of tiles at its
    /// left edge, each as wide as `tile_width_of` says for the video it is from (see
    /// [`tile_width_for`]), with the last one cut short where the clip ends.
    pub fn visible_tiles(
        &self,
        track: &Track,
        tile_width_of: impl Fn(SourceMediaIdentifier) -> f64,
        scroll_offset: f64,
        view_width: f64,
    ) -> Vec<TileLayout> {
        let view_left = scroll_offset;
        let view_right = scroll_offset + view_width;
        let mut tiles = Vec::new();
        for placement in track.placements() {
            let Clip::Video(media) = placement.clip else {
                continue;
            };
            let tile_width = tile_width_of(media.source());
            if tile_width.is_nan() || tile_width < NARROWEST_TILE {
                continue;
            }
            let clip_left = self.x_of_frame(placement.first_timeline_frame.value());
            let clip_right = self.x_of_frame(placement.end_timeline_frame.value());
            if clip_right <= view_left || clip_left >= view_right {
                continue;
            }
            // How far apart the tiles are in time, on the timeline and in the video (which a clip
            // that plays faster goes through faster): the pictures have to be at least this close.
            let seconds_between_tiles = tile_width / self.points_per_second;
            let seconds_of_video_between_tiles = seconds_between_tiles * f64::from(placement.effective_speed.value());
            let level = thumbnail_level_for_spacing(seconds_of_video_between_tiles, self.frame_rate);

            let first_tile = ((view_left.max(clip_left) - clip_left) / tile_width).floor() as u64;
            let last_tile = ((view_right.min(clip_right) - clip_left) / tile_width).ceil() as u64;
            let clip_frame_count = placement.timeline_frame_count();
            for tile_number in first_tile..last_tile {
                let left = clip_left + tile_number as f64 * tile_width;
                let width = (left + tile_width).min(clip_right) - left;
                if width <= 0.0 {
                    continue;
                }
                // The tile stands for the frame that is on screen where its left edge is.
                let offset_in_clip = self
                    .frame_rate
                    .frame_at_seconds(tile_number as f64 * seconds_between_tiles)
                    .min(clip_frame_count - 1);
                let Some(source_frame) =
                    media.source_frame_at_timeline_offset(placement.effective_speed, offset_in_clip)
                else {
                    continue;
                };
                tiles.push(TileLayout {
                    clip: placement.id,
                    source: media.source(),
                    left,
                    width,
                    fraction_of_picture_shown: (width / tile_width) as f32,
                    source_frame,
                    picture: ThumbnailKey::nearest(level, self.frame_rate.seconds_at_frame(source_frame.value())),
                    starts_clip: tile_number == 0,
                    ends_clip: left + tile_width >= clip_right,
                });
            }
        }
        tiles
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{assert_close, clip, clip_id, geometry_at, media, track_of, two_clip_track};
    use super::*;
    use crate::project::{MediaClip, SpeedFactor, TextClip, TextObject, TrackId};

    /// The width of the tiles when a tile is as wide as a second of the video is on the screen, which
    /// is what they were before a tile could be of any width.
    fn a_second_wide(geometry: &TimelineGeometry) -> impl Fn(SourceMediaIdentifier) -> f64 + use<> {
        let width = geometry.points_per_second();
        move |_| width
    }

    #[test]
    fn each_clip_starts_a_fresh_row_of_tiles_with_the_last_tile_cut_short() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), a_second_wide(&geometry), 0.0, 10_000.0);

        // Clip 0 is 333.33 points wide: tiles of 100, 100, 100 and 33.33.
        let clip_zero: Vec<&TileLayout> = tiles.iter().filter(|tile| tile.clip == clip_id(0)).collect();
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
        let clip_one: Vec<&TileLayout> = tiles.iter().filter(|tile| tile.clip == clip_id(1)).collect();
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
            let track = track_of(vec![clip(0, 250, 1), clip(250, 900, 3), clip(900, 913, 1)]);
            let tiles = geometry.visible_tiles(&track, a_second_wide(&geometry), 0.0, 1.0e6);
            for placement in track.placements() {
                let clip_tiles: Vec<&TileLayout> = tiles.iter().filter(|tile| tile.clip == placement.id).collect();
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
        let tiles = geometry.visible_tiles(&two_clip_track(), a_second_wide(&geometry), 150.0, 200.0); // the view covers 150..350
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
        assert!(
            geometry
                .visible_tiles(&two_clip_track(), a_second_wide(&geometry), 600.0, 400.0)
                .is_empty()
        );
        assert!(
            geometry
                .visible_tiles(&track_of(vec![]), a_second_wide(&geometry), 0.0, 400.0)
                .is_empty()
        );
    }

    #[test]
    fn tile_pictures_never_come_from_beyond_the_end_of_their_clip() {
        let geometry = geometry_at(30, 1, 100.0);
        // A 10 frame clip: one cut-short tile whose picture is the clip's first frame.
        let track = track_of(vec![clip(500, 510, 1)]);
        let tiles = geometry.visible_tiles(&track, a_second_wide(&geometry), 0.0, 1000.0);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].source_frame, SourceFrameIndex::new(500));
        assert_eq!(
            tiles[0].source,
            SourceMediaIdentifier::new(1),
            "and it says which video it is from"
        );
    }

    #[test]
    fn the_first_tile_of_a_clip_starts_it_and_the_last_ends_it() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), a_second_wide(&geometry), 0.0, 10_000.0);
        let flags = |clip: u32| -> Vec<(bool, bool)> {
            tiles
                .iter()
                .filter(|tile| tile.clip == clip_id(clip))
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
        let tiles = geometry.visible_tiles(&track_of(vec![clip(0, 10, 1)]), a_second_wide(&geometry), 0.0, 1000.0);
        assert_eq!((tiles[0].starts_clip, tiles[0].ends_clip), (true, true));
    }

    #[test]
    fn scrolling_the_start_of_a_clip_out_of_view_does_not_change_which_tile_starts_it() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&two_clip_track(), a_second_wide(&geometry), 150.0, 200.0);
        // The first tile in view is the second of clip 0 (it begins at 100): it does not start the clip.
        assert!(!tiles[0].starts_clip);
    }

    #[test]
    fn tiles_are_made_for_video_clips_only_and_stay_where_the_clips_are() {
        let geometry = geometry_at(30, 1, 100.0);
        let text = Clip::Text(TextClip::new(TextObject::default(), 60).unwrap());
        let sound = Clip::Audio(media(0, 90, 1));
        // A sound for 90 frames, then 30 empty frames, then a video clip of 60 frames, then a text of 60 frames.
        let track = Track::new(TrackId::new(0), "track 0").with_clips([(0, sound), (30, clip(0, 60, 1)), (0, text)]);
        let tiles = geometry.visible_tiles(&track, a_second_wide(&geometry), 0.0, 10_000.0);
        assert_eq!(
            tiles.len(),
            2,
            "two seconds of the video clip, nothing for the others: {tiles:?}"
        );
        assert!(tiles.iter().all(|tile| tile.clip == clip_id(1)));
        assert_close(tiles[0].left, 400.0); // the video clip starts at frame 120
        assert_close(tiles[1].left, 500.0);
    }

    #[test]
    fn a_clip_plays_at_its_own_speed_times_the_speed_of_its_track_and_its_tiles_show_that() {
        let geometry = geometry_at(30, 1, 100.0);
        // 600 source frames at 3x on a track that plays at 2x: 100 timeline frames, six source frames each.
        let track = Track::new(TrackId::new(0), "track 0")
            .with_clips([(0, clip(0, 600, 3))])
            .with_speed(SpeedFactor::new(2).unwrap());
        let tiles = geometry.visible_tiles(&track, a_second_wide(&geometry), 0.0, 10_000.0);
        let source_frames: Vec<u64> = tiles.iter().map(|tile| tile.source_frame.value()).collect();
        assert_eq!(
            source_frames,
            vec![0, 180, 360, 540],
            "a tile every second of the finished video, 180 source frames apart"
        );
    }

    // --- tiles of other widths ---

    fn pictures_of(tiles: &[TileLayout]) -> Vec<(u8, usize)> {
        tiles
            .iter()
            .map(|tile| (tile.picture.level, tile.picture.index))
            .collect()
    }

    fn source_frames_of(tiles: &[TileLayout]) -> Vec<u64> {
        tiles.iter().map(|tile| tile.source_frame.value()).collect()
    }

    /// Ten seconds of video at 30 frames a second on one track.
    fn ten_seconds() -> Track {
        track_of(vec![clip(0, 300, 1)])
    }

    #[test]
    fn a_tile_that_is_as_wide_as_a_second_has_the_picture_of_a_second_and_one_that_is_half_as_wide_has_two_a_second() {
        let geometry = geometry_at(30, 1, 100.0);
        let tiles = geometry.visible_tiles(&ten_seconds(), |_| 50.0, 0.0, 200.0);
        // Half a second apart: 0, 0.5 s, 1 s, 1.5 s, which are frames 0, 15, 30, 45.
        assert_eq!(
            tiles.iter().map(|tile| tile.left).collect::<Vec<_>>(),
            vec![0.0, 50.0, 100.0, 150.0]
        );
        assert_eq!(source_frames_of(&tiles), vec![0, 15, 30, 45]);
        assert_eq!(
            pictures_of(&tiles),
            vec![(1, 0), (1, 1), (1, 2), (1, 3)],
            "two a second"
        );

        let tiles = geometry.visible_tiles(&ten_seconds(), |_| 25.0, 0.0, 100.0);
        assert_eq!(
            source_frames_of(&tiles),
            vec![0, 7, 15, 22],
            "a quarter of a second is 7.5 frames"
        );
        assert_eq!(
            pictures_of(&tiles),
            vec![(2, 0), (2, 1), (2, 2), (2, 3)],
            "four a second"
        );

        let tiles = geometry.visible_tiles(&ten_seconds(), |_| 100.0, 0.0, 200.0);
        assert_eq!(
            pictures_of(&tiles),
            vec![(0, 0), (0, 1)],
            "a second apart: one a second"
        );
    }

    #[test]
    fn zooming_in_makes_the_same_tiles_stand_for_less_time_and_zooming_out_for_more() {
        let tile_width = 100.0;
        // 400 points a second: a tile is a quarter of a second.
        let zoomed_in = geometry_at(30, 1, 400.0).visible_tiles(&ten_seconds(), |_| tile_width, 0.0, 300.0);
        assert_eq!(pictures_of(&zoomed_in), vec![(2, 0), (2, 1), (2, 2)]);
        assert_eq!(source_frames_of(&zoomed_in), vec![0, 7, 15]);
        // 10 points a second: a tile is ten seconds, so the pictures of whole seconds are far enough apart.
        let zoomed_out = geometry_at(30, 1, 10.0).visible_tiles(&ten_seconds(), |_| tile_width, 0.0, 300.0);
        assert_eq!(pictures_of(&zoomed_out), vec![(0, 0)]);
        let long = track_of(vec![clip(0, 9000, 1)]); // five minutes
        let zoomed_out = geometry_at(30, 1, 10.0).visible_tiles(&long, |_| tile_width, 0.0, 300.0);
        assert_eq!(
            pictures_of(&zoomed_out),
            vec![(0, 0), (0, 10), (0, 20)],
            "ten seconds apart"
        );
    }

    #[test]
    fn a_clip_that_plays_faster_goes_through_more_of_the_video_between_tiles_so_needs_fewer_pictures() {
        let geometry = geometry_at(30, 1, 100.0);
        // Half a second apart on the timeline: at 1x that is half a second of the video, at 4x two seconds.
        let faster = track_of(vec![clip(0, 1200, 4)]);
        let tiles = geometry.visible_tiles(&faster, |_| 50.0, 0.0, 150.0);
        assert_eq!(
            pictures_of(&tiles),
            vec![(0, 0), (0, 2), (0, 4)],
            "the pictures of a second do"
        );
        assert_eq!(source_frames_of(&tiles), vec![0, 60, 120]);
        // A tenth as wide: a twentieth of a second apart is 8 frames of the video: finer than one a second needs.
        let tiles = geometry.visible_tiles(&faster, |_| 12.5, 0.0, 30.0);
        assert_eq!(
            pictures_of(&tiles)[0],
            (1, 0),
            "an eighth of a second on the timeline is half a second of video"
        );
    }

    #[test]
    fn the_pictures_are_never_finer_than_one_for_every_frame() {
        // 1200 points a second and tiles 4 points wide: a tile is a 300th of a second.
        let tiles = geometry_at(30, 1, 1200.0).visible_tiles(&ten_seconds(), |_| 4.0, 0.0, 40.0);
        assert!(
            tiles.iter().all(|tile| tile.picture.level == 4),
            "{:?}",
            pictures_of(&tiles)
        );
        assert_eq!(source_frames_of(&tiles).len(), 10);
        assert!(
            source_frames_of(&tiles).iter().all(|frame| *frame <= 1),
            "ten tiles in the first frame or two"
        );
    }

    #[test]
    fn the_picture_of_a_tile_is_the_nearest_one_to_the_frame_it_stands_for() {
        // A clip that starts 90 frames into the video: tile pictures count from the video, not the clip.
        let geometry = geometry_at(30, 1, 100.0);
        let track = track_of(vec![clip(90, 300, 1)]);
        let tiles = geometry.visible_tiles(&track, |_| 50.0, 0.0, 100.0);
        assert_eq!(source_frames_of(&tiles), vec![90, 105]);
        assert_eq!(
            pictures_of(&tiles),
            vec![(1, 6), (1, 7)],
            "3 s and 3.5 s of the video at two a second"
        );
    }

    #[test]
    fn each_video_has_the_tile_width_that_is_given_for_it() {
        let geometry = geometry_at(30, 1, 100.0);
        let other_video = MediaClip::new(
            SourceMediaIdentifier::new(2),
            SourceFrameIndex::new(0),
            SourceFrameIndex::new(60),
            SpeedFactor::NORMAL,
        )
        .unwrap();
        let track =
            Track::new(TrackId::new(0), "track 0").with_clips([(0, clip(0, 60, 1)), (0, Clip::Video(other_video))]);
        let width_of = |source: SourceMediaIdentifier| {
            if source == SourceMediaIdentifier::new(2) {
                40.0
            } else {
                100.0
            }
        };

        let tiles = geometry.visible_tiles(&track, width_of, 0.0, 1000.0);

        let widths_of_clip = |clip: u32| -> Vec<f64> {
            tiles
                .iter()
                .filter(|tile| tile.clip == clip_id(clip))
                .map(|tile| tile.width)
                .collect()
        };
        assert_eq!(widths_of_clip(0), vec![100.0, 100.0]);
        assert_eq!(widths_of_clip(1), vec![40.0, 40.0, 40.0, 40.0, 40.0]);
        assert!(
            tiles
                .iter()
                .filter(|tile| tile.clip == clip_id(1))
                .all(|tile| tile.source == SourceMediaIdentifier::new(2))
        );
    }

    #[test]
    fn a_width_that_is_not_a_sensible_number_gives_no_tiles_instead_of_a_flood() {
        let geometry = geometry_at(30, 1, 100.0);
        for width in [f64::NAN, 0.0, -50.0, 0.5, 3.9, f64::NEG_INFINITY] {
            let tiles = geometry.visible_tiles(&ten_seconds(), |_| width, 0.0, 10_000.0);
            assert!(tiles.is_empty(), "{width}");
        }
        let tiles = geometry.visible_tiles(&ten_seconds(), |_| f64::INFINITY, 0.0, 10_000.0);
        assert!(
            tiles.len() <= 1,
            "one tile at the most, as wide as the clip is: {}",
            tiles.len()
        );
        assert!(
            !geometry.visible_tiles(&ten_seconds(), |_| 4.0, 0.0, 100.0).is_empty(),
            "4 points is the least"
        );
    }

    #[test]
    fn tiles_of_any_width_fill_a_clip_exactly_and_each_stands_for_a_frame_of_the_clip_in_order() {
        use crate::test_support::DeterministicRandomNumbers;
        let mut random = DeterministicRandomNumbers::with_seed(9);
        for round in 0..200 {
            let points_per_second = 2.0 + random.next_below(1198) as f64;
            let tile_width = 4.0 + random.next_below(300) as f64;
            let speed = 1 + random.next_below(6) as u32;
            let first = random.next_below(500);
            let length = 1 + random.next_below(900);
            let geometry = geometry_at(30, 1, points_per_second);
            let track = track_of(vec![clip(first, first + length * u64::from(speed), speed)]);
            let tiles = geometry.visible_tiles(&track, |_| tile_width, 0.0, 1.0e6);
            let context =
                format!("round {round}: {points_per_second} pps, {tile_width} wide, {speed}x, {length} frames");

            let placement = track.placement_at_index(0).unwrap();
            let mut expected_left = geometry.x_of_frame(placement.first_timeline_frame.value());
            for tile in &tiles {
                assert_close(tile.left, expected_left);
                expected_left += tile.width;
            }
            assert_close(expected_left, geometry.x_of_frame(placement.end_timeline_frame.value()));
            assert!(
                tiles
                    .windows(2)
                    .all(|pair| pair[0].source_frame <= pair[1].source_frame),
                "{context}"
            );
            assert!(tiles.iter().all(|tile| tile.source_frame.value() >= first), "{context}");
            assert!(
                tiles
                    .iter()
                    .all(|tile| tile.source_frame.value() < first + length * u64::from(speed)),
                "{context}"
            );
            let level = tiles[0].picture.level;
            assert!(
                tiles.iter().all(|tile| tile.picture.level == level),
                "{context}: one level for a clip"
            );
            assert!(
                level == 4
                    || ThumbnailKey::pictures_per_second(level) * tile_width / points_per_second * f64::from(speed)
                        >= 1.0,
                "{context}: level {level} is at least as fine as the spacing"
            );
        }
    }

    // --- how wide a tile is ---

    #[test]
    fn a_tile_is_as_wide_as_a_picture_of_the_video_is_at_the_height_of_the_row() {
        assert_close(tile_width_for(104.0, 1920, 1080), 104.0 * 16.0 / 9.0);
        assert_close(tile_width_for(52.0, 1920, 1080), 52.0 * 16.0 / 9.0);
        assert_close(tile_width_for(104.0, 1000, 1000), 104.0);
        assert_close(tile_width_for(30.0, 1280, 720), 30.0 * 16.0 / 9.0);
        assert_close(tile_width_for(104.0, 1440, 1080), 104.0 * 4.0 / 3.0);
    }

    #[test]
    fn a_video_that_is_very_tall_or_very_wide_has_tiles_of_a_sensible_shape() {
        assert_close(tile_width_for(100.0, 1080, 1920), 56.25);
        assert_close(tile_width_for(100.0, 100, 1000), 45.0);
        assert_close(tile_width_for(100.0, 5000, 500), 450.0);
        assert_close(tile_width_for(100.0, 0, 0), 100.0);
        assert_close(tile_width_for(0.0, 1920, 1080), 0.0);
    }
}
