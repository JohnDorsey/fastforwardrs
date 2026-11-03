// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A copy of a VST3 plugin that has been made and can be run: the component (the sound) and the controller (the parameters), connected, with their buses switched on and
//! processing started. The calls a host makes are in the order VST3 gives: make, initialize, connect, state, set up, activate, start processing; and the reverse when it is let go.
//!
//! **Values are plain here** (what `normalizedParamToPlain` says), as everywhere else in the program; the plugin is given the normalised value that the controller says is the same.
//! A value is given to the plugin as a change of its parameter at a sample of the next block, **and** to the controller, so that the two halves agree.
//!
//! Every `unsafe` block says why it is sound: **calling a method of a plugin's object** (sound if the plugin keeps the contract of the VST3 headers, the one thing that cannot be
//! checked, and why a library is only loaded after a person has said yes) and **making the lists of pointers that a block is given** (sound while the buffers live, which the
//! call that runs the block makes true by owning them for its length).

#![allow(unsafe_code)]

use std::collections::HashMap;

use thiserror::Error;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{ComPtr, ComWrapper};

use crate::classid::ClassId;
use crate::description::{BusDescription, ClassFacts, ParameterDescription, wide_text_of};
use crate::module::Module;
use crate::notes::{CONTROLLER_ALL_NOTES_OFF, CONTROLLER_PITCH_BEND, NoteEvent, NoteEventKind};
use crate::objects::{EventList, HostObject, MemoryStream, ParameterChanges};
use crate::state::{StateError, decode_state, encode_state};
use crate::view::OpenView;

/// The most steps of a stepped parameter whose texts are read.
const MOST_STEPS_WITH_TEXTS: i32 = 255;

/// The longest that the silent block is that an instrument with a state runs before any note.
const SILENT_BLOCK_FRAMES: usize = 512;

/// Why a copy could not be made.
#[derive(Debug, Error)]
pub enum InstantiateError {
    #[error("the class could not be made: {0}")]
    CouldNotBeMade(String),
    #[error("the plugin would not initialize (it said {0:#x})")]
    WouldNotInitialize(i32),
    #[error("the class is not a plugin that processes audio (it has no IAudioProcessor)")]
    NotAProcessor,
    #[error("the plugin has no main audio output")]
    NoMainOutput,
    #[error("the plugin cannot process 32-bit sound")]
    No32BitSound,
    #[error("the plugin would not be set up for processing (it said {0:#x})")]
    WouldNotSetUp(i32),
    #[error("the plugin would not become active (it said {0:#x})")]
    WouldNotBecomeActive(i32),
    #[error("the plugin would not start processing (it said {0:#x})")]
    WouldNotStartProcessing(i32),
    #[error("the first block failed: {0}")]
    FirstBlockFailed(RunError),
}

/// Why a block could not be run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunError {
    #[error("the plugin's process said it failed (it said {0:#x})")]
    ProcessFailed(i32),
    #[error("{asked} frames were asked for in one block, and {most} are the most the plugin was set up for")]
    TooLong { asked: usize, most: usize },
}

/// What the plugin asked for between blocks.
#[derive(Debug, Default, PartialEq)]
pub struct BetweenBlocks {
    /// The plugin edited parameters by itself (its own window, or a controller that told the host): `(parameter, plain value)`. They have been given to the next block as well.
    pub edits: Vec<(u32, f64)>,
    /// Its latency changed (`restartComponent`): [`Instance::latency`] says what it is now.
    pub latency_changed: bool,
    /// It asked for itself to be made again (`kReloadComponent` or `kIoChanged`): the owner of the copy decides whether to.
    pub wants_to_be_made_again: bool,
    /// Its parameters changed by themselves (`kParamValuesChanged`).
    pub values_changed: bool,
}

/// A copy of a plugin.
pub struct Instance {
    // (The plugin's interfaces are let go of in the order they are written here, which is the reverse of making them: some plugins (DPF's) complain if their component goes
    // before the interface of their processor. **The window comes first of all**: it is the controller's, and is taken out of its parent before the controller is touched.)
    pub(crate) view: Option<OpenView>,
    mapping: Option<ComPtr<IMidiMapping>>,
    pub(crate) controller: Option<ComPtr<IEditController>>,
    processor: ComPtr<IAudioProcessor>,
    component: ComPtr<IComponent>,
    pub(crate) host: ComWrapper<HostObject>,
    pub(crate) facts: ClassFacts,
    pub(crate) most_frames: usize,
    sample_rate: f64,
    /// The places of the main buses among the buses of the plugin.
    main_input: Option<usize>,
    main_output: usize,
    input_channels: usize,
    output_channels: usize,
    changes_in: ComWrapper<ParameterChanges>,
    changes_out: ComWrapper<ParameterChanges>,
    events: ComWrapper<EventList>,
    /// The changes of parameters that wait for the next block: `(parameter, sample, normalised value)`.
    pub(crate) pending: Vec<(u32, i32, f64)>,
    time_in_samples: i64,
    latency: u32,
    /// What the read-only parameters said last: `(parameter, normalised value)`.
    outputs: Vec<(u32, f64)>,
    /// How many controllers and pitch bends were left out for want of a mapping, since the last time that was asked.
    unmapped: u32,
    mapped: HashMap<i16, Option<u32>>,
    pointers_in: Vec<*mut f32>,
    pointers_out: Vec<*mut f32>,
    buses_in: Vec<AudioBusBuffers>,
    buses_out: Vec<AudioBusBuffers>,
    silence_in: Vec<Vec<f32>>,
    waste_out: Vec<Vec<f32>>,
    context: ProcessContext,
    /// The module comes last: it is let go of after everything made from it (it is here to be kept, and is never read).
    _keeps_the_module_loaded: Module,
}

// SAFETY: a copy is made on one thread and used by one thread at a time (VST3 asks for that of a component and its controller); nothing in it is shared with another copy. The
// pointers in it are to buffers that exist for the length of one call.
unsafe impl Send for Instance {}

impl Drop for Instance {
    fn drop(&mut self) {
        // (A window that is open comes out of its parent first.)
        self.close_view();
        // SAFETY: the plugin's objects are alive until this function ends; stopping, switching off, disconnecting and ending are the calls VST3 makes for letting a plugin go, in
        // the reverse order of making it.
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            if let Some(controller) = &self.controller
                && self.facts.controller_is_separate
            {
                if let (Some(a), Some(b)) = (
                    self.component.cast::<IConnectionPoint>(),
                    controller.cast::<IConnectionPoint>(),
                ) {
                    a.disconnect(b.as_ptr());
                    b.disconnect(a.as_ptr());
                }
                controller.terminate();
            }
            self.component.terminate();
        }
    }
}

fn bus_description(info: &BusInfo) -> BusDescription {
    // (The constants of the bindings' enums are `i32` on Windows and `u32` elsewhere: the type of the field decides.)
    let on_by_default: u32 = BusInfo_::BusFlags_::kDefaultActive as _;
    let main: BusType = BusTypes_::kMain as _;
    BusDescription {
        name: wide_text_of(&info.name),
        channels: u32::try_from(info.channelCount).unwrap_or(0),
        is_main: info.busType == main,
        is_on_by_default: info.flags & on_by_default != 0,
    }
}

/// The buses of one kind and direction.
fn buses_of(component: &ComPtr<IComponent>, media: MediaTypes, direction: BusDirections) -> Vec<BusDescription> {
    // SAFETY: the component is a live COM object; counting its buses has no pointers.
    let count = unsafe { component.getBusCount(media as MediaType, direction as BusDirection) }.clamp(0, 256);
    (0..count)
        .filter_map(|index| {
            // SAFETY: `BusInfo` is a plain struct of integers and arrays of them, for which all zeros is a value.
            let mut info: BusInfo = unsafe { std::mem::zeroed() };
            // SAFETY: `info` lives through the call, which fills it.
            let result =
                unsafe { component.getBusInfo(media as MediaType, direction as BusDirection, index, &mut info) };
            (result == kResultOk).then(|| bus_description(&info))
        })
        .collect()
}

/// The default of a parameter, as a normalised number: what the plugin declares, **unless it declares 0 and its controller's value is not 0**, and then that value. A plugin that does not
/// declare defaults (the VST3 build of Surge XT says 0 for each of its 2,855 values) has its defaults taken from the values its controller has when the plugin is made (after it has been
/// given the state of its component), which are the plugin's own: a host that takes 0 for them pushes 0 into every value and silences the plugin.
pub(crate) fn default_of_a_parameter(declared: f64, in_the_controller_now: f64) -> f64 {
    if declared == 0.0 && in_the_controller_now.is_finite() && in_the_controller_now != 0.0 {
        in_the_controller_now.clamp(0.0, 1.0)
    } else {
        declared
    }
}

fn parameters_of(controller: &ComPtr<IEditController>) -> Vec<ParameterDescription> {
    // SAFETY: the controller is a live COM object; counting its parameters has no pointers.
    let count = unsafe { controller.getParameterCount() }.max(0);
    let mut parameters = Vec::new();
    for index in 0..count {
        // SAFETY: `ParameterInfo` is a plain struct of integers, floats and arrays of them, for which all zeros is a value.
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        // SAFETY: `info` lives through the call, which fills it.
        if unsafe { controller.getParameterInfo(index, &mut info) } != kResultOk {
            continue;
        }
        let flag = |wanted: i32| info.flags & wanted != 0;
        // SAFETY: converting a value is a call with numbers only.
        let (plain_minimum, plain_maximum) = unsafe {
            (
                controller.normalizedParamToPlain(info.id, 0.0),
                controller.normalizedParamToPlain(info.id, 1.0),
            )
        };
        // SAFETY: the controller is live; the call has numbers only.
        let default_normalized = default_of_a_parameter(info.defaultNormalizedValue, unsafe {
            controller.getParamNormalized(info.id)
        });
        let texts = if info.stepCount > 0 && info.stepCount <= MOST_STEPS_WITH_TEXTS {
            (0..=info.stepCount)
                .map(|step| {
                    let mut text: String128 = [0; 128];
                    // SAFETY: `text` lives through the call, which fills it.
                    let result = unsafe {
                        controller.getParamStringByValue(
                            info.id,
                            f64::from(step) / f64::from(info.stepCount),
                            &mut text,
                        )
                    };
                    if result == kResultOk {
                        wide_text_of(&text)
                    } else {
                        step.to_string()
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        parameters.push(ParameterDescription {
            id: info.id,
            title: wide_text_of(&info.title),
            short_title: wide_text_of(&info.shortTitle),
            units: wide_text_of(&info.units),
            step_count: info.stepCount,
            default_normalized,
            // SAFETY: the controller is live; converting a value is a call with numbers only.
            default_plain: unsafe { controller.normalizedParamToPlain(info.id, default_normalized) },
            plain_minimum,
            plain_maximum,
            texts,
            can_automate: flag(ParameterInfo_::ParameterFlags_::kCanAutomate as _),
            is_read_only: flag(ParameterInfo_::ParameterFlags_::kIsReadOnly as _),
            is_hidden: flag(ParameterInfo_::ParameterFlags_::kIsHidden as _),
            is_a_list: flag(ParameterInfo_::ParameterFlags_::kIsList as _),
            is_the_bypass: flag(ParameterInfo_::ParameterFlags_::kIsBypass as _),
            is_a_program_change: flag(ParameterInfo_::ParameterFlags_::kIsProgramChange as _),
            wraps_around: flag(ParameterInfo_::ParameterFlags_::kIsWrapAround as _),
        });
    }
    parameters
}

impl Instance {
    /// Makes a copy of the class `class` of `module`, set up for blocks of at most `most_frames` frames at `sample_rate`, and gives it the `state` that [`Self::save_state`] gave.
    /// The second thing given back says why the state was not taken, if it was not (the copy is made all the same, with the plugin's own state).
    ///
    /// **A plugin that takes notes and is given a state first runs one silent block with no events**: some plugins (Surge XT) apply the state in the first block they process, and
    /// hang when a note is in that block too.
    pub fn new(
        module: &Module,
        class: ClassId,
        sample_rate: f64,
        most_frames: usize,
        state: Option<&[u8]>,
    ) -> Result<(Self, Option<String>), InstantiateError> {
        let host = HostObject::new();
        let host_context = host
            .to_com_ptr::<IHostApplication>()
            .map(|pointer| pointer.as_ptr().cast::<FUnknown>())
            .ok_or_else(|| InstantiateError::CouldNotBeMade("the host object has no IHostApplication".to_owned()))?;
        let handler = host
            .to_com_ptr::<IComponentHandler>()
            .ok_or_else(|| InstantiateError::CouldNotBeMade("the host object has no IComponentHandler".to_owned()))?;

        let component = module
            .create::<IComponent>(class)
            .map_err(InstantiateError::CouldNotBeMade)?;
        // SAFETY: the component is live; the context is the host object, which lives through the call (and the plugin may keep it: it holds a reference).
        let result = unsafe { component.initialize(host_context) };
        if result != kResultOk {
            return Err(InstantiateError::WouldNotInitialize(result));
        }
        let Some(processor) = component.cast::<IAudioProcessor>() else {
            abandon(&component, None);
            return Err(InstantiateError::NotAProcessor);
        };

        // The controller: the component itself, or a class of its own.
        let mut controller = component.cast::<IEditController>();
        let mut controller_is_separate = false;
        if controller.is_none() {
            let mut controller_class: TUID = [0; 16];
            // SAFETY: the component is live; the id lives through the call, which fills it.
            if unsafe { component.getControllerClassId(&mut controller_class) } == kResultOk
                && let Ok(made) = module.create::<IEditController>(ClassId::from_tuid(&controller_class))
            {
                // SAFETY: the controller is live; the context lives through the call.
                if unsafe { made.initialize(host_context) } == kResultOk {
                    controller_is_separate = true;
                    controller = Some(made);
                }
            }
        }
        if let Some(controller) = &controller {
            // SAFETY: the controller is live; the handler is the host object, which lives as long as the copy.
            unsafe { controller.setComponentHandler(handler.as_ptr()) };
            if controller_is_separate
                && let (Some(a), Some(b)) = (
                    component.cast::<IConnectionPoint>(),
                    controller.cast::<IConnectionPoint>(),
                )
            {
                // SAFETY: both are live; connecting them is how VST3 has the two halves find each other.
                unsafe {
                    a.connect(b.as_ptr());
                    b.connect(a.as_ptr());
                }
            }
        }
        if let Some(controller) = &controller {
            give_the_controller_the_state_of_the_component(&component, controller);
        }
        let mapping = controller
            .as_ref()
            .and_then(|controller| controller.cast::<IMidiMapping>());
        let separate_controller = controller.clone().filter(|_| controller_is_separate);

        let facts = ClassFacts {
            audio_inputs: buses_of(&component, MediaTypes_::kAudio, BusDirections_::kInput),
            audio_outputs: buses_of(&component, MediaTypes_::kAudio, BusDirections_::kOutput),
            event_inputs: buses_of(&component, MediaTypes_::kEvent, BusDirections_::kInput),
            parameters: controller.as_ref().map(parameters_of).unwrap_or_default(),
            has_a_controller: controller.is_some(),
            controller_is_separate,
            has_midi_mapping: mapping.is_some(),
            latency: 0,
            tail: 0,
        };
        let Some((main_output, _)) = facts.main_audio_output() else {
            abandon(&component, separate_controller.as_ref());
            return Err(InstantiateError::NoMainOutput);
        };
        // SAFETY: the processor is live; asking about a sample size has only a number.
        if unsafe { processor.canProcessSampleSize(SymbolicSampleSizes_::kSample32 as _) } != kResultOk {
            abandon(&component, separate_controller.as_ref());
            return Err(InstantiateError::No32BitSound);
        }
        let main_input = facts.main_audio_input().map(|(place, _)| place);
        let input_channels = main_input.map_or(0, |place| facts.audio_inputs[place].channels as usize);
        let output_channels = facts.audio_outputs[main_output].channels as usize;

        let mut context: ProcessContext = unsafe_zeroed_context();
        context.sampleRate = sample_rate;
        context.tempo = 120.0;
        context.timeSigNumerator = 4;
        context.timeSigDenominator = 4;
        context.state = (ProcessContext_::StatesAndFlags_::kPlaying
            | ProcessContext_::StatesAndFlags_::kTempoValid
            | ProcessContext_::StatesAndFlags_::kTimeSigValid) as _;

        let mut instance = Self {
            view: None,
            host,
            component,
            processor,
            controller,
            mapping,
            facts,
            most_frames,
            sample_rate,
            main_input,
            main_output,
            input_channels,
            output_channels,
            changes_in: ParameterChanges::new(),
            changes_out: ParameterChanges::new(),
            events: EventList::new(),
            pending: Vec::new(),
            time_in_samples: 0,
            latency: 0,
            outputs: Vec::new(),
            unmapped: 0,
            mapped: HashMap::new(),
            pointers_in: Vec::new(),
            pointers_out: Vec::new(),
            buses_in: Vec::new(),
            buses_out: Vec::new(),
            silence_in: vec![vec![0.0; most_frames]; input_channels],
            waste_out: vec![vec![0.0; most_frames]; output_channels],
            context,
            _keeps_the_module_loaded: module.clone(),
        };
        // (From here on, if making the copy fails, dropping it ends the plugin as VST3 says.)
        let state_note = state
            .and_then(|bytes| instance.restore_state(bytes).err())
            .map(|error| error.to_string());
        instance.set_up_and_start()?;
        instance.facts.latency = instance.read_latency();
        instance.latency = instance.facts.latency;
        // SAFETY: the processor is live; asking for the tail has no arguments.
        instance.facts.tail = unsafe { instance.processor.getTailSamples() };
        if state.is_some() && instance.facts.takes_events() {
            instance
                .run_with_notes(&mut [], &mut [], most_frames.min(SILENT_BLOCK_FRAMES), &[])
                .map_err(InstantiateError::FirstBlockFailed)?;
        }
        Ok((instance, state_note))
    }

    fn set_up_and_start(&mut self) -> Result<(), InstantiateError> {
        let mut setup = ProcessSetup {
            processMode: ProcessModes_::kRealtime as _,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as _,
            maxSamplesPerBlock: i32::try_from(self.most_frames).unwrap_or(i32::MAX),
            sampleRate: self.sample_rate,
        };
        // SAFETY: the processor is live; the setup lives through the call.
        let result = unsafe { self.processor.setupProcessing(&mut setup) };
        if result != kResultOk {
            return Err(InstantiateError::WouldNotSetUp(result));
        }
        // (Only the main buses are switched on: a side chain is on by default in some plugins, and has no sound to give.)
        let switch = |media: MediaTypes, direction: BusDirections, count: usize, wanted: Option<usize>| {
            for place in 0..count {
                // SAFETY: the component is live; the call has numbers only.
                unsafe {
                    self.component.activateBus(
                        media as MediaType,
                        direction as BusDirection,
                        place as i32,
                        TBool::from(Some(place) == wanted),
                    );
                }
            }
        };
        switch(
            MediaTypes_::kAudio,
            BusDirections_::kInput,
            self.facts.audio_inputs.len(),
            self.main_input,
        );
        switch(
            MediaTypes_::kAudio,
            BusDirections_::kOutput,
            self.facts.audio_outputs.len(),
            Some(self.main_output),
        );
        switch(
            MediaTypes_::kEvent,
            BusDirections_::kInput,
            self.facts.event_inputs.len(),
            Some(0),
        );
        // SAFETY: the component and the processor are live; the calls have numbers only.
        let (active, processing) = unsafe { (self.component.setActive(1), self.processor.setProcessing(1)) };
        if active != kResultOk {
            return Err(InstantiateError::WouldNotBecomeActive(active));
        }
        if processing != kResultOk && processing != kNotImplemented {
            return Err(InstantiateError::WouldNotStartProcessing(processing));
        }
        Ok(())
    }

    fn read_latency(&self) -> u32 {
        // SAFETY: the processor is live; asking for the latency has no arguments.
        unsafe { self.processor.getLatencySamples() }
    }

    /// What the class is: its buses and parameters, whether its controller maps MIDI controllers, its latency and its tail.
    pub fn facts(&self) -> &ClassFacts {
        &self.facts
    }

    /// How many samples later its sound comes out than the sound that goes in (it may change: see [`BetweenBlocks::latency_changed`]).
    pub fn latency(&self) -> u32 {
        self.latency
    }

    /// How many channels the main audio input has (0 for an instrument), and the main output.
    pub fn channels(&self) -> (usize, usize) {
        (self.input_channels, self.output_channels)
    }

    /// Gives the parameter `id` the plain value `plain` from sample `at_sample` of the next block. A parameter the controller does not have is left alone.
    pub fn set_value(&mut self, id: u32, plain: f64, at_sample: usize) {
        let Some(controller) = &self.controller else {
            return;
        };
        // SAFETY: the controller is live; the calls have numbers only.
        let normalized = unsafe { controller.plainParamToNormalized(id, plain) }.clamp(0.0, 1.0);
        // SAFETY: as above. (The controller is told too, so that the two halves agree.)
        unsafe { controller.setParamNormalized(id, normalized) };
        self.pending
            .push((id, i32::try_from(at_sample).unwrap_or(i32::MAX), normalized));
        self.note_that_a_value_was_given(id, plain);
    }

    /// The plain value that the controller has for the parameter `id` now (`None` for a plugin with no controller).
    pub fn plain_value(&self, id: u32) -> Option<f64> {
        let controller = self.controller.as_ref()?;
        // SAFETY: the controller is live; the call has numbers only.
        let normalized = unsafe { controller.getParamNormalized(id) };
        Some(self.plain_of(id, normalized))
    }

    pub(crate) fn plain_of(&self, id: u32, normalized: f64) -> f64 {
        match &self.controller {
            // SAFETY: the controller is live; the call has numbers only.
            Some(controller) => unsafe { controller.normalizedParamToPlain(id, normalized) },
            None => normalized,
        }
    }

    /// Runs the plugin on `frames` samples: the channels of its main input and of its main output (a channel that is not given is silence, and one that is not asked for is thrown away).
    pub fn run(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
    ) -> Result<(), RunError> {
        self.run_with_notes(inputs, outputs, frames, &[])
    }

    /// [`Self::run`] with notes, in order of time, each at a sample of this block.
    pub fn run_with_notes(
        &mut self,
        inputs: &mut [&mut [f32]],
        outputs: &mut [&mut [f32]],
        frames: usize,
        notes: &[NoteEvent],
    ) -> Result<(), RunError> {
        if frames > self.most_frames {
            return Err(RunError::TooLong {
                asked: frames,
                most: self.most_frames,
            });
        }
        let frames_in_a_number = i32::try_from(frames).unwrap_or(i32::MAX);
        self.events.clear();
        let mut forget_the_voices = false;
        for note in notes {
            forget_the_voices |= self.add_a_note(*note, frames_in_a_number);
        }
        if forget_the_voices {
            // SAFETY: the processor is live; stopping and starting has numbers only.
            unsafe {
                self.processor.setProcessing(0);
                self.processor.setProcessing(1);
            }
        }
        self.changes_in.fill(&self.pending);
        self.pending.clear();
        self.changes_out.clear();

        self.pointers_in.clear();
        for (place, silence) in self.silence_in.iter_mut().enumerate() {
            match inputs.get_mut(place) {
                Some(channel) if channel.len() >= frames => self.pointers_in.push(channel.as_mut_ptr()),
                _ => self.pointers_in.push(silence.as_mut_ptr()),
            }
        }
        self.pointers_out.clear();
        for (place, waste) in self.waste_out.iter_mut().enumerate() {
            match outputs.get_mut(place) {
                Some(channel) if channel.len() >= frames => self.pointers_out.push(channel.as_mut_ptr()),
                _ => self.pointers_out.push(waste.as_mut_ptr()),
            }
        }
        // One entry for every bus; only the main one holds channels.
        self.buses_in.clear();
        for place in 0..self.facts.audio_inputs.len() {
            let is_main = Some(place) == self.main_input;
            self.buses_in.push(bus_buffers(
                if is_main { self.input_channels } else { 0 },
                &mut self.pointers_in,
            ));
        }
        self.buses_out.clear();
        for place in 0..self.facts.audio_outputs.len() {
            let is_main = place == self.main_output;
            self.buses_out.push(bus_buffers(
                if is_main { self.output_channels } else { 0 },
                &mut self.pointers_out,
            ));
        }
        self.context.projectTimeSamples = self.time_in_samples;
        let changes_in = self
            .changes_in
            .as_com_ref::<IParameterChanges>()
            .map_or(std::ptr::null_mut(), |pointer| pointer.as_ptr());
        let changes_out = self
            .changes_out
            .as_com_ref::<IParameterChanges>()
            .map_or(std::ptr::null_mut(), |pointer| pointer.as_ptr());
        let events = if self.facts.takes_events() {
            self.events
                .as_com_ref::<IEventList>()
                .map_or(std::ptr::null_mut(), |pointer| pointer.as_ptr())
        } else {
            std::ptr::null_mut()
        };
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as _,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as _,
            numSamples: frames_in_a_number,
            numInputs: self.buses_in.len() as i32,
            numOutputs: self.buses_out.len() as i32,
            inputs: self.buses_in.as_mut_ptr(),
            outputs: self.buses_out.as_mut_ptr(),
            inputParameterChanges: changes_in,
            outputParameterChanges: changes_out,
            inputEvents: events,
            outputEvents: std::ptr::null_mut(),
            processContext: &mut self.context,
        };
        // SAFETY: the processor is live. Every pointer in `data` is to something that lives through the call: the buffers (the caller's, or ours), the lists of buses and of
        // pointers (fields of this copy, not touched meanwhile), and the objects for the changes and events (held by this copy). The plugin only reads the inputs.
        let result = unsafe { self.processor.process(&mut data) };
        self.time_in_samples += i64::from(frames_in_a_number);
        if result != kResultOk && result != kResultTrue {
            return Err(RunError::ProcessFailed(result));
        }
        for (id, value) in self.changes_out.last_values() {
            match self.outputs.iter_mut().find(|(known, _)| *known == id) {
                Some(known) => known.1 = value,
                None => self.outputs.push((id, value)),
            }
        }
        Ok(())
    }

    /// Puts a note in the list of events, or in the changes of parameters for the next block. Says whether the plugin has to be made to forget its voices (an "all off" that no
    /// controller of the plugin takes).
    fn add_a_note(&mut self, note: NoteEvent, frames: i32) -> bool {
        let time = i32::try_from(note.time)
            .unwrap_or(i32::MAX)
            .clamp(0, (frames - 1).max(0));
        match note.what {
            NoteEventKind::On {
                channel,
                key,
                velocity,
                id,
            } => {
                let mut event = zeroed_event();
                event.sampleOffset = time;
                event.r#type = Event_::EventTypes_::kNoteOnEvent as _;
                event.__field0.noteOn = NoteOnEvent {
                    channel: i16::from(channel),
                    pitch: i16::from(key),
                    tuning: 0.0,
                    velocity,
                    length: 0,
                    noteId: i32::try_from(id).unwrap_or(i32::MAX),
                };
                self.events.push(event);
            }
            NoteEventKind::Off {
                channel,
                key,
                velocity,
                id,
            } => {
                let mut event = zeroed_event();
                event.sampleOffset = time;
                event.r#type = Event_::EventTypes_::kNoteOffEvent as _;
                event.__field0.noteOff = NoteOffEvent {
                    channel: i16::from(channel),
                    pitch: i16::from(key),
                    velocity,
                    noteId: i32::try_from(id).unwrap_or(i32::MAX),
                    tuning: 0.0,
                };
                self.events.push(event);
            }
            NoteEventKind::Controller { channel, number, value } => {
                self.push_a_controller(channel, i16::from(number), f64::from(value) / 127.0, time);
            }
            NoteEventKind::PitchBend { channel, value } => {
                self.push_a_controller(
                    channel,
                    CONTROLLER_PITCH_BEND,
                    f64::from(value.min(16383)) / 16383.0,
                    time,
                );
            }
            NoteEventKind::AllOff => {
                if !self.push_a_controller(0, CONTROLLER_ALL_NOTES_OFF, 1.0, time) {
                    return true;
                }
            }
        }
        false
    }

    /// Gives the parameter that the controller of the plugin maps the MIDI controller `number` of `channel` to the value, if it maps it; says whether it did.
    fn push_a_controller(&mut self, channel: u8, number: i16, value: f64, time: i32) -> bool {
        let key = (i16::from(channel) << 8) | number;
        let mapping = &self.mapping;
        let found = *self.mapped.entry(key).or_insert_with(|| {
            let mapping = mapping.as_ref()?;
            let mut id: ParamID = 0;
            // SAFETY: the mapping is live; the id lives through the call, which fills it.
            let result = unsafe { mapping.getMidiControllerAssignment(0, i16::from(channel), number, &mut id) };
            (result == kResultTrue).then_some(id)
        });
        match found {
            Some(id) => {
                self.pending.push((id, time, value));
                true
            }
            None => {
                self.unmapped += 1;
                false
            }
        }
    }

    /// How many controllers and pitch wheel positions were left out since the last time this was asked, for want of a parameter to give them to.
    pub fn take_the_count_of_events_left_out(&mut self) -> u32 {
        std::mem::take(&mut self.unmapped)
    }

    /// What the plugin's outputs say (a read-only parameter such as a meter), as `(parameter, plain value)`.
    pub fn control_outputs(&self) -> Vec<(u32, f64)> {
        self.outputs
            .iter()
            .map(|(id, value)| (*id, self.plain_of(*id, *value)))
            .collect()
    }

    /// Makes the plugin forget what it has heard. **The values it was given are kept**, those it has had in a block and those that wait for the next one.
    pub fn reset(&mut self) {
        // SAFETY: the processor and the component are live; the calls have numbers only. (Switching a plugin off and on is how VST3 has a host make it forget.)
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            self.component.setActive(1);
            self.processor.setProcessing(1);
        }
        self.time_in_samples = 0;
    }

    /// What the plugin asked of the host since this was last asked: its own edits (which are given to the next block as well), a change of latency, a wish to be made again.
    pub fn between_blocks(&mut self) -> BetweenBlocks {
        let asked = self.host.take_what_was_asked();
        let mut result = BetweenBlocks::default();
        for (id, normalized) in asked.edits {
            self.pending.push((id, 0, normalized));
            result.edits.push((id, self.plain_of(id, normalized)));
        }
        let latency_changed: i32 = RestartFlags_::kLatencyChanged as _;
        let reload_component: i32 = RestartFlags_::kReloadComponent as _;
        let io_changed: i32 = RestartFlags_::kIoChanged as _;
        if asked.restart_flags & latency_changed != 0 {
            self.latency = self.read_latency();
            result.latency_changed = true;
        }
        result.wants_to_be_made_again = asked.restart_flags & (reload_component | io_changed) != 0;
        let values_changed: i32 = RestartFlags_::kParamValuesChanged as _;
        result.values_changed = asked.restart_flags & values_changed != 0;
        result
    }

    /// The bytes of the plugin's state: its component's, and its controller's if it has one (see [`encode_state`]).
    pub fn save_state(&self) -> Result<Vec<u8>, StateError> {
        let stream = MemoryStream::new(Vec::new());
        let pointer = stream.to_com_ptr::<IBStream>().ok_or(StateError::NotOurs)?;
        // SAFETY: the component is live; the stream lives through the call.
        let result = unsafe { self.component.getState(pointer.as_ptr()) };
        if result != kResultOk {
            return Err(StateError::CouldNotBeSaved(format!("it said {result:#x}")));
        }
        let component = stream.bytes();
        let mut controller = Vec::new();
        if let Some(found) = &self.controller {
            let stream = MemoryStream::new(Vec::new());
            let pointer = stream.to_com_ptr::<IBStream>().ok_or(StateError::NotOurs)?;
            // SAFETY: the controller is live; the stream lives through the call. (A controller with no state of its own says so, and is left with none.)
            if unsafe { found.getState(pointer.as_ptr()) } == kResultOk {
                controller = stream.bytes();
            }
        }
        Ok(encode_state(&component, &controller))
    }

    /// Gives the plugin the state that [`Self::save_state`] made.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), StateError> {
        let (component, controller) = decode_state(bytes)?;
        let stream = MemoryStream::new(component.to_vec());
        let pointer = stream.to_com_ptr::<IBStream>().ok_or(StateError::NotOurs)?;
        // SAFETY: the component is live; the stream lives through the call.
        let result = unsafe { self.component.setState(pointer.as_ptr()) };
        if result != kResultOk {
            return Err(StateError::NotTaken(format!("it said {result:#x}")));
        }
        if let Some(found) = &self.controller {
            stream.rewind();
            // SAFETY: the controller is live; the stream lives through the call. (A controller that does not take the component's state is left as it is: that is not a failure.)
            unsafe { found.setComponentState(pointer.as_ptr()) };
            if !controller.is_empty() {
                let own = MemoryStream::new(controller.to_vec());
                if let Some(own_pointer) = own.to_com_ptr::<IBStream>() {
                    // SAFETY: as above.
                    unsafe { found.setState(own_pointer.as_ptr()) };
                }
            }
        }
        Ok(())
    }
}

/// Gives the controller the state that the component has as it is made: **what a VST3 host does with a plugin it has just made**, so that the values the controller reports are the
/// component's. Without it the controller of Surge XT says that each of its 2,855 values is 0, and a host that gives those values back to the plugin silences it. A component that has
/// no state to give, or a controller that does not take it, is left as it is.
fn give_the_controller_the_state_of_the_component(
    component: &ComPtr<IComponent>,
    controller: &ComPtr<IEditController>,
) {
    let stream = MemoryStream::new(Vec::new());
    let Some(pointer) = stream.to_com_ptr::<IBStream>() else {
        return;
    };
    // SAFETY: the component is live; the stream lives through the call.
    if unsafe { component.getState(pointer.as_ptr()) } != kResultOk {
        return;
    }
    stream.rewind();
    // SAFETY: the controller is live; the stream lives through the call. (A controller that does not take it says so; that is not a failure.)
    unsafe { controller.setComponentState(pointer.as_ptr()) };
}

/// Ends a component that was initialized and will not be used, and its controller if it is a class of its own.
fn abandon(component: &ComPtr<IComponent>, controller: Option<&ComPtr<IEditController>>) {
    // SAFETY: both are live and were initialized; ending them is what VST3 says for letting one go that was never started.
    unsafe {
        if let Some(controller) = controller {
            if let (Some(a), Some(b)) = (
                component.cast::<IConnectionPoint>(),
                controller.cast::<IConnectionPoint>(),
            ) {
                a.disconnect(b.as_ptr());
                b.disconnect(a.as_ptr());
            }
            controller.terminate();
        }
        component.terminate();
    }
}

fn bus_buffers(channels: usize, pointers: &mut [*mut f32]) -> AudioBusBuffers {
    // SAFETY: `AudioBusBuffers` is a plain struct of integers and a union of pointers, for which all zeros is a value (no channels, no buffers).
    let mut bus: AudioBusBuffers = unsafe { std::mem::zeroed() };
    if channels > 0 {
        bus.numChannels = channels as i32;
        bus.__field0.channelBuffers32 = pointers.as_mut_ptr();
    }
    bus
}

fn zeroed_event() -> Event {
    // SAFETY: `Event` is a plain struct of integers, a float and a union of such structs, for which all zeros is a value.
    unsafe { std::mem::zeroed() }
}

fn unsafe_zeroed_context() -> ProcessContext {
    // SAFETY: `ProcessContext` is a plain struct of integers and floats, for which all zeros is a value.
    unsafe { std::mem::zeroed() }
}
