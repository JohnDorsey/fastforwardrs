//! Tests of what the mouse does to racks of keyframes and to the keyframes in them: selecting racks, the menu of a rack and the menu of
//! a keyframe (merging racks, copying keyframes, taking them away), and dragging keyframes from one rack to another.

use egui::{PointerButton, Pos2, Rect, Shape};
use egui_kittest::kittest::{NodeT, Queryable};

use super::keyframe_drag_tests::{
    frame_of, keyframe_at, make_a_keyframe, points_per_frame, selected_keyframes, where_it_is,
    window_with_a_video_and_a_text,
};
use super::rack_tests::racks_drawn;
use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::theme;
use crate::project::{AnimationTarget, ClipId, KeyframeId, PropertyId, RackDestination, RackId};

const MERGE: &str = "Merge racks";
const DELETE_RACK: &str = "Delete rack";
const DUPLICATE_RIGHT: &str = "Duplicate to the right";
const DUPLICATE_LEFT: &str = "Duplicate to the left";
const DELETE_KEYFRAME: &str = "Delete keyframe";

/// A window with a video of 300 frames and a text object in front of it, and in the open drawer of the video three racks, one
/// under another: `upper` and `lower` are of its loudness (keyframes at 100 and 120, and at 200) and `turn` is of how far it is
/// turned (a keyframe at 150).
struct Racks {
    window: Window,
    video: ClipId,
    upper: RackId,
    lower: RackId,
    turn: RackId,
}

fn window_with_racks_of_the_video() -> Racks {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    let target = AnimationTarget::Clip(video);
    for at in [100, 120] {
        make_a_keyframe(&mut window, target, PropertyId::Gain, at);
    }
    let upper = the_rack_of(&window, video, PropertyId::Gain);
    // The first rack waits in the drawer of the track in front while the others are made, so that they are racks of their own.
    let in_front = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveRack {
        rack: upper,
        to: RackDestination::Drawer(AnimationTarget::Track(in_front)),
        shift_in_frames: 0,
        slot: None,
    });
    make_a_keyframe(&mut window, target, PropertyId::Gain, 200);
    let lower = the_rack_of(&window, video, PropertyId::Gain);
    make_a_keyframe(&mut window, target, PropertyId::Rotation, 150);
    let turn = the_rack_of(&window, video, PropertyId::Rotation);
    window.do_this(Command::MoveRack {
        rack: upper,
        to: RackDestination::Drawer(target),
        shift_in_frames: 0,
        slot: Some(0),
    });
    window.harness.run_steps(40);
    window.application_mut().note_everything_is_saved();
    assert_eq!(
        window
            .application()
            .project()
            .drawer_of(target)
            .unwrap()
            .racks()
            .iter()
            .map(|rack| rack.id())
            .collect::<Vec<_>>(),
        vec![upper, lower, turn],
        "the setup: one under another"
    );
    Racks {
        window,
        video,
        upper,
        lower,
        turn,
    }
}

/// The rack that the clip has in its own drawer for `property` of its own.
fn the_rack_of(window: &Window, clip: ClipId, property: PropertyId) -> RackId {
    let target = AnimationTarget::Clip(clip);
    window
        .application()
        .project()
        .drawer_of(target)
        .unwrap()
        .first_rack_for(target, property)
        .unwrap()
        .id()
}

/// A point on the rack that is `index`th from the top on the screen, at frame `at` of the timeline (where there is no keyframe, in
/// these windows).
fn on_the_rack(window: &Window, index: usize, at: u64) -> Pos2 {
    Pos2::new(
        window.x_of_frame_on_the_strip(0, at),
        racks_drawn(window)[index].center().y,
    )
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

fn select_racks(window: &mut Window, racks: &[RackId]) {
    window.do_this(Command::SelectItems {
        items: racks.iter().copied().map(SelectionItem::Rack).collect(),
        change: SelectionChange::Replace,
    });
    window.harness.run_steps(5);
}

fn is_offered(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn is_enabled(window: &Window, label: &str) -> bool {
    !window.harness.get_by_label(label).accesskit_node().is_disabled()
}

/// What is chosen from the menu that is open.
fn choose(window: &mut Window, label: &str) {
    window.harness.get_by_label(label).click();
    window.settle();
}

/// How many outlines of a selected thing are drawn round exactly the rectangles `racks`.
fn outlines_drawn_round(window: &Window, racks: &[Rect]) -> usize {
    window
        .shapes_drawn()
        .into_iter()
        .filter(|shape| {
            matches!(shape, Shape::Rect(rectangle)
                if rectangle.stroke.color == theme::SELECTION_OUTLINE && racks.contains(&rectangle.rect))
        })
        .count()
}

// --- selecting racks ---

#[test]
fn clicking_a_rack_selects_it_and_clicking_another_selects_only_that_one() {
    let Racks {
        mut window,
        upper,
        lower,
        ..
    } = window_with_racks_of_the_video();

    window.click(on_the_rack(&window, 0, 160));
    assert_eq!(selected_racks(&window), vec![upper]);
    window.click(on_the_rack(&window, 1, 160));
    assert_eq!(selected_racks(&window), vec![lower]);
    assert!(!window.application().has_unsaved_changes(), "selecting is not an edit");
}

#[test]
fn clicking_a_rack_with_control_held_adds_it_to_what_is_selected_and_lets_go_of_it_if_it_was_selected() {
    let Racks {
        mut window,
        upper,
        lower,
        turn,
        ..
    } = window_with_racks_of_the_video();

    window.click(on_the_rack(&window, 0, 160));
    window.click_with_control(on_the_rack(&window, 2, 160));
    assert_eq!(selected_racks(&window), vec![upper, turn]);
    window.click_with_control(on_the_rack(&window, 1, 160));
    assert_eq!(selected_racks(&window), vec![upper, lower, turn]);
    window.click_with_control(on_the_rack(&window, 0, 160));
    assert_eq!(selected_racks(&window), vec![lower, turn], "the first was let go of");
}

#[test]
fn a_rack_is_selected_at_the_same_time_as_clips_and_keyframes_when_control_is_held() {
    let Racks {
        mut window,
        video,
        upper,
        ..
    } = window_with_racks_of_the_video();
    let keyframe = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Rotation, 150);
    window.click(window.middle_of_the_strip_at(1, 20));
    window.click_with_control(where_it_is(&window, keyframe));
    window.click_with_control(on_the_rack(&window, 0, 160));

    let items = window.application().selected_items();

    assert!(items.contains(&SelectionItem::Clip(video)), "{items:?}");
    assert!(items.contains(&SelectionItem::Keyframe(keyframe)), "{items:?}");
    assert!(items.contains(&SelectionItem::Rack(upper)), "{items:?}");
    assert_eq!(items.len(), 3);
}

#[test]
fn a_selected_rack_has_the_outline_of_a_selection_and_one_that_is_not_has_none() {
    let Racks {
        mut window,
        upper,
        lower,
        ..
    } = window_with_racks_of_the_video();
    let rectangles = racks_drawn(&window);
    assert_eq!(outlines_drawn_round(&window, &rectangles), 0, "nothing is selected");

    select_racks(&mut window, &[upper]);
    assert_eq!(outlines_drawn_round(&window, &rectangles[..1]), 1, "the first");
    assert_eq!(outlines_drawn_round(&window, &rectangles[1..]), 0, "and not the others");

    select_racks(&mut window, &[upper, lower]);
    assert_eq!(outlines_drawn_round(&window, &rectangles[..2]), 2);
    assert_eq!(outlines_drawn_round(&window, &rectangles[2..]), 0);
}

#[test]
fn right_clicking_a_rack_selects_it_unless_it_is_one_of_those_selected() {
    let Racks {
        mut window,
        upper,
        lower,
        ..
    } = window_with_racks_of_the_video();

    window.right_click(on_the_rack(&window, 1, 160));
    assert_eq!(selected_racks(&window), vec![lower]);

    // (A new window: the menu that is open is in the way of the next right-click.)
    let Racks {
        mut window,
        upper: first,
        lower: second,
        ..
    } = window_with_racks_of_the_video();
    assert_eq!((first, second), (upper, lower), "the same racks, by the same names");
    select_racks(&mut window, &[first, second]);
    window.right_click(on_the_rack(&window, 1, 160));
    assert_eq!(
        selected_racks(&window),
        vec![first, second],
        "one of several selected: they stay selected"
    );
}

#[test]
fn taking_hold_of_a_rack_that_is_not_selected_selects_it_like_taking_hold_of_a_clip_does() {
    let Racks {
        mut window,
        upper,
        turn,
        ..
    } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper]);

    window.drag(
        on_the_rack(&window, 2, 160),
        on_the_rack(&window, 2, 170),
        PointerButton::Primary,
    );

    assert_eq!(
        selected_racks(&window),
        vec![turn],
        "only the one that was taken hold of"
    );
}

#[test]
fn a_rectangle_drawn_on_an_empty_place_lets_go_of_the_racks() {
    let Racks { mut window, upper, .. } = window_with_racks_of_the_video();
    select_racks(&mut window, &[upper]);
    // (Where the text is not, and where the timeline can be seen: the strips show a little over 170 frames.)
    let empty = window.middle_of_the_strip_at(0, 130);

    window.drag(empty, empty + egui::vec2(40.0, 6.0), PointerButton::Primary);

    assert_eq!(selected_racks(&window), Vec::<RackId>::new());
}

// --- the menu of a rack ---

#[test]
fn the_menu_of_a_rack_that_is_alone_offers_merging_that_cannot_be_used_and_taking_it_away_that_can() {
    let Racks { mut window, upper, .. } = window_with_racks_of_the_video();
    let before = window.application().project().clone();

    window.right_click(on_the_rack(&window, 0, 160));

    assert!(is_offered(&window, MERGE) && is_offered(&window, DELETE_RACK));
    assert!(!is_enabled(&window, MERGE), "there is only one rack");
    assert!(is_enabled(&window, DELETE_RACK));
    assert_eq!(
        window.application().project(),
        &before,
        "opening a menu changes nothing"
    );
    assert_eq!(selected_racks(&window), vec![upper]);
}

#[test]
fn the_menu_of_two_selected_racks_of_one_property_merges_them_and_it_is_one_edit_of_the_history() {
    let Racks {
        mut window,
        video,
        upper,
        lower,
        ..
    } = window_with_racks_of_the_video();
    window.click(on_the_rack(&window, 0, 160));
    window.click_with_control(on_the_rack(&window, 1, 160));

    window.right_click(on_the_rack(&window, 1, 160));
    assert!(is_enabled(&window, MERGE));
    assert_eq!(
        selected_racks(&window),
        vec![upper, lower],
        "right-clicking one of them leaves both selected"
    );
    choose(&mut window, MERGE);

    let project = window.application().project();
    assert!(project.located_rack(lower).is_none(), "the one that was under is gone");
    let frames: Vec<u64> = project
        .located_rack(upper)
        .unwrap()
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(frames, vec![100, 120, 200]);
    assert_eq!(
        project
            .keyframes_of(AnimationTarget::Clip(video), PropertyId::Gain)
            .len(),
        3,
        "the loudness has all its keyframes"
    );
    assert_eq!(
        selected_racks(&window),
        vec![upper],
        "the rack that is left is still selected"
    );
    window.do_this(Command::Undo);
    assert!(
        window.application().project().located_rack(lower).is_some(),
        "one undo takes the merge back"
    );
}

#[test]
fn the_menu_of_racks_of_different_properties_says_that_they_cannot_be_merged() {
    let Racks { mut window, .. } = window_with_racks_of_the_video();
    window.click(on_the_rack(&window, 0, 160));
    window.click_with_control(on_the_rack(&window, 2, 160));

    window.right_click(on_the_rack(&window, 2, 160));

    assert!(!is_enabled(&window, MERGE));
    assert!(is_enabled(&window, "Delete 2 racks"), "the item says how many");
}

#[test]
fn taking_racks_away_from_the_menu_takes_their_keyframes_with_them() {
    let Racks {
        mut window,
        video,
        upper,
        lower,
        turn,
    } = window_with_racks_of_the_video();
    window.click(on_the_rack(&window, 0, 160));
    window.click_with_control(on_the_rack(&window, 1, 160));

    window.right_click(on_the_rack(&window, 0, 160));
    choose(&mut window, "Delete 2 racks");

    let project = window.application().project();
    assert!(project.located_rack(upper).is_none() && project.located_rack(lower).is_none());
    assert!(
        project.located_rack(turn).is_some(),
        "the rack that was not selected is there"
    );
    assert!(!project.is_animated(AnimationTarget::Clip(video), PropertyId::Gain));
    assert!(selected_racks(&window).is_empty());
}

// --- the menu of a keyframe ---

#[test]
fn right_clicking_a_keyframe_offers_a_copy_on_either_side_and_taking_it_away_and_selects_it() {
    let Racks { mut window, video, .. } = window_with_racks_of_the_video();
    let keyframe = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Rotation, 150);
    let before = window.application().project().clone();

    window.right_click(where_it_is(&window, keyframe));

    for label in [DUPLICATE_RIGHT, DUPLICATE_LEFT, DELETE_KEYFRAME] {
        assert!(is_offered(&window, label), "{label}");
        assert!(is_enabled(&window, label), "{label}");
    }
    assert_eq!(
        window.application().project(),
        &before,
        "opening a menu changes nothing"
    );
    assert_eq!(selected_keyframes(&window), vec![keyframe]);
}

#[test]
fn a_copy_of_a_keyframe_made_from_the_menu_is_as_far_from_it_as_a_keyframe_is_wide_and_is_selected() {
    let Racks {
        mut window,
        video,
        turn,
        ..
    } = window_with_racks_of_the_video();
    let keyframe = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Rotation, 150);
    let frames_apart = (12.0 / points_per_frame(&window)).ceil() as u64;
    window.right_click(where_it_is(&window, keyframe));

    choose(&mut window, DUPLICATE_RIGHT);

    let frames: Vec<u64> = window
        .application()
        .project()
        .located_rack(turn)
        .unwrap()
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(frames, vec![150, 150 + frames_apart.max(1)]);
    let selected = selected_keyframes(&window);
    assert_eq!(selected.len(), 1);
    assert_ne!(selected[0], keyframe, "the copy is what is selected");
    assert_eq!(frame_of(&window, selected[0]), 150 + frames_apart.max(1));

    // Copying the copy goes on from it, to the right again.
    window.right_click(where_it_is(&window, selected[0]));
    choose(&mut window, DUPLICATE_RIGHT);
    assert_eq!(window.application().project().located_rack(turn).unwrap().rack.len(), 3);
}

#[test]
fn the_menu_of_selected_keyframes_copies_them_all_to_the_left_as_a_group() {
    let Racks {
        mut window,
        video,
        upper,
        ..
    } = window_with_racks_of_the_video();
    let first = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    let second = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 120);
    window.click(where_it_is(&window, first));
    window.click_with_control(where_it_is(&window, second));
    let frames_apart = (12.0 / points_per_frame(&window)).ceil().max(1.0) as u64;

    window.right_click(where_it_is(&window, second));
    assert!(is_offered(&window, "Duplicate 2 keyframes to the left"));
    choose(&mut window, "Duplicate 2 keyframes to the left");

    let frames: Vec<u64> = window
        .application()
        .project()
        .located_rack(upper)
        .unwrap()
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    // 20 frames apart, and the copies are that and the gap before the first.
    let distance = 20 + frames_apart;
    assert_eq!(frames, vec![100 - distance, 120 - distance, 100, 120]);
    assert_eq!(selected_keyframes(&window).len(), 2, "the two copies are selected");
}

#[test]
fn the_item_that_would_put_the_copies_where_the_drawer_has_no_frames_cannot_be_used_and_the_other_can() {
    let mut window = Window::showing(project_of(100));
    let target = AnimationTarget::Clip(window.clip(0, 0));
    let keyframe = make_a_keyframe(&mut window, target, PropertyId::Gain, 99);
    window.application_mut().note_everything_is_saved();

    window.right_click(where_it_is(&window, keyframe));

    assert!(
        !is_enabled(&window, DUPLICATE_RIGHT),
        "the video has 100 frames and the copy would be at 100 or more"
    );
    assert!(is_enabled(&window, DUPLICATE_LEFT));
    window.harness.get_by_label(DUPLICATE_RIGHT).click();
    window.settle();
    assert!(
        !window.application().has_unsaved_changes(),
        "choosing what cannot be used does nothing"
    );
}

#[test]
fn taking_a_keyframe_away_from_the_menu_takes_the_selected_keyframes_and_only_them() {
    let Racks { mut window, video, .. } = window_with_racks_of_the_video();
    let first = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 100);
    let second = keyframe_at(&window, AnimationTarget::Clip(video), PropertyId::Gain, 120);
    window.click(where_it_is(&window, first));
    window.click_with_control(where_it_is(&window, second));

    window.right_click(where_it_is(&window, first));
    choose(&mut window, "Delete 2 keyframes");

    let project = window.application().project();
    assert!(project.locate_keyframe(first).is_none() && project.locate_keyframe(second).is_none());
    assert!(
        project.is_animated(AnimationTarget::Clip(video), PropertyId::Gain),
        "the keyframe at 200 is still there"
    );
}

// --- dragging a keyframe to another rack ---

/// The keyframe of the loudness of the video at `at` frames, which is in the upper rack at 100 and 120, and the lower at 200.
fn the_keyframe_at(racks: &Racks, at: u64) -> KeyframeId {
    keyframe_at(&racks.window, AnimationTarget::Clip(racks.video), PropertyId::Gain, at)
}

fn rack_of(window: &Window, keyframe: KeyframeId) -> RackId {
    window
        .application()
        .project()
        .locate_keyframe(keyframe)
        .unwrap()
        .rack
        .rack
        .id()
}

#[test]
fn dragging_a_keyframe_down_to_another_rack_of_the_same_property_puts_it_there_at_the_frame_it_was_at() {
    let mut racks = window_with_racks_of_the_video();
    let keyframe = the_keyframe_at(&racks, 120);
    let from = where_it_is(&racks.window, keyframe);
    // The lower rack is under the upper rack: straight down from the keyframe.
    let to = Pos2::new(from.x, racks_drawn(&racks.window)[1].center().y);

    racks.window.drag(from, to, PointerButton::Primary);

    assert_eq!(
        rack_of(&racks.window, keyframe),
        racks.lower,
        "it is in the rack under the one it was in"
    );
    assert_eq!(
        frame_of(&racks.window, keyframe),
        120,
        "at the frame it was at: the pointer went straight down"
    );
    assert_eq!(selected_keyframes(&racks.window), vec![keyframe]);
    assert_eq!(
        racks
            .window
            .application()
            .project()
            .keyframes_of(AnimationTarget::Clip(racks.video), PropertyId::Gain)
            .len(),
        3,
        "the loudness has the keyframes it had"
    );
}

#[test]
fn a_drag_of_a_keyframe_from_a_rack_to_another_is_one_step_of_the_history_and_the_keyframe_has_the_name_it_had() {
    let mut racks = window_with_racks_of_the_video();
    let keyframe = the_keyframe_at(&racks, 120);
    let upper = racks.upper;
    racks.window.application_mut().note_everything_is_saved();
    let from = where_it_is(&racks.window, keyframe);
    let to = Pos2::new(from.x + 30.0, racks_drawn(&racks.window)[1].center().y);

    racks.window.drag(from, to, PointerButton::Primary);

    assert_eq!(rack_of(&racks.window, keyframe), racks.lower);
    assert!(racks.window.application().has_unsaved_changes());
    racks.window.do_this(Command::Undo);
    assert_eq!(
        rack_of(&racks.window, keyframe),
        upper,
        "one undo puts it back in the first rack"
    );
    assert_eq!(frame_of(&racks.window, keyframe), 120);
}

#[test]
fn a_keyframe_dragged_over_racks_of_another_property_stays_in_its_own_rack() {
    let mut racks = window_with_racks_of_the_video();
    // The keyframe of how far the video is turned, which is in the third rack, goes up over the two racks of its loudness.
    let keyframe = keyframe_at(
        &racks.window,
        AnimationTarget::Clip(racks.video),
        PropertyId::Rotation,
        150,
    );
    let from = where_it_is(&racks.window, keyframe);
    let to = Pos2::new(from.x, racks_drawn(&racks.window)[0].center().y);

    racks.window.drag(from, to, PointerButton::Primary);

    assert_eq!(rack_of(&racks.window, keyframe), racks.turn);
    assert_eq!(frame_of(&racks.window, keyframe), 150);
    assert_eq!(
        racks
            .window
            .application()
            .project()
            .located_rack(racks.upper)
            .unwrap()
            .rack
            .len(),
        2,
        "and the racks it was over are as they were"
    );
}

#[test]
fn a_keyframe_dragged_down_past_a_rack_of_another_property_goes_on_to_the_rack_of_its_own_beyond_it() {
    let mut racks = window_with_racks_of_the_video();
    // The keyframe at 100, in the upper rack of the loudness, is taken down past the lower rack and the rack of the turn: the
    // lower one is the last that it can be in.
    let keyframe = the_keyframe_at(&racks, 100);
    let from = where_it_is(&racks.window, keyframe);
    let to = Pos2::new(from.x, racks_drawn(&racks.window)[2].center().y);

    racks.window.drag(from, to, PointerButton::Primary);

    assert_eq!(
        rack_of(&racks.window, keyframe),
        racks.lower,
        "it went through the lower rack and stayed in it"
    );
    assert_eq!(frame_of(&racks.window, keyframe), 100);
}

#[test]
fn a_keyframe_dragged_to_a_rack_goes_along_it_only_as_far_as_the_keyframe_that_is_there_lets_it() {
    let mut racks = window_with_racks_of_the_video();
    // The lower rack gets a keyframe at 120, which is where the keyframe at 100 would be put by the pointer.
    let in_the_way = the_keyframe_at(&racks, 200);
    let lower = racks.lower;
    racks
        .window
        .do_this(Command::MoveKeyframes(vec![crate::project::KeyframeMove {
            keyframe: in_the_way,
            to_rack: lower,
            to_frame: 120,
        }]));
    racks.window.harness.run_steps(5);
    let keyframe = the_keyframe_at(&racks, 100);
    let from = where_it_is(&racks.window, keyframe);
    let to = Pos2::new(
        from.x + 20.0 * points_per_frame(&racks.window),
        racks_drawn(&racks.window)[1].center().y,
    );

    racks.window.drag(from, to, PointerButton::Primary);

    // The pointer is at frame 120 over the lower rack, where a keyframe is. The keyframe goes down to the lower rack where it was
    // (100 is free there), and goes along it as far as the one that is there lets it: it stops one frame short.
    assert_eq!(rack_of(&racks.window, keyframe), racks.lower);
    assert_eq!(frame_of(&racks.window, keyframe), 119);
}

#[test]
fn the_selected_keyframes_of_one_rack_go_together_to_the_rack_under_it() {
    let mut racks = window_with_racks_of_the_video();
    let (first, second) = (the_keyframe_at(&racks, 100), the_keyframe_at(&racks, 120));
    racks.window.click(where_it_is(&racks.window, first));
    racks.window.click_with_control(where_it_is(&racks.window, second));
    let from = where_it_is(&racks.window, second);
    let to = Pos2::new(from.x, racks_drawn(&racks.window)[1].center().y);

    racks.window.drag(from, to, PointerButton::Primary);

    assert_eq!(rack_of(&racks.window, first), racks.lower);
    assert_eq!(rack_of(&racks.window, second), racks.lower);
    assert_eq!(
        (frame_of(&racks.window, first), frame_of(&racks.window, second)),
        (100, 120),
        "they keep their places in time"
    );
    assert!(
        racks.window.application().project().located_rack(racks.upper).is_none(),
        "the rack they left has no keyframe, and has gone"
    );
}

#[test]
fn a_keyframe_that_is_dragged_down_and_along_goes_to_the_other_rack_at_the_frame_it_started_at_when_the_pointer_is_near_it()
 {
    let mut racks = window_with_racks_of_the_video();
    let keyframe = the_keyframe_at(&racks, 120);
    let from = where_it_is(&racks.window, keyframe);
    let points = points_per_frame(&racks.window);
    // Down to the lower rack and 3 frames to the right: near the frame it was at, so it is held there.
    let near = Pos2::new(from.x + 3.0 * points, racks_drawn(&racks.window)[1].center().y);
    racks.window.drag(from, near, PointerButton::Primary);
    assert_eq!(rack_of(&racks.window, keyframe), racks.lower);
    assert_eq!(frame_of(&racks.window, keyframe), 120, "held to where it started");

    // And 40 frames to the right is far from it: it goes where the pointer takes it.
    let mut racks = window_with_racks_of_the_video();
    let keyframe = the_keyframe_at(&racks, 120);
    let from = where_it_is(&racks.window, keyframe);
    let far = Pos2::new(from.x + 40.0 * points, racks_drawn(&racks.window)[1].center().y);
    racks.window.drag(from, far, PointerButton::Primary);
    assert_eq!(rack_of(&racks.window, keyframe), racks.lower);
    let frame = frame_of(&racks.window, keyframe);
    assert!((158..=161).contains(&frame), "40 frames on, to within a frame: {frame}");
}
