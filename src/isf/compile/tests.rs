// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Compiling shaders: the files of the spike all compile into what the engine accepts as an effect, a file with a mistake gives the line of the file, and a compiler that panics is a
//! refusal.

use super::*;
use crate::engine::{DEFAULT_MOST_PASSES_OF_A_LOOP, guard_wgsl};
use crate::isf::read;

const COLOR_CONTROLS: &str = include_str!("../fixtures/color_controls.fs");
const PLASMA: &str = include_str!("../fixtures/plasma.fs");
const WIPE: &str = include_str!("../fixtures/wipe_transition.fs");
const BLUR: &str = include_str!("../fixtures/gaussian_blur.fs");
const OLD_STYLE: &str = include_str!("../fixtures/old_style.fs");
const POSTERIZE: &str = include_str!("../fixtures/posterize.fs");
const VIGNETTE: &str = include_str!("../fixtures/vignette.fs");
const KALEIDOSCOPE: &str = include_str!("../fixtures/kaleidoscope.fs");
const BAD_SYNTAX: &str = include_str!("../fixtures/bad_syntax.fs");

fn compiled(source: &str) -> CompiledShader {
    compile(&read(source).unwrap()).unwrap_or_else(|problems| panic!("{problems:?}"))
}

fn refused(source: &str) -> Vec<IsfProblem> {
    compile(&read(source).unwrap()).expect_err("the shader was compiled")
}

fn filter(body: &str) -> String {
    format!("/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }} ] }}*/\n{body}\n")
}

#[test]
fn every_good_shader_of_the_spike_compiles_into_a_module_that_the_engine_takes_as_an_effect() {
    for (name, source) in [
        ("color_controls", COLOR_CONTROLS),
        ("plasma", PLASMA),
        ("gaussian_blur", BLUR),
        ("old_style", OLD_STYLE),
        ("posterize", POSTERIZE),
        ("vignette", VIGNETTE),
        ("kaleidoscope", KALEIDOSCOPE),
        ("wipe_transition", WIPE),
    ] {
        let shader = compiled(source);
        let guarded = guard_wgsl(&shader.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP)
            .unwrap_or_else(|problem| panic!("{name}: the engine's guard refuses it: {problem:?}\n{}", shader.wgsl));
        assert!(
            guarded.has_entry_point(ENTRY_POINT, naga::ShaderStage::Fragment),
            "{name}: {:?}",
            guarded.entry_points
        );
    }
}

#[test]
fn the_entry_point_is_the_one_the_engine_looks_for_and_it_is_the_only_one() {
    let shader = compiled(COLOR_CONTROLS);
    assert!(shader.wgsl.contains("fn fs_main("), "{}", shader.wgsl);
    // (The `main` of the generated GLSL is a function like any other in the module, which the entry point calls.)
    assert_eq!(shader.wgsl.matches("@fragment").count(), 1);
    assert!(
        shader.wgsl.contains(
            "@fragment 
fn fs_main("
        ),
        "{}",
        shader.wgsl
    );
    assert!(shader.wgsl.contains("@builtin(position)"));
}

#[test]
fn the_module_has_the_uniform_block_the_engine_writes_and_a_texture_and_a_sampler_for_each_image() {
    let shader = compiled(BLUR);

    assert_eq!(shader.image_names, ["inputImage", "bufferHorizontal"]);
    assert!(shader.wgsl.contains("array<vec4<f32>, 17>"), "{}", shader.wgsl);
    assert!(shader.wgsl.contains("@group(0) @binding(0)"));
    assert!(shader.wgsl.contains("@group(0) @binding(1)") && shader.wgsl.contains("@group(0) @binding(2)"));
    assert!(shader.wgsl.contains("@group(0) @binding(3)") && shader.wgsl.contains("@group(0) @binding(4)"));
    assert!(compiled(PLASMA).image_names.is_empty());
}

#[test]
fn what_was_changed_in_a_shader_is_told() {
    let shader = compiled(OLD_STYLE);
    assert!(
        shader.notes.iter().any(|note| note.contains("#version")),
        "{:?}",
        shader.notes
    );
    assert!(
        shader.notes.iter().any(|note| note.contains("calls")),
        "{:?}",
        shader.notes
    );
}

#[test]
fn a_mistake_in_a_shader_is_told_with_the_line_of_the_file_not_the_line_of_what_is_compiled() {
    // On line 12 of the file (the compiler notices the missing semicolon at the start of the line after it).
    let problems = refused(BAD_SYNTAX);

    assert_eq!(problems.len(), 1, "only the first syntax error is told: {problems:?}");
    assert_eq!(problems[0].line, Some(12), "{problems:?}");
    assert!(problems[0].column.is_some());
    assert!(!problems[0].message.is_empty());
}

#[test]
fn a_name_that_is_nowhere_is_told_where_it_is_in_the_file() {
    let source = filter(
        "void main() {\n\tvec4 c = IMG_THIS_NORM_PIXEL(inputImage);\n\tgl_FragColor = c * not_declared_anywhere;\n}",
    );
    let problems = refused(&source);

    assert!(!problems.is_empty());
    let line = source
        .lines()
        .position(|line| line.contains("not_declared_anywhere"))
        .unwrap() as u32
        + 1;
    assert!(
        problems.iter().any(|problem| problem.line == Some(line)),
        "line {line} of the file: {problems:?}"
    );
    assert!(
        problems
            .iter()
            .all(|problem| problem.line.is_none_or(|told| told <= source.lines().count() as u32)),
        "{problems:?}"
    );
}

#[test]
fn what_the_rewriting_refuses_is_told_by_the_compile_too() {
    let problems = refused(&filter("vec4 f(sampler2D t) { return vec4(0.0); }\nvoid main() {}"));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].line, Some(2));
    assert!(problems[0].message.contains("sampler2D"));
}

#[test]
fn a_shader_that_the_compiler_cannot_validate_is_told_without_a_place_if_it_has_none() {
    // A shader that has no main at all: the generated end calls one that is not there, which is a mistake in a place that is not the file's.
    let problems = refused("/*{ \"INPUTS\": [] }*/\nfloat f() { return 1.0; }\n");
    assert!(!problems.is_empty());
    assert!(problems.iter().all(|problem| problem.line.is_none()), "{problems:?}");
}

#[test]
fn compiling_in_a_worker_gives_what_compiling_gives() {
    let file = read(COLOR_CONTROLS).unwrap();
    assert_eq!(compile_in_a_worker(&file), compile(&file));
    let bad = read(BAD_SYNTAX).unwrap();
    assert_eq!(compile_in_a_worker(&bad), compile(&bad));
}

#[test]
fn a_panic_in_the_worker_is_a_problem_that_says_what_it_said_and_the_program_goes_on() {
    let problems = in_a_worker::<()>(|| panic!("naga did not like this")).unwrap_err();
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].message.contains("the compiler stopped on this shader")
            && problems[0].message.contains("naga did not like this"),
        "{problems:?}"
    );
    let with_a_string = in_a_worker::<()>(|| panic!("{}", String::from("a formatted one"))).unwrap_err();
    assert!(with_a_string[0].message.contains("a formatted one"));
    // And the next job is done as if nothing had happened.
    assert_eq!(in_a_worker(|| Ok(7)).unwrap(), 7);
    assert_eq!(
        in_a_worker::<()>(|| Err(vec![IsfProblem::nowhere_in_particular("no")]))
            .unwrap_err()
            .len(),
        1
    );
}

#[test]
fn the_worker_has_the_stack_it_is_given() {
    // A frame of a megabyte on the stack of the worker is not a problem for it (the default of a thread is two megabytes: this is a check that the large stack is the one in use).
    let sum = in_a_worker(|| {
        let big = [1u8; 3 * 1024 * 1024];
        Ok(std::hint::black_box(&big)
            .iter()
            .map(|byte| u64::from(*byte))
            .sum::<u64>())
    })
    .unwrap();
    assert_eq!(sum, 3 * 1024 * 1024);
}
