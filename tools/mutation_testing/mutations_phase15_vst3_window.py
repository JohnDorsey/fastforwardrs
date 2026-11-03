# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for phase 15, piece 15.6 (the windows of VST3 plugins): the window of the host crate (`view.rs`), the objects a window needs of the host (`objects.rs`: the
gestures, `setDirty`, the messages of DPF's windows, the run loop), the window of the editor-window program (`Vst3Window`, with the look at the state that a window that says nothing
needs), the scan's word that a plugin has a window, and the editor's use of it.

The tests are the host crate's (`view_tests`, `message_tests`, `run_loop_tests`, against the strict window of the test plugins), the program as a real process
(`tests/plugin_editor_vst3.rs`) and the editor with that program (`tests/plugin_editor_vst3_journey.rs`):

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_phase15_vst3_window.py --no-controls --results first.jsonl

Mistakes that cannot be seen by these tests, said plainly (left out of the list, not forgotten):

* **The descriptors of the run loop** (`look_at_the_descriptors`) are Linux's: this list is run on Windows, where that code is not built. They were tried by hand on Linux (a test of the
  descriptors, and the test window with its pipe), not planted.
* `has_a_view` saying yes for a plugin whose window is not of this system's kind: the test window has only that kind.
* The scale not told on a Mac, and the window id of an `NSView` or a Wayland surface: a test of the handles says them, and no Mac is here to run the rest.
* `Vst3Window::can_make_a_window` ignoring the kind of window: the program only asks for the kind of this system.
"""

import os

HOST_TESTS = os.environ.get("MUTATION_HOST_TESTS", "-p vst3-host --lib")
PROGRAM_TESTS = os.environ.get("MUTATION_PROGRAM_TESTS", "--test plugin_editor_vst3")
JOURNEY_TESTS = os.environ.get("MUTATION_JOURNEY_TESTS", "--test plugin_editor_vst3_journey")
SCAN_TESTS = os.environ.get("MUTATION_SCAN_TESTS", "--test plugin_scan_vst3")
UNIT_TESTS = os.environ.get("MUTATION_UNIT_TESTS", "--lib -- plugin_editor")

VIEW = "crates/vst3-host/src/view.rs"
OBJECTS = "crates/vst3-host/src/objects.rs"
SCAN = "crates/vst3-host/src/scan.rs"
BACKENDS = "src/plugin_editor/backends.rs"
PROGRAM = "src/plugin_editor/program.rs"
EFFECTS = "src/plugin_scan/effects.rs"
FLOW = "src/application/fast_forward_application/plugin_editor_flow.rs"

MUTATIONS = [
    # --- putting the window in its parent (host crate) ---
    ("view: a window that was not attached is kept as one that is", VIEW, "        if attached != kResultOk {", "        if false {", HOST_TESTS),
    ("view: a window that can be resized says it cannot", VIEW, "let can_resize = unsafe { view.canResize() } == kResultTrue;", "let can_resize = unsafe { view.canResize() } != kResultTrue;", HOST_TESTS),
    ("view: the window is not taken out of its parent before it is let go of", VIEW, "                open.view.removed();\n", "", HOST_TESTS),
    ("view: the frame is not taken away when the window is closed", VIEW, "                open.view.setFrame(std::ptr::null_mut());\n", "", PROGRAM_TESTS),
    ("view: the handle of a window of Windows is one more", VIEW, "RawWindowHandle::Win32(handle) => handle.hwnd.get().cast_unsigned(),", "RawWindowHandle::Win32(handle) => handle.hwnd.get().cast_unsigned() + 1,", HOST_TESTS),
    # --- what the window says it did (host crate) ---
    ("view: a value is the same as the one the host knew when it differs by less than a unit", VIEW, "(known - plain).abs() > 1e-9", "(known - plain).abs() > 1.0", HOST_TESTS),
    ("view: the values that differ are not kept as the ones the host knows", VIEW, "        open.known.extend(differing.iter().copied());\n", "", HOST_TESTS),
    ("view: the values that differ are told backwards", VIEW, "            .filter(|(id, plain)| open.known.get(id).is_none_or(|known| (known - plain).abs() > 1e-9))\n            .collect();", "            .filter(|(id, plain)| open.known.get(id).is_none_or(|known| (known - plain).abs() > 1e-9))\n            .rev()\n            .collect();", HOST_TESTS),
    # --- the gestures, and the state (objects) ---
    ("handler: the begin of a gesture is lost", OBJECTS, "        self.note_a_step(Step::Begin(id));\n", "", HOST_TESTS),
    ("handler: the value of a gesture is lost", OBJECTS, "        self.note_a_step(Step::Value(id, value));\n", "", HOST_TESTS),
    ("handler: the end of a gesture is lost", OBJECTS, "        self.note_a_step(Step::End(id));\n", "", HOST_TESTS),
    ("handler: a plugin that says it is clean has changed its state", OBJECTS, "if state != 0 && self.recording.get() {", "if self.recording.get() {", HOST_TESTS),
    # --- the messages and the attributes of DPF's windows (objects) ---
    ("message: a text that does not fit is written with its zero past the room", OBJECTS, "let kept = text.len().min(room - 1);", "let kept = text.len().min(room);", HOST_TESTS),
    ("message: the id that is set is not kept", OBJECTS, "            *self.id.borrow_mut() = CStr::from_ptr(id).to_owned();\n", "", HOST_TESTS),
    # --- the run loop (objects) ---
    ("run loop: a timer is due an interval after the last time it was due, and not after it was called", OBJECTS, "                timer.due = now + timer.every;", "                timer.due += timer.every;", HOST_TESTS),
    ("run loop: a timer of no interval is taken", OBJECTS, "        if milliseconds == 0 {", "        if false {", HOST_TESTS),
    ("run loop: taking a timer out takes out the others", OBJECTS, "            .retain(|timer| timer.handler.as_ptr() != handler);", "            .retain(|timer| timer.handler.as_ptr() == handler);", HOST_TESTS),
    # --- the scan ---
    ("scan: no plugin has a window", SCAN, "let has_a_window = problem.is_none() && instance.has_a_view();", "let has_a_window = false;", SCAN_TESTS),
    ("scan: every plugin has a window", SCAN, "let has_a_window = problem.is_none() && instance.has_a_view();", "let has_a_window = true;", SCAN_TESTS),
    ("effect: the plugin's window is not said", EFFECTS, "        has_a_window: scanned.has_a_window,", "        has_a_window: false,", SCAN_TESTS),
    # --- the window of the editor-window program ---
    ("window: the headless parent is not a window and not one a plugin of the tests takes", BACKENDS, "const PARENT_OF_A_WINDOW_WITHOUT_A_SCREEN: usize = 1;", "const PARENT_OF_A_WINDOW_WITHOUT_A_SCREEN: usize = 0;", PROGRAM_TESTS),
    ("window: the plugin says what the project told it as edits", BACKENDS, "        self.backlog.retain(|event| !matches!(event, EditorEvent::Edit(_)));\n", "", PROGRAM_TESTS),
    ("window: a value that a hand made is a change of the state", BACKENDS, "        if events.iter().any(|event| matches!(event, EditorEvent::Edit(_))) {\n            self.the_state_is_the_values_for_a_moment();\n        }\n", "", PROGRAM_TESTS),
    ("window: the values that the project told are a change of the state", BACKENDS, "        if !values.is_empty() {\n            self.the_state_is_the_values_for_a_moment();\n        }\n", "", PROGRAM_TESTS),
    ("window: a state that the project gave is a change of the state", BACKENDS, "    fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), String> {\n        self.the_state_is_the_values_for_a_moment();\n", "    fn load_the_state(&mut self, bytes: &[u8]) -> Result<(), String> {\n", PROGRAM_TESTS),
    ("window: the state is never looked at", BACKENDS, "< HOW_OFTEN_TO_LOOK_AT_THE_STATE {\n            return;\n        }", "< HOW_OFTEN_TO_LOOK_AT_THE_STATE || true {\n            return;\n        }", PROGRAM_TESTS),
    ("window: a state that is not what it was is not a change", BACKENDS, "is_some_and(|known| *known != state)", "is_some_and(|known| *known == state)", PROGRAM_TESTS),
    ("program: the state is not looked at once more as the window ends", PROGRAM, "if !self.state_is_not_what_was_said && self.editor.state_changed_since_it_was_looked_at() {", "if false && self.editor.state_changed_since_it_was_looked_at() {", PROGRAM_TESTS),
    ("window: a state that is not what it was is not one as the window ends", BACKENDS, "            (Ok(state), Some(known)) => state != *known,", "            (Ok(state), Some(known)) => state == *known,", PROGRAM_TESTS),
    ("window: a window says it has no state", BACKENDS, "impl PluginWindow for Vst3Window {\n    fn has_state(&self) -> bool {\n        true", "impl PluginWindow for Vst3Window {\n    fn has_state(&self) -> bool {\n        false", PROGRAM_TESTS),
    # --- the kind of plugin, and the editor's use of it ---
    ("program: --kind vst3 is a kind of LV2", PROGRAM, '"vst3" => PluginKind::Vst3,', '"vst3" => PluginKind::Lv2,', UNIT_TESTS),
    ("flow: a VST3 plugin's window is asked for as a CLAP plugin's", FLOW, "PluginAddress::Vst3 { class_id } => (PluginKind::Vst3, class_id.text()),", "PluginAddress::Vst3 { class_id } => (PluginKind::Clap, class_id.text()),", JOURNEY_TESTS),
]
