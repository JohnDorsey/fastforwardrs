// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The helper of the tests of the editor-window program (`tests/editor_program`), tried on its own: what it does with a program that has already ended when it is told something.
//! (A plugin that cannot work makes the program say `Failed` and end before it reads a line; on a slow machine the test writes its first line after that. Found in CI of 4773e25, where
//! `a_plugin_without_a_window_or_a_class_that_is_not_there_or_a_module_that_is_not_there_is_a_failure_that_says_why` panicked with a broken pipe, and reproduced on Linux with every CPU busy:
//! 0 of 200 runs failed alone, 83 of 200 with six busy loops on 12 threads, 200 of 200 with twelve.)

mod editor_program;

use std::path::Path;

use editor_program::Running;
use fastforwardrs::plugin_editor::protocol::{FromProgram, ToProgram};

/// A program that says `Failed` and ends with 1, whatever it is told: there is no such library (this fails before the program reads anything).
fn a_program_that_fails_at_once() -> Running {
    Running::start(
        "clap",
        Path::new("a-library-that-is-not-there.clap"),
        "no.such.plugin",
        true,
        &[],
    )
}

fn the_reason_it_failed(program: &Running) -> String {
    match program
        .said_until(|message| matches!(message, FromProgram::Failed { .. }))
        .last()
    {
        Some(FromProgram::Failed { reason }) => reason.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_program_that_has_ended_can_be_told_without_a_panic_and_what_it_said_and_how_it_ended_are_still_read() {
    let mut program = a_program_that_fails_at_once();
    // (Waiting for the end first is what makes the pipe broken for certain: the test does not have to be slow for it.)
    assert_eq!(program.exit_code(), Some(1));

    program.begin(&[(7, 0.5)], Some("a state"));
    program.tell(&ToProgram::Close);

    let reason = the_reason_it_failed(&program);
    assert!(reason.contains("the library could not be loaded"), "{reason}");
    assert_eq!(program.exit_code(), Some(1));
}

#[test]
fn a_program_that_is_told_before_and_after_it_ended_is_told_the_same_way() {
    let mut program = a_program_that_fails_at_once();
    program.begin(&[], None);
    assert_eq!(program.exit_code(), Some(1));
    program.tell(&ToProgram::SaveState);

    assert!(the_reason_it_failed(&program).contains("the library could not be loaded"));
}
