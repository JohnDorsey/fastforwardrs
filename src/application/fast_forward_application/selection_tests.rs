//! Tests of selecting clips, deselecting them, and deleting the selected one, and of how cutting
//! with the S key follows the selection.

use egui::Key;
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_cut_into_three, project_of, project_with_a_video_and_a_sound};
use crate::application::dialogs::Dialog;
use crate::project::{ClipKind, TextObject, TimelineFrameIndex};

/// A window showing a video of 90 frames cut into three clips of 30 (so that all of it is in view).
fn window_with_three_clips() -> Window {
    let mut window = Window::showing(project_cut_into_three(90));
    window.application_mut().note_everything_is_saved();
    window.settle();
    window
}

fn select_the_clip_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.click(point);
}

// --- selecting ---

#[test]
fn clicking_a_clip_selects_it_and_clicking_another_moves_the_selection_to_it() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    assert_eq!(window.application().selected_clip(), Some(window.clip(0, 0)));
    select_the_clip_at(&mut window, 0, 75);
    assert_eq!(window.application().selected_clip(), Some(window.clip(0, 2)));
    assert!(!window.application().has_unsaved_changes(), "selecting is not an edit");
}

#[test]
fn clicking_an_empty_part_of_a_track_selects_nothing() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    assert!(window.application().selected_clip().is_some());

    // The video is 90 frames long, and the strip shows a good deal more than that.
    let past_the_end = window.middle_of_the_strip_at(0, 140);
    window.click(past_the_end);

    assert_eq!(window.application().selected_clip(), None);
}

#[test]
fn clicking_the_ruler_or_a_button_leaves_the_selection_alone() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let selected = window.application().selected_clip();
    window.click(window.point_on_the_ruler_at(60));
    assert_eq!(window.application().selected_clip(), selected);
    window.harness.get_by_label("Play").click();
    window.settle();
    assert_eq!(window.application().selected_clip(), selected);
}

#[test]
fn the_escape_key_deselects_and_does_nothing_when_nothing_is_selected() {
    let mut window = window_with_three_clips();
    window.press_key(Key::Escape);
    assert_eq!(window.application().selected_clip(), None);

    select_the_clip_at(&mut window, 0, 45);
    assert!(window.application().selected_clip().is_some());
    window.press_key(Key::Escape);
    assert_eq!(window.application().selected_clip(), None);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn escape_closes_a_menu_that_is_open_before_it_deselects() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    let selected = window.application().selected_clip();
    // The menu that opens on an empty part of a track.
    let empty_part = window.middle_of_the_strip_at(0, 140);
    window.right_click(empty_part);
    assert!(window.harness.query_by_label_contains("New").is_some());

    window.press_key(Key::Escape);
    assert!(
        window.harness.query_by_label_contains("New").is_none(),
        "the menu is closed"
    );
    assert_eq!(
        window.application().selected_clip(),
        selected,
        "and the clip is still selected"
    );

    window.press_key(Key::Escape);
    assert_eq!(window.application().selected_clip(), None, "the next press deselects");
}

#[test]
fn the_other_keys_are_the_menus_while_one_is_open_too() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    let empty_part = window.middle_of_the_strip_at(0, 140);
    window.right_click(empty_part);
    for key in [Key::Delete, Key::Space, Key::S] {
        window.press_key(key);
    }
    assert_eq!(window.clips().len(), 3, "nothing was deleted or cut");
    assert!(!window.application().is_playing());
}

#[test]
fn the_selection_stays_on_the_clip_when_other_clips_are_changed_around_it() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 75);
    let selected = window.application().selected_clip();
    // Speed up the first clip: the selected one moves earlier on the timeline, and is still the selected one.
    let first = window.clip(0, 0);
    window.do_this(crate::application::commands::Command::SetClipSpeed {
        clip: first,
        speed_factor: crate::project::SpeedFactor::new(5).unwrap(),
    });
    assert_eq!(window.application().selected_clip(), selected);
    assert_eq!(window.clips()[2], (60, 90, 1));
}

#[test]
fn a_selection_in_a_project_that_is_replaced_is_forgotten() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    window.application_mut().start_empty_project();
    window.settle();
    assert_eq!(window.application().selected_clip(), None);
}

// --- deleting ---

#[test]
fn the_delete_key_deletes_the_selected_clip_and_the_clips_after_it_move_up() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    assert_eq!(window.clips(), vec![(0, 30, 1), (30, 60, 1), (60, 90, 1)]);

    window.press_key(Key::Delete);

    assert_eq!(window.clips(), vec![(0, 30, 1), (60, 90, 1)]);
    assert_eq!(
        window.application().project().total_frame_count(),
        60,
        "the clip after it moved up to close the gap"
    );
    assert!(window.application().has_unsaved_changes());
    assert_eq!(
        window.application().selected_clip(),
        None,
        "nothing is selected after deleting"
    );
}

#[test]
fn the_backspace_key_deletes_the_selected_clip_too() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    window.press_key(Key::Backspace);
    assert_eq!(window.clips(), vec![(30, 60, 1), (60, 90, 1)]);
}

#[test]
fn the_delete_key_does_nothing_when_no_clip_is_selected() {
    let mut window = window_with_three_clips();
    window.press_key(Key::Delete);
    assert_eq!(window.clips().len(), 3);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn deleting_does_not_happen_while_a_dialog_is_open() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    window.application_mut().dialog = Some(Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    window.press_key(Key::Delete);
    assert_eq!(window.clips().len(), 3);
}

#[test]
fn deleting_the_last_clip_of_the_project_leaves_an_empty_timeline_and_the_window_copes() {
    let mut window = Window::showing(project_of(90));
    select_the_clip_at(&mut window, 0, 45);
    window.press_key(Key::Delete);
    assert!(window.application().project().has_no_clips());
    assert_eq!(window.current_frame(), 0);
    // Nothing breaks when the keys are pressed with nothing to play.
    window.press_key(Key::Space);
    window.press_key(Key::S);
    assert!(!window.application().is_playing());
}

#[test]
fn the_playhead_follows_the_picture_it_was_on_when_an_earlier_clip_is_deleted() {
    let mut window = window_with_three_clips();
    window.move_current_frame_to(75); // in the last clip, showing source frame 75
    select_the_clip_at(&mut window, 0, 15);
    window.press_key(Key::Delete);
    assert_eq!(window.current_frame(), 45, "the last clip is 30 frames earlier now");
}

#[test]
fn deleting_a_clip_deletes_it_from_its_own_track_only() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    // Track 0 (in front) has the 3 second sound; track 1 has the video.
    let sound_clip = window.clip(0, 0);
    let video_clip = window.clip(1, 0);
    select_the_clip_at(&mut window, 0, 45);
    assert_eq!(window.application().selected_clip(), Some(sound_clip));
    window.press_key(Key::Delete);

    assert!(window.application().project().placement_of(sound_clip).is_none());
    assert!(window.application().project().placement_of(video_clip).is_some());
    assert_eq!(window.clips_of_track(1), vec![(0, 90, 1)]);
}

// --- cutting with a clip selected ---

#[test]
fn s_cuts_every_clip_at_the_current_frame_when_none_is_selected() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    window.cut_at_frame(45);
    assert_eq!(window.clips_of_track(0).len(), 2, "the sound was cut");
    assert_eq!(window.clips_of_track(1).len(), 2, "and the video");
}

#[test]
fn s_cuts_only_the_selected_clip_when_it_is_at_the_current_frame() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    select_the_clip_at(&mut window, 1, 20); // the video, on the track behind
    let video_clip = window.application().selected_clip().unwrap();
    window.move_current_frame_to(45);

    window.press_key(Key::S);

    assert_eq!(
        window.clips_of_track(1),
        vec![(0, 45, 1), (45, 90, 1)],
        "the selected clip was cut"
    );
    assert_eq!(
        window.clips_of_track(0),
        vec![(0, 90, 1)],
        "and the one over it was not"
    );
    assert_eq!(
        window.application().selected_clip(),
        Some(video_clip),
        "the first half of what was cut is still the selected clip"
    );
}

#[test]
fn s_cuts_everything_at_the_current_frame_when_the_selected_clip_is_somewhere_else() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(180, 6));
    // Select the video clip, then cut it in two to have a clip that is not at the playhead.
    window.cut_at_frame(60);
    select_the_clip_at(&mut window, 1, 30);
    let selected = window.application().selected_clip();
    window.move_current_frame_to(120);

    window.press_key(Key::S);

    assert_eq!(
        window.clips_of_track(0).len(),
        3,
        "the sound was cut at 120 (and at 60 before)"
    );
    assert_eq!(window.clips_of_track(1).len(), 3, "and so was the video");
    assert_eq!(window.application().selected_clip(), selected);
    let at_120 = window.application().project().tracks()[1].placement_covering(TimelineFrameIndex::new(120));
    assert!(at_120.is_some());
}

// --- clips of every kind ---

#[test]
fn clicking_a_sound_clip_selects_it_like_any_other() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    select_the_clip_at(&mut window, 0, 30);
    let selected = window.application().selected_clip().unwrap();
    let project = window.application().project();
    assert_eq!(project.placement_of(selected).unwrap().clip.kind(), ClipKind::Audio);
}

// --- several clips at once ---

#[test]
fn control_click_adds_a_clip_to_the_selection_and_control_click_on_a_selected_clip_takes_it_out_again() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let point = window.middle_of_the_strip_at(0, 75);
    window.click_with_control(point);
    assert_eq!(
        window.application().selected_clips(),
        vec![window.clip(0, 0), window.clip(0, 2)]
    );
    assert_eq!(
        window.application().selected_clip(),
        None,
        "there is no single selected clip now"
    );

    let point = window.middle_of_the_strip_at(0, 15);
    window.click_with_control(point);
    assert_eq!(window.application().selected_clips(), vec![window.clip(0, 2)]);
    assert!(!window.application().has_unsaved_changes(), "selecting is not an edit");
}

#[test]
fn clicking_a_clip_without_control_selects_only_that_clip_whatever_was_selected() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let point = window.middle_of_the_strip_at(0, 45);
    window.click_with_control(point);
    assert_eq!(window.application().selected_clips().len(), 2);
    select_the_clip_at(&mut window, 0, 75);
    assert_eq!(window.application().selected_clips(), vec![window.clip(0, 2)]);
}

#[test]
fn clicking_an_empty_part_of_a_track_deselects_all_the_clips() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let point = window.middle_of_the_strip_at(0, 45);
    window.click_with_control(point);
    let past_the_end = window.middle_of_the_strip_at(0, 140);
    window.click(past_the_end);
    assert!(window.application().selected_clips().is_empty());
}

#[test]
fn delete_takes_all_the_selected_clips_off_their_tracks_and_undo_brings_them_all_back_in_one_step() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let point = window.middle_of_the_strip_at(0, 75);
    window.click_with_control(point);

    window.press_key(Key::Delete);

    assert_eq!(window.clips(), vec![(30, 60, 1)], "only the middle clip is left");
    assert!(window.application().selected_clips().is_empty());

    window.harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::Z);
    window.settle();
    assert_eq!(
        window.clips(),
        vec![(0, 30, 1), (30, 60, 1), (60, 90, 1)],
        "one undo brings both back"
    );
    assert_eq!(window.application().selected_clips().len(), 2, "and selects them again");
}

#[test]
fn s_cuts_all_the_selected_clips_that_are_at_the_current_frame_and_no_others() {
    // A video, a sound over it, and a text over both: all three are at frame 45.
    let mut project = project_with_a_video_and_a_sound(90, 3);
    let text_track = project.add_track_in_front();
    project
        .insert_text(text_track, TimelineFrameIndex::new(30), TextObject::default(), 60)
        .unwrap();
    let mut window = Window::showing(project);
    // The tracks, front to back: the text, the sound, the video.
    select_the_clip_at(&mut window, 2, 20); // the video
    let point = window.middle_of_the_strip_at(1, 20);
    window.click_with_control(point); // and the sound
    window.move_current_frame_to(45);

    window.press_key(Key::S);

    assert_eq!(
        window.clips_of_track(2),
        vec![(0, 45, 1), (45, 90, 1)],
        "the video was cut"
    );
    assert_eq!(window.clips_of_track(1).len(), 2, "and the sound");
    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        1,
        "but not the text, which was not selected"
    );
    assert_eq!(
        window.application().selected_clips().len(),
        2,
        "both are still selected"
    );
}
