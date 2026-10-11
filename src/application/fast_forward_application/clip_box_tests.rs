//! Tests of the box of settings that opens above a clip when it is right-clicked: where it is, what
//! it shows, what its controls do, and when it closes.

use egui::{Key, Pos2};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{SOUND, VIDEO, Window, project_cut_into_three, project_of, project_with_a_video_and_a_sound};
use super::{SourceFileState, SourceStatus};
use crate::application::bubble::TAIL_HEIGHT;
use crate::application::commands::Command;
use crate::application::timeline_panel::SettingsBoxSubject;
use crate::project::{Gain, SpeedFactor};

/// A window showing a video of 90 frames cut into three clips of 30, all in view.
fn window_with_three_clips() -> Window {
    let mut window = Window::showing(project_cut_into_three(90));
    window.application_mut().has_unsaved_changes = false;
    window.settle();
    window
}

fn right_click_the_clip_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.right_click(point);
}

fn slider_value(window: &Window, label: &str) -> f64 {
    window
        .harness
        .get_by_label(label)
        .accesskit_node()
        .numeric_value()
        .unwrap_or_else(|| panic!("{label} is a slider with a value"))
}

// --- opening ---

#[test]
fn right_clicking_a_clip_selects_it_and_opens_a_box_right_above_it_with_a_pointer_to_it() {
    let mut window = window_with_three_clips();
    let middle_clip = window.clip(0, 1);

    right_click_the_clip_at(&mut window, 0, 45);

    assert_eq!(window.application().selected_clip(), Some(middle_clip));
    assert_eq!(
        window.application().timeline_view.settings_box_subject(),
        Some(SettingsBoxSubject::Clip(middle_clip))
    );
    let body = window
        .application()
        .timeline_view
        .settings_box_rectangle()
        .expect("drawn");
    let strip = window.strip_of_track(0);
    assert!(
        (body.bottom() + TAIL_HEIGHT - strip.top()).abs() < 1.5,
        "the pointer of the box touches the top edge of the clip: {body:?} over {strip:?}"
    );
    let clip_middle = window.x_of_frame_on_the_strip(0, 45);
    assert!(
        body.left() <= clip_middle && clip_middle <= body.right(),
        "the box is over the clip"
    );
    assert!(
        window.harness.query_by_label("Speed").is_some()
            && window.harness.query_by_label("Gain").is_some()
            && window.harness.query_by_label("Mute").is_some(),
        "the clip's speed, loudness and mute"
    );
    assert!(!window.application().has_unsaved_changes(), "opening it is not an edit");
}

#[test]
fn the_box_says_which_kind_of_clip_it_is_for_and_which_file_it_comes_from() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    right_click_the_clip_at(&mut window, 1, 45);
    assert!(window.harness.query_by_label("Video clip: holiday.mp4").is_some());
    window.press_key(Key::Escape);

    right_click_the_clip_at(&mut window, 0, 45);
    assert!(window.harness.query_by_label("Sound clip: song.mp3").is_some());
}

#[test]
fn the_box_of_a_long_clip_that_is_scrolled_is_over_the_part_of_it_that_can_be_seen() {
    // A 30 second clip with the playhead in the middle of it: its start and its end are both out of view.
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(450);
    let strip = window.strip_of_track(0);
    window.right_click(strip.center());
    let body = window
        .application()
        .timeline_view
        .settings_box_rectangle()
        .expect("drawn");
    assert!(
        body.left() >= strip.left() - 1.0 && body.right() <= strip.right() + 1.0,
        "the box is over what can be seen of the clip: {body:?} over {strip:?}"
    );
    assert!((body.center().x - strip.center().x).abs() < 5.0, "in the middle of it");
}

#[test]
fn right_clicking_the_ruler_or_an_empty_part_of_a_track_does_not_open_a_clips_box() {
    let mut window = window_with_three_clips();
    window.right_click(window.point_on_the_ruler_at(45));
    assert!(!window.application().timeline_view.settings_box_is_open());
    let past_the_end = window.middle_of_the_strip_at(0, 140);
    window.right_click(past_the_end);
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "that opens the menu to make new things"
    );
}

// --- showing and changing ---

#[test]
fn the_box_shows_the_speed_the_gain_and_the_mute_of_the_clip() {
    let mut project = project_cut_into_three(90);
    let middle = project.tracks()[0].placement_at_index(1).unwrap().id;
    let location = crate::project::ClipLocation {
        track: project.tracks()[0].id(),
        clip: middle,
    };
    project.set_clip_speed(location, SpeedFactor::new(3).unwrap()).unwrap();
    project.set_clip_gain(location, Gain::from_decibels(6.0)).unwrap();
    project.set_clip_muted(location, true).unwrap();
    let mut window = Window::showing(project);

    // The middle clip now lasts 10 frames, starting at frame 30.
    right_click_the_clip_at(&mut window, 0, 35);

    assert_eq!(slider_value(&window, "Speed"), 3.0);
    assert_eq!(slider_value(&window, "Gain"), 6.0);
    assert!(window.is_checked("Mute"));
}

#[test]
fn the_speed_slider_in_the_box_sets_the_speed_of_the_clip_and_keeps_the_picture_on_screen() {
    let mut window = window_with_three_clips();
    window.move_current_frame_to(75); // the last clip, showing source frame 75
    right_click_the_clip_at(&mut window, 0, 15); // the first clip

    window.set_slider("Speed", 5.0);

    assert_eq!(window.clips(), vec![(0, 30, 5), (30, 60, 1), (60, 90, 1)]);
    assert!(window.application().has_unsaved_changes());
    // The first clip lasts 6 frames now, so everything after it is 24 frames earlier: the current
    // frame went with the picture it was showing.
    assert_eq!(window.current_frame(), 75 - 24);
    assert!(
        window.application().timeline_view.settings_box_is_open(),
        "the box stays open while it is used"
    );
}

#[test]
fn the_speed_can_be_set_again_and_again_while_the_box_stays_open() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    for speed in [2.0, 10.0, 1.0, 1000.0] {
        window.set_slider("Speed", speed);
        assert_eq!(window.clips()[1].2, speed as u32, "{speed}");
        assert_eq!(slider_value(&window, "Speed"), speed, "the slider shows it");
    }
}

#[test]
fn the_gain_slider_sets_the_gain_of_the_clip_and_stays_within_thirty_decibels_either_way() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    let clip = window.clip(0, 1);
    let gain = |window: &Window| {
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .as_media()
            .unwrap()
            .gain()
            .decibels()
    };

    window.set_slider("Gain", -9.5);
    assert_eq!(gain(&window), -9.5);
    window.set_slider("Gain", 500.0);
    assert_eq!(gain(&window), 30.0);
    window.set_slider("Gain", -500.0);
    assert_eq!(gain(&window), -30.0);
    window.set_slider("Gain", 0.0);
    assert_eq!(gain(&window), 0.0, "0 dB is no change, in the middle of the slider");
}

#[test]
fn the_mute_box_mutes_the_clip_and_unmutes_it() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    let clip = window.clip(0, 1);
    let is_muted = |window: &Window| {
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .as_media()
            .unwrap()
            .is_muted()
    };

    window.click(window.rectangle_of("Mute").center());
    assert!(is_muted(&window));
    assert!(window.application().has_unsaved_changes());
    window.click(window.rectangle_of("Mute").center());
    assert!(!is_muted(&window));
}

#[test]
fn a_change_that_changes_nothing_is_not_an_edit() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    window.set_slider("Speed", 1.0);
    window.set_slider("Gain", 0.0);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn when_the_track_has_its_own_speed_the_box_says_how_fast_the_clip_really_plays() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    window.set_slider("Speed", 3.0);
    assert!(
        window.harness.query_by_label_contains("It plays at").is_none(),
        "no note at normal track speed"
    );

    let track = window.application().project().tracks()[0].id();
    window.application_mut().execute(
        Command::SetTrackSpeed {
            track,
            speed_factor: SpeedFactor::new(2).unwrap(),
        },
        std::time::Instant::now(),
        &egui::Context::default(),
    );
    window.settle();
    assert!(
        window
            .harness
            .query_by_label_contains("It plays at 6x, because its track plays at 2x")
            .is_some(),
        "the clip's 3x and its track's 2x"
    );
}

// --- closing ---

#[test]
fn the_box_closes_on_a_click_outside_on_escape_and_on_another_right_click_of_the_same_clip() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    assert!(window.application().timeline_view.settings_box_is_open());

    window.click(Pos2::new(640.0, 120.0));
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "a click outside closes it"
    );

    right_click_the_clip_at(&mut window, 0, 45);
    window.press_key(Key::Escape);
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "escape closes it"
    );
    assert!(
        window.application().selected_clip().is_some(),
        "and does not deselect the clip"
    );

    right_click_the_clip_at(&mut window, 0, 45);
    assert!(window.application().timeline_view.settings_box_is_open());
    right_click_the_clip_at(&mut window, 0, 45);
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "right-clicking the clip again closes it"
    );
}

#[test]
fn right_clicking_another_clip_moves_the_box_to_it_and_selects_that_clip() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 15);
    let before = window.application().timeline_view.settings_box_rectangle().unwrap();
    right_click_the_clip_at(&mut window, 0, 75);

    let last_clip = window.clip(0, 2);
    assert_eq!(
        window.application().timeline_view.settings_box_subject(),
        Some(SettingsBoxSubject::Clip(last_clip))
    );
    assert_eq!(window.application().selected_clip(), Some(last_clip));
    let after = window.application().timeline_view.settings_box_rectangle().unwrap();
    assert!(
        after.center().x > before.center().x + 100.0,
        "{before:?} then {after:?}"
    );
}

#[test]
fn clicking_a_clip_while_its_box_is_open_closes_the_box_and_selects_the_clip() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 15);
    let point = window.middle_of_the_strip_at(0, 75);
    window.click(point);
    assert!(!window.application().timeline_view.settings_box_is_open());
    assert_eq!(window.application().selected_clip(), Some(window.clip(0, 2)));
}

#[test]
fn the_box_goes_when_its_clip_does() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    let clip = window.clip(0, 1);
    window.application_mut().execute(
        Command::DeleteClip(clip),
        std::time::Instant::now(),
        &egui::Context::default(),
    );
    window.settle();
    assert!(!window.application().timeline_view.settings_box_is_open());
}

#[test]
fn the_box_goes_when_its_clip_is_scrolled_out_of_view() {
    let mut window = Window::showing(project_cut_into_three(900));
    // The first clip is frames 0 to 300; open its box, then make the strip scroll far to the right.
    right_click_the_clip_at(&mut window, 0, 100);
    assert!(window.application().timeline_view.settings_box_is_open());
    window.application_mut().timeline_view.scroll_offset = 100_000.0;
    window.settle();
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "there is nothing to point at"
    );
}

// --- a clip whose file is missing ---

#[test]
fn the_box_of_a_clip_whose_file_is_missing_says_so_and_offers_to_choose_the_file_again() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    window.application_mut().source_files.insert(
        SOUND,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "there is no file there".to_owned(),
        }),
    );
    window.settle();

    right_click_the_clip_at(&mut window, 0, 45);
    assert!(window.harness.query_by_label_contains("cannot be found").is_some());
    assert!(window.harness.query_by_label("Choose the file again").is_some());

    window.press_key(Key::Escape);
    right_click_the_clip_at(&mut window, 1, 45);
    assert!(
        window.harness.query_by_label("Choose the file again").is_none(),
        "the video clip is fine: the file it is made from can be used"
    );
    assert!(window.application().source_file_is_available(VIDEO));
}

#[test]
fn nothing_opens_while_a_dialog_is_up() {
    let mut window = window_with_three_clips();
    window.application_mut().dialog = Some(super::Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();
    right_click_the_clip_at(&mut window, 0, 45);
    assert!(!window.application().timeline_view.settings_box_is_open());
}
