//! Moving clips by dragging them: how far they can go.
//!
//! Dragging a clip moves it as far as the pointer goes along its track (and up or down to the other tracks),
//! until it comes to another clip that is not moving along with it (no two clips can be at the same time on
//! one track), or to the start of the timeline. The clips in the way stop it, they are not pushed. A clip
//! goes to the track the pointer is over if there is room for it there, where it is along the timeline, and
//! otherwise stays on the track it is on: tracks that it does not fit on are not entered. A clip that goes to
//! another track can be held to the time it started at, as a magnet holds iron (see [`Snap`], and
//! [`StartTimeMemory`] for when it is). Everything here is arithmetic on frames and on the places of tracks in the
//! list of them, so that how far is allowed can be tested without a window.

use std::collections::HashSet;

use super::start_time_hold::{HeldTime, StartTimeMemory};
use crate::project::{ClipId, ClipMove, Project, TimelineFrameIndex, TrackId};

/// How far to move clips: down that many tracks in the list of them (up, if negative), and later by that
/// many frames of the timeline (earlier, if negative).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveBy {
    pub tracks_down: i64,
    pub frames: i64,
}

/// How far the clips `moving` can be moved when nothing holds any of them to a time: [`allowed_move_holding`]
/// without a [`Snap`], which is what the tests of how far clips go use.
#[cfg(test)]
pub fn allowed_move(project: &Project, moving: &[ClipId], wanted: MoveBy) -> MoveBy {
    allowed_move_holding(project, moving, wanted, None)
}

/// A time that a dragged clip is held to when it is near, as a magnet holds iron.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snap {
    /// The clip that is held: the one that was taken hold of. The others that move with it go by as much as it
    /// does.
    pub clip: ClipId,
    /// The frame the clip is held to starting at.
    pub start: u64,
    /// How near to `start`, in frames, the frame the clip would start at if it followed the pointer exactly has
    /// to be for it to be held there.
    pub within_frames: u64,
    /// The track that the clip is not held to `start` on, if there is one.
    pub free_on: Option<TrackId>,
}

impl Snap {
    /// What holds `clip`, the clip that was taken hold of, to the time it started at, if anything does: what `memory` says, when a
    /// frame is `points_per_frame` wide.
    pub fn held_by(clip: ClipId, memory: &StartTimeMemory<TrackId>, points_per_frame: f64) -> Option<Self> {
        memory.held_time(points_per_frame).map(|held| Self {
            clip,
            start: held.start,
            within_frames: held.within_frames,
            free_on: held.free_on,
        })
    }

    fn held_time(&self) -> HeldTime<TrackId> {
        HeldTime {
            start: self.start,
            within_frames: self.within_frames,
            free_on: self.free_on,
        }
    }
}

/// How far the clips `moving` can be moved when `wanted` is how far the pointer has gone: `wanted` itself, or
/// as much of it as can be done. All the clips move by the same, so two of them that are moving never come
/// to one another by it: only the clips that are not moving stop them.
///
/// The tracks come first. The clips go down or up as many tracks as are wanted, but not past the first or
/// the last, and if they do not fit on the tracks they would then be on (where they are along the timeline),
/// as many fewer as they do fit on. Then they are moved along the timeline as far as the clips that stay on
/// those tracks leave room for.
///
/// With the clip that was taken hold of held to the time of the [`Snap`] when it is near that, where it is held
/// it is wanted at that time and not where the pointer would put it, and so it fits on a track if it fits there
/// at that time, though it would not at the frame it is at now.
pub fn allowed_move_holding(project: &Project, moving: &[ClipId], wanted: MoveBy, snap: Option<Snap>) -> MoveBy {
    let positions: Vec<usize> = moving
        .iter()
        .filter_map(|clip| project.track_position_holding(*clip))
        .collect();
    let (Some(&highest), Some(&lowest)) = (positions.iter().min(), positions.iter().max()) else {
        // None of them is there, so nothing stops them.
        return MoveBy {
            tracks_down: 0,
            frames: wanted.frames,
        };
    };
    let furthest_up = -to_signed(highest);
    let furthest_down = to_signed(project.tracks().len()) - 1 - to_signed(lowest);
    let wanted_tracks = wanted.tracks_down.clamp(furthest_up, furthest_down);
    // From as many tracks as were wanted back to none, the first that they fit on.
    let tracks_down = std::iter::successors(Some(wanted_tracks), |&tracks| {
        (tracks != 0).then(|| tracks - tracks.signum())
    })
    .find(|&tracks| {
        tracks == 0
            || the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks, 0)
            || frames_to_the_time_held_to(project, wanted, snap, tracks)
                .is_some_and(|held| the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks, held))
    })
    .unwrap_or(0);
    let frames = frames_to_the_time_held_to(project, wanted, snap, tracks_down).unwrap_or(wanted.frames);
    MoveBy {
        tracks_down,
        frames: allowed_frames(project, moving, tracks_down, frames),
    }
}

/// How many frames later (earlier, if negative) than where it is now the clip that `snap` holds is at the time it
/// is held to, if it is held when the clips go down `tracks_down` tracks and `wanted` is what is wanted of
/// them: it is, unless it is on a track that it is free on, or what the pointer wants of it would put it too far
/// from that time.
fn frames_to_the_time_held_to(project: &Project, wanted: MoveBy, snap: Option<Snap>, tracks_down: i64) -> Option<i64> {
    let snap = snap?;
    let from = project.track_position_holding(snap.clip)?;
    let placement = project.tracks()[from].placement_of(snap.clip)?;
    let destination = project
        .tracks()
        .get(usize::try_from(to_signed(from) + tracks_down).ok()?)?;
    snap.held_time()
        .shift_to_it(destination.id(), placement.first_timeline_frame.value(), wanted.frames)
}

/// Whether the clips `moving` can be moved down `tracks_down` tracks and `frames` frames along.
fn the_clips_fit_on_the_tracks_they_would_be_on(
    project: &Project,
    moving: &[ClipId],
    tracks_down: i64,
    frames: i64,
) -> bool {
    let moves = moves_by(project, moving, MoveBy { tracks_down, frames });
    project.clone().move_clips(&moves).is_ok()
}

/// How many frames earlier (negative) or later the clips `moving` can be moved, when they are on the tracks
/// `tracks_down` below the ones they are on, and `wanted` is how many are wanted: never before the start of
/// the timeline, and never more than the clips that stay on those tracks leave room for. (How long a clip
/// lasts is counted by the speed of the track it is on then.)
fn allowed_frames(project: &Project, moving: &[ClipId], tracks_down: i64, wanted: i64) -> i64 {
    let moving_set: HashSet<ClipId> = moving.iter().copied().collect();
    let mut furthest_earlier = i64::MIN;
    let mut furthest_later = i64::MAX;
    for clip in moving {
        let Some(from) = project.track_position_holding(*clip) else {
            continue;
        };
        let Some(placement) = project.tracks()[from].placement_of(*clip) else {
            continue;
        };
        let Some(destination) = usize::try_from(to_signed(from) + tracks_down)
            .ok()
            .and_then(|position| project.tracks().get(position))
        else {
            continue;
        };
        let first = to_signed(placement.first_timeline_frame.value());
        let end = first.saturating_add(to_signed(placement.clip.timeline_frame_count(destination.speed())));
        furthest_earlier = furthest_earlier.max(-first);
        for other in destination.placements() {
            if moving_set.contains(&other.id) {
                continue;
            }
            let other_first = to_signed(other.first_timeline_frame.value());
            let other_end = to_signed(other.end_timeline_frame.value());
            if other_first >= end {
                furthest_later = furthest_later.min(other_first - end);
            } else if other_end <= first {
                furthest_earlier = furthest_earlier.max(other_end - first);
            }
        }
    }
    // (The least is never more than nothing and the most never less, so the range is never empty.)
    wanted.clamp(furthest_earlier.min(0), furthest_later.max(0))
}

fn to_signed<T: TryInto<i64>>(number: T) -> i64 {
    number.try_into().unwrap_or(i64::MAX)
}

/// The moves that put each of the clips `moving` `by` as far from where it is: on the track that many
/// tracks down, starting that many frames later (never before the start of the timeline). A clip that is not
/// in the project, or that would be on a track that is not there, is left out.
pub fn moves_by(project: &Project, moving: &[ClipId], by: MoveBy) -> Vec<ClipMove> {
    moving
        .iter()
        .filter_map(|clip| {
            let from = project.track_position_holding(*clip)?;
            let placement = project.tracks()[from].placement_of(*clip)?;
            let to = project
                .tracks()
                .get(usize::try_from(to_signed(from) + by.tracks_down).ok()?)?;
            let start = to_signed(placement.first_timeline_frame.value())
                .saturating_add(by.frames)
                .max(0);
            Some(ClipMove {
                clip: *clip,
                to_track: to.id(),
                start: TimelineFrameIndex::new(start as u64),
            })
        })
        .collect()
}

#[cfg(test)]
mod snap_tests;
#[cfg(test)]
mod tests;
