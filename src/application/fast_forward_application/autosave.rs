// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeping track of whether the project has changes that are not saved, and saving them by itself
//! when the Autosave setting is on.
//!
//! Autosave saves to the file the project already has. A project that has never been saved has no
//! file yet, and choosing one needs a question to the user, so nothing is saved by itself until
//! the first time they save it.

use std::time::{Duration, Instant};

use egui::Context;

use super::FastForwardApplication;

/// How long the project must stay unchanged before autosave saves it, so that a drag of the
/// fast-forward symbol, say, is saved once when it is let go and not on every frame of it.
const QUIET_TIME_BEFORE_AUTOSAVING: Duration = Duration::from_secs(2);

impl FastForwardApplication {
    /// Whether the project has changes that have not been saved: edits that were made since it was
    /// saved (undoing back to what was saved leaves none), or changes of what is not edited, such
    /// as the project's settings.
    pub fn has_unsaved_changes(&self) -> bool {
        self.unsaved_change_outside_history || !self.history.is_saved()
    }

    /// Records that the project has been changed, in a way that is not an edit of the history (a
    /// setting of the project, or the place a file was found), and is not saved yet.
    pub(super) fn note_unsaved_change(&mut self) {
        self.unsaved_change_outside_history = true;
        self.last_unsaved_change_at = Some(Instant::now());
    }

    /// Records that the project was edited, or that an edit was undone or redone, at `now`: it is
    /// the history that knows whether that leaves anything unsaved.
    pub(super) fn note_the_project_was_edited(&mut self, now: Instant) {
        self.last_unsaved_change_at = Some(now);
    }

    /// Records that the project is as saved.
    pub(super) fn note_everything_is_saved(&mut self) {
        self.history.mark_saved();
        self.unsaved_change_outside_history = false;
        self.last_unsaved_change_at = None;
    }

    /// Whether autosave is on and the project has a file to save to.
    fn autosave_can_save(&self) -> bool {
        self.effective_settings().autosave && self.project_file_path.is_some() && self.running_save.is_none()
    }

    /// Saves the project now if autosave is on, there is a file, and there is something to save.
    /// This is for the moments when waiting is no good, such as before the program asks whether
    /// to throw changes away: with autosave on there is nothing to throw away.
    pub(super) fn autosave_now(&mut self) {
        if !self.has_unsaved_changes() || !self.autosave_can_save() {
            return;
        }
        let Some(path) = self.project_file_path.clone() else {
            return;
        };
        // If saving fails the message says so once, and there is no new try until the project
        // changes again; trying again on every frame would only repeat the message.
        self.last_unsaved_change_at = None;
        // (Saving a project that is an archive adds the text of the project to it and nothing else, so that this is quick however much
        // video is included in it.)
        self.save_project_to(path);
    }

    /// Saves the project if autosave is on and the project has been left alone for long enough.
    /// If it is not time yet, asks the window to look again when it will be.
    pub(super) fn autosave_when_quiet_for_long_enough(&mut self, context: &Context, now: Instant) {
        if !self.has_unsaved_changes() || !self.autosave_can_save() {
            return;
        }
        let Some(changed_at) = self.last_unsaved_change_at else {
            return;
        };
        let quiet_for = now.saturating_duration_since(changed_at);
        if quiet_for >= QUIET_TIME_BEFORE_AUTOSAVING {
            self.autosave_now();
        } else {
            context.request_repaint_after(QUIET_TIME_BEFORE_AUTOSAVING - quiet_for);
        }
    }
}
