// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! An adjustment clip (phase 5, GR-09): a clip with no picture of its own, whose effects are done to **the picture that everything behind it makes**, for as long as it lasts.
//!
//! It has no media and no place on the picture (it is the whole of it), a length that is set like a picture's (5 seconds when it is made: Q-11), and the chain of effects that every
//! clip has, which is all it is for. Like a picture it lasts as long as it says whatever the speed of its track. What is behind it is what is on the tracks behind its own, in its
//! own group if it is in one (see `docs/engine-phase-5-design.md`, H7).

use thiserror::Error;

use crate::project::FrameRate;

/// Returned when an adjustment clip would last no time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("an adjustment clip must last at least one frame")]
pub struct EmptyAdjustmentClip;

/// An adjustment clip on a track, and for how long it lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdjustmentClip {
    duration_in_frames: u64,
}

impl AdjustmentClip {
    pub fn new(duration_in_frames: u64) -> Result<Self, EmptyAdjustmentClip> {
        if duration_in_frames == 0 {
            return Err(EmptyAdjustmentClip);
        }
        Ok(Self { duration_in_frames })
    }

    /// How many frames of the timeline the clip lasts.
    pub fn duration_in_frames(&self) -> u64 {
        self.duration_in_frames
    }

    /// The same clip lasting `duration_in_frames` frames, or `None` for no time at all.
    #[must_use]
    pub fn with_duration_in_frames(&self, duration_in_frames: u64) -> Option<Self> {
        Self::new(duration_in_frames).ok()
    }

    /// The clip as it is when its project runs at the frame rate `to` and not `from`: it lasts as long as it did, counted again at the new rate (for at least one frame).
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Self {
        Self {
            duration_in_frames: from.convert_frame_count_to(self.duration_in_frames, to).max(1),
        }
    }

    /// Cuts the clip in two after its first `frames_in` frames, or `None` if that is not strictly inside it.
    pub fn split_after(&self, frames_in: u64) -> Option<(Self, Self)> {
        if frames_in == 0 || frames_in >= self.duration_in_frames {
            return None;
        }
        Some((
            Self {
                duration_in_frames: frames_in,
            },
            Self {
                duration_in_frames: self.duration_in_frames - frames_in,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(frames_per_second: u32) -> FrameRate {
        FrameRate::from_whole_frames_per_second(frames_per_second).unwrap()
    }

    #[test]
    fn an_adjustment_clip_lasts_at_least_a_frame() {
        assert_eq!(AdjustmentClip::new(0), Err(EmptyAdjustmentClip));
        assert_eq!(AdjustmentClip::new(1).unwrap().duration_in_frames(), 1);
        assert_eq!(AdjustmentClip::new(150).unwrap().duration_in_frames(), 150);
        assert_eq!(
            AdjustmentClip::new(9)
                .unwrap()
                .with_duration_in_frames(4)
                .map(|clip| clip.duration_in_frames()),
            Some(4)
        );
        assert_eq!(AdjustmentClip::new(9).unwrap().with_duration_in_frames(0), None);
    }

    #[test]
    fn an_adjustment_clip_counted_again_at_another_frame_rate_lasts_as_long_in_time_and_a_frame_at_the_least() {
        let clip = AdjustmentClip::new(31).unwrap();
        assert_eq!(clip.retimed(rate(30), rate(60)).duration_in_frames(), 62);
        assert_eq!(
            clip.retimed(rate(30), rate(15)).duration_in_frames(),
            16,
            "half a frame is rounded up"
        );
        assert_eq!(clip.retimed(rate(30), rate(30)), clip);
        assert_eq!(
            AdjustmentClip::new(1)
                .unwrap()
                .retimed(rate(30), rate(12))
                .duration_in_frames(),
            1
        );
    }

    #[test]
    fn an_adjustment_clip_is_cut_into_two_that_last_as_long_as_it_did_and_not_at_its_ends() {
        let clip = AdjustmentClip::new(10).unwrap();
        let (first, second) = clip.split_after(4).unwrap();
        assert_eq!((first.duration_in_frames(), second.duration_in_frames()), (4, 6));
        assert_eq!(clip.split_after(0), None);
        assert_eq!(clip.split_after(10), None);
        assert_eq!(clip.split_after(99), None);
    }
}
