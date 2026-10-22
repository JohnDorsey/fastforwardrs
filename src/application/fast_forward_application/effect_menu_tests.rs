// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The surface of the effects (phase 4, piece 4.7): the menu of the plus button with its search field and its categories, the controls that are made from what an effect says of
//! its values (a list of choices, a switch, a table), the duplicate item in the menu of a box, and what the box says of a table that cannot be used.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};

use super::keyframe_tests::{keyframes_of, window_with_a_video};
use super::preview_layer_tests::select;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::file_chooser::ScriptedFileChooser;
use crate::application::naming::what_an_effect_is_called;
use crate::application::properties_panel::{
    ADD_EFFECT_LABEL, CHOOSE_A_TABLE_LABEL, DUPLICATE_EFFECT_LABEL, KeyframeButton, NO_TABLE_CHOSEN_NOTE,
    TABLE_NOT_IN_THE_PROJECT_NOTE, keyframe_button_label,
};
use crate::project::{
    AnimationTarget, ClipId, EffectInstanceId, EffectKind, Member, PropertyId, PropertyValue, ResourceKind,
    SourceMediaLink,
};

const SEARCH_EFFECTS_HINT: &str = "Search effects";

const HEADINGS: [&str; 5] = ["Colour", "Light", "Blur and sharpen", "Transform", "Looks"];

fn window_with_the_video_selected() -> (Window, ClipId) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    (window, clip)
}

/// Opens the menu of the plus button. (It has a smaller menu for each place effects come from: see [`open_the_smaller_menu_of_the_program`].)
fn open_the_menu(window: &mut Window) {
    let plus = window.rectangle_of(ADD_EFFECT_LABEL).center();
    window.click(plus);
}

/// Opens the smaller menu of the effects of the program, in the menu of the plus button, which is open.
fn open_the_smaller_menu_of_the_program(window: &mut Window) {
    window.open_the_smaller_menu("Built-in");
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

/// Whether the search field of the menu is shown (the window has other fields of text: the title of a track, for one; this is the one that says what it is for).
fn search_field_is_shown(window: &Window) -> bool {
    window
        .harness
        .query_all_by_role(Role::TextInput)
        .any(|field| field.accesskit_node().placeholder() == Some(SEARCH_EFFECTS_HINT))
}

fn type_in_the_search(window: &mut Window, text: &str) {
    window
        .harness
        .query_all_by_role(Role::TextInput)
        .find(|field| field.accesskit_node().placeholder() == Some(SEARCH_EFFECTS_HINT))
        .expect("the search field")
        .type_text(text);
    window.harness.run();
}

/// What the lists of choices of the box of a look-up table say, from the top.
fn the_choices_shown(window: &Window) -> Vec<(String, String)> {
    let mut lists: Vec<_> = window.harness.query_all_by_role(Role::ComboBox).collect();
    lists.sort_by(|a, b| a.rect().top().total_cmp(&b.rect().top()));
    lists
        .iter()
        .map(|list| {
            (
                list.accesskit_node().label().unwrap_or_default().to_owned(),
                list.accesskit_node().value().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn the_effects(window: &Window, clip: ClipId) -> Vec<EffectKind> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect()
}

// --- the menu ---

#[test]
fn the_menu_has_a_field_to_search_the_effects_the_effects_under_the_headings_of_their_categories_and_an_item_for_a_file()
 {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();

    open_the_menu(&mut window);

    assert!(search_field_is_shown(&window), "the search field");
    assert!(
        is_shown(&window, "Built-in \u{23f5}"),
        "a smaller menu for the effects of the program"
    );
    assert!(!is_shown(&window, "Colour"), "and the effects are in it, not here");
    open_the_smaller_menu_of_the_program(&mut window);
    let tops: Vec<f32> = HEADINGS
        .iter()
        .map(|heading| {
            assert!(is_shown(&window, heading), "the heading {heading}");
            window.rectangle_of(heading).top()
        })
        .collect();
    assert!(
        tops.windows(2).all(|pair| pair[0] < pair[1]),
        "in the order of the categories: {tops:?}"
    );
    for kind in EffectKind::BUILT_IN {
        assert!(is_shown(&window, &what_an_effect_is_called(kind)), "{kind}");
    }
    // Each effect is under its own heading.
    let top = |label: &str| window.rectangle_of(label).top();
    assert!(top("Colour") < top("Hue shift") && top("Hue shift") < top("Light"));
    assert!(top("Blur and sharpen") < top("Blur") && top("Blur") < top("Transform"));
    assert!(top("Transform") < top("Crop") && top("Looks") < top("Look-up table"));
    assert!(is_shown(&window, "Add effect from file..."));
    assert!(
        top("Built-in \u{23f5}") < top("Add effect from file..."),
        "(it is at the bottom of the menu, under the smaller menus)"
    );
}

#[test]
fn what_is_typed_in_the_search_field_narrows_the_menu_to_the_effects_that_match_by_name_description_or_category() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu(&mut window);

    type_in_the_search(&mut window, "blur");
    open_the_smaller_menu_of_the_program(&mut window);
    assert!(is_shown(&window, "Blur"));
    assert!(!is_shown(&window, "Hue shift") && !is_shown(&window, "Crop"));
    assert!(is_shown(&window, "Blur and sharpen"), "its heading stays");
    assert!(
        !is_shown(&window, "Colour") && !is_shown(&window, "Transform"),
        "and the others do not"
    );
    assert!(
        is_shown(&window, "Add effect from file..."),
        "(the file item is always there)"
    );
}

#[test]
fn the_search_is_by_the_description_and_the_category_and_not_by_the_case_of_the_letters() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu(&mut window);

    // "LUT" is in the description of the look-up table and in no name.
    type_in_the_search(&mut window, "LUT");
    open_the_smaller_menu_of_the_program(&mut window);
    assert!(is_shown(&window, "Look-up table"));
    assert!(!is_shown(&window, "Hue shift") && !is_shown(&window, "Blur"));
}

#[test]
fn what_nothing_matches_is_said_and_the_menu_has_no_effect_to_choose() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu(&mut window);

    type_in_the_search(&mut window, "zzzz");

    assert!(is_shown(&window, "No effect matches"));
    assert!(
        !is_shown(&window, "Built-in \u{23f5}"),
        "no smaller menu is left: nothing is in it"
    );
    for kind in EffectKind::BUILT_IN {
        assert!(!is_shown(&window, &what_an_effect_is_called(kind)), "{kind}");
    }
}

#[test]
fn enter_adds_the_first_effect_that_matches_and_closes_the_menu() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);

    type_in_the_search(&mut window, "sat");
    window.harness.key_press(Key::Enter);
    window.settle();

    assert_eq!(the_effects(&window, clip), [EffectKind::SATURATION]);
    assert!(!search_field_is_shown(&window), "the menu is closed");
}

#[test]
fn enter_with_nothing_that_matches_adds_nothing() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);

    type_in_the_search(&mut window, "zzzz");
    window.harness.key_press(Key::Enter);
    window.settle();

    assert!(the_effects(&window, clip).is_empty());
}

#[test]
fn the_menu_opens_with_the_field_empty_whatever_was_typed_the_last_time() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();
    open_the_menu(&mut window);
    type_in_the_search(&mut window, "blur");
    // Closed by a click on nothing, and opened again.
    window.click(egui::pos2(900.0, 600.0));
    window.settle();
    open_the_menu(&mut window);
    open_the_smaller_menu_of_the_program(&mut window);

    assert!(is_shown(&window, "Hue shift"), "everything is offered again");
}

#[test]
fn choosing_an_effect_with_the_pointer_still_adds_it() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    open_the_menu(&mut window);
    open_the_smaller_menu_of_the_program(&mut window);

    let item = window.rectangle_of("Exposure").center();
    window.click(item);

    assert_eq!(the_effects(&window, clip), [EffectKind::EXPOSURE]);
}

#[test]
fn the_item_for_a_file_asks_for_one_and_a_menu_that_cannot_have_a_look_up_table_has_no_such_item() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, _) = window_with_the_video_selected();
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(None);
    window.use_file_chooser(&chooser);
    open_the_menu(&mut window);

    let item = window.rectangle_of("Add effect from file...").center();
    window.click(item);

    assert_eq!(chooser.questions_asked().len(), 1);
    // (Rewritten on purpose in phase 6: the file may be a shader as well as a table.)
    assert_eq!(chooser.questions_asked()[0].title, "Choose a file for an effect");
}

// --- the controls ---

fn list_called(window: &Window, label: &str) -> egui::Pos2 {
    window
        .harness
        .query_all_by_role(Role::ComboBox)
        .find(|list| list.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no list is called {label}"))
        .rect()
        .center()
}

fn put_a_look_up_table_on(window: &mut Window, clip: ClipId) {
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::LUT,
    });
}

fn value_of_the_look_up_table(window: &Window, clip: ClipId, slot: u8) -> PropertyValue {
    let property = PropertyId::effect_value(EffectKind::LUT, EffectInstanceId::FIRST, slot);
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .instance(EffectInstanceId::FIRST)
        .unwrap()
        .value_of(property)
        .unwrap()
        .clone()
}

#[test]
fn the_look_up_table_has_a_control_for_each_value_made_from_what_the_effect_says_of_it() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    put_a_look_up_table_on(&mut window, clip);

    assert!(is_shown(&window, "Look-up table"), "the box");
    assert!(
        is_shown(&window, "Table")
            && is_shown(&window, NO_TABLE_CHOSEN_NOTE)
            && is_shown(&window, CHOOSE_A_TABLE_LABEL)
    );
    assert_eq!(
        window
            .harness
            .get_by_label("Intensity")
            .accesskit_node()
            .numeric_value(),
        Some(1.0),
        "a number with a range is a slider"
    );
    for label in ["Input space", "Output space", "Interpolation"] {
        assert!(is_shown(&window, label), "{label}");
    }
    assert_eq!(
        the_choices_shown(&window),
        [
            ("Input space".to_owned(), "Screen codes (sRGB)".to_owned()),
            ("Output space".to_owned(), "Screen codes (sRGB)".to_owned()),
            ("Interpolation".to_owned(), "Tetrahedral".to_owned()),
        ],
        "a choice is a list that shows the option it has"
    );
}

#[test]
fn choosing_an_option_from_the_list_sets_the_value_to_the_id_of_the_option() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    put_a_look_up_table_on(&mut window, clip);

    let list = list_called(&window, "Interpolation");
    window.click(list);
    let option = window.rectangle_of("Trilinear").center();
    window.click(option);

    assert_eq!(
        value_of_the_look_up_table(&window, clip, 4),
        PropertyValue::Words("trilinear".to_owned())
    );
    // The input space is the first of the two lists.
    let first_list = list_called(&window, "Input space");
    window.click(first_list);
    let option = window.rectangle_of("Linear light (the working space)").center();
    window.click(option);
    assert_eq!(
        value_of_the_look_up_table(&window, clip, 2),
        PropertyValue::Words("linear".to_owned())
    );
    assert_eq!(
        value_of_the_look_up_table(&window, clip, 3),
        PropertyValue::Words("screen".to_owned()),
        "and the output space is as it was"
    );
}

#[test]
fn a_value_that_is_a_choice_has_no_buttons_of_keyframes_and_a_number_still_has() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    put_a_look_up_table_on(&mut window, clip);

    let stopwatch_of = |name: &str| {
        window
            .harness
            .query_by_label(&keyframe_button_label(name, KeyframeButton::Stopwatch, false))
            .is_some()
    };
    assert!(stopwatch_of("intensity of the LUT"), "a number has one");
    for name in [
        "input space of the LUT",
        "output space of the LUT",
        "interpolation of the LUT",
        "table of the LUT",
    ] {
        assert!(!stopwatch_of(name), "{name}: it holds until it is changed");
    }
}

#[test]
fn the_button_of_the_table_asks_for_a_file_for_that_effect() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    put_a_look_up_table_on(&mut window, clip);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("grade.cube");
    std::fs::write(&path, "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
    let chooser = ScriptedFileChooser::default();
    chooser.will_answer(Some(path));
    window.use_file_chooser(&chooser);

    let button = window.rectangle_of(CHOOSE_A_TABLE_LABEL).center();
    window.click(button);

    assert_eq!(
        value_of_the_look_up_table(&window, clip, 0),
        PropertyValue::Words("grade.cube".to_owned())
    );
    assert_eq!(the_effects(&window, clip), [EffectKind::LUT], "no new effect");
    assert!(is_shown(&window, "grade.cube"), "the name of the table is shown");
    assert!(!is_shown(&window, NO_TABLE_CHOSEN_NOTE));
}

#[test]
fn a_table_the_project_does_not_have_or_cannot_read_says_so_in_the_box_of_the_effect() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    put_a_look_up_table_on(&mut window, clip);
    let table = PropertyId::effect_value(EffectKind::LUT, EffectInstanceId::FIRST, 0);
    let name_the_table = |window: &mut Window, name: &str| {
        window.do_this(Command::SetEffectValue {
            target: AnimationTarget::Clip(clip),
            property: table,
            value: PropertyValue::Words(name.to_owned()),
        });
    };

    name_the_table(&mut window, "absent.cube");
    assert!(is_shown(&window, TABLE_NOT_IN_THE_PROJECT_NOTE));

    window
        .application_mut()
        .document
        .project
        .add_resource(
            "broken.cube",
            ResourceKind::Cube,
            std::sync::Arc::from(&b"LUT_3D_SIZE two\n"[..]),
            SourceMediaLink::from_absolute_path(std::path::PathBuf::from("/x/broken.cube")),
        )
        .unwrap();
    name_the_table(&mut window, "broken.cube");
    assert!(!is_shown(&window, TABLE_NOT_IN_THE_PROJECT_NOTE));
    assert!(is_shown(
        &window,
        "broken.cube: line 1: LUT_3D_SIZE needs a whole number, and \"two\" is not one"
    ));

    window
        .application_mut()
        .document
        .project
        .add_resource(
            "fine.cube",
            ResourceKind::Cube,
            std::sync::Arc::from(&b"LUT_1D_SIZE 2\n0 0 0\n1 1 1\n"[..]),
            SourceMediaLink::from_absolute_path(std::path::PathBuf::from("/x/fine.cube")),
        )
        .unwrap();
    name_the_table(&mut window, "fine.cube");
    assert!(!is_shown(&window, TABLE_NOT_IN_THE_PROJECT_NOTE));
    assert!(!window.harness.query_all_by_label_contains("fine.cube:").count() > 0);
}

// --- duplicating ---

#[test]
fn duplicate_effect_in_the_menu_of_a_box_puts_a_copy_with_the_same_values_right_after_it() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    for kind in [EffectKind::HUE_SHIFT, EffectKind::EXPOSURE, EffectKind::SATURATION] {
        window.do_this(Command::AddEffect {
            target: AnimationTarget::Clip(clip),
            kind,
        });
    }
    window.do_this(Command::SetEffectValue {
        target: AnimationTarget::Clip(clip),
        property: PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::FIRST, 0),
        value: PropertyValue::Number(77.0),
    });
    window.do_this(Command::SetEffectEnabled {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        enabled: false,
    });

    let heading = window.rectangle_of("Hue shift").center();
    window.right_click(heading);
    assert_eq!(DUPLICATE_EFFECT_LABEL, "Duplicate effect");
    let item = window.rectangle_of(DUPLICATE_EFFECT_LABEL).center();
    window.click(item);

    let effects = window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap();
    let in_order: Vec<(EffectKind, EffectInstanceId, bool)> = effects
        .iter()
        .map(|effect| (effect.kind(), effect.instance(), effect.is_enabled()))
        .collect();
    assert_eq!(
        in_order,
        [
            (EffectKind::HUE_SHIFT, EffectInstanceId::new(0), false),
            (EffectKind::HUE_SHIFT, EffectInstanceId::new(3), false),
            (EffectKind::EXPOSURE, EffectInstanceId::new(1), true),
            (EffectKind::SATURATION, EffectInstanceId::new(2), true),
        ],
        "right after the original, with the name a new effect gets, and as switched off as the original"
    );
    assert_eq!(
        effects.instance(EffectInstanceId::new(3)).unwrap().values(),
        [PropertyValue::Number(77.0)]
    );
}

#[test]
fn a_copy_of_an_effect_is_a_step_of_the_history_and_has_no_keyframes_of_the_original() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_the_video_selected();
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    let hue = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::FIRST, 0);
    window.do_this(Command::AddKeyframe {
        target: AnimationTarget::Clip(clip),
        property: hue,
        frame: crate::project::TimelineFrameIndex::new(10),
    });

    window.do_this(Command::DuplicateEffect {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
    });

    let copy = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(1), 0);
    assert!(
        window
            .application()
            .project()
            .has_the_effect(AnimationTarget::Clip(clip), EffectInstanceId::new(1))
    );
    assert_eq!(
        keyframes_of(&window, clip, hue),
        vec![10],
        "the original has its keyframe"
    );
    assert!(keyframes_of(&window, clip, copy).is_empty(), "and the copy has none");
    super::undo_tests::undo(&mut window);
    assert!(
        !window
            .application()
            .project()
            .has_the_effect(AnimationTarget::Clip(clip), EffectInstanceId::new(1))
    );
}

// --- the most effects there can be ---

fn a_clip_with_all_the_effects_it_can_have() -> (Window, ClipId) {
    let (mut window, clip) = window_with_the_video_selected();
    for _ in 0..crate::project::MOST_EFFECTS_ON_ONE_THING {
        window.do_this(Command::AddEffect {
            target: AnimationTarget::Clip(clip),
            kind: EffectKind::EXPOSURE,
        });
    }
    assert_eq!(the_effects(&window, clip).len(), 64);
    assert!(window.application().interface.dialog.is_none(), "all 64 fit");
    (window, clip)
}

#[test]
fn an_effect_that_is_added_to_a_thing_that_has_the_most_there_can_be_is_not_added_and_the_user_is_told() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = a_clip_with_all_the_effects_it_can_have();

    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::BLUR,
    });

    assert_eq!(the_effects(&window, clip).len(), 64, "nothing was added");
    match &window.application().interface.dialog {
        Some(crate::application::dialogs::Dialog::Message { title, text }) => {
            assert_eq!(title, "The effect cannot be added");
            assert!(text.contains("at most 64 effects"), "{text}");
        }
        other => panic!("the user is told: {other:?}"),
    }
}

#[test]
fn a_copy_of_an_effect_of_a_thing_that_has_the_most_there_can_be_is_not_made_and_the_user_is_told() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = a_clip_with_all_the_effects_it_can_have();

    window.do_this(Command::DuplicateEffect {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
    });

    assert_eq!(the_effects(&window, clip).len(), 64);
    assert!(matches!(
        &window.application().interface.dialog,
        Some(crate::application::dialogs::Dialog::Message { title, .. }) if title == "The effect cannot be copied"
    ));
}
