// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Animation: properties of clips and of tracks that change over time, by keyframes.
//!
//! A property is animated when it has keyframes: values at some frames, from which it is worked out at every other
//! (see [`Keyframes`]). What it is when it is not animated is what the clip or the track says, as it always was.
//! The keyframes of a clip are in the clip and belong to it: they are counted in frames of the timeline from the clip's
//! first frame, and move with the clip, whatever track it is on. The keyframes of a track are counted from the start of
//! the timeline.
//!
//! Which properties can be animated, and what kind of value each has, is [`PropertyId`] (a table, so that what is
//! animated later is a row more); how the keyframes of a clip are kept, cut, stretched and used is here and in the
//! project; where the keyframes are shown and made is the window's.

mod apply;
mod keyframes;
mod property;

use std::collections::BTreeMap;

pub use apply::{
    PROPERTIES_OF_A_PLACEMENT, PROPERTIES_OF_A_TEXT_OBJECT, placement_value, text_value, with_placement_value,
    with_text_value,
};
pub use keyframes::{Keyframe, Keyframes};
pub use property::{PropertyId, PropertyValue, ValueKind};

/// The keyframes of the properties of one clip or one track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    /// Only properties that have at least one keyframe are here. (In the order of [`PropertyId`], which is the order
    /// they are saved and compared in.)
    properties: BTreeMap<PropertyId, Keyframes>,
}

impl Animation {
    /// An animation of nothing.
    pub const fn none() -> Self {
        Self {
            properties: BTreeMap::new(),
        }
    }

    /// An animation of the properties that `keyframes` are given for (ones without any are left out).
    pub fn from_keyframes(keyframes: impl IntoIterator<Item = (PropertyId, Keyframes)>) -> Self {
        Self {
            properties: keyframes
                .into_iter()
                .filter(|(property, keyframes)| {
                    !keyframes.is_empty()
                        && keyframes
                            .iter()
                            .all(|keyframe| keyframe.value.kind() == property.kind())
                })
                .collect(),
        }
    }

    /// Whether nothing is animated.
    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    pub fn is_animated(&self, property: PropertyId) -> bool {
        self.properties.contains_key(&property)
    }

    /// The keyframes of `property`, if it is animated.
    pub fn keyframes_of(&self, property: PropertyId) -> Option<&Keyframes> {
        self.properties.get(&property)
    }

    /// The properties that are animated with their keyframes, in the order of [`PropertyId`].
    pub fn properties(&self) -> impl Iterator<Item = (PropertyId, &Keyframes)> {
        self.properties
            .iter()
            .map(|(property, keyframes)| (*property, keyframes))
    }

    /// Feeds what the animation says about the properties that `keep` says yes to into `hasher`, so that a number made from
    /// it is different whenever that is. (The animation itself has no `Hash`: its values have fractions in them.)
    pub fn hash_properties_into(&self, hasher: &mut impl std::hash::Hasher, keep: impl Fn(PropertyId) -> bool) {
        use std::hash::Hash;
        for (property, keyframes) in self.properties().filter(|(property, _)| keep(*property)) {
            property.hash(hasher);
            for keyframe in keyframes.iter() {
                keyframe.frame.hash(hasher);
                keyframe.value.hash_into(hasher);
            }
        }
    }

    /// The same animation of only the properties that `keep` says yes to.
    #[must_use]
    pub fn restricted_to(&self, keep: impl Fn(PropertyId) -> bool) -> Animation {
        Self {
            properties: self
                .properties
                .iter()
                .filter(|(property, _)| keep(**property))
                .map(|(property, keyframes)| (*property, keyframes.clone()))
                .collect(),
        }
    }

    /// The value `property` has at `frame` if it is animated.
    pub fn value_at(&self, property: PropertyId, frame: u64) -> Option<PropertyValue> {
        self.properties.get(&property)?.value_at(frame)
    }

    /// Puts a keyframe with `value` for `property` at `frame`, in place of the one there if there is one. Returns
    /// false, and changes nothing, if `value` is not the kind of value the property has.
    #[cfg(test)]
    pub fn set_keyframe(&mut self, property: PropertyId, frame: u64, value: PropertyValue) -> bool {
        if value.kind() != property.kind() {
            return false;
        }
        self.properties.entry(property).or_default().set(frame, value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain(db: f32) -> PropertyValue {
        PropertyValue::Number(db)
    }

    fn animated_gain_and_muted() -> Animation {
        let keyframe = |frame, value| Keyframe { frame, value };
        Animation::from_keyframes([
            (
                PropertyId::Gain,
                Keyframes::from_unordered([keyframe(0, gain(0.0)), keyframe(20, gain(-20.0))]),
            ),
            (
                PropertyId::Muted,
                Keyframes::from_unordered([keyframe(10, PropertyValue::Flag(true))]),
            ),
        ])
    }

    #[test]
    fn what_has_no_keyframes_is_not_animated_and_what_has_some_says_what_it_is_at_every_frame() {
        let nothing = Animation::default();
        assert!(nothing.is_empty());
        assert!(!nothing.is_animated(PropertyId::Gain));
        assert_eq!(nothing.value_at(PropertyId::Gain, 5), None);

        let animation = animated_gain_and_muted();
        assert!(!animation.is_empty() && animation.is_animated(PropertyId::Gain));
        assert!(!animation.is_animated(PropertyId::Rotation), "only what has a keyframe");
        assert_eq!(animation.value_at(PropertyId::Gain, 10), Some(gain(-10.0)));
        assert_eq!(animation.value_at(PropertyId::Gain, 99), Some(gain(-20.0)));
        assert_eq!(animation.value_at(PropertyId::Rotation, 10), None);
    }

    #[test]
    fn the_properties_are_listed_in_the_order_of_the_table() {
        let animation = animated_gain_and_muted();
        let listed: Vec<PropertyId> = animation.properties().map(|(property, _)| property).collect();
        assert_eq!(listed, vec![PropertyId::Gain, PropertyId::Muted]);
    }

    #[test]
    fn an_animation_made_from_keyframes_leaves_out_properties_with_none_and_values_of_the_wrong_kind() {
        let mut flags = Keyframes::default();
        flags.set(0, PropertyValue::Flag(true));
        let mut wrong = Keyframes::default();
        wrong.set(0, PropertyValue::Flag(true));
        let animation = Animation::from_keyframes([
            (PropertyId::Muted, flags),
            (PropertyId::Gain, Keyframes::default()),
            (PropertyId::Rotation, wrong),
        ]);
        assert!(animation.is_animated(PropertyId::Muted));
        assert!(!animation.is_animated(PropertyId::Gain));
        assert!(!animation.is_animated(PropertyId::Rotation));
    }
}
