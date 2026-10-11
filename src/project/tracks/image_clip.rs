//! A picture (a photograph, a logo) that is shown for a while: which file it is, for how long, and
//! where on the picture it is.

use thiserror::Error;

use super::placement::Placement;
use crate::project::SourceMediaIdentifier;

/// Returned when a clip would last no time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a clip must last at least one frame")]
pub struct EmptyImageClip;

/// An image on a track, and for how long it is shown. An image has no speed: it is shown for exactly
/// as long as it says, whatever the speed of its track. Until it is moved, resized or turned it has
/// no [`Placement`] of its own and is fitted inside the picture, as a video is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageClip {
    source: SourceMediaIdentifier,
    duration_in_frames: u64,
    placement: Option<Placement>,
}

impl ImageClip {
    pub fn new(source: SourceMediaIdentifier, duration_in_frames: u64) -> Result<Self, EmptyImageClip> {
        if duration_in_frames == 0 {
            return Err(EmptyImageClip);
        }
        Ok(Self {
            source,
            duration_in_frames,
            placement: None,
        })
    }

    /// The image file this is a picture of.
    pub fn source(&self) -> SourceMediaIdentifier {
        self.source
    }

    /// How many frames of the timeline the image is shown for.
    pub fn duration_in_frames(&self) -> u64 {
        self.duration_in_frames
    }

    /// Where the image is on the picture, or `None` if it has not been placed: then it is fitted
    /// inside the picture.
    pub fn placement(&self) -> Option<Placement> {
        self.placement
    }

    #[must_use]
    pub fn with_placement(&self, placement: Option<Placement>) -> Self {
        Self { placement, ..*self }
    }

    /// The same image shown for a different number of frames, or `None` if that is none at all.
    #[must_use]
    pub fn with_duration_in_frames(&self, duration_in_frames: u64) -> Option<Self> {
        (duration_in_frames > 0).then_some(Self {
            duration_in_frames,
            ..*self
        })
    }

    /// Cuts the clip in two after its first `frames_in` frames, or `None` if that is not strictly
    /// inside it. Both halves are the same image in the same place.
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
    use crate::project::tracks::placement::{Angle, CanvasArea};

    const SOURCE: SourceMediaIdentifier = SourceMediaIdentifier::new(4);

    fn somewhere() -> Placement {
        Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.3), Angle::from_degrees(15.0))
    }

    #[test]
    fn an_image_is_shown_for_at_least_a_frame_and_is_not_placed_until_it_is_put_somewhere() {
        assert_eq!(ImageClip::new(SOURCE, 0), Err(EmptyImageClip));
        let clip = ImageClip::new(SOURCE, 150).unwrap();
        assert_eq!(
            (clip.source(), clip.duration_in_frames(), clip.placement()),
            (SOURCE, 150, None)
        );
    }

    #[test]
    fn an_image_can_be_placed_and_unplaced_and_that_changes_nothing_else() {
        let clip = ImageClip::new(SOURCE, 150).unwrap();
        let placed = clip.with_placement(Some(somewhere()));
        assert_eq!(placed.placement(), Some(somewhere()));
        assert_eq!((placed.source(), placed.duration_in_frames()), (SOURCE, 150));
        assert_eq!(placed.with_placement(None), clip);
    }

    #[test]
    fn how_long_an_image_is_shown_for_can_be_changed_but_not_to_none_and_keeps_the_place() {
        let clip = ImageClip::new(SOURCE, 150).unwrap().with_placement(Some(somewhere()));
        let longer = clip.with_duration_in_frames(400).unwrap();
        assert_eq!(
            (longer.duration_in_frames(), longer.placement()),
            (400, Some(somewhere()))
        );
        assert_eq!(clip.with_duration_in_frames(0), None);
    }

    #[test]
    fn cutting_an_image_makes_two_of_the_same_in_the_same_place_that_last_as_long_together() {
        let clip = ImageClip::new(SOURCE, 90).unwrap().with_placement(Some(somewhere()));
        let (first, second) = clip.split_after(30).unwrap();
        assert_eq!((first.duration_in_frames(), second.duration_in_frames()), (30, 60));
        assert_eq!((first.source(), second.source()), (SOURCE, SOURCE));
        assert_eq!(
            (first.placement(), second.placement()),
            (Some(somewhere()), Some(somewhere()))
        );
        for outside in [0, 90, 91] {
            assert_eq!(clip.split_after(outside), None, "after {outside} frames");
        }
    }
}
