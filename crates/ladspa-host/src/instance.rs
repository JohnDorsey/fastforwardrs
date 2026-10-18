// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin that has been made (`instantiate`) and can be given values and sound. Its interface is safe: the ports are connected to buffers that this owns or that the caller
//! lends for the length of one call, so that a plugin can never be left with a pointer to memory that has gone.

#![allow(unsafe_code)]

use std::ffi::c_ulong;
use std::fmt;
use std::sync::Arc;

use thiserror::Error;

use crate::abi::{Handle, RawDescriptor};
use crate::description::{PluginDescription, PortDirection, PortKind};
use crate::library::{Loaded, read_descriptor};

/// How many moments of silence the audio ports are connected to until sound is lent to them (a plugin should not look at its audio in `activate`, and if one does, there is
/// memory there that it may).
const MOMENTS_OF_SILENCE: usize = 8192;

/// Why a plugin could not be made.
#[derive(Debug, Error)]
pub enum InstantiateError {
    #[error("the library has no plugin number {0}")]
    NoSuchPlugin(usize),
    #[error("what the plugin says of itself cannot be used: {0}")]
    BadDescription(String),
    #[error("the plugin does not have the function {0}, which every plugin has to")]
    MissingFunction(&'static str),
    #[error("the plugin could not be made at {0} Hz: its instantiate gave nothing")]
    GaveNothing(u32),
}

/// Why a value could not be given to a port.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ControlError {
    #[error("the plugin has no port number {0}")]
    NoSuchPort(usize),
    #[error("port number {0} is not a control that takes a value")]
    NotAControlInput(usize),
    #[error("a value has to be a number")]
    NotANumber,
}

/// Why a block could not be run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunError {
    #[error("the plugin has {expected} audio inputs and {given} were given")]
    WrongNumberOfInputs { expected: usize, given: usize },
    #[error("the plugin has {expected} audio outputs and {given} were given")]
    WrongNumberOfOutputs { expected: usize, given: usize },
    #[error("a buffer has {has} moments and the block is {block}")]
    BufferTooShort { has: usize, block: usize },
    #[error("a block of {0} moments is more than the plugin's interface can say")]
    BlockTooLong(usize),
}

/// The functions of a plugin that are called after it is made, copied out of its descriptor (they are good for as long as the library is loaded).
#[derive(Clone, Copy)]
struct Functions {
    connect_port: unsafe extern "C" fn(Handle, c_ulong, *mut f32),
    activate: Option<unsafe extern "C" fn(Handle)>,
    run: unsafe extern "C" fn(Handle, c_ulong),
    deactivate: Option<unsafe extern "C" fn(Handle)>,
    cleanup: unsafe extern "C" fn(Handle),
}

/// A plugin that has been made.
pub struct Instance {
    functions: Functions,
    handle: Handle,
    description: PluginDescription,
    /// The value of each port that is a control, by the number of the port (the others are unused). It is never made longer or shorter, and it is where the ports of controls
    /// are connected at every block.
    controls: Box<[f32]>,
    audio_inputs: Vec<usize>,
    audio_outputs: Vec<usize>,
    silence: Box<[f32]>,
    active: bool,
    sample_rate: u32,
    /// (Last: the functions above are only good while the library is loaded, and fields are dropped in order. `Drop` for the instance has run by then, and has called
    /// `cleanup`.)
    _loaded: Arc<Loaded>,
}

// SAFETY: an instance is a pointer into the plugin's own memory, which only this instance uses. LADSPA does not say that a plugin must be usable from any thread, and the ones
// in the world are written as if they were (they keep their state in the instance and, if they have shared state, it is read-only tables). **This is a belief, not something
// that is checked**; what it allows is moving an instance to another thread, never using one from two threads at once (it is not `Sync`).
unsafe impl Send for Instance {}

impl fmt::Debug for Instance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Instance")
            .field("label", &self.description.label)
            .field("library", &self._loaded.path())
            .field("sample_rate", &self.sample_rate)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl Instance {
    pub(crate) fn new(loaded: &Arc<Loaded>, plugin_index: usize, sample_rate: u32) -> Result<Self, InstantiateError> {
        let raw = loaded.descriptor(plugin_index);
        if raw.is_null() {
            return Err(InstantiateError::NoSuchPlugin(plugin_index));
        }
        // SAFETY: not null, and the descriptor that the library gave, which stays good while it is loaded (`loaded` is).
        let description = unsafe { read_descriptor(raw, plugin_index) }.map_err(InstantiateError::BadDescription)?;
        // SAFETY: as above.
        let descriptor: &RawDescriptor = unsafe { &*raw };
        let instantiate = descriptor
            .instantiate
            .ok_or(InstantiateError::MissingFunction("instantiate"))?;
        let functions = Functions {
            connect_port: descriptor
                .connect_port
                .ok_or(InstantiateError::MissingFunction("connect_port"))?,
            activate: descriptor.activate,
            run: descriptor.run.ok_or(InstantiateError::MissingFunction("run"))?,
            deactivate: descriptor.deactivate,
            cleanup: descriptor.cleanup.ok_or(InstantiateError::MissingFunction("cleanup"))?,
        };
        let mut controls = vec![0.0_f32; description.ports.len()].into_boxed_slice();
        for (number, port) in description.ports.iter().enumerate() {
            if port.kind == PortKind::Control && port.direction == PortDirection::Input {
                controls[number] = port.hint.default_value(sample_rate);
            }
        }
        // SAFETY: `raw` is the plugin's own descriptor and the sample rate is a number: this is what `instantiate` is for. It runs the plugin's code.
        let handle = unsafe { instantiate(raw, c_ulong::from(sample_rate)) };
        if handle.is_null() {
            return Err(InstantiateError::GaveNothing(sample_rate));
        }
        let audio_inputs = description.ports_of(PortDirection::Input, PortKind::Audio).collect();
        let audio_outputs = description.ports_of(PortDirection::Output, PortKind::Audio).collect();
        let mut instance = Self {
            functions,
            handle,
            description,
            controls,
            audio_inputs,
            audio_outputs,
            silence: vec![0.0; MOMENTS_OF_SILENCE].into_boxed_slice(),
            active: false,
            sample_rate,
            _loaded: Arc::clone(loaded),
        };
        // (Every port is connected before anything else is called: `activate` may expect it.)
        instance.connect_every_port_to_what_this_owns();
        Ok(instance)
    }

    pub fn description(&self) -> &PluginDescription {
        &self.description
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Gives the control input `port` the value `value`.
    pub fn set_control(&mut self, port: usize, value: f32) -> Result<(), ControlError> {
        let description = self.description.ports.get(port).ok_or(ControlError::NoSuchPort(port))?;
        if description.kind != PortKind::Control || description.direction != PortDirection::Input {
            return Err(ControlError::NotAControlInput(port));
        }
        if !value.is_finite() {
            return Err(ControlError::NotANumber);
        }
        self.controls[port] = value;
        Ok(())
    }

    /// The value of the control `port` (a control input has what it was given; a control output has what the plugin wrote when it last ran).
    pub fn control(&self, port: usize) -> Option<f32> {
        (self.description.ports.get(port)?.kind == PortKind::Control).then(|| self.controls[port])
    }

    /// Calls the plugin's `activate`, if it is not active (it is made active by the first block that is run, if this has not been called).
    pub fn activate(&mut self) {
        if self.active {
            return;
        }
        if let Some(activate) = self.functions.activate {
            // SAFETY: the handle is this instance's own, the instance has not been cleaned up (that is done when it is dropped), and every port is connected to memory that this owns.
            unsafe { activate(self.handle) };
        }
        self.active = true;
    }

    /// Calls the plugin's `deactivate`, if it is active.
    pub fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        if let Some(deactivate) = self.functions.deactivate {
            // SAFETY: as in `activate`.
            unsafe { deactivate(self.handle) };
        }
        self.active = false;
    }

    /// Puts the plugin in the state it had when it began (what a filter remembers, what a reverb rings with): `deactivate` and `activate`.
    pub fn reset(&mut self) {
        self.deactivate();
        self.activate();
    }

    /// Runs the plugin on `frames` moments of sound. `inputs` are the sound that goes in, one buffer for each audio input of the plugin in the order of their ports, and
    /// `outputs` are where the sound comes out, one for each audio output; each is at least `frames` long. **The inputs are lent to be written to**, because a plugin is
    /// given them as pointers that it could write through (and some do): the caller's sound there may be changed, and an output is never the same buffer as an input.
    pub fn run(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
    ) -> Result<(), RunError> {
        if inputs.len() != self.audio_inputs.len() {
            return Err(RunError::WrongNumberOfInputs {
                expected: self.audio_inputs.len(),
                given: inputs.len(),
            });
        }
        if outputs.len() != self.audio_outputs.len() {
            return Err(RunError::WrongNumberOfOutputs {
                expected: self.audio_outputs.len(),
                given: outputs.len(),
            });
        }
        if let Some(short) = inputs
            .iter()
            .map(|buffer| buffer.len())
            .chain(outputs.iter().map(|buffer| buffer.len()))
            .find(|&length| length < frames)
        {
            return Err(RunError::BufferTooShort {
                has: short,
                block: frames,
            });
        }
        let block = c_ulong::try_from(frames).map_err(|_| RunError::BlockTooLong(frames))?;
        self.activate();
        for (buffer, &port) in inputs.iter_mut().zip(&self.audio_inputs) {
            self.connect(port, buffer.as_mut_ptr());
        }
        for (buffer, &port) in outputs.iter_mut().zip(&self.audio_outputs) {
            self.connect(port, buffer.as_mut_ptr());
        }
        self.connect_the_controls();
        // SAFETY: the handle is this instance's own and it has not been cleaned up. Every port is connected: the controls to `self.controls` (just now, so that the pointers
        // come from a borrow that is not older than this call), the audio to buffers that the caller lent, each at least `frames` long and different from each other (they
        // are separate `&mut`), and which are not touched until the call is over.
        unsafe { (self.functions.run)(self.handle, block) };
        // (The audio ports are put back on memory that this owns, so that no pointer to a buffer that the caller may free is left in the plugin.)
        self.connect_the_audio_to_silence();
        Ok(())
    }

    fn connect(&self, port: usize, location: *mut f32) {
        // SAFETY: the handle is this instance's own and it has not been cleaned up, `port` is the number of a port of the plugin, and the location is memory that stays good
        // for as long as the plugin may use it: the callers lend it for one call and put the ports back on `silence` or `controls` after.
        unsafe { (self.functions.connect_port)(self.handle, port as c_ulong, location) };
    }

    fn connect_the_controls(&mut self) {
        for (number, port) in self.description.ports.iter().enumerate() {
            if port.kind == PortKind::Control {
                let location: *mut f32 = &mut self.controls[number];
                // SAFETY: as in `connect`; `controls` is never made longer or shorter, so the location stays where it is.
                unsafe { (self.functions.connect_port)(self.handle, number as c_ulong, location) };
            }
        }
    }

    fn connect_the_audio_to_silence(&mut self) {
        let silence: *mut f32 = self.silence.as_mut_ptr();
        for &port in self.audio_inputs.iter().chain(&self.audio_outputs) {
            self.connect(port, silence);
        }
    }

    fn connect_every_port_to_what_this_owns(&mut self) {
        self.connect_the_controls();
        self.connect_the_audio_to_silence();
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.deactivate();
        // SAFETY: the handle is this instance's own and this is the only time it is cleaned up. Nothing uses it afterwards.
        unsafe { (self.functions.cleanup)(self.handle) };
    }
}
