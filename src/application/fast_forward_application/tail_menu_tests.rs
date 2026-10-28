// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menus of phase 12 (docs/engine-phase-12-design.md): the sharing of a track in the menu of its box, the tail policy of a clip in the menu of the clip, the tail policy of a group in the
//! menu of its bar, and the length of the tail of an effect in the menu of its box: what each offers, what choosing does, that it is one step of undo, what is marked, and what the user
//! is told when a rule of groups refuses it.
//!
//! What the items are called is written out in words here, and not taken from the constants of the code, so that these tests pin what the person sees.

use egui::Pos2;
use egui_kittest::kittest::Queryable;

use super::clip_drag_tests::{in_frame, window_with_three_clips};
use super::group_box_tests::the_three_clips;
use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::naming::what_an_effect_is_called;
use crate::application::selection::SelectionItem;
use crate::project::{
    AnimationTarget, ClipTailEdge, ClipTailPolicy, EffectDomain, EffectInstanceId, EffectKind, GroupKind,
    GroupTailEdge, GroupTailPolicy, Member, TailDepth, TailLength, TrackSharing,
};

/// Right-clicks the clip of the first track that is at `frame`.
fn right_click_the_clip_at(window: &mut Window, frame: u64) {
    let point = in_frame(window, 0, frame);
    window.right_click(point);
}

/// Right-clicks the box at the left of the track at `position`.
fn right_click_the_box_of_track(window: &mut Window, position: usize) {
    let title = window.title_box_of_track(position);
    window.right_click(Pos2::new(title.left() - 4.0, title.center().y));
}

/// Chooses the item that has `words` in its label (the one that is in force has a mark in front of it).
fn choose(window: &mut Window, words: &str) {
    window.harness.get_by_label_contains(words).click();
    window.settle();
}

fn is_shown(window: &Window, words: &str) -> bool {
    window.harness.query_by_label_contains(words).is_some()
}

fn the_message_that_is_up(window: &Window) -> (String, String) {
    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => (title.clone(), text.clone()),
        other => panic!("the user is told: {other:?}"),
    }
}

// --- the menu of a clip ---

#[test]
fn the_menu_of_a_clip_has_the_tail_toward_other_groups_with_the_default_marked_and_choosing_one_is_one_step_of_undo() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    right_click_the_clip_at(&mut window, 35);
    assert!(is_shown(&window, "Tail toward other groups"));
    choose(&mut window, "Tail toward other groups");
    assert!(
        is_shown(&window, "\u{2713} Goes around the group"),
        "what is in force is marked: the tail goes around"
    );
    assert!(is_shown(&window, "\u{2713} Drawn behind the group"));
    assert!(is_shown(&window, "Goes into the group's effects") && is_shown(&window, "Is cut at the group's edge"));
    assert_eq!(
        window.application().project().clip_tail_policy(clips[0]),
        Some(ClipTailPolicy::default()),
        "opening the menu changes nothing"
    );

    choose(&mut window, "Is cut at the group's edge");

    let policy = |window: &Window, clip| window.application().project().clip_tail_policy(clip).unwrap();
    assert_eq!(policy(&window, clips[0]).edge, ClipTailEdge::Cut);
    assert_eq!(policy(&window, clips[0]).depth, TailDepth::Behind);
    assert_eq!(
        policy(&window, clips[1]),
        ClipTailPolicy::default(),
        "the others are as they were"
    );
    undo(&mut window);
    assert_eq!(policy(&window, clips[0]), ClipTailPolicy::default());
    redo(&mut window);
    assert_eq!(policy(&window, clips[0]).edge, ClipTailEdge::Cut);
}

#[test]
fn the_menu_of_several_selected_clips_sets_the_tail_policy_of_all_of_them_in_one_step_and_the_depth_is_chosen_apart_from_the_edge()
 {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    window.do_this(Command::select_clips(
        vec![clips[0], clips[2]],
        crate::application::selection::SelectionChange::Replace,
    ));
    right_click_the_clip_at(&mut window, 35);
    choose(&mut window, "Tail toward other groups");

    choose(&mut window, "Drawn in front of the group");

    let policy = |clip| window.application().project().clip_tail_policy(clip).unwrap();
    assert_eq!(policy(clips[0]).depth, TailDepth::InFront);
    assert_eq!(policy(clips[2]).depth, TailDepth::InFront);
    assert_eq!(policy(clips[1]).depth, TailDepth::Behind);
    undo(&mut window);
    assert_eq!(
        window.application().project().clip_tail_policy(clips[2]),
        Some(ClipTailPolicy::default()),
        "one step for both"
    );
}

#[test]
fn a_clip_on_a_track_where_groups_take_a_window_is_told_that_its_tail_goes_into_the_group_whatever_it_chooses() {
    let mut window = window_with_three_clips();
    let track = window.application().project().tracks()[0].id();
    window
        .application_mut()
        .document
        .project
        .set_track_sharing(track, EffectDomain::Sound, TrackSharing::PerTrackWithWindows)
        .unwrap();
    right_click_the_clip_at(&mut window, 35);
    choose(&mut window, "Tail toward other groups");
    assert!(is_shown(
        &window,
        "a tail goes into the group's effects whatever is chosen"
    ));
}

// --- the menu of the box of a track ---

#[test]
fn the_menu_of_the_box_of_a_track_has_the_sharing_of_its_picture_effects_and_of_its_sound_effects_each_with_four_choices_and_the_default_marked()
 {
    let mut window = window_with_three_clips();
    right_click_the_box_of_track(&mut window, 0);
    assert!(is_shown(&window, "Sharing of picture effects") && is_shown(&window, "Sharing of sound effects"));

    choose(&mut window, "Sharing of sound effects");

    assert!(is_shown(&window, "Separate for each clip"));
    assert!(is_shown(&window, "\u{2713} One copy for each run of clips"));
    assert!(is_shown(&window, "One for the track, groups take a window"));
    assert!(is_shown(&window, "One for the track, never divided"));
}

#[test]
fn choosing_a_sharing_for_the_sound_effects_of_a_track_changes_that_medium_only_and_is_one_step_of_undo() {
    let mut window = window_with_three_clips();
    let track = window.application().project().tracks()[0].id();
    right_click_the_box_of_track(&mut window, 0);
    choose(&mut window, "Sharing of sound effects");

    choose(&mut window, "Separate for each clip");

    let sharing = |window: &Window| window.application().project().track(track).unwrap().sharing();
    assert_eq!(sharing(&window).sound, TrackSharing::PerClip);
    assert_eq!(sharing(&window).picture, TrackSharing::PerRun);
    undo(&mut window);
    assert_eq!(sharing(&window).sound, TrackSharing::PerRun);
    redo(&mut window);
    assert_eq!(sharing(&window).sound, TrackSharing::PerClip);
}

#[test]
fn a_sharing_that_a_group_with_effects_would_break_is_refused_with_the_reason_and_changes_nothing() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let track = window.application().project().tracks()[0].id();
    {
        let project = &mut window.application_mut().document.project;
        // A group of the first two clips with an effect that is switched on takes part of the track, which has a third clip.
        let group = project
            .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
            .unwrap();
        project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    }
    window.settle();
    let before = window.application().project().clone();
    right_click_the_box_of_track(&mut window, 0);
    choose(&mut window, "Sharing of picture effects");

    choose(&mut window, "One for the track, never divided");

    let (title, text) = the_message_that_is_up(&window);
    assert_eq!(title, "The sharing of the effects cannot be changed");
    assert!(text.contains("never divided") && text.contains("picture"), "{text}");
    assert_eq!(window.application().project(), &before, "nothing was changed");
    assert_eq!(
        window.application().project().track(track).unwrap().sharing().picture,
        TrackSharing::PerRun
    );
}

// --- the menu of the bar of a group ---

#[test]
fn the_menu_of_the_bar_of_a_tag_has_no_tail_of_members_because_a_tag_has_no_edge() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let tag = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[2])])
        .unwrap();
    super::group_box_tests::select_only(&mut window, SelectionItem::Group(tag));
    window.settle();

    let point = window.rectangle_of("Tag").center();
    window.right_click(point);

    assert!(is_shown(&window, "Disband"), "the menu is open");
    assert!(!is_shown(&window, "Tail of members"));
}

#[test]
fn the_menu_of_the_bar_of_a_clip_group_has_the_tail_of_its_members_with_the_default_marked_and_choosing_one_is_one_step_of_undo()
 {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let clip_group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    super::group_box_tests::select_only(&mut window, SelectionItem::Group(clip_group));
    window.settle();
    let point = window.rectangle_of("Clip group").center();
    window.right_click(point);
    assert!(is_shown(&window, "Tail of members"));
    choose(&mut window, "Tail of members");
    assert!(is_shown(&window, "\u{2713} Keep going through the group's effects"));

    choose(&mut window, "Leave the group at its edge");

    let policy = window.application().project().group(clip_group).unwrap().tail_policy();
    assert_eq!(
        policy,
        GroupTailPolicy {
            edge: GroupTailEdge::Release,
            depth: TailDepth::Behind
        }
    );
    undo(&mut window);
    assert_eq!(
        window.application().project().group(clip_group).unwrap().tail_policy(),
        GroupTailPolicy::default()
    );
}

// --- the menu of the box of an effect ---

#[test]
fn the_menu_of_the_box_of_an_effect_has_its_tail_length_with_the_automatic_one_marked_and_choosing_a_length_is_one_step_of_undo()
 {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();
    let heading = window
        .rectangle_of(&what_an_effect_is_called(EffectKind::HUE_SHIFT))
        .center();
    window.right_click(heading);
    choose(&mut window, "Tail length");
    assert!(is_shown(&window, "\u{2713} Automatic (what the plugin says)"));
    assert!(is_shown(&window, "No tail") && is_shown(&window, "500 ms") && is_shown(&window, "30 s"));

    choose(&mut window, "2 s");

    let length = |window: &Window| {
        window
            .application()
            .project()
            .effects_of(AnimationTarget::Clip(clip))
            .unwrap()
            .instance(EffectInstanceId::FIRST)
            .unwrap()
            .tail_length()
    };
    assert_eq!(length(&window), TailLength::Milliseconds(2_000));
    undo(&mut window);
    assert_eq!(length(&window), TailLength::Automatic);
    redo(&mut window);
    assert_eq!(length(&window), TailLength::Milliseconds(2_000));
}
