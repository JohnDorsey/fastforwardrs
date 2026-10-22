// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Compiling a shader: the rewritten GLSL goes through naga's front end, is validated, and is written as the WGSL that the picture engine runs (see
//! `engine/effect_shaders.rs` for what that has to be). Whatever is wrong with it is told with the line and the column of the ISF file (PH-11).
//!
//! **The compiler runs on a thread of its own, with a large stack, and a panic in it is a refusal** and not the end of the editor (EN-05): a shader that makes naga panic is a
//! shader that cannot be used, and says so. (Brackets nested deeper than [`MOST_NESTING`](super::rewrite::MOST_NESTING) are refused before the compiler sees them, because a stack
//! that is overrun ends the process and cannot be caught.) Only the **first** syntax error is told: the front end stops at it.

use naga::front::glsl::{Frontend, Options};

use super::reader::{IsfFile, IsfProblem, Shape};
use super::rewrite::{Rewritten, rewrite};
use super::structure::structure_of;
use crate::engine::ShaderStructure;

/// The name of the entry point of a compiled shader: the one the engine looks for.
pub const ENTRY_POINT: &str = "fs_main";

/// The stack of the thread that compiles: far more than a shader with brackets nested as deeply as they are let be needs.
const STACK_OF_THE_COMPILER: usize = 64 * 1024 * 1024;

/// A shader that was compiled.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledShader {
    /// The WGSL module, with the fragment entry point [`ENTRY_POINT`].
    pub wgsl: String,
    /// The images the shader reads and draws into, in the order they are bound in (the kth is at binding `1 + 2k` and its sampler at `2 + 2k`).
    pub image_names: Vec<String>,
    /// What was changed in the file that a person might want to know of.
    pub notes: Vec<String>,
    /// What the shader is for (see [`Shape`]).
    pub shape: Shape,
    /// What the engine is told of the shader besides its code: the pictures it reads, the buffers it draws into, its passes. **A shader can compile and have none**: what this version
    /// cannot give a shader (images of its own, pictures but the one the effect works on) is told here, as the reason that it cannot be run.
    pub structure: Result<ShaderStructure, Vec<IsfProblem>>,
}

/// The place in the ISF file that a place of the rewritten GLSL is, if it is one: the generated beginning and the generated end are no place of the file.
fn place_in_the_file(
    rewritten: &Rewritten,
    line: u32,
    column: u32,
    lines_of_the_file: u32,
) -> (Option<u32>, Option<u32>) {
    match line.checked_sub(rewritten.prelude_lines) {
        Some(file_line) if (1..=lines_of_the_file).contains(&file_line) => (Some(file_line), Some(column)),
        _ => (None, None),
    }
}

/// Compiles the shader of `file`, on the thread that calls this. (See [`compile_in_a_worker`] for the one that does not trust the compiler.)
pub fn compile(file: &IsfFile) -> Result<CompiledShader, Vec<IsfProblem>> {
    let rewritten = rewrite(file).map_err(|problem| vec![problem])?;
    let lines_of_the_file = file.body.lines().count() as u32;
    let mut frontend = Frontend::default();
    let mut module = frontend
        .parse(&Options::from(naga::ShaderStage::Fragment), &rewritten.glsl)
        .map_err(|errors| {
            errors
                .errors
                .iter()
                .map(|error| {
                    let location = error.location(&rewritten.glsl);
                    let (line, column) = location.map_or((None, None), |location| {
                        place_in_the_file(
                            &rewritten,
                            location.line_number,
                            location.line_position,
                            lines_of_the_file,
                        )
                    });
                    IsfProblem {
                        line,
                        column,
                        message: error.kind.to_string(),
                    }
                })
                .collect::<Vec<_>>()
        })?;
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .map_err(|error| {
            let mut cause = error.as_inner().to_string();
            let mut source = std::error::Error::source(error.as_inner());
            while let Some(next) = source {
                cause = format!("{cause}: {next}");
                source = next.source();
            }
            let mut problems: Vec<IsfProblem> = error
                .spans()
                .map(|(span, label)| {
                    let location = span.location(&rewritten.glsl);
                    let (line, column) = place_in_the_file(
                        &rewritten,
                        location.line_number,
                        location.line_position,
                        lines_of_the_file,
                    );
                    IsfProblem {
                        line,
                        column,
                        message: format!("{cause} ({label})"),
                    }
                })
                .collect();
            if problems.is_empty() {
                problems.push(IsfProblem::nowhere_in_particular(format!(
                    "the shader is not valid: {}",
                    error.as_inner()
                )));
            }
            problems
        })?;
    // The entry point is the one that the engine looks for.
    for entry_point in &mut module.entry_points {
        if entry_point.stage == naga::ShaderStage::Fragment {
            entry_point.name = ENTRY_POINT.to_owned();
        }
    }
    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty()).map_err(|error| {
            vec![IsfProblem::nowhere_in_particular(format!(
                "the shader cannot be written as WGSL: {error}"
            ))]
        })?;
    let structure = structure_of(file, &rewritten.image_names);
    Ok(CompiledShader {
        wgsl,
        image_names: rewritten.image_names,
        notes: rewritten.notes,
        shape: file.shape(),
        structure,
    })
}

/// Compiles the shader of `file` on a thread of its own, which a panic does not take the program down with: a panic is a problem like any other.
pub fn compile_in_a_worker(file: &IsfFile) -> Result<CompiledShader, Vec<IsfProblem>> {
    in_a_worker(|| compile(file))
}

/// Does `job` on a thread of its own with a large stack, and makes a panic in it a problem.
fn in_a_worker<T: Send>(job: impl FnOnce() -> Result<T, Vec<IsfProblem>> + Send) -> Result<T, Vec<IsfProblem>> {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("isf compiler".to_owned())
            .stack_size(STACK_OF_THE_COMPILER)
            .spawn_scoped(scope, job);
        match worker {
            Ok(worker) => worker.join().unwrap_or_else(|panic| {
                let said = panic
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "it said nothing".to_owned());
                Err(vec![IsfProblem::nowhere_in_particular(format!(
                    "the compiler stopped on this shader and it cannot be used ({said})"
                ))])
            }),
            Err(error) => Err(vec![IsfProblem::nowhere_in_particular(format!(
                "the compiler could not be started: {error}"
            ))]),
        }
    })
}

#[cfg(test)]
mod tests;
