// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

//! An LV2 plugin library written for the tests of fastforwardrs, and the Turtle of the bundle that goes with it (`write_bundle`). It does nothing but arithmetic. The plugins:
//!
//! | URI | what it is |
//! |---|---|
//! | `urn:fastforwardrs:test:gain` | one audio in, one audio out, a `gain` control |
//! | `urn:fastforwardrs:test:controls` | a plugin with every kind of control port (a switch, a whole number, a choice with scale points, a logarithmic one, one that is a fraction of the sample rate), to be described; it copies its input |
//! | `urn:fastforwardrs:test:latency` | delays its sound by 64 frames and says so on a control output designated as the latency |
//! | `urn:fastforwardrs:test:atoms` | has atom ports; says on control outputs whether its atom input was an empty sequence and how much room its atom output had |
//! | `urn:fastforwardrs:test:options` | says on control outputs what the options it was made with were (the sample rate and the lengths of the blocks), whether it was given the bounded-block feature, and whether the bundle's path it was given ends with a separator |
//! | `urn:fastforwardrs:test:worker` | **requires the worker**: schedules work at its first block, which doubles a number and answers; its output is its input times that number (1 until the answer comes) |
//! | `urn:fastforwardrs:test:needs_log` | **requires `log:log`**, which a host may not give: it is a gain if it is made |
//! | `urn:fastforwardrs:test:state` | a gain that has a **state** (`state:interface`): a number that it multiplies by (set by a control input, and kept by `save` and `restore`) and a name |
//! | `urn:fastforwardrs:test:fails` | whose `instantiate` gives nothing |
//!
//! The Turtle is written by [`write_bundle`] with the library copied beside it.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};

pub const GAIN: &str = "urn:fastforwardrs:test:gain";
pub const CONTROLS: &str = "urn:fastforwardrs:test:controls";
pub const LATENCY: &str = "urn:fastforwardrs:test:latency";
pub const ATOMS: &str = "urn:fastforwardrs:test:atoms";
pub const OPTIONS: &str = "urn:fastforwardrs:test:options";
pub const WORKER: &str = "urn:fastforwardrs:test:worker";
pub const NEEDS_LOG: &str = "urn:fastforwardrs:test:needs_log";
pub const STATE: &str = "urn:fastforwardrs:test:state";
pub const FAILS: &str = "urn:fastforwardrs:test:fails";

/// How many frames the `latency` plugin delays its sound by.
pub const LATENCY_FRAMES: usize = 64;

/// The URIs of the keys of the `state` plugin's properties.
pub const STATE_BOOST_KEY: &str = "urn:fastforwardrs:test:state#boost";
pub const STATE_NAME_KEY: &str = "urn:fastforwardrs:test:state#name";

// ---- the binary interface (a copy of the host's: this library does not depend on it) ----

type Handle = *mut c_void;

#[repr(C)]
struct Feature {
    uri: *const c_char,
    data: *mut c_void,
}

#[repr(C)]
pub struct Descriptor {
    uri: *const c_char,
    instantiate: Option<unsafe extern "C" fn(*const Descriptor, f64, *const c_char, *const *const Feature) -> Handle>,
    connect_port: Option<unsafe extern "C" fn(Handle, u32, *mut c_void)>,
    activate: Option<unsafe extern "C" fn(Handle)>,
    run: Option<unsafe extern "C" fn(Handle, u32)>,
    deactivate: Option<unsafe extern "C" fn(Handle)>,
    cleanup: Option<unsafe extern "C" fn(Handle)>,
    extension_data: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
}

// SAFETY: a descriptor is read-only data (a URI and function pointers) that is never changed.
unsafe impl Sync for Descriptor {}

#[repr(C)]
struct UridMap {
    handle: *mut c_void,
    map: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> u32>,
}

#[repr(C)]
struct UridUnmap {
    handle: *mut c_void,
    unmap: Option<unsafe extern "C" fn(*mut c_void, u32) -> *const c_char>,
}

#[repr(C)]
struct OptionEntry {
    context: i32,
    subject: u32,
    key: u32,
    size: u32,
    type_: u32,
    value: *const c_void,
}

#[repr(C)]
struct WorkerSchedule {
    handle: *mut c_void,
    schedule_work: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_void) -> i32>,
}

type Respond = unsafe extern "C" fn(*mut c_void, u32, *const c_void) -> i32;

#[repr(C)]
struct WorkerInterface {
    work: Option<unsafe extern "C" fn(Handle, Option<Respond>, *mut c_void, u32, *const c_void) -> i32>,
    work_response: Option<unsafe extern "C" fn(Handle, u32, *const c_void) -> i32>,
    end_run: Option<unsafe extern "C" fn(Handle) -> i32>,
}

// SAFETY: read-only function pointers.
unsafe impl Sync for WorkerInterface {}

type Store = unsafe extern "C" fn(*mut c_void, u32, *const c_void, usize, u32, u32) -> i32;
type Retrieve = unsafe extern "C" fn(*mut c_void, u32, *mut usize, *mut u32, *mut u32) -> *const c_void;

#[repr(C)]
struct StateInterface {
    save: Option<unsafe extern "C" fn(Handle, Option<Store>, *mut c_void, u32, *const *const Feature) -> i32>,
    restore: Option<unsafe extern "C" fn(Handle, Option<Retrieve>, *mut c_void, u32, *const *const Feature) -> i32>,
}

// SAFETY: read-only function pointers.
unsafe impl Sync for StateInterface {}

const URID_MAP: &str = "http://lv2plug.in/ns/ext/urid#map";
const URID_UNMAP: &str = "http://lv2plug.in/ns/ext/urid#unmap";
const OPTIONS_FEATURE: &str = "http://lv2plug.in/ns/ext/options#options";
const BOUNDED: &str = "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
const WORKER_SCHEDULE: &str = "http://lv2plug.in/ns/ext/worker#schedule";
const WORKER_INTERFACE: &str = "http://lv2plug.in/ns/ext/worker#interface";
const STATE_INTERFACE: &str = "http://lv2plug.in/ns/ext/state#interface";
const ATOM_SEQUENCE: &str = "http://lv2plug.in/ns/ext/atom#Sequence";
const ATOM_CHUNK: &str = "http://lv2plug.in/ns/ext/atom#Chunk";
const ATOM_FLOAT: &str = "http://lv2plug.in/ns/ext/atom#Float";
const ATOM_STRING: &str = "http://lv2plug.in/ns/ext/atom#String";
const PARAM_SAMPLE_RATE: &str = "http://lv2plug.in/ns/ext/parameters#sampleRate";
const MIN_BLOCK: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const MAX_BLOCK: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
const NOMINAL_BLOCK: &str = "http://lv2plug.in/ns/ext/buf-size#nominalBlockLength";

// ---- the plugins ----

/// What a plugin does, on top of the trampolines that every plugin shares.
trait Plugin {
    fn connect(&mut self, port: u32, data: *mut c_void);
    fn run(&mut self, frames: usize);
    fn activate(&mut self) {}
}

/// What `instantiate` was given, read.
struct Given<'a> {
    sample_rate: f64,
    bundle_ends_with_a_separator: bool,
    features: Vec<(&'a str, *mut c_void)>,
}

impl Given<'_> {
    fn feature(&self, uri: &str) -> Option<*mut c_void> {
        self.features
            .iter()
            .find(|(name, _)| *name == uri)
            .map(|(_, data)| *data)
    }

    /// The number that the host's `urid:map` gives `uri`.
    fn map(&self, uri: &str) -> u32 {
        let Some(data) = self.feature(URID_MAP) else { return 0 };
        map_with(data.cast::<UridMap>(), uri)
    }
}

fn map_with(map: *const UridMap, uri: &str) -> u32 {
    let text = format!("{uri}\0");
    // SAFETY: the host gave the map feature, which is a table that it keeps alive for as long as the plugin; `uri` is a C string made just above.
    unsafe {
        (*map)
            .map
            .map_or(0, |function| function((*map).handle, text.as_ptr().cast()))
    }
}

/// A buffer that a port is connected to: audio (floats) or one number.
struct Ports {
    audio_in: *const f32,
    audio_out: *mut f32,
}

impl Ports {
    fn new() -> Self {
        Self {
            audio_in: std::ptr::null(),
            audio_out: std::ptr::null_mut(),
        }
    }

    fn input(&self, frames: usize) -> &[f32] {
        if self.audio_in.is_null() {
            return &[];
        }
        // SAFETY: the host connected the input to a buffer of at least `frames` samples before `run` (`lv2.h`) and keeps it for the call.
        unsafe { std::slice::from_raw_parts(self.audio_in, frames) }
    }

    fn output(&mut self, frames: usize) -> &mut [f32] {
        if self.audio_out.is_null() {
            return &mut [];
        }
        // SAFETY: as for the input: a buffer of at least `frames` samples that the host connected and keeps for the call, and nothing else here holds it.
        unsafe { std::slice::from_raw_parts_mut(self.audio_out, frames) }
    }
}

/// Reads the number at a control port.
fn control(pointer: *const f32) -> f32 {
    if pointer.is_null() {
        return 0.0;
    }
    // SAFETY: the host connected the port to a float that it keeps for as long as it is connected.
    unsafe { *pointer }
}

fn write_control(pointer: *mut f32, value: f32) {
    if !pointer.is_null() {
        // SAFETY: as for `control`: a float that the host keeps, and that a control output is for the plugin to write.
        unsafe { *pointer = value };
    }
}

macro_rules! audio_ports {
    () => {
        fn connect_audio(&mut self, port: u32, data: *mut c_void) -> bool {
            match port {
                0 => self.ports.audio_in = data.cast(),
                1 => self.ports.audio_out = data.cast(),
                _ => return false,
            }
            true
        }
    };
}

/// `gain`: out = in * gain (port 2). Also what `needs_log` is, and the base of the others.
struct Gain {
    ports: Ports,
    gain: *const f32,
}

impl Gain {
    audio_ports!();
}

impl Plugin for Gain {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        if !self.connect_audio(port, data) && port == 2 {
            self.gain = data.cast();
        }
    }

    fn run(&mut self, frames: usize) {
        let gain = control(self.gain);
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        for (out, sample) in self.ports.output(frames).iter_mut().zip(input) {
            *out = sample * gain;
        }
    }
}

/// `controls`: a copy.
struct Copy {
    ports: Ports,
}

impl Copy {
    audio_ports!();
}

impl Plugin for Copy {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        self.connect_audio(port, data);
    }

    fn run(&mut self, frames: usize) {
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        self.ports.output(frames).copy_from_slice(&input);
    }
}

/// `latency`: a delay of `LATENCY_FRAMES`, and port 2 says so.
struct Latency {
    ports: Ports,
    says: *mut f32,
    memory: Vec<f32>,
}

impl Latency {
    audio_ports!();
}

impl Plugin for Latency {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        if !self.connect_audio(port, data) && port == 2 {
            self.says = data.cast();
        }
    }

    fn run(&mut self, frames: usize) {
        write_control(self.says, LATENCY_FRAMES as f32);
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        let out = self.ports.output(frames);
        for (place, sample) in input.into_iter().enumerate() {
            self.memory.push(sample);
            out[place] = self.memory.remove(0);
        }
    }

    fn activate(&mut self) {
        self.memory = vec![0.0; LATENCY_FRAMES];
    }
}

/// `atoms`: port 2 is an atom input, 3 an atom output, 4 says whether the input was an empty sequence, 5 says the room of the output.
struct Atoms {
    ports: Ports,
    atom_in: *const u8,
    atom_out: *mut u8,
    ok: *mut f32,
    room: *mut f32,
    sequence: u32,
    chunk: u32,
}

impl Atoms {
    audio_ports!();
}

impl Plugin for Atoms {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        if self.connect_audio(port, data) {
            return;
        }
        match port {
            2 => self.atom_in = data.cast(),
            3 => self.atom_out = data.cast(),
            4 => self.ok = data.cast(),
            5 => self.room = data.cast(),
            _ => {}
        }
    }

    fn run(&mut self, frames: usize) {
        let header = |pointer: *const u8| -> Option<(u32, u32)> {
            (!pointer.is_null()).then(|| {
                // SAFETY: the host connected an atom port to a buffer that begins with an `LV2_Atom` (a size and a type, 4 bytes each) and keeps it for the call.
                unsafe { (pointer.cast::<u32>().read(), pointer.cast::<u32>().add(1).read()) }
            })
        };
        let empty_sequence = header(self.atom_in).is_some_and(|(size, type_)| size == 8 && type_ == self.sequence);
        write_control(self.ok, if empty_sequence { 1.0 } else { 0.0 });
        let room = header(self.atom_out)
            .filter(|(_, type_)| *type_ == self.chunk)
            .map_or(0, |(size, _)| size);
        write_control(self.room, room as f32);
        if !self.atom_out.is_null() {
            // What a plugin that has written its output does: the atom is as long as what it wrote (nothing: an empty sequence), so that **the host has to make room again before the next block**.
            // SAFETY: the output buffer begins with an `LV2_Atom` (a size and a type, 4 bytes each) that the plugin may write to, and the host keeps it for the call.
            unsafe {
                self.atom_out.cast::<u32>().write(8);
                self.atom_out.cast::<u32>().add(1).write(self.sequence);
            }
        }
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        self.ports.output(frames).copy_from_slice(&input);
    }
}

/// `options`: ports 2 to 6 say what it was made with.
struct Options {
    ports: Ports,
    says: [*mut f32; 6],
    values: [f32; 6],
}

impl Options {
    audio_ports!();
}

impl Plugin for Options {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        if self.connect_audio(port, data) {
            return;
        }
        if let Some(slot) = (port as usize).checked_sub(2).filter(|slot| *slot < 6) {
            self.says[slot] = data.cast();
        }
    }

    fn run(&mut self, frames: usize) {
        for (slot, value) in self.values.iter().enumerate() {
            write_control(self.says[slot], *value);
        }
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        self.ports.output(frames).copy_from_slice(&input);
    }
}

/// `worker`: out = in * what the work answered (1 until it does).
struct Worker {
    ports: Ports,
    schedule: *const WorkerSchedule,
    sent: bool,
    multiplier: f32,
    handle: Handle,
    /// How many times `end_run` has been called, said on port 3 at each block.
    end_runs: u32,
    says: *mut f32,
    /// How many answers it has been given, said on port 4 at each block.
    answers: u32,
    says_answers: *mut f32,
}

impl Worker {
    audio_ports!();
}

impl Plugin for Worker {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        if self.connect_audio(port, data) {
            return;
        }
        match port {
            3 => self.says = data.cast(),
            4 => self.says_answers = data.cast(),
            _ => {}
        }
    }

    fn run(&mut self, frames: usize) {
        write_control(self.says, self.end_runs as f32);
        write_control(self.says_answers, self.answers as f32);
        if !self.sent && !self.schedule.is_null() {
            self.sent = true;
            let request = 1.0_f32.to_le_bytes();
            // SAFETY: the host gave the schedule feature, a table that it keeps alive for as long as the plugin; the data is 4 bytes that live for the call.
            unsafe {
                if let Some(schedule_work) = (*self.schedule).schedule_work {
                    schedule_work((*self.schedule).handle, 4, request.as_ptr().cast());
                }
            }
        }
        let multiplier = self.multiplier;
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        for (out, sample) in self.ports.output(frames).iter_mut().zip(input) {
            *out = sample * multiplier;
        }
        let _ = self.handle;
    }
}

/// `state`: out = in * boost (port 3 is a request to change it, if it is not 0; port 2 is a spare control).
struct StateGain {
    ports: Ports,
    request: *const f32,
    boost: f32,
    name: String,
}

impl StateGain {
    audio_ports!();
}

impl Plugin for StateGain {
    fn connect(&mut self, port: u32, data: *mut c_void) {
        if !self.connect_audio(port, data) && port == 3 {
            self.request = data.cast();
        }
    }

    fn run(&mut self, frames: usize) {
        let request = control(self.request);
        if request != 0.0 {
            self.boost = request;
            self.name = format!("boost {request}");
        }
        let boost = self.boost;
        let input: Vec<f32> = self.ports.input(frames).to_vec();
        for (out, sample) in self.ports.output(frames).iter_mut().zip(input) {
            *out = sample * boost;
        }
    }
}

const KIND_GAIN: usize = 0;
const KIND_CONTROLS: usize = 1;
const KIND_LATENCY: usize = 2;
const KIND_ATOMS: usize = 3;
const KIND_OPTIONS: usize = 4;
const KIND_WORKER: usize = 5;
const KIND_NEEDS_LOG: usize = 6;
const KIND_STATE: usize = 7;
const KIND_FAILS: usize = 8;

/// Makes the plugin of a kind, from what the host gave.
fn make(kind: usize, given: &Given<'_>) -> Option<Box<dyn Plugin>> {
    Some(match kind {
        KIND_GAIN | KIND_NEEDS_LOG => Box::new(Gain {
            ports: Ports::new(),
            gain: std::ptr::null(),
        }),
        KIND_CONTROLS => Box::new(Copy { ports: Ports::new() }),
        KIND_LATENCY => Box::new(Latency {
            ports: Ports::new(),
            says: std::ptr::null_mut(),
            memory: vec![0.0; LATENCY_FRAMES],
        }),
        KIND_ATOMS => Box::new(Atoms {
            ports: Ports::new(),
            atom_in: std::ptr::null(),
            atom_out: std::ptr::null_mut(),
            ok: std::ptr::null_mut(),
            room: std::ptr::null_mut(),
            sequence: given.map(ATOM_SEQUENCE),
            chunk: given.map(ATOM_CHUNK),
        }),
        KIND_OPTIONS => {
            let mut values = [0.0_f32; 6];
            let keys = [
                given.map(PARAM_SAMPLE_RATE),
                given.map(MIN_BLOCK),
                given.map(MAX_BLOCK),
                given.map(NOMINAL_BLOCK),
            ];
            if let Some(options) = given.feature(OPTIONS_FEATURE).filter(|options| !options.is_null()) {
                let mut option = options.cast::<OptionEntry>().cast_const();
                loop {
                    // SAFETY: the options feature points to an array of options that ends with one whose key is 0, kept by the host for as long as the plugin.
                    let entry = unsafe { &*option };
                    if entry.key == 0 {
                        break;
                    }
                    // SAFETY: an option whose type is a float or an int, as these are, has a value that is a pointer to 4 bytes that the host keeps.
                    let number = unsafe {
                        if entry.key == keys[0] {
                            entry.value.cast::<f32>().read()
                        } else {
                            entry.value.cast::<i32>().read() as f32
                        }
                    };
                    if let Some(slot) = keys.iter().position(|key| *key == entry.key) {
                        values[slot] = number;
                    }
                    // SAFETY: still inside the array: the next entry is there until the one with key 0.
                    option = unsafe { option.add(1) };
                }
            }
            values[4] = if given.feature(BOUNDED).is_some() { 1.0 } else { 0.0 };
            values[5] = if given.bundle_ends_with_a_separator { 1.0 } else { 0.0 };
            Box::new(Options {
                ports: Ports::new(),
                says: [std::ptr::null_mut(); 6],
                values,
            })
        }
        KIND_WORKER => Box::new(Worker {
            ports: Ports::new(),
            schedule: given
                .feature(WORKER_SCHEDULE)
                .map_or(std::ptr::null(), |data| data.cast_const().cast()),
            sent: false,
            multiplier: 1.0,
            handle: std::ptr::null_mut(),
            end_runs: 0,
            says: std::ptr::null_mut(),
            answers: 0,
            says_answers: std::ptr::null_mut(),
        }),
        KIND_STATE => Box::new(StateGain {
            ports: Ports::new(),
            request: std::ptr::null(),
            boost: 1.0,
            name: "default".to_owned(),
        }),
        _ => return None,
    })
}

/// What the host's `instantiate` call gives: the plugin in a box (of a box, so that the pointer is thin), as the handle.
struct Instance {
    plugin: Box<dyn Plugin>,
    kind: usize,
    /// The map, kept for the keys of the state.
    map: *const UridMap,
    /// The unmap, kept to check that it gives back what the map was given.
    unmap: *const UridUnmap,
}

unsafe extern "C" fn instantiate<const KIND: usize>(
    _descriptor: *const Descriptor,
    sample_rate: f64,
    bundle_path: *const c_char,
    features: *const *const Feature,
) -> Handle {
    // SAFETY: the host gives the bundle's path as a C string that is good for the call.
    let bundle_ends_with_a_separator = !bundle_path.is_null()
        && unsafe { CStr::from_ptr(bundle_path) }
            .to_str()
            .is_ok_and(|path| path.ends_with('/') || path.ends_with('\\'));
    let mut list = Vec::new();
    if !features.is_null() {
        let mut place = features;
        loop {
            // SAFETY: the features are a list that ends with a null pointer, kept by the host for as long as the plugin.
            let feature = unsafe { *place };
            if feature.is_null() {
                break;
            }
            // SAFETY: a feature is a URI (a C string) and its data, kept by the host.
            let (uri, data) = unsafe { (CStr::from_ptr((*feature).uri).to_str().unwrap_or(""), (*feature).data) };
            list.push((uri, data));
            // SAFETY: still inside the list: the next is there until the null.
            place = unsafe { place.add(1) };
        }
    }
    let given = Given {
        sample_rate,
        bundle_ends_with_a_separator,
        features: list,
    };
    let _ = given.sample_rate;
    let Some(plugin) = make(KIND, &given) else {
        return std::ptr::null_mut();
    };
    let map = given
        .feature(URID_MAP)
        .map_or(std::ptr::null(), |data| data.cast_const().cast());
    let unmap = given
        .feature(URID_UNMAP)
        .map_or(std::ptr::null(), |data| data.cast_const().cast());
    Box::into_raw(Box::new(Instance {
        plugin,
        kind: KIND,
        map,
        unmap,
    }))
    .cast()
}

unsafe extern "C" fn connect_port(handle: Handle, port: u32, data: *mut c_void) {
    // SAFETY: the handle is the pointer that `instantiate` gave, and is not used after `cleanup`; the host does not call two functions on one instance at the same time.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    instance.plugin.connect(port, data);
}

unsafe extern "C" fn activate(handle: Handle) {
    // SAFETY: as for `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    instance.plugin.activate();
}

unsafe extern "C" fn run(handle: Handle, frames: u32) {
    // SAFETY: as for `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    instance.plugin.run(frames as usize);
}

unsafe extern "C" fn cleanup(handle: Handle) {
    // SAFETY: the handle was made by `Box::into_raw` in `instantiate` and `cleanup` is called once.
    drop(unsafe { Box::from_raw(handle.cast::<Instance>()) });
}

// -- the worker's interface --

unsafe extern "C" fn work(
    _handle: Handle,
    respond: Option<Respond>,
    respond_handle: *mut c_void,
    size: u32,
    data: *const c_void,
) -> i32 {
    if size != 4 || data.is_null() {
        return 1;
    }
    // SAFETY: the host gives `size` bytes that live for the call.
    let request = f32::from_le_bytes(
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), 4) }
            .try_into()
            .unwrap_or([0; 4]),
    );
    let answer = (request * 2.0).to_le_bytes();
    if let Some(respond) = respond {
        // SAFETY: the respond function and its handle are the host's, for the length of this call; the answer is 4 bytes that live for it.
        unsafe { respond(respond_handle, 4, answer.as_ptr().cast()) };
    }
    0
}

unsafe extern "C" fn work_response(handle: Handle, size: u32, body: *const c_void) -> i32 {
    if size != 4 || body.is_null() {
        return 1;
    }
    // SAFETY: the host gives `size` bytes that live for the call; the handle is as for `connect_port`.
    let (answer, instance) = unsafe {
        (
            f32::from_le_bytes(
                std::slice::from_raw_parts(body.cast::<u8>(), 4)
                    .try_into()
                    .unwrap_or([0; 4]),
            ),
            &mut *handle.cast::<Instance>(),
        )
    };
    // (Only the worker plugin has a worker interface, and it is one that has a multiplier: the plugin is made of a `Worker` whose place is at the start of its box.)
    if instance.kind == KIND_WORKER {
        // SAFETY: a `Worker` is what the box holds for this kind: the trait object's data pointer is the `Worker`.
        let worker = unsafe { &mut *std::ptr::from_mut(&mut *instance.plugin).cast::<Worker>() };
        worker.multiplier = answer;
        worker.answers += 1;
    }
    0
}

unsafe extern "C" fn end_run(handle: Handle) -> i32 {
    // SAFETY: as for `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    if instance.kind == KIND_WORKER {
        // SAFETY: a `Worker` is what the box holds for this kind.
        let worker = unsafe { &mut *std::ptr::from_mut(&mut *instance.plugin).cast::<Worker>() };
        worker.end_runs += 1;
    }
    0
}

static WORKER_INTERFACE_TABLE: WorkerInterface = WorkerInterface {
    work: Some(work),
    work_response: Some(work_response),
    end_run: Some(end_run),
};

// -- the state's interface --

unsafe extern "C" fn save(
    handle: Handle,
    store: Option<Store>,
    store_handle: *mut c_void,
    _flags: u32,
    _features: *const *const Feature,
) -> i32 {
    // SAFETY: as for `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    let Some(store) = store else { return 1 };
    if instance.kind != KIND_STATE || instance.map.is_null() {
        return 1;
    }
    // SAFETY: a `StateGain` is what the box holds for this kind.
    let state = unsafe { &*std::ptr::from_ref(&*instance.plugin).cast::<StateGain>() };
    // (A plugin that cannot save its state, as one whose number is not one it can keep: this one cannot when its boost is below zero.)
    if state.boost < 0.0 {
        return 1;
    }
    let float = map_with(instance.map, ATOM_FLOAT);
    let string = map_with(instance.map, ATOM_STRING);
    // The unmap gives back the URI that the map was given the number of (the host gives both, and a plugin may use either).
    if instance.unmap.is_null() {
        return 1;
    }
    // SAFETY: the host gave the unmap feature, a table that it keeps alive for as long as the plugin; the string it gives back is a C string that it keeps.
    let unmapped = unsafe {
        (*instance.unmap)
            .unmap
            .map(|function| function((*instance.unmap).handle, float))
            .filter(|pointer| !pointer.is_null())
            .map(|pointer| CStr::from_ptr(pointer).to_string_lossy().into_owned())
    };
    if unmapped.as_deref() != Some(ATOM_FLOAT) {
        return 1;
    }
    let boost = state.boost.to_le_bytes();
    let name = format!("{}\0", state.name);
    // SAFETY: calling the host's store function with its own handle, the key and type numbers it made, and bytes that live for the call.
    unsafe {
        store(
            store_handle,
            map_with(instance.map, STATE_BOOST_KEY),
            boost.as_ptr().cast(),
            4,
            float,
            3,
        );
        store(
            store_handle,
            map_with(instance.map, STATE_NAME_KEY),
            name.as_ptr().cast(),
            name.len(),
            string,
            3,
        );
    }
    0
}

unsafe extern "C" fn restore(
    handle: Handle,
    retrieve: Option<Retrieve>,
    retrieve_handle: *mut c_void,
    _flags: u32,
    _features: *const *const Feature,
) -> i32 {
    // SAFETY: as for `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    let Some(retrieve) = retrieve else { return 1 };
    if instance.kind != KIND_STATE || instance.map.is_null() {
        return 1;
    }
    // SAFETY: a `StateGain` is what the box holds for this kind.
    let state = unsafe { &mut *std::ptr::from_mut(&mut *instance.plugin).cast::<StateGain>() };
    let (mut size, mut type_, mut flags) = (0_usize, 0_u32, 0_u32);
    // SAFETY: calling the host's retrieve function with its own handle, a key number from its map, and places to write the size, type and flags to; what it gives is a pointer to
    // `size` bytes that are good for the length of this call.
    unsafe {
        let boost = retrieve(
            retrieve_handle,
            map_with(instance.map, STATE_BOOST_KEY),
            &mut size,
            &mut type_,
            &mut flags,
        );
        // (A property that is not there, or that is not a float of 4 bytes with the flags that were stored, is one that this plugin does not take: it says so, as a plugin does.)
        if boost.is_null() {
            return 5;
        }
        if size == 4 && type_ == map_with(instance.map, ATOM_FLOAT) && flags == 3 {
            state.boost = f32::from_le_bytes(
                std::slice::from_raw_parts(boost.cast::<u8>(), 4)
                    .try_into()
                    .unwrap_or([0; 4]),
            );
        }
        let name = retrieve(
            retrieve_handle,
            map_with(instance.map, STATE_NAME_KEY),
            &mut size,
            &mut type_,
            &mut flags,
        );
        if !name.is_null() && size > 0 {
            let bytes = std::slice::from_raw_parts(name.cast::<u8>(), size);
            state.name = String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or(&[])).into_owned();
        }
    }
    0
}

static STATE_INTERFACE_TABLE: StateInterface = StateInterface {
    save: Some(save),
    restore: Some(restore),
};

unsafe extern "C" fn extension_data<const KIND: usize>(uri: *const c_char) -> *const c_void {
    if uri.is_null() {
        return std::ptr::null();
    }
    // SAFETY: the host gives a C string.
    let uri = unsafe { CStr::from_ptr(uri) }.to_str().unwrap_or("");
    match (KIND, uri) {
        (KIND_WORKER, WORKER_INTERFACE) => std::ptr::from_ref(&WORKER_INTERFACE_TABLE).cast(),
        (KIND_STATE, STATE_INTERFACE) => std::ptr::from_ref(&STATE_INTERFACE_TABLE).cast(),
        _ => std::ptr::null(),
    }
}

macro_rules! descriptor {
    ($uri:expr, $kind:expr) => {
        Descriptor {
            uri: concat!($uri, "\0").as_ptr().cast(),
            instantiate: Some(instantiate::<{ $kind }>),
            connect_port: Some(connect_port),
            activate: Some(activate),
            run: Some(run),
            deactivate: None,
            cleanup: Some(cleanup),
            extension_data: Some(extension_data::<{ $kind }>),
        }
    };
}

static DESCRIPTORS: [Descriptor; 9] = [
    descriptor!("urn:fastforwardrs:test:gain", KIND_GAIN),
    descriptor!("urn:fastforwardrs:test:controls", KIND_CONTROLS),
    descriptor!("urn:fastforwardrs:test:latency", KIND_LATENCY),
    descriptor!("urn:fastforwardrs:test:atoms", KIND_ATOMS),
    descriptor!("urn:fastforwardrs:test:options", KIND_OPTIONS),
    descriptor!("urn:fastforwardrs:test:worker", KIND_WORKER),
    descriptor!("urn:fastforwardrs:test:needs_log", KIND_NEEDS_LOG),
    descriptor!("urn:fastforwardrs:test:state", KIND_STATE),
    descriptor!("urn:fastforwardrs:test:fails", KIND_FAILS),
];

/// The function that an LV2 library exports.
#[unsafe(no_mangle)]
pub extern "C" fn lv2_descriptor(index: u32) -> *const Descriptor {
    DESCRIPTORS
        .get(index as usize)
        .map_or(std::ptr::null(), std::ptr::from_ref)
}

// ---- the bundle ----

const PREFIXES: &str = r#"@prefix atom: <http://lv2plug.in/ns/ext/atom#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix midi: <http://lv2plug.in/ns/ext/midi#> .
@prefix pprop: <http://lv2plug.in/ns/ext/port-props#> .
@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix state: <http://lv2plug.in/ns/ext/state#> .
@prefix unit: <http://lv2plug.in/ns/extensions/units#> .
@prefix urid: <http://lv2plug.in/ns/ext/urid#> .
@prefix work: <http://lv2plug.in/ns/ext/worker#> .
@prefix opts: <http://lv2plug.in/ns/ext/options#> .
@prefix bufsz: <http://lv2plug.in/ns/ext/buf-size#> .
@prefix log:  <http://lv2plug.in/ns/ext/log#> .
"#;

fn audio_pair() -> &'static str {
    r#"    lv2:port [
        a lv2:InputPort , lv2:AudioPort ;
        lv2:index 0 ; lv2:symbol "in" ; lv2:name "In" ;
    ] , [
        a lv2:OutputPort , lv2:AudioPort ;
        lv2:index 1 ; lv2:symbol "out" ; lv2:name "Out" ;
    ]"#
}

/// The file name and the text of the Turtle of each of the plugins.
fn description_of(uri: &str, file: &str, name: &str, features: &str, ports: &str) -> (String, String) {
    (
        file.to_owned(),
        format!(
            "{PREFIXES}\n<{uri}>\n    a lv2:Plugin , lv2:UtilityPlugin ;\n    doap:name \"{name}\" ;\n    doap:maintainer [ foaf:name \"fastforwardrs tests\" ] ;\n    lv2:minorVersion 1 ;\n    lv2:microVersion 0 ;\n{features}{pair} ,\n{ports} .\n",
            pair = audio_pair()
        ),
    )
}

/// The Turtle of the bundle of the test plugins, for a library file of this name: `manifest.ttl` and a file for each plugin, as (file name, text).
pub fn bundle_files(library_file_name: &str) -> Vec<(String, String)> {
    let plugins: Vec<(String, String)> = vec![
        description_of(
            GAIN,
            "gain.ttl",
            "Test gain",
            "    lv2:requiredFeature urid:map ;\n",
            r#"    [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "gain" ; lv2:name "Gain" ;
        lv2:default 1.0 ; lv2:minimum 0.0 ; lv2:maximum 4.0 ;
    ]"#,
        ),
        description_of(
            CONTROLS,
            "controls.ttl",
            "Test controls",
            "",
            r#"    [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "bypass" ; lv2:name "Bypass" ;
        lv2:default 0 ; lv2:minimum 0 ; lv2:maximum 1 ; lv2:portProperty lv2:toggled ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 3 ; lv2:symbol "steps" ; lv2:name "Steps" ;
        lv2:default 4 ; lv2:minimum 1 ; lv2:maximum 16 ; lv2:portProperty lv2:integer ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 4 ; lv2:symbol "mode" ; lv2:name "Mode" ;
        lv2:default 1 ; lv2:minimum 0 ; lv2:maximum 2 ; lv2:portProperty lv2:integer , lv2:enumeration ;
        lv2:scalePoint [ rdfs:label "Clean" ; rdf:value 0 ] , [ rdfs:label "Warm" ; rdf:value 1 ] , [ rdfs:label "Hot" ; rdf:value 2 ] ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 5 ; lv2:symbol "cutoff" ; lv2:name "Cutoff" ;
        lv2:default 1000.0 ; lv2:minimum 20.0 ; lv2:maximum 20000.0 ; lv2:portProperty pprop:logarithmic ; unit:unit unit:hz ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 6 ; lv2:symbol "rate" ; lv2:name "Rate" ;
        lv2:default 0.25 ; lv2:minimum 0.0 ; lv2:maximum 0.5 ; lv2:portProperty lv2:sampleRate ;
    ]"#,
        ),
        description_of(
            LATENCY,
            "latency.ttl",
            "Test latency",
            "",
            r#"    [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "latency" ; lv2:name "Latency" ; lv2:designation lv2:latency ; lv2:portProperty lv2:integer ;
    ]"#,
        ),
        description_of(
            ATOMS,
            "atoms.ttl",
            "Test atoms",
            "    lv2:requiredFeature urid:map ;\n",
            r#"    [
        a lv2:InputPort , atom:AtomPort ;
        atom:bufferType atom:Sequence ; atom:supports midi:MidiEvent ;
        lv2:index 2 ; lv2:symbol "events_in" ; lv2:name "Events in" ;
    ] , [
        a lv2:OutputPort , atom:AtomPort ;
        atom:bufferType atom:Sequence ;
        lv2:index 3 ; lv2:symbol "events_out" ; lv2:name "Events out" ;
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 4 ; lv2:symbol "was_empty_sequence" ; lv2:name "Was empty sequence" ;
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 5 ; lv2:symbol "room" ; lv2:name "Room" ;
    ]"#,
        ),
        description_of(
            OPTIONS,
            "options.ttl",
            "Test options",
            "    lv2:requiredFeature urid:map ;\n    lv2:optionalFeature opts:options ;\n",
            r#"    [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 2 ; lv2:symbol "sample_rate" ; lv2:name "Sample rate" ; ] ,
    [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 3 ; lv2:symbol "min_block" ; lv2:name "Min block" ; ] ,
    [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 4 ; lv2:symbol "max_block" ; lv2:name "Max block" ; ] ,
    [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 5 ; lv2:symbol "nominal_block" ; lv2:name "Nominal block" ; ] ,
    [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 6 ; lv2:symbol "bounded" ; lv2:name "Bounded" ; ] ,
    [ a lv2:OutputPort , lv2:ControlPort ; lv2:index 7 ; lv2:symbol "bundle_path_ends_with_a_separator" ; lv2:name "Bundle path ends with a separator" ; ]"#,
        ),
        description_of(
            WORKER,
            "worker.ttl",
            "Test worker",
            "    lv2:requiredFeature urid:map , work:schedule ;\n    lv2:extensionData work:interface ;\n",
            r#"    [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "spare" ; lv2:name "Spare" ; lv2:default 0.0 ; lv2:minimum 0.0 ; lv2:maximum 1.0 ;
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 3 ; lv2:symbol "end_runs" ; lv2:name "Ends of blocks" ;
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 4 ; lv2:symbol "answers" ; lv2:name "Answers" ;
    ]"#,
        ),
        description_of(
            NEEDS_LOG,
            "needs_log.ttl",
            "Test needs log",
            "    lv2:requiredFeature urid:map , log:log ;\n",
            r#"    [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "gain" ; lv2:name "Gain" ; lv2:default 1.0 ; lv2:minimum 0.0 ; lv2:maximum 4.0 ;
    ]"#,
        ),
        description_of(
            STATE,
            "state.ttl",
            "Test state",
            "    lv2:requiredFeature urid:map ;\n    lv2:extensionData state:interface ;\n",
            r#"    [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "spare" ; lv2:name "Spare" ; lv2:default 0.0 ; lv2:minimum 0.0 ; lv2:maximum 1.0 ;
    ] , [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 3 ; lv2:symbol "boost_request" ; lv2:name "Boost request" ; lv2:default 0.0 ; lv2:minimum 0.0 ; lv2:maximum 8.0 ;
    ]"#,
        ),
        description_of(
            FAILS,
            "fails.ttl",
            "Test fails",
            "",
            r#"    [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 2 ; lv2:symbol "spare" ; lv2:name "Spare" ; lv2:default 0.0 ; lv2:minimum 0.0 ; lv2:maximum 1.0 ;
    ]"#,
        ),
    ];
    let uris = [GAIN, CONTROLS, LATENCY, ATOMS, OPTIONS, WORKER, NEEDS_LOG, STATE, FAILS];
    let mut manifest = String::from(PREFIXES);
    for ((file, _), uri) in plugins.iter().zip(uris) {
        manifest.push_str(&format!(
            "\n<{uri}>\n    a lv2:Plugin ;\n    lv2:binary <{library_file_name}> ;\n    rdfs:seeAlso <{file}> .\n"
        ));
    }
    let mut files = vec![("manifest.ttl".to_owned(), manifest)];
    files.extend(plugins);
    files
}

/// Where the file of this library is, **for a test that was built by the same `cargo` run as this crate's library** and depends on this crate: `cargo` builds the library
/// beside the program, in the same folder. (Nothing is built by this function.)
pub fn library_file_for_tests() -> PathBuf {
    let program = std::env::current_exe().expect("a program knows where it is");
    let name = format!(
        "{}lv2_test_plugins{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let file = program.parent().expect("a program is in a folder").join(name);
    assert!(
        file.is_file(),
        "the library of the test plugins is not at {} (the program that looks for it has to have the crate lv2-test-plugins as a dependency)",
        file.display()
    );
    file
}

/// Makes the bundle of the test plugins in `folder`: a folder `test.lv2` with the library copied into it and the Turtle written, and gives it.
pub fn write_bundle(folder: &Path) -> PathBuf {
    let bundle = folder.join("test.lv2");
    std::fs::create_dir_all(&bundle).expect("the bundle's folder is made");
    let library = library_file_for_tests();
    let name = library
        .file_name()
        .expect("a library has a name")
        .to_string_lossy()
        .into_owned();
    std::fs::copy(&library, bundle.join(&name)).expect("the library is copied");
    for (file, text) in bundle_files(&name) {
        std::fs::write(bundle.join(file), text).expect("the Turtle is written");
    }
    bundle
}
