// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host against a real library: the test plugins (`crates/ladspa-test-plugins`, written without the host's types, so that the two agree only where `ladspa.h` makes them).
//! Everything here is in this process; what happens when a library crashes or never returns is tested where the scan is, in a child process.

use std::sync::Mutex;

use ladspa_host::{
    ControlError, DefaultHint, Instance, InstantiateError, Library, OpenError, PortDirection, PortKind, RunError,
    scan_library,
};
use ladspa_test_plugins::{bad_libraries, library_file_for_tests};

fn the_library() -> Library {
    Library::open(&library_file_for_tests()).expect("the test library loads")
}

fn plugin(label: &str) -> (Library, usize) {
    let library = the_library();
    let description = library.describe();
    let index = description
        .plugins
        .iter()
        .find(|plugin| plugin.label == label)
        .unwrap_or_else(|| panic!("no plugin called {label}"))
        .index;
    (library, index)
}

fn instance_of(label: &str, sample_rate: u32) -> Instance {
    let (library, index) = plugin(label);
    library.instantiate(index, sample_rate).expect("the plugin is made")
}

/// A ramp that is different at every moment, so that a plugin that puts a moment in the wrong place shows.
fn ramp(count: usize, offset: f32) -> Vec<f32> {
    (0..count).map(|moment| offset + moment as f32 * 0.001).collect()
}

// --- what a library says ---

#[test]
fn the_library_describes_its_nine_plugins_in_order() {
    let description = the_library().describe();
    assert_eq!(description.problems, Vec::<String>::new());
    let labels: Vec<&str> = description.plugins.iter().map(|plugin| plugin.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "ffrs_test_gain",
            "ffrs_test_one_pole",
            "ffrs_test_swap",
            "ffrs_test_switches",
            "ffrs_test_no_ports",
            "ffrs_test_sine",
            "ffrs_test_mono_to_both",
            "ffrs_test_mix_down",
            "ffrs_test_counted",
        ]
    );
    let ids: Vec<u64> = description.plugins.iter().map(|plugin| plugin.unique_id).collect();
    assert_eq!(ids, [901, 902, 903, 904, 905, 906, 907, 908, 909]);
    let indices: Vec<usize> = description.plugins.iter().map(|plugin| plugin.index).collect();
    assert_eq!(indices, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    let gain = &description.plugins[0];
    assert_eq!(gain.name, "Test plugin ffrs_test_gain");
    assert_eq!(gain.maker, "fastforwardrs tests");
    assert_eq!(gain.copyright, "CC0-1.0");
    assert!(gain.properties.hard_realtime_capable && !gain.properties.realtime && !gain.properties.inplace_broken);
}

#[test]
fn the_ports_of_the_gain_are_a_control_and_two_audio_ports_with_the_hint_of_the_control() {
    let description = the_library().describe();
    let gain = &description.plugins[0];
    let ports: Vec<(&str, PortDirection, PortKind)> = gain
        .ports
        .iter()
        .map(|port| (port.name.as_str(), port.direction, port.kind))
        .collect();
    assert_eq!(
        ports,
        [
            ("Gain", PortDirection::Input, PortKind::Control),
            ("Input", PortDirection::Input, PortKind::Audio),
            ("Output", PortDirection::Output, PortKind::Audio),
        ]
    );
    let hint = gain.ports[0].hint;
    assert_eq!((hint.lower, hint.upper), (Some(0.0), Some(4.0)));
    assert_eq!(hint.default, DefaultHint::One);
    assert!(!hint.toggled && !hint.integer && !hint.logarithmic && !hint.relative_to_sample_rate);
    assert_eq!(gain.audio_port_counts(), (1, 1));
}

#[test]
fn the_plugins_have_the_channel_layouts_they_were_written_with() {
    let description = the_library().describe();
    let counts: Vec<(&str, (usize, usize))> = description
        .plugins
        .iter()
        .map(|plugin| (plugin.label.as_str(), plugin.audio_port_counts()))
        .collect();
    assert_eq!(
        counts,
        [
            ("ffrs_test_gain", (1, 1)),
            ("ffrs_test_one_pole", (1, 1)),
            ("ffrs_test_swap", (2, 2)),
            ("ffrs_test_switches", (1, 1)),
            ("ffrs_test_no_ports", (0, 0)),
            ("ffrs_test_sine", (0, 1)),
            ("ffrs_test_mono_to_both", (1, 2)),
            ("ffrs_test_mix_down", (2, 1)),
            ("ffrs_test_counted", (1, 1)),
        ]
    );
    assert!(description.plugins[4].ports.is_empty());
}

#[test]
fn the_toggle_the_whole_number_and_the_control_output_are_what_their_hints_say() {
    let description = the_library().describe();
    let switches = &description.plugins[3];
    let invert = &switches.ports[0];
    assert!(invert.hint.toggled);
    assert_eq!(invert.hint.default, DefaultHint::Zero);
    let multiplier = &switches.ports[1];
    assert!(multiplier.hint.integer);
    assert_eq!((multiplier.hint.lower, multiplier.hint.upper), (Some(1.0), Some(8.0)));
    let peak = &switches.ports[2];
    assert_eq!((peak.direction, peak.kind), (PortDirection::Output, PortKind::Control));
    assert_eq!(
        switches
            .ports_of(PortDirection::Input, PortKind::Control)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        switches
            .ports_of(PortDirection::Output, PortKind::Control)
            .collect::<Vec<_>>(),
        [2]
    );
}

#[test]
fn the_frequency_of_the_filter_has_bounds_that_are_fractions_of_the_sample_rate() {
    let description = the_library().describe();
    let cutoff = &description.plugins[1].ports[0].hint;
    assert!(cutoff.relative_to_sample_rate && cutoff.logarithmic);
    assert_eq!(cutoff.default, DefaultHint::Low);
    let (lower, upper) = cutoff.bounds(48_000);
    assert!((lower.unwrap() - 48.0).abs() < 1e-3, "{lower:?}");
    assert!((upper.unwrap() - 21_600.0).abs() < 1e-2, "{upper:?}");
}

// --- making and running ---

#[test]
fn a_plugin_starts_with_the_defaults_of_the_hints_of_its_ports() {
    let gain = instance_of("ffrs_test_gain", 48_000);
    assert_eq!(gain.control(0), Some(1.0));
    assert_eq!(gain.control(1), None, "an audio port has no value");
    let switches = instance_of("ffrs_test_switches", 48_000);
    assert_eq!(switches.control(0), Some(0.0), "the toggle is off");
    assert_eq!(
        switches.control(1),
        Some(5.0),
        "the middle of 1 to 8, as a whole number"
    );
    let sine = instance_of("ffrs_test_sine", 48_000);
    assert_eq!(sine.control(0), Some(440.0));
    // The filter's cutoff is a fraction of the sample rate, so it is another number at another rate.
    let at_48000 = instance_of("ffrs_test_one_pole", 48_000).control(0).unwrap();
    let at_44100 = instance_of("ffrs_test_one_pole", 44_100).control(0).unwrap();
    let expected = |rate: f64| (f64::from(0.001_f32) * rate).powf(0.75) * (f64::from(0.45_f32) * rate).powf(0.25);
    assert!((f64::from(at_48000) - expected(48_000.0)).abs() < 1e-2, "{at_48000}");
    assert!((f64::from(at_44100) - expected(44_100.0)).abs() < 1e-2, "{at_44100}");
}

#[test]
fn the_gain_gives_the_input_times_the_value_of_its_control_exactly() {
    let mut gain = instance_of("ffrs_test_gain", 48_000);
    gain.set_control(0, 0.5).unwrap();
    let mut input = ramp(512, 1.0);
    let expected: Vec<f32> = input.iter().map(|value| value * 0.5).collect();
    let mut output = vec![9.0_f32; 512];
    gain.run(&mut [&mut input], &mut [&mut output], 512).unwrap();
    assert_eq!(output, expected);
    // The value can be changed between blocks.
    gain.set_control(0, 4.0).unwrap();
    let mut input = ramp(100, -1.0);
    let expected: Vec<f32> = input.iter().map(|value| value * 4.0).collect();
    let mut output = vec![9.0_f32; 100];
    gain.run(&mut [&mut input], &mut [&mut output], 100).unwrap();
    assert_eq!(output, expected);
    assert!(gain.is_active(), "the first block activates the plugin");
}

#[test]
fn only_as_many_moments_as_asked_for_are_written() {
    let mut gain = instance_of("ffrs_test_gain", 48_000);
    let mut input = ramp(64, 1.0);
    let mut output = vec![9.0_f32; 64];
    gain.run(&mut [&mut input], &mut [&mut output], 10).unwrap();
    assert!(output[..10].iter().all(|&value| value != 9.0));
    assert!(output[10..].iter().all(|&value| value == 9.0));
}

#[test]
fn the_filter_remembers_between_blocks_and_forgets_when_it_is_reset() {
    let rate = 48_000;
    let mut in_two_blocks = instance_of("ffrs_test_one_pole", rate);
    let mut in_one_block = instance_of("ffrs_test_one_pole", rate);
    let hertz = 1_000.0_f32;
    in_two_blocks.set_control(0, hertz).unwrap();
    in_one_block.set_control(0, hertz).unwrap();
    let mut steps = vec![1.0_f32; 512];
    let mut whole = vec![0.0_f32; 512];
    in_one_block
        .run(&mut [&mut steps.clone()], &mut [&mut whole], 512)
        .unwrap();
    let (mut first, mut second) = (vec![0.0_f32; 256], vec![0.0_f32; 256]);
    in_two_blocks
        .run(&mut [&mut steps[..256].to_vec()], &mut [&mut first], 256)
        .unwrap();
    in_two_blocks
        .run(&mut [&mut steps[256..].to_vec()], &mut [&mut second], 256)
        .unwrap();
    assert_eq!([first.clone(), second].concat(), whole, "two blocks are one");
    // The first moment of a step response is the coefficient, and it rises towards 1.
    let coefficient = 1.0 - (-std::f32::consts::TAU * hertz / rate as f32).exp();
    assert_eq!(whole[0], coefficient);
    assert!(whole[..20].windows(2).all(|pair| pair[1] > pair[0]), "it rises");
    assert!((whole[511] - 1.0).abs() < 1e-5, "and gets there: {}", whole[511]);
    // After a reset it begins again.
    in_two_blocks.reset();
    let mut again = vec![0.0_f32; 256];
    in_two_blocks
        .run(&mut [&mut steps[..256].to_vec()], &mut [&mut again], 256)
        .unwrap();
    assert_eq!(again, first);
    steps.clear();
}

#[test]
fn the_stereo_swap_swaps_and_the_layouts_of_one_and_two_are_connected_in_the_order_of_the_ports() {
    let mut swap = instance_of("ffrs_test_swap", 48_000);
    let (mut left, mut right) = (ramp(32, 1.0), ramp(32, -1.0));
    let (expected_left, expected_right) = (right.clone(), left.clone());
    let (mut out_left, mut out_right) = (vec![0.0_f32; 32], vec![0.0_f32; 32]);
    swap.run(&mut [&mut left, &mut right], &mut [&mut out_left, &mut out_right], 32)
        .unwrap();
    assert_eq!((out_left, out_right), (expected_left, expected_right));

    let mut both = instance_of("ffrs_test_mono_to_both", 48_000);
    let mut mono = ramp(32, 2.0);
    let expected = mono.clone();
    let (mut out_left, mut out_right) = (vec![0.0_f32; 32], vec![0.0_f32; 32]);
    both.run(&mut [&mut mono], &mut [&mut out_left, &mut out_right], 32)
        .unwrap();
    assert_eq!((out_left, out_right), (expected.clone(), expected));

    let mut mix = instance_of("ffrs_test_mix_down", 48_000);
    let (mut left, mut right) = (ramp(32, 1.0), ramp(32, 10.0));
    let expected: Vec<f32> = left.iter().zip(&right).map(|(left, right)| left + right).collect();
    let mut out = vec![0.0_f32; 32];
    mix.run(&mut [&mut left, &mut right], &mut [&mut out], 32).unwrap();
    assert_eq!(out, expected);
}

#[test]
fn the_toggle_the_whole_number_and_the_meter_work_through_their_ports() {
    let mut switches = instance_of("ffrs_test_switches", 48_000);
    let mut input = ramp(64, 0.5);
    let loudest = input.iter().copied().fold(0.0_f32, f32::max);
    let mut output = vec![0.0_f32; 64];
    switches.run(&mut [&mut input.clone()], &mut [&mut output], 64).unwrap();
    // The default: not inverted, times 5.
    let expected: Vec<f32> = input.iter().map(|value| value * 5.0).collect();
    assert_eq!(output, expected);
    assert_eq!(
        switches.control(2),
        Some(loudest * 5.0),
        "the meter has what the plugin wrote"
    );
    switches.set_control(0, 1.0).unwrap();
    switches.set_control(1, 2.0).unwrap();
    switches.run(&mut [&mut input], &mut [&mut output], 64).unwrap();
    assert!(output.iter().all(|&value| value < 0.0), "inverted");
    assert_eq!(switches.control(2), Some(loudest * 2.0));
}

#[test]
fn a_generator_makes_sound_from_nothing_and_a_plugin_with_no_ports_runs() {
    let mut sine = instance_of("ffrs_test_sine", 48_000);
    sine.set_control(0, 12_000.0).unwrap();
    let mut out = vec![0.0_f32; 8];
    sine.run(&mut [], &mut [&mut out], 8).unwrap();
    // At a quarter of the sample rate the sine goes 0, 1, 0, -1.
    let expected = [0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0];
    for (value, expected) in out.iter().zip(expected) {
        assert!((value - expected).abs() < 1e-4, "{out:?}");
    }
    let mut nothing = instance_of("ffrs_test_no_ports", 48_000);
    nothing.run(&mut [], &mut [], 512).unwrap();
}

#[test]
fn a_value_that_cannot_be_given_to_a_port_is_refused_and_says_why() {
    let mut gain = instance_of("ffrs_test_gain", 48_000);
    assert_eq!(gain.set_control(7, 1.0), Err(ControlError::NoSuchPort(7)));
    assert_eq!(
        gain.set_control(1, 1.0),
        Err(ControlError::NotAControlInput(1)),
        "an audio port"
    );
    assert_eq!(gain.set_control(0, f32::NAN), Err(ControlError::NotANumber));
    assert_eq!(gain.set_control(0, f32::INFINITY), Err(ControlError::NotANumber));
    assert_eq!(gain.control(0), Some(1.0), "a refused value changes nothing");
    let mut switches = instance_of("ffrs_test_switches", 48_000);
    assert_eq!(
        switches.set_control(2, 1.0),
        Err(ControlError::NotAControlInput(2)),
        "a control output"
    );
}

#[test]
fn a_block_that_the_plugin_cannot_be_run_on_is_refused_and_says_why() {
    let mut gain = instance_of("ffrs_test_gain", 48_000);
    let mut input = vec![0.0_f32; 64];
    let mut output = vec![0.0_f32; 64];
    assert_eq!(
        gain.run(&mut [], &mut [&mut output], 64),
        Err(RunError::WrongNumberOfInputs { expected: 1, given: 0 })
    );
    assert_eq!(
        gain.run(&mut [&mut input], &mut [], 64),
        Err(RunError::WrongNumberOfOutputs { expected: 1, given: 0 })
    );
    let mut short = vec![0.0_f32; 63];
    assert_eq!(
        gain.run(&mut [&mut input], &mut [&mut short], 64),
        Err(RunError::BufferTooShort { has: 63, block: 64 })
    );
    let mut short = vec![0.0_f32; 10];
    assert_eq!(
        gain.run(&mut [&mut short], &mut [&mut output], 64),
        Err(RunError::BufferTooShort { has: 10, block: 64 })
    );
    assert!(!gain.is_active(), "a refused block does not even activate the plugin");
}

#[test]
fn a_plugin_that_is_not_in_the_library_cannot_be_made() {
    let library = the_library();
    assert!(matches!(
        library.instantiate(9, 48_000),
        Err(InstantiateError::NoSuchPlugin(9))
    ));
    assert!(matches!(
        library.instantiate(10_000, 48_000),
        Err(InstantiateError::NoSuchPlugin(10_000))
    ));
}

#[test]
fn an_instance_keeps_the_library_loaded_after_the_library_value_is_dropped() {
    let mut gain = {
        let library = the_library();
        library.instantiate(0, 48_000).unwrap()
    };
    let mut input = ramp(16, 1.0);
    let mut output = vec![0.0_f32; 16];
    gain.run(&mut [&mut input], &mut [&mut output], 16).unwrap();
    assert_eq!(output[1], input[1]);
}

#[test]
fn an_instance_can_be_moved_to_another_thread() {
    let mut gain = instance_of("ffrs_test_gain", 48_000);
    let output = std::thread::spawn(move || {
        let mut input = ramp(16, 1.0);
        let mut output = vec![0.0_f32; 16];
        gain.run(&mut [&mut input], &mut [&mut output], 16).unwrap();
        output
    })
    .join()
    .unwrap();
    assert_eq!(output[2], 1.002);
}

// --- what the host does to a plugin ---

/// The tests that look at the counts of the plugin that counts (and the one that makes and runs every plugin, which makes that one too) are not run at the same time as each
/// other: the counts are of the whole process.
static COUNTING: Mutex<()> = Mutex::new(());

#[derive(Debug, PartialEq, Eq)]
struct Counts {
    live: usize,
    activations: usize,
    deactivations: usize,
    blocks_run: usize,
    ports_not_connected_when_activated: usize,
}

/// The counts that the test library keeps, read through a second handle on the library (the same one that the host has loaded: a library that is loaded twice is loaded once).
fn counts() -> Counts {
    // SAFETY: loading the library that the tests are about; it is the test plugin, which does nothing when it is loaded.
    let library = unsafe { libloading::Library::new(library_file_for_tests()) }.expect("the library loads");
    // SAFETY: `ffrs_test_counts` is exported by the test library with this signature.
    let function: libloading::Symbol<unsafe extern "C" fn(i32) -> usize> =
        unsafe { library.get(b"ffrs_test_counts\0") }.expect("the function is there");
    // SAFETY: the function reads counters and nothing else.
    let count = |which: i32| unsafe { function(which) };
    Counts {
        live: count(0),
        activations: count(1),
        deactivations: count(2),
        blocks_run: count(3),
        ports_not_connected_when_activated: count(4),
    }
}

#[test]
fn the_host_activates_at_the_first_block_deactivates_when_reset_or_dropped_and_cleans_up_every_instance() {
    let _alone = COUNTING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let library = the_library();
    let counted = library
        .describe()
        .plugins
        .iter()
        .position(|plugin| plugin.label == "ffrs_test_counted")
        .unwrap();
    let before = counts();
    let mut first = library.instantiate(counted, 48_000).unwrap();
    let second = library.instantiate(counted, 48_000).unwrap();
    let made = counts();
    assert_eq!(made.live, before.live + 2, "two were made");
    assert_eq!(
        made.activations, before.activations,
        "nothing is activated until a block is run"
    );

    let mut input = ramp(32, 1.0);
    let mut output = vec![0.0_f32; 32];
    first.run(&mut [&mut input], &mut [&mut output], 32).unwrap();
    let ran = counts();
    assert_eq!(ran.activations, before.activations + 1, "the first block activates");
    assert_eq!(ran.blocks_run, before.blocks_run + 1);
    assert_eq!(
        ran.ports_not_connected_when_activated, before.ports_not_connected_when_activated,
        "every port was connected before the plugin was activated"
    );
    first.run(&mut [&mut input], &mut [&mut output], 32).unwrap();
    let ran_again = counts();
    assert_eq!(
        ran_again.activations,
        before.activations + 1,
        "a second block does not activate it again"
    );
    assert_eq!(ran_again.blocks_run, before.blocks_run + 2);
    assert_eq!(ran_again.deactivations, before.deactivations);

    first.reset();
    let reset = counts();
    assert_eq!(reset.deactivations, before.deactivations + 1);
    assert_eq!(
        reset.activations,
        before.activations + 2,
        "a reset is a deactivation and an activation"
    );
    first.deactivate();
    first.deactivate();
    assert_eq!(
        counts().deactivations,
        before.deactivations + 2,
        "deactivating what is not active does nothing"
    );
    first.activate();
    first.activate();
    assert_eq!(
        counts().activations,
        before.activations + 3,
        "activating what is active does nothing"
    );

    drop(first);
    let dropped = counts();
    assert_eq!(
        dropped.deactivations,
        before.deactivations + 3,
        "an instance that is active is deactivated before it is cleaned up"
    );
    assert_eq!(dropped.live, before.live + 1);
    drop(second);
    let all_dropped = counts();
    assert_eq!(
        all_dropped.deactivations,
        before.deactivations + 3,
        "an instance that was never active is not deactivated"
    );
    assert_eq!(
        all_dropped.live, before.live,
        "every instance that was made has been cleaned up"
    );
}

// --- the scan ---

#[test]
fn the_scan_of_the_library_makes_and_runs_every_plugin_and_says_what_it_is_about_to_do() {
    let _alone = COUNTING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut steps = Vec::new();
    let scan =
        scan_library(&library_file_for_tests(), |step| steps.push(step.to_owned())).expect("the library is scanned");
    assert_eq!(scan.problems, Vec::<String>::new());
    assert_eq!(scan.plugins.len(), 9);
    assert!(
        scan.plugins.iter().all(|plugin| plugin.problem.is_none()),
        "{:?}",
        scan.plugins
    );
    assert_eq!(steps.first().map(String::as_str), Some("loading the library"));
    assert_eq!(steps[1], "reading what the plugins say of themselves");
    // Each plugin is made, run and stopped, in that order, and in the order of the library.
    let for_the_gain: Vec<&String> = steps.iter().filter(|step| step.contains("(ffrs_test_gain)")).collect();
    assert_eq!(for_the_gain.len(), 3, "{steps:?}");
    assert!(for_the_gain[0].starts_with("making plugin number 0"));
    assert!(for_the_gain[1].starts_with("running plugin number 0"));
    assert!(for_the_gain[2].starts_with("stopping plugin number 0"));
    assert_eq!(steps.len(), 2 + 9 * 3);
}

// --- libraries that cannot be loaded ---

#[test]
fn a_file_that_is_not_there_is_not_loaded() {
    let folder = tempfile::tempdir().unwrap();
    let error = Library::open(&folder.path().join("nothing_here.dll"))
        .err()
        .expect("it is refused");
    assert!(matches!(error, OpenError::CouldNotBeLoaded(_)), "{error}");
}

#[test]
fn a_file_with_nothing_in_it_and_a_file_of_noise_are_not_loaded() {
    let folder = tempfile::tempdir().unwrap();
    for file in [
        bad_libraries::with_nothing_in_it(folder.path()),
        bad_libraries::damaged(folder.path()),
    ] {
        let error = Library::open(&file).err().expect("it is refused");
        assert!(
            matches!(error, OpenError::CouldNotBeLoaded(_)),
            "{}: {error}",
            file.display()
        );
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn a_library_that_needs_a_library_that_is_not_there_is_not_loaded() {
    let folder = tempfile::tempdir().unwrap();
    let file = bad_libraries::needing_a_library_that_is_not_there(folder.path());
    let error = Library::open(&file).err().expect("it is refused");
    assert!(matches!(error, OpenError::CouldNotBeLoaded(_)), "{error}");
}

#[test]
fn a_library_that_is_not_a_ladspa_library_says_so() {
    let system_library = if cfg!(windows) {
        "kernel32.dll"
    } else if cfg!(target_os = "macos") {
        "/usr/lib/libSystem.B.dylib"
    } else {
        "libc.so.6"
    };
    let error = Library::open(std::path::Path::new(system_library))
        .err()
        .expect("it is refused");
    assert!(matches!(error, OpenError::NotALadspaLibrary(_)), "{error}");
}
