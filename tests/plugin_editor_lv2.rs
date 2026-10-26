// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The windows of LV2 plugins (phase 11, piece 11.5): the editor-window program as a real process, with `--headless --kind lv2` (no window: CI has no screen) and the user interfaces
//! of the LV2 test plugins, and the editor's side of the windows (`EditorWindows`) with that program. What is tried is the protocol: the order of what the program says, what it does with
//! what it is told, what the interface of the plugin is told and says, and how it ends. (A real window is `plugin_editor_lv2_real_window.rs`.)

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fastforwardrs::plugin_editor::program::PluginKind;
use fastforwardrs::plugin_editor::protocol::{FromProgram, ParameterEdit, ToProgram, line_of, message_in};
use fastforwardrs::plugin_editor::sessions::{EditorWindows, WhatToOpen, WindowEvent, WindowKey};
use fastforwardrs::project::{AnimationTarget, ClipId, EffectInstanceId, TrackId};
#[cfg(windows)]
use lv2_test_plugins::UI_REAL_WINDOW_SIZE;
use lv2_test_plugins::{CONTROLS, FAILS, GAIN, UI_CLOSE_VALUE, UI_EDIT_VALUE, UI_SIZE};
use tempfile::TempDir;

const PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");
const HOW_LONG_TO_WAIT: Duration = Duration::from_secs(30);

/// The port of the gain plugin that is its gain.
const GAIN_PORT: u32 = 2;

/// The bundle of the test plugins, in a folder of its own that is kept as long as this is.
struct TheBundle {
    _folder: TempDir,
    bundle: PathBuf,
}

fn the_bundle() -> TheBundle {
    let folder = tempfile::tempdir().unwrap();
    let bundle = lv2_test_plugins::write_bundle(folder.path());
    TheBundle {
        _folder: folder,
        bundle,
    }
}

/// A running editor-window program, for an LV2 plugin.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    said: Receiver<FromProgram>,
    /// The lines that were not messages: what the interface printed.
    noise: Arc<Mutex<Vec<String>>>,
}

impl Running {
    fn start(bundle: &Path, uri: &str) -> Self {
        Self::start_with_or_without_a_window(bundle, uri, true)
    }

    fn start_with_or_without_a_window(bundle: &Path, uri: &str, headless: bool) -> Self {
        let mut command = Command::new(PROGRAM);
        if headless {
            command.arg("--headless");
        }
        let mut child = command
            .args(["--kind", "lv2", "--library"])
            .arg(bundle)
            .args(["--id", uri])
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

    fn begin(&mut self, values: &[(u32, f64)], state: Option<&str>) {
        self.tell(&ToProgram::Begin {
            values: values.to_vec(),
            state: state.map(str::to_owned),
        });
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

    fn exit_code(&mut self) -> Option<i32> {
        self.child.wait().expect("the program ends").code()
    }

    /// The lines that the interface printed, once one has `needle` in it (the interface prints as the program does what it does: a moment may pass).
    fn noise_until(&self, needle: &str) -> Vec<String> {
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
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ready_of_the_gain_window() -> FromProgram {
    FromProgram::Ready {
        width: UI_SIZE.0,
        height: UI_SIZE.1,
        can_resize: true,
        embedded: false,
        has_state: false,
    }
}

// --- the program ---

#[test]
fn the_window_of_an_lv2_plugin_is_made_after_the_values_are_told_says_how_big_it_is_and_makes_its_edit_as_a_gesture() {
    let bundle = the_bundle();
    let mut program = Running::start(&bundle.bundle, GAIN);

    program.begin(&[(GAIN_PORT, 1.5)], None);

    let said = program.said_until(|message| {
        matches!(
            message,
            FromProgram::Edit {
                edit: ParameterEdit::End { .. }
            }
        )
    });
    assert_eq!(
        said,
        vec![
            ready_of_the_gain_window(),
            FromProgram::Edit {
                edit: ParameterEdit::Begin { id: GAIN_PORT }
            },
            FromProgram::Edit {
                edit: ParameterEdit::Value {
                    id: GAIN_PORT,
                    value: f64::from(UI_EDIT_VALUE)
                }
            },
            FromProgram::Edit {
                edit: ParameterEdit::End { id: GAIN_PORT }
            },
        ],
        "the size it asked for, and then the hand: a write that was not a number was not said"
    );
    let noise = program.noise_until("port_event");
    let made = noise
        .iter()
        .find(|line| line.contains("made"))
        .expect("it said what it was given");
    assert!(
        made.contains("parent=none"),
        "headless: the parent feature is there and has nothing: {made}"
    );
    assert!(made.contains("resize=true") && made.contains("touch=true"), "{made}");
    assert!(
        made.contains("gain_port=2"),
        "ui:portMap gave the port of the symbol: {made}"
    );
    assert!(
        made.contains("missing_port=4294967295"),
        "and the number that means that there is none for a symbol that is not a port's: {made}"
    );
    assert!(made.contains("idle=true"), "the idle interface was given: {made}");
    assert!(
        noise
            .iter()
            .any(|line| line.contains("options sample_rate=48000 scale_factor=1")),
        "the options say the sample rate and the scale of the screen: {noise:?}"
    );
    assert!(
        noise.iter().any(|line| line.contains("urid round_trip=true")),
        "what urid:unmap says is what urid:map was given: {noise:?}"
    );
    assert!(
        noise.iter().any(|line| line.contains("resize answers=Some([1, 1, 0])")),
        "ui:resize refuses what is not a size and takes a size: {noise:?}"
    );
    assert!(made.contains("bundle_ends_with_a_separator=true"), "{made}");
    assert!(made.contains(GAIN), "{made}");
    assert!(
        noise.iter().any(|line| line.contains("port_event 2 1.5")),
        "the value that was told before the window was there was given to it as it was made: {noise:?}"
    );
}

#[test]
fn what_the_project_says_reaches_the_interface_and_the_value_that_closes_it_ends_the_program() {
    let bundle = the_bundle();
    let mut program = Running::start(&bundle.bundle, GAIN);
    program.begin(&[], None);
    program.said_until(|message| {
        matches!(
            message,
            FromProgram::Edit {
                edit: ParameterEdit::End { .. }
            }
        )
    });

    program.tell(&ToProgram::Values {
        values: vec![(GAIN_PORT, 2.5)],
    });
    program.noise_until("port_event 2 2.5");
    program.tell(&ToProgram::Values {
        values: vec![(GAIN_PORT, f64::from(UI_CLOSE_VALUE))],
    });

    assert_eq!(program.next(), FromProgram::Closed, "the interface asked to be closed");
    assert_eq!(program.exit_code(), Some(0));
    assert!(
        program
            .noise_until("cleanup")
            .iter()
            .any(|line| line.contains("cleanup")),
        "its window was taken down before the program ended"
    );
}

#[test]
fn closing_it_takes_the_interface_down_and_the_program_ends() {
    let bundle = the_bundle();
    let mut program = Running::start(&bundle.bundle, GAIN);
    program.begin(&[], None);
    program.said_until(|message| {
        matches!(
            message,
            FromProgram::Edit {
                edit: ParameterEdit::End { .. }
            }
        )
    });

    program.tell(&ToProgram::Close);

    assert_eq!(program.next(), FromProgram::Closed);
    assert_eq!(program.exit_code(), Some(0));
    program.noise_until("cleanup");
}

#[test]
fn a_plugin_that_has_no_window_or_whose_window_does_not_come_or_that_is_not_there_is_a_failure_that_says_why() {
    let bundle = the_bundle();
    for (uri, what) in [
        (CONTROLS, "has no window"),
        (FAILS, "gave nothing"),
        ("urn:not:there", "has no plugin called"),
    ] {
        let mut program = Running::start(&bundle.bundle, uri);
        program.begin(&[], None);
        match program
            .said_until(|message| matches!(message, FromProgram::Failed { .. }))
            .last()
        {
            Some(FromProgram::Failed { reason }) => {
                assert!(reason.contains(what), "{uri}: {reason}");
                if uri == CONTROLS {
                    assert_eq!(reason, "the plugin has no window", "said before any window was tried");
                }
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(program.exit_code(), Some(1), "{uri}");
    }
    let program = Running::start(Path::new("a-folder-that-is-not-there"), GAIN);
    match program
        .said_until(|message| matches!(message, FromProgram::Failed { .. }))
        .last()
    {
        Some(FromProgram::Failed { reason }) => assert!(reason.contains("manifest"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_window_of_an_lv2_plugin_has_no_state_of_its_own_and_gives_back_the_one_it_was_given() {
    let bundle = the_bundle();
    let mut program = Running::start(&bundle.bundle, GAIN);
    program.begin(&[], Some("QUJD"));
    program.said_until(|message| {
        matches!(
            message,
            FromProgram::Edit {
                edit: ParameterEdit::End { .. }
            }
        )
    });

    program.tell(&ToProgram::SaveState);

    assert_eq!(
        program.next(),
        FromProgram::State {
            state: "QUJD".to_owned()
        },
        "what the project gave is what it says"
    );
    program.tell(&ToProgram::LoadState {
        state: "REVG".to_owned(),
    });
    program.tell(&ToProgram::SaveState);
    assert_eq!(
        program.next(),
        FromProgram::State {
            state: "REVG".to_owned()
        }
    );
}

/// **A real window**, where there is a screen to make one on (Windows: the program makes a window of its own, the interface makes its window in it, and the size is what the system says
/// of that window).
#[cfg(windows)]
#[test]
fn on_windows_the_interface_is_made_in_a_real_window_of_the_program_and_the_hand_in_it_is_heard() {
    let bundle = the_bundle();
    let mut program = Running::start_with_or_without_a_window(&bundle.bundle, GAIN, false);
    program.begin(&[(GAIN_PORT, 1.0)], None);

    let said = program.said_until(|message| {
        matches!(
            message,
            FromProgram::Edit {
                edit: ParameterEdit::End { .. }
            } | FromProgram::Failed { .. }
        )
    });

    assert_eq!(
        said.first(),
        Some(&FromProgram::Ready {
            width: UI_REAL_WINDOW_SIZE.0,
            height: UI_REAL_WINDOW_SIZE.1,
            can_resize: true,
            embedded: true,
            has_state: false
        }),
        "the size that the system says the interface's window is, and not the size that the interface asked for: {said:?}"
    );
    assert!(
        said.contains(&FromProgram::Edit {
            edit: ParameterEdit::Value {
                id: GAIN_PORT,
                value: f64::from(UI_EDIT_VALUE)
            }
        }),
        "{said:?}"
    );
    let noise = program.noise_until("made");
    assert!(
        noise.iter().any(|line| line.contains("parent=given")),
        "the interface was given a window to be in: {noise:?}"
    );
    program.tell(&ToProgram::Close);
    assert_eq!(
        program
            .said_until(|message| matches!(message, FromProgram::Closed))
            .len(),
        1
    );
    assert_eq!(program.exit_code(), Some(0));
}

/// **For a person to look at**: opens the window of the gain plugin's interface where there is a screen, and keeps it for `FFRS_LOOK_SECONDS` seconds (20 if not said). Run by hand with
/// `cargo test --test plugin_editor_lv2 -- --ignored look_at_the_window`.
#[test]
#[ignore = "opens a window and waits: run by hand"]
fn look_at_the_window_of_the_test_interface() {
    let seconds = std::env::var("FFRS_LOOK_SECONDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(20);
    let bundle = the_bundle();
    let mut program = Running::start_with_or_without_a_window(&bundle.bundle, GAIN, false);
    program.begin(&[(GAIN_PORT, 1.0)], None);
    let ready = program.next();
    println!("the program says: {ready:?}");
    std::thread::sleep(Duration::from_secs(seconds));
    program.tell(&ToProgram::Close);
}

// --- the editor's side ---

fn the_key() -> WindowKey {
    WindowKey {
        target: AnimationTarget::Clip(ClipId::new(TrackId::new(0), 0)),
        instance: EffectInstanceId::FIRST,
    }
}

fn what_to_open(bundle: &Path, values: &[(u32, f64)]) -> WhatToOpen {
    WhatToOpen {
        library: bundle.to_owned(),
        plugin_id: GAIN.to_owned(),
        kind: PluginKind::Lv2,
        title: "a window".to_owned(),
        values: values.to_vec(),
        state: None,
        environment: Vec::new(),
        headless: true,
    }
}

fn polled_until(windows: &mut EditorWindows, done: impl Fn(&[WindowEvent]) -> bool) -> Vec<WindowEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        events.extend(windows.poll().into_iter().map(|(_, event)| event));
        if done(&events) {
            return events;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("it did not come in time: {events:?}");
}

#[test]
fn the_editor_opens_the_window_of_an_lv2_plugin_hears_its_edit_tells_it_values_and_closes_it() {
    let bundle = the_bundle();
    let mut windows = EditorWindows::with_program(PathBuf::from(PROGRAM));
    windows
        .open(the_key(), &what_to_open(&bundle.bundle, &[(GAIN_PORT, 1.0)]))
        .expect("the program starts");

    let events = polled_until(&mut windows, |events| {
        events
            .iter()
            .any(|event| matches!(event, WindowEvent::Edit(ParameterEdit::End { .. })))
    });

    assert_eq!(
        events,
        vec![
            WindowEvent::Ready {
                width: UI_SIZE.0,
                height: UI_SIZE.1,
                can_resize: true,
                embedded: false,
                has_state: false
            },
            WindowEvent::Edit(ParameterEdit::Begin { id: GAIN_PORT }),
            WindowEvent::Edit(ParameterEdit::Value {
                id: GAIN_PORT,
                value: f64::from(UI_EDIT_VALUE)
            }),
            WindowEvent::Edit(ParameterEdit::End { id: GAIN_PORT }),
        ]
    );
    assert!(windows.is_open(the_key()));
    assert!(
        !windows.tell_the_values(the_key(), &[(GAIN_PORT, f64::from(UI_EDIT_VALUE))]),
        "the window has what its own edit made: it is not told again"
    );
    assert!(windows.tell_the_values(the_key(), &[(GAIN_PORT, f64::from(UI_CLOSE_VALUE))]));
    let events = polled_until(&mut windows, |events| events.contains(&WindowEvent::Closed));
    assert!(
        events.contains(&WindowEvent::Closed),
        "the value that closes it closed it"
    );
    assert!(!windows.is_open(the_key()));

    windows
        .open(the_key(), &what_to_open(&bundle.bundle, &[]))
        .expect("it opens again");
    polled_until(&mut windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::Ready { .. }))
    });
    windows.close(the_key());
    polled_until(&mut windows, |events| events.contains(&WindowEvent::Closed));
}

#[test]
fn a_window_that_cannot_be_made_is_a_failure_in_the_editor_with_the_reason() {
    let bundle = the_bundle();
    let mut windows = EditorWindows::with_program(PathBuf::from(PROGRAM));
    let mut what = what_to_open(&bundle.bundle, &[]);
    what.plugin_id = FAILS.to_owned();
    windows.open(the_key(), &what).expect("the program starts");

    let events = polled_until(&mut windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::Failed(_)))
    });

    assert!(
        matches!(events.last(), Some(WindowEvent::Failed(reason)) if reason.contains("gave nothing")),
        "{events:?}"
    );
    assert!(!windows.is_open(the_key()));
}
