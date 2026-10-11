//! Tests of the tracks: their title boxes, the round button that adds one, the order they are in,
//! and the settings of a track in the box that opens above its gear.

use egui::{Key, Pos2, Rect};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::bubble::TAIL_HEIGHT;
use crate::application::timeline_panel::{ADD_TRACK_LABEL, SettingsBoxSubject};
use crate::project::{Gain, SpeedFactor};

fn add_a_track(window: &mut Window) {
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    window.click(button.center());
}

fn the_box_that_is_open(window: &Window) -> Rect {
    window
        .application()
        .timeline_view
        .settings_box_rectangle()
        .expect("a settings box is open and has been drawn")
}

// --- the list of tracks ---

#[test]
fn a_new_project_has_one_track_called_track_0_with_a_title_box_and_a_gear_at_its_left() {
    let window = Window::showing(project_of(300));
    assert_eq!(window.application().project().tracks().len(), 1);
    assert_eq!(window.title_of_track(0), "track 0");
    let title_box = window.title_box_of_track(0);
    let gear = window.gear_of_track(0);
    let strip = window.strip_of_track(0);
    assert!(
        title_box.right() <= gear.left() + 1.0,
        "the gear is beside the title: {title_box:?} {gear:?}"
    );
    assert!(gear.right() < strip.left(), "both are at the left of the strip");
    assert!(
        title_box.center().y > strip.top() && title_box.center().y < strip.bottom(),
        "level with the strip"
    );
    assert_eq!(
        window.text_field_showing("track 0").rect(),
        title_box,
        "the title is shown in the title box"
    );
}

#[test]
fn the_big_round_button_is_under_the_last_track_and_big_and_round() {
    let window = Window::showing(project_of(300));
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    let strip = window.strip_of_track(0);
    assert!(
        button.top() >= strip.bottom(),
        "under the list of tracks: {button:?} {strip:?}"
    );
    assert!(button.width() >= 32.0, "big: {}", button.width());
    assert!((button.width() - button.height()).abs() < 0.5, "round: {button:?}");
    assert!(
        button.center().x < strip.left(),
        "it is under the title boxes, at the left: {button:?}"
    );
}

#[test]
fn pressing_the_button_adds_a_track_in_front_of_the_others_with_the_next_title() {
    let mut window = Window::showing(project_of(300));
    let old_track = window.application().project().tracks()[0].id();
    add_a_track(&mut window);

    let tracks = window.application().project().tracks();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title(), "track 1", "the new track is in front, at the top");
    assert_eq!(tracks[1].id(), old_track);
    assert!(tracks[0].is_empty());
    assert!(window.application().has_unsaved_changes());

    add_a_track(&mut window);
    let titles: Vec<String> = (0..3).map(|position| window.title_of_track(position)).collect();
    assert_eq!(titles, ["track 2", "track 1", "track 0"], "titles go on counting");
}

#[test]
fn tracks_higher_in_the_list_are_higher_on_the_screen() {
    let mut window = Window::showing(project_of(300));
    add_a_track(&mut window);
    add_a_track(&mut window);
    let tops: Vec<f32> = (0..3).map(|position| window.strip_of_track(position).top()).collect();
    assert!(
        tops.windows(2).all(|pair| pair[0] < pair[1]),
        "front to back, top to bottom: {tops:?}"
    );
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    assert!(
        button.top() >= window.strip_of_track(2).bottom(),
        "and the button is under the last"
    );
}

#[test]
fn every_strip_has_the_same_scale_and_the_ruler_is_over_all_of_them() {
    let mut window = Window::showing(project_of(300));
    add_a_track(&mut window);
    let ruler = window.rectangle_of("Ruler");
    for position in 0..2 {
        let strip = window.strip_of_track(position);
        assert_eq!(
            (strip.left(), strip.right()),
            (ruler.left(), ruler.right()),
            "track {position}"
        );
    }
    assert_eq!(
        window.strip_of_track(0).bottom().round() + 6.0,
        window.strip_of_track(1).top().round()
    );
}

#[test]
fn with_more_tracks_than_fit_the_rows_scroll_and_the_button_can_still_be_pressed() {
    let mut window = Window::showing(project_of(300));
    for _ in 0..6 {
        add_a_track(&mut window);
    }
    assert_eq!(window.application().project().tracks().len(), 7);
    let screen = Rect::from_min_size(
        Pos2::ZERO,
        egui::vec2(super::test_window::WINDOW_SIZE[0], super::test_window::WINDOW_SIZE[1]),
    );
    let button = window.rectangle_of(ADD_TRACK_LABEL);
    assert!(screen.contains_rect(button), "the button is on the screen: {button:?}");
    // The rows are in a scrolling area of a limited height, so that the preview keeps its room.
    let first = window.strip_of_track(0);
    let ruler = window.rectangle_of("Ruler");
    assert_eq!(first.top(), ruler.bottom());
    assert!(
        button.top() - first.top() < 4.0 * crate::application::theme::TRACK_HEIGHT,
        "at most a few tracks are shown at once"
    );
    add_a_track(&mut window);
    assert_eq!(window.application().project().tracks().len(), 8);
}

// --- the title ---

#[test]
fn typing_in_the_title_box_renames_the_track() {
    let mut window = Window::showing(project_of(300));
    window.replace_text_in_the_field_showing("track 0", "Music");
    assert_eq!(window.title_of_track(0), "Music");
    assert!(window.application().has_unsaved_changes());
    // The other things that are called by the title are called by the new one.
    assert!(window.harness.query_by_label("Strip of Music").is_some());
}

#[test]
fn the_keys_of_the_keyboard_are_not_shortcuts_while_the_title_is_being_typed() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(100);
    window.click(window.title_box_of_track(0).center());

    for key in [Key::Space, Key::S, Key::Delete, Key::ArrowLeft, Key::Backspace] {
        window.press_key(key);
    }

    assert!(!window.application().is_playing(), "space did not play");
    assert_eq!(window.clips(), vec![(0, 300, 1)], "S did not cut");
    assert_eq!(window.current_frame(), 100, "the arrow did not move the playhead");
}

#[test]
fn clicking_somewhere_else_ends_the_typing_and_the_keys_are_shortcuts_again() {
    let mut window = Window::showing(project_of(300));
    window.click(window.title_box_of_track(0).center());
    // A click on the preview (empty space) takes the keyboard away from the title.
    window.click(Pos2::new(640.0, 120.0));
    window.press_key(Key::Space);
    assert!(window.application().is_playing(), "space plays again");
}

// --- the box of settings of a track ---

#[test]
fn pressing_the_gear_opens_a_box_above_it_with_the_speed_the_gain_and_a_mute_box() {
    let mut window = Window::showing(project_of(300));
    let gear = window.gear_of_track(0);
    window.click(gear.center());

    let track = window.application().project().tracks()[0].id();
    assert_eq!(
        window.application().timeline_view.settings_box_subject(),
        Some(SettingsBoxSubject::Track(track))
    );
    for label in ["Speed", "Gain"] {
        assert!(window.harness.query_by_label(label).is_some(), "{label}");
    }
    assert!(window.harness.query_by_label("Mute").is_some());

    // The box is right above the gear, with its pointer touching the top of the gear.
    let body = the_box_that_is_open(&window);
    assert!(
        (body.bottom() + TAIL_HEIGHT - gear.top()).abs() < 1.5,
        "the box ends a pointer's height above the gear: {body:?} {gear:?}"
    );
    assert!(
        body.left() <= gear.center().x && gear.center().x <= body.right(),
        "over the gear"
    );
}

#[test]
fn the_box_of_a_track_shows_what_the_track_has_now() {
    let mut project = project_of(300);
    let track = project.tracks()[0].id();
    project.set_track_speed(track, SpeedFactor::new(4).unwrap()).unwrap();
    project.set_track_gain(track, Gain::from_decibels(-6.0)).unwrap();
    project.set_track_muted(track, true).unwrap();
    let mut window = Window::showing(project);
    window.click(window.gear_of_track(0).center());

    assert!(window.is_checked("Mute"));
    assert_eq!(slider_value(&window, "Speed"), 4.0);
    assert_eq!(slider_value(&window, "Gain"), -6.0);
}

fn slider_value(window: &Window, label: &str) -> f64 {
    window
        .harness
        .get_by_label(label)
        .accesskit_node()
        .numeric_value()
        .unwrap_or_else(|| panic!("{label} is a slider with a value"))
}

#[test]
fn the_speed_gain_and_mute_of_a_track_can_all_be_changed_in_its_box() {
    let mut window = Window::showing(project_of(300));
    window.click(window.gear_of_track(0).center());
    let track = window.application().project().tracks()[0].id();

    window.set_slider("Speed", 2.0);
    assert_eq!(
        window.application().project().track(track).unwrap().speed(),
        SpeedFactor::new(2).unwrap()
    );
    assert_eq!(
        window.application().project().total_frame_count(),
        150,
        "everything on the track plays twice as fast, so it is half as long"
    );

    window.set_slider("Gain", -12.0);
    assert_eq!(
        window.application().project().track(track).unwrap().gain(),
        Gain::from_decibels(-12.0)
    );

    window.harness.get_by_label("Mute").click();
    window.settle();
    assert!(window.application().project().track(track).unwrap().is_muted());
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn the_gain_slider_goes_as_far_quieter_as_louder_with_no_change_in_the_middle() {
    let mut window = Window::showing(project_of(300));
    window.click(window.gear_of_track(0).center());
    let track = window.application().project().tracks()[0].id();
    let gain = |window: &Window| window.application().project().track(track).unwrap().gain().decibels();

    window.set_slider("Gain", 1000.0);
    assert_eq!(gain(&window), 30.0, "no louder than 30 dB");
    window.set_slider("Gain", -1000.0);
    assert_eq!(gain(&window), -30.0, "and no quieter than minus 30");
    assert_eq!(
        Gain::SMALLEST_DECIBELS,
        -Gain::LARGEST_DECIBELS,
        "so 0 dB is the middle of the slider"
    );
    window.set_slider("Gain", 0.0);
    assert_eq!(gain(&window), 0.0);
}

#[test]
fn the_speed_slider_goes_from_normal_speed_to_a_thousand_times() {
    let mut window = Window::showing(project_of(1000));
    window.click(window.gear_of_track(0).center());
    let track = window.application().project().tracks()[0].id();
    let speed = |window: &Window| window.application().project().track(track).unwrap().speed().value();

    window.set_slider("Speed", 5000.0);
    assert_eq!(speed(&window), 1000);
    window.set_slider("Speed", 0.0);
    assert_eq!(speed(&window), 1, "never below normal speed");
}

#[test]
fn the_box_closes_on_a_click_outside_it_and_on_escape_and_when_the_gear_is_pressed_again() {
    let mut window = Window::showing(project_of(300));
    let gear = window.gear_of_track(0).center();

    window.click(gear);
    assert!(window.application().timeline_view.settings_box_is_open());
    window.click(Pos2::new(640.0, 120.0));
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "a click outside closes it"
    );

    window.click(gear);
    assert!(window.application().timeline_view.settings_box_is_open());
    window.press_key(Key::Escape);
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "escape closes it"
    );
    assert_eq!(
        window.application().selected_clip(),
        None,
        "and was not used for anything else"
    );

    window.click(gear);
    assert!(window.application().timeline_view.settings_box_is_open());
    window.click(gear);
    assert!(
        !window.application().timeline_view.settings_box_is_open(),
        "the gear again closes it"
    );
}

#[test]
fn clicking_inside_the_box_leaves_it_open() {
    let mut window = Window::showing(project_of(300));
    window.click(window.gear_of_track(0).center());
    let body = the_box_that_is_open(&window);
    window.click(body.center());
    assert!(window.application().timeline_view.settings_box_is_open());
}

#[test]
fn the_keys_belong_to_the_box_while_it_is_open() {
    let mut window = Window::showing(project_of(300));
    window.move_current_frame_to(50);
    window.click(window.gear_of_track(0).center());
    for key in [Key::Space, Key::S, Key::ArrowRight] {
        window.press_key(key);
    }
    assert!(!window.application().is_playing());
    assert_eq!(window.clips(), vec![(0, 300, 1)]);
    assert_eq!(window.current_frame(), 50);
}

#[test]
fn each_track_has_a_box_of_its_own_and_the_gear_of_another_track_switches_to_it() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    window.click(window.gear_of_track(0).center());
    let first = window.application().project().tracks()[0].id();
    let second = window.application().project().tracks()[1].id();
    assert_eq!(
        window.application().timeline_view.settings_box_subject(),
        Some(SettingsBoxSubject::Track(first))
    );
    window.click(window.gear_of_track(1).center());
    assert_eq!(
        window.application().timeline_view.settings_box_subject(),
        Some(SettingsBoxSubject::Track(second))
    );
}

#[test]
fn the_box_goes_when_its_track_does_not_exist_any_more() {
    let mut window = Window::showing(project_of(300));
    window.click(window.gear_of_track(0).center());
    assert!(window.application().timeline_view.settings_box_is_open());
    window.application_mut().start_empty_project();
    window.settle();
    assert!(!window.application().timeline_view.settings_box_is_open());
}
