# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for phase 14 (plugins in processes of their own): the frames and messages, the editor's side of a process, the program, and the host's setting.

First pass: the library's own tests of the new modules (the default below). Second pass, for what the first did not notice, the programs that run the real program:

    MUTATION_TESTS="--test plugin_process --test plugin_host_processes" py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_phase14_processes.py --only-not-noticed-in first.jsonl
"""

import os

TESTS = os.environ.get("MUTATION_TESTS", "--lib -- plugin_process")

PROTOCOL = "src/plugin_process/protocol.rs"
REMOTE = "src/plugin_process/remote.rs"
PROGRAM = "src/plugin_process/program.rs"
HOST = "src/plugin_host/host.rs"
PROCESSES = "src/plugin_host/processes.rs"

MUTATIONS = [
    # --- frames ---
    ("frame: the length leaves out the kind and the length of the header", PROTOCOL,
     "let body = 1 + 4 + header.len() + raw.len();", "let body = header.len() + raw.len();", TESTS),
    ("frame: a length of four is long enough", PROTOCOL, "if length < 5 {", "if length < 4 {", TESTS),
    ("frame: a header longer than the frame is allowed to reach into the sound", PROTOCOL,
     "if header_length > body.len() - 5 {", "if header_length > body.len() {", TESTS),
    ("frame: a read that ran out of time is only the one that Windows names", PROTOCOL,
     "io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock", "io::ErrorKind::TimedOut", TESTS),
    # --- messages ---
    ("reader: a count is checked against the bytes and not against the bytes for things of that size", PROTOCOL,
     "if count > self.bytes.len() / least_bytes.max(1) {", "if count > self.bytes.len() {", TESTS),
    ("reader: bytes after the end of a message are no matter", PROTOCOL, "if self.bytes.is_empty() {", "if true {", TESTS),
    ("note: a pitch wheel past 16 bits wraps", PROTOCOL,
     "value: u16::try_from(id).map_err(|_| NotAMessage(\"a pitch wheel is further than it goes\"))?,",
     "value: id as u16,", TESTS),
    ("answer: yes and no are swapped", PROTOCOL,
     "            1 => Ok(()),\n            0 => Err(reader.text()?),", "            0 => Ok(()),\n            1 => Err(reader.text()?),", TESTS),
    ("samples: fewer bytes than were said are enough", PROTOCOL,
     "if raw.len() != channels.len() * frames * 4 {", "if raw.len() < channels.len() * frames * 4 {", TESTS),
    ("samples: a channel that is too short is put into all the same", PROTOCOL,
     "if channels.iter().any(|channel| channel.len() < frames) {", "if false {", TESTS),
    # --- the editor's side ---
    ("remote: any token is the token", REMOTE, "hello.token == token", "hello.token != token", TESTS),
    ("remote: a read that ran out of time is a connection that ended", REMOTE,
     "let words = if error.is_a_timeout() {", "let words = if !error.is_a_timeout() {", TESTS),
    ("remote: a channel longer than the run is refused", REMOTE,
     ".any(|channel| channel.len() < frames)", ".any(|channel| channel.len() > frames)", TESTS),
    ("remote: the commands are given again at the next run", REMOTE,
     "commands: std::mem::take(&mut self.commands),", "commands: self.commands.clone(),", TESTS),
    ("remote: the latency of the last answer is not kept", REMOTE,
     "self.latency = ran.latency as usize;", "self.latency = self.latency;", TESTS),
    ("remote: only the last run's restart is told", REMOTE,
     "self.done_between_blocks.was_restarted |= ran.done_between_blocks.was_restarted;",
     "self.done_between_blocks.was_restarted = ran.done_between_blocks.was_restarted;", TESTS),
    ("remote: only the last run's words are told", REMOTE,
     "self.done_between_blocks.notes.extend(ran.done_between_blocks.notes);",
     "self.done_between_blocks.notes = ran.done_between_blocks.notes;", TESTS),
    ("remote: a value at a sample past 32 bits is given at the first", REMOTE,
     "at_sample: u32::try_from(at_sample).unwrap_or(u32::MAX),", "at_sample: u32::try_from(at_sample).unwrap_or(0),", TESTS),
    ("remote: the words of the program are joined with commas", REMOTE,
     "done.notes.join(\"; \")", "done.notes.join(\", \")", TESTS),
    # --- the program ---
    ("program: the program that the editor closed ends as if it had failed", PROGRAM,
     "Err(_) => std::process::exit(exit::EDITOR_GONE),", "Err(_) => std::process::exit(exit::SAID_SOMETHING_BROKEN),", TESTS),
    ("program: any number of frames is made room for", PROGRAM,
     "if request.frames > MOST_FRAMES_OF_A_RUN", "if request.frames > MOST_FRAMES_OF_A_RUN * 1000", TESTS),
    ("program: what the plugin asked for in the run is not done", PROGRAM,
     "let done_between_blocks = what_was_done_by(copy);", "let done_between_blocks = WhatWasDoneBetweenBlocks::default();", TESTS),
    ("program: a reset is not done", PROGRAM, "Command::Reset => copy.reset(),", "Command::Reset => {}", TESTS),
    ("program: a CLAP plugin is not checked to be the one that was scanned", PROGRAM,
     "if !make.scanned.is_this_clap_plugin(&instance) {", "if false {", TESTS),
    ("program: an LV2 plugin is not checked to be the one that was scanned", PROGRAM,
     "if !make.scanned.is_this_lv2_plugin(description) {", "if false {", TESTS),
    ("program: a LADSPA plugin is not checked to be the one that was scanned", PROGRAM,
     "if !make.scanned.is_this_ladspa_plugin(&instance) {", "if false {", TESTS),
    ("program: the plugin's state is not given", PROGRAM,
     "let state = make.has_state.then(|| Arc::<[u8]>::from(frame.raw.as_slice()));", "let state: Option<Arc<[u8]>> = None;", TESTS),
    # --- the host ---
    ("setting: the instruments setting runs every plugin in a process", PROCESSES,
     "Self::Instruments => plugin.descriptor.makes_sound_from_notes,", "Self::Instruments => true,", TESTS),
    ("setting: everything runs nothing in a process", PROCESSES, "Self::Everything => true,", "Self::Everything => false,", TESTS),
    ("setting: a program that is not of this build is used all the same", PROCESSES,
     "check_that_the_program_is_of_this_build(&program)?;", "let _ = check_that_the_program_is_of_this_build(&program);", TESTS),
    ("host: the project's state is not said to be there", HOST,
     "has_state: effect.state.is_some(),", "has_state: false,", TESTS),
    ("host: a stream that is being stopped makes its instrument all the same", HOST,
     ".is_stopping(stream_of(voice.notes))", ".is_stopping(usize::MAX)", TESTS),
    ("host: a library that could not be loaded is said to have been not made", HOST,
     "NotStarted::Refused(Refused::CouldNotBeLoaded(error)) => {\n                            words_for_a_plugin_that_could_not_be_loaded(plugin, &error)",
     "NotStarted::Refused(Refused::CouldNotBeLoaded(error)) => {\n                            words_for_a_plugin_that_could_not_be_made(plugin, &error)", TESTS),
    ("host: a plugin that is not what was scanned is said to have been not made", HOST,
     "NotStarted::Refused(Refused::NotWhatWasScanned) => {\n                            words_for_a_plugin_that_is_not_what_was_scanned(plugin)",
     "NotStarted::Refused(Refused::NotWhatWasScanned) => {\n                            words_for_a_plugin_that_could_not_be_loaded(plugin, \"\")", TESTS),
    ("host: the program that cannot be started is not said", HOST,
     "could not be run in a program of its own: {why}. The sound goes through it unchanged.", "could not be run: {why}.", TESTS),
]
