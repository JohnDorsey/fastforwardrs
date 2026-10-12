//! Names for the tracks of a project and the clips, racks and keyframes on them that do not change when the project is
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

/// Tells the racks of one project apart (a rack holds the keyframes of one property, see [`super::rack::Rack`]), the same
/// way [`ClipId`] does clips: the track the rack was made on, and which of the racks that track has made it is. A rack that
/// is dragged to a drawer on another track keeps its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RackId {
    origin: TrackId,
    number: u32,
}

impl RackId {
    pub const fn new(origin: TrackId, number: u32) -> Self {
        Self { origin, number }
    }

    /// The track the rack was made on, which is not the track it is on if it has been moved.
    pub const fn origin(self) -> TrackId {
        self.origin
    }

    /// Which of the racks made on its origin track this is.
    pub const fn number(self) -> u32 {
        self.number
    }
}

impl fmt::Display for RackId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "rack {} of {}", self.number, self.origin)
    }
}

/// Tells the keyframes of one project apart, the same way [`ClipId`] does clips: the rack the keyframe was made in, and which
/// of the keyframes that rack has made it is. A keyframe keeps its id when it is moved to another frame and when it is moved
/// to another rack (or its rack is merged with another), so that what points at it (the selection, a drag) goes on pointing
/// at it. (Which rack it is in now is found by looking: see `Project::locate_keyframe`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyframeId {
    origin: RackId,
    number: u32,
}

impl KeyframeId {
    pub const fn new(origin: RackId, number: u32) -> Self {
        Self { origin, number }
    }

    /// The rack the keyframe was made in, which is not the rack it is in if it has been moved to another.
    pub const fn origin(self) -> RackId {
        self.origin
    }

    /// Which of the keyframes made in its origin rack this is.
    pub const fn number(self) -> u32 {
        self.number
    }
}

impl fmt::Display for KeyframeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // (Of the rack it was made in, which is what makes it this keyframe and no other.)
        write!(formatter, "keyframe {} of {}", self.number, self.origin)
    }
}
