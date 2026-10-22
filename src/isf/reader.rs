// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading an ISF file: the JSON header (the first block comment of the file that begins with a brace) and the GLSL after it.
//!
//! **The header is read leniently** (PH-10): files in the wild have a trailing comma, a `//` comment inside the header, a key that is there twice (the last wins) and an `ISFVSN` that is a
//! number and not a string, and a person's shader that works in another program should work here. **The body keeps its line count**: the header is replaced by spaces and
//! line ends, so that a line number that the compiler gives is the line of the file.

use std::collections::HashSet;
use std::fmt;

use serde_json::{Map, Value};

use crate::engine::MOST_NUMBERS_OF_A_FOREIGN_EFFECT;

/// How many of the numbers that a call of a shader is given are for the shader's own inputs: the others are for `TIME`, `TIMEDELTA`, `FRAMEINDEX` and `PASSINDEX`, which are first in every
/// call (see `docs/engine-phase-6-design.md`, J6).
pub const MOST_NUMBERS_OF_THE_INPUTS: usize = MOST_NUMBERS_OF_A_FOREIGN_EFFECT - 4;

/// What is wrong with a file, and where. A file that cannot be read is not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsfProblem {
    /// The line of the file, counted from 1, if the problem is in a place.
    pub line: Option<u32>,
    /// The column, counted from 1 in characters.
    pub column: Option<u32>,
    pub message: String,
}

impl IsfProblem {
    pub(super) fn here(line: u32, column: u32, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            column: Some(column),
            message: message.into(),
        }
    }

    pub(super) fn nowhere_in_particular(message: impl Into<String>) -> Self {
        Self {
            line: None,
            column: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for IsfProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(formatter, "line {line}, column {column}: {}", self.message),
            (Some(line), None) => write!(formatter, "line {line}: {}", self.message),
            _ => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for IsfProblem {}

/// What an input of the shader is, with what its header says of it.
#[derive(Clone, Debug, PartialEq)]
pub enum IsfInputKind {
    Float {
        minimum: f32,
        maximum: f32,
        default: f32,
    },
    Bool {
        default: bool,
    },
    /// A choice among whole numbers: `values[n]` is called `labels[n]`.
    Long {
        values: Vec<i64>,
        labels: Vec<String>,
        default: i64,
    },
    /// Red, green, blue and opacity, each from 0 to 1, not multiplied by the opacity.
    Color {
        default: [f32; 4],
    },
    /// A place on the picture, in pixels of the picture that the shader draws, counted from the bottom left.
    Point2D {
        default: [f32; 2],
    },
    /// A picture that the shader reads: `inputImage` of a filter, `startImage` and `endImage` of a transition.
    Image,
    /// A button, which is on for the one frame it is pressed in.
    Event,
    /// Sound (`audio` and `audioFFT`), which this program does not give to shaders yet.
    Audio,
}

impl IsfInputKind {
    /// How many numbers the input takes in a call of the shader (an image takes none).
    pub fn numbers(&self) -> usize {
        match self {
            Self::Float { .. } | Self::Bool { .. } | Self::Long { .. } | Self::Event => 1,
            Self::Color { .. } => 4,
            Self::Point2D { .. } => 2,
            Self::Image | Self::Audio => 0,
        }
    }
}

/// One of the inputs the header lists.
#[derive(Clone, Debug, PartialEq)]
pub struct IsfInput {
    /// The name the shader uses (a name of GLSL), which is never a place or a position: what a project keeps a value under.
    pub name: String,
    /// What a person is shown, the header's `LABEL` or else the name.
    pub label: String,
    pub kind: IsfInputKind,
}

/// A pass of a multi-pass shader.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IsfPass {
    /// The name of the buffer the pass draws into; the last pass has none (it draws the result).
    pub target: Option<String>,
    /// Whether the buffer keeps what was drawn into it for the next frame.
    pub persistent: bool,
    /// Whether the buffer holds floating point numbers and not 8 bits of each colour.
    pub float: bool,
    /// The expressions of the buffer's size (`$WIDTH/2`), if it has its own.
    pub width: Option<String>,
    pub height: Option<String>,
}

/// An image that the shader brings with it (`IMPORTED`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsfImportedImage {
    pub name: String,
    pub path: String,
}

/// What a shader is for, from the images it reads (PH-13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Reads `inputImage` and gives a picture.
    Filter,
    /// Reads no picture and gives one.
    Generator,
    /// Reads `startImage` and `endImage` and a `progress`.
    Transition,
}

/// A file that has been read.
#[derive(Clone, Debug, PartialEq)]
pub struct IsfFile {
    pub description: String,
    pub credit: String,
    pub version: String,
    pub categories: Vec<String>,
    pub inputs: Vec<IsfInput>,
    pub passes: Vec<IsfPass>,
    pub imported: Vec<IsfImportedImage>,
    /// The file with its header replaced by spaces and line ends: what is left is the GLSL, on the lines that it was on.
    pub body: String,
}

impl IsfFile {
    /// Whether the header lists an image input called `name`.
    pub fn has_image_input(&self, name: &str) -> bool {
        self.inputs
            .iter()
            .any(|input| input.name == name && input.kind == IsfInputKind::Image)
    }

    /// What the shader is for.
    pub fn shape(&self) -> Shape {
        if self.has_image_input("startImage") && self.has_image_input("endImage") {
            Shape::Transition
        } else if self.has_image_input("inputImage") {
            Shape::Filter
        } else {
            Shape::Generator
        }
    }

    /// The image inputs that are none of `inputImage`, `startImage` and `endImage`: a picture that the shader wants from somewhere that this program cannot tell it yet (ID-10).
    pub fn other_image_inputs(&self) -> Vec<&str> {
        self.inputs
            .iter()
            .filter(|input| {
                input.kind == IsfInputKind::Image
                    && !matches!(input.name.as_str(), "inputImage" | "startImage" | "endImage")
            })
            .map(|input| input.name.as_str())
            .collect()
    }

    /// How many numbers the inputs take in a call.
    pub fn numbers_of_the_inputs(&self) -> usize {
        self.inputs.iter().map(|input| input.kind.numbers()).sum()
    }
}

/// Names that the shader has of its own, which an input cannot take.
const RESERVED_NAMES: [&str; 10] = [
    "RENDERSIZE",
    "TIME",
    "TIMEDELTA",
    "PASSINDEX",
    "FRAMEINDEX",
    "DATE",
    "isf_FragNormCoord",
    "vv_FragNormCoord",
    "gl_FragColor",
    "gl_FragCoord",
];

/// Where in `source` (a byte offset that is the start of a character) is: the line and the column, both counted from 1, the column in characters.
fn position_of(source: &str, offset: usize) -> (u32, u32) {
    let before = &source[..offset];
    let line = before.matches('\n').count() as u32 + 1;
    let column = before.rsplit('\n').next().map_or(0, |last| last.chars().count()) as u32 + 1;
    (line, column)
}

/// The JSON text with its comments (`//` to the end of the line) and the commas that come before a closing bracket replaced by spaces, so that **every character that is left is
/// where it was** and the places that the JSON reader gives are places of the header.
fn made_strict(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut strict: Vec<char> = characters.clone();
    let (mut in_string, mut escaped) = (false, false);
    let mut at = 0;
    while at < characters.len() {
        let character = characters[at];
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == '/' && characters.get(at + 1) == Some(&'/') {
            while at < characters.len() && characters[at] != '\n' {
                strict[at] = ' ';
                at += 1;
            }
            continue;
        }
        at += 1;
    }
    // The commas that are followed by a closing bracket, with only space between.
    let (mut in_string, mut escaped) = (false, false);
    for at in 0..strict.len() {
        let character = strict[at];
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == ',' {
            let next = strict[at + 1..].iter().find(|next| !next.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                strict[at] = ' ';
            }
        }
    }
    strict.into_iter().collect()
}

/// Reads an ISF file.
pub fn read(source: &str) -> Result<IsfFile, IsfProblem> {
    // The header is the first block comment whose first character after the comment mark is a brace.
    let mut search_from = 0;
    let (start, end) = loop {
        let Some(found) = source[search_from..].find("/*") else {
            return Err(IsfProblem::nowhere_in_particular(
                "the file has no header: an ISF file begins with a comment that holds the JSON of its inputs, /*{ ... }*/",
            ));
        };
        let start = search_from + found;
        let after = &source[start + 2..];
        let is_json = after.trim_start().starts_with('{');
        let Some(length) = after.find("*/") else {
            let (line, column) = position_of(source, start);
            return Err(IsfProblem::here(line, column, "this comment is never closed"));
        };
        let end = start + 2 + length;
        if is_json {
            break (start, end);
        }
        search_from = end + 2;
    };
    let json_start = start + 2;
    let json_text = &source[json_start..end];
    let (first_line, first_column) = position_of(source, json_start);
    let strict = made_strict(json_text);
    let header: Value = serde_json::from_str(&strict).map_err(|error| {
        let line = first_line + error.line() as u32 - 1;
        let column = if error.line() <= 1 {
            first_column + error.column() as u32 - 1
        } else {
            error.column() as u32
        };
        IsfProblem::here(line, column, format!("the header is not JSON: {error}"))
    })?;
    let Some(header) = header.as_object() else {
        return Err(IsfProblem::here(
            first_line,
            first_column,
            "the header is not an object of JSON",
        ));
    };

    let inputs = read_inputs(header, source, json_start, json_text)?;
    let passes = read_passes(header)?;
    let imported = read_imported(header)?;
    let text_of = |key: &str| header.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let version = match header.get("ISFVSN") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    };
    let categories = header
        .get("CATEGORIES")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    // The comment becomes spaces, and its line ends stay.
    let blanked: String = source[start..end + 2]
        .chars()
        .map(|character| if character == '\n' { '\n' } else { ' ' })
        .collect();
    let file = IsfFile {
        description: text_of("DESCRIPTION"),
        credit: text_of("CREDIT"),
        version,
        categories,
        inputs,
        passes,
        imported,
        body: format!("{}{}{}", &source[..start], blanked, &source[end + 2..]),
    };
    if file.numbers_of_the_inputs() > MOST_NUMBERS_OF_THE_INPUTS {
        return Err(IsfProblem::nowhere_in_particular(format!(
            "the inputs take {} numbers, and {MOST_NUMBERS_OF_THE_INPUTS} are the most a shader can have (a colour takes four, a point two)",
            file.numbers_of_the_inputs()
        )));
    }
    Ok(file)
}

/// The place of the name `name` in the header (where it is first written as a string), for the problems that are about it; the place of the header's beginning if it is not found.
fn place_of_the_name(name: &str, source: &str, json_start: usize, json_text: &str) -> (u32, u32) {
    let quoted = format!("\"{name}\"");
    let offset = json_text.find(&quoted).unwrap_or(0);
    position_of(source, json_start + offset)
}

fn is_a_name_of_glsl(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn number_of(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::Bool(flag) => Some(f64::from(u8::from(*flag))),
        _ => None,
    }
}

fn numbers_of(value: Option<&Value>, count: usize) -> Option<Vec<f32>> {
    let items = value?.as_array()?;
    let numbers: Vec<f32> = items
        .iter()
        .filter_map(|item| number_of(Some(item)))
        .map(|n| n as f32)
        .collect();
    (numbers.len() == items.len() && numbers.len() >= count).then_some(numbers)
}

fn read_inputs(
    header: &Map<String, Value>,
    source: &str,
    json_start: usize,
    json_text: &str,
) -> Result<Vec<IsfInput>, IsfProblem> {
    let mut inputs: Vec<IsfInput> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    for (number, input) in header
        .get("INPUTS")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(input) = input.as_object() else {
            return Err(IsfProblem::nowhere_in_particular(format!(
                "input number {} is not an object of JSON",
                number + 1
            )));
        };
        let Some(name) = input.get("NAME").and_then(Value::as_str) else {
            return Err(IsfProblem::nowhere_in_particular(format!(
                "input number {} has no NAME",
                number + 1
            )));
        };
        let (line, column) = place_of_the_name(name, source, json_start, json_text);
        let wrong = |message: String| IsfProblem::here(line, column, format!("the input {name}: {message}"));
        if !is_a_name_of_glsl(name) {
            return Err(wrong(
                "its name is not one that GLSL can have (letters, digits and underscores, and not a digit first)"
                    .to_owned(),
            ));
        }
        if RESERVED_NAMES.contains(&name)
            || name.starts_with("gl_")
            || name.starts_with("_isf_")
            || name.starts_with("IMG_")
        {
            return Err(wrong("its name is one that the shader has of its own".to_owned()));
        }
        if !names.insert(name.to_owned()) {
            return Err(wrong("there is another input with this name".to_owned()));
        }
        let label = input
            .get("LABEL")
            .and_then(Value::as_str)
            .filter(|label| !label.is_empty())
            .unwrap_or(name)
            .to_owned();
        let type_name = input.get("TYPE").and_then(Value::as_str).unwrap_or("");
        let kind = match type_name {
            "float" => {
                let minimum = number_of(input.get("MIN")).unwrap_or(0.0) as f32;
                let maximum = number_of(input.get("MAX")).unwrap_or(1.0) as f32;
                if minimum > maximum {
                    return Err(wrong(format!("MIN ({minimum}) is more than MAX ({maximum})")));
                }
                let default = number_of(input.get("DEFAULT")).map_or(minimum, |default| default as f32);
                IsfInputKind::Float {
                    minimum,
                    maximum,
                    default,
                }
            }
            "bool" => IsfInputKind::Bool {
                default: number_of(input.get("DEFAULT")).is_some_and(|default| default != 0.0),
            },
            "long" => {
                let labels: Vec<String> = input
                    .get("LABELS")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|label| label.as_str().map_or_else(|| label.to_string(), str::to_owned))
                    .collect();
                let values: Vec<i64> = match input.get("VALUES").and_then(Value::as_array) {
                    Some(values) => values
                        .iter()
                        .filter_map(|value| number_of(Some(value)))
                        .map(|value| value as i64)
                        .collect(),
                    None => (0..labels.len() as i64).collect(),
                };
                if values.is_empty() {
                    return Err(wrong("it is a choice and has no VALUES".to_owned()));
                }
                if !labels.is_empty() && labels.len() != values.len() {
                    return Err(wrong(format!(
                        "it has {} VALUES and {} LABELS, which have to be as many",
                        values.len(),
                        labels.len()
                    )));
                }
                let labels = if labels.is_empty() {
                    values.iter().map(i64::to_string).collect()
                } else {
                    labels
                };
                let default = number_of(input.get("DEFAULT")).map_or(values[0], |default| default as i64);
                let default = if values.contains(&default) { default } else { values[0] };
                IsfInputKind::Long {
                    values,
                    labels,
                    default,
                }
            }
            "color" => {
                let given = numbers_of(input.get("DEFAULT"), 3);
                let default = match given.as_deref() {
                    Some([red, green, blue]) => [*red, *green, *blue, 1.0],
                    Some([red, green, blue, opacity, ..]) => [*red, *green, *blue, *opacity],
                    _ if input.get("DEFAULT").is_none() => [0.0, 0.0, 0.0, 0.0],
                    _ => return Err(wrong("its DEFAULT is not three or four numbers".to_owned())),
                };
                IsfInputKind::Color { default }
            }
            "point2D" => {
                let default = match numbers_of(input.get("DEFAULT"), 2).as_deref() {
                    Some([across, down, ..]) => [*across, *down],
                    _ if input.get("DEFAULT").is_none() => [0.0, 0.0],
                    _ => return Err(wrong("its DEFAULT is not two numbers".to_owned())),
                };
                IsfInputKind::Point2D { default }
            }
            "image" => IsfInputKind::Image,
            "event" => IsfInputKind::Event,
            "audio" | "audioFFT" => IsfInputKind::Audio,
            "" => return Err(wrong("it has no TYPE".to_owned())),
            other => return Err(wrong(format!("its TYPE, {other}, is not one that this program knows"))),
        };
        inputs.push(IsfInput {
            name: name.to_owned(),
            label,
            kind,
        });
    }
    Ok(inputs)
}

fn read_passes(header: &Map<String, Value>) -> Result<Vec<IsfPass>, IsfProblem> {
    let mut passes = Vec::new();
    for (number, pass) in header
        .get("PASSES")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(pass) = pass.as_object() else {
            return Err(IsfProblem::nowhere_in_particular(format!(
                "pass number {} is not an object of JSON",
                number + 1
            )));
        };
        let target = pass.get("TARGET").and_then(Value::as_str).map(str::to_owned);
        if let Some(target) = &target
            && !is_a_name_of_glsl(target)
        {
            return Err(IsfProblem::nowhere_in_particular(format!(
                "the TARGET of pass number {}, {target}, is not a name that GLSL can have",
                number + 1
            )));
        }
        let expression = |key: &str| match pass.get(key) {
            Some(Value::String(text)) => Some(text.clone()),
            Some(Value::Number(number)) => Some(number.to_string()),
            _ => None,
        };
        passes.push(IsfPass {
            target,
            persistent: number_of(pass.get("PERSISTENT")).is_some_and(|flag| flag != 0.0),
            float: number_of(pass.get("FLOAT")).is_some_and(|flag| flag != 0.0),
            width: expression("WIDTH"),
            height: expression("HEIGHT"),
        });
    }
    Ok(passes)
}

fn read_imported(header: &Map<String, Value>) -> Result<Vec<IsfImportedImage>, IsfProblem> {
    let path_of = |value: &Value| {
        value
            .get("PATH")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| value.as_str().map(str::to_owned))
    };
    let mut images = Vec::new();
    match header.get("IMPORTED") {
        Some(Value::Object(map)) => {
            for (name, value) in map {
                let Some(path) = path_of(value) else {
                    return Err(IsfProblem::nowhere_in_particular(format!(
                        "the imported image {name} has no PATH"
                    )));
                };
                images.push(IsfImportedImage {
                    name: name.clone(),
                    path,
                });
            }
        }
        Some(Value::Array(items)) => {
            for item in items {
                let name = item.get("NAME").and_then(Value::as_str);
                let (Some(name), Some(path)) = (name, path_of(item)) else {
                    return Err(IsfProblem::nowhere_in_particular(
                        "an imported image has no NAME or no PATH",
                    ));
                };
                images.push(IsfImportedImage {
                    name: name.to_owned(),
                    path,
                });
            }
        }
        _ => {}
    }
    for image in &images {
        if !is_a_name_of_glsl(&image.name) {
            return Err(IsfProblem::nowhere_in_particular(format!(
                "the imported image {} has a name that GLSL cannot have",
                image.name
            )));
        }
    }
    Ok(images)
}

#[cfg(test)]
mod tests;
