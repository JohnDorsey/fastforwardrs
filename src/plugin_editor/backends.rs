// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor-window program ([`super::program`]) does with a plugin's window, said once ([`PluginWindow`]) and done for each kind of plugin: a CLAP plugin's ([`ClapWindow`], the
//! editor of the `clap-host` crate), an LV2 plugin's ([`Lv2Window`], the user interface of the `lv2-host` crate; section 9 of `docs/engine-phase-11-lv2-design.md`) and a VST3
//! plugin's ([`Vst3Window`], the `IPlugView` of the `vst3-host` crate; section 14 of `docs/engine-phase-15-vst3-design.md`).
//!
//! **The program and the protocol know nothing of which kind it is** (P4 of the design of phase 10): a window is for a library, an id and a set of values, and the numbers that it
//! takes and gives are the ones that the project calls the *handles* of a plugin's values (a CLAP parameter's id, an LV2 control port's index, a VST3 parameter's id).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap_host::{Editor, EditorEvent, ParameterEdit, WindowSize};
use lv2_host::{PluginDescription, UiDescription, UiEvent, UiWindow};
use vst3_host::{ClassId, Instance, Module, ViewEvent};
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

    /// Whether the plugin's state is not what it was when it was last looked at, for a kind of plugin that has to be looked at to know (**one last time, as the window ends**). By default
    /// a plugin says itself when its state changed, and there is nothing to look at.
    fn state_changed_since_it_was_looked_at(&mut self) -> bool {
        false
    }
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

/// What a VST3 plugin's window did, as what an editor says: the steps of a hand's gestures are the same steps, a request for another size is one, a change of the state is one, and
/// values that the plugin changed by itself (a preset chosen in its window) are values that were edited. (A pure function: the order of the events is the order in which the window
/// did them.)
pub fn editor_events_of_a_vst3_window(events: impl IntoIterator<Item = ViewEvent>) -> Vec<EditorEvent> {
    let mut said = Vec::new();
    for event in events {
        match event {
            ViewEvent::Begin { id } => said.push(EditorEvent::Edit(ParameterEdit::Begin { id })),
            ViewEvent::Value { id, plain } => said.push(EditorEvent::Edit(ParameterEdit::Value { id, value: plain })),
            ViewEvent::End { id } => said.push(EditorEvent::Edit(ParameterEdit::End { id })),
            ViewEvent::ResizeRequested { width, height } => said.push(EditorEvent::ResizeRequested { width, height }),
            ViewEvent::StateChanged => said.push(EditorEvent::StateChanged),
            ViewEvent::ValuesChanged(values) => said.extend(
                values
                    .into_iter()
                    .map(|(id, plain)| EditorEvent::Edit(ParameterEdit::Value { id, value: plain })),
            ),
        }
    }
    said
}

/// The id that a window with no screen is put in, which is not a window: only the plugins of the tests (which ask for a parent that is not null and nothing more) take it.
const PARENT_OF_A_WINDOW_WITHOUT_A_SCREEN: usize = 1;

/// How often the window looks at whether the plugin's state is what it was. **A VST3 plugin has no sure way to say that its state changed** (the window of a plugin built with DPF
/// tells its other half in a message, and says nothing to the host: a curve drawn in Wolf Shaper would be lost), so the state is looked at while the window is open.
const HOW_OFTEN_TO_LOOK_AT_THE_STATE: Duration = Duration::from_millis(500);

/// For how long after what the project told the plugin, or what a hand did to a value, a change of the state is taken to be what that made (the value is in the state, and the project has it
/// too), and the state is taken as it is then, not as a change.
const HOW_LONG_A_CHANGE_OF_THE_STATE_IS_THE_VALUES: Duration = Duration::from_millis(300);

/// The sample rate and the most frames of a block of the plugin that is made to have a window: it is made as a plugin is to make its window, and is run only to hear the edits.
const SAMPLE_RATE_OF_A_WINDOW: f64 = 48_000.0;
const MOST_FRAMES_OF_A_WINDOW: usize = 512;

/// The window of a VST3 plugin: **the plugin is made here** (its component and its controller, connected, and run on silence now and then so that the component hears the edits that
/// the window makes), because the window belongs to its controller. The sound is made elsewhere; what this copy is for is the window, and the state that it saves after a hand
/// has changed something.
pub struct Vst3Window {
    instance: Instance,
    /// What the window did that has not been given to whoever asked yet (it is looked at when the state is asked for, so that the state has the edits).
    backlog: Vec<EditorEvent>,
    /// The window of the program that the plugin's window is in, kept for as long as the plugin's window is.
    parent: Option<Arc<Window>>,
    /// The state as it was when it was last looked at (or given, or said): a state that is another is a change.
    state_known: Option<Vec<u8>>,
    state_looked_at: Instant,
    /// Until when a change of the state is the values that the project told or a hand made (see [`HOW_LONG_A_CHANGE_OF_THE_STATE_IS_THE_VALUES`]).
    state_is_the_values_until: Option<Instant>,
}

impl Vst3Window {
    /// The window of the class `class_id` (written as `ClassId::text` does) of the module `library` (a `.vst3` folder or file). An error says why there is no window to show.
    pub fn new(library: &Path, class_id: &str) -> Result<Self, String> {
        let class = ClassId::from_text(class_id)
            .ok_or_else(|| format!("{class_id:?} is not the id of a class of a VST3 plugin"))?;
        let module = Module::open(library).map_err(|error| format!("the library could not be loaded: {error}"))?;
        let (instance, _why_the_state_was_not_taken) =
            Instance::new(&module, class, SAMPLE_RATE_OF_A_WINDOW, MOST_FRAMES_OF_A_WINDOW, None)
                .map_err(|error| error.to_string())?;
        if !instance.has_a_view() {
            return Err("the plugin has no window".to_owned());
        }
        Ok(Self {
            instance,
            backlog: Vec::new(),
            parent: None,
            state_known: None,
            state_looked_at: Instant::now(),
            state_is_the_values_until: None,
        })
    }

    /// Takes the state as it is as the one that a change is a change from.
    fn take_the_state_as_it_is(&mut self) {
        self.instance.apply_waiting_changes();
        self.state_known = self.instance.save_state().ok();
        self.state_looked_at = Instant::now();
        self.state_is_the_values_until = None;
    }

    /// From now on, for a moment, a change of the state is the values that were told or made.
    fn the_state_is_the_values_for_a_moment(&mut self) {
        self.state_is_the_values_until = Some(Instant::now() + HOW_LONG_A_CHANGE_OF_THE_STATE_IS_THE_VALUES);
    }

    /// Looks at the state now and then: a state that is not what it was, and that is not what the values that the project told or a hand made would make of it, is a change that the plugin
    /// did not say.
    fn look_at_the_state(&mut self) {
        let now = Instant::now();
        if let Some(until) = self.state_is_the_values_until {
            if now >= until {
                self.state_is_the_values_until = None;
                self.take_the_state_as_it_is();
            }
            return;
        }
        if now.saturating_duration_since(self.state_looked_at) < HOW_OFTEN_TO_LOOK_AT_THE_STATE {
            return;
        }
        self.state_looked_at = now;
        let Ok(state) = self.instance.save_state() else {
            return;
        };
        if self.state_known.as_ref().is_some_and(|known| *known != state) {
            self.backlog.push(EditorEvent::StateChanged);
        }
        self.state_known = Some(state);
    }

    /// Looks at what the window did, and keeps it to be given. (This is also what gives the component the edits, so it comes before the state is saved.)
    fn look(&mut self) {
        let events = editor_events_of_a_vst3_window(self.instance.view_events());
        if events.iter().any(|event| matches!(event, EditorEvent::Edit(_))) {
            self.the_state_is_the_values_for_a_moment();
        }
        self.backlog.extend(events);
        self.look_at_the_state();
    }
}

impl PluginWindow for Vst3Window {
    fn has_state(&self) -> bool {
        true
    }

    fn can_make_a_window(&mut self, api: &str, floating: bool) -> bool {
        // (A VST3 window is always in a parent: there is no window of its own to ask for.)
        !floating && api == super::the_kind_of_window()
    }

    fn make_the_window(
        &mut self,
        _api: &str,
        floating: bool,
        scale: f64,
        parent: Option<Arc<Window>>,
    ) -> Result<WindowSize, String> {
        // (The state as the project gave it, with its values: what the window does from here on is a change of it.)
        self.take_the_state_as_it_is();
        let size = match (floating, &parent) {
            (false, Some(parent)) => self.instance.open_view_in_a_window(parent.as_ref(), scale),
            (false, None) => return Err("a window of a VST3 plugin is made in a window of the program".to_owned()),
            (true, _) => self.instance.open_view(PARENT_OF_A_WINDOW_WITHOUT_A_SCREEN, scale),
        }
        .map_err(|error| error.to_string())?;
        self.parent = parent;
        Ok(WindowSize {
            width: size.width,
            height: size.height,
            can_resize: size.can_resize,
        })
    }

    fn put_the_window_in_a_window(&mut self, _window: Arc<Window>) -> Result<(), String> {
        // (It was made in the window.)
        if self.instance.view_size().is_some() {
            Ok(())
        } else {
            Err("there is no window to put in it".to_owned())
        }
    }

    fn suggest_a_title(&mut self, _title: &str) {}

    fn show(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn adjust_the_size(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        self.instance.adjust_view_size(width, height)
    }

    fn size_of_the_window(&mut self) -> Option<(u32, u32)> {
        self.instance.view_size()
    }

    fn set_the_scale(&mut self, scale: f64) {
        self.instance.set_view_scale(scale);
    }

    fn set_the_size(&mut self, width: u32, height: u32) -> bool {
        self.instance.set_view_size(width, height)
    }

    fn destroy_the_window(&mut self) {
        self.instance.close_view();
        self.parent = None;
    }

    fn tell_the_values(&mut self, values: &[(u32, f64)]) -> Vec<EditorEvent> {
        // (What the window did before is kept as it is.)
        let before = std::mem::take(&mut self.backlog);
        for (id, plain) in values {
            self.instance.set_value(*id, *plain, 0);
        }
        if !values.is_empty() {
            self.the_state_is_the_values_for_a_moment();
        }
        // A plugin may say, at once, what it was just told, as an edit of its own (Surge XT Effects does): that is not a hand, and is thrown away. What else it says (the size it asks for,
        // that its state changed) is kept for the next look and is not returned here, where the program does not read it.
        self.look();
        self.backlog.retain(|event| !matches!(event, EditorEvent::Edit(_)));
        let said_now = std::mem::replace(&mut self.backlog, before);
        self.backlog.extend(said_now);
        Vec::new()
    }

    fn poll(&mut self) -> Vec<EditorEvent> {
        self.look();
        std::mem::take(&mut self.backlog)
    }

    fn save_the_state(&mut self) -> Result<Vec<u8>, String> {
        // (The edits that wait are given to the component first.)
        self.look();
        let state = self.instance.save_state().map_err(|error| error.to_string())?;
        // (What is said is what the project has: it is not a change from here on.)
        self.state_known = Some(state.clone());
        self.state_looked_at = Instant::now();
        self.state_is_the_values_until = None;
        Ok(state)
    }

    fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.the_state_is_the_values_for_a_moment();
        self.instance.restore_state(bytes).map_err(|error| error.to_string())
    }

    fn forget_that_the_state_changed(&mut self) {
        self.look();
        self.backlog.retain(|event| !matches!(event, EditorEvent::StateChanged));
    }

    fn state_changed_since_it_was_looked_at(&mut self) -> bool {
        self.instance.apply_waiting_changes();
        // (A state that is still the values that were told or made, for a moment, is not looked at: the project has them.)
        if self.state_is_the_values_until.is_some() {
            return false;
        }
        match (self.instance.save_state(), &self.state_known) {
            (Ok(state), Some(known)) => state != *known,
            _ => false,
        }
    }
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

    #[test]
    fn what_a_vst3_window_did_is_what_an_editor_says_in_the_order_that_it_was_done_and_a_preset_is_values_that_were_edited()
     {
        let events = editor_events_of_a_vst3_window([
            ViewEvent::Begin { id: 4 },
            ViewEvent::Value { id: 4, plain: 0.5 },
            ViewEvent::End { id: 4 },
            ViewEvent::ResizeRequested {
                width: 640,
                height: 480,
            },
            ViewEvent::StateChanged,
            ViewEvent::ValuesChanged(vec![(1, 2.0), (7, -3.0)]),
        ]);

        assert_eq!(
            events,
            vec![
                EditorEvent::Edit(ParameterEdit::Begin { id: 4 }),
                EditorEvent::Edit(ParameterEdit::Value { id: 4, value: 0.5 }),
                EditorEvent::Edit(ParameterEdit::End { id: 4 }),
                EditorEvent::ResizeRequested {
                    width: 640,
                    height: 480
                },
                EditorEvent::StateChanged,
                EditorEvent::Edit(ParameterEdit::Value { id: 1, value: 2.0 }),
                EditorEvent::Edit(ParameterEdit::Value { id: 7, value: -3.0 }),
            ]
        );
        assert!(editor_events_of_a_vst3_window([]).is_empty());
    }
}
