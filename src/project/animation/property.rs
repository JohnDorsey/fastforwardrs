// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What can be animated, and the values it takes: the properties of clips and of tracks that can change over
//! time, each with the kind of value it has.

use crate::project::effects::{EffectInstanceId, EffectKind, Effects};
use crate::project::tracks::{ClipKind, TextColour};

/// A property that can be animated: given keyframes, so that it is not the same at every frame.
///
/// Everything that is a setting of a clip or of a track can be, except what makes the clip last longer or
/// shorter (its speed, the speed of its track, and how long a picture or a text object is shown for), which
/// would make the keyframes move about in time.
///
/// Most of the properties are the clip's and the track's own. The others are the values of the effects that a clip or a track has, and are
/// not a row each: *the value in place `slot` of the effect `instance`, which is an effect of kind `effect`* is one property,
/// [`Self::EffectValue`] (see [`crate::project::EffectInstanceId`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PropertyId {
    /// How loud a clip or a track is, in decibels.
    Gain,
    /// Whether a clip or a track is silent.
    Muted,
    /// Where the middle of a video, a picture, a text object or a generator clip is on the picture.
    Position,
    /// How wide and tall the box of a video, a picture, a text object or a generator clip is.
    Size,
    /// How far a video, a picture, a text object or a generator clip is turned, in degrees.
    Rotation,
    /// The value in place `slot` of the effect of kind `effect` that is named `instance` in the chain of effects of a clip or a track (and a
    /// property only of what has that effect). The kind of effect is here so that what the property is, what kind of value it has and what
    /// it is called can be said without looking in the chain.
    EffectValue {
        effect: EffectKind,
        instance: EffectInstanceId,
        slot: u8,
    },
}

/// What a property's value is made of, which says how it changes between two keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ValueKind {
    /// One number, which changes evenly.
    Number,
    /// Two numbers (across and down, or wide and tall), which change evenly.
    Pair,
    /// A colour, whose red, green, blue and clearness change evenly.
    Colour,
    /// Yes or no, which stays what it was until the next keyframe.
    Flag,
    /// Words, which stay what they were until the next keyframe.
    Words,
}

impl PropertyId {
    /// Every property that a clip or a track has of its own, in the order they are kept and shown in. (The values of effects, which are
    /// the others, are as many as the effects there are: see [`Effect::properties`](crate::project::Effect::properties).)
    pub const OWN: [PropertyId; 5] = [Self::Gain, Self::Muted, Self::Position, Self::Size, Self::Rotation];

    /// The property that is the value in place `slot` of the effect `instance`, which is of the kind `effect`.
    pub const fn effect_value(effect: EffectKind, instance: EffectInstanceId, slot: u8) -> Self {
        Self::EffectValue { effect, instance, slot }
    }

    /// What the value of the property is made of. (A value of an effect is what the effect says its value in that place is; a place that the
    /// effect has no value in is no property of anything, and is taken for a number.)
    pub fn kind(self) -> ValueKind {
        match self {
            Self::EffectValue { effect, slot, .. } => effect.value_kind(usize::from(slot)).unwrap_or(ValueKind::Number),
            Self::Gain | Self::Rotation => ValueKind::Number,
            Self::Position | Self::Size => ValueKind::Pair,
            Self::Muted => ValueKind::Flag,
        }
    }

    /// What the property is called, in lower case: the name that a clip's or a track's own property is saved under in a project file, and
    /// how a value of an effect is called when it is told apart from the others (the name that its effect says it has).
    pub fn name(self) -> &'static str {
        match self {
            Self::EffectValue { effect, slot, .. } => effect
                .parameters()
                .get(usize::from(slot))
                .map_or("value of an effect", |parameter| &parameter.name),
            Self::Gain => "gain",
            Self::Muted => "muted",
            Self::Position => "position",
            Self::Size => "size",
            Self::Rotation => "rotation",
        }
    }

    /// What the property is called in a project file: what a clip or a track has of its own is called by its name, and the value of an effect by
    /// the id that its effect gives it (a name that does not change, and that is not a place: see
    /// [`ParameterDescriptor::id`](crate::project::ParameterDescriptor::id)).
    pub fn name_in_a_file(self) -> &'static str {
        match self {
            Self::EffectValue { effect, slot, .. } => effect
                .parameters()
                .get(usize::from(slot))
                .map_or("value of an effect", |parameter| &parameter.id),
            other => other.name(),
        }
    }

    /// The property of a clip or a track that is called `name` in a project file. (Only the properties they have of their own have names
    /// there: see [`Self::OWN`].)
    pub fn from_name(name: &str) -> Option<Self> {
        Self::OWN.into_iter().find(|property| property.name() == name)
    }

    /// Whether this is a value of an effect, which is a property only of what has that effect.
    pub fn is_a_value_of_an_effect(self) -> bool {
        matches!(self, Self::EffectValue { .. })
    }

    /// The name, in the chain of what has it, of the effect that this is a value of, if it is the value of an effect.
    pub fn effect_instance(self) -> Option<EffectInstanceId> {
        match self {
            Self::EffectValue { instance, .. } => Some(instance),
            _ => None,
        }
    }

    /// Whether a track can have this property: how loud it is, and whether it is silent, and, if it is a value of an effect, as far as a
    /// track can have effects at all (whether it has the effect is for [`Self::is_had_by_a_track`] to say).
    pub fn applies_to_a_track(self) -> bool {
        match self {
            Self::EffectValue { effect, .. } => effect.applies_to_a_track(),
            _ => self.is_heard(),
        }
    }

    /// Whether what the property is can be heard: how loud something is, and whether it is silent.
    pub fn is_heard(self) -> bool {
        matches!(self, Self::Gain | Self::Muted)
    }

    /// Whether what the property is can be seen in the picture: where something is and how it looks.
    pub fn is_seen(self) -> bool {
        !self.is_heard()
    }

    /// Whether a clip of this kind has this property: a video, a sound or a MIDI clip has its loudness and its mute switch, a
    /// video, a picture, a text object and a generator clip are somewhere on the picture. (What a text object says and how it looks are the values of its text effect.)
    pub fn applies_to_a_clip_of_kind(self, kind: ClipKind) -> bool {
        match self {
            Self::Gain | Self::Muted => matches!(kind, ClipKind::Video | ClipKind::Audio | ClipKind::Midi),
            Self::Position | Self::Size | Self::Rotation => {
                matches!(
                    kind,
                    ClipKind::Video | ClipKind::Image | ClipKind::Text | ClipKind::Generator
                )
            }
            // (A value of an effect is a property of a clip that can have the kind of effect: whether it has this one is for `is_had_by_a_clip`.)
            Self::EffectValue { effect, .. } => effect.applies_to_a_clip_of_kind(kind),
        }
    }

    /// Whether a clip of kind `kind` that has the effects `effects` has this property: one that a clip of its kind can have and, if
    /// it is a value of an effect, only if the clip has that effect, and the effect can be on a clip of that kind.
    pub fn is_had_by_a_clip(self, kind: ClipKind, effects: &Effects) -> bool {
        self.applies_to_a_clip_of_kind(kind)
            && (!self.is_a_value_of_an_effect()
                || effects
                    .effect_of_the_value(self)
                    .is_some_and(|effect| effect.kind().applies_to_a_clip_of_kind(kind)))
    }

    /// Whether a track that has the effects `effects` has this property: one that a track can have and, if it is a value of an
    /// effect, only if the track has that effect, and the effect can be on a track.
    pub fn is_had_by_a_track(self, effects: &Effects) -> bool {
        self.applies_to_a_track()
            && (!self.is_a_value_of_an_effect()
                || effects
                    .effect_of_the_value(self)
                    .is_some_and(|effect| effect.kind().applies_to_a_track()))
    }
}

/// The value of a property at one moment.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Number(f32),
    /// Across and down, or wide and tall, as fractions of the picture.
    Pair(f32, f32),
    Colour(TextColour),
    Flag(bool),
    Words(String),
}

impl PropertyValue {
    /// Feeds the value into `hasher`, so that what it is can be told from another value that it is not equal to. (There is
    /// no `Hash` for values, as there are numbers with fractions in them.)
    pub fn hash_into(&self, hasher: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        match self {
            Self::Number(number) => (0_u8, number.to_bits()).hash(hasher),
            Self::Pair(first, second) => (1_u8, first.to_bits(), second.to_bits()).hash(hasher),
            Self::Colour(colour) => (2_u8, colour).hash(hasher),
            Self::Flag(flag) => (3_u8, flag).hash(hasher),
            Self::Words(words) => (4_u8, words).hash(hasher),
        }
    }

    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Number(_) => ValueKind::Number,
            Self::Pair(..) => ValueKind::Pair,
            Self::Colour(_) => ValueKind::Colour,
            Self::Flag(_) => ValueKind::Flag,
            Self::Words(_) => ValueKind::Words,
        }
    }

    /// The value that is `fraction` (0 to 1) of the way from this one to `other`: what changes evenly does so, and
    /// what cannot (yes or no, words) stays what this value is until `fraction` is all of the way, which is
    /// `other`. (Values of different kinds are not between anything: this one stays.)
    #[must_use]
    pub fn blended_towards(&self, other: &Self, fraction: f32) -> Self {
        let evenly = |from: f32, to: f32| from + (to - from) * fraction;
        match (self, other) {
            (Self::Number(from), Self::Number(to)) => Self::Number(evenly(*from, *to)),
            (Self::Pair(from_first, from_second), Self::Pair(to_first, to_second)) => {
                Self::Pair(evenly(*from_first, *to_first), evenly(*from_second, *to_second))
            }
            (Self::Colour(from), Self::Colour(to)) => {
                let channel = |from: u8, to: u8| evenly(f32::from(from), f32::from(to)).round().clamp(0.0, 255.0) as u8;
                Self::Colour(TextColour::new(
                    channel(from.red, to.red),
                    channel(from.green, to.green),
                    channel(from.blue, to.blue),
                    channel(from.alpha, to.alpha),
                ))
            }
            (Self::Flag(_), Self::Flag(_)) | (Self::Words(_), Self::Words(_)) => {
                if fraction >= 1.0 {
                    other.clone()
                } else {
                    self.clone()
                }
            }
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        COLOUR_OF_A_TEXT, FONT_OF_A_TEXT, HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE, SIZE_OF_A_TEXT, TEXT_OF_A_TEXT,
        properties_of_every_kind,
    };

    #[test]
    fn every_property_has_a_name_that_only_it_has_and_that_it_is_found_by_if_it_is_one_of_a_clip_or_a_track_itself() {
        let mut names: Vec<&str> = properties_of_every_kind()
            .iter()
            .map(|property| property.name())
            .collect();
        for property in PropertyId::OWN {
            assert_eq!(PropertyId::from_name(property.name()), Some(property));
        }
        // (The value of an effect is found by its name only in the chain of the effect it is of: see the project file.)
        assert_eq!(PropertyId::from_name(HUE.name()), None);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), properties_of_every_kind().len());
        assert_eq!(PropertyId::from_name("speed"), None, "speeds are not animated");
        assert_eq!(PropertyId::from_name("duration"), None);
    }

    #[test]
    fn each_kind_of_clip_and_a_track_have_the_properties_they_have() {
        let of = |kind: ClipKind| -> Vec<PropertyId> {
            properties_of_every_kind()
                .into_iter()
                .filter(|property| property.applies_to_a_clip_of_kind(kind))
                .collect()
        };
        use PropertyId::*;
        // (The values of effects are those of what can have effects at all: whether it has the effect is for `is_had_by_a_clip`.)
        assert_eq!(of(ClipKind::Video), vec![Gain, Muted, Position, Size, Rotation, HUE]);
        assert_eq!(of(ClipKind::Audio), vec![Gain, Muted]);
        // (A MIDI clip, phase 13: the volume of what its own effects make, and its mute switch; it is nowhere on the picture.)
        assert_eq!(of(ClipKind::Midi), vec![Gain, Muted]);
        assert_eq!(of(ClipKind::Image), vec![Position, Size, Rotation, HUE]);
        // (Rewritten on purpose in phase 9: what a text says and how it looks are the values of its text effect, and are not properties of the clip, so they are not in this list;
        // a text that has the text effect has them as the values of an effect.)
        assert_eq!(of(ClipKind::Text), vec![Position, Size, Rotation, HUE]);
        assert_eq!(of(ClipKind::Crate), Vec::<PropertyId>::new());
        let of_a_track: Vec<PropertyId> = properties_of_every_kind()
            .into_iter()
            .filter(|property| property.applies_to_a_track())
            .collect();
        assert_eq!(of_a_track, vec![Gain, Muted, HUE]);
    }

    #[test]
    fn the_value_of_an_effect_is_had_by_a_clip_or_a_track_that_has_that_effect_and_by_no_other() {
        let mut with_the_effect = Effects::none();
        with_the_effect.add(EffectKind::HUE_SHIFT);
        let without = Effects::none();
        let another_one = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(5), 0);
        assert_eq!(another_one.kind(), HUE.kind());

        for kind in [ClipKind::Video, ClipKind::Image, ClipKind::Text] {
            assert!(
                HUE.is_had_by_a_clip(kind, &with_the_effect),
                "{kind:?} that has the effect"
            );
            assert!(!HUE.is_had_by_a_clip(kind, &without), "{kind:?} that has not");
            assert!(
                !another_one.is_had_by_a_clip(kind, &with_the_effect),
                "{kind:?}: another effect, not this one"
            );
        }
        for kind in [ClipKind::Audio, ClipKind::Crate, ClipKind::Midi] {
            assert!(!HUE.is_had_by_a_clip(kind, &with_the_effect), "{kind:?} cannot have it");
        }
        assert!(HUE.is_had_by_a_track(&with_the_effect));
        assert!(!HUE.is_had_by_a_track(&without));
        assert!(!another_one.is_had_by_a_track(&with_the_effect));
        // (What a clip or a track has of its own does not depend on its effects.)
        assert!(PropertyId::Gain.is_had_by_a_track(&without));
        assert!(PropertyId::Position.is_had_by_a_clip(ClipKind::Video, &without));
    }

    #[test]
    fn what_a_value_of_an_effect_is_and_is_called_is_what_the_effect_says_of_the_place_it_is_in() {
        assert_eq!(HUE.kind(), ValueKind::Number);
        assert_eq!(HUE.name(), "hue shift");
        assert!(HUE.is_a_value_of_an_effect());
        assert!(HUE.is_seen() && !HUE.is_heard());
        // (A place that the effect has no value in is nothing that an effect makes, and is no more than a number with a name.)
        let nowhere = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::FIRST, 9);
        assert_eq!(nowhere.kind(), ValueKind::Number);
        assert_eq!(nowhere.name(), "value of an effect");
        assert!(
            PropertyId::OWN
                .iter()
                .all(|property| !property.is_a_value_of_an_effect())
        );
    }

    #[test]
    fn the_list_of_properties_is_in_the_order_they_are_kept_in_and_has_each_once_and_the_values_of_effects_come_after()
    {
        let mut sorted = PropertyId::OWN;
        sorted.sort();
        assert_eq!(sorted, PropertyId::OWN);
        sorted.windows(2).for_each(|pair| assert_ne!(pair[0], pair[1]));
        for property in PropertyId::OWN {
            assert!(property < HUE, "{property:?} before the values of effects");
        }
        let later_effect = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(1), 0);
        assert!(
            HUE < later_effect,
            "the values of the effects are in the order the effects are in"
        );
    }

    #[test]
    fn what_changes_evenly_does_and_yes_or_no_and_words_wait_for_the_next_keyframe() {
        let number = PropertyValue::Number(10.0).blended_towards(&PropertyValue::Number(20.0), 0.25);
        assert_eq!(number, PropertyValue::Number(12.5));
        let pair = PropertyValue::Pair(0.0, 1.0).blended_towards(&PropertyValue::Pair(1.0, 0.0), 0.5);
        assert_eq!(pair, PropertyValue::Pair(0.5, 0.5));
        let off = PropertyValue::Flag(false);
        assert_eq!(
            off.blended_towards(&PropertyValue::Flag(true), 0.99),
            PropertyValue::Flag(false)
        );
        assert_eq!(
            off.blended_towards(&PropertyValue::Flag(true), 1.0),
            PropertyValue::Flag(true)
        );
        let hello = PropertyValue::Words("Hello".to_owned());
        let goodbye = PropertyValue::Words("Goodbye".to_owned());
        assert_eq!(hello.blended_towards(&goodbye, 0.5), hello);
        assert_eq!(hello.blended_towards(&goodbye, 1.0), goodbye);
    }

    #[test]
    fn a_colour_changes_in_each_of_its_channels_and_stays_a_colour() {
        let black = PropertyValue::Colour(TextColour::new(0, 0, 0, 255));
        let white = PropertyValue::Colour(TextColour::new(255, 255, 255, 0));
        assert_eq!(
            black.blended_towards(&white, 0.5),
            PropertyValue::Colour(TextColour::new(128, 128, 128, 128))
        );
        assert_eq!(black.blended_towards(&white, 0.0), black);
        assert_eq!(black.blended_towards(&white, 1.0), white);
    }

    #[test]
    fn values_of_different_kinds_are_not_between_anything() {
        let number = PropertyValue::Number(3.0);
        assert_eq!(number.blended_towards(&PropertyValue::Flag(true), 0.5), number);
        assert_eq!(
            number.blended_towards(&PropertyValue::Flag(true), 1.0),
            number,
            "not even all of the way"
        );
    }

    #[test]
    fn every_value_knows_its_kind_and_the_kind_the_property_it_is_for_has() {
        assert_eq!(PropertyValue::Number(0.0).kind(), PropertyId::Gain.kind());
        assert_eq!(PropertyValue::Pair(0.0, 0.0).kind(), PropertyId::Position.kind());
        assert_eq!(PropertyValue::Pair(0.0, 0.0).kind(), PropertyId::Size.kind());
        assert_eq!(PropertyValue::Number(0.0).kind(), PropertyId::Rotation.kind());
        // (The words, the font, the size and the colour of a text are values of its text effect since phase 9, and have the kinds that they had as properties of the clip.)
        assert_eq!(PropertyValue::Number(0.0).kind(), SIZE_OF_A_TEXT.kind());
        assert_eq!(PropertyValue::Flag(true).kind(), PropertyId::Muted.kind());
        assert_eq!(PropertyValue::Words(String::new()).kind(), TEXT_OF_A_TEXT.kind());
        assert_eq!(PropertyValue::Words(String::new()).kind(), FONT_OF_A_TEXT.kind());
        assert_eq!(PropertyValue::Colour(TextColour::WHITE).kind(), COLOUR_OF_A_TEXT.kind());
    }
}
