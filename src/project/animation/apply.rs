// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the properties are for a clip or a track, one at a time: how each is read out of what a clip says, and
//! written into it, and how an [`Animation`] gives the values that a clip has at a frame.
//!
//! The properties of where something is on the picture are three: where its middle is, how wide and tall it is, and
//! how far it is turned. (A place on the picture is what a [`Placement`] says all together: an area, and a turn.)

use super::{Animation, PropertyId, PropertyValue};
use crate::project::tracks::{Angle, CanvasArea, FontSize, Gain, Placement, TextObject};

/// The properties of where something is on the picture: those that [`placement_value`] has a value for.
pub const PROPERTIES_OF_A_PLACEMENT: [PropertyId; 3] = [PropertyId::Position, PropertyId::Size, PropertyId::Rotation];

/// The properties of a text object, which include where it is on the picture: those that [`text_value`] has a value for.
pub const PROPERTIES_OF_A_TEXT_OBJECT: [PropertyId; 7] = [
    PropertyId::Text,
    PropertyId::FontFamily,
    PropertyId::FontSize,
    PropertyId::Colour,
    PropertyId::Position,
    PropertyId::Size,
    PropertyId::Rotation,
];

/// The value of the placement-related `property` of `placement`: where its middle is (as fractions of the picture), how
/// big its area is, or the degrees it is turned. `None` for any other property.
pub fn placement_value(placement: &Placement, property: PropertyId) -> Option<PropertyValue> {
    let area = placement.area;
    match property {
        PropertyId::Position => Some(PropertyValue::Pair(
            area.left() + area.width() / 2.0,
            area.top() + area.height() / 2.0,
        )),
        PropertyId::Size => Some(PropertyValue::Pair(area.width(), area.height())),
        PropertyId::Rotation => Some(PropertyValue::Number(placement.rotation.degrees())),
        _ => None,
    }
}

/// The placement `placement` made different as `value` says for `property`: its middle moved (the size is kept), its
/// size changed (the middle is kept), or the turn changed. What does not fit the picture is corrected as always (see
/// [`CanvasArea::new`]). Anything else, and a value that is not the kind the property has, leaves the placement alone.
#[must_use]
pub fn with_placement_value(placement: Placement, property: PropertyId, value: &PropertyValue) -> Placement {
    let area = placement.area;
    match (property, value) {
        (PropertyId::Position, PropertyValue::Pair(across, down)) => Placement {
            area: CanvasArea::new(
                across - area.width() / 2.0,
                down - area.height() / 2.0,
                area.width(),
                area.height(),
            ),
            ..placement
        },
        (PropertyId::Size, PropertyValue::Pair(width, height)) => {
            let (across, down) = (area.left() + area.width() / 2.0, area.top() + area.height() / 2.0);
            Placement {
                area: CanvasArea::new(across - width / 2.0, down - height / 2.0, *width, *height),
                ..placement
            }
        }
        (PropertyId::Rotation, PropertyValue::Number(degrees)) => Placement {
            rotation: Angle::from_degrees(*degrees),
            ..placement
        },
        _ => placement,
    }
}

/// The value of the text-related `property` of `object`. `None` for any other property.
pub fn text_value(object: &TextObject, property: PropertyId) -> Option<PropertyValue> {
    match property {
        PropertyId::Text => Some(PropertyValue::Words(object.text.clone())),
        PropertyId::FontFamily => Some(PropertyValue::Words(object.font_family.clone())),
        PropertyId::FontSize => Some(PropertyValue::Number(object.font_size.fraction_of_picture_height())),
        PropertyId::Colour => Some(PropertyValue::Colour(object.colour)),
        PropertyId::Position | PropertyId::Size | PropertyId::Rotation => {
            placement_value(&object.placement(), property)
        }
        PropertyId::Gain | PropertyId::Muted | PropertyId::EffectValue { .. } => None,
    }
}

/// The text object `object` made different as `value` says for `property`. (The properties of where it is on the picture
/// are those of its placement.) Anything else, and a value of the wrong kind, leaves it alone.
#[must_use]
pub fn with_text_value(object: &TextObject, property: PropertyId, value: &PropertyValue) -> TextObject {
    match (property, value) {
        (PropertyId::Text, PropertyValue::Words(text)) => TextObject {
            text: text.clone(),
            ..object.clone()
        },
        (PropertyId::FontFamily, PropertyValue::Words(family)) => TextObject {
            font_family: family.clone(),
            ..object.clone()
        },
        (PropertyId::FontSize, PropertyValue::Number(fraction)) => TextObject {
            font_size: FontSize::from_fraction_of_picture_height(*fraction),
            ..object.clone()
        },
        (PropertyId::Colour, PropertyValue::Colour(colour)) => TextObject {
            colour: *colour,
            ..object.clone()
        },
        (PropertyId::Position | PropertyId::Size | PropertyId::Rotation, _) => {
            object.with_placement(with_placement_value(object.placement(), property, value))
        }
        _ => object.clone(),
    }
}

impl Animation {
    /// `placement` as it is at `frame` of what the animation belongs to: where its middle is, how big it is and how far it
    /// is turned, each as the animation says if it is animated, and as `placement` says if it is not.
    #[must_use]
    pub fn placement_at(&self, placement: Placement, frame: u64) -> Placement {
        PROPERTIES_OF_A_PLACEMENT
            .into_iter()
            .fold(placement, |placement, property| match self.value_at(property, frame) {
                Some(value) => with_placement_value(placement, property, &value),
                None => placement,
            })
    }

    /// `object` as it is at `frame` of what the animation belongs to: every one of its properties as the animation says if
    /// it is animated, and as `object` says if it is not.
    #[must_use]
    pub fn text_object_at(&self, object: &TextObject, frame: u64) -> TextObject {
        PROPERTIES_OF_A_TEXT_OBJECT
            .into_iter()
            .filter(|property| self.is_animated(*property))
            .fold(object.clone(), |object, property| {
                match self.value_at(property, frame) {
                    Some(value) => with_text_value(&object, property, &value),
                    None => object,
                }
            })
    }

    /// How loud something is at `frame`: as the animation says if it is animated, and `gain` if not.
    pub fn gain_at(&self, gain: Gain, frame: u64) -> Gain {
        match self.value_at(PropertyId::Gain, frame) {
            Some(PropertyValue::Number(decibels)) => Gain::from_decibels(decibels),
            _ => gain,
        }
    }

    /// Whether something is silent at `frame`: as the animation says if it is animated, and `muted` if not.
    pub fn is_muted_at(&self, muted: bool, frame: u64) -> bool {
        match self.value_at(PropertyId::Muted, frame) {
            Some(PropertyValue::Flag(is_muted)) => is_muted,
            _ => muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::tracks::TextColour;

    fn somewhere() -> Placement {
        Placement::new(CanvasArea::new(0.1, 0.2, 0.4, 0.3), Angle::from_degrees(25.0))
    }

    fn close(first: f32, second: f32) -> bool {
        (first - second).abs() < 1e-5
    }

    #[test]
    fn a_placement_says_where_its_middle_is_how_big_it_is_and_how_far_it_is_turned() {
        let placement = somewhere();
        let PropertyValue::Pair(across, down) = placement_value(&placement, PropertyId::Position).unwrap() else {
            panic!("a position is two numbers");
        };
        assert!(close(across, 0.3) && close(down, 0.35));
        assert_eq!(
            placement_value(&placement, PropertyId::Size),
            Some(PropertyValue::Pair(0.4, 0.3))
        );
        assert_eq!(
            placement_value(&placement, PropertyId::Rotation),
            Some(PropertyValue::Number(25.0))
        );
        assert_eq!(placement_value(&placement, PropertyId::Gain), None);
        assert_eq!(placement_value(&placement, PropertyId::Text), None);
    }

    /// What keeps the two lists of properties honest: each is exactly the properties that its `..._value` has a value for. A
    /// property that is added to a placement or a text object and not put in its list (or put in a list and given no value) is
    /// found here, and not as a control that changes nothing while the property is animated.
    #[test]
    fn the_lists_of_the_properties_of_a_placement_and_of_a_text_object_are_those_that_have_a_value_in_one() {
        let placement = somewhere();
        let object = TextObject::default();
        for property in crate::test_support::properties_of_every_kind() {
            assert_eq!(
                placement_value(&placement, property).is_some(),
                PROPERTIES_OF_A_PLACEMENT.contains(&property),
                "{property:?} and a placement"
            );
            assert_eq!(
                text_value(&object, property).is_some(),
                PROPERTIES_OF_A_TEXT_OBJECT.contains(&property),
                "{property:?} and a text object"
            );
        }
    }

    /// What the window changes is a loudness or a mute switch, a place on the picture, a text object or a value of an effect, and a
    /// change of one of them while it is animated is made a keyframe by the one thing that knows how. A property that is none of the
    /// four would be changed by nothing.
    #[test]
    fn every_property_is_a_loudness_or_a_mute_switch_or_one_of_a_placement_or_of_a_text_object_or_of_an_effect() {
        for property in crate::test_support::properties_of_every_kind() {
            assert!(
                property.is_heard()
                    || PROPERTIES_OF_A_PLACEMENT.contains(&property)
                    || PROPERTIES_OF_A_TEXT_OBJECT.contains(&property)
                    || property.is_a_value_of_an_effect(),
                "{property:?} is not changed by anything the window does"
            );
        }
    }

    #[test]
    fn a_placement_made_to_have_the_value_it_already_has_is_the_same_placement() {
        let placement = somewhere();
        for property in PROPERTIES_OF_A_PLACEMENT {
            let value = placement_value(&placement, property).unwrap();
            let again = with_placement_value(placement, property, &value);
            let (first, second) = (placement.area, again.area);
            assert!(
                close(first.left(), second.left())
                    && close(first.top(), second.top())
                    && close(first.width(), second.width())
                    && close(first.height(), second.height())
                    && close(placement.rotation.degrees(), again.rotation.degrees()),
                "{property:?}: {placement:?} became {again:?}"
            );
        }
    }

    #[test]
    fn moving_the_middle_keeps_the_size_and_the_turn_and_resizing_keeps_the_middle_and_the_turn() {
        let moved = with_placement_value(somewhere(), PropertyId::Position, &PropertyValue::Pair(0.6, 0.5));
        assert!(close(moved.area.width(), 0.4) && close(moved.area.height(), 0.3));
        assert!(close(moved.area.left() + 0.2, 0.6) && close(moved.area.top() + 0.15, 0.5));
        assert_eq!(moved.rotation, somewhere().rotation);

        let resized = with_placement_value(somewhere(), PropertyId::Size, &PropertyValue::Pair(0.2, 0.1));
        assert!(
            close(resized.area.left() + 0.1, 0.3) && close(resized.area.top() + 0.05, 0.35),
            "{resized:?}"
        );
        assert_eq!(resized.rotation, somewhere().rotation);

        let turned = with_placement_value(somewhere(), PropertyId::Rotation, &PropertyValue::Number(-90.0));
        assert_eq!(turned.area, somewhere().area);
        assert_eq!(turned.rotation.degrees(), -90.0);
    }

    #[test]
    fn a_turn_of_more_than_half_a_turn_is_kept_as_it_was_given() {
        for degrees in [270.0, -540.0, 720.0, 1080.5] {
            let turned = with_placement_value(somewhere(), PropertyId::Rotation, &PropertyValue::Number(degrees));
            assert_eq!(turned.rotation.degrees(), degrees);
            assert_eq!(
                placement_value(&turned, PropertyId::Rotation),
                Some(PropertyValue::Number(degrees)),
                "and it is what the placement says it is"
            );
        }
    }

    #[test]
    fn a_value_of_the_wrong_kind_or_for_another_property_changes_nothing() {
        let placement = somewhere();
        assert_eq!(
            with_placement_value(placement, PropertyId::Position, &PropertyValue::Number(1.0)),
            placement
        );
        assert_eq!(
            with_placement_value(placement, PropertyId::Gain, &PropertyValue::Number(1.0)),
            placement
        );
    }

    #[test]
    fn a_text_object_has_its_words_font_size_colour_and_place_as_properties() {
        let object = TextObject {
            text: "Hello".to_owned(),
            font_family: "Arial".to_owned(),
            font_size: FontSize::from_fraction_of_picture_height(0.1),
            colour: TextColour::new(1, 2, 3, 4),
            ..TextObject::default()
        };
        assert_eq!(
            text_value(&object, PropertyId::Text),
            Some(PropertyValue::Words("Hello".to_owned()))
        );
        assert_eq!(
            text_value(&object, PropertyId::FontFamily),
            Some(PropertyValue::Words("Arial".to_owned()))
        );
        assert_eq!(
            text_value(&object, PropertyId::FontSize),
            Some(PropertyValue::Number(0.1))
        );
        assert_eq!(
            text_value(&object, PropertyId::Colour),
            Some(PropertyValue::Colour(TextColour::new(1, 2, 3, 4)))
        );
        assert_eq!(
            text_value(&object, PropertyId::Rotation),
            Some(PropertyValue::Number(object.rotation.degrees()))
        );
        assert_eq!(text_value(&object, PropertyId::Gain), None);
    }

    #[test]
    fn a_text_object_made_to_have_the_values_it_has_is_the_same_object() {
        let object = TextObject {
            text: "Hello".to_owned(),
            font_family: "Arial".to_owned(),
            colour: TextColour::new(1, 2, 3, 4),
            rotation: Angle::from_degrees(12.0),
            ..TextObject::default()
        };
        for property in crate::test_support::properties_of_every_kind() {
            let Some(value) = text_value(&object, property) else {
                continue;
            };
            let again = with_text_value(&object, property, &value);
            assert_eq!(again.text, object.text);
            assert_eq!(again.font_family, object.font_family);
            assert_eq!(again.colour, object.colour);
            assert!(close(
                again.font_size.fraction_of_picture_height(),
                object.font_size.fraction_of_picture_height()
            ));
            assert!(
                close(again.rotation.degrees(), object.rotation.degrees()),
                "{property:?}"
            );
        }
    }

    #[test]
    fn what_is_not_animated_is_what_it_was_and_what_is_animated_is_what_the_keyframes_say_at_the_frame() {
        let mut animation = Animation::default();
        animation.set_keyframe(PropertyId::Rotation, 0, PropertyValue::Number(0.0));
        animation.set_keyframe(PropertyId::Rotation, 10, PropertyValue::Number(100.0));
        animation.set_keyframe(PropertyId::Position, 10, PropertyValue::Pair(0.9, 0.9));
        let placement = somewhere();

        let at_five = animation.placement_at(placement, 5);
        assert!(close(at_five.rotation.degrees(), 50.0));
        assert!(
            close(at_five.area.left() + at_five.area.width() / 2.0, 0.9),
            "one keyframe: that, everywhere"
        );
        assert!(
            close(at_five.area.width(), placement.area.width()),
            "the size is not animated"
        );

        let nothing = Animation::default();
        assert_eq!(nothing.placement_at(placement, 5), placement);
    }

    #[test]
    fn the_text_of_a_text_object_animated_changes_at_its_keyframes_and_what_is_not_animated_stays() {
        let mut animation = Animation::default();
        animation.set_keyframe(PropertyId::Text, 0, PropertyValue::Words("One".to_owned()));
        animation.set_keyframe(PropertyId::Text, 20, PropertyValue::Words("Two".to_owned()));
        animation.set_keyframe(PropertyId::FontSize, 0, PropertyValue::Number(0.1));
        animation.set_keyframe(PropertyId::FontSize, 20, PropertyValue::Number(0.3));
        let object = TextObject {
            colour: TextColour::new(9, 9, 9, 255),
            ..TextObject::default()
        };

        let at_10 = animation.text_object_at(&object, 10);
        assert_eq!(at_10.text, "One");
        assert!(close(at_10.font_size.fraction_of_picture_height(), 0.2));
        assert_eq!(at_10.colour, TextColour::new(9, 9, 9, 255));
        assert_eq!(animation.text_object_at(&object, 20).text, "Two");
    }

    #[test]
    fn gain_and_muting_are_what_the_animation_says_when_it_says_anything_and_otherwise_what_was_given() {
        let mut animation = Animation::default();
        assert_eq!(
            animation.gain_at(Gain::from_decibels(-3.0), 5),
            Gain::from_decibels(-3.0)
        );
        assert!(animation.is_muted_at(true, 5));
        animation.set_keyframe(PropertyId::Gain, 0, PropertyValue::Number(0.0));
        animation.set_keyframe(PropertyId::Gain, 10, PropertyValue::Number(-20.0));
        animation.set_keyframe(PropertyId::Muted, 5, PropertyValue::Flag(false));
        assert!(close(animation.gain_at(Gain::from_decibels(-3.0), 5).decibels(), -10.0));
        assert!(
            !animation.is_muted_at(true, 0),
            "animated, so what the keyframes say, not what was given"
        );
    }
}
