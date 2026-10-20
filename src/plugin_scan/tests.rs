// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The parts of the scan that do not need a library: the words between the scan program and the editor, the reading of how the program ended, the folders, the cache and the
//! breadcrumb. (The scan program itself, and libraries that crash it, are in `tests/plugin_scan.rs`.)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use ladspa_host::{
    DefaultHint, LibraryScan, PluginDescription, PluginProperties, PortDescription, PortDirection, PortHint, PortKind,
    ScannedPlugin,
};

use super::breadcrumb::recover;
use super::child::{HowItEnded, WhatItDid, outcome_of, read_until_the_end, wait_until_finished};
use super::protocol::{RESULT_PREFIX, STEP_PREFIX, ScanAnswer, answer_line, read_what_was_said, step_line};
use super::*;

fn a_scan() -> LibraryScan {
    LibraryScan {
        plugins: vec![ScannedPlugin {
            description: PluginDescription {
                index: 0,
                unique_id: 901,
                label: "gain".to_owned(),
                name: "Gain".to_owned(),
                maker: "Somebody".to_owned(),
                copyright: "None".to_owned(),
                properties: PluginProperties::default(),
                ports: vec![PortDescription {
                    name: "Gain".to_owned(),
                    direction: PortDirection::Input,
                    kind: PortKind::Control,
                    hint: PortHint {
                        lower: Some(0.0),
                        upper: Some(4.0),
                        toggled: false,
                        relative_to_sample_rate: false,
                        logarithmic: false,
                        integer: false,
                        default: DefaultHint::One,
                    },
                }],
            },
            problem: None,
        }],
        problems: vec!["plugin number 1: it has no ports".to_owned()],
    }
}

// --- the words between the programs ---

#[test]
fn a_step_is_one_line_and_an_answer_is_read_back_as_it_was_written() {
    assert_eq!(
        step_line("making plugin\r\nnumber 3"),
        format!("{STEP_PREFIX}making plugin  number 3")
    );
    let answer = ScanAnswer::Scanned(a_scan());
    let line = answer_line(&answer);
    assert!(line.starts_with(RESULT_PREFIX));
    assert!(!line.contains('\n'));
    let said = read_what_was_said(&line);
    assert_eq!(said.answer, Some(Ok(answer)));
    assert_eq!(said.last_step, None);
}

#[test]
fn only_the_lines_that_begin_with_the_words_count_and_the_last_step_and_the_last_answer_win() {
    let answer = ScanAnswer::CouldNotBeLoaded("no".to_owned());
    let output = format!(
        "noise from a library\n{}\nmore noise: {RESULT_PREFIX} is not at the start\n\n{}\r\n{}\nnoise after\n",
        step_line("loading the library"),
        step_line("making plugin number 2"),
        answer_line(&answer)
    );
    let said = read_what_was_said(&output);
    assert_eq!(said.last_step.as_deref(), Some("making plugin number 2"));
    assert_eq!(said.answer, Some(Ok(answer)));
    let nothing = read_what_was_said("hello\nworld\n");
    assert_eq!((nothing.last_step, nothing.answer), (None, None));
}

#[test]
fn an_answer_that_is_not_json_is_said_so_and_does_not_hide_the_steps() {
    let said = read_what_was_said(&format!(
        "{}\n{RESULT_PREFIX}{{not json\n",
        step_line("running plugin number 1")
    ));
    assert_eq!(said.last_step.as_deref(), Some("running plugin number 1"));
    assert!(matches!(said.answer, Some(Err(_))));
    let wrong_shape = read_what_was_said(&format!("{RESULT_PREFIX}{{\"Something\":1}}\n"));
    assert!(matches!(wrong_shape.answer, Some(Err(_))));
}

// --- how the program ended ---

#[cfg(windows)]
fn status(code: u32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(code)
}

/// (A signal on Unix is the low bits of the raw status; an exit code is shifted up by 8.)
#[cfg(unix)]
fn status(code: u32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(if code & 0x8000_0000 != 0 {
        (code & 0x7F) as i32
    } else {
        (code as i32) << 8
    })
}

/// An exit that is a success, and one that is a crash with the number a segmentation fault has on this system.
fn ended_well() -> ExitStatus {
    status(0)
}

fn crashed() -> ExitStatus {
    if cfg!(windows) {
        status(0xC000_0005)
    } else {
        status(0x8000_0000 | 11)
    }
}

fn said_by_a_scan(steps: &[&str], answer: Option<&ScanAnswer>) -> String {
    let mut said = String::from("noise\n");
    for step in steps {
        said.push_str(&format!("{}\n", step_line(step)));
    }
    if let Some(answer) = answer {
        said.push_str(&format!("{}\n", answer_line(answer)));
    }
    said
}

fn outcome(how_it_ended: HowItEnded, said: String) -> LibraryOutcome {
    outcome_of(&WhatItDid { how_it_ended, said }, Duration::from_secs(5))
}

#[test]
fn a_program_that_answered_gives_the_scan_or_the_reason_the_library_was_refused() {
    let scanned = ScanAnswer::Scanned(a_scan());
    assert_eq!(
        outcome(HowItEnded::Exited(ended_well()), said_by_a_scan(&["a"], Some(&scanned))),
        LibraryOutcome::Scanned(a_scan())
    );
    let refused = ScanAnswer::CouldNotBeLoaded("the specified module could not be found".to_owned());
    assert_eq!(
        outcome(HowItEnded::Exited(ended_well()), said_by_a_scan(&["a"], Some(&refused))),
        LibraryOutcome::Failed(ScanFailure::CouldNotBeLoaded(
            "the specified module could not be found".to_owned()
        ))
    );
    let not_ladspa = ScanAnswer::NotALadspaLibrary("no symbol".to_owned());
    assert_eq!(
        outcome(HowItEnded::Exited(ended_well()), said_by_a_scan(&[], Some(&not_ladspa))),
        LibraryOutcome::Failed(ScanFailure::NotALadspaLibrary("no symbol".to_owned()))
    );
}

#[test]
fn an_answer_that_was_given_counts_even_if_the_program_crashed_afterwards() {
    let scanned = ScanAnswer::Scanned(a_scan());
    assert_eq!(
        outcome(HowItEnded::Exited(crashed()), said_by_a_scan(&["a"], Some(&scanned))),
        LibraryOutcome::Scanned(a_scan())
    );
}

#[test]
fn a_program_that_crashed_is_a_failure_that_says_how_and_what_it_was_doing() {
    let outcome = outcome(
        HowItEnded::Exited(crashed()),
        said_by_a_scan(&["loading the library", "making plugin number 4 (x)"], None),
    );
    let LibraryOutcome::Failed(ScanFailure::Crashed { how, during }) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(during.as_deref(), Some("making plugin number 4 (x)"));
    if cfg!(windows) {
        assert_eq!(how, "0xC0000005: access violation");
    } else {
        assert_eq!(how, "signal 11 (segmentation fault)");
    }
}

#[test]
fn the_exits_of_a_crash_are_told_apart() {
    let how = |code: u32| {
        let LibraryOutcome::Failed(ScanFailure::Crashed { how, .. }) =
            outcome(HowItEnded::Exited(status(code)), String::new())
        else {
            panic!("not a crash");
        };
        how
    };
    if cfg!(windows) {
        assert_eq!(how(0xC000_00FD), "0xC00000FD: stack overflow");
        assert_eq!(
            how(0xC000_0409),
            "0xC0000409: the program stopped itself after finding memory damaged"
        );
        assert_eq!(how(0xC000_001D), "0xC000001D: illegal instruction");
        assert_eq!(how(0xC000_0094), "0xC0000094: division by zero");
        assert_eq!(how(0xC123_4567), "exit code 0xC1234567");
        assert_eq!(how(3), "exit code 3");
    } else {
        assert_eq!(how(0x8000_0000 | 4), "signal 4 (illegal instruction)");
        assert_eq!(how(0x8000_0000 | 6), "signal 6 (abort)");
        assert_eq!(how(0x8000_0000 | 7), "signal 7 (bus error)");
        assert_eq!(how(0x8000_0000 | 8), "signal 8 (arithmetic error)");
        assert_eq!(how(0x8000_0000 | 9), "signal 9 (killed)");
        assert_eq!(how(0x8000_0000 | 15), "signal 15 (terminated)");
        assert_eq!(how(0x8000_0000 | 12), "signal 12");
        assert_eq!(how(3), "exit code 3");
    }
}

#[test]
fn a_program_that_ended_well_and_said_nothing_and_one_that_said_nonsense_are_failures() {
    assert_eq!(
        outcome(
            HowItEnded::Exited(ended_well()),
            said_by_a_scan(&["loading the library"], None)
        ),
        LibraryOutcome::Failed(ScanFailure::GaveNoAnswer {
            during: Some("loading the library".to_owned())
        })
    );
    let nonsense = format!("{RESULT_PREFIX}this is not json\n");
    let LibraryOutcome::Failed(ScanFailure::AnswerWasNotInTheFormat(reason)) =
        outcome(HowItEnded::Exited(ended_well()), nonsense)
    else {
        panic!("not a failure of the format");
    };
    assert!(!reason.is_empty());
}

#[test]
fn a_program_that_was_stopped_is_a_failure_that_says_how_long_it_was_given_and_what_it_was_doing() {
    let outcome = outcome(
        HowItEnded::StoppedAfterTheTimeLimit,
        said_by_a_scan(
            &[
                "loading the library",
                "running plugin number 2 (reverb) on 512 moments of silence",
            ],
            None,
        ),
    );
    assert_eq!(
        outcome,
        LibraryOutcome::Failed(ScanFailure::TookTooLong {
            seconds: 5,
            during: Some("running plugin number 2 (reverb) on 512 moments of silence".to_owned())
        })
    );
    // (Even an answer that is there does not save a program that did not end.)
    let late = ScanAnswer::Scanned(a_scan());
    assert!(matches!(
        self::outcome(HowItEnded::StoppedAfterTheTimeLimit, said_by_a_scan(&[], Some(&late))),
        LibraryOutcome::Failed(ScanFailure::TookTooLong { .. })
    ));
}

#[test]
fn each_failure_is_said_in_words_that_name_what_matters() {
    let said = |failure: ScanFailure| failure.to_string();
    assert!(said(ScanFailure::CouldNotBeLoaded("module missing".to_owned())).contains("module missing"));
    assert!(said(ScanFailure::NotALadspaLibrary("no symbol".to_owned())).contains("not a LADSPA library"));
    let long = said(ScanFailure::TookTooLong {
        seconds: 5,
        during: Some("making plugin number 1".to_owned()),
    });
    assert!(
        long.contains("5 seconds") && long.contains("while it was making plugin number 1"),
        "{long}"
    );
    assert!(
        !said(ScanFailure::TookTooLong {
            seconds: 5,
            during: None
        })
        .contains("while")
    );
    let crash = said(ScanFailure::Crashed {
        how: "signal 11".to_owned(),
        during: Some("running".to_owned()),
    });
    assert!(
        crash.contains("crashed") && crash.contains("signal 11") && crash.contains("while it was running"),
        "{crash}"
    );
    assert!(said(ScanFailure::GaveNoAnswer { during: None }).contains("said nothing"));
    assert!(said(ScanFailure::AnswerWasNotInTheFormat("eof".to_owned())).contains("eof"));
    assert!(said(ScanFailure::WasInUseWhenTheProgramEndedBadly { others: 0 }).ends_with("using this library"));
    assert!(said(ScanFailure::WasInUseWhenTheProgramEndedBadly { others: 1 }).contains("1 other library"));
    assert!(said(ScanFailure::WasInUseWhenTheProgramEndedBadly { others: 2 }).contains("2 other libraries"));
}

// --- reading what the program said ---

/// A pipe that gives a few bytes at a time, as a real one does.
struct SlowPipe<'a> {
    bytes: &'a [u8],
    at: usize,
    most_at_a_time: usize,
}

impl std::io::Read for SlowPipe<'_> {
    fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
        let count = self.most_at_a_time.min(into.len()).min(self.bytes.len() - self.at);
        into[..count].copy_from_slice(&self.bytes[self.at..self.at + count]);
        self.at += count;
        Ok(count)
    }
}

#[test]
fn what_the_program_says_is_kept_up_to_the_limit_and_read_to_the_end_all_the_same() {
    let bytes: Vec<u8> = (0..=99).collect();
    for most_at_a_time in [1, 7, 100] {
        for most in [0, 1, 3, 99, 100, 101] {
            let mut pipe = SlowPipe {
                bytes: &bytes,
                at: 0,
                most_at_a_time,
            };
            let kept = std::sync::Mutex::new(Vec::new());
            read_until_the_end(&mut pipe, &kept, most);
            assert_eq!(
                pipe.at, 100,
                "all of it is read, so that the program is never stopped by a full pipe"
            );
            assert_eq!(
                *kept.lock().unwrap(),
                bytes[..most.min(100)],
                "the first {most} bytes are kept ({most_at_a_time} at a time)"
            );
        }
    }
}

#[test]
fn reading_ends_when_the_pipe_fails() {
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("the pipe broke"))
        }
    }
    let kept = std::sync::Mutex::new(Vec::new());
    read_until_the_end(Broken, &kept, 100);
    assert!(kept.lock().unwrap().is_empty());
}

#[test]
fn a_thread_is_waited_for_until_it_finishes_but_not_for_longer_than_it_is_given() {
    let quick = std::thread::spawn(|| ());
    assert!(wait_until_finished(&quick, Duration::from_secs(10)));
    let slow = std::thread::spawn(|| std::thread::sleep(Duration::from_secs(3)));
    let began = std::time::Instant::now();
    assert!(!wait_until_finished(&slow, Duration::from_millis(100)));
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "it waited {:?}",
        began.elapsed()
    );
    assert!(began.elapsed() >= Duration::from_millis(100));
    let _ = slow.join();
}

#[test]
fn a_scan_is_given_five_seconds_unless_the_scanner_is_told_otherwise() {
    assert_eq!(SCAN_TIME_LIMIT, Duration::from_secs(5));
    let scanner = PluginScanner::new(PathBuf::from("not-a-program"), ScanCache::in_memory());
    assert_eq!(scanner.time_limit(), Duration::from_secs(5));
    assert_eq!(
        scanner.with_time_limit(Duration::from_millis(250)).time_limit(),
        Duration::from_millis(250)
    );
}

// --- what a person said of where to look ---

#[test]
fn where_to_look_is_written_and_read_back_and_a_file_that_is_not_there_is_the_usual_places_with_no_complaint() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("settings").join("plugin_search.json");
    assert_eq!(
        PluginSearchSettings::read(&file),
        (PluginSearchSettings::default(), None)
    );
    let settings = PluginSearchSettings {
        folders: vec![PathBuf::from("/mine/plugins"), PathBuf::from("/other")],
        dependency_folders: vec![PathBuf::from("/mine/libraries")],
        search_parent_folders: false,
    };
    settings.write(&file).unwrap();
    assert_eq!(PluginSearchSettings::read(&file), (settings.clone(), None));
    // (Written again, it is the same text.)
    let text = fs::read_to_string(&file).unwrap();
    settings.write(&file).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn looking_in_the_folders_above_a_plugin_library_is_on_unless_it_was_turned_off_even_in_a_file_that_does_not_say() {
    assert!(PluginSearchSettings::default().search_parent_folders);
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_search.json");
    fs::write(&file, "{\"folders\": [\"/a\"]}").unwrap();
    let (settings, problem) = PluginSearchSettings::read(&file);
    assert_eq!(problem, None);
    assert!(
        settings.search_parent_folders,
        "a file with no word about it leaves it on"
    );
    assert_eq!(settings.folders, [PathBuf::from("/a")]);
    assert!(settings.dependency_folders.is_empty());
    fs::write(&file, "{\"search_parent_folders\": false}").unwrap();
    assert!(!PluginSearchSettings::read(&file).0.search_parent_folders);
}

#[test]
fn a_file_of_places_that_cannot_be_read_gives_the_usual_places_and_says_what_is_wrong() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("plugin_search.json");
    fs::write(&file, "this is not json").unwrap();
    let (settings, problem) = PluginSearchSettings::read(&file);
    assert_eq!(settings, PluginSearchSettings::default());
    assert!(problem.unwrap().contains("cannot be read"));
    // A folder where the file should be cannot be read as one either.
    let (settings, problem) = PluginSearchSettings::read(folder.path());
    assert_eq!(settings, PluginSearchSettings::default());
    assert!(problem.is_some());
}

// --- the folders ---

fn touch(folder: &Path, name: &str) -> PathBuf {
    let path = folder.join(name);
    fs::write(&path, b"x").unwrap();
    path
}

#[test]
fn the_folders_are_the_ones_a_person_added_then_ladspa_path_then_the_usual_without_repeats() {
    let added = vec![PathBuf::from("/mine"), PathBuf::from("/shared")];
    let variable = std::env::join_paths(["/shared", "/from/variable", ""]).unwrap();
    let usual = vec![PathBuf::from("/usr/lib/ladspa"), PathBuf::from("/from/variable")];
    let folders = folders_to_scan(&added, Some(&variable), &usual);
    assert_eq!(
        folders,
        [
            PathBuf::from("/mine"),
            PathBuf::from("/shared"),
            PathBuf::from("/from/variable"),
            PathBuf::from("/usr/lib/ladspa")
        ]
    );
    assert_eq!(folders_to_scan(&[], None, &[]), Vec::<PathBuf>::new());
    assert_eq!(
        folders_to_scan(&[PathBuf::new()], None, &[]),
        Vec::<PathBuf>::new(),
        "an empty path is no folder"
    );
}

#[test]
fn the_usual_folders_are_those_of_ladspa_h_and_the_home_folder_and_windows_has_none() {
    let usual = usual_folders(Some(Path::new("/home/someone")));
    if cfg!(windows) {
        assert!(usual.is_empty());
    } else {
        assert_eq!(
            usual,
            [
                PathBuf::from("/usr/lib/ladspa"),
                PathBuf::from("/usr/local/lib/ladspa"),
                PathBuf::from("/home/someone/.ladspa")
            ]
        );
        assert_eq!(usual_folders(None).len(), 2);
    }
}

#[test]
fn the_libraries_in_a_folder_are_the_files_with_the_ending_of_this_system_in_the_order_of_their_names() {
    let folder = tempfile::tempdir().unwrap();
    let ending = library_file_extension();
    let b = touch(folder.path(), &format!("b.{ending}"));
    let a = touch(folder.path(), &format!("a.{ending}"));
    let upper = touch(folder.path(), &format!("C.{}", ending.to_uppercase()));
    touch(folder.path(), "notes.txt");
    touch(folder.path(), "no_ending");
    // A folder with the ending of a library is not a library, and what is inside a folder is not looked at.
    let inside = folder.path().join(format!("folder.{ending}"));
    fs::create_dir(&inside).unwrap();
    touch(&inside, &format!("inside.{ending}"));
    assert_eq!(libraries_in(folder.path()).unwrap(), [a, b, upper]);
    assert_eq!(
        libraries_in(&folder.path().join("not_there")).unwrap(),
        Vec::<PathBuf>::new()
    );
}

// --- the cache ---

fn a_library_file(folder: &Path, bytes: &[u8]) -> PathBuf {
    let path = folder.join(format!("library.{}", library_file_extension()));
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_scan_is_believed_while_the_file_is_the_same_size_and_time_and_not_after() {
    let folder = tempfile::tempdir().unwrap();
    let library = a_library_file(folder.path(), b"first");
    let mut cache = ScanCache::in_memory();
    let before = FileFingerprint::of(&library).unwrap();
    cache.record(&library, before, CachedOutcome::Scanned(a_scan()));
    assert_eq!(
        cache.lookup(&library, &FileFingerprint::of(&library).unwrap()),
        Some(&CachedOutcome::Scanned(a_scan()))
    );
    // Another size.
    fs::write(&library, b"a longer file").unwrap();
    assert_eq!(cache.lookup(&library, &FileFingerprint::of(&library).unwrap()), None);
    // The same size and another time.
    let same_size_later = FileFingerprint {
        modified_seconds: before.modified_seconds + 1,
        ..before
    };
    assert_eq!(cache.lookup(&library, &same_size_later), None);
    let same_seconds_later_nanoseconds = FileFingerprint {
        modified_nanoseconds: before.modified_nanoseconds.wrapping_add(1),
        ..before
    };
    assert_eq!(cache.lookup(&library, &same_seconds_later_nanoseconds), None);
    assert_eq!(cache.lookup(&folder.path().join("other"), &before), None);
}

#[test]
fn the_cache_is_written_and_read_back_failures_and_all() {
    let folder = tempfile::tempdir().unwrap();
    let library = a_library_file(folder.path(), b"x");
    let other = touch(folder.path(), &format!("other.{}", library_file_extension()));
    let file = folder.path().join("settings").join("scan_cache.json");
    let mut cache = ScanCache::at(file.clone());
    let (fingerprint, other_fingerprint) = (
        FileFingerprint::of(&library).unwrap(),
        FileFingerprint::of(&other).unwrap(),
    );
    cache.record(&library, fingerprint, CachedOutcome::Scanned(a_scan()));
    cache.record(
        &other,
        other_fingerprint,
        CachedOutcome::Failed(ScanFailure::Crashed {
            how: "signal 11".to_owned(),
            during: Some("making plugin number 0".to_owned()),
        }),
    );
    assert_eq!(cache.len(), 2);
    cache.save();
    assert_eq!(cache.last_problem(), None);
    let again = ScanCache::at(file.clone());
    assert_eq!(again.len(), 2);
    assert_eq!(
        again.lookup(&library, &fingerprint),
        Some(&CachedOutcome::Scanned(a_scan()))
    );
    assert!(matches!(
        again.lookup(&other, &other_fingerprint),
        Some(CachedOutcome::Failed(ScanFailure::Crashed { .. }))
    ));
    // Writing the same cache again gives the same text.
    let text = fs::read_to_string(&file).unwrap();
    let mut rewritten = ScanCache::at(file.clone());
    rewritten.save();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn a_cache_file_that_cannot_be_read_is_an_empty_cache_that_says_so_and_one_of_another_format_is_not_believed() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("scan_cache.json");
    fs::write(&file, "this is not json").unwrap();
    let cache = ScanCache::at(file.clone());
    assert!(cache.is_empty());
    assert!(
        cache.last_problem().unwrap().contains("scanned again"),
        "{:?}",
        cache.last_problem()
    );
    // A file that is not there is not a problem either.
    let missing = ScanCache::at(folder.path().join("never_written.json"));
    assert!(missing.is_empty());
    assert_eq!(missing.last_problem(), None);
    // Another format: not an error, only not believed.
    fs::write(&file, "{\"format\": 0, \"libraries\": []}").unwrap();
    let cache = ScanCache::at(file.clone());
    assert!(cache.is_empty());
    assert_eq!(cache.last_problem(), None);
    let library = a_library_file(folder.path(), b"x");
    let mut writer = ScanCache::at(file.clone());
    writer.record(
        &library,
        FileFingerprint::of(&library).unwrap(),
        CachedOutcome::Scanned(a_scan()),
    );
    writer.save();
    let text = fs::read_to_string(&file).unwrap().replace(
        &format!("\"format\": {}", super::protocol::SCAN_FORMAT),
        &format!("\"format\": {}", super::protocol::SCAN_FORMAT + 1),
    );
    fs::write(&file, text).unwrap();
    assert!(
        ScanCache::at(file).is_empty(),
        "the answers of another version of the scan are not believed"
    );
}

#[test]
fn a_cache_that_cannot_be_written_says_so_and_keeps_what_it_has() {
    let folder = tempfile::tempdir().unwrap();
    // The place for the file is below a file, so no folder can be made for it.
    let blocker = touch(folder.path(), "blocker");
    let mut cache = ScanCache::at(blocker.join("inside").join("scan_cache.json"));
    let library = a_library_file(folder.path(), b"x");
    cache.record(
        &library,
        FileFingerprint::of(&library).unwrap(),
        CachedOutcome::Scanned(a_scan()),
    );
    cache.save();
    assert!(
        cache.last_problem().unwrap().contains("cannot be written"),
        "{:?}",
        cache.last_problem()
    );
    assert_eq!(cache.len(), 1);
}

#[test]
fn a_cache_forgets_the_libraries_that_are_not_found_any_more() {
    let folder = tempfile::tempdir().unwrap();
    let kept = a_library_file(folder.path(), b"x");
    let gone = touch(folder.path(), &format!("gone.{}", library_file_extension()));
    let mut cache = ScanCache::in_memory();
    for library in [&kept, &gone] {
        cache.record(
            library,
            FileFingerprint::of(library).unwrap(),
            CachedOutcome::Scanned(a_scan()),
        );
    }
    cache.forget_all_but(std::slice::from_ref(&kept));
    assert_eq!(cache.len(), 1);
    assert!(cache.lookup(&kept, &FileFingerprint::of(&kept).unwrap()).is_some());
}

// --- the breadcrumb ---

fn a_scanner() -> PluginScanner {
    // (No library is scanned here, so the program is never run.)
    PluginScanner::new(PathBuf::from("not-a-program"), ScanCache::in_memory())
}

#[test]
fn a_library_is_written_in_the_breadcrumb_before_it_is_used_and_once() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("breadcrumb").join("in_use.json");
    let mut breadcrumb = Breadcrumb::at(file.clone());
    assert!(!file.exists());
    breadcrumb.note_use_of(Path::new("/plugins/one.so")).unwrap();
    let after_one = fs::read_to_string(&file).unwrap();
    assert!(after_one.contains("/plugins/one.so"), "{after_one}");
    breadcrumb.note_use_of(Path::new("/plugins/two.so")).unwrap();
    breadcrumb.note_use_of(Path::new("/plugins/one.so")).unwrap();
    assert_eq!(
        breadcrumb.used(),
        [PathBuf::from("/plugins/one.so"), PathBuf::from("/plugins/two.so")]
    );
    let after_two = fs::read_to_string(&file).unwrap();
    assert!(
        after_two.find("one.so").unwrap() < after_two.find("two.so").unwrap(),
        "in the order of first use: {after_two}"
    );
    assert_eq!(after_two.matches("one.so").count(), 1);
    breadcrumb.ended_well();
    assert!(!file.exists(), "a run that ended well leaves nothing");
    assert!(breadcrumb.used().is_empty());
}

#[test]
fn a_breadcrumb_that_cannot_be_written_is_an_error_for_the_caller_to_decide_about() {
    let folder = tempfile::tempdir().unwrap();
    let blocker = touch(folder.path(), "blocker");
    let mut breadcrumb = Breadcrumb::at(blocker.join("inside").join("in_use.json"));
    assert!(breadcrumb.note_use_of(Path::new("/plugins/one.so")).is_err());
    assert!(
        breadcrumb.used().is_empty(),
        "a library that was not written down is not said to have been"
    );
}

#[test]
fn a_run_that_ended_badly_fails_the_library_it_started_last_and_names_the_others() {
    let folder = tempfile::tempdir().unwrap();
    let ending = library_file_extension();
    let libraries: Vec<PathBuf> = ["one", "two", "three"]
        .iter()
        .map(|name| touch(folder.path(), &format!("{name}.{ending}")))
        .collect();
    let file = folder.path().join("in_use.json");
    let mut breadcrumb = Breadcrumb::at(file.clone());
    for library in &libraries {
        breadcrumb.note_use_of(library).unwrap();
    }
    // (The run ended: nobody called `ended_well`.)
    drop(breadcrumb);
    let mut scanner = a_scanner();
    let recovery = recover(&file, &mut scanner).expect("the last run left a breadcrumb");
    assert_eq!(recovery.failed, libraries[2]);
    assert_eq!(recovery.also_in_use, [libraries[0].clone(), libraries[1].clone()]);
    assert!(!file.exists(), "the breadcrumb is taken");
    let fingerprint = FileFingerprint::of(&libraries[2]).unwrap();
    assert_eq!(
        scanner.cache().lookup(&libraries[2], &fingerprint),
        Some(&CachedOutcome::Failed(ScanFailure::WasInUseWhenTheProgramEndedBadly {
            others: 2
        }))
    );
    assert_eq!(scanner.cache().len(), 1, "only that one is failed");
    // And it is not found again.
    assert_eq!(recover(&file, &mut scanner), None);
}

#[test]
fn a_breadcrumb_with_one_library_names_no_others_and_one_with_nothing_in_it_or_that_is_nonsense_is_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let library = touch(folder.path(), &format!("one.{}", library_file_extension()));
    let file = folder.path().join("in_use.json");
    let mut breadcrumb = Breadcrumb::at(file.clone());
    breadcrumb.note_use_of(&library).unwrap();
    let recovery = recover(&file, &mut a_scanner()).unwrap();
    assert_eq!((recovery.failed, recovery.also_in_use), (library, Vec::new()));
    fs::write(&file, "{\"libraries\": []}").unwrap();
    assert_eq!(recover(&file, &mut a_scanner()), None);
    assert!(!file.exists());
    fs::write(&file, "this is not json").unwrap();
    assert_eq!(recover(&file, &mut a_scanner()), None);
    assert!(
        !file.exists(),
        "a file that is only there to help is not kept if it cannot"
    );
}

#[test]
fn a_library_that_was_failed_is_written_to_the_cache_file_at_once() {
    let folder = tempfile::tempdir().unwrap();
    let library = a_library_file(folder.path(), b"x");
    let file = folder.path().join("scans.json");
    let mut scanner = PluginScanner::new(PathBuf::from("not-a-program"), ScanCache::at(file.clone()));
    scanner.fail_library(&library, ScanFailure::GaveNoAnswer { during: None });
    let read_back = ScanCache::at(file);
    assert_eq!(
        read_back.lookup(&library, &FileFingerprint::of(&library).unwrap()),
        Some(&CachedOutcome::Failed(ScanFailure::GaveNoAnswer { during: None }))
    );
}

#[test]
fn a_library_that_was_failed_by_the_breadcrumb_is_believed_failed_until_it_changes() {
    let folder = tempfile::tempdir().unwrap();
    let library = a_library_file(folder.path(), b"x");
    let mut scanner = a_scanner();
    scanner.fail_library(&library, ScanFailure::GaveNoAnswer { during: None });
    assert!(
        scanner
            .cache()
            .lookup(&library, &FileFingerprint::of(&library).unwrap())
            .is_some()
    );
    fs::write(&library, b"changed").unwrap();
    assert!(
        scanner
            .cache()
            .lookup(&library, &FileFingerprint::of(&library).unwrap())
            .is_none()
    );
    // A file that is not there cannot be failed: there is nothing to say it is the same file.
    let mut scanner = a_scanner();
    scanner.fail_library(
        &folder.path().join("not_there"),
        ScanFailure::GaveNoAnswer { during: None },
    );
    assert!(scanner.cache().is_empty());
}

// --- CLAP libraries (piece 7.8.2) ---

fn a_clap_scan() -> clap_host::scan::LibraryScan {
    clap_host::scan::LibraryScan {
        plugins: vec![clap_host::scan::ScannedPlugin {
            description: clap_host::PluginDescription {
                id: "org.example.gain".to_owned(),
                name: "Gain".to_owned(),
                ..clap_host::PluginDescription::default()
            },
            audio_inputs: vec![clap_host::scan::AudioPort {
                id: 0,
                name: "Input".to_owned(),
                channel_count: 2,
                port_type: Some("stereo".to_owned()),
                is_main: true,
            }],
            audio_outputs: Vec::new(),
            parameters: Vec::new(),
            latency: 64,
            tail: Some(1000),
            has_state: false,
            problem: None,
        }],
        problems: vec!["plugin number 1 has no id".to_owned()],
    }
}

#[test]
fn the_answer_of_a_clap_scan_is_read_back_as_it_was_written_and_the_other_words_are_unchanged() {
    for answer in [
        ScanAnswer::ScannedClap(a_clap_scan()),
        ScanAnswer::NotAClapLibrary("no factory".to_owned()),
    ] {
        let said = read_what_was_said(&answer_line(&answer));
        assert_eq!(said.answer, Some(Ok(answer)));
    }
    // The words of the LADSPA answers are what they were (the cache and the scan program of another version are told apart by the number, not by the words).
    assert!(answer_line(&ScanAnswer::CouldNotBeLoaded("x".to_owned())).contains("CouldNotBeLoaded"));
    assert_eq!(super::protocol::SCAN_FORMAT, 2);
}

#[test]
fn a_program_that_answered_for_a_clap_library_gives_the_scan_or_the_reason_it_was_refused() {
    let scanned = ScanAnswer::ScannedClap(a_clap_scan());
    assert_eq!(
        outcome(HowItEnded::Exited(ended_well()), said_by_a_scan(&["a"], Some(&scanned))),
        LibraryOutcome::ScannedClap(a_clap_scan())
    );
    let not_clap = ScanAnswer::NotAClapLibrary("no factory".to_owned());
    assert_eq!(
        outcome(HowItEnded::Exited(ended_well()), said_by_a_scan(&[], Some(&not_clap))),
        LibraryOutcome::Failed(ScanFailure::NotAClapLibrary("no factory".to_owned()))
    );
    assert!(
        ScanFailure::NotAClapLibrary("no factory".to_owned())
            .to_string()
            .contains("not a CLAP library")
    );
}

#[test]
fn a_clap_scan_is_kept_in_the_cache_file_and_the_scans_that_failed_for_want_of_a_library_are_forgotten_when_the_search_changes()
 {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("scan_cache.json");
    let clap = touch(folder.path(), "a.clap");
    let not_clap = touch(folder.path(), "b.clap");
    let crashed = touch(folder.path(), "c.clap");
    let mut cache = ScanCache::at(file.clone());
    cache.set_search_signature("first");
    for (path, outcome) in [
        (&clap, CachedOutcome::ScannedClap(a_clap_scan())),
        (
            &not_clap,
            CachedOutcome::Failed(ScanFailure::NotAClapLibrary("no factory".to_owned())),
        ),
        (
            &crashed,
            CachedOutcome::Failed(ScanFailure::Crashed {
                how: "signal 6".to_owned(),
                during: None,
            }),
        ),
    ] {
        let fingerprint = FileFingerprint::of(path).unwrap();
        cache.record(path, fingerprint, outcome);
    }
    cache.save();

    let mut again = ScanCache::at(file);
    assert_eq!(
        again.lookup(&clap, &FileFingerprint::of(&clap).unwrap()),
        Some(&CachedOutcome::ScannedClap(a_clap_scan()))
    );
    again.set_search_signature("second");
    assert!(
        again
            .lookup(&not_clap, &FileFingerprint::of(&not_clap).unwrap())
            .is_none(),
        "a library that was not a CLAP one may be found to be, now that its other libraries can be found"
    );
    assert!(again.lookup(&clap, &FileFingerprint::of(&clap).unwrap()).is_some());
    assert!(
        again
            .lookup(&crashed, &FileFingerprint::of(&crashed).unwrap())
            .is_some(),
        "a crash is the libraries own doing and stays"
    );
}

#[test]
fn a_cache_written_by_the_version_that_scanned_only_ladspa_libraries_is_not_believed() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("scan_cache.json");
    let library = a_library_file(folder.path(), b"x");
    let mut writer = ScanCache::at(file.clone());
    writer.record(
        &library,
        FileFingerprint::of(&library).unwrap(),
        CachedOutcome::Scanned(a_scan()),
    );
    writer.save();
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace("\"format\": 2", "\"format\": 1");
    assert!(text.contains("\"format\": 1"));
    fs::write(&file, text).unwrap();

    assert!(ScanCache::at(file).is_empty());
}

#[test]
fn a_file_is_a_clap_library_by_its_ending_whatever_the_case_and_the_folders_of_clap_are_those_of_the_standard() {
    assert!(is_a_clap_file(Path::new("/a/b/plugin.clap")));
    assert!(is_a_clap_file(Path::new("PLUGIN.CLAP")));
    assert!(!is_a_clap_file(Path::new("/a/b/plugin.dll")));
    assert!(!is_a_clap_file(Path::new("/a/b/clap")));
    assert!(!is_a_clap_file(Path::new("/a/b/plugin.clap.txt")));

    let variable = std::env::join_paths(["/from/variable", "", "/second"]).unwrap();
    let folders = clap_folders(
        Some(&variable),
        Some(Path::new("/home/someone")),
        Some(Path::new("/common")),
        Some(Path::new("/local")),
    );
    // (CLAP_PATH first, in its own order; an empty entry is no folder.)
    assert_eq!(
        &folders[..2],
        [PathBuf::from("/from/variable"), PathBuf::from("/second")]
    );
    let usual = &folders[2..];
    if cfg!(windows) {
        assert_eq!(
            usual,
            [
                Path::new("/common").join("CLAP"),
                Path::new("/local").join("Programs").join("Common").join("CLAP")
            ]
        );
    } else if cfg!(target_os = "macos") {
        assert_eq!(
            usual,
            [
                PathBuf::from("/Library/Audio/Plug-Ins/CLAP"),
                Path::new("/home/someone").join("Library/Audio/Plug-Ins/CLAP")
            ]
        );
    } else {
        assert_eq!(
            usual,
            [PathBuf::from("/usr/lib/clap"), PathBuf::from("/home/someone/.clap")]
        );
    }
    // Nothing known of the home or of the places of Windows: only what is certain.
    let without = clap_folders(None, None, None, None);
    assert_eq!(without.len(), usize::from(!cfg!(windows)));
}

#[test]
fn the_clap_libraries_of_a_folder_are_those_in_it_and_below_it_in_the_order_of_their_paths() {
    let folder = tempfile::tempdir().unwrap();
    let b = touch(folder.path(), "b.clap");
    let upper = touch(folder.path(), "A.CLAP");
    touch(folder.path(), "notes.txt");
    touch(folder.path(), &format!("ladspa.{}", library_file_extension()));
    let vendor = folder.path().join("Vendor");
    fs::create_dir_all(vendor.join("deeper")).unwrap();
    let in_vendor = touch(&vendor, "c.clap");
    let deeper = touch(&vendor.join("deeper"), "d.clap");

    let found = clap_libraries_in(folder.path()).unwrap();

    // (A before b, without regard to case; and what is in a folder is after the files above it only because of its path.)
    assert_eq!(found, [upper, b, in_vendor, deeper]);
    assert_eq!(
        clap_libraries_in(&folder.path().join("not_there")).unwrap(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn the_walk_for_clap_libraries_stops_six_folders_down() {
    let folder = tempfile::tempdir().unwrap();
    let mut at = folder.path().to_owned();
    let mut reached = Vec::new();
    for depth in 0..=8 {
        reached.push((depth, touch(&at, "p.clap")));
        at = at.join(format!("level{depth}"));
        fs::create_dir_all(&at).unwrap();
    }

    let found = clap_libraries_in(folder.path()).unwrap();

    // (The folder asked for is down 0; six folders below it are looked in.)
    let expected: Vec<PathBuf> = reached
        .into_iter()
        .filter(|(depth, _)| *depth <= 6)
        .map(|(_, path)| path)
        .collect();
    assert_eq!(found.len(), expected.len());
    for path in expected {
        assert!(found.contains(&path), "{}", path.display());
    }
}

#[cfg(unix)]
#[test]
fn a_link_to_a_folder_is_not_walked_so_a_link_up_cannot_make_the_walk_endless() {
    let folder = tempfile::tempdir().unwrap();
    let real = touch(folder.path(), "real.clap");
    std::os::unix::fs::symlink(folder.path(), folder.path().join("loop")).unwrap();

    assert_eq!(clap_libraries_in(folder.path()).unwrap(), [real]);
}

#[test]
fn a_bundle_is_fingerprinted_by_the_file_inside_it_and_a_folder_with_no_such_file_by_itself() {
    let folder = tempfile::tempdir().unwrap();
    let bundle = folder.path().join("Plugin.clap");
    let inside = bundle.join("Contents").join("MacOS");
    fs::create_dir_all(&inside).unwrap();
    fs::write(inside.join("Plugin"), b"12345").unwrap();

    let fingerprint = FileFingerprint::of(&bundle).unwrap();

    assert_eq!(fingerprint.size, 5, "the size of the library inside, not of the folder");
    fs::write(inside.join("Plugin"), b"1234567").unwrap();
    assert_eq!(FileFingerprint::of(&bundle).unwrap().size, 7);
    // Two files there: it cannot be told which is the library, and the folder itself is looked at.
    fs::write(inside.join("Other"), b"x").unwrap();
    assert_eq!(
        FileFingerprint::of(&bundle).unwrap().size,
        fs::metadata(&bundle).unwrap().len()
    );
}

#[test]
fn the_folders_looked_in_by_default_are_ladspa_path_and_its_usual_places_then_clap_path_and_its_usual_places() {
    let environment = |name: &str| -> Option<std::ffi::OsString> {
        match name {
            "LADSPA_PATH" => Some("/ladspa/from/variable".into()),
            "CLAP_PATH" => Some("/clap/from/variable".into()),
            "COMMONPROGRAMFILES" => Some("/common".into()),
            "LOCALAPPDATA" => Some("/local".into()),
            _ => None,
        }
    };
    let home = Path::new("/home/someone");

    let folders = folders_looked_in_with(&environment, Some(home));

    let mut expected = vec![PathBuf::from("/ladspa/from/variable")];
    expected.extend(usual_folders(Some(home)));
    expected.extend(clap_folders(
        Some("/clap/from/variable".as_ref()),
        Some(home),
        Some(Path::new("/common")),
        Some(Path::new("/local")),
    ));
    assert_eq!(folders, expected);
    assert!(folders.contains(&PathBuf::from("/clap/from/variable")));
    assert_eq!(
        folders_looked_in_with(&|_| None, None).len(),
        clap_folders(None, None, None, None).len() + usual_folders(None).len()
    );
}

#[test]
fn a_folder_named_like_a_clap_library_is_walked_into_where_a_library_is_a_file() {
    if cfg!(target_os = "macos") {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let named_like_one = folder.path().join("folder.clap");
    fs::create_dir(&named_like_one).unwrap();
    let inside = touch(&named_like_one, "inside.clap");

    assert_eq!(clap_libraries_in(folder.path()).unwrap(), [inside]);
}

#[test]
fn paths_are_put_in_the_order_of_their_text_without_regard_to_case_whatever_order_they_come_in() {
    let mut paths: Vec<PathBuf> = ["/x/b.clap", "/x/B.clap", "/x/a/z.clap", "/x/A.clap", "/y.clap"]
        .iter()
        .map(PathBuf::from)
        .collect();

    super::folders::sort_by_path_without_regard_to_case(&mut paths);

    let sorted: Vec<&str> = paths.iter().map(|path| path.to_str().unwrap()).collect();
    assert_eq!(
        sorted,
        ["/x/A.clap", "/x/a/z.clap", "/x/B.clap", "/x/b.clap", "/y.clap"]
    );
}

#[test]
fn what_a_clap_library_says_when_it_is_refused_is_not_confused_with_what_a_ladspa_one_says() {
    assert_eq!(
        ScanAnswer::from(clap_host::OpenError::NotAClapLibrary("no factory".to_owned())),
        ScanAnswer::NotAClapLibrary("no factory".to_owned())
    );
    assert_eq!(
        ScanAnswer::from(clap_host::OpenError::CouldNotBeLoaded("no file".to_owned())),
        ScanAnswer::CouldNotBeLoaded("no file".to_owned())
    );
}

#[test]
fn the_build_identity_is_read_from_what_a_program_wrote_and_the_last_line_that_says_it_wins() {
    use super::protocol::{BUILD_ID, build_id_in, build_id_line};
    assert_eq!(build_id_in(&build_id_line()).as_deref(), Some(BUILD_ID));
    assert_eq!(
        build_id_in("noise\nffrs-scan-build: 1234\r\nmore noise\n").as_deref(),
        Some("1234")
    );
    assert_eq!(
        build_id_in("ffrs-scan-build: old\nffrs-scan-build: new\n").as_deref(),
        Some("new")
    );
    assert_eq!(build_id_in("a message that is not it: ffrs-scan-build: 99"), None);
    assert_eq!(build_id_in(""), None);
    assert_eq!(BUILD_ID.len(), 16, "the identity is sixteen hexadecimal digits");
}
