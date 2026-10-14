//! Clips that are put in the rectangle of a clip group are put in the group.
//!
//! A clip that is not in a clip group, and is moved to where the rectangle of one is (it has some of its frames, on one of its tracks), **joins**
//! the group, as a clip that is dragged into it does: it is one of its members from then on, and the rectangle is the smallest that has it
//! as well as the others. A clip that is in a clip group already stays in it, and a clip that is in one cannot join another.
//!
//! A drag moves clips a little at a time, many times, each move being worked out from where the clips are then. For a clip that has joined a
//! group while it is dragged to be out of it again when it is dragged on, without any clip that was a member before leaving the group it is in,
//! **what each clip was in when the drag began** is said with each move (see [`ClipGroupBeforeTheDrag`]): a clip that was in no clip group
//! then is taken out of the one it is in now, if it is in one, before it is moved, and is put in the one it is moved to, if it is moved to
//! one. So the clip groups that each of the dragged clips is in is what its place says (and what it was in to begin with), whichever
//! way and however often it was moved on the way.

use super::{GroupId, GroupKind, Member};
use crate::project::tracks::ClipId;
use crate::project::{ClipMove, EditError, Project};

/// The clip group that a clip was directly in when a drag of it began, if it was in one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipGroupBeforeTheDrag {
    pub clip: ClipId,
    pub group: Option<GroupId>,
}

impl Project {
    /// The clip group that each of `clips` is directly in now, to say with the moves of a drag that is beginning (see
    /// [`Self::move_clips_remembering`]). A clip that is not there is left out.
    pub fn clip_groups_before_a_drag_of(&self, clips: &[ClipId]) -> Vec<ClipGroupBeforeTheDrag> {
        clips
            .iter()
            .filter(|clip| self.placement_of(**clip).is_some())
            .map(|clip| ClipGroupBeforeTheDrag {
                clip: *clip,
                group: self.shape_group_of(Member::Clip(*clip)).map(super::Group::id),
            })
            .collect()
    }

    /// [`Self::move_clips`], where `before_the_drag` says which clip group each of the clips was directly in when the drag that moves them
    /// began: a clip that was in none is taken out of the clip group it is in now (which is one that it joined as it was dragged) before it
    /// is moved. Then, as in every move of clips, those that are in no clip group and are moved into the rectangle of one are put in it, in
    /// the one that is the innermost of those that it is in the rectangle of (it is an error if there is no one that is: it is in the
    /// rectangle of two that are not one in the other).
    pub fn move_clips_remembering(
        &mut self,
        moves: &[ClipMove],
        before_the_drag: &[ClipGroupBeforeTheDrag],
    ) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            project.take_out_of_the_clip_groups_they_joined_as_they_were_dragged(before_the_drag);
            project.move_clips_without_looking_at_the_groups(moves)?;
            project.put_the_clips_in_the_clip_groups_they_are_moved_into(moves);
            Ok(())
        })
    }

    /// Takes each of the clips that `before_the_drag` says were in no clip group out of the clip group it is in, if it is in one.
    fn take_out_of_the_clip_groups_they_joined_as_they_were_dragged(
        &mut self,
        before_the_drag: &[ClipGroupBeforeTheDrag],
    ) {
        if self.groups.iter().all(|group| group.kind() != GroupKind::ClipGroup) {
            return;
        }
        for was_in_none in before_the_drag.iter().filter(|before| before.group.is_none()) {
            let member = Member::Clip(was_in_none.clip);
            for group in self
                .groups
                .iter_mut()
                .filter(|group| group.kind() == GroupKind::ClipGroup)
            {
                group.members_mut().retain(|other| *other != member);
            }
        }
    }

    /// Puts each of the clips that are moved, and are in no clip group, in the clip group whose rectangle it is in, if there is one: the
    /// innermost of those it is in the rectangle of, as the rectangles are before any of the clips is put in a group.
    fn put_the_clips_in_the_clip_groups_they_are_moved_into(&mut self, moves: &[ClipMove]) {
        if self.groups.iter().all(|group| group.kind() != GroupKind::ClipGroup) {
            return;
        }
        let parents = self.parents();
        let rectangles: Vec<(GroupId, super::Rectangle)> = self
            .groups
            .iter()
            .filter(|group| group.kind() == GroupKind::ClipGroup)
            .filter_map(|group| Some((group.id(), self.rectangle_of_the_group(group.id())?)))
            .collect();
        let mut joins: Vec<(GroupId, ClipId)> = Vec::new();
        for moved in moves {
            let clip = moved.clip;
            if joins.iter().any(|(_, joined)| *joined == clip) || parents.shape_of(Member::Clip(clip)).is_some() {
                continue;
            }
            let Some(place) = self.rectangle_of_the_clip(clip) else {
                continue;
            };
            let candidates: Vec<GroupId> = rectangles
                .iter()
                .filter(|(_, rectangle)| rectangle.is_on_top_of(&place))
                .map(|(group, _)| *group)
                .collect();
            // The innermost: the one that all the others are round.
            let innermost = candidates.iter().copied().find(|candidate| {
                let round = self.shapes_round(&parents, Member::Group(*candidate));
                candidates
                    .iter()
                    .all(|other| other == candidate || round.contains(other))
            });
            if let Some(group) = innermost {
                joins.push((group, clip));
            }
        }
        for (group, clip) in joins {
            if let Some(found) = self.group_mut(group) {
                found.members_mut().push(Member::Clip(clip));
            }
        }
    }
}

#[cfg(test)]
mod tests;
