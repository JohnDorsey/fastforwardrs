// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's side of the plugin windows (phase 10, piece 10.3): the windows that are open, each a process of the editor-window program ([`super::program`]).
//!
//! [`EditorWindows`] starts a program for a plugin's window, reads what it says on a thread of its own, and gives what it said to whoever asks ([`EditorWindows::poll`]), in order. It
//! knows nothing of projects: a window is for an effect of something (a [`WindowKey`]) and a library and an id; what the window's edits mean to the project is for the window of the
//! editor (`fast_forward_application::plugin_editor_flow`). **Whatever happens to the program comes out as an event** (it ends without saying so: [`WindowEvent::Failed`], and the
//! editor goes on), because a plugin's window is the part of a plugin that crashes most.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use super::program::PluginKind;
use super::protocol::{FromProgram, ParameterEdit, ToProgram, line_of, message_in};
use crate::media::background_command;
use crate::plugin_scan::protocol::{BUILD_ID, BUILD_ID_REQUEST};
use crate::project::{AnimationTarget, EffectInstanceId};

/// The name of the editor-window program, without the ending that programs have on this system.
pub const EDITOR_PROGRAM_NAME: &str = "fastforwardrs-plugin-editor";

/// How long a window is given to end after it was told to close, before the program is stopped.
pub const HOW_LONG_A_WINDOW_MAY_TAKE_TO_CLOSE: Duration = Duration::from_secs(3);

/// Where the editor-window program is, if it is beside the program that is running (as the scan program is), or, when this is a test of the library, in the folder above.
pub fn editor_program_beside_this_program() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    let name = format!("{EDITOR_PROGRAM_NAME}{}", std::env::consts::EXE_SUFFIX);
    let folder = program.parent()?;
    let mut candidates = vec![folder.join(&name)];
    if folder.file_name().is_some_and(|folder_name| folder_name == "deps") {
        candidates.push(folder.parent()?.join(&name));
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// Which window: the plugin effect `instance` of the clip or the track `target`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowKey {
    pub target: AnimationTarget,
    pub instance: EffectInstanceId,
}

/// What a window is opened for.
#[derive(Clone, Debug, PartialEq)]
pub struct WhatToOpen {
    /// A CLAP plugin's library file, or an LV2 plugin's bundle folder.
    pub library: PathBuf,
    /// A CLAP plugin's id, or an LV2 plugin's URI.
    pub plugin_id: String,
    pub kind: PluginKind,
    pub title: String,
    /// The values that the project says (`(id of the parameter, value)`).
    pub values: Vec<(u32, f64)>,
    /// The state that the project keeps for the plugin (base 64), if it keeps one.
    pub state: Option<String>,
    /// What the program's environment is given beside the editor's own (where the libraries that the plugin library needs are looked for).
    pub environment: Vec<(String, String)>,
    /// No window at all (for the tests: CI has no screen).
    pub headless: bool,
}

/// What a window did, or what became of it.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The window is up.
    Ready {
        width: u32,
        height: u32,
        can_resize: bool,
        embedded: bool,
        has_state: bool,
    },
    /// What a person does in the window: a step of a gesture on a parameter.
    Edit(ParameterEdit),
    /// The plugin's state is not what was last said.
    StateChanged,
    /// The state of the plugin (base 64): the answer to [`EditorWindows::ask_for_the_state`].
    State(String),
    ParametersChanged,
    /// The window is closed (by a person, by the plugin, or because the editor said so): the program has ended.
    Closed,
    /// The window could not be made, or the program ended without saying why (it crashed): why.
    Failed(String),
}

/// What the thread that reads a program's lines hands over.
enum Said {
    Message(FromProgram),
    /// The program's output ended: it is gone.
    Ended,
}

/// A program that is showing a window.
struct Running {
    child: Child,
    input: ChildStdin,
    said: Receiver<Said>,
    /// What the program was last told each parameter is, or told by its window: so that a value is not told again to a window that has it.
    known: HashMap<u32, f64>,
    /// When the program was told to close, if it was.
    told_to_close_at: Option<Instant>,
    /// Whether what ended the program was said (a close or a failure).
    ended_with_a_word: bool,
}

impl Running {
    fn tell(&mut self, message: &ToProgram) {
        // (A program that has ended cannot be told anything: its end is what the reading thread reports.)
        let _ = writeln!(self.input, "{}", line_of(message));
        let _ = self.input.flush();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The windows that are open.
#[derive(Default)]
pub struct EditorWindows {
    program: Option<PathBuf>,
    /// Whether the program was found to have been built from the same code as the editor.
    program_is_checked: bool,
    running: HashMap<WindowKey, Running>,
}

impl EditorWindows {
    pub fn new() -> Self {
        Self::default()
    }

    /// The same, with the program that shows the windows given (the tests use the one that cargo built).
    pub fn with_program(program: PathBuf) -> Self {
        Self {
            program: Some(program),
            ..Self::default()
        }
    }

    /// Whether the window `key` is open (or is being opened).
    pub fn is_open(&self, key: WindowKey) -> bool {
        self.running.contains_key(&key)
    }

    /// The windows that are open.
    pub fn keys(&self) -> Vec<WindowKey> {
        self.running.keys().copied().collect()
    }

    /// Opens the window `key` for `what`. The window is up when [`WindowEvent::Ready`] comes. Nothing is done if it is open already. An error says why the program could not be
    /// started.
    pub fn open(&mut self, key: WindowKey, what: &WhatToOpen) -> Result<(), String> {
        if self.running.contains_key(&key) {
            return Ok(());
        }
        let program = self.program_to_use()?;
        let mut command = background_command(&program);
        command
            .envs(what.environment.iter().map(|(name, value)| (name, value)))
            .arg("--library")
            .arg(&what.library)
            .args([
                "--id",
                &what.plugin_id,
                "--kind",
                what.kind.word(),
                "--title",
                &what.title,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if what.headless {
            command.arg("--headless");
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("the program that shows plugin windows could not be started: {error}"))?;
        let input = child.stdin.take().expect("the input was piped");
        let output = child.stdout.take().expect("the output was piped");
        let (sender, said) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Some(message) = message_in::<FromProgram>(&line) {
                    let _ = sender.send(Said::Message(message));
                }
            }
            let _ = sender.send(Said::Ended);
        });
        let mut running = Running {
            child,
            input,
            said,
            known: what.values.iter().copied().collect(),
            told_to_close_at: None,
            ended_with_a_word: false,
        };
        running.tell(&ToProgram::Begin {
            values: what.values.clone(),
            state: what.state.clone(),
        });
        self.running.insert(key, running);
        Ok(())
    }

    /// The program that shows windows, checked once to be built from the code that the editor is.
    fn program_to_use(&mut self) -> Result<PathBuf, String> {
        let program = match &self.program {
            Some(program) => program.clone(),
            None => {
                let found = editor_program_beside_this_program().ok_or_else(|| {
                    format!("the program that shows plugin windows ({EDITOR_PROGRAM_NAME}) is not beside the editor")
                })?;
                self.program = Some(found.clone());
                found
            }
        };
        if !self.program_is_checked {
            check_that_a_program_is_of_this_build(&program)?;
            self.program_is_checked = true;
        }
        Ok(program)
    }

    /// Everything that the windows did since the last call, in order, and what became of those that ended: a window that is closed or has failed is not open any more.
    pub fn poll(&mut self) -> Vec<(WindowKey, WindowEvent)> {
        let mut events = Vec::new();
        let mut gone = Vec::new();
        for (key, running) in &mut self.running {
            while let Ok(said) = running.said.try_recv() {
                match said {
                    Said::Message(message) => {
                        let (event, ends) = event_of(message);
                        running.ended_with_a_word |= ends;
                        if let WindowEvent::Edit(ParameterEdit::Value { id, value }) = &event {
                            running.known.insert(*id, *value);
                        }
                        events.push((*key, event));
                    }
                    Said::Ended => {
                        if !running.ended_with_a_word {
                            let status = running.child.try_wait().ok().flatten();
                            let how = match status {
                                Some(status) => format!("it ended with {status}"),
                                None => "it ended".to_owned(),
                            };
                            events.push((
                                *key,
                                WindowEvent::Failed(format!("the program that shows the plugin's window ended without saying why ({how}): the plugin's window may have crashed")),
                            ));
                        }
                        gone.push(*key);
                    }
                }
            }
            // (The word that ends a window is the last that it says: it is gone, whether or not its program has finished ending.)
            if running.ended_with_a_word && !gone.contains(key) {
                gone.push(*key);
            }
            if let Some(since) = running.told_to_close_at
                && since.elapsed() > HOW_LONG_A_WINDOW_MAY_TAKE_TO_CLOSE
            {
                let _ = running.child.kill();
            }
        }
        for key in gone {
            self.running.remove(&key);
        }
        events
    }

    /// Tells the window `key` what these parameters are now: only those that are not what the window has (what it was last told, or what its own edit made them). Says whether any was
    /// told.
    pub fn tell_the_values(&mut self, key: WindowKey, values: &[(u32, f64)]) -> bool {
        let Some(running) = self.running.get_mut(&key) else {
            return false;
        };
        let changed: Vec<(u32, f64)> = values
            .iter()
            .copied()
            .filter(|(id, value)| running.known.get(id).map(|known| *known as f32) != Some(*value as f32))
            .collect();
        if changed.is_empty() {
            return false;
        }
        running.known.extend(changed.iter().copied());
        running.tell(&ToProgram::Values { values: changed });
        true
    }

    /// Asks the window for the plugin's state (the answer is [`WindowEvent::State`]).
    pub fn ask_for_the_state(&mut self, key: WindowKey) {
        if let Some(running) = self.running.get_mut(&key) {
            running.tell(&ToProgram::SaveState);
        }
    }

    /// Gives the window a state for the plugin (base 64).
    pub fn give_the_state(&mut self, key: WindowKey, state: &str) {
        if let Some(running) = self.running.get_mut(&key) {
            running.tell(&ToProgram::LoadState {
                state: state.to_owned(),
            });
        }
    }

    /// Tells the window to close. It is gone when [`WindowEvent::Closed`] comes (or, if the program does not end, after [`HOW_LONG_A_WINDOW_MAY_TAKE_TO_CLOSE`]).
    pub fn close(&mut self, key: WindowKey) {
        if let Some(running) = self.running.get_mut(&key)
            && running.told_to_close_at.is_none()
        {
            running.told_to_close_at = Some(Instant::now());
            running.tell(&ToProgram::Close);
        }
    }

    pub fn close_all(&mut self) {
        for key in self.keys() {
            self.close(key);
        }
    }
}

/// What a message of the program is as an event, and whether it is the word that ends the program.
fn event_of(message: FromProgram) -> (WindowEvent, bool) {
    match message {
        FromProgram::Ready {
            width,
            height,
            can_resize,
            embedded,
            has_state,
        } => (
            WindowEvent::Ready {
                width,
                height,
                can_resize,
                embedded,
                has_state,
            },
            false,
        ),
        FromProgram::Edit { edit } => (WindowEvent::Edit(edit), false),
        FromProgram::StateChanged => (WindowEvent::StateChanged, false),
        FromProgram::State { state } => (WindowEvent::State(state), false),
        FromProgram::ParametersChanged => (WindowEvent::ParametersChanged, false),
        FromProgram::Closed => (WindowEvent::Closed, true),
        FromProgram::Failed { reason } => (WindowEvent::Failed(reason), true),
    }
}

/// Asks the program which code it was built from, and says why not if it is not the editor's (a `cargo run` builds the editor and not the programs beside it).
fn check_that_a_program_is_of_this_build(program: &Path) -> Result<(), String> {
    let asked = background_command(program)
        .arg(BUILD_ID_REQUEST)
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("the program that shows plugin windows could not be run: {error}"))?;
    refuse_a_program_that_says(&String::from_utf8_lossy(&asked.stdout))
}

/// Whether what a program said when it was asked which code it was built from is this build (or says why not).
fn refuse_a_program_that_says(said: &str) -> Result<(), String> {
    match crate::plugin_scan::protocol::build_id_in(said) {
        Some(id) if id == BUILD_ID => Ok(()),
        Some(_) | None => Err(format!(
            "the program that shows plugin windows ({EDITOR_PROGRAM_NAME}) was built from other code than the editor: build them together (cargo build)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_scan::protocol::{BUILD_ID_PREFIX, build_id_line};

    #[test]
    fn a_program_is_used_only_if_it_says_that_it_was_built_from_this_code() {
        assert_eq!(refuse_a_program_that_says(&build_id_line()), Ok(()));
        let other = format!("{BUILD_ID_PREFIX}not-this-build");
        let error = refuse_a_program_that_says(&other).unwrap_err();
        assert!(error.contains("built from other code"), "{error}");
        assert!(refuse_a_program_that_says("").is_err(), "it said nothing");
    }
}
