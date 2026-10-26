// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor-window program (`fastforwardrs-plugin-editor`, phase 10 piece 10.2) as a real process, with `--headless` (no window: CI has no screen) and the test plugin that has a
//! window and a state. What is tried here is the protocol: the order of what the program says, what it does with what it is told, and how it ends.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap_test_plugins::{EDITOR, EDITOR_EDIT_VALUE, EDITOR_SIZE, GAIN};
use fastforwardrs::plugin_editor::protocol::{FromProgram, ParameterEdit, ToProgram, line_of, message_in};

const PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");
const HOW_LONG_TO_WAIT: Duration = Duration::from_secs(30);

/// A running editor-window program.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    said: Receiver<FromProgram>,
    /// The lines that were not messages: what the plugin printed.
    noise: Arc<Mutex<Vec<String>>>,
}

impl Running {
    fn start(library: &std::path::Path, id: &str, extra: &[&str]) -> Self {
        let mut child = Command::new(PROGRAM)
            .args(["--headless", "--library"])
            .arg(library)
            .args(["--id", id])
            .args(extra)
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

    fn of_the_test_plugin(id: &str) -> Self {
        Self::start(&clap_test_plugins::library_file_for_tests(), id, &[])
    }

    fn tell(&mut self, message: &ToProgram) {
        let stdin = self.stdin.as_mut().expect("the input is open");
        writeln!(stdin, "{}", line_of(message)).expect("the program is told");
        stdin.flush().expect("the program is told");
    }

    fn next(&self) -> FromProgram {
        self.said
            .recv_timeout(HOW_LONG_TO_WAIT)
            .expect("the program says something in time")
    }

    /// What the program says until `last` (which is the last of them).
    fn said_until(&self, last: impl Fn(&FromProgram) -> bool) -> Vec<FromProgram> {
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

    fn begin(&mut self, values: &[(u32, f64)], state: Option<&str>) {
        self.tell(&ToProgram::Begin {
            values: values.to_vec(),
            state: state.map(str::to_owned),
        });
    }

    fn exit_code(&mut self) -> Option<i32> {
        self.child.wait().expect("the program ends").code()
    }

    /// The state of the plugin, as the text that the test plugin saves.
    fn the_state(&mut self) -> String {
        self.tell(&ToProgram::SaveState);
        loop {
            if let FromProgram::State { state } = self.next() {
                return String::from_utf8(decode(&state)).expect("the state is text");
            }
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// (The editor's own base 64 is private: this is the little of it that the tests need.)
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let group = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for place in 0..4 {
            if place <= chunk.len() {
                text.push(ALPHABET[((group >> (18 - 6 * place)) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

fn decode(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let (mut group, mut bits) = (0_u32, 0);
    for letter in text.bytes().filter(|letter| *letter != b'=') {
        let value = ALPHABET
            .iter()
            .position(|candidate| *candidate == letter)
            .expect("base 64") as u32;
        group = (group << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((group >> bits) as u8);
            group &= (1 << bits) - 1;
        }
    }
    bytes
}

#[test]
fn the_program_says_the_window_is_up_and_then_what_the_hand_in_it_does() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[(1, 2.0)], None);

    let said = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    assert_eq!(
        said,
        vec![
            FromProgram::Ready {
                width: EDITOR_SIZE.0,
                height: EDITOR_SIZE.1,
                can_resize: true,
                embedded: false,
                has_state: true,
            },
            FromProgram::Edit {
                edit: ParameterEdit::Begin { id: 1 }
            },
            FromProgram::Edit {
                edit: ParameterEdit::Value {
                    id: 1,
                    value: EDITOR_EDIT_VALUE
                }
            },
            FromProgram::Edit {
                edit: ParameterEdit::End { id: 1 }
            },
            FromProgram::StateChanged,
            FromProgram::ParametersChanged,
        ]
    );
}

#[test]
fn the_values_and_the_state_that_the_editor_begins_with_are_what_the_window_is_made_with() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[(1, 2.0)], Some(&encode(b"edits=7")));
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    let state = program.the_state();

    // (The state gave 7 edits, before the window was made; the window made one more when it was shown.)
    assert!(state.contains("edits=8"), "{state}");
    assert!(state.contains("created=1"), "{state}");
    assert!(state.contains("shown=1"), "{state}");
}

#[test]
fn a_state_that_the_editor_gives_later_is_loaded_and_the_state_is_said_when_it_is_asked_for() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    program.tell(&ToProgram::LoadState {
        state: encode(b"edits=40"),
    });
    program.tell(&ToProgram::Values { values: vec![(1, 3.0)] });

    assert!(program.the_state().contains("edits=40"));
}

#[test]
fn closing_ends_the_program_after_it_says_that_the_window_is_closed() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    program.tell(&ToProgram::Close);

    // (The plugin said that its state changed, and the editor did not ask for it: it is said before the end, so that what a person did last is not lost.)
    assert!(
        matches!(program.next(), FromProgram::State { .. }),
        "the state that changed is said before the end"
    );
    assert_eq!(program.next(), FromProgram::Closed);
    assert_eq!(program.exit_code(), Some(0));
}

#[test]
fn closing_after_the_state_was_asked_for_says_only_that_the_window_is_closed() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::StateChanged));
    let _ = program.the_state();

    program.tell(&ToProgram::Close);

    assert_eq!(
        program.next(),
        FromProgram::Closed,
        "it was asked for, and has not changed since"
    );
    assert_eq!(program.exit_code(), Some(0));
}

#[test]
fn when_the_editor_goes_away_the_window_does_too() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    drop(program.stdin.take());

    assert!(
        matches!(program.next(), FromProgram::State { .. }),
        "the state that changed is said first"
    );
    assert_eq!(program.next(), FromProgram::Closed);
    assert_eq!(program.exit_code(), Some(0));
}

#[test]
fn a_plugin_that_the_library_does_not_have_or_that_has_no_window_is_a_failure_that_says_why_and_ends_the_program() {
    for (id, why) in [
        ("org.fastforwardrs.test.not_there", "could not be made"),
        (GAIN, "no window"),
    ] {
        let mut program = Running::of_the_test_plugin(id);
        match program.next() {
            FromProgram::Failed { reason } => assert!(reason.contains(why), "{id}: {reason}"),
            other => panic!("{id}: {other:?}"),
        }
        assert_eq!(program.exit_code(), Some(1), "{id}");
    }
}

#[test]
fn a_library_that_is_not_there_is_a_failure_that_says_so() {
    let mut program = Running::start(std::path::Path::new("no-such-folder/no-such-library.clap"), EDITOR, &[]);

    match program.next() {
        FromProgram::Failed { reason } => assert!(reason.contains("could not be loaded"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(program.exit_code(), Some(1));
}

#[test]
fn the_program_says_how_to_use_it_when_it_is_used_wrongly_and_what_it_was_built_from_when_it_is_asked() {
    let wrong = Command::new(PROGRAM).arg("--what").output().expect("the program runs");
    assert_eq!(wrong.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("Usage: fastforwardrs-plugin-editor"));

    let asked = Command::new(PROGRAM)
        .arg("--build-id")
        .output()
        .expect("the program runs");
    assert!(asked.status.success());
    assert_eq!(
        fastforwardrs::plugin_scan::protocol::build_id_in(&String::from_utf8_lossy(&asked.stdout)),
        Some(fastforwardrs::plugin_scan::protocol::BUILD_ID.to_owned())
    );
}

#[test]
fn the_values_that_the_editor_begins_with_are_what_the_plugin_has_when_its_window_is_shown_and_later_values_are_what_it_has_after()
 {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[(1, 2.5)], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    let state = program.the_state();
    assert!(
        state.contains("gain_when_shown=2.5"),
        "told before the window was made: {state}"
    );
    assert!(
        state.contains("gain=0.75"),
        "and then the window made its edit: {state}"
    );

    program.tell(&ToProgram::Values { values: vec![(1, 3.0)] });
    assert!(program.the_state().contains("gain=3"), "a later value is told at once");
}

#[test]
fn a_state_that_is_not_base_64_is_not_given_to_the_plugin_and_does_not_end_the_program() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], Some("this is not base 64!!"));
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    program.tell(&ToProgram::LoadState {
        state: "%%%".to_owned(),
    });

    assert!(
        program.the_state().contains("edits=1"),
        "only the edit that the window made"
    );
}

#[test]
fn noise_on_the_input_is_thrown_away() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));
    let stdin = program.stdin.as_mut().unwrap();
    writeln!(stdin, "hello").unwrap();
    writeln!(stdin, "ffrs-editor: {{not json").unwrap();
    stdin.flush().unwrap();

    assert!(program.the_state().contains("shown=1"), "the program goes on");
}

#[test]
fn the_window_is_destroyed_before_the_program_ends_and_what_a_plugin_prints_is_not_taken_for_a_message() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], None);
    let _ = program.said_until(|message| matches!(message, FromProgram::ParametersChanged));

    program.tell(&ToProgram::Close);
    assert!(
        matches!(program.next(), FromProgram::State { .. }),
        "the state that changed is said first"
    );
    assert_eq!(program.next(), FromProgram::Closed);
    assert_eq!(program.exit_code(), Some(0));

    // (The plugin prints a line when its window is destroyed; that line was before the program said that it was closed, and it was not a message.)
    std::thread::sleep(Duration::from_millis(200));
    let noise = program.noise.lock().unwrap().clone();
    assert_eq!(noise, vec!["clap-test-editor: the window was destroyed".to_owned()]);
}

#[test]
fn a_plugin_that_cannot_make_its_window_is_a_failure_that_ends_the_program() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], Some(&encode(b"ask=fail_create")));

    match program.next() {
        FromProgram::Failed { reason } => assert!(reason.contains("could not make its window"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(program.exit_code(), Some(1));
}

#[test]
fn a_plugin_that_closes_its_own_window_ends_the_program_without_being_told_to() {
    let mut program = Running::of_the_test_plugin(EDITOR);
    program.begin(&[], Some(&encode(b"ask=close")));

    let said = program.said_until(|message| matches!(message, FromProgram::Closed));

    assert!(matches!(said.first(), Some(FromProgram::Ready { .. })), "{said:?}");
    assert_eq!(program.exit_code(), Some(0));
}
