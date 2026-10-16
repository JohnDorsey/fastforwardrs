// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the kinds of effect: a descriptor made into a kind, the kinds being the same only when they are the same descriptor, what a kind of
//! effect that the program does not have is and does, and the descriptor of the hue shift as it was in its first version.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

use super::*;
use crate::project::tracks::ClipKind;
use crate::project::{GroupKind, TextColour, ValueKind};
use crate::test_support::{
    a_kind_of_effect_the_program_does_not_have, the_descriptor_of_an_effect_the_program_does_not_have,
};

fn degrees(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

fn made_up(key: &str) -> EffectDescriptor {
    the_descriptor_of_an_effect_the_program_does_not_have(
        key,
        &[("amount", degrees(0.5)), ("on", PropertyValue::Flag(true))],
    )
}

// --- the hue shift as it was ---

/// **What a project file that has a snapshot of the hue shift is read with**: the descriptor of the hue shift as it was in version 1.0, written out
/// here. If this fails, the descriptor was changed without its version being changed, and the effects of the files that were saved before are not
/// the effect that they were: change the version of the descriptor (and see that the files of the version before still bind: see
/// [`EffectDescriptor::can_stand_in_for`]), and only then this.
#[test]
fn the_descriptor_of_the_hue_shift_is_what_it_was_in_its_first_version() {
    let first_version = EffectDescriptor {
        key: Cow::Borrowed("builtin:hue_shift"),
        version: (1, 0),
        name: Cow::Borrowed("hue shift"),
        description: Cow::Borrowed("Turns the colours round the wheel of colours"),
        domain: EffectDomain::Picture,
        changes_each_pixel_by_itself: true,
        parameters: Cow::Owned(vec![ParameterDescriptor {
            id: Cow::Borrowed("degrees"),
            name: Cow::Borrowed("hue shift"),
            label: Cow::Borrowed("Turn"),
            default: degrees(0.0),
            range: Some((-180.0, 180.0)),
            unit: Cow::Borrowed("°"),
            decimals: Some(1),
            unknown_fields: UnknownFields::none(),
        }]),
        unknown_fields: UnknownFields::none(),
    };

    assert_eq!(EffectKind::HUE_SHIFT.descriptor(), &first_version);
}

// --- making a kind of a snapshot ---

#[test]
fn the_snapshot_of_the_descriptor_of_an_effect_of_the_program_is_that_effect() {
    let own = EffectKind::HUE_SHIFT;

    let made = EffectKind::from_snapshot(own.descriptor().clone()).unwrap();

    assert_eq!(made, own);
    assert!(made.is_installed());
}

#[test]
fn a_snapshot_of_the_same_major_number_that_has_nothing_the_effect_of_the_program_has_not_is_that_effect_whatever_its_other_words()
 {
    let mut snapshot = EffectKind::HUE_SHIFT.descriptor().clone();
    snapshot.version = (1, 7);
    snapshot.description = Cow::Borrowed("What an earlier program said it does");
    snapshot.parameters.to_mut()[0].label = Cow::Borrowed("What an earlier program called the control");

    assert_eq!(EffectKind::from_snapshot(snapshot).unwrap(), EffectKind::HUE_SHIFT);
}

#[test]
fn a_snapshot_with_another_major_number_or_a_value_the_effect_of_the_program_has_not_or_a_value_of_another_kind_is_not_that_effect()
 {
    let own = EffectKind::HUE_SHIFT.descriptor().clone();
    let mut other_major = own.clone();
    other_major.version = (2, 0);
    let mut one_more_value = own.clone();
    one_more_value.parameters.to_mut().push(ParameterDescriptor {
        id: Cow::Borrowed("saturation"),
        name: Cow::Borrowed("saturation"),
        label: Cow::Borrowed("Saturation"),
        default: degrees(1.0),
        range: None,
        unit: Cow::Borrowed(""),
        decimals: None,
        unknown_fields: UnknownFields::none(),
    });
    let mut another_kind_of_value = own.clone();
    another_kind_of_value.parameters.to_mut()[0].default = PropertyValue::Flag(false);
    let mut another_domain = own.clone();
    another_domain.domain = EffectDomain::Sound;

    for (snapshot, why) in [
        (other_major, "another major number"),
        (one_more_value, "a value that it has not"),
        (another_kind_of_value, "a value of another kind"),
        (another_domain, "another domain"),
    ] {
        let made = EffectKind::from_snapshot(snapshot).unwrap();
        assert_ne!(made, EffectKind::HUE_SHIFT, "{why}");
        assert!(!made.is_installed(), "{why}: the program does not have it");
        assert_eq!(
            made.key(),
            HUE_SHIFT_KEY,
            "{why}: it is the same key, and still a kind of its own"
        );
    }
}

#[test]
fn a_snapshot_that_has_fewer_values_than_the_effect_of_the_program_is_that_effect() {
    let mut snapshot = EffectKind::HUE_SHIFT.descriptor().clone();
    snapshot.parameters.to_mut().clear();

    assert_eq!(EffectKind::from_snapshot(snapshot).unwrap(), EffectKind::HUE_SHIFT);
}

#[test]
fn the_same_snapshot_made_a_kind_twice_is_the_same_kind_and_another_is_another_and_they_are_in_the_order_of_their_keys()
{
    let first = EffectKind::from_snapshot(made_up("isf:first_made_up")).unwrap();
    let again = EffectKind::from_snapshot(made_up("isf:first_made_up")).unwrap();
    let second = EffectKind::from_snapshot(made_up("isf:second_made_up")).unwrap();

    assert_eq!(first, again);
    assert_ne!(first, second);
    assert!(first < second);
    assert!(second > first);
    assert_eq!(first.cmp(&again), std::cmp::Ordering::Equal);
    assert!(!first.is_installed() && !second.is_installed());
    let hashed: HashSet<EffectKind> = [first, again, second, EffectKind::HUE_SHIFT].into_iter().collect();
    assert_eq!(hashed.len(), 3, "a kind is the same as itself for a hash too");
}

#[test]
fn two_snapshots_of_one_key_and_version_that_say_different_things_are_two_kinds_that_are_told_apart_and_put_in_one_order()
 {
    let mut other = made_up("isf:one_key_two_meanings");
    other.parameters.to_mut()[0].default = degrees(0.75);
    let first = EffectKind::from_snapshot(made_up("isf:one_key_two_meanings")).unwrap();
    let second = EffectKind::from_snapshot(other).unwrap();

    assert_ne!(first, second);
    assert_ne!(
        first.cmp(&second),
        std::cmp::Ordering::Equal,
        "an order that agrees with their being different"
    );
    assert_eq!(first.cmp(&second), second.cmp(&first).reverse());
    assert_eq!(first.default_values()[0], degrees(0.5));
    assert_eq!(second.default_values()[0], degrees(0.75));
}

#[test]
fn the_kinds_with_one_key_are_in_the_order_of_their_versions() {
    let mut newer = made_up("isf:versions");
    newer.version = (1, 3);
    let older = EffectKind::from_snapshot(made_up("isf:versions")).unwrap();
    let newer = EffectKind::from_snapshot(newer).unwrap();

    assert!(older < newer);
}

// --- what is wrong with a snapshot ---

#[test]
fn a_snapshot_that_the_program_cannot_rely_on_is_refused_and_the_reason_says_what_is_wrong() {
    let mut cases: Vec<(EffectDescriptor, &str)> = Vec::new();
    for key in ["", "no namespace", ":no namespace", "no id:", "hue_shift"] {
        cases.push((made_up(key), "is not a namespace, a colon and an id"));
    }
    let mut no_name = made_up("isf:made_up");
    no_name.name = Cow::Borrowed("");
    cases.push((no_name, "has no name"));
    let mut twice = made_up("isf:made_up");
    let copy = twice.parameters[0].clone();
    twice.parameters.to_mut().push(copy);
    cases.push((twice, "two values with the id \"amount\""));
    let mut no_id = made_up("isf:made_up");
    no_id.parameters.to_mut()[1].id = Cow::Borrowed("");
    cases.push((no_id, "no id or no name"));
    let mut not_a_number = made_up("isf:made_up");
    not_a_number.parameters.to_mut()[0].default = degrees(f32::NAN);
    cases.push((not_a_number, "begins as a number that is not one"));
    let mut not_a_pair = made_up("isf:made_up");
    not_a_pair.parameters.to_mut()[0].default = PropertyValue::Pair(0.0, f32::INFINITY);
    cases.push((not_a_pair, "begins as a number that is not one"));
    let mut backwards_range = made_up("isf:made_up");
    backwards_range.parameters.to_mut()[0].range = Some((1.0, 0.0));
    cases.push((backwards_range, "not a least that is no more than a most"));
    let mut infinite_range = made_up("isf:made_up");
    infinite_range.parameters.to_mut()[0].range = Some((0.0, f32::INFINITY));
    cases.push((infinite_range, "not a least that is no more than a most"));
    let mut too_many = made_up("isf:made_up");
    let one = too_many.parameters[0].clone();
    too_many.parameters = Cow::Owned(
        (0..=MOST_VALUES_OF_AN_EFFECT)
            .map(|number| ParameterDescriptor {
                id: Cow::Owned(format!("value_{number}")),
                ..one.clone()
            })
            .collect(),
    );
    cases.push((too_many, "255 are the most there can be"));

    for (snapshot, wanted) in cases {
        let reason = EffectKind::from_snapshot(snapshot.clone()).unwrap_err();
        assert!(reason.contains(wanted), "{:?}: {reason}", snapshot.key);
        assert_eq!(snapshot.problem(), Some(reason));
    }
    assert!(made_up("isf:made_up").problem().is_none());
}

#[test]
fn the_most_values_an_effect_can_have_is_still_a_descriptor_that_is_made_and_has_places_for_all_its_values() {
    let mut most = made_up("isf:the_most_values");
    let one = most.parameters[0].clone();
    most.parameters = Cow::Owned(
        (0..MOST_VALUES_OF_AN_EFFECT)
            .map(|number| ParameterDescriptor {
                id: Cow::Owned(format!("value_{number}")),
                name: Cow::Owned(format!("value {number}")),
                ..one.clone()
            })
            .collect(),
    );

    let kind = EffectKind::from_snapshot(most).unwrap();

    let effect = Effect::new(kind);
    assert_eq!(effect.properties().count(), MOST_VALUES_OF_AN_EFFECT);
    assert_eq!(
        effect.property(MOST_VALUES_OF_AN_EFFECT - 1),
        Some(PropertyId::effect_value(kind, EffectInstanceId::FIRST, 254))
    );
    assert_eq!(effect.property(MOST_VALUES_OF_AN_EFFECT), None);
}

// --- a kind of effect that the program does not have ---

#[test]
fn what_a_kind_that_a_file_brought_says_of_its_values_is_what_the_values_are_and_are_called_and_begin_as() {
    let kind = EffectKind::from_snapshot(the_descriptor_of_an_effect_the_program_does_not_have(
        "isf:every_kind",
        &[
            ("amount", degrees(0.25)),
            ("point", PropertyValue::Pair(0.5, 0.75)),
            ("tint", PropertyValue::Colour(TextColour::new(1, 2, 3, 4))),
            ("on", PropertyValue::Flag(true)),
            ("caption", PropertyValue::Words("words".to_owned())),
        ],
    ))
    .unwrap();

    assert_eq!(kind.value_count(), 5);
    let kinds: Vec<Option<ValueKind>> = (0..6).map(|slot| kind.value_kind(slot)).collect();
    assert_eq!(
        kinds,
        [
            Some(ValueKind::Number),
            Some(ValueKind::Pair),
            Some(ValueKind::Colour),
            Some(ValueKind::Flag),
            Some(ValueKind::Words),
            None
        ]
    );
    assert_eq!(kind.slot_of_the_value_with_id("tint"), Some(2));
    assert_eq!(kind.slot_of_the_value_with_id("nothing"), None);
    let effect = Effect::new(kind);
    let names: Vec<&str> = effect.properties().map(PropertyId::name).collect();
    assert_eq!(names, ["amount", "point", "tint", "on", "caption"]);
    let kinds_of_the_properties: Vec<ValueKind> = effect.properties().map(PropertyId::kind).collect();
    assert_eq!(
        kinds_of_the_properties,
        [
            ValueKind::Number,
            ValueKind::Pair,
            ValueKind::Colour,
            ValueKind::Flag,
            ValueKind::Words
        ],
        "a value is the kind that its effect says, in the place it says"
    );
    assert_eq!(effect.values(), kind.default_values().as_slice());
    assert!(Effect::with_values(kind, kind.default_values()).is_some());
    assert!(
        Effect::with_values(kind, vec![degrees(1.0); 5]).is_none(),
        "not the kinds of value it has, in their places"
    );
}

#[test]
fn a_kind_that_the_program_does_not_have_changes_no_colour() {
    let kind = a_kind_of_effect_the_program_does_not_have("isf:does_nothing_here", &[("amount", degrees(0.5))]);
    let values = vec![degrees(123.456)];

    assert_eq!(
        kind.colour_matrix(&values),
        None,
        "it changes no colour: it is not there"
    );
}

#[test]
fn what_a_kind_can_be_put_on_follows_its_domain_and_whether_it_changes_each_pixel_by_itself() {
    let picture = a_kind_of_effect_the_program_does_not_have("isf:a_picture_effect", &[("amount", degrees(0.0))]);
    let mut sound_descriptor = the_descriptor_of_an_effect_the_program_does_not_have("ladspa:1/a_sound_effect", &[]);
    sound_descriptor.domain = EffectDomain::Sound;
    sound_descriptor.changes_each_pixel_by_itself = false;
    let sound = EffectKind::from_snapshot(sound_descriptor).unwrap();
    let mut blur = the_descriptor_of_an_effect_the_program_does_not_have("isf:a_blur", &[("radius", degrees(1.0))]);
    blur.changes_each_pixel_by_itself = false;
    let blur = EffectKind::from_snapshot(blur).unwrap();

    for clip in [ClipKind::Video, ClipKind::Image, ClipKind::Text] {
        assert!(picture.applies_to_a_clip_of_kind(clip), "{clip:?}");
        assert!(!sound.applies_to_a_clip_of_kind(clip), "{clip:?}");
    }
    assert!(sound.applies_to_a_clip_of_kind(ClipKind::Audio));
    for kind in [picture, sound, blur] {
        assert!(!kind.applies_to_a_clip_of_kind(ClipKind::Crate), "{kind:?}");
        assert!(kind.applies_to_a_track(), "{kind:?}");
        assert!(
            kind.applies_to_a_group_of_kind(GroupKind::Tag),
            "{kind:?}: a tag has it applied to each member"
        );
    }
    assert!(picture.applies_to_a_group_of_kind(GroupKind::ClipGroup));
    assert!(picture.applies_to_a_group_of_kind(GroupKind::TrackGroup));
    assert!(
        !blur.applies_to_a_group_of_kind(GroupKind::ClipGroup),
        "a blur of each member is not a blur of the group"
    );
    assert!(!blur.applies_to_a_group_of_kind(GroupKind::TrackGroup));
    assert!(
        !sound.applies_to_a_group_of_kind(GroupKind::ClipGroup),
        "a sound effect is not for a picture of a group"
    );
}

#[test]
fn the_values_of_a_kind_that_a_file_brought_are_properties_of_the_clips_and_tracks_that_have_it_and_only_those() {
    let kind = a_kind_of_effect_the_program_does_not_have("isf:has_values", &[("amount", degrees(0.5))]);
    let mut chain = Effects::none();
    let instance = chain.add(kind);
    let property = PropertyId::effect_value(kind, instance, 0);

    assert!(property.is_had_by_a_clip(ClipKind::Video, &chain));
    assert!(
        !property.is_had_by_a_clip(ClipKind::Audio, &chain),
        "a picture effect is not on a sound"
    );
    assert!(property.is_had_by_a_track(&chain));
    assert!(!property.is_had_by_a_track(&Effects::none()));
    assert_eq!(chain.value_of(property), Some(degrees(0.5)));
}

// --- what is not a field of the descriptor ---

#[test]
fn what_a_snapshot_says_that_the_program_does_not_know_is_in_the_descriptor_and_makes_it_another_kind() {
    let mut with_unknown = made_up("isf:with_unknown_fields");
    with_unknown.unknown_fields =
        UnknownFields::new(BTreeMap::from([("inputs".to_owned(), serde_json::json!([1, 2]))]));

    let without = EffectKind::from_snapshot(made_up("isf:with_unknown_fields")).unwrap();
    let with = EffectKind::from_snapshot(with_unknown).unwrap();

    assert_ne!(with, without, "what is in it is kept, so it is not the same descriptor");
    assert_eq!(
        with.descriptor().unknown_fields.fields().get("inputs"),
        Some(&serde_json::json!([1, 2]))
    );
    assert!(without.descriptor().unknown_fields.is_empty());
}

#[test]
fn what_a_file_said_of_an_effect_that_the_program_does_not_know_is_kept_by_the_effect_through_every_change_of_its_values()
 {
    let kind = a_kind_of_effect_the_program_does_not_have("isf:keeps_unknown_fields", &[("amount", degrees(0.5))]);
    let unknown = UnknownFields::new(BTreeMap::from([("state".to_owned(), serde_json::json!({"seed": 4}))]));
    let effect = Effect::new(kind).with_unknown_fields(unknown.clone());
    let property = effect.property(0).unwrap();

    let changed = effect.with_value(property, &degrees(0.9)).unwrap();
    let renamed = changed.clone().named(EffectInstanceId::new(3));

    assert_eq!(effect.unknown_fields(), &unknown);
    assert_eq!(changed.unknown_fields(), &unknown);
    assert_eq!(renamed.unknown_fields(), &unknown);
    assert_ne!(
        changed,
        Effect::new(kind).with_value(property, &degrees(0.9)).unwrap(),
        "it is a part of what the effect is"
    );
}

#[test]
fn a_snapshot_of_another_key_is_not_an_effect_of_the_program_however_little_it_has_and_whatever_its_major_number_is() {
    let own = EffectKind::HUE_SHIFT.descriptor();
    let mut with_a_value_that_the_hue_shift_has = own.clone();
    with_a_value_that_the_hue_shift_has.key = Cow::Borrowed("isf:looks_like_the_hue_shift");
    let mut with_no_values = with_a_value_that_the_hue_shift_has.clone();
    with_no_values.key = Cow::Borrowed("isf:looks_like_the_hue_shift_with_no_values");
    with_no_values.parameters.to_mut().clear();

    for snapshot in [with_a_value_that_the_hue_shift_has, with_no_values] {
        let key = snapshot.key.to_string();
        let made = EffectKind::from_snapshot(snapshot).unwrap();
        assert_ne!(made, EffectKind::HUE_SHIFT, "{key}");
        assert!(!made.is_installed(), "{key}");
        assert_eq!(made.key(), key);
    }
}

#[test]
fn a_sound_effect_is_for_sounds_and_for_no_picture_and_its_values_are_the_properties_of_a_sound_and_of_nothing_seen() {
    let mut descriptor =
        the_descriptor_of_an_effect_the_program_does_not_have("ladspa:2/a_sound_effect", &[("gain", degrees(0.0))]);
    descriptor.domain = EffectDomain::Sound;
    descriptor.changes_each_pixel_by_itself = true;
    let sound = EffectKind::from_snapshot(descriptor).unwrap();
    let property = PropertyId::effect_value(sound, EffectInstanceId::FIRST, 0);

    assert!(property.applies_to_a_clip_of_kind(ClipKind::Audio));
    for seen in [ClipKind::Video, ClipKind::Image, ClipKind::Text, ClipKind::Crate] {
        assert!(!property.applies_to_a_clip_of_kind(seen), "{seen:?}");
    }
    assert!(property.applies_to_a_track());
    // (An effect that changes each pixel by itself is of pictures: what it changes is not in a sound, so a group of pictures does not have it.)
    assert!(!sound.applies_to_a_group_of_kind(GroupKind::ClipGroup));
    assert!(!sound.applies_to_a_group_of_kind(GroupKind::TrackGroup));
    assert!(sound.applies_to_a_group_of_kind(GroupKind::Tag));
}
