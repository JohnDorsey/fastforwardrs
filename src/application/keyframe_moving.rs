//! Moving keyframes by dragging them: how far they can go.
//!
//! Dragging a keyframe moves it as far as the pointer goes along the timeline (as far as the drawer its rack is in has frames for,
//! stopping short of the keyframes that are in its way), and up and down to the other racks that animate the same property of the
//! same thing (the racks it can go to), the way clips are dragged to other tracks (see [`super::clip_moving`]): a keyframe goes
//! to the rack the pointer is over if it fits in that where it is along the timeline, and otherwise stays in the rack it is in.
//! All the keyframes that are dragged go the same number of racks down (up, if negative) in the racks they each can go to, and
//! the same number of frames, so that they keep their places in relation to one another. A keyframe that is dragged to another
//! rack can be held to the frame it started at, as a magnet holds iron (see [`super::start_time_hold`]).
//!
//! Everything here is arithmetic on frames and on the places of racks in lists of them, so that how far is allowed can be tested
//! without a window.

use super::start_time_hold::HeldTime;
use crate::project::{KeyframeDestination, KeyframeId, KeyframeMove, Project, RackId};

/// How far to move keyframes: down that many racks in the list of those each can go to (up, if negative), and later by that many
/// frames of the timeline (earlier, if negative).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveBy {
    pub racks_down: i64,
    pub frames: i64,
}

/// What holds the keyframe that was taken hold of to the frame it started at, if it is near it, while it is in another rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    /// The keyframe that was taken hold of. The others that are dragged with it go by as much as it does.
    pub keyframe: KeyframeId,
    pub time: HeldTime<RackId>,
}

/// How far some keyframes can be moved, and the moves that do it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedMove {
    pub by: MoveBy,
    /// The moves that put each of the keyframes `by` as far from where it is.
    pub moves: Vec<KeyframeMove>,
}

/// One of the keyframes that are moving, and the racks it can go to.
struct Lane {
    keyframe: KeyframeId,
    /// The racks the keyframe can go to, as they are one under another on the screen, the top one first. It is one of them.
    racks: Vec<RackId>,
    /// The place of the rack the keyframe is in, in `racks`.
    own: usize,
}

impl Lane {
    /// The rack that the keyframe goes to when it goes `racks_down` racks down, if there is one.
    fn rack_after(&self, racks_down: i64) -> Option<RackId> {
        let position = i64::try_from(self.own).ok()?.checked_add(racks_down)?;
        self.racks.get(usize::try_from(position).ok()?).copied()
    }
}

/// The lanes of the keyframes `moving` that are in the project. `racks_it_can_go_to` gives, for a rack, the racks that a keyframe
/// in it can go to, in the order they are in on the screen: a keyframe that is in a rack that it does not list can only stay in it.
fn lanes_of(project: &Project, moving: &[KeyframeId], racks_it_can_go_to: &dyn Fn(RackId) -> Vec<RackId>) -> Vec<Lane> {
    moving
        .iter()
        .filter_map(|keyframe| {
            let own_rack = project.locate_keyframe(*keyframe)?.rack.rack.id();
            let mut racks = racks_it_can_go_to(own_rack);
            let own = match racks.iter().position(|rack| *rack == own_rack) {
                Some(position) => position,
                None => {
                    racks = vec![own_rack];
                    0
                }
            };
            Some(Lane {
                keyframe: *keyframe,
                racks,
                own,
            })
        })
        .collect()
}

/// Where each of the keyframes goes when they all go `racks_down` racks down. `None` if one of them has no rack that far down.
fn destinations_after(lanes: &[Lane], racks_down: i64) -> Option<Vec<KeyframeDestination>> {
    lanes
        .iter()
        .map(|lane| {
            Some(KeyframeDestination {
                keyframe: lane.keyframe,
                rack: lane.rack_after(racks_down)?,
            })
        })
        .collect()
}

/// Whether the keyframes can be moved to the racks `destinations` say, `shift` frames later (earlier, if negative) than they are.
fn they_fit(project: &Project, destinations: &[KeyframeDestination], shift: i64) -> bool {
    let moves = project.moves_of_keyframes_by(destinations, shift);
    moves.len() == destinations.len() && project.check_the_move_of_keyframes(&moves).is_ok()
}

/// How far the keyframes `moving` can be moved when nothing holds any of them to a time: [`allowed_move_holding`] without a
/// [`Hold`], which is what the tests of how far keyframes go use.
#[cfg(test)]
pub fn allowed_move(
    project: &Project,
    moving: &[KeyframeId],
    racks_it_can_go_to: &dyn Fn(RackId) -> Vec<RackId>,
    wanted: MoveBy,
) -> AllowedMove {
    allowed_move_holding(project, moving, racks_it_can_go_to, wanted, None)
}

/// How far the keyframes `moving` can be moved when `wanted` is how far the pointer has gone: `wanted` itself, or as much of it as
/// can be done. All the keyframes move by the same, so two of them that are moving never come to one another by it: only the
/// keyframes that are not moving stop them.
///
/// The racks come first. The keyframes go down or up as many racks as are wanted, but not past the first or the last that each can
/// go to, and if they do not fit in the racks they would then be in (where they are along the timeline), as many fewer as they do
/// fit in. Then they are moved along the timeline as far as the drawers they are in and the keyframes that stay in those racks
/// leave room for.
///
/// With the keyframe that was taken hold of held to the frame of the [`Hold`] when it is near that, where it is held it is wanted
/// at that frame and not where the pointer would put it, and so it fits in a rack if it fits there at that frame, though it would
/// not at the frame it is at now.
pub fn allowed_move_holding(
    project: &Project,
    moving: &[KeyframeId],
    racks_it_can_go_to: &dyn Fn(RackId) -> Vec<RackId>,
    wanted: MoveBy,
    hold: Option<Hold>,
) -> AllowedMove {
    let lanes = lanes_of(project, moving, racks_it_can_go_to);
    if lanes.is_empty() {
        // None of them is there, so nothing stops them.
        return AllowedMove {
            by: MoveBy {
                racks_down: 0,
                frames: wanted.frames,
            },
            moves: Vec::new(),
        };
    }
    let to_signed = |number: usize| i64::try_from(number).unwrap_or(i64::MAX);
    let furthest_up = -lanes.iter().map(|lane| to_signed(lane.own)).min().unwrap_or(0);
    let furthest_down = lanes
        .iter()
        .map(|lane| to_signed(lane.racks.len() - 1 - lane.own))
        .min()
        .unwrap_or(0);
    // The frames that put the keyframe that was held at the frame it is held to, if it is held when the keyframes go down so many
    // racks.
    let shift_held_to = |racks_down: i64| -> Option<i64> {
        let hold = hold?;
        let lane = lanes.iter().find(|lane| lane.keyframe == hold.keyframe)?;
        let now = project.timeline_frame_of_keyframe(hold.keyframe)?;
        hold.time.shift_to_it(lane.rack_after(racks_down)?, now, wanted.frames)
    };
    let wanted_racks = wanted.racks_down.clamp(furthest_up, furthest_down);
    // From as many racks as were wanted back to none, the first that they fit in.
    let racks_down = std::iter::successors(Some(wanted_racks), |&racks| {
        (racks != 0).then(|| racks - racks.signum())
    })
    .find(|&racks| {
        racks == 0
            || destinations_after(&lanes, racks).is_some_and(|destinations| {
                they_fit(project, &destinations, 0)
                    || shift_held_to(racks).is_some_and(|held| they_fit(project, &destinations, held))
            })
    })
    .unwrap_or(0);
    let destinations = destinations_after(&lanes, racks_down).unwrap_or_default();
    let frames = project.allowed_shift_of_keyframes(&destinations, shift_held_to(racks_down).unwrap_or(wanted.frames));
    AllowedMove {
        by: MoveBy { racks_down, frames },
        moves: project.moves_of_keyframes_by(&destinations, frames),
    }
}

/// The moves that put each of the keyframes `moving`, which are as far down the racks they can go to as `racks_down` says, `frames`
/// later than they are: for when how far they are to go is not what [`allowed_move_holding`] found for them but less, because
/// what is dragged with them is stopped sooner.
pub fn moves_by(
    project: &Project,
    moving: &[KeyframeId],
    racks_it_can_go_to: &dyn Fn(RackId) -> Vec<RackId>,
    by: MoveBy,
) -> Vec<KeyframeMove> {
    let lanes = lanes_of(project, moving, racks_it_can_go_to);
    let destinations = destinations_after(&lanes, by.racks_down).unwrap_or_default();
    project.moves_of_keyframes_by(&destinations, by.frames)
}

#[cfg(test)]
mod tests;
