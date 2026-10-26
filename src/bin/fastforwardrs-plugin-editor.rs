// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shows the window of a plugin in a process of its own: see `fastforwardrs::plugin_editor`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // The editor asks, before it uses this program, whether it was built from the same code as the editor (as it does of the scan program).
    if std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == fastforwardrs::plugin_scan::protocol::BUILD_ID_REQUEST)
        && std::env::args_os().count() == 2
    {
        println!("{}", fastforwardrs::plugin_scan::protocol::build_id_line());
        return ExitCode::SUCCESS;
    }
    let arguments = match fastforwardrs::plugin_editor::program::parse_arguments(std::env::args_os().skip(1)) {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("{error}");
            eprint!("{}", fastforwardrs::plugin_editor::program::USAGE);
            return ExitCode::from(2);
        }
    };
    fastforwardrs::plugin_editor::program::run(
        arguments,
        std::io::BufReader::new(std::io::stdin()),
        Box::new(std::io::stdout()),
    )
}
