// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Inputs that are read from a mask (a picture) instead of from a slider: what the rewriting step makes of them, and what it refuses. (`docs/shader-masks-design.md`.)

use super::*;
use crate::isf::read;

const COLOR_CONTROLS: &str = include_str!("../fixtures/color_controls.fs");
const OLD_STYLE: &str = include_str!("../fixtures/old_style.fs");
const BLUR: &str = include_str!("../fixtures/gaussian_blur.fs");
const POSTERIZE: &str = include_str!("../fixtures/posterize.fs");
const VIGNETTE: &str = include_str!("../fixtures/vignette.fs");

/// A filter with `inputs` (more inputs, with a comma between) and `body` as its GLSL.
fn filter(inputs: &str, body: &str) -> IsfFile {
    let comma = if inputs.is_empty() { "" } else { "," };
    read(&format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}{comma} {inputs} ] }}*/\n{body}\n"
    ))
    .unwrap()
}

/// A filter with two floats, `radius` (0 to 10) and `amount` (-1 to 3), and `body`.
fn filter_with_two_floats(body: &str) -> IsfFile {
    filter(
        "{ \"NAME\": \"radius\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 10.0, \"DEFAULT\": 1.0 }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"MIN\": -1.0, \"MAX\": 3.0, \"DEFAULT\": 0.0 }",
        body,
    )
}

fn driven(name: &str, mask: usize, channel: MaskChannel) -> DrivenInput {
    DrivenInput {
        name: name.to_owned(),
        mask,
        channel,
    }
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

const A_LOOP_THAT_USES_THE_INPUTS_MANY_TIMES: &str = "float ratio = radius * 2.0;\nvoid main() {\n\tvec4 sum = vec4(0.0);\n\tfor (int i = 0; i < 8; i++) {\n\t\tsum += amount * IMG_NORM_PIXEL(inputImage, isf_FragNormCoord + vec2(radius * 0.01 * float(i), 0.0));\n\t}\n\tgl_FragColor = sum * ratio;\n}";

#[test]
fn a_shader_with_no_driven_inputs_is_rewritten_as_it_always_was() {
    for text in [COLOR_CONTROLS, OLD_STYLE, BLUR, POSTERIZE, VIGNETTE] {
        let file = read(text).unwrap();
        let plain = rewrite(&file).unwrap();
        let with_none = rewrite_with_driven_inputs(&file, &[]).unwrap();

        assert_eq!(plain.glsl, with_none.glsl);
        assert_eq!(plain.image_names, with_none.image_names);
        assert_eq!(plain.notes, with_none.notes);
        for left_out in ["_isf_driven", "_isf_mask", "_isf_set_up_driven"] {
            assert!(
                !plain.glsl.contains(left_out),
                "{left_out} is in a shader that has no driven input"
            );
        }
    }
}

#[test]
fn a_driven_input_is_a_global_that_a_function_fills_in_and_main_calls_before_it_sets_up_the_globals() {
    let file = filter_with_two_floats(A_LOOP_THAT_USES_THE_INPUTS_MANY_TIMES);
    let made = rewrite_with_driven_inputs(&file, &[driven("radius", 0, MaskChannel::Red)]).unwrap();

    // The name of the input is the global; the other input is as it was.
    assert!(has_line(&made.glsl, "#define radius (_isf_driven_radius)"));
    assert!(has_line(&made.glsl, "#define amount (_isf_u[1].y)"));
    assert!(has_line(&made.glsl, "float _isf_driven_radius;"));
    let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {");
    assert!(set_up.contains("_isf_driven_radius = "), "{set_up}");
    // main runs it first of all: before the initialisers of the globals (`ratio` uses `radius`) and before the shader's own main.
    let main = function_beginning_with(&made.glsl, "void main() {");
    let places: Vec<usize> = ["_isf_set_up_driven();", "_isf_set_up_globals();", "_isf_user_main();"]
        .iter()
        .map(|call| {
            main.find(call)
                .unwrap_or_else(|| panic!("main does not call {call}:\n{main}"))
        })
        .collect();
    assert!(places[0] < places[1] && places[1] < places[2], "{main}");
    // (The initialiser of `ratio` is among the globals that are set up, and it is where `radius` is used.)
    let globals = function_beginning_with(&made.glsl, "void _isf_set_up_globals() {");
    assert!(globals.contains("ratio = radius * 2.0;"), "{globals}");
}

#[test]
fn the_mask_is_read_once_per_pixel_however_many_times_the_shader_uses_the_input() {
    let file = filter_with_two_floats(A_LOOP_THAT_USES_THE_INPUTS_MANY_TIMES);
    let made = rewrite_with_driven_inputs(&file, &[driven("radius", 0, MaskChannel::Green)]).unwrap();

    let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {");
    assert_eq!(
        set_up
            .matches("_isf_IMG_NORM_PIXEL__isf_mask_0(isf_FragNormCoord)")
            .count(),
        1,
        "{set_up}"
    );
    // The shader's own code (everything after the function that sets the inputs up, which is part of the beginning that is generated) never mentions the mask, and its uses of
    // `radius` (a loop, an initialiser) are of the global.
    let from_the_set_up = &made.glsl[made.glsl.find("void _isf_set_up_driven() {").expect("the set-up")..];
    let the_shader = &from_the_set_up[from_the_set_up.find("\n}\n").expect("the set-up ends") + 3..];
    assert!(
        the_shader.contains("float ratio;"),
        "the shader follows the set-up: {the_shader}"
    );
    assert!(!the_shader.contains("_isf_mask"), "{the_shader}");
    assert!(the_shader.contains("radius * 0.01"), "{the_shader}");
}

#[test]
fn two_inputs_of_one_mask_share_its_one_read_and_each_has_the_channel_it_was_given() {
    let file = filter_with_two_floats("void main() { gl_FragColor = vec4(radius, amount, 0.0, 1.0); }");
    let made = rewrite_with_driven_inputs(
        &file,
        &[
            driven("radius", 0, MaskChannel::Blue),
            driven("amount", 0, MaskChannel::Brightness),
        ],
    )
    .unwrap();

    let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {");
    assert_eq!(
        set_up
            .matches("_isf_IMG_NORM_PIXEL__isf_mask_0(isf_FragNormCoord)")
            .count(),
        1,
        "two inputs of one mask read it once: {set_up}"
    );
    assert!(set_up.contains("_isf_driven_radius = "), "{set_up}");
    assert!(set_up.contains("_isf_driven_amount = "), "{set_up}");
    assert_eq!(
        made.image_names
            .iter()
            .filter(|name| name.starts_with("_isf_mask_"))
            .count(),
        1
    );
}

#[test]
fn each_channel_is_its_own_expression_and_brightness_is_the_luma_of_the_codes() {
    let file = filter_with_two_floats("void main() { gl_FragColor = vec4(radius); }");
    let expression_of = |channel: MaskChannel| {
        let made = rewrite_with_driven_inputs(&file, &[driven("radius", 0, channel)]).unwrap();
        let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {").to_owned();
        set_up
            .lines()
            .find(|line| line.contains("_isf_unit_of_the_channel"))
            .unwrap_or_else(|| panic!("no line says the channel:\n{set_up}"))
            .trim()
            .to_owned()
    };

    assert!(expression_of(MaskChannel::Red).contains("_isf_mask_0_here.r"));
    assert!(expression_of(MaskChannel::Green).contains("_isf_mask_0_here.g"));
    assert!(expression_of(MaskChannel::Blue).contains("_isf_mask_0_here.b"));
    let brightness = expression_of(MaskChannel::Brightness);
    assert!(
        brightness.contains("dot(_isf_mask_0_here.rgb, vec3(0.2126, 0.7152, 0.0722))"),
        "brightness is the Rec. 709 luma of the codes: {brightness}"
    );
}

#[test]
fn where_the_mask_has_nothing_the_value_is_the_inputs_own_number_and_elsewhere_it_runs_from_the_least_to_the_most() {
    // `radius` is 0 to 10 and is number 4 (the first input after the shader's four): `_isf_u[1].x`; `amount` is -1 to 3, number 5.
    let file = filter_with_two_floats("void main() { gl_FragColor = vec4(radius, amount, 0.0, 1.0); }");
    let made = rewrite_with_driven_inputs(
        &file,
        &[
            driven("radius", 0, MaskChannel::Red),
            driven("amount", 0, MaskChannel::Red),
        ],
    )
    .unwrap();

    let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {");
    // The coverage of the mask is its opacity; the unit is the channel taken out of the premultiplied colour; the value is the number of the call where nothing covers, the
    // range of the input where all does, and a mixture between.
    assert!(
        set_up.contains("float _isf_mask_0_cover = _isf_mask_0_here.a;"),
        "{set_up}"
    );
    assert!(
        set_up.contains(
            "_isf_driven_radius = mix(_isf_u[1].x, 0.0 + 10.0 * _isf_unit_of_the_channel_radius, _isf_mask_0_cover);"
        ),
        "{set_up}"
    );
    assert!(
        set_up.contains(
            "_isf_driven_amount = mix(_isf_u[1].y, -1.0 + 4.0 * _isf_unit_of_the_channel_amount, _isf_mask_0_cover);"
        ),
        "{set_up}"
    );
    // The colour of a mask is premultiplied, so the unit divides by the coverage (and is zero where there is none, and never leaves 0 to 1).
    assert!(
        set_up.contains("(_isf_mask_0_cover > 0.0) ? clamp(_isf_mask_0_here.r / _isf_mask_0_cover, 0.0, 1.0) : 0.0"),
        "{set_up}"
    );
}

#[test]
fn the_masks_are_the_last_images_a_texture_and_a_sampler_each_after_the_images_of_the_shader() {
    let file = read(BLUR).unwrap();
    let plain = rewrite(&file).unwrap();
    assert_eq!(plain.image_names, ["inputImage", "bufferHorizontal"]);
    let float_name = file
        .inputs
        .iter()
        .find(|input| matches!(input.kind, IsfInputKind::Float { .. }))
        .expect("the blur has a float")
        .name
        .clone();

    let made = rewrite_with_driven_inputs(&file, &[driven(&float_name, 0, MaskChannel::Red)]).unwrap();

    assert_eq!(made.image_names, ["inputImage", "bufferHorizontal", "_isf_mask_0"]);
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 5) uniform texture2D _isf_mask_0_tex;"
    ));
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 6) uniform sampler _isf_mask_0_samp;"
    ));
    // The images the shader had are where they were.
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 1) uniform texture2D inputImage_tex;"
    ));
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 3) uniform texture2D bufferHorizontal_tex;"
    ));
}

#[test]
fn two_masks_are_two_images_in_the_order_of_their_numbers() {
    let file = filter_with_two_floats("void main() { gl_FragColor = vec4(radius, amount, 0.0, 1.0); }");
    let made = rewrite_with_driven_inputs(
        &file,
        &[
            driven("amount", 1, MaskChannel::Green),
            driven("radius", 0, MaskChannel::Red),
        ],
    )
    .unwrap();

    assert_eq!(made.image_names, ["inputImage", "_isf_mask_0", "_isf_mask_1"]);
    let set_up = function_beginning_with(&made.glsl, "void _isf_set_up_driven() {");
    assert_eq!(
        set_up
            .matches("_isf_IMG_NORM_PIXEL__isf_mask_0(isf_FragNormCoord)")
            .count(),
        1
    );
    assert_eq!(
        set_up
            .matches("_isf_IMG_NORM_PIXEL__isf_mask_1(isf_FragNormCoord)")
            .count(),
        1
    );
}

#[test]
fn an_input_that_does_not_exist_or_is_not_a_float_cannot_be_driven() {
    let file = filter(
        "{ \"NAME\": \"on\", \"TYPE\": \"bool\" }, { \"NAME\": \"pick\", \"TYPE\": \"long\", \"VALUES\": [1, 2] }, { \"NAME\": \"tint\", \"TYPE\": \"color\" }, { \"NAME\": \"spot\", \"TYPE\": \"point2D\" }, { \"NAME\": \"gain\", \"TYPE\": \"float\" }",
        "void main() { gl_FragColor = vec4(gain); }",
    );
    for (name, why) in [
        ("nothing", "has no input called nothing"),
        ("on", "is not a number"),
        ("pick", "is not a number"),
        ("tint", "is not a number"),
        ("spot", "is not a number"),
        ("inputImage", "is not a number"),
    ] {
        let problem = rewrite_with_driven_inputs(&file, &[driven(name, 0, MaskChannel::Red)]).unwrap_err();
        assert!(problem.message.contains(why), "{name}: {}", problem.message);
        assert!(problem.message.contains(name), "{name}: {}", problem.message);
    }
    assert!(rewrite_with_driven_inputs(&file, &[driven("gain", 0, MaskChannel::Red)]).is_ok());
}

#[test]
fn an_input_cannot_be_driven_twice_and_the_masks_cannot_skip_a_number() {
    let file = filter_with_two_floats("void main() { gl_FragColor = vec4(radius, amount, 0.0, 1.0); }");

    let twice = rewrite_with_driven_inputs(
        &file,
        &[
            driven("radius", 0, MaskChannel::Red),
            driven("radius", 1, MaskChannel::Green),
        ],
    )
    .unwrap_err();
    assert!(
        twice.message.contains("radius") && twice.message.contains("twice"),
        "{}",
        twice.message
    );

    let skipping = rewrite_with_driven_inputs(&file, &[driven("radius", 1, MaskChannel::Red)]).unwrap_err();
    assert!(
        skipping.message.contains("mask 1") && skipping.message.contains("mask 0"),
        "{}",
        skipping.message
    );
}

#[test]
fn an_input_that_sets_the_size_of_a_buffer_cannot_be_driven_because_the_size_is_worked_out_once_for_the_whole_picture()
{
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"cells\", \"TYPE\": \"float\", \"MIN\": 1.0, \"MAX\": 64.0 }, { \"NAME\": \"gain\", \"TYPE\": \"float\" } ], \"PASSES\": [ { \"TARGET\": \"small\", \"WIDTH\": \"$WIDTH / cells\", \"HEIGHT\": \"$HEIGHT / cells\" }, {} ] }*/\nvoid main() { gl_FragColor = IMG_NORM_PIXEL(small, isf_FragNormCoord) * gain; }\n";
    let file = read(source).unwrap();

    let problem = rewrite_with_driven_inputs(&file, &[driven("cells", 0, MaskChannel::Red)]).unwrap_err();
    assert!(problem.message.contains("cells"), "{}", problem.message);
    assert!(problem.message.contains("size"), "{}", problem.message);
    // The other float of the same shader can be.
    assert!(rewrite_with_driven_inputs(&file, &[driven("gain", 0, MaskChannel::Red)]).is_ok());
}
