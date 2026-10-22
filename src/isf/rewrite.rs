// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rewriting step: an ISF shader (GLSL as the files in the wild are written, with its own macros and its own idea of where the origin is) becomes the GLSL that naga's front
//! end takes (Vulkan style, version 450). The list of what it has to do is the spike's (`docs/plugin-system-spike-findings.md`, S1), each item found by running into it:
//!
//! * **One uniform block, `vec4 _isf_u[17]`**: the 68 numbers of a call of the engine's effect shaders (`engine/effect_shaders.rs`): the first four are `TIME`, `TIMEDELTA`, `FRAMEINDEX` and
//!   `PASSINDEX`, the next up to 60 are the shader's inputs, one after another as `numbers_of_the_values` makes them, and the last four (the 17th `vec4`) are the engine's: the size of the
//!   picture the effect works on. Each ISF name is a `#define` of the part of the array that it is (nothing is computed by hand, and nothing depends on where a compiler puts a member).
//! * **Images are a `texture2D` and a `sampler` of their own** (naga's front end has no combined `sampler2D` global), bound at `1 + 2k` and `2 + 2k` for the kth image.
//! * **naga's preprocessor has no `##`**, so `IMG_PIXEL(name, ...)` and its relatives are rewritten by a small scanner into calls of generated functions.
//! * **A shader's `main` is renamed and a generated one calls it**, after setting `isf_FragNormCoord`, and gives the result: `gl_FragColor` is a global, and the output is copied last.
//! * **The origin**: ISF is OpenGL (y points up) and the engine is top row first, so the host flips at its boundary: `isf_FragNormCoord`, `gl_FragCoord` and the coordinates that
//!   a picture is read at are flipped, and a shader never knows.
//! * **`textureLod(..., 0.0)` and never `texture`**: shaders sample inside branches all the time, which a call that needs derivatives cannot.
//! * Taken out: `#version`, and a `uniform` of the shader's own that the header already has; `varying` becomes a plain global (nothing gives it a value, so it reads zero).
//!
//! What is refused, with the line: a `sampler2D` anywhere in the code (an image is read by its name, in the macros), and brackets nested more deeply than a compiler's stack is known
//! to take.

use std::collections::HashSet;

use super::reader::{IsfFile, IsfInputKind, IsfProblem};

/// How many `vec4` the uniform block has: 17, which is 68 numbers: the 64 of a call and the engine's four.
pub const VEC4S_OF_THE_UNIFORM_BLOCK: usize = 17;

/// Where in a call's numbers the inputs of the shader begin (the four before them are the shader's own).
pub const NUMBERS_BEFORE_THE_INPUTS: usize = 4;

/// How deeply brackets of every kind may be nested in a shader.
pub const MOST_NESTING: usize = 200;

/// The GLSL that naga is given, and what is known of it.
#[derive(Debug)]
pub struct Rewritten {
    pub glsl: String,
    /// How many lines the generated beginning has: the line of the compiler minus this is the line of the file.
    pub prelude_lines: u32,
    /// The images the shader reads and draws into, in the order they are bound in: the image inputs, then the targets of the passes, then the imported images.
    pub image_names: Vec<String>,
    /// What was changed that a person might want to know of.
    pub notes: Vec<String>,
}

fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// `text` with every character of a comment (`//` to the end of the line, and `/* ... */`) replaced by a space (a line end stays), so that what is left is where it was.
pub(super) fn without_comments(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut result = characters.clone();
    let mut at = 0;
    while at < characters.len() {
        if characters[at] == '/' && characters.get(at + 1) == Some(&'/') {
            while at < characters.len() && characters[at] != '\n' {
                result[at] = ' ';
                at += 1;
            }
        } else if characters[at] == '/' && characters.get(at + 1) == Some(&'*') {
            let mut ended = false;
            while at < characters.len() && !ended {
                ended = characters[at] == '*' && characters.get(at + 1) == Some(&'/');
                for _ in 0..if ended { 2 } else { 1 } {
                    if at < characters.len() {
                        if characters[at] != '\n' {
                            result[at] = ' ';
                        }
                        at += 1;
                    }
                }
            }
        } else {
            at += 1;
        }
    }
    result.into_iter().collect()
}

/// The line and column (from 1) of the character `index` of `text`.
fn place_of(text: &[char], index: usize) -> (u32, u32) {
    let before = &text[..index];
    let line = before.iter().filter(|character| **character == '\n').count() as u32 + 1;
    let column = before.iter().rev().take_while(|character| **character != '\n').count() as u32 + 1;
    (line, column)
}

/// Whether the code has a word `word` (not a part of a longer one), and where its first is.
fn first_use_of(code: &str, word: &str) -> Option<(u32, u32)> {
    let characters: Vec<char> = code.chars().collect();
    let target: Vec<char> = word.chars().collect();
    (0..characters.len()).find_map(|at| {
        (characters[at..].starts_with(&target)
            && (at == 0 || !is_identifier_character(characters[at - 1]))
            && characters
                .get(at + target.len())
                .is_none_or(|next| !is_identifier_character(*next)))
        .then(|| place_of(&characters, at))
    })
}

/// How deeply brackets are nested in the code, and where it first goes deeper than `limit`.
fn first_place_nested_deeper_than(code: &str, limit: usize) -> Option<(u32, u32)> {
    let characters: Vec<char> = code.chars().collect();
    let mut depth = 0usize;
    for (at, character) in characters.iter().enumerate() {
        match character {
            '(' | '[' | '{' => {
                depth += 1;
                if depth > limit {
                    return Some(place_of(&characters, at));
                }
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// `IMG_NORM_PIXEL(inputImage, uv)` becomes `_isf_IMG_NORM_PIXEL_inputImage(uv)`. Anything else about the call is left as its author wrote it. Gives the new text and how many
/// calls were rewritten.
fn rewrite_image_calls(body: &str, image_names: &[String]) -> (String, usize) {
    const CALLS: [&str; 7] = [
        "IMG_PIXEL",
        "IMG_NORM_PIXEL",
        "IMG_THIS_PIXEL",
        "IMG_THIS_NORM_PIXEL",
        "IMG_SIZE",
        "texture2D",
        "texture2DRect",
    ];
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len() + 256);
    let mut rewritten = 0;
    let mut i = 0;
    let skip_space = |mut k: usize| {
        while k < chars.len() && chars[k].is_whitespace() {
            k += 1;
        }
        k
    };
    while i < chars.len() {
        if is_identifier_character(chars[i]) && (i == 0 || !is_identifier_character(chars[i - 1])) {
            let mut j = i;
            while j < chars.len() && is_identifier_character(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            if CALLS.contains(&word.as_str()) {
                let open = skip_space(j);
                if chars.get(open) == Some(&'(') {
                    let name_start = skip_space(open + 1);
                    let mut name_end = name_start;
                    while name_end < chars.len() && is_identifier_character(chars[name_end]) {
                        name_end += 1;
                    }
                    let image: String = chars[name_start..name_end].iter().collect();
                    let after = skip_space(name_end);
                    if image_names.contains(&image) && matches!(chars.get(after), Some(',' | ')')) {
                        out.push_str(&format!("_isf_{word}_{image}("));
                        i = if chars[after] == ',' { after + 1 } else { after };
                        rewritten += 1;
                        continue;
                    }
                }
            }
            out.push_str(&word);
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, rewritten)
}

/// `text` with every whole word `word` replaced by `with`.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let target: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if (i == 0 || !is_identifier_character(chars[i - 1]))
            && chars[i..].starts_with(&target)
            && chars
                .get(i + target.len())
                .is_none_or(|next| !is_identifier_character(*next))
        {
            out.push_str(with);
            i += target.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The expression of number `number` of the call, in the uniform block.
fn number_expression(number: usize) -> String {
    format!("_isf_u[{}].{}", number / 4, ['x', 'y', 'z', 'w'][number % 4])
}

/// The names of what a `uniform` or `varying` line declares (after its type, up to the semicolon, separated by commas, each without an array size or an initial value).
fn names_declared_by(line: &str) -> Vec<String> {
    let declaration = line.split(';').next().unwrap_or("");
    let mut words = declaration.split_whitespace();
    let _qualifier = words.next();
    // (Precision and the type come before the names.)
    let rest: Vec<&str> = words.collect();
    let after_type = rest
        .iter()
        .position(|word| !matches!(*word, "highp" | "mediump" | "lowp"))
        .map_or(0, |position| position + 1);
    rest[after_type.min(rest.len())..]
        .join(" ")
        .split(',')
        .filter_map(|part| {
            let name: String = part
                .trim()
                .chars()
                .take_while(|character| is_identifier_character(*character))
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// Rewrites the shader of `file`.
pub fn rewrite(file: &IsfFile) -> Result<Rewritten, IsfProblem> {
    let code = without_comments(&file.body);
    if let Some((line, column)) = first_use_of(&code, "sampler2D") {
        return Err(IsfProblem::here(
            line,
            column,
            "sampler2D cannot be used here: an image is read by its name, with IMG_NORM_PIXEL(name, coordinates) and the other macros of ISF",
        ));
    }
    if let Some((line, column)) = first_place_nested_deeper_than(&code, MOST_NESTING) {
        return Err(IsfProblem::here(
            line,
            column,
            format!("brackets are nested more than {MOST_NESTING} deep, which no compiler can be trusted with"),
        ));
    }

    // The images, in the order they are bound in.
    let mut image_names: Vec<String> = file
        .inputs
        .iter()
        .filter(|input| input.kind == IsfInputKind::Image)
        .map(|input| input.name.clone())
        .collect();
    let others = file
        .passes
        .iter()
        .filter_map(|pass| pass.target.clone())
        .chain(file.imported.iter().map(|image| image.name.clone()));
    for name in others {
        if !image_names.contains(&name) {
            image_names.push(name);
        }
    }

    let mut notes = Vec::new();
    let mut prelude = String::new();
    prelude.push_str("#version 450\n");
    prelude.push_str(&format!(
        "layout(set = 0, binding = 0) uniform IsfUniforms {{ vec4 _isf_u[{VEC4S_OF_THE_UNIFORM_BLOCK}]; }};\n"
    ));
    prelude.push_str("#define TIME _isf_u[0].x\n#define TIMEDELTA _isf_u[0].y\n#define FRAMEINDEX int(_isf_u[0].z)\n#define PASSINDEX int(_isf_u[0].w)\n");
    prelude.push_str(&format!(
        "#define RENDERSIZE _isf_u[{}].xy\n#define DATE vec4(0.0)\n",
        VEC4S_OF_THE_UNIFORM_BLOCK - 1
    ));
    let mut number = NUMBERS_BEFORE_THE_INPUTS;
    let mut names_of_the_shader: HashSet<&str> = HashSet::new();
    for input in &file.inputs {
        names_of_the_shader.insert(input.name.as_str());
        let n = |offset: usize| number_expression(number + offset);
        let expression = match &input.kind {
            IsfInputKind::Float { .. } => Some(n(0)),
            IsfInputKind::Long { .. } => Some(format!("int({})", n(0))),
            IsfInputKind::Bool { .. } | IsfInputKind::Event => Some(format!("({} != 0.0)", n(0))),
            IsfInputKind::Color { .. } => Some(format!("vec4({}, {}, {}, {})", n(0), n(1), n(2), n(3))),
            // (As the file says it, whatever the size of the picture: a point is a fraction of the picture in 79 of the 90 points of the Vidvox shaders (`MAX` [1, 1]), and a number of
            // cells or squares in the rest, never a place in pixels. The first design scaled it by the height of the picture over 1080 and was wrong: the corpus found it, phase 6.7.)
            IsfInputKind::Point2D { .. } => Some(format!("vec2({}, {})", n(0), n(1))),
            IsfInputKind::Image => None,
            IsfInputKind::Audio => {
                notes.push(format!(
                    "the input {} is sound, which is not given to shaders yet",
                    input.name
                ));
                None
            }
        };
        if let Some(expression) = expression {
            prelude.push_str(&format!("#define {} ({expression})\n", input.name));
        }
        number += input.kind.numbers();
    }
    for (place, name) in image_names.iter().enumerate() {
        names_of_the_shader.insert(name.as_str());
        prelude.push_str(&format!(
            "layout(set = 0, binding = {}) uniform texture2D {name}_tex;\nlayout(set = 0, binding = {}) uniform sampler {name}_samp;\n",
            1 + 2 * place,
            2 + 2 * place
        ));
    }
    prelude.push_str(
        "vec2 isf_FragNormCoord;\n#define vv_FragNormCoord isf_FragNormCoord\nvec4 _isf_color;\n#define gl_FragColor _isf_color\n\
         vec4 _isf_real_fragcoord() { return gl_FragCoord; }\nvec4 _isf_fragcoord;\n#define gl_FragCoord _isf_fragcoord\n",
    );
    for name in &image_names {
        prelude.push_str(&format!(
            "vec4 _isf_IMG_NORM_PIXEL_{n}(vec2 c) {{ return textureLod(sampler2D({n}_tex, {n}_samp), vec2(c.x, 1.0 - c.y), 0.0); }}\n\
             vec2 _isf_IMG_SIZE_{n}() {{ return vec2(textureSize(sampler2D({n}_tex, {n}_samp), 0)); }}\n\
             vec4 _isf_IMG_PIXEL_{n}(vec2 c) {{ return _isf_IMG_NORM_PIXEL_{n}(c / _isf_IMG_SIZE_{n}()); }}\n\
             vec4 _isf_IMG_THIS_PIXEL_{n}() {{ return _isf_IMG_NORM_PIXEL_{n}(isf_FragNormCoord); }}\n\
             vec4 _isf_IMG_THIS_NORM_PIXEL_{n}() {{ return _isf_IMG_NORM_PIXEL_{n}(isf_FragNormCoord); }}\n\
             vec4 _isf_texture2D_{n}(vec2 c) {{ return _isf_IMG_NORM_PIXEL_{n}(c); }}\n\
             vec4 _isf_texture2DRect_{n}(vec2 c) {{ return _isf_IMG_PIXEL_{n}(c); }}\n",
            n = name
        ));
    }
    let prelude_lines = prelude.matches('\n').count() as u32;

    let (body, count) = rewrite_image_calls(&file.body, &image_names);
    if count > 0 {
        notes.push(format!("{count} calls of the macros of images were rewritten"));
    }
    // The lines that Vulkan-style GLSL cannot have (each is still a line: the lines of the file are the lines of what is compiled).
    let mut lines: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#version") {
            notes.push("a #version line was taken out".to_owned());
            lines.push(String::new());
        } else if trimmed.starts_with("varying ") || trimmed.starts_with("uniform ") {
            let declared = names_declared_by(trimmed);
            if declared.iter().any(|name| names_of_the_shader.contains(name.as_str())) {
                notes.push(format!(
                    "`{}` was taken out: the header already has it",
                    trimmed.split(';').next().unwrap_or("")
                ));
                lines.push(String::new());
            } else {
                notes.push(format!(
                    "`{}` was made a plain global",
                    trimmed.split(';').next().unwrap_or("")
                ));
                lines.push(line.replacen("varying ", "", 1).replacen("uniform ", "", 1));
            }
        } else {
            lines.push(line.to_owned());
        }
    }
    let body = replace_word(&lines.join("\n"), "main", "_isf_user_main");
    let mut glsl = prelude;
    glsl.push_str(&body);
    glsl.push_str(&format!(
        "\nlayout(location = 0) out vec4 _isf_out;\nvoid main() {{\n    vec4 real = _isf_real_fragcoord();\n    _isf_fragcoord = vec4(real.x, _isf_u[{last}].y - real.y, real.z, real.w);\n    isf_FragNormCoord = vec2(real.x / _isf_u[{last}].x, 1.0 - real.y / _isf_u[{last}].y);\n    _isf_color = vec4(0.0);\n    _isf_user_main();\n    _isf_out = _isf_color;\n}}\n",
        last = VEC4S_OF_THE_UNIFORM_BLOCK - 1
    ));
    Ok(Rewritten {
        glsl,
        prelude_lines,
        image_names,
        notes,
    })
}

#[cfg(test)]
mod tests;
