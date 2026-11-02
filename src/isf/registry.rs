// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which shaders are usable: the kinds of effect that are ISF shaders are compiled **once each** (by the key, which has the digest of the text in it), the result is kept, and what
//! compiled is put on the engine's shelf (`engine::publish_effect_shader`) for it to take when a frame asks for the effect.
//!
//! A shader is **usable** if it compiles, and is one that this version can run: a filter (it reads `inputImage`), a generator (it reads no picture) or a transition (it reads `startImage`, what is behind the
//! clip, and `endImage`, the clip: phase 6.6), with passes and buffers of its own and the images it brings (see `docs/engine-phase-6-design.md`, J8, `docs/engine-phase-6-4-design.md` and `docs/engine-phase-6-6-design.md`). Any other is a kind of effect that is *not installed*: it is kept in the project with its values and does nothing, and the properties box says
//! why ([`why_not_usable`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::compile::{CompiledShader, compile_in_a_worker, compile_in_a_worker_with_driven_inputs};
use super::descriptor::key_is_of_the_source;
use super::reader::{IsfFile, IsfProblem, Shape, read};
use super::rewrite::DrivenInput;
use crate::engine::ShaderStructure;
use crate::project::{EffectDescriptor, ShaderLanguage, ShaderSource};

/// What was made of the text of a shader.
type Made = Arc<Result<CompiledShader, Vec<IsfProblem>>>;

/// What was made of each text of a shader, by the key it was under: a key has one text if the key is the key of the text, and a project file that was changed by hand can have
/// another text under it, so what is kept for a key is a list, and a text is looked for in it.
type TextsOfAKey = Vec<(ShaderSource, Made)>;

static COMPILED: Mutex<Option<HashMap<String, TextsOfAKey>>> = Mutex::new(None);

/// The reasons that a file that was read cannot be run by this version, if there are any (the file reads and compiles, and still is not what the engine runs yet).
fn what_cannot_be_run_yet(file: &IsfFile) -> Option<IsfProblem> {
    let says = |message: String| Some(IsfProblem::nowhere_in_particular(message));
    // A transition reads two pictures, startImage (what is behind the layer) and endImage (the layer's own); one of them alone is not a transition, and a filter has inputImage.
    let (start, end, input) = (
        file.has_image_input("startImage"),
        file.has_image_input("endImage"),
        file.has_image_input("inputImage"),
    );
    if start != end {
        let (has, lacks) = if start {
            ("startImage", "endImage")
        } else {
            ("endImage", "startImage")
        };
        return says(format!(
            "this shader reads a picture called {has} and none called {lacks}: a transition reads both (startImage is what is behind the clip, endImage is the clip)"
        ));
    }
    if start && input {
        return says(
            "this shader reads inputImage as well as startImage and endImage: a filter reads inputImage and a transition reads the other two, and which picture this one works on is not said"
                .to_owned(),
        );
    }
    if let Some(other) = file.other_image_inputs().first() {
        return says(format!(
            "this shader reads a picture called {other}, which this version cannot give it yet"
        ));
    }
    None
}

/// What is wrong with the images that the shader of `file` brings with it, if anything is: the project must have the ones the header lists, in that order, and each must be a picture.
fn what_is_wrong_with_the_images(file: &IsfFile, source: &ShaderSource) -> Option<IsfProblem> {
    let says = |message: String| Some(IsfProblem::nowhere_in_particular(message));
    for (number, wanted) in file.imported.iter().enumerate() {
        let Some(image) = source.images.get(number) else {
            return says(format!(
                "this shader has an image of its own, {}, which the project does not have (it is kept with the shader, and this one was not given it)",
                wanted.name
            ));
        };
        if image.name() != wanted.name {
            return says(format!(
                "the images this shader brings are not the ones its header lists, in that order ({} where {} is expected)",
                image.name(),
                wanted.name
            ));
        }
        if let Err(why) = image.pixels() {
            return says(format!(
                "the image {} of this shader cannot be used: {why}",
                wanted.name
            ));
        }
    }
    if source.images.len() > file.imported.len() {
        return says(format!(
            "this shader brings {} images and its header lists {}",
            source.images.len(),
            file.imported.len()
        ));
    }
    None
}

/// Reads and compiles the text of an ISF shader, and puts what compiled on the engine's shelf under `key`.
fn make(key: &str, source: &ShaderSource) -> Result<CompiledShader, Vec<IsfProblem>> {
    let text: &str = &source.text;
    if !key_is_of_the_source(key, text, &source.images) {
        return Err(vec![IsfProblem::nowhere_in_particular(
            "the text of this shader is not the one that its key was made from (the file of the project was changed by hand), so it is not run",
        )]);
    }
    let file = read(text).map_err(|problem| vec![problem])?;
    if let Some(problem) = what_cannot_be_run_yet(&file).or_else(|| what_is_wrong_with_the_images(&file, source)) {
        return Err(vec![problem]);
    }
    let compiled = compile_in_a_worker(&file)?;
    // (What it is made of must be something the engine can run: told here, in words, and the shader is not given to it.)
    let structure = compiled.structure.clone()?;
    structure.check().map_err(|why| {
        vec![IsfProblem::nowhere_in_particular(format!(
            "this shader cannot be run yet: {why}"
        ))]
    })?;
    crate::engine::publish_effect_shader_with(key, &compiled.wgsl, structure);
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
        .and_then(|texts| texts.iter().find(|(kept, _)| kept == source))
        .map(|(_, made)| made)
    {
        return Some(Arc::clone(made));
    }
    // (Not under the lock: compiling takes a few milliseconds, and another thread that asks for another shader need not wait for it. Two threads that ask for one both compile it,
    // and the second to finish is the one that is kept: they made the same.)
    let made: Made = Arc::new(make(key, source));
    COMPILED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .entry(key.to_owned())
        .or_default()
        .push((source.clone(), Arc::clone(&made)));
    Some(made)
}

/// The key that the engine has the shader of the effect with key `base_key` under when the numbers `driven` are read from masks (the key of the shader as it is, and which numbers are read
/// from which mask as which channel).
pub fn driven_variant_key(base_key: &str, driven: &[DrivenInput]) -> String {
    let parts: Vec<String> = driven
        .iter()
        .map(|one| format!("{}={}:{:?}", one.name, one.mask, one.channel))
        .collect();
    format!("{base_key}+masks[{}]", parts.join(","))
}

/// Makes the shader of `source` with the numbers `driven` read from masks, and puts it on the engine's shelf under `key`.
fn make_variant(key: &str, source: &ShaderSource, driven: &[DrivenInput]) -> Result<(), Vec<IsfProblem>> {
    let file = read(&source.text).map_err(|problem| vec![problem])?;
    let compiled = compile_in_a_worker_with_driven_inputs(&file, driven)?;
    let structure = compiled.structure.clone()?;
    structure.check().map_err(|why| {
        vec![IsfProblem::nowhere_in_particular(format!(
            "this shader cannot be run yet: {why}"
        ))]
    })?;
    crate::engine::publish_effect_shader_with(key, &compiled.wgsl, structure);
    Ok(())
}

/// The inputs that set the size of a buffer of each shader (see `structure::inputs_that_set_a_size`), by the key of its effect: worked out once.
static SIZE_SETTERS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

/// The names of the inputs of the shader of `descriptor` that a size of one of its buffers is made from. A size is worked out once for the whole picture, so such an input cannot be read
/// from a mask (`docs/shader-masks-design.md`). Nothing for an effect that is no ISF shader, or whose text cannot be read.
pub fn inputs_that_set_the_size_of_a_buffer(descriptor: &EffectDescriptor) -> Vec<String> {
    let Some(source) = descriptor
        .shader
        .as_ref()
        .filter(|source| source.language == ShaderLanguage::Isf)
    else {
        return Vec::new();
    };
    let key = descriptor.key.to_string();
    if let Some(found) = SIZE_SETTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|found| found.get(&key))
    {
        return found.clone();
    }
    let found = read(&source.text)
        .map(|file| super::structure::inputs_that_set_a_size(&file))
        .unwrap_or_default();
    SIZE_SETTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(key, found.clone());
    found
}

/// What was made of each shader that reads masks, by its key: whether it could be made.
static COMPILED_VARIANTS: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// The key under which the engine has the shader of `descriptor` with the numbers `driven` read from masks, made and put on the engine's shelf if it was not yet (the same shader with the
/// same numbers read from the same masks is made once). `None` if the effect is no ISF shader that this version can run, or if the shader cannot be made that way (the effect then does
/// nothing, as one that is not installed does).
pub fn driven_variant_of(descriptor: &EffectDescriptor, driven: &[DrivenInput]) -> Option<String> {
    let source = descriptor.shader.as_ref()?;
    if driven.is_empty() || !matches!(compiled_shader_of(descriptor).as_deref(), Some(Ok(_))) {
        return None;
    }
    let key = driven_variant_key(descriptor.key.as_ref(), driven);
    if let Some(made) = COMPILED_VARIANTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|compiled| compiled.get(&key))
    {
        return made.then_some(key);
    }
    // (Not under the lock, as for a shader that reads no mask.)
    let made = make_variant(&key, source, driven).is_ok();
    COMPILED_VARIANTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(key.clone(), made);
    made.then_some(key)
}

/// What the shader of `descriptor` is for, if it is an ISF shader that this version can run (a filter reads `inputImage`; a generator reads no picture; a transition reads `startImage` and `endImage`).
pub fn shape_of(descriptor: &EffectDescriptor) -> Option<Shape> {
    match compiled_shader_of(descriptor)?.as_ref() {
        Ok(compiled) => Some(compiled.shape),
        Err(_) => None,
    }
}

/// Whether the effect described by `descriptor` is an ISF shader that this version can run.
pub fn is_usable(descriptor: &EffectDescriptor) -> bool {
    matches!(compiled_shader_of(descriptor).as_deref(), Some(Ok(_)))
}

/// Whether the effect described by `descriptor` is an ISF shader that this version can run and that keeps something from one frame to the next (a buffer with `PERSISTENT`), so that
/// what it shows depends on the frames before it.
pub fn depends_on_earlier_frames(descriptor: &EffectDescriptor) -> bool {
    matches!(
        compiled_shader_of(descriptor).as_deref(),
        Some(Ok(compiled)) if compiled.structure.as_ref().is_ok_and(ShaderStructure::depends_on_earlier_frames)
    )
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
