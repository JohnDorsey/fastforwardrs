// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

//! A CLAP plugin library written for the tests of fastforwardrs (the plugins of the CI), in the way `ladspa-test-plugins` is for LADSPA.
//!
//! Every plugin has a different shape, so that a host that confuses one thing for another shows:
//!
//! | id | what it has |
//! |---|---|
//! | `org.fastforwardrs.test.gain` | stereo in and out, one parameter (`Gain`, 0 to 4, default 1, id 1), applied **at the sample an event says** |
//! | `org.fastforwardrs.test.swap` | stereo in and out, no parameters: left and right swapped |
//! | `org.fastforwardrs.test.mono_half` | one channel in and out ("mono"), no parameters: the sound times a half |
//! | `org.fastforwardrs.test.latency` | stereo; says it has a latency of 64 samples and delays the sound by 64 samples |
//! | `org.fastforwardrs.test.tail` | stereo; says it has a tail of 1000 samples; an echo that rings |
//! | `org.fastforwardrs.test.choice` | stereo; a stepped parameter (`Steps`, 0 to 3, id 1) and an enum (`Mode`, 0 to 2, id 2, with texts); the sound times (steps + 1) times (mode + 1) |
//! | `org.fastforwardrs.test.instrument` | no audio input (a generator), a note port: not an effect |
//! | `org.fastforwardrs.test.threads` | stereo; **checks the thread it is called on** (the host's `thread-check`) and gives the sound as it is if every call was on the right thread, and with the sign changed if one was not |
//!
//! | `…fails_to_activate`, `…fails_to_process`, `…panics` | stereo; misbehave on purpose: `activate` says no, `process` says it failed, `process` panics |
//!
//! It does nothing but arithmetic.

use std::ffi::CStr;
use std::sync::atomic::{AtomicU64, Ordering};

use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts, PluginAudioPortsImpl,
};
use clack_extensions::latency::{HostLatency, PluginLatency, PluginLatencyImpl};
use clack_extensions::params::{HostParams, ParamRescanFlags};
use clack_extensions::params::{
    ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter, PluginAudioProcessorParams, PluginMainThreadParams,
    PluginParams,
};
use clack_extensions::tail::{PluginTail, PluginTailImpl, TailLength};
use clack_extensions::thread_check::HostThreadCheck;
use clack_plugin::clack_export_entry;
use clack_plugin::entry::prelude::*;
use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::factory::plugin::PluginFactoryWrapper;
use clack_plugin::prelude::*;

/// The ids of the plugins.
pub const GAIN: &str = "org.fastforwardrs.test.gain";
pub const SWAP: &str = "org.fastforwardrs.test.swap";
pub const MONO_HALF: &str = "org.fastforwardrs.test.mono_half";
pub const LATENCY: &str = "org.fastforwardrs.test.latency";
pub const TAIL: &str = "org.fastforwardrs.test.tail";
pub const CHOICE: &str = "org.fastforwardrs.test.choice";
pub const INSTRUMENT: &str = "org.fastforwardrs.test.instrument";
pub const THREADS: &str = "org.fastforwardrs.test.threads";
/// Plugins that misbehave, for the tests of what a host does about it: `activate` says no, `process` says it failed, `process` panics.
pub const FAILS_TO_ACTIVATE: &str = "org.fastforwardrs.test.fails_to_activate";
pub const FAILS_TO_PROCESS: &str = "org.fastforwardrs.test.fails_to_process";
pub const PANICS: &str = "org.fastforwardrs.test.panics";
/// Stereo, with the parameter `Gain` of the gain plugin; at its first block it asks the host for a restart and for a call on the main thread, and in that call it says that its parameters changed.
pub const CALLBACKS: &str = "org.fastforwardrs.test.callbacks";
/// A stereo main input and a second input (a side chain); the sound is the main input plus the first channel of the side chain.
pub const SIDE_CHAIN: &str = "org.fastforwardrs.test.side_chain";
/// Like `tail`, and says its tail is for ever.
pub const TAIL_FOREVER: &str = "org.fastforwardrs.test.tail_forever";
/// The sound times the sample rate it was activated at, over 1000.
pub const RATE: &str = "org.fastforwardrs.test.rate";
/// Like `latency`, and at its first block it asks for a call on the main thread, in which it says that its latency changed (so the host makes it again).
pub const LATENCY_CHANGES: &str = "org.fastforwardrs.test.latency_changes";
/// A plugin that does not allow two of itself to be alive at once (as a plugin may that keeps something for all its instances): **one that is activated while another is alive gives its
/// sound with the sign changed**, for as long as it lives. (Found with a reverb that began to roar when a new instance was made while the one before was still being let go of.)
pub const SINGLE_INSTANCE: &str = "org.fastforwardrs.test.single_instance";
/// A plugin with a bug that real plugins have: **it reads memory it never wrote** (when it is activated it adds up bytes of buffers it has just been given and not filled), and gives
/// the sound times a gain that is made from the sum. In memory that is cleared the gain is 1; in memory that holds other bytes it is not. (Found with a reverb that roared, warbled and went
/// silent at random; the check of a plugin's start is tested with this.)
pub const READS_GARBAGE: &str = "org.fastforwardrs.test.reads_garbage";

/// What the library is asked to do wrong, if anything: the value of the environment variable `FFRS_TEST_PLUGIN`, read once, when the library is first asked. It is for the
/// tests of the scan, which run the scan program (a process of its own) with the variable set; **in a test program the variable is never set**.
///
/// `crash_when_loaded`, `hang_when_loaded`, `init_says_no` (the entry's `init` returns false), `crash_when_described`, `crash_when_made`, `hang_when_made`, `crash_when_activated`,
/// `crash_when_run`, `hang_when_run`, `exit_when_run` (ends the program with no answer), `answer_wrong_when_run` (writes a line that is not an answer, and ends the program),
/// `print_noise` (writes to the standard output while it is loaded and while it is made), `no_plugins` (a library with none).
fn misbehaviour() -> &'static str {
    static HOW: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HOW.get_or_init(|| std::env::var("FFRS_TEST_PLUGIN").unwrap_or_default())
}

/// Does what the misbehaviour `how` says, if that is the one that was asked for,: crashes, never returns, or ends the program.
fn misbehave_if_asked(how: &str) {
    if misbehaviour() != how {
        return;
    }
    if how.starts_with("crash") {
        std::process::abort();
    } else if how.starts_with("hang") {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    } else if how.starts_with("exit") {
        std::process::exit(0);
    } else if how.starts_with("answer_wrong") {
        println!("\nffrs-scan-result: {{this is not an answer");
        std::process::exit(0);
    }
}

/// How many samples of latency the plugin `latency` says it has (and has).
pub const LATENCY_SAMPLES: u32 = 64;
/// How many samples of tail the plugin `tail` says it has.
pub const TAIL_SAMPLES: u32 = 1000;
/// How far apart the echoes of the plugin `tail` are.
pub const ECHO_SAMPLES: usize = 100;

const KIND_GAIN: u8 = 0;
const KIND_SWAP: u8 = 1;
const KIND_MONO_HALF: u8 = 2;
const KIND_LATENCY: u8 = 3;
const KIND_TAIL: u8 = 4;
const KIND_CHOICE: u8 = 5;
const KIND_INSTRUMENT: u8 = 6;
const KIND_THREADS: u8 = 7;
const KIND_FAILS_TO_ACTIVATE: u8 = 8;
const KIND_FAILS_TO_PROCESS: u8 = 9;
const KIND_PANICS: u8 = 10;
const KIND_CALLBACKS: u8 = 11;
const KIND_SIDE_CHAIN: u8 = 12;
const KIND_TAIL_FOREVER: u8 = 13;
const KIND_RATE: u8 = 14;
const KIND_LATENCY_CHANGES: u8 = 15;
const KIND_SINGLE_INSTANCE: u8 = 16;
const KIND_READS_GARBAGE: u8 = 17;

/// How many `single_instance` plugins are alive.
static SINGLE_INSTANCES_ALIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The ids of the plugins, in the order of the kinds.
const IDS: [&str; 18] = [
    GAIN,
    SWAP,
    MONO_HALF,
    LATENCY,
    TAIL,
    CHOICE,
    INSTRUMENT,
    THREADS,
    FAILS_TO_ACTIVATE,
    FAILS_TO_PROCESS,
    PANICS,
    CALLBACKS,
    SIDE_CHAIN,
    TAIL_FOREVER,
    RATE,
    LATENCY_CHANGES,
    SINGLE_INSTANCE,
    READS_GARBAGE,
];
const NAMES: [&str; 18] = [
    "Gain",
    "Swap",
    "Mono half",
    "Latency",
    "Tail",
    "Choice",
    "Instrument",
    "Threads",
    "Fails to activate",
    "Fails to process",
    "Panics",
    "Callbacks",
    "Side chain",
    "Tail forever",
    "Rate",
    "Latency changes",
    "Single instance",
    "Reads garbage",
];

/// What the parameters of a plugin are, as `(id, name, minimum, maximum, default, flags)`.
fn parameters_of(kind: u8) -> &'static [(u32, &'static str, f64, f64, f64, ParamInfoFlags)] {
    const GAIN_PARAMETER: [(u32, &str, f64, f64, f64, ParamInfoFlags); 1] =
        [(1, "Gain", 0.0, 4.0, 1.0, ParamInfoFlags::IS_AUTOMATABLE)];
    const CHOICE_PARAMETERS: [(u32, &str, f64, f64, f64, ParamInfoFlags); 2] = [
        (
            1,
            "Steps",
            0.0,
            3.0,
            0.0,
            ParamInfoFlags::IS_AUTOMATABLE.union(ParamInfoFlags::IS_STEPPED),
        ),
        (
            2,
            "Mode",
            0.0,
            2.0,
            0.0,
            ParamInfoFlags::IS_AUTOMATABLE
                .union(ParamInfoFlags::IS_STEPPED)
                .union(ParamInfoFlags::IS_ENUM),
        ),
    ];
    match kind {
        KIND_GAIN | KIND_CALLBACKS => &GAIN_PARAMETER,
        KIND_CHOICE => &CHOICE_PARAMETERS,
        _ => &[],
    }
}

/// The texts of the enum parameter `Mode`.
const MODE_TEXTS: [&str; 3] = ["off", "double", "triple"];

/// What the main thread and the audio thread of one plugin share.
pub struct Shared {
    /// The values of the parameters (as the bits of an `f64`), by the order of `parameters_of`.
    values: [AtomicU64; 2],
}

impl Shared {
    fn new(kind: u8) -> Self {
        let defaults = parameters_of(kind);
        let value = |number: usize| AtomicU64::new(defaults.get(number).map_or(0.0, |parameter| parameter.4).to_bits());
        Self {
            values: [value(0), value(1)],
        }
    }

    fn get(&self, number: usize) -> f64 {
        f64::from_bits(self.values[number].load(Ordering::SeqCst))
    }

    fn set(&self, number: usize, value: f64) {
        self.values[number].store(value.to_bits(), Ordering::SeqCst);
    }
}

/// The plugin of the kind `KIND`.
pub struct TestPlugin<const KIND: u8>;

pub struct Main<'a, const KIND: u8> {
    shared: &'a Shared,
    host: HostMainThreadHandle<'a>,
}

impl<const KIND: u8> Plugin for TestPlugin<KIND> {
    type AudioProcessor<'a> = Processor<'a, KIND>;
    type Shared<'a> = Shared;
    type MainThread<'a> = Main<'a, KIND>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Self::Shared<'_>>) {
        builder.register::<PluginAudioPorts>();
        if !parameters_of(KIND).is_empty() {
            builder.register::<PluginParams>();
        }
        if KIND == KIND_LATENCY || KIND == KIND_LATENCY_CHANGES {
            builder.register::<PluginLatency>();
        }
        if KIND == KIND_TAIL || KIND == KIND_TAIL_FOREVER {
            builder.register::<PluginTail>();
        }
    }
}

impl PluginShared<'_> for Shared {}

impl<'a, const KIND: u8> PluginMainThread<'a, Shared> for Main<'a, KIND> {
    fn on_main_thread(&self) {
        // (The call that the plugin asked for: it says that its parameters are not what they were.)
        if KIND == KIND_CALLBACKS
            && let Some(params) = self.host.get_extension::<HostParams>()
        {
            params.rescan(&self.host, ParamRescanFlags::ALL);
        }
        // (The `latency_changes` plugin says, in the call it asked for, that its latency changed.)
        if KIND == KIND_LATENCY_CHANGES
            && let Some(latency) = self.host.get_extension::<HostLatency>()
        {
            latency.changed(&self.host);
        }
    }
}

impl<const KIND: u8> PluginAudioPortsImpl for Main<'_, KIND> {
    fn count(&self, is_input: bool) -> u32 {
        match (is_input, KIND) {
            (true, KIND_INSTRUMENT) => 0,
            (true, KIND_SIDE_CHAIN) => 2,
            _ => 1,
        }
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if (is_input && KIND == KIND_INSTRUMENT) || index > u32::from(is_input && KIND == KIND_SIDE_CHAIN) {
            return;
        }
        let is_side_chain = index == 1;
        let mono = KIND == KIND_MONO_HALF;
        writer.set(&AudioPortInfo {
            id: ClapId::new(match (is_input, is_side_chain) {
                (true, false) => 0,
                (false, _) => 1,
                (true, true) => 2,
            }),
            name: match (is_input, is_side_chain) {
                (true, false) => b"Input",
                (false, _) => b"Output",
                (true, true) => b"Side chain",
            },
            channel_count: if mono { 1 } else { 2 },
            flags: if is_side_chain {
                AudioPortFlags::empty()
            } else {
                AudioPortFlags::IS_MAIN
            },
            port_type: Some(if mono {
                AudioPortType::MONO
            } else {
                AudioPortType::STEREO
            }),
            in_place_pair: None,
        });
    }
}

impl<const KIND: u8> PluginMainThreadParams for Main<'_, KIND> {
    fn count(&self) -> u32 {
        parameters_of(KIND).len() as u32
    }

    fn get_info(&self, param_index: u32, info: &mut ParamInfoWriter) {
        if let Some(&(id, name, minimum, maximum, default, flags)) = parameters_of(KIND).get(param_index as usize) {
            info.set(&ParamInfo {
                id: ClapId::new(id),
                flags,
                cookie: Default::default(),
                name: name.as_bytes(),
                module: b"",
                min_value: minimum,
                max_value: maximum,
                default_value: default,
            });
        }
    }

    fn get_value(&self, param_id: ClapId) -> Option<f64> {
        let number = parameters_of(KIND)
            .iter()
            .position(|parameter| parameter.0 == param_id.get())?;
        Some(self.shared.get(number))
    }

    fn value_to_text(&self, param_id: ClapId, value: f64, writer: &mut ParamDisplayWriter) -> core::fmt::Result {
        use core::fmt::Write;
        if KIND == KIND_CHOICE && param_id.get() == 2 {
            return writer.write_str(MODE_TEXTS.get(value as usize).copied().unwrap_or("?"));
        }
        write!(writer, "{value:.2}")
    }

    fn text_to_value(&self, _param_id: ClapId, text: &CStr) -> Option<f64> {
        text.to_str().ok()?.trim().parse().ok()
    }

    fn flush(&self, _input: &InputEvents, _output: &mut OutputEvents) {}
}

impl<const KIND: u8> PluginLatencyImpl for Main<'_, KIND> {
    fn get(&self) -> u32 {
        LATENCY_SAMPLES
    }
}

impl<const KIND: u8> PluginTailImpl for Processor<'_, KIND> {
    fn get(&self) -> TailLength {
        if KIND == KIND_TAIL_FOREVER {
            TailLength::Infinite
        } else {
            TailLength::Finite(TAIL_SAMPLES)
        }
    }
}

/// **A bug, on purpose:** takes many buffers of large sizes (like the delay lines of a reverb), looks at some words of each **without writing them first**, and makes a gain of 1 plus
/// a quarter for each word that is all ones (`0xFF` bytes) or all `0xCD` (what a debug allocator fills with): in memory that holds zeros there are none. Reading memory that was never
/// written is what the plugin that roared did.
#[allow(unsafe_code)]
fn gain_from_memory_it_did_not_write() -> f32 {
    use std::alloc::{Layout, alloc, dealloc};
    let mut buffers = Vec::new();
    let mut words = 0_u32;
    for number in 0..48_usize {
        let size = 65_536 + number * 7_000;
        let layout = Layout::from_size_align(size, 8).expect("a valid layout");
        // SAFETY: the layout has a size above zero. The words that are read were never written (the point of this plugin): they are read with `read_volatile`, so that the reads are
        // made, from places inside the buffer.
        unsafe {
            let buffer = alloc(layout);
            if buffer.is_null() {
                continue;
            }
            for place in [0, size / 4, size / 2, size * 3 / 4] {
                let word = std::ptr::read_volatile(buffer.add(place / 4 * 4).cast::<u32>());
                if word == u32::MAX || word == 0xCDCD_CDCD {
                    words += 1;
                }
            }
            buffers.push((buffer, layout));
        }
    }
    for (pointer, layout) in buffers {
        // SAFETY: each pointer was given by `alloc` with this layout, and is let go of once.
        unsafe { dealloc(pointer, layout) };
    }
    1.0 + words as f32 * 0.25
}

/// The audio thread of a plugin.
pub struct Processor<'a, const KIND: u8> {
    shared: &'a Shared,
    host: HostAudioProcessorHandle<'a>,
    /// Whether every thread check so far said what it should (`threads` only).
    threads_were_right: bool,
    /// Whether the plugin has asked the host for its restart and its call (`callbacks` only).
    asked: bool,
    /// Whether another `single_instance` was alive when this one was activated (`single_instance` only).
    crowded: bool,
    /// The gain that the bytes it read when it was activated made (`reads_garbage` only; 1 when they were all zero).
    garbage_gain: f32,
    /// The sample rate it was activated at.
    sample_rate: f64,
    /// The most frames it was activated for: a host that gives it more breaks the contract, and the plugin says so.
    most_frames: u32,
    /// The values of the parameters, as the last event that was applied says.
    values: [f64; 2],
    /// What the plugin has said, the last `LATENCY_SAMPLES` samples of each channel (`latency`), or the last `ECHO_SAMPLES` samples of what it gave (`tail`).
    memory: [Vec<f32>; 2],
    position: usize,
}

impl<'a, const KIND: u8> PluginAudioProcessor<'a, Shared, Main<'a, KIND>> for Processor<'a, KIND> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        _main_thread: &Main<'a, KIND>,
        shared: &'a Shared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let length = match KIND {
            KIND_LATENCY | KIND_LATENCY_CHANGES => LATENCY_SAMPLES as usize,
            KIND_TAIL => ECHO_SAMPLES,
            _ => 0,
        };
        misbehave_if_asked("crash_when_activated");
        if KIND == KIND_FAILS_TO_ACTIVATE {
            return Err(PluginError::Message("this plugin never activates"));
        }
        // `activate` is a call on the main thread, and not on the audio thread.
        let threads_were_right = KIND != KIND_THREADS || {
            let handle: &HostSharedHandle = &host;
            host.get_extension::<HostThreadCheck>().is_some_and(|check| {
                check.is_main_thread(handle) == Some(true) && check.is_audio_thread(handle) == Some(false)
            })
        };
        Ok(Self {
            shared,
            host,
            threads_were_right,
            asked: false,
            crowded: KIND == KIND_SINGLE_INSTANCE && SINGLE_INSTANCES_ALIVE.fetch_add(1, Ordering::SeqCst) > 0,
            garbage_gain: if KIND == KIND_READS_GARBAGE {
                gain_from_memory_it_did_not_write()
            } else {
                1.0
            },
            sample_rate: audio_config.sample_rate,
            most_frames: audio_config.max_frames_count,
            values: [shared.get(0), shared.get(1)],
            memory: [vec![0.0; length], vec![0.0; length]],
            position: 0,
        })
    }

    fn process(&mut self, _process: Process, mut audio: Audio, events: Events) -> Result<ProcessStatus, PluginError> {
        if audio.frames_count() > self.most_frames {
            return Err(PluginError::Message(
                "it was given more frames than it was activated for",
            ));
        }
        // The events are in order of time: the sound is made in stretches, each from one event to the next.
        for how in [
            "crash_when_run",
            "hang_when_run",
            "exit_when_run",
            "answer_wrong_when_run",
        ] {
            misbehave_if_asked(how);
        }
        if (KIND == KIND_CALLBACKS || KIND == KIND_LATENCY_CHANGES) && !self.asked {
            self.asked = true;
            if KIND == KIND_CALLBACKS {
                self.host.request_restart();
            }
            self.host.request_callback();
        }
        if KIND == KIND_FAILS_TO_PROCESS {
            return Err(PluginError::Message("this plugin never processes"));
        }
        if KIND == KIND_PANICS {
            panic!("this plugin panics when it processes");
        }
        if KIND == KIND_THREADS {
            // `process` is a call on the audio thread, and not on the main thread.
            let handle: &HostSharedHandle = &self.host;
            let right = self.host.get_extension::<HostThreadCheck>().is_some_and(|check| {
                check.is_audio_thread(handle) == Some(true) && check.is_main_thread(handle) == Some(false)
            });
            self.threads_were_right &= right;
        }
        let frames = audio.frames_count() as usize;
        let mut done = 0;
        let mut pending: Vec<(usize, u32, f64)> = Vec::new();
        for event in events.input {
            if let Some(CoreEventSpace::ParamValue(change)) = event.as_core_event()
                && let Some(id) = change.param_id()
            {
                pending.push((change.header().time() as usize, id.get(), change.value()));
            }
        }
        let mut next_event = 0;
        while done < frames {
            while next_event < pending.len() && pending[next_event].0 <= done {
                let (_, id, value) = pending[next_event];
                if let Some(number) = parameters_of(KIND).iter().position(|parameter| parameter.0 == id) {
                    self.values[number] = value;
                    self.shared.set(number, value);
                }
                next_event += 1;
            }
            let end = pending
                .get(next_event)
                .map_or(frames, |event| event.0.min(frames))
                .max(done + 1);
            self.process_stretch(&mut audio, done, end)?;
            done = end;
        }
        Ok(match KIND {
            KIND_TAIL => ProcessStatus::Tail,
            KIND_SWAP => ProcessStatus::ContinueIfNotQuiet,
            KIND_MONO_HALF => ProcessStatus::Sleep,
            _ => ProcessStatus::Continue,
        })
    }

    fn reset(&mut self) {
        for memory in &mut self.memory {
            memory.fill(0.0);
        }
        self.position = 0;
    }
}

impl<const KIND: u8> Drop for Processor<'_, KIND> {
    fn drop(&mut self) {
        if KIND == KIND_SINGLE_INSTANCE {
            SINGLE_INSTANCES_ALIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl<const KIND: u8> PluginAudioProcessorParams for Processor<'_, KIND> {
    fn flush(&mut self, _input: &InputEvents, _output: &mut OutputEvents) {}
}

impl<const KIND: u8> Processor<'_, KIND> {
    /// Makes the samples `from` up to `to` of every channel.
    fn process_stretch(&mut self, audio: &mut Audio, from: usize, to: usize) -> Result<(), PluginError> {
        let multiplier = match KIND {
            KIND_GAIN | KIND_CALLBACKS => self.values[0] as f32,
            KIND_MONO_HALF => 0.5,
            KIND_CHOICE => ((self.values[0] as i64 + 1) * (self.values[1] as i64 + 1)) as f32,
            KIND_THREADS if !self.threads_were_right => -1.0,
            KIND_SINGLE_INSTANCE if self.crowded => -1.0,
            KIND_READS_GARBAGE => self.garbage_gain,
            KIND_RATE => (self.sample_rate / 1000.0) as f32,
            _ => 1.0,
        };
        // The side chain (the first channel of an input that has no output beside it) is read first.
        let mut side = vec![0.0_f32; to - from];
        let mut side_channels = 0;
        if KIND == KIND_SIDE_CHAIN {
            for mut pair in &mut *audio {
                let Some(channels) = pair.channels()?.into_f32() else {
                    continue;
                };
                for channel in channels {
                    if let ChannelPair::InputOnly(input) = channel {
                        if side_channels == 0 {
                            side.copy_from_slice(&input[from..to]);
                        }
                        side_channels += 1;
                    }
                }
            }
            // (A host that gives fewer channels to the side chain than the port has is heard: a thousand is added.)
            if side_channels != 2 {
                side.fill(1000.0);
            }
        }
        for mut pair in &mut *audio {
            let Some(channels) = pair.channels()?.into_f32() else {
                continue;
            };
            let mut channel_number = 0;
            let mut inputs: Vec<Vec<f32>> = Vec::new();
            let mut outputs: Vec<&mut [f32]> = Vec::new();
            for channel in channels {
                match channel {
                    ChannelPair::InputOutput(input, output) => {
                        inputs.push(input[from..to].to_vec());
                        outputs.push(&mut output[from..to]);
                    }
                    ChannelPair::InPlace(buffer) => {
                        inputs.push(buffer[from..to].to_vec());
                        outputs.push(&mut buffer[from..to]);
                    }
                    ChannelPair::OutputOnly(buffer) => buffer[from..to].fill(0.0),
                    ChannelPair::InputOnly(_) => {}
                }
                channel_number += 1;
            }
            let _ = channel_number;
            self.make(&inputs, &mut outputs, multiplier, &side);
        }
        Ok(())
    }

    fn make(&mut self, inputs: &[Vec<f32>], outputs: &mut [&mut [f32]], multiplier: f32, side: &[f32]) {
        let channels = outputs.len().min(inputs.len());
        for moment in 0..outputs.first().map_or(0, |output| output.len()) {
            for channel in 0..channels {
                let heard = inputs[channel][moment];
                let source = match (KIND, channels) {
                    (KIND_SWAP, 2) => inputs[1 - channel][moment],
                    _ => heard,
                };
                let made = match KIND {
                    KIND_LATENCY | KIND_LATENCY_CHANGES => {
                        let slot = (self.position + moment) % LATENCY_SAMPLES as usize;
                        let earlier = self.memory[channel][slot];
                        self.memory[channel][slot] = source;
                        earlier
                    }
                    KIND_TAIL => {
                        let slot = (self.position + moment) % ECHO_SAMPLES;
                        let made = source + 0.5 * self.memory[channel][slot];
                        self.memory[channel][slot] = made;
                        made
                    }
                    KIND_SIDE_CHAIN => source + side[moment],
                    _ => source * multiplier,
                };
                outputs[channel][moment] = made;
            }
        }
        self.position += outputs.first().map_or(0, |output| output.len());
    }
}

/// The plugins of the library.
struct Factory {
    descriptors: Vec<PluginDescriptor>,
}

impl PluginFactoryImpl for Factory {
    fn plugin_count(&self) -> u32 {
        if misbehaviour() == "no_plugins" {
            0
        } else {
            self.descriptors.len() as u32
        }
    }

    fn plugin_descriptor(&self, index: u32) -> Option<&PluginDescriptor> {
        misbehave_if_asked("crash_when_described");
        self.descriptors.get(index as usize)
    }

    fn create_plugin<'a>(&'a self, host_info: HostInfo<'a>, plugin_id: &CStr) -> Option<PluginInstance<'a>> {
        misbehave_if_asked("crash_when_made");
        misbehave_if_asked("hang_when_made");
        if misbehaviour() == "print_noise" {
            println!("noise from a plugin that is being made");
        }
        let number = IDS.iter().position(|id| plugin_id.to_str() == Ok(*id))?;
        let descriptor = &self.descriptors[number];
        macro_rules! made {
            ($kind:expr) => {
                Some(PluginInstance::new::<TestPlugin<{ $kind }>>(
                    host_info,
                    descriptor,
                    |_| Ok(Shared::new($kind)),
                    |host, shared| Ok(Main { shared, host }),
                ))
            };
        }
        match number as u8 {
            KIND_GAIN => made!(KIND_GAIN),
            KIND_SWAP => made!(KIND_SWAP),
            KIND_MONO_HALF => made!(KIND_MONO_HALF),
            KIND_LATENCY => made!(KIND_LATENCY),
            KIND_LATENCY_CHANGES => made!(KIND_LATENCY_CHANGES),
            KIND_SINGLE_INSTANCE => made!(KIND_SINGLE_INSTANCE),
            KIND_READS_GARBAGE => made!(KIND_READS_GARBAGE),
            KIND_TAIL => made!(KIND_TAIL),
            KIND_CHOICE => made!(KIND_CHOICE),
            KIND_INSTRUMENT => made!(KIND_INSTRUMENT),
            KIND_THREADS => made!(KIND_THREADS),
            KIND_FAILS_TO_ACTIVATE => made!(KIND_FAILS_TO_ACTIVATE),
            KIND_FAILS_TO_PROCESS => made!(KIND_FAILS_TO_PROCESS),
            KIND_PANICS => made!(KIND_PANICS),
            KIND_CALLBACKS => made!(KIND_CALLBACKS),
            KIND_SIDE_CHAIN => made!(KIND_SIDE_CHAIN),
            KIND_TAIL_FOREVER => made!(KIND_TAIL_FOREVER),
            KIND_RATE => made!(KIND_RATE),
            _ => None,
        }
    }
}

pub struct TestEntry {
    factory: PluginFactoryWrapper<Factory>,
}

impl Entry for TestEntry {
    fn new(_plugin_path: Option<&CStr>) -> Result<Self, EntryLoadError> {
        misbehave_if_asked("crash_when_loaded");
        misbehave_if_asked("hang_when_loaded");
        if misbehaviour() == "init_says_no" {
            return Err(EntryLoadError);
        }
        if misbehaviour() == "print_noise" {
            println!("noise from a library that is being loaded");
        }
        let descriptors = IDS
            .iter()
            .zip(NAMES)
            .map(|(id, name)| {
                PluginDescriptor::new(id, name)
                    .with_vendor("fastforwardrs")
                    .with_description("A plugin written for the tests")
            })
            .collect();
        Ok(Self {
            factory: PluginFactoryWrapper::new(Factory { descriptors }),
        })
    }

    fn declare_factories<'a>(&'a self, builder: &mut EntryFactories<'a>) {
        builder.register_factory(&self.factory);
    }
}

clack_export_entry!(TestEntry);

/// Where the file of this library is, **for a test (or any program) that was built by the same `cargo` run as this crate's library** and depends on this crate: `cargo`
/// builds the library beside the program, in the same folder. (Nothing is built by this function.)
pub fn library_file_for_tests() -> std::path::PathBuf {
    let program = std::env::current_exe().expect("a program knows where it is");
    let name = format!(
        "{}clap_test_plugins{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let file = program.parent().expect("a program is in a folder").join(name);
    assert!(
        file.is_file(),
        "the library of the test plugins is not at {} (the program that looks for it has to have the crate clap-test-plugins as a dependency)",
        file.display()
    );
    file
}
