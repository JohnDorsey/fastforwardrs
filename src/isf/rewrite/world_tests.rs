// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Inputs that are read from a mask **laid in space** (a plane, a sphere or a cylinder) instead of over the picture: what the rewriting step makes of them, in which numbers of the call
//! their placement is, and what it refuses. (`docs/shader-masks-design.md`, the section on masks in space.) What the generated code *does* is run on the software adapter by
//! `engine/world_mask_tests.rs`.

use super::*;
use crate::isf::read;

/// A filter whose inputs are `inputs` (each a float from 0 to 1 that says `WORLD_SPACE`, unless the text says otherwise) and whose `main` is `body`.
fn filter(inputs: &str, body: &str) -> IsfFile {
    let comma = if inputs.is_empty() { "" } else { "," };
    read(&format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}{comma} {inputs} ] }}*/\n{body}\n"
    ))
    .unwrap()
}

fn float_that_can_be_read_in_space(name: &str) -> String {
    format!(
        "{{ \"NAME\": \"{name}\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true }}"
    )
}

fn float_that_cannot(name: &str) -> String {
    format!("{{ \"NAME\": \"{name}\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5 }}")
}

/// A shader that says where it is working and uses `level` there.
const USES_THE_LEVEL_AT_A_POINT: &str =
    "void main() {\n\tisf_world_position = vec3(isf_FragNormCoord, 0.0);\n\tgl_FragColor = vec4(vec3(level), 1.0);\n}";

fn level_in_space(shape: WorldShape, mask: usize, channel: MaskChannel) -> DrivenInput {
    DrivenInput {
        name: "level".to_owned(),
        mask,
        channel,
        mapping: Mapping::Straight,
        space: Space::World(shape),
    }
}

fn the_level_file() -> IsfFile {
    filter(&float_that_can_be_read_in_space("level"), USES_THE_LEVEL_AT_A_POINT)
}

fn has_line(glsl: &str, line: &str) -> bool {
    glsl.lines().any(|found| found == line)
}

/// The text of `glsl` from the line that begins with `from` to the first line that is `}` after it.
fn function_beginning_with<'text>(glsl: &'text str, from: &str) -> &'text str {
    let start = glsl
        .find(from)
        .unwrap_or_else(|| panic!("no function begins with {from:?} in:\n{glsl}"));
    let end = glsl[start..].find("\n}\n").expect("a function ends") + start + 3;
    &glsl[start..end]
}

#[test]
fn a_shader_that_never_names_the_point_of_space_has_nothing_of_the_kind_in_its_text() {
    let file = filter(
        &float_that_can_be_read_in_space("level"),
        "void main() { gl_FragColor = vec4(vec3(level), 1.0); }",
    );
    for made in [
        rewrite_with_driven_inputs(&file, &[]).unwrap(),
        rewrite_with_driven_inputs(
            &file,
            &[DrivenInput {
                space: Space::Screen,
                ..level_in_space(WorldShape::Plane, 0, MaskChannel::Red)
            }],
        )
        .unwrap(),
    ] {
        assert!(!made.glsl.contains(WORLD_POSITION), "{}", made.glsl);
        assert!(!made.glsl.contains("_isf_world_"), "{}", made.glsl);
    }
}

#[test]
fn a_shader_that_names_the_point_of_space_has_it_as_a_global_whether_or_not_an_input_is_read_in_space() {
    let file = the_level_file();
    let none = rewrite_with_driven_inputs(&file, &[]).unwrap();
    let in_space =
        rewrite_with_driven_inputs(&file, &[level_in_space(WorldShape::Plane, 0, MaskChannel::Red)]).unwrap();

    for made in [&none, &in_space] {
        assert!(has_line(&made.glsl, "vec3 isf_world_position;"), "{}", made.glsl);
    }
    assert!(has_line(&none.glsl, "#define level (_isf_u[1].x)"));
}

#[test]
fn an_input_read_in_space_is_a_call_of_a_function_so_that_it_is_read_at_every_use_and_nothing_is_set_up_for_it_before_main()
 {
    let file = the_level_file();
    let made = rewrite_with_driven_inputs(&file, &[level_in_space(WorldShape::Plane, 0, MaskChannel::Red)]).unwrap();

    assert!(
        has_line(&made.glsl, "#define level (_isf_world_level())"),
        "{}",
        made.glsl
    );
    let function = function_beginning_with(&made.glsl, "float _isf_world_level() {");
    // The mask is read inside it, at the place on the shape, and not at the pixel.
    assert_eq!(
        function.matches("_isf_IMG_NORM_PIXEL__isf_mask_0(_isf_at)").count(),
        1,
        "{function}"
    );
    assert!(!function.contains("isf_FragNormCoord"), "{function}");
    // Nothing reads the mask before main runs: there is no function that sets up globals, no global, and main does not call it.
    assert!(!made.glsl.contains("_isf_set_up_driven"), "{}", made.glsl);
    assert!(!made.glsl.contains("_isf_driven_level"), "{}", made.glsl);
    // The mask is still an image of the shader, after its own.
    assert_eq!(
        made.image_names,
        vec!["inputImage".to_owned(), "_isf_mask_0".to_owned()]
    );
}

#[test]
fn a_mask_read_over_the_picture_by_one_input_and_in_space_by_another_is_read_once_at_the_pixel_and_again_at_each_use_in_space()
 {
    let file = filter(
        &format!(
            "{}, {}",
            float_that_can_be_read_in_space("level"),
            float_that_can_be_read_in_space("amount")
        ),
        "void main() { isf_world_position = vec3(0.0); gl_FragColor = vec4(level, amount, 0.0, 1.0); }",
    );
    let in_space = level_in_space(WorldShape::Sphere, 0, MaskChannel::Green);
    let over_the_picture = DrivenInput {
        name: "amount".to_owned(),
        mask: 0,
        channel: MaskChannel::Blue,
        mapping: Mapping::Straight,
        space: Space::Screen,
    };
    let made = rewrite_with_driven_inputs(&file, &[in_space, over_the_picture]).unwrap();

    assert!(has_line(&made.glsl, "#define level (_isf_world_level())"));
    assert!(has_line(&made.glsl, "#define amount (_isf_driven_amount)"));
    let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {");
    assert_eq!(
        set_up
            .matches("_isf_IMG_NORM_PIXEL__isf_mask_0(isf_FragNormCoord)")
            .count(),
        1,
        "{set_up}"
    );
    assert!(set_up.contains("_isf_driven_amount = "), "{set_up}");
    assert!(
        !set_up.contains("level"),
        "the input read in space is not set up before main: {set_up}"
    );
}

#[test]
fn a_mask_that_only_inputs_in_space_read_is_not_read_at_the_pixel() {
    let file = the_level_file();
    let made = rewrite_with_driven_inputs(
        &file,
        &[level_in_space(WorldShape::Cylinder, 0, MaskChannel::Brightness)],
    )
    .unwrap();

    // (The helpers that every image has read the picture at the pixel; the read of the set-up function is a variable of its own.)
    assert!(!made.glsl.contains("_isf_mask_0_here"), "{}", made.glsl);
    assert!(!made.glsl.contains("_isf_mask_0_cover"), "{}", made.glsl);
}

#[test]
fn each_shape_finds_the_nearest_place_in_its_own_way() {
    let file = the_level_file();
    let text = |shape| {
        let made = rewrite_with_driven_inputs(&file, &[level_in_space(shape, 0, MaskChannel::Red)]).unwrap();
        function_beginning_with(&made.glsl, "float _isf_world_level() {").to_owned()
    };
    let (plane, sphere, cylinder) = (
        text(WorldShape::Plane),
        text(WorldShape::Sphere),
        text(WorldShape::Cylinder),
    );

    // A plane is dropped straight onto: no angles. The others go round: the angle round y (x against z), and for the sphere the angle up from the middle's level.
    assert!(!plane.contains("atan("), "{plane}");
    assert!(!plane.contains("asin("), "{plane}");
    assert!(
        sphere.contains("atan(_isf_d.x, _isf_d.z)") && sphere.contains("asin("),
        "{sphere}"
    );
    assert!(
        cylinder.contains("atan(_isf_d.x, _isf_d.z)") && !cylinder.contains("asin("),
        "{cylinder}"
    );
    // The sphere has no use for the size and the cylinder only for the height: neither reads the width.
    assert!(!sphere.contains("_isf_size."), "{sphere}");
    assert!(!cylinder.contains("_isf_size.x"), "{cylinder}");
    assert!(
        plane.contains("_isf_size.x") && plane.contains("_isf_size.y"),
        "{plane}"
    );
}

#[test]
fn the_placement_of_each_input_read_in_space_is_nine_numbers_after_those_of_the_inputs_and_the_graphs_in_the_order_of_the_inputs()
 {
    let file = filter(
        &format!(
            "{}, {}",
            float_that_can_be_read_in_space("level"),
            float_that_can_be_read_in_space("amount")
        ),
        "void main() { isf_world_position = vec3(0.0); gl_FragColor = vec4(level, amount, 0.0, 1.0); }",
    );
    let level = DrivenInput {
        mapping: Mapping::Graph,
        ..level_in_space(WorldShape::Plane, 0, MaskChannel::Red)
    };
    let amount = DrivenInput {
        name: "amount".to_owned(),
        ..level_in_space(WorldShape::Plane, 0, MaskChannel::Red)
    };
    let made = rewrite_with_driven_inputs(&file, &[level, amount]).unwrap();

    // The four of the shader, then level (4) and amount (5), then level's graph (6 to 21), then the placement of level (22 to 30) and of amount (31 to 39).
    let level_function = function_beginning_with(&made.glsl, "float _isf_world_level() {");
    let amount_function = function_beginning_with(&made.glsl, "float _isf_world_amount() {");
    let places = |first: usize| {
        let middle = format!(
            "vec3({}, {}, {})",
            number_expression(first),
            number_expression(first + 1),
            number_expression(first + 2)
        );
        let turn = format!(
            "vec4({}, {}, {}, {})",
            number_expression(first + 3),
            number_expression(first + 4),
            number_expression(first + 5),
            number_expression(first + 6)
        );
        let size = format!(
            "vec2({}, {})",
            number_expression(first + 7),
            number_expression(first + 8)
        );
        (middle, turn, size)
    };
    for (function, first) in [(level_function, 22), (amount_function, 31)] {
        let (middle, turn, size) = places(first);
        for wanted in [middle, turn, size] {
            assert!(function.contains(&wanted), "{wanted} is not in:\n{function}");
        }
    }
    // The own number of each is where the mask has nothing, as for a mask over the picture.
    assert!(level_function.contains("mix(_isf_u[1].x,"), "{level_function}");
    assert!(amount_function.contains("mix(_isf_u[1].y,"), "{amount_function}");
}

#[test]
fn the_places_in_space_of_inputs_without_graphs_follow_the_numbers_of_the_inputs_at_once() {
    let file = the_level_file();
    let made = rewrite_with_driven_inputs(&file, &[level_in_space(WorldShape::Plane, 0, MaskChannel::Red)]).unwrap();

    // The four of the shader and one input: the placement is numbers 5 to 13.
    let function = function_beginning_with(&made.glsl, "float _isf_world_level() {");
    assert!(
        function.contains("vec3(_isf_u[1].y, _isf_u[1].z, _isf_u[1].w)"),
        "{function}"
    );
    assert!(
        function.contains("vec4(_isf_u[2].x, _isf_u[2].y, _isf_u[2].z, _isf_u[2].w)"),
        "{function}"
    );
    assert!(function.contains("vec2(_isf_u[3].x, _isf_u[3].y)"), "{function}");
}

#[test]
fn an_input_that_does_not_say_world_space_cannot_be_read_in_space() {
    let file = filter(&float_that_cannot("level"), USES_THE_LEVEL_AT_A_POINT);
    let problem = rewrite_with_driven_inputs(&file, &[level_in_space(WorldShape::Plane, 0, MaskChannel::Red)])
        .expect_err("the header does not say so");

    assert!(problem.message.contains("level"), "{}", problem.message);
    assert!(problem.message.contains("WORLD_SPACE"), "{}", problem.message);
    // (Over the picture it can: the header's promise is about space only.)
    let over_the_picture = DrivenInput {
        space: Space::Screen,
        ..level_in_space(WorldShape::Plane, 0, MaskChannel::Red)
    };
    assert!(rewrite_with_driven_inputs(&file, &[over_the_picture]).is_ok());
}

#[test]
fn a_shader_that_never_sets_the_point_of_space_cannot_have_an_input_read_in_space() {
    let file = filter(
        &float_that_can_be_read_in_space("level"),
        "void main() { gl_FragColor = vec4(vec3(level), 1.0); }",
    );
    let problem = rewrite_with_driven_inputs(&file, &[level_in_space(WorldShape::Plane, 0, MaskChannel::Red)])
        .expect_err("there is no point of space");

    assert!(problem.message.contains(WORLD_POSITION), "{}", problem.message);
    assert!(problem.message.contains("level"), "{}", problem.message);
}

#[test]
fn a_comment_that_names_the_point_of_space_does_not_count_as_setting_it() {
    let file = filter(
        &float_that_can_be_read_in_space("level"),
        "// isf_world_position is not set here\nvoid main() { gl_FragColor = vec4(vec3(level), 1.0); }",
    );

    assert!(rewrite_with_driven_inputs(&file, &[level_in_space(WorldShape::Plane, 0, MaskChannel::Red)]).is_err());
    assert!(
        !rewrite_with_driven_inputs(&file, &[])
            .unwrap()
            .glsl
            .contains("vec3 isf_world_position;")
    );
}

#[test]
fn only_a_float_can_say_world_space_in_its_header() {
    let problem = read("/*{ \"INPUTS\": [ { \"NAME\": \"flag\", \"TYPE\": \"bool\", \"WORLD_SPACE\": true } ] }*/\nvoid main() { gl_FragColor = vec4(1.0); }\n")
        .expect_err("a bool cannot be read from a mask");
    assert!(
        problem.message.contains("flag") && problem.message.contains("WORLD_SPACE"),
        "{}",
        problem.message
    );

    let problem = read("/*{ \"INPUTS\": [ { \"NAME\": \"level\", \"TYPE\": \"float\", \"WORLD_SPACE\": \"yes\" } ] }*/\nvoid main() { gl_FragColor = vec4(1.0); }\n")
        .expect_err("WORLD_SPACE is true or false");
    assert!(
        problem.message.contains("level") && problem.message.contains("true or false"),
        "{}",
        problem.message
    );

    let file = read("/*{ \"INPUTS\": [ { \"NAME\": \"a\", \"TYPE\": \"float\", \"WORLD_SPACE\": true }, { \"NAME\": \"b\", \"TYPE\": \"float\", \"WORLD_SPACE\": false }, { \"NAME\": \"c\", \"TYPE\": \"float\" } ] }*/\nvoid main() { gl_FragColor = vec4(1.0); }\n").unwrap();
    assert_eq!(
        file.inputs.iter().map(|input| input.world_space).collect::<Vec<_>>(),
        vec![true, false, false]
    );
}

#[test]
fn a_placement_that_has_no_room_among_the_numbers_of_the_call_is_refused_and_an_input_over_the_picture_never_is() {
    // Fifty-six numbers of inputs and the four of the shader fill 60 of the 64; the nine of a placement do not fit.
    let floats: Vec<String> = (0..56)
        .map(|number| float_that_can_be_read_in_space(&format!("p{number}")))
        .collect();
    let file = filter(
        &floats.join(","),
        "void main() { isf_world_position = vec3(0.0); gl_FragColor = IMG_THIS_PIXEL(inputImage) * p0; }",
    );
    let in_space = DrivenInput {
        name: "p3".to_owned(),
        ..level_in_space(WorldShape::Plane, 0, MaskChannel::Red)
    };

    let problem = rewrite_with_driven_inputs(&file, std::slice::from_ref(&in_space)).expect_err("no room");
    assert!(
        problem.message.contains("room") && problem.message.contains("p3"),
        "{}",
        problem.message
    );
    let over_the_picture = DrivenInput {
        space: Space::Screen,
        ..in_space
    };
    assert!(rewrite_with_driven_inputs(&file, &[over_the_picture]).is_ok());
}

#[test]
fn what_the_generated_function_names_cannot_be_the_name_of_an_input() {
    // An input's name is a #define, so a local of a generated function that was called the same would be replaced by the input's expression: every local has an _isf_ name.
    let file = filter(
        &["middle", "turn", "size", "d", "at", "inside", "away", "level"]
            .map(float_that_can_be_read_in_space)
            .join(","),
        "void main() { isf_world_position = vec3(0.0); gl_FragColor = vec4(level, middle, turn, size + d + at + inside + away); }",
    );
    for shape in WorldShape::ALL {
        let made = rewrite_with_driven_inputs(&file, &[level_in_space(shape, 0, MaskChannel::Red)]).unwrap();
        let function = function_beginning_with(&made.glsl, "float _isf_world_level() {");
        for name in ["middle", "turn", "size", "inside", "away", "d", "at"] {
            let as_a_word = function
                .split(|character: char| !(character.is_alphanumeric() || character == '_'))
                .any(|word| word == name);
            assert!(
                !as_a_word,
                "{name} is a word of the function for {shape:?}:\n{function}"
            );
        }
    }
}

#[test]
fn the_header_may_say_how_many_units_across_the_shape_of_the_shader_is_and_a_size_that_is_not_one_is_refused() {
    let with = |size: &str| {
        read(&format!(
            "/*{{ \"WORLD_SIZE\": {size}, \"INPUTS\": [ {{ \"NAME\": \"a\", \"TYPE\": \"float\" }} ] }}*/\nvoid main() {{ gl_FragColor = vec4(1.0); }}\n"
        ))
    };

    assert_eq!(with("23.0").unwrap().world_size, Some(23.0));
    assert_eq!(with("0.5").unwrap().world_size, Some(0.5));
    for bad in ["0", "-3.0", "\"big\"", "true", "2000000.0"] {
        let problem = with(bad).expect_err(bad);
        assert!(problem.message.contains("WORLD_SIZE"), "{bad}: {}", problem.message);
    }
    let none = read("/*{ \"INPUTS\": [] }*/\nvoid main() { gl_FragColor = vec4(1.0); }\n").unwrap();
    assert_eq!(none.world_size, None);
}
