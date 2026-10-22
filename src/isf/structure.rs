// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the engine is told of a shader besides its code: the pictures it reads, the buffers it draws into, and its passes (see `docs/engine-phase-6-4-design.md`, K1, K2 and K4).
//! Made from the header of the file and the order that the rewriting step bound the images in.

use super::reader::{IsfFile, IsfInputKind, IsfProblem};
use super::rewrite::NUMBERS_BEFORE_THE_INPUTS;
use super::size_expression::{NamesOfNumbers, read_size_expression};
use crate::engine::{BufferSpec, ImageSlot, PassSpec, ShaderStructure, SizeExpression};

/// What the engine is told of the shader of `file`, whose images the rewriting step bound in the order of `image_names` (the image inputs, then the targets of the passes, then
/// the imported images).
pub fn structure_of(file: &IsfFile, image_names: &[String]) -> Result<ShaderStructure, Vec<IsfProblem>> {
    let says = |message: String| vec![IsfProblem::nowhere_in_particular(message)];

    // The buffers are the targets of the passes, in the order they are first drawn into; a buffer is to last if any pass that draws into it says so.
    let mut buffer_names: Vec<&str> = Vec::new();
    let mut buffers: Vec<BufferSpec> = Vec::new();
    for pass in &file.passes {
        let Some(target) = pass.target.as_deref() else { continue };
        if file
            .inputs
            .iter()
            .any(|input| input.kind == IsfInputKind::Image && input.name == target)
        {
            return Err(says(format!(
                "a pass draws into {target}, which is the name of a picture that the shader is given"
            )));
        }
        match buffer_names.iter().position(|name| *name == target) {
            Some(place) => {
                buffers[place].persistent |= pass.persistent;
                buffers[place].float |= pass.float;
            }
            None => {
                buffer_names.push(target);
                buffers.push(BufferSpec {
                    persistent: pass.persistent,
                    float: pass.float,
                });
            }
        }
    }

    // The inputs that sizes can be made from: the ones that are one number, and where that number is among the numbers of a call.
    let mut names: Vec<(&str, usize)> = Vec::new();
    let mut place = NUMBERS_BEFORE_THE_INPUTS;
    for input in &file.inputs {
        if matches!(
            input.kind,
            IsfInputKind::Float { .. } | IsfInputKind::Long { .. } | IsfInputKind::Bool { .. }
        ) {
            names.push((input.name.as_str(), place));
        }
        place += input.kind.numbers();
    }
    let names: NamesOfNumbers<'_> = &names;

    let mut passes = Vec::with_capacity(file.passes.len().max(1));
    for (number, pass) in file.passes.iter().enumerate() {
        let expression = |which: &str, text: &Option<String>| -> Result<Option<SizeExpression>, Vec<IsfProblem>> {
            text.as_deref()
                .map(|text| {
                    read_size_expression(text, names)
                        .map_err(|why| says(format!("the {which} of pass number {}: {why}", number + 1)))
                })
                .transpose()
        };
        passes.push(PassSpec {
            target: pass
                .target
                .as_deref()
                .and_then(|target| buffer_names.iter().position(|name| *name == target)),
            width: expression("WIDTH", &pass.width)?,
            height: expression("HEIGHT", &pass.height)?,
        });
    }
    // (A shader with no PASSES is one pass that draws the picture the effect makes.)
    if passes.is_empty() {
        passes.push(PassSpec {
            target: None,
            width: None,
            height: None,
        });
    }

    let mut images = Vec::with_capacity(image_names.len());
    for name in image_names {
        images.push(match buffer_names.iter().position(|buffer| *buffer == name) {
            Some(buffer) => ImageSlot::Buffer(buffer),
            // (The picture that the effect works on: the only picture that a shader is given, so far.)
            None if name == "inputImage" => ImageSlot::Picture,
            // (The images the shader brings with it: the nth is the nth that its header lists.)
            None if file.imported.iter().any(|image| image.name == *name) => ImageSlot::Imported(
                file.imported
                    .iter()
                    .position(|image| image.name == *name)
                    .expect("checked in the line above"),
            ),
            None => {
                return Err(says(format!(
                    "this shader reads a picture called {name}, which this version cannot give it yet"
                )));
            }
        });
    }
    Ok(ShaderStructure {
        images,
        buffers,
        passes,
        pass_index_number: Some(3),
    })
}

#[cfg(test)]
mod tests;
