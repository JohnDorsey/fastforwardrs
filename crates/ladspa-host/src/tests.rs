// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The facts of the interface and of the hints, and a count of the `unsafe` code of this crate. (What loading a real library does is in `tests/test_plugins.rs`.)

use std::ffi::c_ulong;
use std::mem::size_of;

use crate::abi::{self, RawDescriptor, RawPortRangeHint};
use crate::description::{DefaultHint, PortDescription, PortDirection, PortHint, PortKind};

// --- the interface ---

#[test]
fn an_unsigned_long_is_32_bits_on_windows_and_64_on_the_other_64_bit_systems() {
    // (The reason that the fields are `c_ulong` and not `u64`: see `abi`.)
    if cfg!(windows) {
        assert_eq!(size_of::<c_ulong>(), 4);
    } else if cfg!(target_pointer_width = "64") {
        assert_eq!(size_of::<c_ulong>(), 8);
    }
}

/// Compiles only if the unique id, the number of ports, and the arguments of the functions that are numbers are `unsigned long`.
#[allow(dead_code)]
fn the_numbers_of_the_interface_are_unsigned_longs(descriptor: &RawDescriptor) -> (c_ulong, c_ulong) {
    let _: Option<unsafe extern "C" fn(*const RawDescriptor, c_ulong) -> abi::Handle> = descriptor.instantiate;
    let _: Option<unsafe extern "C" fn(abi::Handle, c_ulong, *mut f32)> = descriptor.connect_port;
    let _: Option<unsafe extern "C" fn(abi::Handle, c_ulong)> = descriptor.run;
    (descriptor.unique_id, descriptor.port_count)
}

/// The places that a C compiler puts the fields of `LADSPA_Descriptor` in, on a system whose pointers are 64 bits, whether its `unsigned long` is 32 bits or 64 (the padding
/// before the next pointer makes them the same): worked out by hand from `ladspa.h`.
#[test]
#[cfg(target_pointer_width = "64")]
fn the_fields_of_the_descriptor_are_where_a_c_compiler_puts_them() {
    use std::mem::{align_of, offset_of};
    assert_eq!(offset_of!(RawDescriptor, unique_id), 0);
    assert_eq!(offset_of!(RawDescriptor, label), 8);
    assert_eq!(offset_of!(RawDescriptor, properties), 16);
    assert_eq!(offset_of!(RawDescriptor, name), 24);
    assert_eq!(offset_of!(RawDescriptor, maker), 32);
    assert_eq!(offset_of!(RawDescriptor, copyright), 40);
    assert_eq!(offset_of!(RawDescriptor, port_count), 48);
    assert_eq!(offset_of!(RawDescriptor, port_descriptors), 56);
    assert_eq!(offset_of!(RawDescriptor, port_names), 64);
    assert_eq!(offset_of!(RawDescriptor, port_range_hints), 72);
    assert_eq!(offset_of!(RawDescriptor, implementation_data), 80);
    assert_eq!(offset_of!(RawDescriptor, instantiate), 88);
    assert_eq!(offset_of!(RawDescriptor, connect_port), 96);
    assert_eq!(offset_of!(RawDescriptor, activate), 104);
    assert_eq!(offset_of!(RawDescriptor, run), 112);
    assert_eq!(offset_of!(RawDescriptor, run_adding), 120);
    assert_eq!(offset_of!(RawDescriptor, set_run_adding_gain), 128);
    assert_eq!(offset_of!(RawDescriptor, deactivate), 136);
    assert_eq!(offset_of!(RawDescriptor, cleanup), 144);
    assert_eq!(size_of::<RawDescriptor>(), 152);
    assert_eq!(align_of::<RawDescriptor>(), 8);
}

#[test]
fn the_range_hint_is_an_int_and_two_floats() {
    use std::mem::offset_of;
    assert_eq!(offset_of!(RawPortRangeHint, hint_descriptor), 0);
    assert_eq!(offset_of!(RawPortRangeHint, lower_bound), 4);
    assert_eq!(offset_of!(RawPortRangeHint, upper_bound), 8);
    assert_eq!(size_of::<RawPortRangeHint>(), 12);
}

#[test]
fn the_flags_are_the_values_of_ladspa_h() {
    assert_eq!(
        [abi::PORT_INPUT, abi::PORT_OUTPUT, abi::PORT_CONTROL, abi::PORT_AUDIO],
        [1, 2, 4, 8]
    );
    assert_eq!(
        [
            abi::HINT_BOUNDED_BELOW,
            abi::HINT_BOUNDED_ABOVE,
            abi::HINT_TOGGLED,
            abi::HINT_SAMPLE_RATE,
            abi::HINT_LOGARITHMIC,
            abi::HINT_INTEGER
        ],
        [0x1, 0x2, 0x4, 0x8, 0x10, 0x20]
    );
    assert_eq!(abi::HINT_DEFAULT_MASK, 0x3C0);
    assert_eq!(
        [
            abi::HINT_DEFAULT_NONE,
            abi::HINT_DEFAULT_MINIMUM,
            abi::HINT_DEFAULT_LOW,
            abi::HINT_DEFAULT_MIDDLE,
            abi::HINT_DEFAULT_HIGH,
            abi::HINT_DEFAULT_MAXIMUM,
            abi::HINT_DEFAULT_ZERO,
            abi::HINT_DEFAULT_ONE,
            abi::HINT_DEFAULT_ONE_HUNDRED,
            abi::HINT_DEFAULT_FOUR_HUNDRED_FORTY
        ],
        [0x0, 0x40, 0x80, 0xC0, 0x100, 0x140, 0x200, 0x240, 0x280, 0x2C0]
    );
    assert_eq!(
        [
            abi::PROPERTY_REALTIME,
            abi::PROPERTY_INPLACE_BROKEN,
            abi::PROPERTY_HARD_REALTIME_CAPABLE
        ],
        [1, 2, 4]
    );
}

// --- the hints ---

fn hint(bits: i32, lower: f32, upper: f32) -> PortHint {
    PortHint::from_bits(bits, lower, upper)
}

const BOTH_BOUNDS: i32 = abi::HINT_BOUNDED_BELOW | abi::HINT_BOUNDED_ABOVE;

#[test]
fn each_default_hint_is_decoded_to_the_default_it_names() {
    let decoded = |default: i32| hint(default, 0.0, 0.0).default;
    assert_eq!(decoded(abi::HINT_DEFAULT_NONE), DefaultHint::None);
    assert_eq!(decoded(abi::HINT_DEFAULT_MINIMUM), DefaultHint::Minimum);
    assert_eq!(decoded(abi::HINT_DEFAULT_LOW), DefaultHint::Low);
    assert_eq!(decoded(abi::HINT_DEFAULT_MIDDLE), DefaultHint::Middle);
    assert_eq!(decoded(abi::HINT_DEFAULT_HIGH), DefaultHint::High);
    assert_eq!(decoded(abi::HINT_DEFAULT_MAXIMUM), DefaultHint::Maximum);
    assert_eq!(decoded(abi::HINT_DEFAULT_ZERO), DefaultHint::Zero);
    assert_eq!(decoded(abi::HINT_DEFAULT_ONE), DefaultHint::One);
    assert_eq!(decoded(abi::HINT_DEFAULT_ONE_HUNDRED), DefaultHint::OneHundred);
    assert_eq!(
        decoded(abi::HINT_DEFAULT_FOUR_HUNDRED_FORTY),
        DefaultHint::FourHundredFortyHertz
    );
    // (The numbers that `ladspa.h` leaves unused say nothing.)
    assert_eq!(decoded(0x180), DefaultHint::None);
    assert_eq!(decoded(0x1C0), DefaultHint::None);
    assert_eq!(decoded(0x300), DefaultHint::None);
    assert_eq!(decoded(0x3C0), DefaultHint::None);
}

#[test]
fn the_flags_of_a_hint_are_decoded_one_by_one_and_a_bound_is_there_only_if_the_flag_says_so() {
    let all = hint(
        abi::HINT_BOUNDED_BELOW
            | abi::HINT_BOUNDED_ABOVE
            | abi::HINT_TOGGLED
            | abi::HINT_SAMPLE_RATE
            | abi::HINT_LOGARITHMIC
            | abi::HINT_INTEGER,
        -2.0,
        3.0,
    );
    assert_eq!((all.lower, all.upper), (Some(-2.0), Some(3.0)));
    assert!(all.toggled && all.relative_to_sample_rate && all.logarithmic && all.integer);
    let none = hint(0, -2.0, 3.0);
    assert_eq!((none.lower, none.upper), (None, None));
    assert!(!none.toggled && !none.relative_to_sample_rate && !none.logarithmic && !none.integer);
    let only_below = hint(abi::HINT_BOUNDED_BELOW, -2.0, 3.0);
    assert_eq!((only_below.lower, only_below.upper), (Some(-2.0), None));
    let only_above = hint(abi::HINT_BOUNDED_ABOVE, -2.0, 3.0);
    assert_eq!((only_above.lower, only_above.upper), (None, Some(3.0)));
    for flag in [
        abi::HINT_TOGGLED,
        abi::HINT_SAMPLE_RATE,
        abi::HINT_LOGARITHMIC,
        abi::HINT_INTEGER,
    ] {
        let one = hint(flag, 0.0, 0.0);
        let set = [one.toggled, one.relative_to_sample_rate, one.logarithmic, one.integer];
        assert_eq!(set.iter().filter(|&&on| on).count(), 1, "flag {flag}");
    }
}

#[test]
fn a_bound_that_is_not_a_number_is_not_there() {
    let hint = hint(BOTH_BOUNDS, f32::NAN, f32::INFINITY);
    assert_eq!((hint.lower, hint.upper), (None, None));
}

#[test]
fn the_defaults_in_a_straight_range_are_the_quarters_of_it() {
    let default = |default: i32| hint(BOTH_BOUNDS | default, 0.0, 4.0).default_value(48_000);
    assert_eq!(default(abi::HINT_DEFAULT_MINIMUM), 0.0);
    assert_eq!(default(abi::HINT_DEFAULT_LOW), 1.0);
    assert_eq!(default(abi::HINT_DEFAULT_MIDDLE), 2.0);
    assert_eq!(default(abi::HINT_DEFAULT_HIGH), 3.0);
    assert_eq!(default(abi::HINT_DEFAULT_MAXIMUM), 4.0);
    assert_eq!(default(abi::HINT_DEFAULT_NONE), 0.0);
    // (A range that does not hold zero: nothing asked for is the nearest value that is in it.)
    assert_eq!(hint(BOTH_BOUNDS, 5.0, 9.0).default_value(48_000), 5.0);
    assert_eq!(hint(BOTH_BOUNDS, -9.0, -5.0).default_value(48_000), -5.0);
}

#[test]
fn the_defaults_in_a_logarithmic_range_are_the_quarters_of_the_logarithm() {
    let (lower, upper) = (20.0_f64, 20_000.0_f64);
    let log = |default: i32| hint(BOTH_BOUNDS | abi::HINT_LOGARITHMIC | default, 20.0, 20_000.0).default_value(48_000);
    let expected = |weight: f64| (lower.powf(1.0 - weight) * upper.powf(weight)) as f32;
    assert!((log(abi::HINT_DEFAULT_LOW) - expected(0.25)).abs() < 1e-3);
    assert!((log(abi::HINT_DEFAULT_MIDDLE) - expected(0.5)).abs() < 1e-3);
    assert!((log(abi::HINT_DEFAULT_HIGH) - expected(0.75)).abs() < 1e-2);
    assert_eq!(log(abi::HINT_DEFAULT_MINIMUM), 20.0);
    assert_eq!(log(abi::HINT_DEFAULT_MAXIMUM), 20_000.0);
    // The middle of a logarithmic range is its geometric mean: 20 and 20000 have 632.45...
    assert!((log(abi::HINT_DEFAULT_MIDDLE) - 632.455_5).abs() < 1e-2);
    // A range that reaches zero has no logarithm, and is straight.
    let through_zero = hint(
        BOTH_BOUNDS | abi::HINT_LOGARITHMIC | abi::HINT_DEFAULT_MIDDLE,
        0.0,
        100.0,
    );
    assert_eq!(through_zero.default_value(48_000), 50.0);
}

#[test]
fn bounds_that_are_fractions_of_the_sample_rate_are_made_into_values_at_the_rate_asked_for() {
    let relative = hint(
        BOTH_BOUNDS | abi::HINT_SAMPLE_RATE | abi::HINT_LOGARITHMIC | abi::HINT_DEFAULT_LOW,
        0.001,
        0.45,
    );
    assert_eq!(
        relative.bounds(48_000),
        (
            Some(f64::from(0.001_f32) * 48_000.0),
            Some(f64::from(0.45_f32) * 48_000.0)
        )
    );
    assert_eq!(
        relative.bounds(44_100),
        (
            Some(f64::from(0.001_f32) * 44_100.0),
            Some(f64::from(0.45_f32) * 44_100.0)
        )
    );
    // A quarter of the way up the logarithm of 48 to 21600 (Hz).
    let (lower, upper) = (f64::from(0.001_f32) * 48_000.0, f64::from(0.45_f32) * 48_000.0);
    let expected = (lower.powf(0.75) * upper.powf(0.25)) as f32;
    assert!(
        (relative.default_value(48_000) - expected).abs() < 1e-3,
        "{} against {expected}",
        relative.default_value(48_000)
    );
    // (At another rate it is another value: that is the point of the flag.)
    assert!(relative.default_value(44_100) < relative.default_value(48_000));
    // A hint with no flag for it does not scale.
    let plain = hint(BOTH_BOUNDS, 0.0, 2.0);
    assert_eq!(plain.bounds(48_000), (Some(0.0), Some(2.0)));
}

#[test]
fn the_literal_defaults_are_the_numbers_they_name_and_are_kept_in_the_bounds() {
    let default = |bits: i32, lower: f32, upper: f32| hint(bits, lower, upper).default_value(48_000);
    assert_eq!(default(abi::HINT_DEFAULT_ZERO, 0.0, 0.0), 0.0);
    assert_eq!(default(abi::HINT_DEFAULT_ONE, 0.0, 0.0), 1.0);
    assert_eq!(default(abi::HINT_DEFAULT_ONE_HUNDRED, 0.0, 0.0), 100.0);
    assert_eq!(default(abi::HINT_DEFAULT_FOUR_HUNDRED_FORTY, 0.0, 0.0), 440.0);
    assert_eq!(default(BOTH_BOUNDS | abi::HINT_DEFAULT_ONE_HUNDRED, 0.0, 50.0), 50.0);
    assert_eq!(default(BOTH_BOUNDS | abi::HINT_DEFAULT_ZERO, 1.0, 50.0), 1.0);
    // (A literal is not made a fraction of the sample rate by the flag that does that to the bounds.)
    assert_eq!(default(abi::HINT_SAMPLE_RATE | abi::HINT_DEFAULT_ONE, 0.0, 0.0), 1.0);
}

#[test]
fn a_default_that_needs_a_bound_that_is_not_there_is_zero() {
    assert_eq!(hint(abi::HINT_DEFAULT_MINIMUM, 3.0, 9.0).default_value(48_000), 0.0);
    assert_eq!(hint(abi::HINT_DEFAULT_MAXIMUM, 3.0, 9.0).default_value(48_000), 0.0);
    assert_eq!(
        hint(abi::HINT_BOUNDED_BELOW | abi::HINT_DEFAULT_MIDDLE, 3.0, 9.0).default_value(48_000),
        3.0
    );
}

#[test]
fn a_whole_number_is_rounded_and_a_toggle_with_no_default_is_off() {
    let integer = hint(BOTH_BOUNDS | abi::HINT_INTEGER | abi::HINT_DEFAULT_MIDDLE, 1.0, 8.0);
    assert_eq!(integer.default_value(48_000), 5.0, "4.5 is rounded up");
    let low = hint(BOTH_BOUNDS | abi::HINT_INTEGER | abi::HINT_DEFAULT_LOW, 0.0, 10.0);
    assert_eq!(low.default_value(48_000), 3.0, "2.5 is rounded up");
    assert_eq!(hint(abi::HINT_TOGGLED, 0.0, 0.0).default_value(48_000), 0.0);
    assert_eq!(
        hint(abi::HINT_TOGGLED | abi::HINT_DEFAULT_ONE, 0.0, 0.0).default_value(48_000),
        1.0
    );
}

// --- the ports ---

fn port(bits: i32) -> Result<PortDescription, String> {
    PortDescription::from_bits("Port".to_owned(), bits, hint(0, 0.0, 0.0))
}

#[test]
fn a_port_is_an_input_or_an_output_and_a_control_or_audio_and_nothing_else_is_a_port() {
    let kinds = |bits: i32| port(bits).map(|port| (port.direction, port.kind));
    assert_eq!(
        kinds(abi::PORT_INPUT | abi::PORT_CONTROL),
        Ok((PortDirection::Input, PortKind::Control))
    );
    assert_eq!(
        kinds(abi::PORT_INPUT | abi::PORT_AUDIO),
        Ok((PortDirection::Input, PortKind::Audio))
    );
    assert_eq!(
        kinds(abi::PORT_OUTPUT | abi::PORT_CONTROL),
        Ok((PortDirection::Output, PortKind::Control))
    );
    assert_eq!(
        kinds(abi::PORT_OUTPUT | abi::PORT_AUDIO),
        Ok((PortDirection::Output, PortKind::Audio))
    );
    assert!(
        kinds(abi::PORT_AUDIO)
            .unwrap_err()
            .contains("neither an input nor an output")
    );
    assert!(
        kinds(abi::PORT_INPUT | abi::PORT_OUTPUT | abi::PORT_AUDIO)
            .unwrap_err()
            .contains("both an input and an output")
    );
    assert!(
        kinds(abi::PORT_INPUT)
            .unwrap_err()
            .contains("neither control nor audio")
    );
    assert!(
        kinds(abi::PORT_INPUT | abi::PORT_CONTROL | abi::PORT_AUDIO)
            .unwrap_err()
            .contains("both control and audio")
    );
    assert_eq!(kinds(0).unwrap_err(), "port \"Port\" is neither an input nor an output");
}

// --- the unsafe code ---

/// What is said of each `unsafe` in the source of this crate: where it is, and what it is. **Every block and every `impl` has a `SAFETY:` comment directly above it, and every
/// function a `# Safety` section in its documentation.** The list is printed (`cargo test -p ladspa-host audit -- --nocapture`) for the report of the piece.
#[test]
fn every_unsafe_block_says_why_it_is_sound_audit() {
    let source_folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut audited = Vec::new();
    let mut problems = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&source_folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    files.sort();
    for file in files {
        // (This file talks about `unsafe` in strings, to look for it.)
        if file.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
        }
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            let Some(at) = find_the_word_unsafe(code) else { continue };
            let after = code[at + "unsafe".len()..].trim_start();
            let kind = if after.starts_with('{') {
                "block"
            } else if after.starts_with("impl") {
                "impl"
            } else if after.starts_with("fn") {
                "fn"
            } else if after.starts_with("extern \"system\" {") || after.starts_with("extern \"C\" {") {
                "extern block"
            } else if after.starts_with("extern \"C\" fn(") {
                // (The type of a function that is called from the library, which has no name: not code.)
                continue;
            } else if after.starts_with("extern \"C\" fn") {
                "fn"
            } else {
                problems.push(format!(
                    "{name}:{}: an unsafe that the audit does not know: {code}",
                    number + 1
                ));
                continue;
            };
            // The comment directly above, through the attributes.
            let mut above = Vec::new();
            let mut up = number;
            while up > 0 {
                let previous = lines[up - 1].trim_start();
                if previous.starts_with("//") || previous.starts_with("#[") {
                    above.push(previous);
                    up -= 1;
                } else {
                    break;
                }
            }
            let says = if kind == "fn" { "# Safety" } else { "SAFETY" };
            if !above.iter().any(|comment| comment.contains(says)) {
                problems.push(format!(
                    "{name}:{}: an unsafe {kind} with no \"{says}\" above it",
                    number + 1
                ));
            }
            audited.push(format!("{name}:{}: unsafe {kind}", number + 1));
        }
    }
    println!(
        "{} unsafe places in ladspa-host:\n{}",
        audited.len(),
        audited.join("\n")
    );
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(
        audited.len() >= 15,
        "the audit found only {} places, which is too few to be true",
        audited.len()
    );
}

/// Where the word `unsafe` is in a line of code, if it is (not as a part of a longer word, not in a string).
fn find_the_word_unsafe(code: &str) -> Option<usize> {
    let code_only = code.split("//").next().unwrap_or(code);
    let mut from = 0;
    while let Some(found) = code_only[from..].find("unsafe") {
        let at = from + found;
        let before = code_only[..at].chars().next_back();
        let after = code_only[at + 6..].chars().next();
        let part_of_a_longer_word = |c: char| c.is_alphanumeric() || c == '_';
        let is_word = !before.is_some_and(part_of_a_longer_word) && !after.is_some_and(part_of_a_longer_word);
        let in_a_string = code_only[..at].matches('"').count() % 2 == 1;
        if is_word && !in_a_string {
            return Some(at);
        }
        from = at + 6;
    }
    None
}
