//! Asking a project about its groups: which there are, what is in them, which a thing is in, and what is inside what.

use std::collections::HashMap;

use super::{Group, GroupId, GroupKind, Member};
use crate::project::tracks::{ClipId, TrackId};
use crate::project::{Effects, Project};

/// Which clip group or track group each thing is directly in, worked out once for a project, for work that asks it about many things.
pub(super) struct Parents {
    /// The clip group or track group that a thing is directly in, if it is in one.
    shape: HashMap<Member, GroupId>,
}

impl Parents {
    /// The clip group or the track group that `member` is directly in.
    pub(super) fn shape_of(&self, member: Member) -> Option<GroupId> {
        self.shape.get(&member).copied()
    }

    /// Whether `member` is in the clip group or track group `group`, directly or in one that is in it (and so on as far as it
    /// goes): it is inside its rectangle. (`depth` is as many steps as there are groups, so that a group that is in itself, which the
    /// rules do not allow, is not followed for ever.)
    pub(super) fn is_inside(&self, member: Member, group: GroupId, depth: usize) -> bool {
        let mut at = member;
        for _ in 0..=depth {
            match self.shape_of(at) {
                Some(parent) if parent == group => return true,
                Some(parent) => at = Member::Group(parent),
                None => return false,
            }
        }
        false
    }
}

impl Project {
    /// The groups, in the order they were made in.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn group(&self, group: GroupId) -> Option<&Group> {
        self.groups.iter().find(|candidate| candidate.id() == group)
    }

    pub(super) fn group_mut(&mut self, group: GroupId) -> Option<&mut Group> {
        self.groups.iter_mut().find(|candidate| candidate.id() == group)
    }

    /// Whether the project has what `member` is: the clip, the track or the group.
    pub fn has_the_member(&self, member: Member) -> bool {
        match member {
            Member::Clip(clip) => self.placement_of(clip).is_some(),
            Member::Track(track) => self.track(track).is_some(),
            Member::Group(group) => self.group(group).is_some(),
        }
    }

    /// Which clip group or track group each thing in the project is in, worked out now.
    pub(super) fn parents(&self) -> Parents {
        let mut parents = Parents { shape: HashMap::new() };
        for group in self.groups.iter().filter(|group| group.kind().is_a_shape()) {
            for member in group.members() {
                parents.shape.entry(*member).or_insert(group.id());
            }
        }
        parents
    }

    /// The clip group or track group that `member` is directly in. (A clip or a track can be in one of these at most.)
    pub fn shape_group_of(&self, member: Member) -> Option<&Group> {
        self.groups
            .iter()
            .find(|group| group.kind().is_a_shape() && group.has_as_a_member(member))
    }

    /// The tags that `member` is directly in, in the order the tags are in the project.
    pub fn tags_of(&self, member: Member) -> Vec<&Group> {
        self.groups
            .iter()
            .filter(|group| group.kind() == GroupKind::Tag && group.has_as_a_member(member))
            .collect()
    }

    /// Every group that has `member` as one of its members, directly: the tags and the clip group or track group.
    pub fn groups_holding(&self, member: Member) -> Vec<&Group> {
        self.groups
            .iter()
            .filter(|group| group.has_as_a_member(member))
            .collect()
    }

    /// Whether `member` is inside the clip group or track group `group`: it is in it, or in a clip group or track group that is in it,
    /// and so on as far as that goes. A clip on a track of a track group is not *in* the group: it is inside it by where it is.
    /// (See [`Self::clip_is_inside`] for that.)
    pub fn is_in_the_shape(&self, member: Member, group: GroupId) -> bool {
        self.parents().is_inside(member, group, self.groups.len())
    }

    /// Whether the clip `clip` is inside the group `group`: for a clip group, whether it is in it or in a clip group that is; for a
    /// track group, whether it is on a track of it or on a track of a track group that is in it (a clip is inside a track group by
    /// being on its tracks); for a tag, whether it is in it or in a group that is in it.
    pub fn clip_is_inside(&self, clip: ClipId, group: GroupId) -> bool {
        let Some(found) = self.group(group) else {
            return false;
        };
        match found.kind() {
            GroupKind::ClipGroup => self.is_in_the_shape(Member::Clip(clip), group),
            GroupKind::TrackGroup => self
                .track_holding(clip)
                .is_some_and(|track| self.is_in_the_shape(Member::Track(track.id()), group)),
            GroupKind::Tag => self.clips_in(group).contains(&clip),
        }
    }

    /// Every clip in the group `group`, directly or in the groups that are in it as far down as that goes: for a tag, the clips that are
    /// in it and the clips of the groups that are in it; for a clip group, its clips and those of the clip groups in it; for a track
    /// group, every clip on its tracks and those of the track groups in it. Each is there once, in the order the groups list them (and
    /// the tracks hold them).
    pub fn clips_in(&self, group: GroupId) -> Vec<ClipId> {
        let mut found = Vec::new();
        self.collect_the_clips_in(group, self.groups.len(), &mut found);
        found
    }

    fn collect_the_clips_in(&self, group: GroupId, depth: usize, found: &mut Vec<ClipId>) {
        let Some(group) = self.group(group) else {
            return;
        };
        for member in group.members() {
            match *member {
                Member::Clip(clip) => {
                    if !found.contains(&clip) {
                        found.push(clip);
                    }
                }
                Member::Track(track) => {
                    for entry in self.track(track).into_iter().flat_map(|track| track.entries()) {
                        if !found.contains(&entry.id()) {
                            found.push(entry.id());
                        }
                    }
                }
                Member::Group(inner) => {
                    if let Some(depth) = depth.checked_sub(1) {
                        self.collect_the_clips_in(inner, depth, found);
                    }
                }
            }
        }
    }

    /// Every track in the group `group`, directly or in the track groups that are in it (and the tags that have a group that has tracks
    /// in them), in the order the groups list them.
    pub fn tracks_in(&self, group: GroupId) -> Vec<TrackId> {
        let mut found = Vec::new();
        self.collect_the_tracks_in(group, self.groups.len(), &mut found);
        found
    }

    fn collect_the_tracks_in(&self, group: GroupId, depth: usize, found: &mut Vec<TrackId>) {
        let Some(group) = self.group(group) else {
            return;
        };
        for member in group.members() {
            match *member {
                Member::Track(track) => {
                    if !found.contains(&track) {
                        found.push(track);
                    }
                }
                Member::Group(inner) => {
                    if let Some(depth) = depth.checked_sub(1) {
                        self.collect_the_tracks_in(inner, depth, found);
                    }
                }
                Member::Clip(_) => {}
            }
        }
    }

    /// The groups that the clip `clip` is inside, from the one that is nearest to it to the one that is furthest: the clip groups it
    /// is in (one in another), then the track groups its track is in (one in another). A tag is not among them: a clip is in a tag,
    /// it is not inside it.
    pub fn groups_round_the_clip(&self, clip: ClipId) -> Vec<GroupId> {
        let parents = self.parents();
        let mut round = self.shapes_round(&parents, Member::Clip(clip));
        if let Some(track) = self.track_holding(clip) {
            round.extend(self.shapes_round(&parents, Member::Track(track.id())));
        }
        round
    }

    /// The clip groups or track groups that `member` is in, one in another: the nearest first.
    pub(super) fn shapes_round(&self, parents: &Parents, member: Member) -> Vec<GroupId> {
        let mut round = Vec::new();
        let mut at = member;
        while let Some(parent) = parents.shape_of(at) {
            if round.len() > self.groups.len() {
                // (A group in itself, which the rules do not allow.)
                break;
            }
            round.push(parent);
            at = Member::Group(parent);
        }
        round
    }

    /// The chain of effects of `member`, if it has one (a clip, a track or a group that is there): the effects it has and, for a clip or
    /// a group, the badges of the tags it is in.
    pub fn chain_of_the_member(&self, member: Member) -> Option<&Effects> {
        match member {
            Member::Clip(clip) => self.placement_of(clip).map(|placement| placement.effects),
            Member::Track(track) => self.track(track).map(|track| track.effects()),
            Member::Group(group) => self.group(group).map(Group::effects),
        }
    }
}

#[cfg(test)]
mod tests;
