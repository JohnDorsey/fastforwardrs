//! Effects: what is done to the picture of a clip, or of a track, on its way to the finished picture.
//!
//! A clip or a track has the effects that were put on it, each at most once, one after another in the order they were put on in.
//! What an effect does is set by its *values*, which are properties like any other (see [`PropertyId`]): each can be animated,
//! with keyframes, and is changed in the properties panel with the same stopwatch. An effect that is put on a clip or a track
//! has the values it begins with (see [`EffectKind::default_values`]), and is taken off with its keyframes.
//!
//! What there is to know about each *kind* of effect is a row of one table ([`EffectKind::ALL`] is its rows, in the order
//! they are offered in): its name, what it does, its values, what it can be put on, and how it changes a picture. A new effect is
//! a new row, and the code that is the same for all of them (putting them on, taking them off, saving them, the panel that
//! has them, the preview, the export) is not changed. (So far there is one row, and the ways that an effect can change a picture
//! are the ways of changing its colours; sound, and what is seen of other pictures, come with the effects that need them.)

use std::fmt;

use super::animation::{PropertyId, PropertyValue};
use super::tracks::ClipKind;
use crate::colour_effects::ColourMatrix;

/// The kinds of effect there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EffectKind {
    /// The hues of the colours of the picture are turned round the wheel of colours, as far as the effect says.
    HueShift,
}

/// What a kind of effect is: one row of the table.
struct EffectRow {
    kind: EffectKind,
    /// The name it is saved under in a project file, and called by to the people who use it (with a capital letter).
    name: &'static str,
    /// What it does, in a few words, for the menu that it is chosen from.
    description: &'static str,
    /// The values it has, each a property, in the order they are kept and shown in.
    properties: &'static [PropertyId],
    /// What its values are when it is put on, in the same order.
    default_values: fn() -> Vec<PropertyValue>,
    /// Whether it can be put on a video, a picture and a text object (what is seen), and on a track (everything seen on it).
    on_what_is_seen: bool,
    on_a_track: bool,
    /// How it changes the colours of a picture, if it does, from its values.
    colour_matrix: Option<fn(&[PropertyValue]) -> ColourMatrix>,
    /// Which values the export uses in place of the values the effect has at a frame: what it has, as near as is worth telling
    /// apart (a change that nobody can see is not a stretch of its own in the export, which is slow to make: see
    /// [`EffectKind::as_exported`]).
    as_exported: fn(&[PropertyValue]) -> Vec<PropertyValue>,
}

const ROWS: [EffectRow; 1] = [EffectRow {
    kind: EffectKind::HueShift,
    name: "hue shift",
    description: "Turns the colours round the wheel of colours",
    properties: &[PropertyId::HueShift],
    default_values: hue_shift_defaults,
    on_what_is_seen: true,
    on_a_track: true,
    colour_matrix: Some(hue_shift_matrix),
    as_exported: hue_shift_as_exported,
}];

/// The steps, in degrees, that the hue shift is exported in: a turn of the colours that is a degree off is not seen.
const HUE_STEP_IN_DEGREES: f32 = 1.0;

/// The hue shift is exported in whole steps of [`HUE_STEP_IN_DEGREES`]. (An animated hue shift that is slow then makes a stretch
/// of the export of its own for every few frames, and not for every frame.)
fn hue_shift_as_exported(values: &[PropertyValue]) -> Vec<PropertyValue> {
    match values {
        [PropertyValue::Number(degrees)] => {
            vec![PropertyValue::Number(
                (degrees / HUE_STEP_IN_DEGREES).round() * HUE_STEP_IN_DEGREES,
            )]
        }
        other => other.to_vec(),
    }
}

/// The one value of the hue shift, how far it turns the colours, is nothing to begin with.
fn hue_shift_defaults() -> Vec<PropertyValue> {
    vec![PropertyValue::Number(0.0)]
}

/// How the hue shift changes colours: it turns them by the degrees that its one value says.
fn hue_shift_matrix(values: &[PropertyValue]) -> ColourMatrix {
    match values {
        [PropertyValue::Number(degrees)] => ColourMatrix::hue_rotation(*degrees),
        _ => ColourMatrix::NOTHING_CHANGES,
    }
}

impl fmt::Display for EffectKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl EffectKind {
    /// Every kind of effect, in the order they are offered in.
    pub const ALL: [EffectKind; 1] = [Self::HueShift];

    fn row(self) -> &'static EffectRow {
        ROWS.iter()
            .find(|row| row.kind == self)
            .expect("every kind of effect has a row in the table")
    }

    /// The name of the kind of effect in a project file, which is what it is called, in lower case.
    pub fn name(self) -> &'static str {
        self.row().name
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// What the effect does, in a few words.
    pub fn description(self) -> &'static str {
        self.row().description
    }

    /// The properties that are the values of the effect, in order.
    pub fn properties(self) -> &'static [PropertyId] {
        self.row().properties
    }

    /// What the values of the effect are when it is put on something, one for each of its properties.
    pub fn default_values(self) -> Vec<PropertyValue> {
        (self.row().default_values)()
    }

    /// Whether a clip of this kind can have the effect.
    pub fn applies_to_a_clip_of_kind(self, kind: ClipKind) -> bool {
        self.row().on_what_is_seen && matches!(kind, ClipKind::Video | ClipKind::Image | ClipKind::Text)
    }

    /// Whether a track can have the effect.
    pub fn applies_to_a_track(self) -> bool {
        self.row().on_a_track
    }

    /// The values that are exported when the values of the effect at a frame are `values` (the same values of the same kinds): the
    /// values as near as is worth telling apart, which is `values` themselves for an effect whose values are all that there is to
    /// see of them. What is the same when it is exported is exported as one thing, which makes the export quicker to get ready.
    pub fn as_exported(self, values: &[PropertyValue]) -> Vec<PropertyValue> {
        (self.row().as_exported)(values)
    }

    /// How the effect changes the colours of a picture when its values are `values` (one for each of its properties, in the
    /// order of [`Self::properties`]), if it changes colours at all. A matrix that changes nothing if the values are not those
    /// of the effect.
    pub fn colour_matrix(self, values: &[PropertyValue]) -> Option<ColourMatrix> {
        self.row().colour_matrix.map(|matrix| matrix(values))
    }
}

/// An effect that a clip or a track has: which, and what its values are when they are not animated.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    kind: EffectKind,
    /// The value of each of the properties of the kind of effect, in the order of [`EffectKind::properties`].
    values: Vec<PropertyValue>,
}

impl Effect {
    /// The effect of this kind as it is when it is put on.
    pub fn new(kind: EffectKind) -> Self {
        Self {
            kind,
            values: kind.default_values(),
        }
    }

    /// The effect of this kind with these values, if there is one for each of its properties and each is the kind of value the
    /// property has.
    pub fn with_values(kind: EffectKind, values: Vec<PropertyValue>) -> Option<Self> {
        let right_values = values.len() == kind.properties().len()
            && values
                .iter()
                .zip(kind.properties())
                .all(|(value, property)| value.kind() == property.kind());
        right_values.then_some(Self { kind, values })
    }

    pub fn kind(&self) -> EffectKind {
        self.kind
    }

    /// The values the effect has when they are not animated, one for each of its properties, in order.
    pub fn values(&self) -> &[PropertyValue] {
        &self.values
    }

    /// The value of `property` when it is not animated, if it is one of the effect's.
    pub fn value_of(&self, property: PropertyId) -> Option<&PropertyValue> {
        let place = self.kind.properties().iter().position(|own| *own == property)?;
        self.values.get(place)
    }

    /// The effect with `value` for `property`. `None` if it is not a property of the effect, or not the kind of value that it has.
    #[must_use]
    pub fn with_value(&self, property: PropertyId, value: &PropertyValue) -> Option<Self> {
        let place = self.kind.properties().iter().position(|own| *own == property)?;
        (value.kind() == property.kind()).then(|| {
            let mut values = self.values.clone();
            values[place] = value.clone();
            Self {
                kind: self.kind,
                values,
            }
        })
    }
}

/// The effects of a clip or of a track, in the order they were put on in. There is at most one of each kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    effects: Vec<Effect>,
}

impl Effects {
    /// No effects.
    pub fn none() -> Self {
        Self::default()
    }

    /// Puts `effect`, with the values it has, on, last. `false`, and nothing is changed, if there is one of that kind already.
    pub fn put_on(&mut self, effect: Effect) -> bool {
        if self.contains(effect.kind()) {
            return false;
        }
        self.effects.push(effect);
        true
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// The effects, the first that was put on first.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Effect> {
        self.effects.iter()
    }

    pub fn contains(&self, kind: EffectKind) -> bool {
        self.get(kind).is_some()
    }

    pub fn get(&self, kind: EffectKind) -> Option<&Effect> {
        self.effects.iter().find(|effect| effect.kind == kind)
    }

    /// Puts the effect of `kind` on, last, with the values it begins with. `false`, and nothing is changed, if there is one of
    /// that kind already.
    pub fn add(&mut self, kind: EffectKind) -> bool {
        self.put_on(Effect::new(kind))
    }

    /// Takes the effect of `kind` off. `false` if there was none.
    pub fn remove(&mut self, kind: EffectKind) -> bool {
        let before = self.effects.len();
        self.effects.retain(|effect| effect.kind != kind);
        self.effects.len() != before
    }

    /// The value of `property` when it is not animated, if it is a property of one of the effects.
    pub fn value_of(&self, property: PropertyId) -> Option<PropertyValue> {
        self.effects
            .iter()
            .find_map(|effect| effect.value_of(property))
            .cloned()
    }

    /// Gives `property`, which must be one of an effect that is here, `value`. `false` if it is not, or is not that kind of value.
    pub fn set_value(&mut self, property: PropertyId, value: &PropertyValue) -> bool {
        for effect in &mut self.effects {
            if let Some(changed) = effect.with_value(property, value) {
                *effect = changed;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
