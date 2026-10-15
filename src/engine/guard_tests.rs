// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tests of the guard that need no graphics card: what it refuses and where it says the mistake is, and what the tree of a shader is after every
//! loop has its counter. (That a loop that does not end *does* end on a card is tested on the software adapter, in `foreign_effect_tests`.)

use naga::{Block, Expression, Literal, Module, ShaderStage, Statement};

use super::guard::{DEFAULT_MOST_PASSES_OF_A_LOOP, GuardedShader, ShaderProblem, cap_loops, guard_wgsl};
use super::shaders;

/// A shader that draws one colour and has no loop in it.
const NO_LOOP: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(position.x / 100.0, 0.5, 0.25, 1.0);
}
";

/// A shader of loops of every kind and in every place a loop can be: in the entry point and in a function it calls, in a loop, in a branch, in a `switch`
/// and in the `continuing` part of a loop.
const LOOPS_EVERYWHERE: &str = "
fn doubled_three_times(start: f32) -> f32 {
    var value = start;
    for (var round = 0; round < 3; round++) {
        value = value * 2.0;
    }
    return value;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var sum = doubled_three_times(position.x);
    loop {
        if sum > 1000.0 { break; }
        for (var inner = 0; inner < 2; inner++) {
            sum = sum + 1.0;
        }
        continuing {
            var extra = 0.0;
            while extra < 2.0 {
                extra = extra + 1.0;
            }
            sum = sum + extra;
        }
    }
    if position.y > 5.0 {
        while sum > 10.0 {
            sum = sum * 0.5;
        }
    } else {
        for (var in_the_else = 0; in_the_else < 3; in_the_else++) {
            sum = sum + 2.0;
        }
    }
    {
        var counted = 0;
        loop {
            counted = counted + 1;
            if counted > 4 { break; }
        }
        sum = sum + f32(counted);
    }
    switch i32(position.x) {
        case 0: {
            loop {
                sum = sum + 0.001;
                if sum > 2.0 { break; }
            }
        }
        default: {
            sum = sum + 1.0;
        }
    }
    return vec4<f32>(sum, 0.0, 0.0, 1.0);
}
";

/// How many loops `LOOPS_EVERYWHERE` has: one in the function it calls, the `loop` (with a `for` inside it and a `while` in its `continuing` part), the `while`
/// in the branch and the `for` in the other part of it, the `loop` in a block of its own and the `loop` in the `switch`.
const LOOPS_IN_LOOPS_EVERYWHERE: usize = 8;

/// A loop whose body reads a texture with the card working out the size of the steps (which is only allowed where every pixel makes the same passes).
const A_LOOP_THAT_SAMPLES: &str = "
@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var sum = vec4<f32>(0.0);
    for (var step = 0; step < 4; step++) {
        sum = sum + textureSample(picture, picture_sampler, position.xy / 100.0 + vec2<f32>(f32(step) * 0.01, 0.0));
    }
    return sum / 4.0;
}
";

/// A shader that does not end.
const A_LOOP_THAT_NEVER_ENDS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var value = position.x * 0.001;
    loop {
        value = fract(value * 1.0001 + 0.37);
    }
    return vec4<f32>(value, 0.5, 0.25, 1.0);
}
";

fn parsed(source: &str) -> Module {
    naga::front::wgsl::parse_str(source).unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)))
}

fn guarded(source: &str, most_passes_of_a_loop: u32) -> GuardedShader {
    guard_wgsl(source, most_passes_of_a_loop).unwrap_or_else(|problem| panic!("{problem}"))
}

fn is_valid(module: &Module) -> bool {
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(module)
        .is_ok()
}

/// Every block that is the body of a loop in `block`, however deep, outer loops before the loops in them.
fn loop_bodies<'a>(block: &'a Block, found: &mut Vec<&'a Block>) {
    for statement in block.iter() {
        match statement {
            Statement::Block(inner) => loop_bodies(inner, found),
            Statement::If { accept, reject, .. } => {
                loop_bodies(accept, found);
                loop_bodies(reject, found);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    loop_bodies(&case.body, found);
                }
            }
            Statement::Loop { body, continuing, .. } => {
                found.push(body);
                loop_bodies(body, found);
                loop_bodies(continuing, found);
            }
            _ => {}
        }
    }
}

/// Every function of the module, with the bodies of the loops of each.
fn loops_of(module: &Module) -> Vec<(&naga::Function, Vec<&Block>)> {
    let functions = module
        .functions
        .iter()
        .map(|(_, function)| function)
        .chain(module.entry_points.iter().map(|entry_point| &entry_point.function));
    functions
        .map(|function| {
            let mut bodies = Vec::new();
            loop_bodies(&function.body, &mut bodies);
            (function, bodies)
        })
        .collect()
}

fn how_many_loops(module: &Module) -> usize {
    loops_of(module).iter().map(|(_, bodies)| bodies.len()).sum()
}

/// The `(group, binding)` of every resource the shader has, in order.
fn bindings_of(module: &Module) -> Vec<(u32, u32)> {
    let mut bindings: Vec<(u32, u32)> = module
        .global_variables
        .iter()
        .filter_map(|(_, variable)| variable.binding.map(|binding| (binding.group, binding.binding)))
        .collect();
    bindings.sort_unstable();
    bindings
}

fn entry_points_of(module: &Module) -> Vec<(String, ShaderStage)> {
    let mut entry_points: Vec<(String, ShaderStage)> = module
        .entry_points
        .iter()
        .map(|entry_point| (entry_point.name.clone(), entry_point.stage))
        .collect();
    entry_points.sort_by(|first, second| first.0.cmp(&second.0));
    entry_points
}

// ---- refusing a shader, with its place ----

#[test]
fn a_shader_that_does_not_read_is_refused_and_the_place_is_where_the_mistake_is() {
    // The `;` is missing after the second line of the body, so what is wrong is found at the `return` that comes where the `;` should have been.
    let source = "@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let brightness = position.x * 0.5
    return vec4<f32>(brightness);
}
";
    let problem = guard_wgsl(source, DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap_err();
    assert_eq!((problem.line, problem.column), (Some(4), Some(5)), "{problem}");
    assert!(problem.message.contains(';'), "{problem}");
    assert!(
        problem.to_string().starts_with("line 4, column 5: "),
        "the words begin with the place: {problem}"
    );
}

#[test]
fn the_place_of_a_mistake_is_counted_in_lines_from_1_and_in_bytes_from_1() {
    // The mistake is the unknown name, on the fifth line, after the `    return vec4<f32>(` that begins it.
    let source = "@fragment
fn fs_main() -> @location(0) vec4<f32> {
    // a comment
    // another comment
    return vec4<f32>(nothing_of_this_name);
}
";
    let problem = guard_wgsl(source, 10).unwrap_err();
    assert_eq!(problem.line, Some(5), "{problem}");
    let start_of_the_name = "    return vec4<f32>(".len() as u32 + 1;
    assert_eq!(problem.column, Some(start_of_the_name), "{problem}");
    assert!(problem.message.contains("nothing_of_this_name"), "{problem}");
}

#[test]
fn a_shader_that_reads_but_is_not_allowed_is_refused_and_the_place_is_in_the_shader_as_it_was_written() {
    // A list of numbers in what the shader is given has to have 16 bytes for each, however few the number needs. That it does not is not found by the
    // check that reads the text: it reads as well as any other, and what it says is against the rules of what a shader is given.
    let source = "// a shader
@group(0) @binding(0) var<uniform> values: array<f32, 8>;

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(values[0]);
}
";
    let problem = guard_wgsl(source, DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap_err();
    assert_eq!(problem.line, Some(2), "{problem}");
    assert!(problem.column.is_some_and(|column| column >= 1), "{problem}");
    assert!(!problem.message.is_empty());
    // And with 16 bytes for each number it is all right: it was that.
    let fine = source
        .replace("array<f32, 8>", "array<vec4<f32>, 8>")
        .replace("values[0]", "values[0].x");
    assert!(guard_wgsl(&fine, DEFAULT_MOST_PASSES_OF_A_LOOP).is_ok(), "{fine}");
}

#[test]
fn a_problem_that_has_no_place_is_told_without_one_and_one_with_only_a_line_has_only_that() {
    let without = ShaderProblem {
        message: "it is no good".to_owned(),
        line: None,
        column: None,
    };
    assert_eq!(without.to_string(), "it is no good");
    let only_a_line = ShaderProblem {
        message: "it is no good".to_owned(),
        line: Some(7),
        column: None,
    };
    assert_eq!(only_a_line.to_string(), "line 7: it is no good");
    let both = ShaderProblem {
        message: "it is no good".to_owned(),
        line: Some(7),
        column: Some(3),
    };
    assert_eq!(both.to_string(), "line 7, column 3: it is no good");
}

#[test]
fn text_that_is_not_a_shader_is_refused_and_a_text_with_nothing_in_it_is_a_shader_with_nothing_in_it() {
    for source in ["this is not a shader", "fn", "@fragment fn fs_main( {", "{", "}"] {
        assert!(guard_wgsl(source, 10).is_err(), "{source:?}");
    }
    // (What is wrong with it is that it has no entry point, which is for the one that uses the shader to say.)
    let nothing = guarded("", 10);
    assert_eq!((nothing.entry_points, nothing.loops_capped), (Vec::new(), 0));
}

// ---- putting a counter in every loop ----

#[test]
fn a_shader_with_no_loop_is_given_no_counter_and_comes_out_as_it_went_in() {
    let shader = guarded(NO_LOOP, 10);
    assert_eq!(shader.loops_capped, 0);
    assert!(shader.has_entry_point("fs_main", ShaderStage::Fragment));
    assert!(!shader.has_entry_point("fs_main", ShaderStage::Vertex));
    assert!(!shader.has_entry_point("vs_main", ShaderStage::Fragment));
    assert!(!shader.wgsl.contains("passes_of_a_loop_so_far"), "{}", shader.wgsl);
}

#[test]
fn every_loop_of_a_shader_is_counted_wherever_it_is_and_in_whatever_it_is() {
    let shader = guarded(LOOPS_EVERYWHERE, 10);
    assert_eq!(shader.loops_capped, LOOPS_IN_LOOPS_EVERYWHERE);
    // (The text that comes out is a shader of its own with as many loops as before: the counters are not loops.)
    assert_eq!(how_many_loops(&parsed(&shader.wgsl)), LOOPS_IN_LOOPS_EVERYWHERE);
    assert!(
        shader.wgsl.matches("passes_of_a_loop_so_far").count() >= LOOPS_IN_LOOPS_EVERYWHERE * 3,
        "each loop's counter is declared, added to and looked at: {}",
        shader.wgsl
    );
}

#[test]
fn a_counter_is_a_variable_of_its_own_that_begins_at_nothing_and_is_not_made_again_by_every_pass() {
    let mut module = parsed(LOOPS_EVERYWHERE);
    let loops_before = how_many_loops(&module);
    assert_eq!(loops_before, LOOPS_IN_LOOPS_EVERYWHERE);
    let variables_before: usize = module
        .functions
        .iter()
        .map(|(_, function)| function.local_variables.len())
        .chain(
            module
                .entry_points
                .iter()
                .map(|entry_point| entry_point.function.local_variables.len()),
        )
        .sum();
    assert_eq!(cap_loops(&mut module, 7), LOOPS_IN_LOOPS_EVERYWHERE);

    let mut counters_seen = Vec::new();
    for (function_number, (function, bodies)) in loops_of(&module).into_iter().enumerate() {
        for body in bodies {
            // The head of the body: the counter is looked at, added to and put back, and the loop is left if it has gone over the number it is let.
            let [
                Statement::Emit(_),
                Statement::Store { pointer, .. },
                Statement::If {
                    condition,
                    accept,
                    reject,
                },
                ..,
            ] = &body[..]
            else {
                panic!(
                    "the body of a loop does not begin with its counter: {:?}",
                    &body[..body.len().min(4)]
                );
            };
            assert!(matches!(&accept[..], [Statement::Break]), "{accept:?}");
            assert!(reject.is_empty());
            let Expression::LocalVariable(counter) = &function.expressions[*pointer] else {
                panic!("the counter is not a variable of the function");
            };
            let counter = *counter;
            counters_seen.push((function_number, counter.index()));
            let variable = &function.local_variables[counter];
            let Some(initial_value) = variable.init else {
                panic!("the counter has no first value");
            };
            assert!(
                matches!(
                    function.expressions[initial_value],
                    Expression::Literal(Literal::U32(0))
                ),
                "{:?}",
                function.expressions[initial_value]
            );
            assert_eq!(
                module.types[variable.ty].inner,
                naga::TypeInner::Scalar(naga::Scalar::U32),
                "the counter is a whole number that cannot go below nothing"
            );
            // The number of passes it is let make is the one that was asked for.
            let Expression::Binary { right: limit, .. } = &function.expressions[*condition] else {
                panic!("the loop is left on a comparison");
            };
            assert!(
                matches!(function.expressions[*limit], Expression::Literal(Literal::U32(7))),
                "{:?}",
                function.expressions[*limit]
            );
        }
    }
    assert_eq!(counters_seen.len(), LOOPS_IN_LOOPS_EVERYWHERE);
    counters_seen.sort_unstable();
    counters_seen.dedup();
    assert_eq!(
        counters_seen.len(),
        LOOPS_IN_LOOPS_EVERYWHERE,
        "every loop has a counter of its own, none shares one with another"
    );
    let variables_after: usize = module
        .functions
        .iter()
        .map(|(_, function)| function.local_variables.len())
        .chain(
            module
                .entry_points
                .iter()
                .map(|entry_point| entry_point.function.local_variables.len()),
        )
        .sum();
    assert_eq!(variables_after, variables_before + LOOPS_IN_LOOPS_EVERYWHERE);
    assert!(is_valid(&module));
}

#[test]
fn what_a_loop_did_before_is_left_in_the_loop_after_its_counter() {
    let mut module = parsed(LOOPS_EVERYWHERE);
    let lengths_before: Vec<usize> = loops_of(&module)
        .iter()
        .flat_map(|(_, bodies)| bodies.iter().map(|body| body.len()))
        .collect();
    cap_loops(&mut module, 10);
    let lengths_after: Vec<usize> = loops_of(&module)
        .iter()
        .flat_map(|(_, bodies)| bodies.iter().map(|body| body.len()))
        .collect();
    assert_eq!(lengths_before.len(), lengths_after.len());
    for (before, after) in lengths_before.iter().zip(&lengths_after) {
        assert_eq!(
            *after,
            before + 3,
            "three statements are added at the head of every loop, and none is taken away"
        );
    }
}

#[test]
fn a_loop_that_reads_a_texture_with_the_card_choosing_the_step_is_still_allowed_with_its_counter() {
    // The check that every pixel makes the same passes (which that way of reading needs) is not spoiled by the way out that is put at the head of the loop.
    for most_passes in [0, 1, 4, 100_000, u32::MAX] {
        let shader = guarded(A_LOOP_THAT_SAMPLES, most_passes);
        assert_eq!(shader.loops_capped, 1);
        assert_eq!(bindings_of(&parsed(&shader.wgsl)), vec![(0, 0), (0, 1)]);
    }
}

#[test]
fn any_number_of_passes_gives_a_shader_that_passes_the_checks_also_none_and_no_limit() {
    for most_passes in [0, 1, 2, 99_999, DEFAULT_MOST_PASSES_OF_A_LOOP, u32::MAX - 1, u32::MAX] {
        let shader = guarded(LOOPS_EVERYWHERE, most_passes);
        assert_eq!(shader.loops_capped, LOOPS_IN_LOOPS_EVERYWHERE, "{most_passes}");
        let again = parsed(&shader.wgsl);
        assert!(is_valid(&again), "{most_passes}");
        assert_eq!(
            shader.wgsl.matches(&format!("> {most_passes}u)")).count(),
            LOOPS_IN_LOOPS_EVERYWHERE,
            "every loop is left when its counter is over the number of passes it is let make ({most_passes}): {}",
            shader.wgsl
        );
    }
}

#[test]
fn a_loop_that_never_ends_is_a_loop_like_any_other_to_the_guard() {
    let shader = guarded(A_LOOP_THAT_NEVER_ENDS, 5);
    assert_eq!(shader.loops_capped, 1);
    assert_eq!(how_many_loops(&parsed(&shader.wgsl)), 1);
    assert!(shader.wgsl.contains("break"), "{}", shader.wgsl);
}

#[test]
fn the_text_that_comes_out_has_the_same_entry_points_and_resources_as_the_shader_that_went_in() {
    for source in [NO_LOOP, LOOPS_EVERYWHERE, A_LOOP_THAT_SAMPLES, A_LOOP_THAT_NEVER_ENDS] {
        let original = parsed(source);
        let shader = guarded(source, 1000);
        let again = parsed(&shader.wgsl);
        assert_eq!(entry_points_of(&again), entry_points_of(&original));
        assert_eq!(bindings_of(&again), bindings_of(&original));
        let mut named: Vec<(String, ShaderStage)> = shader.entry_points.clone();
        named.sort_by(|first, second| first.0.cmp(&second.0));
        assert_eq!(named, entry_points_of(&original));
    }
}

#[test]
fn the_shaders_of_the_engine_pass_through_the_guard_with_what_they_have_and_no_counter() {
    for (name, source) in shaders::ALL {
        let original = parsed(source);
        let shader =
            guard_wgsl(source, DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap_or_else(|problem| panic!("{name}: {problem}"));
        assert_eq!(
            shader.loops_capped,
            how_many_loops(&original),
            "{name}: one counter for each loop it has, and it has none unless it says so"
        );
        let again = parsed(&shader.wgsl);
        assert_eq!(entry_points_of(&again), entry_points_of(&original), "{name}");
        assert_eq!(bindings_of(&again), bindings_of(&original), "{name}");
        assert_eq!(how_many_loops(&again), how_many_loops(&original), "{name}");
    }
}

#[test]
fn a_shader_that_stops_being_all_right_with_its_counters_in_is_refused_and_it_is_the_limit_that_is_blamed() {
    // A guard that put a `break` where there is no loop to leave would make a shader that was all right a shader that is not: the shader is not to be blamed for it.
    fn a_mistaken_cap(module: &mut Module, _most_passes: u32) -> usize {
        for entry_point in module.entry_points.iter_mut() {
            entry_point.function.body.push(Statement::Break, naga::Span::UNDEFINED);
        }
        0
    }
    let problem = super::guard::guard_wgsl_capped_by(NO_LOOP, 10, a_mistaken_cap).unwrap_err();
    assert!(
        problem
            .message
            .starts_with("the limit on the passes of a loop could not be put into the shader"),
        "{problem}"
    );
    assert_eq!((problem.line, problem.column), (None, None));
    // And the same shader goes through when the limit is put in right.
    assert!(guard_wgsl(NO_LOOP, 10).is_ok());
}
