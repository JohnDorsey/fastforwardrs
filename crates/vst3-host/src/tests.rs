// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tests of the host, against the test plugins (`vst3-test-plugins`), which are VST3 plugins in the library a real host would load.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use vst3_test_plugins as plugins;

use crate::{
    BetweenBlocks, ClassFacts, ClassId, Instance, InstantiateError, Module, NoteEvent, NoteEventKind, OpenError,
    RunError, StateError, decode_state, encode_state, library_file_of, scan_module,
};

// --- helpers ---

/// The folder with the module of the test plugins, made once.
fn the_module_folder() -> &'static PathBuf {
    static FOLDER: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &FOLDER
        .get_or_init(|| {
            let folder = tempfile::tempdir().expect("a folder is made");
            let module = plugins::write_module(folder.path());
            (folder, module)
        })
        .1
}

pub(crate) fn the_module() -> Module {
    Module::open(the_module_folder()).expect("the module of the test plugins opens")
}

pub(crate) fn id(numbers: [u32; 4]) -> ClassId {
    ClassId::from_numbers(numbers[0], numbers[1], numbers[2], numbers[3])
}

pub(crate) fn make(numbers: [u32; 4]) -> Instance {
    Instance::new(&the_module(), id(numbers), 48_000.0, 512, None)
        .expect("the plugin is made")
        .0
}

const A_BLOCK: usize = 128;

/// A sound that is never silent and never repeats within a block.
fn a_sound(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|place| ((place as f32) * 0.05 + 0.1).sin() * 0.5 + 0.01)
        .collect()
}

/// Runs a stereo effect on the same sound in both channels, and gives what came out of the left one.
fn run_effect(instance: &mut Instance, frames: usize) -> Vec<f32> {
    let (mut left, mut right) = (a_sound(frames), a_sound(frames));
    let (mut out_left, mut out_right) = (vec![0.0; frames], vec![0.0; frames]);
    instance
        .run(
            &mut [&mut left, &mut right],
            &mut [&mut out_left, &mut out_right],
            frames,
        )
        .expect("the block is run");
    out_left
}

/// Runs an instrument for `frames` with `notes`, and gives the left channel.
fn run_instrument(instance: &mut Instance, frames: usize, notes: &[NoteEvent]) -> Vec<f32> {
    let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
    instance
        .run_with_notes(&mut [], &mut [&mut left, &mut right], frames, notes)
        .expect("the block is run");
    left
}

fn note_on(time: u32, key: u8, id: u32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity: 0.8,
            id,
        },
    }
}

fn note_off(time: u32, key: u8, id: u32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel: 0,
            key,
            velocity: 0.0,
            id,
        },
    }
}

fn parameter(facts: &ClassFacts, id: u32) -> &crate::ParameterDescription {
    facts
        .parameters
        .iter()
        .find(|parameter| parameter.id == id)
        .expect("the parameter is there")
}

// --- the id of a class ---

#[test]
fn a_class_id_is_written_as_32_capital_hexadecimal_digits_and_read_back() {
    let class = ClassId::from_numbers(0xABCD_EF01, 0x9182_FAEB, 0x0000_00FF, 0x1234_5678);
    assert_eq!(class.text(), "ABCDEF019182FAEB000000FF12345678");
    assert_eq!(ClassId::from_text("ABCDEF019182FAEB000000FF12345678"), Some(class));
    assert_eq!(
        ClassId::from_text("abcdef019182faeb000000ff12345678"),
        Some(class),
        "lower case is read too"
    );
    assert_eq!(ClassId::from_text("ABCDEF019182FAEB000000FF1234567"), None, "31 digits");
    assert_eq!(
        ClassId::from_text("ABCDEF019182FAEB000000FF123456789"),
        None,
        "33 digits"
    );
    assert_eq!(
        ClassId::from_text("ABCDEF019182FAEB000000FF1234567G"),
        None,
        "not hexadecimal"
    );
    assert_eq!(class.numbers(), [0xABCD_EF01, 0x9182_FAEB, 0x0000_00FF, 0x1234_5678]);
}

#[test]
fn a_class_id_goes_through_the_bytes_of_this_system_and_back() {
    for numbers in [
        [0, 0, 0, 0],
        [u32::MAX, u32::MAX, u32::MAX, u32::MAX],
        [0x0102_0304, 0x0506_0708, 0x090A_0B0C, 0x0D0E_0F10],
        [0xABCD_EF01, 0x9182_FAEB, 0x566D_6241, 0x5367_5854],
    ] {
        let class = ClassId::from_numbers(numbers[0], numbers[1], numbers[2], numbers[3]);
        assert_eq!(ClassId::from_tuid(&class.to_tuid()), class, "{numbers:x?}");
    }
}

#[test]
fn the_bytes_a_factory_gives_mean_the_same_four_numbers_on_every_system() {
    // Seen from real plugins on Windows (the Surge XT synthesizer, and a class of Dragonfly Reverb): their bytes, and the numbers that Steinberg's INLINE_UID has for them.
    let as_bytes = |bytes: [u8; 16]| bytes.map(|byte| byte as std::ffi::c_char);
    if cfg!(target_os = "windows") {
        let surge = ClassId::from_tuid(&as_bytes([
            0x01, 0xef, 0xcd, 0xab, 0x82, 0x91, 0xeb, 0xfa, 0x56, 0x6d, 0x62, 0x41, 0x53, 0x67, 0x58, 0x54,
        ]));
        assert_eq!(surge.numbers(), [0xABCD_EF01, 0x9182_FAEB, 0x566D_6241, 0x5367_5854]);
    } else {
        // (The other systems write the numbers as they are, most significant byte first.)
        let same = ClassId::from_tuid(&as_bytes([
            0xab, 0xcd, 0xef, 0x01, 0x91, 0x82, 0xfa, 0xeb, 0x56, 0x6d, 0x62, 0x41, 0x53, 0x67, 0x58, 0x54,
        ]));
        assert_eq!(same.numbers(), [0xABCD_EF01, 0x9182_FAEB, 0x566D_6241, 0x5367_5854]);
    }
}

#[test]
fn a_class_id_is_saved_as_its_text() {
    let class = id(plugins::GAIN);
    let saved = serde_json::to_string(&class).expect("it is saved");
    assert_eq!(saved, format!("\"{}\"", class.text()));
    assert_eq!(serde_json::from_str::<ClassId>(&saved).expect("it is read"), class);
    assert!(serde_json::from_str::<ClassId>("\"nothing\"").is_err());
}

// --- finding and opening a module ---

#[test]
fn the_library_of_a_module_is_where_this_system_keeps_it() {
    let file = library_file_of(the_module_folder()).expect("the library is found");
    assert!(file.is_file());
    let relative = file.strip_prefix(the_module_folder()).expect("it is inside the module");
    let (architecture, extension) = if cfg!(target_os = "windows") {
        ("x86_64-win", ".vst3")
    } else if cfg!(target_os = "macos") {
        ("MacOS", "")
    } else {
        ("x86_64-linux", ".so")
    };
    if cfg!(target_arch = "x86_64") {
        assert_eq!(
            relative,
            Path::new("Contents")
                .join(architecture)
                .join(format!("test{extension}"))
        );
    }
}

#[test]
fn a_file_is_its_own_library_and_a_folder_without_one_is_refused() {
    let file = library_file_of(the_module_folder()).expect("the library is found");
    assert_eq!(library_file_of(&file).expect("a file is its own library"), file);

    let folder = tempfile::tempdir().expect("a folder is made");
    let empty = folder.path().join("empty.vst3");
    std::fs::create_dir_all(empty.join("Contents")).expect("a folder is made");
    assert!(matches!(library_file_of(&empty), Err(OpenError::NoLibrary(_))));
    assert!(matches!(Module::open(&empty), Err(OpenError::NoLibrary(_))));
}

#[test]
fn something_that_is_not_a_library_is_refused() {
    let folder = tempfile::tempdir().expect("a folder is made");
    let file = folder.path().join("text.vst3");
    std::fs::write(&file, "not a library").expect("a file is written");
    assert!(matches!(Module::open(&file), Err(OpenError::CouldNotBeLoaded(_))));
}

#[test]
fn a_library_that_is_not_a_vst3_module_is_refused_for_having_no_factory() {
    let system_library = if cfg!(target_os = "windows") {
        std::env::var_os("SystemRoot").map(|root| PathBuf::from(root).join("System32").join("kernel32.dll"))
    } else if cfg!(target_os = "linux") {
        [
            "/lib/x86_64-linux-gnu/libm.so.6",
            "/usr/lib/libm.so.6",
            "/lib64/libm.so.6",
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
    } else {
        None
    };
    let Some(library) = system_library.filter(|path| path.is_file()) else {
        return;
    };
    assert!(
        matches!(Module::open(&library), Err(OpenError::NotAModule)),
        "{library:?}"
    );
}

// --- what a module says ---

#[test]
fn the_factory_says_who_made_it() {
    let factory = the_module().factory_description();
    assert_eq!(factory.vendor, "fastforwardrs tests");
    assert_eq!(factory.url, "https://github.com/JohnDorsey/fastforwardrs");
}

#[test]
fn the_classes_of_a_module_are_listed_with_their_categories_names_and_ids() {
    let classes = the_module().classes();
    assert_eq!(classes.len(), 16);
    let gain = classes
        .iter()
        .find(|class| class.id == id(plugins::GAIN))
        .expect("the gain effect is there");
    assert!(gain.is_a_plugin());
    assert_eq!(gain.category, "Audio Module Class");
    // (The name is the Unicode one, which the factory gives with a letter that is not ASCII, so that a host that reads the plain one shows.)
    assert_eq!(gain.name, "Test Gain \u{e9}");
    assert_eq!(gain.sub_categories, vec!["Fx".to_owned()]);
    assert_eq!(gain.vendor, "fastforwardrs tests");
    assert_eq!(gain.version, "1.2.3");
    assert_eq!(gain.sdk_version, "VST 3.7.0");
    let latency = classes
        .iter()
        .find(|class| class.id == id(plugins::LATENCY))
        .expect("the latency effect is there");
    assert_eq!(latency.sub_categories, vec!["Fx".to_owned(), "Delay".to_owned()]);
    let controller = classes
        .iter()
        .find(|class| class.id == id(plugins::GAIN_CONTROLLER))
        .expect("the controller class is there");
    assert!(!controller.is_a_plugin());
    assert_eq!(controller.category, "Component Controller Class");
    let compatibility = classes
        .iter()
        .find(|class| class.id == id(plugins::COMPATIBILITY))
        .expect("it is there");
    assert!(!compatibility.is_a_plugin());
    assert!(
        compatibility.sub_categories.is_empty(),
        "an empty text is no sub-category"
    );
    assert_eq!(classes.iter().filter(|class| class.is_a_plugin()).count(), 11);
}

#[test]
fn the_scan_lists_the_plugins_and_what_each_is_and_not_the_other_classes() {
    let mut steps = Vec::new();
    let scan =
        scan_module(the_module_folder(), &mut |step| steps.push(step.to_owned())).expect("the module is scanned");
    assert_eq!(scan.factory.vendor, "fastforwardrs tests");
    let names: Vec<&str> = scan.classes.iter().map(|scanned| scanned.class.name.as_str()).collect();
    assert_eq!(names.len(), 11, "{names:?}");
    assert!(!names.iter().any(|name| name.contains("Compatibility")), "{names:?}");
    // The healthy ones have what they are, and no problem; the others say why not.
    for numbers in [
        plugins::GAIN,
        plugins::LATENCY,
        plugins::INSTRUMENT,
        plugins::SINGLE,
        plugins::SIDE_CHAIN_INSTRUMENT,
        plugins::SIDE_CHAIN_EFFECT,
        plugins::NO_DEFAULTS,
    ] {
        let scanned = scan
            .classes
            .iter()
            .find(|scanned| scanned.class.id == id(numbers))
            .expect("it is scanned");
        assert_eq!(scanned.problem, None, "{}", scanned.class.name);
        assert!(scanned.facts.is_some());
    }
    let problem_of = |numbers: [u32; 4]| {
        scan.classes
            .iter()
            .find(|scanned| scanned.class.id == id(numbers))
            .and_then(|scanned| scanned.problem.clone())
            .expect("it has a problem")
    };
    assert!(problem_of(plugins::FAILS_TO_INITIALIZE).contains("would not initialize"));
    assert!(problem_of(plugins::FAILS_TO_ACTIVATE).contains("would not become active"));
    assert!(problem_of(plugins::FAILS_TO_PROCESS).contains("process said it failed"));
    assert!(problem_of(plugins::NOT_NUMBERS).contains("not made of numbers"));
    // It said what it was doing as it went (the last step is what is known if the program dies).
    assert!(steps.first().expect("steps").starts_with("loading "));
    assert!(steps.iter().any(|step| step.starts_with("making Test Gain")));
    assert!(steps.iter().any(|step| step.starts_with("running Test Gain")));
    // Which have a state to keep.
    let has_state = |numbers: [u32; 4]| {
        scan.classes
            .iter()
            .find(|scanned| scanned.class.id == id(numbers))
            .expect("scanned")
            .has_state
    };
    assert!(has_state(plugins::GAIN));
    assert!(has_state(plugins::SIDE_CHAIN_INSTRUMENT));
    assert!(!has_state(plugins::LATENCY), "it says it has none");
}

// --- what a class is ---

#[test]
fn the_gain_effect_is_two_classes_with_main_stereo_buses_and_every_kind_of_parameter() {
    let instance = make(plugins::GAIN);
    let facts = instance.facts();
    assert!(facts.has_a_controller);
    assert!(facts.controller_is_separate, "the controller is a class of its own");
    assert!(!facts.has_midi_mapping);
    assert_eq!(facts.audio_inputs.len(), 1);
    assert_eq!(facts.audio_outputs.len(), 1);
    assert!(facts.audio_inputs[0].is_main && facts.audio_outputs[0].is_main);
    assert_eq!(
        (facts.audio_inputs[0].channels, facts.audio_outputs[0].channels),
        (2, 2)
    );
    assert_eq!(facts.audio_inputs[0].name, "Input");
    assert!(facts.event_inputs.is_empty());
    assert_eq!(instance.channels(), (2, 2));

    let ids: Vec<u32> = facts.parameters.iter().map(|parameter| parameter.id).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5], "in the controller's order");
    let gain = parameter(facts, plugins::PARAMETER_GAIN);
    assert_eq!(
        (gain.title.as_str(), gain.units.as_str(), gain.step_count),
        ("Gain", "x", 0)
    );
    assert_eq!((gain.plain_minimum, gain.plain_maximum), (0.0, 4.0));
    assert_eq!(gain.default_normalized, 0.25);
    assert_eq!(gain.default_plain, 1.0, "a quarter of 0 to 4, as the plugin says");
    assert!(gain.can_automate && !gain.is_read_only && !gain.is_hidden && !gain.is_the_bypass);
    assert!(gain.texts.is_empty(), "a continuous parameter has no step texts");
    let mode = parameter(facts, plugins::PARAMETER_MODE);
    assert_eq!(mode.step_count, 2);
    assert!(mode.is_a_list);
    assert_eq!(mode.texts, ["Plain", "Double", "Triple"]);
    assert_eq!((mode.plain_minimum, mode.plain_maximum), (0.0, 2.0));
    assert!(parameter(facts, plugins::PARAMETER_SECRET).is_hidden);
    assert!(parameter(facts, plugins::PARAMETER_LEVEL).is_read_only);
    let bypass = parameter(facts, plugins::PARAMETER_BYPASS);
    assert!(bypass.is_the_bypass);
    assert_eq!(bypass.step_count, 1);
    assert_eq!(bypass.texts, ["Off", "On"]);
}

#[test]
fn a_plugin_with_no_controller_has_no_parameters_and_says_its_latency_and_tail() {
    let instance = make(plugins::LATENCY);
    let facts = instance.facts();
    assert!(!facts.has_a_controller);
    assert!(facts.parameters.is_empty());
    assert_eq!(facts.latency, plugins::LATENCY_SAMPLES as u32);
    assert_eq!(instance.latency(), 64);
    assert_eq!(facts.tail, plugins::TAIL_SAMPLES);
}

#[test]
fn an_instrument_has_no_audio_input_one_event_input_and_a_midi_mapping() {
    let instance = make(plugins::INSTRUMENT);
    let facts = instance.facts();
    assert!(facts.audio_inputs.is_empty());
    assert_eq!(facts.main_audio_input(), None);
    assert_eq!(facts.audio_outputs.len(), 1);
    assert_eq!(facts.event_inputs.len(), 1);
    assert_eq!(facts.event_inputs[0].channels, 16);
    assert!(facts.takes_events());
    assert!(facts.has_midi_mapping);
    assert_eq!(instance.channels(), (0, 2));
    assert_eq!(facts.parameters.len(), 3);
}

#[test]
fn an_instrument_whose_only_input_is_a_side_chain_has_no_main_input_and_its_extra_outputs_are_off() {
    let instance = make(plugins::SIDE_CHAIN_INSTRUMENT);
    let facts = instance.facts();
    assert_eq!(facts.audio_inputs.len(), 1);
    assert!(!facts.audio_inputs[0].is_main, "a side chain is not a main bus");
    assert_eq!(facts.main_audio_input(), None);
    assert_eq!(facts.audio_outputs.len(), 3);
    assert_eq!(facts.main_audio_output().map(|(place, _)| place), Some(0));
    assert!(facts.audio_outputs[0].is_on_by_default);
    assert!(!facts.audio_outputs[1].is_on_by_default && !facts.audio_outputs[2].is_on_by_default);
    assert_eq!(instance.channels(), (0, 2), "the side chain is not the input");
}

#[test]
fn an_effect_whose_first_input_is_a_side_chain_has_its_main_input_second_and_hears_it() {
    let mut instance = make(plugins::SIDE_CHAIN_EFFECT);
    let facts = instance.facts();
    assert_eq!(facts.audio_inputs.len(), 2);
    assert!(!facts.audio_inputs[0].is_main, "the side chain is first");
    assert_eq!(
        facts.main_audio_input().map(|(place, _)| place),
        Some(1),
        "the main input is found by its type"
    );
    assert_eq!(instance.channels(), (2, 2));
    let heard = run_effect(&mut instance, A_BLOCK);
    let expected: Vec<f32> = a_sound(A_BLOCK).iter().map(|sample| sample * 0.5).collect();
    assert_eq!(
        heard, expected,
        "the main input was given the sound, and the side chain was left off"
    );
}

#[test]
fn a_plugin_that_is_its_own_controller_is_found_without_a_second_class() {
    let instance = make(plugins::SINGLE);
    let facts = instance.facts();
    assert!(facts.has_a_controller);
    assert!(!facts.controller_is_separate);
    assert_eq!(facts.parameters.len(), 1);
    assert_eq!(facts.parameters[0].title, "Gain");
}

#[test]
fn a_class_that_is_not_a_plugin_cannot_be_made_and_the_compatibility_class_says_so() {
    let module = the_module();
    let refused = Instance::new(&module, id(plugins::COMPATIBILITY), 48_000.0, 512, None);
    assert!(matches!(refused, Err(InstantiateError::CouldNotBeMade(_))));
    let unknown = Instance::new(&module, ClassId::from_numbers(1, 2, 3, 4), 48_000.0, 512, None);
    assert!(matches!(unknown, Err(InstantiateError::CouldNotBeMade(_))));
    // (A controller is not a component: it has no IComponent.)
    let controller = Instance::new(&module, id(plugins::GAIN_CONTROLLER), 48_000.0, 512, None);
    assert!(matches!(controller, Err(InstantiateError::CouldNotBeMade(_))));
}

// --- sound ---

#[test]
fn the_gain_effect_with_its_own_values_multiplies_the_sound() {
    let mut instance = make(plugins::GAIN);
    // (The default gain is 1: normalised 0.25 of 0 to 4.)
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard, a_sound(A_BLOCK));

    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);
    let heard = run_effect(&mut instance, A_BLOCK);
    let expected: Vec<f32> = a_sound(A_BLOCK).iter().map(|sample| sample * 2.0).collect();
    assert_eq!(heard, expected, "twice as loud");
}

#[test]
fn a_value_is_applied_at_the_sample_it_is_given_for() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_GAIN, 3.0, 40);
    let heard = run_effect(&mut instance, A_BLOCK);
    let input = a_sound(A_BLOCK);
    assert_eq!(&heard[..40], &input[..40], "before the sample, the default");
    for place in 40..A_BLOCK {
        assert_eq!(heard[place], input[place] * 3.0, "sample {place}");
    }
}

#[test]
fn several_values_in_one_block_are_applied_each_at_its_sample_whatever_order_they_were_given_in() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_GAIN, 4.0, 90);
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 30);
    instance.set_value(plugins::PARAMETER_MODE, 1.0, 60);
    let heard = run_effect(&mut instance, A_BLOCK);
    let input = a_sound(A_BLOCK);
    for place in 0..A_BLOCK {
        let factor = match place {
            0..=29 => 1.0,
            30..=59 => 2.0,
            60..=89 => 4.0,
            _ => 8.0,
        };
        // (From sample 60 the mode is "double", which makes it twice as loud; from 90 the gain is 4.)
        let factor = if (60..90).contains(&place) { 2.0 * 2.0 } else { factor };
        assert_eq!(heard[place], input[place] * factor, "sample {place}");
    }
}

#[test]
fn a_value_stays_for_the_blocks_that_follow() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);
    run_effect(&mut instance, A_BLOCK);
    let heard = run_effect(&mut instance, A_BLOCK);
    let expected: Vec<f32> = a_sound(A_BLOCK).iter().map(|sample| sample * 2.0).collect();
    assert_eq!(heard, expected);
}

#[test]
fn a_list_parameter_chooses_between_its_steps_and_the_bypass_passes_the_sound_as_it_is() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_MODE, 2.0, 0);
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard[7], a_sound(A_BLOCK)[7] * 3.0, "triple");
    instance.set_value(plugins::PARAMETER_GAIN, 4.0, 0);
    instance.set_value(plugins::PARAMETER_BYPASS, 1.0, 0);
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard, a_sound(A_BLOCK), "bypassed");
}

#[test]
fn a_value_for_a_parameter_the_plugin_does_not_have_changes_nothing() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(99, 1.0, 0);
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard, a_sound(A_BLOCK));
    // A plugin with no controller takes no values at all.
    let mut latency = make(plugins::LATENCY);
    latency.set_value(1, 1.0, 0);
    run_effect(&mut latency, A_BLOCK);
}

#[test]
fn a_meter_of_the_plugin_is_given_back_as_a_control_output_in_plain_values() {
    let mut instance = make(plugins::GAIN);
    assert_eq!(instance.control_outputs(), vec![], "nothing before the first block");
    let heard = run_effect(&mut instance, A_BLOCK);
    let peak = heard.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let outputs = instance.control_outputs();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].0, plugins::PARAMETER_LEVEL);
    assert!(
        (outputs[0].1 - f64::from(peak)).abs() < 1e-7,
        "{outputs:?} against {peak}"
    );
}

#[test]
fn a_plugin_whose_controller_says_every_value_is_nothing_until_it_is_given_the_state_of_its_component_is_given_it_when_it_is_made()
 {
    // (Surge XT's controller does: 2,855 values of 0, and defaults of 0 beside, until the host gives it the component's state, as VST3 hosts do when they make a plugin. The test
    // plugin's controller is made like that, and the gain that a host sees is the gain the component has: 1, which is a quarter of the way up the range 0 to 4.)
    let instance = make(plugins::GAIN);

    assert_eq!(instance.plain_value(plugins::PARAMETER_GAIN), Some(1.0));
    assert_eq!(instance.plain_value(plugins::PARAMETER_MODE), Some(0.0));
}

#[test]
fn a_component_that_has_no_state_to_give_is_not_asked_to_give_one_to_its_controller() {
    // (The instrument's component says "not implemented" when it is asked for its state, and its controller forgets its values when it is handed a state it cannot read, as some real
    // ones do: the pitch wheel is in the middle, and stays there.)
    let instance = make(plugins::INSTRUMENT);

    assert_eq!(instance.plain_value(plugins::PARAMETER_BEND), Some(0.5));
    assert_eq!(
        parameter(instance.facts(), plugins::PARAMETER_BEND).default_plain,
        0.5,
        "and the default is as the controller declares it"
    );
}

#[test]
fn a_plugin_that_declares_no_defaults_has_for_its_defaults_the_values_its_controller_has_when_it_is_made() {
    // (The controller of this class says 0 for the default of each parameter, as Surge XT's does for its 2,855; the gain the plugin has is 1, a quarter of the way up the range 0 to 4.)
    let instance = make(plugins::NO_DEFAULTS);
    let gain = parameter(instance.facts(), plugins::PARAMETER_GAIN);

    assert_eq!(gain.default_normalized, 0.25);
    assert_eq!(gain.default_plain, 1.0);
    assert_eq!(parameter(instance.facts(), plugins::PARAMETER_MODE).default_plain, 0.0);
    // (One that declares them is as it was: see the test of the gain's facts.)
    assert_eq!(
        parameter(make(plugins::GAIN).facts(), plugins::PARAMETER_GAIN).default_plain,
        1.0
    );
}

#[test]
fn the_default_of_a_parameter_is_what_is_declared_unless_that_is_nothing_and_the_controller_has_a_value() {
    use crate::instance::default_of_a_parameter;
    // What is declared, when it is something.
    assert_eq!(default_of_a_parameter(0.25, 0.5), 0.25);
    assert_eq!(default_of_a_parameter(0.3, 0.0), 0.3);
    // Nothing declared, and a value in the controller.
    assert_eq!(default_of_a_parameter(0.0, 0.5), 0.5);
    assert_eq!(default_of_a_parameter(0.0, 1.0), 1.0);
    // Nothing declared, and nothing in the controller, or a value that is not a number or not a position in the range.
    assert_eq!(default_of_a_parameter(0.0, 0.0), 0.0);
    assert_eq!(default_of_a_parameter(0.0, f64::NAN), 0.0);
    assert_eq!(default_of_a_parameter(0.0, f64::INFINITY), 0.0);
    assert_eq!(default_of_a_parameter(0.0, 1.5), 1.0);
    assert_eq!(default_of_a_parameter(0.0, -0.5), 0.0);
}

#[test]
fn a_value_that_was_given_before_a_reset_and_not_yet_to_a_block_is_still_given_to_the_next_block() {
    // (A reset makes the plugin forget what it heard and keeps the values it was given, as the other formats' copies do. A program that is started again is given its values, and then the
    // reset of the run it ended in.)
    let mut instance = make(plugins::GAIN);
    let silent_gain = run_effect(&mut instance, A_BLOCK);
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);

    instance.reset();
    let after_the_reset = run_effect(&mut instance, A_BLOCK);

    // (The gain of 1 at the start is the sound itself; the gain of 2 that was given before the reset is twice that.)
    assert_eq!(after_the_reset[40], a_sound(A_BLOCK)[40] * 2.0);
    assert_eq!(silent_gain[40], a_sound(A_BLOCK)[40]);
}

#[test]
fn a_delay_comes_out_late_by_its_latency_across_blocks_and_a_reset_forgets_what_was_in_it() {
    let mut instance = make(plugins::LATENCY);
    let frames = 50;
    // An impulse in the first sample of the first block.
    let mut impulse = vec![0.0_f32; frames];
    impulse[0] = 1.0;
    let mut heard = Vec::new();
    for block in 0..4 {
        let mut input = if block == 0 { impulse.clone() } else { vec![0.0; frames] };
        let mut other = input.clone();
        let (mut out_left, mut out_right) = (vec![0.0; frames], vec![0.0; frames]);
        instance
            .run(
                &mut [&mut input, &mut other],
                &mut [&mut out_left, &mut out_right],
                frames,
            )
            .expect("the block is run");
        heard.extend(out_left);
    }
    let late = heard.iter().position(|sample| *sample != 0.0).expect("it comes out");
    assert_eq!(late, 64, "as late as the latency it says");
    assert_eq!(heard.iter().filter(|sample| **sample != 0.0).count(), 1);

    // Reset after something was put in: the delay is empty again.
    let mut instance = make(plugins::LATENCY);
    let mut input = vec![1.0_f32; frames];
    let mut other = input.clone();
    let (mut out_left, mut out_right) = (vec![0.0; frames], vec![0.0; frames]);
    instance
        .run(
            &mut [&mut input, &mut other],
            &mut [&mut out_left, &mut out_right],
            frames,
        )
        .expect("the block is run");
    instance.reset();
    let heard = run_effect_of_silence(&mut instance, 100);
    assert!(
        heard.iter().all(|sample| *sample == 0.0),
        "{:?}",
        heard.iter().position(|sample| *sample != 0.0)
    );
}

fn run_effect_of_silence(instance: &mut Instance, frames: usize) -> Vec<f32> {
    let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
    let (mut out_left, mut out_right) = (vec![1.0; frames], vec![1.0; frames]);
    instance
        .run(
            &mut [&mut left, &mut right],
            &mut [&mut out_left, &mut out_right],
            frames,
        )
        .expect("the block is run");
    out_left
}

#[test]
fn a_block_without_its_input_is_run_on_silence_and_one_with_no_room_for_the_output_is_thrown_away() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);
    let (mut out_left, mut out_right) = (vec![1.0_f32; 64], vec![1.0_f32; 64]);
    instance
        .run(&mut [], &mut [&mut out_left, &mut out_right], 64)
        .expect("run on silence");
    assert!(
        out_left.iter().all(|sample| *sample == 0.0),
        "gain of silence is silence"
    );
    instance
        .run(&mut [], &mut [], 64)
        .expect("run with nowhere to put the sound");
}

#[test]
fn a_block_longer_than_the_copy_was_made_for_is_refused() {
    let mut instance = make(plugins::GAIN);
    let mut long = vec![0.0_f32; 600];
    let mut other = long.clone();
    let result = instance.run(&mut [&mut long], &mut [&mut other], 513);
    assert_eq!(result, Err(RunError::TooLong { asked: 513, most: 512 }));
    assert!(instance.run(&mut [], &mut [], 512).is_ok());
}

#[test]
fn a_plugin_that_is_its_own_controller_takes_values_at_samples_too() {
    let mut instance = make(plugins::SINGLE);
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 100);
    let heard = run_effect(&mut instance, A_BLOCK);
    let input = a_sound(A_BLOCK);
    assert_eq!(&heard[..100], &input[..100]);
    assert_eq!(heard[100], input[100] * 2.0);
}

// --- notes ---

#[test]
fn a_note_sounds_from_its_sample_to_the_sample_of_its_off() {
    let mut instance = make(plugins::INSTRUMENT);
    let heard = run_instrument(&mut instance, A_BLOCK, &[note_on(10, 60, 1), note_off(100, 60, 1)]);
    let held = 0.8_f32 * 60.0 / 100.0;
    assert!(heard[..10].iter().all(|sample| *sample == 0.0));
    assert!(
        heard[10..100].iter().all(|sample| *sample == held),
        "{:?}",
        &heard[8..14]
    );
    assert!(heard[100..].iter().all(|sample| *sample == 0.0));
}

#[test]
fn two_notes_add_up_and_an_off_ends_the_note_of_its_id_whatever_the_order() {
    let mut instance = make(plugins::INSTRUMENT);
    let notes = [note_on(0, 60, 1), note_on(20, 72, 2), note_off(40, 60, 1)];
    let heard = run_instrument(&mut instance, 60, &notes);
    let first = 0.8_f32 * 60.0 / 100.0;
    let second = 0.8_f32 * 72.0 / 100.0;
    assert_eq!(heard[10], first);
    assert_eq!(heard[30], first + second);
    assert_eq!(heard[50], second, "the first note ended");
    // The note that is held goes on into the next block.
    let next = run_instrument(&mut instance, 10, &[]);
    assert!(next.iter().all(|sample| *sample == second));
}

#[test]
fn the_pitch_wheel_and_a_controller_reach_the_parameters_the_plugin_maps_them_to() {
    let mut instance = make(plugins::INSTRUMENT);
    // The wheel all the way up is the parameter Bend at 1.0, which makes (1 - 0.5) * 0.5 = 0.25 with no note held.
    let wheel = NoteEvent {
        time: 5,
        what: NoteEventKind::PitchBend {
            channel: 0,
            value: 16383,
        },
    };
    let heard = run_instrument(&mut instance, 20, &[wheel]);
    assert_eq!(heard[4], 0.0);
    assert_eq!(heard[5], 0.25);
    // The modulation wheel (controller 1) adds a quarter of its value, but only while a note is held.
    let modulation = NoteEvent {
        time: 0,
        what: NoteEventKind::Controller {
            channel: 0,
            number: 1,
            value: 127,
        },
    };
    let mut instance = make(plugins::INSTRUMENT);
    let heard = run_instrument(&mut instance, 20, &[modulation, note_on(10, 60, 1)]);
    assert_eq!(heard[5], 0.0, "no note is held");
    assert_eq!(heard[15], 0.8_f32 * 60.0 / 100.0 + 0.25);
}

#[test]
fn all_notes_off_goes_through_the_mapping_when_there_is_one() {
    let mut instance = make(plugins::INSTRUMENT);
    let all_off = NoteEvent {
        time: 50,
        what: NoteEventKind::AllOff,
    };
    let heard = run_instrument(&mut instance, 80, &[note_on(0, 60, 1), all_off]);
    assert_eq!(heard[49], 0.8_f32 * 60.0 / 100.0);
    assert_eq!(heard[50], 0.0);
    assert_eq!(instance.take_the_count_of_events_left_out(), 0);
}

#[test]
fn all_notes_off_makes_a_plugin_without_a_mapping_forget_its_voices_by_being_stopped_and_started() {
    // (The instrument with a side chain has a controller without a mapping.)
    let mut instance = make(plugins::SIDE_CHAIN_INSTRUMENT);
    let held = run_instrument(&mut instance, 20, &[note_on(0, 60, 1)]);
    assert_eq!(held[10], 0.8_f32 * 60.0 / 100.0);
    let all_off = NoteEvent {
        time: 0,
        what: NoteEventKind::AllOff,
    };
    let after = run_instrument(&mut instance, 20, &[all_off]);
    assert!(after.iter().all(|sample| *sample == 0.0), "{:?}", &after[..4]);
}

#[test]
fn a_controller_that_nothing_maps_is_left_out_and_counted_once() {
    let mut instance = make(plugins::INSTRUMENT);
    let pedal = NoteEvent {
        time: 0,
        what: NoteEventKind::Controller {
            channel: 0,
            number: 64,
            value: 127,
        },
    };
    run_instrument(&mut instance, 20, &[pedal, pedal]);
    assert_eq!(instance.take_the_count_of_events_left_out(), 2);
    assert_eq!(
        instance.take_the_count_of_events_left_out(),
        0,
        "counted since it was asked"
    );
}

#[test]
fn notes_given_to_an_effect_are_ignored() {
    let mut instance = make(plugins::GAIN);
    let (mut out_left, mut out_right) = (vec![0.0_f32; 64], vec![0.0_f32; 64]);
    instance
        .run_with_notes(&mut [], &mut [&mut out_left, &mut out_right], 64, &[note_on(0, 60, 1)])
        .expect("the block is run");
    assert!(out_left.iter().all(|sample| *sample == 0.0));
}

// --- state ---

#[test]
fn a_state_made_of_two_parts_is_taken_apart_and_nothing_else_is_taken() {
    let bytes = encode_state(b"component", b"controller");
    assert_eq!(
        decode_state(&bytes),
        Ok((b"component".as_slice(), b"controller".as_slice()))
    );
    assert_eq!(
        decode_state(&encode_state(b"", b"")),
        Ok((b"".as_slice(), b"".as_slice()))
    );
    for cut in 0..bytes.len() {
        assert_eq!(decode_state(&bytes[..cut]), Err(StateError::NotOurs), "cut at {cut}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(decode_state(&longer), Err(StateError::NotOurs));
    let mut other_version = bytes.clone();
    other_version[4] = 2;
    assert_eq!(decode_state(&other_version), Err(StateError::NotOurs));
    let mut other_start = bytes;
    other_start[0] = b'X';
    assert_eq!(decode_state(&other_start), Err(StateError::NotOurs));
    assert_eq!(decode_state(b""), Err(StateError::NotOurs));
}

#[test]
fn the_state_of_a_plugin_brings_a_new_copy_to_where_the_old_one_was() {
    let mut old = make(plugins::GAIN);
    old.set_value(plugins::PARAMETER_GAIN, 3.0, 0);
    old.set_value(plugins::PARAMETER_MODE, 1.0, 0);
    // (The values reach the plugin's own state when the plugin has processed them.)
    run_effect(&mut old, A_BLOCK);
    let state = old.save_state().expect("the state is saved");
    let (component, controller) = decode_state(&state).expect("it is ours");
    assert_eq!(component.len(), 4 + 4 * 8);
    assert_eq!(controller, b"", "the controller says it has no state of its own");

    let (mut new, note) =
        Instance::new(&the_module(), id(plugins::GAIN), 48_000.0, 512, Some(&state)).expect("the copy is made");
    assert_eq!(note, None);
    let heard = run_effect(&mut new, A_BLOCK);
    let expected: Vec<f32> = a_sound(A_BLOCK).iter().map(|sample| sample * 6.0).collect();
    assert_eq!(heard, expected, "gain 3, double");
}

#[test]
fn a_state_can_be_taken_by_a_copy_that_is_running() {
    let mut source = make(plugins::GAIN);
    source.set_value(plugins::PARAMETER_GAIN, 2.0, 0);
    run_effect(&mut source, A_BLOCK);
    let state = source.save_state().expect("the state is saved");
    let mut target = make(plugins::GAIN);
    target.restore_state(&state).expect("the state is taken");
    let heard = run_effect(&mut target, A_BLOCK);
    assert_eq!(heard[9], a_sound(A_BLOCK)[9] * 2.0);
}

#[test]
fn a_state_that_is_not_ours_or_that_the_plugin_refuses_is_said_so_and_the_copy_is_made_all_the_same() {
    let (mut copy, note) =
        Instance::new(&the_module(), id(plugins::GAIN), 48_000.0, 512, Some(b"not a state")).expect("it is made");
    assert!(
        note.expect("it says why").contains("not a state of a VST3 plugin"),
        "bytes that are not ours"
    );
    assert_eq!(run_effect(&mut copy, A_BLOCK), a_sound(A_BLOCK), "its own state");

    let refused = encode_state(b"nonsense", b"");
    let (_, note) = Instance::new(&the_module(), id(plugins::GAIN), 48_000.0, 512, Some(&refused)).expect("it is made");
    assert!(note.expect("it says why").contains("did not take the state"));
}

#[test]
fn a_plugin_with_no_state_says_so_when_it_is_asked_for_one() {
    let instance = make(plugins::LATENCY);
    assert!(matches!(instance.save_state(), Err(StateError::CouldNotBeSaved(_))));
}

#[test]
fn an_instrument_that_applies_its_state_in_its_first_block_is_given_a_silent_block_before_any_note() {
    // The state is applied in the first block that the plugin processes, and a note in that block makes it fail (as Surge XT's VST3 hangs for some patches).
    let state = encode_state(b"edits=2", b"");
    let (mut instance, note) = Instance::new(
        &the_module(),
        id(plugins::SIDE_CHAIN_INSTRUMENT),
        48_000.0,
        512,
        Some(&state),
    )
    .expect("it is made");
    assert_eq!(note, None);
    let heard = run_instrument(&mut instance, 20, &[note_on(0, 60, 1)]);
    assert_eq!(
        heard[5],
        0.8_f32 * 60.0 / 100.0 * 0.25,
        "the state was applied (two edits make it a quarter as loud) before the note"
    );
    // The same note with no state needs no silent block.
    let mut plain = make(plugins::SIDE_CHAIN_INSTRUMENT);
    assert_eq!(
        run_instrument(&mut plain, 20, &[note_on(0, 60, 1)])[5],
        0.8_f32 * 60.0 / 100.0
    );
}

#[test]
fn an_effect_that_is_given_a_state_is_not_run_before_the_first_block_it_is_given() {
    // (Only a plugin that takes notes has the silent block: an effect has no note to be hurt by.)
    let state = old_gain_state(2.0);
    let (mut instance, _) =
        Instance::new(&the_module(), id(plugins::GAIN), 48_000.0, 512, Some(&state)).expect("it is made");
    assert_eq!(
        instance.control_outputs(),
        vec![],
        "no block was run: a meter would have a value"
    );
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard[3], a_sound(A_BLOCK)[3] * 2.0);
}

fn old_gain_state(gain: f64) -> Vec<u8> {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_GAIN, gain, 0);
    run_effect(&mut instance, 8);
    instance.save_state().expect("the state is saved")
}

// --- what the plugin asks for ---

#[test]
fn nothing_is_asked_between_blocks_by_a_plugin_that_asks_for_nothing() {
    let mut instance = make(plugins::GAIN);
    run_effect(&mut instance, A_BLOCK);
    assert_eq!(instance.between_blocks(), BetweenBlocks::default());
}

#[test]
fn an_edit_and_a_change_of_latency_that_the_controller_tells_the_host_are_given_between_blocks_and_the_edit_reaches_the_next_block()
 {
    let mut instance = make(plugins::GAIN);
    let state = old_gain_state(1.0);
    let (component, _) = decode_state(&state).expect("it is ours");
    // (The controller of the test plugin, given the state `EDIT`, edits Gain to three quarters of the way up and says its latency changed.)
    instance
        .restore_state(&encode_state(component, b"EDIT"))
        .expect("the state is taken");
    let asked = instance.between_blocks();
    assert_eq!(
        asked.edits,
        vec![(plugins::PARAMETER_GAIN, 3.0)],
        "three quarters of 0 to 4"
    );
    assert!(asked.latency_changed);
    assert!(!asked.wants_to_be_made_again);
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard[5], a_sound(A_BLOCK)[5] * 3.0, "the edit was given to the sound");
    assert_eq!(instance.between_blocks(), BetweenBlocks::default(), "it was said once");
}

// --- plugins that fail ---

#[test]
fn a_plugin_that_would_not_initialize_or_become_active_is_refused_with_the_words() {
    let module = the_module();
    let initialize = Instance::new(&module, id(plugins::FAILS_TO_INITIALIZE), 48_000.0, 512, None);
    assert!(
        matches!(initialize, Err(InstantiateError::WouldNotInitialize(1))),
        "kResultFalse is 1"
    );
    let activate = Instance::new(&module, id(plugins::FAILS_TO_ACTIVATE), 48_000.0, 512, None);
    assert!(matches!(activate, Err(InstantiateError::WouldNotBecomeActive(1))));
}

#[test]
fn a_plugin_whose_process_fails_says_so_for_the_block() {
    let mut instance = make(plugins::FAILS_TO_PROCESS);
    let (mut left, mut right) = (vec![0.0_f32; 16], vec![0.0_f32; 16]);
    let result = instance.run(&mut [], &mut [&mut left, &mut right], 16);
    assert_eq!(result, Err(RunError::ProcessFailed(1)));
}

#[test]
fn a_plugin_that_writes_things_that_are_not_numbers_is_run_all_the_same() {
    // (Whether that is allowed is for the caller to say: the scan refuses such a plugin, and the effect puts silence in its place.)
    let mut instance = make(plugins::NOT_NUMBERS);
    let heard = run_effect(&mut instance, 16);
    assert!(heard.iter().all(|sample| sample.is_nan()));
}

// --- more of what a host has to get right ---

#[test]
fn a_value_is_held_to_the_range_of_its_parameter_and_the_controller_has_what_the_plugin_has() {
    let mut instance = make(plugins::GAIN);
    assert_eq!(instance.plain_value(plugins::PARAMETER_GAIN), Some(1.0), "the default");
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);
    assert_eq!(
        instance.plain_value(plugins::PARAMETER_GAIN),
        Some(2.0),
        "the controller was told"
    );
    instance.set_value(plugins::PARAMETER_GAIN, 100.0, 0);
    assert_eq!(
        instance.plain_value(plugins::PARAMETER_GAIN),
        Some(4.0),
        "held to the top of 0 to 4"
    );
    let heard = run_effect(&mut instance, A_BLOCK);
    assert_eq!(heard[9], a_sound(A_BLOCK)[9] * 4.0, "the plugin was given the top too");
    instance.set_value(plugins::PARAMETER_GAIN, -5.0, 0);
    assert_eq!(
        instance.plain_value(plugins::PARAMETER_GAIN),
        Some(0.0),
        "held to the bottom"
    );
    assert_eq!(make(plugins::LATENCY).plain_value(1), None, "no controller");
}

#[test]
fn a_value_is_given_to_one_block_only() {
    let mut instance = make(plugins::GAIN);
    instance.set_value(plugins::PARAMETER_GAIN, 3.0, 40);
    run_effect(&mut instance, A_BLOCK);
    instance.set_value(plugins::PARAMETER_GAIN, 2.0, 0);
    let heard = run_effect(&mut instance, A_BLOCK);
    let input = a_sound(A_BLOCK);
    for place in 0..A_BLOCK {
        assert_eq!(
            heard[place],
            input[place] * 2.0,
            "sample {place}: the three of the block before is not given again"
        );
    }
}

#[test]
fn a_note_past_the_end_of_the_block_is_put_at_its_last_sample() {
    let mut instance = make(plugins::INSTRUMENT);
    let heard = run_instrument(&mut instance, 100, &[note_on(500, 60, 1)]);
    assert_eq!(heard[98], 0.0);
    assert_eq!(heard[99], 0.8_f32 * 60.0 / 100.0);
}

#[test]
fn two_notes_of_one_key_are_told_apart_by_their_ids() {
    let mut instance = make(plugins::INSTRUMENT);
    let notes = [note_on(0, 60, 1), note_on(0, 60, 2), note_off(20, 60, 2)];
    let heard = run_instrument(&mut instance, 40, &notes);
    let one = 0.8_f32 * 60.0 / 100.0;
    assert_eq!(heard[10], one + one);
    assert_eq!(heard[30], one, "only the note of the id in the off ended");
    // The other one ends with its own id (and the plugin, which knows ids, would not end it by its key).
    let heard = run_instrument(&mut instance, 10, &[note_off(0, 60, 1)]);
    assert!(heard.iter().all(|sample| *sample == 0.0));
}

#[test]
fn what_the_plugin_has_after_a_state_is_the_state_in_its_controller_too() {
    let mut running = make(plugins::GAIN);
    let state = old_gain_state(3.0);
    running.restore_state(&state).expect("the state is taken");
    assert_eq!(
        running.plain_value(plugins::PARAMETER_GAIN),
        Some(3.0),
        "the controller was given the component's state"
    );
}

#[test]
fn each_restart_a_plugin_asks_for_is_told_for_what_it_is() {
    let state = old_gain_state(1.0);
    let (component, _) = decode_state(&state).expect("it is ours");
    for (words, reload_or_io, values) in [
        (&b"RELOAD"[..], true, false),
        (&b"IO"[..], true, false),
        (&b"VALUES"[..], false, true),
    ] {
        let mut instance = make(plugins::GAIN);
        instance
            .restore_state(&encode_state(component, words))
            .expect("the state is taken");
        let asked = instance.between_blocks();
        assert_eq!(asked.wants_to_be_made_again, reload_or_io, "{words:?}");
        assert_eq!(asked.values_changed, values, "{words:?}");
        assert!(!asked.latency_changed, "{words:?}");
        assert!(asked.edits.is_empty());
    }
}

#[test]
fn a_meter_follows_the_blocks_and_is_not_added_to() {
    let mut instance = make(plugins::GAIN);
    run_effect(&mut instance, A_BLOCK);
    let first = instance.control_outputs();
    // (A gain of one and a half keeps the peak under one, where the meter of the plugin stops.)
    instance.set_value(plugins::PARAMETER_GAIN, 1.5, 0);
    let heard = run_effect(&mut instance, A_BLOCK);
    let second = instance.control_outputs();
    assert_eq!(second.len(), 1, "one meter, said again");
    assert_ne!(first, second);
    let peak = heard.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!((second[0].1 - f64::from(peak)).abs() < 1e-7);
}

// --- letting go ---

#[test]
fn copies_are_made_and_let_go_again_and_again_and_the_module_outlives_the_copies() {
    let module = the_module();
    for _ in 0..20 {
        let (mut instance, _) =
            Instance::new(&module, id(plugins::GAIN), 48_000.0, 512, None).expect("the copy is made");
        run_effect(&mut instance, 16);
    }
    let (mut instance, _) =
        Instance::new(&module, id(plugins::INSTRUMENT), 48_000.0, 512, None).expect("the copy is made");
    drop(module);
    assert_eq!(
        run_instrument(&mut instance, 8, &[]).len(),
        8,
        "the copy still has its library"
    );
}

#[test]
fn two_copies_of_one_plugin_do_not_share_anything() {
    let mut first = make(plugins::GAIN);
    let mut second = make(plugins::GAIN);
    first.set_value(plugins::PARAMETER_GAIN, 4.0, 0);
    let heard_first = run_effect(&mut first, 32);
    let heard_second = run_effect(&mut second, 32);
    assert_eq!(heard_first[3], a_sound(32)[3] * 4.0);
    assert_eq!(heard_second[3], a_sound(32)[3]);
}

#[test]
fn a_copy_can_be_sent_to_another_thread() {
    fn is_send<T: Send>() {}
    is_send::<Instance>();
    let mut instance = make(plugins::GAIN);
    let heard = std::thread::spawn(move || run_effect(&mut instance, 16))
        .join()
        .expect("the thread ends");
    assert_eq!(heard, a_sound(16));
}

// --- the unsafe code ---

#[test]
fn every_unsafe_block_of_this_crate_says_why_it_is_fine() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut blocks = 0;
    for entry in std::fs::read_dir(source)
        .expect("the source is read")
        .map(Result::unwrap)
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with("tests.rs") || !name.ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).expect("a file is read");
        let lines: Vec<&str> = text.lines().collect();
        for (place, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("#![") {
                continue;
            }
            let is_a_block = code.contains("unsafe {") || code.contains("unsafe impl");
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
        blocks > 40,
        "the audit found only {blocks} blocks: it is looking in the wrong place"
    );
}

/// Dragonfly Hall Reverb's class, as this host writes it on Windows and as on the other systems: the same 16 bytes, read as four numbers in another order (DPF writes the bytes it means, and
/// Windows reads them as a `GUID`). (Written down from the real module on each system, 2026-11-03.)
const DRAGONFLY_HALL_ON_WINDOWS: &str = "445046206173636C3172666400000000";
const DRAGONFLY_HALL_ON_THE_OTHERS: &str = "2046504473616C633172666400000000";

#[test]
fn a_class_id_is_also_known_by_the_texts_that_other_systems_write_for_the_same_bytes() {
    // The class as this system writes it, and as the other writes it: each is known by the other's text.
    let (own, other) = if cfg!(target_os = "windows") {
        (DRAGONFLY_HALL_ON_WINDOWS, DRAGONFLY_HALL_ON_THE_OTHERS)
    } else {
        (DRAGONFLY_HALL_ON_THE_OTHERS, DRAGONFLY_HALL_ON_WINDOWS)
    };
    let class = ClassId::from_text(own).unwrap();
    let known_by = class.texts_that_other_systems_write();
    assert!(known_by.contains(&other.to_owned()), "{known_by:?}");
    assert!(
        !known_by.contains(&own.to_owned()),
        "its own text is not an other's: {known_by:?}"
    );
    assert_eq!(known_by.len(), 1, "one other text: {known_by:?}");
    assert!(ClassId::from_text(&known_by[0]).is_some(), "{known_by:?}");
}

#[test]
fn a_class_id_that_was_written_with_the_sdk_macro_is_the_same_text_everywhere_and_is_known_by_the_bytes_of_the_other_system()
 {
    // INLINE_UID plugins (Surge XT) have the same four numbers on every system, and other bytes: the text that another system writes for *its bytes* is a different one, which is only
    // an alias, and the id's own text is the one that is written.
    let class = id([0x1234_5678, 0x9ABC_DEF0, 0x1357_9BDF, 0x0246_8ACE]);
    assert_eq!(class.text(), "123456789ABCDEF013579BDF02468ACE");
    let known_by = class.texts_that_other_systems_write();
    assert!(!known_by.contains(&class.text()));
    assert_eq!(known_by.len(), 1, "the bytes of the other system: {known_by:?}");
    // (And the alias, read as an id of this system, is known by the text of the class: the swap of the bytes is its own inverse.)
    let back = ClassId::from_text(&known_by[0])
        .unwrap()
        .texts_that_other_systems_write();
    assert!(back.contains(&class.text()), "{back:?}");
}
