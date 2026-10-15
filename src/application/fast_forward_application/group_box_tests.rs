// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the box that the properties panel shows for a group: the outline and the bar at its head (the kind, the name that is typed, the
//! colour, the menu that disbands the group or deletes it with what is in it), and the list of what is in the group (a member is selected by
//! a double click, and a right click takes it out).
//!
//! What the controls are called is written out in words here, and not taken from the constants of the code, so that these tests pin what the
//! person sees.

use egui::{Color32, Rect, Shape};
use egui_kittest::kittest::{NodeT, Queryable};

use super::clip_drag_tests::window_with_three_clips;
use super::test_window::Window;
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{ClipId, GroupColour, GroupId, GroupKind, Member};

const THE_HINT: &str = "Select a clip or a track to see its properties.";
const DISBAND: &str = "Disband";
const DELETE_ALL: &str = "Delete all";
const REMOVE_FROM_GROUP: &str = "Remove from group";
const THE_COLOUR_BUTTON: &str = "The colour of the group";

/// How the first and the second clip of [`window_with_three_clips`] are listed (they start at frames 20 and 65 of a video at 30 frames a second).
pub(super) const FIRST_CLIP_LISTED: &str = "Video clip: holiday.mp4 (at 00:00:00.20)";
pub(super) const SECOND_CLIP_LISTED: &str = "Video clip: holiday.mp4 (at 00:00:02.05)";
pub(super) const THIRD_CLIP_LISTED: &str = "Video clip: holiday.mp4 (at 00:00:03.20)";

/// The three clips of [`window_with_three_clips`], from the left.
pub(super) fn the_three_clips(window: &Window) -> [ClipId; 3] {
    [window.clip(0, 0), window.clip(0, 1), window.clip(0, 2)]
}

pub(super) fn select_only(window: &mut Window, item: SelectionItem) {
    window.do_this(Command::SelectItems {
        items: vec![item],
        change: SelectionChange::Replace,
    });
}

/// A window with the three clips of [`window_with_three_clips`] and a tag of the first two, which is selected, with nothing unsaved and nothing
/// to undo.
pub(super) fn window_with_a_tag_of_the_first_two_clips_selected() -> (Window, GroupId, [ClipId; 3]) {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let tag = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(tag));
    window.application_mut().note_everything_is_saved();
    (window, tag, clips)
}

pub(super) fn colour_of(window: &Window, group: GroupId) -> GroupColour {
    window.application().project().group(group).unwrap().colour()
}

fn as_a_colour(colour: GroupColour) -> Color32 {
    Color32::from_rgb(colour.red, colour.green, colour.blue)
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

/// The outlines that are drawn in the colour `colour`, at least as thick as `at_least` points.
fn outlines_drawn(window: &Window, colour: Color32, at_least: f32) -> Vec<Rect> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.stroke.color == colour && rectangle.stroke.width >= at_least => {
                Some(rectangle.rect)
            }
            _ => None,
        })
        .collect()
}

/// The rectangles that are filled with `colour`.
fn rectangles_filled_with(window: &Window, colour: Color32) -> Vec<Rect> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == colour => Some(rectangle.rect),
            _ => None,
        })
        .collect()
}

/// The colour that `words` are written in, where they are drawn (the first place, if they are drawn in several).
fn writing_colour_of(window: &Window, words: &str) -> Color32 {
    window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text() == words => text
                .galley
                .job
                .sections
                .first()
                .map(|section| text.override_text_color.unwrap_or(section.format.color)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{words:?} is not drawn"))
}

fn right_click_the_bar(window: &mut Window) {
    let point = window.rectangle_of("Tag").center();
    window.right_click(point);
}

fn choose(window: &mut Window, label: &str) {
    window.harness.get_by_label(label).click();
    window.settle();
}

// --- what the box has in it ---

#[test]
fn the_box_of_a_group_has_its_kind_its_name_in_a_box_to_type_in_the_control_of_its_colour_and_what_is_in_it_in_the_order_it_was_put_in()
 {
    let (window, _, _) = window_with_a_tag_of_the_first_two_clips_selected();

    assert!(!is_shown(&window, THE_HINT), "something is selected");
    assert!(is_shown(&window, "Tag"), "its kind");
    assert!(
        window.text_field_showing("Tag 1").rect().height() > 0.0,
        "its name, which can be typed"
    );
    assert!(is_shown(&window, THE_COLOUR_BUTTON));
    let (first, second) = (
        window.rectangle_of(FIRST_CLIP_LISTED),
        window.rectangle_of(SECOND_CLIP_LISTED),
    );
    assert!(first.top() < second.top(), "in the order it was put in");
    assert!(
        !is_shown(&window, THIRD_CLIP_LISTED),
        "the clip that is not in the group is not listed"
    );
}

#[test]
fn the_kind_of_a_clip_group_and_of_a_track_group_is_at_the_head_of_their_boxes() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let clip_group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(clip_group));
    assert!(is_shown(&window, "Clip group"));
    assert!(window.text_field_showing("Clip group 1").rect().width() > 0.0);

    let track = window.application().project().tracks()[0].id();
    let track_group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(track_group));
    assert!(is_shown(&window, "Track group"));
    assert!(window.text_field_showing("Track group 1").rect().width() > 0.0);
    assert!(
        is_shown(&window, "Track: track 0"),
        "a track is listed as the lists of the panel list it, by its title"
    );
}

#[test]
fn the_box_has_a_thick_outline_in_the_colour_of_the_group_and_a_bar_in_that_colour_at_its_head() {
    let (window, tag, _) = window_with_a_tag_of_the_first_two_clips_selected();
    let colour = as_a_colour(colour_of(&window, tag));

    let outlines = outlines_drawn(&window, colour, 4.0);
    assert_eq!(outlines.len(), 1, "one thick outline: {outlines:?}");
    let outline = outlines[0];
    assert!(
        outline.width() > 100.0 && outline.height() > 60.0,
        "it is round the whole box: {outline:?}"
    );
    // (The bar, and the swatch of the colour control on it, which shows the colour: the bar is the wider.)
    let filled = rectangles_filled_with(&window, colour);
    assert_eq!(filled.len(), 2, "the bar and the swatch: {filled:?}");
    let bar = filled
        .iter()
        .copied()
        .max_by(|first, second| first.width().total_cmp(&second.width()))
        .unwrap();
    let swatch = filled
        .iter()
        .copied()
        .min_by(|first, second| first.width().total_cmp(&second.width()))
        .unwrap();
    assert!(
        bar.contains_rect(swatch),
        "the colour control is on the bar: {swatch:?} on {bar:?}"
    );
    assert!(
        outline.contains_rect(bar),
        "the bar is inside the outline: {bar:?} in {outline:?}"
    );
    assert!(
        (bar.top() - outline.top()).abs() < 6.0,
        "it is at the head of the box: {bar:?} in {outline:?}"
    );
    assert!(
        window.rectangle_of("Tag").intersects(bar),
        "the kind is written on the bar"
    );
}

#[test]
fn the_writing_on_the_bar_is_white_on_a_dark_colour_and_black_on_a_light_one() {
    let (mut window, tag, _) = window_with_a_tag_of_the_first_two_clips_selected();
    for (colour, writing, what) in [
        (GroupColour::new(20, 30, 120), Color32::WHITE, "a dark blue"),
        (GroupColour::new(250, 240, 150), Color32::BLACK, "a light yellow"),
    ] {
        window.do_this(Command::SetGroupColour { group: tag, colour });
        assert_eq!(writing_colour_of(&window, "Tag"), writing, "the kind on {what}");
        assert_eq!(
            outlines_drawn(&window, as_a_colour(colour), 4.0).len(),
            1,
            "the outline follows the colour of the group: {what}"
        );
    }
}

// --- changing what the bar has ---

#[test]
fn the_colour_button_opens_a_picker_whose_changes_colour_the_group_and_its_outlines_as_one_step_of_the_history() {
    let (mut window, tag, _) = window_with_a_tag_of_the_first_two_clips_selected();
    let before = colour_of(&window, tag);
    let swatch = window.rectangle_of(THE_COLOUR_BUTTON).center();
    window.click(swatch);

    window.set_the_number_box_that_shows("R ", 200.0);
    window.set_the_number_box_that_shows("B ", 10.0);

    let after = colour_of(&window, tag);
    assert_eq!(
        after,
        GroupColour::new(200, before.green, 10),
        "red and blue were set, and green is as it was"
    );
    assert_eq!(
        outlines_drawn(&window, as_a_colour(after), 4.0).len(),
        1,
        "the outline of the box has the colour it was given"
    );

    window.press_key_outside_the_text(egui::Key::Escape);
    undo(&mut window);
    assert_eq!(
        colour_of(&window, tag),
        before,
        "one step: the two changes were made to the same group, one after another"
    );
}

#[test]
fn typing_in_the_box_of_the_name_renames_the_group_and_undoing_it_gives_the_name_back() {
    let (mut window, tag, _) = window_with_a_tag_of_the_first_two_clips_selected();

    window.replace_text_in_the_field_showing("Tag 1", "Holiday");

    assert_eq!(window.application().project().group(tag).unwrap().name(), "Holiday");
    assert!(window.application().has_unsaved_changes());
    assert!(
        window.text_field_showing("Holiday").rect().width() > 0.0,
        "the box shows it"
    );

    // (The command, and not the keys: while the box has the keyboard focus the keys are the box's own, which undoes what was typed in it and
    // renames the group again, so that the history of the window is not what is looked at.)
    window.do_this(Command::Undo);
    assert_eq!(window.application().project().group(tag).unwrap().name(), "Tag 1");
    window.do_this(Command::Redo);
    assert_eq!(window.application().project().group(tag).unwrap().name(), "Holiday");
}

#[test]
fn what_is_typed_in_the_box_of_the_name_one_after_another_is_one_step_of_the_history() {
    let (mut window, tag, _) = window_with_a_tag_of_the_first_two_clips_selected();

    window.replace_text_in_the_field_showing("Tag 1", "H");
    window.replace_text_in_the_field_showing("H", "Hi");
    assert_eq!(window.application().project().group(tag).unwrap().name(), "Hi");

    // (The command, and not the keys: see the test before this one.)
    window.do_this(Command::Undo);

    assert_eq!(
        window.application().project().group(tag).unwrap().name(),
        "Tag 1",
        "both letters were one step"
    );
}

#[test]
fn the_name_of_a_group_can_be_the_name_of_another_and_can_be_empty() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    let other = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[2])])
        .unwrap();
    window
        .application_mut()
        .document
        .project
        .rename_group(other, "Holiday")
        .unwrap();
    select_only(&mut window, SelectionItem::Group(tag));

    window.replace_text_in_the_field_showing("Tag 1", "Holiday");
    assert_eq!(window.application().project().group(tag).unwrap().name(), "Holiday");
    assert_eq!(window.application().project().group(other).unwrap().name(), "Holiday");

    // (Emptied as a person empties it: everything in the box is selected, and then deleted.)
    let field = window.text_field_showing("Holiday").rect().center();
    window.click(field);
    window.press_key_with(egui::Modifiers::COMMAND, egui::Key::A);
    window.press_key(egui::Key::Backspace);
    assert_eq!(window.application().project().group(tag).unwrap().name(), "");
}

// --- the menu of the bar ---

#[test]
fn the_menu_of_the_bar_has_disband_and_delete_all_and_opening_it_changes_nothing() {
    let (mut window, _, _) = window_with_a_tag_of_the_first_two_clips_selected();
    let before = window.application().project().clone();

    right_click_the_bar(&mut window);

    assert!(is_shown(&window, DISBAND));
    assert!(is_shown(&window, DELETE_ALL));
    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn disband_deletes_the_group_and_none_of_its_members_and_it_is_one_step_of_the_history() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    right_click_the_bar(&mut window);

    choose(&mut window, DISBAND);

    assert_eq!(window.application().project().groups().len(), 0);
    for clip in clips {
        assert!(
            window.application().project().placement_of(clip).is_some(),
            "{clip:?} is still there"
        );
    }
    assert_eq!(
        window.application().selected_items(),
        vec![],
        "the group that is gone is not selected"
    );
    assert!(is_shown(&window, THE_HINT));

    undo(&mut window);
    assert_eq!(window.application().project().groups().len(), 1);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);
}

#[test]
fn the_delete_key_takes_the_selected_group_apart_and_the_clips_in_it_stay() {
    let (mut window, _, clips) = window_with_a_tag_of_the_first_two_clips_selected();

    window.press_key_outside_the_text(egui::Key::Delete);

    assert_eq!(window.application().project().groups().len(), 0);
    for clip in clips {
        assert!(
            window.application().project().placement_of(clip).is_some(),
            "{clip:?} is still there"
        );
    }
    assert_eq!(window.application().selected_items(), vec![]);

    undo(&mut window);
    assert_eq!(window.application().project().groups().len(), 1);
}

#[test]
fn the_delete_key_deletes_the_selected_clips_and_takes_the_selected_groups_apart_in_one_step() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Group(tag), SelectionItem::Clip(clips[2])],
        change: SelectionChange::Replace,
    });

    window.press_key_outside_the_text(egui::Key::Delete);

    assert!(
        window.application().project().placement_of(clips[2]).is_none(),
        "the clip that was selected is deleted"
    );
    assert!(window.application().project().placement_of(clips[0]).is_some());
    assert!(window.application().project().placement_of(clips[1]).is_some());
    assert_eq!(window.application().project().groups().len(), 0);

    undo(&mut window);
    assert!(
        window.application().project().placement_of(clips[2]).is_some(),
        "one step puts back both"
    );
    assert_eq!(window.application().project().groups().len(), 1);
}

#[test]
fn delete_all_deletes_the_group_and_everything_in_it_and_one_undo_brings_it_all_back() {
    let (mut window, _, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    right_click_the_bar(&mut window);

    choose(&mut window, DELETE_ALL);

    assert_eq!(window.application().project().groups().len(), 0);
    assert!(window.application().project().placement_of(clips[0]).is_none());
    assert!(window.application().project().placement_of(clips[1]).is_none());
    assert!(
        window.application().project().placement_of(clips[2]).is_some(),
        "the clip that was not in the group is still there"
    );
    assert_eq!(window.application().selected_items(), vec![]);

    undo(&mut window);
    assert_eq!(window.application().project().groups().len(), 1);
    assert!(window.application().project().placement_of(clips[0]).is_some());
    assert!(window.application().project().placement_of(clips[1]).is_some());
}

#[test]
fn delete_all_of_a_group_that_has_every_track_of_the_project_is_refused_with_a_message_and_changes_nothing() {
    let mut window = window_with_three_clips();
    let track = window.application().project().tracks()[0].id();
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(group));
    window.application_mut().note_everything_is_saved();
    let before = window.application().project().clone();
    let point = window.rectangle_of("Track group").center();
    window.right_click(point);

    choose(&mut window, DELETE_ALL);

    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
    match &window.application().interface.dialog {
        Some(crate::application::dialogs::Dialog::Message { title, text }) => {
            assert_eq!(title, "The group cannot be deleted with all that is in it");
            assert!(text.contains("always has a track"), "{text}");
        }
        other => panic!("the user is told: {other:?}"),
    }
}

// --- the list of what is in the group ---

#[test]
fn a_member_is_selected_by_a_double_click_and_not_by_a_click() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    let row = window.rectangle_of(SECOND_CLIP_LISTED).center();

    window.click(row);
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Group(tag)],
        "a click selects nothing: taking hold of what is in a list must not select it"
    );

    window.double_click(row);
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(clips[1])],
        "a double click selects the member, and only it"
    );
    assert!(is_shown(&window, "Video clip"), "its own properties are shown");
}

#[test]
fn a_member_that_is_a_group_is_selected_by_a_double_click_as_a_group() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let inner = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    let outer = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Group(inner), Member::Clip(clips[2])])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(outer));
    let row = window.rectangle_of("Clip group: Clip group 1").center();

    window.double_click(row);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(inner)]);
    assert!(
        is_shown(&window, "Clip group"),
        "the box of the group that was a member"
    );
}

#[test]
fn the_menu_of_a_member_takes_it_out_of_the_group_and_it_stays_in_the_project() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    let row = window.rectangle_of(FIRST_CLIP_LISTED).center();
    window.right_click(row);
    assert!(is_shown(&window, REMOVE_FROM_GROUP));
    assert_eq!(
        window.application().project().group(tag).unwrap().members().len(),
        2,
        "opening the menu changes nothing"
    );

    choose(&mut window, REMOVE_FROM_GROUP);

    assert_eq!(
        window.application().project().group(tag).unwrap().members(),
        [Member::Clip(clips[1])]
    );
    assert!(window.application().project().placement_of(clips[0]).is_some());
    assert!(!is_shown(&window, FIRST_CLIP_LISTED), "it is not listed any more");
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);

    undo(&mut window);
    assert_eq!(window.application().project().group(tag).unwrap().members().len(), 2);
}

#[test]
fn taking_the_last_member_out_of_a_group_drops_the_group() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();
    for listed in [FIRST_CLIP_LISTED, SECOND_CLIP_LISTED] {
        let row = window.rectangle_of(listed).center();
        window.right_click(row);
        choose(&mut window, REMOVE_FROM_GROUP);
    }

    assert!(window.application().project().group(tag).is_none());
    assert_eq!(
        window.application().selected_items(),
        vec![],
        "a group that is gone is not selected"
    );
    assert!(is_shown(&window, THE_HINT));
    assert!(window.application().project().placement_of(clips[0]).is_some());
}

// (Each of the next three looks at what is selected right after the command, before any frame has been drawn: a frame forgets what is
// gone from what is selected too, so a test that lets frames go by cannot tell whether the command did.)

#[test]
fn the_group_that_is_gone_when_its_last_member_is_taken_out_is_not_selected_even_before_the_next_frame() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();

    for clip in [clips[0], clips[1]] {
        window.do_this_and_draw_nothing(Command::RemoveFromGroup {
            group: tag,
            member: Member::Clip(clip),
        });
    }

    assert!(window.application().project().group(tag).is_none());
    assert_eq!(window.application().selected_items(), vec![]);
}

#[test]
fn a_group_that_is_disbanded_is_not_selected_even_before_the_next_frame() {
    let (mut window, tag, _) = window_with_a_tag_of_the_first_two_clips_selected();

    window.do_this_and_draw_nothing(Command::DisbandGroup(tag));

    assert!(window.application().project().group(tag).is_none());
    assert_eq!(window.application().selected_items(), vec![]);
}

#[test]
fn a_group_that_is_deleted_with_all_that_is_in_it_is_not_selected_even_before_the_next_frame() {
    let (mut window, tag, clips) = window_with_a_tag_of_the_first_two_clips_selected();

    window.do_this_and_draw_nothing(Command::DeleteGroupAndEverythingInIt(tag));

    assert!(window.application().project().group(tag).is_none());
    assert!(window.application().project().placement_of(clips[0]).is_none());
    assert_eq!(window.application().selected_items(), vec![]);
}

#[test]
fn deleting_a_group_with_all_that_is_in_it_leaves_the_playhead_where_it_was_if_that_is_still_in_the_timeline() {
    let (mut window, tag, clips) = window_with_three_clips_and_a_tag_of_the_last_two();
    window.move_current_frame_to(30);

    window.do_this(Command::DeleteGroupAndEverythingInIt(tag));

    assert!(window.application().project().placement_of(clips[1]).is_none());
    assert_eq!(window.current_frame(), 30);
}

#[test]
fn deleting_a_group_with_all_that_is_in_it_takes_the_playhead_to_the_new_end_if_it_was_past_it_and_it_stays_there() {
    let (mut window, tag, _) = window_with_three_clips_and_a_tag_of_the_last_two();
    // (The clips are at 20 to 50, 65 to 95 and 110 to 140: with the last two gone the timeline ends at 50.)
    window.move_current_frame_to(120);

    window.do_this(Command::DeleteGroupAndEverythingInIt(tag));

    assert_eq!(window.application().project().total_frame_count(), 50);
    assert_eq!(window.current_frame(), 49, "the last frame that is left");
    assert_eq!(
        window
            .application()
            .playback
            .clock
            .position(std::time::Instant::now(), 10_000)
            .value(),
        49,
        "and it is not still at 120 where it would come back to if the timeline grew again"
    );
}

/// The window of [`window_with_three_clips`] with a tag of the last two of its clips, and nothing selected.
fn window_with_three_clips_and_a_tag_of_the_last_two() -> (Window, GroupId, [ClipId; 3]) {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let tag = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[1]), Member::Clip(clips[2])])
        .unwrap();
    window.settle();
    (window, tag, clips)
}

#[test]
fn a_member_in_the_middle_of_a_clip_group_cannot_be_taken_out_and_the_user_is_told_why() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let group = window
        .application_mut()
        .document
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(clips[0]), Member::Clip(clips[1]), Member::Clip(clips[2])],
        )
        .unwrap();
    select_only(&mut window, SelectionItem::Group(group));
    window.application_mut().note_everything_is_saved();
    let row = window.rectangle_of(SECOND_CLIP_LISTED).center();
    window.right_click(row);

    assert!(
        is_shown(&window, REMOVE_FROM_GROUP),
        "the item is there, as the maintainer asked"
    );
    choose(&mut window, REMOVE_FROM_GROUP);

    assert_eq!(
        window.application().project().group(group).unwrap().members().len(),
        3,
        "nothing changed"
    );
    assert!(!window.application().has_unsaved_changes());
    match &window.application().interface.dialog {
        Some(crate::application::dialogs::Dialog::Message { title, text }) => {
            assert_eq!(title, "That cannot be taken out of the group");
            assert!(
                text.contains("would be inside the rectangle of the clip group \"Clip group 1\""),
                "{text}"
            );
        }
        other => panic!("the user is told: {other:?}"),
    }
}

#[test]
fn nothing_in_the_box_of_a_group_can_be_changed_while_a_message_is_up() {
    let (mut window, _, _) = window_with_a_tag_of_the_first_two_clips_selected();
    assert!(!window.text_field_showing("Tag 1").accesskit_node().is_disabled());

    window.application_mut().interface.dialog = Some(crate::application::dialogs::Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    assert!(window.text_field_showing("Tag 1").accesskit_node().is_disabled());
}
