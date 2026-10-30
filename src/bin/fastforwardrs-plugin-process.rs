// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs one copy of a sound plugin in a process of its own: see `fastforwardrs::plugin_process`.

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
    let address = match fastforwardrs::plugin_process::program::parse_arguments(std::env::args_os().skip(1)) {
        Ok(address) => address,
        Err(error) => {
            eprintln!("{error}");
            eprint!("{}", fastforwardrs::plugin_process::program::USAGE);
            return ExitCode::from(2);
        }
    };
    let token = std::env::var(fastforwardrs::plugin_process::protocol::TOKEN_VARIABLE).unwrap_or_default();
    // The program ends here and now, without the plugin being destroyed or its library unloaded: some crash when they are, and nothing is left to do.
    std::process::exit(fastforwardrs::plugin_process::program::run(address, &token))
}
