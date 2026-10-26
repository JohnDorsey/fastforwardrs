// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the host with the real test plugins (`crates/lv2-test-plugins`), loaded in this process: they do nothing but arithmetic.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lv2_test_plugins::{
    ATOMS, CONTROLS, FAILS, GAIN, LATENCY, LATENCY_FRAMES, NEEDS_LOG, OPTIONS, STATE, STATE_BOOST_KEY, STATE_NAME_KEY,
    WORKER,
};
use tempfile::TempDir;

use crate::description::{PluginDescription, PortKind, describe_bundle};
use crate::state::{Property, decode, encode};
use crate::{Instance, InstantiateError, Library, OpenError, RunError, StateError};

const RATE: f64 = 48_000.0;
const MOST: u32 = 512;

/// The bundle of the test plugins, in a folder of its own, with what it says; the folder is kept for as long as the first is.
struct Bundle {
    _folder: TempDir,
    folder: PathBuf,
    plugins: Vec<PluginDescription>,
    library: Library,
}

fn the_bundle() -> Bundle {
    let folder = tempfile::tempdir().unwrap();
    let bundle = lv2_test_plugins::write_bundle(folder.path());
    let described = describe_bundle(&bundle).expect("the bundle is described");
    assert_eq!(described.problems, Vec::<String>::new());
    let library = Library::open(&described.plugins[0].binary).expect("the library is opened");
    Bundle {
        folder: bundle,
        _folder: folder,
        plugins: described.plugins,
        library,
    }
}

impl Bundle {
    fn plugin(&self, uri: &str) -> &PluginDescription {
        self.plugins
            .iter()
            .find(|plugin| plugin.uri == uri)
            .unwrap_or_else(|| panic!("no plugin {uri}"))
    }

    fn make(&self, uri: &str) -> Instance {
        Instance::new(&self.library, self.plugin(uri), RATE, MOST)
            .unwrap_or_else(|error| panic!("{uri} was not made: {error}"))
    }

    fn port(&self, uri: &str, symbol: &str) -> u32 {
        self.plugin(uri)
            .ports
            .iter()
            .find(|port| port.symbol == symbol)
            .unwrap()
            .index
    }
}

fn ramp(count: usize) -> Vec<f32> {
    (0..count).map(|moment| 0.5 + moment as f32 * 0.01).collect()
}

/// Runs a plugin that has one audio input and one audio output on `input`, in one block, and gives what it made.
fn run_mono(instance: &mut Instance, input: &[f32]) -> Vec<f32> {
    let frames = input.len();
    let mut input = input.to_vec();
    let mut output = vec![0.0; frames];
    instance
        .run(&mut [&mut input], &mut [&mut output], frames)
        .expect("the block is run");
    output
}

// --- the bundle ---

#[test]
fn the_test_bundle_is_described_with_all_its_plugins_and_its_library_has_them() {
    let bundle = the_bundle();

    let mut uris: Vec<&str> = bundle.plugins.iter().map(|plugin| plugin.uri.as_str()).collect();
    uris.sort_unstable();
    let mut expected = vec![GAIN, CONTROLS, LATENCY, ATOMS, OPTIONS, WORKER, NEEDS_LOG, STATE, FAILS];
    expected.sort_unstable();
    assert_eq!(uris, expected);
    for plugin in &bundle.plugins {
        assert!(bundle.library.has_plugin(&plugin.uri), "{}", plugin.uri);
        assert_eq!(plugin.bundle, bundle.folder);
        assert!(plugin.binary.is_file());
    }
    assert!(!bundle.library.has_plugin("urn:fastforwardrs:test:nothing"));
    let controls = bundle.plugin(CONTROLS);
    assert_eq!(
        controls
            .ports
            .iter()
            .filter(|port| port.kind == PortKind::Control)
            .count(),
        5
    );
    assert!(
        bundle
            .plugin(NEEDS_LOG)
            .required_features
            .iter()
            .any(|feature| feature.ends_with("log#log"))
    );
    assert!(
        bundle
            .plugin(STATE)
            .extension_data
            .iter()
            .any(|extension| extension.ends_with("state#interface"))
    );
}

#[test]
fn a_file_that_is_not_an_lv2_library_is_refused_with_the_reason() {
    let folder = tempfile::tempdir().unwrap();
    let not_a_library = folder.path().join("text.dll");
    std::fs::write(&not_a_library, "this is text").unwrap();

    let error = Library::open(&not_a_library).map(|_| ()).unwrap_err();
    assert!(matches!(error, OpenError::CouldNotBeLoaded(_)), "{error}");

    // (A program that can be loaded and has no lv2_descriptor: this test's own.)
    let this_program = std::env::current_exe().unwrap();
    assert!(Library::open(&this_program).is_err());
}

// --- making a plugin ---

#[test]
fn a_gain_makes_the_sound_times_its_gain_and_the_gain_is_what_the_last_value_says() {
    let bundle = the_bundle();
    let mut gain = bundle.make(GAIN);
    let gain_port = bundle.port(GAIN, "gain");
    assert_eq!(gain.audio_port_counts(), (1, 1));
    assert_eq!(gain.control(gain_port), Some(1.0), "the default the plugin says");
    let input = ramp(64);

    assert_eq!(run_mono(&mut gain, &input), input, "a gain of one");
    assert!(gain.set_control(gain_port, 0.5));
    let halved = run_mono(&mut gain, &input);
    let expected: Vec<f32> = input.iter().map(|sample| sample * 0.5).collect();
    assert_eq!(halved, expected);
    assert!(!gain.set_control(99, 1.0), "no such port");
    assert!(!gain.set_control(0, 1.0), "an audio port takes no value");
}

#[test]
fn a_plugin_that_requires_something_the_host_does_not_give_is_not_made_and_the_reason_is_said() {
    let bundle = the_bundle();

    let error = Instance::new(&bundle.library, bundle.plugin(NEEDS_LOG), RATE, MOST).unwrap_err();

    assert!(matches!(error, InstantiateError::MissingFeatures(_)), "{error}");
    assert!(error.to_string().contains("log#log"), "{error}");
}

#[test]
fn a_plugin_whose_instantiate_gives_nothing_is_not_made_and_a_plugin_the_library_does_not_have_is_said() {
    let bundle = the_bundle();

    let error = Instance::new(&bundle.library, bundle.plugin(FAILS), RATE, MOST).unwrap_err();
    assert!(matches!(error, InstantiateError::InstantiateFailed(48_000)), "{error}");

    let mut not_there = bundle.plugin(GAIN).clone();
    not_there.uri = "urn:fastforwardrs:test:nothing".to_owned();
    let error = Instance::new(&bundle.library, &not_there, RATE, MOST).unwrap_err();
    assert!(matches!(error, InstantiateError::NoSuchPlugin(_)), "{error}");
}

#[test]
fn a_block_that_is_longer_than_the_plugin_was_made_for_or_a_buffer_that_is_shorter_than_the_block_is_refused() {
    let bundle = the_bundle();
    let mut gain = bundle.make(GAIN);

    let mut long = vec![0.0_f32; MOST as usize + 1];
    let mut out = vec![0.0_f32; MOST as usize + 1];
    let error = gain
        .run(&mut [&mut long], &mut [&mut out], MOST as usize + 1)
        .unwrap_err();
    assert_eq!(
        error,
        RunError::BlockTooBig {
            frames: MOST as usize + 1,
            most: MOST as usize
        }
    );

    let mut short = vec![0.0_f32; 8];
    let mut out = vec![0.0_f32; 16];
    let error = gain.run(&mut [&mut short], &mut [&mut out], 16).unwrap_err();
    assert_eq!(error, RunError::BufferTooShort { has: 8, block: 16 });
}

#[test]
fn an_audio_input_that_is_not_given_is_silence_and_an_output_that_is_not_given_is_thrown_away() {
    let bundle = the_bundle();
    let mut gain = bundle.make(GAIN);

    // No input: the plugin hears silence, and the output is made all the same.
    let mut out = vec![9.0_f32; 32];
    gain.run(&mut [], &mut [&mut out], 32).unwrap();
    assert_eq!(out, vec![0.0; 32]);

    // No output: what it makes is thrown away, and the next block is as it was.
    let input = ramp(32);
    let mut input_copy = input.clone();
    gain.run(&mut [&mut input_copy], &mut [], 32).unwrap();
    assert_eq!(run_mono(&mut gain, &input), input);

    // (What was thrown away is in the buffer that outputs that are not given go to: an input that is not given is not that buffer, but silence.)
    let mut again = input.clone();
    gain.run(&mut [&mut again], &mut [], 32).unwrap();
    let mut out = vec![9.0_f32; 32];
    gain.run(&mut [], &mut [&mut out], 32).unwrap();
    assert_eq!(
        out,
        vec![0.0; 32],
        "no input is silence, whatever was thrown away before"
    );
}

// --- latency, atoms, options ---

#[test]
fn a_plugin_that_delays_the_sound_says_how_much_on_its_latency_port_and_it_is_what_the_host_reads() {
    let bundle = the_bundle();
    let mut latency = bundle.make(LATENCY);
    assert_eq!(latency.latency(), 0, "nothing is known before a block");
    let input = ramp(200);

    let out = run_mono(&mut latency, &input);

    assert_eq!(latency.latency() as usize, LATENCY_FRAMES);
    // (The plugin forgets what it has heard when it is activated: blocks that follow one another are one stream, and it is not activated again at each of them.)
    let more = ramp(100);
    let next = run_mono(&mut latency, &more);
    assert_eq!(
        next[..LATENCY_FRAMES],
        input[200 - LATENCY_FRAMES..],
        "the second block goes on from the first"
    );
    assert!(
        !latency.set_control(bundle.port(LATENCY, "latency"), 5.0),
        "a control output takes no value"
    );
    assert_eq!(
        latency.control(bundle.port(LATENCY, "latency")),
        Some(LATENCY_FRAMES as f32),
        "and is what the plugin made it"
    );
    assert!(
        out[..LATENCY_FRAMES].iter().all(|sample| *sample == 0.0),
        "the first frames are the silence it begins with"
    );
    assert_eq!(
        out[LATENCY_FRAMES..],
        input[..200 - LATENCY_FRAMES],
        "then the sound, late"
    );
}

#[test]
fn an_atom_input_holds_an_empty_sequence_and_an_atom_output_has_room_at_every_block() {
    let bundle = the_bundle();
    let mut atoms = bundle.make(ATOMS);
    let (ok, room) = (bundle.port(ATOMS, "was_empty_sequence"), bundle.port(ATOMS, "room"));

    for block in 0..3 {
        run_mono(&mut atoms, &ramp(16));
        assert_eq!(
            atoms.control(ok),
            Some(1.0),
            "block {block}: the input is an empty sequence"
        );
        // (The room is the buffer less the 8 bytes of the atom's own header; the plugin writes to its output at every block, as a plugin that has made a sequence does, and the next block
        // has all its room again.)
        assert_eq!(
            atoms.control(room),
            Some((crate::instance::ATOM_BUFFER_BYTES - 8) as f32),
            "block {block}: room for the output"
        );
    }
}

#[test]
fn a_plugin_is_told_the_sample_rate_and_the_lengths_of_the_blocks_in_its_options() {
    let bundle = the_bundle();
    let mut options = bundle.make(OPTIONS);

    run_mono(&mut options, &ramp(16));

    let says = |symbol: &str| options.control(bundle.port(OPTIONS, symbol)).unwrap();
    assert_eq!(says("sample_rate"), RATE as f32);
    assert_eq!(says("min_block"), 1.0);
    assert_eq!(says("max_block"), MOST as f32);
    assert_eq!(says("nominal_block"), MOST as f32);
    assert_eq!(says("bounded"), 1.0, "the host says that the blocks are bounded");
    assert_eq!(
        says("bundle_path_ends_with_a_separator"),
        1.0,
        "the bundle's path ends with a separator, as a plugin that adds a file's name to it needs"
    );
}

// --- the worker ---

#[test]
fn work_that_a_plugin_schedules_is_done_on_another_thread_and_its_answer_reaches_the_plugin_at_a_later_block() {
    let bundle = the_bundle();
    let mut worker = bundle.make(WORKER);
    let input = ramp(32);

    // The first block schedules the work, and the answer has not come.
    assert_eq!(run_mono(&mut worker, &input), input, "a multiplier of one");

    // The answer (the work doubles a number) comes by a block that is run soon enough; the blocks are run until the output is doubled.
    let began = Instant::now();
    let mut seen = Vec::new();
    while began.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(5));
        seen = run_mono(&mut worker, &input);
        if seen[0] != input[0] {
            break;
        }
    }
    let doubled: Vec<f32> = input.iter().map(|sample| sample * 2.0).collect();
    assert_eq!(seen, doubled, "the plugin was given what the work answered");

    // The answer is given once, and not again at every block after it.
    for _ in 0..3 {
        run_mono(&mut worker, &input);
    }
    assert_eq!(worker.control(bundle.port(WORKER, "answers")), Some(1.0));
}

#[test]
fn a_plugin_with_a_worker_that_is_let_go_of_at_once_does_not_hang_or_crash() {
    let bundle = the_bundle();
    for _ in 0..20 {
        let mut worker = bundle.make(WORKER);
        run_mono(&mut worker, &ramp(8));
        drop(worker);
    }
}

// --- state ---

#[test]
fn the_state_of_a_plugin_is_saved_and_given_to_another_instance_that_then_sounds_as_the_first_did() {
    let bundle = the_bundle();
    let request = bundle.port(STATE, "boost_request");
    let input = ramp(32);

    let mut first = bundle.make(STATE);
    assert!(first.has_state());
    first.set_control(request, 3.0);
    let boosted = run_mono(&mut first, &input);
    let tripled: Vec<f32> = input.iter().map(|sample| sample * 3.0).collect();
    assert_eq!(boosted, tripled);
    let state = first.save_state().expect("the state is saved");

    let mut second = bundle.make(STATE);
    assert_eq!(
        run_mono(&mut second, &input),
        input,
        "before it has the state it is the default"
    );
    second.restore_state(&state).expect("the state is taken");
    assert_eq!(
        run_mono(&mut second, &input),
        tripled,
        "after, it sounds as the first did"
    );

    // The state says its keys and types as URIs, which mean something to any instance.
    let properties = decode(&state).unwrap();
    let keys: Vec<&str> = properties.iter().map(|property| property.key.as_str()).collect();
    assert!(
        keys.contains(&STATE_BOOST_KEY) && keys.contains(&STATE_NAME_KEY),
        "{keys:?}"
    );
    let name = properties
        .iter()
        .find(|property| property.key == STATE_NAME_KEY)
        .unwrap();
    assert!(name.type_.ends_with("atom#String"));
    assert!(String::from_utf8_lossy(&name.value).starts_with("boost 3"));
}

#[test]
fn a_plugin_that_has_no_state_or_a_state_that_is_not_one_is_said() {
    let bundle = the_bundle();
    let mut gain = bundle.make(GAIN);
    assert!(!gain.has_state());
    assert_eq!(gain.save_state(), Err(StateError::NoState));

    // A plugin that cannot save says so (this one cannot when its boost is below zero).
    let mut unable = bundle.make(STATE);
    unable.set_control(bundle.port(STATE, "boost_request"), -1.0);
    run_mono(&mut unable, &ramp(8));
    assert_eq!(unable.save_state(), Err(StateError::SaveFailed(1)));
    assert_eq!(gain.restore_state(b"anything"), Err(StateError::NoState));

    let mut state = bundle.make(STATE);
    assert!(matches!(
        state.restore_state(b"not a state at all"),
        Err(StateError::NotAState(_))
    ));
    assert!(matches!(state.restore_state(b""), Err(StateError::NotAState(_))));
}

#[test]
fn the_bytes_of_a_state_are_read_back_and_what_is_cut_short_or_has_more_is_refused() {
    let properties = vec![
        Property {
            key: "urn:key:a".to_owned(),
            type_: "urn:type:float".to_owned(),
            flags: 3,
            value: vec![1, 2, 3, 4],
        },
        Property {
            key: "urn:key:b".to_owned(),
            type_: "urn:type:string".to_owned(),
            flags: 0,
            value: b"some words\0".to_vec(),
        },
    ];

    let bytes = encode(&properties);

    assert_eq!(decode(&bytes).unwrap(), properties);
    assert_eq!(decode(&encode(&[])).unwrap(), Vec::new());
    for cut in [0, 3, 10, bytes.len() - 1] {
        assert!(decode(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    let mut more = bytes.clone();
    more.push(0);
    assert!(decode(&more).is_err(), "more after the end");
    let mut other_version = bytes.clone();
    other_version[4] = 9;
    assert!(decode(&other_version).is_err());
}

// --- the binary interface ---

#[cfg(target_pointer_width = "64")]
#[test]
fn the_structures_of_the_interface_are_as_wide_as_the_headers_say_on_a_64_bit_system() {
    use crate::abi::{
        RawAtomSequenceHeader, RawFeature, RawOption, RawStateInterface, RawUridMap, RawWorkerInterface,
        RawWorkerSchedule,
    };
    use std::mem::size_of;

    assert_eq!(size_of::<RawFeature>(), 16, "a pointer and a pointer");
    assert_eq!(size_of::<RawOption>(), 32, "five 32-bit fields, padding, and a pointer");
    assert_eq!(size_of::<RawAtomSequenceHeader>(), 16);
    assert_eq!(size_of::<RawUridMap>(), 16);
    assert_eq!(size_of::<RawWorkerSchedule>(), 16);
    assert_eq!(size_of::<RawWorkerInterface>(), 24);
    assert_eq!(size_of::<RawStateInterface>(), 16);
    assert_eq!(
        size_of::<Option<unsafe extern "C" fn()>>(),
        size_of::<usize>(),
        "a function pointer that may be null is a pointer"
    );
}

// --- the unsafe code ---

#[test]
fn every_unsafe_block_of_this_crate_says_why_it_is_fine() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut blocks = 0;
    for entry in std::fs::read_dir(source).unwrap().map(Result::unwrap) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with("tests.rs") || !name.ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (place, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("#![") {
                continue;
            }
            let is_a_block = code.contains("unsafe {") || code.contains("unsafe impl") || code.contains("unsafe { ");
            if is_a_block {
                blocks += 1;
                let said = (place.saturating_sub(12)..=place).any(|before| lines[before].contains("SAFETY"));
                assert!(
                    said,
                    "the unsafe code at {name}:{} does not say why it is sound",
                    place + 1
                );
            }
        }
    }
    assert!(
        blocks > 20,
        "the audit found only {blocks} blocks: it is looking in the wrong place"
    );
}

#[test]
fn a_control_that_is_a_fraction_of_the_sample_rate_begins_as_that_fraction_of_the_rate_and_the_others_as_they_say() {
    let bundle = the_bundle();
    let controls = bundle.make(CONTROLS);

    assert_eq!(
        controls.control(bundle.port(CONTROLS, "rate")),
        Some(12_000.0),
        "0.25 of 48000"
    );
    assert_eq!(controls.control(bundle.port(CONTROLS, "cutoff")), Some(1000.0));
    assert_eq!(controls.control(bundle.port(CONTROLS, "mode")), Some(1.0));
    assert_eq!(controls.control(bundle.port(CONTROLS, "bypass")), Some(0.0));
}

#[test]
fn a_plugin_with_a_worker_is_told_when_each_block_ends() {
    let bundle = the_bundle();
    let mut worker = bundle.make(WORKER);
    let end_runs = bundle.port(WORKER, "end_runs");

    for _ in 0..3 {
        run_mono(&mut worker, &ramp(8));
    }

    // (The plugin says on a block how many ends it had been told of before it: after three blocks, two.)
    assert_eq!(worker.control(end_runs), Some(2.0));
}

#[test]
fn a_state_that_has_nothing_the_plugin_asks_for_is_taken_and_the_plugin_goes_on_as_it_was() {
    let bundle = the_bundle();
    let mut state = bundle.make(STATE);
    let input = ramp(16);

    // (The plugin says that a property is missing, as plugins do, and the host does not take that for a failure.)
    assert_eq!(state.restore_state(&encode(&[])), Ok(()));

    assert_eq!(run_mono(&mut state, &input), input);
}
