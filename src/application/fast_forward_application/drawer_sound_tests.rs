//! Tests of the sounds that drawers make: one as a drawer slides open and another as it slides shut, played by the editor like the
//! snip of a cut is. (What the sounds are like is the business of `playback::sound_effects::drawer`; these are about when they
//! are made.)

use egui_kittest::kittest::Queryable;

use super::drawer_motion_tests::{press_the_button_of_the_drawer_of_the_first_track, window_with_two_tracks};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::timeline_panel::track_drawer_button_label;
use crate::playback::SoundEffect;
use crate::project::{AnimationTarget, PropertyId, TimelineFrameIndex};

/// How the sounds sound for the speakers the tests give the editor (they play 48000 moments a second).
fn opening_sound() -> Vec<f32> {
    SoundEffect::DrawerOpening.synthesize(48000)
}

fn closing_sound() -> Vec<f32> {
    SoundEffect::DrawerClosing.synthesize(48000)
}

fn let_the_drawers_finish(window: &mut Window) {
    window.harness.run_steps(40);
}

#[test]
fn a_drawer_that_is_opened_with_the_mouse_makes_the_sound_of_a_drawer_opening_once() {
    let mut window = window_with_two_tracks();
    let log = window.log_the_sound_effects();

    press_the_button_of_the_drawer_of_the_first_track(&mut window);
    let_the_drawers_finish(&mut window);

    let played = log.sounds_played();
    assert_eq!(
        played.len(),
        1,
        "one sound for the whole of the sliding, and not one for each frame of it"
    );
    assert_eq!(*played[0], *opening_sound());
}

#[test]
fn a_drawer_that_is_shut_with_the_mouse_makes_the_sound_of_a_drawer_shutting() {
    let mut window = window_with_two_tracks();
    let log = window.log_the_sound_effects();
    press_the_button_of_the_drawer_of_the_first_track(&mut window);
    let_the_drawers_finish(&mut window);

    press_the_button_of_the_drawer_of_the_first_track(&mut window);
    let_the_drawers_finish(&mut window);

    let played = log.sounds_played();
    assert_eq!(played.len(), 2);
    assert_eq!(*played[0], *opening_sound());
    assert_eq!(*played[1], *closing_sound());
    assert_ne!(*played[0], *played[1], "they are not one sound");
}

#[test]
fn a_drawer_that_is_opened_over_the_rows_under_it_with_shift_makes_the_same_sound() {
    let mut window = window_with_two_tracks();
    let log = window.log_the_sound_effects();

    window.hold_shift();
    press_the_button_of_the_drawer_of_the_first_track(&mut window);
    window.let_go_of_shift();
    let_the_drawers_finish(&mut window);

    assert_eq!(log.count_of_sounds_played(), 1);
    assert_eq!(*log.sounds_played()[0], *opening_sound());
}

#[test]
fn the_drawer_of_a_track_makes_the_sounds_too() {
    let mut window = window_with_two_tracks();
    let log = window.log_the_sound_effects();
    let label = track_drawer_button_label(&window.title_of_track(0));
    let box_of_the_track = window.title_box_of_track(0);
    window.move_pointer_to(box_of_the_track.center());
    window.harness.run_steps(2);
    let button = window.harness.get_by_label(&label).rect().center();

    window.click(button);
    let_the_drawers_finish(&mut window);

    assert_eq!(log.count_of_sounds_played(), 1);
    assert_eq!(*log.sounds_played()[0], *opening_sound());
}

#[test]
fn a_keyframe_made_in_a_drawer_that_is_shut_opens_it_with_the_sound_and_one_made_in_a_drawer_that_is_open_does_not() {
    let mut window = window_with_two_tracks();
    let log = window.log_the_sound_effects();
    let clip = window.clip(0, 0);
    let make_a_keyframe_at = |window: &mut Window, frame: u64| {
        window.do_this(Command::AddKeyframe {
            target: AnimationTarget::Clip(clip),
            property: PropertyId::Gain,
            frame: TimelineFrameIndex::new(frame),
        });
        let_the_drawers_finish(window);
    };

    make_a_keyframe_at(&mut window, 10);
    assert_eq!(
        log.count_of_sounds_played(),
        1,
        "the drawer that the keyframe is in opened"
    );
    assert_eq!(*log.sounds_played()[0], *opening_sound());

    make_a_keyframe_at(&mut window, 20);
    assert_eq!(log.count_of_sounds_played(), 1, "it was open already");
}

#[test]
fn a_clip_that_is_only_hovered_over_makes_no_sound_with_its_button_which_is_not_pressed() {
    let mut window = window_with_two_tracks();
    let log = window.log_the_sound_effects();
    let strip = window.strip_of_track(0);

    window.move_pointer_to(egui::pos2(strip.center().x, strip.top() + 20.0));
    window.harness.run_steps(10);

    assert_eq!(log.count_of_sounds_played(), 0);
}

#[test]
fn drawers_work_as_ever_and_say_nothing_when_the_editor_has_no_speakers() {
    let mut window = window_with_two_tracks();
    let distance_between_the_rows = |window: &Window| window.strip_of_track(1).top() - window.strip_of_track(0).top();
    let distance_before = distance_between_the_rows(&window);

    press_the_button_of_the_drawer_of_the_first_track(&mut window);
    let_the_drawers_finish(&mut window);

    assert!(
        distance_between_the_rows(&window) > distance_before + 10.0,
        "the drawer opened and pushed the row under it down"
    );
    assert_eq!(window.application().sound_effects.last_problem(), None);
}

#[test]
fn undoing_and_redoing_what_was_done_in_a_drawer_make_no_sound_of_drawers() {
    let mut window = window_with_two_tracks();
    let clip = window.clip(0, 0);
    window.do_this(Command::AddKeyframe {
        target: AnimationTarget::Clip(clip),
        property: PropertyId::Gain,
        frame: TimelineFrameIndex::new(10),
    });
    let_the_drawers_finish(&mut window);
    let log = window.log_the_sound_effects();

    window.do_this(Command::Undo);
    window.do_this(Command::Redo);
    let_the_drawers_finish(&mut window);

    assert_eq!(
        log.count_of_sounds_played(),
        0,
        "the drawer is as it was, and is not opened again by being drawn"
    );
}
