// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an ISF file is to the rest of the program: **a kind of effect** (an [`EffectDescriptor`]), with the values its inputs make, and the numbers that those values are in a call of
//! its shader (see `docs/engine-phase-6-design.md`, J3, J5 and J6).
//!
//! The mapping of the inputs (PH-10): a `float` is a number (its `MIN` and `MAX` are the slider's range), a `bool` a switch, a `long` a choice (a value of it is called by the whole number
//! it stands for), a `color` a colour, a `point2D` a pair, an `event` a switch that is off; an image is no value (it is an input of the layer). **The name of an input is the id of its
//! value**, which is what a project file keeps it under.

use std::borrow::Cow;

use super::reader::{IsfFile, IsfInputKind};
use crate::digest::Sha256Digest;
use crate::project::{
    ChoiceOption, EffectDescriptor, EffectDomain, ParameterControl, ParameterDescriptor, PropertyValue, ShaderImage,
    ShaderLanguage, ShaderSource, TextColour, UnknownFields, ValueScale,
};

/// How many characters of the digest of a shader are in its key.
const CHARACTERS_OF_THE_DIGEST_IN_A_KEY: usize = 12;

/// What a person sees of a file's name: without its ending, with spaces for underscores and minuses, in lower case.
fn words_of(file_name: &str) -> String {
    let stem = file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem);
    stem.replace(['_', '-'], " ").trim().to_lowercase()
}

fn slug_of(words: &str) -> String {
    let slug: String = words
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    let slug = slug.trim_matches('_').to_owned();
    if slug.is_empty() { "shader".to_owned() } else { slug }
}

/// The digest of what a shader is, as hex: the digest of its text; and, if it brings images, of its text and then, for each image, its name and the digest of its bytes (so that the same
/// text with other images is another shader, and a shader with none has the digest of its text, as it always had).
fn digest_of_the_source(text: &str, images: &[ShaderImage]) -> String {
    if images.is_empty() {
        return Sha256Digest::of_bytes(text.as_bytes()).to_hex();
    }
    let mut bytes = text.as_bytes().to_vec();
    for image in images {
        bytes.push(0);
        bytes.extend_from_slice(image.name().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(Sha256Digest::of_bytes(image.bytes()).to_hex().as_bytes());
    }
    Sha256Digest::of_bytes(&bytes).to_hex()
}

/// The key of the kind of effect that `source` is, under the name `file_name`: **`isf:` the name, a minus and the first characters of the digest of the whole text**, so that two files of
/// one name are two kinds, and one file is one kind wherever it is read.
pub fn key_of(file_name: &str, source: &str) -> String {
    key_of_a_source_with_images(file_name, source, &[])
}

/// The same for a shader that brings `images` with it (they are a part of what the shader is: its digest is made of its text, and then of the name and the digest of the bytes of each image).
pub fn key_of_a_source_with_images(file_name: &str, source: &str, images: &[ShaderImage]) -> String {
    let digest = digest_of_the_source(source, images);
    format!(
        "isf:{}-{}",
        slug_of(&words_of(file_name)),
        &digest[..CHARACTERS_OF_THE_DIGEST_IN_A_KEY]
    )
}

/// Whether `key` is the key of a shader whose text is `text`: it ends with the first characters of the digest of the text (see [`key_of`]). A project file that was changed by hand can
/// have a text that is not the one its key was made from; two kinds of one key that are two texts must not be run as one.
pub fn key_is_of_the_text(key: &str, text: &str) -> bool {
    key_is_of_the_source(key, text, &[])
}

/// The same for a shader that brings `images` with it.
pub fn key_is_of_the_source(key: &str, text: &str, images: &[ShaderImage]) -> bool {
    let digest = digest_of_the_source(text, images);
    key.rsplit_once('-')
        .is_some_and(|(_, ending)| ending == &digest[..CHARACTERS_OF_THE_DIGEST_IN_A_KEY])
}

fn colour_of(default: [f32; 4]) -> TextColour {
    let code = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    TextColour::new(code(default[0]), code(default[1]), code(default[2]), code(default[3]))
}

fn parameter(id: &str, label: &str, default: PropertyValue, control: ParameterControl) -> ParameterDescriptor {
    ParameterDescriptor {
        id: Cow::Owned(id.to_owned()),
        name: Cow::Owned(label.to_lowercase()),
        label: Cow::Owned(label.to_owned()),
        default,
        range: None,
        unit: Cow::Borrowed(""),
        decimals: None,
        unknown_fields: UnknownFields::none(),
        scale: ValueScale::Linear,
        control,
    }
}

/// The kind of effect that the file `file_name` (its text is `source`, which `file` was read from) is. It is not a kind that is installed until its shader is: that is for the host
/// to say (`docs/engine-phase-6-design.md`, J3).
pub fn descriptor_of(file: &IsfFile, file_name: &str, source: &str) -> EffectDescriptor {
    descriptor_with_images_of(file, file_name, source, Vec::new())
}

/// The same for a shader that brings `images` with it: the ones that its header lists as `IMPORTED`, in that order (see [`ShaderSource::images`]); they are a part of the kind, and of its key.
pub fn descriptor_with_images_of(
    file: &IsfFile,
    file_name: &str,
    source: &str,
    images: Vec<ShaderImage>,
) -> EffectDescriptor {
    let mut parameters = Vec::new();
    for input in &file.inputs {
        let made = match &input.kind {
            IsfInputKind::Float {
                minimum,
                maximum,
                default,
            } => {
                let mut value = parameter(
                    &input.name,
                    &input.label,
                    PropertyValue::Number(*default),
                    ParameterControl::Slider,
                );
                value.range = Some((*minimum, *maximum));
                value
            }
            IsfInputKind::Bool { default } => parameter(
                &input.name,
                &input.label,
                PropertyValue::Flag(*default),
                ParameterControl::Toggle,
            ),
            IsfInputKind::Event => parameter(
                &input.name,
                &input.label,
                PropertyValue::Flag(false),
                ParameterControl::Toggle,
            ),
            IsfInputKind::Long {
                values,
                labels,
                default,
            } => {
                let options: Vec<ChoiceOption> = values
                    .iter()
                    .zip(labels)
                    .map(|(value, label)| ChoiceOption {
                        id: Cow::Owned(value.to_string()),
                        label: Cow::Owned(label.clone()),
                    })
                    .collect();
                parameter(
                    &input.name,
                    &input.label,
                    PropertyValue::Words(default.to_string()),
                    ParameterControl::Choice(Cow::Owned(options)),
                )
            }
            IsfInputKind::Color { default } => parameter(
                &input.name,
                &input.label,
                PropertyValue::Colour(colour_of(*default)),
                ParameterControl::Automatic,
            ),
            IsfInputKind::Point2D { default } => parameter(
                &input.name,
                &input.label,
                PropertyValue::Pair(default[0], default[1]),
                ParameterControl::Automatic,
            ),
            // (A picture is an input of the layer and sound is not given to shaders yet: neither is a value.)
            IsfInputKind::Image | IsfInputKind::Audio => continue,
        };
        parameters.push(made);
    }
    let words = words_of(file_name);
    EffectDescriptor {
        key: Cow::Owned(key_of_a_source_with_images(file_name, source, &images)),
        version: (1, 0),
        name: Cow::Owned(if words.is_empty() { "shader".to_owned() } else { words }),
        description: Cow::Owned(file.description.clone()),
        category: Cow::Owned(file.categories.first().cloned().unwrap_or_else(|| "Shader".to_owned())),
        domain: EffectDomain::Picture,
        // (A shader can do anything to the opacity of a pixel and to the pixels round it.)
        changes_each_pixel_by_itself: false,
        makes_sound_from_notes: false,
        parameters: Cow::Owned(parameters),
        unknown_fields: UnknownFields::none(),
        shader: Some(ShaderSource {
            language: ShaderLanguage::Isf,
            text: Cow::Owned(source.to_owned()),
            images,
        }),
    }
}

/// The numbers that these values are in a call of a shader, one after another in the order of the values: a number is one, a pair two, a colour four (red, green, blue and opacity, each
/// from 0 to 1, **not** multiplied by the opacity), a switch 0 or 1, a choice the whole number it stands for. (A choice that is not a whole number is 0, as ISF has no other.)
pub fn numbers_of_the_values(values: &[PropertyValue]) -> Vec<f32> {
    let mut numbers = Vec::with_capacity(values.len());
    for value in values {
        match value {
            PropertyValue::Number(number) => numbers.push(*number),
            PropertyValue::Pair(across, down) => numbers.extend([*across, *down]),
            PropertyValue::Colour(colour) => numbers.extend(
                [colour.red, colour.green, colour.blue, colour.alpha].map(|channel| f32::from(channel) / 255.0),
            ),
            PropertyValue::Flag(on) => numbers.push(f32::from(u8::from(*on))),
            PropertyValue::Words(words) => {
                numbers.push(words.trim().parse::<i64>().map_or(0.0, |number| number as f32))
            }
        }
    }
    numbers
}

#[cfg(test)]
mod tests;
