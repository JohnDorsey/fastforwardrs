use super::*;
use crate::colour_effects::ColourMatrix;

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

/// What ties the table of properties to the table of effects: a property of an effect says which effect it is of, and the
/// effect lists it, and no other property is of an effect.
#[test]
fn the_values_of_the_effects_are_the_properties_that_say_they_are_of_an_effect_and_each_has_one_effect() {
    for kind in EffectKind::ALL {
        assert!(!kind.properties().is_empty(), "{kind:?} has values");
        for property in kind.properties() {
            assert_eq!(property.effect(), Some(kind), "{property:?}");
        }
    }
    for property in PropertyId::ALL {
        let effects_listing_it: Vec<EffectKind> = EffectKind::ALL
            .into_iter()
            .filter(|kind| kind.properties().contains(&property))
            .collect();
        assert!(
            effects_listing_it.len() <= 1,
            "{property:?} is a value of {effects_listing_it:?}"
        );
        assert_eq!(property.effect(), effects_listing_it.first().copied(), "{property:?}");
    }
}

#[test]
fn what_an_effect_begins_with_is_a_value_of_the_kind_each_of_its_properties_has() {
    for kind in EffectKind::ALL {
        let values = kind.default_values();
        assert_eq!(values.len(), kind.properties().len(), "{kind:?}");
        for (value, property) in values.iter().zip(kind.properties()) {
            assert_eq!(value.kind(), property.kind(), "{kind:?}: {property:?}");
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
        for property in kind.properties() {
            assert!(property.applies_to_a_track(), "{property:?}");
            for clip_kind in [
                ClipKind::Video,
                ClipKind::Audio,
                ClipKind::Image,
                ClipKind::Text,
                ClipKind::Crate,
            ] {
                assert_eq!(
                    property.applies_to_a_clip_of_kind(clip_kind),
                    kind.applies_to_a_clip_of_kind(clip_kind),
                    "{property:?} and {clip_kind:?} are as the effect says"
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
    assert_eq!(effect.value_of(PropertyId::HueShift), Some(&degrees(0.0)));
    assert_eq!(effect.value_of(PropertyId::Gain), None, "not one of its properties");

    let turned = effect.with_value(PropertyId::HueShift, &degrees(120.0)).unwrap();
    assert_eq!(turned.value_of(PropertyId::HueShift), Some(&degrees(120.0)));
    assert_eq!(
        effect.values(),
        [degrees(0.0)],
        "the effect it was made from is as it was"
    );
    assert_eq!(
        effect.with_value(PropertyId::HueShift, &PropertyValue::Flag(true)),
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
fn a_clip_or_a_track_has_no_effects_to_begin_with_and_an_effect_is_put_on_once_and_taken_off() {
    let mut effects = Effects::none();
    assert!(effects.is_empty());
    assert_eq!(effects.len(), 0);
    assert!(!effects.contains(EffectKind::HueShift));

    assert!(effects.add(EffectKind::HueShift));
    assert!(effects.contains(EffectKind::HueShift));
    assert_eq!(effects.len(), 1);
    assert!(!effects.add(EffectKind::HueShift), "there is one already");
    assert_eq!(effects.len(), 1);

    assert!(effects.remove(EffectKind::HueShift));
    assert!(effects.is_empty());
    assert!(!effects.remove(EffectKind::HueShift), "there is none to take off");
}

#[test]
fn the_value_of_a_property_of_an_effect_that_is_on_is_found_and_changed_and_one_that_is_not_on_is_not() {
    let mut effects = Effects::none();
    assert_eq!(effects.value_of(PropertyId::HueShift), None, "not on");
    assert!(
        !effects.set_value(PropertyId::HueShift, &degrees(30.0)),
        "not on, so there is no value to change"
    );

    effects.add(EffectKind::HueShift);
    assert_eq!(effects.value_of(PropertyId::HueShift), Some(degrees(0.0)));
    assert!(effects.set_value(PropertyId::HueShift, &degrees(30.0)));
    assert_eq!(effects.value_of(PropertyId::HueShift), Some(degrees(30.0)));
    assert!(
        !effects.set_value(PropertyId::HueShift, &PropertyValue::Flag(false)),
        "not that kind of value"
    );
    assert!(
        !effects.set_value(PropertyId::Gain, &degrees(1.0)),
        "not a value of an effect"
    );
    assert_eq!(effects.value_of(PropertyId::HueShift), Some(degrees(30.0)), "as it was");

    effects.remove(EffectKind::HueShift);
    effects.add(EffectKind::HueShift);
    assert_eq!(
        effects.value_of(PropertyId::HueShift),
        Some(degrees(0.0)),
        "put on again, it begins again"
    );
}

#[test]
fn effects_that_are_put_on_are_in_the_order_they_are_put_on_in_with_only_the_first_of_each_kind() {
    let first = Effect::with_values(EffectKind::HueShift, vec![degrees(10.0)]).unwrap();
    let second = Effect::with_values(EffectKind::HueShift, vec![degrees(20.0)]).unwrap();
    let mut effects = Effects::none();
    assert!(effects.put_on(first.clone()));
    assert!(!effects.put_on(second), "there is one of that kind already");
    assert_eq!(effects.len(), 1);
    assert_eq!(effects.get(EffectKind::HueShift), Some(&first));
    assert_eq!(effects.iter().collect::<Vec<_>>(), vec![&first]);
}
