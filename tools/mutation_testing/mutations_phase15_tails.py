# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for what was found by use after 15.6 and for the measured tail (sections 15 and 16 of docs/engine-phase-15-vst3-design.md): the tail that a reverb that says
nothing is given, the wake-up of the editor by a window, the meter of a ring, the list of what has been measured and the plan's use of it, when a measurement is made, and that a
measurement is made silently and only for a plugin that runs in a program of its own.

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_phase15_tails.py --no-controls --results first.jsonl

Left out as not seeable (said plainly): `the_measured_tail_with` returning at once for a synthesizer, before it reads anything. It changes nothing that a test can see (a
synthesizer is never in the list of what is measured, so the answer is none all the same); it only keeps the program from reading a state of tens of megabytes at each call.

The words that pick the tests are in the names of the tests (the runner runs those whose names contain them).
"""

import os

LIB = os.environ.get(
    "MUTATION_TESTS",
    "--lib -- measured_tails ring_out a_tail_length_that_is_said tail_measuring a_scan_that_ends a_synthesizer_is_not_measured",
)
HOST = os.environ.get(
    "MUTATION_HOST_TESTS", "--test plugin_host_processes -- measure ring a_vst3_effect_that_delays"
)
RULE = "--lib -- a_reverb_or_a_delay an_lv2_reverb_or_delay"
SESSIONS = "--test plugin_editor_sessions -- wakes_the_interface wakes_the_interface_to_say_so"
JOURNEY = "--test plugin_editor_vst3_journey -- asks_the_editor_to_draw"

SOUND = "src/composition/sound.rs"
MEASURED = "src/composition/sound/measured_tails.rs"
METER = "src/sound_engine/ring_out.rs"
RING_OUTS = "src/plugin_host/ring_outs.rs"
HOST_RS = "src/plugin_host/host.rs"
EFFECTS = "src/plugin_scan/effects.rs"
SESSIONS_RS = "src/plugin_editor/sessions.rs"
EDITOR_FLOW = "src/application/fast_forward_application/plugin_editor_flow.rs"
MEASURING_FLOW = "src/application/fast_forward_application/tail_measuring_flow.rs"
SCAN_FLOW = "src/application/fast_forward_application/sound_plugins_flow.rs"

MUTATIONS = [
    # --- after 15.6: a reverb that says no tail, and the wake-up ---
    ("rule: a reverb that says nothing is given what any plugin is", EFFECTS,
     "    if rings_on {\n        MOST_PRE_ROLL_SAMPLES\n    } else {\n        SAMPLE_RATE as usize / 2\n    }",
     "    let _ = rings_on;\n    SAMPLE_RATE as usize / 2", RULE),
    ("wake: the editor gives the sessions a wake-up that does nothing", EDITOR_FLOW,
     ".wake_the_interface_with(move || context.request_repaint());", ".wake_the_interface_with(move || {});", JOURNEY),
    ("wake: the reader does not wake the interface for a word", SESSIONS_RS,
     "                    let _ = sender.send(Said::Message(message));\n                    wake();",
     "                    let _ = sender.send(Said::Message(message));", SESSIONS),
    ("wake: the reader does not wake the interface at the end", SESSIONS_RS,
     "            let _ = sender.send(Said::Ended);\n            wake();", "            let _ = sender.send(Said::Ended);", SESSIONS),
    # --- the list of what has been measured, and the plan ---
    ("plan: a measured tail is ignored", SOUND,
     "TailLength::Automatic => the_measured_tail_with(kind, values, state).unwrap_or(plugin.pre_roll_samples as u64),",
     "TailLength::Automatic => { let _ = (values, state); plugin.pre_roll_samples as u64 }", LIB),
    ("signature: the tail is not heard", SOUND,
     "                tail_samples(\n                    layer.kind,\n                    layer.tail_length,\n                    &layer.values,\n                    TheState::Fingerprint(effect.effect.state_fingerprint()),\n                )\n                .hash(hasher);\n",
     "", LIB),
    ("key: the state is not part of it", MEASURED, "    state.fingerprint().hash(&mut hasher);\n", "    let _ = state;\n", LIB),
    ("key: the values are not part of it", MEASURED, "        value.hash_into(&mut hasher);\n", "        let _ = value;\n", LIB),
    ("note: every note counts as a change", MEASURED, "    if before != Some(samples) {", "    if true {", LIB),
    ("wanted: a tail that was said is measured too", MEASURED,
     "            && effect.tail_length() == TailLength::Automatic\n", "", LIB),
    ("wanted: an effect is wanted once for each time it is there", MEASURED,
     "                if seen.insert(key) {", "                if { seen.insert(key); true } {", LIB),
    ("wanted: a synthesizer is measured", MEASURED, "            && !effect.kind().makes_sound_from_notes()\n", "", LIB),
    # --- the meter ---
    ("meter: sixty decibels are thirty", METER,
     "pub const DECIBELS_THAT_IS_SILENCE: f64 = 60.0;", "pub const DECIBELS_THAT_IS_SILENCE: f64 = 30.0;", LIB),
    ("meter: a hum is a ring", METER,
     "    let average = samples.iter().map(|sample| f64::from(*sample)).sum::<f64>() / samples.len() as f64;",
     "    let average = 0.0;", LIB),
    ("meter: the latency is part of the ring", METER,
     "samples: end_of_the_ring.saturating_sub(end_of_the_input) as u64,", "samples: end_of_the_ring.saturating_sub(burst) as u64,", LIB),
    ("meter: a loud window does not end a quiet", METER,
     "quiet_windows = if power > loudest * silent { 0 } else { quiet_windows + 1 };", "quiet_windows += 1;", LIB),
    ("meter: it never says that it is still ringing", METER,
     "is_still_ringing: stopped_by_the_end && last_loud + 1 == powers.len(),", "is_still_ringing: false,", LIB),
    # --- when ---
    ("when: the project is looked at at once, however it is changing", MEASURING_FLOW,
     "        if quiet_for >= QUIET_BEFORE_MEASURING {", "        if true {", LIB),
    ("when: a claim is not let go of", RING_OUTS, "            ring_outs.in_flight.remove(key);\n", "", LIB),
    ("when: a failure is not remembered", RING_OUTS,
     "                ring_outs.could_not_be.insert(*key, generation);\n", "                let _ = generation;\n", LIB),
    ("when: a failure is tried again at once", RING_OUTS,
     "ring_outs.could_not_be.get(&want.key) == Some(&generation)", "ring_outs.could_not_be.get(&want.key) == Some(&u64::MAX)", LIB),
    ("when: a scan that ends does not forget what was measured", SCAN_FLOW,
     "        crate::composition::forget_the_measured_tails();\n", "", LIB),
    # --- the host ---
    ("host: a plugin that runs in the editor's process is measured", HOST_RS,
     "        if !self.runs_in_a_process_of_its_own().includes(plugin) {\n            return None;\n        }\n        let clip = SoundClipToExport {",
     "        let clip = SoundClipToExport {", HOST),
    ("host: a measurement wipes what the board says of the plugin", HOST_RS,
     "        if tells_the_boards {\n            // (What was wrong the last time", "        if true {\n            // (What was wrong the last time", HOST),
]
