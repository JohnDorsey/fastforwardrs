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
//! * **A global is given its initialiser by a generated function that runs before the shader's `main`** (`_isf_set_up_globals`), and not where it is declared: **naga drops the initialiser of a
//!   global that is not a constant, without a word**, so `float ratio = RENDERSIZE.x / RENDERSIZE.y;` made `ratio` zero. (Found by the corpus of the Vidvox shaders, phase 6.6: Circle Crop was black
//!   at every progress.) A `const`, an array and a declaration inside a condition of the preprocessor are left alone.
//! * **A float input can be read from a mask** ([`rewrite_with_driven_inputs`], `docs/shader-masks-design.md`): the input's name stands for a global, `_isf_driven_NAME`, that a generated
//!   function (`_isf_set_up_driven`) fills in **once per pixel, before anything else of the shader runs**, from one read of the mask picture (one more image of the shader, bound after the
//!   others). The shader's text is not changed and its uses of the input (in a loop, in an initialiser) are reads of a global, not of the picture.
//! * **The mask can be laid in space instead** ([`Space::World`], `docs/shader-masks-design.md`, section 12): for an input whose header says `"WORLD_SPACE": true`, in a shader that sets `isf_world_position` to
//!   the point of space it is working at, the input's name stands for a call of a generated function, `_isf_world_NAME()`, that finds the place of a plane, a sphere or a cylinder that is nearest to that
//!   point and reads the mask there: **at every use**, since the point changes as the shader goes on. The place of the shape is nine numbers of the call ([`NUMBERS_OF_A_WORLD_PLACEMENT`]).
//!
//! What is refused, with the line: a `sampler2D` anywhere in the code (an image is read by its name, in the macros), and brackets nested more deeply than a compiler's stack is known
//! to take.

use std::collections::HashSet;

use super::reader::{IsfFile, IsfInputKind, IsfProblem};

/// How many `vec4` the uniform block has: 17, which is 68 numbers: the 64 of a call and the engine's four.
pub const VEC4S_OF_THE_UNIFORM_BLOCK: usize = 17;

/// The name of the global that a shader sets to the point of space that it is working at, for the inputs that are read from a mask laid in space (`WORLD_SPACE` in the header). It is
/// declared (as a `vec3`) for a shader that uses the name; a shader that does not has nothing of the kind in its text.
pub const WORLD_POSITION: &str = "isf_world_position";

/// What the name of the image of a mask begins with (the number of the mask follows): the image of the mask `n` of a shader is called `_isf_mask_n`. (A name that begins with `_isf_`
/// is never a shader's own.)
pub const MASK_IMAGE_PREFIX: &str = "_isf_mask_";

/// The name of the image that the mask with this number is read as.
pub fn image_name_of_the_mask(number: usize) -> String {
    format!("{MASK_IMAGE_PREFIX}{number}")
}

/// Which part of the colour of a mask gives the value of a driven input. All of them are worked out from the **codes** of the colour, the numbers a shader reads, and not from light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaskChannel {
    Red,
    Green,
    Blue,
    /// The luma of the codes, with the weights of Rec. 709: 0.2126 red, 0.7152 green and 0.0722 blue.
    Brightness,
}

impl MaskChannel {
    pub const ALL: [Self; 4] = [Self::Red, Self::Green, Self::Blue, Self::Brightness];

    /// The GLSL that works out this channel from the (premultiplied) colour `colour`, a `vec4`.
    fn expression_for(self, colour: &str) -> String {
        match self {
            Self::Red => format!("{colour}.r"),
            Self::Green => format!("{colour}.g"),
            Self::Blue => format!("{colour}.b"),
            Self::Brightness => format!("dot({colour}.rgb, vec3(0.2126, 0.7152, 0.0722))"),
        }
    }
}

/// How many points a graph has at most (see [`Mapping::Graph`]).
pub const MOST_POINTS_OF_A_GRAPH: usize = 8;

/// How many numbers of a call a graph takes: the places of its points, then their heights.
pub const NUMBERS_OF_A_GRAPH: usize = 2 * MOST_POINTS_OF_A_GRAPH;

/// How many numbers a call can have, the four of the shader's own among them: the uniform block, less the vec4 that has the size of the picture.
pub const MOST_NUMBERS_OF_A_CALL: usize = 4 * (VEC4S_OF_THE_UNIFORM_BLOCK - 1);

/// How the unit of the channel of a mask (0 where the channel is nothing, 1 where it is all) becomes the place of the value in the range of the input (0 is the least of it, 1 the most).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mapping {
    /// The place is the unit. (Needs nothing in the call.)
    Straight,
    /// The place is the height of a **graph of line segments** at the unit. The graph is **data**, not text of the shader: its [`MOST_POINTS_OF_A_GRAPH`] points are [`NUMBERS_OF_A_GRAPH`]
    /// numbers of the call, **after the numbers of all the inputs** and, when there are several graphs, one after another in the order of the inputs of the file: first the places of the
    /// points (0 to 1, going up, the first 0 and the last 1), then their heights (0 to 1). A graph with fewer points repeats its last point to fill the rest, so that dragging a point
    /// compiles nothing.
    Graph,
}

/// The shape that a mask laid **in space** is wrapped on: the value of an input at a point of space is read from the mask at the place of the shape that is **nearest to the point**.
///
/// Each shape has its own frame: the origin is the middle of the shape, **y is up** the picture, and **z is the front** (the way the picture faces; it is read the right way round from the
/// front). A point of space is first moved into that frame (see [`NUMBERS_OF_A_WORLD_PLACEMENT`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldShape {
    /// A flat rectangle in the plane of x and y, `width` across (x) and `height` up (y): the picture is laid on it, and the place nearest to a point is where it is dropped straight onto
    /// the plane (along z). A point whose nearest place on the plane is outside the rectangle is not covered by the mask.
    Plane,
    /// A sphere: the picture is wrapped round it as a map of the world is (the middle of the picture faces z, the top is the pole at +y, the left and right edges meet behind). The place
    /// nearest to a point is on the line from the middle through the point, so **the size of the sphere does not matter**, and a point at the middle itself is not covered.
    Sphere,
    /// A cylinder round the y axis: the picture is wrapped once round it (the middle of the picture faces z, the left and right edges meet behind), `height` tall. The place nearest to a
    /// point is where the line from the axis through the point meets it, so **the size round does not matter**, a point on the axis is not covered, and nor is one above or below the
    /// cylinder's height.
    Cylinder,
}

impl WorldShape {
    pub const ALL: [Self; 3] = [Self::Plane, Self::Sphere, Self::Cylinder];

    /// The word that the key of a shader that is made for this shape has in it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Sphere => "sphere",
            Self::Cylinder => "cylinder",
        }
    }

    /// The GLSL that works out `_isf_at` (the place in the picture of the mask, 0 to 1 across and up) and `_isf_inside` (1 if the place is on the shape, 0 if it is not) from `_isf_d`, the
    /// point in the frame of the shape, and `_isf_size` (the width and the height). (Every name is one that no input of a shader can have: an input's name is a `#define`.)
    fn place_in_the_picture(self) -> &'static str {
        match self {
            Self::Plane => {
                "    vec2 _isf_at = vec2(_isf_d.x / _isf_size.x + 0.5, _isf_d.y / _isf_size.y + 0.5);\n    float _isf_inside = (_isf_at.x >= 0.0 && _isf_at.x <= 1.0 && _isf_at.y >= 0.0 && _isf_at.y <= 1.0) ? 1.0 : 0.0;\n"
            }
            Self::Sphere => {
                "    float _isf_away = length(_isf_d);\n    vec2 _isf_at = vec2(0.5, 0.5);\n    float _isf_inside = 0.0;\n    if (_isf_away > 1.0e-6) {\n        _isf_at = vec2(atan(_isf_d.x, _isf_d.z) / 6.28318530718 + 0.5, asin(clamp(_isf_d.y / _isf_away, -1.0, 1.0)) / 3.14159265359 + 0.5);\n        _isf_inside = 1.0;\n    }\n"
            }
            Self::Cylinder => {
                "    vec2 _isf_at = vec2(0.5, _isf_d.y / _isf_size.y + 0.5);\n    float _isf_inside = 0.0;\n    if (length(_isf_d.xz) > 1.0e-6 && _isf_at.y >= 0.0 && _isf_at.y <= 1.0) {\n        _isf_at.x = atan(_isf_d.x, _isf_d.z) / 6.28318530718 + 0.5;\n        _isf_inside = 1.0;\n    }\n"
            }
        }
    }
}

/// Where a mask is laid: over the picture, or in space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    /// The mask is the picture's shape: the value of an input at a pixel is read from the mask at that pixel, once, before the shader runs.
    Screen,
    /// The mask is wrapped on a shape in space: the value of an input is read, **every time the shader uses it**, from the mask at the place of the shape nearest to
    /// `isf_world_position`, the point of space that the shader says it is working at (and so the input must say `"WORLD_SPACE": true` in the header, and the shader must set
    /// `isf_world_position` before it uses the input: see `IsfInput::world_space` in the reader).
    World(WorldShape),
}

/// How many numbers of a call a mask laid in space takes: **where the shape is** (3: the place of its middle), **how it is turned** (4: the quaternion `x, y, z, w` that turns a point of space
/// into the frame of the shape; the caller makes it from angles, so that dragging an angle compiles nothing) and **how big it is** (2: the width and the height; a sphere has no use for them
/// and a cylinder only for the height). They come after the points of the graphs (and, when several inputs are read in space, one placement after another in the order of the inputs of
/// the file).
pub const NUMBERS_OF_A_WORLD_PLACEMENT: usize = 9;

/// A float input of a shader whose value is read from a **mask**: a picture that the engine draws and the shader is given as one more image. (The slider's own number is
/// what the input is **where the mask has nothing**; see `rewrite_with_driven_inputs`.)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DrivenInput {
    /// The name of the input in the shader.
    pub name: String,
    /// Which mask: the masks of a shader are numbered from 0, and inputs may share one.
    pub mask: usize,
    pub channel: MaskChannel,
    pub mapping: Mapping,
    /// Where the mask is laid: over the picture, read at each pixel, or in space, read at each point of space the shader works at.
    pub space: Space,
}

/// A GLSL number for `value` (always with a point or an exponent, so that it is a float).
fn glsl_float(value: f32) -> String {
    format!("{value:?}")
}

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

/// A variable at the top level of the shader's code that is given a value where it is declared.
struct Initialised {
    name: String,
    /// Where in the code the name ends: what is taken off the declaration begins here (so that nothing but `float name;` is left).
    name_end: usize,
    /// Where the initialiser is: after the `=`, up to the comma or the semicolon.
    initialiser: std::ops::Range<usize>,
}

/// Words that make a statement at the top level something other than a type and the names of variables of it: such a statement is left as it is.
const NOT_A_PLAIN_VARIABLE: [&str; 15] = [
    "const",
    "uniform",
    "varying",
    "attribute",
    "in",
    "out",
    "inout",
    "layout",
    "struct",
    "precision",
    "typedef",
    "buffer",
    "shared",
    "flat",
    "smooth",
];

fn skip_whitespace(code: &[char], mut at: usize) -> usize {
    while at < code.len() && code[at].is_whitespace() {
        at += 1;
    }
    at
}

/// Where the line of a directive of the preprocessor that begins at `at` ends (a line that ends with a backslash goes on), if `at` is the `#` that begins one: the first character
/// of its line that is not a space.
fn end_of_the_directive_at(code: &[char], at: usize) -> Option<usize> {
    if code[at] != '#'
        || !code[..at]
            .iter()
            .rev()
            .take_while(|character| **character != '\n')
            .all(|character| *character == ' ' || *character == '\t' || *character == '\r')
    {
        return None;
    }
    let mut end = at;
    loop {
        while end < code.len() && code[end] != '\n' {
            end += 1;
        }
        if end < code.len() && end > 0 && code[end - 1] == '\\' {
            end += 1;
            continue;
        }
        return Some(end);
    }
}

/// The variables that a statement of the top level, `code[start..semicolon]` (and the semicolon), declares with an initialiser, if it is a plain declaration of variables (a type and the names
/// of some, each with an initialiser or none); and nothing if it is anything else, or if one of the names is an array (an array whose size comes from its initialiser cannot lose it).
fn variables_declared_with_initialisers(code: &[char], start: usize, semicolon: usize) -> Option<Vec<Initialised>> {
    let is_the_start_of_a_word = |character: char| character.is_ascii_alphabetic() || character == '_';
    // The words before the first thing that is not one: qualifiers, the type, and the first name.
    let mut words: Vec<(String, usize)> = Vec::new();
    let mut at = skip_whitespace(code, start);
    loop {
        let word_start = at;
        while at < semicolon && is_identifier_character(code[at]) {
            at += 1;
        }
        if at == word_start || !is_the_start_of_a_word(code[word_start]) {
            return None;
        }
        words.push((code[word_start..at].iter().collect(), at));
        let after = skip_whitespace(code, at);
        if after > at && after < semicolon && is_the_start_of_a_word(code[after]) {
            at = after;
        } else {
            break;
        }
    }
    if words.len() < 2
        || words[..words.len() - 1]
            .iter()
            .any(|(word, _)| NOT_A_PLAIN_VARIABLE.contains(&word.as_str()))
    {
        return None;
    }
    let (first_name, first_name_end) = words.last().cloned()?;
    let mut found = Vec::new();
    let (mut name, mut name_end) = (first_name, first_name_end);
    let mut at = skip_whitespace(code, first_name_end);
    loop {
        match code.get(at) {
            Some('=') if code.get(at + 1) != Some(&'=') => {
                let initialiser_start = at + 1;
                let mut depth = 0usize;
                let mut end = initialiser_start;
                while end < semicolon {
                    match code[end] {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => depth = depth.saturating_sub(1),
                        ',' if depth == 0 => break,
                        _ => {}
                    }
                    end += 1;
                }
                found.push(Initialised {
                    name: name.clone(),
                    name_end,
                    initialiser: initialiser_start..end,
                });
                at = end;
            }
            Some('[' | '(') => return None,
            _ => {}
        }
        if at >= semicolon {
            break;
        }
        if code[at] != ',' {
            return None;
        }
        at = skip_whitespace(code, at + 1);
        let name_start = at;
        while at < semicolon && is_identifier_character(code[at]) {
            at += 1;
        }
        if at == name_start {
            return None;
        }
        name = code[name_start..at].iter().collect();
        name_end = at;
        at = skip_whitespace(code, at);
    }
    (!found.is_empty()).then_some(found)
}

/// Every variable at the top level of `code` that is declared with an initialiser, in the order they are in the code: not those inside a function or a structure, and not those inside a
/// condition of the preprocessor (`#if`, `#ifdef`, `#ifndef`), which this scanner does not follow.
fn globals_with_initialisers(code: &[char]) -> Vec<Initialised> {
    let mut found = Vec::new();
    let (mut braces, mut brackets, mut conditions) = (0usize, 0usize, 0usize);
    let mut statement_start = 0;
    let mut at = 0;
    while at < code.len() {
        if let Some(end) = end_of_the_directive_at(code, at) {
            let directive: String = code[at + 1..end].iter().collect();
            match directive
                .trim_start()
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .collect::<String>()
                .as_str()
            {
                "if" | "ifdef" | "ifndef" => conditions += 1,
                "endif" => conditions = conditions.saturating_sub(1),
                _ => {}
            }
            at = end;
            statement_start = at;
            continue;
        }
        match code[at] {
            '{' => braces += 1,
            '}' => {
                braces = braces.saturating_sub(1);
                if braces == 0 {
                    statement_start = at + 1;
                }
            }
            '(' | '[' => brackets += 1,
            ')' | ']' => brackets = brackets.saturating_sub(1),
            ';' if braces == 0 && brackets == 0 => {
                if conditions == 0
                    && let Some(mut these) = variables_declared_with_initialisers(code, statement_start, at)
                {
                    found.append(&mut these);
                }
                statement_start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    found
}

/// `body` with the initialiser of every variable at its top level taken off the declaration (`float ratio = RENDERSIZE.x / RENDERSIZE.y;` becomes `float ratio;`), and what each was, as the
/// name and the text of the initialiser, in order. What was taken off leaves its line ends, so that every line of the file is the line it was.
fn lift_the_initialisers_of_globals(body: &str) -> (String, Vec<(String, String)>) {
    let original: Vec<char> = body.chars().collect();
    // (The same characters, with the comments blanked: where the declarations are is found in these, and what the initialisers say is read from them.)
    let code: Vec<char> = without_comments(body).chars().collect();
    let found = globals_with_initialisers(&code);
    if found.is_empty() {
        return (body.to_owned(), Vec::new());
    }
    let mut lifted = String::with_capacity(body.len());
    let mut assignments = Vec::with_capacity(found.len());
    let mut copied = 0;
    for variable in &found {
        lifted.extend(&original[copied..variable.name_end]);
        lifted.extend(
            original[variable.name_end..variable.initialiser.end]
                .iter()
                .filter(|character| **character == '\n'),
        );
        copied = variable.initialiser.end;
        let text: String = code[variable.initialiser.clone()].iter().collect();
        assignments.push((variable.name.clone(), text.trim().to_owned()));
    }
    lifted.extend(&original[copied..]);
    (lifted, assignments)
}

/// A float input that is read from a mask, with what the rewriting step needs of it.
struct InputReadFromAMask<'file> {
    name: &'file str,
    minimum: f32,
    maximum: f32,
    /// Which of the numbers of a call is the input's own: the value it has where the mask has nothing.
    own_number: usize,
    /// Which of the numbers of a call the points of its graph begin at, if it has one.
    graph_at: Option<usize>,
    /// Which of the numbers of a call the placement of its mask begins at, if the mask is laid in space.
    world_at: Option<usize>,
    read_from: &'file DrivenInput,
}

/// The GLSL function `_isf_graph_NAME(t)`: the height, 0 to 1, of the graph at `t`, 0 to 1, whose points are the numbers of the call that begin at `first_number` (the places of the points,
/// then their heights). One segment for each pair of points, each used for the places between its points; a pair that is at one place has no places between and is left alone.
fn function_of_a_graph(name: &str, first_number: usize) -> String {
    let place = |point: usize| number_expression(first_number + point);
    let height = |point: usize| number_expression(first_number + MOST_POINTS_OF_A_GRAPH + point);
    let mut text = format!(
        "float _isf_graph_{name}(float t) {{
    float height = {};
",
        height(0)
    );
    for point in 0..MOST_POINTS_OF_A_GRAPH - 1 {
        let (from, to) = (place(point), place(point + 1));
        text.push_str(&format!(
            "    if (t >= {from} && t <= {to} && {to} > {from}) {{ height = mix({}, {}, (t - {from}) / ({to} - {from})); }}
",
            height(point),
            height(point + 1),
        ));
    }
    text.push_str(
        "    return height;
}
",
    );
    text
}

/// The place in the range of the input that the channel of the colour `colour` (a `vec4`, premultiplied) of a mask is: the unit of the channel with the opacity divided out (0 where nothing
/// covers), made into a height by the graph of the input if it has one.
fn place_in_the_range_of(input: &InputReadFromAMask<'_>, colour: &str) -> String {
    let channel = input.read_from.channel.expression_for(colour);
    let unit = format!("({colour}.a > 0.0 ? clamp({channel} / {colour}.a, 0.0, 1.0) : 0.0)");
    match input.graph_at {
        Some(_) => format!("_isf_graph_{}({unit})", input.name),
        None => unit,
    }
}

/// The globals that the inputs read from masks over the picture are, and the function that fills them in: each mask is read once (at the pixel being made), and each input is worked out from
/// that one read. `inputs` are the inputs in the order of the file (those read from masks laid in space are not among the globals: see [`functions_of_the_inputs_read_in_space`]).
fn function_that_sets_up_the_driven_inputs(inputs: &[InputReadFromAMask<'_>]) -> String {
    let over_the_picture: Vec<&InputReadFromAMask<'_>> = inputs
        .iter()
        .filter(|input| input.read_from.space == Space::Screen)
        .collect();
    let mut masks: Vec<usize> = over_the_picture.iter().map(|input| input.read_from.mask).collect();
    masks.sort_unstable();
    masks.dedup();
    let mut text = String::new();
    for input in &over_the_picture {
        text.push_str(&format!(
            "float _isf_driven_{};
",
            input.name
        ));
    }
    text.push_str(
        "void _isf_set_up_driven() {
",
    );
    for mask in masks {
        text.push_str(&format!(
            "    vec4 _isf_mask_{mask}_here = _isf_IMG_NORM_PIXEL__isf_mask_{mask}(isf_FragNormCoord);
    float _isf_mask_{mask}_cover = _isf_mask_{mask}_here.a;
"
        ));
    }
    for input in over_the_picture {
        let (name, mask) = (input.name, input.read_from.mask);
        let channel = input
            .read_from
            .channel
            .expression_for(&format!("_isf_mask_{mask}_here"));
        // (Where the place in the range is the unit, or the height of the graph at the unit.)
        let place = match input.graph_at {
            Some(_) => format!("_isf_graph_{name}(_isf_unit_of_the_channel_{name})"),
            None => format!("_isf_unit_of_the_channel_{name}"),
        };
        text.push_str(&format!(
            "    float _isf_unit_of_the_channel_{name} = (_isf_mask_{mask}_cover > 0.0) ? clamp({channel} / _isf_mask_{mask}_cover, 0.0, 1.0) : 0.0;
    _isf_driven_{name} = mix({own}, {minimum} + {range} * {place}, _isf_mask_{mask}_cover);
",
            own = number_expression(input.own_number),
            minimum = glsl_float(input.minimum),
            range = glsl_float(input.maximum - input.minimum),
        ));
    }
    text.push_str(
        "}
",
    );
    text
}

/// The function `_isf_world_NAME()` of each input that is read from a mask laid in space: the value of the input at the point `isf_world_position`, which the shader sets (a function and not
/// a global, because the point changes as the shader goes on: the mask is read at **every use** of the input). The point is moved into the frame of the shape (the numbers of its placement,
/// see [`NUMBERS_OF_A_WORLD_PLACEMENT`]), the nearest place of the shape is found ([`WorldShape::place_in_the_picture`]) and the mask is read there; where the point has no place on the
/// shape the mask has nothing. The value is made from what is read as for a mask over the picture (the input's own number where nothing covers, the range spread over the channel
/// between).
fn functions_of_the_inputs_read_in_space(inputs: &[InputReadFromAMask<'_>]) -> String {
    let mut text = String::new();
    for input in inputs {
        let Space::World(shape) = input.read_from.space else {
            continue;
        };
        let (name, mask) = (input.name, input.read_from.mask);
        let first = input
            .world_at
            .expect("an input read in space has the place of its placement");
        let number = |offset: usize| number_expression(first + offset);
        let place = place_in_the_range_of(input, "_isf_here");
        text.push_str(&format!(
            "float _isf_world_{name}() {{
    vec3 _isf_middle = vec3({}, {}, {});
    vec4 _isf_turn = vec4({}, {}, {}, {});
    vec2 _isf_size = vec2({}, {});
    vec3 _isf_d = isf_world_position - _isf_middle;
    _isf_d = _isf_d + 2.0 * cross(_isf_turn.xyz, cross(_isf_turn.xyz, _isf_d) + _isf_turn.w * _isf_d);
{}    vec4 _isf_here = _isf_IMG_NORM_PIXEL__isf_mask_{mask}(_isf_at);
    float _isf_cover = _isf_here.a * _isf_inside;
    return mix({own}, {minimum} + {range} * {place}, _isf_cover);
}}
",
            number(0),
            number(1),
            number(2),
            number(3),
            number(4),
            number(5),
            number(6),
            number(7),
            number(8),
            shape.place_in_the_picture(),
            own = number_expression(input.own_number),
            minimum = glsl_float(input.minimum),
            range = glsl_float(input.maximum - input.minimum),
        ));
    }
    text
}

/// The functions `_isf_graph_NAME` of the inputs that are mapped by a graph.
fn functions_of_the_graphs(inputs: &[InputReadFromAMask<'_>]) -> String {
    inputs
        .iter()
        .filter_map(|input| Some(function_of_a_graph(input.name, input.graph_at?)))
        .collect()
}

/// What is wrong with reading the inputs `driven` from masks, if anything is: an input that is not there or is not a float, one that is read twice, one that sets the size of a buffer (a
/// size is worked out once for the whole picture), a range too wide for a float, and masks that skip a number.
fn what_is_wrong_with_the_driven_inputs(file: &IsfFile, driven: &[DrivenInput]) -> Option<IsfProblem> {
    let says = |message: String| Some(IsfProblem::nowhere_in_particular(message));
    let that_set_a_size = super::structure::inputs_that_set_a_size(file);
    for (place, one) in driven.iter().enumerate() {
        let name = &one.name;
        let Some(input) = file.inputs.iter().find(|input| input.name == *name) else {
            return says(format!(
                "the shader has no input called {name}, so there is nothing to read from a mask"
            ));
        };
        let IsfInputKind::Float { minimum, maximum, .. } = input.kind else {
            return says(format!(
                "the input {name} is not a number (a float), so it cannot be read from a mask: only a number that can vary smoothly across a picture can"
            ));
        };
        if driven[..place].iter().any(|earlier| earlier.name == *name) {
            return says(format!("the input {name} is read from a mask twice"));
        }
        if that_set_a_size.contains(name) {
            return says(format!(
                "the input {name} sets the size of a buffer of the shader, which is worked out once for the whole picture, so it cannot vary across it"
            ));
        }
        if !(maximum - minimum).is_finite() {
            return says(format!(
                "the range of the input {name} is too wide for a number to be read from a mask"
            ));
        }
        if matches!(one.space, Space::World(_)) {
            if !input.world_space {
                return says(format!(
                    "the input {name} cannot be read from a mask laid in space: its header does not say \"WORLD_SPACE\": true, which is the shader's promise that it says where it is working"
                ));
            }
            if first_use_of(&without_comments(&file.body), WORLD_POSITION).is_none() {
                return says(format!(
                    "the input {name} cannot be read from a mask laid in space: the shader never sets {WORLD_POSITION}, so there is no point of space to read the mask at"
                ));
            }
        }
    }
    let masks = driven.iter().map(|one| one.mask + 1).max().unwrap_or(0);
    (0..masks)
        .find(|number| driven.iter().all(|one| one.mask != *number))
        .and_then(|missing| {
            says(format!(
                "mask {} is used and mask {missing} is not: the masks of a shader are numbered from 0, with none left out",
                masks - 1
            ))
        })
}

/// Rewrites the shader of `file`, with no input read from a mask. (What the tests of the rewriting step call; the program always says which inputs are driven, and none is a list.)
#[cfg(test)]
pub fn rewrite(file: &IsfFile) -> Result<Rewritten, IsfProblem> {
    rewrite_with_driven_inputs(file, &[])
}

/// Rewrites the shader of `file`, with the float inputs `driven` read from masks.
///
/// Each mask is one more image of the shader (named by [`image_name_of_the_mask`]), after the others, and is read **once** at the pixel being made. A mask is premultiplied: its opacity is
/// how much of the pixel it covers. For an input the **unit** is the channel of the colour with the opacity divided out (0 where nothing covers), and the value is
/// `mix(the input's own number in the call, MIN + (MAX - MIN) * unit, how much covers)`: so **where the mask has nothing, the input is the number the call has for it** (the engine puts
/// the value of a slider made for the purpose there), where it has everything the range of the input is spread over the channel, and in between it is a mixture. The map from the unit to
/// the value is a straight line for now.
pub fn rewrite_with_driven_inputs(file: &IsfFile, driven: &[DrivenInput]) -> Result<Rewritten, IsfProblem> {
    if let Some(problem) = what_is_wrong_with_the_driven_inputs(file, driven) {
        return Err(problem);
    }
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
    // (The masks come last, so that every image the shader has is where it would be without them.)
    let images_of_the_shader = image_names.len();
    let masks = driven.iter().map(|one| one.mask + 1).max().unwrap_or(0);
    image_names.extend((0..masks).map(image_name_of_the_mask));

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
    // (The inputs that are read from a mask, in the order of the file.)
    let mut inputs_read_from_masks: Vec<InputReadFromAMask<'_>> = Vec::new();
    for input in &file.inputs {
        names_of_the_shader.insert(input.name.as_str());
        let n = |offset: usize| number_expression(number + offset);
        let expression = match &input.kind {
            IsfInputKind::Float { minimum, maximum, .. } => match driven.iter().find(|one| one.name == input.name) {
                Some(read_from) => {
                    inputs_read_from_masks.push(InputReadFromAMask {
                        name: &input.name,
                        minimum: *minimum,
                        maximum: *maximum,
                        own_number: number,
                        graph_at: None,
                        world_at: None,
                        read_from,
                    });
                    Some(match read_from.space {
                        Space::Screen => format!("_isf_driven_{}", input.name),
                        Space::World(_) => format!("_isf_world_{}()", input.name),
                    })
                }
                None => Some(n(0)),
            },
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
    // (The points of the graphs come after the numbers of all the inputs, one graph after another in the order of the file; then the placements of the masks laid in space, in the same way.)
    for input in &mut inputs_read_from_masks {
        if input.read_from.mapping == Mapping::Graph {
            input.graph_at = Some(number);
            number += NUMBERS_OF_A_GRAPH;
        }
    }
    for input in &mut inputs_read_from_masks {
        if matches!(input.read_from.space, Space::World(_)) {
            input.world_at = Some(number);
            number += NUMBERS_OF_A_WORLD_PLACEMENT;
        }
    }
    if number > MOST_NUMBERS_OF_A_CALL {
        let name = inputs_read_from_masks
            .iter()
            .find(|input| {
                input
                    .graph_at
                    .is_some_and(|at| at + NUMBERS_OF_A_GRAPH > MOST_NUMBERS_OF_A_CALL)
                    || input
                        .world_at
                        .is_some_and(|at| at + NUMBERS_OF_A_WORLD_PLACEMENT > MOST_NUMBERS_OF_A_CALL)
            })
            .map_or("", |input| input.name);
        return Err(IsfProblem::nowhere_in_particular(format!(
            "there is no room for the graph or the place in space of the input {name} among the numbers of the shader: a graph takes {NUMBERS_OF_A_GRAPH} and a place in space {NUMBERS_OF_A_WORLD_PLACEMENT} of the {MOST_NUMBERS_OF_A_CALL} numbers that a shader can have"
        )));
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
    // (The point of space that a shader says it works at: only for a shader that names it, so that the text of every other shader is what it always was.)
    if first_use_of(&code, WORLD_POSITION).is_some() {
        prelude.push_str(&format!("vec3 {WORLD_POSITION};\n"));
    }
    let any_over_the_picture = inputs_read_from_masks
        .iter()
        .any(|input| input.read_from.space == Space::Screen);
    if !inputs_read_from_masks.is_empty() {
        prelude.push_str(&functions_of_the_graphs(&inputs_read_from_masks));
        if any_over_the_picture {
            prelude.push_str(&function_that_sets_up_the_driven_inputs(&inputs_read_from_masks));
        }
        prelude.push_str(&functions_of_the_inputs_read_in_space(&inputs_read_from_masks));
    }
    let prelude_lines = prelude.matches('\n').count() as u32;

    let (body, count) = rewrite_image_calls(&file.body, &image_names[..images_of_the_shader]);
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
    let (body, set_up) = lift_the_initialisers_of_globals(&body);
    if !set_up.is_empty() {
        let names: Vec<&str> = set_up.iter().map(|(name, _)| name.as_str()).collect();
        notes.push(format!(
            "{} {} set before main runs: a compiler does not keep the initialiser of a global that is not a constant",
            names.join(", "),
            if names.len() == 1 { "is" } else { "are" }
        ));
    }
    let mut glsl = prelude;
    glsl.push_str(&body);
    if !set_up.is_empty() {
        glsl.push_str("\nvoid _isf_set_up_globals() {\n");
        for (name, initialiser) in &set_up {
            glsl.push_str(&format!("    {name} = {initialiser};\n"));
        }
        glsl.push_str("}\n");
    }
    glsl.push_str(&format!(
        "\nlayout(location = 0) out vec4 _isf_out;\nvoid main() {{\n    vec4 real = _isf_real_fragcoord();\n    _isf_fragcoord = vec4(real.x, _isf_u[{last}].y - real.y, real.z, real.w);\n    isf_FragNormCoord = vec2(real.x / _isf_u[{last}].x, 1.0 - real.y / _isf_u[{last}].y);\n    _isf_color = vec4(0.0);\n{driven_call}{set_up_call}    _isf_user_main();\n    _isf_out = _isf_color;\n}}\n",
        last = VEC4S_OF_THE_UNIFORM_BLOCK - 1,
        // (The inputs read from masks first of all: the initialisers of the globals may use them.)
        driven_call = if any_over_the_picture {
            "    _isf_set_up_driven();\n"
        } else {
            ""
        },
        set_up_call = if set_up.is_empty() { "" } else { "    _isf_set_up_globals();\n" }
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

#[cfg(test)]
mod driven_tests;
#[cfg(test)]
mod world_tests;
