# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for phase 15, piece 15.1 (the VST3 host crate): the id of a class, the state, the reading of a module, and everything an instance does with a plugin.

The tests are those of the crate, run against the test plugins (`crates/vst3-test-plugins`), which check a good deal of what a host has to do (the two halves connected, the
buses switched on, the channels given):

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_phase15_vst3_host.py --no-controls --results first.jsonl

Mistakes that cannot be seen by these tests, said plainly (left out of the list, not forgotten):

* `InitDll` / `ModuleEntry` saying no: no test plugin can say it (the entry points are the library's, not the class's).
* The latency read again when a plugin says it changed: the test plugin's latency does not change.
* A channel slice that is shorter than the block being used all the same: that is a read past the end of a buffer, which no test should do.
"""

import os

TESTS = os.environ.get("MUTATION_TESTS", "-p vst3-host --lib")

CLASS_ID = "crates/vst3-host/src/classid.rs"
STATE = "crates/vst3-host/src/state.rs"
INSTANCE = "crates/vst3-host/src/instance.rs"
DESCRIPTION = "crates/vst3-host/src/description.rs"
SCAN = "crates/vst3-host/src/scan.rs"
OBJECTS = "crates/vst3-host/src/objects.rs"

MUTATIONS = [
    # --- the id of a class ---
    ("class id: the first number is read the wrong way round (Windows)", CLASS_ID,
     "let a = (byte(3) << 24) | (byte(2) << 16) | (byte(1) << 8) | byte(0);",
     "let a = (byte(0) << 24) | (byte(1) << 16) | (byte(2) << 8) | byte(3);", TESTS),
    ("class id: the halves of the second number are not swapped (Windows)", CLASS_ID,
     "let b = (byte(5) << 24) | (byte(4) << 16) | (byte(7) << 8) | byte(6);",
     "let b = (byte(4) << 24) | (byte(5) << 16) | (byte(6) << 8) | byte(7);", TESTS),
    ("class id: the text is not padded to eight digits a number", CLASS_ID, '"{:08X}{:08X}{:08X}{:08X}"', '"{:X}{:X}{:X}{:X}"', TESTS),
    ("class id: a text of 31 digits is read", CLASS_ID, "if text.len() != 32 || !text.is_ascii() {", "if text.len() < 31 || !text.is_ascii() {", TESTS),
    # --- the state ---
    ("state: bytes after the end are no matter", STATE, "if !rest.is_empty() {", "if false {", TESTS),
    ("state: a part that is cut short is taken as far as it goes", STATE,
     "rest.split_at_checked(length).ok_or(StateError::NotOurs)", "Ok(rest.split_at(length.min(rest.len())))", TESTS),
    # --- what a factory says ---
    ("classes: the names in Unicode are not asked for", DESCRIPTION,
     "if unsafe { factory3.getClassInfoUnicode(index, &mut wide) } == kResultOk {",
     "if false && unsafe { factory3.getClassInfoUnicode(index, &mut wide) } == kResultOk {", TESTS),
    ("classes: an empty sub-category is a sub-category", DESCRIPTION, ".filter(|part| !part.is_empty())", "", TESTS),
    # --- what a class is ---
    ("buses: every bus is a main bus", INSTANCE, "is_main: info.busType == main,", "is_main: true,", TESTS),
    ("parameters: the texts of the steps are asked for the wrong values", INSTANCE,
     "f64::from(step) / f64::from(info.stepCount),", "f64::from(step),", TESTS),
    ("parameters: a hidden parameter is told read-only", INSTANCE,
     "is_hidden: flag(ParameterInfo_::ParameterFlags_::kIsHidden as _),", "is_hidden: flag(ParameterInfo_::ParameterFlags_::kIsReadOnly as _),", TESTS),
    # --- making a copy ---
    ("make: the controller is not told the handler", INSTANCE, "unsafe { controller.setComponentHandler(handler.as_ptr()) };", "let _ = &handler;", TESTS),
    ("make: the component is not connected to its controller", INSTANCE, "                    a.connect(b.as_ptr());\n", "", TESTS),
    ("make: the controller is not connected to its component", INSTANCE, "                    b.connect(a.as_ptr());\n", "", TESTS),
    ("make: every bus is switched on", INSTANCE, "TBool::from(Some(place) == wanted),", "TBool::from(true),", TESTS),
    ("make: no silent block before the first note of an instrument with a state", INSTANCE,
     "if state.is_some() && instance.facts.takes_events() {", "if false && state.is_some() && instance.facts.takes_events() {", TESTS),
    ("make: a state is given to the controller as well, but not the component's to the controller", INSTANCE,
     "unsafe { found.setComponentState(pointer.as_ptr()) };", "let _ = found;", TESTS),
    # --- values ---
    ("value: not held to the range", INSTANCE, ".clamp(0.0, 1.0);", ";", TESTS),
    ("value: the controller is not told", INSTANCE, "unsafe { controller.setParamNormalized(id, normalized) };", "let _ = controller;", TESTS),
    ("value: the points of a parameter are not put in order of the sample", OBJECTS,
     "order.sort_by_key(|place| (changes[*place].0, changes[*place].1));", "order.sort_by_key(|place| (changes[*place].0, 0));", TESTS),
    ("value: the changes are kept for the blocks after", INSTANCE, "        self.changes_in.fill(&self.pending);\n        self.pending.clear();\n",
     "        self.changes_in.fill(&self.pending);\n", TESTS),
    ("value: an edit of the plugin is not given to the next block", INSTANCE, "self.pending.push((id, 0, normalized));", "let _ = (id, normalized);", TESTS),
    # --- blocks ---
    ("run: a block as long as the copy was made for is too long", INSTANCE, "if frames > self.most_frames {", "if frames >= self.most_frames {", TESTS),
    ("run: the main input is any input", INSTANCE, "let is_main = Some(place) == self.main_input;", "let is_main = true;", TESTS),
    ("run: the main output is any output", INSTANCE, "let is_main = place == self.main_output;", "let is_main = true;", TESTS),
    ("run: a meter's value is added to the list each time", INSTANCE, "Some(known) => known.1 = value,", "Some(_) => self.outputs.push((id, value)),", TESTS),
    # --- notes ---
    ("notes: a note past the end of the block is not moved to its last sample", INSTANCE, ".clamp(0, (frames - 1).max(0));", ";", TESTS),
    ("notes: a note on has no id", INSTANCE,
     "                    length: 0,\n                    noteId: i32::try_from(id).unwrap_or(i32::MAX),", "                    length: 0,\n                    noteId: -1,", TESTS),
    ("notes: the pitch wheel's top is one past", INSTANCE, "f64::from(value.min(16383)) / 16383.0,", "f64::from(value.min(16383)) / 16384.0,", TESTS),
    ("notes: a controller's top is one past", INSTANCE, "f64::from(value) / 127.0, time);", "f64::from(value) / 128.0, time);", TESTS),
    ("notes: all notes off that nothing maps does not make the plugin forget", INSTANCE, "                    return true;\n", "                    return false;\n", TESTS),
    ("notes: a controller that nothing maps is not counted", INSTANCE, "                self.unmapped += 1;\n", "", TESTS),
    ("notes: the count of what was left out is not started again", INSTANCE, "std::mem::take(&mut self.unmapped)", "self.unmapped", TESTS),
    # --- after a restart ---
    ("reset: the plugin is not made inactive and active again", INSTANCE,
     "            self.component.setActive(0);\n            self.component.setActive(1);\n", "", TESTS),
    # --- the scan ---
    ("scan: a sound that is not numbers is no problem", SCAN, "(!is_made_of_numbers(&outputs)).then(", "(false && !is_made_of_numbers(&outputs)).then(", TESTS),
    ("scan: every plugin is said to have a state", SCAN, "instance.save_state().is_ok())", "true)", TESTS),
]
