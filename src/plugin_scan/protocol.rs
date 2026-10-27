// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the scan program and the editor say to each other: the scan program writes lines on its standard output, and the editor reads them.
//!
//! A library that is being scanned is free to write to the standard output too (some plugins print), so **only lines that begin with a fixed word count**; the others are
//! noise and are thrown away. A line that begins with [`STEP_PREFIX`] says what the scan is about to do (the last of them says what it was doing if it crashes or is stopped), and
//! a line that begins with [`RESULT_PREFIX`] is the answer, in JSON, and is the last thing the scan program says.

use ladspa_host::{LibraryScan, OpenError};
use serde::{Deserialize, Serialize};

/// What a line that says what the scan is about to do begins with.
pub const STEP_PREFIX: &str = "ffrs-scan-step: ";

/// What the line with the answer begins with.
pub const RESULT_PREFIX: &str = "ffrs-scan-result: ";

/// The identity of the code this program was built from (see `build.rs`): the editor and the scan program, built together, have the same one.
pub const BUILD_ID: &str = env!("FFRS_BUILD_ID");

/// What the scan program is asked to tell instead of scanning a library.
pub const BUILD_ID_REQUEST: &str = "--build-id";

/// What the line that tells the identity begins with.
pub const BUILD_ID_PREFIX: &str = "ffrs-scan-build: ";

/// The line the scan program answers [`BUILD_ID_REQUEST`] with.
pub fn build_id_line() -> String {
    format!("{BUILD_ID_PREFIX}{BUILD_ID}")
}

/// The identity a program said it was built from, in what it wrote (`None`: it did not say, as a scan program of before this existed does not).
pub fn build_id_in(output: &str) -> Option<String> {
    output
        .lines()
        .filter_map(|line| line.trim_end().strip_prefix(BUILD_ID_PREFIX))
        .next_back()
        .map(str::to_owned)
}

/// What is in the cache of scans as well as on the wire. **Raise it when the scan finds something it did not (or means something else by what it finds)**, so that the answers
/// that were kept are not believed any more. (2: the scan reads CLAP libraries too. 3: the scan checks whether a CLAP plugin's start depends on memory it did not write. 4: the scan says whether a CLAP plugin has a window. 5: the scan reads LV2 bundles too. 6: what an LV2 plugin's bundle says of its windows (the features and the extension data of an interface). 7: the presets that an LV2 bundle says.)
pub const SCAN_FORMAT: u32 = 7;

/// The answer of the scan program.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ScanAnswer {
    Scanned(LibraryScan),
    /// A `.clap` file that loaded, with what its plugins said and how each one did when it was made and run.
    ScannedClap(clap_host::scan::LibraryScan),
    /// An LV2 bundle (a folder with a `manifest.ttl`) that was read, with what its plugins said and how each did when it was made and run.
    ScannedLv2(lv2_host::BundleScan),
    CouldNotBeLoaded(String),
    NotALadspaLibrary(String),
    NotAClapLibrary(String),
    /// A folder that is not an LV2 bundle (no manifest that can be read as Turtle).
    NotAnLv2Bundle(String),
}

impl From<clap_host::OpenError> for ScanAnswer {
    fn from(error: clap_host::OpenError) -> Self {
        match error {
            clap_host::OpenError::CouldNotBeLoaded(reason) => Self::CouldNotBeLoaded(reason),
            clap_host::OpenError::NotAClapLibrary(reason) => Self::NotAClapLibrary(reason),
        }
    }
}

impl From<OpenError> for ScanAnswer {
    fn from(error: OpenError) -> Self {
        match error {
            OpenError::CouldNotBeLoaded(reason) => Self::CouldNotBeLoaded(reason),
            OpenError::NotALadspaLibrary(reason) => Self::NotALadspaLibrary(reason),
        }
    }
}

/// The line that says the scan is about to do `step` (a step is one line: anything that would end it is made a space).
pub fn step_line(step: &str) -> String {
    format!("{STEP_PREFIX}{}", on_one_line(step))
}

/// The line with the answer.
pub fn answer_line(answer: &ScanAnswer) -> String {
    format!(
        "{RESULT_PREFIX}{}",
        serde_json::to_string(answer).expect("an answer can be written as JSON (it has only text, numbers and lists)")
    )
}

fn on_one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

/// What the scan program said, as far as it can be understood.
#[derive(Debug, PartialEq)]
pub struct WhatWasSaid {
    /// The last step that it said it was about to do.
    pub last_step: Option<String>,
    /// The last answer line: the answer, or why the line is not one.
    pub answer: Option<Result<ScanAnswer, String>>,
}

/// Reads the standard output of the scan program: the steps and the answer are found among the lines, and the rest is thrown away.
pub fn read_what_was_said(output: &str) -> WhatWasSaid {
    let mut said = WhatWasSaid {
        last_step: None,
        answer: None,
    };
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(step) = line.strip_prefix(STEP_PREFIX) {
            said.last_step = Some(step.to_owned());
        } else if let Some(json) = line.strip_prefix(RESULT_PREFIX) {
            said.answer = Some(serde_json::from_str(json).map_err(|error| error.to_string()));
        }
    }
    said
}
