// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving clips by dragging them: how far they can go.
//!
//! Dragging a clip moves it as far as the pointer goes along its track (and up or down to the other tracks),
//! until it comes to another clip that is not moving along with it (no two clips can be at the same time on
//! one track), or to the start of the timeline, or to what the rules of clip groups do not allow: a clip that is in no clip group and goes into
//! the rectangle of one **joins** it (see [`crate::project::Project::move_clips_remembering`]), unless that would make the rectangle take in a
//! clip that is not in the group, and one that is in a group cannot be moved so that the rectangle takes in a clip that is not in it, or into
//! another's (see [`stopped_by_the_groups`]). The clips in the way stop it, they are not pushed. A clip
//! goes to the track the pointer is over if there is room for it there, where it is along the timeline, and
//! otherwise stays on the track it is on: tracks that it does not fit on are not entered. A clip that goes to
//! another track can be held to the time it started at, as a magnet holds iron (see [`Snap`], and
//! [`StartTimeMemory`] for when it is). Everything here is arithmetic on frames and on the places of tracks in the
//! list of them, so that how far is allowed can be tested without a window.

use std::collections::{HashMap, HashSet};

use super::start_time_hold::{HeldTime, StartTimeMemory};
use crate::project::{
    ClipGroupBeforeTheDrag, ClipId, ClipMove, GroupId, GroupKind, Member, Project, TimelineFrameIndex, TrackId,
};

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
    allowed_move_holding(project, moving, wanted, None, &[])
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
///
/// `before_the_drag` says which clip group each of the clips was in when the drag began (see
/// [`crate::project::Project::clip_groups_before_a_drag_of`]; empty for a move that is not a drag): the clips that joined a clip group as
/// they were dragged are out of it again where the moves are tried, as the move itself makes them.
pub fn allowed_move_holding(
    project: &Project,
    moving: &[ClipId],
    wanted: MoveBy,
    snap: Option<Snap>,
    before_the_drag: &[ClipGroupBeforeTheDrag],
) -> MoveBy {
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
            || the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks, 0, before_the_drag)
            || frames_to_the_time_held_to(project, wanted, snap, tracks).is_some_and(|held| {
                the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks, held, before_the_drag)
            })
    })
    .unwrap_or(0);
    let frames = frames_to_the_time_held_to(project, wanted, snap, tracks_down).unwrap_or(wanted.frames);
    let frames = allowed_frames(project, moving, tracks_down, frames);
    MoveBy {
        tracks_down,
        frames: stopped_by_the_groups(project, moving, tracks_down, frames, before_the_drag),
    }
}

/// How many of the `frames` that the clips `moving` can be moved along, when they are on the tracks `tracks_down` below the ones they are on,
/// the rules of clip groups let them be moved: a clip that is in a clip group stops where its group would take in a clip that is not in it,
/// and a clip that is in none and goes into the rectangle of one joins it, and stops where that would make the rectangle take in a clip that is
/// not in the group (or where it would be in the rectangle of two groups that are not one in the other, or is in a group already, and so cannot
/// join). (Clips that are all of a group move along with its rectangle.) They are stopped as clips are by other clips: the first place along the
/// way where the move would not be allowed is where they stop, and they do not go on past it to a place where it would be allowed again, which
/// is what a drag that jumped over a rectangle would be.
///
/// Where there are no clip groups nothing is looked at, and the answer is `frames`. Otherwise, when the clips are allowed where they are
/// now, the first place that is not allowed is found without trying each of the places along the way (see
/// [`stop_found_where_the_rules_of_clip_groups_can_change`]), which takes the same time however far the clips are taken, and the answer is
/// checked: it must be a place that the move itself (see [`Project::move_clips_remembering`]) allows. If the clips are not allowed where they
/// are now (which is so when the tracks they go to were chosen for the time they are held to), or that cannot say, or what it says is not
/// allowed, each place along the way is tried as the move itself is, on a copy of the project, until one is not allowed.
fn stopped_by_the_groups(
    project: &Project,
    moving: &[ClipId],
    tracks_down: i64,
    frames: i64,
    before_the_drag: &[ClipGroupBeforeTheDrag],
) -> i64 {
    let has_clip_groups = project
        .groups()
        .iter()
        .any(|group| group.kind() == GroupKind::ClipGroup);
    if !has_clip_groups || frames == 0 {
        return frames;
    }
    let is_allowed =
        |shift: i64| the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks_down, shift, before_the_drag);
    if is_allowed(0)
        && let Some(stop) =
            stop_found_where_the_rules_of_clip_groups_can_change(project, moving, tracks_down, frames, before_the_drag)
        && is_allowed(stop)
    {
        return stop;
    }
    stopped_by_trying_each_place(project, moving, tracks_down, frames, before_the_drag)
}

/// [`stopped_by_the_groups`], found by trying each place along the way (and so taking longer the further the clips are taken, which is why it
/// is not what is done when there is another way): the answer for the tests of the other to be the same as.
fn stopped_by_trying_each_place(
    project: &Project,
    moving: &[ClipId],
    tracks_down: i64,
    frames: i64,
    before_the_drag: &[ClipGroupBeforeTheDrag],
) -> i64 {
    let is_allowed =
        |shift: i64| the_clips_fit_on_the_tracks_they_would_be_on(project, moving, tracks_down, shift, before_the_drag);
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

impl ClipAfterTheMove {
    /// The frames the clip lasts from and until when the clips that are shifted have been shifted `shift` frames along.
    fn frames_after(&self, shift: i64) -> (i64, i64) {
        if self.is_shifted {
            (self.first.saturating_add(shift), self.end.saturating_add(shift))
        } else {
            (self.first, self.end)
        }
    }
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

/// A clip group as the rules look at it when clips are moved.
struct ClipGroupAfterTheMove {
    /// The places in the list of clips of the clips that are in it, directly or in the clip groups that are in it: the clips that are moved
    /// and are in no clip group (which is the clips that can join one) are not among them, as they are in it by where they are.
    members: Vec<usize>,
    /// Whether the clip at each place in the list of clips is one of those.
    has: Vec<bool>,
    /// The places in the list of clip groups of the clip groups that it is in, one in another: the nearest first.
    round: Vec<usize>,
}

/// What the rules of clip groups look at, for a move of clips down some tracks, worked out once so that whether the rules are kept can be
/// said for any number of frames along without looking at the project again.
struct RulesOfClipGroupsAfterTheMove {
    clips: Vec<ClipAfterTheMove>,
    groups: Vec<ClipGroupAfterTheMove>,
    /// The places in the list of clips of the clips that are moved and are in no clip group (when the drag began, if it is a drag): each is
    /// put in the clip group whose rectangle it is in, if it is in one.
    joiners: Vec<usize>,
}

impl RulesOfClipGroupsAfterTheMove {
    /// The rules for moving the clips `moving` down `tracks_down` tracks, where `before_the_drag` says which clip group each was in when the
    /// drag began (the clip groups they are in now otherwise). `None` if one of them would go to a track that is not there.
    fn of(
        project: &Project,
        moving: &[ClipId],
        tracks_down: i64,
        before_the_drag: &[ClipGroupBeforeTheDrag],
    ) -> Option<Self> {
        let clips = clips_after_the_move(project, moving, tracks_down)?;
        let place_of: HashMap<ClipId, usize> = clips.iter().enumerate().map(|(place, clip)| (clip.id, place)).collect();
        let was_in_no_clip_group = |clip: ClipId| match before_the_drag.iter().find(|before| before.clip == clip) {
            Some(before) => before.group.is_none(),
            None => project.shape_group_of(Member::Clip(clip)).is_none(),
        };
        let joiners: Vec<usize> = clips
            .iter()
            .enumerate()
            .filter(|(_, clip)| clip.is_shifted && was_in_no_clip_group(clip.id))
            .map(|(place, _)| place)
            .collect();
        let clip_groups: Vec<GroupId> = project
            .groups()
            .iter()
            .filter(|group| group.kind() == GroupKind::ClipGroup)
            .map(|group| group.id())
            .collect();
        let groups = clip_groups
            .iter()
            .map(|group| {
                let mut has = vec![false; clips.len()];
                for clip in project.clips_in(*group) {
                    if let Some(&place) = place_of.get(&clip)
                        && !joiners.contains(&place)
                    {
                        has[place] = true;
                    }
                }
                let members = (0..clips.len()).filter(|place| has[*place]).collect();
                let mut round = Vec::new();
                let mut at = *group;
                while let Some(parent) = project.shape_group_of(Member::Group(at)) {
                    if round.len() > clip_groups.len() {
                        break;
                    }
                    round.extend(clip_groups.iter().position(|candidate| *candidate == parent.id()));
                    at = parent.id();
                }
                ClipGroupAfterTheMove { members, has, round }
            })
            .collect();
        Some(Self { clips, groups, joiners })
    }

    /// The rectangle of each clip group (the smallest that holds its members) when the clips that are moved are `shift` frames along: from
    /// the first frame to the end, and from the first track to the last. `None` for a group that has no members left (they are all joiners).
    fn rectangles(&self, shift: i64) -> Vec<Option<(i64, i64, usize, usize)>> {
        self.groups
            .iter()
            .map(|group| {
                group
                    .members
                    .iter()
                    .fold(None::<(i64, i64, usize, usize)>, |found, &member| {
                        let clip = &self.clips[member];
                        let (first, end) = clip.frames_after(shift);
                        Some(match found {
                            None => (first, end, clip.track_position, clip.track_position),
                            Some((found_first, found_end, first_track, last_track)) => (
                                found_first.min(first),
                                found_end.max(end),
                                first_track.min(clip.track_position),
                                last_track.max(clip.track_position),
                            ),
                        })
                    })
            })
            .collect()
    }

    /// Whether the rules of clip groups are kept when the clips that are moved are `shift` frames along: each of the clips that can join a
    /// group joins the innermost one whose rectangle it is in (which has to be one that all the others it is in the rectangle of are round),
    /// and then no clip is inside the rectangle of a group, the smallest that holds its members and the clips that joined it, without being
    /// in it.
    fn hold_for(&self, shift: i64) -> bool {
        let rectangles = self.rectangles(shift);
        let mut joined: Vec<Vec<usize>> = vec![Vec::new(); self.groups.len()];
        for &joiner in &self.joiners {
            let clip = &self.clips[joiner];
            let (first, end) = clip.frames_after(shift);
            let candidates: Vec<usize> = (0..self.groups.len())
                .filter(|group| {
                    rectangles[*group].is_some_and(|(group_first, group_end, first_track, last_track)| {
                        first < group_end
                            && group_first < end
                            && (first_track..=last_track).contains(&clip.track_position)
                    })
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let has_an_innermost = candidates.iter().any(|candidate| {
                candidates
                    .iter()
                    .all(|other| other == candidate || self.groups[*candidate].round.contains(other))
            });
            if !has_an_innermost {
                return false;
            }
            for candidate in candidates {
                joined[candidate].push(joiner);
            }
        }
        for (index, group) in self.groups.iter().enumerate() {
            let Some((first, end, first_track, last_track)) = rectangles[index] else {
                continue;
            };
            let (first, end) = joined[index].iter().fold((first, end), |(first, end), joiner| {
                let (joiner_first, joiner_end) = self.clips[*joiner].frames_after(shift);
                (first.min(joiner_first), end.max(joiner_end))
            });
            for (place, clip) in self.clips.iter().enumerate() {
                if group.has[place]
                    || joined[index].contains(&place)
                    || !(first_track..=last_track).contains(&clip.track_position)
                {
                    continue;
                }
                let (clip_first, clip_end) = clip.frames_after(shift);
                if clip_first < end && first < clip_end {
                    return false;
                }
            }
        }
        true
    }

    /// The shifts, from the one nearest to none towards `frames` and as far as it, that the answer to [`Self::hold_for`] can change at: each
    /// comparison it makes is of an edge of a clip that is shifted with an edge of one that is not (two that are shifted are as far from one
    /// another for every shift), and is different on the two sides of the shift where the edges are the same, which is where they are one
    /// frame apart as well. `frames` itself is the last.
    fn shifts_where_the_answer_can_change(&self, frames: i64) -> Vec<i64> {
        let direction = frames.signum();
        let tracks_in_a_rectangle: HashSet<usize> = self
            .rectangles(0)
            .into_iter()
            .flatten()
            .flat_map(|(_, _, first_track, last_track)| first_track..=last_track)
            .collect();
        let moved_edges: Vec<i64> = self
            .clips
            .iter()
            .filter(|clip| clip.is_shifted)
            .flat_map(|clip| [clip.first, clip.end])
            .collect();
        let staying_edges: Vec<i64> = self
            .clips
            .iter()
            .filter(|clip| !clip.is_shifted && tracks_in_a_rectangle.contains(&clip.track_position))
            .flat_map(|clip| [clip.first, clip.end])
            .collect();
        let is_on_the_way = |shift: i64| shift.signum() == direction && shift.abs() <= frames.abs();
        let mut shifts: Vec<i64> = Vec::new();
        for moved in &moved_edges {
            for staying in &staying_edges {
                let together = staying.saturating_sub(*moved);
                shifts.extend(
                    [together.saturating_sub(1), together.saturating_add(1)]
                        .into_iter()
                        .filter(|shift| is_on_the_way(*shift)),
                );
            }
        }
        shifts.sort_unstable_by_key(|shift| shift.abs());
        shifts.dedup();
        shifts
    }
}

/// How far the clips can be shifted of the `frames`, as far as the rules of clip groups go, found by looking at the places where whether they
/// are kept can change and not at each place along the way: the first of them (counting away from none) where they are not kept is where the
/// clips stop short of. `None` if that cannot be said: the clips would go to a track that is not there, or the shift of none is itself one
/// that the rules do not allow.
///
/// It is the rule of [`crate::project::Project::problems_with_the_groups`] that no clip is inside a clip group that it is not in, and only that
/// one: the others do not depend on the shift (and the clips that would be on one another are not what this is about: where the clips can go
/// without that is what [`allowed_frames`] has found).
fn stop_found_where_the_rules_of_clip_groups_can_change(
    project: &Project,
    moving: &[ClipId],
    tracks_down: i64,
    frames: i64,
    before_the_drag: &[ClipGroupBeforeTheDrag],
) -> Option<i64> {
    let rules = RulesOfClipGroupsAfterTheMove::of(project, moving, tracks_down, before_the_drag)?;
    if !rules.hold_for(0) {
        return None;
    }
    let direction = frames.signum();
    for shift in rules.shifts_where_the_answer_can_change(frames) {
        if !rules.hold_for(shift) {
            // (What is between this shift and the one before it, if that is not the one next to it, is as that is: the answer does not change
            // where it is not at one of the shifts.)
            return Some(shift - direction);
        }
    }
    Some(frames)
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

/// Whether the clips `moving` can be moved down `tracks_down` tracks and `frames` frames along (as the clips that were in the groups
/// `before_the_drag` says when the drag began, if it is a drag).
fn the_clips_fit_on_the_tracks_they_would_be_on(
    project: &Project,
    moving: &[ClipId],
    tracks_down: i64,
    frames: i64,
    before_the_drag: &[ClipGroupBeforeTheDrag],
) -> bool {
    let moves = moves_by(project, moving, MoveBy { tracks_down, frames });
    project.clone().move_clips_remembering(&moves, before_the_drag).is_ok()
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
