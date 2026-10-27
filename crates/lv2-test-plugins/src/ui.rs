// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

//! The user interfaces (windows) of the test plugins (`lv2ui_descriptor`, written from `ui.h`):
//!
//! * `urn:fastforwardrs:test:gain#ui`, the window of the gain plugin. **When it is made** it says what it was given (`lv2-test-ui: made ...`, `options ...`, `urid ...`, `resize ...`:
//!   lines on its standard output, which the program that hosts it passes over as noise), asks the host to make its window [`UI_SIZE`] with `ui:resize` (after two sizes that are not
//!   sizes, which the host is to refuse), and, **on Windows, if it has a parent window, makes a real child window in it** ([`UI_REAL_WINDOW_SIZE`], a `STATIC` with a line of text, so
//!   that a person can see it, and so that the size that the system says is not the size that was asked for); with no parent it draws nothing. **The first time it is given its
//!   time** (`idle`) it does what a person's hand would: it takes hold of the gain (`touch`), writes [`UI_EDIT_VALUE`] to it, and lets go, and writes what is not a `float` (a
//!   protocol that is not 0, a size that is not four bytes, no buffer), which the host has to throw away. Whenever it is told a value it says so (`port_event`); **if the value is
//!   [`UI_CLOSE_VALUE`] for the gain, its next `idle` says that the window is to close.** It can be resized by the host (it says `resized` and the size).
//! * `urn:fastforwardrs:test:quiet#ui`, the window of the latency plugin, **which asks for nothing and answers to nothing**: no size, no `idle`, no `resize` extension data.
//! * `urn:fastforwardrs:test:fails#ui`, whose `instantiate` gives nothing.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use super::{Feature, OptionEntry, UridMap, UridUnmap, map_with};

pub const GAIN_UI: &str = "urn:fastforwardrs:test:gain#ui";
pub const QUIET_UI: &str = "urn:fastforwardrs:test:quiet#ui";
pub const FAILS_UI: &str = "urn:fastforwardrs:test:fails#ui";

/// What the gain becomes when the window of the gain plugin makes its edit.
pub const UI_EDIT_VALUE: f32 = 0.75;
/// What the window of the gain plugin asks to be made.
pub const UI_SIZE: (u32, u32) = (360, 240);
/// How big the real window that it makes is, where it makes one (Windows, with a parent): not what it asked for, so that the size that the system says is told from the one that was asked.
pub const UI_REAL_WINDOW_SIZE: (u32, u32) = (380, 250);
/// The value of the gain that makes the window ask to be closed.
pub const UI_CLOSE_VALUE: f32 = 3.0;
/// The value that makes the window ask to be closed when it is told it for **the port after the gain** (a meter: the gain plugin has no such port, and a test that plays no sound
/// says it as if the plugin made it).
pub const UI_METER_CLOSE_VALUE: f32 = 7.0;

type Write = unsafe extern "C" fn(*mut c_void, u32, u32, u32, *const c_void);

#[repr(C)]
pub struct UiDescriptor {
    uri: *const c_char,
    instantiate: Option<
        unsafe extern "C" fn(
            *const UiDescriptor,
            *const c_char,
            *const c_char,
            Option<Write>,
            *mut c_void,
            *mut *mut c_void,
            *const *const Feature,
        ) -> *mut c_void,
    >,
    cleanup: Option<unsafe extern "C" fn(*mut c_void)>,
    port_event: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32, *const c_void)>,
    extension_data: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

// SAFETY: a descriptor is read-only data (a URI and function pointers) that is never changed.
unsafe impl Sync for UiDescriptor {}

#[repr(C)]
struct Resize {
    handle: *mut c_void,
    ui_resize: Option<unsafe extern "C" fn(*mut c_void, i32, i32) -> i32>,
}

// SAFETY: read-only function pointers.
unsafe impl Sync for Resize {}

#[repr(C)]
struct Touch {
    handle: *mut c_void,
    touch: Option<unsafe extern "C" fn(*mut c_void, u32, bool)>,
}

#[repr(C)]
struct PortMap {
    handle: *mut c_void,
    port_index: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> u32>,
}

#[repr(C)]
struct Idle {
    idle: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

// SAFETY: read-only function pointers.
unsafe impl Sync for Idle {}

const UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
const UI_RESIZE: &str = "http://lv2plug.in/ns/extensions/ui#resize";
const UI_TOUCH: &str = "http://lv2plug.in/ns/extensions/ui#touch";
const UI_PORT_MAP: &str = "http://lv2plug.in/ns/extensions/ui#portMap";
const UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
const UI_SCALE_FACTOR: &str = "http://lv2plug.in/ns/extensions/ui#scaleFactor";

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: *mut c_void,
            menu: *mut c_void,
            instance: *mut c_void,
            parameter: *mut c_void,
        ) -> *mut c_void;
        pub fn DestroyWindow(window: *mut c_void) -> i32;
        pub fn SetWindowPos(
            window: *mut c_void,
            after: *mut c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }

    pub const WS_CHILD: u32 = 0x4000_0000;
    pub const WS_VISIBLE: u32 = 0x1000_0000;
    pub const SS_CENTER: u32 = 0x0000_0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOZORDER: u32 = 0x0004;
}

/// The window of the gain plugin: what it keeps.
struct Ui {
    write: Option<Write>,
    controller: *mut c_void,
    touch: *const Touch,
    gain_port: u32,
    edited: bool,
    close_next: bool,
    /// The child window that it made, if it made one (Windows, with a parent).
    #[cfg_attr(not(windows), allow(dead_code))]
    child: *mut c_void,
}

/// The data that the host gave for the feature `uri`, if it gave the feature (it may be null: a feature that has no data, or `ui:parent` of a window that is not there).
fn feature_of(features: *const *const Feature, uri: &str) -> Option<*mut c_void> {
    if features.is_null() {
        return None;
    }
    let mut place = 0;
    loop {
        // SAFETY: the host gives a list of pointers that ends with a null pointer (`lv2.h`), and each is to a feature that is good for the call.
        let feature = unsafe { *features.add(place) };
        if feature.is_null() {
            return None;
        }
        // SAFETY: a feature's URI is a C string that the host keeps.
        let name = unsafe { CStr::from_ptr((*feature).uri) };
        if name.to_str() == Ok(uri) {
            // SAFETY: as above.
            return Some(unsafe { (*feature).data });
        }
        place += 1;
    }
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn make_a_child_window(parent: *mut c_void) -> *mut c_void {
    let class = wide("STATIC");
    let title = wide("fastforwardrs test LV2 window");
    // SAFETY: `CreateWindowExW` with a class that the system has, a title that lives for the call, and a parent that the host gave as the window that this one is to be in.
    unsafe {
        win32::CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            win32::WS_CHILD | win32::WS_VISIBLE | win32::SS_CENTER,
            0,
            0,
            UI_REAL_WINDOW_SIZE.0 as i32,
            UI_REAL_WINDOW_SIZE.1 as i32,
            parent,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    }
}

#[cfg(not(windows))]
fn make_a_child_window(_parent: *mut c_void) -> *mut c_void {
    // (Nothing is drawn on the other systems: the window is not made.)
    std::ptr::null_mut()
}

/// What the host's options say of the sample rate and the scale factor, as the text that is printed.
fn options_said(features: *const *const Feature) -> String {
    let (Some(map), Some(options)) = (
        feature_of(features, super::URID_MAP).filter(|data| !data.is_null()),
        feature_of(features, super::OPTIONS_FEATURE).filter(|data| !data.is_null()),
    ) else {
        return "options missing".to_owned();
    };
    let map = map.cast::<UridMap>().cast_const();
    let (rate_key, scale_key, float) = (
        map_with(map, super::PARAM_SAMPLE_RATE),
        map_with(map, UI_SCALE_FACTOR),
        map_with(map, super::ATOM_FLOAT),
    );
    let (mut rate, mut scale) = (None, None);
    let mut place = 0;
    loop {
        // SAFETY: the options are a list that ends with an option whose key is 0 (`options.h`), kept by the host for as long as the interface.
        let option = unsafe { &*options.cast::<OptionEntry>().add(place) };
        if option.key == 0 {
            break;
        }
        if option.type_ == float && option.size == 4 && !option.value.is_null() {
            // SAFETY: the option says that its value is a `float` of four bytes.
            let value = unsafe { std::ptr::read_unaligned(option.value.cast::<f32>()) };
            if option.key == rate_key {
                rate = Some(value);
            } else if option.key == scale_key {
                scale = Some(value);
            }
        }
        place += 1;
    }
    format!(
        "options sample_rate={} scale_factor={}",
        rate.map_or("none".to_owned(), |value| value.to_string()),
        scale.map_or("none".to_owned(), |value| value.to_string())
    )
}

/// Whether the host's `urid:unmap` gives back the URI that its `urid:map` made a number of.
fn urid_round_trip(features: *const *const Feature) -> bool {
    let (Some(map), Some(unmap)) = (
        feature_of(features, super::URID_MAP).filter(|data| !data.is_null()),
        feature_of(features, super::URID_UNMAP).filter(|data| !data.is_null()),
    ) else {
        return false;
    };
    let uri = "urn:fastforwardrs:test:ui_round_trip";
    let urid = map_with(map.cast::<UridMap>().cast_const(), uri);
    let unmap = unmap.cast::<UridUnmap>();
    // SAFETY: the host gave the unmap feature, which is a table that it keeps for as long as the interface.
    let back = unsafe { (*unmap).unmap.map(|function| function((*unmap).handle, urid)) };
    // SAFETY: a URI that the host gives is a C string that it keeps.
    urid != 0 && back.is_some_and(|text| !text.is_null() && unsafe { CStr::from_ptr(text) }.to_str() == Ok(uri))
}

unsafe extern "C" fn instantiate_gain(
    _descriptor: *const UiDescriptor,
    plugin_uri: *const c_char,
    bundle_path: *const c_char,
    write: Option<Write>,
    controller: *mut c_void,
    widget: *mut *mut c_void,
    features: *const *const Feature,
) -> *mut c_void {
    // SAFETY: the host gives C strings that are good for the call.
    let (plugin, bundle) = unsafe {
        (
            CStr::from_ptr(plugin_uri).to_string_lossy(),
            CStr::from_ptr(bundle_path).to_string_lossy(),
        )
    };
    let parent = feature_of(features, UI_PARENT);
    let resize = feature_of(features, UI_RESIZE).filter(|data| !data.is_null());
    let touch = feature_of(features, UI_TOUCH).filter(|data| !data.is_null());
    let port_map = feature_of(features, UI_PORT_MAP).filter(|data| !data.is_null());
    let idle_given = feature_of(features, UI_IDLE_INTERFACE).is_some();
    let (mut gain_port, mut missing_port) = (u32::MAX, 0);
    if let Some(data) = port_map {
        let map = data.cast::<PortMap>();
        // SAFETY: the host gave the port map feature, which is a table that it keeps for as long as the interface; the symbols are C strings that live for the calls.
        unsafe {
            if let Some(function) = (*map).port_index {
                gain_port = function((*map).handle, c"gain".as_ptr());
                missing_port = function((*map).handle, c"no_such_port".as_ptr());
            }
        }
    }
    let parent_said = match parent {
        None => "missing",
        Some(data) if data.is_null() => "none",
        Some(_) => "given",
    };
    println!(
        "lv2-test-ui: made plugin={plugin} bundle_ends_with_a_separator={} parent={parent_said} resize={} touch={} idle={idle_given} gain_port={gain_port} missing_port={missing_port}",
        bundle.ends_with(['/', '\\']),
        resize.is_some(),
        touch.is_some()
    );
    println!("lv2-test-ui: {}", options_said(features));
    println!("lv2-test-ui: urid round_trip={}", urid_round_trip(features));
    let mut child = std::ptr::null_mut();
    if let Some(parent) = parent.filter(|data| !data.is_null()) {
        child = make_a_child_window(parent);
    }
    if let Some(data) = resize {
        let function = data.cast::<Resize>();
        // SAFETY: the host gave the resize feature, which is a table that it keeps for as long as the interface.
        let answers = unsafe {
            (*function).ui_resize.map(|resize| {
                // (Two that are not sizes, which the host refuses; and the size that it asks for.)
                [
                    resize((*function).handle, 0, 100),
                    resize((*function).handle, -5, 100),
                    resize((*function).handle, UI_SIZE.0 as i32, UI_SIZE.1 as i32),
                ]
            })
        };
        println!("lv2-test-ui: resize answers={answers:?}");
    }
    if !widget.is_null() {
        // SAFETY: the host gave a place for the widget.
        unsafe { *widget = child };
    }
    Box::into_raw(Box::new(Ui {
        write,
        controller,
        touch: touch.map_or(std::ptr::null(), |data| data.cast::<Touch>().cast_const()),
        gain_port,
        edited: false,
        close_next: false,
        child,
    }))
    .cast()
}

unsafe extern "C" fn instantiate_quiet(
    _descriptor: *const UiDescriptor,
    _plugin_uri: *const c_char,
    _bundle_path: *const c_char,
    _write: Option<Write>,
    _controller: *mut c_void,
    widget: *mut *mut c_void,
    _features: *const *const Feature,
) -> *mut c_void {
    println!("lv2-test-ui: quiet made");
    if !widget.is_null() {
        // SAFETY: the host gave a place for the widget.
        unsafe { *widget = std::ptr::null_mut() };
    }
    Box::into_raw(Box::new(Ui {
        write: None,
        controller: std::ptr::null_mut(),
        touch: std::ptr::null(),
        gain_port: u32::MAX,
        edited: true,
        close_next: false,
        child: std::ptr::null_mut(),
    }))
    .cast()
}

unsafe extern "C" fn instantiate_that_fails(
    _descriptor: *const UiDescriptor,
    _plugin_uri: *const c_char,
    _bundle_path: *const c_char,
    _write: Option<Write>,
    _controller: *mut c_void,
    _widget: *mut *mut c_void,
    _features: *const *const Feature,
) -> *mut c_void {
    std::ptr::null_mut()
}

unsafe extern "C" fn cleanup(handle: *mut c_void) {
    // SAFETY: the pointer is the `Box` that an `instantiate` of this library made, which the host gives back once.
    let ui = unsafe { Box::from_raw(handle.cast::<Ui>()) };
    #[cfg(windows)]
    if !ui.child.is_null() {
        // SAFETY: the window was made by this interface and has not been destroyed.
        unsafe { win32::DestroyWindow(ui.child) };
    }
    drop(ui);
    println!("lv2-test-ui: cleanup");
}

unsafe extern "C" fn port_event(handle: *mut c_void, port: u32, size: u32, format: u32, buffer: *const c_void) {
    // SAFETY: the pointer is the `Ui` that an `instantiate` of this library made, which the host keeps until `cleanup`, and calls on one thread.
    let ui = unsafe { &mut *handle.cast::<Ui>() };
    if format == 0 && size == 4 && !buffer.is_null() {
        // SAFETY: protocol 0 is a `float` of four bytes, good for the call.
        let value = unsafe { std::ptr::read_unaligned(buffer.cast::<f32>()) };
        println!("lv2-test-ui: port_event {port} {value}");
        if (port == ui.gain_port && value == UI_CLOSE_VALUE)
            || (port == ui.gain_port.wrapping_add(1) && value == UI_METER_CLOSE_VALUE)
        {
            ui.close_next = true;
        }
    }
}

unsafe extern "C" fn idle(handle: *mut c_void) -> i32 {
    // SAFETY: as for `port_event`.
    let ui = unsafe { &mut *handle.cast::<Ui>() };
    if !ui.edited {
        ui.edited = true;
        if let Some(write) = ui.write
            && ui.gain_port != u32::MAX
        {
            let value = UI_EDIT_VALUE;
            let not_a_float: u32 = 7;
            let two_floats = [value, value];
            // SAFETY: calling the host's own functions with the controller it gave, a port, a size, a protocol and a pointer to values that live for the calls (or none: the host is not to
            // read what it is not given).
            unsafe {
                if !ui.touch.is_null()
                    && let Some(touch) = (*ui.touch).touch
                {
                    touch((*ui.touch).handle, ui.gain_port, true);
                }
                write(ui.controller, ui.gain_port, 4, 0, (&raw const value).cast());
                // (A protocol that is not 0, which is an atom: this host does not understand it and is to throw it away. So is a size that is not a `float`'s, and no buffer.)
                write(ui.controller, ui.gain_port, 4, 12_345, (&raw const not_a_float).cast());
                write(ui.controller, ui.gain_port, 8, 0, two_floats.as_ptr().cast());
                write(ui.controller, ui.gain_port, 4, 0, std::ptr::null());
                if !ui.touch.is_null()
                    && let Some(touch) = (*ui.touch).touch
                {
                    touch((*ui.touch).handle, ui.gain_port, false);
                }
            }
        }
    }
    i32::from(ui.close_next)
}

unsafe extern "C" fn host_resizes(handle: *mut c_void, width: i32, height: i32) -> i32 {
    println!("lv2-test-ui: resized {width} {height}");
    #[cfg(windows)]
    {
        // SAFETY: as for `port_event`.
        let ui = unsafe { &*handle.cast::<Ui>() };
        if !ui.child.is_null() {
            // SAFETY: the window was made by this interface and has not been destroyed.
            unsafe {
                win32::SetWindowPos(
                    ui.child,
                    std::ptr::null_mut(),
                    0,
                    0,
                    width,
                    height,
                    win32::SWP_NOMOVE | win32::SWP_NOZORDER,
                )
            };
        }
    }
    #[cfg(not(windows))]
    let _ = handle;
    0
}

static RESIZE_INTERFACE: Resize = Resize {
    handle: std::ptr::null_mut(),
    ui_resize: Some(host_resizes),
};

static IDLE_INTERFACE: Idle = Idle { idle: Some(idle) };

unsafe extern "C" fn extension_data(uri: *const c_char) -> *const c_void {
    // SAFETY: the host gives a C string that is good for the call.
    match unsafe { CStr::from_ptr(uri) }.to_str() {
        Ok(UI_IDLE_INTERFACE) => (&raw const IDLE_INTERFACE).cast(),
        Ok(UI_RESIZE) => (&raw const RESIZE_INTERFACE).cast(),
        _ => std::ptr::null(),
    }
}

static UI_DESCRIPTORS: [UiDescriptor; 3] = [
    UiDescriptor {
        uri: c"urn:fastforwardrs:test:gain#ui".as_ptr(),
        instantiate: Some(instantiate_gain),
        cleanup: Some(cleanup),
        port_event: Some(port_event),
        extension_data: Some(extension_data),
    },
    UiDescriptor {
        uri: c"urn:fastforwardrs:test:quiet#ui".as_ptr(),
        instantiate: Some(instantiate_quiet),
        cleanup: Some(cleanup),
        port_event: None,
        extension_data: None,
    },
    UiDescriptor {
        uri: c"urn:fastforwardrs:test:fails#ui".as_ptr(),
        instantiate: Some(instantiate_that_fails),
        cleanup: Some(cleanup),
        port_event: None,
        extension_data: None,
    },
];

/// The function that an LV2 user interface library exports.
#[unsafe(no_mangle)]
pub extern "C" fn lv2ui_descriptor(index: u32) -> *const UiDescriptor {
    UI_DESCRIPTORS
        .get(index as usize)
        .map_or(std::ptr::null(), std::ptr::from_ref)
}
