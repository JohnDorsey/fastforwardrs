// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The inside of the scan program (`fastforwardrs-plugin-scan`, which is only this, called from `main`). **It loads the library it is given**, which is what it is for: the
//! editor starts it on a library, and it is the one that crashes or never returns if the library does.

use std::io::{self, Write};
use std::path::Path;

use super::protocol::{ScanAnswer, answer_line, step_line};

/// What the program says when it is used wrongly.
pub const USAGE: &str = "Usage: fastforwardrs-plugin-scan <library file>\n\nLoads a LADSPA library, reads what its plugins say, makes each plugin and runs it once on\nsilence, and writes what it found on its standard output (lines that begin with\n\"ffrs-scan-step: \" say what it is about to do, and the line that begins with\n\"ffrs-scan-result: \" is the answer). It exists so that a library that crashes or never\nreturns does that to this program and not to the editor.\n";

/// Scans the library at `library`, writing the steps and then the answer to `out`. Every line is flushed when it is written: if the library crashes the program, the last step is
/// already out. (Each line begins on a new line, because a library may have written something that does not end in one.)
pub fn run(library: &Path, out: &mut impl Write) -> io::Result<()> {
    ladspa_host::prepare_to_crash_quietly();
    let mut said = |line: &str| -> io::Result<()> {
        out.write_all(format!("\n{line}\n").as_bytes())?;
        out.flush()
    };
    let mut broken_pipe = None;
    let answer = match ladspa_host::scan_library(library, |step| {
        if let Err(error) = said(&step_line(step)) {
            broken_pipe.get_or_insert(error);
        }
    }) {
        Ok(scan) => ScanAnswer::Scanned(scan),
        Err(error) => ScanAnswer::from(error),
    };
    if let Some(error) = broken_pipe {
        return Err(error);
    }
    said(&answer_line(&answer))
}
