// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the commands about groups do, and what the user is told when one cannot be done.

use std::time::Instant;

use super::FastForwardApplication;
use super::drawing::source_looks_of;
use crate::application::group_messages;
use crate::application::selection::{Selection, SelectionItem};
use crate::project::{EditError, GroupId, GroupKind, Member};

impl FastForwardApplication {
    /// Tells the user why `error` (what a group could not be given, or what the rules of groups would not have) stopped what they asked for,
    /// under the heading `title`, if it is something that groups make happen. `false` if it is not, and nothing was said.
    pub(super) fn say_why_a_group_edit_was_refused(&mut self, title: &str, error: &EditError) -> bool {
        let reason = {
            let source_looks = source_looks_of(&self.document.project, &self.sources.files);
            group_messages::the_reason(&self.document.project, &source_looks, error)
        };
        match reason {
            Some(reason) => {
                self.show_message(title, reason);
                true
            }
            None => false,
        }
    }

    /// Makes a group of `members` and selects it, and nothing else (what was selected is not any more), or tells the user why it cannot
    /// be made.
    pub(super) fn create_a_group(&mut self, kind: GroupKind, members: &[Member]) {
        match self.document.project.create_group(kind, members) {
            Ok(group) => self.interface.selection = Selection::from_items([SelectionItem::Group(group)]),
            Err(error) => {
                self.say_why_a_group_edit_was_refused(&format!("A {kind} cannot be made of those"), &error);
            }
        }
    }

    /// Puts `members` in the group `group`, those of them that can be, telling the user why those that cannot be are not.
    pub(super) fn add_to_a_group(&mut self, group: GroupId, members: &[Member]) {
        let mut refused = None;
        for member in members {
            if let Err(error) = self.document.project.add_to_the_group(group, *member)
                && !matches!(error, EditError::AlreadyInTheGroup { .. })
            {
                refused.get_or_insert(error);
            }
        }
        if let Some(error) = refused {
            self.say_why_a_group_edit_was_refused("That cannot be added to the group", &error);
        }
    }

    /// Takes `member` out of the group `group`, or tells the user why it cannot be.
    pub(super) fn remove_from_a_group(&mut self, group: GroupId, member: Member) {
        match self.document.project.take_out_of_the_group(group, member) {
            Ok(()) => self.interface.selection.forget_what_is_gone(&self.document.project),
            Err(error) => {
                self.say_why_a_group_edit_was_refused("That cannot be taken out of the group", &error);
            }
        }
    }

    /// Takes the group apart (what was in it stays).
    pub(super) fn disband_a_group(&mut self, group: GroupId) {
        if self.document.project.disband_group(group).is_ok() {
            self.interface.selection.forget_what_is_gone(&self.document.project);
        }
    }

    /// Deletes the group and all that is in it, or tells the user why that cannot be done. The playhead stays where it was, unless that
    /// is past the end of what is left.
    pub(super) fn delete_a_group_and_everything_in_it(&mut self, group: GroupId, now: Instant) {
        let current_frame = self
            .playback
            .clock
            .position(now, self.document.project.total_frame_count());
        match self.document.project.delete_the_group_and_everything_in_it(group) {
            Ok(()) => {
                self.interface.selection.forget_what_is_gone(&self.document.project);
                let new_total_frames = self.document.project.total_frame_count();
                self.playback.clock.seek(now, current_frame, new_total_frames);
            }
            Err(error) => {
                self.say_why_a_group_edit_was_refused("The group cannot be deleted with all that is in it", &error);
            }
        }
    }
}
