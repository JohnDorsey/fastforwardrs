// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of an LV2 plugin (the extension `ui`; piece 11.5 of `docs/engine-phase-11-lv2-design.md`, section 9).
//!
//! **A user interface is code of its own that does not run the plugin**: it is made from the plugin's URI and the bundle's path, with a function to write to (what a person did to a
//! port) and a controller that is given back to that function, and it is told what the ports are with `port_event`. This module loads the binary that has the interface
//! (`lv2ui_descriptor`), makes it **inside a window that the program made** (`ui:parent`: a `HWND` on Windows, a window id on X11, an `NSView` on macOS), and turns what it does into
//! [`UiEvent`]s. Control ports only (protocol 0, one `float`); a user interface that talks in atoms is shown, and what it says in atoms is thrown away.
//!
//! **A [`UiWindow`] belongs to the thread that made it**, which has to be the main thread of the program that has the window (the interface makes windows, and a window belongs to a
//! thread with a message loop). Every `unsafe` block says why it is sound: **calling a function of the user interface** (sound if it keeps the contract of `ui.h`, why a library
//! is only loaded after a person has said yes) and **reading what the interface points to** (good for as long as the library is loaded, which the window keeps).

#![allow(unsafe_code)]

use std::any::Any;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, Mutex, PoisonError};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use thiserror::Error;

use crate::abi::{self, RawFeature, RawOption, RawUridMap, RawUridUnmap};
use crate::description::{PluginDescription, UiDescription};
use crate::features::UridTable;

/// How big a window is when nothing says (the user interface has not asked for a size, and the system cannot say the size of what it made).
pub const DEFAULT_WINDOW_SIZE: (u32, u32) = (400, 300);

/// The most user interfaces that a library may have: it is asked for them one after another until it says there are no more.
const MOST_INTERFACES: u32 = 256;

/// `LV2UI_INVALID_PORT_INDEX`.
const INVALID_PORT_INDEX: u32 = u32::MAX;

/// Why a window could not be made.
#[derive(Debug, Error)]
pub enum UiError {
    #[error("the user interface has no library in the bundle")]
    NoBinary,
    #[error("the library of the user interface could not be loaded: {0}")]
    CouldNotBeLoaded(String),
    #[error("the library has no function called lv2ui_descriptor, so it has no user interfaces ({0})")]
    NotAnLv2UiLibrary(String),
    #[error("the library has no user interface called {0}")]
    NoSuchInterface(String),
    #[error("the user interface does not have the function {0}, which every one has to")]
    MissingFunction(&'static str),
    #[error("the user interface could not be made: its instantiate gave nothing")]
    InstantiateFailed,
    #[error("the window that it was to be put in has no handle that a user interface can use")]
    NoHandleForTheParent,
    #[error("{0}")]
    CannotUse(String),
}

/// What a user interface did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiEvent {
    /// A control port has this value (written by the interface: a person did it).
    Write { port: u32, value: f32 },
    /// A hand took hold of a control (`grabbed`) or let go of it.
    Touch { port: u32, grabbed: bool },
    /// The interface asks that its window be this big (`ui:resize`).
    ResizeRequested { width: u32, height: u32 },
}

// ---- the binary interface of `ui.h` ----

/// `LV2UI_Write_Function`.
type WriteFunction = unsafe extern "C" fn(
    controller: *mut c_void,
    port_index: u32,
    buffer_size: u32,
    port_protocol: u32,
    buffer: *const c_void,
);

/// `LV2UI_Descriptor`.
#[repr(C)]
struct RawUiDescriptor {
    uri: *const c_char,
    instantiate: Option<
        unsafe extern "C" fn(
            descriptor: *const RawUiDescriptor,
            plugin_uri: *const c_char,
            bundle_path: *const c_char,
            write_function: Option<WriteFunction>,
            controller: *mut c_void,
            widget: *mut *mut c_void,
            features: *const *const RawFeature,
        ) -> *mut c_void,
    >,
    cleanup: Option<unsafe extern "C" fn(ui: *mut c_void)>,
    port_event: Option<
        unsafe extern "C" fn(ui: *mut c_void, port_index: u32, buffer_size: u32, format: u32, buffer: *const c_void),
    >,
    extension_data: Option<unsafe extern "C" fn(uri: *const c_char) -> *const c_void>,
}

/// `LV2UI_DescriptorFunction`: what a library exports as `lv2ui_descriptor`.
type UiDescriptorFunction = unsafe extern "C" fn(index: u32) -> *const RawUiDescriptor;

/// `LV2UI_Resize`: a feature (the interface tells the host how big it is) and also the data of an interface's `ui:resize` (the host tells the interface).
#[repr(C)]
struct RawUiResize {
    handle: *mut c_void,
    ui_resize: Option<unsafe extern "C" fn(handle: *mut c_void, width: i32, height: i32) -> i32>,
}

/// `LV2UI_Touch`.
#[repr(C)]
struct RawUiTouch {
    handle: *mut c_void,
    touch: Option<unsafe extern "C" fn(handle: *mut c_void, port_index: u32, grabbed: bool)>,
}

/// `LV2UI_Port_Map`.
#[repr(C)]
struct RawUiPortMap {
    handle: *mut c_void,
    port_index: Option<unsafe extern "C" fn(handle: *mut c_void, symbol: *const c_char) -> u32>,
}

/// `LV2UI_Idle_Interface`.
#[repr(C)]
struct RawUiIdleInterface {
    idle: Option<unsafe extern "C" fn(ui: *mut c_void) -> i32>,
}

// ---- what the interface calls ----

/// What is shared between the window and the callbacks that the interface calls: it is pointed to by the controller and by the features, so it lives in an `Arc` that the window holds.
struct Shared {
    events: Mutex<VecDeque<UiEvent>>,
    /// The size that the interface last asked for.
    asked_size: Mutex<Option<(u32, u32)>>,
    /// The ports' symbols and indexes (for `ui:portMap`).
    symbols: Vec<(CString, u32)>,
}

impl Shared {
    fn push(&self, event: UiEvent) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(event);
    }
}

/// What a callback's handle (a pointer to the [`Shared`] that the window keeps) is.
///
/// # Safety
///
/// `handle` is null or the pointer that the features and the controller were made with, which the window keeps alive for as long as the interface exists.
unsafe fn shared_of<'handle>(handle: *mut c_void) -> Option<&'handle Shared> {
    // SAFETY: the caller says that it is null or what it was made with, and the window outlives the interface (it calls `cleanup` before it lets go of the `Arc`).
    unsafe { handle.cast::<Shared>().as_ref() }
}

/// `LV2UI_Write_Function`: a person did something to a port.
unsafe extern "C" fn write_callback(
    controller: *mut c_void,
    port_index: u32,
    buffer_size: u32,
    port_protocol: u32,
    buffer: *const c_void,
) {
    // SAFETY: the controller is what the window gave `instantiate`.
    let Some(shared) = (unsafe { shared_of(controller) }) else {
        return;
    };
    // (Protocol 0 is a `float` of four bytes: an atom, or anything else, is not understood and is thrown away.)
    if port_protocol != 0 || buffer_size as usize != size_of::<f32>() || buffer.is_null() {
        return;
    }
    // SAFETY: the interface gives a pointer to `buffer_size` bytes, here four, that are good for the length of the call; they may not be aligned.
    let value = unsafe { std::ptr::read_unaligned(buffer.cast::<f32>()) };
    shared.push(UiEvent::Write {
        port: port_index,
        value,
    });
}

/// `ui:resize`'s `ui_resize`, as a feature: the interface says how big its window is.
unsafe extern "C" fn resize_callback(handle: *mut c_void, width: i32, height: i32) -> i32 {
    // SAFETY: the handle is what the feature was made with.
    let Some(shared) = (unsafe { shared_of(handle) }) else {
        return 1;
    };
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return 1;
    };
    if width == 0 || height == 0 {
        return 1;
    }
    *shared.asked_size.lock().unwrap_or_else(PoisonError::into_inner) = Some((width, height));
    shared.push(UiEvent::ResizeRequested { width, height });
    0
}

/// `ui:touch`'s `touch`.
unsafe extern "C" fn touch_callback(handle: *mut c_void, port_index: u32, grabbed: bool) {
    // SAFETY: the handle is what the feature was made with.
    if let Some(shared) = unsafe { shared_of(handle) } {
        shared.push(UiEvent::Touch {
            port: port_index,
            grabbed,
        });
    }
}

/// `ui:portMap`'s `port_index`: the index of the port with this symbol, or the one that means there is none.
unsafe extern "C" fn port_index_callback(handle: *mut c_void, symbol: *const c_char) -> u32 {
    // SAFETY: the handle is what the feature was made with.
    let Some(shared) = (unsafe { shared_of(handle) }) else {
        return INVALID_PORT_INDEX;
    };
    if symbol.is_null() {
        return INVALID_PORT_INDEX;
    }
    // SAFETY: the interface gives a C string that is good for the length of the call.
    let symbol = unsafe { CStr::from_ptr(symbol) };
    shared
        .symbols
        .iter()
        .find(|(known, _)| known.as_c_str() == symbol)
        .map_or(INVALID_PORT_INDEX, |(_, index)| *index)
}

/// `urid:map`.
unsafe extern "C" fn map_callback(handle: *mut c_void, uri: *const c_char) -> u32 {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    // SAFETY: the handle is the `UridTable` that the window's features were made with (kept by the window); `uri` is a C string that the interface gives for the call.
    let (table, uri) = unsafe { (&*(handle as *const UridTable), CStr::from_ptr(uri)) };
    table.map(&uri.to_string_lossy())
}

/// `urid:unmap`.
unsafe extern "C" fn unmap_callback(handle: *mut c_void, urid: u32) -> *const c_char {
    if handle.is_null() {
        return std::ptr::null();
    }
    // SAFETY: as for `map_callback`.
    let table = unsafe { &*(handle as *const UridTable) };
    table.unmap(urid).unwrap_or(std::ptr::null())
}

/// What the options say: pointed to by the options feature.
struct OptionValues {
    sample_rate: f32,
    scale_factor: f32,
}

/// What the host gives the interface, and what that points to (never moved or changed once made, because the interface keeps the pointers).
struct UiFeatures {
    _table: Arc<UridTable>,
    _map: Box<RawUridMap>,
    _unmap: Box<RawUridUnmap>,
    _resize: Box<RawUiResize>,
    _touch: Box<RawUiTouch>,
    _port_map: Box<RawUiPortMap>,
    _option_values: Box<OptionValues>,
    _options: Box<[RawOption]>,
    _uris: Vec<CString>,
    _features: Vec<RawFeature>,
    /// The null-terminated list that `instantiate` takes.
    pointers: Vec<*const RawFeature>,
}

impl UiFeatures {
    fn new(shared: &Arc<Shared>, parent: *mut c_void, sample_rate: f32, scale: f64) -> Self {
        let table = Arc::new(UridTable::default());
        let table_pointer = Arc::as_ptr(&table) as *mut c_void;
        let shared_pointer = Arc::as_ptr(shared) as *mut c_void;
        let map = Box::new(RawUridMap {
            handle: table_pointer,
            map: Some(map_callback),
        });
        let unmap = Box::new(RawUridUnmap {
            handle: table_pointer,
            unmap: Some(unmap_callback),
        });
        let resize = Box::new(RawUiResize {
            handle: shared_pointer,
            ui_resize: Some(resize_callback),
        });
        let touch = Box::new(RawUiTouch {
            handle: shared_pointer,
            touch: Some(touch_callback),
        });
        let port_map = Box::new(RawUiPortMap {
            handle: shared_pointer,
            port_index: Some(port_index_callback),
        });
        let option_values = Box::new(OptionValues {
            sample_rate,
            scale_factor: scale as f32,
        });
        let float = table.map(abi::ATOM_FLOAT);
        let option = |key: &str, value: *const f32| RawOption {
            context: 0,
            subject: 0,
            key: table.map(key),
            size: 4,
            type_: float,
            value: value.cast(),
        };
        let options: Box<[RawOption]> = vec![
            option(abi::PARAM_SAMPLE_RATE, &raw const option_values.sample_rate),
            option(abi::UI_SCALE_FACTOR, &raw const option_values.scale_factor),
            // (The list ends with an option of all zeros.)
            RawOption {
                context: 0,
                subject: 0,
                key: 0,
                size: 0,
                type_: 0,
                value: std::ptr::null(),
            },
        ]
        .into_boxed_slice();

        let names = [
            abi::URID_MAP,
            abi::URID_UNMAP,
            abi::OPTIONS,
            abi::UI_PARENT,
            abi::UI_RESIZE,
            abi::UI_IDLE_INTERFACE,
            abi::UI_TOUCH,
            abi::UI_PORT_MAP,
        ];
        let uris: Vec<CString> = names.iter().filter_map(|name| CString::new(*name).ok()).collect();
        let features: Vec<RawFeature> = uris
            .iter()
            .map(|uri| {
                let data: *mut c_void = match uri.to_str().unwrap_or("") {
                    abi::URID_MAP => (&raw const *map).cast_mut().cast(),
                    abi::URID_UNMAP => (&raw const *unmap).cast_mut().cast(),
                    abi::OPTIONS => options.as_ptr().cast_mut().cast(),
                    abi::UI_PARENT => parent,
                    abi::UI_RESIZE => (&raw const *resize).cast_mut().cast(),
                    abi::UI_TOUCH => (&raw const *touch).cast_mut().cast(),
                    abi::UI_PORT_MAP => (&raw const *port_map).cast_mut().cast(),
                    _ => std::ptr::null_mut(),
                };
                RawFeature {
                    uri: uri.as_ptr(),
                    data,
                }
            })
            .collect();
        let mut pointers: Vec<*const RawFeature> = features.iter().map(std::ptr::from_ref).collect();
        pointers.push(std::ptr::null());
        Self {
            _table: table,
            _map: map,
            _unmap: unmap,
            _resize: resize,
            _touch: touch,
            _port_map: port_map,
            _option_values: option_values,
            _options: options,
            _uris: uris,
            _features: features,
            pointers,
        }
    }
}

/// How big a window that a user interface made is, as the system says, if it can say (on Windows it is the client area of the window: `ui.h` has no call for it).
#[cfg(windows)]
fn size_of_a_native_widget(widget: *mut c_void) -> Option<(u32, u32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
    if widget.is_null() {
        return None;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `GetClientRect` takes any value as a window handle and answers 0 for one that is not a window; `rect` is a place for the answer that lives for the call.
    let answered = unsafe { GetClientRect(widget, &mut rect) };
    let (width, height) = (u32::try_from(rect.right).ok()?, u32::try_from(rect.bottom).ok()?);
    (answered != 0 && width > 0 && height > 0).then_some((width, height))
}

/// (No system but Windows says it: the size comes from what the interface asked for.)
#[cfg(not(windows))]
fn size_of_a_native_widget(_widget: *mut c_void) -> Option<(u32, u32)> {
    None
}

/// What a window of this program is, as the handle that a user interface is given as its parent (`ui:parent`), if it has one of a kind that a user interface can use.
fn handle_of_a_window<W: HasWindowHandle>(window: &W) -> Option<*mut c_void> {
    let raw = window.window_handle().ok()?.as_raw();
    Some(match raw {
        RawWindowHandle::Win32(handle) => std::ptr::with_exposed_provenance_mut(handle.hwnd.get().cast_unsigned()),
        RawWindowHandle::Xlib(handle) => std::ptr::with_exposed_provenance_mut(usize::try_from(handle.window).ok()?),
        RawWindowHandle::Xcb(handle) => {
            std::ptr::with_exposed_provenance_mut(usize::try_from(handle.window.get()).ok()?)
        }
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
        _ => return None,
    })
}

/// A user interface that is made and running: **a window of a plugin**.
pub struct UiWindow {
    descriptor: *const RawUiDescriptor,
    handle: *mut c_void,
    /// What the interface says its window is (a `HWND`, a window id, an `NSView`), or null when it says nothing.
    widget: *mut c_void,
    idle_interface: *const RawUiIdleInterface,
    resize_interface: *const RawUiResize,
    shared: Arc<Shared>,
    _features: UiFeatures,
    /// The window of the program that this one is in, kept so that it lives as long as the interface does.
    _parent: Option<Box<dyn Any>>,
    /// (Last: everything above that points into the library is only good while this is alive.)
    _library: libloading::Library,
}

impl UiWindow {
    /// Makes the user interface `ui` of `plugin` **with no window to put it in** (the tests, where there is no screen): it is given `ui:parent` with no data, and an interface that
    /// needs a window is free to say that it cannot be made. The values are what it is told the ports are as soon as it is made.
    pub fn open_without_a_window(
        plugin: &PluginDescription,
        ui: &UiDescription,
        values: &[(u32, f64)],
    ) -> Result<Self, UiError> {
        Self::open(plugin, ui, std::ptr::null_mut(), None, 1.0, values)
    }

    /// Makes the user interface `ui` of `plugin` **inside `window`**, which this keeps until the interface is gone. `scale` is the scale of the screen (told as `ui:scaleFactor`).
    pub fn open_in_a_window<W: HasWindowHandle + 'static>(
        plugin: &PluginDescription,
        ui: &UiDescription,
        window: Arc<W>,
        scale: f64,
        values: &[(u32, f64)],
    ) -> Result<Self, UiError> {
        let parent = handle_of_a_window(&*window).ok_or(UiError::NoHandleForTheParent)?;
        Self::open(plugin, ui, parent, Some(Box::new(window)), scale, values)
    }

    fn open(
        plugin: &PluginDescription,
        ui: &UiDescription,
        parent: *mut c_void,
        keep: Option<Box<dyn Any>>,
        scale: f64,
        values: &[(u32, f64)],
    ) -> Result<Self, UiError> {
        let binary = ui.binary.as_ref().ok_or(UiError::NoBinary)?;
        // SAFETY: loading a library runs its initialisation, which is code of the plugin; that it may be run is for whoever calls this to have been told (the editor's `plugin_trust`).
        let library = unsafe { libloading::Library::new(binary) }
            .map_err(|error| UiError::CouldNotBeLoaded(error.to_string()))?;
        // SAFETY: the symbol is looked up by name and read as the function that `ui.h` says it is; a library that has a symbol of that name that is something else breaks the contract
        // that makes a library that is loaded a library that is trusted.
        let function: UiDescriptorFunction = unsafe {
            *library
                .get::<UiDescriptorFunction>(b"lv2ui_descriptor\0")
                .map_err(|error| UiError::NotAnLv2UiLibrary(error.to_string()))?
        };
        let descriptor =
            find_the_descriptor(function, &ui.uri).ok_or_else(|| UiError::NoSuchInterface(ui.uri.clone()))?;
        // SAFETY: the descriptor is good for as long as the library is loaded, and `library` is kept by the window made below (or dropped after the last use of the descriptor here).
        let (instantiate, cleanup_exists) = unsafe { ((*descriptor).instantiate, (*descriptor).cleanup.is_some()) };
        let instantiate = instantiate.ok_or(UiError::MissingFunction("instantiate"))?;
        if !cleanup_exists {
            return Err(UiError::MissingFunction("cleanup"));
        }

        let shared = Arc::new(Shared {
            events: Mutex::default(),
            asked_size: Mutex::default(),
            symbols: plugin
                .ports
                .iter()
                .filter_map(|port| Some((CString::new(port.symbol.as_str()).ok()?, port.index)))
                .collect(),
        });
        let features = UiFeatures::new(&shared, parent, 48_000.0, scale);
        let plugin_uri = CString::new(plugin.uri.as_str())
            .map_err(|_| UiError::CannotUse("the plugin's URI has a zero byte in it".to_owned()))?;
        let mut bundle = plugin.bundle.to_string_lossy().into_owned();
        if !bundle.ends_with(std::path::MAIN_SEPARATOR) && !bundle.ends_with('/') {
            bundle.push(std::path::MAIN_SEPARATOR);
        }
        let bundle = CString::new(bundle)
            .map_err(|_| UiError::CannotUse("the bundle's path has a zero byte in it".to_owned()))?;
        let mut widget: *mut c_void = std::ptr::null_mut();
        // SAFETY: calling the interface's `instantiate` with its own descriptor, the plugin's URI and the bundle's path (C strings that live for the call), the function and the controller
        // that it will call (the controller is the `Shared` that `shared` keeps for as long as the interface exists), a place for the widget, and the features (kept in `features`,
        // which the window keeps); that it keeps the contract of `ui.h` is what trusting its library is.
        let handle = unsafe {
            instantiate(
                descriptor,
                plugin_uri.as_ptr(),
                bundle.as_ptr(),
                Some(write_callback),
                Arc::as_ptr(&shared) as *mut c_void,
                &raw mut widget,
                features.pointers.as_ptr(),
            )
        };
        if handle.is_null() {
            return Err(UiError::InstantiateFailed);
        }
        let idle_interface = extension_of(descriptor, abi::UI_IDLE_INTERFACE).cast::<RawUiIdleInterface>();
        let resize_interface = extension_of(descriptor, abi::UI_RESIZE).cast::<RawUiResize>();
        let mut window = Self {
            descriptor,
            handle,
            widget,
            idle_interface,
            resize_interface,
            shared,
            _features: features,
            _parent: keep,
            _library: library,
        };
        window.tell_the_values(values);
        Ok(window)
    }

    /// Whether the window can be made another size (the interface answers to `ui:resize`).
    pub fn can_be_resized(&self) -> bool {
        if self.handle.is_null() {
            return false;
        }
        // SAFETY: the pointer is null or what `extension_data` gave, which is good for as long as the library is loaded.
        unsafe { self.resize_interface.as_ref() }.is_some_and(|interface| interface.ui_resize.is_some())
    }

    /// How big the window is: what the system says that the interface's window is, where it can say; else what the interface last asked for; else [`DEFAULT_WINDOW_SIZE`].
    pub fn size(&self) -> (u32, u32) {
        size_of_a_native_widget(self.widget)
            .or(*self.shared.asked_size.lock().unwrap_or_else(PoisonError::into_inner))
            .unwrap_or(DEFAULT_WINDOW_SIZE)
    }

    /// Asks the interface to make its window this big. Whether it says that it did.
    pub fn set_size(&mut self, width: u32, height: u32) -> bool {
        if self.handle.is_null() {
            return false;
        }
        // SAFETY: as for `can_be_resized`.
        let Some(function) = (unsafe { self.resize_interface.as_ref() }).and_then(|interface| interface.ui_resize)
        else {
            return false;
        };
        let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
            return false;
        };
        // SAFETY: calling the interface's own function with the handle of the interface (the contract of `ui:resize` as the interface's extension data).
        let answered = unsafe { function(self.handle, width, height) };
        answered == 0
    }

    /// Tells the interface what these ports are now (control ports: `(index, value)`).
    pub fn tell_the_values(&mut self, values: &[(u32, f64)]) {
        if self.handle.is_null() {
            return;
        }
        // SAFETY: the descriptor is good for as long as the library is loaded.
        let Some(port_event) = (unsafe { (*self.descriptor).port_event }) else {
            return;
        };
        for (port, value) in values {
            let value = *value as f32;
            // SAFETY: calling the interface's own function with its handle, a port, the size of a `float` and protocol 0 (a `float`), and a pointer to one that lives for the call.
            unsafe {
                port_event(
                    self.handle,
                    *port,
                    size_of::<f32>() as u32,
                    0,
                    (&raw const value).cast(),
                )
            };
        }
    }

    /// Gives the interface its time (`ui:idleInterface`), which it needs to draw and to answer a hand. Whether the interface asked to be closed.
    pub fn idle(&mut self) -> bool {
        if self.handle.is_null() {
            return false;
        }
        // SAFETY: the pointer is null or what `extension_data` gave, good as long as the library is loaded.
        let Some(function) = (unsafe { self.idle_interface.as_ref() }).and_then(|interface| interface.idle) else {
            return false;
        };
        // SAFETY: calling the interface's own function with its handle (the contract of `ui:idleInterface`).
        unsafe { function(self.handle) != 0 }
    }

    /// What the interface did since the last time, in order.
    pub fn poll(&mut self) -> Vec<UiEvent> {
        self.shared
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    /// Takes the interface down (its window with it). Done by dropping the window too.
    pub fn destroy(&mut self) {
        if self.handle.is_null() {
            return;
        }
        // SAFETY: the descriptor is good for as long as the library is loaded; `cleanup` was looked for when the interface was made.
        if let Some(cleanup) = unsafe { (*self.descriptor).cleanup } {
            // SAFETY: calling the interface's own `cleanup` once, with its handle, which is not used again (it is set to null just after).
            unsafe { cleanup(self.handle) };
        }
        self.handle = std::ptr::null_mut();
        self.widget = std::ptr::null_mut();
    }
}

impl Drop for UiWindow {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// The descriptor of the user interface whose URI is `uri`, if the library has one.
fn find_the_descriptor(function: UiDescriptorFunction, uri: &str) -> Option<*const RawUiDescriptor> {
    for index in 0..MOST_INTERFACES {
        // SAFETY: calling the library's `lv2ui_descriptor`, which has to answer for any index with a descriptor or null (`ui.h`).
        let descriptor = unsafe { function(index) };
        if descriptor.is_null() {
            return None;
        }
        // SAFETY: a descriptor that the library gave is good for as long as it is loaded; its `uri` is a C string that the library keeps (or null in a library that is broken).
        let said = unsafe {
            let uri_pointer = (*descriptor).uri;
            (!uri_pointer.is_null()).then(|| CStr::from_ptr(uri_pointer).to_string_lossy().into_owned())
        };
        if said.as_deref() == Some(uri) {
            return Some(descriptor);
        }
    }
    None
}

/// What the interface's `extension_data` gives for `uri` (null if it has none, or has no such function).
fn extension_of(descriptor: *const RawUiDescriptor, uri: &str) -> *const c_void {
    let Ok(uri) = CString::new(uri) else {
        return std::ptr::null();
    };
    // SAFETY: the descriptor is good for as long as the library is loaded.
    let Some(function) = (unsafe { (*descriptor).extension_data }) else {
        return std::ptr::null();
    };
    // SAFETY: calling the interface's own `extension_data` with a C string that lives for the call.
    unsafe { function(uri.as_ptr()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value that the options of `features` have for the key that the URI `key` is the name of.
    fn option_of(features: &UiFeatures, key: &str) -> Option<f32> {
        let key = features._table.map(key);
        features
            ._options
            .iter()
            .take_while(|option| option.key != 0)
            .find(|option| option.key == key)
            // SAFETY: the option's value is a pointer to a `float` that `features` keeps for as long as it lives.
            .map(|option| unsafe { std::ptr::read_unaligned(option.value.cast::<f32>()) })
    }

    fn a_shared() -> Arc<Shared> {
        Arc::new(Shared {
            events: Mutex::default(),
            asked_size: Mutex::default(),
            symbols: Vec::new(),
        })
    }

    #[test]
    fn the_options_that_an_interface_is_given_say_the_sample_rate_and_the_scale_of_the_screen() {
        let features = UiFeatures::new(&a_shared(), std::ptr::null_mut(), 44_100.0, 2.5);

        assert_eq!(option_of(&features, abi::PARAM_SAMPLE_RATE), Some(44_100.0));
        assert_eq!(option_of(&features, abi::UI_SCALE_FACTOR), Some(2.5));
        assert_eq!(option_of(&features, "urn:something:else"), None);
    }

    #[test]
    fn the_parent_that_an_interface_is_given_is_the_one_that_it_was_made_with() {
        let shared = a_shared();
        let parent = std::ptr::with_exposed_provenance_mut::<c_void>(0x1234);
        let features = UiFeatures::new(&shared, parent, 48_000.0, 1.0);

        let name_of = |feature: &RawFeature| {
            // SAFETY: each feature's URI is a C string that `features` keeps for as long as it lives.
            let name = unsafe { CStr::from_ptr(feature.uri) };
            name.to_string_lossy().into_owned()
        };
        let given: Vec<(String, *mut c_void)> = features
            ._features
            .iter()
            .map(|feature| (name_of(feature), feature.data))
            .collect();

        let data_of = |uri: &str| given.iter().find(|(name, _)| name == uri).map(|(_, data)| *data);
        assert_eq!(data_of(abi::UI_PARENT), Some(parent));
        assert_eq!(
            data_of(abi::UI_IDLE_INTERFACE),
            Some(std::ptr::null_mut()),
            "a feature that is a yes"
        );
        for uri in [
            abi::URID_MAP,
            abi::URID_UNMAP,
            abi::OPTIONS,
            abi::UI_RESIZE,
            abi::UI_TOUCH,
            abi::UI_PORT_MAP,
        ] {
            assert!(data_of(uri).is_some_and(|data| !data.is_null()), "{uri} has data");
        }
        assert_eq!(given.len(), 8, "and no more");
        assert!(
            features.pointers.last().is_some_and(|last| last.is_null()),
            "the list ends with null"
        );
        assert_eq!(features.pointers.len(), 9);
    }
}
