// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a library could not be scanned. These are kept in the file of scans (a library that failed is not tried again until it changes), and said to a person in words.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a library is failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanFailure {
    /// The system's loader refused the file (it is not a library of this system, it is damaged or empty, a library that it needs is not there, or its start-up code failed).
    CouldNotBeLoaded(String),
    /// It loaded, and it has no `ladspa_descriptor`: it is a library, and not a LADSPA one.
    NotALadspaLibrary(String),
    /// It loaded, and it has no CLAP plugin factory: it is a library, and not a CLAP one.
    NotAClapLibrary(String),
    /// The scan did not end in the time it was given, and was stopped. `during` is the last thing it said it was about to do.
    TookTooLong { seconds: u64, during: Option<String> },
    /// The scanner ended without an answer and with an exit that is not a success: it crashed. `how` says how, `during` what it was about to do.
    Crashed { how: String, during: Option<String> },
    /// The scanner ended well without an answer.
    GaveNoAnswer { during: Option<String> },
    /// The scanner's answer is not in the format.
    AnswerWasNotInTheFormat(String),
    /// The editor ended badly while it was using this library, and this was the last library it began to use. `others` is how many others it was using.
    WasInUseWhenTheProgramEndedBadly { others: usize },
}

impl fmt::Display for ScanFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let during = |during: &Option<String>| match during {
            Some(step) => format!(" while it was {step}"),
            None => String::new(),
        };
        match self {
            Self::CouldNotBeLoaded(reason) => write!(formatter, "it could not be loaded: {reason}"),
            Self::NotALadspaLibrary(reason) => write!(formatter, "it is not a LADSPA library: {reason}"),
            Self::NotAClapLibrary(reason) => write!(formatter, "it is not a CLAP library: {reason}"),
            Self::TookTooLong { seconds, during: step } => {
                write!(
                    formatter,
                    "it did not finish in {seconds} seconds and was stopped{}",
                    during(step)
                )
            }
            Self::Crashed { how, during: step } => write!(formatter, "it crashed the scanner ({how}){}", during(step)),
            Self::GaveNoAnswer { during: step } => {
                write!(formatter, "the scanner ended and said nothing about it{}", during(step))
            }
            Self::AnswerWasNotInTheFormat(reason) => {
                write!(
                    formatter,
                    "what the scanner said about it was not in the format: {reason}"
                )
            }
            Self::WasInUseWhenTheProgramEndedBadly { others: 0 } => {
                write!(formatter, "the program ended badly while it was using this library")
            }
            Self::WasInUseWhenTheProgramEndedBadly { others } => write!(
                formatter,
                "the program ended badly while it was using this library, and {others} other librar{} (this one was the last that was started)",
                if *others == 1 { "y" } else { "ies" }
            ),
        }
    }
}
