// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of a source file, said from the menu of a clip made from it (phase 3, piece 3.6): what the menu offers, that the window of the file's colour space opens, that
//! a choice in it is an edit of the project (one step of the history, however many choices in a row are made on that file), and that the preview is told.

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{VIDEO, Window, project_of};
use crate::application::colour_of_a_file_window::{CURVE_LABEL, MATRIX_LABEL, PRIMARIES_LABEL, RANGE_LABEL};
use crate::application::timeline_panel::COLOUR_OF_THE_FILE_LABEL;
use crate::colour_space::{TransferFunction, VideoMatrix};
use crate::project::InputColourOverride;

fn window_with_a_video() -> Window {
    let mut window = Window::showing(project_of(60));
    window.application_mut().note_everything_is_saved();
    window
}

/// Opens the menu of the video's clip and chooses the item that opens the window of its colour space.
fn open_the_colour_window(window: &mut Window) {
    let point = window.middle_of_the_strip_at(0, 30);
    window.right_click(point);
    window.harness.get_by_label(COLOUR_OF_THE_FILE_LABEL).click();
    window.settle();
}

/// In the window of the colour space, the drop-down list that shows `shown`, opened, and `option` chosen.
fn choose(window: &mut Window, shown: &str, option: &str) {
    window.harness.get_by_value(shown).click();
    window.settle();
    window.harness.get_by_label(option).click();
    window.settle();
}

#[test]
fn the_menu_of_a_clip_of_a_video_offers_the_window_of_its_colour_space_and_opening_a_menu_changes_nothing() {
    let mut window = window_with_a_video();
    let before = window.application().project().clone();

    let point = window.middle_of_the_strip_at(0, 30);
    window.right_click(point);

    let item = window.harness.get_by_label(COLOUR_OF_THE_FILE_LABEL);
    assert!(!item.accesskit_node().is_disabled());
    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn choosing_the_item_opens_a_window_with_the_four_lines_and_opening_it_changes_nothing() {
    let mut window = window_with_a_video();
    let before = window.application().project().clone();

    open_the_colour_window(&mut window);

    window.harness.get_by_label("Colour space of holiday.mp4");
    for label in [MATRIX_LABEL, RANGE_LABEL, PRIMARIES_LABEL, CURVE_LABEL] {
        window.harness.get_by_label(label);
    }
    window.harness.get_by_value("Automatic (BT.709)");
    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn a_choice_in_the_window_is_an_edit_of_the_project_that_undo_takes_back() {
    let mut window = window_with_a_video();
    open_the_colour_window(&mut window);

    choose(&mut window, "Automatic (BT.709)", "BT.601");

    assert_eq!(
        window.application().project().source_colour(VIDEO),
        Some(InputColourOverride {
            matrix: Some(VideoMatrix::Bt601),
            ..InputColourOverride::AUTOMATIC
        })
    );
    assert!(window.application().has_unsaved_changes());
    assert!(window.application().can_undo());

    window.press_key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!(
        window.application().project().source_colour(VIDEO),
        Some(InputColourOverride::AUTOMATIC)
    );
    assert!(!window.application().can_undo(), "it was one step");
}

#[test]
fn choices_in_a_row_on_the_same_file_are_one_step_of_the_history() {
    let mut window = window_with_a_video();
    open_the_colour_window(&mut window);

    choose(&mut window, "Automatic (BT.709)", "BT.601");
    choose(&mut window, "Automatic (sRGB)", "Gamma 2.4 (BT.1886)");

    let said = window.application().project().source_colour(VIDEO).unwrap();
    assert_eq!(said.matrix, Some(VideoMatrix::Bt601));
    assert_eq!(said.curve, Some(TransferFunction::Gamma24));

    window.press_key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!(
        window.application().project().source_colour(VIDEO),
        Some(InputColourOverride::AUTOMATIC),
        "both choices are taken back by one undo"
    );
}

#[test]
fn the_window_can_be_closed_and_what_was_said_stays_said() {
    let mut window = window_with_a_video();
    open_the_colour_window(&mut window);
    choose(&mut window, "Automatic (BT.709)", "BT.2020");

    // (The cross of the window.)
    window.harness.get_by_label("Close window").click();
    window.settle();

    assert!(
        window.harness.query_by_label(MATRIX_LABEL).is_none(),
        "the window is gone"
    );
    assert_eq!(
        window.application().project().source_colour(VIDEO).unwrap().matrix,
        Some(VideoMatrix::Bt2020)
    );
}
