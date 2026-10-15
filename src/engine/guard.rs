// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The guard: what is done to a shader that the engine did not write, before the graphics card is given it.
//!
//! A shader that does not end keeps a graphics card busy for ever, and on a card that is a processor (the software adapters) the work goes on even after
//! the device it was given to is thrown away (the findings of phase 0.5, S2). So the engine does these things with the shaders of strangers:
//!
//! 1. It **reads them itself**, with the front end the graphics library reads them with (naga), and refuses one that does not parse or does not mean
//!    anything, saying **where** (line and column), so that whoever wrote it can find the mistake.
//! 2. It gives **every loop a counter of its own**, which ends the loop after a number of passes (the *cap*), whatever way out the loop has of its own. This
//!    is done to naga's tree of the shader, not to the text, so it does not matter how the shader was written.
//! 3. What is given to the library is the text of the shader with the counters in it, which the library reads again like any other shader.
//!
//! **What a cap does and does not do.** It bounds the work: a loop that would not end ends, and the picture is then *cut short* (what the shader made
//! of the passes it was let make). It does not make the work cheap: the counter counts all the passes that a loop makes in one run of the shader, also
//! over several entries into it, so a loop inside a loop is let make the cap's number of passes *in all*, and the worst case of a shader is about the cap times the
//! number of its loops times the number of pixels times the cost of a pass. That is why the engine also has a watchdog over every frame (see `worker`).

use naga::{
    BinaryOperator, Block, Expression, Function, Handle, Literal, LocalVariable, Module, Range, Scalar, ShaderStage,
    Span, Statement, Type, TypeInner,
};

/// How many passes a loop of a shader that the engine did not write is let make, unless it is told another number. (Measured in phase 0.5, S2: a shader
/// that is one loop of this many passes draws a picture of 64 by 64 pixels in 0.18 of a second on WARP and 1.2 seconds on lavapipe. The cost goes with the number
/// of pixels, so a whole frame takes far longer, which is for the watchdog to stop.) The number is for the engine to change, not the shader.
pub const DEFAULT_MOST_PASSES_OF_A_LOOP: u32 = 100_000;

/// What is wrong with a shader, and where in its text, if that is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderProblem {
    /// What is wrong, in words.
    pub message: String,
    /// The line it is on, the first line being 1.
    pub line: Option<u32>,
    /// The place in the line (counted in bytes), the first being 1.
    pub column: Option<u32>,
}

impl std::fmt::Display for ShaderProblem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(formatter, "line {line}, column {column}: {}", self.message),
            (Some(line), None) => write!(formatter, "line {line}: {}", self.message),
            _ => write!(formatter, "{}", self.message),
        }
    }
}

impl std::error::Error for ShaderProblem {}

/// A shader that has been read, checked and made safe to give to the graphics library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedShader {
    /// The text to give to the graphics library: the shader, with the counters of its loops in it.
    pub wgsl: String,
    /// How many loops were given a counter.
    pub loops_capped: usize,
    /// The names and the kinds of the entry points the shader has.
    pub entry_points: Vec<(String, ShaderStage)>,
}

impl GuardedShader {
    /// Whether the shader has an entry point of this name and kind.
    pub fn has_entry_point(&self, name: &str, stage: ShaderStage) -> bool {
        self.entry_points
            .iter()
            .any(|(entry_name, entry_stage)| entry_name == name && *entry_stage == stage)
    }
}

/// Reads the shader `source` (WGSL), refuses it with its place if it is not a good one, and otherwise makes every loop of it end after
/// `most_passes_of_a_loop` passes (`u32::MAX` is as good as no limit: it takes more passes than any frame is waited for).
pub fn guard_wgsl(source: &str, most_passes_of_a_loop: u32) -> Result<GuardedShader, ShaderProblem> {
    guard_wgsl_capped_by(source, most_passes_of_a_loop, cap_loops)
}

/// [`guard_wgsl`] with the part that puts the counters in given, so that a test can give one that is wrong and see that it is the counters that are blamed.
pub(super) fn guard_wgsl_capped_by(
    source: &str,
    most_passes_of_a_loop: u32,
    cap: fn(&mut Module, u32) -> usize,
) -> Result<GuardedShader, ShaderProblem> {
    let mut module = naga::front::wgsl::parse_str(source).map_err(|error| {
        let place = error.location(source);
        ShaderProblem {
            message: error.message().to_owned(),
            line: place.map(|place| place.line_number),
            column: place.map(|place| place.line_position),
        }
    })?;

    // The shader is checked as it was written first, so that a mistake is reported as the shader's, with the place it has in the shader...
    validator().validate(&module).map_err(|error| {
        let place = error.location(source);
        ShaderProblem {
            message: with_its_causes(error.as_inner()),
            line: place.map(|place| place.line_number),
            column: place.map(|place| place.line_position),
        }
    })?;

    // ...and then again with the counters in it, which a shader that was all right cannot fail unless the counters are what is wrong.
    let loops_capped = cap(&mut module, most_passes_of_a_loop);
    let information = validator().validate(&module).map_err(|error| ShaderProblem {
        message: format!(
            "the limit on the passes of a loop could not be put into the shader: {}",
            with_its_causes(error.as_inner())
        ),
        line: None,
        column: None,
    })?;
    let wgsl = naga::back::wgsl::write_string(&module, &information, naga::back::wgsl::WriterFlags::empty()).map_err(
        |error| ShaderProblem {
            message: format!("the shader could not be written out again: {error}"),
            line: None,
            column: None,
        },
    )?;
    Ok(GuardedShader {
        wgsl,
        loops_capped,
        entry_points: module
            .entry_points
            .iter()
            .map(|entry_point| (entry_point.name.clone(), entry_point.stage))
            .collect(),
    })
}

/// The checker of shaders. Everything the graphics library might be asked to take is let through here: whether this card takes it is for the library to
/// say when it is given the shader (a shader that needs more than the card has is refused by it, and that is told as it is).
fn validator() -> naga::valid::Validator {
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
}

/// An error and what it came from, in one line.
fn with_its_causes(error: &(dyn std::error::Error + 'static)) -> String {
    let mut words = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        words.push_str(": ");
        words.push_str(&next.to_string());
        cause = next.source();
    }
    words
}

/// Gives every loop of the module a counter of its own that ends the loop after `most_passes` passes. Returns how many loops it did that to.
pub fn cap_loops(module: &mut Module, most_passes: u32) -> usize {
    let counter_type = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::U32),
        },
        Span::UNDEFINED,
    );
    let mut capped = 0;
    for (_, function) in module.functions.iter_mut() {
        capped += cap_loops_of_function(function, counter_type, most_passes);
    }
    for entry_point in module.entry_points.iter_mut() {
        capped += cap_loops_of_function(&mut entry_point.function, counter_type, most_passes);
    }
    capped
}

fn cap_loops_of_function(function: &mut Function, counter_type: Handle<Type>, most_passes: u32) -> usize {
    // (The body is taken out of the function while it is changed, because the changes add to the rest of the function: its expressions and variables.)
    let mut body = std::mem::take(&mut function.body);
    let capped = cap_loops_of_block(&mut body, function, counter_type, most_passes);
    function.body = body;
    capped
}

fn cap_loops_of_block(
    block: &mut Block,
    function: &mut Function,
    counter_type: Handle<Type>,
    most_passes: u32,
) -> usize {
    let mut capped = 0;
    for (statement, _) in block.span_iter_mut() {
        match statement {
            Statement::Block(inner) => capped += cap_loops_of_block(inner, function, counter_type, most_passes),
            Statement::If { accept, reject, .. } => {
                capped += cap_loops_of_block(accept, function, counter_type, most_passes);
                capped += cap_loops_of_block(reject, function, counter_type, most_passes);
            }
            Statement::Switch { cases, .. } => {
                for case in cases.iter_mut() {
                    capped += cap_loops_of_block(&mut case.body, function, counter_type, most_passes);
                }
            }
            Statement::Loop { body, continuing, .. } => {
                capped += cap_loops_of_block(body, function, counter_type, most_passes);
                capped += cap_loops_of_block(continuing, function, counter_type, most_passes);
                put_a_counter_in_the_loop(body, function, counter_type, most_passes);
                capped += 1;
            }
            _ => {}
        }
    }
    capped
}

/// Puts at the head of `body`, the body of a loop, what counts the pass and leaves the loop when there have been more than `most_passes`. The counter is a
/// variable of the function that begins at 0 when the function is run, so it counts every pass through this loop, also over several entries into it.
fn put_a_counter_in_the_loop(body: &mut Block, function: &mut Function, counter_type: Handle<Type>, most_passes: u32) {
    let zero = function
        .expressions
        .append(Expression::Literal(Literal::U32(0)), Span::UNDEFINED);
    let counter = function.local_variables.append(
        LocalVariable {
            name: Some("passes_of_a_loop_so_far".to_owned()),
            ty: counter_type,
            init: Some(zero),
        },
        Span::UNDEFINED,
    );
    let pointer = function
        .expressions
        .append(Expression::LocalVariable(counter), Span::UNDEFINED);
    let one = function
        .expressions
        .append(Expression::Literal(Literal::U32(1)), Span::UNDEFINED);
    let limit = function
        .expressions
        .append(Expression::Literal(Literal::U32(most_passes)), Span::UNDEFINED);
    // (These three are worked out one after another each time round, so they are what the statement that comes next says it emits.)
    let passes_before = function
        .expressions
        .append(Expression::Load { pointer }, Span::UNDEFINED);
    let passes_with_this_one = function.expressions.append(
        Expression::Binary {
            op: BinaryOperator::Add,
            left: passes_before,
            right: one,
        },
        Span::UNDEFINED,
    );
    let too_many = function.expressions.append(
        Expression::Binary {
            op: BinaryOperator::Greater,
            left: passes_with_this_one,
            right: limit,
        },
        Span::UNDEFINED,
    );
    let mut head = Block::new();
    head.push(
        Statement::Emit(Range::new_from_bounds(passes_before, too_many)),
        Span::UNDEFINED,
    );
    head.push(
        Statement::Store {
            pointer,
            value: passes_with_this_one,
        },
        Span::UNDEFINED,
    );
    let mut leave_the_loop = Block::new();
    leave_the_loop.push(Statement::Break, Span::UNDEFINED);
    head.push(
        Statement::If {
            condition: too_many,
            accept: leave_the_loop,
            reject: Block::new(),
        },
        Span::UNDEFINED,
    );
    head.extend_block(std::mem::take(body));
    *body = head;
}
