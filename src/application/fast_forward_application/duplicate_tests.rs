// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the menus that put a copy of a clip right beside it, or a copy of a track in front of it or behind it (and delete
//! what they are about): what they offer, what choosing does, to what, and that each is one step of the history.

use egui::Pos2;
use egui_kittest::kittest::{NodeT, Queryable};

use super::keyframe_drag_tests::{make_a_keyframe, window_with_a_video_and_a_text};
use super::test_window::{Window, project_cut_into_three, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{
    AnimationTarget, ClipId, ClipSide, Project, PropertyId, RackDestination, TextObject, TimelineFrameIndex, TrackId,
    TrackSide,
};

const DUPLICATE_RIGHT: &str = "Duplicate to the right";
const DUPLICATE_LEFT: &str = "Duplicate to the left";
const DELETE_CLIP: &str = "Delete clip";
const DUPLICATE_ABOVE: &str = "Duplicate above";
const DUPLICATE_BELOW: &str = "Duplicate below";
const DELETE_TRACK: &str = "Delete track";

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

/// A window with a video of 30 frames at the start of the track behind and, in front of it, a text object from frame 70 to
/// frame 100, with nothing unsaved. The video has room only at its right (it is at the start of the timeline); the text has
/// room at both of its sides (a gap of 40 frames between it and the video, which is more than the 30 frames of a copy).
fn window_with_a_video_and_a_text_with_room_beside_it() -> (Window, ClipId, ClipId) {
    let mut project: Project = project_of(30);
    let video = project.tracks()[0].entries()[0].id();
    let in_front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(in_front, frame(70), TextObject::default(), 30)
        .expect("there is room");
    let mut window = Window::showing(project);
    window.application_mut().note_everything_is_saved();
    (window, video, text)
}

/// Where the text is on the screen (a point on it), in the window of [`window_with_a_video_and_a_text_with_room_beside_it`].
fn on_the_text(window: &Window) -> Pos2 {
    window.middle_of_the_strip_at(0, 85)
}

/// Where the video is on the screen (a point on it), in the same window.
fn on_the_video(window: &Window) -> Pos2 {
    window.middle_of_the_strip_at(1, 15)
}

/// What is chosen from the menu that is open.
fn choose(window: &mut Window, label: &str) {
    window.harness.get_by_label(label).click();
    window.settle();
}

fn is_offered(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn is_enabled(window: &Window, label: &str) -> bool {
    !window.harness.get_by_label(label).accesskit_node().is_disabled()
}

/// The first frame and the frame after the last one of each clip of the track at `position`, in the order of time.
fn spans_of_the_clips_on(window: &Window, position: usize) -> Vec<(u64, u64)> {
    window.application().project().tracks()[position]
        .placements()
        .map(|placement| {
            (
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value(),
            )
        })
        .collect()
}

fn select(window: &mut Window, clips: &[ClipId]) {
    window.do_this(Command::select_clips(clips.to_vec(), SelectionChange::Replace));
}

fn text_of(window: &Window, clip: ClipId) -> TextObject {
    window
        .application()
        .project()
        .placement_of(clip)
        .and_then(|placement| placement.clip.as_text().map(|text| text.object.clone()))
        .expect("a text clip")
}

fn frames_of_the_keyframes_in_the_drawer_of(window: &Window, owner: AnimationTarget) -> Vec<Vec<u64>> {
    window
        .application()
        .project()
        .drawer_of(owner)
        .expect("the owner is there")
        .racks()
        .iter()
        .map(|rack| rack.keyframes().iter().map(|keyframe| keyframe.frame).collect())
        .collect()
}

fn assert_the_racks_are_sound(window: &Window) {
    assert_eq!(
        window.application().project().problems_with_the_racks(),
        Vec::<String>::new()
    );
}

// --- what the menu of a clip offers ---

#[test]
fn right_clicking_a_clip_offers_a_copy_on_either_side_and_deleting_it_and_changes_nothing_by_itself() {
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let before = window.application().project().clone();

    let point = on_the_text(&window);
    window.right_click(point);

    for label in [DUPLICATE_RIGHT, DUPLICATE_LEFT, DELETE_CLIP] {
        assert!(is_offered(&window, label), "{label}");
        assert!(
            is_enabled(&window, label),
            "{label} can be used: there is room at both sides"
        );
    }
    assert!(
        !is_offered(&window, "New"),
        "it is the menu of a clip, not of an empty place"
    );
    assert_eq!(
        window.application().project(),
        &before,
        "opening a menu changes nothing"
    );
    assert!(!window.application().has_unsaved_changes());
    assert_eq!(
        window.application().selected_clips(),
        vec![text],
        "right-clicking selected it"
    );
}

#[test]
fn an_item_that_would_put_the_copy_where_there_is_no_room_cannot_be_used_and_the_other_can() {
    let (mut window, _, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let point = on_the_video(&window);
    window.right_click(point);
    assert!(is_enabled(&window, DUPLICATE_RIGHT));
    assert!(
        !is_enabled(&window, DUPLICATE_LEFT),
        "the video begins where the timeline does: nothing can be to its left"
    );
    assert!(is_enabled(&window, DELETE_CLIP));

    window.harness.get_by_label(DUPLICATE_LEFT).click();
    window.settle();
    assert_eq!(
        spans_of_the_clips_on(&window, 1),
        vec![(0, 30)],
        "choosing what cannot be used does nothing"
    );
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn a_clip_that_touches_the_clips_on_both_sides_of_it_has_no_room_for_a_copy_on_either() {
    let mut window = Window::showing(project_cut_into_three(90));
    let middle = window.middle_of_the_track_at(45);
    window.right_click(middle);
    assert!(!is_enabled(&window, DUPLICATE_RIGHT));
    assert!(!is_enabled(&window, DUPLICATE_LEFT));
    assert!(is_enabled(&window, DELETE_CLIP));
}

#[test]
fn a_clip_that_has_a_clip_touching_it_has_room_only_on_its_other_side() {
    let mut window = Window::showing(project_cut_into_three(90));
    let first = window.middle_of_the_track_at(15);
    window.right_click(first);
    assert!(!is_enabled(&window, DUPLICATE_RIGHT), "the second clip is right there");
    assert!(!is_enabled(&window, DUPLICATE_LEFT), "and the timeline begins");

    let last = window.middle_of_the_track_at(75);
    window.right_click(last);
    assert!(
        is_enabled(&window, DUPLICATE_RIGHT),
        "after the last clip there is nothing"
    );
    assert!(!is_enabled(&window, DUPLICATE_LEFT), "the second clip is right there");
}

// --- copying one clip ---

#[test]
fn duplicating_to_the_right_puts_a_copy_right_after_the_clip_and_selects_the_copy() {
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let point = on_the_text(&window);
    window.right_click(point);
    choose(&mut window, DUPLICATE_RIGHT);

    assert_eq!(spans_of_the_clips_on(&window, 0), vec![(70, 100), (100, 130)]);
    let copy = window.clip(0, 1);
    assert_ne!(copy, text, "a clip of its own");
    assert_eq!(
        window
            .application()
            .project()
            .placement_of(text)
            .unwrap()
            .first_timeline_frame,
        frame(70),
        "the clip is where it was and has the name it had"
    );
    assert_eq!(
        text_of(&window, copy),
        text_of(&window, text),
        "all that the clip says, the copy says"
    );
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(copy)]);
    assert!(window.application().has_unsaved_changes());
    assert!(!is_offered(&window, DELETE_CLIP), "the menu closed");
}

#[test]
fn duplicating_to_the_left_puts_a_copy_that_ends_where_the_clip_begins_and_selects_the_copy() {
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let point = on_the_text(&window);
    window.right_click(point);
    choose(&mut window, DUPLICATE_LEFT);

    assert_eq!(spans_of_the_clips_on(&window, 0), vec![(40, 70), (70, 100)]);
    let copy = window.clip(0, 0);
    assert_ne!(copy, text);
    assert_eq!(
        window
            .application()
            .project()
            .placement_of(text)
            .unwrap()
            .first_timeline_frame,
        frame(70),
        "the clip is where it was"
    );
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(copy)]);
}

#[test]
fn a_copy_of_a_video_is_the_same_part_of_the_same_file_and_does_not_change_the_original() {
    let (mut window, video, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let original_before = window.application().project().placement_of(video).unwrap().clip.clone();
    let point = on_the_video(&window);
    window.right_click(point);
    choose(&mut window, DUPLICATE_RIGHT);

    assert_eq!(spans_of_the_clips_on(&window, 1), vec![(0, 30), (30, 60)]);
    let copy = window.clip(1, 1);
    let project = window.application().project();
    assert_eq!(project.placement_of(video).unwrap().clip.clone(), original_before);
    assert_eq!(
        project.placement_of(copy).unwrap().clip.clone(),
        original_before,
        "the same part of the same file, at the same speed and loudness"
    );
}

#[test]
fn a_copy_at_the_end_of_the_project_makes_the_project_longer_and_leaves_the_playhead_alone() {
    let (mut window, _, _) = window_with_a_video_and_a_text_with_room_beside_it();
    assert_eq!(window.application().project().total_frame_count(), 100);
    window.do_this(Command::SeekTo(frame(12)));
    let point = on_the_text(&window);
    window.right_click(point);
    choose(&mut window, DUPLICATE_RIGHT);
    assert_eq!(window.application().project().total_frame_count(), 130);
    assert_eq!(window.current_frame(), 12);
}

#[test]
fn a_copy_has_a_copy_of_the_keyframes_in_the_drawer_of_the_clip_which_are_of_the_copy() {
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 85);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 95);
    let rack_before = window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(text))
        .unwrap()
        .racks()[0]
        .id();
    let point = on_the_text(&window);
    window.right_click(point);
    choose(&mut window, DUPLICATE_RIGHT);

    let copy = window.clip(0, 1);
    let project = window.application().project();
    let copy_rack = &project.drawer_of(AnimationTarget::Clip(copy)).unwrap().racks()[0];
    assert_eq!(copy_rack.target(), AnimationTarget::Clip(copy), "it animates the copy");
    assert_eq!(copy_rack.property(), PropertyId::Rotation);
    assert_ne!(copy_rack.id(), rack_before, "a rack of its own");
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(copy)),
        vec![vec![15, 25]],
        "at the same places in the clip"
    );
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(text)),
        vec![vec![15, 25]],
        "and the clip has the keyframes it had"
    );
    assert_eq!(
        project.drawer_of(AnimationTarget::Clip(text)).unwrap().racks()[0].id(),
        rack_before
    );
    assert_eq!(
        project
            .keyframes_of(AnimationTarget::Clip(copy), PropertyId::Rotation)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect::<Vec<_>>(),
        vec![115, 125],
        "in time, they are where the copy is"
    );
    assert_the_racks_are_sound(&window);
}

#[test]
fn a_copy_of_a_clip_that_has_the_keyframes_of_another_clip_in_its_drawer_does_to_that_clip_what_the_clip_did() {
    // The purpose of drawers: the loudness of a big video goes down when a small clip is there, and a copy of the small clip
    // lowers it again where the copy is.
    let (mut window, video, text) = window_with_a_video_and_a_text();
    for at in [50, 70] {
        make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, at);
    }
    let rack = window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(video))
        .unwrap()
        .first_rack_for(AnimationTarget::Clip(video), PropertyId::Gain)
        .unwrap()
        .id();
    window.do_this(Command::MoveRacks {
        racks: vec![rack],
        to: RackDestination::Drawer(AnimationTarget::Clip(text)),
        shift_in_frames: 0,
        slot: None,
    });
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(text)),
        vec![vec![10, 30]]
    );

    let point = window.middle_of_the_strip_at(0, 70);
    window.right_click(point);
    choose(&mut window, DUPLICATE_RIGHT);

    let copy = window.clip(0, 1);
    let project = window.application().project();
    assert_eq!(project.placement_of(copy).unwrap().first_timeline_frame, frame(100));
    let racks_of_the_copy = project.drawer_of(AnimationTarget::Clip(copy)).unwrap().racks();
    assert_eq!(racks_of_the_copy.len(), 1);
    assert_eq!(
        racks_of_the_copy[0].target(),
        AnimationTarget::Clip(video),
        "it still lowers the video, not the copy"
    );
    assert_eq!(
        project
            .keyframes_of(AnimationTarget::Clip(video), PropertyId::Gain)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect::<Vec<_>>(),
        vec![50, 70, 110, 130],
        "the video has the keyframes of the clip and now those of the copy"
    );
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(text)),
        vec![vec![10, 30]],
        "the clip is as it was"
    );
    assert_the_racks_are_sound(&window);
}

#[test]
fn the_menu_of_a_keyframe_crate_copies_it_with_the_racks_in_it() {
    let (mut window, video, _) = window_with_a_video_and_a_text();
    make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 110);
    make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 150);
    let rack = window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(video))
        .unwrap()
        .racks()[0]
        .id();
    let front = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveRacks {
        racks: vec![rack],
        to: RackDestination::NewCrate {
            track: front,
            from_frame: 100,
            until_frame: 300,
        },
        shift_in_frames: 0,
        slot: None,
    });
    // (It lasts from the first keyframe to the last, and not through the empty stretch it was made in.)
    assert_eq!(spans_of_the_clips_on(&window, 0), vec![(40, 100), (110, 151)]);

    // Near the top of the crate, where there is nothing but the crate: its racks are in the middle of it.
    let strip = window.strip_of_track(0);
    let on_the_crate = Pos2::new(window.x_of_frame_on_the_strip(0, 130), strip.top() + 14.0);
    window.right_click(on_the_crate);
    choose(&mut window, DUPLICATE_RIGHT);

    assert_eq!(
        spans_of_the_clips_on(&window, 0),
        vec![(40, 100), (110, 151), (151, 192)]
    );
    let (original, copy) = (window.clip(0, 1), window.clip(0, 2));
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(copy)),
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(original)),
        "the copy has the racks of the crate"
    );
    assert_eq!(
        window
            .application()
            .project()
            .keyframes_of(AnimationTarget::Clip(video), PropertyId::Gain)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect::<Vec<_>>(),
        vec![110, 150, 151, 191],
        "and what it does to the video, it does where the copy is"
    );
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(copy)]);
    assert_the_racks_are_sound(&window);
}

#[test]
fn a_copy_is_one_step_of_the_history_which_takes_back_the_copy_and_what_was_selected() {
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let point = on_the_text(&window);
    window.right_click(point);
    let before = window.application().project().clone();
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(text)]);
    choose(&mut window, DUPLICATE_RIGHT);
    let copy = window.clip(0, 1);

    window.do_this(Command::Undo);
    assert_eq!(window.application().project(), &before, "one undo takes the copy away");
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(text)],
        "and what was selected is selected"
    );
    window.do_this(Command::Redo);
    assert_eq!(spans_of_the_clips_on(&window, 0), vec![(70, 100), (100, 130)]);
    assert_eq!(window.clip(0, 1), copy, "the same copy");
}

// --- what is done to several clips ---

#[test]
fn a_clip_that_is_one_of_several_selected_has_a_menu_about_all_of_them_and_the_menu_says_how_many() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    select(&mut window, &[video, text]);
    let point = on_the_text(&window);
    window.right_click(point);

    assert_eq!(
        window.application().selected_clips().len(),
        2,
        "right-clicking one leaves them selected"
    );
    assert!(is_offered(&window, "Duplicate 2 clips to the right"));
    assert!(is_offered(&window, "Duplicate 2 clips to the left"));
    assert!(is_offered(&window, "Delete 2 clips"));
    assert!(!is_offered(&window, DUPLICATE_RIGHT), "the menu is not about one clip");
    assert!(!is_offered(&window, DELETE_CLIP));
}

#[test]
fn duplicating_several_selected_clips_copies_each_beside_itself_and_selects_the_copies() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    select(&mut window, &[video, text]);
    let point = on_the_text(&window);
    window.right_click(point);
    choose(&mut window, "Duplicate 2 clips to the right");

    assert_eq!(spans_of_the_clips_on(&window, 0), vec![(70, 100), (100, 130)]);
    assert_eq!(spans_of_the_clips_on(&window, 1), vec![(0, 30), (30, 60)]);
    let mut copies = vec![window.clip(0, 1), window.clip(1, 1)];
    copies.sort();
    let mut selected = window.application().selected_clips();
    selected.sort();
    assert_eq!(selected, copies, "the copies, and nothing else, are selected");
    assert!(
        window
            .application()
            .selected_items()
            .iter()
            .all(|item| matches!(item, SelectionItem::Clip(_)))
    );
}

#[test]
fn when_only_some_of_the_selected_clips_have_room_the_item_can_be_used_and_copies_those() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    select(&mut window, &[video, text]);
    let point = on_the_text(&window);
    window.right_click(point);
    assert!(
        is_enabled(&window, "Duplicate 2 clips to the left"),
        "the text has room, though the video, at the start of the timeline, has none"
    );
    choose(&mut window, "Duplicate 2 clips to the left");

    assert_eq!(
        spans_of_the_clips_on(&window, 0),
        vec![(40, 70), (70, 100)],
        "the text was copied"
    );
    assert_eq!(spans_of_the_clips_on(&window, 1), vec![(0, 30)], "the video was not");
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(window.clip(0, 0))],
        "only the copy that was made is selected"
    );
}

#[test]
fn when_none_of_the_selected_clips_has_room_the_item_cannot_be_used() {
    let mut window = Window::showing(project_cut_into_three(90));
    let (first, middle) = (window.clip(0, 0), window.clip(0, 1));
    select(&mut window, &[first, middle]);
    let point = window.middle_of_the_track_at(15);
    window.right_click(point);
    assert!(is_offered(&window, "Duplicate 2 clips to the right"));
    assert!(
        !is_enabled(&window, "Duplicate 2 clips to the right"),
        "the third clip is where the copy of the second would be"
    );
    assert!(!is_enabled(&window, "Duplicate 2 clips to the left"));
    assert!(is_enabled(&window, "Delete 2 clips"));
}

#[test]
fn right_clicking_a_clip_that_is_not_selected_makes_the_menu_about_that_clip_alone() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    select(&mut window, &[video]);
    let point = on_the_text(&window);
    window.right_click(point);

    assert_eq!(
        window.application().selected_clips(),
        vec![text],
        "the clip that was clicked is all that is selected"
    );
    assert!(is_offered(&window, DUPLICATE_RIGHT));
    assert!(is_offered(&window, DELETE_CLIP));
    choose(&mut window, DELETE_CLIP);
    assert_eq!(
        spans_of_the_clips_on(&window, 0),
        Vec::<(u64, u64)>::new(),
        "the text is gone"
    );
    assert_eq!(
        spans_of_the_clips_on(&window, 1),
        vec![(0, 30)],
        "the video was not what the menu was about"
    );
}

// --- deleting from the menu ---

#[test]
fn the_menu_deletes_the_clip_or_all_the_selected_clips_and_it_is_one_step_of_the_history() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let before = window.application().project().clone();
    select(&mut window, &[video, text]);
    let point = on_the_text(&window);
    window.right_click(point);
    choose(&mut window, "Delete 2 clips");

    assert_eq!(spans_of_the_clips_on(&window, 0), Vec::<(u64, u64)>::new());
    assert_eq!(spans_of_the_clips_on(&window, 1), Vec::<(u64, u64)>::new());
    assert!(
        window.application().selected_items().is_empty(),
        "what is gone is not selected"
    );
    assert!(window.application().has_unsaved_changes());

    window.do_this(Command::Undo);
    assert_eq!(window.application().project(), &before, "one undo brings both back");
}

// --- when there is no room and the command is given anyway ---

#[test]
fn a_command_to_copy_a_clip_that_has_no_room_changes_nothing_and_tells_the_user() {
    let (mut window, video, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let before = window.application().project().clone();
    window.do_this(Command::DuplicateClips {
        clips: vec![video],
        side: ClipSide::Left,
    });

    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes(), "nothing was edited");
    match &window.application().dialog {
        Some(Dialog::Message { title, .. }) => assert_eq!(title, "There is no room for a copy there"),
        other => panic!("the user is told: {other:?}"),
    }
}

#[test]
fn a_command_to_copy_clips_of_which_only_some_have_room_copies_those_and_says_nothing_of_the_others() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    window.do_this(Command::DuplicateClips {
        clips: vec![video, text],
        side: ClipSide::Left,
    });

    // (The video is at the start of the timeline and has no room at its left; the text has.)
    assert_eq!(
        spans_of_the_clips_on(&window, 0),
        vec![(40, 70), (70, 100)],
        "the text has a copy before it"
    );
    assert_eq!(
        spans_of_the_clips_on(&window, 1),
        vec![(0, 30)],
        "and the video has none"
    );
    assert_eq!(
        window.application().selected_clips().len(),
        1,
        "the copy of the text is what is selected"
    );
    assert!(
        window.application().dialog.is_none(),
        "something was copied, and nothing is said about the clip that had no room"
    );
}

#[test]
fn a_command_to_copy_clips_that_are_not_there_does_nothing() {
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    window.do_this(Command::select_clips(vec![text], SelectionChange::Replace));
    let before = window.application().project().clone();
    window.do_this(Command::DuplicateClips {
        clips: vec![ClipId::new(TrackId::new(77), 3)],
        side: ClipSide::Right,
    });
    assert_eq!(window.application().project(), &before);
    assert_eq!(
        window.application().selected_clips(),
        vec![text],
        "what was selected stays selected"
    );
    assert!(
        window.application().dialog.is_none(),
        "a clip that is not there is not for want of room, and nothing is said"
    );
}

// --- the menu of the box of a track ---

fn right_click_the_box_of_track(window: &mut Window, position: usize) {
    let title_box = window.title_box_of_track(position);
    window.right_click(Pos2::new(title_box.left() - 4.0, title_box.center().y));
}

#[test]
fn right_clicking_the_box_of_a_track_offers_a_copy_above_it_and_below_it_and_deleting_it() {
    let (mut window, _, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let before = window.application().project().clone();
    right_click_the_box_of_track(&mut window, 0);

    for label in [DUPLICATE_ABOVE, DUPLICATE_BELOW, DELETE_TRACK] {
        assert!(is_offered(&window, label), "{label}");
        assert!(is_enabled(&window, label), "{label}");
    }
    assert_eq!(
        window.application().project(),
        &before,
        "opening the menu changes nothing"
    );
}

#[test]
fn a_project_with_only_one_track_can_have_a_copy_of_it_made_though_the_track_cannot_be_deleted() {
    let mut window = Window::showing(project_of(30));
    right_click_the_box_of_track(&mut window, 0);
    assert!(is_enabled(&window, DUPLICATE_ABOVE));
    assert!(is_enabled(&window, DUPLICATE_BELOW));
    assert!(!is_enabled(&window, DELETE_TRACK));
}

#[test]
fn duplicating_a_track_below_puts_a_copy_with_copies_of_its_clips_behind_it_and_selects_the_copy() {
    let (mut window, video, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let tracks_before: Vec<TrackId> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    right_click_the_box_of_track(&mut window, 0);
    choose(&mut window, DUPLICATE_BELOW);

    let tracks = window.application().project().tracks();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].id(), tracks_before[0], "the track is where it was");
    assert_eq!(
        tracks[2].id(),
        tracks_before[1],
        "the track behind it is now two places behind"
    );
    let copy = tracks[1].id();
    assert!(!tracks_before.contains(&copy), "a track of its own");
    assert_eq!(
        tracks[1].title(),
        format!("{} copy", tracks[0].title()),
        "its title says that it is a copy of the track, so that the two can be told apart"
    );
    assert_eq!(
        spans_of_the_clips_on(&window, 1),
        vec![(70, 100)],
        "the clips are where they were"
    );
    let copy_of_the_text = window.clip(1, 0);
    assert_ne!(copy_of_the_text, text, "the clips are copies too");
    assert_eq!(text_of(&window, copy_of_the_text), text_of(&window, text));
    assert_eq!(
        spans_of_the_clips_on(&window, 2),
        vec![(0, 30)],
        "the video track is as it was"
    );
    assert!(window.application().project().placement_of(video).is_some());
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(copy)]);
    assert!(window.application().has_unsaved_changes());
    assert!(!is_offered(&window, DELETE_TRACK), "the menu closed");
}

#[test]
fn duplicating_a_track_above_puts_the_copy_in_front_of_it() {
    let (mut window, _, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let tracks_before: Vec<TrackId> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    right_click_the_box_of_track(&mut window, 1);
    choose(&mut window, DUPLICATE_ABOVE);

    let tracks = window.application().project().tracks();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].id(), tracks_before[0]);
    assert_eq!(
        tracks[2].id(),
        tracks_before[1],
        "the track that was copied is behind its copy"
    );
    let copy = tracks[1].id();
    assert!(!tracks_before.contains(&copy));
    assert_eq!(
        spans_of_the_clips_on(&window, 1),
        vec![(0, 30)],
        "the copy has the clip the track had"
    );
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(copy)]);
}

#[test]
fn a_copy_of_a_track_has_copies_of_the_keyframes_in_the_drawers_of_the_track_and_of_its_clips_which_are_of_the_copies()
{
    let (mut window, _, text) = window_with_a_video_and_a_text_with_room_beside_it();
    let track = window.application().project().tracks()[0].id();
    make_a_keyframe(&mut window, AnimationTarget::Track(track), PropertyId::Gain, 50);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 85);
    let racks_before = window.application().project().clone();

    right_click_the_box_of_track(&mut window, 0);
    choose(&mut window, DUPLICATE_BELOW);

    let project = window.application().project();
    let copy = project.tracks()[1].id();
    let copy_of_the_text = window.clip(1, 0);
    let in_the_drawer_of_the_copy = project.drawer_of(AnimationTarget::Track(copy)).unwrap().racks();
    assert_eq!(in_the_drawer_of_the_copy.len(), 1);
    assert_eq!(
        in_the_drawer_of_the_copy[0].target(),
        AnimationTarget::Track(copy),
        "it animates the copy"
    );
    assert_eq!(in_the_drawer_of_the_copy[0].property(), PropertyId::Gain);
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Track(copy)),
        vec![vec![50]]
    );
    let in_the_drawer_of_the_copy_of_the_text = project
        .drawer_of(AnimationTarget::Clip(copy_of_the_text))
        .unwrap()
        .racks();
    assert_eq!(in_the_drawer_of_the_copy_of_the_text.len(), 1);
    assert_eq!(
        in_the_drawer_of_the_copy_of_the_text[0].target(),
        AnimationTarget::Clip(copy_of_the_text)
    );
    assert_eq!(
        frames_of_the_keyframes_in_the_drawer_of(&window, AnimationTarget::Clip(copy_of_the_text)),
        vec![vec![15]]
    );
    for owner in [AnimationTarget::Track(track), AnimationTarget::Clip(text)] {
        assert_eq!(
            project.drawer_of(owner),
            racks_before.drawer_of(owner),
            "what the track and its clip had is as it was"
        );
    }
    assert_the_racks_are_sound(&window);
}

#[test]
fn a_copy_of_a_track_is_one_step_of_the_history_which_takes_back_the_track_and_what_was_selected() {
    let (mut window, _, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let before = window.application().project().clone();
    let selection_before = window.application().selected_items();
    right_click_the_box_of_track(&mut window, 0);
    choose(&mut window, DUPLICATE_BELOW);
    assert_eq!(window.application().project().tracks().len(), 3);

    window.do_this(Command::Undo);
    assert_eq!(window.application().project(), &before);
    assert_eq!(window.application().selected_items(), selection_before);
    window.do_this(Command::Redo);
    assert_eq!(window.application().project().tracks().len(), 3);
}

#[test]
fn duplicating_a_track_that_is_not_there_does_nothing() {
    let (mut window, _, _) = window_with_a_video_and_a_text_with_room_beside_it();
    let before = window.application().project().clone();
    window.do_this(Command::DuplicateTrack {
        track: TrackId::new(99),
        side: TrackSide::Below,
    });
    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
}
