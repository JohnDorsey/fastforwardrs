// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the editor instance (phase 10, piece 10.1) with the test plugin that has a window and a state (`org.fastforwardrs.test.editor`). That plugin draws nothing: its state is text
//! that says what the host did to its window, so that what the host did can be seen from here.

use clap_test_plugins::{EDITOR, EDITOR_ASKED_SIZE, EDITOR_EDIT_VALUE, EDITOR_SIZE, GAIN};

use crate::Library;
use crate::editor::{Editor, EditorError, EditorEvent, ParameterEdit};

fn the_library() -> Library {
    Library::open(&clap_test_plugins::library_file_for_tests()).expect("the test library is opened")
}

fn editor_of(id: &str) -> Editor {
    Editor::new(&the_library(), id).expect("the editor is made")
}

/// What the editor plugin says that was done to its window.
fn what_was_done(editor: &mut Editor) -> String {
    String::from_utf8(editor.save_the_state().expect("the state is saved")).expect("the state is text")
}

#[test]
fn a_plugin_that_has_no_window_is_said_to_have_none_and_cannot_be_given_one() {
    let mut editor = editor_of(GAIN);

    assert!(!editor.has_a_window());
    assert!(!editor.can_make_a_window("win32", true));
    assert!(matches!(
        editor.make_the_window("win32", true, 1.0),
        Err(EditorError::NoWindow)
    ));
    assert!(!editor.has_state(), "and no state either");
    assert!(matches!(editor.save_the_state(), Err(EditorError::State(_))));
}

#[test]
fn a_plugin_that_has_a_window_says_which_kinds_it_can_make() {
    let mut editor = editor_of(EDITOR);

    assert!(editor.has_a_window());
    for api in ["win32", "x11"] {
        for floating in [true, false] {
            assert!(editor.can_make_a_window(api, floating), "{api}, floating: {floating}");
        }
    }
    assert!(editor.can_make_a_window("cocoa", true));
    assert!(
        !editor.can_make_a_window("cocoa", false),
        "the test plugin makes a window of this kind only floating"
    );
    assert!(!editor.can_make_a_window("not a kind of window", true));
    assert!(editor.has_state());
    assert_eq!(editor.preferred_window(), None, "the test plugin has no preference");
}

#[test]
fn a_window_is_made_at_the_scale_given_and_says_how_big_it_is_and_that_it_can_be_resized() {
    let mut editor = editor_of(EDITOR);

    let size = editor.make_the_window("win32", true, 1.5).expect("the window is made");

    assert_eq!((size.width, size.height), EDITOR_SIZE);
    assert!(size.can_resize);
    let done = what_was_done(&mut editor);
    assert!(done.contains("created=1"), "{done}");
    assert!(done.contains("floating=1"), "{done}");
    assert!(done.contains("scale=1.5"), "{done}");
    assert!(done.contains("shown=0"), "not shown yet: {done}");
}

#[test]
fn a_window_that_is_embedded_is_made_not_floating_and_is_put_in_the_window_it_is_given() {
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("x11", false, 1.0).expect("the window is made");

    // (A window number that is not a window: the plugin draws nothing and only says that it was given one.)
    #[allow(unsafe_code)]
    // SAFETY: the test plugin does not use the window it is given.
    unsafe {
        editor
            .put_the_window_in(clack_extensions::gui::Window::from_x11_handle(1))
            .expect("the plugin takes the window");
    }

    let done = what_was_done(&mut editor);
    assert!(done.contains("floating=0"), "{done}");
    assert!(done.contains("embedded=1"), "{done}");
}

#[test]
fn showing_the_window_gives_the_edit_that_it_makes_as_a_gesture_and_says_that_the_state_changed() {
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("win32", true, 1.0).unwrap();
    assert_eq!(editor.poll(), vec![], "nothing happens before it is shown");

    editor.show().expect("the window is shown");
    let events = editor.poll();

    assert_eq!(
        events,
        vec![
            EditorEvent::Edit(ParameterEdit::Begin { id: 1 }),
            EditorEvent::Edit(ParameterEdit::Value {
                id: 1,
                value: EDITOR_EDIT_VALUE
            }),
            EditorEvent::Edit(ParameterEdit::End { id: 1 }),
            EditorEvent::StateChanged,
            EditorEvent::ParametersChanged,
        ],
        "a gesture of one value, in order, then the state, and the rescan that the plugin asked for in the call on the main thread (which is made first)"
    );
    assert!(
        what_was_done(&mut editor).contains("callbacks=1"),
        "the call that the plugin asked for was made"
    );
    assert_eq!(editor.poll(), vec![], "and it is said once");
    assert!(what_was_done(&mut editor).contains("shown=1"));
}

#[test]
fn hiding_is_the_plugins_doing_and_a_window_that_is_destroyed_is_destroyed_once() {
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("win32", true, 1.0).unwrap();
    editor.show().unwrap();
    editor.hide();

    let done = what_was_done(&mut editor);
    assert!(done.contains("hidden=1"), "{done}");

    editor.destroy_the_window();
    editor.destroy_the_window();
    let done = what_was_done(&mut editor);
    assert!(done.contains("destroyed=1"), "once, though it was asked twice: {done}");
}

#[test]
fn what_the_project_says_the_values_are_reaches_the_plugin_and_it_says_nothing_back() {
    let mut editor = editor_of(EDITOR);
    assert_eq!(editor.value_of(1), Some(1.0), "a new plugin has the default");

    let echoed = editor.tell_the_values(&[(1, 2.5)]);

    assert_eq!(echoed, vec![], "a plugin that is told a value says nothing of it");
    assert_eq!(editor.value_of(1), Some(2.5));
    assert_eq!(editor.value_of(99), None, "a parameter that it has not");
}

#[test]
fn a_value_that_the_window_made_is_what_the_plugin_says_the_parameter_is() {
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("win32", true, 1.0).unwrap();
    editor.show().unwrap();
    let _ = editor.poll();

    assert_eq!(editor.value_of(1), Some(EDITOR_EDIT_VALUE));
}

#[test]
fn the_size_of_the_window_can_be_adjusted_and_set() {
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("win32", false, 1.0).unwrap();

    assert_eq!(
        editor.adjust_the_size(405, 333),
        Some((400, 330)),
        "the plugin rounds down to tens"
    );
    assert!(editor.set_the_size(500, 400));

    assert!(what_was_done(&mut editor).contains("size=500x400"));
}

#[test]
fn a_state_that_is_saved_can_be_given_back_and_what_was_done_to_a_window_is_not_in_it() {
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("win32", true, 1.0).unwrap();
    editor.show().unwrap();
    let _ = editor.poll();
    let saved = editor.save_the_state().unwrap();
    assert!(String::from_utf8_lossy(&saved).contains("edits=1"));

    let mut another = editor_of(EDITOR);
    another.load_the_state(b"edits=7").expect("the state is loaded");

    let done = what_was_done(&mut another);
    assert!(done.contains("edits=7"), "{done}");
    assert!(
        done.contains("created=0"),
        "what was done to another's window is not in it: {done}"
    );
}

#[test]
fn two_editors_of_one_plugin_are_two_windows_that_do_not_know_of_each_other() {
    let mut first = editor_of(EDITOR);
    let mut second = editor_of(EDITOR);
    first.make_the_window("win32", true, 1.0).unwrap();
    first.show().unwrap();
    let _ = first.poll();

    assert_eq!(second.value_of(1), Some(1.0));
    assert!(what_was_done(&mut second).contains("shown=0"));
    assert_eq!(first.value_of(1), Some(EDITOR_EDIT_VALUE));
}

#[test]
fn an_editor_that_is_dropped_destroys_its_window_first() {
    // (The plugin's resources for its window are freed on the thread that made them, before the plugin is: the test is that this does not crash.)
    let mut editor = editor_of(EDITOR);
    editor.make_the_window("win32", true, 1.0).unwrap();
    editor.show().unwrap();
    drop(editor);
}

/// The events that come from a plugin that is told (by a state) to ask something of the host.
fn what_the_plugin_asks_when_a_state_says(ask: &str) -> Vec<EditorEvent> {
    let mut editor = editor_of(EDITOR);
    editor.load_the_state(format!("ask={ask}").as_bytes()).unwrap();
    editor
        .poll()
        .into_iter()
        // (The test plugin says that its state changed when it is given one, as many plugins do: that is not what is asked about here.)
        .filter(|event| !matches!(event, EditorEvent::ParametersChanged | EditorEvent::StateChanged))
        .collect()
}

#[test]
fn what_the_plugin_asks_of_the_host_about_its_window_is_given_to_the_user_of_the_editor() {
    assert_eq!(
        what_the_plugin_asks_when_a_state_says("resize"),
        vec![EditorEvent::ResizeRequested {
            width: EDITOR_ASKED_SIZE.0,
            height: EDITOR_ASKED_SIZE.1
        }]
    );
    assert_eq!(
        what_the_plugin_asks_when_a_state_says("show"),
        vec![EditorEvent::ShowRequested]
    );
    assert_eq!(
        what_the_plugin_asks_when_a_state_says("hide"),
        vec![EditorEvent::HideRequested]
    );
    assert_eq!(
        what_the_plugin_asks_when_a_state_says("close"),
        vec![EditorEvent::Closed { destroyed: false }]
    );
    assert_eq!(
        what_the_plugin_asks_when_a_state_says("destroyed"),
        vec![EditorEvent::Closed { destroyed: true }]
    );
    assert_eq!(what_the_plugin_asks_when_a_state_says("nothing"), vec![]);
}
