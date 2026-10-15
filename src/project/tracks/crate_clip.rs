// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A keyframe crate: a clip that has no picture and no sound, and holds racks of keyframes in its drawer.
//!
//! A crate is how racks of keyframes are put on a track on their own, where they are not in the drawer of a clip: it is a
//! stretch of the track (whatever the speed of its track, like a picture), and is nothing but a drawer that is always open.
//! It cannot have a drawer, because that is what it is.
//!
//! **A crate lasts exactly as long as the keyframes in it need**: from the frame of the first of them, in whichever of its racks,
//! to the frame of the last (and so at least one frame, for a crate with one keyframe). Nobody sets how long it lasts: it shrinks
//! when keyframes are taken out or moved in, and grows when keyframes come that need more room (where there is room for it:
//! see `Project::fit_the_crate` and `Project::make_the_crate_begin_by`, which keep it so).
//!
//! What is in a crate is in the drawer of its place on the track (see [`super::TrackEntry`]), as for any clip, counted in frames
//! from the first frame of the crate: it moves, is cut and deleted with it.

use thiserror::Error;

use crate::project::FrameRate;

/// Returned when a crate would last no time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a crate must last at least one frame")]
pub struct EmptyCrate;

/// A keyframe crate on a track, and for how long it lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrateClip {
    duration_in_frames: u64,
}

impl CrateClip {
    pub fn new(duration_in_frames: u64) -> Result<Self, EmptyCrate> {
        if duration_in_frames == 0 {
            return Err(EmptyCrate);
        }
        Ok(Self { duration_in_frames })
    }

    /// How many frames of the timeline the crate lasts.
    pub fn duration_in_frames(&self) -> u64 {
        self.duration_in_frames
    }

    /// The crate as it is when its project runs at the frame rate `to` and not `from`: it lasts as long as it did, counted again at the
    /// new rate (for at least one frame). Its keyframes are counted again with it, and it is made as long as they need afterwards (see
    /// `Project::fit_every_crate`).
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Self {
        Self {
            duration_in_frames: from.convert_frame_count_to(self.duration_in_frames, to).max(1),
        }
    }

    /// Cuts the crate in two after its first `frames_in` frames, or `None` if that is not strictly inside it.
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

    #[test]
    fn a_crate_lasts_at_least_a_frame() {
        assert_eq!(CrateClip::new(0), Err(EmptyCrate));
        assert_eq!(CrateClip::new(1).unwrap().duration_in_frames(), 1);
        assert_eq!(CrateClip::new(9).unwrap().duration_in_frames(), 9);
    }

    #[test]
    fn a_crate_counted_again_at_another_frame_rate_lasts_as_long_in_time_and_a_frame_at_the_least() {
        let rate = |frames_per_second| FrameRate::from_whole_frames_per_second(frames_per_second).unwrap();
        let crate_clip = CrateClip::new(31).unwrap();

        assert_eq!(crate_clip.retimed(rate(30), rate(60)).duration_in_frames(), 62);
        assert_eq!(
            crate_clip.retimed(rate(30), rate(15)).duration_in_frames(),
            16,
            "half a frame is rounded up"
        );
        assert_eq!(crate_clip.retimed(rate(30), rate(30)), crate_clip);
        assert_eq!(
            CrateClip::new(1)
                .unwrap()
                .retimed(rate(30), rate(12))
                .duration_in_frames(),
            1,
            "a frame at 30 per second would last no time at 12 per second, and a crate has to last"
        );
    }

    #[test]
    fn a_crate_is_cut_into_two_that_last_as_long_as_it_did_and_not_at_its_ends() {
        let crate_clip = CrateClip::new(10).unwrap();
        let (first, second) = crate_clip.split_after(4).unwrap();
        assert_eq!((first.duration_in_frames(), second.duration_in_frames()), (4, 6));
        assert_eq!(crate_clip.split_after(0), None);
        assert_eq!(crate_clip.split_after(10), None);
        assert_eq!(crate_clip.split_after(99), None);
    }
}
