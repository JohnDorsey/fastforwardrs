// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a value of a plugin is as a value of an effect, and back (phase 10, P2 of `docs/engine-phase-10-editors-design.md`): the project keeps a parameter of a plugin as the value of
//! an effect (a number, a switch, or the id of an option of a choice), and the plugin and its window have it as a number. **One place says how, for the sound that is made from the
//! project and for the window that edits it**, so that they cannot disagree.

use crate::project::{EffectDescriptor, ParameterControl, PropertyValue};

/// The number that a plugin is told for the value `value` of an effect: a number as it is, a switch as 0 or 1, the id of an option of a choice as the number it is (the id of an option is
/// the number the plugin gives it). A value of another kind is 0.
pub fn number_for_a_plugin(value: &PropertyValue) -> f32 {
    match value {
        PropertyValue::Number(number) => *number,
        PropertyValue::Flag(on) => f32::from(u8::from(*on)),
        PropertyValue::Words(words) => words.trim().parse::<f32>().unwrap_or(0.0),
        PropertyValue::Pair(..) | PropertyValue::Colour(_) => 0.0,
    }
}

/// The value of the effect that `descriptor` describes, in place `slot`, that a plugin's number `number` is: what [`number_for_a_plugin`] undoes. `None` if the effect has no value
/// there.
pub fn value_for_a_plugin_number(descriptor: &EffectDescriptor, slot: usize, number: f64) -> Option<PropertyValue> {
    let parameter = descriptor.parameters.get(slot)?;
    Some(match &parameter.control {
        ParameterControl::Toggle => PropertyValue::Flag(number >= 0.5),
        ParameterControl::Choice(_) => PropertyValue::Words((number.round() as i64).to_string()),
        ParameterControl::Integer => PropertyValue::Number(number.round() as f32),
        _ => {
            let number = number as f32;
            PropertyValue::Number(match parameter.range {
                Some((least, most)) if number.is_finite() => number.clamp(least, most),
                _ => number,
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::project::{ChoiceOption, ParameterDescriptor, UnknownFields, ValueScale};

    fn parameter(control: ParameterControl, default: PropertyValue, range: Option<(f32, f32)>) -> ParameterDescriptor {
        ParameterDescriptor {
            id: Cow::Borrowed("x"),
            name: Cow::Borrowed("x"),
            label: Cow::Borrowed("X"),
            default,
            range,
            unit: Cow::Borrowed(""),
            decimals: None,
            unknown_fields: UnknownFields::none(),
            scale: ValueScale::Linear,
            control,
        }
    }

    /// An effect that has, in order, a slider from 0 to 10, a switch, a choice of two (ids 0 and 1) and a whole number from 2 to 8.
    fn the_descriptor() -> EffectDescriptor {
        let mut descriptor = crate::test_support::the_descriptor_of_an_effect_the_program_does_not_have("isf:x", &[]);
        descriptor.parameters = Cow::Owned(vec![
            parameter(
                ParameterControl::Automatic,
                PropertyValue::Number(1.0),
                Some((0.0, 10.0)),
            ),
            parameter(ParameterControl::Toggle, PropertyValue::Flag(false), None),
            parameter(
                ParameterControl::Choice(Cow::Owned(vec![
                    ChoiceOption {
                        id: Cow::Borrowed("0"),
                        label: Cow::Borrowed("off"),
                    },
                    ChoiceOption {
                        id: Cow::Borrowed("1"),
                        label: Cow::Borrowed("on"),
                    },
                ])),
                PropertyValue::Words("0".to_owned()),
                None,
            ),
            parameter(ParameterControl::Integer, PropertyValue::Number(2.0), Some((2.0, 8.0))),
        ]);
        descriptor
    }

    #[test]
    fn a_value_of_an_effect_is_the_number_that_a_plugin_is_told() {
        assert_eq!(number_for_a_plugin(&PropertyValue::Number(2.5)), 2.5);
        assert_eq!(number_for_a_plugin(&PropertyValue::Flag(true)), 1.0);
        assert_eq!(number_for_a_plugin(&PropertyValue::Flag(false)), 0.0);
        assert_eq!(number_for_a_plugin(&PropertyValue::Words(" 3 ".to_owned())), 3.0);
        assert_eq!(
            number_for_a_plugin(&PropertyValue::Words("not a number".to_owned())),
            0.0
        );
        assert_eq!(number_for_a_plugin(&PropertyValue::Pair(1.0, 2.0)), 0.0);
    }

    #[test]
    fn a_number_that_a_plugin_has_is_the_value_of_the_effect_that_its_control_says() {
        let descriptor = the_descriptor();

        assert_eq!(
            value_for_a_plugin_number(&descriptor, 0, 4.5),
            Some(PropertyValue::Number(4.5))
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 1, 1.0),
            Some(PropertyValue::Flag(true))
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 1, 0.49),
            Some(PropertyValue::Flag(false))
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 1, 0.5),
            Some(PropertyValue::Flag(true))
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 2, 1.0),
            Some(PropertyValue::Words("1".to_owned()))
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 2, 0.9),
            Some(PropertyValue::Words("1".to_owned())),
            "a choice is the nearest option, and not the number cut"
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 3, 4.6),
            Some(PropertyValue::Number(5.0))
        );
        assert_eq!(value_for_a_plugin_number(&descriptor, 4, 1.0), None, "no value there");
    }

    #[test]
    fn a_number_past_the_range_of_a_slider_is_brought_back_to_it_as_the_sound_does() {
        let descriptor = the_descriptor();

        assert_eq!(
            value_for_a_plugin_number(&descriptor, 0, 99.0),
            Some(PropertyValue::Number(10.0))
        );
        assert_eq!(
            value_for_a_plugin_number(&descriptor, 0, -5.0),
            Some(PropertyValue::Number(0.0))
        );
    }

    #[test]
    fn a_value_goes_to_the_plugin_and_comes_back_the_same() {
        let descriptor = the_descriptor();
        for (slot, value) in [
            (0, PropertyValue::Number(7.25)),
            (1, PropertyValue::Flag(true)),
            (2, PropertyValue::Words("1".to_owned())),
            (3, PropertyValue::Number(6.0)),
        ] {
            let number = f64::from(number_for_a_plugin(&value));
            assert_eq!(
                value_for_a_plugin_number(&descriptor, slot, number),
                Some(value),
                "slot {slot}"
            );
        }
    }
}
