// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! An LV2 plugin that has been made from a library, and can be given values and sound.
//!
//! **Every port is connected to something that is the host's own** from the moment the plugin is made: a control port to a number in a list the instance keeps, an audio port to
//! a buffer of silence (an input) or of nowhere (an output) until the block's own buffers are connected, an atom port to a buffer that holds an empty sequence (an input) or has
//! room (an output). So a plugin that has ports that the program has no use for runs all the same.

#![allow(unsafe_code)]

use std::ffi::{CString, c_void};
use std::fmt;
use std::sync::Arc;

use thiserror::Error;

use crate::abi::{self, Handle, RawAtomSequenceHeader, RawDescriptor, RawStateInterface, RawWorkerInterface};
use crate::description::{PluginDescription, PortDirection, PortKind};
use crate::features::{FeatureSet, UridTable};
use crate::library::{Library, Loaded};
use crate::worker::{Worker, WorkerShared};

/// How many bytes an atom port's buffer has. (A plugin that says it needs more, with `rsz:minimumSize`, is given that.)
pub(crate) const ATOM_BUFFER_BYTES: usize = 64 * 1024;

/// Why a plugin could not be made.
#[derive(Debug, Error)]
pub enum InstantiateError {
    /// It requires something that this program does not give a plugin; the URIs are said.
    #[error("the plugin requires {0}, which this program does not give a plugin")]
    MissingFeatures(String),
    #[error("the library has no plugin called {0}")]
    NoSuchPlugin(String),
    #[error("the plugin does not have the function {0}, which every plugin has to")]
    MissingFunction(&'static str),
    #[error("the plugin could not be made at {0} Hz: its instantiate gave nothing")]
    InstantiateFailed(u32),
    #[error("{0}")]
    CannotUse(String),
}

/// Why a block could not be run.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RunError {
    #[error("a block of {frames} frames is more than the {most} that the plugin was made for")]
    BlockTooBig { frames: usize, most: usize },
    #[error("a buffer has {has} frames and the block is {block}")]
    BufferTooShort { has: usize, block: usize },
}

/// The buffer of an atom port: words, so that it is aligned to 8 bytes as atoms must be.
struct AtomBuffer {
    port: u32,
    direction: PortDirection,
    words: Box<[u64]>,
}

/// An LV2 plugin that is made and active.
pub struct Instance {
    pub(crate) description: PluginDescription,
    pub(crate) descriptor: *const RawDescriptor,
    pub(crate) handle: Handle,
    pub(crate) features: FeatureSet,
    pub(crate) table: Arc<UridTable>,
    worker: Option<Worker>,
    worker_interface: *const RawWorkerInterface,
    pub(crate) state_interface: *const RawStateInterface,
    controls: Box<[f32]>,
    audio_inputs: Vec<u32>,
    audio_outputs: Vec<u32>,
    silent: Box<[f32]>,
    sink: Box<[f32]>,
    atoms: Vec<AtomBuffer>,
    latency_port: Option<u32>,
    latency: u32,
    sample_rate: f64,
    most_frames: usize,
    active: bool,
    /// (Last: the plugin is only good while the library is loaded, and fields are dropped in order, after `drop` has cleaned the plugin up.)
    _loaded: Arc<Loaded>,
}

// SAFETY: an LV2 plugin may be used from any one thread at a time (the standard does not tie an instance to a thread; `run` and `save`/`restore` are not to be called at the same
// time on one instance, which `&mut self` makes true), and everything else here is data that is owned by the instance.
unsafe impl Send for Instance {}

impl fmt::Debug for Instance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Instance")
            .field("plugin", &self.description.uri)
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

impl Instance {
    /// Makes the plugin `description` from `library`, at `sample_rate`, for blocks of at most `most_frames` frames, and activates it.
    pub fn new(
        library: &Library,
        description: &PluginDescription,
        sample_rate: f64,
        most_frames: u32,
    ) -> Result<Self, InstantiateError> {
        let missing: Vec<&str> = description
            .required_features
            .iter()
            .map(String::as_str)
            .filter(|feature| !abi::SUPPORTED_FEATURES.contains(feature))
            .collect();
        if !missing.is_empty() {
            return Err(InstantiateError::MissingFeatures(missing.join(", ")));
        }
        let descriptor = library
            .loaded
            .descriptor_of(&description.uri)
            .ok_or_else(|| InstantiateError::NoSuchPlugin(description.uri.clone()))?;
        // SAFETY: the descriptor is good for as long as the library is loaded, which `library` keeps true for the length of this call and the instance keeps true afterwards.
        let (instantiate, connect_port, run) =
            unsafe { ((*descriptor).instantiate, (*descriptor).connect_port, (*descriptor).run) };
        let instantiate = instantiate.ok_or(InstantiateError::MissingFunction("instantiate"))?;
        if connect_port.is_none() {
            return Err(InstantiateError::MissingFunction("connect_port"));
        }
        if run.is_none() {
            return Err(InstantiateError::MissingFunction("run"));
        }

        let table = Arc::new(UridTable::default());
        let wanted: Vec<String> = description
            .required_features
            .iter()
            .chain(&description.optional_features)
            .cloned()
            .collect();
        let wants_a_worker = wanted.iter().any(|feature| feature == abi::WORKER_SCHEDULE);
        let (worker_shared, worker_receiver) = if wants_a_worker {
            let (shared, receiver) = WorkerShared::new();
            (Some(shared), Some(receiver))
        } else {
            (None, None)
        };
        let features = FeatureSet::new(&table, &wanted, sample_rate, most_frames, worker_shared.as_ref());

        // The bundle's folder, with the separator at the end that the standard says the path has.
        let mut bundle = description.bundle.to_string_lossy().into_owned();
        if !bundle.ends_with(std::path::MAIN_SEPARATOR) && !bundle.ends_with('/') {
            bundle.push(std::path::MAIN_SEPARATOR);
        }
        let bundle = CString::new(bundle)
            .map_err(|_| InstantiateError::CannotUse("the bundle's path has a zero byte in it".to_owned()))?;

        // SAFETY: calling the plugin's `instantiate` with its own descriptor, a sample rate, the bundle's path (a C string that lives for the call) and the list of features that
        // `features` keeps alive and null-terminated, as `lv2.h` says; the plugin may keep the pointers to the features (so `features` is kept for as long as the instance is).
        let handle = unsafe { instantiate(descriptor, sample_rate, bundle.as_ptr(), features.as_pointer()) };
        if handle.is_null() {
            return Err(InstantiateError::InstantiateFailed(sample_rate as u32));
        }

        let most_frames = most_frames as usize;
        let mut controls = vec![0.0_f32; description.ports.len()].into_boxed_slice();
        let mut audio_inputs = Vec::new();
        let mut audio_outputs = Vec::new();
        let mut atoms = Vec::new();
        let mut latency_port = None;
        for port in &description.ports {
            match port.kind {
                PortKind::Control => {
                    controls[port.index as usize] = default_of(port, sample_rate);
                    if port.reports_latency && port.direction == PortDirection::Output {
                        latency_port = Some(port.index);
                    }
                }
                PortKind::Audio => match port.direction {
                    PortDirection::Input => audio_inputs.push(port.index),
                    PortDirection::Output => audio_outputs.push(port.index),
                },
                PortKind::ControlVoltage => {}
                PortKind::Atom => {
                    let wanted_bytes = port
                        .atom
                        .as_ref()
                        .and_then(|atom| atom.minimum_size)
                        .map_or(ATOM_BUFFER_BYTES, |bytes| (bytes as usize).max(ATOM_BUFFER_BYTES));
                    atoms.push(AtomBuffer {
                        port: port.index,
                        direction: port.direction,
                        words: vec![0_u64; wanted_bytes.div_ceil(8)].into_boxed_slice(),
                    });
                }
            }
        }

        // The extensions of the plugin that the host uses.
        let worker_interface = extension(descriptor, abi::WORKER_INTERFACE).cast::<RawWorkerInterface>();
        let state_interface = extension(descriptor, abi::STATE_INTERFACE).cast::<RawStateInterface>();
        let worker = match (worker_shared, worker_receiver) {
            (Some(shared), Some(receiver)) if !worker_interface.is_null() => {
                // SAFETY: `handle` is the plugin that was just made and `worker_interface` the interface that it gave; the `Worker` is stopped (its thread ended) in `drop`, before
                // the plugin is cleaned up and before the library can be let go of.
                Some(unsafe { Worker::start(shared, receiver, handle, worker_interface) })
            }
            _ => None,
        };

        let mut instance = Self {
            description: description.clone(),
            descriptor,
            handle,
            features,
            table,
            worker,
            worker_interface,
            state_interface,
            controls,
            audio_inputs,
            audio_outputs,
            silent: vec![0.0; most_frames].into_boxed_slice(),
            sink: vec![0.0; most_frames].into_boxed_slice(),
            atoms,
            latency_port,
            latency: 0,
            sample_rate,
            most_frames,
            active: false,
            _loaded: Arc::clone(&library.loaded),
        };
        instance.connect_everything_that_is_the_hosts();
        instance.activate();
        Ok(instance)
    }

    pub fn description(&self) -> &PluginDescription {
        &self.description
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The most frames of a block that this instance was made for.
    pub fn most_frames(&self) -> usize {
        self.most_frames
    }

    /// How many audio inputs and audio outputs the plugin has (CV ports are not counted).
    pub fn audio_port_counts(&self) -> (usize, usize) {
        (self.audio_inputs.len(), self.audio_outputs.len())
    }

    /// How late the plugin's sound is, in frames, as its latency port said after the last block (0 if it has none).
    pub fn latency(&self) -> u32 {
        self.latency
    }

    /// Gives the control input `port` (the plugin's own number for it) this value, to be read at the next block. `false` if there is no such control input.
    pub fn set_control(&mut self, port: u32, value: f32) -> bool {
        let is_a_control_input = self.description.ports.get(port as usize).is_some_and(|description| {
            description.kind == PortKind::Control && description.direction == PortDirection::Input
        });
        if is_a_control_input {
            self.controls[port as usize] = value;
        }
        is_a_control_input
    }

    /// What the control port `port` holds now (an output's value is what the last block made it).
    pub fn control(&self, port: u32) -> Option<f32> {
        let is_a_control = self
            .description
            .ports
            .get(port as usize)
            .is_some_and(|description| description.kind == PortKind::Control);
        is_a_control.then(|| self.controls[port as usize])
    }

    fn connect(&self, port: u32, pointer: *mut c_void) {
        // SAFETY: calling the plugin's `connect_port` (checked present when it was made) with its handle, a port that it described and a pointer to memory that the instance
        // owns and keeps where it is for as long as the plugin is connected to it (the boxes of this struct are never resized; the block's own buffers are connected again at
        // every run and are good for the length of it).
        unsafe {
            if let Some(connect_port) = (*self.descriptor).connect_port {
                connect_port(self.handle, port, pointer);
            }
        }
    }

    fn connect_everything_that_is_the_hosts(&mut self) {
        for port in self.description.ports.clone() {
            let pointer: *mut c_void = match port.kind {
                PortKind::Control => (&raw mut self.controls[port.index as usize]).cast(),
                PortKind::Audio | PortKind::ControlVoltage => match port.direction {
                    PortDirection::Input => self.silent.as_mut_ptr().cast(),
                    PortDirection::Output => self.sink.as_mut_ptr().cast(),
                },
                PortKind::Atom => {
                    let Some(buffer) = self.atoms.iter_mut().find(|buffer| buffer.port == port.index) else {
                        continue;
                    };
                    buffer.words.as_mut_ptr().cast()
                }
            };
            self.connect(port.index, pointer);
        }
    }

    /// Starts the plugin (if it is not started).
    pub fn activate(&mut self) {
        if self.active {
            return;
        }
        // SAFETY: calling the plugin's `activate`, if it has one, with its handle, when it is not active (the standard says it is called once before `run`, and again after a `deactivate`).
        unsafe {
            if let Some(activate) = (*self.descriptor).activate {
                activate(self.handle);
            }
        }
        self.active = true;
    }

    /// Stops the plugin (if it is started).
    pub fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        // SAFETY: calling the plugin's `deactivate`, if it has one, with its handle, when it is active.
        unsafe {
            if let Some(deactivate) = (*self.descriptor).deactivate {
                deactivate(self.handle);
            }
        }
        self.active = false;
    }

    /// Makes the plugin forget what it has heard (it is stopped and started: LV2 has no other way). Its values are kept.
    pub fn reset(&mut self) {
        self.deactivate();
        self.activate();
    }

    /// Makes the sound of one block: `inputs` are the plugin's audio inputs, in order, and `outputs` its audio outputs; an input that is not given is silence and an output that is
    /// not given is thrown away, and a buffer that is not needed is left alone. Every buffer has to have at least `frames` frames.
    pub fn run(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
    ) -> Result<(), RunError> {
        if frames > self.most_frames {
            return Err(RunError::BlockTooBig {
                frames,
                most: self.most_frames,
            });
        }
        for buffer in inputs
            .iter()
            .map(|buffer| buffer.len())
            .chain(outputs.iter().map(|buffer| buffer.len()))
        {
            if buffer < frames {
                return Err(RunError::BufferTooShort {
                    has: buffer,
                    block: frames,
                });
            }
        }
        self.activate();
        self.get_the_atom_buffers_ready();
        self.give_the_plugin_what_its_work_answered();
        for (place, port) in self.audio_inputs.clone().into_iter().enumerate() {
            let pointer: *mut c_void = match inputs.get_mut(place) {
                Some(buffer) => buffer.as_mut_ptr().cast(),
                None => self.silent.as_mut_ptr().cast(),
            };
            self.connect(port, pointer);
        }
        for (place, port) in self.audio_outputs.clone().into_iter().enumerate() {
            let pointer: *mut c_void = match outputs.get_mut(place) {
                Some(buffer) => buffer.as_mut_ptr().cast(),
                None => self.sink.as_mut_ptr().cast(),
            };
            self.connect(port, pointer);
        }
        // SAFETY: calling the plugin's `run` (checked present when it was made) with its handle and a number of frames that no buffer it is connected to is shorter than: its audio
        // ports are connected to the block's buffers just above (each at least `frames` long, checked) or to the instance's own, which are `most_frames` long and `frames` is not
        // more than that.
        unsafe {
            if let Some(run) = (*self.descriptor).run {
                run(self.handle, u32::try_from(frames).unwrap_or(u32::MAX));
            }
        }
        self.tell_the_plugin_the_work_is_done_for_the_block();
        if let Some(port) = self.latency_port {
            let latency = self.controls[port as usize];
            self.latency = if latency.is_finite() && latency > 0.0 {
                latency.round() as u32
            } else {
                0
            };
        }
        Ok(())
    }

    /// An atom input holds an empty sequence, and an atom output has all its room, as the standard says the host leaves them before each `run`.
    fn get_the_atom_buffers_ready(&mut self) {
        let sequence = self.table.map(abi::ATOM_SEQUENCE);
        let chunk = self.table.map(abi::ATOM_CHUNK);
        for buffer in &mut self.atoms {
            let bytes = buffer.words.len() * 8;
            let header = match buffer.direction {
                // (An empty sequence: the atom's size is the body's header alone, 8 bytes.)
                PortDirection::Input => RawAtomSequenceHeader {
                    size: 8,
                    type_: sequence,
                    unit: 0,
                    pad: 0,
                },
                // (A chunk with all the room: its size is what is left after its own 8 bytes.)
                PortDirection::Output => RawAtomSequenceHeader {
                    size: u32::try_from(bytes - 8).unwrap_or(u32::MAX),
                    type_: chunk,
                    unit: 0,
                    pad: 0,
                },
            };
            // SAFETY: the buffer is at least 64 KiB, so it has room for the header's 16 bytes, and it is made of 64-bit words, so it is aligned to more than the header's 4-byte
            // fields need.
            unsafe {
                buffer.words.as_mut_ptr().cast::<RawAtomSequenceHeader>().write(header);
            }
        }
    }

    /// What the work that the plugin scheduled has answered is given to it, before the block.
    fn give_the_plugin_what_its_work_answered(&mut self) {
        let (Some(worker), false) = (&self.worker, self.worker_interface.is_null()) else {
            return;
        };
        for answer in worker.shared().take_responses() {
            // SAFETY: calling the plugin's `work_response`, if it has one, with its handle and the bytes of an answer that live for the call; the interface is the plugin's own and
            // lives as long as the library.
            unsafe {
                if let Some(work_response) = (*self.worker_interface).work_response {
                    work_response(
                        self.handle,
                        u32::try_from(answer.len()).unwrap_or(u32::MAX),
                        answer.as_ptr().cast(),
                    );
                }
            }
        }
    }

    /// `end_run`, after the block, if the plugin has a worker.
    fn tell_the_plugin_the_work_is_done_for_the_block(&mut self) {
        if self.worker.is_none() || self.worker_interface.is_null() {
            return;
        }
        // SAFETY: calling the plugin's `end_run`, if it has one, with its handle; the interface is the plugin's own and lives as long as the library.
        unsafe {
            if let Some(end_run) = (*self.worker_interface).end_run {
                end_run(self.handle);
            }
        }
    }
}

/// What `extension_data` says for `uri`: a pointer that is good for as long as the library is loaded, or null.
fn extension(descriptor: *const RawDescriptor, uri: &str) -> *const c_void {
    let Ok(uri) = CString::new(uri) else {
        return std::ptr::null();
    };
    // SAFETY: calling the library's `extension_data`, if it has one, with a C string that lives for the call; what it answers is a pointer to a table that the plugin keeps for as
    // long as the library is loaded (`lv2.h`).
    unsafe {
        match (*descriptor).extension_data {
            Some(extension_data) => extension_data(uri.as_ptr()),
            None => std::ptr::null(),
        }
    }
}

/// What a control port is when it is made: its default if it says one (and the sample rate times it if that is how it is meant), otherwise its minimum, otherwise 0.
fn default_of(port: &crate::description::PortDescription, sample_rate: f64) -> f32 {
    let scale = if port.sample_rate_relative { sample_rate } else { 1.0 };
    let mut value = port.default.or(port.minimum).unwrap_or(0.0) * scale;
    if let Some(minimum) = port.minimum {
        value = value.max(minimum * scale);
    }
    if let Some(maximum) = port.maximum {
        value = value.min(maximum * scale);
    }
    value as f32
}

impl Drop for Instance {
    fn drop(&mut self) {
        // The worker's thread ends first: it calls the plugin, which is not to happen once the plugin is cleaned up.
        if let Some(worker) = &mut self.worker {
            worker.stop();
        }
        self.deactivate();
        // SAFETY: calling the plugin's `cleanup`, if it has one, with its handle, once: after this the handle is not used (the library is let go of after, as the last field).
        unsafe {
            if let Some(cleanup) = (*self.descriptor).cleanup {
                cleanup(self.handle);
            }
        }
    }
}
