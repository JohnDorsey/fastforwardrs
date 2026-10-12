//! A keyframe crate: a clip that has no picture and no sound, and holds racks of keyframes in its drawer.
//!
//! A crate is how racks of keyframes are put on a track on their own, where they are not in the drawer of a clip: it is a
//! stretch of the track, which lasts as long as it says (whatever the speed of its track, like a picture), and is nothing
//! but a drawer that is always open. It cannot have a drawer, because that is what it is.
//!
//! What is in a crate is in the drawer of its place on the track (see [`super::TrackEntry`]), as for any clip: it moves,
//! is cut, stretched and deleted with it.

use thiserror::Error;

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

    /// The same crate lasting a different number of frames, or `None` if that is none at all.
    #[must_use]
    pub fn with_duration_in_frames(&self, duration_in_frames: u64) -> Option<Self> {
        Self::new(duration_in_frames).ok()
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
        assert!(CrateClip::new(5).unwrap().with_duration_in_frames(0).is_none());
        assert_eq!(
            CrateClip::new(5)
                .unwrap()
                .with_duration_in_frames(9)
                .unwrap()
                .duration_in_frames(),
            9
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
