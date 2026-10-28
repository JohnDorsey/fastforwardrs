// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A stretch of a source, played at a speed: what a clip of a video, of a sound file and of a MIDI file have in common (and what their arithmetic is, once, so that they cannot differ).

use thiserror::Error;

use crate::project::{FrameRate, SourceFrameIndex, SpeedFactor};

/// Returned when a clip would contain no frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error(
    "a clip must contain at least one frame, but it was given {first_source_frame} up to (not including) {end_source_frame}"
)]
pub struct EmptyClip {
    pub first_source_frame: SourceFrameIndex,
    pub end_source_frame: SourceFrameIndex,
}

/// A run of whole frames of a source, played at a single speed.
///
/// It covers source frames `first_source_frame` up to but not including `end_source_frame`. (For a sound file, or for the music of a MIDI file, a frame is a stretch as long as a frame of
/// the project's video.) At a speed of `N` it shows every `N`th frame of that run, starting with the first, so it occupies `ceil(source frames / N)` frames of the finished video. This is
/// exactly the set of frames the export produces, which is what makes the live preview trustworthy.
///
/// The speed that counts is the clip's own speed times the speed of the track it is on, so the arithmetic that depends on it takes that speed (the *effective* speed) as an argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    first_source_frame: SourceFrameIndex,
    end_source_frame: SourceFrameIndex,
    speed: SpeedFactor,
}

impl SourceSpan {
    /// `first_source_frame` up to `end_source_frame`, at `speed`.
    pub fn new(
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
            first_source_frame,
            end_source_frame,
            speed,
        })
    }

    pub fn first_source_frame(&self) -> SourceFrameIndex {
        self.first_source_frame
    }

    /// The frame just after the last frame (the end is exclusive).
    pub fn end_source_frame(&self) -> SourceFrameIndex {
        self.end_source_frame
    }

    /// How much faster than normal it plays itself, not counting its track's speed.
    pub fn speed(&self) -> SpeedFactor {
        self.speed
    }

    #[must_use]
    pub fn with_speed(&self, speed: SpeedFactor) -> Self {
        Self { speed, ..*self }
    }

    /// The span as it is when its project runs at the frame rate `to` and not `from`: it covers the same stretch of the source, with its first and its end frame counted again at the new
    /// rate (and at least one frame of it). Pieces cut from one source that touch each other still do.
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Self {
        let first_source_frame = from.convert_frame_count_to(self.first_source_frame.value(), to);
        let end_source_frame = from
            .convert_frame_count_to(self.end_source_frame.value(), to)
            .max(first_source_frame + 1);
        Self {
            first_source_frame: SourceFrameIndex::new(first_source_frame),
            end_source_frame: SourceFrameIndex::new(end_source_frame),
            ..*self
        }
    }

    /// How many source frames it covers.
    pub fn source_frame_count(&self) -> u64 {
        self.end_source_frame.value() - self.first_source_frame.value()
    }

    /// How many frames it occupies on the timeline when it plays at `effective_speed`: every `effective_speed`th source frame.
    pub fn timeline_frame_count(&self, effective_speed: SpeedFactor) -> u64 {
        self.source_frame_count().div_ceil(u64::from(effective_speed.value()))
    }

    /// The source frame shown by the clip's frame number `offset_within_clip` (0 is the clip's first frame), or `None` if the clip is shorter than that.
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

    /// The clip's frame number (0 is its first) that is on screen while `source_frame` of the source is being played, or `None` if the source frame is not part of this clip. For a
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

    /// Cuts the span in two just before `split_source_frame`. `None` if the split point is not strictly inside it.
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
