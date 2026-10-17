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

use super::{
    EffectDescriptor, EffectDomain, EffectKind, ParameterControl, ParameterDescriptor, UnknownFields, ValueScale,
};
use crate::colour_effects::ColourMatrix;
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

/// The hue shift: the hues of the colours of the picture are turned round the wheel of colours, as far as its one value says.
pub(super) static HUE_SHIFT: EffectDescriptor = EffectDescriptor {
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
    key: Cow::Borrowed(OPACITY_KEY),
    version: (1, 0),
    name: Cow::Borrowed("opacity"),
    description: Cow::Borrowed("Makes the picture more or less see-through"),
    category: Cow::Borrowed("Transform"),
    domain: EffectDomain::Picture,
    // (It is not done to each pixel by itself: on a group it would be done to each member and not to the picture that they make together.)
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

/// The most stops of exposure that is made: two to this is the most that the light is multiplied by, which half floats can hold many times over.
pub const MOST_STOPS_OF_EXPOSURE: f32 = 12.0;

/// What an effect asks to be done to the picture of its layer, with its values at a frame. The preview and the export turn each into a call of the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PictureOperation {
    /// The colours are mixed with a matrix (and moved by its offsets), in the codes of a screen. Neighbouring ones are one operation (see [`Self::then_if_alike`]).
    Mix(ColourMatrix),
    /// The light is made brighter or darker by this many stops.
    Exposure { stops: f32 },
    /// The picture is made this much opaque (from 0 to 1) of what it is.
    Opacity { opacity: f32 },
}

impl PictureOperation {
    /// Whether the operation changes no pixel, or none that can be told from the pixel it was: a mix that changes nothing, no exposure, all the opacity. (The same limit
    /// that the engine leaves such a call out by; a test holds the two to each other.)
    pub fn changes_nothing(&self) -> bool {
        match self {
            Self::Mix(matrix) => matrix.changes_nothing(),
            Self::Exposure { stops } => (stops.exp2() - 1.0).abs() < NEAR_ENOUGH_TO_CHANGE_NOTHING,
            Self::Opacity { opacity } => 1.0 - opacity < NEAR_ENOUGH_TO_CHANGE_NOTHING,
        }
    }

    /// The one operation that does what this one does and then what `next` does, if they are alike so that they can be (two colour mixes), and `None` if not.
    #[must_use]
    pub fn then_if_alike(self, next: Self) -> Option<Self> {
        match (self, next) {
            (Self::Mix(first), Self::Mix(second)) => Some(Self::Mix(first.then(second))),
            _ => None,
        }
    }
}

/// What the program does for one of its effects.
struct Behaviour {
    kind: EffectKind,
    /// What it asks to be done to the picture, from its values.
    operation: fn(&[PropertyValue]) -> PictureOperation,
}

const BEHAVIOURS: [Behaviour; 5] = [
    Behaviour {
        kind: EffectKind::HUE_SHIFT,
        operation: hue_shift_operation,
    },
    Behaviour {
        kind: EffectKind::SATURATION,
        operation: saturation_operation,
    },
    Behaviour {
        kind: EffectKind::BRIGHTNESS_AND_CONTRAST,
        operation: brightness_and_contrast_operation,
    },
    Behaviour {
        kind: EffectKind::EXPOSURE,
        operation: exposure_operation,
    },
    Behaviour {
        kind: EffectKind::OPACITY,
        operation: opacity_operation,
    },
];

/// What the effect `kind` asks to be done to a picture when its values are `values`, if it is an effect of the program.
pub(super) fn picture_operation(kind: EffectKind, values: &[PropertyValue]) -> Option<PictureOperation> {
    let operation = BEHAVIOURS.iter().find(|behaviour| behaviour.kind == kind)?.operation;
    Some(operation(values))
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

fn opacity_operation(values: &[PropertyValue]) -> PictureOperation {
    match values {
        [PropertyValue::Number(opacity)] => PictureOperation::Opacity {
            opacity: opacity.clamp(0.0, 1.0),
        },
        _ => PictureOperation::Opacity { opacity: 1.0 },
    }
}
