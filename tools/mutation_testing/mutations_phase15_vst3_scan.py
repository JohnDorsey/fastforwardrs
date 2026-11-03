# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for phase 15, piece 15.2 (VST3 plugins found by the scan and made effects): the folders and the walks, the cache's fingerprint, the words between the scan
program and the editor, what a class makes of an effect, the host's answer for a VST3 plugin, and the editor's words.

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_phase15_vst3_scan.py --no-controls --results first.jsonl

The first pass is the library's tests of the modules named below; the scan program itself is run by the integration test `plugin_scan_vst3` (a mistake in `program.rs` is
aimed at it).
"""

import os

TESTS = os.environ.get("MUTATION_TESTS", "--lib -- vst3 plugin_scan::tests sound_plugins_tests")
SCAN_PROGRAM_TESTS = "--test plugin_scan_vst3"

EFFECTS = "src/plugin_scan/effects.rs"
FOLDERS = "src/plugin_scan/folders.rs"
CACHE = "src/plugin_scan/cache.rs"
PROTOCOL = "src/plugin_scan/protocol.rs"
PROGRAM = "src/plugin_scan/program.rs"
HOST = "src/plugin_host/host.rs"
FLOW = "src/application/fast_forward_application/sound_plugins_flow.rs"
MANAGER = "src/application/plugin_manager_window.rs"

HIDDEN_AND_THE_REST = (
    "if parameter.is_hidden || parameter.is_read_only || !parameter.can_automate || parameter.is_a_program_change {"
)

MUTATIONS = [
    # --- what a class makes of an effect ---
    ("effect: the key is in lower case", EFFECTS, 'format!("vst3:{}", class_id.text())',
     'format!("vst3:{}", class_id.text().to_lowercase())', TESTS),
    ("effect: a hidden parameter is a value", EFFECTS, HIDDEN_AND_THE_REST,
     "if parameter.is_read_only || !parameter.can_automate || parameter.is_a_program_change {", TESTS),
    ("effect: a read-only parameter is a value", EFFECTS, HIDDEN_AND_THE_REST,
     "if parameter.is_hidden || !parameter.can_automate || parameter.is_a_program_change {", TESTS),
    ("effect: a parameter that cannot be automated is a value", EFFECTS, HIDDEN_AND_THE_REST,
     "if parameter.is_hidden || parameter.is_read_only || parameter.is_a_program_change {", TESTS),
    ("effect: a change of program is a value", EFFECTS, HIDDEN_AND_THE_REST,
     "if parameter.is_hidden || parameter.is_read_only || !parameter.can_automate {", TESTS),
    ("effect: a range of one number is refused", EFFECTS,
     r'''    if !(least.is_finite() && most.is_finite() && least <= most) {
        return Err(format!(
            "its parameter \u{ab}{}\u{bb} has a range that is not a least that is no more than a most ({least} to {most})",
            parameter.title''',
     r'''    if !(least.is_finite() && most.is_finite() && least < most) {
        return Err(format!(
            "its parameter \u{ab}{}\u{bb} has a range that is not a least that is no more than a most ({least} to {most})",
            parameter.title''', TESTS),
    ("effect: a default past the range is kept", EFFECTS, "parameter.default_plain.clamp(least, most)", "parameter.default_plain", TESTS),
    ("effect: any one-step parameter is a switch", EFFECTS,
     "parameter.step_count == 1 && least == 0.0 && most == 1.0;", "parameter.step_count == 1 && least == 0.0;", TESTS),
    ("effect: more texts than steps make a choice", EFFECTS,
     "&& parameter.texts.len() == parameter.step_count as usize + 1", "&& parameter.texts.len() >= parameter.step_count as usize + 1", TESTS),
    ("effect: a range that is not as long as the steps makes a choice", EFFECTS,
     "&& (most - least - steps).abs() < 1e-6", "&& (most - least - steps).abs() < 100.0", TESTS),
    ("effect: a list that starts between whole numbers makes a choice", EFFECTS, "&& least.fract() == 0.0;", ";", TESTS),
    ("effect: the options of a choice begin at nought", EFFECTS,
     "let first = least as i64;\n        let options: Vec<ChoiceOption> = parameter\n            .texts",
     "let first = 0_i64;\n        let options: Vec<ChoiceOption> = parameter\n            .texts", TESTS),
    ("effect: a stepped parameter is a number with no steps", EFFECTS,
     "    } else if parameter.step_count > 0 {\n        (\n            ParameterControl::Integer,", "    } else if false {\n        (\n            ParameterControl::Integer,", TESTS),
    ("effect: the units are not trimmed", EFFECTS, "unit: Cow::Owned(parameter.units.trim().to_owned()),",
     "unit: Cow::Owned(parameter.units.clone()),", TESTS),
    ("effect: one channel is two", EFFECTS, "        1 => Ok(1),\n        2 => Ok(2),", "        1 => Ok(2),\n        2 => Ok(2),", TESTS),
    ("effect: a plugin with no main input is a synthesizer whether or not it takes notes", EFFECTS,
     "let is_a_synthesizer = main_input.is_none() && facts.takes_events();", "let is_a_synthesizer = main_input.is_none();", TESTS),
    ("effect: the cap of values is one past", EFFECTS,
     "            if parameters.len() == MOST_VALUES_OF_AN_EFFECT {\n                // (The first ones are kept, as for CLAP: the rest are the ones the plugin lists last.)\n                break;\n            }\n            parameters.push(value);\n            value_handles.push(parameter.id);\n        }\n    }\n    let file = module",
     "            if parameters.len() > MOST_VALUES_OF_AN_EFFECT {\n                // (The first ones are kept, as for CLAP: the rest are the ones the plugin lists last.)\n                break;\n            }\n            parameters.push(value);\n            value_handles.push(parameter.id);\n        }\n    }\n    let file = module", TESTS),
    ("effect: a tail of nothing is taken as a tail of nothing", EFFECTS,
     "    if tail == 0 {\n        pre_roll_of_a_plugin_that_says_nothing(sub_categories)",
     "    if false {\n        pre_roll_of_a_plugin_that_says_nothing(sub_categories)", TESTS),
    ("effect: the note about the window is for fewer values", EFFECTS,
     "if scanned.has_state && parameters.len() <= 2 {", "if scanned.has_state && parameters.len() < 2 {", TESTS),
    ("effect: the plugin's problem is not told", EFFECTS,
     '    if let Some(problem) = &scanned.problem {\n        return Err(format!("it could not be made or run: {problem}"));\n    }\n    let Some(facts) = &scanned.facts else {\n        return Err("it could not be made".to_owned());',
     '    let Some(facts) = &scanned.facts else {\n        return Err("it could not be made".to_owned());', TESTS),
    ("found: a plugin is named by its name and not by its id", EFFECTS,
     "                        &scanned.class.id.text(),\n                        &scanned.class.name,", "                        &scanned.class.name,\n                        &scanned.class.name,", TESTS),
    # --- folders ---
    ("folders: the ending is case sensitive", FOLDERS, '.is_some_and(|extension| extension.eq_ignore_ascii_case("vst3"))',
     '.is_some_and(|extension| extension == "vst3")', TESTS),
    ("folders: a folder that is being made is looked in", FOLDERS,
     "        } else if kind.is_dir() && folders_down > 0 && !is_being_made(&path) {\n            // (A folder that cannot be read is left out: the rest of the folder is still looked in.)\n            let _ = collect_vst3_modules",
     "        } else if kind.is_dir() && folders_down > 0 {\n            // (A folder that cannot be read is left out: the rest of the folder is still looked in.)\n            let _ = collect_vst3_modules", TESTS),
    ("folders: modules are looked for one folder down only", FOLDERS, "const MOST_FOLDERS_DOWN_FOR_VST3: usize = 6;",
     "const MOST_FOLDERS_DOWN_FOR_VST3: usize = 1;", TESTS),
    ("folders: the walk for LV2 bundles goes into a module", FOLDERS,
     "            && !is_a_clap_file(&path)\n            && !is_a_vst3_module(&path)\n        {", "            && !is_a_clap_file(&path)\n        {", TESTS),
    ("folders: the walk for CLAP libraries goes into a module", FOLDERS,
     "} else if kind.is_dir() && folders_down > 0 && !is_being_made(&path) && !is_a_vst3_module(&path) {",
     "} else if kind.is_dir() && folders_down > 0 && !is_being_made(&path) {", TESTS),
    # --- the cache ---
    ("cache: a module that is a folder is fingerprinted by the folder", CACHE, "(path.is_dir() && is_a_vst3_module(path))",
     "(false && is_a_vst3_module(path))", TESTS),
    # --- the words between the programs ---
    ("answer: a module without a factory is a library that would not load", PROTOCOL,
     "vst3_host::OpenError::NotAModule | vst3_host::OpenError::NoFactory => {",
     "vst3_host::OpenError::NotAModule => {", TESTS),
    # --- the scan program ---
    ("program: a module is read as another kind", PROGRAM, "let answer = if is_a_vst3_module(library) {",
     "let answer = if false && is_a_vst3_module(library) {", SCAN_PROGRAM_TESTS),
    # --- the host and the editor ---
    ("host: a VST3 plugin is run in the editor's process when the setting says nothing",
     "src/plugin_host/processes.rs",
     "        if matches!(plugin.address, PluginAddress::Vst3 { .. }) {\n            return true;\n        }",
     "        if false && matches!(plugin.address, PluginAddress::Vst3 { .. }) {\n            return true;\n        }",
     "--test plugin_host_processes -- vst3"),
    ("editor: a VST3 plugin that is gone is not explained", FLOW, 'namespace != "lv2" && namespace != "vst3"', 'namespace != "lv2"', TESTS),
    ("editor: the modules that failed are told for any kind of library", FLOW, '"vst3" => is_a_vst3_module(path),', '"vst3" => true,', TESTS),
    ("editor: a module is counted as a plugin", MANAGER, "let count = scan.classes.len();", "let count = 1;", TESTS),
    # --- a project of the other system finds its VST3 plugin (section 17) ---
    ("installed: any VST3 key finds any VST3 plugin", "src/project/effects/installed.rs",
     "            .any(|text| text.eq_ignore_ascii_case(named))", "            .any(|_| true)", TESTS),
    ("installed: another system's text finds a plugin that does not fit in the rest", "src/project/effects/installed.rs",
     "            && self.descriptor.fits_but_for_the_key(snapshot)\n", "", TESTS),
    ("installed: a key of another format that has the text of a class finds a VST3 plugin", "src/project/effects/installed.rs",
     "        let Some(named) = snapshot.key.strip_prefix(\"vst3:\") else {", "        let Some(named) = snapshot.key.split(':').next_back() else {", TESTS),
]
