//! Moving clips by dragging them: how far they can go.
//!
//! Dragging a clip moves it as far as the pointer goes along its track (and up or down to the other tracks),
//! until it comes to another clip that is not moving along with it (no two clips can be at the same time on
//! one track), or to the start of the timeline, or to what the rules of clip groups do not allow (a clip that is not in a clip group cannot
//! be inside its rectangle: see [`stopped_by_the_groups`]). The clips in the way stop it, they are not pushed. A clip
//! goes to the track the pointer is over if there is room for it there, where it is along the timeline, and
//! otherwise stays on the track it is on: tracks that it does not fit on are not entered. A clip that goes to
//! another track can be held to the time it started at, as a magnet holds iron (see [`Snap`], and
//! [`StartTimeMemory`] for when it is). Everything here is arithmetic on frames and on the places of tracks in the
//! list of them, so that how far is allowed can be tested without a window.

use std::collections::HashSet;

use super::start_time_hold::{HeldTime, StartTimeMemory};
use crate::project::{ClipId, ClipMove, GroupKind, Project, TimelineFrameIndex, TrackId};

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
/// those tracks leave room for, and as far as the rectangles of clip groups let them (see [`stopped_by_the_groups`]).
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
    let frames = allowed_frames(project, moving, tracks_down, frames);
    MoveBy {
        tracks_down,
        frames: stopped_by_the_groups(project, moving, tracks_down, frames),
    }
}

/// How many of the `frames` that the clips `moving` can be moved along, when they are on the tracks `tracks_down` below the ones they are on,
/// the rules of clip groups let them be moved: a clip that is not in a clip group stops against the rectangle of the group, which it cannot
/// be inside, and a clip that is in one stops when its group would take in a clip that is not in it. (Clips that are all of a group move
/// along with its rectangle.) They are stopped as clips are by other clips: the first place along the way where the move would not be
/// allowed is where they stop, and they do not go on past it to a place where it would be allowed again, which is what a drag that jumped
/// over a rectangle would be.
///
/// Where there are no clip groups nothing is looked at, and the answer is `frames`. Otherwise, when the clips are allowed where they are
/// now, the first place that is not allowed is worked out by arithmetic (see [`stop_worked_out_by_arithmetic`]), which takes the same time
/// however far the clips are taken, and the answer is checked: it must be a place that the move itself (see [`Project::move_clips`]) allows.
/// If the clips are not allowed where they are now (which is so when the tracks they go to were chosen for the time they are held to), or the
/// arithmetic cannot say, or what it says is not allowed, each place along the way is tried as the move itself is, on a copy of the project,
/// until one is not allowed.
fn stopped_by_the_groups(project: &Project, moving: &[ClipId], tracks_down: i64, frames: i64) -> i64 {
    let has_clip_groups = project
        .groups()
        .iter()
        .any(|group| group.kind() == GroupKind::ClipGroup);
    if !has_clip_groups || frames == 0 {
        return frames;
    }
    let is_allowed = |shift: i64| the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks_down, shift);
    if is_allowed(0)
        && let Some(stop) = stop_worked_out_by_arithmetic(project, moving, tracks_down, frames)
        && is_allowed(stop)
    {
        return stop;
    }
    stopped_by_trying_each_place(project, moving, tracks_down, frames)
}

/// [`stopped_by_the_groups`], found by trying each place along the way (and so taking longer the further the clips are taken, which is why it
/// is not what is done when there is another way): the answer for the tests of the arithmetic to be the same as.
fn stopped_by_trying_each_place(project: &Project, moving: &[ClipId], tracks_down: i64, frames: i64) -> i64 {
    let is_allowed = |shift: i64| the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks_down, shift);
    if is_allowed(0) {
        let direction = frames.signum();
        let mut reached = 0;
        for step in 1..=frames.unsigned_abs() {
            let shift = to_signed(step) * direction;
            if !is_allowed(shift) {
                break;
            }
            reached = shift;
        }
        return reached;
    }
    // Where they are now is not a place that the move is allowed (the tracks they would be on were chosen with the time they are held to,
    // which is where `frames` is): `frames` is where they go if that is allowed, and otherwise the nearest place to it that is, on the way back.
    if is_allowed(frames) {
        return frames;
    }
    let direction = -frames.signum();
    (1..=frames.unsigned_abs())
        .map(|step| frames + to_signed(step) * direction)
        .find(|shift| is_allowed(*shift))
        .unwrap_or(0)
}

/// Where a clip would be if the clips that are moved were moved down `tracks_down` tracks, before they are shifted along: the place of its
/// track in the list of tracks, the frames that it lasts from and until (counted by the speed of the track it would be on), and whether it
/// is one of the clips that are shifted, so that it goes along as the shift is made longer.
#[derive(Clone, Copy)]
struct ClipAfterTheMove {
    id: ClipId,
    track_position: usize,
    first: i64,
    end: i64,
    is_shifted: bool,
}

/// Every clip of the project as [`ClipAfterTheMove`] says, or `None` if one of the clips that are moved would go to a track that is not there.
fn clips_after_the_move(project: &Project, moving: &[ClipId], tracks_down: i64) -> Option<Vec<ClipAfterTheMove>> {
    let moving: HashSet<ClipId> = moving.iter().copied().collect();
    let mut clips = Vec::new();
    for (position, track) in project.tracks().iter().enumerate() {
        for placement in track.placements() {
            let first = to_signed(placement.first_timeline_frame.value());
            if moving.contains(&placement.id) {
                let destination_position = usize::try_from(to_signed(position) + tracks_down).ok()?;
                let destination = project.tracks().get(destination_position)?;
                clips.push(ClipAfterTheMove {
                    id: placement.id,
                    track_position: destination_position,
                    first,
                    end: first.saturating_add(to_signed(placement.clip.timeline_frame_count(destination.speed()))),
                    is_shifted: true,
                });
            } else {
                clips.push(ClipAfterTheMove {
                    id: placement.id,
                    track_position: position,
                    first,
                    end: to_signed(placement.end_timeline_frame.value()),
                    is_shifted: false,
                });
            }
        }
    }
    Some(clips)
}

/// A stretch of shifts (how many frames later the clips that are moved are put, earlier if it is negative), from the first of them to the
/// last, both counted.
type Shifts = (i64, i64);

/// What decides whether something is so for a shift: for each shift, or for none, or for those that are below some shift or above some shift.
#[derive(Clone, Copy)]
enum When {
    Always,
    Never,
    ShiftIsBelow(i64),
    ShiftIsAbove(i64),
}

impl When {
    /// The shifts it is so for, if there are any.
    fn shifts(self) -> Option<Shifts> {
        match self {
            Self::Always => Some((i64::MIN, i64::MAX)),
            Self::Never => None,
            Self::ShiftIsBelow(limit) => Some((i64::MIN, limit.saturating_sub(1))),
            Self::ShiftIsAbove(limit) => Some((limit.saturating_add(1), i64::MAX)),
        }
    }
}

/// The shifts that are in both stretches, if there are any.
fn shifts_in_both(first: Option<Shifts>, second: Option<Shifts>) -> Option<Shifts> {
    let (first, second) = (first?, second?);
    let (from, to) = (first.0.max(second.0), first.1.min(second.1));
    (from <= to).then_some((from, to))
}

/// The first and the end of the frames of the clips of a clip group that stay where they are, and of those that are shifted (none, if there is
/// no clip of that kind in it).
struct MembersAfterTheMove {
    staying: Option<(i64, i64)>,
    shifted: Option<(i64, i64)>,
}

/// The shifts that take the clip `clip`, which is not in the clip group that has the clips `members` in it, inside the rectangle of the group:
/// the rectangle holds all of the members (so it begins where the first of them does, and ends where the last of them does, wherever they are
/// when the clips that are shifted have been), and the clip is inside when it has any frames of the rectangle (and is on one of its tracks, which
/// the caller has looked at: the shift does not change that). They are the shifts where the rectangle begins before the clip ends and the clip
/// begins before the rectangle ends, and each of those is so when it is for the members that stay or for those that are shifted: so there are at most
/// four stretches of them, found without trying any shift.
fn shifts_that_take_a_clip_inside(members: &MembersAfterTheMove, clip: &ClipAfterTheMove) -> Vec<Shifts> {
    let goes_along = clip.is_shifted;
    // (The rectangle begins before the clip ends. For the members that stay: where they begin is before where the clip ends then, and where it ends
    // is further on by the shift, if it goes along. For those that are shifted: where they begin is further on by the shift, and so is the end of the
    // clip if it goes along.)
    let rectangle_begins_before_the_clip_ends = [
        members.staying.map(|(first, _)| {
            if goes_along {
                When::ShiftIsAbove(first - clip.end)
            } else if first < clip.end {
                When::Always
            } else {
                When::Never
            }
        }),
        members.shifted.map(|(first, _)| {
            if !goes_along {
                When::ShiftIsBelow(clip.end - first)
            } else if first < clip.end {
                When::Always
            } else {
                When::Never
            }
        }),
    ];
    // (The clip begins before the rectangle ends.)
    let clip_begins_before_the_rectangle_ends = [
        members.staying.map(|(_, end)| {
            if goes_along {
                When::ShiftIsBelow(end - clip.first)
            } else if clip.first < end {
                When::Always
            } else {
                When::Never
            }
        }),
        members.shifted.map(|(_, end)| {
            if !goes_along {
                When::ShiftIsAbove(clip.first - end)
            } else if clip.first < end {
                When::Always
            } else {
                When::Never
            }
        }),
    ];
    let mut shifts = Vec::new();
    for begins_before in rectangle_begins_before_the_clip_ends.into_iter().flatten() {
        for begins_before_the_end in clip_begins_before_the_rectangle_ends.into_iter().flatten() {
            if let Some(found) = shifts_in_both(begins_before.shifts(), begins_before_the_end.shifts()) {
                shifts.push(found);
            }
        }
    }
    shifts
}

/// How far the clips can be shifted of the `frames`, as far as the rectangles of clip groups go, worked out without trying any shift: the
/// first shift (counting away from none) that takes a clip into a rectangle it is not in, or takes a rectangle round a clip that is not in it,
/// is where they stop short of. `None` if that cannot be said: the clips would go to a track that is not there, or the shift of none is itself
/// one that the rules of clip groups do not allow.
///
/// It is the rule of [`crate::project::Project::problems_with_the_groups`] that no clip is inside a clip group that it is not in, and only that
/// one: the others do not depend on the shift (and the clips that would be on one another are not what this is about: where the clips can go
/// without that is what [`allowed_frames`] has found).
fn stop_worked_out_by_arithmetic(project: &Project, moving: &[ClipId], tracks_down: i64, frames: i64) -> Option<i64> {
    let after = clips_after_the_move(project, moving, tracks_down)?;
    let mut not_allowed: Vec<Shifts> = Vec::new();
    for group in project
        .groups()
        .iter()
        .filter(|group| group.kind() == GroupKind::ClipGroup)
    {
        let members: HashSet<ClipId> = project.clips_in(group.id()).into_iter().collect();
        let mut staying: Option<(i64, i64)> = None;
        let mut shifted: Option<(i64, i64)> = None;
        let mut tracks: Option<(usize, usize)> = None;
        for clip in after.iter().filter(|clip| members.contains(&clip.id)) {
            let extent = if clip.is_shifted { &mut shifted } else { &mut staying };
            *extent = Some(match *extent {
                Some((first, end)) => (first.min(clip.first), end.max(clip.end)),
                None => (clip.first, clip.end),
            });
            tracks = Some(match tracks {
                Some((first, last)) => (first.min(clip.track_position), last.max(clip.track_position)),
                None => (clip.track_position, clip.track_position),
            });
        }
        let Some((first_track, last_track)) = tracks else {
            continue;
        };
        let members_after_the_move = MembersAfterTheMove { staying, shifted };
        for clip in after
            .iter()
            .filter(|clip| !members.contains(&clip.id) && (first_track..=last_track).contains(&clip.track_position))
        {
            not_allowed.extend(shifts_that_take_a_clip_inside(&members_after_the_move, clip));
        }
    }
    if not_allowed.iter().any(|(from, to)| *from <= 0 && 0 <= *to) {
        return None;
    }
    Some(if frames > 0 {
        let first_not_allowed = not_allowed
            .iter()
            .map(|(from, _)| *from)
            .filter(|from| *from >= 1)
            .min();
        first_not_allowed.map_or(frames, |first| frames.min(first - 1))
    } else {
        let last_not_allowed = not_allowed.iter().map(|(_, to)| *to).filter(|to| *to <= -1).max();
        last_not_allowed.map_or(frames, |last| frames.max(last + 1))
    })
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
mod group_tests;
#[cfg(test)]
mod snap_tests;
#[cfg(test)]
mod tests;
