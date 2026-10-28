// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the ring of a clip or of an instance of a track's effects goes at the edges of groups (phase 12, piece 12.3d; section 7 of docs/engine-phase-12-design.md): the **tail policies**.
//!
//! What a clip's effects go on making after the clip ends (its **ring**) is sound that belongs to the clip. While it is in the group that the clip is in, it is in the sum of that group, and
//! the group's effects do their work on it (that is *keep alive*, and it is what happens if nobody says otherwise: [`tail_routes_of`] says nothing and the ring goes where the clip does).
//! The policies of a group and of a clip change where it goes, at some frame of the timeline:
//!
//! * at the **edge of a group that the ring is in** (the frame its last member ends at), the policy of the group says: *keep alive* (it stays, and nothing after it changes: the ring is
//!   in the instance of the group's effects, and what that makes belongs to the group), *release* (it leaves the group, unprocessed by it, and is in the group round that one, or in the sum
//!   of all the sound), or *cut* (it ends there, with the edge fade);
//! * where the ring **reaches the first frame of a group that it is not in** and that has the track of the clip across its rectangle, the policy of the clip says: *around* (nothing
//!   changes), *into* (the group adopts it: it is in the group from then on, and the group's edge, at the frame its last member ends, is the next edge the ring meets) or *cut* (it
//!   ends there, with the edge fade).
//!
//! The answer is a list of **routes**: from this frame on the ring goes to this group (or to the sum of all the sound, or nowhere). The first stretch, the clip itself, goes where the
//! plan says the clip is sent. Where nothing changes the list is empty.

use super::SoundGroup;
use crate::project::{ClipTailEdge, GroupTailEdge, Rectangle};

/// Where the sound goes from some frame on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailDestination {
    /// Into the sum of this group of `SoundPlan::groups`.
    Group(usize),
    /// Into the sum of all the sound.
    Sum,
    /// Nowhere: it has ended (it fades out over the edge fade, and then it is not heard).
    Cut,
}

/// From this frame of the timeline on, the ring goes to `to` (and from the frame of the next route, to that). Over the edge fade that follows `from_frame` the sound goes to both: less and
/// less to where it was going and more and more to `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TailRoute {
    pub from_frame: u64,
    pub to: TailDestination,
}

/// What the policies of a group need of it: where it is on the timeline, and what it does with the tails of its members at its edge.
#[derive(Clone, Copy, Debug)]
pub(super) struct GroupGeometry {
    pub rectangle: Rectangle,
    pub tail_edge: GroupTailEdge,
}

/// Something that rings: a clip that is not heard by an instance of its track's effects, or such an instance.
#[derive(Clone, Copy, Debug)]
pub(super) struct Emitter {
    /// The place of its track in the list of tracks.
    pub row: usize,
    /// The frame its ring begins at (where it ends) and how many frames it lasts for at most.
    pub ring_from: u64,
    pub ring_frames: u64,
    /// The group it is sent to.
    pub sent_to: Option<usize>,
    /// What the clip says (for an instance of a track's effects: the last clip it hears, whose the tail of a run is).
    pub policy: ClipTailEdge,
}

#[derive(Clone, Copy, Debug)]
enum Event {
    /// The ring reaches the edge of this group, where its last member ends.
    EdgeOf(usize),
    /// The ring reaches the first frame of this group, which it is not in.
    EntersTheRectangleOf(usize),
}

/// Whether the sound that is in `at` (a group, or the sum of all the sound) is in the group `group`: it is in it, or in a group that is in it.
fn is_in(groups: &[SoundGroup], mut at: Option<usize>, group: usize) -> bool {
    for _ in 0..=groups.len() {
        match at {
            Some(found) if found == group => return true,
            Some(found) => at = groups[found].parent,
            None => return false,
        }
    }
    false
}

fn destination_of(at: Option<usize>) -> TailDestination {
    at.map_or(TailDestination::Sum, TailDestination::Group)
}

/// The routes of the ring of `emitter`, in the order of the frames they begin at. Empty where the ring goes where its clip does all the way (which is what no policy does).
pub(super) fn tail_routes_of(groups: &[SoundGroup], geometry: &[GroupGeometry], emitter: &Emitter) -> Vec<TailRoute> {
    if emitter.ring_frames == 0 {
        return Vec::new();
    }
    let ring_end = emitter.ring_from + emitter.ring_frames;
    let edges_above = |start: Option<usize>, events: &mut Vec<(u64, Event)>| {
        let mut at = start;
        for _ in 0..=groups.len() {
            let Some(group) = at else { break };
            events.push((geometry[group].rectangle.end_frame, Event::EdgeOf(group)));
            at = groups[group].parent;
        }
    };
    let mut events: Vec<(u64, Event)> = Vec::new();
    edges_above(emitter.sent_to, &mut events);
    for (group, found) in geometry.iter().enumerate() {
        let rectangle = found.rectangle;
        if !is_in(groups, emitter.sent_to, group)
            && rectangle.reaches_the_track_at(emitter.row)
            && emitter.ring_from <= rectangle.first_frame
            && rectangle.first_frame < ring_end
        {
            events.push((rectangle.first_frame, Event::EntersTheRectangleOf(group)));
        }
    }

    let mut routes: Vec<TailRoute> = Vec::new();
    let mut at = emitter.sent_to;
    let mut edges_met: Vec<usize> = Vec::new();
    while let Some(next) = (0..events.len()).min_by_key(|&place| {
        // (At one frame the edge of a group that the ring is in comes before the edge of one that it enters.)
        (
            events[place].0,
            matches!(events[place].1, Event::EntersTheRectangleOf(_)),
        )
    }) {
        let (frame, event) = events.remove(next);
        if frame >= ring_end {
            break;
        }
        match event {
            Event::EdgeOf(group) => {
                if !is_in(groups, at, group) || edges_met.contains(&group) {
                    continue;
                }
                edges_met.push(group);
                match geometry[group].tail_edge {
                    // The ring is in the instance of the effects of the group, and what that makes is the group's: nothing after this changes where it goes.
                    GroupTailEdge::KeepAlive => break,
                    GroupTailEdge::Release => {
                        at = groups[group].parent;
                        routes.push(TailRoute {
                            from_frame: frame,
                            to: destination_of(at),
                        });
                    }
                    GroupTailEdge::Cut => {
                        routes.push(TailRoute {
                            from_frame: frame,
                            to: TailDestination::Cut,
                        });
                        break;
                    }
                }
            }
            Event::EntersTheRectangleOf(group) => {
                if is_in(groups, at, group) {
                    continue;
                }
                match emitter.policy {
                    ClipTailEdge::Around => {}
                    ClipTailEdge::Into => {
                        at = Some(group);
                        routes.push(TailRoute {
                            from_frame: frame,
                            to: TailDestination::Group(group),
                        });
                        // (The edges of this group, and of those it is in, are the ones the ring meets from here on.)
                        edges_above(Some(group), &mut events);
                    }
                    ClipTailEdge::Cut => {
                        routes.push(TailRoute {
                            from_frame: frame,
                            to: TailDestination::Cut,
                        });
                        break;
                    }
                }
            }
        }
    }
    routes
}
