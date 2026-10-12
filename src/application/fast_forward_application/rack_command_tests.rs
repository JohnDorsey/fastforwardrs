//! Tests of what the commands about keyframes and racks do in the editor: where a keyframe that was just made is seen,
//! and moving, taking away and selecting keyframes.

use egui::Shape;
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::theme;
use crate::project::{
    AnimationTarget, ClipId, Gain, KeyframeId, KeyframeMove, PropertyId, RackDestination, RackId, TimelineFrameIndex,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

/// A window with a video of 300 frames, with nothing unsaved and nothing to undo, and its one clip.
fn window_with_a_video() -> (Window, ClipId) {
    let mut window = Window::showing(project_of(300));
    window.application_mut().note_everything_is_saved();
    let clip = window.clip(0, 0);
    (window, clip)
}

fn drawers_drawn(window: &Window) -> usize {
    window
        .shapes_drawn()
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND))
        .count()
}

fn add_a_keyframe(window: &mut Window, target: AnimationTarget, property: PropertyId, at: u64) {
    window.do_this(Command::AddKeyframe {
        target,
        property,
        frame: frame(at),
    });
}

/// The keyframes of the rack that the clip has for its property, which was made in its own drawer.
fn keyframes_of_the_rack_of(window: &Window, clip: ClipId, property: PropertyId) -> (RackId, Vec<KeyframeId>) {
    let drawer = window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(clip))
        .unwrap();
    let rack = drawer
        .rack_for(AnimationTarget::Clip(clip), property)
        .expect("the clip has a rack for it");
    (
        rack.id(),
        rack.keyframes().iter().map(|keyframe| rack.id_of(keyframe)).collect(),
    )
}

fn selected_keyframes(window: &Window) -> Vec<KeyframeId> {
    window
        .application()
        .selected_items()
        .into_iter()
        .filter_map(|item| match item {
            SelectionItem::Keyframe(keyframe) => Some(keyframe),
            _ => None,
        })
        .collect()
}

// --- the drawer opens where a keyframe is made ---

#[test]
fn a_keyframe_made_with_the_stopwatch_opens_the_drawer_it_is_in() {
    let (mut window, clip) = window_with_a_video();
    window.harness.run_steps(40);
    assert_eq!(drawers_drawn(&window), 0, "the drawer is shut to begin with");

    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    window.harness.run_steps(40);

    assert_eq!(drawers_drawn(&window), 1, "the drawer of the clip is open");
}

#[test]
fn a_keyframe_made_for_a_track_opens_the_drawer_of_the_track() {
    let (mut window, _) = window_with_a_video();
    let track = window.application().project().tracks()[0].id();

    add_a_keyframe(&mut window, AnimationTarget::Track(track), PropertyId::Gain, 20);
    window.harness.run_steps(40);

    assert_eq!(drawers_drawn(&window), 1);
    let strip = window.strip_of_track(0);
    let drawer = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND => Some(rectangle.rect),
            _ => None,
        })
        .unwrap();
    assert!(
        (drawer.left() - strip.left()).abs() < 0.6 && (drawer.right() - strip.right()).abs() < 0.6,
        "it is the one of the track, which is as wide as the strip"
    );
}

#[test]
fn changing_a_property_that_is_animated_at_a_frame_that_has_no_keyframe_opens_the_drawer_too() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 0);
    window.harness.run_steps(40);
    // The drawer was opened by the first keyframe: shut it, as a person would.
    let button = window
        .harness
        .get_by_label(&crate::application::timeline_panel::drawer_button_label(
            &window.title_of_track(0),
            clip,
        ))
        .rect()
        .center();
    window.click(button);
    window.harness.run_steps(40);
    assert_eq!(drawers_drawn(&window), 0, "shut by hand");

    window.move_current_frame_to(50);
    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-9.0),
    });
    window.harness.run_steps(40);

    assert_eq!(
        drawers_drawn(&window),
        1,
        "a keyframe was made at frame 50, and the drawer opened"
    );
}

#[test]
fn a_keyframe_that_was_there_already_does_not_open_the_drawer_again() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    window.harness.run_steps(40);
    let button = window
        .harness
        .get_by_label(&crate::application::timeline_panel::drawer_button_label(
            &window.title_of_track(0),
            clip,
        ))
        .rect()
        .center();
    window.click(button);
    window.harness.run_steps(40);
    assert_eq!(drawers_drawn(&window), 0);

    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    window.harness.run_steps(40);

    assert_eq!(drawers_drawn(&window), 0, "nothing was made, so nothing was opened");
}

#[test]
fn a_drawer_that_is_open_stays_open_when_another_keyframe_is_made() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 40);
    window.harness.run_steps(40);

    assert_eq!(
        drawers_drawn(&window),
        1,
        "it is open, and it was not shut by the second keyframe"
    );
}

// --- selecting, moving and taking away keyframes ---

#[test]
fn keyframes_are_selected_with_the_command_that_selects_items_and_forgotten_when_taken_away() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 40);
    let (_, keyframes) = keyframes_of_the_rack_of(&window, clip, PropertyId::Gain);
    assert_eq!(keyframes.len(), 2);

    window.do_this(Command::SelectItems {
        items: keyframes.iter().copied().map(SelectionItem::Keyframe).collect(),
        change: SelectionChange::Replace,
    });
    assert_eq!(selected_keyframes(&window), keyframes);

    window.do_this(Command::SelectItems {
        items: vec![
            SelectionItem::Keyframe(keyframes[0]),
            SelectionItem::Keyframe(KeyframeId::new(RackId::new(clip.origin(), 99), 0)),
        ],
        change: SelectionChange::Toggle,
    });
    assert_eq!(
        selected_keyframes(&window),
        vec![keyframes[1]],
        "the first was let go of and the one that is not there was not selected"
    );

    window.do_this(Command::DeleteKeyframes(vec![keyframes[1]]));
    assert!(
        window.application().selected_items().is_empty(),
        "a keyframe that is gone is not selected"
    );
    assert!(
        window
            .application()
            .project()
            .is_animated(AnimationTarget::Clip(clip), PropertyId::Gain),
        "the other keyframe is there"
    );
}

#[test]
fn deleting_keyframes_takes_them_away_and_it_is_one_edit_that_can_be_undone() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 40);
    let (rack, keyframes) = keyframes_of_the_rack_of(&window, clip, PropertyId::Gain);
    window.application_mut().note_everything_is_saved();

    window.do_this(Command::DeleteKeyframes(vec![keyframes[0]]));

    assert_eq!(
        window
            .application()
            .project()
            .keyframes_of(AnimationTarget::Clip(clip), PropertyId::Gain)
            .len(),
        1
    );
    assert!(window.application().has_unsaved_changes());
    window.do_this(Command::Undo);
    assert_eq!(
        window.application().project().located_rack(rack).unwrap().rack.len(),
        2,
        "undone"
    );
}

#[test]
fn moving_keyframes_with_the_command_moves_them_and_dragging_them_is_one_step_of_the_history() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    let (rack, keyframes) = keyframes_of_the_rack_of(&window, clip, PropertyId::Gain);
    window.application_mut().note_everything_is_saved();

    for to_frame in [21, 22, 23] {
        window.do_this(Command::MoveKeyframes(vec![KeyframeMove {
            keyframe: keyframes[0],
            to_frame,
        }]));
    }

    assert_eq!(
        window
            .application()
            .project()
            .located_rack(rack)
            .unwrap()
            .rack
            .keyframes()[0]
            .frame,
        23
    );
    window.do_this(Command::Undo);
    assert_eq!(
        window
            .application()
            .project()
            .located_rack(rack)
            .unwrap()
            .rack
            .keyframes()[0]
            .frame,
        20,
        "the whole drag was one edit"
    );
}

#[test]
fn moving_a_rack_with_the_command_moves_it_to_the_drawer_it_is_told_to() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    let (rack, _) = keyframes_of_the_rack_of(&window, clip, PropertyId::Gain);
    let track = window.application().project().tracks()[0].id();

    window.do_this(Command::MoveRack {
        rack,
        to: RackDestination::Drawer(AnimationTarget::Track(track)),
        shift_in_frames: 0,
    });

    let project = window.application().project();
    assert!(project.drawer_of(AnimationTarget::Clip(clip)).unwrap().is_empty());
    assert_eq!(project.drawer_of(AnimationTarget::Track(track)).unwrap().len(), 1);
    assert_eq!(
        project
            .keyframes_of(AnimationTarget::Clip(clip), PropertyId::Gain)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect::<Vec<_>>(),
        vec![20],
        "the video has the keyframe where it was in time"
    );
}

#[test]
fn a_rack_that_cannot_go_where_it_is_told_to_is_not_moved() {
    let (mut window, clip) = window_with_a_video();
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    let (rack, _) = keyframes_of_the_rack_of(&window, clip, PropertyId::Gain);
    let before = window.application().project().clone();
    let nowhere = AnimationTarget::Track(crate::project::TrackId::new(99));

    window.do_this(Command::MoveRack {
        rack,
        to: RackDestination::Drawer(nowhere),
        shift_in_frames: 0,
    });

    assert_eq!(window.application().project(), &before);
}
