// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of LV2 bundles with the real scan program (`fastforwardrs-plugin-scan`) and the test plugins (`crates/lv2-test-plugins`) (piece 11.2 of
//! `docs/engine-phase-11-lv2-design.md`): a bundle is a folder, the scan reads its Turtle, loads its binary and makes and runs each plugin once, and **what it finds is the
//! effects that the editor offers and the reasons for the plugins that it does not**.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use fastforwardrs::plugin_scan::{
    LibraryOutcome, PluginScanner, ScanCache, ScanFailure, key_of_an_lv2_plugin, plugins_found_in,
};
use fastforwardrs::project::{ParameterControl, PluginAddress, PropertyValue, ValueScale};
use lv2_test_plugins::{CONTROLS, FAILS, GAIN, LATENCY, NEEDS_LOG, STATE, write_bundle};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

fn scanner() -> PluginScanner {
    PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory()).with_time_limit(Duration::from_secs(60))
}

/// A folder (of bundles, as `LV2_PATH` has) with the test bundle in it.
fn folder_with_the_test_bundle() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let bundle = write_bundle(folder.path());
    (folder, bundle)
}

fn scanned(outcome: LibraryOutcome) -> lv2_host::BundleScan {
    match outcome {
        LibraryOutcome::ScannedLv2(scan) => scan,
        other => panic!("the bundle was not scanned as an LV2 bundle: {other:?}"),
    }
}

#[test]
fn a_bundle_is_scanned_by_the_scan_program_and_each_of_its_plugins_says_whether_it_works() {
    let (_folder, bundle) = folder_with_the_test_bundle();

    let report = scanner().scan_library(&bundle).expect("the scan could be run");

    assert!(!report.from_cache);
    let scan = scanned(report.outcome);
    assert_eq!(scan.plugins.len(), 10);
    assert_eq!(scan.problems, Vec::<String>::new());
    let of = |uri: &str| {
        scan.plugins
            .iter()
            .find(|plugin| plugin.description.uri == uri)
            .unwrap()
    };
    assert_eq!(of(GAIN).problem, None);
    assert!(
        of(NEEDS_LOG)
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("log#log"))
    );
    assert!(
        of(FAILS)
            .problem
            .as_deref()
            .is_some_and(|problem| problem.contains("gave nothing"))
    );
    assert_eq!(of(LATENCY).latency, 64);
}

#[test]
fn a_folder_of_bundles_is_scanned_and_kept_and_a_bundle_that_changed_is_scanned_again() {
    let (folder, bundle) = folder_with_the_test_bundle();
    let cache_file = folder.path().join("scans.json");
    let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file.clone()))
        .with_time_limit(Duration::from_secs(60));

    let first = scanner.scan_folders(&[folder.path().to_owned()]);

    assert_eq!(first.problems, Vec::<String>::new());
    assert_eq!(first.libraries.len(), 1, "the bundle is the one library of the folder");
    assert_eq!(first.libraries[0].path, bundle);
    assert!(!first.libraries[0].from_cache);
    assert!(cache_file.is_file(), "what was found is kept");

    // Another scanner that reads the file does not scan the bundle again...
    let mut again = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file.clone()))
        .with_time_limit(Duration::from_secs(60));
    let second = again.scan_folders(&[folder.path().to_owned()]);
    assert!(second.libraries[0].from_cache);

    // ...until a file of the bundle is not what it was (a file of its description, not its binary: the size of a folder is of all that is in it).
    let ttl = bundle.join("gain.ttl");
    let mut text = fs::read_to_string(&ttl).unwrap();
    text.push_str("\n# a change\n");
    fs::write(&ttl, text).unwrap();
    let mut third_scanner =
        PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file)).with_time_limit(Duration::from_secs(60));
    let third = third_scanner.scan_folders(&[folder.path().to_owned()]);
    assert!(!third.libraries[0].from_cache, "a file of the bundle changed");
}

#[test]
fn a_folder_that_is_not_a_bundle_and_has_no_library_is_not_scanned_and_a_bundle_without_a_manifest_that_is_turtle_is_a_failure()
 {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir(folder.path().join("not_a_bundle")).unwrap();
    fs::write(folder.path().join("not_a_bundle").join("readme.txt"), "nothing").unwrap();
    let broken = folder.path().join("broken.lv2");
    fs::create_dir(&broken).unwrap();
    fs::write(broken.join("manifest.ttl"), "this is <not turtle").unwrap();
    let mut scanner = scanner();

    let scan = scanner.scan_folders(&[folder.path().to_owned()]);

    assert_eq!(
        scan.libraries.len(),
        1,
        "only the folder that has a manifest is a bundle: {:?}",
        scan.libraries
    );
    match &scan.libraries[0].outcome {
        LibraryOutcome::Failed(failure @ ScanFailure::NotAnLv2Bundle(reason)) => {
            assert!(reason.contains("manifest.ttl"), "{reason}");
            assert!(
                failure.to_string().starts_with("it is not an LV2 bundle: "),
                "{failure}"
            );
        }
        other => panic!("{other:?}"),
    }
}

/// The effects that the scan of the test bundle makes, and the plugins that are not used with the reasons.
fn what_the_scan_found() -> fastforwardrs::plugin_scan::PluginsFound {
    let (_folder, bundle) = folder_with_the_test_bundle();
    let report = scanner().scan_library(&bundle).expect("the scan could be run");
    plugins_found_in(&[report])
}

#[test]
fn the_plugins_of_a_bundle_that_work_are_effects_and_the_others_say_why_not() {
    let found = what_the_scan_found();

    let mut keys: Vec<String> = found
        .effects
        .iter()
        .map(|effect| effect.descriptor.key.to_string())
        .collect();
    keys.sort();
    // (The plugins that have an audio input and an audio output and were made and run: the gain, the controls, the latency, the atoms, the options, the worker, the state.)
    let mut expected: Vec<String> = [
        GAIN,
        CONTROLS,
        LATENCY,
        lv2_test_plugins::ATOMS,
        lv2_test_plugins::OPTIONS,
        lv2_test_plugins::WORKER,
        STATE,
    ]
    .iter()
    .map(|uri| key_of_an_lv2_plugin(uri))
    .collect();
    expected.sort();
    assert_eq!(keys, expected);
    let reasons: Vec<&str> = found.not_used.iter().map(|not_used| not_used.reason.as_str()).collect();
    assert_eq!(reasons.len(), 3, "{reasons:?}");
    assert!(reasons.iter().any(|reason| reason.contains("log#log")), "{reasons:?}");
    assert!(
        reasons.iter().any(|reason| reason.contains("gave nothing")),
        "{reasons:?}"
    );
    assert!(
        reasons.iter().any(|reason| reason.contains("no audio input")),
        "the instrument is not an effect yet: {reasons:?}"
    );
}

#[test]
fn what_a_port_says_is_the_kind_of_value_the_effect_has() {
    let found = what_the_scan_found();
    let controls = found
        .effects
        .iter()
        .find(|effect| effect.descriptor.key == key_of_an_lv2_plugin(CONTROLS))
        .expect("the plugin with every kind of control port is an effect");

    assert_eq!(controls.audio_inputs, 1);
    assert_eq!(controls.audio_outputs, 1);
    assert_eq!(
        controls.value_handles,
        vec![2, 3, 4, 5, 6],
        "the control inputs, by the number of the port"
    );
    assert!(matches!(&controls.address, PluginAddress::Lv2 { uri } if uri == CONTROLS));
    let parameters = &controls.descriptor.parameters;
    let by_id = |id: &str| parameters.iter().find(|parameter| parameter.id == id).unwrap();

    let bypass = by_id("bypass");
    assert!(matches!(bypass.control, ParameterControl::Toggle));
    assert_eq!(bypass.default, PropertyValue::Flag(false));
    let steps = by_id("steps");
    assert!(matches!(steps.control, ParameterControl::Integer));
    assert_eq!(
        (steps.default.clone(), steps.range),
        (PropertyValue::Number(4.0), Some((1.0, 16.0)))
    );
    let mode = by_id("mode");
    let ParameterControl::Choice(options) = &mode.control else {
        panic!("a choice: {:?}", mode.control)
    };
    let seen: Vec<(String, String)> = options
        .iter()
        .map(|option| (option.id.to_string(), option.label.to_string()))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("0".to_owned(), "Clean".to_owned()),
            ("1".to_owned(), "Warm".to_owned()),
            ("2".to_owned(), "Hot".to_owned())
        ]
    );
    assert_eq!(mode.default, PropertyValue::Words("1".to_owned()));
    let cutoff = by_id("cutoff");
    assert_eq!(cutoff.scale, ValueScale::Logarithmic);
    assert_eq!(cutoff.unit.as_ref(), "Hz");
    assert_eq!(cutoff.range, Some((20.0, 20_000.0)));
    let rate = by_id("rate");
    assert_eq!(
        rate.range,
        Some((0.0, 24_000.0)),
        "a fraction of the sample rate of the sound engine, 48000"
    );
    assert_eq!(rate.default, PropertyValue::Number(12_000.0));
}

#[test]
fn an_effect_is_offered_under_the_name_the_author_and_the_key_that_the_bundle_gives() {
    let found = what_the_scan_found();
    let gain = found
        .effects
        .iter()
        .find(|effect| effect.descriptor.key == key_of_an_lv2_plugin(GAIN))
        .unwrap();

    assert_eq!(
        gain.descriptor.key, "lv2:urn_fastforwardrs_test_gain",
        "the colons of the URI are made underscores: a key has the one colon"
    );
    assert_eq!(gain.descriptor.name, "test gain");
    assert_eq!(
        gain.descriptor.category, "fastforwardrs tests",
        "the author is the category"
    );
    assert_eq!(
        gain.descriptor.version,
        (1, 0),
        "the minor and the micro version of the plugin"
    );
    // (Phase 11.5: the gain plugin of the test bundle has a window of this system's class, and the other plugins have none, except the quiet one that the latency plugin has.)
    assert!(
        gain.has_a_window,
        "its bundle names an interface of this system's class"
    );
    let controls = found
        .effects
        .iter()
        .find(|effect| effect.descriptor.key == key_of_an_lv2_plugin(lv2_test_plugins::CONTROLS))
        .unwrap();
    assert!(!controls.has_a_window, "it names none");
    // The library of the effect is the bundle's folder (what is trusted is the folder that the bundles are in).
    assert_eq!(gain.library.file_name().unwrap(), "test.lv2");
}

#[test]
fn the_presets_of_the_bundle_come_with_the_effects_they_are_for_with_their_values_made_as_the_controls_of_the_effect_make_them()
 {
    let found = what_the_scan_found();
    let effect = |uri: &str| {
        found
            .effects
            .iter()
            .find(|effect| effect.descriptor.key == key_of_an_lv2_plugin(uri))
            .unwrap()
    };

    let gain = effect(GAIN);
    let labels: Vec<&str> = gain.presets.iter().map(|preset| preset.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            lv2_test_plugins::GAIN_PRESET_CLOSES_THE_WINDOW_LABEL,
            lv2_test_plugins::GAIN_PRESET_QUIET_LABEL
        ],
        "in the order of their URIs"
    );
    assert_eq!(
        gain.presets[1].values,
        vec![(0, PropertyValue::Number(lv2_test_plugins::GAIN_PRESET_QUIET_VALUE))]
    );
    assert_eq!(gain.presets[0].values, vec![(0, PropertyValue::Number(3.0))]);
    let controls = effect(CONTROLS);
    assert_eq!(controls.presets.len(), 1);
    assert_eq!(
        controls.presets[0].label,
        lv2_test_plugins::CONTROLS_PRESET_EVERYTHING_LABEL
    );
    assert_eq!(
        controls.presets[0].values,
        vec![
            (0, PropertyValue::Flag(true)),
            (1, PropertyValue::Number(6.0)),
            (2, PropertyValue::Words("2".to_owned())),
            (3, PropertyValue::Number(440.0)),
            (4, PropertyValue::Number(12_000.0)),
        ],
        "a switch, a whole number, a choice, a frequency, and a range that is a fraction of the sample rate: all as numbers of the file, made into the values"
    );
    assert!(
        found
            .effects
            .iter()
            .filter(|effect| effect.descriptor.key != key_of_an_lv2_plugin(GAIN)
                && effect.descriptor.key != key_of_an_lv2_plugin(CONTROLS))
            .all(|effect| effect.presets.is_empty()),
        "the other plugins have none, and the preset of a plugin that is not in the bundle is for no one"
    );
}
