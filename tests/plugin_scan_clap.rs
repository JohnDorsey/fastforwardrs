// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of CLAP libraries with the real scan program (`fastforwardrs-plugin-scan`) and the test plugins (`crates/clap-test-plugins`), which are asked to crash, never return,
//! print noise and so on through an environment variable of the scan program (piece 7.8.2 of `docs/engine-phase-7-clap-design.md`). **Whatever a library does, the editor (this
//! test) goes on and is told why**, as for LADSPA (`tests/plugin_scan.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap_test_plugins::{CHOICE, FAILS_TO_ACTIVATE, GAIN, LATENCY, LATENCY_SAMPLES, library_file_for_tests};
use fastforwardrs::plugin_scan::{
    Breadcrumb, CachedOutcome, FileFingerprint, LibraryOutcome, PluginScanner, ScanCache, ScanFailure, recover,
};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");
const MISBEHAVE: &str = "FFRS_TEST_PLUGIN";
/// How many plugins the test library has.
const PLUGINS: usize = 21;

/// A copy of the good test library, named as a CLAP library is, in a folder of its own.
fn good_library_in(folder: &Path) -> PathBuf {
    let copy = folder.join("good.clap");
    fs::copy(library_file_for_tests(), &copy).expect("the library is copied");
    copy
}

fn scanner() -> PluginScanner {
    // (The check of the start of each plugin is given all the time it wants: it is given 12 seconds for a whole library otherwise, and a computer that is slow, as a runner of CI was
    // once, has the plugins at the end of the library not checked, which says `NotChecked`. The time limit of the scan is what stops a library that never ends.)
    PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory())
        .with_time_limit(Duration::from_secs(30))
        .with_environment_variable(fastforwardrs::plugin_scan::program::START_CHECK_SECONDS_VARIABLE, "600")
}

fn misbehaving(how: &str) -> PluginScanner {
    scanner().with_environment_variable(MISBEHAVE, how)
}

fn scanned(outcome: LibraryOutcome) -> clap_host::scan::LibraryScan {
    match outcome {
        LibraryOutcome::ScannedClap(scan) => scan,
        other => panic!("the library was not scanned as a CLAP library: {other:?}"),
    }
}

fn failure(outcome: LibraryOutcome) -> ScanFailure {
    match outcome {
        LibraryOutcome::Failed(failure) => failure,
        other => panic!("the library did not fail: {other:?}"),
    }
}

fn scan_with(how: &str) -> LibraryOutcome {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let report = misbehaving(how).scan_library(&library).expect("the scan could be run");
    assert!(!report.from_cache);
    report.outcome
}

#[test]
fn the_scan_of_a_good_clap_library_finds_every_plugin_with_its_ports_and_parameters_and_each_was_made_and_run() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());

    let report = scanner().scan_library(&library).expect("the scan could be run");
    assert_eq!(report.path, library);
    let scan = scanned(report.outcome);

    assert_eq!(scan.problems, Vec::<String>::new());
    assert_eq!(scan.plugins.len(), PLUGINS);
    let plugin = |id: &str| scan.plugins.iter().find(|plugin| plugin.description.id == id).unwrap();
    // (The ones that misbehave on purpose are the only ones with a problem: the scan made and ran each.)
    let with_a_problem: Vec<&str> = scan
        .plugins
        .iter()
        .filter(|plugin| plugin.problem.is_some())
        .map(|plugin| plugin.description.id.as_str())
        .collect();
    assert_eq!(with_a_problem.len(), 3, "{with_a_problem:?}");
    assert!(plugin(FAILS_TO_ACTIVATE).problem.is_some());
    // What the plugins said came across the wire whole.
    let gain = plugin(GAIN);
    assert_eq!((gain.audio_inputs.len(), gain.audio_outputs.len()), (1, 1));
    assert_eq!(gain.audio_inputs[0].channel_count, 2);
    assert_eq!(
        (
            gain.parameters[0].id,
            gain.parameters[0].minimum,
            gain.parameters[0].maximum
        ),
        (1, 0.0, 4.0)
    );
    assert_eq!(plugin(LATENCY).latency, LATENCY_SAMPLES);
    assert_eq!(plugin(CHOICE).parameters[1].texts, ["off", "double", "triple"]);
    // The check of the start came across the wire too: the scan program, which does nothing else while it checks, found the plugin that reads memory it did not write (as it does
    // on Windows and Ubuntu; macOS was not verified) and none of the others that are steady.
    use clap_host::start_check::StartCheck;
    if cfg!(target_os = "macos") {
        assert!(matches!(
            plugin(clap_test_plugins::READS_GARBAGE).start,
            StartCheck::Unreliable { .. } | StartCheck::Steady
        ));
    } else {
        assert!(
            matches!(
                plugin(clap_test_plugins::READS_GARBAGE).start,
                StartCheck::Unreliable { .. }
            ),
            "{:?}",
            plugin(clap_test_plugins::READS_GARBAGE).start
        );
    }
    assert_eq!(gain.start, StartCheck::Steady);
    assert_eq!(plugin(clap_test_plugins::INSTRUMENT).start, StartCheck::NotChecked);
    assert_eq!(plugin(FAILS_TO_ACTIVATE).start, StartCheck::NotChecked);
}

#[test]
fn a_clap_library_that_crashes_when_loaded_or_when_asked_for_its_plugins_is_failed_and_says_where() {
    for (how, doing) in [
        ("crash_when_loaded", "loading the library"),
        ("crash_when_described", "reading what the plugins say of themselves"),
    ] {
        let ScanFailure::Crashed { how: crash, during } = failure(scan_with(how)) else {
            panic!("{how}: not a crash");
        };
        assert_eq!(during.as_deref(), Some(doing), "{how}");
        assert!(!crash.is_empty(), "{how}");
    }
}

#[test]
fn a_clap_library_that_crashes_when_a_plugin_is_made_activated_or_run_is_failed_and_says_which() {
    let making = format!("making the plugin {GAIN} at 48000 Hz");
    let running = format!("running the plugin {GAIN} on 512 frames of silence");
    for (how, doing) in [
        ("crash_when_made", &making),
        ("crash_when_activated", &making),
        ("crash_when_run", &running),
    ] {
        let ScanFailure::Crashed { during, .. } = failure(scan_with(how)) else {
            panic!("{how}: not a crash");
        };
        assert_eq!(during.as_deref(), Some(doing.as_str()), "{how}");
    }
}

#[test]
fn a_clap_library_that_never_returns_is_stopped_at_the_time_limit_and_the_scan_goes_on() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let making = format!("making the plugin {GAIN} at 48000 Hz");
    let running = format!("running the plugin {GAIN} on 512 frames of silence");
    for (how, doing) in [
        ("hang_when_loaded", "loading the library".to_owned()),
        ("hang_when_made", making),
        ("hang_when_run", running),
    ] {
        let mut scanner = misbehaving(how).with_time_limit(Duration::from_secs(1));
        let began = Instant::now();

        let report = scanner.scan_library(&library).expect("the scan could be run");

        let took = began.elapsed();
        assert_eq!(
            failure(report.outcome),
            ScanFailure::TookTooLong {
                seconds: 1,
                during: Some(doing)
            },
            "{how}"
        );
        assert!(
            took >= Duration::from_secs(1) && took < Duration::from_secs(4),
            "{how}: stopped after {took:?}"
        );
    }
}

#[test]
fn a_clap_library_whose_init_says_no_is_failed_as_one_that_could_not_be_loaded() {
    assert!(matches!(
        failure(scan_with("init_says_no")),
        ScanFailure::CouldNotBeLoaded(_)
    ));
}

#[test]
fn a_clap_library_that_ends_the_program_without_an_answer_or_with_a_wrong_one_is_failed() {
    let ScanFailure::GaveNoAnswer { during } = failure(scan_with("exit_when_run")) else {
        panic!("not an exit with no answer");
    };
    assert_eq!(
        during.as_deref(),
        Some(format!("running the plugin {GAIN} on 512 frames of silence").as_str())
    );
    assert!(matches!(
        failure(scan_with("answer_wrong_when_run")),
        ScanFailure::AnswerWasNotInTheFormat(_)
    ));
}

#[test]
fn a_clap_library_that_prints_noise_is_scanned_all_the_same_and_one_with_no_plugins_has_none() {
    assert_eq!(scanned(scan_with("print_noise")).plugins.len(), PLUGINS);
    let none = scanned(scan_with("no_plugins"));
    assert!(none.plugins.is_empty() && none.problems.is_empty());
}

#[test]
fn files_named_as_clap_libraries_that_are_not_are_failed_with_what_the_system_said() {
    let folder = tempfile::tempdir().unwrap();
    let empty = folder.path().join("empty.clap");
    fs::write(&empty, b"").unwrap();
    let text = folder.path().join("text.clap");
    fs::write(&text, b"this is not a library").unwrap();
    // A library of this system that is not a CLAP one: the test library of LADSPA plugins, given a CLAP name.
    let other_kind = folder.path().join("ladspa.clap");
    fs::copy(ladspa_test_plugins::library_file_for_tests(), &other_kind).unwrap();

    for file in [&empty, &text, &other_kind] {
        let report = scanner().scan_library(file).expect("the scan could be run");
        let failed = failure(report.outcome);
        assert!(
            matches!(
                failed,
                ScanFailure::CouldNotBeLoaded(_) | ScanFailure::NotAClapLibrary(_)
            ),
            "{}: {failed}",
            file.display()
        );
        assert!(!failed.to_string().is_empty());
    }
}

#[test]
fn the_scan_of_a_folder_goes_through_the_clap_libraries_below_it_and_the_ladspa_ones_in_it_and_a_bad_one_costs_the_others_nothing()
 {
    let folder = tempfile::tempdir().unwrap();
    let good = good_library_in(folder.path());
    let below = folder.path().join("vendor").join("product");
    fs::create_dir_all(&below).unwrap();
    let deep = below.join("deep.clap");
    fs::copy(library_file_for_tests(), &deep).unwrap();
    let bad = folder.path().join("bad.clap");
    fs::write(&bad, b"nothing").unwrap();
    let ladspa = folder.path().join(format!(
        "ladspa.{}",
        fastforwardrs::plugin_scan::library_file_extension()
    ));
    fs::copy(ladspa_test_plugins::library_file_for_tests(), &ladspa).unwrap();
    fs::write(folder.path().join("readme.txt"), b"not a library").unwrap();

    let scan = scanner().scan_folders(&[folder.path().to_owned()]);

    assert_eq!(scan.problems, Vec::<String>::new());
    let found: Vec<&Path> = scan.libraries.iter().map(|report| report.path.as_path()).collect();
    // (The LADSPA libraries of the folder first, then the CLAP ones, in the order of their paths.)
    assert_eq!(found, [ladspa.as_path(), bad.as_path(), good.as_path(), deep.as_path()]);
    let outcome_of = |path: &Path| {
        &scan
            .libraries
            .iter()
            .find(|report| report.path == path)
            .unwrap()
            .outcome
    };
    assert!(matches!(outcome_of(&ladspa), LibraryOutcome::Scanned(_)));
    assert!(matches!(outcome_of(&bad), LibraryOutcome::Failed(_)));
    assert!(matches!(outcome_of(&good), LibraryOutcome::ScannedClap(_)));
    assert!(matches!(outcome_of(&deep), LibraryOutcome::ScannedClap(_)));
}

#[test]
fn a_clap_scan_that_was_made_is_not_made_again_until_the_file_changes_not_even_by_another_run_of_the_editor() {
    let folder = tempfile::tempdir().unwrap();
    let cache_file = folder.path().join("cache.json");
    let library = good_library_in(folder.path());

    let first = {
        let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file.clone()))
            .with_time_limit(Duration::from_secs(30));
        let scan = scanner.scan_folders(&[folder.path().to_owned()]);
        assert!(!scan.libraries[0].from_cache);
        scan.libraries[0].outcome.clone()
    };
    assert!(matches!(first, LibraryOutcome::ScannedClap(_)));

    // Another run of the editor: the scan program is one that cannot be run, and the answer comes from the file all the same.
    let mut again = PluginScanner::new(PathBuf::from("no such program"), ScanCache::at(cache_file.clone()));
    let report = again.scan_library(&library).expect("the cache answers");
    assert!(report.from_cache);
    assert_eq!(report.outcome, first);

    // The file changed: the cache is not believed.
    fs::OpenOptions::new()
        .append(true)
        .open(&library)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"x"))
        .unwrap();
    let mut changed = PluginScanner::new(PathBuf::from("no such program"), ScanCache::at(cache_file));
    assert!(changed.scan_library(&library).is_err());
}

#[test]
fn a_clap_library_that_was_in_use_when_the_editor_ended_badly_is_failed_by_the_breadcrumb_until_it_changes() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let breadcrumb_file = folder.path().join("plugins_in_use.json");
    Breadcrumb::at(breadcrumb_file.clone()).note_use_of(&library).unwrap();
    let mut scanner = scanner();

    let recovery = recover(&breadcrumb_file, &mut scanner).expect("the last run left a breadcrumb");

    assert_eq!(recovery.failed, library);
    let kept = scanner
        .cache()
        .lookup(&library, &FileFingerprint::of(&library).unwrap());
    assert_eq!(
        kept,
        Some(&CachedOutcome::Failed(ScanFailure::WasInUseWhenTheProgramEndedBadly {
            others: 0
        }))
    );
}

/// A bundle, which is what a CLAP library is on macOS: a folder, with the library in `Contents/MacOS` and a description of it. (Made by the test, and only where it is loaded.)
#[cfg(target_os = "macos")]
#[test]
fn a_clap_library_that_is_a_bundle_is_found_and_scanned_on_macos() {
    let folder = tempfile::tempdir().unwrap();
    let bundle = folder.path().join("Good.clap");
    let executable_folder = bundle.join("Contents").join("MacOS");
    fs::create_dir_all(&executable_folder).unwrap();
    fs::copy(library_file_for_tests(), executable_folder.join("Good")).unwrap();
    fs::write(
        bundle.join("Contents").join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Good</string>
<key>CFBundleIdentifier</key><string>org.fastforwardrs.test.good</string>
<key>CFBundlePackageType</key><string>BNDL</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>"#,
    )
    .unwrap();

    let scan = scanner().scan_folders(&[folder.path().to_owned()]);

    assert_eq!(scan.problems, Vec::<String>::new());
    assert_eq!(scan.libraries.len(), 1);
    assert_eq!(scan.libraries[0].path, bundle);
    assert_eq!(scanned(scan.libraries[0].outcome.clone()).plugins.len(), PLUGINS);
}

#[test]
fn the_plugins_of_the_test_library_that_can_be_effects_are_found_and_the_others_are_listed_with_the_reason() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());

    let scan = scanner().scan_folders(&[folder.path().to_owned()]);
    let found = fastforwardrs::plugin_scan::plugins_found_in(&scan.libraries);

    let keys: Vec<&str> = found
        .effects
        .iter()
        .map(|effect| effect.descriptor.key.as_ref())
        .collect();
    assert_eq!(keys.len(), 18, "{keys:?}");
    assert!(keys.iter().all(|key| key.starts_with("clap:org.fastforwardrs.test.")));
    assert!(found.effects.iter().all(|effect| effect.library == library));
    let reason_of = |id: &str| {
        found
            .not_used
            .iter()
            .find(|not_used| not_used.label == id)
            .unwrap_or_else(|| panic!("{id} is not listed: {:?}", found.not_used))
            .reason
            .clone()
    };
    assert_eq!(found.not_used.len(), 3, "{:?}", found.not_used);
    // (The three instruments are effects since phase 13: synthesizers, with no main audio input, one stereo output, and the flag that says they are played by notes. The third has an input that
    // is a side chain and not the main one, as Surge XT has: it is a synthesizer all the same.)
    for id in [
        clap_test_plugins::INSTRUMENT,
        clap_test_plugins::INSTRUMENT_MIDI,
        clap_test_plugins::INSTRUMENT_WITH_A_SIDE_CHAIN,
    ] {
        let synthesizer = found
            .effects
            .iter()
            .find(|effect| effect.descriptor.key == format!("clap:{id}"))
            .unwrap_or_else(|| panic!("{id} is not an effect"));
        assert!(synthesizer.descriptor.makes_sound_from_notes, "{id}");
        assert_eq!((synthesizer.audio_inputs, synthesizer.audio_outputs), (0, 2), "{id}");
    }
    assert!(
        found
            .effects
            .iter()
            .filter(|effect| effect.descriptor.makes_sound_from_notes)
            .count()
            == 3
    );
    assert!(reason_of(FAILS_TO_ACTIVATE).contains("could not be made or run"));
    assert!(reason_of(clap_test_plugins::FAILS_TO_PROCESS).contains("could not be made or run"));
    assert!(reason_of(clap_test_plugins::PANICS).contains("could not be made or run"));
    // The side chain is not looked at; the choice has its enum; the mono plugin is one channel in and out.
    let effect = |id: &str| {
        found
            .effects
            .iter()
            .find(|effect| effect.descriptor.key == format!("clap:{id}"))
            .unwrap()
    };
    assert_eq!(
        (
            effect(clap_test_plugins::SIDE_CHAIN).audio_inputs,
            effect(clap_test_plugins::SIDE_CHAIN).audio_outputs
        ),
        (2, 2)
    );
    assert_eq!(
        (
            effect(clap_test_plugins::MONO_HALF).audio_inputs,
            effect(clap_test_plugins::MONO_HALF).audio_outputs
        ),
        (1, 1)
    );
    assert_eq!(effect(CHOICE).value_handles, [1, 2]);
    assert_eq!(effect(CHOICE).descriptor.parameters.len(), 2);
    // (Phase 10: the scan says which plugins have a window of their own.)
    assert!(
        effect(clap_test_plugins::EDITOR).has_a_window,
        "the one that has a window"
    );
    for id in [GAIN, CHOICE, clap_test_plugins::SWAP, clap_test_plugins::MONO_HALF] {
        assert!(!effect(id).has_a_window, "{id} has none");
    }
}
