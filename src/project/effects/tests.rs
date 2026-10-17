// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::colour_effects::ColourMatrix;
use crate::project::tracks::ClipKind;
use crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE;

fn degrees(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

// --- the effects of the program ---

#[test]
fn the_effects_of_the_program_are_installed_and_found_by_their_key_which_each_has_only_once_and_in_the_namespace_of_the_program()
 {
    let mut keys: Vec<&str> = Vec::new();
    for kind in EffectKind::BUILT_IN {
        assert!(kind.is_installed(), "{kind:?}");
        assert_eq!(EffectKind::built_in_with_key(kind.key()), Some(kind));
        assert_eq!(kind.descriptor().problem(), None, "{kind:?}");
        assert!(kind.key().starts_with("builtin:"), "{kind:?}");
        keys.push(kind.key());
    }
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), EffectKind::BUILT_IN.len());
    assert_eq!(EffectKind::built_in_with_key("builtin:not_an_effect"), None);
    assert_eq!(EffectKind::HUE_SHIFT.key(), HUE_SHIFT_KEY);
}

#[test]
fn every_kind_has_a_name_in_lower_case_that_only_it_has_and_something_to_say_for_itself() {
    let mut names: Vec<&str> = EffectKind::BUILT_IN.iter().map(|kind| kind.name()).collect();
    for kind in EffectKind::BUILT_IN {
        assert!(!kind.name().is_empty(), "{kind:?}");
        assert!(!kind.description().is_empty(), "{kind:?}");
        assert_eq!(kind.name(), kind.name().to_lowercase(), "{kind:?}");
        assert_eq!(kind.to_string(), kind.name(), "it is shown by its name");
    }
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), EffectKind::BUILT_IN.len());
}

/// What ties the descriptors of effects to the properties: the values of an effect are properties that say which effect they are of, in which
/// place, and what kind of value and what name they have; and a property of an effect is of no other.
#[test]
fn the_values_of_an_effect_are_properties_that_say_which_effect_and_which_place_they_are_of() {
    let mut names_of_values: Vec<&str> = Vec::new();
    for kind in EffectKind::BUILT_IN {
        assert!(kind.value_count() > 0, "{kind:?} has values");
        let effect = Effect::new(kind);
        let properties: Vec<PropertyId> = effect.properties().collect();
        assert_eq!(properties.len(), kind.value_count(), "{kind:?}");
        for (slot, property) in properties.iter().enumerate() {
            let parameter = &kind.parameters()[slot];
            assert!(property.is_a_value_of_an_effect(), "{property:?}");
            assert_eq!(Some(property.kind()), kind.value_kind(slot), "{kind:?}, place {slot}");
            assert_eq!(property.name(), parameter.name, "{kind:?}, place {slot}");
            assert_eq!(effect.value_of(*property), Some(&parameter.default));
            assert_eq!(
                effect.property_with_id(&parameter.id),
                Some(*property),
                "found by its id"
            );
            assert_eq!(kind.slot_of_the_value_with_id(&parameter.id), Some(slot));
            // (Only this effect has it: not another effect, not another one of the same kind, not another place.)
            let other_instance = effect.clone().named(EffectInstanceId::new(7));
            assert_eq!(
                other_instance.value_of(*property),
                None,
                "{property:?} of the effect named 7"
            );
            names_of_values.push(property.name());
        }
        assert_eq!(
            effect.property(kind.value_count()),
            None,
            "{kind:?} has no value after its last"
        );
        assert_eq!(effect.property_with_id("not a value"), None);
    }
    // (A file names a value of an effect by its id and the effect it is of, but the names are told apart by people: they are not the names of
    // the properties that a clip or a track has of its own.)
    for property in PropertyId::OWN {
        assert!(!property.is_a_value_of_an_effect(), "{property:?}");
        assert!(!names_of_values.contains(&property.name()), "{property:?}");
    }
}

#[test]
fn what_an_effect_begins_with_is_a_value_of_the_kind_each_of_its_properties_has() {
    for kind in EffectKind::BUILT_IN {
        let values = kind.default_values();
        assert_eq!(values.len(), kind.value_count(), "{kind:?}");
        for (slot, value) in values.iter().enumerate() {
            assert_eq!(Some(value.kind()), kind.value_kind(slot), "{kind:?}");
        }
        assert!(Effect::with_values(kind, values).is_some(), "{kind:?}");
    }
}

#[test]
fn an_effect_that_is_put_on_with_the_values_it_begins_with_changes_no_colour_if_it_changes_colours() {
    for kind in EffectKind::BUILT_IN {
        if let Some(matrix) = kind.colour_matrix(&kind.default_values()) {
            assert!(
                matrix.changes_nothing(),
                "{kind:?} changes the colours as soon as it is put on"
            );
        }
    }
}

#[test]
fn what_can_have_an_effect_is_what_the_domain_of_the_effect_says_and_a_track_can_have_one_too() {
    for kind in EffectKind::BUILT_IN {
        let (seen, heard) = ([ClipKind::Video, ClipKind::Image, ClipKind::Text], [ClipKind::Audio]);
        let (can, cannot): (&[ClipKind], &[ClipKind]) = match kind.domain() {
            EffectDomain::Picture => (&seen, &heard),
            EffectDomain::Sound => (&heard, &seen),
        };
        for clip in can {
            assert!(kind.applies_to_a_clip_of_kind(*clip), "{kind:?} on {clip:?}");
        }
        for clip in cannot.iter().chain(&[ClipKind::Crate]) {
            assert!(!kind.applies_to_a_clip_of_kind(*clip), "{kind:?} on {clip:?}");
        }
        assert!(kind.applies_to_a_track());
        let mut chain = Effects::none();
        chain.add(kind);
        for property in Effect::new(kind).properties() {
            assert!(property.is_had_by_a_track(&chain), "{property:?}");
            assert!(
                !property.is_had_by_a_track(&Effects::none()),
                "{property:?} without the effect"
            );
            for clip_kind in [
                ClipKind::Video,
                ClipKind::Audio,
                ClipKind::Image,
                ClipKind::Text,
                ClipKind::Crate,
            ] {
                assert_eq!(
                    property.is_had_by_a_clip(clip_kind, &chain),
                    kind.applies_to_a_clip_of_kind(clip_kind),
                    "{property:?} and {clip_kind:?} are as the effect says"
                );
                assert!(
                    !property.is_had_by_a_clip(clip_kind, &Effects::none()),
                    "{property:?} and {clip_kind:?} without the effect"
                );
            }
        }
    }
}

#[test]
fn the_hue_shift_turns_the_colours_by_its_value_and_changes_nothing_for_a_value_that_is_not_a_turn() {
    let kind = EffectKind::HUE_SHIFT;
    assert_eq!(
        kind.colour_matrix(&[degrees(180.0)]),
        Some(ColourMatrix::hue_rotation(180.0))
    );
    assert_eq!(
        kind.colour_matrix(&[degrees(-90.0)]),
        Some(ColourMatrix::hue_rotation(-90.0))
    );
    for not_the_values in [
        vec![],
        vec![PropertyValue::Flag(true)],
        vec![degrees(1.0), degrees(2.0)],
    ] {
        assert_eq!(
            kind.colour_matrix(&not_the_values),
            Some(ColourMatrix::NOTHING_CHANGES),
            "{not_the_values:?}"
        );
    }
}

// --- an effect ---

#[test]
fn a_new_effect_has_the_values_its_kind_begins_with_and_its_values_can_be_read_and_changed_by_property() {
    let effect = Effect::new(EffectKind::HUE_SHIFT);
    assert_eq!(effect.kind(), EffectKind::HUE_SHIFT);
    assert_eq!(effect.values(), [degrees(0.0)]);
    assert_eq!(effect.value_of(HUE), Some(&degrees(0.0)));
    assert_eq!(effect.value_of(PropertyId::Gain), None, "not one of its properties");

    let turned = effect.with_value(HUE, &degrees(120.0)).unwrap();
    assert_eq!(turned.value_of(HUE), Some(&degrees(120.0)));
    assert_eq!(
        effect.values(),
        [degrees(0.0)],
        "the effect it was made from is as it was"
    );
    assert_eq!(
        effect.with_value(HUE, &PropertyValue::Flag(true)),
        None,
        "not the kind of value it has"
    );
    assert_eq!(
        effect.with_value(PropertyId::Rotation, &degrees(1.0)),
        None,
        "not its property"
    );
}

#[test]
fn an_effect_is_not_made_from_values_that_are_not_the_ones_its_kind_has() {
    assert!(Effect::with_values(EffectKind::HUE_SHIFT, vec![degrees(90.0)]).is_some());
    assert!(
        Effect::with_values(EffectKind::HUE_SHIFT, vec![]).is_none(),
        "one value too few"
    );
    assert!(Effect::with_values(EffectKind::HUE_SHIFT, vec![degrees(1.0), degrees(2.0)]).is_none());
    assert!(Effect::with_values(EffectKind::HUE_SHIFT, vec![PropertyValue::Words("blue".to_owned())]).is_none());
}

// --- the effects of a clip or a track ---

#[test]
fn a_clip_or_a_track_has_no_effects_to_begin_with_and_an_effect_is_put_on_as_often_as_it_is_asked_for_and_taken_off_by_its_name()
 {
    let mut effects = Effects::none();
    assert!(effects.is_empty());
    assert_eq!(effects.len(), 0);
    assert!(!effects.has_the_instance(EffectInstanceId::FIRST));

    let first = effects.add(EffectKind::HUE_SHIFT);
    assert!(effects.has_the_instance(first));
    assert_eq!(effects.len(), 1);
    let second = effects.add(EffectKind::HUE_SHIFT);
    assert_ne!(first, second, "it is one more, and not the same effect again");
    assert_eq!(effects.len(), 2);
    assert!(effects.has_the_instance(first) && effects.has_the_instance(second));

    assert!(effects.remove(first));
    assert_eq!(effects.len(), 1);
    assert!(effects.has_the_instance(second), "the other of its kind is still on");
    assert!(effects.remove(second));
    assert!(effects.is_empty());
    assert!(!effects.remove(second), "there is none to take off");
}

#[test]
fn the_value_of_a_property_of_an_effect_that_is_on_is_found_and_changed_and_one_that_is_not_on_is_not() {
    let mut effects = Effects::none();
    assert_eq!(effects.value_of(HUE), None, "not on");
    assert!(
        !effects.set_value(HUE, &degrees(30.0)),
        "not on, so there is no value to change"
    );

    effects.add(EffectKind::HUE_SHIFT);
    assert_eq!(effects.value_of(HUE), Some(degrees(0.0)));
    assert!(effects.set_value(HUE, &degrees(30.0)));
    assert_eq!(effects.value_of(HUE), Some(degrees(30.0)));
    assert!(
        !effects.set_value(HUE, &PropertyValue::Flag(false)),
        "not that kind of value"
    );
    assert!(
        !effects.set_value(PropertyId::Gain, &degrees(1.0)),
        "not a value of an effect"
    );
    assert_eq!(effects.value_of(HUE), Some(degrees(30.0)), "as it was");

    effects.remove(EffectInstanceId::FIRST);
    effects.add(EffectKind::HUE_SHIFT);
    assert_eq!(
        effects.value_of(HUE),
        Some(degrees(0.0)),
        "put on again, it begins again"
    );
}

#[test]
fn effects_that_are_put_on_are_in_the_order_they_are_put_on_in_with_the_values_they_have_and_each_named_in_the_chain() {
    let first = Effect::with_values(EffectKind::HUE_SHIFT, vec![degrees(10.0)]).unwrap();
    let second = Effect::with_values(EffectKind::HUE_SHIFT, vec![degrees(20.0)]).unwrap();
    let mut effects = Effects::none();
    let first_name = effects.put_on(first.clone());
    let second_name = effects.put_on(second.clone());
    assert_eq!(
        (first_name, second_name),
        (EffectInstanceId::FIRST, EffectInstanceId::new(1))
    );
    assert_eq!(effects.len(), 2);
    assert_eq!(
        effects.iter().collect::<Vec<_>>(),
        vec![&first.named(first_name), &second.named(second_name)],
        "both of the kind, in the order they were put on in, with the values they came with"
    );
}

#[test]
fn a_value_in_a_place_the_effect_has_no_value_in_is_not_a_value_of_it_and_is_not_changed() {
    let effect = Effect::new(EffectKind::HUE_SHIFT);
    let nowhere = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::FIRST, 1);

    assert_eq!(effect.value_of(nowhere), None);
    assert_eq!(
        effect.with_value(nowhere, &degrees(10.0)),
        None,
        "and trying to change it is not a crash"
    );
    let mut chain = Effects::none();
    chain.put_on(effect);
    assert!(!chain.set_value(nowhere, &degrees(10.0)));
    assert!(!chain.has_the_value(nowhere));
}

#[test]
fn an_effect_is_switched_on_when_it_is_put_on_and_a_change_of_a_value_does_not_switch_it() {
    let mut chain = Effects::none();
    let instance = chain.add(EffectKind::HUE_SHIFT);
    assert!(chain.instance(instance).unwrap().is_enabled());

    assert!(chain.set_enabled(instance, false));
    let property = PropertyId::effect_value(EffectKind::HUE_SHIFT, instance, 0);
    assert!(chain.set_value(property, &degrees(45.0)));

    assert!(
        !chain.instance(instance).unwrap().is_enabled(),
        "a value that is changed leaves it off"
    );
    assert!(
        !chain.set_enabled(EffectInstanceId::new(9), true),
        "there is no such effect"
    );
    assert!(Effect::new(EffectKind::HUE_SHIFT).with_enabled(false) != Effect::new(EffectKind::HUE_SHIFT));
}
