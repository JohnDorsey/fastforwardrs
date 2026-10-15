// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rules that a project's groups have to keep, written once, as a check that finds every way they are broken.
//!
//! Nothing here changes anything. The editing of groups, and every edit of clips and tracks that could break a rule, is made so that
//! it is taken back if this check finds something afterwards (see [`super::upkeep`]); a project that is loaded is checked by it too, and
//! so are the tests of edits that are made in random order.
//!
//! The rules are:
//!
//! * every group has something in it, and what is in it is in the project, once, and is a kind of thing that a group of that kind can
//!   hold (a tag: clips and groups; a clip group: clips and clip groups; a track group: tracks and track groups);
//! * a group is not in itself, through other groups or not;
//! * a clip, a track or a group is directly in one clip group or track group at most (it is inside one shape, and so in one chain of
//!   shapes), and in any number of tags;
//! * where a clip or a group is in a tag, its chain of effects has the badge of the tag, once, and a chain has no badge of a tag that
//!   does not have what has the chain in it (and a track or a group has no badge of the track);
//! * **no clip that is not in a clip group, or in a clip group that is in it, is inside its rectangle** (the smallest that holds its
//!   members, see [`super::rectangles`]);
//! * a track group has next to each other in the list of tracks, with no track between that is not in it, the tracks that are in it,
//!   directly or in the track groups that are in it;
//! * a clip group that has tracks in common with a track group is inside it: its tracks are all tracks of the track group (it is not
//!   across the edge of a track group, and a track group is not in the rectangle of a clip group).

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use super::{Group, GroupId, GroupKind, Member};
use crate::project::Project;
use crate::project::tracks::{ClipId, TrackId};

/// One way in which the groups of a project break a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum GroupProblem {
    #[error("{group} has {member} in it, which the project does not have")]
    HasAMemberThatIsNotThere { group: GroupId, member: Member },
    #[error("{group} has {member} in it twice")]
    HasAMemberTwice { group: GroupId, member: Member },
    #[error("{group} is a {kind}, which cannot have {member} in it")]
    CannotHold {
        group: GroupId,
        kind: GroupKind,
        member: Member,
    },
    #[error(
        "{member} is in {first} and in {second}, which it cannot be: it can only be in one clip group or track group"
    )]
    InTwoShapes {
        member: Member,
        first: GroupId,
        second: GroupId,
    },
    #[error("{group} is in itself")]
    IsInItself { group: GroupId },
    #[error("{group} has nothing in it")]
    HasNothingInIt { group: GroupId },
    #[error("{member} is in {group}, and has no badge of it in its chain of effects")]
    HasNoBadgeOfTheGroupItIsIn { member: Member, group: GroupId },
    #[error("{owner} has the badge of {group} in its chain of effects, and is not in it")]
    HasTheBadgeOfAGroupItIsNotIn { owner: Member, group: GroupId },
    #[error("{owner} has the badge of its track in its chain of effects, and only a clip can")]
    HasTheBadgeOfATrackAndIsNotAClip { owner: Member },
    #[error("{clip} is inside the rectangle of {group}, and is not in it")]
    IsInsideAClipGroupWithoutBeingInIt { group: GroupId, clip: ClipId },
    #[error("{track} is between the tracks of {group}, and is not in it")]
    IsBetweenTheTracksOfATrackGroupWithoutBeingInIt { group: GroupId, track: TrackId },
    #[error("{clip_group} reaches across the edge of {track_group}: part of it is inside, and part is not")]
    ClipGroupReachesAcrossTheEdgeOfATrackGroup { clip_group: GroupId, track_group: GroupId },
}

impl GroupProblem {
    /// The group that the problem is about, if it is about one.
    pub fn group(&self) -> Option<GroupId> {
        match self {
            Self::HasAMemberThatIsNotThere { group, .. }
            | Self::HasAMemberTwice { group, .. }
            | Self::CannotHold { group, .. }
            | Self::IsInItself { group }
            | Self::HasNothingInIt { group }
            | Self::HasNoBadgeOfTheGroupItIsIn { group, .. }
            | Self::HasTheBadgeOfAGroupItIsNotIn { group, .. }
            | Self::IsInsideAClipGroupWithoutBeingInIt { group, .. }
            | Self::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt { group, .. } => Some(*group),
            Self::InTwoShapes { second, .. } => Some(*second),
            Self::ClipGroupReachesAcrossTheEdgeOfATrackGroup { clip_group, .. } => Some(*clip_group),
            Self::HasTheBadgeOfATrackAndIsNotAClip { .. } => None,
        }
    }
}

impl Project {
    /// Every rule that the groups of the project break: none, in a project that is as it should be.
    pub fn problems_with_the_groups(&self) -> Vec<GroupProblem> {
        let mut problems = Vec::new();
        if self.groups.is_empty() {
            return problems;
        }
        self.look_at_what_is_in_the_groups(&mut problems);
        self.look_for_groups_in_themselves(&mut problems);
        self.look_at_the_badges(&mut problems);
        // (Where things are is only looked at when what is in what makes sense, which the rectangles are worked out from.)
        if problems.is_empty() {
            self.look_at_the_shapes(&mut problems);
        }
        problems
    }

    /// The first rule that is broken, if one is.
    pub fn first_problem_with_the_groups(&self) -> Option<GroupProblem> {
        self.problems_with_the_groups().into_iter().next()
    }

    /// Whether a group of kind `group` can have `member` in it: the kind of thing that a group of that kind holds.
    pub(super) fn can_be_in_a_group_of_kind(&self, kind: GroupKind, member: Member) -> bool {
        match member {
            Member::Clip(_) => kind.can_hold_a_clip(),
            Member::Track(_) => kind.can_hold_a_track(),
            Member::Group(inner) => self
                .group(inner)
                .is_some_and(|inner| kind.can_hold_a_group_of_kind(inner.kind())),
        }
    }

    fn look_at_what_is_in_the_groups(&self, problems: &mut Vec<GroupProblem>) {
        let mut shape_of: HashMap<Member, GroupId> = HashMap::new();
        for group in &self.groups {
            if group.members().is_empty() {
                problems.push(GroupProblem::HasNothingInIt { group: group.id() });
            }
            let mut seen = HashSet::new();
            for &member in group.members() {
                if !seen.insert(member) {
                    problems.push(GroupProblem::HasAMemberTwice {
                        group: group.id(),
                        member,
                    });
                    continue;
                }
                if !self.has_the_member(member) {
                    problems.push(GroupProblem::HasAMemberThatIsNotThere {
                        group: group.id(),
                        member,
                    });
                    continue;
                }
                if !self.can_be_in_a_group_of_kind(group.kind(), member) {
                    problems.push(GroupProblem::CannotHold {
                        group: group.id(),
                        kind: group.kind(),
                        member,
                    });
                    continue;
                }
                if group.kind().is_a_shape()
                    && let Some(first) = shape_of.insert(member, group.id())
                {
                    problems.push(GroupProblem::InTwoShapes {
                        member,
                        first,
                        second: group.id(),
                    });
                }
            }
        }
    }

    /// Finds the groups that are in themselves: following what is in what, from a group, comes back to it.
    fn look_for_groups_in_themselves(&self, problems: &mut Vec<GroupProblem>) {
        #[derive(Clone, Copy, PartialEq)]
        enum Visit {
            Going,
            Done,
        }
        fn visit(project: &Project, group: &Group, visits: &mut HashMap<GroupId, Visit>, found: &mut HashSet<GroupId>) {
            visits.insert(group.id(), Visit::Going);
            for member in group.members() {
                let Member::Group(inner) = *member else {
                    continue;
                };
                let Some(inner_group) = project.group(inner) else {
                    continue;
                };
                match visits.get(&inner).copied() {
                    Some(Visit::Going) => {
                        found.insert(inner);
                    }
                    Some(Visit::Done) => {}
                    None => visit(project, inner_group, visits, found),
                }
            }
            visits.insert(group.id(), Visit::Done);
        }
        let mut visits = HashMap::new();
        let mut found = HashSet::new();
        for group in &self.groups {
            if !visits.contains_key(&group.id()) {
                visit(self, group, &mut visits, &mut found);
            }
        }
        let mut found: Vec<GroupId> = found.into_iter().collect();
        found.sort();
        problems.extend(found.into_iter().map(|group| GroupProblem::IsInItself { group }));
    }

    /// Checks that the chains and the tags agree: a thing is in a tag when, and only when, its chain has the badge of the tag.
    fn look_at_the_badges(&self, problems: &mut Vec<GroupProblem>) {
        for group in self.groups.iter().filter(|group| group.kind() == GroupKind::Tag) {
            for &member in group.members() {
                if let Some(chain) = self.chain_of_the_member(member)
                    && !chain.has_the_tag_badge(group.id())
                {
                    problems.push(GroupProblem::HasNoBadgeOfTheGroupItIsIn {
                        member,
                        group: group.id(),
                    });
                }
            }
        }
        let owners = self
            .tracks
            .iter()
            .flat_map(|track| {
                std::iter::once(Member::Track(track.id()))
                    .chain(track.entries().iter().map(|entry| Member::Clip(entry.id())))
            })
            .chain(self.groups.iter().map(|group| Member::Group(group.id())));
        for owner in owners {
            let Some(chain) = self.chain_of_the_member(owner) else {
                continue;
            };
            for badge in chain.tag_badges() {
                let is_in_it = self
                    .group(badge)
                    .is_some_and(|tag| tag.kind() == GroupKind::Tag && tag.has_as_a_member(owner));
                if !is_in_it {
                    problems.push(GroupProblem::HasTheBadgeOfAGroupItIsNotIn { owner, group: badge });
                }
            }
            if chain.keeps_the_badge_of_the_track() && !matches!(owner, Member::Clip(_)) {
                problems.push(GroupProblem::HasTheBadgeOfATrackAndIsNotAClip { owner });
            }
        }
    }

    /// Checks where things are: that nothing but what is in a clip group is inside its rectangle, that the tracks of a track group are
    /// next to each other, and that no clip group is across the edge of a track group.
    fn look_at_the_shapes(&self, problems: &mut Vec<GroupProblem>) {
        let parents = self.parents();
        let depth = self.groups.len();

        let mut rectangles_of_clip_groups = Vec::new();
        for group in self.groups.iter().filter(|group| group.kind() == GroupKind::ClipGroup) {
            let Some(rectangle) = self.rectangle_of_the_group(group.id()) else {
                continue;
            };
            for position in rectangle.tracks() {
                for placement in self.tracks[position].placements() {
                    let in_the_rectangle = rectangle.has_any_of_the_frames(
                        placement.first_timeline_frame.value(),
                        placement.end_timeline_frame.value(),
                    );
                    if in_the_rectangle && !parents.is_inside(Member::Clip(placement.id), group.id(), depth) {
                        problems.push(GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                            group: group.id(),
                            clip: placement.id,
                        });
                    }
                }
            }
            rectangles_of_clip_groups.push((group.id(), rectangle));
        }

        let mut tracks_of_track_groups = Vec::new();
        for group in self.groups.iter().filter(|group| group.kind() == GroupKind::TrackGroup) {
            let tracks = self.tracks_in(group.id());
            let positions: Vec<usize> = tracks.iter().filter_map(|track| self.track_position(*track)).collect();
            let (Some(&first), Some(&last)) = (positions.iter().min(), positions.iter().max()) else {
                continue;
            };
            for position in first..=last {
                let track = self.tracks[position].id();
                if !tracks.contains(&track) {
                    problems.push(GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt {
                        group: group.id(),
                        track,
                    });
                }
            }
            tracks_of_track_groups.push((group.id(), first..=last));
        }

        for (clip_group, rectangle) in &rectangles_of_clip_groups {
            for (track_group, tracks) in &tracks_of_track_groups {
                let (first, last) = (rectangle.first_track, rectangle.last_track);
                let is_apart = last < *tracks.start() || *tracks.end() < first;
                let is_inside = tracks.contains(&first) && tracks.contains(&last);
                if !is_apart && !is_inside {
                    problems.push(GroupProblem::ClipGroupReachesAcrossTheEdgeOfATrackGroup {
                        clip_group: *clip_group,
                        track_group: *track_group,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
