//! Names for the tracks of a project and the clips on them that do not change when the project is
//! edited, so that something can keep pointing at a clip (the selected one, say) while other
//! clips are cut, deleted or moved along the timeline around it, or it is moved to another track.
//!
//! They are made up when a project is made or loaded, and are never written to a project file.

use std::fmt;

/// Tells the tracks of one project apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackId(u32);

impl TrackId {
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "track {}", self.0)
    }
}

/// Tells the clips of one project apart, for as long as they are in it and wherever on its tracks they are
/// put: the track the clip was made on, and which of the clips that track has made it is. A clip that is
/// moved to another track keeps its id, so that what points at it (the selection, a drag, a window) goes on
/// pointing at it. (The tracks only make ids with their own number as the origin, so two tracks never make
/// the same one.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipId {
    origin: TrackId,
    number: u32,
}

impl ClipId {
    pub const fn new(origin: TrackId, number: u32) -> Self {
        Self { origin, number }
    }

    /// The track the clip was made on, which is not the track it is on if it has been moved.
    pub const fn origin(self) -> TrackId {
        self.origin
    }

    /// Which of the clips made on its origin track this is.
    pub const fn number(self) -> u32 {
        self.number
    }
}

impl fmt::Display for ClipId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // (Of the track it was made on, which is what makes it this clip and no other.)
        write!(formatter, "clip {} of {}", self.number, self.origin)
    }
}
