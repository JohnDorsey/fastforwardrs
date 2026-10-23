// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effect that is what a text clip is made of (phase 9.1, `docs/engine-phase-9-design.md`, T2 and T3): `builtin:text`, with the values that a text object has and the controls that a text
//! needs, which is **not an effect that anything is offered**: it is found by its key, it is installed, and only a text clip can have it.

use super::*;
use crate::project::GroupKind;
use crate::project::animation::{PropertyValue, ValueKind};
use crate::project::tracks::{ClipKind, FontSize, TextColour, TextObject};

#[test]
fn the_text_effect_has_the_values_that_a_text_object_has_and_begins_as_a_new_text_object() {
    let kind = EffectKind::TEXT;
    assert_eq!(kind.key(), super::built_in::TEXT_KEY);
    assert_eq!(kind.key(), "builtin:text");
    assert_eq!(kind.version(), (1, 0));
    assert_eq!(kind.domain(), EffectDomain::Picture);
    assert_eq!(kind.name(), "text");
    assert_eq!(kind.descriptor().problem(), None);
    let ids: Vec<&str> = kind.parameters().iter().map(|parameter| &*parameter.id).collect();
    assert_eq!(
        ids,
        ["text", "font", "size", "colour"],
        "the order that a text object has them in"
    );
    let kinds: Vec<ValueKind> = kind.parameters().iter().map(ParameterDescriptor::value_kind).collect();
    assert_eq!(
        kinds,
        [ValueKind::Words, ValueKind::Words, ValueKind::Number, ValueKind::Colour]
    );
    let new = TextObject::default();
    assert_eq!(
        kind.default_values(),
        [
            // (A text is not made in a static, so the words that a new text says, `TextObject::NEW_TEXT`, are put in by whoever makes the clip, as the rest of a new text object is.)
            PropertyValue::Words(String::new()),
            PropertyValue::Words(new.font_family),
            PropertyValue::Number(new.font_size.fraction_of_picture_height()),
            PropertyValue::Colour(new.colour),
        ],
        "a new text effect says what a new text object says, but for the words"
    );
    assert_eq!(
        kind.default_values()[3],
        PropertyValue::Colour(TextColour::WHITE),
        "(and that is white)"
    );
}

#[test]
fn the_controls_of_the_text_effect_are_a_box_of_lines_a_list_of_fonts_the_size_in_points_and_a_colour() {
    let controls: Vec<ParameterControl> = EffectKind::TEXT
        .parameters()
        .iter()
        .map(|parameter| parameter.control.clone())
        .collect();
    assert_eq!(
        controls,
        [
            ParameterControl::Multiline,
            ParameterControl::Font,
            ParameterControl::PointsOfThePictureHeight,
            ParameterControl::Automatic,
        ]
    );
    let size = &EffectKind::TEXT.parameters()[2];
    assert_eq!(
        size.range,
        Some((FontSize::SMALLEST_FRACTION, FontSize::LARGEST_FRACTION)),
        "the sizes that a text object can have, as the fractions of the picture's height that are kept"
    );
    assert_eq!(&*size.unit, "pt");
}

#[test]
fn the_text_effect_is_not_one_of_those_that_are_offered_and_is_found_by_its_key_and_stands_in_for_its_snapshot() {
    assert!(
        !EffectKind::BUILT_IN.contains(&EffectKind::TEXT),
        "the effects that everything else is offered are not a list that has a text in it"
    );
    assert!(!EffectKind::offered().contains(&EffectKind::TEXT));
    assert_eq!(EffectKind::built_in_with_key("builtin:text"), Some(EffectKind::TEXT));
    assert!(EffectKind::TEXT.is_installed());
    assert!(!EffectKind::TEXT.is_a_generator(), "it is not a shader");
    assert_eq!(
        EffectKind::from_snapshot(EffectKind::TEXT.descriptor().clone()),
        Ok(EffectKind::TEXT),
        "a file that has a snapshot of it is given the effect of the program"
    );
    // (A snapshot of a text effect with fewer values, which an older program would have written, is still the effect of the program: the others begin as they begin.)
    let mut fewer = EffectKind::TEXT.descriptor().clone();
    fewer.parameters.to_mut().truncate(2);
    assert_eq!(EffectKind::from_snapshot(fewer), Ok(EffectKind::TEXT));
}

#[test]
fn only_a_text_clip_can_have_the_text_effect() {
    assert!(EffectKind::TEXT.applies_to_a_clip_of_kind(ClipKind::Text));
    for kind in [
        ClipKind::Video,
        ClipKind::Audio,
        ClipKind::Image,
        ClipKind::Crate,
        ClipKind::Adjustment,
        ClipKind::Generator,
    ] {
        assert!(!EffectKind::TEXT.applies_to_a_clip_of_kind(kind), "{kind:?}");
    }
    assert!(!EffectKind::TEXT.applies_to_a_track());
    for kind in [GroupKind::Tag, GroupKind::ClipGroup, GroupKind::TrackGroup] {
        assert!(!EffectKind::TEXT.applies_to_a_group_of_kind(kind), "{kind:?}");
    }
}

#[test]
fn the_text_effect_asks_nothing_of_a_picture_because_it_is_what_makes_the_picture() {
    assert!(
        EffectKind::TEXT
            .picture_operation(&EffectKind::TEXT.default_values())
            .is_none()
    );
    assert!(
        EffectKind::TEXT
            .colour_matrix(&EffectKind::TEXT.default_values())
            .is_none()
    );
}

/// The text effect with the control of its value number `slot` changed to `control`.
fn with_the_control(slot: usize, control: ParameterControl) -> EffectDescriptor {
    let mut descriptor = EffectKind::TEXT.descriptor().clone();
    descriptor.parameters.to_mut()[slot].control = control;
    descriptor
}

#[test]
fn a_box_of_lines_and_a_list_of_fonts_are_for_words_and_the_size_in_points_is_for_a_number() {
    for (slot, control, is_wrong_for) in [
        (2, ParameterControl::Multiline, "a number"),
        (3, ParameterControl::Multiline, "a colour"),
        (2, ParameterControl::Font, "a number"),
        (3, ParameterControl::Font, "a colour"),
        (0, ParameterControl::PointsOfThePictureHeight, "words"),
        (3, ParameterControl::PointsOfThePictureHeight, "a colour"),
    ] {
        let problem = with_the_control(slot, control.clone())
            .problem()
            .unwrap_or_else(|| panic!("{control:?} on {is_wrong_for} is refused"));
        assert!(problem.contains(EffectKind::TEXT.key()), "{problem}");
        assert!(problem.contains(&*EffectKind::TEXT.parameters()[slot].id), "{problem}");
    }
    // (And they fit what they are for.)
    assert_eq!(with_the_control(1, ParameterControl::Multiline).problem(), None);
    assert_eq!(with_the_control(0, ParameterControl::Font).problem(), None);
    assert_eq!(
        with_the_control(2, ParameterControl::PointsOfThePictureHeight).problem(),
        None
    );
}
