// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Compiling shaders: the files of the spike all compile into what the engine accepts as an effect, a file with a mistake gives the line of the file, and a compiler that panics is a
//! refusal.

use super::*;
use crate::engine::{DEFAULT_MOST_PASSES_OF_A_LOOP, ImageSlot, guard_wgsl};
use crate::isf::{IsfInputKind, MaskChannel, read};

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

/// A filter that reads `inputImage` in a loop of eight rounds and uses `radius` (a float from 0 to 10) twice in each round.
const A_FILTER_THAT_USES_ITS_INPUT_IN_A_LOOP: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"radius\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 10.0, \"DEFAULT\": 1.0 } ] }*/\nvoid main() {\n\tvec4 sum = vec4(0.0);\n\tfor (int i = 0; i < 8; i++) {\n\t\tsum += radius * IMG_NORM_PIXEL(inputImage, isf_FragNormCoord + vec2(radius * 0.01 * float(i), 0.0));\n\t}\n\tgl_FragColor = sum / 8.0;\n}\n";

fn driven_radius(mask: usize, channel: MaskChannel) -> DrivenInput {
    DrivenInput {
        name: "radius".to_owned(),
        mask,
        channel,
    }
}

/// The text of the function `name` of a WGSL module: from `fn name(` to the next function.
fn function_of_the_module<'wgsl>(wgsl: &'wgsl str, name: &str) -> &'wgsl str {
    let start = wgsl
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("the module has no function {name}:\n{wgsl}"));
    let end = wgsl[start + 1..]
        .find("\nfn ")
        .map_or(wgsl.len(), |next| start + 1 + next);
    &wgsl[start..end]
}

#[test]
fn a_shader_with_an_input_read_from_a_mask_compiles_and_the_engine_accepts_it_as_an_effect() {
    let file = read(A_FILTER_THAT_USES_ITS_INPUT_IN_A_LOOP).unwrap();
    let shader = compile_with_driven_inputs(&file, &[driven_radius(0, MaskChannel::Brightness)])
        .unwrap_or_else(|problems| panic!("{problems:?}"));

    assert_eq!(shader.image_names, ["inputImage", "_isf_mask_0"]);
    let guarded = guard_wgsl(&shader.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP)
        .unwrap_or_else(|problem| panic!("the engine's guard refuses it: {problem:?}\n{}", shader.wgsl));
    assert!(guarded.has_entry_point(ENTRY_POINT, naga::ShaderStage::Fragment));
    // What the engine is told: the picture, and the mask with its number.
    let structure = shader.structure.expect("the structure is made");
    assert_eq!(structure.images, [ImageSlot::Picture, ImageSlot::Mask(0)]);
    assert_eq!(structure.masks(), 1);
    assert_eq!(structure.check(), Ok(()));
}

#[test]
fn the_mask_is_read_in_the_function_that_sets_the_inputs_up_and_never_in_the_shaders_own_code() {
    let file = read(A_FILTER_THAT_USES_ITS_INPUT_IN_A_LOOP).unwrap();
    let shader = compile_with_driven_inputs(&file, &[driven_radius(0, MaskChannel::Red)]).unwrap();
    let wgsl = &shader.wgsl;

    // (naga writes the functions under the names the GLSL gave them. The loop is in the shader's own main; the reads of the mask are in the function that sets the inputs up, once, and
    // the loop reads a global.)
    let the_shaders_main = function_of_the_module(wgsl, "_isf_user_main");
    assert!(
        !the_shaders_main.contains("_isf_mask_0"),
        "the shader's own main reads the mask:\n{the_shaders_main}"
    );
    assert!(
        the_shaders_main.contains("_isf_driven_radius"),
        "the loop reads the global:\n{the_shaders_main}"
    );
    // (The read of the mask is a function of its own, which naga writes under the name `_isf_IMG_NORM_PIXEL_isf_mask_0_`; the set-up calls it once.)
    let set_up = function_of_the_module(wgsl, "_isf_set_up_driven");
    assert_eq!(
        set_up.matches("_isf_IMG_NORM_PIXEL_isf_mask_0_(").count(),
        1,
        "one read of the mask per pixel:\n{set_up}"
    );
    let the_read = function_of_the_module(wgsl, "_isf_IMG_NORM_PIXEL_isf_mask_0_");
    assert!(
        the_read.contains("textureSampleLevel(_isf_mask_0_tex"),
        "the function reads the mask:\n{the_read}"
    );
    // (And the generated main, which the entry point calls, runs the set-up before the shader's own main, and does not read the mask itself.)
    let main = function_of_the_module(wgsl, "main");
    let places: Vec<Option<usize>> = ["_isf_set_up_driven()", "_isf_user_main()"]
        .iter()
        .map(|call| main.find(call))
        .collect();
    assert!(
        matches!(places[..], [Some(set_up), Some(user_main)] if set_up < user_main),
        "{main}"
    );
    assert!(!main.contains("_isf_IMG_NORM_PIXEL_isf_mask_0_("), "{main}");
}

#[test]
fn a_shader_with_nothing_read_from_a_mask_compiles_to_what_it_always_did() {
    let file = read(A_FILTER_THAT_USES_ITS_INPUT_IN_A_LOOP).unwrap();
    let plain = compile(&file).unwrap();
    let with_none = compile_with_driven_inputs(&file, &[]).unwrap();

    assert_eq!(plain, with_none);
    assert!(!plain.wgsl.contains("_isf_mask"));
    assert_eq!(plain.image_names, ["inputImage"]);
    assert_eq!(plain.structure.as_ref().unwrap().masks(), 0);
}

/// The shaders of a folder (`FASTFORWARDRS_ISF_CORPUS=<folder>`; the folder the Bundles tab fetches the Vidvox collection into is one): every float input of every shader that
/// compiles as it is, **read from a mask, alone and then all together with masks and channels that differ**, must compile too, except the inputs that set the size of a buffer, which
/// are refused for that and nothing else. **Run by hand**:
///
/// `FASTFORWARDRS_ISF_CORPUS=<folder> cargo test --lib --release every_float_input_of_the_corpus -- --ignored --nocapture`
#[test]
#[ignore = "run by hand, on a folder of shaders"]
fn every_float_input_of_the_corpus_can_be_read_from_a_mask() {
    let folder = std::env::var("FASTFORWARDRS_ISF_CORPUS").expect("FASTFORWARDRS_ISF_CORPUS names the folder");
    let mut paths: Vec<_> = std::fs::read_dir(&folder)
        .expect("the folder can be read")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "fs"))
        .collect();
    paths.sort();

    let (mut shaders, mut floats, mut alone, mut refused_for_a_size, mut with_floats_and_masks) = (0, 0, 0, 0, 0);
    let mut failures: Vec<String> = Vec::new();
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(file) = read(&std::fs::read_to_string(path).unwrap_or_default()) else {
            continue;
        };
        if compile(&file).is_err() {
            // (A shader that does not compile as it is says nothing about this.)
            continue;
        }
        shaders += 1;
        let sets_a_size = super::super::structure::inputs_that_set_a_size(&file);
        let drivable: Vec<&str> = file
            .inputs
            .iter()
            .filter(|input| matches!(input.kind, IsfInputKind::Float { .. }))
            .map(|input| input.name.as_str())
            .collect();
        floats += drivable.len();
        let driven_by = |name: &str, number: usize| DrivenInput {
            name: name.to_owned(),
            mask: number % 3,
            channel: MaskChannel::ALL[number % MaskChannel::ALL.len()],
        };
        let mut together: Vec<DrivenInput> = Vec::new();
        for (number, input) in drivable.iter().enumerate() {
            let result = compile_with_driven_inputs(&file, &[driven_by(input, 0)]);
            if sets_a_size.iter().any(|set| set == input) {
                match result {
                    Err(problems) if problems[0].message.contains("size") => refused_for_a_size += 1,
                    other => failures.push(format!(
                        "{name}: {input} sets a size and was not refused for it: {other:?}"
                    )),
                }
                continue;
            }
            match result {
                Ok(shader) if shader.structure.as_ref().is_ok_and(|structure| structure.masks() == 1) => alone += 1,
                other => failures.push(format!(
                    "{name}: {input} alone: {:?}",
                    other.map(|shader| shader.structure)
                )),
            }
            together.push(driven_by(input, number));
        }
        // (The masks have to be numbered from 0 with none left out: the first mask that the inputs use is 0, then 1, then 2.)
        for (place, one) in together.iter_mut().enumerate() {
            one.mask = place % 3;
        }
        if !together.is_empty() {
            match compile_with_driven_inputs(&file, &together) {
                Ok(shader) => {
                    let masks = shader.structure.as_ref().map_or(0, ShaderStructure::masks);
                    assert_eq!(masks, together.len().min(3), "{name}");
                    with_floats_and_masks += 1;
                }
                Err(problems) => failures.push(format!("{name}: all together: {problems:?}")),
            }
        }
    }
    println!(
        "{shaders} shaders that compile, {floats} float inputs: {alone} compile read from a mask alone, {refused_for_a_size} are refused because they set a size, {with_floats_and_masks} shaders compile with all their other floats read from three masks at once, {} failures",
        failures.len()
    );
    for failure in &failures {
        println!("{failure}");
    }
    assert!(failures.is_empty());
    assert!(shaders > 0, "the folder has shaders");
}

#[test]
fn what_is_wrong_with_the_driven_inputs_is_a_problem_of_the_compile() {
    let file = read(A_FILTER_THAT_USES_ITS_INPUT_IN_A_LOOP).unwrap();
    let problems = compile_with_driven_inputs(
        &file,
        &[DrivenInput {
            name: "nothing".to_owned(),
            mask: 0,
            channel: MaskChannel::Red,
        }],
    )
    .unwrap_err();
    assert_eq!(problems.len(), 1);
    assert!(problems[0].message.contains("nothing"), "{}", problems[0].message);
}
