// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scans a LADSPA library in a process of its own: see `fastforwardrs::plugin_scan`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let (Some(library), None) = (arguments.next(), arguments.next()) else {
        eprint!("{}", fastforwardrs::plugin_scan::program::USAGE);
        return ExitCode::from(2);
    };
    if let Err(error) = fastforwardrs::plugin_scan::program::run(std::path::Path::new(&library), &mut std::io::stdout())
    {
        eprintln!("the answer could not be written: {error}");
        return ExitCode::from(1);
    }
    // The program ends here and now, without the library being unloaded: some libraries crash when they are, and the answer has been given.
    std::process::exit(0)
}
