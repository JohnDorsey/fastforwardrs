// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The three controls that the text effect has (phase 9.1): a box of lines for the words, a list of fonts for the font, and the size in points for a fraction of the picture's height. Each row
//! is drawn on its own (nothing else is in the window), and what it asks for when it is used is what is looked at.

use egui::accesskit::{Action, ActionData, ActionRequest};
use egui_kittest::kittest::{NodeT, Queryable};

use super::*;
use crate::project::EffectInstanceId;

/// The row of the value in place `slot` of the text effect, shown as `shown`, drawn alone; `interact` is done to it, and what it asked for after that is given (`None` for nothing).
fn the_row_of_the_text_effect(
    slot: u8,
    shown: PropertyValue,
    fonts: Option<&[String]>,
    interact: impl FnOnce(&mut egui_kittest::Harness<'_>),
) -> Option<PropertyValue> {
    let project = Project::empty();
    let asked = std::cell::RefCell::new(None);
    let property = PropertyId::effect_value(EffectKind::TEXT, EffectInstanceId::FIRST, slot);
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        match control_for(ui, &project, fonts, EffectKind::TEXT, property, &shown) {
            Some(Change::Value(value)) => *asked.borrow_mut() = Some(value),
            Some(Change::ChooseAFile) => panic!("a file is not chosen for a text"),
            None => {}
        }
    });
    harness.run();
    interact(&mut harness);
    harness.run();
    drop(harness);
    asked.into_inner()
}

fn set_the_value_of(harness: &mut egui_kittest::Harness<'_>, label: &str, value: f64) {
    let (target_node, target_tree) = harness.get_by_label(label).accesskit_node().locate();
    harness.event(egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::SetValue,
        target_tree,
        target_node,
        data: Some(ActionData::NumericValue(value)),
    }));
}

#[test]
fn the_words_of_the_text_are_a_box_of_lines_and_what_is_typed_is_what_the_value_becomes() {
    let nothing_typed = the_row_of_the_text_effect(0, PropertyValue::Words("Hello".to_owned()), None, |_| {});
    assert!(nothing_typed.is_none(), "a row that is only looked at asks for nothing");

    let typed = the_row_of_the_text_effect(0, PropertyValue::Words("Hello".to_owned()), None, |harness| {
        assert!(harness.query_by_label("Text").is_some(), "the row has its label");
        // (A box takes what is typed when it has the focus.)
        harness
            .query_all_by_role(egui::accesskit::Role::MultilineTextInput)
            .next()
            .expect("the words are in a box of lines")
            .focus();
        harness.run();
        harness
            .query_all_by_role(egui::accesskit::Role::MultilineTextInput)
            .next()
            .expect("the words are still in it")
            .type_text(", world");
    });
    assert_eq!(typed, Some(PropertyValue::Words("Hello, world".to_owned())));
}

#[test]
fn the_font_of_the_text_is_chosen_from_the_fonts_of_the_computer_and_no_font_is_the_default() {
    let fonts = ["Arial".to_owned(), "Courier New".to_owned()];
    let chosen = the_row_of_the_text_effect(1, PropertyValue::Words(String::new()), Some(&fonts), |harness| {
        harness.get_by_value(DEFAULT_FONT_LABEL).click();
        harness.run();
        harness.get_by_label("Courier New").click();
    });
    assert_eq!(chosen, Some(PropertyValue::Words("Courier New".to_owned())));

    let back_to_the_default =
        the_row_of_the_text_effect(1, PropertyValue::Words("Arial".to_owned()), Some(&fonts), |harness| {
            harness.get_by_value("Arial").click();
            harness.run();
            harness.get_by_label(DEFAULT_FONT_LABEL).click();
        });
    assert_eq!(
        back_to_the_default,
        Some(PropertyValue::Words(String::new())),
        "the default font is no font at all"
    );

    let unchanged = the_row_of_the_text_effect(1, PropertyValue::Words("Arial".to_owned()), Some(&fonts), |harness| {
        harness.get_by_value("Arial").click();
        harness.run();
        harness.get_by_label("Arial").click();
    });
    assert!(unchanged.is_none(), "choosing what it is already asks for nothing");
}

#[test]
fn a_font_that_the_computer_does_not_have_is_shown_as_it_is_kept_and_the_list_waits_for_the_fonts() {
    let fonts = ["Arial".to_owned()];
    let asked = the_row_of_the_text_effect(1, PropertyValue::Words("Gone".to_owned()), Some(&fonts), |harness| {
        assert!(
            harness.query_by_value("Gone").is_some(),
            "a font that is not among those there are is still the one that is chosen"
        );
    });
    assert!(asked.is_none());
    let waiting = the_row_of_the_text_effect(1, PropertyValue::Words("Arial".to_owned()), None, |harness| {
        assert!(harness.query_by_label(LOOKING_FOR_THE_FONTS_NOTE).is_some());
        assert!(
            harness.query_by_value("Arial").is_none(),
            "no list is shown while there are no fonts to list"
        );
    });
    assert!(waiting.is_none());
}

#[test]
fn the_size_is_shown_in_points_on_a_picture_1080_tall_and_kept_as_a_fraction_of_its_height() {
    let seen = std::cell::Cell::new(0.0);
    the_row_of_the_text_effect(2, PropertyValue::Number(0.06), None, |harness| {
        seen.set(harness.get_by_label("Size").accesskit_node().numeric_value().unwrap());
    });
    assert!((seen.get() - 64.8).abs() < 0.01, "{} points: 0.06 of 1080", seen.get());

    let changed = the_row_of_the_text_effect(2, PropertyValue::Number(0.06), None, |harness| {
        set_the_value_of(harness, "Size", 120.0);
    });
    let Some(PropertyValue::Number(fraction)) = changed else {
        panic!("a number was asked for: {changed:?}");
    };
    assert!(
        (fraction - 120.0 / 1080.0).abs() < 1e-5,
        "120 points are {fraction} of the height of the picture"
    );

    // A size that is kept past what a text can have (a hand-written file) is shown as the size that is made of it, which is brought back inside, as a text object has always done; and a
    // number that is not a number is the default size.
    for (kept, shown_as) in [(0.9, 540.0), (0.0, 10.8), (f32::NAN, 64.8)] {
        let seen = std::cell::Cell::new(0.0);
        the_row_of_the_text_effect(2, PropertyValue::Number(kept), None, |harness| {
            seen.set(harness.get_by_label("Size").accesskit_node().numeric_value().unwrap());
        });
        assert!(
            (seen.get() - shown_as).abs() < 0.01,
            "{kept} is shown as {} points",
            seen.get()
        );
    }
    // A size past what a text can have is brought back inside, as a text object has always done.
    let too_big = the_row_of_the_text_effect(2, PropertyValue::Number(0.06), None, |harness| {
        set_the_value_of(harness, "Size", 100_000.0);
    });
    assert_eq!(too_big, Some(PropertyValue::Number(FontSize::LARGEST_FRACTION)));
}
