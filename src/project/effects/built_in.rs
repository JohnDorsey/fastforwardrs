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

/// What the program does for one of its effects.
struct Behaviour {
    kind: EffectKind,
    /// How it changes the colours of a picture, if it does, from its values.
    colour_matrix: Option<fn(&[PropertyValue]) -> ColourMatrix>,
}

const BEHAVIOURS: [Behaviour; 1] = [Behaviour {
    kind: EffectKind::HUE_SHIFT,
    colour_matrix: Some(hue_shift_matrix),
}];

/// How the effect `kind` changes the colours of a picture when its values are `values`, if it is an effect of the program and changes
/// colours at all.
pub(super) fn colour_matrix(kind: EffectKind, values: &[PropertyValue]) -> Option<ColourMatrix> {
    let matrix = BEHAVIOURS
        .iter()
        .find(|behaviour| behaviour.kind == kind)?
        .colour_matrix?;
    Some(matrix(values))
}

/// How the hue shift changes colours: it turns them by the degrees that its one value says.
fn hue_shift_matrix(values: &[PropertyValue]) -> ColourMatrix {
    match values {
        [PropertyValue::Number(degrees)] => ColourMatrix::hue_rotation(*degrees),
        _ => ColourMatrix::NOTHING_CHANGES,
    }
}
