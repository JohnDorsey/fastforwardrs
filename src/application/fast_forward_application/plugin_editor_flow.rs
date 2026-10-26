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
use std::time::{Duration, Instant};

use egui::Context;

use super::FastForwardApplication;
use crate::application::commands::Command;
use crate::plugin_editor::protocol::ParameterEdit;
use crate::plugin_editor::sessions::{EditorWindows, WhatToOpen, WindowEvent, WindowKey};
use crate::plugin_editor::values::{number_for_a_plugin, value_for_a_plugin_number};
use crate::plugin_trust::{FolderStanding, folder_of};
use crate::project::{
    AnimationTarget, EffectInstanceId, EffectKind, InstalledPlugin, PluginAddress, PropertyId, TimelineFrameIndex,
};

/// How often the window is told what the project says (while something changes it: the playhead moves over keyframes).
const HOW_OFTEN_TO_TELL_THE_VALUES: Duration = Duration::from_millis(25);

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
        let PluginAddress::Clap { id } = &plugin.address else {
            return Err("only a CLAP plugin has a window".to_owned());
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
        let what = WhatToOpen {
            library: plugin.library.clone(),
            plugin_id: id.clone(),
            title,
            values,
            state: None,
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
        self.interface.plugin_editors.windows.open(key, &what)
    }

    pub(super) fn close_a_plugin_editor(&mut self, target: AnimationTarget, instance: EffectInstanceId) {
        self.interface
            .plugin_editors
            .windows
            .close(WindowKey { target, instance });
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
                self.interface.plugin_editors.states.insert(key, state);
            }
            WindowEvent::StateChanged => self.interface.plugin_editors.windows.ask_for_the_state(key),
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
            }
            WindowEvent::Failed(reason) => {
                self.interface
                    .plugin_editors
                    .gestures
                    .retain(|(gesture_key, _)| *gesture_key != key);
                self.interface.plugin_editors.projects.remove(&key);
                self.interface
                    .plugin_editors
                    .problems
                    .insert(key, format!("the plugin's window ended: {reason}"));
            }
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
            self.interface.plugin_editors.windows.tell_the_values(key, &values);
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
}
