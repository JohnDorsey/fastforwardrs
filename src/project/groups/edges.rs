//! Moving the left or the right edge of a clip group: what is in it is what is inside the frames it is made to have.
//!
//! The rectangle of a clip group is never kept, it is worked out from what is in the group (see [`super::rectangles`]): the smallest that holds
//! it. So "making the group wider or narrower" is making it have more things in it, or fewer: moving the edge of the rectangle to a frame
//! **takes in** the clips (or the clip groups) that are then inside the frames, on the tracks the group reaches across, and **lets go of** those
//! of its own that are then outside them. An edge cannot be put where it would cut across a clip or a clip group (part of it inside the frames,
//! and part outside), and the rules of groups have to be kept (see [`super::rules`]) with the clips that are taken in: they are not in another
//! clip group, or are in one that is round this one (they leave it and are in this one), and the rectangle that is made does not have a clip that
//! is not in the group in it.
//!
//! After the edge is let go the rectangle is the smallest that holds what is in the group, so a group that is made wider than what it holds
//! (by an edge that was put where there is nothing to take in) is not wider: it fits what is in it.

use std::ops::RangeInclusive;

use super::{GroupId, GroupKind, Member};
use crate::project::tracks::ClipId;
use crate::project::{EditError, Project, Rectangle};

/// Which side of the rectangle of a clip group an edge is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipGroupEdge {
    /// The edge before the first frame of the group.
    Left,
    /// The edge after the last frame of the group.
    Right,
}

/// A clip, or a clip group, that is in the way of an edge or is taken in by it: what is moved into or out of a group as a whole.
#[derive(Clone, Copy, Debug)]
struct Unit {
    member: Member,
    rectangle: Rectangle,
}

impl Project {
    /// The frames that an edge of the clip group `group` can be moved to, which the group is then given: the places for the left edge are
    /// its first frame, for the right edge the frame after its last (the first frame of the timeline is 0). The places are in ranges, in the
    /// order of the frames and none beside another, and none is for an edge that would cut across a clip or a clip group, or that would
    /// break a rule of groups (an edge that would leave nothing in the group does: it is not put where it would have no frame between it
    /// and the other); they go on for ever after the last clip for the right edge.
    pub fn places_for_an_edge_of_the_clip_group(
        &self,
        group: GroupId,
        edge: ClipGroupEdge,
    ) -> Vec<RangeInclusive<u64>> {
        let Some(rectangle) = self.clip_group_rectangle(group) else {
            return Vec::new();
        };
        let units = self.units_on_the_tracks_of_the_clip_group(group, &rectangle);
        // Where something begins or ends (the group's own edges are among the edges of what is in it), and the frames the edge could be at
        // in the places between them (none of them has anything that begins or ends in it): what is on each side of the edge is the same for
        // each place of one of those.
        let mut events: Vec<u64> = units
            .iter()
            .flat_map(|unit| [unit.rectangle.first_frame, unit.rectangle.end_frame])
            .collect();
        events.sort_unstable();
        events.dedup();
        let mut places: Vec<RangeInclusive<u64>> = Vec::new();
        let mut regions: Vec<RangeInclusive<u64>> = Vec::new();
        // The first frame of the timeline up to the first event, each event, and the places between one event and the next.
        let mut from = 0;
        for event in &events {
            if *event > from {
                regions.push(from..=*event - 1);
            }
            regions.push(*event..=*event);
            from = *event + 1;
        }
        regions.push(from..=u64::MAX);
        for region in regions {
            // (A region is cut at every frame where something begins or ends, so it is all strictly inside a clip or a clip group, where an
            // edge cuts across it, or none of it is.)
            let start = *region.start();
            if units
                .iter()
                .any(|unit| unit.rectangle.first_frame < start && start < unit.rectangle.end_frame)
            {
                continue;
            }
            // What is on each side is the same all along the region, so the rules need to be looked at in one place only.
            let (first_frame, end_frame) = match edge {
                ClipGroupEdge::Left => (start, rectangle.end_frame),
                ClipGroupEdge::Right => (rectangle.first_frame, start),
            };
            if self
                .would_be_allowed(|project| project.set_the_frames_of_the_clip_group(group, first_frame, end_frame))
                .is_ok()
            {
                match places.last_mut() {
                    Some(last) if *last.end() + 1 == start => *last = *last.start()..=*region.end(),
                    _ => places.push(region),
                }
            }
        }
        places
    }

    /// Makes the clip group `group` have the clips and the clip groups that are inside the frames `first_frame` up to (not including)
    /// `end_frame`, on the tracks it reaches across: those that were not in it are put in it (taken out of the clip group that is round it
    /// that they are directly in, if they are in one), and those that were and are not inside the frames are taken out of it (and put in the
    /// clip group that is round it, if they are still inside its rectangle). It is an error, and nothing is changed, if the group is not a clip
    /// group, if the frames cut across a clip or a clip group that is on those tracks (part of it inside them and part outside), if nothing
    /// would be left in the group, or if that would break a rule of groups.
    pub fn set_the_frames_of_the_clip_group(
        &mut self,
        group: GroupId,
        first_frame: u64,
        end_frame: u64,
    ) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            let rectangle = project.clip_group_rectangle_or_error(group)?;
            let units = project.units_on_the_tracks_of_the_clip_group(group, &rectangle);
            let mut inside: Vec<Member> = Vec::new();
            for unit in &units {
                let is_inside = first_frame <= unit.rectangle.first_frame && unit.rectangle.end_frame <= end_frame;
                let overlaps = unit.rectangle.has_any_of_the_frames(first_frame, end_frame);
                if overlaps && !is_inside {
                    return Err(EditError::TheEdgeWouldCutAcross { member: unit.member });
                }
                if is_inside {
                    inside.push(unit.member);
                }
            }
            let parents = project.parents();
            let ancestors = project.shapes_round(&parents, Member::Group(group));
            let before: Vec<Member> = project
                .group(group)
                .map(|found| found.members().to_vec())
                .unwrap_or_default();
            let (staying, leaving): (Vec<Member>, Vec<Member>) =
                before.iter().partition(|member| inside.contains(member));
            let joining: Vec<Member> = inside
                .iter()
                .copied()
                .filter(|member| !before.contains(member))
                .collect();
            if staying.is_empty() && joining.is_empty() {
                return Err(EditError::NothingWouldBeLeftInTheGroup { group });
            }
            // What joins is taken out of the group round this one that it is directly in (it is in this one, which is in that one).
            for member in &joining {
                if let Some(parent) = parents.shape_of(*member)
                    && let Some(found) = project.group_mut(parent)
                {
                    found.members_mut().retain(|other| other != member);
                }
            }
            if let Some(found) = project.group_mut(group) {
                found.members_mut().retain(|member| !leaving.contains(member));
                found.members_mut().extend(joining.iter().copied());
            }
            // What was let go of stays in the clip group round this one, as far as it is still inside its rectangle.
            if let Some(&round) = ancestors.first() {
                let mut waiting: Vec<Member> = leaving.clone();
                while let Some(round_rectangle) = project.rectangle_of_the_group(round) {
                    let Some(place) = waiting.iter().position(|member| {
                        project
                            .rectangle_of_the_member(*member)
                            .is_some_and(|member_rectangle| member_rectangle.is_on_top_of(&round_rectangle))
                    }) else {
                        break;
                    };
                    let member = waiting.remove(place);
                    if let Some(found) = project.group_mut(round) {
                        found.members_mut().push(member);
                    }
                }
            }
            Ok(())
        })
    }

    /// The rectangle of the clip group `group`, or why there is none (it is not a clip group, or has nothing in it).
    fn clip_group_rectangle_or_error(&self, group: GroupId) -> Result<Rectangle, EditError> {
        let found = self.group(group).ok_or(EditError::UnknownGroup { group })?;
        if found.kind() != GroupKind::ClipGroup {
            return Err(EditError::NotAClipGroup { group });
        }
        self.rectangle_of_the_group(group)
            .ok_or(EditError::NothingWouldBeLeftInTheGroup { group })
    }

    fn clip_group_rectangle(&self, group: GroupId) -> Option<Rectangle> {
        self.clip_group_rectangle_or_error(group).ok()
    }

    /// The rectangle of a clip or a clip group, if there is one.
    fn rectangle_of_the_member(&self, member: Member) -> Option<Rectangle> {
        match member {
            Member::Clip(clip) => self.rectangle_of_the_clip(clip),
            Member::Group(group) => self.rectangle_of_the_group(group),
            Member::Track(track) => self.rectangle_of_the_track(track),
        }
    }

    /// What is on the tracks that the clip group `group` reaches across, each as a whole: a clip that is not in a clip group is a unit, a clip
    /// group that is not round `group` and is not one of those in it is one, with all that is in it (the largest of those that a clip is in),
    /// and a member of `group` is one. (Each is there once, with where it is: the clips of the tracks, in the order of the tracks and of the
    /// frames, that are not in a unit that was found before.)
    fn units_on_the_tracks_of_the_clip_group(&self, group: GroupId, rectangle: &Rectangle) -> Vec<Unit> {
        let parents = self.parents();
        let ancestors = self.shapes_round(&parents, Member::Group(group));
        let mut units: Vec<Unit> = Vec::new();
        for position in rectangle.tracks() {
            let Some(track) = self.tracks.get(position) else {
                continue;
            };
            for placement in track.placements() {
                let member = self.the_unit_of_a_clip(placement.id, group, &parents, &ancestors);
                if units.iter().any(|unit| unit.member == member) {
                    continue;
                }
                if let Some(rectangle) = self.rectangle_of_the_member(member) {
                    units.push(Unit { member, rectangle });
                }
            }
        }
        units
    }

    /// What a clip is as far as the clip group `group` is concerned: the member of `group` that it is in (the clip, or the clip group that
    /// has it in), if it is in `group`; and otherwise the clip group that it is in that is the largest of those that are not round `group`
    /// (those that are not one in another that is round it), or the clip, if it is in none.
    fn the_unit_of_a_clip(
        &self,
        clip: ClipId,
        group: GroupId,
        parents: &super::queries::Parents,
        ancestors: &[GroupId],
    ) -> Member {
        let round = self.shapes_round(parents, Member::Clip(clip));
        if let Some(place) = round.iter().position(|found| *found == group) {
            return if place == 0 {
                Member::Clip(clip)
            } else {
                Member::Group(round[place - 1])
            };
        }
        let not_round_it: Vec<GroupId> = round
            .iter()
            .copied()
            .take_while(|found| !ancestors.contains(found))
            .collect();
        match not_round_it.last() {
            Some(largest) => Member::Group(*largest),
            None => Member::Clip(clip),
        }
    }
}

#[cfg(test)]
mod tests;
