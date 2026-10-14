//! Tests of the menus that make groups (the menu of a selection of clips, and the menu of the box of a track): what they offer, what choosing
//! an item does (the group that is made is the only thing that is selected afterwards, and it is one step of the history), and what the
//! user is told when it cannot be done.
//!
//! What the items are called is written out in words here, and not taken from the constants of the code, so that these tests pin what the
//! person sees.

use egui::Pos2;
use egui_kittest::kittest::{NodeT, Queryable};

use super::clip_drag_tests::{in_frame, project_of_tracks, window_with_three_clips};
use super::group_box_tests::the_three_clips;
use super::test_window::Window;
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{ClipId, GroupKind, Member};

const CREATE_TAG: &str = "Create tag";
const CREATE_CLIP_GROUP: &str = "Create clip group";
const ADD_TO_TAG: &str = "Add to tag";
const CREATE_TRACK_GROUP: &str = "Create track group";

fn select_clips(window: &mut Window, clips: &[ClipId]) {
    window.do_this(Command::select_clips(clips.to_vec(), SelectionChange::Replace));
}

/// Right-clicks the clip of the first track that is at `frame`.
fn right_click_the_clip_at(window: &mut Window, frame: u64) {
    let point = in_frame(window, 0, frame);
    window.right_click(point);
}

/// Chooses the item of the menu that is open that is called `label`.
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

/// What the message that is up says, as its title and its words.
fn the_message_that_is_up(window: &Window) -> (String, String) {
    match &window.application().dialog {
        Some(Dialog::Message { title, text }) => (title.clone(), text.clone()),
        other => panic!("the user is told: {other:?}"),
    }
}

fn groups_of(window: &Window) -> Vec<(GroupKind, String, Vec<Member>)> {
    window
        .application()
        .project()
        .groups()
        .iter()
        .map(|group| (group.kind(), group.name().to_owned(), group.members().to_vec()))
        .collect()
}

/// Right-clicks the box at the left of the track at `position`.
fn right_click_the_box_of_track(window: &mut Window, position: usize) {
    let title = window.title_box_of_track(position);
    window.right_click(Pos2::new(title.left() - 4.0, title.center().y));
}

// --- the menu of the clips ---

#[test]
fn the_menu_of_two_selected_clips_offers_a_tag_and_a_clip_group_and_nothing_to_add_to_before_there_is_a_tag() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    select_clips(&mut window, &[clips[0], clips[1]]);
    let before = window.application().project().clone();

    right_click_the_clip_at(&mut window, 35);

    assert!(is_offered(&window, CREATE_TAG) && is_enabled(&window, CREATE_TAG));
    assert!(is_offered(&window, CREATE_CLIP_GROUP) && is_enabled(&window, CREATE_CLIP_GROUP));
    assert!(
        window.harness.query_by_label_contains(ADD_TO_TAG).is_none(),
        "there is no tag to add them to"
    );
    assert_eq!(
        window.application().project(),
        &before,
        "opening the menu changes nothing"
    );
    assert_eq!(
        window.application().selected_clips(),
        vec![clips[0], clips[1]],
        "right-clicking one of the selected clips leaves them all selected"
    );
}

#[test]
fn one_clip_can_be_made_a_tag_but_a_clip_group_is_made_of_two_at_the_least() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 35);

    assert!(is_enabled(&window, CREATE_TAG));
    assert!(
        !is_enabled(&window, CREATE_CLIP_GROUP),
        "it is offered, and cannot be used: it says why when the pointer is on it"
    );
    choose(&mut window, CREATE_CLIP_GROUP);
    assert_eq!(groups_of(&window), vec![], "choosing what cannot be used does nothing");
}

#[test]
fn choosing_create_tag_makes_a_tag_of_the_selected_clips_even_with_another_clip_between_them_and_selects_only_the_tag()
{
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    select_clips(&mut window, &[clips[0], clips[2]]);
    right_click_the_clip_at(&mut window, 35);

    choose(&mut window, CREATE_TAG);

    assert_eq!(
        groups_of(&window),
        vec![(
            GroupKind::Tag,
            "Tag 1".to_owned(),
            vec![Member::Clip(clips[0]), Member::Clip(clips[2])]
        )]
    );
    let tag = window.application().project().groups()[0].id();
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Group(tag)],
        "the group is the only thing that is selected: not the clips it was made of"
    );
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn choosing_create_clip_group_makes_a_clip_group_of_clips_that_are_next_to_each_other() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    select_clips(&mut window, &[clips[0], clips[1]]);
    right_click_the_clip_at(&mut window, 80);

    choose(&mut window, CREATE_CLIP_GROUP);

    assert_eq!(
        groups_of(&window),
        vec![(
            GroupKind::ClipGroup,
            "Clip group 1".to_owned(),
            vec![Member::Clip(clips[0]), Member::Clip(clips[1])]
        )]
    );
    let group = window.application().project().groups()[0].id();
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(group)]);
}

#[test]
fn a_clip_group_that_would_have_a_clip_that_is_not_in_it_inside_it_is_refused_with_a_message_that_names_the_clip() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    select_clips(&mut window, &[clips[0], clips[2]]);
    right_click_the_clip_at(&mut window, 35);
    window.application_mut().note_everything_is_saved();

    choose(&mut window, CREATE_CLIP_GROUP);

    assert_eq!(groups_of(&window), vec![], "nothing was made");
    let (title, text) = the_message_that_is_up(&window);
    assert_eq!(title, "A clip group cannot be made of those");
    assert!(
        text.contains("\"Video clip: holiday.mp4 (at 00:00:02.05)\" would be inside the rectangle of a group"),
        "the clip in the way is named: {text}"
    );
    assert_eq!(
        window.application().selected_clips(),
        vec![clips[0], clips[2]],
        "what was selected stays selected"
    );
    assert!(!window.application().has_unsaved_changes(), "it was not an edit");
}

#[test]
fn making_a_group_is_one_step_of_the_history_and_undoing_it_selects_what_was_selected_before() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    select_clips(&mut window, &[clips[0], clips[1]]);
    right_click_the_clip_at(&mut window, 35);
    choose(&mut window, CREATE_TAG);
    assert_eq!(groups_of(&window).len(), 1);

    undo(&mut window);
    assert_eq!(groups_of(&window), vec![], "one step takes the whole group back");
    assert_eq!(window.application().selected_clips(), vec![clips[0], clips[1]]);

    redo(&mut window);
    assert_eq!(groups_of(&window).len(), 1);
    let tag = window.application().project().groups()[0].id();
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);
}

#[test]
fn a_tag_that_is_there_is_offered_in_the_list_that_clips_are_added_to_it_from_and_choosing_it_adds_them() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    select_clips(&mut window, &[clips[0]]);
    right_click_the_clip_at(&mut window, 35);
    choose(&mut window, CREATE_TAG);
    select_clips(&mut window, &[clips[1]]);
    right_click_the_clip_at(&mut window, 80);
    assert!(
        window.harness.query_by_label_contains(ADD_TO_TAG).is_some(),
        "now there is a tag to add to"
    );

    window.harness.get_by_label_contains(ADD_TO_TAG).click();
    window.settle();
    window.harness.get_by_label_contains("Tag 1").click();
    window.settle();

    assert_eq!(
        groups_of(&window),
        vec![(
            GroupKind::Tag,
            "Tag 1".to_owned(),
            vec![Member::Clip(clips[0]), Member::Clip(clips[1])]
        )],
        "it is last in the tag"
    );
    assert_eq!(
        window.application().selected_clips(),
        vec![clips[1]],
        "adding to a tag leaves the selection as it was"
    );
}

// --- the menu of the boxes of the tracks ---

fn window_with_three_tracks() -> Window {
    Window::showing(project_of_tracks(&[&[(20, 30)], &[(20, 30)], &[(20, 30)]]))
}

#[test]
fn the_menu_of_a_track_offers_to_make_a_track_group_of_the_selected_tracks() {
    let mut window = window_with_three_tracks();
    let tracks: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Track(tracks[0]), SelectionItem::Track(tracks[1])],
        change: SelectionChange::Replace,
    });
    right_click_the_box_of_track(&mut window, 0);
    assert!(is_offered(&window, CREATE_TRACK_GROUP));

    choose(&mut window, CREATE_TRACK_GROUP);

    assert_eq!(
        groups_of(&window),
        vec![(
            GroupKind::TrackGroup,
            "Track group 1".to_owned(),
            vec![Member::Track(tracks[0]), Member::Track(tracks[1])]
        )]
    );
    let group = window.application().project().groups()[0].id();
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Group(group)],
        "the tracks are not selected any more: the group is"
    );
}

#[test]
fn the_menu_of_a_track_that_is_not_selected_is_about_that_track_alone() {
    let mut window = window_with_three_tracks();
    let tracks: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    window.do_this(Command::select_track(tracks[0], SelectionChange::Replace));
    right_click_the_box_of_track(&mut window, 2);

    choose(&mut window, CREATE_TRACK_GROUP);

    assert_eq!(
        groups_of(&window),
        vec![(
            GroupKind::TrackGroup,
            "Track group 1".to_owned(),
            vec![Member::Track(tracks[2])]
        )]
    );
}

#[test]
fn tracks_that_have_another_track_between_them_cannot_make_a_track_group_and_the_message_names_the_track_in_the_way() {
    let mut window = window_with_three_tracks();
    let tracks: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Track(tracks[0]), SelectionItem::Track(tracks[2])],
        change: SelectionChange::Replace,
    });
    right_click_the_box_of_track(&mut window, 0);
    window.application_mut().note_everything_is_saved();

    choose(&mut window, CREATE_TRACK_GROUP);

    assert_eq!(groups_of(&window), vec![]);
    let (title, text) = the_message_that_is_up(&window);
    assert_eq!(title, "A track group cannot be made of those");
    assert!(
        text.contains("\"Track: track 1\" is between the tracks of a group, and is not in it"),
        "the track in the way is named: {text}"
    );
    assert!(!window.application().has_unsaved_changes());
}
