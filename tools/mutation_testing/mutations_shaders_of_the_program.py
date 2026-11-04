# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for the shaders that come with the program (docs/own-shaders.md): the drawing of a shader to a picture (`shader_files::pictures`), the list and the scan of the
program's own shaders, how the editor installs them and shows one of them in its preview, and **the tests of the shaders themselves**: a mistake planted in a shader that makes one of its inputs do nothing must be noticed by
the test that says that every input changes the picture, a copy of a shared block that is not the true text must be noticed by the test that compares them, and a label or the words of a choice that
are too long for the properties panel must be noticed by the test that looks at the boxes as they are drawn.

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_shaders_of_the_program.py --no-controls --results first.jsonl

The words that pick the tests are in the names of the tests (the runner runs those whose names contain them).
"""

import os

PICTURES = os.environ.get("MUTATION_PICTURE_TESTS", "--lib -- shader_files::pictures")
OWN = os.environ.get("MUTATION_OWN_TESTS", "--lib -- shader_files::shaders_of_the_program")
WINDOW = os.environ.get("MUTATION_WINDOW_TESTS", "--lib -- shaders_of_the_program_tests")

PICTURES_RS = "src/shader_files/pictures.rs"
SCAN_RS = "src/shader_files/scan.rs"
OWN_RS = "src/shader_files/shaders_of_the_program.rs"
FLOW_RS = "src/application/fast_forward_application/sound_plugins_flow.rs"
LAYERS_RS = "src/composition/pictures.rs"

MUTATIONS = [
    # --- the scan of the program's own shaders ---
    ("scan: a shader of the program says it is in a file", SCAN_RS,
     '            "built into the program".to_owned()', '            "in a file".to_owned()', OWN),
    ("scan: the place of a shader is the other way round", SCAN_RS,
     "        if self.built_into_the_program {", "        if !self.built_into_the_program {", OWN),
    ("scan: the shaders that follow come first", SCAN_RS,
     "        self.shaders.extend(later.shaders);", "        self.shaders.splice(0..0, later.shaders);", OWN),
    ("scan: what went wrong in the later scan is lost", SCAN_RS,
     "        self.problems.extend(later.problems);", "        let _ = later.problems;", OWN),
    ("own: a shader of the program is not said to be built into it", OWN_RS,
     "                built_into_the_program: true,", "                built_into_the_program: false,", OWN),
    ("own: a shader of the program has no name", OWN_RS,
     "                file_name: shader.file_name.to_owned(),", "                file_name: String::new(),", OWN),
    # --- how the editor installs them ---
    ("flow: the editor that is told to install them does not", FLOW_RS,
     "    pub fn install_the_shaders_of_the_program(&mut self) {\n        self.environment.plugin_search.with_the_shaders_of_the_program = true;",
     "    pub fn install_the_shaders_of_the_program(&mut self) {\n        self.environment.plugin_search.with_the_shaders_of_the_program = false;", WINDOW),
    ("flow: the editor of a person does not install them", FLOW_RS,
     "    pub(super) fn find_plugins_in(&mut self, places: PlacesOfAPersonsPlugins) {\n        self.environment.plugin_search.with_the_shaders_of_the_program = true;",
     "    pub(super) fn find_plugins_in(&mut self, places: PlacesOfAPersonsPlugins) {", WINDOW),
    ("flow: nothing to look in and no program is the end of it, shaders of the program or not", FLOW_RS,
     "        if program.is_none() && shader_folders.is_empty() && !with_the_shaders_of_the_program {",
     "        if program.is_none() && shader_folders.is_empty() {", WINDOW),
    ("flow: the shaders of the folders come before those of the program", FLOW_RS,
     "                    shaders = scan_of_the_shaders_of_the_program().followed_by(shaders);",
     "                    shaders = shaders.followed_by(scan_of_the_shaders_of_the_program());", WINDOW),
    # --- a generator of the program in the preview of a window whose picture is made by the engine ---
    ("layers: a generator clip makes no layer, so there is nothing of it for the engine to draw", LAYERS_RS,
     "                Clip::Generator(_) => layers.push(candidate.entry(LayerContent::Generator)),",
     "                Clip::Generator(_) => {}", WINDOW),
    # --- drawing a shader to a picture ---
    ("pictures: a colour with no opacity is clear", PICTURES_RS,
     "            let opacity = if digits.len() == 8 { channel(6)? } else { 255 };",
     "            let opacity = if digits.len() == 8 { channel(6)? } else { 0 };", PICTURES),
    ("pictures: a pair is written with a comma", PICTURES_RS,
     "            let (across, down) = text.split_once('/').ok_or_else(wrong)?;",
     "            let (across, down) = text.split_once(',').ok_or_else(wrong)?;", PICTURES),
    ("pictures: a shader that remembers is drawn only at its time", PICTURES_RS,
     "        let first_frame = if self.remembers_earlier_frames { 0 } else { target_frame };",
     "        let first_frame = target_frame;", PICTURES),
    ("pictures: a shader is not told that it remembers", PICTURES_RS,
     "            remembers_earlier_frames: structure.depends_on_earlier_frames(),",
     "            remembers_earlier_frames: false,", PICTURES),
    ("sheet: the pictures go down before they go across", PICTURES_RS,
     "        let (column, row) = (number % across, number / across);", "        let (column, row) = (number / across, number % across);", PICTURES),
    ("sheet: a picture is put as far across as it is high", PICTURES_RS,
     "            let to = ((row * height + line) * sheet_width + column * width) * 4;",
     "            let to = ((row * height + line) * sheet_width + column * height) * 4;", PICTURES),
    ("files: what could not be drawn is counted once", PICTURES_RS,
     "                failed += asked.times.len() * sets.len();", "                failed += 1;", PICTURES),
    ("files: the time of a frame is not measured", PICTURES_RS,
     "                        if asked.more_times_to_measure > 0 && !installed.remembers_earlier_frames {",
     "                        if false && !installed.remembers_earlier_frames {", PICTURES),
    ("files: the sheet has one column", PICTURES_RS,
     "asked.sheet_columns.unwrap_or(asked.times.len())", "asked.sheet_columns.unwrap_or(1)", PICTURES),
    ("files: the WGSL is written when it is not asked for", PICTURES_RS,
     "        if asked.write_the_wgsl\n", "        if !asked.write_the_wgsl\n", PICTURES),
    # --- the tests of the shaders: an input that does nothing must be noticed ---
    ("shader: the speed of the wobble of Newton Basins does nothing", "assets/shaders/fractals/Newton Basins.fs",
     "float relax = relaxation + wobble * sin(TAU * wobbleSpeed * TIME);", "float relax = relaxation + wobble * sin(TAU * 0.12 * TIME);", OWN),
    ("shader: the speed of the point of Julia Dance does nothing", "assets/shaders/fractals/Julia Dance.fs",
     "the_point(pathStart + pathSpeed * TIME)", "the_point(pathStart + 0.04 * TIME)", OWN),
    ("shader: the speed of the wobble of the bulb does nothing", "assets/shaders/fractals/Mandelbulb.fs",
     "bulbPower + powerWobble * sin(TAU * wobbleSpeed * TIME)", "bulbPower + powerWobble * sin(TAU * 0.04 * TIME)", OWN),
    ("shader: the twist of Tunnel Warp does nothing", "assets/shaders/trippy/Tunnel Warp.fs",
     "float(wraps) + twist * depth * 0.1;", "float(wraps);", OWN),
    ("shader: the turn of the colours of Video Feedback does nothing", "assets/shaders/trippy/Video Feedback.fs",
     "hue_turned(echoed.rgb, radians(hueSpeed) * TIMEDELTA)", "hue_turned(echoed.rgb, 0.0)", OWN),
    ("shader: the amount of Ripple Rings does nothing", "assets/shaders/trippy/Ripple Rings.fs",
     "gl_FragColor = mix(original, vec4(colour, middle.a), amount);", "gl_FragColor = vec4(colour, middle.a);", OWN),
    ("shader: the second colour of Plasma Storm is never used (the palette does not reach it)", "assets/shaders/trippy/Plasma Storm.fs",
     "	return mix(colourA.rgb, colourB.rgb, x);", "	return mix(colourA.rgb, colourA.rgb, x);", OWN),
    # --- a copy of a shared block that is not the true text ---
    ("shared: one copy of the palettes is not the true text", "assets/shaders/trippy/Plasma Storm.fs",
     "return clamp(vec3(3.0 * x, 3.0 * x - 1.0, 3.0 * x - 2.0), 0.0, 1.0);", "return clamp(vec3(3.0 * x, 3.0 * x - 1.0, 3.0 * x - 1.9), 0.0, 1.0);", OWN),
    ("shared: an input is named like a word of a block", "assets/shaders/trippy/Liquid Warp.fs",
     '{ "NAME": "fineness",', '{ "NAME": "strength",', OWN),
    # --- what a person reads and reaches in the box of a shader stays in the panel (the test looks at the box as it is drawn) ---
    ("shader: the label of a number is so long that its box is under the buttons of its row", "assets/shaders/trippy/Plasma Storm.fs",
     '"LABEL": "Cycle speed"', '"LABEL": "Speed of the colour cycling"', WINDOW),
    ("shader: the words of a choice are so long that they make the box wider than the panel", "assets/shaders/trippy/Neon Edges.fs",
     '"The way the edge faces"', '"The way the edge faces, which is the way that it points"', WINDOW),
]
