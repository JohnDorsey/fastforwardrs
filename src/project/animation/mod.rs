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

/// A keyframe that was taken away, and whether it left the property with none.
#[derive(Clone, Debug, PartialEq)]
pub struct RemovedKeyframe {
    pub value: PropertyValue,
    /// Whether the property is not animated any more: it was the last keyframe it had.
    pub was_the_last: bool,
}

/// The keyframes of the properties of one clip or one track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    /// Only properties that have at least one keyframe are here. (In the order of [`PropertyId`], which is the order
    /// they are saved and compared in.)
    properties: BTreeMap<PropertyId, Keyframes>,
}

impl Animation {
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
    pub fn set_keyframe(&mut self, property: PropertyId, frame: u64, value: PropertyValue) -> bool {
        if value.kind() != property.kind() {
            return false;
        }
        self.properties.entry(property).or_default().set(frame, value);
        true
    }

    /// Takes away the keyframe of `property` at `frame`, if there is one.
    pub fn remove_keyframe(&mut self, property: PropertyId, frame: u64) -> Option<RemovedKeyframe> {
        let keyframes = self.properties.get_mut(&property)?;
        let value = keyframes.remove(frame)?;
        let was_the_last = keyframes.is_empty();
        if was_the_last {
            self.properties.remove(&property);
        }
        Some(RemovedKeyframe { value, was_the_last })
    }

    /// The animations of the two parts that what they belong to is cut into `frames_in` frames after its start (more than
    /// nothing): see [`Keyframes::split_at`].
    #[must_use]
    pub fn split_at(&self, frames_in: u64) -> (Animation, Animation) {
        let mut first = Self::default();
        let mut second = Self::default();
        for (property, keyframes) in &self.properties {
            let (before, after) = keyframes.split_at(frames_in);
            first.properties.insert(*property, before);
            second.properties.insert(*property, after);
        }
        (first, second)
    }

    /// The same animation with every keyframe at the frame that `to_frame` says its frame becomes (keyframes that
    /// land on one frame are one: the later is kept).
    #[must_use]
    pub fn mapped_frames(&self, to_frame: impl Fn(u64) -> u64) -> Animation {
        Self::from_keyframes(self.properties.iter().map(|(property, keyframes)| {
            (
                *property,
                Keyframes::from_unordered(keyframes.iter().map(|keyframe| Keyframe {
                    frame: to_frame(keyframe.frame),
                    value: keyframe.value.clone(),
                })),
            )
        }))
    }

    /// The animation of what it belongs to when it is `length` frames long and was longer.
    #[must_use]
    pub fn truncated_to(&self, length: u64) -> Animation {
        self.mapped(|keyframes| keyframes.truncated_to(length))
    }

    /// The animation of what it belongs to when it is `to_length` frames long and was `from_length`: see
    /// [`Keyframes::rescaled`].
    #[must_use]
    pub fn rescaled(&self, from_length: u64, to_length: u64) -> Animation {
        self.mapped(|keyframes| keyframes.rescaled(from_length, to_length))
    }

    fn mapped(&self, change: impl Fn(&Keyframes) -> Keyframes) -> Animation {
        Self::from_keyframes(
            self.properties
                .iter()
                .map(|(property, keyframes)| (*property, change(keyframes))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain(db: f32) -> PropertyValue {
        PropertyValue::Number(db)
    }

    fn animated_gain_and_muted() -> Animation {
        let mut animation = Animation::default();
        assert!(animation.set_keyframe(PropertyId::Gain, 0, gain(0.0)));
        assert!(animation.set_keyframe(PropertyId::Gain, 20, gain(-20.0)));
        assert!(animation.set_keyframe(PropertyId::Muted, 10, PropertyValue::Flag(true)));
        animation
    }

    #[test]
    fn nothing_is_animated_until_a_keyframe_is_set() {
        let mut animation = Animation::default();
        assert!(animation.is_empty());
        assert!(!animation.is_animated(PropertyId::Gain));
        assert_eq!(animation.value_at(PropertyId::Gain, 5), None);
        animation.set_keyframe(PropertyId::Gain, 5, gain(3.0));
        assert!(!animation.is_empty() && animation.is_animated(PropertyId::Gain));
        assert!(!animation.is_animated(PropertyId::Muted), "only what has a keyframe");
        assert_eq!(animation.value_at(PropertyId::Gain, 99), Some(gain(3.0)));
    }

    #[test]
    fn a_keyframe_of_the_wrong_kind_of_value_is_refused_and_changes_nothing() {
        let mut animation = Animation::default();
        assert!(!animation.set_keyframe(PropertyId::Gain, 0, PropertyValue::Flag(true)));
        assert!(!animation.set_keyframe(PropertyId::Position, 0, gain(1.0)));
        assert!(animation.is_empty());
    }

    #[test]
    fn removing_the_last_keyframe_of_a_property_says_so_and_makes_it_not_animated() {
        let mut animation = animated_gain_and_muted();
        assert_eq!(
            animation.remove_keyframe(PropertyId::Gain, 0),
            Some(RemovedKeyframe {
                value: gain(0.0),
                was_the_last: false
            })
        );
        assert!(animation.is_animated(PropertyId::Gain));
        assert_eq!(
            animation.remove_keyframe(PropertyId::Gain, 20),
            Some(RemovedKeyframe {
                value: gain(-20.0),
                was_the_last: true
            })
        );
        assert!(!animation.is_animated(PropertyId::Gain));
        assert!(animation.is_animated(PropertyId::Muted), "the others are as they were");
        assert_eq!(animation.remove_keyframe(PropertyId::Gain, 20), None);
        assert_eq!(animation.remove_keyframe(PropertyId::Rotation, 3), None);
    }

    #[test]
    fn the_properties_are_listed_in_the_order_of_the_table() {
        let animation = animated_gain_and_muted();
        let listed: Vec<PropertyId> = animation.properties().map(|(property, _)| property).collect();
        assert_eq!(listed, vec![PropertyId::Gain, PropertyId::Muted]);
    }

    #[test]
    fn cutting_cuts_every_property_and_both_parts_stay_animated_where_the_whole_was() {
        let (first, second) = animated_gain_and_muted().split_at(15);
        for part in [&first, &second] {
            assert!(part.is_animated(PropertyId::Gain) && part.is_animated(PropertyId::Muted));
        }
        assert_eq!(first.value_at(PropertyId::Gain, 14), Some(gain(-14.0)));
        assert_eq!(second.value_at(PropertyId::Gain, 0), Some(gain(-15.0)));
        assert_eq!(first.value_at(PropertyId::Muted, 14), Some(PropertyValue::Flag(true)));
        assert_eq!(second.value_at(PropertyId::Muted, 0), Some(PropertyValue::Flag(true)));
    }

    #[test]
    fn the_frames_of_the_keyframes_can_be_changed_and_those_that_land_together_are_one() {
        let animation = animated_gain_and_muted();
        let doubled = animation.mapped_frames(|frame| frame * 2);
        assert_eq!(doubled.value_at(PropertyId::Gain, 40), Some(gain(-20.0)));
        assert_eq!(doubled.value_at(PropertyId::Gain, 20), Some(gain(-10.0)));
        assert_eq!(
            doubled
                .keyframes_of(PropertyId::Muted)
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .frame,
            20
        );
        let all_together = animation.mapped_frames(|_| 7);
        assert_eq!(all_together.keyframes_of(PropertyId::Gain).unwrap().len(), 1);
        assert_eq!(
            all_together.value_at(PropertyId::Gain, 7),
            Some(gain(-20.0)),
            "the later is kept"
        );
    }

    #[test]
    fn shortening_and_stretching_apply_to_every_property() {
        let animation = animated_gain_and_muted();
        let shorter = animation.truncated_to(12);
        assert_eq!(shorter.keyframes_of(PropertyId::Gain).unwrap().len(), 2);
        assert_eq!(
            shorter.value_at(PropertyId::Gain, 11),
            animation.value_at(PropertyId::Gain, 11)
        );
        let longer = animation.rescaled(21, 42);
        assert_eq!(
            longer
                .keyframes_of(PropertyId::Muted)
                .unwrap()
                .iter()
                .map(|keyframe| keyframe.frame)
                .collect::<Vec<_>>(),
            vec![20]
        );
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
