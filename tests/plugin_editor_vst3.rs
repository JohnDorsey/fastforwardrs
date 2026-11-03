// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The windows of VST3 plugins (piece 15.6 of `docs/engine-phase-15-vst3-design.md`): the editor-window program as a real process, with `--headless --kind vst3` (no window of the
//! program: CI has no screen) and the window of the VST3 test plugins, and the editor's side of the windows (`EditorWindows`) with that program. What is tried is the protocol: the
//! order of what the program says, what it does with what it is told, what the plugin's window is told and says, the state it saves after a hand has changed something, and how
//! it ends. The test window does what a key says (`FFRS_TEST_VIEW_KEYS`, pressed when it is attached: `e` an edit, `r` a request for another size, `b` one it does not allow, `d`
//! "the state changed", `v` "the values changed") and says what it was told on the standard output (`FFRS_TEST_VIEW_PRINT`). A real window is the last test of the file.

mod editor_program;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use editor_program::{PROGRAM, Running};
use fastforwardrs::base64;
use fastforwardrs::plugin_editor::program::PluginKind;
use fastforwardrs::plugin_editor::protocol::{FromProgram, ParameterEdit, ToProgram};
use fastforwardrs::plugin_editor::sessions::{EditorWindows, WhatToOpen, WindowEvent, WindowKey};
use fastforwardrs::project::{AnimationTarget, ClipId, EffectInstanceId, TrackId};
use tempfile::TempDir;
use vst3_host::{ClassId, Instance, Module};
use vst3_test_plugins as plugins;

const GAIN: u32 = plugins::PARAMETER_GAIN;

/// The module of the test plugins, in a folder of its own that is kept as long as this is.
struct TheModule {
    _folder: TempDir,
    module: PathBuf,
}

fn the_module() -> TheModule {
    let folder = tempfile::tempdir().unwrap();
    let module = plugins::write_module(folder.path());
    TheModule {
        _folder: folder,
        module,
    }
}

fn class_text(numbers: [u32; 4]) -> String {
    ClassId::from_numbers(numbers[0], numbers[1], numbers[2], numbers[3]).text()
}

fn start_with(module: &Path, class: [u32; 4], keys: &str, headless: bool) -> Running {
    Running::start(
        "vst3",
        module,
        &class_text(class),
        headless,
        &[("FFRS_TEST_VIEW_KEYS", keys), ("FFRS_TEST_VIEW_PRINT", "1")],
    )
}

fn start(module: &Path, keys: &str) -> Running {
    start_with(module, plugins::GAIN, keys, true)
}

/// What the gain effect has as its gain once it has been given `state` (base 64): a plugin of its own is made and given it.
fn gain_in_the_state(module: &Path, state: &str) -> f64 {
    let module = Module::open(module).expect("the module opens");
    let class = ClassId::from_numbers(plugins::GAIN[0], plugins::GAIN[1], plugins::GAIN[2], plugins::GAIN[3]);
    let (mut instance, _) = Instance::new(&module, class, 48_000.0, 512, None).expect("the plugin is made");
    instance
        .restore_state(&base64::decode(state).expect("the state is base 64"))
        .expect("the state is taken");
    instance.plain_value(GAIN).expect("the plugin has a controller")
}

/// What the hidden value `Secret` of the gain effect is in `state` (base 64): the value that a message to the component changes without a word to the host.
fn secret_in_the_state(module: &Path, state: &str) -> f64 {
    let module = Module::open(module).expect("the module opens");
    let class = ClassId::from_numbers(plugins::GAIN[0], plugins::GAIN[1], plugins::GAIN[2], plugins::GAIN[3]);
    let (mut instance, _) = Instance::new(&module, class, 48_000.0, 512, None).expect("the plugin is made");
    instance
        .restore_state(&base64::decode(state).expect("the state is base 64"))
        .expect("the state is taken");
    instance
        .plain_value(plugins::PARAMETER_SECRET)
        .expect("the plugin has a controller")
}

/// The state of a gain effect whose gain is `gain`, as base 64: made as a project would have it (a plugin is given a value and a block, and saves).
fn state_with_the_gain(module: &Path, gain: f64) -> String {
    let module = Module::open(module).expect("the module opens");
    let class = ClassId::from_numbers(plugins::GAIN[0], plugins::GAIN[1], plugins::GAIN[2], plugins::GAIN[3]);
    let (mut instance, _) = Instance::new(&module, class, 48_000.0, 512, None).expect("the plugin is made");
    instance.set_value(GAIN, gain, 0);
    let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
    instance
        .run(&mut [], &mut [&mut left, &mut right], 64)
        .expect("a block is run");
    base64::encode(&instance.save_state().expect("the state is saved"))
}

fn ready_of_the_test_window() -> FromProgram {
    FromProgram::Ready {
        width: plugins::VIEW_WIDTH as u32,
        height: plugins::VIEW_HEIGHT as u32,
        can_resize: true,
        embedded: false,
        has_state: true,
    }
}

fn the_gesture_of_the_edit() -> Vec<FromProgram> {
    vec![
        FromProgram::Edit {
            edit: ParameterEdit::Begin { id: GAIN },
        },
        FromProgram::Edit {
            edit: ParameterEdit::Value { id: GAIN, value: 3.0 },
        },
        FromProgram::Edit {
            edit: ParameterEdit::End { id: GAIN },
        },
    ]
}

// --- the program ---

#[test]
fn the_window_of_a_vst3_plugin_is_made_after_the_values_are_told_says_how_big_it_is_and_makes_its_edit_as_a_gesture() {
    let module = the_module();
    let mut program = start(&module.module, "e");

    program.begin(&[(GAIN, 2.0)], None);

    let said = program.said_until_the_end_of_a_gesture();
    let mut expected = vec![ready_of_the_test_window()];
    expected.extend(the_gesture_of_the_edit());
    assert_eq!(
        said, expected,
        "the size, and then the hand (as plain values, not 0.75)"
    );
    let noise = program.noise_until("attached");
    assert!(
        noise
            .iter()
            .any(|line| line.contains("attached parent=1 ") && !line.contains("parent=0")),
        "headless: a parent that is not a window, and not nothing: {noise:?}"
    );
    assert!(
        noise.iter().any(|line| line.contains("setFrame given")),
        "the plugin's window was given a frame to ask for another size with: {noise:?}"
    );
}

#[test]
fn what_the_hand_changed_is_in_the_state_that_the_window_saves() {
    let module = the_module();
    let mut program = start(&module.module, "e");
    program.begin(&[(GAIN, 2.0)], None);
    program.said_until_the_end_of_a_gesture();

    program.tell(&ToProgram::SaveState);

    match program.next() {
        FromProgram::State { state } => assert_eq!(
            gain_in_the_state(&module.module, &state),
            3.0,
            "the gain the hand made, and not the 2 that the project told: the component heard the edit"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_values_the_project_says_reach_the_plugin_and_are_not_said_back_as_edits() {
    let module = the_module();
    let mut program = start(&module.module, "");
    program.begin(&[(GAIN, 2.0)], None);
    assert_eq!(program.next(), ready_of_the_test_window());

    program.tell(&ToProgram::Values {
        values: vec![(GAIN, 1.5)],
    });
    program.tell(&ToProgram::SaveState);

    match program.next() {
        FromProgram::State { state } => assert_eq!(gain_in_the_state(&module.module, &state), 1.5),
        other => panic!("nothing but the state was to be said: {other:?}"),
    }
}

#[test]
fn a_plugin_that_says_back_every_value_it_is_given_as_an_edit_of_its_own_is_not_heard_as_a_hand() {
    // (Surge XT Effects made edits that nobody made. The editor would take them for a person's, which would end what can be redone: the window says nothing of what the project told it.)
    let module = the_module();
    let mut program = Running::start(
        "vst3",
        &module.module,
        &class_text(plugins::GAIN),
        true,
        &[("FFRS_TEST_PLUGIN", "echo_values")],
    );
    program.begin(&[(GAIN, 2.0)], None);
    assert_eq!(program.next(), ready_of_the_test_window());

    program.tell(&ToProgram::Values {
        values: vec![(GAIN, 1.5)],
    });
    program.tell(&ToProgram::SaveState);

    match program.next() {
        FromProgram::State { state } => assert_eq!(gain_in_the_state(&module.module, &state), 1.5),
        other => panic!("what the plugin said back was not to be said: {other:?}"),
    }
    assert!(
        program.says_nothing_for(Duration::from_millis(1200)),
        "and it was not said a moment later either"
    );
}

#[test]
fn a_state_the_project_gave_is_taken_and_the_values_that_come_with_it_are_after_it() {
    let module = the_module();
    let state = state_with_the_gain(&module.module, 2.5);

    let mut only_the_state = start(&module.module, "");
    only_the_state.begin(&[], Some(&state));
    assert_eq!(only_the_state.next(), ready_of_the_test_window());
    only_the_state.tell(&ToProgram::SaveState);
    match only_the_state.next() {
        FromProgram::State { state } => assert_eq!(gain_in_the_state(&module.module, &state), 2.5),
        other => panic!("{other:?}"),
    }

    let mut both = start(&module.module, "");
    both.begin(&[(GAIN, 1.0)], Some(&state));
    assert_eq!(both.next(), ready_of_the_test_window());
    both.tell(&ToProgram::SaveState);
    match both.next() {
        FromProgram::State { state } => assert_eq!(
            gain_in_the_state(&module.module, &state),
            1.0,
            "the value is told after the state, as the plugin that makes the sound is"
        ),
        other => panic!("{other:?}"),
    }

    let mut changed = start(&module.module, "");
    changed.begin(&[], None);
    assert_eq!(changed.next(), ready_of_the_test_window());
    changed.tell(&ToProgram::LoadState { state });
    changed.tell(&ToProgram::SaveState);
    match changed.next() {
        FromProgram::State { state } => assert_eq!(gain_in_the_state(&module.module, &state), 2.5),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_request_for_another_size_is_answered_and_a_size_the_window_does_not_allow_is_brought_inside_what_it_does() {
    let module = the_module();
    let mut asked = start(&module.module, "r");
    asked.begin(&[], None);
    assert_eq!(asked.next(), ready_of_the_test_window());
    let (width, height) = plugins::VIEW_ASKED;
    asked.noise_until(&format!("onSize {width}x{height}"));

    let mut too_much = start(&module.module, "b");
    too_much.begin(&[], None);
    assert_eq!(too_much.next(), ready_of_the_test_window());
    let (width, height) = plugins::VIEW_MOST;
    too_much.noise_until(&format!("onSize {width}x{height}"));
    assert!(
        !too_much
            .noise()
            .iter()
            .any(|line| line.contains("2000") || line.contains("1500")),
        "it was told the size it can be, never the one it asked for: {:?}",
        too_much.noise()
    );
}

#[test]
fn a_plugin_that_says_its_state_changed_is_heard() {
    let module = the_module();
    let mut program = start(&module.module, "d");

    program.begin(&[], None);

    assert_eq!(program.next(), ready_of_the_test_window());
    assert_eq!(program.next(), FromProgram::StateChanged);
}

#[test]
fn values_the_plugin_changed_by_itself_are_edits_and_its_state_changed() {
    let module = the_module();
    let mut program = start(&module.module, "v");

    program.begin(&[(GAIN, 1.0)], None);

    assert_eq!(program.next(), ready_of_the_test_window());
    assert_eq!(
        program.next(),
        FromProgram::Edit {
            edit: ParameterEdit::Value { id: GAIN, value: 2.0 }
        },
        "the value it set (a half, which is the plain 2) differs from the 1 that was told"
    );
    assert_eq!(program.next(), FromProgram::StateChanged);
}

#[test]
fn a_change_of_the_state_that_the_plugin_tells_only_to_itself_is_heard_and_is_in_the_state_that_the_window_saves() {
    // (A window built with DPF tells its other half what a hand did to its state, in a message, and the host is told nothing: a curve that is drawn in Wolf Shaper would be lost.)
    let module = the_module();
    let mut program = start(&module.module, "s");

    program.begin(&[], None);

    assert_eq!(program.next(), ready_of_the_test_window());
    assert_eq!(
        program.next(),
        FromProgram::StateChanged,
        "the program looked at the state, and it was not what it was"
    );
    program.tell(&ToProgram::SaveState);
    match program.next() {
        FromProgram::State { state } => assert_eq!(secret_in_the_state(&module.module, &state), 0.25),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_change_of_the_state_that_was_made_in_the_last_moment_before_the_window_closes_is_said_before_it_does() {
    // (The state is looked at twice a second. A person who draws a curve and closes the window at once would lose it, if the program did not look once more as it ends.)
    let module = the_module();
    let mut program = start(&module.module, "s");
    program.begin(&[], None);
    assert_eq!(program.next(), ready_of_the_test_window());

    program.tell(&ToProgram::Close);

    let said = program.said_until(|message| matches!(message, FromProgram::Closed));
    assert!(
        said.contains(&FromProgram::StateChanged),
        "the state is said to have changed before it is said: {said:?}"
    );
    let state = said
        .iter()
        .find_map(|message| match message {
            FromProgram::State { state } => Some(state.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the state was said before the window closed: {said:?}"));
    assert_eq!(secret_in_the_state(&module.module, &state), 0.25);
    assert_eq!(said.last(), Some(&FromProgram::Closed));
}

#[test]
fn what_a_hands_edit_of_a_value_or_the_projects_values_and_state_change_in_the_state_is_not_a_change_of_it() {
    let module = the_module();

    // A gesture on a value: the state has the value, and the project has it too.
    let mut edited = start(&module.module, "e");
    edited.begin(&[], None);
    edited.said_until_the_end_of_a_gesture();
    assert!(
        edited.says_nothing_for(Duration::from_millis(2200)),
        "the value is in the state, and that is not a change that the project does not know of"
    );

    // Values that the project says (alone: a state that it gives in the same test would be what hides a mistake in this).
    let mut told = start(&module.module, "");
    told.begin(&[], None);
    assert_eq!(told.next(), ready_of_the_test_window());
    told.tell(&ToProgram::Values {
        values: vec![(GAIN, 1.5)],
    });
    assert!(
        told.says_nothing_for(Duration::from_millis(2200)),
        "what the project told is not what the window did"
    );

    // A state that the project gives (alone, for the same reason).
    let mut given = start(&module.module, "");
    given.begin(&[], None);
    assert_eq!(given.next(), ready_of_the_test_window());
    given.tell(&ToProgram::LoadState {
        state: state_with_the_gain(&module.module, 2.5),
    });
    assert!(
        given.says_nothing_for(Duration::from_millis(2200)),
        "what the project gave is not what the window did"
    );
}

#[test]
fn closing_it_takes_the_window_out_of_its_parent_and_the_program_ends() {
    let module = the_module();
    let mut program = start(&module.module, "");
    program.begin(&[], None);
    assert_eq!(program.next(), ready_of_the_test_window());

    program.tell(&ToProgram::Close);

    assert_eq!(program.next(), FromProgram::Closed);
    assert_eq!(program.exit_code(), Some(0));
    let noise = program.noise_until("removed");
    let removed = noise.iter().position(|line| line.contains("removed")).unwrap();
    let frame_taken = noise
        .iter()
        .position(|line| line.contains("setFrame none"))
        .expect("the frame is taken away after the window is");
    assert!(removed < frame_taken, "{noise:?}");
}

#[test]
fn a_state_that_changed_when_the_program_ends_is_said_before_it_does() {
    let module = the_module();
    let mut program = start(&module.module, "e");
    program.begin(&[], None);
    program.said_until_the_end_of_a_gesture();
    program.tell(&ToProgram::Close);

    // (The gesture made no `StateChanged`, so the program has nothing to say that was not asked for: the state is only said when it is asked for or when the plugin said it changed.)
    let said = program.said_until(|message| matches!(message, FromProgram::Closed));
    assert_eq!(said, vec![FromProgram::Closed]);
}

#[test]
fn a_plugin_without_a_window_or_a_class_that_is_not_there_or_a_module_that_is_not_there_is_a_failure_that_says_why() {
    let module = the_module();
    for (class, what) in [
        (class_text(plugins::LATENCY), "the plugin has no window"),
        (class_text(plugins::INSTRUMENT), "the plugin has no window"),
        ("not a class id".to_owned(), "is not the id of a class"),
        (class_text([1, 2, 3, 4]), "could not be made"),
    ] {
        let mut program = Running::start("vst3", &module.module, &class, true, &[]);
        program.begin(&[], None);
        match program
            .said_until(|message| matches!(message, FromProgram::Failed { .. }))
            .last()
        {
            Some(FromProgram::Failed { reason }) => assert!(reason.contains(what), "{class}: {reason}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(program.exit_code(), Some(1), "{class}");
    }
    let mut program = Running::start(
        "vst3",
        Path::new("a-folder-that-is-not-there.vst3"),
        &class_text(plugins::GAIN),
        true,
        &[],
    );
    program.begin(&[], None);
    match program
        .said_until(|message| matches!(message, FromProgram::Failed { .. }))
        .last()
    {
        Some(FromProgram::Failed { reason }) => assert!(reason.contains("could not be loaded"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_window_that_crashes_when_it_is_put_in_its_parent_is_a_program_that_ends_without_saying_it_is_ready() {
    let module = the_module();
    let mut program = Running::start(
        "vst3",
        &module.module,
        &class_text(plugins::GAIN),
        true,
        &[("FFRS_TEST_PLUGIN", "crash_when_attached")],
    );
    program.begin(&[], None);

    // (A program that crashes says nothing, and its output ends; the editor's side turns that into a failure: see the test of the sessions below.)
    assert!(
        program.says_nothing_for(Duration::from_secs(10)),
        "it was not to say it is ready"
    );
    assert_ne!(program.exit_code(), Some(0));
}

// --- the editor's side ---

fn the_key() -> WindowKey {
    WindowKey {
        target: AnimationTarget::Clip(ClipId::new(TrackId::new(0), 0)),
        instance: EffectInstanceId::FIRST,
    }
}

fn what_to_open(module: &Path, values: &[(u32, f64)], keys: &str) -> WhatToOpen {
    WhatToOpen {
        library: module.to_owned(),
        plugin_id: class_text(plugins::GAIN),
        kind: PluginKind::Vst3,
        title: "a window".to_owned(),
        values: values.to_vec(),
        state: None,
        environment: vec![("FFRS_TEST_VIEW_KEYS".to_owned(), keys.to_owned())],
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
fn the_editor_opens_the_window_of_a_vst3_plugin_hears_its_edit_asks_for_its_state_and_closes_it() {
    let module = the_module();
    let mut windows = EditorWindows::with_program(PathBuf::from(PROGRAM));
    windows
        .open(the_key(), &what_to_open(&module.module, &[(GAIN, 2.0)], "e"))
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
                width: plugins::VIEW_WIDTH as u32,
                height: plugins::VIEW_HEIGHT as u32,
                can_resize: true,
                embedded: false,
                has_state: true
            },
            WindowEvent::Edit(ParameterEdit::Begin { id: GAIN }),
            WindowEvent::Edit(ParameterEdit::Value { id: GAIN, value: 3.0 }),
            WindowEvent::Edit(ParameterEdit::End { id: GAIN }),
        ]
    );
    assert!(windows.is_open(the_key()));
    windows.ask_for_the_state(the_key());
    let events = polled_until(&mut windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::State(_)))
    });
    match events.last() {
        Some(WindowEvent::State(state)) => assert_eq!(gain_in_the_state(&module.module, state), 3.0),
        other => panic!("{other:?}"),
    }
    windows.close(the_key());
    polled_until(&mut windows, |events| events.contains(&WindowEvent::Closed));
    assert!(!windows.is_open(the_key()));
}

#[test]
fn a_vst3_window_that_crashes_is_a_failure_in_the_editor_and_the_editor_goes_on() {
    let module = the_module();
    let mut windows = EditorWindows::with_program(PathBuf::from(PROGRAM));
    let mut what = what_to_open(&module.module, &[], "");
    what.environment
        .push(("FFRS_TEST_PLUGIN".to_owned(), "crash_when_attached".to_owned()));
    windows.open(the_key(), &what).expect("the program starts");

    let events = polled_until(&mut windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::Failed(_)))
    });

    assert!(matches!(events.last(), Some(WindowEvent::Failed(_))), "{events:?}");
    assert!(!windows.is_open(the_key()));
}

// --- a real window ---

/// **A real window**, where there is a screen to make one on (Windows: the program makes a window of its own, and the plugin's window is put in it): the plugin is given the window's
/// id and a scale, and what a hand does in it is heard.
#[cfg(windows)]
#[test]
fn on_windows_the_plugins_window_is_put_in_a_real_window_of_the_program_and_the_hand_in_it_is_heard() {
    let module = the_module();
    let mut program = start_with(&module.module, plugins::GAIN, "er", false);
    program.begin(&[(GAIN, 1.0)], None);

    let said = program.said_until_the_end_of_a_gesture();

    match said.first() {
        Some(FromProgram::Ready {
            embedded: true,
            can_resize: true,
            has_state: true,
            ..
        }) => {}
        other => panic!("it is embedded in a window of the program: {other:?}"),
    }
    assert!(
        said.contains(&FromProgram::Edit {
            edit: ParameterEdit::Value { id: GAIN, value: 3.0 }
        }),
        "{said:?}"
    );
    let noise = program.noise_until("attached");
    assert!(
        noise.iter().any(|line| line.contains("attached parent=")
            && !line.contains("parent=0 ")
            && !line.contains("parent=1 ")),
        "the id of a window, and not the one that headless gives: {noise:?}"
    );
    let (width, height) = plugins::VIEW_ASKED;
    program.noise_until(&format!("onSize {width}x{height}"));
    program.tell(&ToProgram::Close);
    assert_eq!(
        program
            .said_until(|message| matches!(message, FromProgram::Closed))
            .len(),
        1
    );
    assert_eq!(program.exit_code(), Some(0));
    program.noise_until("removed");
}

/// **For a person to look at**: opens the window of a real VST3 plugin (`FFRS_LOOK_MODULE` and `FFRS_LOOK_CLASS`, the class id as `ClassId::text` writes it) where there is a screen,
/// and keeps it for `FFRS_LOOK_SECONDS` seconds (20 if not said). Run by hand with `cargo test --test plugin_editor_vst3 -- --ignored look_at_the_window`.
#[test]
#[ignore = "opens a window and waits: run by hand"]
fn look_at_the_window_of_a_real_plugin() {
    let module = PathBuf::from(std::env::var_os("FFRS_LOOK_MODULE").expect("FFRS_LOOK_MODULE names the .vst3"));
    let class = std::env::var("FFRS_LOOK_CLASS").expect("FFRS_LOOK_CLASS names the class");
    let seconds = std::env::var("FFRS_LOOK_SECONDS")
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(20);
    let mut program = Running::start("vst3", &module, &class, false, &[]);
    program.begin(&[], None);
    println!("the program says: {:?}", program.next());
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        if let Some(said) = program.next_within(Duration::from_millis(100)) {
            println!("the program says: {said:?}");
        }
    }
    program.tell(&ToProgram::Close);
    println!("the program says: {:?}", program.next_within(Duration::from_secs(5)));
}
