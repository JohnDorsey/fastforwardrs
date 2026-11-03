// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

//! A VST3 plugin library written for the tests of fastforwardrs (the plugins of the CI), in the way `clap-test-plugins` and `lv2-test-plugins` are for CLAP and LV2.
//!
//! Every plugin has a different shape, so that a host that confuses one thing for another shows. All of them do nothing but arithmetic. **They are strict**, as some real plugins
//! are: a plugin of two classes gives nothing until its halves are connected (`process` fails, and the controller's values are not numbers); the main output has to be switched on;
//! a bus that is switched on has to be given its channels and one that is switched off none; the points of a parameter have to be in order of the sample.
//!
//! | class | what it is |
//! |---|---|
//! | [`GAIN`] | **an effect in two classes** (a component and a controller, which the host has to connect), stereo in and out. Parameters: `Gain` (1: plain 0 to 4, default 1, automatable, **applied at the sample a change is given**), `Mode` (2: a list of three, `Plain`, `Double`, `Triple`: the sound times 1, 2 or 3), `Secret` (3: hidden), `Level` (4: **read-only**, the peak of the last block, given back as an output parameter change), `Bypass` (5: passes the sound as it is). Its state is 4 + 4 * 8 bytes; **its controller has no state of its own** (`getState` says not implemented, as real plugins do) |
//! | [`LATENCY`] | a stereo effect with **no controller at all**: it delays the sound by 64 samples, says so, and says it has a tail of 1,000 samples |
//! | [`INSTRUMENT`] | a synthesizer (no audio input, one stereo output, one event input of 16 channels): **while notes are held, every sample of both channels is the sum over the held notes of `velocity * pitch / 100`**, plus `(Bend - 0.5) * 0.5`, plus `Wheel * 0.25` while any note is held. A note off silences its note at once. Its controller has `IMidiMapping`: pitch bend (129) is `Bend` (20), controller 1 is `Wheel` (21), controller 123 is `AllNotesOff` (22: any value from one half up silences every note) |
//! | [`SINGLE`] | an effect whose **component is its own controller** (one object): one parameter, `Gain` (1), the sound times the gain |
//! | [`SIDE_CHAIN_EFFECT`] | an effect with **two audio inputs, the side chain first and the main one second** (as Surge XT Effects has them, the other way round): the sound is the main input times a half. A host has to find the main one by its type, not by its place |
//! | [`SIDE_CHAIN_INSTRUMENT`] | the instrument as Surge XT has it: its only audio input is a **side-chain** (`kAux`), it has two more outputs of the same kind, and **it applies its state in the first block that follows, and a note in that block makes `process` fail** (the VST3 build of Surge XT hangs like that for some patches); a block with no events before the first note does not hurt. The state is text, of which `edits=N` is what it keeps: each edit makes the sound half as loud again |
//! | [`NO_DEFAULTS`] | the gain effect whose **controller says that the default of every parameter is 0**, as the controller of Surge XT does (and, like Surge's, says that every value is 0 until it is given the component's state) |
//! | [`COMPATIBILITY`] | a class of the category `Plugin Compatibility Class`, which a host does not make |
//! | [`FAILS_TO_INITIALIZE`], [`FAILS_TO_ACTIVATE`], [`FAILS_TO_PROCESS`], [`NOT_NUMBERS`] | the gain effect, which misbehaves on purpose: `initialize` says no, `setActive` says no, `process` says it failed, the sound is not made of numbers |

#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    clippy::missing_safety_doc
)]

use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::{ptr, slice};

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComPtr, ComRef, ComWrapper, uid};

// ---------------------------------------------------------------------------------------------------------------------
// The names. A class id is four numbers (the way Steinberg writes them); the host keeps them in that form.

pub const GAIN: [u32; 4] = [0x5445_5354, 0x4741_494E, 0x0000_0001, 0x0000_0001];
pub const GAIN_CONTROLLER: [u32; 4] = [0x5445_5354, 0x4741_494E, 0x0000_0001, 0x0000_0002];
pub const LATENCY: [u32; 4] = [0x5445_5354, 0x4C41_5445, 0x0000_0001, 0x0000_0001];
pub const INSTRUMENT: [u32; 4] = [0x5445_5354, 0x494E_5354, 0x0000_0001, 0x0000_0001];
pub const INSTRUMENT_CONTROLLER: [u32; 4] = [0x5445_5354, 0x494E_5354, 0x0000_0001, 0x0000_0002];
pub const SINGLE: [u32; 4] = [0x5445_5354, 0x534E_474C, 0x0000_0001, 0x0000_0001];
pub const SIDE_CHAIN_EFFECT: [u32; 4] = [0x5445_5354, 0x5345_4646, 0x0000_0001, 0x0000_0001];
pub const SIDE_CHAIN_INSTRUMENT: [u32; 4] = [0x5445_5354, 0x5349_4443, 0x0000_0001, 0x0000_0001];
pub const SIDE_CHAIN_INSTRUMENT_CONTROLLER: [u32; 4] = [0x5445_5354, 0x5349_4443, 0x0000_0001, 0x0000_0002];
pub const COMPATIBILITY: [u32; 4] = [0x5445_5354, 0x434F_4D50, 0x0000_0001, 0x0000_0001];
pub const FAILS_TO_INITIALIZE: [u32; 4] = [0x5445_5354, 0x4641_494C, 0x0000_0001, 0x0000_0001];
pub const FAILS_TO_ACTIVATE: [u32; 4] = [0x5445_5354, 0x4641_494C, 0x0000_0002, 0x0000_0001];
pub const FAILS_TO_PROCESS: [u32; 4] = [0x5445_5354, 0x4641_494C, 0x0000_0003, 0x0000_0001];
pub const NOT_NUMBERS: [u32; 4] = [0x5445_5354, 0x4641_494C, 0x0000_0004, 0x0000_0001];
pub const NO_DEFAULTS: [u32; 4] = [0x5445_5354, 0x4E4F_4446, 0x0000_0001, 0x0000_0001];
pub const NO_DEFAULTS_CONTROLLER: [u32; 4] = [0x5445_5354, 0x4E4F_4446, 0x0000_0001, 0x0000_0002];

/// The ids of the parameters.
pub const PARAMETER_GAIN: u32 = 1;
pub const PARAMETER_MODE: u32 = 2;
pub const PARAMETER_SECRET: u32 = 3;
pub const PARAMETER_LEVEL: u32 = 4;
pub const PARAMETER_BYPASS: u32 = 5;
pub const PARAMETER_BEND: u32 = 20;
pub const PARAMETER_WHEEL: u32 = 21;
pub const PARAMETER_ALL_NOTES_OFF: u32 = 22;

/// The delay of [`LATENCY`], in samples, and the tail it says it has.
pub const LATENCY_SAMPLES: usize = 64;
pub const TAIL_SAMPLES: u32 = 1000;

/// The start of the state of [`GAIN`] and [`SINGLE`].
const STATE_MAGIC: &[u8; 4] = b"TGN1";

// ---------------------------------------------------------------------------------------------------------------------
// Misbehaving on request, as the other test libraries do.

/// What the library is asked to do wrong, if anything: the value of the environment variable `FFRS_TEST_PLUGIN`, read once, when the library is first asked. It is for the tests of
/// the program that runs plugins, which start it with the variable set; **in a test program the variable is never set**.
///
/// `crash_when_loaded`, `hang_when_loaded`, `init_says_no` (the module's start-up function returns false), `crash_when_made`, `hang_when_made` (the factory is asked for a class),
/// `crash_when_activated`, `crash_when_run`, `hang_when_run`, `exit_when_run` (ends the program with no answer), `crash_at_every_fourth_run` (a program that counts its runs and
/// crashes at the fourth, so a program that is started again plays three runs and crashes again: for the tests of a copy that is started again), `print_noise` (writes to the
/// standard output while it is loaded and while it is made).
fn misbehaviour() -> &'static str {
    static HOW: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HOW.get_or_init(|| std::env::var("FFRS_TEST_PLUGIN").unwrap_or_default())
}

/// Does what the misbehaviour `how` says, if that is the one that was asked for: crashes, never returns, or ends the program.
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
    }
}

/// Crashes at every fourth run of the program (`crash_at_every_fourth_run`), if that is what was asked for.
fn crash_at_every_fourth_run_if_asked() {
    static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    if misbehaviour() == "crash_at_every_fourth_run" && RUNS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) % 4 == 3
    {
        std::process::abort();
    }
}

/// What the module does when it is started: misbehaves as asked, and says whether it started.
fn start_up() -> bool {
    misbehave_if_asked("crash_when_loaded");
    misbehave_if_asked("hang_when_loaded");
    if misbehaviour() == "print_noise" {
        println!("noise from the plugin module, while it is loaded");
    }
    misbehaviour() != "init_says_no"
}

// ---------------------------------------------------------------------------------------------------------------------
// Text, as the interfaces want it.

fn copy_cstring(source: &str, destination: &mut [c_char]) {
    for (place, byte) in destination.iter_mut().enumerate() {
        *byte = source.as_bytes().get(place).map_or(0, |byte| *byte as c_char);
    }
    if let Some(last) = destination.last_mut() {
        *last = 0;
    }
}

fn copy_wstring(source: &str, destination: &mut [TChar]) {
    let mut units = source.encode_utf16();
    for place in destination.iter_mut() {
        *place = units.next().unwrap_or(0);
    }
    if let Some(last) = destination.last_mut() {
        *last = 0;
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// What a plugin is.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Gain,
    Latency,
    Instrument,
    SideChainInstrument,
    /// An effect whose **first** input is a side chain and whose second is the main one (so that a host that takes the first input for the main one is heard).
    SideChainEffect,
    Single,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Initialize,
    Activate,
    Process,
    NotNumbers,
    /// Not a fault of the plugin: its controller does not say what the defaults of its parameters are (it says 0 for each), as the controller of Surge XT does.
    NoDefaults,
}

impl Kind {
    fn takes_notes(self) -> bool {
        matches!(self, Self::Instrument | Self::SideChainInstrument)
    }

    /// Whether the plugin has a controller of a class of its own, which the host has to connect to it.
    fn has_a_separate_controller(self) -> bool {
        matches!(self, Self::Gain | Self::Instrument | Self::SideChainInstrument)
    }

    /// The parameters a plugin of this kind has, in order.
    fn parameters(self) -> &'static [u32] {
        match self {
            Self::Gain => &[
                PARAMETER_GAIN,
                PARAMETER_MODE,
                PARAMETER_SECRET,
                PARAMETER_LEVEL,
                PARAMETER_BYPASS,
            ],
            Self::Single => &[PARAMETER_GAIN],
            Self::Instrument => &[PARAMETER_BEND, PARAMETER_WHEEL, PARAMETER_ALL_NOTES_OFF],
            Self::Latency | Self::SideChainInstrument | Self::SideChainEffect => &[],
        }
    }
}

/// A constant of one of the enums of the bindings, which are `i32` on Windows and `u32` elsewhere, as the `i32` that a field wants.
fn flag(value: impl TryInto<i32>) -> i32 {
    value.try_into().unwrap_or(0)
}

fn default_normalized(id: u32) -> f64 {
    match id {
        PARAMETER_GAIN => 0.25,
        PARAMETER_BEND => 0.5,
        _ => 0.0,
    }
}

fn plain_of(id: u32, normalized: f64) -> f64 {
    match id {
        PARAMETER_GAIN => normalized * 4.0,
        PARAMETER_MODE => (normalized * 2.0).round(),
        PARAMETER_BYPASS | PARAMETER_ALL_NOTES_OFF => normalized.round(),
        _ => normalized,
    }
}

fn normalized_of(id: u32, plain: f64) -> f64 {
    match id {
        PARAMETER_GAIN => plain / 4.0,
        PARAMETER_MODE => plain / 2.0,
        _ => plain,
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The controller's part, which a plugin has as a class of its own or in itself.

struct ControllerCore {
    kind: Kind,
    values: RefCell<[f64; 32]>,
    /// The host's handler, which the controller tells its edits to.
    handler: RefCell<Option<ComPtr<IComponentHandler>>>,
    /// Whether the controller has been connected to its component: a controller of a class of its own gives nothing until it is (a controller that is the component is).
    connected: Cell<bool>,
    /// Whether the controller has been given the state of its component (`setComponentState`, and the state was the component's). **Until it has, it says that every value is 0**, as the controller
    /// of Surge XT does: a host that does not give it the component's state at the start has controller values that are not the plugin's.
    synced: Cell<bool>,
    /// Whether the controller says what the default of each parameter is (the other says 0 for each).
    declares_defaults: bool,
}

impl ControllerCore {
    fn new(kind: Kind, connected: bool, declares_defaults: bool) -> Self {
        let mut values = [0.0; 32];
        for id in kind.parameters() {
            values[*id as usize] = default_normalized(*id);
        }
        Self {
            kind,
            values: RefCell::new(values),
            handler: RefCell::new(None),
            connected: Cell::new(connected),
            // (Only a controller whose component has a state to give has anything to wait for.)
            synced: Cell::new(!matches!(kind, Kind::Gain | Kind::Single)),
            declares_defaults,
        }
    }

    unsafe fn keep_the_handler(&self, handler: *mut IComponentHandler) {
        *self.handler.borrow_mut() = ComRef::from_raw(handler).map(|handler| handler.to_com_ptr());
    }

    /// A controller is given a state of its own by the host: the state `EDIT` makes it do what a window does when a hand turns a knob (an edit of `Gain` to three quarters of the
    /// way up, in the three calls VST3 has for it) and then ask the host to note that its latency changed; `RELOAD`, `IO` and `VALUES` make it ask for a restart of that kind.
    unsafe fn take_a_state(&self, state: *mut IBStream) -> tresult {
        let words = read_all(state);
        let Some(handler) = self.handler.borrow().clone() else {
            return kResultOk;
        };
        match words.as_slice() {
            b"EDIT" => {
                handler.beginEdit(PARAMETER_GAIN);
                handler.performEdit(PARAMETER_GAIN, 0.75);
                handler.endEdit(PARAMETER_GAIN);
                handler.restartComponent(RestartFlags_::kLatencyChanged as _);
            }
            // (The other restarts a plugin can ask for, each alone.)
            b"RELOAD" => {
                handler.restartComponent(RestartFlags_::kReloadComponent as _);
            }
            b"IO" => {
                handler.restartComponent(RestartFlags_::kIoChanged as _);
            }
            b"VALUES" => {
                handler.restartComponent(RestartFlags_::kParamValuesChanged as _);
            }
            _ => {}
        }
        kResultOk
    }

    /// Takes the values out of the bytes a component's state is (the controller has none of its own).
    fn read_component_state(&self, state: *mut IBStream) -> tresult {
        let bytes = read_all(state);
        let Some(values) = values_of_a_state(&bytes) else {
            // (A controller that is given a state it cannot read forgets what it had: some real ones do. A host that gives it the state of a component that has none is heard.)
            *self.values.borrow_mut() = [0.0; 32];
            return kResultFalse;
        };
        self.synced.set(true);
        let mut mine = self.values.borrow_mut();
        for (id, value) in values {
            mine[id as usize] = value;
        }
        kResultOk
    }

    unsafe fn parameter_info(&self, index: i32, info: *mut ParameterInfo) -> tresult {
        let Some(id) = self.kind.parameters().get(usize::try_from(index).unwrap_or(usize::MAX)) else {
            return kInvalidArgument;
        };
        let info = &mut *info;
        let flags: i32 = ParameterInfo_::ParameterFlags_::kCanAutomate as _;
        let (title, units, steps, flags) = match *id {
            PARAMETER_GAIN => ("Gain", "x", 0, flags),
            PARAMETER_MODE => ("Mode", "", 2, flags | flag(ParameterInfo_::ParameterFlags_::kIsList)),
            PARAMETER_SECRET => (
                "Secret",
                "",
                0,
                flags | flag(ParameterInfo_::ParameterFlags_::kIsHidden),
            ),
            PARAMETER_LEVEL => ("Level", "", 0, ParameterInfo_::ParameterFlags_::kIsReadOnly as _),
            PARAMETER_BYPASS => (
                "Bypass",
                "",
                1,
                flags | flag(ParameterInfo_::ParameterFlags_::kIsBypass),
            ),
            PARAMETER_BEND => ("Bend", "", 0, flags),
            PARAMETER_WHEEL => ("Wheel", "", 0, flags),
            _ => ("All notes off", "", 1, flags),
        };
        info.id = *id;
        copy_wstring(title, &mut info.title);
        copy_wstring(title, &mut info.shortTitle);
        copy_wstring(units, &mut info.units);
        info.stepCount = steps;
        info.defaultNormalizedValue = if self.declares_defaults {
            default_normalized(*id)
        } else {
            0.0
        };
        info.unitId = 0;
        info.flags = flags;
        kResultOk
    }

    unsafe fn text_of(&self, id: u32, normalized: f64, text: *mut String128) -> tresult {
        let text = &mut *text;
        let words = match id {
            PARAMETER_GAIN => format!("{:.2}", plain_of(id, normalized)),
            PARAMETER_MODE => ["Plain", "Double", "Triple"][plain_of(id, normalized) as usize].to_owned(),
            PARAMETER_BYPASS => (if plain_of(id, normalized) >= 1.0 { "On" } else { "Off" }).to_owned(),
            _ => format!("{normalized:.3}"),
        };
        copy_wstring(&words, text);
        kResultOk
    }
}

macro_rules! the_controller_is {
    ($type:ty) => {
        impl IEditControllerTrait for $type {
            unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
                self.core.read_component_state(state)
            }
            unsafe fn setState(&self, state: *mut IBStream) -> tresult {
                self.core.take_a_state(state)
            }
            unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
                kNotImplemented
            }
            unsafe fn getParameterCount(&self) -> i32 {
                self.core.kind.parameters().len() as i32
            }
            unsafe fn getParameterInfo(&self, index: i32, info: *mut ParameterInfo) -> tresult {
                self.core.parameter_info(index, info)
            }
            unsafe fn getParamStringByValue(&self, id: u32, normalized: f64, text: *mut String128) -> tresult {
                self.core.text_of(id, normalized, text)
            }
            unsafe fn getParamValueByString(&self, _id: u32, _text: *mut TChar, _normalized: *mut f64) -> tresult {
                kNotImplemented
            }
            unsafe fn normalizedParamToPlain(&self, id: u32, normalized: f64) -> f64 {
                plain_of(id, normalized)
            }
            unsafe fn plainParamToNormalized(&self, id: u32, plain: f64) -> f64 {
                normalized_of(id, plain)
            }
            unsafe fn getParamNormalized(&self, id: u32) -> f64 {
                if !self.core.connected.get() {
                    return f64::NAN;
                }
                if !self.core.synced.get() {
                    return 0.0;
                }
                self.core.values.borrow()[(id as usize).min(31)]
            }
            unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
                if self.core.kind.parameters().contains(&id) {
                    self.core.values.borrow_mut()[id as usize] = value;
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
                self.core.keep_the_handler(handler);
                kResultOk
            }
            unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
                ptr::null_mut()
            }
        }
    };
}

struct Controller {
    core: ControllerCore,
}

impl Class for Controller {
    type Interfaces = (IEditController, IConnectionPoint);
}

impl IPluginBaseTrait for Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

the_controller_is!(Controller);

impl IConnectionPointTrait for Controller {
    unsafe fn connect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.core.connected.set(true);
        kResultOk
    }
    unsafe fn disconnect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.core.connected.set(false);
        kResultOk
    }
    unsafe fn notify(&self, _message: *mut IMessage) -> tresult {
        kResultOk
    }
}

/// The controller of the instrument, which maps MIDI controllers to parameters.
struct InstrumentController {
    core: ControllerCore,
}

impl Class for InstrumentController {
    type Interfaces = (IEditController, IMidiMapping, IConnectionPoint);
}

impl IPluginBaseTrait for InstrumentController {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

the_controller_is!(InstrumentController);

impl IConnectionPointTrait for InstrumentController {
    unsafe fn connect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.core.connected.set(true);
        kResultOk
    }
    unsafe fn disconnect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.core.connected.set(false);
        kResultOk
    }
    unsafe fn notify(&self, _message: *mut IMessage) -> tresult {
        kResultOk
    }
}

impl IMidiMappingTrait for InstrumentController {
    unsafe fn getMidiControllerAssignment(&self, bus: i32, _channel: i16, number: i16, id: *mut u32) -> tresult {
        if bus != 0 {
            return kResultFalse;
        }
        let parameter = match i32::from(number) {
            129 => PARAMETER_BEND,
            1 => PARAMETER_WHEEL,
            123 => PARAMETER_ALL_NOTES_OFF,
            _ => return kResultFalse,
        };
        *id = parameter;
        kResultTrue
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The processor. With `SINGLE` true it is its own controller too.

/// A change of a parameter or a note, at a sample of the block.
enum What {
    Parameter(u32, f64),
    NoteOn { pitch: i16, velocity: f32, id: i32 },
    NoteOff { pitch: i16, id: i32 },
}

struct Plugin<const SINGLE: bool> {
    kind: Kind,
    fault: Fault,
    core: ControllerCore,
    /// The values of the parameters as the processor has them (normalised).
    values: RefCell<[f64; 32]>,
    delay: RefCell<Vec<[f32; 2]>>,
    held: RefCell<Vec<(i16, i32, f32)>>,
    /// For the instrument with a side chain: the state was given and is not applied yet; what it says, once it is.
    state_waits: Cell<bool>,
    edits_kept: Cell<u32>,
    /// How loud the instrument is: each edit of its state made it half as loud again.
    volume: Cell<f32>,
    /// Where the next sample of the delay goes (the delay is a ring that is as long as it).
    position: Cell<usize>,
    /// Whether the host connected the component to its controller (a plugin of two classes does not work if it did not).
    connected: Cell<bool>,
    /// Which buses are switched on: the inputs and the outputs. The buses that are on by default are the inputs and the first output.
    switched_on: RefCell<[[bool; 4]; 2]>,
}

impl<const SINGLE: bool> Plugin<SINGLE> {
    fn new(kind: Kind, fault: Fault) -> Self {
        let mut values = [0.0; 32];
        for id in kind.parameters() {
            values[*id as usize] = default_normalized(*id);
        }
        // (The pitch wheel of an instrument starts in the middle, whether or not its controller has the parameter.)
        values[PARAMETER_BEND as usize] = default_normalized(PARAMETER_BEND);
        Self {
            kind,
            fault,
            core: ControllerCore::new(kind, true, true),
            values: RefCell::new(values),
            delay: RefCell::new(vec![[0.0; 2]; LATENCY_SAMPLES]),
            held: RefCell::new(Vec::new()),
            state_waits: Cell::new(false),
            edits_kept: Cell::new(0),
            volume: Cell::new(1.0),
            position: Cell::new(0),
            connected: Cell::new(false),
            switched_on: RefCell::new([[true; 4], [true, false, false, false]]),
        }
    }

    fn factor_of_the_gain(&self) -> f32 {
        let values = self.values.borrow();
        let mode = plain_of(PARAMETER_MODE, values[PARAMETER_MODE as usize]);
        (plain_of(PARAMETER_GAIN, values[PARAMETER_GAIN as usize]) * (mode + 1.0)) as f32
    }

    /// What the instrument sounds like now, for one sample.
    fn instrument_sample(&self) -> f32 {
        let held = self.held.borrow();
        let values = self.values.borrow();
        let mut sum: f32 = held
            .iter()
            .map(|(pitch, _, velocity)| velocity * f32::from(*pitch) / 100.0)
            .sum();
        sum += ((values[PARAMETER_BEND as usize] - 0.5) * 0.5) as f32;
        if !held.is_empty() {
            sum += (values[PARAMETER_WHEEL as usize] * 0.25) as f32;
        }
        sum * self.volume.get()
    }

    fn apply(&self, what: &What) {
        match *what {
            What::Parameter(id, value) => {
                if let Some(place) = self.values.borrow_mut().get_mut(id as usize) {
                    *place = value;
                }
                if id == PARAMETER_ALL_NOTES_OFF && value >= 0.5 {
                    self.held.borrow_mut().clear();
                }
            }
            What::NoteOn { pitch, velocity, id } => self.held.borrow_mut().push((pitch, id, velocity)),
            What::NoteOff { pitch, id } => {
                let mut held = self.held.borrow_mut();
                if let Some(place) = held.iter().position(
                    |(held_pitch, held_id, _)| {
                        if id >= 0 { *held_id == id } else { *held_pitch == pitch }
                    },
                ) {
                    held.remove(place);
                }
            }
        }
    }

    /// The state's text, applied: `edits=N` halves the sound N times. (Real instruments apply a state they were given when they next process.)
    fn apply_the_state_that_waits(&self) {
        if self.state_waits.replace(false) {
            self.volume.set(0.5_f32.powi(self.edits_kept.get() as i32));
        }
    }
}

impl<const SINGLE: bool> IPluginBaseTrait for Plugin<SINGLE> {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        if self.fault == Fault::Initialize {
            kResultFalse
        } else {
            kResultOk
        }
    }
    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl Class for Plugin<false> {
    type Interfaces = (IComponent, IAudioProcessor, IConnectionPoint);
}

impl IConnectionPointTrait for Plugin<false> {
    unsafe fn connect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.connected.set(true);
        kResultOk
    }
    unsafe fn disconnect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.connected.set(false);
        kResultOk
    }
    unsafe fn notify(&self, _message: *mut IMessage) -> tresult {
        kResultOk
    }
}

impl Class for Plugin<true> {
    type Interfaces = (IComponent, IAudioProcessor, IEditController);
}

the_controller_is!(Plugin<true>);

/// The bytes that a stream holds, from its start to its end.
fn read_all(state: *mut IBStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    let Some(stream) = (unsafe { ComRef::from_raw(state) }) else {
        return bytes;
    };
    let mut chunk = [0_u8; 256];
    loop {
        let mut read = 0;
        let result = unsafe { stream.read(chunk.as_mut_ptr().cast(), chunk.len() as i32, &mut read) };
        if result != kResultOk || read <= 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
    }
    bytes
}

fn write_all(state: *mut IBStream, bytes: &[u8]) -> tresult {
    let Some(stream) = (unsafe { ComRef::from_raw(state) }) else {
        return kInvalidArgument;
    };
    let mut written = 0;
    unsafe { stream.write(bytes.as_ptr() as *mut c_void, bytes.len() as i32, &mut written) }
}

/// The ids and values a state of the gain effect holds.
fn values_of_a_state(bytes: &[u8]) -> Option<Vec<(u32, f64)>> {
    if bytes.len() != 4 + 4 * 8 || &bytes[..4] != STATE_MAGIC {
        return None;
    }
    let ids = [PARAMETER_GAIN, PARAMETER_MODE, PARAMETER_SECRET, PARAMETER_BYPASS];
    Some(
        ids.iter()
            .enumerate()
            .map(|(place, id)| {
                let start = 4 + place * 8;
                (
                    *id,
                    f64::from_le_bytes(bytes[start..start + 8].try_into().expect("eight bytes")),
                )
            })
            .collect(),
    )
}

impl<const SINGLE: bool> IComponentTrait for Plugin<SINGLE> {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        let numbers = match self.kind {
            Kind::Gain if self.fault == Fault::NoDefaults => NO_DEFAULTS_CONTROLLER,
            Kind::Gain => GAIN_CONTROLLER,
            Kind::Instrument => INSTRUMENT_CONTROLLER,
            Kind::SideChainInstrument => SIDE_CHAIN_INSTRUMENT_CONTROLLER,
            Kind::Latency | Kind::Single | Kind::SideChainEffect => return kResultFalse,
        };
        *class_id = uid(numbers[0], numbers[1], numbers[2], numbers[3]);
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media: MediaType, direction: BusDirection) -> i32 {
        let audio = media == MediaTypes_::kAudio as _;
        let input = direction == BusDirections_::kInput as _;
        match (audio, input, self.kind) {
            (true, true, Kind::Instrument) => 0,
            (true, true, Kind::SideChainInstrument) => 1,
            (true, true, Kind::SideChainEffect) => 2,
            (true, true, _) => 1,
            (true, false, Kind::SideChainInstrument) => 3,
            (true, false, _) => 1,
            (false, true, kind) if kind.takes_notes() => 1,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(&self, media: MediaType, direction: BusDirection, index: i32, bus: *mut BusInfo) -> tresult {
        let count = self.getBusCount(media, direction);
        if index < 0 || index >= count {
            return kInvalidArgument;
        }
        let bus = &mut *bus;
        bus.mediaType = media;
        bus.direction = direction;
        let audio = media == MediaTypes_::kAudio as _;
        let input = direction == BusDirections_::kInput as _;
        let (name, channels, aux) = match (audio, input, index) {
            (true, true, _) if self.kind == Kind::SideChainInstrument => ("Sidechain", 2, true),
            (true, true, 0) if self.kind == Kind::SideChainEffect => ("Sidechain", 2, true),
            (true, true, _) => ("Input", 2, false),
            (true, false, 0) => ("Output", 2, false),
            (true, false, 1) => ("Scene A", 2, true),
            (true, false, _) => ("Scene B", 2, true),
            (false, _, _) => ("Notes", 16, false),
        };
        copy_wstring(name, &mut bus.name);
        bus.channelCount = channels;
        bus.busType = (if aux { BusTypes_::kAux } else { BusTypes_::kMain }) as BusType;
        // (An aux bus that is on by default is how Surge XT has them; the extra outputs are off.)
        bus.flags = if audio && !input && index > 0 {
            0
        } else {
            BusInfo_::BusFlags_::kDefaultActive as _
        };
        kResultOk
    }

    unsafe fn getRoutingInfo(&self, _in: *mut RoutingInfo, _out: *mut RoutingInfo) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(&self, media: MediaType, direction: BusDirection, index: i32, state: TBool) -> tresult {
        if media == MediaTypes_::kAudio as MediaType
            && let Ok(index) = usize::try_from(index)
            && index < 4
        {
            let row = usize::from(direction != BusDirections_::kInput as BusDirection);
            self.switched_on.borrow_mut()[row][index] = state != 0;
        }
        kResultOk
    }

    unsafe fn setActive(&self, state: TBool) -> tresult {
        if state != 0 && self.fault == Fault::Activate {
            return kResultFalse;
        }
        if state != 0 {
            misbehave_if_asked("crash_when_activated");
        }
        if state == 0 {
            self.held.borrow_mut().clear();
            self.delay.borrow_mut().iter_mut().for_each(|sample| *sample = [0.0; 2]);
            self.position.set(0);
        }
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        let bytes = read_all(state);
        if self.kind == Kind::SideChainInstrument {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let edits = text
                .split("edits=")
                .nth(1)
                .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next());
            self.edits_kept
                .set(edits.and_then(|digits| digits.parse().ok()).unwrap_or(0));
            self.state_waits.set(true);
            return kResultOk;
        }
        let Some(values) = values_of_a_state(&bytes) else {
            return kResultFalse;
        };
        let mut mine = self.values.borrow_mut();
        for (id, value) in values {
            mine[id as usize] = value;
        }
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        if self.kind == Kind::SideChainInstrument {
            return write_all(state, format!("edits={}", self.edits_kept.get()).as_bytes());
        }
        if !matches!(self.kind, Kind::Gain | Kind::Single) {
            return kNotImplemented;
        }
        let values = self.values.borrow();
        let mut bytes = STATE_MAGIC.to_vec();
        for id in [PARAMETER_GAIN, PARAMETER_MODE, PARAMETER_SECRET, PARAMETER_BYPASS] {
            bytes.extend_from_slice(&values[id as usize].to_le_bytes());
        }
        write_all(state, &bytes)
    }
}

impl<const SINGLE: bool> IAudioProcessorTrait for Plugin<SINGLE> {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        count_in: i32,
        outputs: *mut SpeakerArrangement,
        count_out: i32,
    ) -> tresult {
        let stereo = |pointer: *mut SpeakerArrangement, count: i32| {
            (0..count).all(|place| *pointer.offset(place as isize) == SpeakerArr::kStereo)
        };
        let wanted_in = self.getBusCount(MediaTypes_::kAudio as _, BusDirections_::kInput as _);
        let wanted_out = self.getBusCount(MediaTypes_::kAudio as _, BusDirections_::kOutput as _);
        if count_in == wanted_in && count_out == wanted_out && stereo(inputs, count_in) && stereo(outputs, count_out) {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        direction: BusDirection,
        index: i32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        let count = self.getBusCount(MediaTypes_::kAudio as _, direction);
        if index < 0 || index >= count {
            return kInvalidArgument;
        }
        *arrangement = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
        if size == SymbolicSampleSizes_::kSample32 as _ {
            kResultOk
        } else {
            kNotImplemented
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        if self.kind == Kind::Latency {
            LATENCY_SAMPLES as u32
        } else {
            0
        }
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, state: TBool) -> tresult {
        // (Stopping flushes the voices, as real instruments do: it is how a host that cannot say "all notes off" makes one forget.)
        if state == 0 {
            self.held.borrow_mut().clear();
        }
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        misbehave_if_asked("crash_when_run");
        misbehave_if_asked("hang_when_run");
        misbehave_if_asked("exit_when_run");
        crash_at_every_fourth_run_if_asked();
        if self.fault == Fault::Process {
            return kResultFalse;
        }
        let data = &*data;
        let samples = usize::try_from(data.numSamples).unwrap_or(0);

        // What a host has to have done before the first block: connected the two halves of a plugin that has them, switched the main output on, and given channels to every bus
        // that it switched on (a bus that is on and has none is a host that does not know what it switched on).
        if self.kind.has_a_separate_controller() && !self.connected.get() {
            return kNotInitialized;
        }
        let switched_on = *self.switched_on.borrow();
        if !switched_on[1][0] {
            return kNotInitialized;
        }
        for (on, count, buses) in [
            (&switched_on[0], data.numInputs, data.inputs),
            (&switched_on[1], data.numOutputs, data.outputs),
        ] {
            for (place, on) in on.iter().enumerate().take(usize::try_from(count).unwrap_or(0)) {
                let channels = (*buses.add(place)).numChannels;
                // (A bus that is on has its channels; a bus that is off has none: the host that gives channels to a bus it switched off has lost track of what it did.)
                if (*on && channels == 0) || (!*on && channels != 0) {
                    return kInvalidArgument;
                }
            }
        }

        // What happens in the block, in order of time.
        let mut changes: Vec<(usize, What)> = Vec::new();
        if let Some(parameters) = ComRef::from_raw(data.inputParameterChanges) {
            for place in 0..parameters.getParameterCount() {
                let Some(queue) = ComRef::from_raw(parameters.getParameterData(place)) else {
                    continue;
                };
                let id = queue.getParameterId();
                let mut last_at = 0;
                for point in 0..queue.getPointCount() {
                    let (mut at, mut value) = (0, 0.0);
                    if queue.getPoint(point, &mut at, &mut value) == kResultOk {
                        // (VST3 says the points of a queue are in order of the sample; a plugin may rely on it.)
                        if at < last_at {
                            return kInvalidArgument;
                        }
                        last_at = at;
                        changes.push((usize::try_from(at).unwrap_or(0), What::Parameter(id, value)));
                    }
                }
            }
        }
        let mut note_in_this_block = false;
        if let Some(events) = ComRef::from_raw(data.inputEvents) {
            for place in 0..events.getEventCount() {
                let mut event: Event = std::mem::zeroed();
                if events.getEvent(place, &mut event) != kResultOk {
                    continue;
                }
                let at = usize::try_from(event.sampleOffset).unwrap_or(0);
                if event.r#type == Event_::EventTypes_::kNoteOnEvent as _ {
                    let note = event.__field0.noteOn;
                    note_in_this_block = true;
                    changes.push((
                        at,
                        What::NoteOn {
                            pitch: note.pitch,
                            velocity: note.velocity,
                            id: note.noteId,
                        },
                    ));
                } else if event.r#type == Event_::EventTypes_::kNoteOffEvent as _ {
                    let note = event.__field0.noteOff;
                    changes.push((
                        at,
                        What::NoteOff {
                            pitch: note.pitch,
                            id: note.noteId,
                        },
                    ));
                }
            }
        }
        changes.sort_by_key(|(at, _)| *at);

        // (The instrument with a side chain applies its state now, and does not live through a note in the same block: the real one hangs, this one says it failed.)
        if self.kind == Kind::SideChainInstrument {
            if self.state_waits.get() && note_in_this_block {
                return kInternalError;
            }
            self.apply_the_state_that_waits();
        }

        // The buses: the first output, and the first input if there is a main one.
        let outputs = if data.numOutputs > 0 {
            slice::from_raw_parts(data.outputs, 1)
        } else {
            &[]
        };
        let Some(output) = outputs.first() else {
            return kResultOk;
        };
        let output_channels = slice::from_raw_parts(output.__field0.channelBuffers32, output.numChannels as usize);
        let main_input = if self.kind == Kind::SideChainEffect { 1 } else { 0 };
        let input_channels: &[*mut f32] = if self.kind.takes_notes() || data.numInputs <= main_input {
            &[]
        } else {
            let input = &*data.inputs.add(main_input as usize);
            if input.numChannels == 0 {
                &[]
            } else {
                slice::from_raw_parts(input.__field0.channelBuffers32, input.numChannels as usize)
            }
        };

        let mut next = 0;
        let mut peak = 0.0_f32;
        for sample in 0..samples {
            while next < changes.len() && changes[next].0 <= sample {
                self.apply(&changes[next].1);
                next += 1;
            }
            let bypass = plain_of(PARAMETER_BYPASS, self.values.borrow()[PARAMETER_BYPASS as usize]) >= 1.0;
            for (channel, out) in output_channels.iter().enumerate().take(2) {
                let heard = match self.kind {
                    Kind::Instrument | Kind::SideChainInstrument => self.instrument_sample(),
                    Kind::Latency => {
                        let input = input_channels.get(channel).map_or(0.0, |input| *input.add(sample));
                        let mut delay = self.delay.borrow_mut();
                        let place = self.position.get() % LATENCY_SAMPLES;
                        // (A ring that is as long as the delay: what is there is what went in 64 samples ago.)
                        let delayed = delay[place][channel];
                        delay[place][channel] = input;
                        delayed
                    }
                    Kind::SideChainEffect => input_channels
                        .get(channel)
                        .map_or(0.0, |input| *input.add(sample) * 0.5),
                    Kind::Gain | Kind::Single => {
                        let input = input_channels.get(channel).map_or(0.0, |input| *input.add(sample));
                        if bypass {
                            input
                        } else {
                            input * self.factor_of_the_gain()
                        }
                    }
                };
                let heard = if self.fault == Fault::NotNumbers {
                    f32::NAN
                } else {
                    heard
                };
                *out.add(sample) = heard;
                peak = peak.max(heard.abs());
            }
            self.position.set(self.position.get() + 1);
        }
        for change in &changes[next..] {
            self.apply(&change.1);
        }

        // The meter: the peak of the block, as an output parameter change.
        if self.kind == Kind::Gain
            && let Some(parameters) = ComRef::from_raw(data.outputParameterChanges)
        {
            let mut place = 0;
            let id = PARAMETER_LEVEL;
            if let Some(queue) = ComRef::from_raw(parameters.addParameterData(&id, &mut place)) {
                let mut point = 0;
                queue.addPoint(0, f64::from(peak.min(1.0)), &mut point);
            }
        }
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        if self.kind == Kind::Latency { TAIL_SAMPLES } else { 0 }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The factory.

struct ClassEntry {
    numbers: [u32; 4],
    category: &'static str,
    name: &'static str,
    sub_categories: &'static str,
    make: fn() -> Option<ComPtr<FUnknown>>,
}

const AUDIO: &str = "Audio Module Class";
const CONTROLLER: &str = "Component Controller Class";

fn unknown_of<C: Class>(wrapper: ComWrapper<C>) -> Option<ComPtr<FUnknown>> {
    wrapper.to_com_ptr::<FUnknown>()
}

fn plugin(kind: Kind, fault: Fault) -> Option<ComPtr<FUnknown>> {
    unknown_of(ComWrapper::new(Plugin::<false>::new(kind, fault)))
}

const CLASSES: &[ClassEntry] = &[
    ClassEntry {
        numbers: GAIN,
        category: AUDIO,
        name: "Test Gain",
        sub_categories: "Fx",
        make: || plugin(Kind::Gain, Fault::None),
    },
    ClassEntry {
        numbers: GAIN_CONTROLLER,
        category: CONTROLLER,
        name: "Test Gain",
        sub_categories: "Fx",
        make: || {
            unknown_of(ComWrapper::new(Controller {
                core: ControllerCore::new(Kind::Gain, false, true),
            }))
        },
    },
    ClassEntry {
        numbers: LATENCY,
        category: AUDIO,
        name: "Test Latency",
        sub_categories: "Fx|Delay",
        make: || plugin(Kind::Latency, Fault::None),
    },
    ClassEntry {
        numbers: INSTRUMENT,
        category: AUDIO,
        name: "Test Instrument",
        sub_categories: "Instrument|Synth",
        make: || plugin(Kind::Instrument, Fault::None),
    },
    ClassEntry {
        numbers: INSTRUMENT_CONTROLLER,
        category: CONTROLLER,
        name: "Test Instrument",
        sub_categories: "Instrument|Synth",
        make: || {
            unknown_of(ComWrapper::new(InstrumentController {
                core: ControllerCore::new(Kind::Instrument, false, true),
            }))
        },
    },
    ClassEntry {
        numbers: SIDE_CHAIN_EFFECT,
        category: AUDIO,
        name: "Test Effect With A Side Chain",
        sub_categories: "Fx",
        make: || plugin(Kind::SideChainEffect, Fault::None),
    },
    ClassEntry {
        numbers: SINGLE,
        category: AUDIO,
        name: "Test Single",
        sub_categories: "Fx",
        make: || unknown_of(ComWrapper::new(Plugin::<true>::new(Kind::Single, Fault::None))),
    },
    ClassEntry {
        numbers: SIDE_CHAIN_INSTRUMENT,
        category: AUDIO,
        name: "Test Instrument With A Side Chain",
        sub_categories: "Instrument|Synth",
        make: || plugin(Kind::SideChainInstrument, Fault::None),
    },
    ClassEntry {
        numbers: SIDE_CHAIN_INSTRUMENT_CONTROLLER,
        category: CONTROLLER,
        name: "Test Instrument With A Side Chain",
        sub_categories: "Instrument|Synth",
        make: || {
            unknown_of(ComWrapper::new(Controller {
                core: ControllerCore::new(Kind::SideChainInstrument, false, true),
            }))
        },
    },
    ClassEntry {
        numbers: COMPATIBILITY,
        category: "Plugin Compatibility Class",
        name: "Test Compatibility",
        sub_categories: "",
        make: || None,
    },
    ClassEntry {
        numbers: FAILS_TO_INITIALIZE,
        category: AUDIO,
        name: "Test Fails To Initialize",
        sub_categories: "Fx",
        make: || plugin(Kind::Gain, Fault::Initialize),
    },
    ClassEntry {
        numbers: FAILS_TO_ACTIVATE,
        category: AUDIO,
        name: "Test Fails To Activate",
        sub_categories: "Fx",
        make: || plugin(Kind::Gain, Fault::Activate),
    },
    ClassEntry {
        numbers: FAILS_TO_PROCESS,
        category: AUDIO,
        name: "Test Fails To Process",
        sub_categories: "Fx",
        make: || plugin(Kind::Gain, Fault::Process),
    },
    ClassEntry {
        numbers: NO_DEFAULTS,
        category: AUDIO,
        name: "Test Gain Without Defaults",
        sub_categories: "Fx",
        make: || plugin(Kind::Gain, Fault::NoDefaults),
    },
    ClassEntry {
        numbers: NO_DEFAULTS_CONTROLLER,
        category: CONTROLLER,
        name: "Test Gain Without Defaults",
        sub_categories: "Fx",
        make: || {
            unknown_of(ComWrapper::new(Controller {
                core: ControllerCore::new(Kind::Gain, false, false),
            }))
        },
    },
    ClassEntry {
        numbers: NOT_NUMBERS,
        category: AUDIO,
        name: "Test Not Numbers",
        sub_categories: "Fx",
        make: || plugin(Kind::Gain, Fault::NotNumbers),
    },
];

struct Factory;

impl Class for Factory {
    type Interfaces = (IPluginFactory3,);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        let info = &mut *info;
        copy_cstring("fastforwardrs tests", &mut info.vendor);
        copy_cstring("https://github.com/JohnDorsey/fastforwardrs", &mut info.url);
        copy_cstring("nobody@example.invalid", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as _;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        CLASSES.len() as i32
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        let Some(entry) = CLASSES.get(usize::try_from(index).unwrap_or(usize::MAX)) else {
            return kInvalidArgument;
        };
        let info = &mut *info;
        info.cid = uid(entry.numbers[0], entry.numbers[1], entry.numbers[2], entry.numbers[3]);
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as _;
        copy_cstring(entry.category, &mut info.category);
        copy_cstring(entry.name, &mut info.name);
        kResultOk
    }

    unsafe fn createInstance(&self, cid: FIDString, iid: FIDString, object: *mut *mut c_void) -> tresult {
        let wanted = *(cid as *const TUID);
        let Some(entry) = CLASSES
            .iter()
            .find(|entry| uid(entry.numbers[0], entry.numbers[1], entry.numbers[2], entry.numbers[3]) == wanted)
        else {
            return kInvalidArgument;
        };
        misbehave_if_asked("crash_when_made");
        misbehave_if_asked("hang_when_made");
        if misbehaviour() == "print_noise" {
            println!("noise from the plugin module, while a class is made");
        }
        match (entry.make)() {
            Some(instance) => {
                let pointer = instance.as_ptr();
                ((*(*pointer).vtbl).queryInterface)(pointer, iid as *mut TUID, object)
            }
            None => kNotImplemented,
        }
    }
}

impl IPluginFactory2Trait for Factory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        let Some(entry) = CLASSES.get(usize::try_from(index).unwrap_or(usize::MAX)) else {
            return kInvalidArgument;
        };
        let info = &mut *info;
        info.cid = uid(entry.numbers[0], entry.numbers[1], entry.numbers[2], entry.numbers[3]);
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as _;
        copy_cstring(entry.category, &mut info.category);
        copy_cstring(entry.name, &mut info.name);
        info.classFlags = 0;
        copy_cstring(entry.sub_categories, &mut info.subCategories);
        copy_cstring("fastforwardrs tests", &mut info.vendor);
        copy_cstring("1.2.3", &mut info.version);
        copy_cstring("VST 3.7.0", &mut info.sdkVersion);
        kResultOk
    }
}

impl IPluginFactory3Trait for Factory {
    unsafe fn getClassInfoUnicode(&self, index: i32, info: *mut PClassInfoW) -> tresult {
        let Some(entry) = CLASSES.get(usize::try_from(index).unwrap_or(usize::MAX)) else {
            return kInvalidArgument;
        };
        let info = &mut *info;
        info.cid = uid(entry.numbers[0], entry.numbers[1], entry.numbers[2], entry.numbers[3]);
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as _;
        copy_cstring(entry.category, &mut info.category);
        // (The name in Unicode has a letter that is not ASCII, so that a host that reads the plain one is told apart.)
        copy_wstring(&format!("{} \u{e9}", entry.name), &mut info.name);
        info.classFlags = 0;
        copy_cstring(entry.sub_categories, &mut info.subCategories);
        copy_wstring("fastforwardrs tests", &mut info.vendor);
        copy_wstring("1.2.3", &mut info.version);
        copy_wstring("VST 3.7.0", &mut info.sdkVersion);
        kResultOk
    }

    unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
}

// The entry points. Each system has its own names; the host calls the ones of its system (and a module without them is fine on Windows).

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "system" fn InitDll() -> bool {
    start_up()
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "system" fn ExitDll() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "system" fn ModuleEntry(_library_handle: *mut c_void) -> bool {
    start_up()
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "system" fn ModuleExit() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "system" fn bundleEntry(_bundle: *mut c_void) -> bool {
    start_up()
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "system" fn bundleExit() -> bool {
    true
}

#[unsafe(no_mangle)]
extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory)
        .to_com_ptr::<IPluginFactory>()
        .map_or(ptr::null_mut(), ComPtr::into_raw)
}

// ---------------------------------------------------------------------------------------------------------------------
// For the tests that load it.

/// Where the file of this library is, **for a test (or any program) that was built by the same `cargo` run as this crate's library** and depends on this crate: `cargo`
/// builds the library beside the program, in the same folder. (Nothing is built by this function.)
pub fn library_file_for_tests() -> PathBuf {
    let program = std::env::current_exe().expect("a program knows where it is");
    let name = format!(
        "{}vst3_test_plugins{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let file = program.parent().expect("a program is in a folder").join(name);
    assert!(
        file.is_file(),
        "the library of the test plugins is not at {} (the program that looks for it has to have the crate vst3-test-plugins as a dependency)",
        file.display()
    );
    file
}

/// The folder and the file name that a VST3 module of this system keeps its library in, as the VST3 documentation says (written here on its own, not taken from the host).
fn place_in_a_module() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        (
            if cfg!(target_arch = "aarch64") {
                "arm64-win"
            } else {
                "x86_64-win"
            },
            ".vst3",
        )
    } else if cfg!(target_os = "macos") {
        ("MacOS", "")
    } else {
        (
            if cfg!(target_arch = "aarch64") {
                "aarch64-linux"
            } else {
                "x86_64-linux"
            },
            ".so",
        )
    }
}

/// Makes a module of the test plugins in `folder`: a folder `test.vst3` with the library copied into it where the system keeps it, and gives the folder.
pub fn write_module(folder: &Path) -> PathBuf {
    let module = folder.join("test.vst3");
    let (architecture, extension) = place_in_a_module();
    let inside = module.join("Contents").join(architecture);
    std::fs::create_dir_all(&inside).expect("the module's folder is made");
    std::fs::copy(library_file_for_tests(), inside.join(format!("test{extension}"))).expect("the library is copied");
    module
}
