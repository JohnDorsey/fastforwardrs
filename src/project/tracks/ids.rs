//! Names for the tracks of a project and the clips on them that do not change when the project is
//! edited, so that something can keep pointing at a clip (the selected one, say) while other
//! clips are cut, deleted or moved along the timeline around it.
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

/// Tells the clips of one track apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipId(u32);

impl ClipId {
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ClipId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "clip {}", self.0)
    }
}

/// One clip of a project: which track it is on, and which clip of that track it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipLocation {
    pub track: TrackId,
    pub clip: ClipId,
}

impl fmt::Display for ClipLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} of {}", self.clip, self.track)
    }
}
