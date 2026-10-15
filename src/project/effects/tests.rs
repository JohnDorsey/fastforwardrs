// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::colour_effects::ColourMatrix;
use crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE;

fn degrees(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

// --- the table ---

/// What keeps the table honest: there is one row for each kind, and the rows are in the order of [`EffectKind::ALL`], which is
/// the order the effects are offered in. A kind that is added to the enumeration and not to the table is found here, and not
/// when it is first put on something.
#[test]
fn the_table_has_one_row_for_each_kind_of_effect_in_the_order_that_they_are_offered_in() {
    assert_eq!(ROWS.len(), EffectKind::ALL.len());
    for (row, kind) in ROWS.iter().zip(EffectKind::ALL) {
        assert_eq!(row.kind, kind);
        assert_eq!(kind.row().kind, kind);
    }
}

#[test]
fn every_kind_has_a_name_that_only_it_has_and_that_it_is_found_by_and_something_to_say_for_itself() {
    let mut names: Vec<&str> = EffectKind::ALL.iter().map(|kind| kind.name()).collect();
    for kind in EffectKind::ALL {
        assert_eq!(EffectKind::from_name(kind.name()), Some(kind));
        assert!(!kind.description().is_empty(), "{kind:?}");
        assert_eq!(
            kind.name(),
            kind.name().to_lowercase(),
            "it is the same in a file whatever the language"
        );
    }
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), EffectKind::ALL.len());
    assert_eq!(EffectKind::from_name("not an effect"), None);
}

/// What ties the table of effects to the properties: the values of an effect are properties that say which effect they are of, in which
/// place, and what kind of value and what name they have; and a property of an effect is of no other.
#[test]
fn the_values_of_an_effect_are_properties_that_say_which_effect_and_which_place_they_are_of() {
    let mut names_of_values: Vec<&str> = Vec::new();
    for kind in EffectKind::ALL {
        assert!(!kind.value_kinds().is_empty(), "{kind:?} has values");
        assert_eq!(
            kind.value_names().len(),
            kind.value_kinds().len(),
            "{kind:?} names its values"
        );
        let effect = Effect::new(kind);
        let properties: Vec<PropertyId> = effect.properties().collect();
        assert_eq!(properties.len(), kind.value_kinds().len(), "{kind:?}");
        for (slot, property) in properties.iter().enumerate() {
            assert!(property.is_a_value_of_an_effect(), "{property:?}");
            assert_eq!(property.kind(), kind.value_kinds()[slot], "{kind:?}, place {slot}");
            assert_eq!(property.name(), kind.value_names()[slot], "{kind:?}, place {slot}");
            assert_eq!(effect.value_of(*property), Some(&kind.default_values()[slot]));
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
            effect.property(kind.value_kinds().len()),
            None,
            "{kind:?} has no value after its last"
        );
    }
    // (A file of version 10 names a value of an effect by one name, so the names are not those of another effect's values or of the properties
    // that a clip or a track has of its own.)
    for property in PropertyId::OWN {
        assert!(!property.is_a_value_of_an_effect(), "{property:?}");
        assert!(!names_of_values.contains(&property.name()), "{property:?}");
    }
    let mut sorted = names_of_values.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names_of_values.len(), "{names_of_values:?}");
}

#[test]
fn what_an_effect_begins_with_is_a_value_of_the_kind_each_of_its_properties_has() {
    for kind in EffectKind::ALL {
        let values = kind.default_values();
        assert_eq!(values.len(), kind.value_kinds().len(), "{kind:?}");
        for (value, value_kind) in values.iter().zip(kind.value_kinds()) {
            assert_eq!(value.kind(), *value_kind, "{kind:?}");
        }
        assert!(Effect::with_values(kind, values).is_some(), "{kind:?}");
    }
}

#[test]
fn an_effect_that_is_put_on_with_the_values_it_begins_with_changes_no_colour_if_it_changes_colours() {
    for kind in EffectKind::ALL {
        if let Some(matrix) = kind.colour_matrix(&kind.default_values()) {
            assert!(
                matrix.changes_nothing(),
                "{kind:?} changes the colours as soon as it is put on"
            );
        }
    }
}

#[test]
fn what_can_have_an_effect_is_what_is_seen_and_not_a_sound_or_a_crate_and_a_track_can_have_one_too() {
    for kind in EffectKind::ALL {
        for seen in [ClipKind::Video, ClipKind::Image, ClipKind::Text] {
            assert!(kind.applies_to_a_clip_of_kind(seen), "{kind:?} on {seen:?}");
        }
        for not_seen in [ClipKind::Audio, ClipKind::Crate] {
            assert!(!kind.applies_to_a_clip_of_kind(not_seen), "{kind:?} on {not_seen:?}");
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
    let kind = EffectKind::HueShift;
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
    let effect = Effect::new(EffectKind::HueShift);
    assert_eq!(effect.kind(), EffectKind::HueShift);
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
    assert!(Effect::with_values(EffectKind::HueShift, vec![degrees(90.0)]).is_some());
    assert!(
        Effect::with_values(EffectKind::HueShift, vec![]).is_none(),
        "one value too few"
    );
    assert!(Effect::with_values(EffectKind::HueShift, vec![degrees(1.0), degrees(2.0)]).is_none());
    assert!(Effect::with_values(EffectKind::HueShift, vec![PropertyValue::Words("blue".to_owned())]).is_none());
}

// --- the effects of a clip or a track ---

#[test]
fn a_clip_or_a_track_has_no_effects_to_begin_with_and_an_effect_is_put_on_as_often_as_it_is_asked_for_and_taken_off_by_its_name()
 {
    let mut effects = Effects::none();
    assert!(effects.is_empty());
    assert_eq!(effects.len(), 0);
    assert!(!effects.has_the_instance(EffectInstanceId::FIRST));

    let first = effects.add(EffectKind::HueShift);
    assert!(effects.has_the_instance(first));
    assert_eq!(effects.len(), 1);
    let second = effects.add(EffectKind::HueShift);
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

    effects.add(EffectKind::HueShift);
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
    effects.add(EffectKind::HueShift);
    assert_eq!(
        effects.value_of(HUE),
        Some(degrees(0.0)),
        "put on again, it begins again"
    );
}

#[test]
fn effects_that_are_put_on_are_in_the_order_they_are_put_on_in_with_the_values_they_have_and_each_named_in_the_chain() {
    let first = Effect::with_values(EffectKind::HueShift, vec![degrees(10.0)]).unwrap();
    let second = Effect::with_values(EffectKind::HueShift, vec![degrees(20.0)]).unwrap();
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
    let effect = Effect::new(EffectKind::HueShift);
    let nowhere = PropertyId::effect_value(EffectKind::HueShift, EffectInstanceId::FIRST, 1);

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
