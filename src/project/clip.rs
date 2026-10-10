//! One piece of the video track: a run of frames from a source video, played at one speed.

use thiserror::Error;

use super::{SourceFrameIndex, SourceVideoIdentifier, SpeedFactor};

/// A run of whole frames from a source video, played at a single speed.
///
/// A clip covers source frames `first_source_frame` up to but not including `end_source_frame`.
/// At speed factor `N` it shows every `N`th frame of that run, starting with the first, so it
/// occupies `ceil(source frames / N)` frames of the finished video. This is exactly the set of
/// frames the export produces, which is what makes the live preview trustworthy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    source_video_identifier: SourceVideoIdentifier,
    first_source_frame: SourceFrameIndex,
    end_source_frame: SourceFrameIndex,
    speed_factor: SpeedFactor,
}

/// Returned when a clip would contain no frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error(
    "a clip must contain at least one frame, but it was given {first_source_frame} up to (not including) {end_source_frame}"
)]
pub struct EmptyClip {
    pub first_source_frame: SourceFrameIndex,
    pub end_source_frame: SourceFrameIndex,
}

impl Clip {
    pub fn new(
        source_video_identifier: SourceVideoIdentifier,
        first_source_frame: SourceFrameIndex,
        end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
    ) -> Result<Self, EmptyClip> {
        if first_source_frame >= end_source_frame {
            return Err(EmptyClip {
                first_source_frame,
                end_source_frame,
            });
        }
        Ok(Self {
            source_video_identifier,
            first_source_frame,
            end_source_frame,
            speed_factor,
        })
    }

    pub fn source_video_identifier(&self) -> SourceVideoIdentifier {
        self.source_video_identifier
    }

    pub fn first_source_frame(&self) -> SourceFrameIndex {
        self.first_source_frame
    }

    /// The frame just after the clip's last frame (the end is exclusive).
    pub fn end_source_frame(&self) -> SourceFrameIndex {
        self.end_source_frame
    }

    pub fn speed_factor(&self) -> SpeedFactor {
        self.speed_factor
    }

    /// How many source frames the clip covers.
    pub fn source_frame_count(&self) -> u64 {
        self.end_source_frame.value() - self.first_source_frame.value()
    }

    /// How many frames the clip occupies on the timeline: every `speed_factor`th source frame.
    pub fn timeline_frame_count(&self) -> u64 {
        self.source_frame_count().div_ceil(u64::from(self.speed_factor.value()))
    }

    /// The source frame shown by the clip's frame number `offset_within_clip` (0 is the clip's
    /// first frame), or `None` if the clip is shorter than that.
    pub fn source_frame_at_timeline_offset(&self, offset_within_clip: u64) -> Option<SourceFrameIndex> {
        if offset_within_clip >= self.timeline_frame_count() {
            return None;
        }
        let skipped_source_frames = offset_within_clip * u64::from(self.speed_factor.value());
        Some(SourceFrameIndex::new(
            self.first_source_frame.value() + skipped_source_frames,
        ))
    }

    /// The clip's frame number (0 is its first) that is on screen while `source_frame` of the
    /// source video is being played, or `None` if the source frame is not part of this clip.
    /// For a sped-up clip several source frames share one timeline frame.
    pub fn timeline_offset_of_source_frame(&self, source_frame: SourceFrameIndex) -> Option<u64> {
        if source_frame < self.first_source_frame || source_frame >= self.end_source_frame {
            return None;
        }
        let source_frames_in = source_frame.value() - self.first_source_frame.value();
        Some(source_frames_in / u64::from(self.speed_factor.value()))
    }

    /// The same clip played at a different speed.
    pub fn with_speed_factor(&self, speed_factor: SpeedFactor) -> Self {
        Self { speed_factor, ..*self }
    }

    /// Cuts the clip in two just before `split_source_frame`; both halves keep the clip's speed.
    /// Returns `None` if the split point is not strictly inside the clip.
    pub fn split_before_source_frame(&self, split_source_frame: SourceFrameIndex) -> Option<(Self, Self)> {
        if split_source_frame <= self.first_source_frame || split_source_frame >= self.end_source_frame {
            return None;
        }
        let first_half = Self {
            end_source_frame: split_source_frame,
            ..*self
        };
        let second_half = Self {
            first_source_frame: split_source_frame,
            ..*self
        };
        Some((first_half, second_half))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(first: u64, end: u64, speed: u32) -> Clip {
        Clip::new(
            SourceVideoIdentifier::new(1),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::new(speed).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn a_clip_must_contain_at_least_one_frame() {
        let video = SourceVideoIdentifier::new(1);
        let frame = SourceFrameIndex::new(10);
        assert!(Clip::new(video, frame, frame, SpeedFactor::NORMAL).is_err());
        assert!(Clip::new(video, SourceFrameIndex::new(11), frame, SpeedFactor::NORMAL).is_err());
        assert!(Clip::new(video, frame, SourceFrameIndex::new(11), SpeedFactor::NORMAL).is_ok());
    }

    #[test]
    fn at_normal_speed_the_timeline_length_equals_the_source_length() {
        let clip = clip(100, 400, 1);
        assert_eq!(clip.source_frame_count(), 300);
        assert_eq!(clip.timeline_frame_count(), 300);
    }

    #[test]
    fn a_sped_up_clip_keeps_every_nth_frame_and_rounds_a_partial_group_up() {
        assert_eq!(clip(0, 300, 4).timeline_frame_count(), 75);
        // 10 frames at 3x keep frames 0, 3, 6 and 9: four frames, the last one a partial group.
        assert_eq!(clip(0, 10, 3).timeline_frame_count(), 4);
        // A clip shorter than the speed factor still shows its first frame.
        assert_eq!(clip(0, 3, 100).timeline_frame_count(), 1);
    }

    #[test]
    fn timeline_frames_map_to_every_nth_source_frame_starting_with_the_first() {
        let clip = clip(100, 110, 3);
        let shown: Vec<u64> = (0..5)
            .filter_map(|offset| clip.source_frame_at_timeline_offset(offset))
            .map(SourceFrameIndex::value)
            .collect();
        assert_eq!(shown, vec![100, 103, 106, 109]);
        assert_eq!(clip.source_frame_at_timeline_offset(4), None);
    }

    #[test]
    fn source_frames_map_back_to_the_timeline_frame_they_are_shown_in() {
        let clip = clip(100, 110, 3);
        let offsets: Vec<Option<u64>> = [99, 100, 101, 102, 103, 109, 110]
            .into_iter()
            .map(|frame| clip.timeline_offset_of_source_frame(SourceFrameIndex::new(frame)))
            .collect();
        assert_eq!(offsets, vec![None, Some(0), Some(0), Some(0), Some(1), Some(3), None]);
    }

    #[test]
    fn every_timeline_frame_round_trips_through_its_source_frame() {
        for speed in [1, 2, 3, 4, 7, 16, 100] {
            let clip = clip(37, 1000, speed);
            for offset in 0..clip.timeline_frame_count() {
                let source_frame = clip.source_frame_at_timeline_offset(offset).unwrap();
                assert_eq!(clip.timeline_offset_of_source_frame(source_frame), Some(offset));
            }
        }
    }

    #[test]
    fn splitting_cuts_the_source_range_in_two_and_keeps_the_speed() {
        let (first_half, second_half) = clip(100, 400, 4)
            .split_before_source_frame(SourceFrameIndex::new(240))
            .unwrap();
        assert_eq!(first_half, clip(100, 240, 4));
        assert_eq!(second_half, clip(240, 400, 4));
    }

    #[test]
    fn splitting_at_or_beyond_the_ends_is_refused() {
        let clip = clip(100, 400, 1);
        for frame in [0, 99, 100, 400, 401] {
            assert_eq!(clip.split_before_source_frame(SourceFrameIndex::new(frame)), None);
        }
        assert!(clip.split_before_source_frame(SourceFrameIndex::new(101)).is_some());
        assert!(clip.split_before_source_frame(SourceFrameIndex::new(399)).is_some());
    }

    #[test]
    fn changing_the_speed_changes_only_the_speed() {
        let original = clip(100, 400, 1);
        let faster = original.with_speed_factor(SpeedFactor::new(5).unwrap());
        assert_eq!(faster, clip(100, 400, 5));
        assert_eq!(original, clip(100, 400, 1));
    }
}
