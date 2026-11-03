# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for phase 15, piece 15.3 (VST3 plugins run in a program of their own): the copy that the program holds, the program's making of it, what says whether it is the
plugin that was scanned, the notes, and the host's choice to run a VST3 plugin in a program whatever the setting.

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_phase15_vst3_process.py --no-controls --results first.jsonl

The tests are those of `tests/plugin_process.rs` and `tests/plugin_host_processes.rs` whose names say `vst3`, which run the real program (`fastforwardrs-plugin-process`) with the test
plugins (`crates/vst3-test-plugins`).

Mistakes that cannot be seen by these tests, said plainly (left out of the list, not forgotten):

* The words said when a plugin's latency changed (alone, or when it was made again): no test plugin changes its latency.
* `most_samples_at_once` and `takes_values_at_samples` of the copy: the program and a copy in a test process say the same, whatever they say, and the blocks are never longer than the
  most that the copy is set up for.
"""

import os

TESTS = os.environ.get("MUTATION_TESTS", "--test plugin_process --test plugin_host_processes -- vst3")

COPY = "src/plugin_host/vst3_copy.rs"
PROGRAM = "src/plugin_process/program.rs"
SCANNED = "src/plugin_host/scanned.rs"
PROCESSES = "src/plugin_host/processes.rs"
HOST = "src/plugin_host/host.rs"
NOTES = "src/sound_engine/note_events.rs"

MUTATIONS = [
    # --- the copy ---
    ("copy: a value is given at the first sample of the block", COPY,
     "self.instance.set_value(handle, f64::from(value), at_sample);", "self.instance.set_value(handle, f64::from(value), 0);", TESTS),
    ("copy: the notes of a run are not given", COPY, ".run_with_notes(inputs, outputs, frames, &notes)", ".run_with_notes(inputs, outputs, frames, &[])", TESTS),
    ("copy: a reset does nothing", COPY, "    fn reset(&mut self) {\n        self.instance.reset();\n    }", "    fn reset(&mut self) {}", TESTS),
    ("copy: the latency is not told", COPY, "    fn latency(&self) -> usize {\n        self.instance.latency() as usize", "    fn latency(&self) -> usize {\n        0", TESTS),
    ("copy: controllers that were left out are told at every block", COPY,
     "take_the_count_of_events_left_out() > 0 && !self.said_that_events_were_left_out {", "take_the_count_of_events_left_out() > 0 {", TESTS),
    ("copy: controllers that were left out are never told", COPY,
     "take_the_count_of_events_left_out() > 0 && !self.said_that_events_were_left_out {", "take_the_count_of_events_left_out() > 1000 && !self.said_that_events_were_left_out {", TESTS),
    ("copy: a plugin is made again one time too many", COPY, "if self.restarts < MOST_RESTARTS_OF_A_COPY {", "if self.restarts <= MOST_RESTARTS_OF_A_COPY {", TESTS),
    ("copy: a plugin that was made again is not said to be", COPY, "                        was_restarted = true;\n", "", TESTS),
    ("copy: it is never said that a plugin is not made again any more", COPY,
     "} else if self.restarts == MOST_RESTARTS_OF_A_COPY {", "} else if self.restarts > MOST_RESTARTS_OF_A_COPY {", TESTS),
    ("copy: a latency that did not change is said to have", COPY,
     "} else if asked.latency_changed && self.instance.latency() != latency_before {", "} else if asked.latency_changed {", TESTS),
    ("copy: a plugin that is made again is said to have another latency every time", COPY,
     "let latency_changed = again.latency() != self.instance.latency();", "let latency_changed = again.latency() == self.instance.latency();", TESTS),
    # --- the program makes it ---
    ("program: a class the module has not is as good as one it has", PROGRAM,
     ".any(|class| class.id == class_id && class.is_a_plugin())", ".any(|class| class.is_a_plugin())", TESTS),
    ("program: a class that is not a plugin is as good as one that is", PROGRAM,
     ".any(|class| class.id == class_id && class.is_a_plugin())", ".any(|class| class.id == class_id)", TESTS),
    ("program: a copy is not checked against what was scanned", PROGRAM, "if !make.scanned.is_this_vst3_plugin(&instance) {", "if false && !make.scanned.is_this_vst3_plugin(&instance) {", TESTS),
    ("program: the state is not given to the plugin", PROGRAM, "    let maker = Vst3Maker {\n        module,\n        class_id,\n        state,\n    };", "    let maker = Vst3Maker {\n        module,\n        class_id,\n        state: None,\n    };", TESTS),
    ("program: what is said of a state that was not taken is not said", PROGRAM, "Ok((Box::new(Vst3Copy::new(instance, maker)), state_not_taken))", "Ok((Box::new(Vst3Copy::new(instance, maker)), None))", TESTS),
    # --- what says whether it is what was scanned ---
    ("scanned: only the outputs are looked at", SCANNED, "instance.channels() == (self.audio_inputs, self.audio_outputs)", "instance.channels().1 == self.audio_outputs", TESTS),
    ("scanned: only the inputs are looked at", SCANNED, "instance.channels() == (self.audio_inputs, self.audio_outputs)", "instance.channels().0 == self.audio_inputs", TESTS),
    ("scanned: one parameter of the values is enough", SCANNED, "&& self.value_handles.iter().all(|handle| {", "&& self.value_handles.iter().any(|handle| {", TESTS),
    # --- the host ---
    ("host: a VST3 plugin is run in a program only if the setting says so", PROCESSES,
     "if matches!(plugin.address, PluginAddress::Vst3 { .. }) {\n            return true;\n        }", "if false && matches!(plugin.address, PluginAddress::Vst3 { .. }) {\n            return true;\n        }", TESTS),
    ("host: a module is called a library", HOST, 'PluginAddress::Vst3 { .. } => "module",', 'PluginAddress::Vst3 { .. } => "library",', TESTS),
    # --- the notes ---
    ("notes: a note on is a note off for a VST3 plugin", NOTES, "} => vst3_host::NoteEventKind::On {", "} => vst3_host::NoteEventKind::Off {", TESTS),
    ("notes: the pitch wheel is always in the middle for a VST3 plugin", NOTES,
     "NoteEventKind::PitchBend { channel, value } => vst3_host::NoteEventKind::PitchBend { channel, value },",
     "NoteEventKind::PitchBend { channel, value } => vst3_host::NoteEventKind::PitchBend { channel, value: value.min(1) },", TESTS),
    ("notes: all off is a controller for a VST3 plugin", NOTES, "NoteEventKind::AllOff => vst3_host::NoteEventKind::AllOff,",
     "NoteEventKind::AllOff => vst3_host::NoteEventKind::Controller { channel: 0, number: 1, value: 0 },", TESTS),
]
