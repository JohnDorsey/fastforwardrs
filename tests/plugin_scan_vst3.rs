// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of VST3 modules with the real scan program (`fastforwardrs-plugin-scan`) and the test plugins (`crates/vst3-test-plugins`) (piece 15.2 of
//! `docs/engine-phase-15-vst3-design.md`): a module is a folder, the scan loads its library, lists its classes and makes and runs each plugin once, and **what it finds is the
//! effects that the editor offers and the reasons for the plugins that it does not**.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use fastforwardrs::plugin_scan::{
    LibraryOutcome, PluginScanner, PluginsFound, ScanCache, ScanFailure, key_of_a_vst3_plugin, plugins_found_in,
};
use fastforwardrs::project::{InstalledPlugin, ParameterControl, PluginAddress, PropertyValue, ValueScale};
use vst3_host::ClassId;
use vst3_test_plugins as plugins;
use vst3_test_plugins::write_module;

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

fn scanner() -> PluginScanner {
    PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory()).with_time_limit(Duration::from_secs(60))
}

fn id(numbers: [u32; 4]) -> ClassId {
    ClassId::from_numbers(numbers[0], numbers[1], numbers[2], numbers[3])
}

/// A folder (of modules, as `%COMMONPROGRAMFILES%\VST3` is) with the test module in it.
fn folder_with_the_test_module() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let module = write_module(folder.path());
    wait_until_the_times_of_the_files_have_settled(&module);
    (folder, module)
}

/// Windows reports the time of a file that was only just written a little late (it moved by a millisecond between two looks, with nothing written): a test of the cache waits
/// until two looks give the same, as a module that was installed a while ago does at once.
fn wait_until_the_times_of_the_files_have_settled(module: &std::path::Path) {
    let mut last = fastforwardrs::plugin_scan::FileFingerprint::of(module).unwrap();
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(20));
        let now = fastforwardrs::plugin_scan::FileFingerprint::of(module).unwrap();
        if now == last {
            return;
        }
        last = now;
    }
    panic!("the times of the files did not settle");
}

fn scanned(outcome: LibraryOutcome) -> vst3_host::ModuleScan {
    match outcome {
        LibraryOutcome::ScannedVst3(scan) => scan,
        other => panic!("the module was not scanned as a VST3 module: {other:?}"),
    }
}

fn what_the_scan_found() -> PluginsFound {
    let (_folder, module) = folder_with_the_test_module();
    let report = scanner().scan_library(&module).expect("the scan could be run");
    plugins_found_in(&[report])
}

fn effect_of(found: &PluginsFound, numbers: [u32; 4]) -> &InstalledPlugin {
    let key = key_of_a_vst3_plugin(id(numbers));
    found
        .effects
        .iter()
        .find(|effect| effect.descriptor.key == key)
        .unwrap_or_else(|| panic!("no effect {key}: {:?}", found.not_used))
}

#[test]
fn a_module_is_scanned_by_the_scan_program_and_each_of_its_plugins_says_whether_it_works() {
    let (_folder, module) = folder_with_the_test_module();

    let report = scanner().scan_library(&module).expect("the scan could be run");

    assert!(!report.from_cache);
    let scan = scanned(report.outcome);
    assert_eq!(scan.factory.vendor, "fastforwardrs tests");
    assert_eq!(
        scan.classes.len(),
        11,
        "the plugins; not the controllers or the compatibility class"
    );
    let of = |numbers: [u32; 4]| scan.classes.iter().find(|class| class.class.id == id(numbers)).unwrap();
    assert_eq!(of(plugins::GAIN).problem, None);
    assert_eq!(of(plugins::LATENCY).facts.as_ref().unwrap().latency, 64);
    assert!(
        of(plugins::FAILS_TO_INITIALIZE)
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("would not initialize"))
    );
    assert!(
        of(plugins::FAILS_TO_PROCESS)
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("process said it failed"))
    );
    assert!(
        of(plugins::NOT_NUMBERS)
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("not made of numbers"))
    );
}

#[test]
fn a_folder_of_modules_is_scanned_and_kept_and_a_module_that_changed_is_scanned_again() {
    let (folder, module) = folder_with_the_test_module();
    let cache_file = folder.path().join("scans.json");
    let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file.clone()))
        .with_time_limit(Duration::from_secs(60));

    let first = scanner.scan_folders(&[folder.path().to_owned()]);

    assert_eq!(first.problems, Vec::<String>::new());
    assert_eq!(first.libraries.len(), 1, "the module is the one library of the folder");
    assert_eq!(first.libraries[0].path, module);
    assert!(!first.libraries[0].from_cache);
    assert_eq!(scanned(first.libraries[0].outcome.clone()).classes.len(), 11);

    // Again: from the cache, without the scan program.
    let second = scanner.scan_folders(&[folder.path().to_owned()]);
    assert!(second.libraries[0].from_cache);
    assert_eq!(second.libraries[0].outcome, first.libraries[0].outcome);

    // From the file the cache was written to, by a scanner that never saw the module.
    let mut after_a_restart = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file));
    assert!(after_a_restart.scan_folders(&[folder.path().to_owned()]).libraries[0].from_cache);

    // The library inside the module changes (a byte is added to the end: it still loads): it is scanned again.
    let inside = fastforwardrs_library_inside(&module);
    let mut bytes = fs::read(&inside).unwrap();
    bytes.push(0);
    fs::write(&inside, bytes).unwrap();
    let third = scanner.scan_folders(&[folder.path().to_owned()]);
    assert!(
        !third.libraries[0].from_cache,
        "the module is not the one that was scanned"
    );
}

fn fastforwardrs_library_inside(module: &std::path::Path) -> PathBuf {
    vst3_host::library_file_of(module).expect("the library is in the module")
}

#[test]
fn something_named_like_a_module_that_is_not_one_is_a_failure_and_a_library_of_another_kind_is_told_apart() {
    let folder = tempfile::tempdir().unwrap();

    // A folder with the ending and nothing in it.
    let empty = folder.path().join("empty.vst3");
    fs::create_dir_all(empty.join("Contents")).unwrap();
    let report = scanner().scan_library(&empty).expect("the scan could be run");
    assert!(
        matches!(report.outcome, LibraryOutcome::Failed(ScanFailure::CouldNotBeLoaded(ref reason)) if reason.contains("no library")),
        "{:?}",
        report.outcome
    );

    // A file that is no library.
    let text = folder.path().join("text.vst3");
    fs::write(&text, "not a library").unwrap();
    let report = scanner().scan_library(&text).expect("the scan could be run");
    assert!(
        matches!(report.outcome, LibraryOutcome::Failed(ScanFailure::CouldNotBeLoaded(_))),
        "{:?}",
        report.outcome
    );

    // A library that loads, and is a CLAP plugin library: it has no factory.
    let clap = folder.path().join("really_clap.vst3");
    fs::copy(clap_test_plugins::library_file_for_tests(), &clap).unwrap();
    let report = scanner().scan_library(&clap).expect("the scan could be run");
    assert!(
        matches!(report.outcome, LibraryOutcome::Failed(ScanFailure::NotAVst3Module(_))),
        "{:?}",
        report.outcome
    );
}

#[test]
fn the_plugins_of_a_module_that_work_are_effects_and_the_others_say_why_not() {
    let found = what_the_scan_found();

    let keys: Vec<String> = found
        .effects
        .iter()
        .map(|effect| effect.descriptor.key.to_string())
        .collect();
    assert_eq!(found.effects.len(), 7, "{keys:?}");
    for numbers in [
        plugins::GAIN,
        plugins::LATENCY,
        plugins::INSTRUMENT,
        plugins::SINGLE,
        plugins::SIDE_CHAIN_INSTRUMENT,
        plugins::SIDE_CHAIN_EFFECT,
        plugins::NO_DEFAULTS,
    ] {
        effect_of(&found, numbers);
    }
    assert_eq!(found.not_used.len(), 4, "{:?}", found.not_used);
    let reasons: Vec<&str> = found.not_used.iter().map(|not_used| not_used.reason.as_str()).collect();
    for wanted in [
        "would not initialize",
        "would not become active",
        "process said it failed",
        "not made of numbers",
    ] {
        assert!(
            reasons.iter().any(|reason| reason.contains(wanted)),
            "{wanted}: {reasons:?}"
        );
    }
    assert!(
        found
            .not_used
            .iter()
            .any(|not_used| not_used.name == "Test Fails To Activate \u{e9}"
                && not_used.label == id(plugins::FAILS_TO_ACTIVATE).text()),
        "named by its name and its id: {:?}",
        found.not_used
    );
}

#[test]
fn a_plugin_whose_controller_makes_a_window_is_said_to_have_one_and_the_others_are_not() {
    let found = what_the_scan_found();

    for (numbers, has_a_window) in [
        (plugins::GAIN, true),
        (plugins::SINGLE, true),
        (plugins::LATENCY, false),
        (plugins::INSTRUMENT, false),
        (plugins::SIDE_CHAIN_EFFECT, false),
        (plugins::NO_DEFAULTS, true),
    ] {
        assert_eq!(
            effect_of(&found, numbers).has_a_window,
            has_a_window,
            "{}",
            id(numbers).text()
        );
    }
}

#[test]
fn what_a_parameter_says_is_the_kind_of_value_the_effect_has() {
    let found = what_the_scan_found();
    let gain = effect_of(&found, plugins::GAIN);

    // Gain (continuous), Mode (a list), Bypass (a switch); Secret is hidden and Level is read-only, and are not values.
    assert_eq!(gain.value_handles, [1, 2, 5]);
    let [amount, mode, bypass] = &gain.descriptor.parameters[..] else {
        panic!("three values: {:?}", gain.descriptor.parameters)
    };
    assert_eq!(amount.label, "Gain");
    assert_eq!(amount.unit, "x");
    assert_eq!(amount.control, ParameterControl::Automatic);
    assert_eq!(amount.default, PropertyValue::Number(1.0), "a quarter of 0 to 4");
    assert_eq!(amount.range, Some((0.0, 4.0)));
    assert_eq!(amount.scale, ValueScale::Linear);
    let ParameterControl::Choice(options) = &mode.control else {
        panic!("a choice: {:?}", mode.control);
    };
    let options: Vec<(&str, &str)> = options.iter().map(|option| (&*option.id, &*option.label)).collect();
    assert_eq!(options, [("0", "Plain"), ("1", "Double"), ("2", "Triple")]);
    assert_eq!(mode.default, PropertyValue::Words("0".to_owned()));
    assert_eq!(bypass.control, ParameterControl::Toggle);
    assert_eq!(bypass.default, PropertyValue::Flag(false));
}

#[test]
fn an_effect_is_offered_under_the_name_the_vendor_the_key_and_the_address_that_the_module_gives() {
    let found = what_the_scan_found();
    let gain = effect_of(&found, plugins::GAIN);

    assert_eq!(gain.descriptor.key, key_of_a_vst3_plugin(id(plugins::GAIN)));
    assert!(gain.descriptor.key.starts_with("vst3:") && gain.descriptor.key.len() == 5 + 32);
    assert_eq!(gain.descriptor.name, "test gain \u{e9}");
    assert_eq!(gain.descriptor.category, "fastforwardrs tests");
    assert_eq!(
        gain.address,
        PluginAddress::Vst3 {
            class_id: id(plugins::GAIN)
        }
    );
    assert!(gain.library.ends_with("test.vst3"));
    assert_eq!((gain.audio_inputs, gain.audio_outputs), (2, 2));
    assert!(!gain.descriptor.makes_sound_from_notes);
    assert!(gain.has_a_window, "its controller makes a window");
    assert!(
        gain.descriptor.description.contains("VST3 plugin (Fx)"),
        "{}",
        gain.descriptor.description
    );
}

#[test]
fn the_instrument_is_a_synthesizer_the_effect_with_a_side_chain_is_an_effect_and_the_delay_has_its_tail() {
    let found = what_the_scan_found();

    let instrument = effect_of(&found, plugins::INSTRUMENT);
    assert!(instrument.descriptor.makes_sound_from_notes);
    assert_eq!((instrument.audio_inputs, instrument.audio_outputs), (0, 2));
    // Bend (0 to 1, in the middle), Wheel, and AllNotesOff (a switch).
    let controls: Vec<&ParameterControl> = instrument
        .descriptor
        .parameters
        .iter()
        .map(|value| &value.control)
        .collect();
    assert_eq!(
        controls,
        [
            &ParameterControl::Automatic,
            &ParameterControl::Automatic,
            &ParameterControl::Toggle
        ]
    );
    assert_eq!(instrument.descriptor.parameters[0].default, PropertyValue::Number(0.5));

    let surge_like = effect_of(&found, plugins::SIDE_CHAIN_INSTRUMENT);
    assert!(
        surge_like.descriptor.makes_sound_from_notes,
        "its only input is a side chain"
    );
    assert_eq!(surge_like.audio_inputs, 0);

    let with_a_side_chain = effect_of(&found, plugins::SIDE_CHAIN_EFFECT);
    assert!(!with_a_side_chain.descriptor.makes_sound_from_notes);
    assert_eq!(
        (with_a_side_chain.audio_inputs, with_a_side_chain.audio_outputs),
        (2, 2)
    );

    let delay = effect_of(&found, plugins::LATENCY);
    assert_eq!(
        delay.pre_roll_samples,
        plugins::TAIL_SAMPLES as usize,
        "the tail it says"
    );
    assert!(delay.descriptor.parameters.is_empty());

    let single = effect_of(&found, plugins::SINGLE);
    assert_eq!(single.descriptor.parameters.len(), 1);
    assert_eq!(single.value_handles, [1]);
}

/// Real VST3 modules through the real scan program and into effects: **ignored**, it needs a folder with real `.vst3` modules (`FFRS_VST3_MODULES`), which CI does not have. It is
/// the check that what the editor offers for Dragonfly Reverb and Surge XT is what they are.
/// `FFRS_VST3_MODULES=E:/s/sffrs/vst3_real cargo test --test plugin_scan_vst3 -- --ignored --nocapture`
#[test]
#[ignore = "needs real VST3 modules (FFRS_VST3_MODULES)"]
fn real_modules_are_scanned_and_become_the_effects_a_person_is_offered() {
    let folder =
        PathBuf::from(std::env::var("FFRS_VST3_MODULES").expect("FFRS_VST3_MODULES names a folder with .vst3 modules"));
    let mut scanner =
        PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory()).with_time_limit(Duration::from_secs(60));

    let scan = scanner.scan_folders(&[folder]);

    assert_eq!(scan.problems, Vec::<String>::new());
    for report in &scan.libraries {
        println!(
            "{}: {:?}",
            report.path.display(),
            std::mem::discriminant(&report.outcome)
        );
    }
    let found = plugins_found_in(&scan.libraries);
    for effect in &found.effects {
        println!(
            "{} ({}): {} values, {} in {} out, synthesizer {}, pre-roll {}",
            effect.descriptor.name,
            effect.descriptor.key,
            effect.descriptor.parameters.len(),
            effect.audio_inputs,
            effect.audio_outputs,
            effect.descriptor.makes_sound_from_notes,
            effect.pre_roll_samples
        );
    }
    for not_used in &found.not_used {
        println!("not used: {} {}: {}", not_used.name, not_used.label, not_used.reason);
    }
    // (The folder holds the CLAP and LV2 builds of the same plugins too: only the VST3 ones are looked at.)
    let named = |name: &str| {
        found
            .effects
            .iter()
            .find(|effect| effect.descriptor.name == name && effect.descriptor.key.starts_with("vst3:"))
    };
    let room = named("dragonfly room reverb").expect("Dragonfly Room Reverb is an effect");
    assert_eq!((room.audio_inputs, room.audio_outputs), (2, 2));
    assert!(!room.descriptor.makes_sound_from_notes);
    assert!(room.descriptor.parameters.len() >= 10, "the sliders of a reverb");
    let surge = named("surge xt").expect("Surge XT is an effect");
    assert!(
        surge.descriptor.makes_sound_from_notes,
        "its only input is a side chain"
    );
    // (774 of its 2,855 parameters can be automated; they were cut to the first 255 until 2026-11-03.)
    assert_eq!(
        surge.descriptor.parameters.len(),
        774,
        "every parameter of Surge XT that a person can automate is a value"
    );
    assert_eq!(surge.value_handles.len(), 774);
    assert!(named("surge xt effects").is_some());
}
