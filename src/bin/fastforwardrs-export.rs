// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The export from the command line, with no window: see `fastforwardrs::application::command_line`.

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(fastforwardrs::application::command_line::run(
        std::env::args_os().skip(1),
        &mut std::io::stderr(),
    ))
}
