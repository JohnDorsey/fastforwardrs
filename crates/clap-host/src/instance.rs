// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A plugin that has been made, activated and can be run.

use std::ffi::CString;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::latency::PluginLatency;
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_extensions::state::PluginState;
use clack_extensions::tail::{PluginTail, TailLength};
use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::host::{Host, MainThreadPart, Requests, Shared, Threads};
use crate::library::Library;
use crate::main_thread::MainThread;
use crate::scan::{AudioPort, Parameter};

/// Why a plugin could not be made.
#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("the plugin could not be made: {0}")]
    CouldNotBeMade(String),
    #[error("the plugin could not be activated: {0}")]
    CouldNotBeActivated(String),
    #[error("the id of the plugin is not text that can be given to a library")]
    BadId,
    #[error("the main thread of the plugins has ended")]
    MainThreadEnded,
}

/// Why a block could not be run.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RunError {
    /// The plugin could not start processing.
    #[error("the plugin could not start processing: {0}")]
    CouldNotStart(String),
    /// `process` said that it failed: the sound it made is not to be used.
    #[error("the plugin said that processing failed")]
    ProcessingFailed,
    /// The buffers given are not what the ports of the plugin are.
    #[error("{0}")]
    WrongBuffers(String),
}

/// A parameter is to have this value at the sample `time` of the block.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterChange {
    pub time: u32,
    pub id: u32,
    pub value: f64,
}

/// What `process` said about how it wants to go on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    Continue,
    ContinueIfNotQuiet,
    Tail,
    Sleep,
}

/// What an instance says of itself once it is activated.
pub(crate) struct Gathered {
    pub(crate) inputs: Vec<AudioPort>,
    pub(crate) outputs: Vec<AudioPort>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) latency: u32,
    pub(crate) has_state: bool,
    pub(crate) has_gui: bool,
    pub(crate) has_tail: bool,
}

/// A plugin that is active.
pub struct Instance {
    worker: Arc<MainThread>,
    key: u64,
    processor: Option<PluginAudioProcessor<Host>>,
    requests: Arc<Requests>,
    threads: Arc<Threads>,
    gathered: Gathered,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    input_events: EventBuffer,
    output_events: EventBuffer,
    /// Sound for the ports that are not given any, silent.
    scratch: Mutex<Vec<Vec<f32>>>,
}

// (The audio processor is `Send`; nothing else in the instance is tied to a thread.)
impl Instance {
    /// Makes the plugin `id` of `library` on `worker`, and activates it at `sample_rate` for blocks of 1 to `most_frames`.
    pub fn new(
        worker: &Arc<MainThread>,
        library: &Library,
        id: &str,
        sample_rate: f64,
        most_frames: u32,
    ) -> Result<Self, InstanceError> {
        let id = CString::new(id).map_err(|_| InstanceError::BadId)?;
        let entry = library.entry.clone();
        let requests = Arc::new(Requests::default());
        let threads = Arc::new(Threads::default());
        let made = {
            let (requests, threads) = (Arc::clone(&requests), Arc::clone(&threads));
            worker
                .call(
                    move |instances| -> Result<(u64, PluginAudioProcessor<Host>, Gathered), InstanceError> {
                        *threads.main.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(std::thread::current().id());
                        let host_info = HostInfo::new(
                            "fastforwardrs",
                            "John Dorsey",
                            "https://github.com/JohnDorsey/fastforwardrs",
                            "0.1.0",
                        )
                        .map_err(|error| InstanceError::CouldNotBeMade(error.to_string()))?;
                        let (shared_requests, shared_threads, main_requests) =
                            (Arc::clone(&requests), Arc::clone(&threads), Arc::clone(&requests));
                        let mut instance = PluginInstance::<Host>::new(
                            move |()| Shared {
                                requests: shared_requests,
                                threads: shared_threads,
                            },
                            move |_| MainThreadPart {
                                requests: main_requests,
                                _shared: std::marker::PhantomData,
                            },
                            &entry,
                            &id,
                            &host_info,
                        )
                        .map_err(|error| InstanceError::CouldNotBeMade(error.to_string()))?;
                        let processor = instance
                            .activate(
                                |_, _| (),
                                PluginAudioConfiguration {
                                    sample_rate,
                                    min_frames_count: 1,
                                    max_frames_count: most_frames,
                                },
                            )
                            .map_err(|error| InstanceError::CouldNotBeActivated(error.to_string()))?;
                        let gathered = gather(&mut instance, true);
                        let key = instances.next;
                        instances.next += 1;
                        instances.map.insert(key, instance);
                        Ok((key, PluginAudioProcessor::Stopped(processor), gathered))
                    },
                )
                .ok_or(InstanceError::MainThreadEnded)??
        };
        let (key, processor, gathered) = made;
        let channels = gathered
            .inputs
            .iter()
            .map(|port| port.channel_count as usize)
            .sum::<usize>()
            .max(1);
        let out_channels = gathered
            .outputs
            .iter()
            .map(|port| port.channel_count as usize)
            .sum::<usize>()
            .max(1);
        Ok(Self {
            worker: Arc::clone(worker),
            key,
            processor: Some(processor),
            requests,
            threads,
            input_ports: AudioPorts::with_capacity(channels, gathered.inputs.len().max(1)),
            output_ports: AudioPorts::with_capacity(out_channels, gathered.outputs.len().max(1)),
            gathered,
            input_events: EventBuffer::new(),
            output_events: EventBuffer::new(),
            scratch: Mutex::new(Vec::new()),
        })
    }

    /// The audio input ports the plugin has, in order (the main one first).
    pub fn audio_inputs(&self) -> &[AudioPort] {
        &self.gathered.inputs
    }

    pub fn audio_outputs(&self) -> &[AudioPort] {
        &self.gathered.outputs
    }

    pub fn parameters(&self) -> &[Parameter] {
        &self.gathered.parameters
    }

    /// How many samples later the plugin's sound comes out than the sound that goes in (read when it was activated).
    pub fn latency(&self) -> u32 {
        self.gathered.latency
    }

    /// Whether the plugin has a state to be saved (it keeps something that is not a parameter).
    pub fn has_state(&self) -> bool {
        self.gathered.has_state
    }

    /// Whether the plugin has a window of its own to show (`clap.gui`).
    pub fn has_a_window(&self) -> bool {
        self.gathered.has_gui
    }

    /// How long the plugin goes on making sound after its input stops, in samples, if it says (`u32::MAX` is for ever).
    pub fn tail(&mut self) -> Option<u32> {
        if !self.gathered.has_tail {
            return None;
        }
        let processor = self.processor.as_mut()?;
        let extension = processor.shared_plugin_handle().get_extension::<PluginTail>()?;
        Some(match extension.get(&processor.plugin_handle()) {
            TailLength::Finite(samples) => samples,
            TailLength::Infinite => u32::MAX,
        })
    }

    /// Whether the plugin has asked to be deactivated and activated again (a change of latency, of ports...).
    pub fn restart_was_requested(&self) -> bool {
        self.requests.restart.load(Ordering::SeqCst)
    }

    /// Whether the plugin has said that its parameters are not what they were.
    pub fn parameters_changed(&self) -> bool {
        self.requests.parameters_changed.load(Ordering::SeqCst)
    }

    /// Does the call on the main thread that the plugin asked for (`request_callback`), if it did. Says whether there was one to do.
    pub fn do_the_main_thread_callback_if_asked(&mut self) -> bool {
        if !self.requests.callback.swap(false, Ordering::SeqCst) {
            return false;
        }
        let key = self.key;
        let _ = self.worker.call(move |instances| {
            if let Some(instance) = instances.map.get_mut(&key) {
                instance.call_on_main_thread_callback();
            }
        });
        true
    }

    /// Makes the plugin forget what it has heard (its parameters are kept).
    pub fn reset(&mut self) {
        if let Some(processor) = self.processor.as_mut() {
            processor.reset();
        }
    }

    /// Runs the plugin on a block. `inputs` and `outputs` are the channels of the main ports (the first of each); the other ports the plugin has are given silence and their output
    /// is thrown away. All channels are as long as the block. `changes` are in order of time. **This is the audio thread.**
    pub fn run(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        changes: &[ParameterChange],
    ) -> Result<ProcessOutcome, RunError> {
        let frames = outputs.first().map_or(0, |channel| channel.len());
        if let Some(wrong) = self.channels_are_wrong(inputs, outputs, frames) {
            return Err(RunError::WrongBuffers(wrong));
        }
        *self
            .threads
            .audio
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(std::thread::current().id());
        let processor = self
            .processor
            .as_mut()
            .expect("the processor is there until the instance is dropped");
        if !processor.is_started() {
            processor
                .start_processing()
                .map_err(|error| RunError::CouldNotStart(error.to_string()))?;
        }
        self.input_events.clear();
        self.output_events.clear();
        for change in changes {
            if let Some(id) = ClapId::from_raw(change.id) {
                self.input_events
                    .push(&ParamValueEvent::new(change.time, id, Pckn::match_all(), change.value));
            }
        }

        // The ports that are not the main ones are given silence.
        let mut scratch = self.scratch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let extra_channels: usize = self
            .gathered
            .inputs
            .iter()
            .skip(1)
            .chain(self.gathered.outputs.iter().skip(1))
            .map(|port| port.channel_count as usize)
            .sum();
        scratch.resize_with(extra_channels, Vec::new);
        for channel in scratch.iter_mut() {
            channel.clear();
            channel.resize(frames, 0.0);
        }
        let mut scratch_channels = scratch.iter_mut();
        let mut input_buffers: Vec<Vec<&mut [f32]>> = Vec::new();
        let mut output_buffers: Vec<Vec<&mut [f32]>> = Vec::new();
        let (mut main_inputs, mut main_outputs) = (Some(inputs), Some(outputs));
        for (number, port) in self.gathered.inputs.iter().enumerate() {
            if number == 0 {
                input_buffers.push(
                    main_inputs
                        .take()
                        .unwrap_or_default()
                        .iter_mut()
                        .map(|channel| &mut **channel)
                        .collect(),
                );
            } else {
                input_buffers.push(
                    scratch_channels
                        .by_ref()
                        .take(port.channel_count as usize)
                        .map(Vec::as_mut_slice)
                        .collect(),
                );
            }
        }
        for (number, port) in self.gathered.outputs.iter().enumerate() {
            if number == 0 {
                output_buffers.push(
                    main_outputs
                        .take()
                        .unwrap_or_default()
                        .iter_mut()
                        .map(|channel| &mut **channel)
                        .collect(),
                );
            } else {
                output_buffers.push(
                    scratch_channels
                        .by_ref()
                        .take(port.channel_count as usize)
                        .map(Vec::as_mut_slice)
                        .collect(),
                );
            }
        }

        let input_audio = self
            .input_ports
            .with_input_buffers(input_buffers.iter_mut().map(|channels| {
                AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_input_only(
                        channels
                            .iter_mut()
                            .map(|channel| InputChannel::variable(&mut **channel)),
                    ),
                }
            }));
        let mut input_audio = input_audio;
        let mut output_audio = self
            .output_ports
            .with_output_buffers(output_buffers.iter_mut().map(|channels| AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(channels.iter_mut().map(|channel| &mut **channel)),
            }));
        let input_events = InputEvents::from_buffer(&self.input_events);
        let mut output_events = OutputEvents::from_buffer(&mut self.output_events);
        let started = processor
            .as_started_mut()
            .map_err(|error| RunError::CouldNotStart(error.to_string()))?;
        let status = started
            .process(
                &input_audio,
                &mut output_audio,
                &input_events,
                &mut output_events,
                None,
                None,
            )
            .map_err(|_| RunError::ProcessingFailed)?;
        let _ = &mut input_audio;
        Ok(match status {
            ProcessStatus::Continue => ProcessOutcome::Continue,
            ProcessStatus::ContinueIfNotQuiet => ProcessOutcome::ContinueIfNotQuiet,
            ProcessStatus::Tail => ProcessOutcome::Tail,
            ProcessStatus::Sleep => ProcessOutcome::Sleep,
        })
    }

    fn channels_are_wrong(&self, inputs: &[&mut [f32]], outputs: &[&mut [f32]], frames: usize) -> Option<String> {
        let wanted = |ports: &[AudioPort]| ports.first().map_or(0, |port| port.channel_count as usize);
        if inputs.len() != wanted(&self.gathered.inputs) {
            return Some(format!(
                "the plugin has {} channels in its main input, and {} were given",
                wanted(&self.gathered.inputs),
                inputs.len()
            ));
        }
        if outputs.len() != wanted(&self.gathered.outputs) {
            return Some(format!(
                "the plugin has {} channels in its main output, and {} were given",
                wanted(&self.gathered.outputs),
                outputs.len()
            ));
        }
        if inputs
            .iter()
            .chain(outputs.iter())
            .any(|channel| channel.len() != frames)
        {
            return Some("the channels are not as long as each other".to_owned());
        }
        None
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let Some(mut processor) = self.processor.take() else {
            return;
        };
        // (Processing is stopped on this thread, which is the audio thread; the rest is for the main thread, which waits for it here so that the plugin is gone when this returns.)
        let stopped = processor.ensure_processing_stopped();
        let _ = stopped;
        let key = self.key;
        let _ = self.worker.call(move |instances| {
            if let Some(mut instance) = instances.map.remove(&key) {
                instance.deactivate(processor.into_stopped());
            }
        });
    }
}

/// Asks an instance that is active what it has: ports, parameters, latency, state. (On the main thread.)
pub(crate) fn gather<H: HostHandlers>(instance: &mut PluginInstance<H>, active: bool) -> Gathered {
    let mut gathered = Gathered {
        inputs: Vec::new(),
        outputs: Vec::new(),
        parameters: Vec::new(),
        latency: 0,
        has_state: false,
        has_gui: false,
        has_tail: false,
    };
    let handle = instance.plugin_handle();
    if let Some(ports) = handle.get_extension::<PluginAudioPorts>() {
        let mut buffer = AudioPortInfoBuffer::new();
        for is_input in [true, false] {
            for index in 0..ports.count(&handle, is_input) {
                if let Some(info) = ports.get(&handle, index, is_input, &mut buffer) {
                    let port = AudioPort {
                        id: info.id.get(),
                        name: String::from_utf8_lossy(info.name).into_owned(),
                        channel_count: info.channel_count,
                        port_type: info.port_type.map(|kind| kind.0.to_string_lossy().into_owned()),
                        is_main: info
                            .flags
                            .contains(clack_extensions::audio_ports::AudioPortFlags::IS_MAIN),
                    };
                    if is_input {
                        gathered.inputs.push(port);
                    } else {
                        gathered.outputs.push(port);
                    }
                }
            }
        }
    }
    if let Some(params) = handle.get_extension::<PluginParams>() {
        let mut buffer = ParamInfoBuffer::new();
        for index in 0..params.count(&handle) {
            if let Some(info) = params.get_info(&handle, index, &mut buffer) {
                let mut texts = Vec::new();
                if info.flags.contains(ParamInfoFlags::IS_ENUM) {
                    let mut text = [0u8; 256];
                    let (least, most) = (info.min_value.round() as i64, info.max_value.round() as i64);
                    for value in least..=most.min(least + 255) {
                        texts.push(
                            params
                                .value_to_text(&handle, info.id, value as f64, &mut text)
                                .map(|written| String::from_utf8_lossy(written).into_owned())
                                .unwrap_or_default(),
                        );
                    }
                }
                gathered.parameters.push(Parameter {
                    id: info.id.get(),
                    name: String::from_utf8_lossy(info.name).into_owned(),
                    module: String::from_utf8_lossy(info.module).into_owned(),
                    minimum: info.min_value,
                    maximum: info.max_value,
                    default: info.default_value,
                    flags: info.flags.bits(),
                    texts,
                });
            }
        }
    }
    // (A plugin that is not active is not asked its latency: it cannot know it before it knows the sample rate, and some say so.)
    if active && let Some(latency) = handle.get_extension::<PluginLatency>() {
        gathered.latency = latency.get(&handle);
    }
    gathered.has_state = handle.get_extension::<PluginState>().is_some();
    gathered.has_gui = handle.get_extension::<clack_extensions::gui::PluginGui>().is_some();
    gathered.has_tail = handle.get_extension::<PluginTail>().is_some();
    gathered
}
