// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a shader that others wrote is made of besides its code: the pictures it reads, the buffers it draws into, and the passes it is run in (phase 6.4; see
//! `docs/engine-phase-6-4-design.md`, K1 to K8).
//!
//! The engine does not know ISF. The host of ISF files (`src/isf`) reads a file's header and tells the engine, when it gives it the shader, **a [`ShaderStructure`]**: the engine makes
//! the textures, the sizes and the passes from it. A shader that was given with none is **one pass that reads one picture** ([`ShaderStructure::one_pass_of_one_picture`]), which is
//! what phase 1 made it.
//!
//! Everything here is a function of its arguments and has no graphics card in it, so what a pass will read and where it will draw can be looked at and tested without one.

/// How many pictures a shader may read, all together: the picture the effect works on, its buffers, and the images that it has of its own. (Fifteen bindings are the least a card gives
/// a stage; each picture takes two, a texture and a sampler, and the numbers take one.)
pub const MOST_IMAGES_OF_A_SHADER: usize = 7;

/// How many passes a shader may be run in.
pub const MOST_PASSES_OF_A_SHADER: usize = 16;

/// The most pixels across or down that a buffer may be, whatever its size says.
pub const LARGEST_SIDE_OF_A_BUFFER: u32 = 8192;

/// A sum, a product, a call of a function of the arithmetic of the expression of a size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

/// The functions that the expression of a size may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFunction {
    Floor,
    Ceil,
    Round,
    Abs,
    Min,
    Max,
    Pow,
    Sqrt,
    Sin,
    Cos,
    Exp,
    Log,
    Log2,
    Sign,
}

impl MathFunction {
    /// How many arguments the function takes.
    pub fn arguments(self) -> usize {
        match self {
            Self::Min | Self::Max | Self::Pow => 2,
            _ => 1,
        }
    }
}

/// The expression that says how many pixels across or down a buffer is. It is made by the host of the shader file from the text of the file, and **evaluated by the engine for each
/// call**, with the numbers of the call and the size of the picture the effect works on.
#[derive(Clone, Debug, PartialEq)]
pub enum SizeExpression {
    Number(f64),
    /// The width of the picture the effect works on (`$WIDTH`).
    Width,
    /// The height of the picture the effect works on (`$HEIGHT`).
    Height,
    /// The number of the call with this place in its numbers (an input of the shader that is read by its name).
    Value(usize),
    Negate(Box<SizeExpression>),
    Binary(Operator, Box<SizeExpression>, Box<SizeExpression>),
    /// A function and its arguments (as many as [`MathFunction::arguments`] says).
    Function(MathFunction, Vec<SizeExpression>),
}

impl SizeExpression {
    /// What the expression comes to for a picture `picture_size` (width, height) and a call with these `numbers`. A number that the call does not have is zero; the result can be
    /// anything that a `f64` can be (not a number, too: see [`size_from`]).
    pub fn evaluate(&self, picture_size: (u32, u32), numbers: &[f32]) -> f64 {
        match self {
            Self::Number(number) => *number,
            Self::Width => f64::from(picture_size.0),
            Self::Height => f64::from(picture_size.1),
            Self::Value(place) => f64::from(numbers.get(*place).copied().unwrap_or(0.0)),
            Self::Negate(inner) => -inner.evaluate(picture_size, numbers),
            Self::Binary(operator, left, right) => {
                let (left, right) = (
                    left.evaluate(picture_size, numbers),
                    right.evaluate(picture_size, numbers),
                );
                match operator {
                    Operator::Add => left + right,
                    Operator::Subtract => left - right,
                    Operator::Multiply => left * right,
                    Operator::Divide => left / right,
                    Operator::Remainder => left % right,
                }
            }
            Self::Function(function, arguments) => {
                // (`min` and `max` of the language take the number that is a number; a function short of an argument is no number at all.)
                if arguments.len() < function.arguments() {
                    return f64::NAN;
                }
                let argument = |place: usize| {
                    arguments
                        .get(place)
                        .map_or(f64::NAN, |argument| argument.evaluate(picture_size, numbers))
                };
                match function {
                    MathFunction::Floor => argument(0).floor(),
                    MathFunction::Ceil => argument(0).ceil(),
                    MathFunction::Round => argument(0).round(),
                    MathFunction::Abs => argument(0).abs(),
                    MathFunction::Min => argument(0).min(argument(1)),
                    MathFunction::Max => argument(0).max(argument(1)),
                    MathFunction::Pow => argument(0).powf(argument(1)),
                    MathFunction::Sqrt => argument(0).sqrt(),
                    MathFunction::Sin => argument(0).sin(),
                    MathFunction::Cos => argument(0).cos(),
                    MathFunction::Exp => argument(0).exp(),
                    MathFunction::Log => argument(0).ln(),
                    MathFunction::Log2 => argument(0).log2(),
                    MathFunction::Sign => {
                        let number = argument(0);
                        if number.is_nan() || number == 0.0 {
                            number
                        } else {
                            number.signum()
                        }
                    }
                }
            }
        }
    }
}

/// How many pixels a size that came to `value` is: the whole number under it, at least 1 and at most `largest` (**a number that is not a number is 1**).
pub fn size_from(value: f64, largest: u32) -> u32 {
    if value.is_nan() {
        return 1;
    }
    value
        .floor()
        .clamp(1.0, f64::from(largest.min(LARGEST_SIDE_OF_A_BUFFER))) as u32
}

/// What one of the pictures a shader reads is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSlot {
    /// The picture the effect works on: the layer's own, or what the effect before this one made.
    Picture,
    /// The buffer with this number (see [`ShaderStructure::buffers`]).
    Buffer(usize),
    /// The image with this number among the ones that the shader brings with it (phase 6.4.4): the call says which pictures they are (`EffectCall::images`).
    Imported(usize),
}

/// A picture that a shader draws into in one pass and reads in the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferSpec {
    /// Whether it keeps what was drawn into it for the next frame (phase 6.4.2: the engine keeps it under the state of the call, see [`EffectState`](super::EffectState)).
    pub persistent: bool,
    /// Whether it holds floating point numbers (half floats here) and not 8 bits of each colour.
    pub float: bool,
}

/// One pass of a shader.
#[derive(Clone, Debug, PartialEq)]
pub struct PassSpec {
    /// The buffer that the pass draws into; none for a pass that draws the picture that the effect makes.
    pub target: Option<usize>,
    /// How many pixels across and down the buffer is; the size of the picture the effect works on if not said (or if the pass has no target).
    pub width: Option<SizeExpression>,
    pub height: Option<SizeExpression>,
}

/// What a shader is made of besides its code.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderStructure {
    /// The pictures the shader reads, in the order they are bound in: the kth is the texture at binding `1 + 2k` and its sampler at `2 + 2k`.
    pub images: Vec<ImageSlot>,
    pub buffers: Vec<BufferSpec>,
    /// The passes, in order. The last draws the picture that the effect makes.
    pub passes: Vec<PassSpec>,
    /// Which of the numbers of a call is the number of the pass being run (`PASSINDEX`), for a shader that has such a number. A shader of phase 1 owns all its numbers, and has none.
    pub pass_index_number: Option<usize>,
}

/// What a pass of a call draws into, and how big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassFacts {
    pub target: Option<usize>,
    /// The size of the picture drawn: the size of the buffer, or of the picture the effect works on.
    pub size: (u32, u32),
    /// The format of the texture drawn into.
    pub format: TargetFormat,
}

/// The format of the texture that a pass draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetFormat {
    /// The format of the pictures that effects make (half floats): what a pass with no target, and a buffer with `float`, draw into.
    Half,
    /// 8 bits of each colour: a buffer that is not `float`.
    EightBit,
}

/// What an image of a shader reads in a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSource {
    /// The picture the effect works on.
    Picture,
    /// What the pass with this number drew, earlier in this frame.
    DrawnByPass(usize),
    /// Nothing: a buffer that has not been drawn into yet in this frame, and keeps nothing from the frame before.
    Clear,
    /// The image that the shader brings with it with this number: a picture that the call names.
    Imported(usize),
    /// What the buffer held when the frame before ended: a buffer that lasts, and that no earlier pass of this frame drew into. (A call that has no state to keep it under reads nothing,
    /// as it would of a buffer that does not last.)
    FromTheFrameBefore,
}

impl ShaderStructure {
    /// One pass that reads the picture the effect works on and draws what the effect makes: what a shader given to the engine with nothing said of it is.
    pub fn one_pass_of_one_picture() -> Self {
        Self {
            images: vec![ImageSlot::Picture],
            buffers: Vec::new(),
            passes: vec![PassSpec {
                target: None,
                width: None,
                height: None,
            }],
            pass_index_number: None,
        }
    }

    /// Whether this is what [`Self::one_pass_of_one_picture`] makes.
    pub fn is_one_pass_of_one_picture(&self) -> bool {
        *self == Self::one_pass_of_one_picture()
    }

    /// In words, what is wrong with the structure if anything is: more pictures or passes than there may be, a buffer that is not one of the buffers, a buffer drawn into by two
    /// passes, and a last pass that draws into a buffer and not the picture the effect makes.
    pub fn check(&self) -> Result<(), String> {
        if self.images.len() > MOST_IMAGES_OF_A_SHADER {
            return Err(format!(
                "the shader reads {} pictures, and {MOST_IMAGES_OF_A_SHADER} are the most",
                self.images.len()
            ));
        }
        if self.passes.is_empty() {
            return Err("the shader has no pass".to_owned());
        }
        if self.passes.len() > MOST_PASSES_OF_A_SHADER {
            return Err(format!(
                "the shader has {} passes, and {MOST_PASSES_OF_A_SHADER} are the most",
                self.passes.len()
            ));
        }
        for image in &self.images {
            if let ImageSlot::Buffer(number) = image
                && *number >= self.buffers.len()
            {
                return Err(format!(
                    "a picture of the shader is buffer {number}, which there is not"
                ));
            }
        }
        let mut imported_seen = [false; MOST_IMAGES_OF_A_SHADER];
        for image in &self.images {
            if let ImageSlot::Imported(number) = image {
                if *number >= MOST_IMAGES_OF_A_SHADER {
                    return Err(format!(
                        "a picture of the shader is imported image {number}, which there cannot be"
                    ));
                }
                if std::mem::replace(&mut imported_seen[*number], true) {
                    return Err(format!("imported image {number} is two of the pictures of the shader"));
                }
            }
        }
        let mut drawn_into = vec![false; self.buffers.len()];
        for (number, pass) in self.passes.iter().enumerate() {
            let Some(target) = pass.target else { continue };
            if target >= self.buffers.len() {
                return Err(format!("pass {number} draws into buffer {target}, which there is not"));
            }
            if drawn_into[target] {
                return Err(format!(
                    "buffer {target} is drawn into by more than one pass, which is not run yet"
                ));
            }
            drawn_into[target] = true;
        }
        if self.passes.last().is_some_and(|last| last.target.is_some()) {
            return Err(
                "the last pass draws into a buffer and not into the picture the effect makes, which is not run yet"
                    .to_owned(),
            );
        }
        Ok(())
    }

    /// How many images the shader brings with it, which a call has to name the pictures of: the number after the largest of them.
    pub fn imported_images(&self) -> usize {
        self.images
            .iter()
            .filter_map(|image| match image {
                ImageSlot::Imported(number) => Some(number + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Whether the shader keeps something from one frame to the next, so that what it shows depends on the frames before: some buffer of it lasts.
    pub fn depends_on_earlier_frames(&self) -> bool {
        self.buffers.iter().any(|buffer| buffer.persistent)
    }

    /// What each pass draws into and how big that is, for a picture the effect works on of `picture_size` and a call with these `numbers`; `largest_side` is the most pixels
    /// the card allows a texture to be across.
    pub fn facts_of_the_passes(&self, picture_size: (u32, u32), numbers: &[f32], largest_side: u32) -> Vec<PassFacts> {
        self.passes
            .iter()
            .map(|pass| match pass.target {
                None => PassFacts {
                    target: None,
                    size: picture_size,
                    format: TargetFormat::Half,
                },
                Some(buffer) => {
                    let side = |expression: &Option<SizeExpression>, whole: u32| match expression {
                        Some(expression) => size_from(expression.evaluate(picture_size, numbers), largest_side),
                        None => whole,
                    };
                    PassFacts {
                        target: Some(buffer),
                        size: (side(&pass.width, picture_size.0), side(&pass.height, picture_size.1)),
                        format: if self.buffers.get(buffer).is_some_and(|spec| spec.float) {
                            TargetFormat::Half
                        } else {
                            TargetFormat::EightBit
                        },
                    }
                }
            })
            .collect()
    }

    /// What each of the pictures of the shader reads in the pass number `pass`: the picture the effect works on, or a buffer as an earlier pass drew it, or, if no earlier pass drew
    /// it, what a buffer that lasts held at the end of the frame before (and nothing for one that does not).
    pub fn what_each_image_reads(&self, pass: usize) -> Vec<ImageSource> {
        self.images
            .iter()
            .map(|image| match image {
                ImageSlot::Picture => ImageSource::Picture,
                ImageSlot::Imported(number) => ImageSource::Imported(*number),
                ImageSlot::Buffer(buffer) => self
                    .passes
                    .iter()
                    .take(pass)
                    .position(|earlier| earlier.target == Some(*buffer))
                    .map_or_else(
                        || {
                            if self.buffers[*buffer].persistent {
                                ImageSource::FromTheFrameBefore
                            } else {
                                ImageSource::Clear
                            }
                        },
                        ImageSource::DrawnByPass,
                    ),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
