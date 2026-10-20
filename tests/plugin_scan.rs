// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scan of plugin libraries with the real scan program (`fastforwardrs-plugin-scan`) and the test plugins (`crates/ladspa-test-plugins`), which are asked to crash, never
//! return, print noise and so on through an environment variable of the scan program. **Whatever a library does, the editor (this test) goes on and is told why.**

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use fastforwardrs::plugin_scan::{
    CachedOutcome, FileFingerprint, LibraryOutcome, LibraryScan, PluginScanner, ScanCache, ScanFailure, ScannerProblem,
    scanner_program_beside_this_program,
};
use ladspa_test_plugins::{bad_libraries, library_file_for_tests};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");
const MISBEHAVE: &str = "FFRS_TEST_PLUGIN";

/// A copy of the good test library in a folder of its own, so that the folder holds what a test puts in it.
fn good_library_in(folder: &Path) -> PathBuf {
    let copy = folder.join(format!("good.{}", fastforwardrs::plugin_scan::library_file_extension()));
    fs::copy(library_file_for_tests(), &copy).expect("the library is copied");
    copy
}

fn scanner() -> PluginScanner {
    PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory()).with_time_limit(Duration::from_secs(4))
}

fn misbehaving(how: &str) -> PluginScanner {
    scanner().with_environment_variable(MISBEHAVE, how)
}

fn scanned(outcome: LibraryOutcome) -> LibraryScan {
    match outcome {
        LibraryOutcome::Scanned(scan) => scan,
        LibraryOutcome::Failed(failure) => panic!("the library failed: {failure}"),
        LibraryOutcome::ScannedClap(scan) => panic!("the library was scanned as a CLAP one: {scan:?}"),
    }
}

fn failure(outcome: LibraryOutcome) -> ScanFailure {
    match outcome {
        LibraryOutcome::Failed(failure) => failure,
        LibraryOutcome::Scanned(scan) => panic!("the library did not fail: {scan:?}"),
        LibraryOutcome::ScannedClap(scan) => panic!("the library did not fail: {scan:?}"),
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
fn the_scan_of_a_good_library_finds_its_nine_plugins_and_every_one_was_made_and_run() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let report = scanner().scan_library(&library).expect("the scan could be run");
    assert_eq!(report.path, library);
    let scan = scanned(report.outcome);
    assert_eq!(scan.problems, Vec::<String>::new());
    let labels: Vec<&str> = scan
        .plugins
        .iter()
        .map(|plugin| plugin.description.label.as_str())
        .collect();
    assert_eq!(labels[0], "ffrs_test_gain");
    assert_eq!(labels.len(), 9);
    assert!(
        scan.plugins.iter().all(|plugin| plugin.problem.is_none()),
        "{:?}",
        scan.plugins
    );
    // The description came across the wire whole: the ports of the gain and the hint of its control.
    let gain = &scan.plugins[0].description;
    assert_eq!(gain.unique_id, 901);
    assert_eq!(gain.ports.len(), 3);
    assert_eq!(
        (gain.ports[0].hint.lower, gain.ports[0].hint.upper),
        (Some(0.0), Some(4.0))
    );
}

#[test]
fn a_library_that_crashes_when_it_is_asked_for_a_plugin_is_failed_and_says_where() {
    let ScanFailure::Crashed { how, during } = failure(scan_with("crash_when_described")) else {
        panic!("not a crash");
    };
    assert_eq!(during.as_deref(), Some("reading what the plugins say of themselves"));
    assert!(
        how.contains("access violation") || how.contains("segmentation fault") || how.contains("signal"),
        "{how}"
    );
}

#[test]
fn a_library_that_crashes_when_a_plugin_is_made_or_run_is_failed_and_says_which() {
    let ScanFailure::Crashed { during, .. } = failure(scan_with("crash_when_made")) else {
        panic!("not a crash");
    };
    assert_eq!(
        during.as_deref(),
        Some("making plugin number 0 (ffrs_test_gain) at 48000 Hz")
    );
    let ScanFailure::Crashed { during, .. } = failure(scan_with("crash_when_run")) else {
        panic!("not a crash");
    };
    assert_eq!(
        during.as_deref(),
        Some("running plugin number 0 (ffrs_test_gain) on 512 moments of silence")
    );
}

#[test]
fn a_library_that_never_returns_is_stopped_at_the_time_limit_and_the_scan_goes_on() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    for (how, doing) in [
        ("hang_when_described", "reading what the plugins say of themselves"),
        ("hang_when_made", "making plugin number 0 (ffrs_test_gain) at 48000 Hz"),
    ] {
        let mut scanner = misbehaving(how).with_time_limit(Duration::from_secs(1));
        let began = Instant::now();
        let report = scanner.scan_library(&library).expect("the scan could be run");
        let took = began.elapsed();
        assert_eq!(
            failure(report.outcome),
            ScanFailure::TookTooLong {
                seconds: 1,
                during: Some(doing.to_owned())
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
fn a_library_that_ends_the_program_without_an_answer_or_with_a_wrong_one_is_failed() {
    let ScanFailure::GaveNoAnswer { during } = failure(scan_with("exit_when_described")) else {
        panic!("not an exit with no answer");
    };
    assert_eq!(during.as_deref(), Some("reading what the plugins say of themselves"));
    assert!(matches!(
        failure(scan_with("answer_wrong_when_described")),
        ScanFailure::AnswerWasNotInTheFormat(_)
    ));
}

#[test]
fn a_library_that_prints_noise_is_scanned_all_the_same() {
    let scan = scanned(scan_with("print_noise"));
    assert_eq!(scan.plugins.len(), 9);
}

#[test]
fn a_library_with_no_plugins_is_scanned_and_has_none() {
    let scan = scanned(scan_with("no_plugins"));
    assert!(scan.plugins.is_empty() && scan.problems.is_empty());
}

#[test]
fn a_plugin_whose_descriptor_is_not_usable_is_left_out_and_said_and_the_rest_are_there() {
    let scan = scanned(scan_with("broken_descriptor"));
    assert_eq!(scan.plugins.len(), 9);
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(scan.problems[0].starts_with("plugin number 9:"), "{}", scan.problems[0]);
    assert!(
        scan.problems[0].contains("does not say what they are"),
        "{}",
        scan.problems[0]
    );
}

#[test]
fn a_plugin_that_says_it_has_a_million_ports_is_not_believed_and_the_scan_does_not_read_past_what_it_lists() {
    let scan = scanned(scan_with("absurd_port_count"));
    assert_eq!(scan.plugins.len(), 9);
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(scan.problems[0].starts_with("plugin number 9:"), "{}", scan.problems[0]);
    assert!(
        scan.problems[0].contains("1000000 ports") && scan.problems[0].contains("4096"),
        "{}",
        scan.problems[0]
    );
}

#[test]
fn a_library_that_never_runs_out_of_plugins_is_read_as_far_as_the_limit_and_says_so() {
    let scan = scanned(scan_with("endless_plugins"));
    assert_eq!(scan.plugins.len(), 1024);
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(
        scan.problems[0].contains("more than 1024 plugins"),
        "{}",
        scan.problems[0]
    );
    assert_eq!(scan.plugins[1023].description.index, 1023);
}

#[test]
fn a_plugin_that_gives_nothing_when_it_is_made_is_listed_with_the_reason() {
    let scan = scanned(scan_with("fail_to_make"));
    assert_eq!(scan.plugins.len(), 9);
    for plugin in &scan.plugins {
        let problem = plugin.problem.as_deref().expect("it could not be made");
        assert!(problem.contains("instantiate gave nothing"), "{problem}");
    }
}

#[test]
fn files_that_are_not_libraries_are_failed_with_what_the_system_said() {
    let folder = tempfile::tempdir().unwrap();
    let mut scanner = scanner();
    for file in [
        bad_libraries::with_nothing_in_it(folder.path()),
        bad_libraries::damaged(folder.path()),
        bad_libraries::needing_a_library_that_is_not_there(folder.path()),
    ] {
        let report = scanner.scan_library(&file).expect("the scan could be run");
        let ScanFailure::CouldNotBeLoaded(reason) = failure(report.outcome) else {
            panic!("{} was not refused by the loader", file.display());
        };
        assert!(!reason.is_empty(), "{}", file.display());
    }
}

#[test]
fn the_scan_program_says_that_a_library_of_the_system_that_is_not_a_ladspa_library_is_not_one() {
    let system_library = if cfg!(windows) {
        "kernel32.dll"
    } else if cfg!(target_os = "macos") {
        "/usr/lib/libSystem.B.dylib"
    } else {
        "libc.so.6"
    };
    // (The scan program is given a name that the loader finds, and that is not a file that the editor can look at: so go round the scanner.)
    let output = Command::new(SCANNER)
        .arg(system_library)
        .output()
        .expect("the scan program runs");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("NotALadspaLibrary"), "{said}");
    assert!(output.status.success());
}

#[test]
fn the_scan_program_says_how_it_is_used_when_it_is_used_wrongly() {
    for arguments in [Vec::<&str>::new(), vec!["one", "two"]] {
        let output = Command::new(SCANNER)
            .args(&arguments)
            .output()
            .expect("the scan program runs");
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("Usage: fastforwardrs-plugin-scan"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn a_scan_that_was_made_is_not_made_again_until_the_file_changes_not_even_by_another_run_of_the_editor() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let cache_file = folder.path().join("settings").join("scans.json");
    // A library that never returns, scanned once.
    let began = Instant::now();
    let mut first = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file.clone()))
        .with_time_limit(Duration::from_secs(1))
        .with_environment_variable(MISBEHAVE, "hang_when_described");
    let scan = first.scan_folders(&[folder.path().to_owned()]);
    assert_eq!(scan.problems, Vec::<String>::new());
    assert_eq!(scan.libraries.len(), 1);
    assert!(matches!(
        scan.libraries[0].outcome,
        LibraryOutcome::Failed(ScanFailure::TookTooLong { .. })
    ));
    assert!(!scan.libraries[0].from_cache);
    drop(first);
    // The next run of the editor (a scanner with the cache file and a scan program that does not exist, which proves that it was not run) knows.
    let mut second = PluginScanner::new(
        PathBuf::from("this-program-does-not-exist"),
        ScanCache::at(cache_file.clone()),
    );
    let scan = second.scan_folders(&[folder.path().to_owned()]);
    assert_eq!(scan.problems, Vec::<String>::new());
    assert!(scan.libraries[0].from_cache);
    assert!(matches!(
        scan.libraries[0].outcome,
        LibraryOutcome::Failed(ScanFailure::TookTooLong { .. })
    ));
    assert!(began.elapsed() < Duration::from_secs(10));
    // Until the file is another size: then it is scanned again, by the real program, and is good.
    fs::OpenOptions::new()
        .append(true)
        .open(&library)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"x"))
        .unwrap();
    let mut third = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file));
    let scan = third.scan_folders(&[folder.path().to_owned()]);
    assert!(!scan.libraries[0].from_cache);
    assert_eq!(
        scanned(scan.libraries[0].outcome.clone()).plugins.len(),
        9,
        "the same library with a byte added is still a library"
    );
}

#[test]
fn the_scan_of_a_folder_goes_through_every_library_and_a_bad_one_costs_the_others_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let ending = fastforwardrs::plugin_scan::library_file_extension();
    let good = good_library_in(folder.path());
    let empty = bad_libraries::with_nothing_in_it(folder.path());
    let damaged = bad_libraries::damaged(folder.path());
    fs::write(folder.path().join("readme.txt"), "not a library").unwrap();
    let missing_folder = folder.path().join("not_there");
    let mut scanner = scanner();
    let scan = scanner.scan_folders(&[folder.path().to_owned(), missing_folder]);
    assert_eq!(
        scan.problems,
        Vec::<String>::new(),
        "a folder that is not there is not a problem"
    );
    let mut by_name = vec![damaged, empty, good.clone()];
    by_name.sort_by_key(|path| path.file_name().unwrap().to_string_lossy().to_lowercase());
    let paths: Vec<PathBuf> = scan.libraries.iter().map(|report| report.path.clone()).collect();
    assert_eq!(paths, by_name, "in the order of the names ({ending})");
    for report in &scan.libraries {
        if report.path == good {
            assert_eq!(scanned(report.outcome.clone()).plugins.len(), 9);
        } else {
            assert!(
                matches!(report.outcome, LibraryOutcome::Failed(ScanFailure::CouldNotBeLoaded(_))),
                "{}",
                report.path.display()
            );
        }
    }
    // A second scan of the same folders runs nothing.
    let again = scanner.scan_folders(&[folder.path().to_owned()]);
    assert!(again.libraries.iter().all(|report| report.from_cache));
}

#[test]
fn a_library_that_is_gone_from_the_folders_is_forgotten_by_the_scanner() {
    let folder = tempfile::tempdir().unwrap();
    let kept = good_library_in(folder.path());
    let other = bad_libraries::damaged(folder.path());
    let mut scanner = scanner();
    scanner.scan_folders(&[folder.path().to_owned()]);
    assert_eq!(scanner.cache().len(), 2);
    fs::remove_file(&other).unwrap();
    let scan = scanner.scan_folders(&[folder.path().to_owned()]);
    assert_eq!(scan.libraries.len(), 1);
    assert_eq!(scanner.cache().len(), 1, "the one that is gone is not kept");
    assert!(
        scanner
            .cache()
            .lookup(&kept, &FileFingerprint::of(&kept).unwrap())
            .is_some()
    );
}

#[test]
fn a_scan_program_that_cannot_be_run_is_the_editors_problem_and_not_the_librarys() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let mut scanner = PluginScanner::new(PathBuf::from("this-program-does-not-exist"), ScanCache::in_memory());
    let error = scanner.scan_library(&library).expect_err("it could not be run");
    assert!(matches!(error, ScannerProblem::CouldNotRunTheScanner { .. }), "{error}");
    assert!(scanner.cache().is_empty(), "the library is not failed for it");
    let scan = scanner.scan_folders(&[folder.path().to_owned()]);
    assert!(scan.libraries.is_empty());
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(scan.problems[0].contains("could not be run"));
}

#[test]
fn a_library_that_is_gone_when_it_is_looked_at_is_a_problem_and_not_a_failure() {
    let folder = tempfile::tempdir().unwrap();
    let error = scanner()
        .scan_library(&folder.path().join("gone.dll"))
        .expect_err("it is not there");
    assert!(
        matches!(error, ScannerProblem::CouldNotLookAtTheLibrary { .. }),
        "{error}"
    );
}

#[test]
fn the_editor_finds_the_scan_program_beside_itself() {
    let found = scanner_program_beside_this_program().expect("the scan program is built for these tests");
    assert_eq!(fs::canonicalize(found).unwrap(), fs::canonicalize(SCANNER).unwrap());
}

#[test]
fn the_test_plugins_that_were_found_are_effects_with_the_values_their_ports_make_and_two_are_left_out_with_the_reason()
{
    let folder = tempfile::tempdir().unwrap();
    good_library_in(folder.path());
    let mut scanner = scanner();
    let scan = scanner.scan_folders(&[folder.path().to_owned()]);
    let found = fastforwardrs::plugin_scan::plugins_found_in(&scan.libraries);
    let keys: Vec<String> = found
        .effects
        .iter()
        .map(|effect| effect.descriptor.key.to_string())
        .collect();
    assert_eq!(
        keys,
        [
            "ladspa:good:ffrs_test_gain",
            "ladspa:good:ffrs_test_one_pole",
            "ladspa:good:ffrs_test_swap",
            "ladspa:good:ffrs_test_switches",
            "ladspa:good:ffrs_test_mono_to_both",
            "ladspa:good:ffrs_test_mix_down",
            "ladspa:good:ffrs_test_counted",
        ]
    );
    let left_out: Vec<(&str, bool)> = found
        .not_used
        .iter()
        .map(|left_out| (left_out.label.as_str(), left_out.reason.contains("no audio input")))
        .collect();
    assert_eq!(left_out, [("ffrs_test_no_ports", true), ("ffrs_test_sine", true)]);

    let gain = &found.effects[0];
    assert_eq!(gain.descriptor.category, "fastforwardrs tests");
    assert_eq!(gain.descriptor.parameters.len(), 1);
    assert_eq!(gain.descriptor.parameters[0].id, "port0_gain");
    assert_eq!(gain.descriptor.parameters[0].range, Some((0.0, 4.0)));
    assert_eq!(
        gain.descriptor.parameters[0].default,
        fastforwardrs::project::PropertyValue::Number(1.0)
    );
    let filter = &found.effects[1];
    let cutoff = &filter.descriptor.parameters[0];
    assert_eq!(cutoff.scale, fastforwardrs::project::ValueScale::Logarithmic);
    let (low, high) = cutoff.range.unwrap();
    assert!((low - 48.0).abs() < 1e-3 && (high - 21_600.0).abs() < 1e-2);
    let switches = &found.effects[3];
    let ids: Vec<&str> = switches
        .descriptor
        .parameters
        .iter()
        .map(|value| value.id.as_ref())
        .collect();
    assert_eq!(
        ids,
        ["port0_invert", "port1_multiplier"],
        "the meter is a control output: not a value"
    );
    assert_eq!(switches.value_handles, [0, 1]);
    assert_eq!((found.effects[4].audio_inputs, found.effects[4].audio_outputs), (1, 2));
    assert_eq!((found.effects[5].audio_inputs, found.effects[5].audio_outputs), (2, 1));
}

#[test]
fn what_the_cache_keeps_of_a_scan_is_what_the_scan_gave() {
    let folder = tempfile::tempdir().unwrap();
    let library = good_library_in(folder.path());
    let mut scanner = scanner();
    let report = scanner.scan_library(&library).unwrap();
    let kept = scanner
        .cache()
        .lookup(&library, &FileFingerprint::of(&library).unwrap());
    assert_eq!(kept, Some(&CachedOutcome::Scanned(scanned(report.outcome))));
}

/// Scans the plugins that are really installed on a computer, and says what came of each: not a check (what is installed is not ours), a look at how the scan fares with
/// plugins that other people wrote. Run it with `--ignored --nocapture`, with `FASTFORWARDRS_LADSPA_FOLDER` set to a folder of LADSPA libraries (the folder `plugins\ladspa` of an
/// installation of LMMS, say). The scan program is run on every library in it; nothing is changed.
#[test]
#[ignore = "looks at plugins that are installed on this computer; run with --ignored --nocapture and FASTFORWARDRS_LADSPA_FOLDER set"]
fn a_look_at_the_plugins_that_are_installed_in_a_folder() {
    let folder = PathBuf::from(
        std::env::var_os("FASTFORWARDRS_LADSPA_FOLDER")
            .expect("set FASTFORWARDRS_LADSPA_FOLDER to a folder of LADSPA libraries"),
    );
    // (`FASTFORWARDRS_DEPENDENCY_FOLDER` is a folder with the libraries the plugin libraries need, and `FASTFORWARDRS_SEARCH_PARENTS=1` looks in the folders above them too.)
    let dependency_folders: Vec<PathBuf> = std::env::var_os("FASTFORWARDRS_DEPENDENCY_FOLDER")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let search_parents = std::env::var("FASTFORWARDRS_SEARCH_PARENTS").is_ok_and(|value| value == "1");
    let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory())
        .looking_for_dependencies_in(dependency_folders, search_parents);
    let began = Instant::now();
    let scan = scanner.scan_folders(&[folder]);
    let (mut good, mut plugins, mut plugins_with_problems) = (0, 0, 0);
    for report in &scan.libraries {
        let name = report.path.file_name().unwrap().to_string_lossy();
        match &report.outcome {
            LibraryOutcome::Scanned(library) => {
                good += 1;
                plugins += library.plugins.len();
                let broken: Vec<String> = library
                    .plugins
                    .iter()
                    .filter_map(|plugin| {
                        plugin
                            .problem
                            .as_ref()
                            .map(|problem| format!("{}: {problem}", plugin.description.label))
                    })
                    .collect();
                plugins_with_problems += broken.len();
                println!(
                    "{name}: {} plugins{}",
                    library.plugins.len(),
                    if broken.is_empty() {
                        String::new()
                    } else {
                        format!(", with problems: {}", broken.join("; "))
                    }
                );
                for problem in &library.problems {
                    println!("    {problem}");
                }
            }
            LibraryOutcome::ScannedClap(library) => {
                println!("{name}: a CLAP library, {} plugins", library.plugins.len())
            }
            LibraryOutcome::Failed(failure) => println!("{name}: FAILED: {failure}"),
        }
    }
    println!(
        "{} libraries: {good} scanned, {} failed; {plugins} plugins, {plugins_with_problems} of them could not be made or run; {:?} in all",
        scan.libraries.len(),
        scan.libraries.len() - good,
        began.elapsed()
    );
    for problem in &scan.problems {
        println!("PROBLEM: {problem}");
    }
}

#[test]
fn the_scan_program_says_which_code_it_was_built_from_and_it_is_the_code_of_this_editor() {
    let output = Command::new(SCANNER)
        .arg(fastforwardrs::plugin_scan::protocol::BUILD_ID_REQUEST)
        .output()
        .expect("the scan program is run");

    assert!(output.status.success());
    let said = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        fastforwardrs::plugin_scan::protocol::build_id_in(&said).as_deref(),
        Some(fastforwardrs::plugin_scan::protocol::BUILD_ID),
        "{said}"
    );
}

#[test]
fn a_scan_program_that_was_not_built_from_the_code_of_the_editor_is_refused_once_and_the_scans_that_were_kept_are_kept()
{
    let folder = tempfile::tempdir().unwrap();
    let known = good_library_in(folder.path());
    let cache_file = folder.path().join("settings").join("scans.json");
    // The library `known` was scanned by a program that is the editor's.
    let mut first = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::at(cache_file.clone()))
        .with_time_limit(Duration::from_secs(4));
    let primed = first.scan_folders(&[folder.path().to_owned()]);
    assert_eq!(primed.libraries.len(), 1);
    drop(first);
    let another = folder.path().join(format!(
        "another.{}",
        fastforwardrs::plugin_scan::library_file_extension()
    ));
    fs::copy(library_file_for_tests(), &another).unwrap();
    let a_third = folder.path().join(format!(
        "third.{}",
        fastforwardrs::plugin_scan::library_file_extension()
    ));
    fs::copy(library_file_for_tests(), &a_third).unwrap();
    let kept = ScanCache::at(cache_file.clone()).len();
    assert_eq!(kept, 1);

    // A program that is not the editor's scan program (it does not say which code it was built from, as a scan program of before it could does not).
    let mut stale = PluginScanner::new(
        PathBuf::from(env!("CARGO_BIN_EXE_fastforwardrs-export")),
        ScanCache::at(cache_file.clone()),
    );
    let scan = stale.scan_folders(&[folder.path().to_owned()]);

    assert_eq!(
        scan.problems.len(),
        1,
        "said once, though two libraries were to be scanned: {:?}",
        scan.problems
    );
    assert!(
        scan.problems[0].contains("not built from the same code as this editor")
            && scan.problems[0].contains("cargo build --release --bins"),
        "{:?}",
        scan.problems
    );
    assert!(
        stale
            .scan_library(&another)
            .is_err_and(|problem| matches!(problem, ScannerProblem::TheScanProgramIsNotThisEditors(_))),
        "the library that was not scanned is not scanned"
    );
    assert_eq!(scan.libraries.len(), 1, "only what was kept is reported");
    assert!(
        scan.libraries
            .iter()
            .all(|report| report.from_cache && report.path == known),
        "nothing was scanned"
    );
    assert_eq!(
        ScanCache::at(cache_file).len(),
        kept,
        "and nothing that was kept is forgotten"
    );
}
