// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A generator clip (phase 5, EF-02, Q-11): a clip with no media whose picture is made by the program from a few values. The only generator so far is **Solid colour**, one
//! colour (with its clearness) that fills the clip's place on the picture, which is the whole of it until the clip is moved, resized or turned like a picture. (The generators of
//! ISF files come with phase 6 and the text generator with phase 9: see `docs/engine-phase-5-design.md`, H8.)
//!
//! Like a picture it lasts as long as it says whatever the speed of its track, and it has the chain of effects that every clip has.

use thiserror::Error;

use super::placement::{CanvasArea, Placement};
use super::text_object::TextColour;
use crate::project::FrameRate;

/// Returned when a generator clip would last no time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a generator clip must last at least one frame")]
pub struct EmptyGeneratorClip;

/// What a generator makes the picture of: the kinds there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeneratorKind {
    /// One colour over the whole of the clip's place on the picture.
    SolidColour,
}

impl GeneratorKind {
    /// The kinds there are, in the order they are offered in.
    pub const ALL: [GeneratorKind; 1] = [Self::SolidColour];

    /// What the kind is called in a project file, and for as long as the kind exists: a namespace, a colon and an id, as the key of an effect is.
    pub fn key(self) -> &'static str {
        match self {
            Self::SolidColour => "builtin:solid_colour",
        }
    }

    /// The kind that is called `key` in a project file, if there is one.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// What the kind is called, as it is shown.
    pub fn name(self) -> &'static str {
        match self {
            Self::SolidColour => "Solid colour",
        }
    }

    /// The colour that a new generator of the kind makes the picture of.
    pub fn colour_it_begins_as(self) -> TextColour {
        match self {
            Self::SolidColour => TextColour::new(128, 128, 128, 255),
        }
    }
}

/// A generator clip on a track: which generator, what it makes the picture of, for how long, and where on the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorClip {
    kind: GeneratorKind,
    duration_in_frames: u64,
    /// The colour the generator makes the picture of (for a solid colour: the one it is), straight (not premultiplied).
    pub colour: TextColour,
    placement: Placement,
}

impl GeneratorClip {
    /// A generator of `kind` that lasts `duration_in_frames` frames, makes the picture of the colour it begins as and fills the whole picture.
    pub fn new(kind: GeneratorKind, duration_in_frames: u64) -> Result<Self, EmptyGeneratorClip> {
        if duration_in_frames == 0 {
            return Err(EmptyGeneratorClip);
        }
        Ok(Self {
            kind,
            duration_in_frames,
            colour: kind.colour_it_begins_as(),
            placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
        })
    }

    pub fn kind(&self) -> GeneratorKind {
        self.kind
    }

    /// How many frames of the timeline the clip lasts.
    pub fn duration_in_frames(&self) -> u64 {
        self.duration_in_frames
    }

    /// Where the clip is on the picture, and how far it is turned.
    pub fn placement(&self) -> Placement {
        self.placement
    }

    #[must_use]
    pub fn with_placement(&self, placement: Placement) -> Self {
        Self { placement, ..*self }
    }

    #[must_use]
    pub fn with_colour(&self, colour: TextColour) -> Self {
        Self { colour, ..*self }
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

    /// Cuts the clip in two after its first `frames_in` frames, or `None` if that is not strictly inside it. Both halves are the same generator with the same colour in the
    /// same place.
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

    #[test]
    fn a_generator_clip_lasts_at_least_a_frame_and_begins_as_grey_over_the_whole_picture() {
        assert_eq!(
            GeneratorClip::new(GeneratorKind::SolidColour, 0),
            Err(EmptyGeneratorClip)
        );
        let clip = GeneratorClip::new(GeneratorKind::SolidColour, 150).unwrap();
        assert_eq!(clip.kind(), GeneratorKind::SolidColour);
        assert_eq!(clip.duration_in_frames(), 150);
        assert_eq!(clip.colour, TextColour::new(128, 128, 128, 255));
        assert_eq!(clip.placement(), Placement::upright(CanvasArea::WHOLE_PICTURE));
    }

    #[test]
    fn a_kind_is_found_by_the_key_it_is_saved_under_and_by_no_other() {
        for kind in GeneratorKind::ALL {
            assert_eq!(GeneratorKind::from_key(kind.key()), Some(kind));
            assert!(kind.key().contains(':') && !kind.name().is_empty());
        }
        assert_eq!(
            GeneratorKind::from_key("builtin:solid_colour"),
            Some(GeneratorKind::SolidColour)
        );
        assert_eq!(GeneratorKind::from_key("builtin:nothing"), None);
        assert_eq!(GeneratorKind::from_key(""), None);
    }

    #[test]
    fn its_colour_and_place_and_length_change_one_at_a_time_and_nothing_else_does() {
        let clip = GeneratorClip::new(GeneratorKind::SolidColour, 90).unwrap();
        let red = TextColour::new(255, 0, 0, 200);

        let coloured = clip.with_colour(red);
        assert_eq!(
            (coloured.colour, coloured.placement(), coloured.duration_in_frames()),
            (red, clip.placement(), 90)
        );
        let placed = clip.with_placement(somewhere());
        assert_eq!(
            (placed.colour, placed.placement(), placed.duration_in_frames()),
            (clip.colour, somewhere(), 90)
        );
        let longer = coloured
            .with_placement(somewhere())
            .with_duration_in_frames(400)
            .unwrap();
        assert_eq!(
            (longer.colour, longer.placement(), longer.duration_in_frames()),
            (red, somewhere(), 400)
        );
        assert_eq!(clip.with_duration_in_frames(0), None);
    }

    #[test]
    fn cutting_a_generator_makes_two_of_the_same_colour_in_the_same_place_that_last_as_long_together() {
        let clip = GeneratorClip::new(GeneratorKind::SolidColour, 90)
            .unwrap()
            .with_colour(TextColour::new(1, 2, 3, 4))
            .with_placement(somewhere());
        let (first, second) = clip.split_after(30).unwrap();
        assert_eq!((first.duration_in_frames(), second.duration_in_frames()), (30, 60));
        for half in [first, second] {
            assert_eq!(
                (half.colour, half.placement(), half.kind()),
                (clip.colour, somewhere(), GeneratorKind::SolidColour)
            );
        }
        for outside in [0, 90, 91] {
            assert_eq!(clip.split_after(outside), None, "after {outside} frames");
        }
    }

    #[test]
    fn a_generator_counted_again_at_another_frame_rate_lasts_as_long_in_time_and_a_frame_at_the_least() {
        let clip = GeneratorClip::new(GeneratorKind::SolidColour, 31)
            .unwrap()
            .with_colour(TextColour::new(9, 9, 9, 9));
        assert_eq!(clip.retimed(rate(30), rate(60)).duration_in_frames(), 62);
        assert_eq!(
            clip.retimed(rate(30), rate(15)).duration_in_frames(),
            16,
            "half a frame is rounded up"
        );
        assert_eq!(clip.retimed(rate(30), rate(30)), clip);
        assert_eq!(clip.retimed(rate(30), rate(60)).colour, clip.colour);
        assert_eq!(
            GeneratorClip::new(GeneratorKind::SolidColour, 1)
                .unwrap()
                .retimed(rate(30), rate(12))
                .duration_in_frames(),
            1
        );
    }
}
