// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rewriting step: what it makes of the shaders of the spike and of what is odd in a shader, and what it refuses.

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

fn rewritten(file: &IsfFile) -> Rewritten {
    rewrite(file).unwrap()
}

fn has_line(glsl: &str, line: &str) -> bool {
    glsl.lines().any(|found| found == line)
}

#[test]
fn what_is_compiled_has_the_lines_of_the_file_after_a_beginning_of_the_lines_that_are_counted() {
    let file = read(COLOR_CONTROLS).unwrap();
    let made = rewritten(&file);

    let lines: Vec<&str> = made.glsl.lines().collect();
    let in_the_file = file
        .body
        .lines()
        .position(|line| line.contains("luma_weights"))
        .unwrap();
    assert!(lines[made.prelude_lines as usize + in_the_file].contains("luma_weights"));
    // The line of a function in the middle of the file is where it was.
    let main_in_the_file = file.body.lines().position(|line| line.contains("void main")).unwrap();
    assert!(lines[made.prelude_lines as usize + main_in_the_file].contains("void _isf_user_main"));
    assert!(lines[0] == "#version 450");
}

#[test]
fn the_inputs_are_parts_of_the_uniform_block_after_the_four_numbers_of_the_shader() {
    let made = rewritten(&read(COLOR_CONTROLS).unwrap());

    assert!(made.glsl.contains("uniform IsfUniforms { vec4 _isf_u[17]; };"));
    // Numbers 0 to 3 are TIME and the others; brightness is number 4.
    assert!(has_line(&made.glsl, "#define brightness (_isf_u[1].x)"));
    assert!(has_line(&made.glsl, "#define contrast (_isf_u[1].y)"));
    assert!(has_line(&made.glsl, "#define saturation (_isf_u[1].z)"));
    assert!(has_line(&made.glsl, "#define invert ((_isf_u[1].w != 0.0))"));
    assert!(has_line(
        &made.glsl,
        "#define tint (vec4(_isf_u[2].x, _isf_u[2].y, _isf_u[2].z, _isf_u[2].w))"
    ));
    assert!(has_line(&made.glsl, "#define TIME _isf_u[0].x"));
    assert!(has_line(&made.glsl, "#define TIMEDELTA _isf_u[0].y"));
    assert!(has_line(&made.glsl, "#define FRAMEINDEX int(_isf_u[0].z)"));
    assert!(has_line(&made.glsl, "#define PASSINDEX int(_isf_u[0].w)"));
    assert!(has_line(&made.glsl, "#define RENDERSIZE _isf_u[16].xy"));
}

#[test]
fn a_colour_that_does_not_begin_a_vec4_is_made_of_the_numbers_it_is_and_a_choice_and_a_point_are_what_they_are() {
    // A float first (number 4), so that the colour is numbers 5 to 8: it is in two vec4s.
    let file = filter(
        "{ \"NAME\": \"a\", \"TYPE\": \"float\" }, { \"NAME\": \"c\", \"TYPE\": \"color\" }, { \"NAME\": \"p\", \"TYPE\": \"point2D\" }, { \"NAME\": \"l\", \"TYPE\": \"long\", \"VALUES\": [1, 2] }, { \"NAME\": \"e\", \"TYPE\": \"event\" }",
        "void main() {}",
    );
    let made = rewritten(&file);

    assert!(has_line(
        &made.glsl,
        "#define c (vec4(_isf_u[1].y, _isf_u[1].z, _isf_u[1].w, _isf_u[2].x))"
    ));
    // The point is numbers 9 and 10, as they are (it is not scaled by the size of the picture: see `a_point_reaches_the_shader_as_the_file_wrote_it_at_every_size_of_the_picture`).
    assert!(has_line(&made.glsl, "#define p (vec2(_isf_u[2].y, _isf_u[2].z))"));
    assert!(has_line(&made.glsl, "#define l (int(_isf_u[2].w))"));
    assert!(has_line(&made.glsl, "#define e ((_isf_u[3].x != 0.0))"));
}

#[test]
fn the_images_are_a_texture_and_a_sampler_each_in_the_order_inputs_then_targets_then_imported() {
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"half\" }, { \"TARGET\": \"inputImage\" }, {} ], \"IMPORTED\": { \"lut\": { \"PATH\": \"a.png\" } } }*/\nvoid main() {}\n";
    let made = rewritten(&read(source).unwrap());

    assert_eq!(made.image_names, ["inputImage", "half", "lut"]);
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 1) uniform texture2D inputImage_tex;"
    ));
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 2) uniform sampler inputImage_samp;"
    ));
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 3) uniform texture2D half_tex;"
    ));
    assert!(has_line(
        &made.glsl,
        "layout(set = 0, binding = 6) uniform sampler lut_samp;"
    ));
    assert_eq!(
        rewritten(&read(BLUR).unwrap()).image_names,
        ["inputImage", "bufferHorizontal"]
    );
}

#[test]
fn a_shader_with_no_images_has_no_functions_that_read_them() {
    let file = read("/*{ \"INPUTS\": [] }*/\nvoid main() { gl_FragColor = vec4(1.0); }\n").unwrap();
    let made = rewritten(&file);
    assert!(made.image_names.is_empty());
    assert!(!made.glsl.contains("_isf_IMG_"));
}

#[test]
fn the_calls_of_the_macros_of_images_are_rewritten_into_calls_of_functions_of_the_image() {
    let file = filter(
        "",
        "void main() {\n\tvec4 a = IMG_NORM_PIXEL(inputImage, isf_FragNormCoord);\n\tvec4 b = IMG_THIS_PIXEL ( inputImage );\n\tvec2 s = IMG_SIZE(inputImage);\n\tvec4 c = texture2D(inputImage,uv);\n\tvec4 d = IMG_PIXEL(inputImage, gl_FragCoord.xy);\n\tvec4 e = texture2DRect(inputImage, gl_FragCoord.xy);\n\tvec4 f = IMG_THIS_NORM_PIXEL(inputImage);\n}",
    );
    let made = rewritten(&file);

    for expected in [
        "vec4 a = _isf_IMG_NORM_PIXEL_inputImage( isf_FragNormCoord);",
        "vec4 b = _isf_IMG_THIS_PIXEL_inputImage();",
        "vec2 s = _isf_IMG_SIZE_inputImage();",
        "vec4 c = _isf_texture2D_inputImage(uv);",
        "vec4 d = _isf_IMG_PIXEL_inputImage( gl_FragCoord.xy);",
        "vec4 e = _isf_texture2DRect_inputImage( gl_FragCoord.xy);",
        "vec4 f = _isf_IMG_THIS_NORM_PIXEL_inputImage();",
    ] {
        assert!(
            made.glsl.lines().any(|line| line.trim() == expected),
            "{expected}\n{}",
            made.glsl
        );
    }
    assert!(
        made.notes.iter().any(|note| note.starts_with("7 calls")),
        "{:?}",
        made.notes
    );
}

#[test]
fn a_call_with_another_name_or_a_name_that_only_ends_like_one_is_left_alone() {
    let file = filter(
        "",
        "void main() {\n\tvec4 a = IMG_NORM_PIXEL(notAnImage, uv);\n\tvec4 b = MY_IMG_PIXEL(inputImage, uv);\n\tvec4 c = IMG_PIXELS(inputImage, uv);\n\tfloat IMG_SIZE_X = 1.0;\n}",
    );
    let made = rewritten(&file);
    assert!(made.glsl.contains("IMG_NORM_PIXEL(notAnImage, uv)"));
    assert!(made.glsl.contains("MY_IMG_PIXEL(inputImage, uv)"));
    assert!(made.glsl.contains("IMG_PIXELS(inputImage, uv)"));
    assert!(made.glsl.contains("IMG_SIZE_X = 1.0"));
    assert!(
        !made.notes.iter().any(|note| note.contains("calls")),
        "{:?}",
        made.notes
    );
}

#[test]
fn the_functions_of_an_image_flip_the_picture_the_way_the_host_does_at_its_boundary() {
    let made = rewritten(&filter("", "void main() {}"));

    // Read at (x, 1 - y), at level 0.
    assert!(made.glsl.contains(
        "vec4 _isf_IMG_NORM_PIXEL_inputImage(vec2 c) { return textureLod(sampler2D(inputImage_tex, inputImage_samp), vec2(c.x, 1.0 - c.y), 0.0); }"
    ));
    assert!(
        made.glsl
            .contains("textureSize(sampler2D(inputImage_tex, inputImage_samp), 0)")
    );
    // The coordinates of the shader are those of OpenGL: the generated main flips the ones the card gives.
    assert!(
        made.glsl
            .contains("_isf_fragcoord = vec4(real.x, _isf_u[16].y - real.y, real.z, real.w);")
    );
    assert!(
        made.glsl
            .contains("isf_FragNormCoord = vec2(real.x / _isf_u[16].x, 1.0 - real.y / _isf_u[16].y);")
    );
    assert!(
        !made.glsl.contains("texture(sampler2D"),
        "never the call that needs derivatives"
    );
}

#[test]
fn the_shaders_own_main_is_renamed_as_a_word_and_a_generated_one_calls_it() {
    let file = filter(
        "",
        "float domain = 1.0;\nfloat mainColour() { return domain; }\nvoid main() { gl_FragColor = vec4(mainColour()); }",
    );
    let made = rewritten(&file);

    assert!(made.glsl.contains("float domain = 1.0;"));
    assert!(made.glsl.contains("float mainColour()"));
    assert!(made.glsl.contains("void _isf_user_main() {"));
    let end = made.glsl.rfind("void main() {").expect("a generated main");
    assert!(made.glsl[end..].contains("_isf_user_main();"));
    assert!(made.glsl[end..].contains("_isf_out = _isf_color;"));
    assert_eq!(made.glsl.matches("void main() {").count(), 1);
}

#[test]
fn what_vulkan_style_glsl_cannot_have_is_taken_out_or_made_plain_and_keeps_its_line() {
    let made = rewritten(&read(OLD_STYLE).unwrap());

    assert!(
        made.glsl
            .lines()
            .all(|line| !line.trim_start().starts_with("#version 120"))
    );
    assert_eq!(made.glsl.matches("#version").count(), 1, "only the one that is ours");
    assert!(
        has_line(&made.glsl, "vec2 left_over_varying;"),
        "a varying is a plain global"
    );
    assert!(made.notes.iter().any(|note| note.contains("#version")));
    assert!(
        made.notes
            .iter()
            .any(|note| note.contains("varying vec2 left_over_varying")),
        "{:?}",
        made.notes
    );
    let file = read(OLD_STYLE).unwrap();
    let lines: Vec<&str> = made.glsl.lines().collect();
    let line_of_main = file.body.lines().position(|line| line.contains("void main")).unwrap();
    assert!(lines[made.prelude_lines as usize + line_of_main].contains("_isf_user_main"));
}

#[test]
fn a_uniform_that_the_header_already_has_is_taken_out_and_another_is_a_plain_global() {
    let file = filter(
        "{ \"NAME\": \"amount\", \"TYPE\": \"float\" }",
        "uniform float amount;\nuniform highp float other, amount2;\nuniform float x = 1.0, amount;\nvoid main() {}",
    );
    let made = rewritten(&file);

    assert!(!made.glsl.contains("uniform float amount;"));
    assert!(has_line(&made.glsl, "highp float other, amount2;"));
    assert!(
        !made.glsl.contains("float x = 1.0, amount;"),
        "a declaration of two with one of them the header's is taken out"
    );
    assert!(made.notes.iter().any(|note| note.contains("the header already has it")));
}

#[test]
fn the_names_a_declaration_declares_are_found_after_the_precision_and_the_type() {
    assert_eq!(names_declared_by("uniform float a;"), ["a"]);
    assert_eq!(names_declared_by("uniform highp vec2 p, q = vec2(1.0);"), ["p", "q"]);
    assert_eq!(names_declared_by("varying vec4 colours[4];"), ["colours"]);
    assert_eq!(names_declared_by("uniform float;"), Vec::<String>::new());
}

#[test]
fn a_sampler2d_in_the_code_is_refused_with_its_place_and_one_in_a_comment_is_not() {
    let file = filter(
        "",
        "// a sampler2D in a comment\n/* and sampler2D\n in another */\nvec4 f(sampler2D tex) {\n\treturn vec4(0.0);\n}\nvoid main() {}",
    );
    let problem = rewrite(&file).unwrap_err();
    assert_eq!((problem.line, problem.column), (Some(5), Some(8)), "{problem}");
    assert!(problem.message.contains("an image is read by its name"), "{problem}");

    let in_a_comment = filter("", "// sampler2D\n/* sampler2D */\nvoid main() {}");
    assert!(rewrite(&in_a_comment).is_ok());
    let longer = filter("", "float sampler2DShadow2 = 1.0;\nvoid main() {}");
    assert!(rewrite(&longer).is_ok(), "a longer word is not it");
}

#[test]
fn brackets_nested_more_than_the_limit_are_refused_where_they_pass_it_and_the_limit_itself_is_not() {
    let nested = |depth: usize| {
        let inside = "(".repeat(depth);
        filter(
            "",
            &format!("void main() {{ float f = {inside}1.0{}; }}", ")".repeat(depth)),
        )
    };
    // (The braces of the function are the first of them.)
    assert!(rewrite(&nested(MOST_NESTING - 1)).is_ok());
    let problem = rewrite(&nested(MOST_NESTING)).unwrap_err();
    assert!(problem.message.contains("nested more than 200"), "{problem}");
    assert_eq!(problem.line, Some(2), "the line of the file that has them");
}

#[test]
fn without_comments_leaves_every_character_where_it_was() {
    let text = "a // b\nc /* d\ne */ f\n// g";
    let plain = without_comments(text);
    assert_eq!(plain.len(), text.len());
    assert_eq!(plain.lines().count(), text.lines().count());
    assert_eq!(plain, "a     \nc     \n     f\n    ");
    assert_eq!(without_comments("no comment"), "no comment");
    assert_eq!(without_comments("/* never closed"), "               ");
}

#[test]
fn every_spike_shader_that_is_good_is_rewritten() {
    for source in [VIGNETTE, POSTERIZE, BLUR, OLD_STYLE, COLOR_CONTROLS] {
        let file = read(source).unwrap();
        let made = rewritten(&file);
        assert!(made.glsl.ends_with("}\n"));
        assert!(made.prelude_lines > 20);
    }
}
