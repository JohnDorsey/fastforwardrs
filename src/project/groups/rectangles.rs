//! Where things are on the timeline as rectangles: so many frames across, and so many tracks down. The rectangle of a clip is the
//! frames it lasts, on the track it is on; that of a track is every frame of the project, on that track; and that of a group is the
//! smallest that holds everything in it. It is worked out from where things are whenever it is wanted, and never kept, so that it is
//! never out of date.

use std::ops::RangeInclusive;

use super::{GroupId, Member};
use crate::project::Project;
use crate::project::tracks::{ClipId, TrackId};

/// A stretch of the timeline across some tracks that are next to each other in the list of tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rectangle {
    /// The first frame of the timeline in it.
    pub first_frame: u64,
    /// The frame just after the last frame in it (the end is exclusive).
    pub end_frame: u64,
    /// The place in the list of tracks of the front-most track it reaches (0 is the front).
    pub first_track: usize,
    /// The place in the list of tracks of the back-most track it reaches.
    pub last_track: usize,
}

impl Rectangle {
    /// The rectangle of frames `first_frame..end_frame` of the track at `track` in the list of tracks.
    pub fn of_frames_on_a_track(first_frame: u64, end_frame: u64, track: usize) -> Self {
        Self {
            first_frame,
            end_frame,
            first_track: track,
            last_track: track,
        }
    }

    /// The smallest rectangle that has all of this one and all of `other`.
    #[must_use]
    pub fn holding(self, other: Self) -> Self {
        Self {
            first_frame: self.first_frame.min(other.first_frame),
            end_frame: self.end_frame.max(other.end_frame),
            first_track: self.first_track.min(other.first_track),
            last_track: self.last_track.max(other.last_track),
        }
    }

    /// The places in the list of tracks of the tracks it reaches across, the front-most first.
    pub fn tracks(&self) -> RangeInclusive<usize> {
        self.first_track..=self.last_track
    }

    /// Whether the track at `track` in the list of tracks is one of those it reaches across.
    pub fn reaches_the_track_at(&self, track: usize) -> bool {
        self.tracks().contains(&track)
    }

    /// Whether any frame is in both this rectangle and the frames `first_frame..end_frame`.
    pub fn has_any_of_the_frames(&self, first_frame: u64, end_frame: u64) -> bool {
        first_frame < self.end_frame && self.first_frame < end_frame
    }

    /// Whether this rectangle and `other` have a frame of a track in both: they are on top of each other there.
    pub fn is_on_top_of(&self, other: &Self) -> bool {
        self.has_any_of_the_frames(other.first_frame, other.end_frame)
            && self.first_track <= other.last_track
            && other.first_track <= self.last_track
    }
}

impl Project {
    /// Where the clip `clip` is: its frames, on the track it is on now. `None` if there is no such clip.
    pub fn rectangle_of_the_clip(&self, clip: ClipId) -> Option<Rectangle> {
        let position = self.track_position_holding(clip)?;
        let placement = self.tracks[position].placement_of(clip)?;
        Some(Rectangle::of_frames_on_a_track(
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value(),
            position,
        ))
    }

    /// Where the track `track` is: every frame of the project (up to where the longest track ends), on that track. `None` if there
    /// is no such track.
    pub fn rectangle_of_the_track(&self, track: TrackId) -> Option<Rectangle> {
        let position = self.track_position(track)?;
        Some(Rectangle::of_frames_on_a_track(0, self.total_frame_count(), position))
    }

    /// The smallest rectangle that has everything that is in the group `group` in it (what its members are, and the members of
    /// the groups that are in it, as far down as that goes). `None` if there is no such group, or nothing is in it.
    ///
    /// For a clip group this is the rectangle that nothing but its own may be in. For a track group it reaches across all the
    /// frames of the project (it has only tracks and track groups in it, and the rectangle of a track does). For a tag it is only the
    /// box that holds everything in it.
    pub fn rectangle_of_the_group(&self, group: GroupId) -> Option<Rectangle> {
        self.rectangle_of_the_group_below(group, self.groups.len())
    }

    /// [`Self::rectangle_of_the_group`], looking down through at most `depth` levels of groups in groups (so that a project that
    /// has a group in itself, which the rules do not allow, is not looked at for ever).
    fn rectangle_of_the_group_below(&self, group: GroupId, depth: usize) -> Option<Rectangle> {
        let group = self.group(group)?;
        let mut rectangle: Option<Rectangle> = None;
        for member in group.members() {
            let of_the_member = match *member {
                Member::Clip(clip) => self.rectangle_of_the_clip(clip),
                Member::Track(track) => self.rectangle_of_the_track(track),
                Member::Group(inner) => depth
                    .checked_sub(1)
                    .and_then(|depth| self.rectangle_of_the_group_below(inner, depth)),
            };
            rectangle = match (rectangle, of_the_member) {
                (Some(so_far), Some(next)) => Some(so_far.holding(next)),
                (so_far, next) => so_far.or(next),
            };
        }
        rectangle
    }
}

#[cfg(test)]
mod tests;
