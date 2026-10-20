// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Running the scan program on one library, and understanding how it ended. **Whatever the library does to the scan program** (crash it, make it wait for ever, make it say
//! nonsense) **comes out here as a [`ScanFailure`]**, and the editor goes on.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::failure::ScanFailure;
use super::protocol::{ScanAnswer, read_what_was_said};
use super::scanner::LibraryOutcome;
use crate::media::{background_command, collect_error_output_in_background};

/// How long the scan of a library may take before the program is stopped. (A plugin that needs longer than this to be made is not one to use in a preview.)
pub const SCAN_TIME_LIMIT: Duration = Duration::from_secs(5);

/// The most that is read of what the scan program says, in bytes: more is thrown away. (The answer for a library of a hundred plugins is a few hundred kilobytes.)
const MOST_TO_READ: usize = 32 * 1024 * 1024;

/// How long to wait, after the program has ended or been stopped, for the threads that read what it said to finish. (A library that started another program that holds the
/// pipe open would keep them for ever.)
const LONGEST_WAIT_FOR_THE_READERS: Duration = Duration::from_secs(2);

/// How often to look at whether the program has ended.
const HOW_OFTEN_TO_LOOK: Duration = Duration::from_millis(2);

/// The name of the scan program, without the ending that programs have on this system.
pub const SCANNER_PROGRAM_NAME: &str = "fastforwardrs-plugin-scan";

/// Where the scan program is, if it is beside the program that is running (the editor and the scan program are built into one folder, and installed into one), or, when this is a
/// test of the library (which `cargo` puts in the folder `deps` inside it), the folder above.
pub fn scanner_program_beside_this_program() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    let name = format!("{SCANNER_PROGRAM_NAME}{}", std::env::consts::EXE_SUFFIX);
    let folder = program.parent()?;
    let mut candidates = vec![folder.join(&name)];
    if folder.file_name().is_some_and(|folder_name| folder_name == "deps") {
        candidates.push(folder.parent()?.join(&name));
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// How the program ended.
pub(super) enum HowItEnded {
    Exited(ExitStatus),
    /// It was still running when the time ran out, and was stopped.
    StoppedAfterTheTimeLimit,
}

/// What the scan program did.
pub(super) struct WhatItDid {
    pub(super) how_it_ended: HowItEnded,
    pub(super) said: String,
}

/// Runs the scan program `program` on `library`, and says what came of it. An error is for when the program **could not be run at all**, which is not the library's fault
/// (it is not a reason to fail the library).
pub fn scan_in_a_program(
    program: &Path,
    library: &Path,
    time_limit: Duration,
    environment: &[(String, String)],
) -> io::Result<LibraryOutcome> {
    let mut command: Command = background_command(program);
    command
        .envs(environment.iter().map(|(name, value)| (name, value)))
        .arg(library)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let standard_output = child.stdout.take().expect("the output was piped");
    let standard_error = child.stderr.take().expect("the error output was piped");
    let said = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader = {
        let said = Arc::clone(&said);
        thread::spawn(move || read_until_the_end(standard_output, &said, MOST_TO_READ))
    };
    let error_reader = collect_error_output_in_background(standard_error);

    let deadline = Instant::now() + time_limit;
    let how_it_ended = loop {
        match child.try_wait()? {
            Some(status) => break HowItEnded::Exited(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break HowItEnded::StoppedAfterTheTimeLimit;
            }
            None => thread::sleep(HOW_OFTEN_TO_LOOK),
        }
    };
    // (What it said before it ended is in the pipe, and is read in a moment: wait for the end of the pipe, but not for ever.)
    if wait_until_finished(&reader, LONGEST_WAIT_FOR_THE_READERS) {
        let _ = reader.join();
        let _ = error_reader.join();
    }
    let said = String::from_utf8_lossy(&said.lock().unwrap_or_else(|poisoned| poisoned.into_inner())).into_owned();
    Ok(outcome_of(&WhatItDid { how_it_ended, said }, time_limit))
}

/// Waits until the thread has finished, but no longer than `longest`. Whether it has.
pub(super) fn wait_until_finished<T>(thread: &JoinHandle<T>, longest: Duration) -> bool {
    let waited_from = Instant::now();
    while !thread.is_finished() && waited_from.elapsed() < longest {
        thread::sleep(HOW_OFTEN_TO_LOOK);
    }
    thread.is_finished()
}

/// Reads everything the program writes into `into`, up to `most` bytes; what is past the limit is read and thrown away, so that the program is never stopped by a full pipe.
pub(super) fn read_until_the_end(mut pipe: impl Read, into: &Mutex<Vec<u8>>, most: usize) {
    let mut buffer = [0_u8; 16 * 1024];
    let mut kept = 0;
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => {
                let room = most.saturating_sub(kept).min(count);
                if room > 0 {
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .extend_from_slice(&buffer[..room]);
                    kept += room;
                }
            }
        }
    }
}

/// What came of the scan, from what the program did.
pub(super) fn outcome_of(did: &WhatItDid, time_limit: Duration) -> LibraryOutcome {
    let said = read_what_was_said(&did.said);
    let during = said.last_step;
    match &did.how_it_ended {
        HowItEnded::StoppedAfterTheTimeLimit => LibraryOutcome::Failed(ScanFailure::TookTooLong {
            seconds: time_limit.as_secs(),
            during,
        }),
        HowItEnded::Exited(status) => match said.answer {
            Some(Ok(ScanAnswer::Scanned(scan))) => LibraryOutcome::Scanned(scan),
            Some(Ok(ScanAnswer::ScannedClap(scan))) => LibraryOutcome::ScannedClap(scan),
            Some(Ok(ScanAnswer::CouldNotBeLoaded(reason))) => {
                LibraryOutcome::Failed(ScanFailure::CouldNotBeLoaded(reason))
            }
            Some(Ok(ScanAnswer::NotALadspaLibrary(reason))) => {
                LibraryOutcome::Failed(ScanFailure::NotALadspaLibrary(reason))
            }
            Some(Ok(ScanAnswer::NotAClapLibrary(reason))) => {
                LibraryOutcome::Failed(ScanFailure::NotAClapLibrary(reason))
            }
            Some(Err(reason)) => LibraryOutcome::Failed(ScanFailure::AnswerWasNotInTheFormat(reason)),
            None if status.success() => LibraryOutcome::Failed(ScanFailure::GaveNoAnswer { during }),
            None => LibraryOutcome::Failed(ScanFailure::Crashed {
                how: how_it_ended_badly(*status),
                during,
            }),
        },
    }
}

/// How a program that ended badly ended, in words.
fn how_it_ended_badly(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let name = match signal {
                4 => " (illegal instruction)",
                6 => " (abort)",
                7 => " (bus error)",
                8 => " (arithmetic error)",
                9 => " (killed)",
                11 => " (segmentation fault)",
                15 => " (terminated)",
                _ => "",
            };
            return format!("signal {signal}{name}");
        }
    }
    match status.code() {
        // (The codes that Windows gives a program that crashed.)
        Some(code) if code as u32 == 0xC000_0005 => "0xC0000005: access violation".to_owned(),
        Some(code) if code as u32 == 0xC000_00FD => "0xC00000FD: stack overflow".to_owned(),
        Some(code) if code as u32 == 0xC000_0409 => {
            "0xC0000409: the program stopped itself after finding memory damaged".to_owned()
        }
        Some(code) if code as u32 == 0xC000_001D => "0xC000001D: illegal instruction".to_owned(),
        Some(code) if code as u32 == 0xC000_0094 => "0xC0000094: division by zero".to_owned(),
        Some(code) if (code as u32) >= 0xC000_0000 => format!("exit code 0x{:X}", code as u32),
        Some(code) => format!("exit code {code}"),
        None => "ended with no exit code".to_owned(),
    }
}
