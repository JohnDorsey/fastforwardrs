//! Keeping the groups right while clips and tracks are edited.
//!
//! Most edits of clips and tracks have nothing to do with groups, and some can break their rules: a clip that is dragged into the
//! rectangle of a clip group, a clip that is sped up so that its group's rectangle takes in another, a track that is moved into the
//! middle of a track group. Instead of working out for each edit what it can break, **every edit that can break a rule is made through
//! [`Project::keeping_the_groups_valid`]**: it makes the edit, checks the rules of [`super::rules`] afterwards, and if one is broken puts
//! the project back as it was and says which. (A project that has no groups skips all of it, so that such a project is edited as fast, and
//! in the same way, as it was before there were groups.)
//!
//! The edits that put a new clip or track in the project, or take one out of it, have something to do for the groups besides being checked:
//! a clip that is cut in two has its second half in the groups that the first is in, a copy of a clip or of a track is in the groups of what
//! it is a copy of, and a clip or a track that is taken out of the project is taken out of its groups, which are dropped when nothing is
//! left in them. That is here too.

use super::{GroupId, Member};
use crate::project::{EditError, Project};

impl Project {
    /// Makes `edit` to the project, and, if the project has groups (before or after), checks that every rule about them is kept
    /// afterwards. If one is not, or if `edit` fails, the project is put back as it was, and the error says which rule was broken.
    pub(in crate::project) fn keeping_the_groups_valid<T>(
        &mut self,
        edit: impl FnOnce(&mut Self) -> Result<T, EditError>,
    ) -> Result<T, EditError> {
        let has_groups = !self.groups.is_empty();
        self.editing_and_checking_the_groups(has_groups, edit)
    }

    /// As [`Self::keeping_the_groups_valid`], for an edit that may make the first group: it is checked even if there were none.
    pub(in crate::project) fn editing_and_checking_the_groups<T>(
        &mut self,
        check: bool,
        edit: impl FnOnce(&mut Self) -> Result<T, EditError>,
    ) -> Result<T, EditError> {
        if !check {
            let outcome = edit(self);
            debug_assert!(
                self.groups.is_empty(),
                "an edit that is not checked cannot make a group: it is not looked at"
            );
            return outcome;
        }
        let before = self.clone();
        match edit(self) {
            Ok(value) => match self.first_problem_with_the_groups() {
                None => Ok(value),
                Some(problem) => {
                    *self = before;
                    Err(EditError::BreaksTheRulesOfGroups(problem))
                }
            },
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }

    /// Whether `edit`, which is made to a copy of the project, would be allowed by the rules about groups and would succeed: for finding out
    /// whether something can be done without doing it.
    pub fn would_be_allowed(&self, edit: impl FnOnce(&mut Self) -> Result<(), EditError>) -> Result<(), EditError> {
        let mut copy = self.clone();
        copy.keeping_the_groups_valid(edit)
    }

    /// Puts `copy` in every group that `original` is in, right after it in the group's list. The chain of effects of `copy` must have the
    /// badges of the tags already (a copy is made from the chain of the original, which has them).
    pub(in crate::project) fn put_the_copy_in_the_groups_of(&mut self, copy: Member, original: Member) {
        for group in &mut self.groups {
            if group.has_as_a_member(copy) {
                continue;
            }
            if let Some(place) = group.members().iter().position(|member| *member == original) {
                group.members_mut().insert(place + 1, copy);
            }
        }
    }

    /// Takes `member`, which is no longer in the project, out of every group it was in, and drops the groups that nothing is left in.
    pub(in crate::project) fn take_out_of_the_groups(&mut self, member: Member) {
        for group in &mut self.groups {
            group.members_mut().retain(|other| *other != member);
        }
        self.drop_the_empty_groups();
    }

    /// Drops every group that has nothing in it, and so (as a group that is in another is in its list) the groups that had only those in
    /// them, and so on.
    pub(super) fn drop_the_empty_groups(&mut self) {
        while let Some(empty) = self
            .groups
            .iter()
            .find(|group| group.members().is_empty())
            .map(|group| group.id())
        {
            self.groups.retain(|group| group.id() != empty);
            for group in &mut self.groups {
                group.members_mut().retain(|member| *member != Member::Group(empty));
            }
        }
    }

    /// Puts the badge of the tag `tag` in the chain of effects of `member` if `member` has one that can have it, last.
    pub(super) fn put_the_badge_of_a_tag_in_the_chain_of(&mut self, member: Member, tag: GroupId) {
        if let Some(chain) = self.chain_of_the_member_mut(member) {
            chain.add_tag_badge(tag);
        }
    }
}

#[cfg(test)]
mod tests;
