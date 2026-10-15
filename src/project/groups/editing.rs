// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making groups, changing them, and taking them apart.
//!
//! Every change that can break a rule of groups (see [`super::rules`]) is made so that it is taken back, with the rule that it would
//! break, if it does (see [`super::upkeep`]): a clip group is made when the rectangle of the clips that are to be in it has nothing
//! else in it, a member is taken out of a clip group when what is left does not have the member inside its rectangle, and so on.

use super::{Group, GroupColour, GroupId, GroupKind, Member};
use crate::project::{EditError, Effects, Project};

impl Project {
    /// The name a new group of kind `kind` is given: the kind with the smallest number that no group of that kind has the name of
    /// ("Tag 1", "Tag 2", and so on; a number that was a group's and is free again is used again).
    pub fn name_for_a_new_group_of_kind(&self, kind: GroupKind) -> String {
        (1..)
            .map(|number| format!("{} {number}", kind.name_with_a_capital()))
            .find(|name| {
                !self
                    .groups
                    .iter()
                    .any(|group| group.kind() == kind && group.name() == name)
            })
            .expect("there are not as many groups as there are numbers")
    }

    /// The colour a new group is given: the first of the colours of [`GroupColour::PALETTE`] that no group has, and when every one of
    /// them has been given, the ones after that are given again from the first, in turn.
    pub fn colour_for_a_new_group(&self) -> GroupColour {
        GroupColour::PALETTE
            .into_iter()
            .find(|colour| !self.groups.iter().any(|group| group.colour() == *colour))
            .unwrap_or(GroupColour::PALETTE[self.groups.len() % GroupColour::PALETTE.len()])
    }

    /// The chain of effects of `member`, to change it.
    pub(in crate::project) fn chain_of_the_member_mut(&mut self, member: Member) -> Option<&mut Effects> {
        match member {
            Member::Clip(clip) => {
                let position = self.track_position_holding(clip)?;
                self.tracks[position].effects_of_the_clip_mut(clip).ok()
            }
            Member::Track(track) => self.track_mut(track).map(|track| track.effects_mut()),
            Member::Group(group) => self.group_mut(group).map(Group::effects_mut),
        }
    }

    /// Makes a group of kind `kind` with `members` in it, in that order, named and coloured as a new group is (see
    /// [`Self::name_for_a_new_group_of_kind`] and [`Self::colour_for_a_new_group`]). Where the group is a tag, each of the members has the
    /// badge of it, last, in its chain of effects.
    ///
    /// It is an error, and nothing is changed, if there is nothing to make it of, if a clip group would have less than two things in it, or
    /// if the group would break a rule: something is in it that a group of its kind cannot hold or that is in another clip group or track
    /// group already, a clip group would have a clip in its rectangle that is not in it, the tracks of a track group are not next to each
    /// other, and so on (see [`super::GroupProblem`]).
    pub fn create_group(&mut self, kind: GroupKind, members: &[Member]) -> Result<GroupId, EditError> {
        if members.is_empty() {
            return Err(EditError::NothingToMakeAGroupOf { kind });
        }
        if kind == GroupKind::ClipGroup && members.len() < 2 {
            return Err(EditError::TooFewToMakeAClipGroup);
        }
        let name = self.name_for_a_new_group_of_kind(kind);
        let colour = self.colour_for_a_new_group();
        self.editing_and_checking_the_groups(true, |project| {
            let id = GroupId::new(project.next_group_number);
            project.next_group_number += 1;
            project
                .groups
                .push(Group::new(id, kind, name, colour).with_members(members.iter().copied()));
            if kind == GroupKind::Tag {
                for member in members {
                    project.put_the_badge_of_a_tag_in_the_chain_of(*member, id);
                }
            }
            Ok(id)
        })
    }

    pub fn rename_group(&mut self, group: GroupId, name: impl Into<String>) -> Result<(), EditError> {
        self.group_mut(group)
            .ok_or(EditError::UnknownGroup { group })?
            .set_name(name);
        Ok(())
    }

    pub fn set_group_colour(&mut self, group: GroupId, colour: GroupColour) -> Result<(), EditError> {
        self.group_mut(group)
            .ok_or(EditError::UnknownGroup { group })?
            .set_colour(colour);
        Ok(())
    }

    /// Puts `member` in the group `group`, last. (Where the group is a tag, `member` has the badge of it, last, in its chain.) It is an
    /// error, and nothing is changed, if there is no such group or `member` is in it already, or if that would break a rule (see
    /// [`Self::create_group`]).
    pub fn add_to_the_group(&mut self, group: GroupId, member: Member) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            let found = project.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
            if found.has_as_a_member(member) {
                return Err(EditError::AlreadyInTheGroup { group, member });
            }
            let kind = found.kind();
            found.members_mut().push(member);
            if kind == GroupKind::Tag {
                project.put_the_badge_of_a_tag_in_the_chain_of(member, group);
            }
            Ok(())
        })
    }

    /// Takes `member` out of the group `group`; it stays in the project. (Where the group is a tag, its badge is taken out of the chain of
    /// `member`.) A group that has nothing in it then is dropped. It is an error, and nothing is changed, if there is no such group or
    /// `member` is not in it, or if that would break a rule: a clip that is taken out of a clip group while it is still inside its
    /// rectangle, because of the members that are left, would be in it without being a member.
    pub fn take_out_of_the_group(&mut self, group: GroupId, member: Member) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            let found = project.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
            let Some(place) = found.members().iter().position(|other| *other == member) else {
                return Err(EditError::NotInTheGroup { group, member });
            };
            found.members_mut().remove(place);
            if found.kind() == GroupKind::Tag
                && let Some(chain) = project.chain_of_the_member_mut(member)
            {
                chain.remove_tag_badge(group);
            }
            project.drop_the_empty_groups();
            Ok(())
        })
    }

    /// Takes the group `group` apart: it is gone, and none of what was in it is: the members **take its place** in the groups it was in.
    /// Where the group was in a clip group or a track group, its members are in that group directly now (so that what was inside it is
    /// inside still); where it was in tags, they are in the tags, each with the badge of the tag where the badge of the group was in its
    /// chain (a tag that was a member takes the place of the tag that is gone, in the chains of its members), and where a tag cannot have
    /// a member (a track), it is not in it. A group that has nothing left in it is dropped. It is an error, and nothing is changed, if
    /// there is no such group.
    pub fn disband_group(&mut self, group: GroupId) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            let disbanded = project.group(group).cloned().ok_or(EditError::UnknownGroup { group })?;
            let parents: Vec<GroupId> = project
                .groups_holding(Member::Group(group))
                .iter()
                .map(|parent| parent.id())
                .collect();
            // The tags it was in, in the order its chain has their badges in.
            let tags_it_was_in: Vec<GroupId> = disbanded.effects().tag_badges().collect();

            for parent in parents {
                let kind_of_the_parent = project
                    .group(parent)
                    .map(Group::kind)
                    .expect("a parent that was just found");
                let taking_its_place: Vec<Member> = disbanded
                    .members()
                    .iter()
                    .copied()
                    .filter(|member| project.can_be_in_a_group_of_kind(kind_of_the_parent, *member))
                    .filter(|member| {
                        !project
                            .group(parent)
                            .is_some_and(|found| found.has_as_a_member(*member))
                    })
                    .collect();
                let found = project.group_mut(parent).expect("a parent that was just found");
                let place = found
                    .members()
                    .iter()
                    .position(|member| *member == Member::Group(group))
                    .expect("a parent has the group in it");
                found.members_mut().splice(place..=place, taking_its_place);
            }
            for &member in disbanded.members() {
                let tags_it_is_in_now: Vec<GroupId> = tags_it_was_in
                    .iter()
                    .copied()
                    .filter(|tag| project.group(*tag).is_some_and(|found| found.has_as_a_member(member)))
                    .collect();
                if let Some(chain) = project.chain_of_the_member_mut(member) {
                    if disbanded.kind() == GroupKind::Tag {
                        chain.replace_tag_badge(group, &tags_it_is_in_now);
                    } else {
                        for tag in tags_it_is_in_now {
                            chain.add_tag_badge(tag);
                        }
                    }
                }
            }
            project.groups.retain(|found| found.id() != group);
            project.drop_the_empty_groups();
            Ok(())
        })
    }

    /// Takes the group `group` and everything that is in it away: its clips come off their tracks (as when they are deleted, one after
    /// another), its tracks go with all that is on them, and so do those of the groups that are in it, as far down as that goes. A track
    /// group, or a tag with tracks in it, that has every track of the project in it is not deleted, because a project always has a track.
    /// It is an error, and nothing is changed, if there is no such group or that would be so, or if the clips that are left could not be
    /// where they are (see [`super::GroupProblem`]).
    pub fn delete_the_group_and_everything_in_it(&mut self, group: GroupId) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            project.group(group).ok_or(EditError::UnknownGroup { group })?;
            let tracks = project.tracks_in(group);
            let clips = project.clips_in(group);
            if tracks.len() >= project.tracks.len() {
                return Err(EditError::WouldDeleteEveryTrack);
            }
            for clip in clips {
                let is_on_a_track_that_goes = project
                    .track_holding(clip)
                    .is_some_and(|holding| tracks.contains(&holding.id()));
                if !is_on_a_track_that_goes {
                    project.delete_clip_without_looking_at_the_groups(clip)?;
                }
            }
            for track in tracks {
                project.delete_track_without_looking_at_the_groups(track)?;
            }
            project.drop_the_empty_groups();
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
