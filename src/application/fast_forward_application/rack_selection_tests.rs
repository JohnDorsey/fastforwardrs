//! Tests of selecting racks of keyframes (by command, with the mouse, and the keys that are about what is selected), and of what the
//! properties panel says of the racks that are selected.

use egui::Key;
use egui_kittest::kittest::Queryable;

use super::keyframe_drag_tests::{make_a_keyframe, window_with_a_video_and_a_text};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{AnimationTarget, ClipId, PropertyId, RackId};

/// A window with a video and a text, where the video has a rack of its loudness with keyframes at frames 100 and 120 (in its own
/// drawer) and another of its rotation with one at 150.
fn window_with_two_racks() -> (Window, ClipId, RackId, RackId) {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    make_a_keyframe(&mut window, target, PropertyId::Gain, 100);
    make_a_keyframe(&mut window, target, PropertyId::Gain, 120);
    make_a_keyframe(&mut window, target, PropertyId::Rotation, 150);
    let project = window.application().project();
    let drawer = project.drawer_of(target).unwrap();
    let loudness = drawer.first_rack_for(target, PropertyId::Gain).unwrap().id();
    let turn = drawer.first_rack_for(target, PropertyId::Rotation).unwrap().id();
    (window, video, loudness, turn)
}

fn select(window: &mut Window, racks: &[RackId], change: SelectionChange) {
    window.do_this(Command::SelectItems {
        items: racks.iter().copied().map(SelectionItem::Rack).collect(),
        change,
    });
    window.harness.run_steps(5);
}

fn selected_racks(window: &Window) -> Vec<RackId> {
    window
        .application()
        .selected_items()
        .into_iter()
        .filter_map(|item| match item {
            SelectionItem::Rack(rack) => Some(rack),
            _ => None,
        })
        .collect()
}

// --- the properties panel ---

#[test]
fn a_selected_rack_has_a_box_in_the_properties_panel_that_says_what_it_animates_and_how_many_keyframes_it_has() {
    let (mut window, _, loudness, _) = window_with_two_racks();

    select(&mut window, &[loudness], SelectionChange::Replace);

    assert!(
        window.harness.query_by_label("Rack of keyframes").is_some(),
        "the box is headed with what it is"
    );
    assert!(
        window.harness.query_by_label_contains("2 keyframes, from").is_some(),
        "how many keyframes, and from when to when"
    );
}

#[test]
fn a_rack_with_one_keyframe_says_when_it_is_at() {
    let (mut window, _, _, turn) = window_with_two_racks();

    select(&mut window, &[turn], SelectionChange::Replace);

    assert!(window.harness.query_by_label_contains("1 keyframe, at").is_some());
    assert!(window.harness.query_by_label_contains("keyframes, from").is_none());
}

#[test]
fn several_selected_racks_are_listed_and_clicking_one_selects_only_that_one() {
    let (mut window, _, loudness, turn) = window_with_two_racks();
    select(&mut window, &[loudness, turn], SelectionChange::Replace);

    assert!(
        window.harness.query_by_label("Rack of keyframes").is_none(),
        "no box: there are two"
    );
    let entry = window
        .harness
        .get_by_label_contains("Rack of keyframes: Rotation (1 keyframe, at");
    let middle = entry.rect().center();
    window.click(middle);

    assert_eq!(selected_racks(&window), vec![turn]);
    assert!(
        window.harness.query_by_label("Rack of keyframes").is_some(),
        "now it has its box"
    );
}

#[test]
fn a_rack_and_a_keyframe_that_are_selected_together_are_both_listed() {
    let (mut window, _, loudness, turn) = window_with_two_racks();
    let keyframe_of_the_turn = window
        .application()
        .project()
        .located_rack(turn)
        .unwrap()
        .rack
        .keyframes()[0]
        .id();

    window.do_this(Command::SelectItems {
        items: vec![
            SelectionItem::Rack(loudness),
            SelectionItem::Keyframe(keyframe_of_the_turn),
        ],
        change: SelectionChange::Replace,
    });
    window.harness.run_steps(5);

    assert!(
        window
            .harness
            .query_by_label_contains("Rack of keyframes: Gain")
            .is_some()
    );
    assert!(window.harness.query_by_label_contains("Keyframe: Rotation").is_some());
}

// --- the keys ---

#[test]
fn the_delete_key_takes_away_the_selected_racks_with_their_keyframes_and_leaves_the_rest() {
    let (mut window, video, loudness, turn) = window_with_two_racks();
    select(&mut window, &[loudness], SelectionChange::Replace);

    window.press_key(Key::Delete);

    let project = window.application().project();
    assert!(project.located_rack(loudness).is_none(), "the selected rack is gone");
    assert!(
        !project.is_animated(AnimationTarget::Clip(video), PropertyId::Gain),
        "and so are its keyframes"
    );
    assert!(project.located_rack(turn).is_some(), "the other is there");
    assert!(selected_racks(&window).is_empty());
}

#[test]
fn escape_lets_go_of_the_selected_racks() {
    let (mut window, _, loudness, turn) = window_with_two_racks();
    select(&mut window, &[loudness, turn], SelectionChange::Replace);

    window.press_key(Key::Escape);

    assert!(selected_racks(&window).is_empty());
}
