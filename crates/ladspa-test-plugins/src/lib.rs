// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: CC0-1.0

//! A LADSPA library for the tests of fastforwardrs (phase 7). It is written **without** the types of the `ladspa-host` crate, from `ladspa.h`, on purpose: a mistake in the
//! host's idea of the interface would otherwise be a mistake in the plugin as well, and the two would agree with each other and not with every plugin in the world.
//!
//! It has nine plugins, so that the tests do not depend on anybody's install:
//!
//! | label | what it has |
//! |---|---|
//! | `ffrs_test_gain` | a control (`Gain`, 0 to 4, default 1), an audio input and an audio output: the output is the input times the gain |
//! | `ffrs_test_one_pole` | a control that is a frequency (its bounds are fractions of the sample rate, logarithmic, default low) and a filter that remembers: a low-pass |
//! | `ffrs_test_swap` | two audio inputs and two audio outputs: left and right swapped |
//! | `ffrs_test_switches` | a toggle, a whole number, a control output (the highest value of the last block), an input and an output |
//! | `ffrs_test_no_ports` | no ports at all |
//! | `ffrs_test_sine` | no audio input (a generator): a sine wave of the frequency of a control |
//! | `ffrs_test_mono_to_both` | one audio input and two audio outputs |
//! | `ffrs_test_mix_down` | two audio inputs and one audio output |
//! | `ffrs_test_counted` | passes the sound through, and counts what the host does to it (instances made and cleaned up, activations, deactivations, blocks run, ports not connected when it was activated): `ffrs_test_counts` gives the numbers |
//!
//! **It misbehaves on request**: the environment variable `FFRS_TEST_PLUGIN` makes the library crash, never return, print noise, say nothing, or fail in a place the tests ask
//! for: see `Misbehaviour` below. (The tests that use it run the library in a child process, which is what the scan is for.)

#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, c_ulong, c_void};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

// --- ladspa.h, as written for this library ---

#[repr(C)]
struct PortRangeHint {
    hint_descriptor: c_int,
    lower_bound: f32,
    upper_bound: f32,
}

#[repr(C)]
pub struct Descriptor {
    unique_id: c_ulong,
    label: *const c_char,
    properties: c_int,
    name: *const c_char,
    maker: *const c_char,
    copyright: *const c_char,
    port_count: c_ulong,
    port_descriptors: *const c_int,
    port_names: *const *const c_char,
    port_range_hints: *const PortRangeHint,
    implementation_data: *mut c_void,
    instantiate: Option<unsafe extern "C" fn(*const Descriptor, c_ulong) -> *mut c_void>,
    connect_port: Option<unsafe extern "C" fn(*mut c_void, c_ulong, *mut f32)>,
    activate: Option<unsafe extern "C" fn(*mut c_void)>,
    run: Option<unsafe extern "C" fn(*mut c_void, c_ulong)>,
    run_adding: Option<unsafe extern "C" fn(*mut c_void, c_ulong)>,
    set_run_adding_gain: Option<unsafe extern "C" fn(*mut c_void, f32)>,
    deactivate: Option<unsafe extern "C" fn(*mut c_void)>,
    cleanup: Option<unsafe extern "C" fn(*mut c_void)>,
}

const IN: c_int = 1;
const OUT: c_int = 2;
const CONTROL: c_int = 4;
const AUDIO: c_int = 8;

const BELOW: c_int = 0x1;
const ABOVE: c_int = 0x2;
const TOGGLED: c_int = 0x4;
const SAMPLE_RATE: c_int = 0x8;
const LOGARITHMIC: c_int = 0x10;
const INTEGER: c_int = 0x20;
const DEFAULT_LOW: c_int = 0x80;
const DEFAULT_MIDDLE: c_int = 0xC0;
const DEFAULT_ZERO: c_int = 0x200;
const DEFAULT_ONE: c_int = 0x240;
const DEFAULT_440: c_int = 0x2C0;

/// How the library is asked to misbehave: the value of `FFRS_TEST_PLUGIN`.
#[derive(PartialEq, Eq)]
enum Misbehaviour {
    /// It behaves.
    None,
    /// Crashes the first time it is asked for a plugin.
    CrashWhenDescribed,
    /// Never returns when it is asked for a plugin.
    HangWhenDescribed,
    /// Writes a line of a wrong answer for the scan and ends the program, as a library that writes to the standard output and calls `exit` would.
    AnswerWrongWhenDescribed,
    /// Ends the program, successfully and quietly, when it is asked for a plugin.
    ExitWhenDescribed,
    /// Writes noise to the standard output every time it is asked for a plugin, and otherwise behaves.
    PrintNoise,
    /// Has no plugins.
    NoPlugins,
    /// Has a plugin after the ninth whose descriptor is not usable: it says it has three ports and does not say what they are.
    BrokenDescriptor,
    /// Has a plugin after the ninth that says it has a million ports (and lists three).
    AbsurdPortCount,
    /// Has no last plugin: it gives a descriptor for any number it is asked for.
    EndlessPlugins,
    /// Crashes when a plugin is made.
    CrashWhenMade,
    /// Never returns when a plugin is made.
    HangWhenMade,
    /// Gives nothing when a plugin is made.
    FailToMake,
    /// Crashes when a plugin is run.
    CrashWhenRun,
}

fn misbehaviour() -> Misbehaviour {
    match std::env::var("FFRS_TEST_PLUGIN").unwrap_or_default().as_str() {
        "crash_when_described" => Misbehaviour::CrashWhenDescribed,
        "hang_when_described" => Misbehaviour::HangWhenDescribed,
        "answer_wrong_when_described" => Misbehaviour::AnswerWrongWhenDescribed,
        "exit_when_described" => Misbehaviour::ExitWhenDescribed,
        "print_noise" => Misbehaviour::PrintNoise,
        "no_plugins" => Misbehaviour::NoPlugins,
        "broken_descriptor" => Misbehaviour::BrokenDescriptor,
        "absurd_port_count" => Misbehaviour::AbsurdPortCount,
        "endless_plugins" => Misbehaviour::EndlessPlugins,
        "crash_when_made" => Misbehaviour::CrashWhenMade,
        "hang_when_made" => Misbehaviour::HangWhenMade,
        "fail_to_make" => Misbehaviour::FailToMake,
        "crash_when_run" => Misbehaviour::CrashWhenRun,
        _ => Misbehaviour::None,
    }
}

fn crash() -> ! {
    // SAFETY: none: this is a write to address 0, which is the point. (It crashes the program, which is what the tests ask for.)
    unsafe { std::ptr::null_mut::<u8>().write_volatile(1) };
    std::process::abort()
}

fn hang() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

// --- the plugins ---

#[derive(Clone, Copy)]
enum Kind {
    Gain,
    OnePole,
    Swap,
    Switches,
    NoPorts,
    Sine,
    MonoToBoth,
    MixDown,
    Counted,
}

const KINDS: [Kind; 9] = [
    Kind::Gain,
    Kind::OnePole,
    Kind::Swap,
    Kind::Switches,
    Kind::NoPorts,
    Kind::Sine,
    Kind::MonoToBoth,
    Kind::MixDown,
    Kind::Counted,
];

/// The first of the unique ids of the plugins (the ones from 1 to 1000 are for private use).
const FIRST_ID: c_ulong = 901;

/// The id of the plugin whose descriptor is not usable.
const BROKEN_ID: c_ulong = 999;

struct PortSpecification {
    name: &'static str,
    bits: c_int,
    hint: c_int,
    lower: f32,
    upper: f32,
}

const fn port(name: &'static str, bits: c_int, hint: c_int, lower: f32, upper: f32) -> PortSpecification {
    PortSpecification {
        name,
        bits,
        hint,
        lower,
        upper,
    }
}

fn ports_of(kind: Kind) -> Vec<PortSpecification> {
    match kind {
        Kind::Gain => vec![
            port("Gain", IN | CONTROL, BELOW | ABOVE | DEFAULT_ONE, 0.0, 4.0),
            port("Input", IN | AUDIO, 0, 0.0, 0.0),
            port("Output", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::OnePole => vec![
            port(
                "Cutoff",
                IN | CONTROL,
                BELOW | ABOVE | SAMPLE_RATE | LOGARITHMIC | DEFAULT_LOW,
                0.001,
                0.45,
            ),
            port("Input", IN | AUDIO, 0, 0.0, 0.0),
            port("Output", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::Swap => vec![
            port("Input left", IN | AUDIO, 0, 0.0, 0.0),
            port("Input right", IN | AUDIO, 0, 0.0, 0.0),
            port("Output left", OUT | AUDIO, 0, 0.0, 0.0),
            port("Output right", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::Switches => vec![
            port("Invert", IN | CONTROL, TOGGLED | DEFAULT_ZERO, 0.0, 0.0),
            port(
                "Multiplier",
                IN | CONTROL,
                BELOW | ABOVE | INTEGER | DEFAULT_MIDDLE,
                1.0,
                8.0,
            ),
            port("Peak", OUT | CONTROL, 0, 0.0, 0.0),
            port("Input", IN | AUDIO, 0, 0.0, 0.0),
            port("Output", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::NoPorts => Vec::new(),
        Kind::Sine => vec![
            port(
                "Frequency",
                IN | CONTROL,
                BELOW | ABOVE | LOGARITHMIC | DEFAULT_440,
                20.0,
                20000.0,
            ),
            port("Output", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::MonoToBoth => vec![
            port("Input", IN | AUDIO, 0, 0.0, 0.0),
            port("Output left", OUT | AUDIO, 0, 0.0, 0.0),
            port("Output right", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::MixDown => vec![
            port("Input left", IN | AUDIO, 0, 0.0, 0.0),
            port("Input right", IN | AUDIO, 0, 0.0, 0.0),
            port("Output", OUT | AUDIO, 0, 0.0, 0.0),
        ],
        Kind::Counted => vec![
            port("Value", IN | CONTROL, BELOW | ABOVE | DEFAULT_ZERO, 0.0, 1.0),
            port("Input", IN | AUDIO, 0, 0.0, 0.0),
            port("Output", OUT | AUDIO, 0, 0.0, 0.0),
        ],
    }
}

fn label_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Gain => "ffrs_test_gain\0",
        Kind::OnePole => "ffrs_test_one_pole\0",
        Kind::Swap => "ffrs_test_swap\0",
        Kind::Switches => "ffrs_test_switches\0",
        Kind::NoPorts => "ffrs_test_no_ports\0",
        Kind::Sine => "ffrs_test_sine\0",
        Kind::MonoToBoth => "ffrs_test_mono_to_both\0",
        Kind::MixDown => "ffrs_test_mix_down\0",
        Kind::Counted => "ffrs_test_counted\0",
    }
}

/// The descriptors, made once and never changed or freed, which is what makes pointers to them good for as long as the library is loaded.
struct Table {
    plugins: Vec<Descriptor>,
    /// The plugin that is only there on request, which does not say what its ports are.
    broken: Descriptor,
    /// The one that says it has a million ports.
    absurd: Descriptor,
}

// SAFETY: the descriptors are made once, before anyone can see them, and are never changed; the pointers in them point at memory that is leaked and so never freed.
unsafe impl Sync for Table {}
// SAFETY: as above.
unsafe impl Send for Table {}

static TABLE: OnceLock<Table> = OnceLock::new();

fn c_string(text: &str) -> *const c_char {
    let mut bytes = text.as_bytes().to_vec();
    if bytes.last() != Some(&0) {
        bytes.push(0);
    }
    Box::leak(bytes.into_boxed_slice()).as_ptr().cast()
}

fn descriptor_of(id: c_ulong, label: &str, ports: &[PortSpecification]) -> Descriptor {
    let bits: Vec<c_int> = ports.iter().map(|port| port.bits).collect();
    let names: Vec<*const c_char> = ports.iter().map(|port| c_string(port.name)).collect();
    let hints: Vec<PortRangeHint> = ports
        .iter()
        .map(|port| PortRangeHint {
            hint_descriptor: port.hint,
            lower_bound: port.lower,
            upper_bound: port.upper,
        })
        .collect();
    Descriptor {
        unique_id: id,
        label: c_string(label),
        properties: 4,
        name: c_string(&format!("Test plugin {label}")),
        maker: c_string("fastforwardrs tests"),
        copyright: c_string("CC0-1.0"),
        port_count: ports.len() as c_ulong,
        port_descriptors: Box::leak(bits.into_boxed_slice()).as_ptr(),
        port_names: Box::leak(names.into_boxed_slice()).as_ptr(),
        port_range_hints: Box::leak(hints.into_boxed_slice()).as_ptr(),
        implementation_data: std::ptr::null_mut(),
        instantiate: Some(instantiate),
        connect_port: Some(connect_port),
        activate: Some(activate),
        run: Some(run),
        run_adding: None,
        set_run_adding_gain: None,
        deactivate: Some(deactivate),
        cleanup: Some(cleanup),
    }
}

fn table() -> &'static Table {
    TABLE.get_or_init(|| {
        let plugins: Vec<Descriptor> = KINDS
            .iter()
            .enumerate()
            .map(|(number, &kind)| descriptor_of(FIRST_ID + number as c_ulong, label_of(kind), &ports_of(kind)))
            .collect();
        // The plugins that are only there on request. The first says it has three ports and does not say what they are...
        let mut broken = descriptor_of(BROKEN_ID, "ffrs_test_broken\0", &[]);
        broken.port_count = 3;
        broken.port_descriptors = std::ptr::null();
        // ...and the second lists three ports and says it has a million.
        let mut absurd = descriptor_of(BROKEN_ID + 1, "ffrs_test_absurd\0", &ports_of(Kind::Gain));
        absurd.port_count = 1_000_000;
        Table {
            plugins,
            broken,
            absurd,
        }
    })
}

/// The one function that a LADSPA library exports.
///
/// # Safety
///
/// Any number may be given.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ladspa_descriptor(index: c_ulong) -> *const Descriptor {
    match misbehaviour() {
        Misbehaviour::CrashWhenDescribed => crash(),
        Misbehaviour::HangWhenDescribed => hang(),
        Misbehaviour::AnswerWrongWhenDescribed => {
            println!("ffrs-scan-result: this is not json");
            std::process::exit(0)
        }
        Misbehaviour::ExitWhenDescribed => std::process::exit(0),
        Misbehaviour::PrintNoise => {
            println!("noise from a plugin library, which is not an answer");
            println!("ffrs-scan: neither is this");
        }
        Misbehaviour::NoPlugins => return std::ptr::null(),
        _ => {}
    }
    let index = index as usize;
    let table = table();
    match (misbehaviour(), index) {
        (_, index) if index < KINDS.len() => &table.plugins[index],
        (Misbehaviour::BrokenDescriptor, _) if index == KINDS.len() => &table.broken,
        (Misbehaviour::AbsurdPortCount, _) if index == KINDS.len() => &table.absurd,
        // (For ever: any number is a plugin.)
        (Misbehaviour::EndlessPlugins, index) => &table.plugins[index % KINDS.len()],
        _ => std::ptr::null(),
    }
}

// --- what the host does to the plugin that counts ---

static LIVE: AtomicUsize = AtomicUsize::new(0);
static ACTIVATIONS: AtomicUsize = AtomicUsize::new(0);
static DEACTIVATIONS: AtomicUsize = AtomicUsize::new(0);
static BLOCKS_RUN: AtomicUsize = AtomicUsize::new(0);
static PORTS_NOT_CONNECTED_WHEN_ACTIVATED: AtomicUsize = AtomicUsize::new(0);

/// What the plugin `ffrs_test_counted` has been through, in this process: 0 instances that are made and not cleaned up, 1 activations, 2 deactivations, 3 blocks run, 4 ports that
/// were not connected when an instance was activated. (Any other number gives 0.)
#[unsafe(no_mangle)]
pub extern "C" fn ffrs_test_counts(which: c_int) -> usize {
    match which {
        0 => LIVE.load(Ordering::SeqCst),
        1 => ACTIVATIONS.load(Ordering::SeqCst),
        2 => DEACTIVATIONS.load(Ordering::SeqCst),
        3 => BLOCKS_RUN.load(Ordering::SeqCst),
        4 => PORTS_NOT_CONNECTED_WHEN_ACTIVATED.load(Ordering::SeqCst),
        _ => 0,
    }
}

// --- an instance ---

struct Instance {
    kind: Kind,
    sample_rate: f32,
    ports: [*mut f32; 8],
    /// What a plugin remembers: the last output of the filter, and the phase of the sine.
    memory: f32,
}

unsafe extern "C" fn instantiate(descriptor: *const Descriptor, sample_rate: c_ulong) -> *mut c_void {
    match misbehaviour() {
        Misbehaviour::CrashWhenMade => crash(),
        Misbehaviour::HangWhenMade => hang(),
        Misbehaviour::FailToMake => return std::ptr::null_mut(),
        _ => {}
    }
    // SAFETY: the host gives the descriptor that this library gave it.
    let id = unsafe { (*descriptor).unique_id };
    let Some(&kind) = KINDS.get(id.wrapping_sub(FIRST_ID) as usize) else {
        return std::ptr::null_mut();
    };
    if matches!(kind, Kind::Counted) {
        LIVE.fetch_add(1, Ordering::SeqCst);
    }
    Box::into_raw(Box::new(Instance {
        kind,
        sample_rate: sample_rate as f32,
        ports: [std::ptr::null_mut(); 8],
        memory: 0.0,
    }))
    .cast()
}

unsafe extern "C" fn connect_port(handle: *mut c_void, port: c_ulong, location: *mut f32) {
    // SAFETY: the host gives the handle that `instantiate` made, which `cleanup` has not freed.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    if let Some(slot) = instance.ports.get_mut(port as usize) {
        *slot = location;
    }
}

unsafe extern "C" fn activate(handle: *mut c_void) {
    // SAFETY: as in `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    instance.memory = 0.0;
    if matches!(instance.kind, Kind::Counted) {
        ACTIVATIONS.fetch_add(1, Ordering::SeqCst);
        let ports = ports_of(instance.kind).len();
        let not_connected = instance.ports[..ports]
            .iter()
            .filter(|location| location.is_null())
            .count();
        PORTS_NOT_CONNECTED_WHEN_ACTIVATED.fetch_add(not_connected, Ordering::SeqCst);
    }
}

unsafe extern "C" fn deactivate(handle: *mut c_void) {
    // SAFETY: as in `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    if matches!(instance.kind, Kind::Counted) {
        DEACTIVATIONS.fetch_add(1, Ordering::SeqCst);
    }
}

unsafe extern "C" fn cleanup(handle: *mut c_void) {
    // SAFETY: the host gives the handle that `instantiate` made, once, and does not use it again.
    let instance = unsafe { Box::from_raw(handle.cast::<Instance>()) };
    if matches!(instance.kind, Kind::Counted) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

unsafe extern "C" fn run(handle: *mut c_void, sample_count: c_ulong) {
    if misbehaviour() == Misbehaviour::CrashWhenRun {
        crash();
    }
    // SAFETY: as in `connect_port`.
    let instance = unsafe { &mut *handle.cast::<Instance>() };
    let count = sample_count as usize;
    let ports = instance.ports;
    // SAFETY (for each read and write below): the host has connected every port to memory of at least `sample_count` moments (a control to one), and that is the contract.
    unsafe {
        match instance.kind {
            Kind::Gain => {
                let gain = *ports[0];
                for moment in 0..count {
                    *ports[2].add(moment) = *ports[1].add(moment) * gain;
                }
            }
            Kind::OnePole => {
                let hertz = *ports[0];
                let coefficient = 1.0 - (-std::f32::consts::TAU * hertz / instance.sample_rate).exp();
                for moment in 0..count {
                    instance.memory += coefficient * (*ports[1].add(moment) - instance.memory);
                    *ports[2].add(moment) = instance.memory;
                }
            }
            Kind::Swap => {
                for moment in 0..count {
                    let (left, right) = (*ports[0].add(moment), *ports[1].add(moment));
                    *ports[2].add(moment) = right;
                    *ports[3].add(moment) = left;
                }
            }
            Kind::Switches => {
                let sign = if *ports[0] > 0.0 { -1.0 } else { 1.0 };
                let multiplier = *ports[1];
                let mut peak = 0.0_f32;
                for moment in 0..count {
                    let value = *ports[3].add(moment) * multiplier * sign;
                    *ports[4].add(moment) = value;
                    peak = peak.max(value.abs());
                }
                *ports[2] = peak;
            }
            Kind::NoPorts => {}
            Kind::Sine => {
                let step = std::f32::consts::TAU * *ports[0] / instance.sample_rate;
                for moment in 0..count {
                    *ports[1].add(moment) = instance.memory.sin();
                    instance.memory = (instance.memory + step) % std::f32::consts::TAU;
                }
            }
            Kind::MonoToBoth => {
                for moment in 0..count {
                    let value = *ports[0].add(moment);
                    *ports[1].add(moment) = value;
                    *ports[2].add(moment) = value;
                }
            }
            Kind::MixDown => {
                for moment in 0..count {
                    *ports[2].add(moment) = *ports[0].add(moment) + *ports[1].add(moment);
                }
            }
            Kind::Counted => {
                BLOCKS_RUN.fetch_add(1, Ordering::SeqCst);
                for moment in 0..count {
                    *ports[2].add(moment) = *ports[1].add(moment);
                }
            }
        }
    }
}

// --- for the tests that use the library (the rest of this file is not part of the plugins) ---

/// Where the file of this library is, **for a test (or any program) that was built by the same `cargo` run as this crate's library** and depends on this crate: `cargo`
/// builds the library beside the program, in the same folder. (Nothing is built by this function.)
pub fn library_file_for_tests() -> std::path::PathBuf {
    let program = std::env::current_exe().expect("a program knows where it is");
    let name = format!(
        "{}ladspa_test_plugins{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let file = program.parent().expect("a program is in a folder").join(name);
    assert!(
        file.exists(),
        "the library of the test plugins is not at {} (the program that looks for it has to have the crate ladspa-test-plugins as a dependency)",
        file.display()
    );
    file
}

/// Libraries that cannot be loaded, made from the good one, for the tests of what happens when a library is bad.
pub mod bad_libraries {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A file with a library's name and nothing in it.
    pub fn with_nothing_in_it(folder: &Path) -> PathBuf {
        let file = folder.join(format!(
            "{}empty{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        fs::write(&file, b"").expect("the file is written");
        file
    }

    /// A file with a library's name and bytes that are not a library.
    pub fn damaged(folder: &Path) -> PathBuf {
        let file = folder.join(format!(
            "{}damaged{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        let bytes: Vec<u8> = (0..4096_u32)
            .map(|number| (number.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        fs::write(&file, bytes).expect("the file is written");
        file
    }

    /// A copy of the good library in which the name of a library that it needs is spelled so that there is none by that name: the system's loader refuses it, with the
    /// message it gives for a missing dependency.
    pub fn needing_a_library_that_is_not_there(folder: &Path) -> PathBuf {
        let mut bytes = fs::read(super::library_file_for_tests()).expect("the library is read");
        let (name, spelled_wrong): (&[u8], &[u8]) = if cfg!(windows) {
            (b"kernel32.dll", b"kernel3x.dll")
        } else if cfg!(target_os = "macos") {
            (b"libSystem.B.dylib", b"libSystem.B.dyliX")
        } else {
            (b"libc.so.6", b"libc.so.7")
        };
        let mut found = 0;
        let mut at = 0;
        while at + name.len() <= bytes.len() {
            // (The names of libraries are not case-sensitive in a Windows file.)
            if bytes[at..at + name.len()].eq_ignore_ascii_case(name) {
                bytes[at..at + name.len()].copy_from_slice(spelled_wrong);
                found += 1;
                at += name.len();
            } else {
                at += 1;
            }
        }
        assert!(
            found > 0,
            "the library does not name a library that needs: {}",
            String::from_utf8_lossy(name)
        );
        let file: PathBuf = folder.join(format!(
            "{}needing_something{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        fs::write(&file, bytes).expect("the file is written");
        file
    }
}
