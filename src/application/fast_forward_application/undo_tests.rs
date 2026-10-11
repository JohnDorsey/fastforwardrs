//! Tests of undo and redo in the real window: the keys, the Edit menu, what one step takes back, and
//! what is not an edit.

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_cut_into_three, project_of};
use crate::application::commands::Command;
use crate::project::{ProjectSettings, SpeedFactor};

/// A window showing a video of 90 frames cut into three clips of 30 (so that all of it is in view),
/// with nothing to undo and nothing unsaved.
fn window_with_three_clips() -> Window {
    let mut window = Window::showing(project_cut_into_three(90));
    window.application_mut().note_everything_is_saved();
    window.settle();
    window
}

fn press_with_control(window: &mut Window, key: Key) {
    window.harness.key_press_modifiers(Modifiers::COMMAND, key);
    window.harness.step();
    window.settle();
}

fn press_with_control_and_shift(window: &mut Window, key: Key) {
    window
        .harness
        .key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), key);
    window.harness.step();
    window.settle();
}

fn undo(window: &mut Window) {
    press_with_control(window, Key::Z);
}

fn redo(window: &mut Window) {
    press_with_control(window, Key::Y);
}

fn select_the_clip_at(window: &mut Window, frame: u64) {
    let point = window.middle_of_the_track_at(frame);
    window.click(point);
}

// --- the keys ---

#[test]
fn control_z_takes_back_a_cut_and_control_y_and_control_shift_z_do_it_again() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(45);
    assert_eq!(window.clips().len(), 4);

    undo(&mut window);
    assert_eq!(window.clips().len(), 3, "the cut is taken back");
    assert!(window.application().can_redo());

    redo(&mut window);
    assert_eq!(window.clips().len(), 4, "and done again with Ctrl+Y");

    undo(&mut window);
    press_with_control_and_shift(&mut window, Key::Z);
    assert_eq!(window.clips().len(), 4, "and with Ctrl+Shift+Z too");
}

#[test]
fn undoing_goes_back_through_the_edits_one_at_a_time_in_the_reverse_order() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(15);
    window.cut_at_frame(45);
    window.cut_at_frame(75);
    assert_eq!(window.clips().len(), 6);

    for expected in [5, 4, 3] {
        undo(&mut window);
        assert_eq!(window.clips().len(), expected);
    }
    undo(&mut window);
    assert_eq!(window.clips().len(), 3, "nothing is left to undo");
    assert!(!window.application().can_undo());

    for expected in [4, 5, 6] {
        redo(&mut window);
        assert_eq!(window.clips().len(), expected);
    }
}

#[test]
fn an_edit_after_an_undo_ends_what_could_have_been_redone() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(45);
    undo(&mut window);
    assert!(window.application().can_redo());
    window.cut_at_frame(15);
    assert!(!window.application().can_redo());
    redo(&mut window);
    assert_eq!(window.clips().len(), 4, "redoing does nothing now");
}

#[test]
fn the_keys_do_nothing_when_there_is_nothing_to_undo_or_redo() {
    let mut window = window_with_three_clips();
    undo(&mut window);
    redo(&mut window);
    assert_eq!(window.clips().len(), 3);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn the_keys_undo_and_redo_when_the_timeline_is_empty_which_is_how_deleting_the_last_clip_leaves_it() {
    let mut window = Window::showing(project_of(300)); // one clip, so that deleting it empties the timeline
    window.application_mut().note_everything_is_saved();
    select_the_clip_at(&mut window, 100);
    window.press_key(Key::Delete);
    assert_eq!(window.clips(), Vec::new());
    assert_eq!(
        window.application().project().total_frame_count(),
        0,
        "nothing is on the timeline"
    );

    undo(&mut window);
    assert_eq!(window.clips(), vec![(0, 300, 1)], "Ctrl+Z brings the clip back");
    assert!(!window.application().has_unsaved_changes());

    redo(&mut window);
    assert_eq!(
        window.clips(),
        Vec::new(),
        "and Ctrl+Y takes it off again, to an empty timeline once more"
    );
    redo(&mut window);
    undo(&mut window);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}

#[test]
fn the_keys_are_for_the_text_field_while_one_is_being_typed_in_and_do_not_undo_the_edits_of_the_project() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(45);
    window.click(window.title_box_of_track(0).center());
    undo(&mut window);
    assert_eq!(window.clips().len(), 4, "the cut was not taken back");
}

// --- the Edit menu ---

#[test]
fn the_edit_menu_has_undo_and_redo_with_their_keys_and_each_is_available_only_when_it_can_be_used() {
    let mut window = window_with_three_clips();
    let item_is_disabled = |window: &Window, label: &str| {
        window
            .harness
            .get_by_label_contains(label)
            .accesskit_node()
            .is_disabled()
    };

    window.harness.get_by_label("Edit").click();
    window.settle();
    assert!(item_is_disabled(&window, "Undo"), "nothing to undo yet");
    assert!(item_is_disabled(&window, "Redo"));
    assert!(
        window.harness.query_by_label_contains("Ctrl+Z").is_some(),
        "the key is shown"
    );
    assert!(window.harness.query_by_label_contains("Ctrl+Y").is_some());
    let heights = [
        window.harness.get_by_label_contains("Undo").rect().top(),
        window.harness.get_by_label_contains("Redo").rect().top(),
        window.harness.get_by_label_contains("Deselect").rect().top(),
    ];
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "Undo, Redo, then Deselect: {heights:?}"
    );
    window.press_key(Key::Escape);

    window.cut_at_frame(45);
    window.harness.get_by_label("Edit").click();
    window.settle();
    assert!(!item_is_disabled(&window, "Undo"), "there is a cut to undo");
    assert!(item_is_disabled(&window, "Redo"));

    window.harness.get_by_label_contains("Undo").click();
    window.settle();
    assert_eq!(window.clips().len(), 3, "choosing it undoes");
    window.harness.get_by_label("Edit").click();
    window.settle();
    assert!(item_is_disabled(&window, "Undo"));
    assert!(!item_is_disabled(&window, "Redo"));
    window.harness.get_by_label_contains("Redo").click();
    window.settle();
    assert_eq!(window.clips().len(), 4, "and choosing Redo does it again");
}

// --- what is one step ---

#[test]
fn a_whole_drag_of_a_slider_is_one_step_and_different_things_are_different_steps() {
    let mut window = window_with_three_clips();
    window.right_click(window.middle_of_the_track_at(45));
    // The slider is moved several times, as a drag does.
    for speed in [2.0, 5.0, 10.0, 20.0] {
        window.set_slider("Speed", speed);
    }
    window.set_slider("Gain", 6.0);
    window.press_key(Key::Escape);
    assert_eq!(window.clips()[1], (30, 60, 20));

    undo(&mut window);
    assert_eq!(
        window.clips()[1].2,
        20,
        "the gain, which was the latest, is taken back first"
    );
    undo(&mut window);
    assert_eq!(
        window.clips()[1],
        (30, 60, 1),
        "then the whole drag of the speed slider, in one go"
    );
    assert!(
        !window.application().can_undo(),
        "there were two steps: the speed, and the gain"
    );
}

#[test]
fn a_change_that_changes_nothing_is_not_a_step() {
    let mut window = window_with_three_clips();
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackSpeed {
        track,
        speed_factor: SpeedFactor::NORMAL,
    });
    assert!(!window.application().can_undo());
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn undoing_a_speed_change_puts_the_current_frame_back_so_that_the_same_picture_is_on_screen() {
    let mut window = window_with_three_clips();
    window.move_current_frame_to(75); // in the last clip
    let first_clip = window.clip(0, 0);
    window.do_this(Command::SetClipSpeed {
        clip: first_clip,
        speed_factor: SpeedFactor::new(5).unwrap(),
    });
    assert_eq!(window.current_frame(), 75 - 24, "the playhead went with the picture");

    undo(&mut window);

    assert_eq!(window.current_frame(), 75, "and goes back with it");
}

// --- selecting and deleting ---

#[test]
fn undoing_a_delete_brings_the_clip_back_where_it_was_and_selected_again() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 45);
    let selected = window.application().selected_clip().unwrap();
    window.press_key(Key::Delete);
    assert_eq!(window.clips().len(), 2);
    assert_eq!(window.application().selected_clip(), None);

    undo(&mut window);

    assert_eq!(window.clips(), vec![(0, 30, 1), (30, 60, 1), (60, 90, 1)]);
    assert_eq!(
        window.application().selected_clip(),
        Some(selected),
        "with what was selected"
    );
}

#[test]
fn selecting_is_not_an_edit_so_there_is_nothing_to_undo_after_it() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 45);
    assert!(!window.application().can_undo());
    undo(&mut window);
    assert!(
        window.application().selected_clip().is_some(),
        "and the selection stays"
    );
}

// --- tracks ---

#[test]
fn undoing_the_adding_of_a_track_takes_the_track_away_again() {
    let mut window = window_with_three_clips();
    window.do_this(Command::AddTrack);
    assert_eq!(window.application().project().tracks().len(), 2);
    undo(&mut window);
    assert_eq!(window.application().project().tracks().len(), 1);
    redo(&mut window);
    assert_eq!(window.application().project().tracks().len(), 2);
    assert_eq!(window.title_of_track(0), "track 1");
}

// --- saved or not ---

#[test]
fn undoing_back_to_the_project_as_it_was_saved_leaves_no_unsaved_changes() {
    let mut window = window_with_three_clips();
    assert!(!window.application().has_unsaved_changes());
    window.cut_at_frame(45);
    assert!(window.application().has_unsaved_changes());
    undo(&mut window);
    assert!(
        !window.application().has_unsaved_changes(),
        "it is as it was when it was saved"
    );
    redo(&mut window);
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn saving_in_the_middle_of_the_history_marks_that_point_as_saved() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(15);
    window.application_mut().note_everything_is_saved(); // saved with one cut
    window.cut_at_frame(75);
    assert!(window.application().has_unsaved_changes());
    undo(&mut window);
    assert!(!window.application().has_unsaved_changes(), "back to what was saved");
    undo(&mut window);
    assert!(window.application().has_unsaved_changes(), "before it was saved");
}

// --- what undo leaves alone ---

#[test]
fn undoing_does_not_undo_a_setting_of_the_project() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(45);
    let settings = ProjectSettings {
        autosave: Some(true),
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    };
    window.do_this(Command::SetProjectSettings(settings));

    undo(&mut window);

    assert_eq!(window.clips().len(), 3, "the cut was taken back");
    assert_eq!(
        window.application().project().settings(),
        settings,
        "the setting was not"
    );
}

#[test]
fn a_new_project_has_nothing_to_undo() {
    let mut window = window_with_three_clips();
    window.cut_at_frame(45);
    assert!(window.application().can_undo());
    window.application_mut().start_empty_project();
    window.settle();
    assert!(!window.application().can_undo());
    assert!(!window.application().can_redo());
    undo(&mut window);
    assert!(window.application().project().has_no_clips());
}

#[test]
fn undo_works_in_a_project_with_one_clip_too() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().note_everything_is_saved();
    window.cut_at_frame(100);
    assert_eq!(window.clips(), vec![(0, 100, 1), (100, 300, 1)]);
    undo(&mut window);
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
}
