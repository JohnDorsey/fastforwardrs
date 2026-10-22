// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A generator clip (phase 6, EF-02, Q-11): a clip with no media whose picture is made by a shader (an ISF file that reads no picture) and not taken from a file. **What it makes is
//! its first effect**, the *content*: an effect of the chain that every clip has, of a kind that takes no picture, with values that are properties like those of any effect (so
//! they have keyframes, controls and a place in the file with no more code). The rest of the chain is done to what the content makes, as it is done to the picture of a video.
//!
//! The content cannot be taken off, switched off, copied or moved (a generator that makes nothing is not one: see `Project::the_clip_whose_content_is`). Like a picture the clip lasts as
//! long as it says whatever the speed of its track (5 seconds when it is made: Q-11), and like a text object it is somewhere on the picture: the whole of it until it is put
//! somewhere.

use thiserror::Error;

use super::placement::{CanvasArea, Placement};
use crate::project::{EffectInstanceId, FrameRate};

/// Returned when a generator clip would last no time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a generator clip must last at least one frame")]
pub struct EmptyGeneratorClip;

/// A generator clip on a track: for how long, which effect of its chain is what it makes, and where on the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorClip {
    duration_in_frames: u64,
    content: EffectInstanceId,
    placement: Placement,
}

impl GeneratorClip {
    /// A generator that lasts `duration_in_frames` frames, whose content is the effect named `content` in the chain of the clip, and that is the whole picture.
    pub fn new(duration_in_frames: u64, content: EffectInstanceId) -> Result<Self, EmptyGeneratorClip> {
        if duration_in_frames == 0 {
            return Err(EmptyGeneratorClip);
        }
        Ok(Self {
            duration_in_frames,
            content,
            placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
        })
    }

    /// How many frames of the timeline the clip lasts.
    pub fn duration_in_frames(&self) -> u64 {
        self.duration_in_frames
    }

    /// Which effect of the chain of the clip is what it makes.
    pub fn content(&self) -> EffectInstanceId {
        self.content
    }

    /// Where the clip is on the picture, and how far it is turned.
    pub fn placement(&self) -> Placement {
        self.placement
    }

    #[must_use]
    pub fn with_placement(&self, placement: Placement) -> Self {
        Self { placement, ..*self }
    }

    /// The same clip lasting `duration_in_frames` frames, or `None` for no time at all.
    #[must_use]
    pub fn with_duration_in_frames(&self, duration_in_frames: u64) -> Option<Self> {
        (duration_in_frames > 0).then_some(Self {
            duration_in_frames,
            ..*self
        })
    }

    /// The clip as it is when its project runs at the frame rate `to` and not `from`: it lasts as long as it did, counted again at the new rate (for at least one frame).
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Self {
        Self {
            duration_in_frames: from.convert_frame_count_to(self.duration_in_frames, to).max(1),
            ..*self
        }
    }

    /// Cuts the clip in two after its first `frames_in` frames, or `None` if that is not strictly inside it. Both halves have the same content and are in the same place.
    pub fn split_after(&self, frames_in: u64) -> Option<(Self, Self)> {
        if frames_in == 0 || frames_in >= self.duration_in_frames {
            return None;
        }
        Some((
            Self {
                duration_in_frames: frames_in,
                ..*self
            },
            Self {
                duration_in_frames: self.duration_in_frames - frames_in,
                ..*self
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::tracks::placement::Angle;

    fn rate(frames_per_second: u32) -> FrameRate {
        FrameRate::from_whole_frames_per_second(frames_per_second).unwrap()
    }

    fn somewhere() -> Placement {
        Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.3), Angle::from_degrees(15.0))
    }

    fn a_generator(duration: u64) -> GeneratorClip {
        GeneratorClip::new(duration, EffectInstanceId::FIRST).unwrap()
    }

    #[test]
    fn a_generator_clip_lasts_at_least_a_frame_and_is_the_whole_picture_until_it_is_put_somewhere() {
        assert_eq!(GeneratorClip::new(0, EffectInstanceId::FIRST), Err(EmptyGeneratorClip));
        let clip = GeneratorClip::new(150, EffectInstanceId::new(3)).unwrap();
        assert_eq!(clip.duration_in_frames(), 150);
        assert_eq!(clip.content(), EffectInstanceId::new(3));
        assert_eq!(clip.placement(), Placement::upright(CanvasArea::WHOLE_PICTURE));
    }

    #[test]
    fn its_place_and_length_change_one_at_a_time_and_nothing_else_does() {
        let clip = a_generator(90);
        let placed = clip.with_placement(somewhere());
        assert_eq!(
            (placed.placement(), placed.duration_in_frames(), placed.content()),
            (somewhere(), 90, EffectInstanceId::FIRST)
        );
        let longer = placed.with_duration_in_frames(400).unwrap();
        assert_eq!(
            (longer.placement(), longer.duration_in_frames(), longer.content()),
            (somewhere(), 400, EffectInstanceId::FIRST)
        );
        assert_eq!(clip.with_duration_in_frames(0), None);
    }

    #[test]
    fn cutting_a_generator_makes_two_with_the_same_content_in_the_same_place_that_last_as_long_together() {
        let clip = a_generator(90).with_placement(somewhere());
        let (first, second) = clip.split_after(30).unwrap();
        assert_eq!((first.duration_in_frames(), second.duration_in_frames()), (30, 60));
        for half in [first, second] {
            assert_eq!(
                (half.placement(), half.content()),
                (somewhere(), EffectInstanceId::FIRST)
            );
        }
        for outside in [0, 90, 91] {
            assert_eq!(clip.split_after(outside), None, "after {outside} frames");
        }
    }

    #[test]
    fn a_generator_counted_again_at_another_frame_rate_lasts_as_long_in_time_and_a_frame_at_the_least() {
        let clip = a_generator(31).with_placement(somewhere());
        assert_eq!(clip.retimed(rate(30), rate(60)).duration_in_frames(), 62);
        assert_eq!(
            clip.retimed(rate(30), rate(15)).duration_in_frames(),
            16,
            "half a frame is rounded up"
        );
        assert_eq!(clip.retimed(rate(30), rate(30)), clip);
        assert_eq!(clip.retimed(rate(30), rate(60)).placement(), somewhere());
        assert_eq!(a_generator(1).retimed(rate(30), rate(12)).duration_in_frames(), 1);
    }
}
