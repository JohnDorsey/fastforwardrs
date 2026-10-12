//! Tests of dragging keyframes, alone and with clips, and of drawing a rectangle round keyframes to select them.

use egui::{Key, PointerButton, Pos2, vec2};

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::selection::SelectionItem;
use crate::project::{AnimationTarget, ClipId, KeyframeId, Project, PropertyId, TextObject, TimelineFrameIndex};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

/// A window with a video of 300 frames and a text object in front of it from frame 40 to 100, with nothing unsaved.
pub(super) fn window_with_a_video_and_a_text() -> (Window, ClipId, ClipId) {
    let mut project: Project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    let in_front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(in_front, frame(40), TextObject::default(), 60)
        .expect("there is room");
    let mut window = Window::showing(project);
    window.application_mut().note_everything_is_saved();
    (window, video, text)
}

pub(super) fn make_a_keyframe(
    window: &mut Window,
    target: AnimationTarget,
    property: PropertyId,
    at: u64,
) -> KeyframeId {
    window.do_this(Command::AddKeyframe {
        target,
        property,
        frame: frame(at),
    });
    window.harness.run_steps(40);
    window
        .application()
        .project()
        .keyframes_of(target, property)
        .iter()
        .find(|keyframe| keyframe.frame == at)
        .expect("the keyframe is there");
    keyframe_at(window, target, property, at)
}

pub(super) fn keyframe_at(window: &Window, target: AnimationTarget, property: PropertyId, at: u64) -> KeyframeId {
    let project = window.application().project();
    let location = project
        .locate_the_keyframe_at(target, property, at)
        .expect("a keyframe is there");
    let rack = project.located_rack(location.rack).unwrap().rack;
    rack.id_of(rack.at(location.frame_in_the_rack).unwrap())
}

pub(super) fn frame_of(window: &Window, keyframe: KeyframeId) -> u64 {
    window
        .application()
        .project()
        .timeline_frame_of_keyframe(keyframe)
        .expect("the keyframe is there")
}

/// The middle of the diamond of `keyframe`: where it was drawn in the last frame.
pub(super) fn where_it_is(window: &Window, keyframe: KeyframeId) -> Pos2 {
    use egui_kittest::kittest::Queryable;
    window
        .harness
        .get_by_label(&crate::application::timeline_panel::keyframe_label(keyframe))
        .rect()
        .center()
}

pub(super) fn points_per_frame(window: &Window) -> f32 {
    window.x_of_frame_on_the_strip(0, 11) - window.x_of_frame_on_the_strip(0, 10)
}

pub(super) fn selected_keyframes(window: &Window) -> Vec<KeyframeId> {
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

// --- dragging a keyframe ---

#[test]
fn dragging_a_keyframe_moves_it_along_the_timeline_as_far_as_the_pointer_goes_and_selects_it() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    let from = where_it_is(&window, keyframe);

    window.drag(
        from,
        from + vec2(20.0 * points_per_frame(&window), 0.0),
        PointerButton::Primary,
    );

    let after = frame_of(&window, keyframe);
    assert!(
        (118..=120).contains(&after),
        "it went 20 frames, less what the first move of the pointer took: {after}"
    );
    assert_eq!(selected_keyframes(&window), vec![keyframe]);
    assert_eq!(
        window
            .application()
            .project()
            .keyframes_of(AnimationTarget::Clip(video), PropertyId::Gain)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect::<Vec<_>>(),
        vec![after],
        "the property has the keyframe it has moved"
    );
}

#[test]
fn a_dragged_keyframe_goes_back_and_forth_with_the_pointer_and_the_drag_is_one_step_of_the_history() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    window.application_mut().note_everything_is_saved();
    let from = where_it_is(&window, keyframe);
    let per_frame = points_per_frame(&window);

    window.drag_and_hold(from, from + vec2(30.0 * per_frame, 0.0), PointerButton::Primary);
    assert!(frame_of(&window, keyframe) > 120);
    window.move_pointer_to(from + vec2(-10.0 * per_frame, 0.0));
    window.move_pointer_to(from + vec2(-10.0 * per_frame, 0.0));
    assert!(
        frame_of(&window, keyframe) <= 91,
        "back past where it started: {}",
        frame_of(&window, keyframe)
    );
    window.release(from + vec2(-10.0 * per_frame, 0.0), PointerButton::Primary);
    window.settle();

    assert!(window.application().has_unsaved_changes());
    window.do_this(Command::Undo);
    assert_eq!(frame_of(&window, keyframe), 100, "one undo takes the whole drag back");
}

#[test]
fn a_dragged_keyframe_stops_at_the_end_of_its_drawer_and_at_the_start_of_it() {
    let (mut window, _, text) = window_with_a_video_and_a_text();
    // The drawer of the text lasts from frame 40 to frame 99.
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 60);
    let per_frame = points_per_frame(&window);
    let from = where_it_is(&window, keyframe);

    window.drag(from, from + vec2(300.0 * per_frame, 0.0), PointerButton::Primary);
    assert_eq!(frame_of(&window, keyframe), 99, "the last frame of the text");

    let from = where_it_is(&window, keyframe);
    window.drag(from, from + vec2(-300.0 * per_frame, 0.0), PointerButton::Primary);
    assert_eq!(frame_of(&window, keyframe), 40, "the first frame of the text");
}

#[test]
fn a_dragged_keyframe_stops_short_of_the_keyframe_of_its_rack_that_is_next_to_it() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    let first = make_a_keyframe(&mut window, target, PropertyId::Gain, 100);
    let second = make_a_keyframe(&mut window, target, PropertyId::Gain, 130);
    let per_frame = points_per_frame(&window);
    let from = where_it_is(&window, first);

    window.drag(from, from + vec2(100.0 * per_frame, 0.0), PointerButton::Primary);

    assert_eq!(frame_of(&window, first), 129, "one frame before the other");
    assert_eq!(frame_of(&window, second), 130, "which did not move");
}

#[test]
fn keyframes_that_are_selected_together_are_dragged_together_and_keep_the_distance_between_them() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    let gain = make_a_keyframe(&mut window, target, PropertyId::Gain, 100);
    let rotation = make_a_keyframe(&mut window, target, PropertyId::Rotation, 120);
    let per_frame = points_per_frame(&window);
    let at_the_gain = where_it_is(&window, gain);
    window.click(at_the_gain);
    window.click_with_control(where_it_is(&window, rotation));
    assert_eq!(selected_keyframes(&window).len(), 2);

    let from = where_it_is(&window, rotation);
    window.drag(from, from + vec2(20.0 * per_frame, 0.0), PointerButton::Primary);

    let (gain_now, rotation_now) = (frame_of(&window, gain), frame_of(&window, rotation));
    assert_eq!(rotation_now - gain_now, 20, "the distance between them is what it was");
    assert!(gain_now > 110, "and they moved: {gain_now}");
    assert_eq!(selected_keyframes(&window).len(), 2, "both are still selected");
}

#[test]
fn keyframes_dragged_together_are_stopped_when_the_first_of_them_comes_to_the_end_of_its_drawer() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    let in_the_text = make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 50);
    // The video's drawer is not the text's: the keyframes of the two are limited by their own drawers.
    let in_the_video = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 90);
    let per_frame = points_per_frame(&window);
    window.click(where_it_is(&window, in_the_text));
    window.click_with_control(where_it_is(&window, in_the_video));

    let from = where_it_is(&window, in_the_video);
    window.drag(from, from + vec2(100.0 * per_frame, 0.0), PointerButton::Primary);

    assert_eq!(
        frame_of(&window, in_the_text),
        99,
        "the one in the drawer of the text stops at its end"
    );
    assert_eq!(
        frame_of(&window, in_the_video) - 90,
        frame_of(&window, in_the_text) - 50,
        "and the other stopped with it, having gone as far"
    );
}

#[test]
fn dragging_a_keyframe_that_is_not_selected_selects_it_in_place_of_what_was_selected() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let first = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    let second = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Rotation, 150);
    window.click(where_it_is(&window, first));
    window.click_with_control(where_it_is(&window, second));
    // A third, which is not selected.
    let third = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Position, 70);
    let from = where_it_is(&window, third);

    window.drag(
        from,
        from + vec2(5.0 * points_per_frame(&window), 0.0),
        PointerButton::Primary,
    );

    assert_eq!(selected_keyframes(&window), vec![third]);
    assert_eq!(frame_of(&window, first), 100, "the others did not move");
    assert_eq!(frame_of(&window, second), 150);
}

// --- dragging clips and keyframes together ---

#[test]
fn a_clip_that_is_dragged_takes_the_selected_keyframes_that_are_in_other_drawers_the_same_way() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 150);
    let per_frame = points_per_frame(&window);
    window.click(window.middle_of_the_track_at(70));
    window.click_with_control(where_it_is(&window, keyframe));
    assert_eq!(window.application().selected_clips(), vec![text]);
    assert_eq!(selected_keyframes(&window), vec![keyframe]);
    let start_of_the_text = window
        .application()
        .project()
        .placement_of(text)
        .unwrap()
        .first_timeline_frame
        .value();

    let from = window.middle_of_the_track_at(70);
    window.drag(from, from + vec2(40.0 * per_frame, 0.0), PointerButton::Primary);

    let text_moved_by = window
        .application()
        .project()
        .placement_of(text)
        .unwrap()
        .first_timeline_frame
        .value()
        - start_of_the_text;
    assert!(text_moved_by >= 36, "the text went along: {text_moved_by}");
    assert_eq!(
        frame_of(&window, keyframe) - 150,
        text_moved_by,
        "and the keyframe went as far, in the same frames"
    );
}

#[test]
fn a_keyframe_that_is_dragged_takes_the_selected_clips_the_same_way() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 150);
    let per_frame = points_per_frame(&window);
    window.click(window.middle_of_the_track_at(70));
    window.click_with_control(where_it_is(&window, keyframe));
    let start_of_the_text = window
        .application()
        .project()
        .placement_of(text)
        .unwrap()
        .first_timeline_frame
        .value();

    let from = where_it_is(&window, keyframe);
    window.drag(from, from + vec2(25.0 * per_frame, 0.0), PointerButton::Primary);

    let text_moved_by = window
        .application()
        .project()
        .placement_of(text)
        .unwrap()
        .first_timeline_frame
        .value()
        - start_of_the_text;
    assert!(text_moved_by >= 21, "{text_moved_by}");
    assert_eq!(frame_of(&window, keyframe) - 150, text_moved_by);
}

#[test]
fn a_keyframe_in_the_drawer_of_a_clip_that_is_dragged_goes_with_the_clip_and_is_not_moved_a_second_time() {
    let (mut window, _, text) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 60);
    let per_frame = points_per_frame(&window);
    window.click(window.middle_of_the_track_at(90));
    window.click_with_control(where_it_is(&window, keyframe));
    let place_in_the_drawer = |window: &Window| {
        let located = window.application().project().located_rack(keyframe.rack()).unwrap();
        located.rack.keyframe(keyframe).unwrap().frame
    };
    let before = place_in_the_drawer(&window);

    let from = window.middle_of_the_track_at(90);
    window.drag(from, from + vec2(30.0 * per_frame, 0.0), PointerButton::Primary);

    assert!(
        window
            .application()
            .project()
            .placement_of(text)
            .unwrap()
            .first_timeline_frame
            .value()
            > 40,
        "the clip moved"
    );
    assert_eq!(
        place_in_the_drawer(&window),
        before,
        "the keyframe is where it was in the drawer, which went with the clip"
    );
}

#[test]
fn clips_and_keyframes_dragged_together_are_one_step_of_the_history() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 150);
    let per_frame = points_per_frame(&window);
    window.click(window.middle_of_the_track_at(70));
    window.click_with_control(where_it_is(&window, keyframe));
    window.application_mut().note_everything_is_saved();
    let from = window.middle_of_the_track_at(70);

    window.drag(from, from + vec2(40.0 * per_frame, 0.0), PointerButton::Primary);
    assert!(window.application().has_unsaved_changes());
    window.do_this(Command::Undo);

    assert_eq!(
        window
            .application()
            .project()
            .placement_of(text)
            .unwrap()
            .first_timeline_frame
            .value(),
        40
    );
    assert_eq!(frame_of(&window, keyframe), 150);
}

#[test]
fn a_drag_of_clips_and_keyframes_that_begins_by_going_to_another_track_is_one_step_of_the_history_too() {
    let (mut window, video, text) = window_with_a_video_and_a_text();
    // An empty track behind the other two: the text is on the first track, and the video, which has a keyframe, on the second.
    window.do_this(Command::AddTrack);
    let empty = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveTrack {
        track: empty,
        to_position: 2,
    });
    let keyframe = make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 150);
    let per_frame = points_per_frame(&window);
    let from = window.middle_of_the_strip_at(0, 70);
    window.click(from);
    window.click_with_control(where_it_is(&window, keyframe));
    assert_eq!(window.application().selected_clips(), vec![text]);
    assert_eq!(selected_keyframes(&window), vec![keyframe]);
    window.application_mut().note_everything_is_saved();
    let before = window.application().project().clone();

    // Straight down to the empty track, which is no time at all, and then along it.
    let down = window.middle_of_the_strip_at(2, 70);
    window.drag_and_hold(from, down, PointerButton::Primary);
    for step in 1..=4 {
        window.move_pointer_to(down + vec2(10.0 * step as f32 * per_frame, 0.0));
    }
    window.release(down + vec2(40.0 * per_frame, 0.0), PointerButton::Primary);
    window.settle();

    assert_eq!(
        window.application().project().track_position_holding(text),
        Some(2),
        "the text went to the empty track"
    );
    assert!(
        frame_of(&window, keyframe) > 150,
        "and the keyframe went along: {}",
        frame_of(&window, keyframe)
    );
    window.do_this(Command::Undo);
    assert_eq!(
        window.application().project(),
        &before,
        "one undo takes the whole drag back, the frames that only changed the track included"
    );
}

// --- a rectangle round keyframes, and taking them away ---

#[test]
fn a_rectangle_drawn_round_keyframes_selects_them_and_not_those_it_does_not_go_round() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    let inside = make_a_keyframe(&mut window, target, PropertyId::Gain, 100);
    let also_inside = make_a_keyframe(&mut window, target, PropertyId::Rotation, 110);
    let outside = make_a_keyframe(&mut window, target, PropertyId::Position, 160);
    let (a, b, c) = (
        where_it_is(&window, inside),
        where_it_is(&window, also_inside),
        where_it_is(&window, outside),
    );
    // The rectangle is begun in an empty place of the strip of the track in front, and goes down over the racks.
    let begin = window.middle_of_the_strip_at(0, 135);
    let end = Pos2::new(a.x - 12.0, c.y + 10.0);
    assert!(
        begin.x > b.x + 12.0 && begin.x < c.x - 12.0,
        "between the keyframes that are to be selected and the other"
    );

    window.drag(begin, end, PointerButton::Primary);

    let selected = selected_keyframes(&window);
    assert!(
        selected.contains(&inside) && selected.contains(&also_inside),
        "{selected:?}"
    );
    assert!(!selected.contains(&outside), "{selected:?}");
}

#[test]
fn the_delete_key_takes_away_the_selected_keyframes_and_leaves_the_rest() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    let first = make_a_keyframe(&mut window, target, PropertyId::Gain, 100);
    make_a_keyframe(&mut window, target, PropertyId::Gain, 120);
    window.click(where_it_is(&window, first));

    window.press_key(Key::Delete);

    let project = window.application().project();
    assert!(
        project.keyframes_of(target, PropertyId::Gain).at(100).is_none(),
        "the selected one is gone"
    );
    assert!(
        project.keyframes_of(target, PropertyId::Gain).at(120).is_some(),
        "and the other is there"
    );
}
