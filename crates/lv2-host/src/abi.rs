// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The binary interface of LV2 (`lv2.h` and the headers of the extensions that are used: `urid`, `options`, `worker`, `state`, `atom`), written out as Rust types. **This is the part
//! that is easy to get wrong**, so each fact that could be is stated here and tested (`tests.rs`):
//!
//! * a C `enum` is an `int`, so the context of an option, the status that the worker and the state functions give, and the flags of a property are 32-bit integers;
//! * `LV2_URID` is a `uint32_t`; the size in `LV2_State_Store_Function` is a `size_t`, which is as wide as a pointer (not 32 bits on 64-bit systems);
//! * every function of a descriptor can be missing in a library that is broken, so each is an `Option` of a function pointer (null is `None`), and the host looks before it calls.

#![allow(unsafe_code)]

use std::ffi::{c_char, c_void};

/// `LV2_Handle`: what `instantiate` gives and every other function takes.
pub(crate) type Handle = *mut c_void;

/// `LV2_Feature`: a URI, and what the host gives with it (null for a feature that has no data).
#[repr(C)]
pub(crate) struct RawFeature {
    pub uri: *const c_char,
    pub data: *mut c_void,
}

/// `LV2_Descriptor`.
#[repr(C)]
pub(crate) struct RawDescriptor {
    pub uri: *const c_char,
    pub instantiate: Option<
        unsafe extern "C" fn(
            descriptor: *const RawDescriptor,
            sample_rate: f64,
            bundle_path: *const c_char,
            features: *const *const RawFeature,
        ) -> Handle,
    >,
    pub connect_port: Option<unsafe extern "C" fn(instance: Handle, port: u32, data: *mut c_void)>,
    pub activate: Option<unsafe extern "C" fn(instance: Handle)>,
    pub run: Option<unsafe extern "C" fn(instance: Handle, sample_count: u32)>,
    pub deactivate: Option<unsafe extern "C" fn(instance: Handle)>,
    pub cleanup: Option<unsafe extern "C" fn(instance: Handle)>,
    pub extension_data: Option<unsafe extern "C" fn(uri: *const c_char) -> *const c_void>,
}

/// `LV2_Descriptor_Function`: the one function that a library exports, as `lv2_descriptor`.
pub(crate) type DescriptorFunction = unsafe extern "C" fn(index: u32) -> *const RawDescriptor;

/// `LV2_URID_Map`.
#[repr(C)]
pub(crate) struct RawUridMap {
    pub handle: *mut c_void,
    pub map: Option<unsafe extern "C" fn(handle: *mut c_void, uri: *const c_char) -> u32>,
}

/// `LV2_URID_Unmap`.
#[repr(C)]
pub(crate) struct RawUridUnmap {
    pub handle: *mut c_void,
    pub unmap: Option<unsafe extern "C" fn(handle: *mut c_void, urid: u32) -> *const c_char>,
}

/// `LV2_Options_Option`. (`context` is an `LV2_Options_Context`, an `int`; 0 is the instance.)
#[repr(C)]
pub(crate) struct RawOption {
    pub context: i32,
    pub subject: u32,
    pub key: u32,
    pub size: u32,
    pub type_: u32,
    pub value: *const c_void,
}

/// `LV2_Worker_Respond_Function`.
pub(crate) type RespondFunction = unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> i32;

/// `LV2_Worker_Schedule`.
#[repr(C)]
pub(crate) struct RawWorkerSchedule {
    pub handle: *mut c_void,
    pub schedule_work: Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> i32>,
}

/// `LV2_Worker_Interface`.
#[repr(C)]
pub(crate) struct RawWorkerInterface {
    pub work: Option<
        unsafe extern "C" fn(
            instance: Handle,
            respond: Option<RespondFunction>,
            respond_handle: *mut c_void,
            size: u32,
            data: *const c_void,
        ) -> i32,
    >,
    pub work_response: Option<unsafe extern "C" fn(instance: Handle, size: u32, body: *const c_void) -> i32>,
    pub end_run: Option<unsafe extern "C" fn(instance: Handle) -> i32>,
}

/// `LV2_State_Store_Function`.
pub(crate) type StoreFunction = unsafe extern "C" fn(
    handle: *mut c_void,
    key: u32,
    value: *const c_void,
    size: usize,
    type_: u32,
    flags: u32,
) -> i32;

/// `LV2_State_Retrieve_Function`.
pub(crate) type RetrieveFunction = unsafe extern "C" fn(
    handle: *mut c_void,
    key: u32,
    size: *mut usize,
    type_: *mut u32,
    flags: *mut u32,
) -> *const c_void;

/// `LV2_State_Interface`.
#[repr(C)]
pub(crate) struct RawStateInterface {
    pub save: Option<
        unsafe extern "C" fn(
            instance: Handle,
            store: Option<StoreFunction>,
            handle: *mut c_void,
            flags: u32,
            features: *const *const RawFeature,
        ) -> i32,
    >,
    pub restore: Option<
        unsafe extern "C" fn(
            instance: Handle,
            retrieve: Option<RetrieveFunction>,
            handle: *mut c_void,
            flags: u32,
            features: *const *const RawFeature,
        ) -> i32,
    >,
}

/// The header of an `LV2_Atom_Sequence`: an `LV2_Atom` (size of what follows it, and the type) and the body's own header (the unit of the time stamps, and padding).
#[repr(C)]
pub(crate) struct RawAtomSequenceHeader {
    pub size: u32,
    pub type_: u32,
    pub unit: u32,
    pub pad: u32,
}

// The URIs.
pub(crate) const URID_MAP: &str = "http://lv2plug.in/ns/ext/urid#map";
pub(crate) const URID_UNMAP: &str = "http://lv2plug.in/ns/ext/urid#unmap";
pub(crate) const OPTIONS: &str = "http://lv2plug.in/ns/ext/options#options";
pub(crate) const BOUNDED_BLOCK_LENGTH: &str = "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
pub(crate) const WORKER_SCHEDULE: &str = "http://lv2plug.in/ns/ext/worker#schedule";
pub(crate) const WORKER_INTERFACE: &str = "http://lv2plug.in/ns/ext/worker#interface";
pub(crate) const STATE_INTERFACE: &str = "http://lv2plug.in/ns/ext/state#interface";
pub(crate) const STATE_LOAD_DEFAULT_STATE: &str = "http://lv2plug.in/ns/ext/state#loadDefaultState";
pub(crate) const HARD_RT_CAPABLE: &str = "http://lv2plug.in/ns/lv2core#hardRTCapable";
pub(crate) const IS_LIVE: &str = "http://lv2plug.in/ns/lv2core#isLive";
pub(crate) const IN_PLACE_BROKEN: &str = "http://lv2plug.in/ns/lv2core#inPlaceBroken";
pub(crate) const PARAM_SAMPLE_RATE: &str = "http://lv2plug.in/ns/ext/parameters#sampleRate";
pub(crate) const BUFSZ_MIN_BLOCK_LENGTH: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
pub(crate) const BUFSZ_MAX_BLOCK_LENGTH: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
pub(crate) const BUFSZ_NOMINAL_BLOCK_LENGTH: &str = "http://lv2plug.in/ns/ext/buf-size#nominalBlockLength";
pub(crate) const BUFSZ_SEQUENCE_SIZE: &str = "http://lv2plug.in/ns/ext/buf-size#sequenceSize";
pub(crate) const ATOM_FLOAT: &str = "http://lv2plug.in/ns/ext/atom#Float";
pub(crate) const ATOM_INT: &str = "http://lv2plug.in/ns/ext/atom#Int";
pub(crate) const ATOM_SEQUENCE: &str = "http://lv2plug.in/ns/ext/atom#Sequence";
pub(crate) const ATOM_CHUNK: &str = "http://lv2plug.in/ns/ext/atom#Chunk";
pub(crate) const MIDI_EVENT: &str = "http://lv2plug.in/ns/ext/midi#MidiEvent";

/// The features that this host can give a plugin that says it needs them (and the ones that need nothing from the host but a yes). A plugin that requires another is not made.
pub(crate) const SUPPORTED_FEATURES: [&str; 9] = [
    URID_MAP,
    URID_UNMAP,
    OPTIONS,
    BOUNDED_BLOCK_LENGTH,
    WORKER_SCHEDULE,
    STATE_LOAD_DEFAULT_STATE,
    HARD_RT_CAPABLE,
    IS_LIVE,
    IN_PLACE_BROKEN,
];

// The URIs of the user interfaces (the extension `ui`), and what the host gives them.
pub(crate) const UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
pub(crate) const UI_RESIZE: &str = "http://lv2plug.in/ns/extensions/ui#resize";
pub(crate) const UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
pub(crate) const UI_TOUCH: &str = "http://lv2plug.in/ns/extensions/ui#touch";
pub(crate) const UI_PORT_MAP: &str = "http://lv2plug.in/ns/extensions/ui#portMap";
pub(crate) const UI_SCALE_FACTOR: &str = "http://lv2plug.in/ns/extensions/ui#scaleFactor";

/// The features that this host can give a user interface that says it needs them. A user interface that requires another is not shown. (`ui:noUserResize` and `ui:fixedSize` are
/// things that a UI says of itself; the host that gives a UI no way to be resized does not break them.)
pub(crate) const SUPPORTED_UI_FEATURES: [&str; 8] = [
    UI_PARENT,
    UI_RESIZE,
    UI_IDLE_INTERFACE,
    UI_TOUCH,
    UI_PORT_MAP,
    URID_MAP,
    URID_UNMAP,
    OPTIONS,
];
