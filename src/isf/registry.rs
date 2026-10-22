// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which shaders are usable: the kinds of effect that are ISF shaders are compiled **once each** (by the key, which has the digest of the text in it), the result is kept, and what
//! compiled is put on the engine's shelf (`engine::publish_effect_shader`) for it to take when a frame asks for the effect.
//!
//! A shader is **usable** if it compiles, and is one that this version can run: so far a filter of one picture with no passes of its own and no imported images (see
//! `docs/engine-phase-6-design.md`, J8). Any other is a kind of effect that is *not installed*: it is kept in the project with its values and does nothing, and the properties box says
//! why ([`why_not_usable`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::compile::{CompiledShader, compile_in_a_worker};
use super::descriptor::key_is_of_the_text;
use super::reader::{IsfFile, IsfProblem, Shape, read};
use crate::project::{EffectDescriptor, ShaderLanguage};

/// What was made of the text of a shader.
type Made = Arc<Result<CompiledShader, Vec<IsfProblem>>>;

/// What was made of each text of a shader, by the key it was under: a key has one text if the key is the key of the text, and a project file that was changed by hand can have
/// another text under it, so what is kept for a key is a list, and a text is looked for in it.
type TextsOfAKey = Vec<(String, Made)>;

static COMPILED: Mutex<Option<HashMap<String, TextsOfAKey>>> = Mutex::new(None);

/// The reasons that a file that was read cannot be run by this version, if there are any (the file reads and compiles, and still is not what the engine runs yet).
fn what_cannot_be_run_yet(file: &IsfFile) -> Option<IsfProblem> {
    let says = |message: String| Some(IsfProblem::nowhere_in_particular(message));
    if file.shape() != Shape::Filter {
        return says(format!(
            "this shader is a {}, which this version cannot run yet (only filters, which read inputImage)",
            if file.shape() == Shape::Generator {
                "generator"
            } else {
                "transition"
            }
        ));
    }
    if let Some(other) = file.other_image_inputs().first() {
        return says(format!(
            "this shader reads a picture called {other}, which this version cannot give it yet"
        ));
    }
    if !file.passes.is_empty() {
        return says("this shader has passes of its own, which this version cannot run yet".to_owned());
    }
    if !file.imported.is_empty() {
        return says("this shader has images of its own (IMPORTED), which this version cannot run yet".to_owned());
    }
    None
}

/// Reads and compiles the text of an ISF shader, and puts what compiled on the engine's shelf under `key`.
fn make(key: &str, text: &str) -> Result<CompiledShader, Vec<IsfProblem>> {
    if !key_is_of_the_text(key, text) {
        return Err(vec![IsfProblem::nowhere_in_particular(
            "the text of this shader is not the one that its key was made from (the file of the project was changed by hand), so it is not run",
        )]);
    }
    let file = read(text).map_err(|problem| vec![problem])?;
    if let Some(problem) = what_cannot_be_run_yet(&file) {
        return Err(vec![problem]);
    }
    let compiled = compile_in_a_worker(&file)?;
    crate::engine::publish_effect_shader(key, &compiled.wgsl);
    Ok(compiled)
}

/// What was made of the shader of `descriptor`, once, or `None` if the effect is no ISF shader.
pub fn compiled_shader_of(descriptor: &EffectDescriptor) -> Option<Made> {
    let source = descriptor.shader.as_ref()?;
    if source.language != ShaderLanguage::Isf {
        return None;
    }
    let key = descriptor.key.as_ref();
    if let Some(made) = COMPILED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|compiled| compiled.get(key))
        .and_then(|texts| texts.iter().find(|(text, _)| *text == *source.text))
        .map(|(_, made)| made)
    {
        return Some(Arc::clone(made));
    }
    // (Not under the lock: compiling takes a few milliseconds, and another thread that asks for another shader need not wait for it. Two threads that ask for one both compile it,
    // and the second to finish is the one that is kept: they made the same.)
    let made: Made = Arc::new(make(key, &source.text));
    COMPILED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .entry(key.to_owned())
        .or_default()
        .push((source.text.to_string(), Arc::clone(&made)));
    Some(made)
}

/// Whether the effect described by `descriptor` is an ISF shader that this version can run.
pub fn is_usable(descriptor: &EffectDescriptor) -> bool {
    matches!(compiled_shader_of(descriptor).as_deref(), Some(Ok(_)))
}

/// Why the effect described by `descriptor`, which is an ISF shader, cannot be run, in words, with the place in the file if the mistake has one. `None` if it can be, or is no ISF shader.
pub fn why_not_usable(descriptor: &EffectDescriptor) -> Option<String> {
    match compiled_shader_of(descriptor).as_deref()? {
        Ok(_) => None,
        Err(problems) => Some(
            problems
                .iter()
                .map(IsfProblem::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        ),
    }
}

#[cfg(test)]
mod tests;
