// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's side of the plugin windows (`plugin_editor::sessions`, phase 10 piece 10.3) with the real editor-window program, headless, and the test plugin that has a window.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap_test_plugins::{EDITOR, EDITOR_EDIT_VALUE, GAIN};
use fastforwardrs::plugin_editor::protocol::ParameterEdit;
use fastforwardrs::plugin_editor::sessions::{EditorWindows, WhatToOpen, WindowEvent, WindowKey};
use fastforwardrs::project::{AnimationTarget, ClipId, EffectInstanceId, TrackId};

const PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");

fn the_key() -> WindowKey {
    WindowKey {
        target: AnimationTarget::Clip(ClipId::new(TrackId::new(0), 0)),
        instance: EffectInstanceId::FIRST,
    }
}

fn what_to_open(id: &str, values: &[(u32, f64)], state: Option<&str>) -> WhatToOpen {
    WhatToOpen {
        library: clap_test_plugins::library_file_for_tests(),
        plugin_id: id.to_owned(),
        title: "a window".to_owned(),
        values: values.to_vec(),
        state: state.map(str::to_owned),
        environment: Vec::new(),
        headless: true,
    }
}

fn windows() -> EditorWindows {
    EditorWindows::with_program(PathBuf::from(PROGRAM))
}

/// Polls until `done` says that what has come is enough, and gives everything that came (for the key).
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

fn the_state(windows: &mut EditorWindows) -> String {
    windows.ask_for_the_state(the_key());
    let events = polled_until(windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::State(_)))
    });
    let Some(WindowEvent::State(state)) = events.into_iter().find(|event| matches!(event, WindowEvent::State(_)))
    else {
        unreachable!()
    };
    String::from_utf8(decode(&state)).expect("the state is text")
}

// (The editor's own base 64 is private: this is the little of it that the tests need.)
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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
fn a_window_is_opened_and_what_it_says_comes_in_order() {
    let mut windows = windows();
    windows
        .open(the_key(), &what_to_open(EDITOR, &[(1, 2.0)], None))
        .expect("it opens");
    assert!(windows.is_open(the_key()));

    let events = polled_until(&mut windows, |events| events.contains(&WindowEvent::ParametersChanged));

    assert!(
        matches!(
            events[0],
            WindowEvent::Ready {
                embedded: false,
                has_state: true,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(
        events[1..],
        [
            WindowEvent::Edit(ParameterEdit::Begin { id: 1 }),
            WindowEvent::Edit(ParameterEdit::Value {
                id: 1,
                value: EDITOR_EDIT_VALUE
            }),
            WindowEvent::Edit(ParameterEdit::End { id: 1 }),
            WindowEvent::StateChanged,
            WindowEvent::ParametersChanged,
        ]
    );
}

#[test]
fn a_window_that_is_opened_twice_is_one_window_and_the_second_opening_changes_nothing() {
    let mut windows = windows();
    windows
        .open(the_key(), &what_to_open(EDITOR, &[(1, 2.5)], None))
        .unwrap();
    windows
        .open(the_key(), &what_to_open(EDITOR, &[(1, 9.0)], None))
        .unwrap();
    let _ = polled_until(&mut windows, |events| events.contains(&WindowEvent::ParametersChanged));

    assert_eq!(windows.keys(), vec![the_key()]);
    let state = the_state(&mut windows);
    assert!(
        state.contains("gain_when_shown=2.5"),
        "the window that was open is the one that is there: {state}"
    );
}

#[test]
fn the_values_that_a_window_began_with_are_not_told_to_it_again() {
    let mut windows = windows();
    windows
        .open(the_key(), &what_to_open(EDITOR, &[(1, 2.5)], None))
        .unwrap();
    // (Before anything the window did has been taken in: what it has is what it was begun with.)
    windows.tell_the_values(the_key(), &[(1, 2.5)]);
    let _ = polled_until(&mut windows, |events| events.contains(&WindowEvent::ParametersChanged));

    assert!(
        the_state(&mut windows).contains("told=1"),
        "only the values it began with: they were not told again"
    );
}

#[test]
fn a_value_is_told_to_a_window_only_when_it_is_not_what_the_window_has() {
    let mut windows = windows();
    windows
        .open(the_key(), &what_to_open(EDITOR, &[(1, 2.5)], None))
        .unwrap();
    let _ = polled_until(&mut windows, |events| events.contains(&WindowEvent::ParametersChanged));
    // (The values it began with: one told. Then the window's own edit made the gain 0.75, which is known.)
    assert!(the_state(&mut windows).contains("told=1"));

    // (What the window's own edit made the gain is what it has: it is not told back.)
    windows.tell_the_values(the_key(), &[(1, EDITOR_EDIT_VALUE)]);
    assert!(
        the_state(&mut windows).contains("told=1"),
        "what its own edit made is what it has"
    );

    windows.tell_the_values(the_key(), &[(1, 3.0)]);
    windows.tell_the_values(the_key(), &[(1, 3.0)]);
    let state = the_state(&mut windows);
    assert!(
        state.contains("told=2"),
        "a new value once, however often it is said: {state}"
    );
    assert!(state.contains("gain=3"), "{state}");
}

#[test]
fn closing_ends_the_window_and_it_is_not_open_any_more() {
    let mut windows = windows();
    windows.open(the_key(), &what_to_open(EDITOR, &[], None)).unwrap();
    let _ = polled_until(&mut windows, |events| events.contains(&WindowEvent::ParametersChanged));

    windows.close(the_key());
    let events = polled_until(&mut windows, |events| events.contains(&WindowEvent::Closed));

    assert!(events.contains(&WindowEvent::Closed));
    assert!(!windows.is_open(the_key()));
    assert!(windows.keys().is_empty());
}

#[test]
fn a_state_is_given_to_the_window_and_asked_of_it() {
    let mut windows = windows();
    windows.open(the_key(), &what_to_open(EDITOR, &[], None)).unwrap();
    let _ = polled_until(&mut windows, |events| events.contains(&WindowEvent::ParametersChanged));

    // ("edits=40" in base 64.)
    windows.give_the_state(the_key(), "ZWRpdHM9NDA=");

    assert!(the_state(&mut windows).contains("edits=40"));
}

#[test]
fn a_plugin_without_a_window_is_a_failure_and_the_window_is_not_open() {
    let mut windows = windows();
    windows.open(the_key(), &what_to_open(GAIN, &[], None)).unwrap();

    let events = polled_until(&mut windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::Failed(_)))
    });

    let Some(WindowEvent::Failed(reason)) = events.last() else {
        panic!("{events:?}")
    };
    assert!(reason.contains("no window"), "{reason}");
    assert!(!windows.is_open(the_key()));
}

#[test]
fn a_program_that_crashes_is_a_failure_that_says_so_and_the_editor_goes_on() {
    let mut windows = windows();
    // ("ask=crash" in base 64: the plugin takes the program down when it is asked for its call on the main thread.)
    windows
        .open(the_key(), &what_to_open(EDITOR, &[], Some("YXNrPWNyYXNo")))
        .unwrap();

    let events = polled_until(&mut windows, |events| {
        events.iter().any(|event| matches!(event, WindowEvent::Failed(_)))
    });

    let Some(WindowEvent::Failed(reason)) = events.last() else {
        panic!("{events:?}")
    };
    assert!(reason.contains("ended without saying why"), "{reason}");
    assert!(!windows.is_open(the_key()));
}

#[test]
fn a_program_that_is_not_there_is_an_error_that_says_so() {
    let mut windows = EditorWindows::with_program(PathBuf::from("no-such-folder/fastforwardrs-plugin-editor"));

    let error = windows.open(the_key(), &what_to_open(EDITOR, &[], None)).unwrap_err();

    assert!(error.contains("could not be run"), "{error}");
    assert!(!windows.is_open(the_key()));
}

#[test]
fn a_program_that_is_not_the_editors_is_refused() {
    // (A program that does not say which code it was built from: any program that is not one of ours.)
    let not_ours = if cfg!(windows) {
        "C:/Windows/System32/whoami.exe"
    } else {
        "/bin/true"
    };
    let mut windows = EditorWindows::with_program(PathBuf::from(not_ours));

    let error = windows.open(the_key(), &what_to_open(EDITOR, &[], None)).unwrap_err();

    assert!(error.contains("built from other code"), "{error}");
}

#[test]
fn every_window_can_be_closed_at_once() {
    let mut windows = windows();
    let other = WindowKey {
        instance: EffectInstanceId::new(1),
        ..the_key()
    };
    windows.open(the_key(), &what_to_open(EDITOR, &[], None)).unwrap();
    windows.open(other, &what_to_open(EDITOR, &[], None)).unwrap();
    let _ = polled_until(&mut windows, |events| {
        events
            .iter()
            .filter(|event| **event == WindowEvent::ParametersChanged)
            .count()
            == 2
    });

    windows.close_all();
    let _ = polled_until(&mut windows, |events| {
        events.iter().filter(|event| **event == WindowEvent::Closed).count() == 2
    });

    assert!(windows.keys().is_empty());
}
