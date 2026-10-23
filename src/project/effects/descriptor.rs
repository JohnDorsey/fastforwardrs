// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a type of effect is, as data: its key, its version, what it is called and does, and the values it has, each with a name that does not
//! change (the *id* of the value), what kind of value it is, what it begins as and how a control for it looks. Nothing here says what the effect
//! *does* to a picture: that is for what runs it (see [`EffectKind::colour_matrix`](super::EffectKind::colour_matrix)), so a descriptor is
//! something that a file can bring, and that a project file keeps a copy of (a *snapshot*) for each type of effect that it uses.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use serde_json::Value;

use crate::project::animation::{PropertyValue, ValueKind};
use crate::rgba_picture::RgbaPicture;

/// The most values that an effect can have (a value is named in the code by a small number, its *slot*: see
/// [`PropertyId::EffectValue`](crate::project::PropertyId::EffectValue)).
pub const MOST_VALUES_OF_AN_EFFECT: usize = 255;

/// What a type of effect works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectDomain {
    /// The picture of a clip, or of a track: what is seen. (Video, pictures and text objects have one.)
    Picture,
    /// The sound of a clip, or of a track. (Nothing makes use of one yet: that comes with the sound engine.)
    Sound,
}

/// What a file says that this version of the program does not know, kept as it was, so that it is written back as it was (a file that a newer
/// program wrote, with more in it, survives being opened and saved by this one).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnknownFields(BTreeMap<String, Value>);

impl UnknownFields {
    /// No unknown fields.
    pub const fn none() -> Self {
        Self(BTreeMap::new())
    }

    pub fn new(fields: BTreeMap<String, Value>) -> Self {
        Self(fields)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The fields, by name.
    pub fn fields(&self) -> &BTreeMap<String, Value> {
        &self.0
    }
}

/// How a number is laid along its slider: evenly, or so that equal steps are equal ratios (for what spans orders of magnitude, such as a radius or a time).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueScale {
    #[default]
    Linear,
    /// Needs a range that is above 0 at both ends.
    Logarithmic,
}

/// One of the things a choice offers: what a file calls it, and what the control says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceOption {
    pub id: Cow<'static, str>,
    pub label: Cow<'static, str>,
}

/// What the control of a value is. A value is always one of the kinds of [`PropertyValue`]; this says how the panel lets a person change it (and a *choice* and a *resource*
/// are words: the id of an option, the name of a resource).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ParameterControl {
    /// What its kind has: a slider or a field for a number, a field for two, a colour button, a checkbox, a text field for words.
    #[default]
    Automatic,
    /// A number with a slider (it must have a range).
    Slider,
    /// A whole number.
    Integer,
    /// A flag, as a switch.
    Toggle,
    /// Words that are the id of one of the options, chosen from a list. (Not animated by its stopwatch as a number is: it holds until the next keyframe.)
    Choice(Cow<'static, [ChoiceOption]>),
    /// Words that are the name of a resource of this kind (`cube`), chosen with a file chooser, which the project keeps (see `docs/engine-phase-4-design.md`).
    Resource(Cow<'static, str>),
    /// Words that may have more than a line: a box of a few lines (what a text says).
    Multiline,
    /// Words that are the name of a font, chosen from the fonts of the computer (an empty name is the program's own font; a name that the computer does not have is kept as it is).
    Font,
    /// A number that is **a fraction of the height of the picture**, which is what it is kept as and shown as the points that it would be on a picture 1080 tall (what a font's size is: `FontSize`).
    PointsOfThePictureHeight,
}

/// One of the values that a type of effect has.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterDescriptor {
    /// What names the value in project files, and for as long as the effect type exists: it is never a position, and never changes. (The
    /// values of an effect are kept by their place in the descriptor in the program, and by their ids in a file.)
    pub id: Cow<'static, str>,
    /// What the value is called when it is told apart from the other properties of a clip or a track (its keyframes, the buttons of them):
    /// in lower case, as the names of the other properties are.
    pub name: Cow<'static, str>,
    /// What the control of the value says.
    pub label: Cow<'static, str>,
    /// What the value is when the effect is put on. Which kind of value it is, is the kind of this.
    pub default: PropertyValue,
    /// The least and the most that the control of a number shows: it can be typed past them, as a turn of the colours is typed to go round more
    /// than once. (Nothing limits what a number is yet.)
    pub range: Option<(f32, f32)>,
    /// What is written after a number (a degree sign, say), or nothing.
    pub unit: Cow<'static, str>,
    /// The most decimals that a number is shown with, or as many as it has.
    pub decimals: Option<u8>,
    /// What a file said of the value that this version of the program does not know, to be written back as it was.
    pub unknown_fields: UnknownFields,
    /// How a number is laid along its slider.
    pub scale: ValueScale,
    /// How the control of the value works.
    pub control: ParameterControl,
}

impl ParameterDescriptor {
    /// What kind of value the value is.
    pub fn value_kind(&self) -> ValueKind {
        self.default.kind()
    }

    /// What the value begins as when an effect is put on. **A choice whose default is no words at all begins as its first option** (the effects of the program have their
    /// descriptors in statics, where a word cannot be made); every other value begins as its default.
    pub fn value_it_begins_as(&self) -> PropertyValue {
        match (&self.control, &self.default) {
            (ParameterControl::Choice(options), PropertyValue::Words(default)) if default.is_empty() => {
                options.first().map_or_else(
                    || self.default.clone(),
                    |first| PropertyValue::Words(first.id.to_string()),
                )
            }
            _ => self.default.clone(),
        }
    }
}

/// The language of the shader that runs an effect that is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderLanguage {
    /// Interactive Shader Format: a JSON header and the GLSL after it (see `crate::isf`).
    Isf,
}

impl ShaderLanguage {
    /// What a project file calls it.
    pub fn name_in_a_file(self) -> &'static str {
        match self {
            Self::Isf => "isf",
        }
    }

    /// The language that a project file calls `name`, if there is one.
    pub fn from_name_in_a_file(name: &str) -> Option<Self> {
        match name {
            "isf" => Some(Self::Isf),
            _ => None,
        }
    }
}

/// What runs an effect that is a shader: **the whole text of its file**, kept in the descriptor so that a project that uses the effect carries it, and runs where it was never installed
/// (a shader is data, which no prompt is asked about: see `docs/engine-phase-6-design.md`, J3). The most that a text of one can be is [`MOST_BYTES_OF_A_SHADER`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderSource {
    pub language: ShaderLanguage,
    pub text: Cow<'static, str>,
    /// The images that the shader brings with it (an ISF file's `IMPORTED`), in the order the file lists them: **kept in the shader, as its text is** (phase 6.4.4; see
    /// `docs/engine-phase-6-4-design.md`, K15).
    pub images: Vec<ShaderImage>,
}

/// The most images that a shader may bring with it.
pub const MOST_IMAGES_A_SHADER_BRINGS: usize = 6;

/// The most bytes that the file of one of those images may have (a PNG or a JPEG).
pub const MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER: usize = 4 * 1024 * 1024;

/// The most pixels across or down that one of those images may have.
pub const MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER: u32 = 4096;

/// An image that a shader brings with it: the name the shader reads it by, and the bytes of its file (a PNG or a JPEG), which are what the project carries.
#[derive(Clone, Debug)]
pub struct ShaderImage {
    name: String,
    bytes: Arc<[u8]>,
    /// What the file is as pixels, read once (the first time it is asked for) and shared by the copies. **Not a part of what makes two images equal**: it is a function of the bytes.
    pixels: Arc<OnceLock<Result<Arc<RgbaPicture>, String>>>,
}

impl PartialEq for ShaderImage {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.bytes == other.bytes
    }
}

impl Eq for ShaderImage {}

impl ShaderImage {
    pub fn new(name: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            name: name.into(),
            bytes: bytes.into(),
            pixels: Arc::default(),
        }
    }

    /// The name the shader reads the image by.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The bytes of the file, which the project keeps.
    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    /// What the file is as pixels (straight, not multiplied by the opacity, top row first), or what is wrong with it: more bytes than an image may have, a file that is not a PNG or a
    /// JPEG, a picture of more than [`MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER`] pixels across or down, or none. **Read once.**
    pub fn pixels(&self) -> Result<Arc<RgbaPicture>, String> {
        self.pixels
            .get_or_init(|| {
                if self.bytes.len() > MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER {
                    return Err(format!(
                        "the file has {} bytes, and {MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER} are the most",
                        self.bytes.len()
                    ));
                }
                let format =
                    image::guess_format(&self.bytes).map_err(|_| "the file is not a PNG or a JPEG".to_owned())?;
                if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
                    return Err("the file is not a PNG or a JPEG".to_owned());
                }
                let decoded = image::load_from_memory_with_format(&self.bytes, format)
                    .map_err(|error| error.to_string())?
                    .to_rgba8();
                let (width, height) = (decoded.width(), decoded.height());
                if width == 0 || height == 0 {
                    return Err("the picture has no pixels".to_owned());
                }
                if width > MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER || height > MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER {
                    return Err(format!(
                        "the picture is {width} by {height} pixels, and {MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER} across or down are the most"
                    ));
                }
                Ok(Arc::new(RgbaPicture {
                    width_in_pixels: width,
                    height_in_pixels: height,
                    rgba_pixels: decoded.into_raw(),
                }))
            })
            .clone()
    }
}

/// The most bytes that the text of a shader may have.
pub const MOST_BYTES_OF_A_SHADER: usize = 256 * 1024;

/// A type of effect: what is the same about all the effects of it that there are.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectDescriptor {
    /// What names the type of effect, in project files and everywhere else it is told from others: a namespace, a colon and an id
    /// (`builtin:hue_shift`). It never has a file path in it, and never a position.
    pub key: Cow<'static, str>,
    /// The major and the minor number. The same major number means that what was made with one can be used with the other.
    pub version: (u32, u32),
    /// What it is called, in lower case.
    pub name: Cow<'static, str>,
    /// What it does, in a few words, for the menu that it is chosen from.
    pub description: Cow<'static, str>,
    /// The heading it is listed under in the menu that it is chosen from (*Colour*, *Light*...), or nothing, for one that files bring without saying.
    pub category: Cow<'static, str>,
    pub domain: EffectDomain,
    /// Whether it changes each pixel of a picture by itself, whatever the pixels round it are (as a change of colours does, and a blur does
    /// not). An effect that does can be put on a clip group or a track group, which is done to each of its members for now instead of to
    /// the picture they make together, which is only the same picture for such an effect (see `project::groups::effects_of_groups`).
    pub changes_each_pixel_by_itself: bool,
    /// The values that it has, in the order they are kept and shown in.
    pub parameters: Cow<'static, [ParameterDescriptor]>,
    /// What a file said of it that this version does not know.
    pub unknown_fields: UnknownFields,
    /// The shader that runs it, if it is one (see [`ShaderSource`]); `None` for the effects of the program and of plugins.
    pub shader: Option<ShaderSource>,
}

impl EffectDescriptor {
    /// What is wrong with the descriptor, if anything is: it has to be one that the rest of the program can rely on.
    pub fn problem(&self) -> Option<String> {
        let key_is_right = self
            .key
            .split_once(':')
            .is_some_and(|(namespace, id)| !namespace.is_empty() && !id.is_empty());
        if !key_is_right {
            return Some(format!(
                "the key \"{}\" is not a namespace, a colon and an id",
                self.key
            ));
        }
        if self.name.is_empty() {
            return Some(format!("the effect \"{}\" has no name", self.key));
        }
        if let Some(shader) = &self.shader
            && shader.text.len() > MOST_BYTES_OF_A_SHADER
        {
            return Some(format!(
                "the shader of the effect \"{}\" is {} bytes, and {MOST_BYTES_OF_A_SHADER} are the most there can be",
                self.key,
                shader.text.len()
            ));
        }
        if self.parameters.len() > MOST_VALUES_OF_AN_EFFECT {
            return Some(format!(
                "the effect \"{}\" has {} values, and {MOST_VALUES_OF_AN_EFFECT} are the most there can be",
                self.key,
                self.parameters.len()
            ));
        }
        for (place, parameter) in self.parameters.iter().enumerate() {
            if parameter.id.is_empty() || parameter.name.is_empty() {
                return Some(format!(
                    "value number {} of the effect \"{}\" has no id or no name",
                    place + 1,
                    self.key
                ));
            }
            if self.parameters[..place].iter().any(|other| other.id == parameter.id) {
                return Some(format!(
                    "the effect \"{}\" has two values with the id \"{}\"",
                    self.key, parameter.id
                ));
            }
            if !is_a_number_that_can_be_used(&parameter.default) {
                return Some(format!(
                    "the value \"{}\" of the effect \"{}\" begins as a number that is not one",
                    parameter.id, self.key
                ));
            }
            if let Some((least, most)) = parameter.range
                && !(least.is_finite() && most.is_finite() && least <= most)
            {
                return Some(format!(
                    "the range of the value \"{}\" of the effect \"{}\" is not a least that is no more than a most",
                    parameter.id, self.key
                ));
            }
            if let Some(problem) = problem_with_how_it_is_shown(&self.key, parameter) {
                return Some(problem);
            }
        }
        None
    }

    /// Whether an effect that was made with the type of effect that `snapshot` describes can be used as an effect of this type instead:
    /// the same key and major number, the same domain, and every value that the snapshot has is a value of this one of the same kind. (What
    /// this has more than the snapshot does has what it begins as.)
    pub fn can_stand_in_for(&self, snapshot: &EffectDescriptor) -> bool {
        self.key == snapshot.key
            && self.version.0 == snapshot.version.0
            && self.domain == snapshot.domain
            && snapshot.parameters.iter().all(|wanted| {
                self.parameters
                    .iter()
                    .any(|own| own.id == wanted.id && own.value_kind() == wanted.value_kind())
            })
    }
}

/// What is wrong with the scale and the control of `parameter`, if anything: they have to fit the kind of value that it is.
fn problem_with_how_it_is_shown(key: &str, parameter: &ParameterDescriptor) -> Option<String> {
    let id = &parameter.id;
    let kind = parameter.value_kind();
    let wrong = |what: &str| Some(format!("the value \"{id}\" of the effect \"{key}\" {what}"));
    if parameter.scale == ValueScale::Logarithmic {
        let range_is_above_zero = matches!(parameter.range, Some((least, _)) if least > 0.0);
        if kind != ValueKind::Number || parameter.range.is_none() {
            return wrong("has a logarithmic scale, which a number with a range needs");
        }
        if !range_is_above_zero {
            return wrong("has a logarithmic scale, which needs a range that is above 0 at both ends");
        }
    }
    match &parameter.control {
        ParameterControl::Automatic => None,
        ParameterControl::Slider if kind == ValueKind::Number => None,
        ParameterControl::Slider => wrong("is a slider, and is not a number"),
        ParameterControl::Integer if kind == ValueKind::Number => None,
        ParameterControl::Integer => wrong("is a whole number, and is not a number"),
        ParameterControl::Toggle if kind == ValueKind::Flag => None,
        ParameterControl::Toggle => wrong("is a toggle, and is not a flag"),
        ParameterControl::Choice(options) => {
            if kind != ValueKind::Words {
                return wrong("is a choice, and is not words (an option is named by its id)");
            }
            if options.is_empty() {
                return wrong("is a choice that has no options");
            }
            if options.iter().any(|option| option.id.is_empty()) {
                return wrong("has an option with no id");
            }
            for (place, option) in options.iter().enumerate() {
                if options[..place].iter().any(|other| other.id == option.id) {
                    return wrong(&format!("has two options with the id \"{}\"", option.id));
                }
            }
            match &parameter.default {
                PropertyValue::Words(default)
                    if default.is_empty() || options.iter().any(|option| option.id == default.as_str()) =>
                {
                    None
                }
                _ => wrong("begins as an option that it has not"),
            }
        }
        ParameterControl::Resource(resource_kind) if kind != ValueKind::Words => {
            let _ = resource_kind;
            wrong("is a resource, and is not words (it is named by the resource's name)")
        }
        ParameterControl::Resource(resource_kind) if resource_kind.is_empty() => wrong("is a resource with no kind"),
        ParameterControl::Resource(_) => None,
        ParameterControl::Multiline if kind == ValueKind::Words => None,
        ParameterControl::Multiline => wrong("is a box of lines, and is not words"),
        ParameterControl::Font if kind == ValueKind::Words => None,
        ParameterControl::Font => wrong("is a font, and is not words (it is named by the name of the font)"),
        ParameterControl::PointsOfThePictureHeight if kind == ValueKind::Number => None,
        ParameterControl::PointsOfThePictureHeight => {
            wrong("is a size in points, and is not a number (it is a fraction of the picture's height)")
        }
    }
}

/// Whether `value`, if it is a number or two, is made of numbers that are not infinite and are numbers.
fn is_a_number_that_can_be_used(value: &PropertyValue) -> bool {
    match value {
        PropertyValue::Number(number) => number.is_finite(),
        PropertyValue::Pair(first, second) => first.is_finite() && second.is_finite(),
        PropertyValue::Colour(_) | PropertyValue::Flag(_) | PropertyValue::Words(_) => true,
    }
}
