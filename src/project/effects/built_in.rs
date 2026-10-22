// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects that the program has in itself: each is a descriptor like the ones that files bring (see [`EffectDescriptor`]), and the program
//! runs it, which is what is here: how it changes the colours of a picture, and what it is exported as. A new built-in effect is a new
//! descriptor, a new constant of [`EffectKind`] (and a place in [`EffectKind::BUILT_IN`]) and a new row of the behaviours here. (This is the one place that knows what a built-in effect does: what runs the others will be
//! somewhere else, and be found by the namespace of the key.)
//!
//! **A descriptor here is never changed without its version being changed**: a project file keeps a snapshot of the descriptor of each effect it
//! uses, and an effect is used as the effect of the program only when the snapshot is of the same major number and has nothing that this has not.
//! (A test has the descriptor as it was in the first version, to say so when it is changed.)

use std::borrow::Cow;
use std::sync::Arc;

use super::{
    ChoiceOption, EffectDescriptor, EffectDomain, EffectKind, ParameterControl, ParameterDescriptor, UnknownFields,
    ValueScale,
};
use crate::colour_effects::ColourMatrix;
use crate::cube_table::{CubeTable, Interpolation};
use crate::project::animation::PropertyValue;

/// The key of the hue shift.
pub const HUE_SHIFT_KEY: &str = "builtin:hue_shift";

/// The key of the saturation.
pub const SATURATION_KEY: &str = "builtin:saturation";

/// The key of brightness and contrast.
pub const BRIGHTNESS_AND_CONTRAST_KEY: &str = "builtin:brightness_contrast";

/// The key of the exposure.
pub const EXPOSURE_KEY: &str = "builtin:exposure";

/// The key of the opacity.
pub const OPACITY_KEY: &str = "builtin:opacity";

/// The key of the blur.
pub const BLUR_KEY: &str = "builtin:blur";

/// The key of the crop.
pub const CROP_KEY: &str = "builtin:crop";

/// The key of the look-up table.
pub const LUT_KEY: &str = "builtin:lut";

/// The hue shift: the hues of the colours of the picture are turned round the wheel of colours, as far as its one value says.
pub(super) static HUE_SHIFT: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(HUE_SHIFT_KEY),
    version: (1, 1),
    name: Cow::Borrowed("hue shift"),
    description: Cow::Borrowed("Turns the colours round the wheel of colours"),
    category: Cow::Borrowed("Colour"),
    domain: EffectDomain::Picture,
    changes_each_pixel_by_itself: true,
    parameters: Cow::Borrowed(&[ParameterDescriptor {
        id: Cow::Borrowed("degrees"),
        name: Cow::Borrowed("hue shift"),
        label: Cow::Borrowed("Turn"),
        default: PropertyValue::Number(0.0),
        // (From half a turn back to half a turn on is all the colours there are to turn to. The value can be typed to go further, as a turn
        // is typed, which is how many times the colours go round between two keyframes.)
        range: Some((-180.0, 180.0)),
        unit: Cow::Borrowed("°"),
        decimals: Some(1),
        unknown_fields: UnknownFields::none(),
        scale: ValueScale::Linear,
        control: ParameterControl::Automatic,
    }]),
    unknown_fields: UnknownFields::none(),
};

/// Makes the colours more or less colourful.
pub(super) static SATURATION: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(SATURATION_KEY),
    version: (1, 0),
    name: Cow::Borrowed("saturation"),
    description: Cow::Borrowed("Makes the colours more or less colourful"),
    category: Cow::Borrowed("Colour"),
    domain: EffectDomain::Picture,
    changes_each_pixel_by_itself: true,
    parameters: Cow::Borrowed(&[ParameterDescriptor {
        id: Cow::Borrowed("amount"),
        name: Cow::Borrowed("saturation"),
        label: Cow::Borrowed("Amount"),
        // (1 is the colours as they are, 0 is no colour at all, and 2 is twice as far from grey. It can be typed to go further.)
        default: PropertyValue::Number(1.0),
        range: Some((0.0, 2.0)),
        unit: Cow::Borrowed(""),
        decimals: Some(2),
        unknown_fields: UnknownFields::none(),
        scale: ValueScale::Linear,
        control: ParameterControl::Automatic,
    }]),
    unknown_fields: UnknownFields::none(),
};

/// Makes the picture lighter or darker, and its lights and darks further apart or nearer together.
pub(super) static BRIGHTNESS_AND_CONTRAST: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(BRIGHTNESS_AND_CONTRAST_KEY),
    version: (1, 0),
    name: Cow::Borrowed("brightness and contrast"),
    description: Cow::Borrowed(
        "Makes the picture lighter or darker, and its lights and darks further apart or nearer together",
    ),
    category: Cow::Borrowed("Light"),
    domain: EffectDomain::Picture,
    changes_each_pixel_by_itself: true,
    parameters: Cow::Borrowed(&[
        ParameterDescriptor {
            id: Cow::Borrowed("brightness"),
            name: Cow::Borrowed("brightness"),
            label: Cow::Borrowed("Brightness"),
            // (The fraction of the steps of a colour that every colour is moved by: 0.1 is a tenth of them more.)
            default: PropertyValue::Number(0.0),
            range: Some((-1.0, 1.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(2),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
        ParameterDescriptor {
            id: Cow::Borrowed("contrast"),
            name: Cow::Borrowed("contrast"),
            label: Cow::Borrowed("Contrast"),
            // (How many times as far from the middle grey every colour is made: 1 is as it is, 0 is all the middle grey.)
            default: PropertyValue::Number(1.0),
            range: Some((0.0, 2.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(2),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
    ]),
    unknown_fields: UnknownFields::none(),
};

/// Makes the light of the picture brighter or darker as the exposure of a camera does.
pub(super) static EXPOSURE: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(EXPOSURE_KEY),
    version: (1, 0),
    name: Cow::Borrowed("exposure"),
    description: Cow::Borrowed(
        "Makes the light of the picture brighter or darker, by whole and part stops, as the exposure of a camera does",
    ),
    category: Cow::Borrowed("Light"),
    domain: EffectDomain::Picture,
    changes_each_pixel_by_itself: true,
    parameters: Cow::Borrowed(&[ParameterDescriptor {
        id: Cow::Borrowed("stops"),
        name: Cow::Borrowed("exposure"),
        label: Cow::Borrowed("Amount"),
        // (A stop is twice the light. It can be typed to go further, up to twelve.)
        default: PropertyValue::Number(0.0),
        range: Some((-5.0, 5.0)),
        unit: Cow::Borrowed(" stops"),
        decimals: Some(2),
        unknown_fields: UnknownFields::none(),
        scale: ValueScale::Linear,
        control: ParameterControl::Automatic,
    }]),
    unknown_fields: UnknownFields::none(),
};

/// Makes the picture more or less see-through.
pub(super) static OPACITY: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(OPACITY_KEY),
    version: (1, 0),
    name: Cow::Borrowed("opacity"),
    description: Cow::Borrowed("Makes the picture more or less see-through"),
    category: Cow::Borrowed("Transform"),
    domain: EffectDomain::Picture,
    // (It is not done to each pixel by itself: it changes how clear a pixel is. Since phase 5 that does not decide what a group can have: the engine makes the picture of a group.)
    changes_each_pixel_by_itself: false,
    parameters: Cow::Borrowed(&[ParameterDescriptor {
        id: Cow::Borrowed("opacity"),
        name: Cow::Borrowed("opacity"),
        label: Cow::Borrowed("Amount"),
        default: PropertyValue::Number(1.0),
        range: Some((0.0, 1.0)),
        unit: Cow::Borrowed(""),
        decimals: Some(2),
        unknown_fields: UnknownFields::none(),
        scale: ValueScale::Linear,
        control: ParameterControl::Automatic,
    }]),
    unknown_fields: UnknownFields::none(),
};

/// How far from one a factor of the light or of the opacity may be, for the operation to be taken for one that changes nothing: a change of less than a hundredth of a
/// step of the 256 steps of a colour changes no pixel. (The same limit as the colour matrix's.)
const NEAR_ENOUGH_TO_CHANGE_NOTHING: f32 = 1.0 / 256.0 / 100.0;

/// Softens the picture with a Gaussian blur.
pub(super) static BLUR: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(BLUR_KEY),
    version: (1, 0),
    name: Cow::Borrowed("blur"),
    description: Cow::Borrowed("Softens the picture with a Gaussian blur, of light"),
    category: Cow::Borrowed("Blur and sharpen"),
    domain: EffectDomain::Picture,
    // (Not done to each pixel by itself: it mixes a pixel with the ones round it, so on a group it would not be what it is done to the picture the members make together.)
    changes_each_pixel_by_itself: false,
    parameters: Cow::Borrowed(&[ParameterDescriptor {
        id: Cow::Borrowed("radius"),
        name: Cow::Borrowed("blur"),
        label: Cow::Borrowed("Radius"),
        // (In points of a picture 1080 high: a picture of another height is blurred in proportion, so that a small preview of a picture looks like the whole of it. It is
        // the standard deviation of the Gaussian. It can be typed to go further, up to what the card can do in a pass.)
        default: PropertyValue::Number(0.0),
        range: Some((0.0, 100.0)),
        unit: Cow::Borrowed(" pt"),
        decimals: Some(1),
        unknown_fields: UnknownFields::none(),
        scale: ValueScale::Linear,
        control: ParameterControl::Automatic,
    }]),
    unknown_fields: UnknownFields::none(),
};

/// Cuts the edges off the picture, with a soft edge if wanted.
pub(super) static CROP: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(CROP_KEY),
    version: (1, 0),
    name: Cow::Borrowed("crop"),
    description: Cow::Borrowed("Cuts the edges off the picture, with a soft edge if wanted: what is cut off is clear"),
    category: Cow::Borrowed("Transform"),
    domain: EffectDomain::Picture,
    changes_each_pixel_by_itself: false,
    parameters: Cow::Borrowed(&[
        ParameterDescriptor {
            id: Cow::Borrowed("left"),
            name: Cow::Borrowed("crop from the left"),
            label: Cow::Borrowed("Left"),
            // (The fraction of the picture's width that is cut off from the left. The same for the others.)
            default: PropertyValue::Number(0.0),
            range: Some((0.0, 1.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(3),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
        ParameterDescriptor {
            id: Cow::Borrowed("top"),
            name: Cow::Borrowed("crop from the top"),
            label: Cow::Borrowed("Top"),
            default: PropertyValue::Number(0.0),
            range: Some((0.0, 1.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(3),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
        ParameterDescriptor {
            id: Cow::Borrowed("right"),
            name: Cow::Borrowed("crop from the right"),
            label: Cow::Borrowed("Right"),
            default: PropertyValue::Number(0.0),
            range: Some((0.0, 1.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(3),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
        ParameterDescriptor {
            id: Cow::Borrowed("bottom"),
            name: Cow::Borrowed("crop from the bottom"),
            label: Cow::Borrowed("Bottom"),
            default: PropertyValue::Number(0.0),
            range: Some((0.0, 1.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(3),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
        ParameterDescriptor {
            id: Cow::Borrowed("feather"),
            name: Cow::Borrowed("feather of the crop"),
            label: Cow::Borrowed("Feather"),
            // (How wide the soft edge is, as a fraction of the picture's height; half of it is inside each edge and half outside. Every edge has it, those that cut nothing
            // too: a feather with nothing cut off is a soft edge round the picture.)
            default: PropertyValue::Number(0.0),
            range: Some((0.0, 0.5)),
            unit: Cow::Borrowed(""),
            decimals: Some(3),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
    ]),
    unknown_fields: UnknownFields::none(),
};

/// The radius of a blur, in points of a picture 1080 high, under which it changes nothing: the Gaussian is narrower than a hundredth of a pixel. (The same limit as the
/// engine's.)
const MOST_THAT_A_BLUR_CAN_BE_AND_CHANGE_NOTHING: f32 = 0.01;

/// The fraction of the picture that a crop or its soft edge can be, under which it changes nothing: a ten-thousandth. (The same limit as the engine's.)
const MOST_THAT_A_CROP_CAN_CUT_AND_CHANGE_NOTHING: f32 = 1.0e-4;

/// The spaces the colours go into a table in and come out of it in (CM-11), and the ids that name them in a file and in the choice of the effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LutSpace {
    /// The codes of a screen (sRGB). What a table made for pictures wants, and the default.
    Screen,
    /// Rec.709 video: the primaries of Rec.709 and the power 2.4 of BT.1886.
    Video,
    /// Linear light, in the primaries of the working space.
    Linear,
}

impl LutSpace {
    pub const ALL: [Self; 3] = [Self::Screen, Self::Video, Self::Linear];

    pub fn id(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Video => "video",
            Self::Linear => "linear",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|space| space.id() == id)
    }
}

/// The options of the two spaces of a table (the same list for the input and for the output).
static OPTIONS_OF_A_SPACE: [ChoiceOption; 3] = [
    ChoiceOption {
        id: Cow::Borrowed("screen"),
        label: Cow::Borrowed("Screen codes (sRGB)"),
    },
    ChoiceOption {
        id: Cow::Borrowed("video"),
        label: Cow::Borrowed("Rec.709 video (gamma 2.4)"),
    },
    ChoiceOption {
        id: Cow::Borrowed("linear"),
        label: Cow::Borrowed("Linear light (the working space)"),
    },
];

static OPTIONS_OF_THE_WAY_TO_LOOK_UP: [ChoiceOption; 2] = [
    ChoiceOption {
        id: Cow::Borrowed("tetrahedral"),
        label: Cow::Borrowed("Tetrahedral"),
    },
    ChoiceOption {
        id: Cow::Borrowed("trilinear"),
        label: Cow::Borrowed("Trilinear"),
    },
];

/// Looks the colours of the picture up in a table (a `.cube` file).
pub(super) static LUT: EffectDescriptor = EffectDescriptor {
    shader: None,
    key: Cow::Borrowed(LUT_KEY),
    version: (1, 0),
    name: Cow::Borrowed("look-up table"),
    description: Cow::Borrowed(
        "Looks the colours of the picture up in a table of colours (a LUT, a .cube file), such as a look or a film emulation",
    ),
    category: Cow::Borrowed("Looks"),
    domain: EffectDomain::Picture,
    changes_each_pixel_by_itself: true,
    parameters: Cow::Borrowed(&[
        ParameterDescriptor {
            id: Cow::Borrowed("table"),
            name: Cow::Borrowed("table of the LUT"),
            label: Cow::Borrowed("Table"),
            // (The name of a resource of the project. Nothing, to begin with: an effect with no table does nothing.)
            default: PropertyValue::Words(String::new()),
            range: None,
            unit: Cow::Borrowed(""),
            decimals: None,
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Resource(Cow::Borrowed("cube")),
        },
        ParameterDescriptor {
            id: Cow::Borrowed("intensity"),
            name: Cow::Borrowed("intensity of the LUT"),
            label: Cow::Borrowed("Intensity"),
            default: PropertyValue::Number(1.0),
            range: Some((0.0, 1.0)),
            unit: Cow::Borrowed(""),
            decimals: Some(2),
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Automatic,
        },
        ParameterDescriptor {
            id: Cow::Borrowed("input_space"),
            name: Cow::Borrowed("input space of the LUT"),
            label: Cow::Borrowed("Input space"),
            // (The first option: see `ParameterDescriptor::value_it_begins_as`.)
            default: PropertyValue::Words(String::new()),
            range: None,
            unit: Cow::Borrowed(""),
            decimals: None,
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Choice(Cow::Borrowed(&OPTIONS_OF_A_SPACE)),
        },
        ParameterDescriptor {
            id: Cow::Borrowed("output_space"),
            name: Cow::Borrowed("output space of the LUT"),
            label: Cow::Borrowed("Output space"),
            default: PropertyValue::Words(String::new()),
            range: None,
            unit: Cow::Borrowed(""),
            decimals: None,
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Choice(Cow::Borrowed(&OPTIONS_OF_A_SPACE)),
        },
        ParameterDescriptor {
            id: Cow::Borrowed("interpolation"),
            name: Cow::Borrowed("interpolation of the LUT"),
            label: Cow::Borrowed("Interpolation"),
            default: PropertyValue::Words(String::new()),
            range: None,
            unit: Cow::Borrowed(""),
            decimals: None,
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control: ParameterControl::Choice(Cow::Borrowed(&OPTIONS_OF_THE_WAY_TO_LOOK_UP)),
        },
    ]),
    unknown_fields: UnknownFields::none(),
};

/// The most stops of exposure that is made: two to this is the most that the light is multiplied by, which half floats can hold many times over.
pub const MOST_STOPS_OF_EXPOSURE: f32 = 12.0;

/// A look-up table to be applied: the table, how much of it, the spaces, and the way of looking up.
#[derive(Clone, Debug, PartialEq)]
pub struct LutOperation {
    pub table: Arc<CubeTable>,
    pub intensity: f32,
    pub input: LutSpace,
    pub output: LutSpace,
    pub interpolation: Interpolation,
}

/// What an effect asks to be done to the picture of its layer, with its values at a frame. The preview and the export turn each into a call of the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum PictureOperation {
    /// The colours are mixed with a matrix (and moved by its offsets), in the codes of a screen. Neighbouring ones are one operation (see [`Self::then_if_alike`]).
    Mix(ColourMatrix),
    /// The light is made brighter or darker by this many stops.
    Exposure { stops: f32 },
    /// The picture is made this much opaque (from 0 to 1) of what it is.
    Opacity { opacity: f32 },
    /// The colours are looked up in a table.
    Lut(LutOperation),
    /// The picture is blurred by a Gaussian of this standard deviation, in points of a picture 1080 high.
    Blur { radius: f32 },
    /// The fractions of the picture that are cut off from the left, the top, the right and the bottom, and the width of the soft edge as a fraction of the picture's height.
    Crop {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        feather: f32,
    },
    /// A shader (an ISF file) is run with these numbers: the four of the shader itself (`TIME`, `TIMEDELTA`, `FRAMEINDEX`, `PASSINDEX`) and then those of its inputs (see
    /// `crate::isf::numbers_of_the_values`). The engine gives it the size of the picture. What it does is not known, so it is never an operation that changes nothing.
    ///
    /// A shader that depends on earlier frames (it has a buffer that lasts: phase 6.4.2) also says **which state it goes on with and at which frame of its own clock**, as `(id, frame)`
    /// (see `crate::engine::EffectState`); every other shader has none.
    ///
    /// A shader that brings images with it (phase 6.4.4) also carries them, in the order its file lists them.
    Shader {
        key: String,
        numbers: Vec<f32>,
        state: Option<(u64, u64)>,
        images: Vec<ImportedPicture>,
    },
}

/// An image that a shader brings with it, as an operation carries it: the pixels, and a number that tells it from every other (the same for the same image of the same shader, whatever the
/// frame): what the picture's id in the engine is made from. Two are equal if their numbers are.
#[derive(Clone, Debug)]
pub struct ImportedPicture {
    pub id: u64,
    pub picture: std::sync::Arc<crate::rgba_picture::RgbaPicture>,
}

impl PartialEq for ImportedPicture {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ImportedPicture {}

impl PictureOperation {
    /// Whether the operation changes no pixel, or none that can be told from the pixel it was: a mix that changes nothing, no exposure, all the opacity. (The same limit
    /// that the engine leaves such a call out by; a test holds the two to each other.)
    pub fn changes_nothing(&self) -> bool {
        match self {
            Self::Mix(matrix) => matrix.changes_nothing(),
            Self::Exposure { stops } => (stops.exp2() - 1.0).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING,
            Self::Opacity { opacity } => 1.0 - opacity < NEAR_ENOUGH_TO_CHANGE_NOTHING,
            Self::Lut(look) => look.intensity < NEAR_ENOUGH_TO_CHANGE_NOTHING,
            Self::Shader { .. } => false,
            Self::Blur { radius } => *radius < MOST_THAT_A_BLUR_CAN_BE_AND_CHANGE_NOTHING,
            Self::Crop {
                left,
                top,
                right,
                bottom,
                feather,
            } => [left, top, right, bottom, feather]
                .into_iter()
                .all(|fraction| *fraction < MOST_THAT_A_CROP_CAN_CUT_AND_CHANGE_NOTHING),
        }
    }

    /// The one operation that does what this one does and then what `next` does, if they are alike so that they can be (two colour mixes), and `None` if not.
    #[must_use]
    pub fn then_if_alike(&self, next: &Self) -> Option<Self> {
        match (self, next) {
            (Self::Mix(first), Self::Mix(second)) => Some(Self::Mix(first.then(*second))),
            _ => None,
        }
    }
}

/// The function that works out the operation of an effect from its values and the tables of them.
type HowToMakeTheOperation = fn(&[PropertyValue], &[Option<Arc<CubeTable>>]) -> Option<PictureOperation>;

/// What the program does for one of its effects.
struct Behaviour {
    kind: EffectKind,
    /// What it asks to be done to the picture, from its values and the tables that its resources are (one for each value, `Some` for a value that names a table the
    /// project has and can read); `None` for an effect that does nothing as it is.
    operation: HowToMakeTheOperation,
}

const BEHAVIOURS: [Behaviour; 8] = [
    Behaviour {
        kind: EffectKind::HUE_SHIFT,
        operation: |values, _| Some(hue_shift_operation(values)),
    },
    Behaviour {
        kind: EffectKind::SATURATION,
        operation: |values, _| Some(saturation_operation(values)),
    },
    Behaviour {
        kind: EffectKind::BRIGHTNESS_AND_CONTRAST,
        operation: |values, _| Some(brightness_and_contrast_operation(values)),
    },
    Behaviour {
        kind: EffectKind::EXPOSURE,
        operation: |values, _| Some(exposure_operation(values)),
    },
    Behaviour {
        kind: EffectKind::OPACITY,
        operation: |values, _| Some(opacity_operation(values)),
    },
    Behaviour {
        kind: EffectKind::BLUR,
        operation: |values, _| Some(blur_operation(values)),
    },
    Behaviour {
        kind: EffectKind::CROP,
        operation: |values, _| Some(crop_operation(values)),
    },
    Behaviour {
        kind: EffectKind::LUT,
        operation: lut_operation,
    },
];

/// What the effect `kind` asks to be done to a picture when its values are `values`, if it is an effect of the program.
pub(super) fn picture_operation(
    kind: EffectKind,
    values: &[PropertyValue],
    tables: &[Option<Arc<CubeTable>>],
) -> Option<PictureOperation> {
    let operation = BEHAVIOURS.iter().find(|behaviour| behaviour.kind == kind)?.operation;
    operation(values, tables)
}

/// The look-up table effect: nothing, while it has no table (none is named, the project has none of the name, or the file is not a table); and otherwise a look-up in the
/// table, by the spaces and the way that the other values say (one that is none of the options is the default).
fn lut_operation(values: &[PropertyValue], tables: &[Option<Arc<CubeTable>>]) -> Option<PictureOperation> {
    let [
        _,
        PropertyValue::Number(intensity),
        PropertyValue::Words(input),
        PropertyValue::Words(output),
        PropertyValue::Words(way),
    ] = values
    else {
        return None;
    };
    let table = tables.first()?.clone()?;
    Some(PictureOperation::Lut(LutOperation {
        table,
        intensity: intensity.clamp(0.0, 1.0),
        input: LutSpace::from_id(input).unwrap_or(LutSpace::Screen),
        output: LutSpace::from_id(output).unwrap_or(LutSpace::Screen),
        interpolation: Interpolation::from_id(way).unwrap_or_default(),
    }))
}

/// How the hue shift changes colours: it turns them by the degrees that its one value says.
fn hue_shift_operation(values: &[PropertyValue]) -> PictureOperation {
    PictureOperation::Mix(match values {
        [PropertyValue::Number(degrees)] => ColourMatrix::hue_rotation(*degrees),
        _ => ColourMatrix::NOTHING_CHANGES,
    })
}

fn saturation_operation(values: &[PropertyValue]) -> PictureOperation {
    PictureOperation::Mix(match values {
        [PropertyValue::Number(amount)] => ColourMatrix::saturation(*amount),
        _ => ColourMatrix::NOTHING_CHANGES,
    })
}

fn brightness_and_contrast_operation(values: &[PropertyValue]) -> PictureOperation {
    PictureOperation::Mix(match values {
        [PropertyValue::Number(brightness), PropertyValue::Number(contrast)] => {
            ColourMatrix::brightness_and_contrast(*brightness, *contrast)
        }
        _ => ColourMatrix::NOTHING_CHANGES,
    })
}

fn exposure_operation(values: &[PropertyValue]) -> PictureOperation {
    match values {
        [PropertyValue::Number(stops)] => PictureOperation::Exposure {
            stops: stops.clamp(-MOST_STOPS_OF_EXPOSURE, MOST_STOPS_OF_EXPOSURE),
        },
        _ => PictureOperation::Exposure { stops: 0.0 },
    }
}

fn blur_operation(values: &[PropertyValue]) -> PictureOperation {
    match values {
        [PropertyValue::Number(radius)] => PictureOperation::Blur {
            radius: radius.max(0.0),
        },
        _ => PictureOperation::Blur { radius: 0.0 },
    }
}

fn crop_operation(values: &[PropertyValue]) -> PictureOperation {
    match values {
        [
            PropertyValue::Number(left),
            PropertyValue::Number(top),
            PropertyValue::Number(right),
            PropertyValue::Number(bottom),
            PropertyValue::Number(feather),
        ] => PictureOperation::Crop {
            left: left.clamp(0.0, 1.0),
            top: top.clamp(0.0, 1.0),
            right: right.clamp(0.0, 1.0),
            bottom: bottom.clamp(0.0, 1.0),
            feather: feather.clamp(0.0, 1.0),
        },
        _ => PictureOperation::Crop {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            feather: 0.0,
        },
    }
}

fn opacity_operation(values: &[PropertyValue]) -> PictureOperation {
    match values {
        [PropertyValue::Number(opacity)] => PictureOperation::Opacity {
            opacity: opacity.clamp(0.0, 1.0),
        },
        _ => PictureOperation::Opacity { opacity: 1.0 },
    }
}
