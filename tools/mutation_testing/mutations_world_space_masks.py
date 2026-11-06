# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for the masks that are laid in space (docs/shader-masks-design.md, the section on masks in space): the text that the rewriting step makes for each shape, the numbers of the
placement and where they are in the call, the quaternion that turns a point of space into the frame of a shape, what the project and the file keep and refuse, the numbers that the plan makes,
the mask button and the window of the mask, and the three shaders that say they work in space. Each must be noticed by the tests made for it.

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_world_space_masks.py --no-controls --results first.jsonl

The words that pick the tests are in the names of the tests (the runner runs those whose names contain them).
"""

ISF = "--lib -- isf::rewrite::world_tests"
ENGINE = "--lib -- engine::world_mask_tests isf::rewrite::world_tests"
PROJECT = "--lib -- project::world_driven project::effects::driven::space"
FILE = "--lib -- project_file::world_driven"
COMPOSITION = "--lib -- composition::pictures::world_mask"
WINDOW = "--lib -- world_mask_ui_tests"
SHADERS = "--lib -- shaders_of_the_program::world_tests"

REWRITE_RS = "src/isf/rewrite.rs"
REGISTRY_RS = "src/isf/registry.rs"
SPACE_RS = "src/project/effects/driven/space.rs"
EFFECTS_RS = "src/project/effects.rs"
DRIVEN_VALUES_RS = "src/project/driven_values.rs"
FILE_RS = "src/project_file/effects.rs"
PICTURES_RS = "src/composition/pictures.rs"
BUTTONS_RS = "src/application/properties_panel/keyframe_buttons.rs"
WINDOW_RS = "src/application/mask_settings_window.rs"

MUTATIONS = [
    # --- what the rewriting step makes for each shape ---
    ("plane: the width is the height", REWRITE_RS,
     "vec2(_isf_d.x / _isf_size.x + 0.5, _isf_d.y / _isf_size.y + 0.5)", "vec2(_isf_d.x / _isf_size.y + 0.5, _isf_d.y / _isf_size.y + 0.5)", ENGINE),
    ("plane: the right edge is not an edge", REWRITE_RS,
     "_isf_at.x >= 0.0 && _isf_at.x <= 1.0 && _isf_at.y >= 0.0", "_isf_at.x >= 0.0 && _isf_at.x <= 2.0 && _isf_at.y >= 0.0", ENGINE),
    ("sphere: the angle round is taken from the wrong two coordinates", REWRITE_RS,
     "_isf_at = vec2(atan(_isf_d.x, _isf_d.z)", "_isf_at = vec2(atan(_isf_d.z, _isf_d.x)", ENGINE),
    ("sphere: the latitude takes a full turn for the picture's height", REWRITE_RS,
     "/ 3.14159265359 + 0.5);", "/ 6.28318530718 + 0.5);", ENGINE),
    ("cylinder: there is no top edge", REWRITE_RS,
     "_isf_at.y >= 0.0 && _isf_at.y <= 1.0) {\\n        _isf_at.x", "_isf_at.y >= 0.0) {\\n        _isf_at.x", ENGINE),
    ("cylinder: its height is its width", REWRITE_RS,
     "vec2 _isf_at = vec2(0.5, _isf_d.y / _isf_size.y + 0.5);", "vec2 _isf_at = vec2(0.5, _isf_d.y / _isf_size.x + 0.5);", ENGINE),
    ("space: a point is turned the wrong way into the frame of the shape", REWRITE_RS,
     "_isf_d = _isf_d + 2.0 * cross(", "_isf_d = _isf_d - 2.0 * cross(", ENGINE),
    ("space: the middle of the shape is added and not taken away", REWRITE_RS,
     "isf_world_position - _isf_middle", "isf_world_position + _isf_middle", ENGINE),
    ("space: a point that is off the shape is covered all the same", REWRITE_RS,
     "float _isf_cover = _isf_here.a * _isf_inside;", "float _isf_cover = _isf_here.a;", ENGINE),
    ("numbers: the width and the height are read the other way round", REWRITE_RS,
     "            number(7),\n            number(8),\n", "            number(8),\n            number(7),\n", ISF),
    ("numbers: one place in space overlaps the next", REWRITE_RS,
     "            input.world_at = Some(number);\n            number += NUMBERS_OF_A_WORLD_PLACEMENT;", "            input.world_at = Some(number);\n            number += NUMBERS_OF_A_WORLD_PLACEMENT - 1;", ISF),
    ("validation: an input that does not say world space is let into space", REWRITE_RS,
     "if !input.world_space {", "if false && !input.world_space {", ISF),
    ("validation: a shader that never sets the point is let have inputs in space", REWRITE_RS,
     "if first_use_of(&without_comments(&file.body), WORLD_POSITION).is_none() {", "if false && first_use_of(&without_comments(&file.body), WORLD_POSITION).is_none() {", ISF),
    ("set-up: the inputs in space are set up before main as well", REWRITE_RS,
     "        .filter(|input| input.read_from.space == Space::Screen)\n        .collect();\n    let mut masks", "        .collect();\n    let mut masks", ISF),
    ("global: the point of space is declared for every shader", REWRITE_RS,
     "if first_use_of(&code, WORLD_POSITION).is_some() {", "if true {", ISF),
    ("key: the shapes are one shader", REGISTRY_RS,
     'Space::World(shape) => format!(":in-space-{}", shape.word()),', 'Space::World(_) => ":in-space".to_owned(),', COMPOSITION),
    ("shader says: every input is said to work in space", REGISTRY_RS,
     "                .filter(|input| input.world_space)\n                .map(|input| input.name.clone())\n                .collect(),\n            size:", "                .map(|input| input.name.clone())\n                .collect(),\n            size:", PROJECT),
    # --- the numbers of a placement ---
    ("quaternion: the shape's turn is not taken back", SPACE_RS,
     "[0.0 - x, 0.0 - y, 0.0 - z, w]", "[x, y, z, w]", PROJECT),
    ("quaternion: the yaw goes the other way", SPACE_RS,
     "product(turn_about([0.0, 1.0, 0.0], yaw), turn_about([1.0, 0.0, 0.0], pitch))", "product(turn_about([0.0, 1.0, 0.0], -yaw), turn_about([1.0, 0.0, 0.0], pitch))", PROJECT),
    ("quaternion: the yaw is done before the pitch to the shape", SPACE_RS,
     "product(turn_about([0.0, 1.0, 0.0], yaw), turn_about([1.0, 0.0, 0.0], pitch))", "product(turn_about([1.0, 0.0, 0.0], pitch), turn_about([0.0, 1.0, 0.0], yaw))", PROJECT),
    ("numbers: the width and the height of a placement are the other way round", SPACE_RS,
     "[x, y, z, qx, qy, qz, qw, self.width, self.height]", "[x, y, z, qx, qy, qz, qw, self.height, self.width]", PROJECT),
    ("well-formed: a shape of no width is one", SPACE_RS,
     "(SMALLEST_SIZE_OF_A_SHAPE..=LARGEST_DISTANCE_OF_A_SHAPE).contains(size)", "(0.0..=LARGEST_DISTANCE_OF_A_SHAPE).contains(size)", PROJECT),
    ("shape: choosing another shape forgets where the shape was", SPACE_RS,
     "        Self { shape, ..self }", "        Self { shape, ..Self::default() }", PROJECT),
    # --- the project ---
    ("room: a place in space is not counted among the numbers of a call", EFFECTS_RS,
     "            + places * NUMBERS_OF_A_WORLD_PLACEMENT\n", "", PROJECT),
    ("room: the place being asked for is not counted", EFFECTS_RS,
     "places + 1) > crate::isf::MOST_NUMBERS_OF_A_CALL", "places) > crate::isf::MOST_NUMBERS_OF_A_CALL", PROJECT),
    ("space: every number is said to work in space", EFFECTS_RS,
     "                .any(|name| name.as_str() == &*parameter.id)\n    }\n\n    /// What a mask laid in space begins as", "                .any(|_| true)\n    }\n\n    /// What a mask laid in space begins as", PROJECT),
    ("space: the project lays a mask in space for a number that cannot", DRIVEN_VALUES_RS,
     "            if driven.space.is_in_space()\n", "            if false && driven.space.is_in_space()\n", PROJECT),
    ("size: a mask begins in space as the default size whatever the shader says", EFFECTS_RS,
     "                width: size,\n", "                width: default.width,\n", PROJECT),
    # --- the file ---
    ("file: a number that cannot work in space is let have its mask laid there", FILE_RS,
     "if !effect.can_the_value_be_laid_in_space(slot) {", "if false && !effect.can_the_value_be_laid_in_space(slot) {", FILE),
    ("file: a place that is not one is kept", FILE_RS,
     "if !placement.is_well_formed() {", "if false && !placement.is_well_formed() {", FILE),
    ("file: the place of a mask in space is not written", FILE_RS,
     "MaskSpace::World(placement) => Some(SpaceDocument::World((&placement).into())),", "MaskSpace::World(_) => None,", FILE),
    # --- the plan ---
    ("plan: the place of a mask in space is not in the call", PICTURES_RS,
     "        placement_numbers.extend(driven.space.numbers_of_the_call().into_iter().flatten());\n", "", COMPOSITION),
    ("plan: the places come before the graphs", PICTURES_RS,
     "graph_numbers.into_iter().chain(placement_numbers).collect()", "placement_numbers.into_iter().chain(graph_numbers).collect()", COMPOSITION),
    ("plan: a mask in space is made for the screen", PICTURES_RS,
     "            space: driven.space.for_the_shader(),", "            space: crate::isf::Space::Screen,", COMPOSITION),
    # --- the button and the window ---
    ("button: the name does not say that world space is available", BUTTONS_RS,
     "    let label = if mask.world_space_is_available {", "    let label = if false && mask.world_space_is_available {", WINDOW),
    ("button: no diamond is drawn", BUTTONS_RS,
     "    if with_a_diamond {\n        painter.add(", "    if false && with_a_diamond {\n        painter.add(", WINDOW),
    ("window: the choice of space is in the window of every number", WINDOW_RS,
     "if effect.can_the_value_be_laid_in_space(which.slot) {", "if true {", WINDOW),
    ("window: a mask laid in space begins as the default size", WINDOW_RS,
     "placement_or(effect.placement_in_space_to_begin_with())", "placement_or(WorldPlacement::default())", WINDOW),
    ("window: a sphere has a width", WINDOW_RS,
     "    if placement.shape == WorldShape::Plane {\n        ui.label(WIDTH_LABEL);", "    if true {\n        ui.label(WIDTH_LABEL);", WINDOW),
    # --- the shaders ---
    ("shader: the Mandelbulb does not say where it is", "assets/shaders/fractals/Mandelbulb.fs",
     "\tisf_world_position = point;\n", "", SHADERS),
    ("shader: the Folded Sponge does not say where it is", "assets/shaders/fractals/Folded Sponge.fs",
     "\tisf_world_position = point;\n", "", SHADERS),
    ("shader: the Mandelbox does not say where it is", "assets/shaders/fractals/Mandelbox.fs",
     "\tisf_world_position = point;\n", "", SHADERS),
    ("shader: the Mandelbulb says a number works in space that it uses outside the function of the shape", "assets/shaders/fractals/Mandelbulb.fs",
     '{ "NAME": "glowAmount", "LABEL": "Glow", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.5 }', '{ "NAME": "glowAmount", "LABEL": "Glow", "TYPE": "float", "MIN": 0.0, "MAX": 2.0, "DEFAULT": 0.5, "WORLD_SPACE": true }', SHADERS),
]
