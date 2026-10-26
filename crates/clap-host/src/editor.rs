// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor instance of a plugin: the instance that a plugin's **window** is made on (phase 10, piece 10.1; P1 and P2 of `docs/engine-phase-10-editors-design.md`).
//!
//! It is never run for sound: it stays *inactive*, so that its parameters are given and taken with `params.flush` on the main thread, and what a person does in the window comes out
//! as **parameter edits** (`begin`, `value`, `end` of a gesture) and as "the state changed". The sound is made by other instances of the plugin, from the project; the editor
//! instance and they are told the same values, which is what keeps them alike.
//!
//! **An [`Editor`] belongs to the thread that made it, which has to be the main thread of the program that shows the window** (the plugin's rules for its window: a message loop on
//! Windows, the main thread on macOS). It is made by the helper program of the editor windows, and by the tests.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui, Window};
use clack_extensions::params::PluginParams;
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
use clack_host::events::event_types::ParamValueEvent;
use clack_host::events::spaces::CoreEventSpace;
use clack_host::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::instance::{Gathered, gather};
use crate::library::Library;
use crate::scan::Parameter;

/// Why an editor could not be made or its window could not be shown.
#[derive(Debug, Error)]
pub enum EditorError {
    #[error("the plugin could not be made: {0}")]
    CouldNotBeMade(String),
    #[error("the id of the plugin is not text that can be given to a library")]
    BadId,
    #[error("the plugin has no window")]
    NoWindow,
    #[error("the plugin cannot make a window of the kind {api} that is {kind}")]
    NotSupported { api: String, kind: &'static str },
    #[error("the plugin could not make its window")]
    CouldNotMakeTheWindow,
    #[error("the plugin could not put its window in the one it was given")]
    CouldNotEmbed,
    #[error("the plugin could not show its window")]
    CouldNotShow,
    #[error("the plugin has no state, or could not save or load it: {0}")]
    State(String),
}

/// One step of what a person does to a parameter in the window of a plugin.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParameterEdit {
    /// The hand has taken hold of the parameter (a gesture begins): what follows is one edit, until it ends.
    Begin { id: u32 },
    /// The parameter has this value.
    Value { id: u32, value: f64 },
    /// The hand has let go.
    End { id: u32 },
}

/// What the plugin did, or asked for, that the user of the editor is to know.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorEvent {
    /// A change of a parameter by the plugin (by its window, for the most part).
    Edit(ParameterEdit),
    /// The plugin says that its state is not what was last saved.
    StateChanged,
    /// The plugin's parameters are not what they were (it asked to rescan them): see [`Editor::parameters`] again by making the editor again.
    ParametersChanged,
    /// The plugin asks that its window be this big (an embedded window that the plugin has resized itself).
    ResizeRequested {
        width: u32,
        height: u32,
    },
    ShowRequested,
    HideRequested,
    /// The window was closed (a floating window, by the person) or the plugin lost it; `destroyed` says that the plugin has destroyed it already.
    Closed {
        destroyed: bool,
    },
}

/// How big the window of a plugin is, and whether it can be made another size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
    pub can_resize: bool,
}

/// What the plugin asked for through the host, which is looked at by [`Editor::poll`].
#[derive(Default)]
struct Requests {
    flush: AtomicBool,
    callback: AtomicBool,
    state_dirty: AtomicBool,
    parameters_changed: AtomicBool,
    show: AtomicBool,
    hide: AtomicBool,
    resize: Mutex<Option<(u32, u32)>>,
    closed: Mutex<Option<bool>>,
}

struct Shared {
    requests: Arc<Requests>,
}

impl<'a> SharedHandler<'a> for Shared {
    fn request_restart(&self) {}

    fn request_process(&self) {}

    fn request_callback(&self) {
        self.requests.callback.store(true, Ordering::SeqCst);
    }
}

impl HostParamsImplShared for Shared {
    fn request_flush(&self) {
        self.requests.flush.store(true, Ordering::SeqCst);
    }
}

impl HostGuiImpl for Shared {
    fn resize_hints_changed(&self) {}

    fn request_resize(&self, new_size: GuiSize) -> Result<(), HostError> {
        *self
            .requests
            .resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((new_size.width, new_size.height));
        Ok(())
    }

    fn request_show(&self) -> Result<(), HostError> {
        self.requests.show.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn request_hide(&self) -> Result<(), HostError> {
        self.requests.hide.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn closed(&self, was_destroyed: bool) {
        *self
            .requests
            .closed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(was_destroyed);
    }
}

struct MainThreadPart<'a> {
    requests: Arc<Requests>,
    _shared: std::marker::PhantomData<&'a ()>,
}

impl<'a> MainThreadHandler<'a> for MainThreadPart<'a> {}

impl HostParamsImplMainThread for MainThreadPart<'_> {
    fn rescan(&self, flags: ParamRescanFlags) {
        if flags.intersects(ParamRescanFlags::ALL | ParamRescanFlags::INFO) {
            self.requests.parameters_changed.store(true, Ordering::SeqCst);
        }
    }

    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostStateImpl for MainThreadPart<'_> {
    fn mark_dirty(&self) {
        self.requests.state_dirty.store(true, Ordering::SeqCst);
    }
}

/// The handlers of the editor's host.
struct EditorHost;

impl HostHandlers for EditorHost {
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThreadPart<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostParams>()
            .register::<HostGui>()
            .register::<HostState>();
    }
}

/// An inactive instance of a plugin that a window is shown for.
pub struct Editor {
    instance: PluginInstance<EditorHost>,
    requests: Arc<Requests>,
    gathered: Gathered,
    window_exists: bool,
    /// The window that the plugin's window is in, kept so that it is there as long as the plugin's window is (**the last field: it is dropped after the plugin**).
    parent: Option<Arc<dyn std::any::Any>>,
}

impl Editor {
    /// Makes the plugin `id` of `library`, **on the calling thread**, which is the thread that all the other calls have to be made on.
    pub fn new(library: &Library, id: &str) -> Result<Self, EditorError> {
        let id = CString::new(id).map_err(|_| EditorError::BadId)?;
        let requests = Arc::new(Requests::default());
        let host_info = HostInfo::new(
            "fastforwardrs",
            "John Dorsey",
            "https://github.com/JohnDorsey/fastforwardrs",
            "0.1.0",
        )
        .map_err(|error| EditorError::CouldNotBeMade(error.to_string()))?;
        let (shared_requests, main_requests) = (Arc::clone(&requests), Arc::clone(&requests));
        let mut instance = PluginInstance::<EditorHost>::new(
            move |()| Shared {
                requests: shared_requests,
            },
            move |_| MainThreadPart {
                requests: main_requests,
                _shared: std::marker::PhantomData,
            },
            &library.entry,
            &id,
            &host_info,
        )
        .map_err(|error| EditorError::CouldNotBeMade(error.to_string()))?;
        let gathered = gather(&mut instance, false);
        Ok(Self {
            instance,
            requests,
            gathered,
            window_exists: false,
            parent: None,
        })
    }

    /// The parameters the plugin has, as it said when it was made.
    pub fn parameters(&self) -> &[Parameter] {
        &self.gathered.parameters
    }

    /// Whether the plugin has a state to be saved.
    pub fn has_state(&self) -> bool {
        self.gathered.has_state
    }

    /// Whether the plugin has a window at all.
    pub fn has_a_window(&mut self) -> bool {
        self.instance.plugin_handle().get_extension::<PluginGui>().is_some()
    }

    /// Whether the plugin can make a window of the kind `api` (`"win32"`, `"cocoa"`, `"x11"`, `"wayland"`), **floating** (a window of its own) or not (inside one that it is given).
    pub fn can_make_a_window(&mut self, api: &str, floating: bool) -> bool {
        let Ok(api) = CString::new(api) else {
            return false;
        };
        let handle = self.instance.plugin_handle();
        let Some(gui) = handle.get_extension::<PluginGui>() else {
            return false;
        };
        gui.is_api_supported(
            &handle,
            GuiConfiguration {
                api_type: GuiApiType(&api),
                is_floating: floating,
            },
        )
    }

    /// What the plugin would rather: the kind of window, and whether floating. (It is only a hint.)
    pub fn preferred_window(&mut self) -> Option<(String, bool)> {
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>()?;
        let preferred = gui.get_preferred_api(&handle)?;
        Some((
            preferred.api_type.0.to_string_lossy().into_owned(),
            preferred.is_floating,
        ))
    }

    /// Makes the window (not shown yet): asks the plugin to make it for `api`, floating or not, tells it the `scale` of the screen, and asks how big it is.
    pub fn make_the_window(&mut self, api: &str, floating: bool, scale: f64) -> Result<WindowSize, EditorError> {
        let api_text = CString::new(api).map_err(|_| EditorError::BadId)?;
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>().ok_or(EditorError::NoWindow)?;
        let configuration = GuiConfiguration {
            api_type: GuiApiType(&api_text),
            is_floating: floating,
        };
        if !gui.is_api_supported(&handle, configuration) {
            return Err(EditorError::NotSupported {
                api: api.to_owned(),
                kind: if floating { "floating" } else { "embedded" },
            });
        }
        gui.create(&handle, configuration)
            .map_err(|_| EditorError::CouldNotMakeTheWindow)?;
        self.window_exists = true;
        // (A plugin may say no to a scale: it then asks the system itself.)
        let _ = gui.set_scale(&handle, scale);
        let size = gui.get_size(&handle).ok_or(EditorError::CouldNotMakeTheWindow)?;
        Ok(WindowSize {
            width: size.width,
            height: size.height,
            can_resize: gui.can_resize(&handle),
        })
    }

    /// Puts the plugin's window inside `parent`, a window that the program has made.
    ///
    /// # Safety
    ///
    /// `parent` must be a window that is valid until [`Self::destroy_the_window`] (or the editor) is dropped.
    #[allow(unsafe_code)]
    pub unsafe fn put_the_window_in(&mut self, parent: Window<'_, '_>) -> Result<(), EditorError> {
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>().ok_or(EditorError::NoWindow)?;
        // SAFETY: the caller says that the window is valid for as long as the plugin's window is.
        unsafe { gui.set_parent(&handle, parent) }.map_err(|_| EditorError::CouldNotEmbed)
    }

    /// Puts the plugin's window inside `window`, which the editor **keeps** until the plugin's window is destroyed (so it is valid for as long as the plugin uses it): the safe way to
    /// embed a window, for a program that makes its windows with a library of windows.
    pub fn put_the_window_in_a_window<W: raw_window_handle::HasWindowHandle + 'static>(
        &mut self,
        window: Arc<W>,
    ) -> Result<(), EditorError> {
        let parent = Window::from_window(&*window).ok_or(EditorError::CouldNotEmbed)?;
        #[allow(unsafe_code)]
        // SAFETY: the window is kept in `self.parent`, which is dropped after the plugin and its window are.
        unsafe {
            self.put_the_window_in(parent)?;
        }
        self.parent = Some(window);
        Ok(())
    }

    /// Gives a floating window a title to use, if it likes.
    pub fn suggest_a_title(&mut self, title: &str) {
        let Ok(title) = CString::new(title) else {
            return;
        };
        let handle = self.instance.plugin_handle();
        if let Some(gui) = handle.get_extension::<PluginGui>() {
            gui.suggest_title(&handle, &title);
        }
    }

    /// Shows the window.
    pub fn show(&mut self) -> Result<(), EditorError> {
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>().ok_or(EditorError::NoWindow)?;
        gui.show(&handle).map_err(|_| EditorError::CouldNotShow)
    }

    /// Hides the window (it is not destroyed).
    pub fn hide(&mut self) {
        let handle = self.instance.plugin_handle();
        if let Some(gui) = handle.get_extension::<PluginGui>() {
            let _ = gui.hide(&handle);
        }
    }

    /// The size that is nearest to `width` by `height` that the window can be, if it says.
    pub fn adjust_the_size(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>()?;
        gui.adjust_size(&handle, GuiSize { width, height })
            .map(|size| (size.width, size.height))
    }

    /// How big the plugin says its window is now. (It can be other than it said when the window was made: a plugin that applies a zoom of its own does it when it is given its parent.)
    pub fn size_of_the_window(&mut self) -> Option<(u32, u32)> {
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>()?;
        gui.get_size(&handle).map(|size| (size.width, size.height))
    }

    /// Tells the plugin that the screen's scale is this now (a window that was moved to another screen).
    pub fn set_the_scale(&mut self, scale: f64) {
        let handle = self.instance.plugin_handle();
        if let Some(gui) = handle.get_extension::<PluginGui>() {
            let _ = gui.set_scale(&handle, scale);
        }
    }

    /// Tells the plugin that its window is this big now. Whether it took it.
    pub fn set_the_size(&mut self, width: u32, height: u32) -> bool {
        let handle = self.instance.plugin_handle();
        let Some(gui) = handle.get_extension::<PluginGui>() else {
            return false;
        };
        gui.set_size(&handle, GuiSize { width, height }).is_ok()
    }

    /// Frees the window (the plugin's resources for it). Nothing happens if there is none.
    pub fn destroy_the_window(&mut self) {
        if !std::mem::take(&mut self.window_exists) {
            return;
        }
        let handle = self.instance.plugin_handle();
        if let Some(gui) = handle.get_extension::<PluginGui>() {
            gui.destroy(&handle);
        }
    }

    /// Tells the plugin what these parameters are now (`(id, value)`, in order), as the project says. What the plugin says back at once is returned: **a plugin should say nothing of
    /// a change it was told of, but some do**, and the user of the editor is to leave out what is what it was told.
    pub fn tell_the_values(&mut self, values: &[(u32, f64)]) -> Vec<EditorEvent> {
        let mut input = EventBuffer::new();
        for &(id, value) in values {
            if let Some(id) = ClapId::from_raw(id) {
                input.push(&ParamValueEvent::new(0, id, Pckn::match_all(), value));
            }
        }
        self.flush(&input)
    }

    /// Does what the plugin asked for on the main thread, and gives what it did and asked for since the last call: edits of parameters, a change of state, a request for another
    /// size, to show, to hide, a window that closed. **Call it often** (a plugin's window needs the main thread).
    pub fn poll(&mut self) -> Vec<EditorEvent> {
        if self.requests.callback.swap(false, Ordering::SeqCst) {
            self.instance.call_on_main_thread_callback();
        }
        let mut events = Vec::new();
        if self.requests.flush.swap(false, Ordering::SeqCst) {
            events.extend(self.flush(&EventBuffer::new()));
        }
        if self.requests.state_dirty.swap(false, Ordering::SeqCst) {
            events.push(EditorEvent::StateChanged);
        }
        if self.requests.parameters_changed.swap(false, Ordering::SeqCst) {
            events.push(EditorEvent::ParametersChanged);
        }
        let resize = self
            .requests
            .resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some((width, height)) = resize {
            events.push(EditorEvent::ResizeRequested { width, height });
        }
        if self.requests.show.swap(false, Ordering::SeqCst) {
            events.push(EditorEvent::ShowRequested);
        }
        if self.requests.hide.swap(false, Ordering::SeqCst) {
            events.push(EditorEvent::HideRequested);
        }
        let closed = self
            .requests
            .closed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(destroyed) = closed {
            events.push(EditorEvent::Closed { destroyed });
        }
        events
    }

    /// What the plugin says that the parameter `id` is now, if it has it.
    pub fn value_of(&mut self, id: u32) -> Option<f64> {
        let handle = self.instance.plugin_handle();
        let params = handle.get_extension::<PluginParams>()?;
        params.get_value(&handle, ClapId::from_raw(id)?)
    }

    /// The state of the plugin, saved. (Not the parameters' values alone: whatever the plugin keeps.)
    pub fn save_the_state(&mut self) -> Result<Vec<u8>, EditorError> {
        let handle = self.instance.plugin_handle();
        let state = handle
            .get_extension::<PluginState>()
            .ok_or_else(|| EditorError::State("it has none".to_owned()))?;
        let mut bytes = Vec::new();
        state
            .save(&handle, &mut bytes)
            .map_err(|error| EditorError::State(error.to_string()))?;
        Ok(bytes)
    }

    /// Forgets that the plugin said that its state changed, if it did and [`Self::poll`] has not told: **for a plugin that says so while it is given a state** (many do), which is a change
    /// that nobody made.
    pub fn forget_that_the_state_changed(&mut self) {
        self.requests.state_dirty.store(false, Ordering::SeqCst);
    }

    /// Gives the plugin a state that it saved.
    pub fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), EditorError> {
        let handle = self.instance.plugin_handle();
        let state = handle
            .get_extension::<PluginState>()
            .ok_or_else(|| EditorError::State("it has none".to_owned()))?;
        state
            .load(&handle, &mut &*bytes)
            .map_err(|error| EditorError::State(error.to_string()))
    }

    /// Runs `params.flush` (the plugin is inactive, so on the main thread) with `input`, and gives the edits that came out.
    fn flush(&mut self, input: &EventBuffer) -> Vec<EditorEvent> {
        let mut output = EventBuffer::new();
        let Some(params) = self.instance.plugin_handle().get_extension::<PluginParams>() else {
            return Vec::new();
        };
        let Some(mut inactive) = self.instance.inactive_plugin_handle() else {
            return Vec::new();
        };
        params.flush(&mut inactive, &input.as_input(), &mut output.as_output());
        output
            .iter()
            .filter_map(|event| match event.as_core_event()? {
                CoreEventSpace::ParamValue(change) => Some(EditorEvent::Edit(ParameterEdit::Value {
                    id: change.param_id()?.get(),
                    value: change.value(),
                })),
                CoreEventSpace::ParamGestureBegin(gesture) => Some(EditorEvent::Edit(ParameterEdit::Begin {
                    id: gesture.param_id()?.get(),
                })),
                CoreEventSpace::ParamGestureEnd(gesture) => Some(EditorEvent::Edit(ParameterEdit::End {
                    id: gesture.param_id()?.get(),
                })),
                _ => None,
            })
            .collect()
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.destroy_the_window();
    }
}
