// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The inside of the scan program (`fastforwardrs-plugin-scan`, which is only this, called from `main`). **It loads the library it is given**, which is what it is for: the
//! editor starts it on a library, and it is the one that crashes or never returns if the library does.

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use super::folders::{is_a_clap_file, is_an_lv2_bundle};
use super::protocol::{ScanAnswer, answer_line, step_line};

/// What the program says when it is used wrongly.
pub const USAGE: &str = "Usage: fastforwardrs-plugin-scan <library file or LV2 bundle folder>

Loads a LADSPA library (or a CLAP one, if the file ends in .clap; or reads an LV2 bundle, if it is a folder with a manifest.ttl, and loads its library), reads what its plugins say,
makes each plugin and runs it once on silence, and writes what it found on its standard
output (lines that begin with \"ffrs-scan-step: \" say what it is about to do, and the line
that begins with \"ffrs-scan-result: \" is the answer). It exists so that a library that
crashes or never returns does that to this program and not to the editor.
";

/// The environment variable that gives the program the time, in whole seconds, to check the start of the plugins of a CLAP library for (instead of the time that `clap_host` gives a
/// library): **for the tests**, whose answers must not depend on how fast the computer is (a check that is not made says `NotChecked`).
pub const START_CHECK_SECONDS_VARIABLE: &str = "FASTFORWARDRS_START_CHECK_SECONDS";

/// The time to check the start of the plugins of a CLAP library for, if `variable` (the value of [`START_CHECK_SECONDS_VARIABLE`]) says one: whole seconds, at most a day.
pub fn start_check_time_from(variable: Option<&str>) -> Option<Duration> {
    let seconds: u64 = variable?.trim().parse().ok()?;
    (seconds <= 24 * 60 * 60).then(|| Duration::from_secs(seconds))
}

/// Scans the library at `library`, writing the steps and then the answer to `out`. Every line is flushed when it is written: if the library crashes the program, the last step is
/// already out. (Each line begins on a new line, because a library may have written something that does not end in one.) **A file that ends in `.clap` is read as a CLAP
/// library, and any other as a LADSPA one.**
pub fn run(library: &Path, out: &mut impl Write) -> io::Result<()> {
    // (Stops the system from asking a person what to do about a crash: the editor is the one that is told. This is a call of the LADSPA host's crate, which has the unsafe code.)
    ladspa_host::prepare_to_crash_quietly();
    let mut said = |line: &str| -> io::Result<()> {
        out.write_all(format!("\n{line}\n").as_bytes())?;
        out.flush()
    };
    let mut broken_pipe = None;
    let mut step = |step: &str| {
        if let Err(error) = said(&step_line(step)) {
            broken_pipe.get_or_insert(error);
        }
    };
    let answer = if is_an_lv2_bundle(library) {
        match lv2_host::scan_bundle(library, &mut step) {
            Ok(scan) => ScanAnswer::ScannedLv2(scan),
            Err(error) => ScanAnswer::NotAnLv2Bundle(error.to_string()),
        }
    } else if is_a_clap_file(library) {
        let result = match start_check_time_from(std::env::var(START_CHECK_SECONDS_VARIABLE).ok().as_deref()) {
            Some(longest) => clap_host::scan::scan_library_checking_starts_for(library, longest, &mut step),
            None => clap_host::scan::scan_library(library, &mut step),
        };
        match result {
            Ok(scan) => ScanAnswer::ScannedClap(scan),
            Err(error) => ScanAnswer::from(error),
        }
    } else {
        match ladspa_host::scan_library(library, &mut step) {
            Ok(scan) => ScanAnswer::Scanned(scan),
            Err(error) => ScanAnswer::from(error),
        }
    };
    if let Some(error) = broken_pipe {
        return Err(error);
    }
    said(&answer_line(&answer))
}
