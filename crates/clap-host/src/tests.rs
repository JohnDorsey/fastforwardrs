// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the host with the real test plugins (`crates/clap-test-plugins`), loaded in this process: they do nothing but arithmetic.

use std::sync::Arc;

use clap_test_plugins::{
    CALLBACKS, CHOICE, FAILS_TO_ACTIVATE, FAILS_TO_PROCESS, GAIN, INSTRUMENT, LATENCY, LATENCY_SAMPLES, MONO_HALF,
    PANICS, RATE, SIDE_CHAIN, SWAP, TAIL, TAIL_FOREVER, TAIL_SAMPLES, THREADS,
};

use crate::scan::{LibraryScan, scan_library};
use crate::{Instance, Library, MainThread, ParameterChange, ProcessOutcome, RunError};

fn the_library() -> Library {
    Library::open(&clap_test_plugins::library_file_for_tests()).expect("the test library is opened")
}

fn made(id: &str) -> Instance {
    Instance::new(&MainThread::start(), &the_library(), id, 48_000.0, 512).expect("the plugin is made")
}

fn ramp(count: usize, offset: f32) -> Vec<f32> {
    (0..count).map(|moment| offset + moment as f32 * 0.01).collect()
}

/// Runs a stereo plugin on `left` and `right`, in one block, and gives what it made.
fn run_stereo(
    instance: &mut Instance,
    left: &[f32],
    right: &[f32],
    changes: &[ParameterChange],
) -> (Vec<f32>, Vec<f32>) {
    let (mut in_left, mut in_right) = (left.to_vec(), right.to_vec());
    let (mut out_left, mut out_right) = (vec![0.0; left.len()], vec![0.0; right.len()]);
    instance
        .run(
            &mut [&mut in_left, &mut in_right],
            &mut [&mut out_left, &mut out_right],
            changes,
        )
        .expect("the block is run");
    (out_left, out_right)
}

fn scan_of_the_library() -> LibraryScan {
    scan_library(&clap_test_plugins::library_file_for_tests(), |_| {}).expect("the library is scanned")
}

// --- the library ---

#[test]
fn the_library_says_what_plugins_it_has_and_what_each_says_of_itself() {
    let described = the_library().describe();

    assert_eq!(described.problems, Vec::<String>::new());
    let ids: Vec<&str> = described.plugins.iter().map(|plugin| plugin.id.as_str()).collect();
    assert!(ids.contains(&THREADS));
    assert_eq!(ids.len(), 15, "{ids:?}");
    assert!(ids.contains(&GAIN) && ids.contains(&INSTRUMENT));
    let gain = described.plugins.iter().find(|plugin| plugin.id == GAIN).unwrap();
    assert_eq!((gain.name.as_str(), gain.vendor.as_str()), ("Gain", "fastforwardrs"));
    assert_eq!(gain.description, "A plugin written for the tests");
}

#[test]
fn a_file_that_is_not_a_library_or_is_not_a_clap_one_is_refused_with_a_reason() {
    let folder = tempfile::tempdir().unwrap();
    let nothing = folder.path().join("empty.clap");
    std::fs::write(&nothing, b"").unwrap();
    assert!(matches!(
        Library::open(&nothing),
        Err(crate::OpenError::CouldNotBeLoaded(_))
    ));
    assert!(matches!(
        Library::open(&folder.path().join("not_there.clap")),
        Err(crate::OpenError::CouldNotBeLoaded(_))
    ));
}

// --- the scan ---

#[test]
fn the_scan_says_the_ports_the_parameters_the_latency_and_the_tail_of_each_plugin() {
    let scan = scan_of_the_library();
    let plugin = |id: &str| scan.plugins.iter().find(|plugin| plugin.description.id == id).unwrap();

    let with_a_problem: Vec<&str> = scan
        .plugins
        .iter()
        .filter(|plugin| plugin.problem.is_some())
        .map(|plugin| plugin.description.id.as_str())
        .collect();
    assert_eq!(
        with_a_problem,
        [FAILS_TO_ACTIVATE, FAILS_TO_PROCESS, PANICS],
        "the scan finds the plugins that cannot be activated or cannot process, and only those: {scan:?}"
    );
    let gain = plugin(GAIN);
    assert_eq!((gain.audio_inputs.len(), gain.audio_outputs.len()), (1, 1));
    assert_eq!(gain.audio_inputs[0].channel_count, 2);
    assert_eq!(gain.audio_inputs[0].port_type.as_deref(), Some("stereo"));
    assert!(gain.audio_inputs[0].is_main);
    assert_eq!(gain.parameters.len(), 1);
    let parameter = &gain.parameters[0];
    assert_eq!((parameter.id, parameter.name.as_str()), (1, "Gain"));
    assert_eq!(
        (parameter.minimum, parameter.maximum, parameter.default),
        (0.0, 4.0, 1.0)
    );
    assert_eq!(plugin(MONO_HALF).audio_inputs[0].channel_count, 1);
    assert_eq!(plugin(MONO_HALF).audio_inputs[0].port_type.as_deref(), Some("mono"));
    assert!(
        plugin(INSTRUMENT).audio_inputs.is_empty(),
        "an instrument has no audio input"
    );
    assert_eq!(plugin(INSTRUMENT).audio_outputs.len(), 1);
    assert_eq!(plugin(LATENCY).latency, LATENCY_SAMPLES);
    assert_eq!(plugin(GAIN).latency, 0);
    assert_eq!(plugin(TAIL).tail, Some(TAIL_SAMPLES));
    assert_eq!(plugin(GAIN).tail, None);
    let choice = plugin(CHOICE);
    assert_eq!(choice.parameters.len(), 2);
    assert_eq!(
        choice.parameters[1].texts,
        ["off", "double", "triple"],
        "the texts of an enum"
    );
    assert!(
        choice.parameters[0].texts.is_empty(),
        "a stepped parameter that is not an enum has none"
    );
}

#[test]
fn a_plugin_that_is_not_in_the_library_cannot_be_made() {
    let result = Instance::new(
        &MainThread::start(),
        &the_library(),
        "org.example.nothing",
        48_000.0,
        512,
    );
    assert!(
        matches!(result, Err(crate::InstanceError::CouldNotBeMade(_))),
        "{:?}",
        result.err()
    );
}

// --- running ---

#[test]
fn the_gain_plugin_gives_the_arithmetic_and_a_parameter_changes_at_the_sample_the_event_says() {
    let mut instance = made(GAIN);
    let (left, right) = (ramp(300, 1.0), ramp(300, -1.0));

    let (out_left, out_right) = run_stereo(
        &mut instance,
        &left,
        &right,
        &[
            ParameterChange {
                time: 100,
                id: 1,
                value: 2.0,
            },
            ParameterChange {
                time: 250,
                id: 1,
                value: 0.5,
            },
        ],
    );

    for moment in 0..300 {
        let gain = match moment {
            0..=99 => 1.0,
            100..=249 => 2.0,
            _ => 0.5,
        };
        assert_eq!(out_left[moment], left[moment] * gain, "left, sample {moment}");
        assert_eq!(out_right[moment], right[moment] * gain, "right, sample {moment}");
    }
}

#[test]
fn a_parameter_that_was_set_stays_for_the_blocks_after_it() {
    let mut instance = made(GAIN);
    let ones = vec![1.0_f32; 64];
    run_stereo(
        &mut instance,
        &ones,
        &ones,
        &[ParameterChange {
            time: 10,
            id: 1,
            value: 3.0,
        }],
    );

    let (left, _) = run_stereo(&mut instance, &ones, &ones, &[]);

    assert_eq!(left, vec![3.0; 64], "no event, the value it had");
}

#[test]
fn a_swap_gives_the_channels_to_each_other_and_a_mono_plugin_is_given_one_channel() {
    let mut swap = made(SWAP);
    let (left, right) = (ramp(32, 1.0), ramp(32, -5.0));
    let (swapped_left, swapped_right) = run_stereo(&mut swap, &left, &right, &[]);
    assert_eq!((swapped_left, swapped_right), (right.clone(), left.clone()));

    let mut mono = made(MONO_HALF);
    assert_eq!(mono.audio_inputs()[0].channel_count, 1);
    let mut input = ramp(32, 2.0);
    let mut output = vec![0.0; 32];
    mono.run(&mut [&mut input], &mut [&mut output], &[]).unwrap();
    assert_eq!(
        output,
        ramp(32, 2.0).iter().map(|sample| sample * 0.5).collect::<Vec<_>>()
    );
}

#[test]
fn a_stepped_parameter_and_an_enum_make_the_sound_times_their_values() {
    let mut instance = made(CHOICE);
    let ones = vec![1.0_f32; 16];

    let (left, _) = run_stereo(
        &mut instance,
        &ones,
        &ones,
        &[
            ParameterChange {
                time: 0,
                id: 1,
                value: 2.0,
            },
            ParameterChange {
                time: 8,
                id: 2,
                value: 1.0,
            },
        ],
    );

    assert_eq!(&left[..8], &[3.0; 8], "steps 2: times 3, mode 0: times 1");
    assert_eq!(&left[8..], &[6.0; 8], "mode 1: times 2 as well");
}

#[test]
fn a_plugin_with_latency_gives_the_sound_that_many_samples_late_over_any_blocks() {
    let mut instance = made(LATENCY);
    assert_eq!(instance.latency(), LATENCY_SAMPLES);
    let whole: Vec<f32> = (1..=300).map(|number| number as f32).collect();
    let mut made_left = Vec::new();

    for block in whole.chunks(100) {
        let (left, _) = run_stereo(&mut instance, block, block, &[]);
        made_left.extend(left);
    }

    let delay = LATENCY_SAMPLES as usize;
    assert_eq!(
        &made_left[..delay],
        &vec![0.0; delay][..],
        "silence while the plugin fills up"
    );
    assert_eq!(&made_left[delay..], &whole[..300 - delay], "then the sound, late");
}

#[test]
fn a_reset_makes_a_plugin_forget_what_it_heard() {
    let mut instance = made(LATENCY);
    let loud = vec![5.0_f32; 128];
    run_stereo(&mut instance, &loud, &loud, &[]);
    instance.reset();

    let (left, _) = run_stereo(&mut instance, &vec![0.0; 128], &vec![0.0; 128], &[]);

    assert_eq!(left, vec![0.0; 128], "what it heard before the reset does not come out");
}

#[test]
fn a_plugin_with_a_tail_rings_after_its_input_stops_and_says_how_long() {
    let mut instance = made(TAIL);
    assert_eq!(instance.tail(), Some(TAIL_SAMPLES));
    let mut impulse = vec![0.0_f32; 400];
    impulse[0] = 1.0;

    let (left, _) = run_stereo(&mut instance, &impulse, &impulse, &[]);

    assert_eq!((left[0], left[100], left[200], left[300]), (1.0, 0.5, 0.25, 0.125));
    assert_eq!(left[50], 0.0);
}

#[test]
fn channels_that_are_not_what_the_ports_are_are_refused_with_the_reason() {
    let mut instance = made(GAIN);
    let mut one = vec![0.0_f32; 16];
    let mut out_left = vec![0.0_f32; 16];
    let mut out_right = vec![0.0_f32; 16];

    let result = instance.run(&mut [&mut one], &mut [&mut out_left, &mut out_right], &[]);

    assert!(
        matches!(&result, Err(RunError::WrongBuffers(reason)) if reason.contains("main input")),
        "{result:?}"
    );
}

#[test]
fn many_instances_of_one_plugin_and_of_others_run_on_one_main_thread_and_are_let_go_of_cleanly() {
    let worker = MainThread::start();
    let library = the_library();
    let ones = vec![1.0_f32; 32];
    let mut instances: Vec<Instance> = (0..6)
        .map(|number| {
            let id = if number % 2 == 0 { GAIN } else { SWAP };
            Instance::new(&worker, &library, id, 48_000.0, 512).unwrap()
        })
        .collect();
    for instance in &mut instances {
        run_stereo(instance, &ones, &ones, &[]);
    }
    drop(instances);

    let mut again = Instance::new(&worker, &library, GAIN, 48_000.0, 512).unwrap();
    assert_eq!(run_stereo(&mut again, &ones, &ones, &[]).0, ones);
    let _ = Arc::strong_count(&worker);
}

#[test]
fn the_host_tells_a_plugin_truthfully_which_thread_is_the_main_one_and_which_is_the_audio_one() {
    let mut instance = made(THREADS);
    let ones = vec![1.0_f32; 32];

    // (It gives the sound as it is only if `activate` was on the main thread and `process` on the audio thread, by the host's own `thread-check`.)
    let (left, _) = run_stereo(&mut instance, &ones, &ones, &[]);
    assert_eq!(left, ones);

    // The audio thread is whichever thread runs: another thread doing the same is as right.
    let from_another_thread = std::thread::spawn(move || run_stereo(&mut instance, &[1.0; 32], &[1.0; 32], &[]).0)
        .join()
        .unwrap();
    assert_eq!(from_another_thread, ones);
}

// --- plugins that misbehave ---

#[test]
fn a_plugin_that_will_not_activate_is_refused_with_the_reason_and_the_host_goes_on() {
    let worker = MainThread::start();
    let result = Instance::new(&worker, &the_library(), FAILS_TO_ACTIVATE, 48_000.0, 512);
    assert!(
        matches!(result, Err(crate::InstanceError::CouldNotBeActivated(_))),
        "{:?}",
        result.err()
    );

    let mut gain = Instance::new(&worker, &the_library(), GAIN, 48_000.0, 512).unwrap();
    assert_eq!(
        run_stereo(&mut gain, &[1.0; 8], &[1.0; 8], &[]).0,
        vec![1.0; 8],
        "the host and its other plugins are fine"
    );
}

#[test]
fn a_plugin_whose_process_fails_or_panics_gives_an_error_and_not_a_crash() {
    for id in [FAILS_TO_PROCESS, PANICS] {
        let mut instance = made(id);
        let (mut input, mut output) = ([vec![1.0_f32; 16], vec![1.0; 16]], [vec![0.0_f32; 16], vec![0.0; 16]]);
        let [in_left, in_right] = &mut input;
        let [out_left, out_right] = &mut output;

        let result = instance.run(&mut [in_left, in_right], &mut [out_left, out_right], &[]);

        assert!(result.is_err(), "{id}: {result:?}");
    }
}

// --- what a plugin asks of the host ---

#[test]
fn a_plugin_that_asks_for_a_restart_and_a_call_on_the_main_thread_is_heard_and_the_call_is_made() {
    let mut instance = made(CALLBACKS);
    assert!(!instance.restart_was_requested() && !instance.parameters_changed());
    assert!(
        !instance.do_the_main_thread_callback_if_asked(),
        "nothing was asked for yet"
    );

    run_stereo(&mut instance, &[1.0; 8], &[1.0; 8], &[]);

    assert!(
        instance.restart_was_requested(),
        "the plugin asked for a restart while processing"
    );
    assert!(
        !instance.parameters_changed(),
        "the call on the main thread has not been made"
    );
    assert!(instance.do_the_main_thread_callback_if_asked(), "it asked for the call");
    assert!(
        instance.parameters_changed(),
        "in the call, the plugin said that its parameters changed"
    );
    assert!(
        !instance.do_the_main_thread_callback_if_asked(),
        "one call for one request"
    );
}

#[test]
fn a_plugin_with_a_side_chain_is_given_silence_for_it_and_the_main_ports_come_through() {
    let mut instance = made(SIDE_CHAIN);
    assert_eq!(instance.audio_inputs().len(), 2);
    assert!(instance.audio_inputs()[0].is_main && !instance.audio_inputs()[1].is_main);
    let (left, right) = (ramp(64, 1.0), ramp(64, -1.0));

    let (out_left, out_right) = run_stereo(&mut instance, &left, &right, &[]);
    assert_eq!(
        (out_left, out_right),
        (left.clone(), right.clone()),
        "main plus silence"
    );

    let (again_left, _) = run_stereo(&mut instance, &left, &right, &[]);
    assert_eq!(again_left, left, "the silence is silence in every block");
}

#[test]
fn a_plugin_that_says_its_latency_changed_makes_the_host_ask_for_a_restart() {
    let mut instance = made(LATENCY);
    run_stereo(&mut instance, &[1.0; 8], &[1.0; 8], &[]);
    assert!(
        !instance.restart_was_requested(),
        "it has only asked for a call on the main thread"
    );

    assert!(instance.do_the_main_thread_callback_if_asked());

    assert!(
        instance.restart_was_requested(),
        "in the call, it said that its latency changed"
    );
}

#[test]
fn a_tail_that_is_for_ever_is_u32_max_and_a_plugin_without_a_tail_says_none() {
    assert_eq!(made(TAIL_FOREVER).tail(), Some(u32::MAX));
    assert_eq!(made(GAIN).tail(), None);
}

#[test]
fn what_a_plugin_says_of_its_state_after_a_block_is_given_as_the_outcome() {
    let (left, right) = (vec![0.0_f32; 8], vec![0.0_f32; 8]);
    let outcome = |id: &str| {
        let mut instance = made(id);
        let (mut in_left, mut in_right) = (left.clone(), right.clone());
        let (mut out_left, mut out_right) = (left.clone(), right.clone());
        let channels = instance.audio_inputs()[0].channel_count as usize;
        if channels == 1 {
            instance.run(&mut [&mut in_left], &mut [&mut out_left], &[]).unwrap()
        } else {
            instance
                .run(
                    &mut [&mut in_left, &mut in_right],
                    &mut [&mut out_left, &mut out_right],
                    &[],
                )
                .unwrap()
        }
    };

    assert_eq!(outcome(GAIN), ProcessOutcome::Continue);
    assert_eq!(outcome(TAIL), ProcessOutcome::Tail);
    assert_eq!(outcome(SWAP), ProcessOutcome::ContinueIfNotQuiet);
    assert_eq!(outcome(MONO_HALF), ProcessOutcome::Sleep);
}

#[test]
fn a_plugin_is_activated_at_the_sample_rate_it_is_given() {
    let ones = vec![1.0_f32; 4];
    for (rate, expected) in [(44_100.0, 44.1_f32), (96_000.0, 96.0)] {
        let mut instance = Instance::new(&MainThread::start(), &the_library(), RATE, rate, 512).unwrap();
        let (left, _) = run_stereo(&mut instance, &ones, &ones, &[]);
        assert_eq!(left, vec![expected; 4], "at {rate}");
    }
}

#[test]
fn buffers_that_are_the_wrong_length_or_too_few_in_the_output_are_refused() {
    let mut instance = made(GAIN);
    let (mut in_left, mut in_right) = (vec![0.0_f32; 16], vec![0.0_f32; 16]);
    let (mut out_left, mut out_right) = (vec![0.0_f32; 16], vec![0.0_f32; 16]);

    let one_output = instance.run(&mut [&mut in_left, &mut in_right], &mut [&mut out_left], &[]);
    assert!(
        matches!(&one_output, Err(RunError::WrongBuffers(reason)) if reason.contains("main output")),
        "{one_output:?}"
    );

    let mut short = vec![0.0_f32; 8];
    let short_input = instance.run(
        &mut [&mut in_left, &mut short],
        &mut [&mut out_left, &mut out_right],
        &[],
    );
    assert!(
        matches!(&short_input, Err(RunError::WrongBuffers(reason)) if reason.contains("as long")),
        "{short_input:?}"
    );
}

#[test]
fn a_descriptor_without_an_id_or_with_a_blank_id_or_name_is_a_problem_that_says_which_plugin() {
    use crate::library::problem_with_descriptor;
    assert_eq!(
        problem_with_descriptor(3, None, "Name"),
        Some("plugin number 3 has no id".to_owned())
    );
    assert_eq!(
        problem_with_descriptor(3, Some("  "), "Name"),
        Some("plugin number 3 has a blank id".to_owned())
    );
    assert_eq!(
        problem_with_descriptor(3, Some("a.b"), " "),
        Some("the plugin a.b has a blank name".to_owned())
    );
    assert_eq!(problem_with_descriptor(3, Some("a.b"), "Name"), None);
}

// --- the audit ---

#[test]
fn the_only_unsafe_code_of_this_crate_is_the_loading_of_the_library_and_it_says_why() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains("unsafe") && !code.trim_start().starts_with("#![") {
                found.push(format!(
                    "{}:{}",
                    path.file_name().unwrap().to_string_lossy(),
                    number + 1
                ));
            }
        }
    }
    // (The lint lines `#![...]` of `lib.rs` are not code. `#[allow(unsafe_code)]` and the `unsafe { ... }` block it allows: both in `library.rs`, and nowhere else.)
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|place| place.starts_with("library.rs:")), "{found:?}");
    let library = std::fs::read_to_string(source.join("library.rs")).unwrap();
    assert!(
        library.contains("// SAFETY:"),
        "the unsafe block has to say why it is fine"
    );
}
