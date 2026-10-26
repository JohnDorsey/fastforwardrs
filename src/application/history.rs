// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Going back and forward through the edits that were made: undo and redo.
//!
//! Before an edit is made to the project the editor keeps a copy of it, and undoing the edit is
//! putting that copy back. (A project is plain data and small next to a video, so a copy is cheap,
//! and it is the simplest way to be sure that undoing gives back exactly what was there.) What was
//! selected is kept with each copy, so that undoing also gives back the selection that the edit was
//! made with.
//!
//! **One step for one gesture.** Dragging a slider, or typing in a text box, makes an edit in every
//! frame of the window, but is one thing for the person doing it. Edits that say they are about the
//! same thing (the same [`EditKey`]) are made into one step, as long as the mouse button is held down
//! or the last of them was a moment ago, so that one undo takes back the whole drag.
//!
//! **Saved or not.** The history knows which state of the project was the one that was saved, so
//! that undoing back to it counts as having no unsaved changes again.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use super::selection::SelectionItem;
use crate::project::Project;

/// How many steps can be undone. The oldest are forgotten when there are more.
pub const MOST_STEPS_KEPT: usize = 200;

/// Edits about the same thing that are made this soon after one another are one step, even when the
/// mouse button is not held down (typing, say).
pub const MERGE_WINDOW: Duration = Duration::from_secs(1);

/// What an edit is about: edits with equal keys, made one after another, are one step. Edits that
/// are never to be joined (cutting, deleting) have no key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditKey(u64);

impl EditKey {
    /// The key for edits about `thing`.
    pub fn about(thing: &impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        thing.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Names a state the project has been in, so that it can be told whether the project is as it was
/// when it was saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StateId(u64);

/// One step that can be undone (or redone): the project as it was before it, and what was selected.
struct Step {
    project_before: Project,
    selection_before: Vec<SelectionItem>,
    state_before: StateId,
    /// What the edits of the step are about, while more may still be joined to it.
    key: Option<EditKey>,
    last_edited_at: Instant,
}

/// What undoing or redoing gives back.
#[derive(Clone, Debug, PartialEq)]
pub struct Restored {
    pub project: Project,
    pub selection: Vec<SelectionItem>,
}

/// The edits that were made, and the ones that were undone.
pub struct History {
    /// The steps that can be undone, the latest last.
    undo: Vec<Step>,
    /// The steps that were undone, the one to redo first last.
    redo: Vec<Step>,
    /// The state the project is in now.
    current: StateId,
    next_state_number: u64,
    /// The state the project was in when it was saved.
    saved: Option<StateId>,
    /// Goes up whenever the history is started afresh, so that someone who made a note of it can
    /// tell that the project was replaced in the meantime.
    generation: u64,
    /// How soon after one another edits about the same thing are one step, when the mouse button is not held down.
    merge_window: Duration,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// A history of a project that has just been made or loaded: nothing to undo, and as saved.
    pub fn new() -> Self {
        let mut history = Self {
            undo: Vec::new(),
            redo: Vec::new(),
            current: StateId(0),
            next_state_number: 1,
            saved: None,
            generation: 0,
            merge_window: MERGE_WINDOW,
        };
        history.reset();
        history
    }

    /// Makes edits about the same thing one step however long it is between them. (For the tests of a window, which make one edit after
    /// another in real time: a computer that is busy can take more than [`MERGE_WINDOW`] over a frame, and then what was meant to be
    /// one step is two. What [`MERGE_WINDOW`] means is tried with times that are given, in the tests of this module.)
    #[cfg(test)]
    pub fn merge_edits_whenever_they_are_made(&mut self) {
        self.merge_window = Duration::MAX;
    }

    /// Undoes [`Self::merge_edits_whenever_they_are_made`]: edits are one step only when they come within [`MERGE_WINDOW`] of each other (or a pointer is down), so that a test of the
    /// window can give the times itself.
    #[cfg(test)]
    pub fn merge_edits_by_time_again(&mut self) {
        self.merge_window = MERGE_WINDOW;
    }

    /// Forgets everything (a different project was made or loaded), and counts the project as saved.
    pub fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.current = self.new_state();
        self.saved = Some(self.current);
        self.generation += 1;
    }

    /// A number that is different after every [`Self::reset`].
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What the project is now, as a pair of numbers that differ whenever the project does: another project (a [`Self::reset`]), and every edit, undo and redo. (An undo that goes back to a
    /// state that the project was in gives that state's numbers again, and the numbers do not say anything else: only whether it is the same as when they were last asked for.)
    pub fn state_of_the_project(&self) -> (u64, u64) {
        (self.generation, self.current.0)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Whether the project is in the state it was saved in.
    pub fn is_saved(&self) -> bool {
        self.saved == Some(self.current)
    }

    /// Says that the project, as it is now, is saved.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.current);
    }

    fn new_state(&mut self) -> StateId {
        let state = StateId(self.next_state_number);
        self.next_state_number += 1;
        state
    }

    /// Records that an edit was made. `project_before` is the project as it was, and
    /// `selection_before` what was selected, before the edit. Whatever had been undone can no
    /// longer be redone.
    ///
    /// If the latest step is about the same thing as this edit (`key`), and the mouse button is
    /// held down (`pointer_is_down`) or the step was last edited less than [`MERGE_WINDOW`] ago, the
    /// edit becomes part of that step instead of a step of its own.
    pub fn record_change(
        &mut self,
        project_before: Project,
        selection_before: Vec<SelectionItem>,
        key: Option<EditKey>,
        now: Instant,
        pointer_is_down: bool,
    ) {
        self.redo.clear();
        let state_after = self.new_state();
        if let Some(key) = key
            && let Some(latest) = self.undo.last_mut()
            && latest.key == Some(key)
            && (pointer_is_down || now.saturating_duration_since(latest.last_edited_at) < self.merge_window)
        {
            latest.last_edited_at = now;
            self.current = state_after;
            return;
        }
        self.undo.push(Step {
            project_before,
            selection_before,
            state_before: self.current,
            key,
            last_edited_at: now,
        });
        self.current = state_after;
        if self.undo.len() > MOST_STEPS_KEPT {
            self.undo.remove(0);
        }
    }

    /// Takes back the latest step. `project_now` and `selection_now` are what there is now, which a
    /// redo gives back. `None` if there is nothing to undo.
    pub fn undo(&mut self, project_now: &Project, selection_now: Vec<SelectionItem>) -> Option<Restored> {
        let step = self.undo.pop()?;
        self.redo.push(Step {
            project_before: project_now.clone(),
            selection_before: selection_now,
            state_before: self.current,
            key: None,
            last_edited_at: Instant::now(),
        });
        self.current = step.state_before;
        self.stop_joining_edits_to_the_latest_step();
        Some(Restored {
            project: step.project_before,
            selection: step.selection_before,
        })
    }

    /// Does again the step that was undone last. `None` if there is nothing to redo.
    pub fn redo(&mut self, project_now: &Project, selection_now: Vec<SelectionItem>) -> Option<Restored> {
        let step = self.redo.pop()?;
        self.undo.push(Step {
            project_before: project_now.clone(),
            selection_before: selection_now,
            state_before: self.current,
            key: None,
            last_edited_at: Instant::now(),
        });
        self.current = step.state_before;
        Some(Restored {
            project: step.project_before,
            selection: step.selection_before,
        })
    }

    /// An edit made after an undo is a step of its own, not part of the step that is now the latest.
    fn stop_joining_edits_to_the_latest_step(&mut self) {
        if let Some(latest) = self.undo.last_mut() {
            latest.key = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project that is told apart from the others by how many tracks it has.
    fn project_of(tracks: usize) -> Project {
        let mut project = Project::empty();
        for _ in 1..tracks {
            project.add_track_in_front();
        }
        project
    }

    fn track_count(project: &Project) -> usize {
        project.tracks().len()
    }

    fn key(about: &str) -> Option<EditKey> {
        Some(EditKey::about(&about))
    }

    /// Records the change from a project of `before` tracks to one of `before + 1`.
    fn record(history: &mut History, before: usize, key: Option<EditKey>, now: Instant, pointer_is_down: bool) {
        history.record_change(project_of(before), Vec::new(), key, now, pointer_is_down);
    }

    #[test]
    fn a_new_history_has_nothing_to_undo_or_redo_and_counts_the_project_as_saved() {
        let history = History::new();
        assert!(!history.can_undo() && !history.can_redo());
        assert!(history.is_saved());
    }

    #[test]
    fn undoing_gives_back_the_project_from_before_and_redoing_gives_back_what_was_undone() {
        let mut history = History::new();
        record(&mut history, 1, None, Instant::now(), false);
        let now = project_of(2);

        let undone = history.undo(&now, Vec::new()).unwrap();
        assert_eq!(track_count(&undone.project), 1);
        assert!(!history.can_undo() && history.can_redo());

        let redone = history.redo(&undone.project, Vec::new()).unwrap();
        assert_eq!(track_count(&redone.project), 2, "what was there when it was undone");
        assert!(history.can_undo() && !history.can_redo());
    }

    #[test]
    fn nothing_to_undo_or_redo_gives_nothing() {
        let mut history = History::new();
        assert!(history.undo(&project_of(1), Vec::new()).is_none());
        assert!(history.redo(&project_of(1), Vec::new()).is_none());
    }

    #[test]
    fn steps_are_undone_latest_first_and_redone_in_the_order_they_were_made() {
        let mut history = History::new();
        let start = Instant::now();
        for tracks in 1..=3 {
            record(&mut history, tracks, None, start, false);
        }
        let mut project = project_of(4);
        let mut seen = Vec::new();
        while let Some(restored) = history.undo(&project, Vec::new()) {
            seen.push(track_count(&restored.project));
            project = restored.project;
        }
        assert_eq!(seen, vec![3, 2, 1]);
        let mut seen = Vec::new();
        while let Some(restored) = history.redo(&project, Vec::new()) {
            seen.push(track_count(&restored.project));
            project = restored.project;
        }
        assert_eq!(seen, vec![2, 3, 4]);
    }

    #[test]
    fn the_selection_a_step_was_made_with_comes_back_with_it() {
        use crate::project::{ClipId, TrackId};
        let clip = SelectionItem::Clip(ClipId::new(TrackId::new(0), 5));
        let track = SelectionItem::Track(TrackId::new(2));
        let mut history = History::new();
        history.record_change(project_of(1), vec![clip, track], None, Instant::now(), false);
        let undone = history.undo(&project_of(2), Vec::new()).unwrap();
        assert_eq!(undone.selection, vec![clip, track], "a track too");
        let redone = history.redo(&undone.project, vec![clip]).unwrap();
        assert_eq!(
            redone.selection,
            Vec::<SelectionItem>::new(),
            "what was selected when it was undone"
        );
    }

    #[test]
    fn an_edit_after_an_undo_ends_what_could_have_been_redone() {
        let mut history = History::new();
        let now = Instant::now();
        record(&mut history, 1, None, now, false);
        history.undo(&project_of(2), Vec::new()).unwrap();
        assert!(history.can_redo());
        record(&mut history, 1, None, now, false);
        assert!(!history.can_redo());
    }

    // --- one step for one gesture ---

    #[test]
    fn edits_about_the_same_thing_made_while_the_mouse_button_is_down_are_one_step() {
        let mut history = History::new();
        let start = Instant::now();
        record(&mut history, 1, key("speed"), start, true);
        // Seconds pass with the button held down and nothing changing; then the drag goes on.
        record(&mut history, 2, key("speed"), start + Duration::from_secs(5), true);
        record(&mut history, 3, key("speed"), start + Duration::from_secs(6), true);

        let undone = history.undo(&project_of(4), Vec::new()).unwrap();
        assert_eq!(
            track_count(&undone.project),
            1,
            "the project from before the whole drag"
        );
        assert!(!history.can_undo(), "it was a single step");
    }

    #[test]
    fn edits_about_the_same_thing_in_quick_succession_are_one_step_and_a_pause_makes_a_new_one() {
        let mut history = History::new();
        let start = Instant::now();
        record(&mut history, 1, key("text"), start, false);
        record(&mut history, 2, key("text"), start + Duration::from_millis(300), false);
        record(&mut history, 3, key("text"), start + Duration::from_millis(900), false);
        // The last edit was 1.2 seconds ago: a new step.
        record(&mut history, 4, key("text"), start + Duration::from_millis(2100), false);

        let latest = history.undo(&project_of(5), Vec::new()).unwrap();
        assert_eq!(track_count(&latest.project), 4);
        let earlier = history.undo(&latest.project, Vec::new()).unwrap();
        assert_eq!(track_count(&earlier.project), 1, "the first three edits are one step");
        assert!(!history.can_undo());
    }

    #[test]
    fn edits_about_the_same_thing_are_one_step_however_far_apart_when_the_history_is_asked_to_merge_them_whenever_they_are_made()
     {
        let mut history = History::new();
        history.merge_edits_whenever_they_are_made();
        let start = Instant::now();
        record(&mut history, 1, key("text"), start, false);
        record(&mut history, 2, key("text"), start + Duration::from_secs(3600), false);
        record(&mut history, 3, None, start + Duration::from_secs(3601), false);

        assert!(
            history.undo(&project_of(4), Vec::new()).is_some(),
            "what has no key is a step of its own"
        );
        let earlier = history.undo(&project_of(3), Vec::new()).unwrap();
        assert_eq!(
            track_count(&earlier.project),
            1,
            "the two edits an hour apart are one step"
        );
        assert!(!history.can_undo());
    }

    #[test]
    fn edits_about_different_things_are_different_steps() {
        let mut history = History::new();
        let start = Instant::now();
        record(&mut history, 1, key("speed"), start, true);
        record(&mut history, 2, key("gain"), start, true);
        record(&mut history, 3, None, start, true);
        record(&mut history, 4, None, start, true);
        let mut steps = 0;
        let mut project = project_of(5);
        while let Some(restored) = history.undo(&project, Vec::new()) {
            project = restored.project;
            steps += 1;
        }
        assert_eq!(
            steps, 4,
            "an edit with no key is never joined, not even to another with no key"
        );
    }

    #[test]
    fn an_edit_after_an_undo_is_not_joined_to_the_step_before_the_one_that_was_undone() {
        let mut history = History::new();
        let start = Instant::now();
        record(&mut history, 1, key("speed"), start, true);
        record(&mut history, 2, None, start, true);
        history.undo(&project_of(3), Vec::new()).unwrap();
        // The same kind of edit as the step that is now the latest, straight away: still its own step.
        record(&mut history, 2, key("speed"), start, true);
        let mut steps = 0;
        let mut project = project_of(3);
        while let Some(restored) = history.undo(&project, Vec::new()) {
            project = restored.project;
            steps += 1;
        }
        assert_eq!(steps, 2);
    }

    // --- saved or not ---

    #[test]
    fn an_edit_makes_the_project_unsaved_and_saving_makes_it_saved_again() {
        let mut history = History::new();
        record(&mut history, 1, None, Instant::now(), false);
        assert!(!history.is_saved());
        history.mark_saved();
        assert!(history.is_saved());
    }

    #[test]
    fn undoing_back_to_the_state_that_was_saved_counts_as_saved_and_going_past_it_does_not() {
        let mut history = History::new();
        let now = Instant::now();
        record(&mut history, 1, None, now, false); // 1 -> 2
        history.mark_saved();
        record(&mut history, 2, None, now, false); // 2 -> 3
        assert!(!history.is_saved());

        let back_to_saved = history.undo(&project_of(3), Vec::new()).unwrap();
        assert!(history.is_saved(), "that is the project that was saved");
        let before_saved = history.undo(&back_to_saved.project, Vec::new()).unwrap();
        assert!(!history.is_saved(), "that is from before it was saved");
        history.redo(&before_saved.project, Vec::new()).unwrap();
        assert!(history.is_saved(), "and redoing brings it back");
    }

    #[test]
    fn more_edits_joined_to_a_step_make_the_project_unsaved_again_even_if_it_was_saved_part_way() {
        let mut history = History::new();
        let start = Instant::now();
        record(&mut history, 1, key("speed"), start, true);
        history.mark_saved();
        record(&mut history, 2, key("speed"), start, true); // joined to the step that was saved
        assert!(!history.is_saved());
    }

    #[test]
    fn a_new_history_after_a_reset_is_saved_has_nothing_to_undo_and_has_a_new_generation() {
        let mut history = History::new();
        let generation = history.generation();
        record(&mut history, 1, None, Instant::now(), false);
        history.reset();
        assert!(!history.can_undo() && !history.can_redo() && history.is_saved());
        assert_ne!(history.generation(), generation);
    }

    #[test]
    fn only_the_latest_steps_are_kept() {
        let mut history = History::new();
        let now = Instant::now();
        for step in 0..MOST_STEPS_KEPT + 30 {
            history.record_change(project_of(1 + step % 3), Vec::new(), None, now, false);
        }
        let mut steps = 0;
        let mut project = project_of(1);
        while let Some(restored) = history.undo(&project, Vec::new()) {
            project = restored.project;
            steps += 1;
        }
        assert_eq!(steps, MOST_STEPS_KEPT);
    }
}
