// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the properties are for a clip or a track, one at a time: how each is read out of what a clip says, and
//! written into it, and how an [`Animation`] gives the values that a clip has at a frame.
//!
//! The properties of where something is on the picture are three: where its middle is, how wide and tall it is, and
//! how far it is turned. (A place on the picture is what a [`Placement`] says all together: an area, and a turn.)

use super::{Animation, PropertyId, PropertyValue};
use crate::project::tracks::{Angle, CanvasArea, FontSize, Gain, Placement, TextObject};
use crate::project::{EffectInstanceId, EffectKind};

/// The properties of where something is on the picture: those that [`placement_value`] has a value for.
pub const PROPERTIES_OF_A_PLACEMENT: [PropertyId; 3] = [PropertyId::Position, PropertyId::Size, PropertyId::Rotation];

/// The properties of a text object that are the values of the text effect that makes it, which is named `content` in the chain of the text clip: its words, its font, its size and its
/// colour, in the order that [`values_of_a_text_object`] has them. (Where it is on the picture is the placement's: [`PROPERTIES_OF_A_PLACEMENT`].)
pub const fn properties_of_the_text_effect(content: EffectInstanceId) -> [PropertyId; 4] {
    [
        PropertyId::effect_value(EffectKind::TEXT, content, 0),
        PropertyId::effect_value(EffectKind::TEXT, content, 1),
        PropertyId::effect_value(EffectKind::TEXT, content, 2),
        PropertyId::effect_value(EffectKind::TEXT, content, 3),
    ]
}

/// The properties of a text object, which include where it is on the picture: those of the text effect that is named `content`, and the three of a placement.
pub const fn properties_of_a_text_object(content: EffectInstanceId) -> [PropertyId; 7] {
    let text = properties_of_the_text_effect(content);
    [
        text[0],
        text[1],
        text[2],
        text[3],
        PropertyId::Position,
        PropertyId::Size,
        PropertyId::Rotation,
    ]
}

/// The words that the text clip `text` says when its effects are `effects`, as they are when they are not animated: the first value of its text effect (nothing if the clip has none).
pub fn words_of_a_text_clip(text: &crate::project::tracks::TextClip, effects: &crate::project::Effects) -> String {
    match effects.value_of(properties_of_the_text_effect(text.content())[0]) {
        Some(PropertyValue::Words(words)) => words,
        _ => String::new(),
    }
}

/// The values that the text effect has when it says what `object` says: its words, its font, its size (a fraction of the height of the picture) and its colour, in the order the effect has them.
/// (Where the object is on the picture is not a value of the effect: it is the clip's.)
pub fn values_of_a_text_object(object: &TextObject) -> [PropertyValue; 4] {
    [
        PropertyValue::Words(object.text.clone()),
        PropertyValue::Words(object.font_family.clone()),
        PropertyValue::Number(object.font_size.fraction_of_picture_height()),
        PropertyValue::Colour(object.colour),
    ]
}

/// The text object that has the values `values` of the text effect (its words, its font, its size and its colour, in that order) and is where `placement` says. A value that is missing, or is not the
/// kind it should be, is what a new text object has; a size is brought inside what a text can be, as always.
pub fn text_object_of_values(values: &[PropertyValue], placement: Placement) -> TextObject {
    let new = TextObject::default();
    let words = |slot: usize, otherwise: &str| match values.get(slot) {
        Some(PropertyValue::Words(words)) => words.clone(),
        _ => otherwise.to_owned(),
    };
    TextObject {
        text: words(0, &new.text),
        font_family: words(1, &new.font_family),
        font_size: match values.get(2) {
            Some(PropertyValue::Number(fraction)) => FontSize::from_fraction_of_picture_height(*fraction),
            _ => new.font_size,
        },
        colour: match values.get(3) {
            Some(PropertyValue::Colour(colour)) => *colour,
            _ => new.colour,
        },
        ..new.with_placement(placement)
    }
}

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
        assert_eq!(placement_value(&placement, crate::test_support::TEXT_OF_A_TEXT), None);
    }

    /// What keeps the list of the properties of a placement honest: it is exactly the properties that `placement_value` has a value for. A property that is added to a placement and not put in the
    /// list (or put in the list and given no value) is found here, and not as a control that changes nothing while the property is animated. (Rewritten on purpose in phase 9: what a text says and how
    /// it looks are the values of its text effect, so the properties of a text object are those of that effect and the three of its placement, and the list of them is made for the effect's name in its
    /// chain.)
    #[test]
    fn the_list_of_the_properties_of_a_placement_is_those_that_have_a_value_in_one_and_those_of_a_text_object_are_those_of_its_text_effect_and_its_placement()
     {
        let placement = somewhere();
        for property in crate::test_support::properties_of_every_kind() {
            assert_eq!(
                placement_value(&placement, property).is_some(),
                PROPERTIES_OF_A_PLACEMENT.contains(&property),
                "{property:?} and a placement"
            );
        }
        let content = EffectInstanceId::new(3);
        let of_the_effect = properties_of_the_text_effect(content);
        assert_eq!(
            of_the_effect.len(),
            EffectKind::TEXT.parameters().len(),
            "one for each value that the text effect has"
        );
        let object = TextObject::default();
        let values = values_of_a_text_object(&object);
        for (slot, property) in of_the_effect.into_iter().enumerate() {
            assert_eq!(
                property,
                PropertyId::effect_value(EffectKind::TEXT, content, u8::try_from(slot).unwrap())
            );
            assert_eq!(property.kind(), values[slot].kind(), "slot {slot}");
        }
        let of_the_object = properties_of_a_text_object(content);
        assert_eq!(of_the_object[..4], of_the_effect);
        assert_eq!(of_the_object[4..], PROPERTIES_OF_A_PLACEMENT);
    }

    /// What the window changes is a loudness or a mute switch, a place on the picture, or a value of an effect (which a text object's words, font, size and colour are), and a
    /// change of one of them while it is animated is made a keyframe by the one thing that knows how. A property that is none of the
    /// four would be changed by nothing.
    #[test]
    fn every_property_is_a_loudness_or_a_mute_switch_or_one_of_a_placement_or_of_an_effect() {
        for property in crate::test_support::properties_of_every_kind() {
            assert!(
                property.is_heard()
                    || PROPERTIES_OF_A_PLACEMENT.contains(&property)
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
    fn a_text_object_has_its_words_font_size_and_colour_as_the_values_of_a_text_effect() {
        let object = TextObject {
            text: "Hello".to_owned(),
            font_family: "Arial".to_owned(),
            font_size: FontSize::from_fraction_of_picture_height(0.1),
            colour: TextColour::new(1, 2, 3, 4),
            ..TextObject::default()
        };
        assert_eq!(
            values_of_a_text_object(&object),
            [
                PropertyValue::Words("Hello".to_owned()),
                PropertyValue::Words("Arial".to_owned()),
                PropertyValue::Number(0.1),
                PropertyValue::Colour(TextColour::new(1, 2, 3, 4)),
            ]
        );
        assert_eq!(
            EffectKind::TEXT.default_values()[1..],
            values_of_a_text_object(&TextObject::default())[1..],
            "the effect begins as a new text object, but for the words"
        );
    }

    #[test]
    fn a_text_object_made_of_the_values_it_has_and_the_place_it_is_at_is_the_same_object() {
        let object = TextObject {
            text: "Hello".to_owned(),
            font_family: "Arial".to_owned(),
            colour: TextColour::new(1, 2, 3, 4),
            rotation: Angle::from_degrees(12.0),
            ..TextObject::default()
        };
        assert_eq!(
            text_object_of_values(&values_of_a_text_object(&object), object.placement()),
            object
        );
    }

    #[test]
    fn a_value_that_is_missing_or_of_another_kind_is_what_a_new_text_object_has_and_a_size_is_brought_inside_what_a_text_can_be()
     {
        let new = TextObject::default();
        let elsewhere = somewhere();
        let made = text_object_of_values(&[], elsewhere);
        assert_eq!(
            made,
            new.with_placement(elsewhere),
            "nothing given: a new text object, where it is said to be"
        );
        let wrong = text_object_of_values(
            &[
                PropertyValue::Number(1.0),
                PropertyValue::Flag(true),
                PropertyValue::Words("not a size".to_owned()),
                PropertyValue::Pair(1.0, 2.0),
            ],
            elsewhere,
        );
        assert_eq!(
            wrong,
            new.with_placement(elsewhere),
            "every value of another kind: a new text object"
        );
        let too_big = text_object_of_values(
            &[
                PropertyValue::Words("a".to_owned()),
                PropertyValue::Words(String::new()),
                PropertyValue::Number(9.0),
            ],
            elsewhere,
        );
        assert_eq!(
            too_big.font_size,
            FontSize::from_fraction_of_picture_height(FontSize::LARGEST_FRACTION)
        );
        assert_eq!(too_big.text, "a", "and what is right is kept");
    }

    #[test]
    fn the_words_of_a_text_clip_are_the_first_value_of_its_text_effect_and_nothing_if_it_has_none() {
        let text = crate::project::tracks::TextClip::new(30, EffectInstanceId::FIRST).unwrap();
        assert_eq!(
            words_of_a_text_clip(&text, &crate::project::Effects::none()),
            "",
            "no text effect: no words"
        );
        let mut effects = crate::project::Effects::none();
        effects.add(EffectKind::TEXT);
        effects.set_value(
            properties_of_the_text_effect(EffectInstanceId::FIRST)[0],
            &PropertyValue::Words("Hello".to_owned()),
        );
        assert_eq!(words_of_a_text_clip(&text, &effects), "Hello");
        let elsewhere = crate::project::tracks::TextClip::new(30, EffectInstanceId::new(7)).unwrap();
        assert_eq!(
            words_of_a_text_clip(&elsewhere, &effects),
            "",
            "the effect of another name is not its content"
        );
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
