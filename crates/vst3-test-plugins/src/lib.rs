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
//! | the window | [`GAIN`] and [`SINGLE`] have a window (`createView("editor")`): see [`VIEW_WIDTH`]. It is **strict**: it wants a parent and the kind of window of this system, it takes `onSize` only while it is attached, and it is told by keys (`onKeyDown`, which a host forwards) to do what a hand would: `e` an edit of `Gain` in the three calls VST3 has for it, `r` a request for another size (`b` for one that is more than it allows), `d` `setDirty`, `v` a change of the controller's value with `kParamValuesChanged`, `w` three of them, `l` (Linux) a timer and a descriptor asked of the host's run loop, `s` a change of the component's state that is told in a message and not to the host (the hidden value `Secret` goes up a quarter), `m` an edit that is told in a message made by the host (as DPF's windows tell theirs: it is made only if the message comes back whole). The keys in the variable `FFRS_TEST_VIEW_KEYS` are pressed when it is attached |
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
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::{ptr, slice};

#[cfg(target_os = "linux")]
use vst3::Steinberg::Linux::{
    FileDescriptor, IEventHandler, IEventHandlerTrait, IRunLoop, IRunLoopTrait, ITimerHandler, ITimerHandlerTrait,
};
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
/// standard output while it is loaded and while it is made), `echo_values` (every value that the controller is given is said back as an edit of its own), `crash_when_attached` and `hang_when_attached` (its window is put in a parent).
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
    values: Rc<RefCell<[f64; 32]>>,
    /// The host's handler, which the controller tells its edits to.
    handler: RefCell<Option<ComPtr<IComponentHandler>>>,
    /// The host that the plugin was initialized with (it makes messages and lists, as a plugin does that talks to its other half).
    host: RefCell<Option<ComPtr<IHostApplication>>>,
    /// The other half, as the host connected it (the component, for a controller of a class of its own).
    peer: RefCell<Option<ComPtr<IConnectionPoint>>>,
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
            values: Rc::new(RefCell::new(values)),
            handler: RefCell::new(None),
            host: RefCell::new(None),
            peer: RefCell::new(None),
            connected: Cell::new(connected),
            // (Only a controller whose component has a state to give has anything to wait for.)
            synced: Cell::new(!matches!(kind, Kind::Gain | Kind::Single)),
            declares_defaults,
        }
    }

    /// `echo_values`: says a value that it was given as an edit of its own, in the three calls, as some plugins do (Surge XT Effects makes edits that nobody made). A host must not take
    /// that for a hand.
    unsafe fn say_it_as_an_edit_if_asked(&self, id: u32, value: f64) {
        if misbehaviour() != "echo_values" {
            return;
        }
        let handler = self.handler.borrow().clone();
        if let Some(handler) = handler {
            handler.beginEdit(id);
            handler.performEdit(id, value);
            handler.endEdit(id);
        }
    }

    /// The window of the plugin, if it has one and `name` is what a window of an editor is called.
    unsafe fn create_a_view(&self, name: *const c_char) -> *mut IPlugView {
        let wanted = !name.is_null() && CStr::from_ptr(name).to_bytes() == b"editor";
        if !wanted || !matches!(self.kind, Kind::Gain | Kind::Single) {
            return ptr::null_mut();
        }
        let view = ComWrapper::new(TestView {
            me: Cell::new(ptr::null_mut()),
            handler: self.handler.borrow().clone(),
            host: self.host.borrow().clone(),
            peer: self.peer.borrow().clone(),
            values: Rc::clone(&self.values),
            attached: Cell::new(false),
            size: Cell::new((VIEW_WIDTH, VIEW_HEIGHT)),
            frame: RefCell::new(None),
            #[cfg(target_os = "linux")]
            run_loop: RefCell::new(None),
        });
        let Some(pointer) = view.to_com_ptr::<IPlugView>() else {
            return ptr::null_mut();
        };
        view.me.set(pointer.as_ptr());
        pointer.into_raw()
    }

    unsafe fn keep_the_host(&self, context: *mut FUnknown) {
        *self.host.borrow_mut() = ComRef::from_raw(context).and_then(|context| context.cast::<IHostApplication>());
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
                    self.core.say_it_as_an_edit_if_asked(id, value);
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
                self.core.keep_the_handler(handler);
                kResultOk
            }
            unsafe fn createView(&self, name: *const c_char) -> *mut IPlugView {
                self.core.create_a_view(name)
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
    unsafe fn initialize(&self, context: *mut FUnknown) -> tresult {
        self.core.keep_the_host(context);
        kResultOk
    }
    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

the_controller_is!(Controller);

impl IConnectionPointTrait for Controller {
    unsafe fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.core.connected.set(true);
        *self.core.peer.borrow_mut() = ComRef::from_raw(other).map(|other| other.to_com_ptr());
        kResultOk
    }
    unsafe fn disconnect(&self, _other: *mut IConnectionPoint) -> tresult {
        self.core.connected.set(false);
        *self.core.peer.borrow_mut() = None;
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
// The window of the gain effect.

/// How big the window is before a host says another size: 300 by 200.
pub const VIEW_WIDTH: i32 = 300;
pub const VIEW_HEIGHT: i32 = 200;
/// The least and the most that the window says it can be (`checkSizeConstraint` brings a size inside them).
pub const VIEW_LEAST: (i32, i32) = (200, 150);
pub const VIEW_MOST: (i32, i32) = (800, 600);
/// The size that the key `r` asks the host for, and the one (more than the window allows) that the key `b` asks for.
pub const VIEW_ASKED: (i32, i32) = (640, 480);
pub const VIEW_ASKED_TOO_BIG: (i32, i32) = (2000, 1500);
/// The value of `Gain` (normalised) that the key `e` makes an edit of, and the one the key `v` sets and says has changed.
pub const VIEW_EDIT_NORMALIZED: f64 = 0.75;
pub const VIEW_VALUES_NORMALIZED: f64 = 0.5;

/// The window of [`GAIN`] and [`SINGLE`]. It does nothing but keep its size and do what a key says.
struct TestView {
    /// Its own pointer as a window (`resizeView` is given the view that asks), set when it is made.
    me: Cell<*mut IPlugView>,
    handler: Option<ComPtr<IComponentHandler>>,
    host: Option<ComPtr<IHostApplication>>,
    peer: Option<ComPtr<IConnectionPoint>>,
    values: Rc<RefCell<[f64; 32]>>,
    attached: Cell<bool>,
    size: Cell<(i32, i32)>,
    frame: RefCell<Option<ComPtr<IPlugFrame>>>,
    /// What the window registered with the host's run loop, to take away when it is taken out of its parent.
    #[cfg(target_os = "linux")]
    run_loop: RefCell<Option<RunLoopUse>>,
}

impl Class for TestView {
    type Interfaces = (IPlugView, IPlugViewContentScaleSupport);
}

impl Drop for TestView {
    fn drop(&mut self) {
        // (Strict: a window that is let go of while it is still in its parent is a host that did not take it out first; real plugins crash then, and this one ends the program.)
        if self.attached.get() {
            std::process::abort();
        }
    }
}

/// The kind of parent window that a window of this system is put in.
fn the_platform_type() -> &'static [u8] {
    if cfg!(target_os = "windows") {
        b"HWND"
    } else if cfg!(target_os = "macos") {
        b"NSView"
    } else {
        b"X11EmbedWindowID"
    }
}

impl TestView {
    /// Makes a message with the host that has this id and sends it to the other half. Whether it did.
    unsafe fn send_a_message_to_the_other_half(&self, id: &CStr) -> bool {
        let (Some(host), Some(peer)) = (&self.host, &self.peer) else {
            return false;
        };
        let (mut class, mut interface) = (IMessage_iid, IMessage_iid);
        let mut object: *mut c_void = ptr::null_mut();
        if host.createInstance(&mut class, &mut interface, &mut object) != kResultOk {
            return false;
        }
        let Some(message) = ComPtr::from_raw(object.cast::<IMessage>()) else {
            return false;
        };
        message.setMessageID(id.as_ptr());
        peer.notify(message.as_ptr()) == kResultOk
    }

    /// Makes a list and a message with the host, fills the message with a number, a float, a text and some bytes, and reads them back from a second look at its attributes. Whether all of it
    /// is as it was put.
    unsafe fn a_message_goes_through_the_host(&self) -> bool {
        let Some(host) = &self.host else {
            return false;
        };
        // A list on its own.
        let (mut class, mut interface) = (IAttributeList_iid, IAttributeList_iid);
        let mut object: *mut c_void = ptr::null_mut();
        if host.createInstance(&mut class, &mut interface, &mut object) != kResultOk
            || ComPtr::from_raw(object.cast::<IAttributeList>()).is_none()
        {
            return false;
        }
        // A message.
        let (mut class, mut interface) = (IMessage_iid, IMessage_iid);
        let mut object: *mut c_void = ptr::null_mut();
        if host.createInstance(&mut class, &mut interface, &mut object) != kResultOk {
            return false;
        }
        let Some(message) = ComPtr::from_raw(object.cast::<IMessage>()) else {
            return false;
        };
        message.setMessageID(c"parameter-set".as_ptr());
        let Some(attributes) = ComRef::from_raw(message.getAttributes()) else {
            return false;
        };
        let note: Vec<u16> = "hi".encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = [1_u8, 2, 3];
        let put = attributes.setInt(c"rindex".as_ptr(), i64::from(PARAMETER_GAIN))
            | attributes.setFloat(c"value".as_ptr(), VIEW_EDIT_NORMALIZED)
            | attributes.setString(c"note".as_ptr(), note.as_ptr())
            | attributes.setBinary(c"blob".as_ptr(), bytes.as_ptr().cast(), 3);
        if put != kResultOk {
            return false;
        }
        // The other half looks at the message again.
        let Some(again) = ComRef::from_raw(message.getAttributes()) else {
            return false;
        };
        let (mut number, mut float, mut text) = (0_i64, 0.0_f64, [0_u16; 8]);
        let (mut data, mut size): (*const c_void, u32) = (ptr::null(), 0);
        let got = again.getInt(c"rindex".as_ptr(), &mut number)
            | again.getFloat(c"value".as_ptr(), &mut float)
            | again.getString(c"note".as_ptr(), text.as_mut_ptr(), 16)
            | again.getBinary(c"blob".as_ptr(), &mut data, &mut size);
        got == kResultOk
            && number == i64::from(PARAMETER_GAIN)
            && float == VIEW_EDIT_NORMALIZED
            && text[..3] == note[..]
            && size == 3
            && slice::from_raw_parts(data.cast::<u8>(), 3) == bytes
            && CStr::from_ptr(message.getMessageID()).to_bytes() == b"parameter-set"
    }

    /// Says what the window was told, on the standard output, when the variable `FFRS_TEST_VIEW_PRINT` is set (for the tests of the program that shows windows, which read it).
    fn say(&self, words: String) {
        if std::env::var_os("FFRS_TEST_VIEW_PRINT").is_some() {
            println!("test view: {words}");
        }
    }

    /// What a hand would do, by the key that says it. Whether the key is one of them.
    unsafe fn press(&self, key: u8) -> bool {
        match key {
            b'e' => {
                // (A controller keeps the value it edits.)
                self.values.borrow_mut()[PARAMETER_GAIN as usize] = VIEW_EDIT_NORMALIZED;
                if let Some(handler) = &self.handler {
                    handler.beginEdit(PARAMETER_GAIN);
                    handler.performEdit(PARAMETER_GAIN, VIEW_EDIT_NORMALIZED);
                    handler.endEdit(PARAMETER_GAIN);
                }
                true
            }
            b'r' | b'b' => {
                let asked = if key == b'r' { VIEW_ASKED } else { VIEW_ASKED_TOO_BIG };
                let frame = self.frame.borrow().clone();
                if let Some(frame) = frame {
                    let mut wanted = ViewRect {
                        left: 0,
                        top: 0,
                        right: asked.0,
                        bottom: asked.1,
                    };
                    frame.resizeView(self.me.get(), &mut wanted);
                }
                true
            }
            b'm' => {
                // (What a plugin built with DPF does for every edit of its window: a message made by the host, filled, and read back by the other half. Only a message that comes back whole is an
                // edit.)
                if self.a_message_goes_through_the_host() {
                    self.press(b'e')
                } else {
                    false
                }
            }
            b'l' => {
                // (Linux: the window asks the host's run loop for a timer and a descriptor. Only when it is told to: the window of every other test is to make no event of its own.)
                #[cfg(target_os = "linux")]
                {
                    let frame = self.frame.borrow().clone();
                    if let Some(frame) = frame {
                        self.use_the_run_loop(&frame);
                        return true;
                    }
                }
                false
            }
            b's' => {
                // (A change of the state that is told to the other half in a message and to the host in nothing: a plugin built with DPF does that when a hand draws in its window.)
                self.send_a_message_to_the_other_half(c"bump-secret")
            }
            b'd' => {
                if let Some(handler) = self
                    .handler
                    .as_ref()
                    .and_then(|handler| handler.cast::<IComponentHandler2>())
                {
                    handler.setDirty(1);
                }
                true
            }
            b'w' => {
                // (Three values changed at once, as a preset chosen in a window does: `Gain` to 2, `Mode` to 2, `Bypass` on.)
                {
                    let mut values = self.values.borrow_mut();
                    values[PARAMETER_GAIN as usize] = VIEW_VALUES_NORMALIZED;
                    values[PARAMETER_MODE as usize] = 1.0;
                    values[PARAMETER_BYPASS as usize] = 1.0;
                }
                if let Some(handler) = &self.handler {
                    handler.restartComponent(RestartFlags_::kParamValuesChanged as _);
                }
                true
            }
            b'v' => {
                self.values.borrow_mut()[PARAMETER_GAIN as usize] = VIEW_VALUES_NORMALIZED;
                if let Some(handler) = &self.handler {
                    handler.restartComponent(RestartFlags_::kParamValuesChanged as _);
                }
                true
            }
            _ => false,
        }
    }
}

impl IPlugViewTrait for TestView {
    unsafe fn isPlatformTypeSupported(&self, kind: FIDString) -> tresult {
        if !kind.is_null() && CStr::from_ptr(kind).to_bytes() == the_platform_type() {
            kResultTrue
        } else {
            kResultFalse
        }
    }
    unsafe fn attached(&self, parent: *mut c_void, kind: FIDString) -> tresult {
        misbehave_if_asked("crash_when_attached");
        misbehave_if_asked("hang_when_attached");
        // (Strict: a window wants a parent, of the kind of this system, and is put in one only once.)
        if parent.is_null() || self.isPlatformTypeSupported(kind) != kResultTrue || self.attached.get() {
            return kResultFalse;
        }
        self.attached.set(true);
        self.say(format!(
            "attached parent={} kind={}",
            parent as usize,
            String::from_utf8_lossy(the_platform_type())
        ));
        if let Ok(keys) = std::env::var("FFRS_TEST_VIEW_KEYS") {
            for key in keys.bytes() {
                self.press(key);
            }
        }
        kResultOk
    }
    unsafe fn removed(&self) -> tresult {
        if !self.attached.replace(false) {
            return kResultFalse;
        }
        // (What the window asked of the run loop is taken away with it.)
        #[cfg(target_os = "linux")]
        {
            *self.run_loop.borrow_mut() = None;
        }
        self.say("removed".to_owned());
        kResultOk
    }
    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kResultFalse
    }
    unsafe fn onKeyDown(&self, key: char16, _code: int16, _modifiers: int16) -> tresult {
        if self.attached.get() && u8::try_from(key).is_ok_and(|key| self.press(key)) {
            kResultTrue
        } else {
            kResultFalse
        }
    }
    unsafe fn onKeyUp(&self, _key: char16, _code: int16, _modifiers: int16) -> tresult {
        kResultFalse
    }
    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        let (width, height) = self.size.get();
        *size = ViewRect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        kResultOk
    }
    unsafe fn onSize(&self, size: *mut ViewRect) -> tresult {
        if !self.attached.get() {
            return kResultFalse;
        }
        let rect = &*size;
        self.size.set((rect.right - rect.left, rect.bottom - rect.top));
        self.say(format!("onSize {}x{}", rect.right - rect.left, rect.bottom - rect.top));
        kResultOk
    }
    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }
    unsafe fn setFrame(&self, frame: *mut IPlugFrame) -> tresult {
        *self.frame.borrow_mut() = ComRef::from_raw(frame).map(|frame| frame.to_com_ptr());
        self.say(format!("setFrame {}", if frame.is_null() { "none" } else { "given" }));
        kResultOk
    }
    unsafe fn canResize(&self) -> tresult {
        kResultTrue
    }
    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        let rect = &mut *rect;
        let width = (rect.right - rect.left).clamp(VIEW_LEAST.0, VIEW_MOST.0);
        let height = (rect.bottom - rect.top).clamp(VIEW_LEAST.1, VIEW_MOST.1);
        rect.right = rect.left + width;
        rect.bottom = rect.top + height;
        kResultTrue
    }
}

impl IPlugViewContentScaleSupportTrait for TestView {
    unsafe fn setContentScaleFactor(&self, factor: IPlugViewContentScaleSupport_::ScaleFactor) -> tresult {
        self.say(format!("scale {factor}"));
        // (A window that has not been given a size by the host is as big as its scale makes it.)
        if !self.attached.get() {
            self.size.set((
                (f64::from(VIEW_WIDTH) * f64::from(factor)).round() as i32,
                (f64::from(VIEW_HEIGHT) * f64::from(factor)).round() as i32,
            ));
        }
        kResultOk
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The window's use of the host's run loop (Linux): a timer, and a pipe that it has something to read from.

/// What the window of the test plugin asks of the run loop of the host on Linux, as a plugin's window does with its timers and its connection to the display: a timer that is called every
/// 10 ms and, at its third call, makes the edit of the key `e`; and a pipe whose read end is looked at, one byte of which is put in it when the window is put in its parent, which makes the
/// request for another size of the key `r` when it is read.
#[cfg(target_os = "linux")]
struct RunLoopUse {
    run_loop: ComPtr<IRunLoop>,
    timer: ComWrapper<TestTimer>,
    watcher: ComWrapper<TestWatcher>,
    pipe: [i32; 2],
}

#[cfg(target_os = "linux")]
impl Drop for RunLoopUse {
    fn drop(&mut self) {
        // SAFETY: the run loop is live; the handlers are the ones that were registered; the descriptors are the pipe's, which this made and closes.
        unsafe {
            if let Some(timer) = self.timer.to_com_ptr::<ITimerHandler>() {
                self.run_loop.unregisterTimer(timer.as_ptr());
            }
            if let Some(watcher) = self.watcher.to_com_ptr::<IEventHandler>() {
                self.run_loop.unregisterEventHandler(watcher.as_ptr());
            }
            libc::close(self.pipe[0]);
            libc::close(self.pipe[1]);
        }
    }
}

#[cfg(target_os = "linux")]
struct TestTimer {
    ticks: Cell<u32>,
    handler: Option<ComPtr<IComponentHandler>>,
    values: Rc<RefCell<[f64; 32]>>,
}

#[cfg(target_os = "linux")]
impl Class for TestTimer {
    type Interfaces = (ITimerHandler,);
}

#[cfg(target_os = "linux")]
impl ITimerHandlerTrait for TestTimer {
    unsafe fn onTimer(&self) {
        self.ticks.set(self.ticks.get() + 1);
        if self.ticks.get() == 3
            && let Some(handler) = &self.handler
        {
            self.values.borrow_mut()[PARAMETER_GAIN as usize] = VIEW_EDIT_NORMALIZED;
            handler.beginEdit(PARAMETER_GAIN);
            handler.performEdit(PARAMETER_GAIN, VIEW_EDIT_NORMALIZED);
            handler.endEdit(PARAMETER_GAIN);
        }
    }
}

#[cfg(target_os = "linux")]
struct TestWatcher {
    descriptor: i32,
    view: *const TestView,
}

#[cfg(target_os = "linux")]
impl Class for TestWatcher {
    type Interfaces = (IEventHandler,);
}

#[cfg(target_os = "linux")]
impl IEventHandlerTrait for TestWatcher {
    unsafe fn onFDIsSet(&self, descriptor: FileDescriptor) {
        // (Strict: it is called for the descriptor that it was registered for, and only when there is something to read.)
        if descriptor != self.descriptor {
            return;
        }
        let mut byte = 0_u8;
        if libc::read(descriptor, (&mut byte as *mut u8).cast(), 1) == 1 {
            // SAFETY: the view outlives its watcher (the watcher is taken out of the run loop when the view is taken out of its parent, before the view goes).
            (*self.view).press(b'r');
        }
    }
}

#[cfg(target_os = "linux")]
impl TestView {
    /// Asks the host's run loop (the frame's `IRunLoop`) for a timer and a descriptor.
    unsafe fn use_the_run_loop(&self, frame: &ComPtr<IPlugFrame>) {
        let Some(run_loop) = frame.cast::<IRunLoop>() else {
            return;
        };
        let mut pipe = [0_i32; 2];
        if libc::pipe(pipe.as_mut_ptr()) != 0 {
            return;
        }
        let timer = ComWrapper::new(TestTimer {
            ticks: Cell::new(0),
            handler: self.handler.clone(),
            values: Rc::clone(&self.values),
        });
        let watcher = ComWrapper::new(TestWatcher {
            descriptor: pipe[0],
            view: self,
        });
        if let (Some(timer_pointer), Some(watcher_pointer)) = (
            timer.to_com_ptr::<ITimerHandler>(),
            watcher.to_com_ptr::<IEventHandler>(),
        ) {
            run_loop.registerTimer(timer_pointer.as_ptr(), 10);
            run_loop.registerEventHandler(watcher_pointer.as_ptr(), pipe[0]);
            // (Something to read: the host has to call the watcher for it.)
            libc::write(pipe[1], [1_u8].as_ptr().cast(), 1);
            *self.run_loop.borrow_mut() = Some(RunLoopUse {
                run_loop,
                timer,
                watcher,
                pipe,
            });
        }
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
    unsafe fn initialize(&self, context: *mut FUnknown) -> tresult {
        self.core.keep_the_host(context);
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
    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        // (The message that the test window sends, as the window of a plugin built with DPF sends its edits of the state: it changes what the component keeps, and nothing is said to the host.)
        let Some(message) = ComRef::from_raw(message) else {
            return kInvalidArgument;
        };
        let id = message.getMessageID();
        if !id.is_null() && CStr::from_ptr(id).to_bytes() == b"bump-secret" {
            let mut values = self.values.borrow_mut();
            let place = &mut values[PARAMETER_SECRET as usize];
            *place = (*place + 0.25).min(1.0);
        }
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
