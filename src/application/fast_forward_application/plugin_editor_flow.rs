// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The windows of plugins (phase 10, piece 10.3; `docs/engine-phase-10-editors-design.md`): opening one for a plugin effect, what a person does in it becoming edits of the project, what the
//! project says reaching it, and closing it when it is not wanted.
//!
//! **A window is a window of another program** (see [`crate::plugin_editor`]), and it is shown only for a plugin that the editor would run itself: its folder has been said yes to. What a
//! person does in it arrives as parameter edits, and each becomes the command that the panel's control for the same value would have made (`SetEffectValue`), so that undo, keyframes at the
//! playhead and the history's joining of one gesture into one step are the ones the panel has. What the project says (a change in the panel, an undo, keyframes while the project plays) is
//! told to the window, except for a parameter that a hand is holding.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::plugin_editor::program::PluginKind;
use crate::plugin_editor::protocol::ParameterEdit;
use crate::plugin_editor::sessions::{EditorWindows, WhatToOpen, WindowEvent, WindowKey};
use crate::plugin_editor::values::{number_for_a_plugin, value_for_a_plugin_number};
use crate::plugin_trust::{FolderStanding, folder_of};
use crate::project::{
    AnimationTarget, EffectInstanceId, EffectKind, InstalledPlugin, PluginAddress, PropertyId, TimelineFrameIndex,
};

/// How often the window is told what the project says (while something changes it: the playhead moves over keyframes).
const HOW_OFTEN_TO_TELL_THE_VALUES: Duration = Duration::from_millis(25);

/// How soon after it was asked for the state of a plugin is asked for again, while its window goes on saying that the state changed (a hand that drags a knob makes a plugin say so at every
/// step, and a state may be a megabyte).
const HOW_OFTEN_TO_ASK_FOR_THE_STATE: Duration = Duration::from_millis(150);

/// For how long after a window was given a state, or told values, what it says of its state is taken to be its answer to that, and not a person's edit (a plugin that is given a state or a
/// value may say that its state changed). **A state that is kept is an edit of the project, and an edit that nobody made would end what can be redone.**
const HOW_LONG_WHAT_THE_PROJECT_TOLD_IS_BEING_TAKEN: Duration = Duration::from_millis(500);

/// The environment variable that makes the windows that are opened have no window at all: **for the tests**, which have no screen.
pub const HEADLESS_VARIABLE: &str = "FASTFORWARDRS_EDITOR_HEADLESS";

/// The windows of plugins that are open, and what is known of them.
#[derive(Default)]
pub(super) struct PluginEditors {
    windows: EditorWindows,
    /// Why a window could not be opened or ended, to be said in the box of the effect, until it is opened again.
    problems: HashMap<WindowKey, String>,
    /// The project that each window was opened for (see `History::generation`): a window of another project than the one that is open is closed.
    projects: HashMap<WindowKey, u64>,
    /// The state that each window last said the plugin has (base 64).
    states: HashMap<WindowKey, String>,
    /// The state (the bytes) that each window is known to have: what its plugin last said, or what it was last given. (The same `Arc` as the effect's, so that nothing is compared byte by
    /// byte while they are the same.)
    states_the_windows_have: HashMap<WindowKey, Arc<[u8]>>,
    /// The windows whose plugin said that its state changed, and has not been asked for it since.
    states_that_changed: HashSet<WindowKey>,
    /// When the state of each window was last asked for.
    states_asked_at: HashMap<WindowKey, Instant>,
    /// The windows whose state was asked for because their plugin said that it changed: the answer is kept in the project. (The answer to any other asking is not: it is not a person's edit.)
    states_awaited: HashSet<WindowKey>,
    /// When each window was last given a state or told values.
    told_at_by_window: HashMap<WindowKey, Instant>,
    /// The gestures that are going on: a hand is holding the parameter (`begin` came, `end` has not).
    gestures: HashSet<(WindowKey, u32)>,
    told_at: Option<Instant>,
    /// Whether the windows that are opened have no window at all, if something said so (the environment variable says so otherwise).
    headless: Option<bool>,
}

impl PluginEditors {
    pub(super) fn is_open(&self, target: AnimationTarget, instance: EffectInstanceId) -> bool {
        self.windows.is_open(WindowKey { target, instance })
    }

    pub(super) fn state_of(&self, target: AnimationTarget, instance: EffectInstanceId) -> Option<&str> {
        self.states.get(&WindowKey { target, instance }).map(String::as_str)
    }

    pub(super) fn problem_with(&self, target: AnimationTarget, instance: EffectInstanceId) -> Option<&str> {
        self.problems.get(&WindowKey { target, instance }).map(String::as_str)
    }

    /// Asks the window for the plugin's state: what it said before is forgotten, so that what is there when the answer comes is the answer.
    pub(super) fn ask_for_the_state(&mut self, target: AnimationTarget, instance: EffectInstanceId) {
        let key = WindowKey { target, instance };
        self.states.remove(&key);
        self.windows.ask_for_the_state(key);
    }

    /// Whether what the window `key` says of its state at `now` is its answer to what the project told it a moment ago (its state, its values, what it began with), and not a person's
    /// edit.
    fn what_it_says_is_the_answer_to_what_the_project_told(&self, key: WindowKey, now: Instant) -> bool {
        self.told_at_by_window.get(&key).is_some_and(|told_at| {
            now.saturating_duration_since(*told_at) < HOW_LONG_WHAT_THE_PROJECT_TOLD_IS_BEING_TAKEN
        })
    }

    /// Notes that the plugin of the window `key` said, at `now`, that its state changed: unless it is what a plugin says to what the project just told it, which is not a person's edit.
    fn note_that_the_state_changed(&mut self, key: WindowKey, now: Instant) {
        if !self.what_it_says_is_the_answer_to_what_the_project_told(key, now) {
            self.states_that_changed.insert(key);
        }
    }

    /// Asks for the state of the windows whose plugin said that it changed, not more often than [`HOW_OFTEN_TO_ASK_FOR_THE_STATE`] for one window; the answers are kept.
    fn ask_for_the_states_that_changed(&mut self, now: Instant) {
        let due: Vec<WindowKey> = self
            .states_that_changed
            .iter()
            .copied()
            .filter(|key| self.the_state_may_be_asked_for(*key, now))
            .collect();
        for key in due {
            self.states_that_changed.remove(&key);
            self.states_awaited.insert(key);
            self.states_asked_at.insert(key, now);
            self.windows.ask_for_the_state(key);
        }
    }

    /// Whether the state of the window `key` may be asked for at `now`: it was not asked for a moment ago.
    fn the_state_may_be_asked_for(&self, key: WindowKey, now: Instant) -> bool {
        self.states_asked_at
            .get(&key)
            .is_none_or(|asked_at| now.saturating_duration_since(*asked_at) >= HOW_OFTEN_TO_ASK_FOR_THE_STATE)
    }

    /// Closes the window `key`. What a person did last in it is the project's before the window is gone: the plugin is asked for its state first if it said that it had changed (and the
    /// answer is kept).
    fn close_asking_for_the_state_that_changed(&mut self, key: WindowKey) {
        if self.states_that_changed.remove(&key) {
            self.states_awaited.insert(key);
            self.windows.ask_for_the_state(key);
        }
        self.windows.close(key);
    }

    /// Forgets what is known of the state of the window `key` (it is closed).
    fn forget_the_state_of(&mut self, key: WindowKey) {
        self.states_the_windows_have.remove(&key);
        self.states_that_changed.remove(&key);
        self.states_asked_at.remove(&key);
        self.states_awaited.remove(&key);
        self.told_at_by_window.remove(&key);
    }

    /// Uses this program for the windows, and (`headless`) opens them with no window.
    pub(super) fn use_this_program(&mut self, program: std::path::PathBuf, headless: bool) {
        self.windows = EditorWindows::with_program(program);
        self.headless = Some(headless);
    }

    /// Whether a hand is holding a parameter in a window: the history counts the edits that come while it is as one gesture, as it counts those that are made while the mouse button is
    /// down.
    pub(super) fn a_gesture_is_open(&self) -> bool {
        !self.gestures.is_empty()
    }
}

/// `values` without the parameters that a hand is holding in the window `key`: the project is catching up with the hand, and what the project says now is not what the hand is making.
fn without_those_that_a_hand_holds(
    values: Vec<(u32, f64)>,
    key: WindowKey,
    gestures: &HashSet<(WindowKey, u32)>,
) -> Vec<(u32, f64)> {
    values
        .into_iter()
        .filter(|(id, _)| !gestures.contains(&(key, *id)))
        .collect()
}

impl FastForwardApplication {
    /// Says yes to the folder that the plugin library `library` is in (**for the tests**, in place of the question that is asked).
    #[cfg(test)]
    pub(super) fn approve_the_plugin_folder_of_the_tests(&mut self, library: std::path::PathBuf) {
        self.environment.plugin_host.approve_folder(&folder_of(&library));
    }

    /// Says no to the folder that the plugin library `library` is in (**for the tests**).
    #[cfg(test)]
    pub(super) fn decline_the_plugin_folder_of_the_tests(&mut self, library: std::path::PathBuf) {
        self.environment.plugin_host.decline_folder(&folder_of(&library));
    }

    /// The kind of effect and the plugin that the effect `instance` of `target` is, if it is a plugin that is installed.
    fn the_plugin_of_an_effect(
        &self,
        target: AnimationTarget,
        instance: EffectInstanceId,
    ) -> Option<(EffectKind, &'static InstalledPlugin)> {
        let kind = self.document.project.effects_of(target)?.instance(instance)?.kind();
        Some((kind, kind.installed_plugin()?))
    }

    /// The values of the plugin's parameters at `frame`, as the project says: `(id of the parameter, value)`.
    fn the_values_of_a_plugin_at(
        &self,
        target: AnimationTarget,
        instance: EffectInstanceId,
        kind: EffectKind,
        plugin: &InstalledPlugin,
        frame: TimelineFrameIndex,
    ) -> Vec<(u32, f64)> {
        plugin
            .value_handles
            .iter()
            .enumerate()
            .filter_map(|(slot, id)| {
                let property = PropertyId::effect_value(kind, instance, u8::try_from(slot).ok()?);
                let value = self.document.project.property_value_at(target, property, frame)?;
                Some((*id, f64::from(number_for_a_plugin(&value))))
            })
            .collect()
    }

    /// Opens the window of the plugin effect `instance` of `target`, or says why it cannot be (in the box of the effect).
    pub(super) fn open_a_plugin_editor(&mut self, target: AnimationTarget, instance: EffectInstanceId) {
        let key = WindowKey { target, instance };
        self.interface.plugin_editors.problems.remove(&key);
        match self.start_a_plugin_editor(key) {
            Ok(()) => {
                let generation = self.document.history.generation();
                self.interface.plugin_editors.projects.insert(key, generation);
            }
            Err(why) => {
                self.interface.plugin_editors.problems.insert(key, why);
            }
        }
    }

    fn start_a_plugin_editor(&mut self, key: WindowKey) -> Result<(), String> {
        let (kind, plugin) = self
            .the_plugin_of_an_effect(key.target, key.instance)
            .ok_or("it is not a plugin that is installed")?;
        if !plugin.has_a_window {
            return Err("the plugin has no window".to_owned());
        }
        let (kind_of_plugin, id) = match &plugin.address {
            PluginAddress::Clap { id } => (PluginKind::Clap, id),
            PluginAddress::Lv2 { uri } => (PluginKind::Lv2, uri),
            _ => return Err("only a CLAP plugin or an LV2 plugin has a window".to_owned()),
        };
        // (A library is loaded only from a folder that has been said yes to: a window loads it too.)
        let folder = folder_of(&plugin.library);
        match self.environment.plugin_host.standing_of(&folder) {
            FolderStanding::Approved => {}
            FolderStanding::NotAsked => {
                return Err(
                    "nobody has said yes to the folder of the plugin yet: it is asked when the sound needs it"
                        .to_owned(),
                );
            }
            FolderStanding::Declined => return Err("the folder of the plugin was said no to".to_owned()),
        }
        let frame = self.frame_at_the_playhead(Instant::now());
        let values = self.the_values_of_a_plugin_at(key.target, key.instance, kind, plugin, frame);
        let title = format!("{} \u{2014} fastforwardrs", plugin.descriptor.name);
        let state = self
            .document
            .project
            .effects_of(key.target)
            .and_then(|chain| chain.instance(key.instance))
            .and_then(|effect| effect.state().cloned());
        let what = WhatToOpen {
            library: plugin.library.clone(),
            plugin_id: id.clone(),
            kind: kind_of_plugin,
            title,
            values,
            state: state.as_deref().map(crate::base64::encode),
            environment: self
                .environment
                .plugin_host
                .environment_for_a_program_that_loads(&plugin.library),
            headless: self
                .interface
                .plugin_editors
                .headless
                .unwrap_or_else(|| std::env::var_os(HEADLESS_VARIABLE).is_some()),
        };
        self.interface.plugin_editors.windows.open(key, &what)?;
        // (What the window begins with is what it has: the project's state is not given to it again.)
        match state {
            Some(state) => {
                self.interface.plugin_editors.states_the_windows_have.insert(key, state);
            }
            None => {
                self.interface.plugin_editors.states_the_windows_have.remove(&key);
            }
        }
        Ok(())
    }

    pub(super) fn close_a_plugin_editor(&mut self, target: AnimationTarget, instance: EffectInstanceId) {
        self.interface
            .plugin_editors
            .close_asking_for_the_state_that_changed(WindowKey { target, instance });
    }

    /// Does what the windows did, and tells them what the project says, and closes those that are not wanted. **Every frame of the window.**
    pub(super) fn look_after_the_plugin_editors(
        &mut self,
        now: Instant,
        current_frame: TimelineFrameIndex,
        context: &Context,
    ) {
        let editors = &mut self.interface.plugin_editors;
        if editors.windows.keys().is_empty() && editors.gestures.is_empty() {
            return;
        }
        self.close_the_plugin_editors_that_are_not_wanted();
        for (key, event) in self.interface.plugin_editors.windows.poll() {
            self.do_what_a_plugin_editor_said(key, event, now, context);
        }
        self.interface.plugin_editors.ask_for_the_states_that_changed(now);
        self.give_the_plugin_editors_the_state_that_the_project_has(now);
        // (A window that is open keeps the window of the editor awake: what a hand does in it has to be seen.)
        context.request_repaint_after(Duration::from_millis(30));
        let editors = &self.interface.plugin_editors;
        if editors
            .told_at
            .is_some_and(|told_at| now.saturating_duration_since(told_at) < HOW_OFTEN_TO_TELL_THE_VALUES)
        {
            return;
        }
        self.interface.plugin_editors.told_at = Some(now);
        self.tell_the_plugin_editors_what_the_project_says(current_frame);
    }

    /// Closes the windows of effects that are gone (taken off, or the project is another).
    fn close_the_plugin_editors_that_are_not_wanted(&mut self) {
        let generation = self.document.history.generation();
        let unwanted: Vec<WindowKey> = self
            .interface
            .plugin_editors
            .windows
            .keys()
            .into_iter()
            .filter(|key| {
                self.interface.plugin_editors.projects.get(key) != Some(&generation)
                    || self.the_plugin_of_an_effect(key.target, key.instance).is_none()
            })
            .collect();
        for key in unwanted {
            self.interface.plugin_editors.windows.close(key);
        }
    }

    pub(super) fn do_what_a_plugin_editor_said(
        &mut self,
        key: WindowKey,
        event: WindowEvent,
        now: Instant,
        context: &Context,
    ) {
        match event {
            WindowEvent::Ready { .. } | WindowEvent::ParametersChanged => {}
            WindowEvent::State(state) => {
                self.interface.plugin_editors.states.insert(key, state.clone());
                // (Kept if it answers an asking because the plugin said its state changed, or if it is what a window says as it ends, having said so and not been asked: the answer to any
                // other asking is not a person's edit.)
                let editors = &mut self.interface.plugin_editors;
                let was_awaited = editors.states_awaited.remove(&key);
                let was_a_change_not_yet_asked_for = editors.states_that_changed.remove(&key);
                if was_awaited || was_a_change_not_yet_asked_for {
                    self.keep_the_state_that_a_plugin_editor_said(key, &state, now, context);
                }
            }
            WindowEvent::StateChanged => self.interface.plugin_editors.note_that_the_state_changed(key, now),
            WindowEvent::Edit(ParameterEdit::Begin { id }) => {
                self.interface.plugin_editors.gestures.insert((key, id));
            }
            WindowEvent::Edit(ParameterEdit::End { id }) => {
                self.interface.plugin_editors.gestures.remove(&(key, id));
            }
            WindowEvent::Edit(ParameterEdit::Value { id, value }) => {
                self.apply_an_edit_of_a_plugin_editor(key, id, value, now, context);
            }
            WindowEvent::Closed => {
                self.interface
                    .plugin_editors
                    .gestures
                    .retain(|(gesture_key, _)| *gesture_key != key);
                self.interface.plugin_editors.projects.remove(&key);
                self.interface.plugin_editors.forget_the_state_of(key);
            }
            WindowEvent::Failed(reason) => {
                self.interface
                    .plugin_editors
                    .gestures
                    .retain(|(gesture_key, _)| *gesture_key != key);
                self.interface.plugin_editors.projects.remove(&key);
                self.interface.plugin_editors.forget_the_state_of(key);
                self.interface
                    .plugin_editors
                    .problems
                    .insert(key, format!("the plugin's window ended: {reason}"));
            }
        }
    }

    /// The state that a plugin said (base 64) becomes the state of its effect, if it is not what the effect has: the window is the project's, and what a person did in it is kept.
    fn keep_the_state_that_a_plugin_editor_said(
        &mut self,
        key: WindowKey,
        said: &str,
        now: Instant,
        context: &Context,
    ) {
        let Ok(bytes) = crate::base64::decode(said) else {
            return;
        };
        if self.the_plugin_of_an_effect(key.target, key.instance).is_none() {
            return;
        }
        let the_effects_state = self
            .document
            .project
            .effects_of(key.target)
            .and_then(|chain| chain.instance(key.instance))
            .and_then(|effect| effect.state().cloned());
        let state: Arc<[u8]> = match the_effects_state {
            // (What the effect has already: the same, and the effect's own bytes are the ones that are kept.)
            Some(had) if *had == *bytes => had,
            _ => Arc::from(bytes),
        };
        self.interface
            .plugin_editors
            .states_the_windows_have
            .insert(key, Arc::clone(&state));
        let command = Command::SetPluginState {
            owner: key.target.into(),
            instance: key.instance,
            state: Some(state),
        };
        self.execute_all(vec![command], now, context);
    }

    /// Gives a window the state that the project has for its effect when that is not what the window has (an undo, a redo): the window shows what the project says.
    fn give_the_plugin_editors_the_state_that_the_project_has(&mut self, now: Instant) {
        for key in self.interface.plugin_editors.windows.keys() {
            let Some(state) = self
                .document
                .project
                .effects_of(key.target)
                .and_then(|chain| chain.instance(key.instance))
                .and_then(|effect| effect.state().cloned())
            else {
                continue;
            };
            let editors = &mut self.interface.plugin_editors;
            match editors.states_the_windows_have.get(&key) {
                Some(has) if Arc::ptr_eq(has, &state) => continue,
                Some(has) if **has == *state => {
                    // (The same bytes in another `Arc`: the next look is quick.)
                    editors.states_the_windows_have.insert(key, state);
                    continue;
                }
                _ => {}
            }
            editors.windows.give_the_state(key, &crate::base64::encode(&state));
            editors.states_the_windows_have.insert(key, state);
            editors.told_at_by_window.insert(key, now);
            // (What the window had that changed is lost to what the project says.)
            editors.states_that_changed.remove(&key);
            editors.states_awaited.remove(&key);
        }
    }

    /// A value that a hand made in a window becomes the value of the effect.
    fn apply_an_edit_of_a_plugin_editor(
        &mut self,
        key: WindowKey,
        id: u32,
        number: f64,
        now: Instant,
        context: &Context,
    ) {
        let Some((kind, plugin)) = self.the_plugin_of_an_effect(key.target, key.instance) else {
            return;
        };
        let Some(slot) = plugin.value_handles.iter().position(|handle| *handle == id) else {
            return;
        };
        let Some(value) = value_for_a_plugin_number(&plugin.descriptor, slot, number) else {
            return;
        };
        let Ok(slot) = u8::try_from(slot) else {
            return;
        };
        let command = Command::SetEffectValue {
            target: key.target,
            property: PropertyId::effect_value(kind, key.instance, slot),
            value,
        };
        self.execute_all(vec![command], now, context);
    }

    /// Tells every open window what the project says its parameters are at the playhead (the window knows what it was told last, and only what is not that is said), except those that a
    /// hand is holding.
    fn tell_the_plugin_editors_what_the_project_says(&mut self, current_frame: TimelineFrameIndex) {
        for key in self.interface.plugin_editors.windows.keys() {
            let Some((kind, plugin)) = self.the_plugin_of_an_effect(key.target, key.instance) else {
                continue;
            };
            let values = without_those_that_a_hand_holds(
                self.the_values_of_a_plugin_at(key.target, key.instance, kind, plugin, current_frame),
                key,
                &self.interface.plugin_editors.gestures,
            );
            let editors = &mut self.interface.plugin_editors;
            if editors.windows.tell_the_values(key, &values) {
                editors.told_at_by_window.insert(key, Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ClipId, TrackId};

    fn a_key(instance: u32) -> WindowKey {
        WindowKey {
            target: AnimationTarget::Clip(ClipId::new(TrackId::new(0), 0)),
            instance: EffectInstanceId::new(instance),
        }
    }

    #[test]
    fn the_values_that_a_hand_holds_are_left_out_and_only_in_the_window_that_it_holds_them_in() {
        let gestures: HashSet<(WindowKey, u32)> = [(a_key(0), 2)].into_iter().collect();
        let values = vec![(1, 0.5), (2, 0.75), (3, 1.0)];

        assert_eq!(
            without_those_that_a_hand_holds(values.clone(), a_key(0), &gestures),
            vec![(1, 0.5), (3, 1.0)]
        );
        assert_eq!(
            without_those_that_a_hand_holds(values.clone(), a_key(1), &gestures),
            values,
            "another window has all of them"
        );
        assert_eq!(
            without_those_that_a_hand_holds(values.clone(), a_key(0), &HashSet::new()),
            values
        );
    }

    #[test]
    fn what_a_window_says_of_its_state_just_after_the_project_told_it_something_is_not_a_persons_edit() {
        let mut editors = PluginEditors::default();
        let start = Instant::now();
        assert!(
            !editors.what_it_says_is_the_answer_to_what_the_project_told(a_key(0), start),
            "nothing was told"
        );

        editors.told_at_by_window.insert(a_key(0), start);
        assert!(
            editors.what_it_says_is_the_answer_to_what_the_project_told(a_key(0), start + Duration::from_millis(100))
        );
        assert!(
            !editors.what_it_says_is_the_answer_to_what_the_project_told(a_key(0), start + Duration::from_secs(2)),
            "a moment is not for ever"
        );
        assert!(
            !editors.what_it_says_is_the_answer_to_what_the_project_told(a_key(1), start),
            "another window was told nothing"
        );
    }

    #[test]
    fn the_state_of_a_window_is_not_asked_for_again_and_again_while_it_is_dragged() {
        let mut editors = PluginEditors::default();
        let start = Instant::now();
        assert!(editors.the_state_may_be_asked_for(a_key(0), start), "never asked");

        editors.states_asked_at.insert(a_key(0), start);
        assert!(!editors.the_state_may_be_asked_for(a_key(0), start + Duration::from_millis(50)));
        assert!(editors.the_state_may_be_asked_for(a_key(0), start + HOW_OFTEN_TO_ASK_FOR_THE_STATE));
        assert!(editors.the_state_may_be_asked_for(a_key(1), start), "another window");
    }

    #[test]
    fn closing_a_window_whose_plugin_said_that_its_state_changed_waits_for_the_answer_and_one_that_did_not_does_not() {
        let mut editors = PluginEditors::default();
        editors.states_that_changed.insert(a_key(0));

        editors.close_asking_for_the_state_that_changed(a_key(0));
        editors.close_asking_for_the_state_that_changed(a_key(1));

        assert!(editors.states_awaited.contains(&a_key(0)), "its answer is kept");
        assert!(!editors.states_that_changed.contains(&a_key(0)));
        assert!(
            !editors.states_awaited.contains(&a_key(1)),
            "nothing changed: nothing is asked"
        );
    }

    #[test]
    fn a_change_of_state_that_comes_just_after_the_project_told_the_window_something_is_not_noted_and_one_that_comes_later_is()
     {
        let mut editors = PluginEditors::default();
        let start = Instant::now();
        editors.told_at_by_window.insert(a_key(0), start);

        editors.note_that_the_state_changed(a_key(0), start + Duration::from_millis(100));
        assert!(editors.states_that_changed.is_empty(), "the answer to what was told");

        editors.note_that_the_state_changed(a_key(0), start + Duration::from_secs(2));
        assert!(editors.states_that_changed.contains(&a_key(0)), "a person's");
        editors.note_that_the_state_changed(a_key(1), start);
        assert!(
            editors.states_that_changed.contains(&a_key(1)),
            "another window was told nothing"
        );
    }

    #[test]
    fn the_states_that_changed_are_asked_for_and_kept_but_not_asked_for_again_until_a_moment_has_gone() {
        let mut editors = PluginEditors::default();
        let start = Instant::now();
        editors.states_that_changed.insert(a_key(0));

        editors.ask_for_the_states_that_changed(start);
        assert!(editors.states_that_changed.is_empty());
        assert!(editors.states_awaited.contains(&a_key(0)), "the answer is kept");
        assert_eq!(
            editors.states_asked_at.get(&a_key(0)),
            Some(&start),
            "and when it was asked is noted"
        );

        editors.states_that_changed.insert(a_key(0));
        editors.ask_for_the_states_that_changed(start + Duration::from_millis(50));
        assert!(editors.states_that_changed.contains(&a_key(0)), "too soon: it waits");

        editors.ask_for_the_states_that_changed(start + HOW_OFTEN_TO_ASK_FOR_THE_STATE);
        assert!(editors.states_that_changed.is_empty(), "asked again");
    }
}
