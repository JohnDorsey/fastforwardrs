//! A clip made from a stretch of a video or of a sound file.

use thiserror::Error;

use super::gain::Gain;
use crate::project::{SourceFrameIndex, SourceMediaIdentifier, SpeedFactor};

/// A run of whole frames from a source file, played at a single speed.
///
/// The clip covers source frames `first_source_frame` up to but not including
/// `end_source_frame`. (For a sound file a frame is a stretch of the sound as long as a frame of
/// the project's video.) At a speed of `N` it shows every `N`th frame of that run, starting with
/// the first, so it occupies `ceil(source frames / N)` frames of the finished video. This is
/// exactly the set of frames the export produces, which is what makes the live preview
/// trustworthy.
///
/// The speed that counts is the clip's own speed times the speed of the track it is on, so the
/// arithmetic that depends on it takes that speed (the *effective* speed) as an argument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaClip {
    source: SourceMediaIdentifier,
    first_source_frame: SourceFrameIndex,
    end_source_frame: SourceFrameIndex,
    speed: SpeedFactor,
    gain: Gain,
    muted: bool,
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

impl MediaClip {
    /// A clip of `first_source_frame` up to `end_source_frame` of the file `source`, at `speed`,
    /// at the loudness it was recorded at.
    pub fn new(
        source: SourceMediaIdentifier,
        first_source_frame: SourceFrameIndex,
        end_source_frame: SourceFrameIndex,
        speed: SpeedFactor,
    ) -> Result<Self, EmptyClip> {
        if first_source_frame >= end_source_frame {
            return Err(EmptyClip {
                first_source_frame,
                end_source_frame,
            });
        }
        Ok(Self {
            source,
            first_source_frame,
            end_source_frame,
            speed,
            gain: Gain::UNCHANGED,
            muted: false,
        })
    }

    pub fn source(&self) -> SourceMediaIdentifier {
        self.source
    }

    pub fn first_source_frame(&self) -> SourceFrameIndex {
        self.first_source_frame
    }

    /// The frame just after the clip's last frame (the end is exclusive).
    pub fn end_source_frame(&self) -> SourceFrameIndex {
        self.end_source_frame
    }

    /// How much faster than normal the clip itself plays, not counting its track's speed.
    pub fn speed(&self) -> SpeedFactor {
        self.speed
    }

    pub fn gain(&self) -> Gain {
        self.gain
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    #[must_use]
    pub fn with_speed(&self, speed: SpeedFactor) -> Self {
        Self { speed, ..*self }
    }

    #[must_use]
    pub fn with_gain(&self, gain: Gain) -> Self {
        Self { gain, ..*self }
    }

    #[must_use]
    pub fn with_muted(&self, muted: bool) -> Self {
        Self { muted, ..*self }
    }

    /// How many source frames the clip covers.
    pub fn source_frame_count(&self) -> u64 {
        self.end_source_frame.value() - self.first_source_frame.value()
    }

    /// How many frames the clip occupies on the timeline when it plays at `effective_speed`: every
    /// `effective_speed`th source frame.
    pub fn timeline_frame_count(&self, effective_speed: SpeedFactor) -> u64 {
        self.source_frame_count().div_ceil(u64::from(effective_speed.value()))
    }

    /// The source frame shown by the clip's frame number `offset_within_clip` (0 is the clip's
    /// first frame), or `None` if the clip is shorter than that.
    pub fn source_frame_at_timeline_offset(
        &self,
        effective_speed: SpeedFactor,
        offset_within_clip: u64,
    ) -> Option<SourceFrameIndex> {
        if offset_within_clip >= self.timeline_frame_count(effective_speed) {
            return None;
        }
        let skipped_source_frames = offset_within_clip * u64::from(effective_speed.value());
        Some(SourceFrameIndex::new(
            self.first_source_frame.value() + skipped_source_frames,
        ))
    }

    /// The clip's frame number (0 is its first) that is on screen while `source_frame` of the
    /// source is being played, or `None` if the source frame is not part of this clip. For a
    /// sped-up clip several source frames share one timeline frame.
    pub fn timeline_offset_of_source_frame(
        &self,
        effective_speed: SpeedFactor,
        source_frame: SourceFrameIndex,
    ) -> Option<u64> {
        if source_frame < self.first_source_frame || source_frame >= self.end_source_frame {
            return None;
        }
        let source_frames_in = source_frame.value() - self.first_source_frame.value();
        Some(source_frames_in / u64::from(effective_speed.value()))
    }

    /// Cuts the clip in two just before `split_source_frame`; both halves keep everything else
    /// about the clip. Returns `None` if the split point is not strictly inside the clip.
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

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn clip(first: u64, end: u64, factor: u32) -> MediaClip {
        MediaClip::new(
            SourceMediaIdentifier::new(1),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            speed(factor),
        )
        .unwrap()
    }

    #[test]
    fn a_clip_must_contain_at_least_one_frame() {
        let source = SourceMediaIdentifier::new(1);
        let frame = SourceFrameIndex::new(10);
        assert!(MediaClip::new(source, frame, frame, SpeedFactor::NORMAL).is_err());
        assert!(MediaClip::new(source, SourceFrameIndex::new(11), frame, SpeedFactor::NORMAL).is_err());
        assert!(MediaClip::new(source, frame, SourceFrameIndex::new(11), SpeedFactor::NORMAL).is_ok());
    }

    #[test]
    fn a_new_clip_is_at_the_loudness_it_was_recorded_at_and_not_muted() {
        let clip = clip(0, 10, 1);
        assert_eq!(clip.gain(), Gain::UNCHANGED);
        assert!(!clip.is_muted());
    }

    #[test]
    fn at_normal_speed_the_timeline_length_equals_the_source_length() {
        let clip = clip(100, 400, 1);
        assert_eq!(clip.source_frame_count(), 300);
        assert_eq!(clip.timeline_frame_count(SpeedFactor::NORMAL), 300);
    }

    #[test]
    fn a_sped_up_clip_keeps_every_nth_frame_and_rounds_a_partial_group_up() {
        assert_eq!(clip(0, 300, 1).timeline_frame_count(speed(4)), 75);
        // 10 frames at 3x keep frames 0, 3, 6 and 9: four frames, the last one a partial group.
        assert_eq!(clip(0, 10, 1).timeline_frame_count(speed(3)), 4);
        // A clip shorter than the speed factor still shows its first frame.
        assert_eq!(clip(0, 3, 1).timeline_frame_count(speed(100)), 1);
    }

    #[test]
    fn it_is_the_effective_speed_that_counts_and_not_the_speed_the_clip_was_made_with() {
        let clip = clip(0, 120, 3);
        assert_eq!(clip.speed(), speed(3));
        // On a track that plays at 2x, the clip's 3x makes 6x: that is what the caller passes in.
        assert_eq!(clip.timeline_frame_count(speed(6)), 20);
        assert_eq!(clip.timeline_frame_count(speed(3)), 40);
    }

    #[test]
    fn timeline_frames_map_to_every_nth_source_frame_starting_with_the_first() {
        let clip = clip(100, 110, 1);
        let shown: Vec<u64> = (0..5)
            .filter_map(|offset| clip.source_frame_at_timeline_offset(speed(3), offset))
            .map(SourceFrameIndex::value)
            .collect();
        assert_eq!(shown, vec![100, 103, 106, 109]);
        assert_eq!(clip.source_frame_at_timeline_offset(speed(3), 4), None);
    }

    #[test]
    fn source_frames_map_back_to_the_timeline_frame_they_are_shown_in() {
        let clip = clip(100, 110, 1);
        let offsets: Vec<Option<u64>> = [99, 100, 101, 102, 103, 109, 110]
            .into_iter()
            .map(|frame| clip.timeline_offset_of_source_frame(speed(3), SourceFrameIndex::new(frame)))
            .collect();
        assert_eq!(offsets, vec![None, Some(0), Some(0), Some(0), Some(1), Some(3), None]);
    }

    #[test]
    fn every_timeline_frame_round_trips_through_its_source_frame() {
        for factor in [1, 2, 3, 4, 7, 16, 100] {
            let clip = clip(37, 1000, 1);
            for offset in 0..clip.timeline_frame_count(speed(factor)) {
                let source_frame = clip.source_frame_at_timeline_offset(speed(factor), offset).unwrap();
                assert_eq!(
                    clip.timeline_offset_of_source_frame(speed(factor), source_frame),
                    Some(offset)
                );
            }
        }
    }

    #[test]
    fn splitting_cuts_the_source_range_in_two_and_keeps_everything_else() {
        let original = clip(100, 400, 4).with_gain(Gain::from_decibels(-6.0)).with_muted(true);
        let (first_half, second_half) = original.split_before_source_frame(SourceFrameIndex::new(240)).unwrap();
        assert_eq!(
            first_half,
            clip(100, 240, 4).with_gain(Gain::from_decibels(-6.0)).with_muted(true)
        );
        assert_eq!(
            second_half,
            clip(240, 400, 4).with_gain(Gain::from_decibels(-6.0)).with_muted(true)
        );
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
    fn changing_the_speed_the_gain_or_the_muting_changes_only_that() {
        let original = clip(100, 400, 1);
        assert_eq!(original.with_speed(speed(5)), clip(100, 400, 5));
        assert_eq!(original.with_gain(Gain::from_decibels(3.0)).speed(), speed(1));
        assert_eq!(
            original.with_gain(Gain::from_decibels(3.0)).gain(),
            Gain::from_decibels(3.0)
        );
        assert!(original.with_muted(true).is_muted());
        assert_eq!(original.with_muted(true).with_muted(false), original);
        assert_eq!(original, clip(100, 400, 1), "the original is untouched");
    }
}
