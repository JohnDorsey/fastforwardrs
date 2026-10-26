// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor-window program ([`super::program`]) does with a plugin's window, said once ([`PluginWindow`]) and done for each kind of plugin: a CLAP plugin's ([`ClapWindow`], the
//! editor of the `clap-host` crate) and an LV2 plugin's ([`Lv2Window`], the user interface of the `lv2-host` crate; section 9 of `docs/engine-phase-11-lv2-design.md`).
//!
//! **The program and the protocol know nothing of which kind it is** (P4 of the design of phase 10): a window is for a library, an id and a set of values, and the numbers that it
//! takes and gives are the ones that the project calls the *handles* of a plugin's values (a CLAP parameter's id, an LV2 control port's index).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use clap_host::{Editor, EditorEvent, ParameterEdit, WindowSize};
use lv2_host::{PluginDescription, UiDescription, UiEvent, UiWindow};
use winit::window::Window;

/// A plugin's window, as the program that shows it uses it.
pub trait PluginWindow {
    /// Whether the plugin keeps a state that the window can change (and the project keeps it).
    fn has_state(&self) -> bool;

    /// Whether a window of the kind `api` (`win32`, `x11` or `cocoa`), floating or embedded, can be made.
    fn can_make_a_window(&mut self, api: &str, floating: bool) -> bool;

    /// Makes the window (not shown yet) and says how big it is. **`parent` is the window of the program that it is to be put in, for the kinds of plugin that need it at once** (LV2
    /// makes its window in its parent; CLAP is given its parent after).
    fn make_the_window(
        &mut self,
        api: &str,
        floating: bool,
        scale: f64,
        parent: Option<Arc<Window>>,
    ) -> Result<WindowSize, String>;

    /// Puts the plugin's window in `window` (for the kinds of plugin that did not need it when the window was made).
    fn put_the_window_in_a_window(&mut self, window: Arc<Window>) -> Result<(), String>;

    /// Gives a floating window a title to use, if it likes.
    fn suggest_a_title(&mut self, title: &str);

    fn show(&mut self) -> Result<(), String>;

    /// The size that is nearest to `width` by `height` that the window can be, if the plugin says.
    fn adjust_the_size(&mut self, width: u32, height: u32) -> Option<(u32, u32)>;

    /// How big the plugin says its window is now.
    fn size_of_the_window(&mut self) -> Option<(u32, u32)>;

    /// Tells the plugin that the screen's scale is this now.
    fn set_the_scale(&mut self, scale: f64);

    /// Tells the plugin that its window is this big now. Whether it took it.
    fn set_the_size(&mut self, width: u32, height: u32) -> bool;

    fn destroy_the_window(&mut self);

    /// Tells the plugin what these values are now (`(handle, value)`); what it says back at once is returned. **The values may be told before the window is made.**
    fn tell_the_values(&mut self, values: &[(u32, f64)]) -> Vec<EditorEvent>;

    /// What the plugin did since the last time (edits, a change of state, a request for another size, a close).
    fn poll(&mut self) -> Vec<EditorEvent>;

    fn save_the_state(&mut self) -> Result<Vec<u8>, String>;

    fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), String>;

    /// Forgets that the state changed (it was changed by being given a state, and not by a hand).
    fn forget_that_the_state_changed(&mut self);
}

/// The window of a CLAP plugin.
pub struct ClapWindow {
    editor: Editor,
}

impl ClapWindow {
    pub fn new(editor: Editor) -> Self {
        Self { editor }
    }
}

impl PluginWindow for ClapWindow {
    fn has_state(&self) -> bool {
        self.editor.has_state()
    }

    fn can_make_a_window(&mut self, api: &str, floating: bool) -> bool {
        self.editor.can_make_a_window(api, floating)
    }

    fn make_the_window(
        &mut self,
        api: &str,
        floating: bool,
        scale: f64,
        _parent: Option<Arc<Window>>,
    ) -> Result<WindowSize, String> {
        self.editor
            .make_the_window(api, floating, scale)
            .map_err(|error| error.to_string())
    }

    fn put_the_window_in_a_window(&mut self, window: Arc<Window>) -> Result<(), String> {
        self.editor
            .put_the_window_in_a_window(window)
            .map_err(|error| error.to_string())
    }

    fn suggest_a_title(&mut self, title: &str) {
        self.editor.suggest_a_title(title);
    }

    fn show(&mut self) -> Result<(), String> {
        self.editor.show().map_err(|error| error.to_string())
    }

    fn adjust_the_size(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        self.editor.adjust_the_size(width, height)
    }

    fn size_of_the_window(&mut self) -> Option<(u32, u32)> {
        self.editor.size_of_the_window()
    }

    fn set_the_scale(&mut self, scale: f64) {
        self.editor.set_the_scale(scale);
    }

    fn set_the_size(&mut self, width: u32, height: u32) -> bool {
        self.editor.set_the_size(width, height)
    }

    fn destroy_the_window(&mut self) {
        self.editor.destroy_the_window();
    }

    fn tell_the_values(&mut self, values: &[(u32, f64)]) -> Vec<EditorEvent> {
        self.editor.tell_the_values(values)
    }

    fn poll(&mut self) -> Vec<EditorEvent> {
        self.editor.poll()
    }

    fn save_the_state(&mut self) -> Result<Vec<u8>, String> {
        self.editor.save_the_state().map_err(|error| error.to_string())
    }

    fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.editor.load_the_state(bytes).map_err(|error| error.to_string())
    }

    fn forget_that_the_state_changed(&mut self) {
        self.editor.forget_that_the_state_changed();
    }
}

/// What an LV2 window did, as what an editor says: a hand that takes hold of a control and lets go of it is the begin and the end of a gesture, a value that is written is a value, a
/// request for another size is one. (A pure function: the order of the events is the order in which the window did them.)
pub fn editor_events_of_a_user_interface(events: impl IntoIterator<Item = UiEvent>) -> Vec<EditorEvent> {
    events
        .into_iter()
        .map(|event| match event {
            UiEvent::Write { port, value } => EditorEvent::Edit(ParameterEdit::Value {
                id: port,
                value: f64::from(value),
            }),
            UiEvent::Touch { port, grabbed: true } => EditorEvent::Edit(ParameterEdit::Begin { id: port }),
            UiEvent::Touch { port, grabbed: false } => EditorEvent::Edit(ParameterEdit::End { id: port }),
            UiEvent::ResizeRequested { width, height } => EditorEvent::ResizeRequested { width, height },
        })
        .collect()
}

/// The window of an LV2 plugin: the user interface that its bundle says is of this system's kind. **The plugin itself is not run here** (the sound is made elsewhere): the window shows
/// the values that it is told, and says what a hand did. It has no state of its own (the one that the project keeps is given back as it was given).
pub struct Lv2Window {
    plugin: PluginDescription,
    /// What the window was last told each control port is: given to the user interface as soon as it is made (the values are told before there is a window).
    told: BTreeMap<u32, f64>,
    /// The state that the project gave (the window cannot change it, so it is what it says if it is asked).
    state_given: Option<Vec<u8>>,
    window: Option<UiWindow>,
}

impl Lv2Window {
    /// The window of the plugin whose URI is `uri` in the bundle `bundle`, **which is read here** (its Turtle: nothing is loaded until the window is made). An error says why the plugin
    /// has no window that can be shown.
    pub fn new(bundle: &Path, uri: &str) -> Result<Self, String> {
        let description = lv2_host::describe_bundle(bundle).map_err(|error| error.to_string())?;
        let plugin = description
            .plugins
            .into_iter()
            .find(|plugin| plugin.uri == uri)
            .ok_or_else(|| format!("the bundle has no plugin called {uri}"))?;
        if plugin.user_interfaces.is_empty() {
            return Err("the plugin has no window".to_owned());
        }
        Ok(Self {
            plugin,
            told: BTreeMap::new(),
            state_given: None,
            window: None,
        })
    }

    fn the_interface_for(&self, api: &str) -> Option<UiDescription> {
        lv2_host::window_of(&self.plugin, api).cloned()
    }

    fn values_told(&self) -> Vec<(u32, f64)> {
        self.told.iter().map(|(port, value)| (*port, *value)).collect()
    }
}

impl PluginWindow for Lv2Window {
    fn has_state(&self) -> bool {
        false
    }

    fn can_make_a_window(&mut self, api: &str, floating: bool) -> bool {
        // (A user interface is made inside a window of the program; a window of its own is made only where there is no screen, which the program asks for without asking this.)
        !floating && self.the_interface_for(api).is_some()
    }

    fn make_the_window(
        &mut self,
        api: &str,
        floating: bool,
        scale: f64,
        parent: Option<Arc<Window>>,
    ) -> Result<WindowSize, String> {
        let interface = self
            .the_interface_for(api)
            .ok_or_else(|| format!("the plugin has no window of the kind {api} that this program can show"))?;
        let values = self.values_told();
        let window = match (floating, parent) {
            (false, Some(parent)) => UiWindow::open_in_a_window(&self.plugin, &interface, parent, scale, &values),
            (false, None) => return Err("a window of an LV2 plugin is made in a window of the program".to_owned()),
            (true, _) => UiWindow::open_without_a_window(&self.plugin, &interface, &values),
        }
        .map_err(|error| error.to_string())?;
        let (width, height) = window.size();
        let size = WindowSize {
            width,
            height,
            can_resize: window.can_be_resized(),
        };
        self.window = Some(window);
        Ok(size)
    }

    fn put_the_window_in_a_window(&mut self, _window: Arc<Window>) -> Result<(), String> {
        // (It was made in the window.)
        if self.window.is_some() {
            Ok(())
        } else {
            Err("there is no window to put in it".to_owned())
        }
    }

    fn suggest_a_title(&mut self, _title: &str) {}

    fn show(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn adjust_the_size(&mut self, _width: u32, _height: u32) -> Option<(u32, u32)> {
        None
    }

    fn size_of_the_window(&mut self) -> Option<(u32, u32)> {
        self.window.as_ref().map(UiWindow::size)
    }

    fn set_the_scale(&mut self, _scale: f64) {}

    fn set_the_size(&mut self, width: u32, height: u32) -> bool {
        self.window
            .as_mut()
            .is_some_and(|window| window.set_size(width, height))
    }

    fn destroy_the_window(&mut self) {
        // (Dropping it takes the user interface down.)
        self.window = None;
    }

    fn tell_the_values(&mut self, values: &[(u32, f64)]) -> Vec<EditorEvent> {
        self.told.extend(values.iter().copied());
        if let Some(window) = &mut self.window {
            window.tell_the_values(values);
        }
        Vec::new()
    }

    fn poll(&mut self) -> Vec<EditorEvent> {
        let Some(window) = &mut self.window else {
            return Vec::new();
        };
        let closes = window.idle();
        let mut events = editor_events_of_a_user_interface(window.poll());
        if closes {
            events.push(EditorEvent::Closed { destroyed: false });
        }
        events
    }

    fn save_the_state(&mut self) -> Result<Vec<u8>, String> {
        self.state_given
            .clone()
            .ok_or_else(|| "the window of an LV2 plugin has no state of its own".to_owned())
    }

    fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.state_given = Some(bytes.to_vec());
        Ok(())
    }

    fn forget_that_the_state_changed(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_user_interface_did_is_what_an_editor_says_in_the_order_that_it_was_done() {
        let events = editor_events_of_a_user_interface([
            UiEvent::Touch { port: 2, grabbed: true },
            UiEvent::Write { port: 2, value: 0.75 },
            UiEvent::Touch {
                port: 2,
                grabbed: false,
            },
            UiEvent::ResizeRequested {
                width: 360,
                height: 240,
            },
        ]);

        assert_eq!(
            events,
            vec![
                EditorEvent::Edit(ParameterEdit::Begin { id: 2 }),
                EditorEvent::Edit(ParameterEdit::Value { id: 2, value: 0.75 }),
                EditorEvent::Edit(ParameterEdit::End { id: 2 }),
                EditorEvent::ResizeRequested {
                    width: 360,
                    height: 240
                },
            ]
        );
        assert!(editor_events_of_a_user_interface([]).is_empty());
    }
}
