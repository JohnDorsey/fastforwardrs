// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor-window program (`fastforwardrs-plugin-editor`) as a real process that a test drives with lines, for the tests of the windows of one kind of plugin. (The tests of the windows
//! of LV2 plugins, `plugin_editor_lv2.rs`, have a copy of this from before it was shared.)

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fastforwardrs::plugin_editor::protocol::{FromProgram, ParameterEdit, ToProgram, line_of, message_in};

pub const PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");
pub const HOW_LONG_TO_WAIT: Duration = Duration::from_secs(30);

/// A running editor-window program.
pub struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    said: Receiver<FromProgram>,
    /// The lines that were not messages: what the plugin printed.
    noise: Arc<Mutex<Vec<String>>>,
}

impl Running {
    /// Starts the program for a plugin of the kind `kind` (`clap`, `lv2` or `vst3`), with these variables of the environment added to its own.
    pub fn start(kind: &str, library: &Path, id: &str, headless: bool, environment: &[(&str, &str)]) -> Self {
        let mut command = Command::new(PROGRAM);
        if headless {
            command.arg("--headless");
        }
        command.envs(environment.iter().copied());
        let mut child = command
            .args(["--kind", kind, "--library"])
            .arg(library)
            .args(["--id", id])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the program starts");
        let stdout = child.stdout.take().expect("the output is piped");
        let (sender, said) = mpsc::channel();
        let noise = Arc::new(Mutex::new(Vec::new()));
        let noise_of_the_reader = Arc::clone(&noise);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(message) = message_in::<FromProgram>(&line) {
                    let _ = sender.send(message);
                } else if !line.trim().is_empty() {
                    noise_of_the_reader.lock().unwrap().push(line);
                }
            }
        });
        let stdin = child.stdin.take();
        Self {
            child,
            stdin,
            said,
            noise,
        }
    }

    pub fn tell(&mut self, message: &ToProgram) {
        let stdin = self.stdin.as_mut().expect("the input is open");
        writeln!(stdin, "{}", line_of(message)).expect("the program is told");
        stdin.flush().expect("the program is told");
    }

    pub fn next(&self) -> FromProgram {
        self.said
            .recv_timeout(HOW_LONG_TO_WAIT)
            .expect("the program says something in time")
    }

    /// What the program says, if it says something within `how_long`.
    pub fn next_within(&self, how_long: Duration) -> Option<FromProgram> {
        self.said.recv_timeout(how_long).ok()
    }

    /// Whether the program says something within `how_long`. (What it says is thrown away.)
    pub fn says_nothing_for(&self, how_long: Duration) -> bool {
        self.said.recv_timeout(how_long).is_err()
    }

    pub fn begin(&mut self, values: &[(u32, f64)], state: Option<&str>) {
        self.tell(&ToProgram::Begin {
            values: values.to_vec(),
            state: state.map(str::to_owned),
        });
    }

    /// What the program says until `last` (which is the last of them).
    pub fn said_until(&self, last: impl Fn(&FromProgram) -> bool) -> Vec<FromProgram> {
        let mut said = Vec::new();
        loop {
            let message = self.next();
            let done = last(&message);
            said.push(message);
            if done {
                return said;
            }
        }
    }

    /// What the program says until a hand lets go of a parameter (the end of the first gesture) or it fails.
    pub fn said_until_the_end_of_a_gesture(&self) -> Vec<FromProgram> {
        self.said_until(|message| {
            matches!(
                message,
                FromProgram::Edit {
                    edit: ParameterEdit::End { .. }
                } | FromProgram::Failed { .. }
            )
        })
    }

    pub fn exit_code(&mut self) -> Option<i32> {
        self.child.wait().expect("the program ends").code()
    }

    /// The lines that the plugin printed, once one has `needle` in it (the plugin prints as the program does what it does: a moment may pass).
    pub fn noise_until(&self, needle: &str) -> Vec<String> {
        let began = Instant::now();
        loop {
            let lines = self.noise.lock().unwrap().clone();
            if lines.iter().any(|line| line.contains(needle)) {
                return lines;
            }
            assert!(began.elapsed() < HOW_LONG_TO_WAIT, "{needle:?} was not said: {lines:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The lines that the plugin printed so far.
    pub fn noise(&self) -> Vec<String> {
        self.noise.lock().unwrap().clone()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
