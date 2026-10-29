// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The check of a plugin's start, on the real test plugins.
//!
//! **One test, in a program of its own**, because the check is about what is in the memory of the program: other tests that run at the same time in the same program take and let go
//! of memory too, and a plugin that is made while they do is given what they left. (As the scan is, the check is done by a program that does nothing else at the time.)

use std::time::Duration;

use clap_host::scan::scan_library_checking_starts_for;
use clap_host::start_check::{StartCheck, check_the_start};
use clap_host::{Library, MainThread};
use clap_test_plugins::{FAILS_TO_ACTIVATE, FAILS_TO_PROCESS, GAIN, INSTRUMENT, READS_GARBAGE, SINGLE_INSTANCE};

#[test]
fn the_check_finds_the_plugin_that_reads_memory_it_did_not_write_and_no_other_of_the_test_plugins() {
    let file = clap_test_plugins::library_file_for_tests();

    // The scan checks every plugin that can be run on audio. (With a time that no runner uses up: the plugins after the time given are not checked, and with the time that the scan
    // is given for a real library, a computer twice as slow as the one this was made on left the later test plugins `NotChecked`, as the Windows runner of CI did once.)
    let scan =
        scan_library_checking_starts_for(&file, Duration::from_secs(600), |_| {}).expect("the library is scanned");
    for plugin in &scan.plugins {
        let id = plugin.description.id.as_str();
        if id == READS_GARBAGE {
            // (Verified on Windows and on Ubuntu, 40 of 40 times each. How memory is given out on macOS was not verified: there the check may miss it, and must not say anything else.)
            if cfg!(target_os = "macos") {
                assert!(
                    matches!(plugin.start, StartCheck::Unreliable { .. } | StartCheck::Steady),
                    "{:?}",
                    plugin.start
                );
            } else {
                assert!(
                    matches!(plugin.start, StartCheck::Unreliable { .. }),
                    "{:?}",
                    plugin.start
                );
            }
        } else if id == SINGLE_INSTANCE {
            // (It gives its sound with the sign changed while another of it is alive; it is made one at a time here, so it is steady.)
            assert_eq!(plugin.start, StartCheck::Steady, "{id}");
        } else if plugin.problem.is_some() || plugin.main_audio_input().is_none() {
            assert_eq!(
                plugin.start,
                StartCheck::NotChecked,
                "{id}: it cannot be run, or is not an effect"
            );
        } else {
            assert_eq!(plugin.start, StartCheck::Steady, "{id}");
        }
    }

    // A plugin that cannot be run, or has no audio to give it, is not checked.
    let library = Library::open(&file).expect("the library is opened");
    let worker = MainThread::start();
    for id in [INSTRUMENT, FAILS_TO_PROCESS, FAILS_TO_ACTIVATE] {
        assert_eq!(
            check_the_start(&worker, &library, id, 2, 2),
            StartCheck::NotChecked,
            "{id}"
        );
    }
    assert_eq!(
        check_the_start(&worker, &library, GAIN, 0, 2),
        StartCheck::NotChecked,
        "no audio input to give"
    );

    // The check is done for as long as the library is given, and no longer.
    let none = scan_library_checking_starts_for(&file, Duration::ZERO, |_| {}).expect("the library is scanned");
    assert!(
        none.plugins.iter().all(|plugin| plugin.start == StartCheck::NotChecked),
        "no time, no check"
    );
}
