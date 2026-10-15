// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the chain of effects in the properties panel: the boxes of the effects with the badges among them (of the tags that a clip or a
//! group is in, of the track of a clip, and last of the clip group or track group it is in), which are all taken hold of and put in another
//! place by dragging, and which a double click on a badge goes from to what it stands for.
//!
//! What the controls are called is written out in words here, and not taken from the constants of the code, so that these tests pin what the
//! person sees.

use egui::{PointerButton, Rect, Shape};
use egui_kittest::kittest::Queryable;

use super::clip_drag_tests::{project_of_tracks, window_with_three_clips};
use super::group_box_tests::{colour_of, select_only, the_three_clips};
use super::test_window::Window;
use super::undo_tests::{redo, undo};
use crate::application::selection::SelectionItem;
use crate::application::theme;
use crate::application::timeline_panel::GripDotArrangement;
use crate::project::{
    AnimationTarget, ChainRow, ClipId, EffectKind, GroupId, GroupKind, Member, PropertyId, PropertyValue, TrackId,
};

const HUE_SHIFT: &str = "Hue shift";

/// What the chain of `owner` shows, from the first thing that is applied to the last, as names that can be compared: the kinds of the effects
/// and the badges.
fn the_chain_of(window: &Window, owner: Member) -> Vec<String> {
    window
        .application()
        .project()
        .chain_rows_of(owner)
        .iter()
        .map(|row| match row {
            ChainRow::Effect(effect) => effect.kind().name().to_owned(),
            ChainRow::TagBadge(tag) => format!("tag {}", tag.name()),
            ChainRow::TrackBadge { track, .. } => format!("track {}", track.title()),
            ChainRow::ShapeBadge(group) => format!("{} {}", group.kind().name(), group.name()),
        })
        .collect()
}

/// A window with the three clips of [`window_with_three_clips`] where the first two are in a tag, and the first has the hue shift (after the
/// badge of the tag, which it got first), selected. Nothing is unsaved and there is nothing to undo.
fn window_with_a_clip_that_is_in_a_tag_and_has_an_effect() -> (Window, ClipId, GroupId) {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let tag = window
        .application_mut()
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window
        .application_mut()
        .project
        .add_effect(AnimationTarget::Clip(clips[0]), EffectKind::HueShift)
        .unwrap();
    select_only(&mut window, SelectionItem::Clip(clips[0]));
    window.application_mut().note_everything_is_saved();
    (window, clips[0], tag)
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn middle_of(window: &Window, label: &str) -> egui::Pos2 {
    window.rectangle_of(label).center()
}

// --- what the chain shows ---

#[test]
fn a_clip_that_is_in_a_tag_has_the_badge_of_the_tag_in_its_chain_with_the_effects_in_the_order_they_are_applied_in() {
    let (window, clip, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();

    assert_eq!(the_chain_of(&window, Member::Clip(clip)), ["tag Tag 1", "hue shift"]);
    let (badge, effect) = (window.rectangle_of("Tag: Tag 1"), window.rectangle_of(HUE_SHIFT));
    assert!(
        badge.top() < effect.top(),
        "from the first applied, at the top, to the last"
    );
    assert!(
        badge.top() > window.rectangle_of("Video clip").top(),
        "the chain is under the box of the clip"
    );
}

#[test]
fn the_badge_of_a_tag_is_in_the_colour_of_the_tag_and_has_the_dots_that_say_it_can_be_taken_hold_of() {
    let (window, _, tag) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let colour = colour_of(&window, tag);
    let colour = egui::Color32::from_rgb(colour.red, colour.green, colour.blue);

    let badges: Vec<Rect> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == colour => Some(rectangle.rect),
            _ => None,
        })
        .collect();
    assert_eq!(badges.len(), 1, "one badge: {badges:?}");
    assert!(
        badges[0].contains_rect(window.rectangle_of("Tag: Tag 1")),
        "it has the words on it"
    );
    let dots = window.grip_dots_drawn(GripDotArrangement::FOR_A_RACK);
    assert!(
        dots.iter().any(|dots| badges[0].contains_rect(*dots)),
        "dots on the badge: {dots:?} on {:?}",
        badges[0]
    );
}

// --- double click ---

#[test]
fn a_click_on_a_badge_selects_nothing_and_a_double_click_selects_what_it_stands_for() {
    let (mut window, clip, tag) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let badge = middle_of(&window, "Tag: Tag 1");

    window.click(badge);
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(clip)],
        "a click is for taking hold of the badge, and does not select the tag"
    );

    window.double_click(badge);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);
    assert!(is_shown(&window, "Tag"), "the box of the tag is shown");
}

// --- dragging ---

#[test]
fn dragging_a_badge_down_past_an_effect_puts_it_after_the_effect_as_one_step_of_the_history() {
    let (mut window, clip, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let owner = Member::Clip(clip);
    let from = middle_of(&window, "Tag: Tag 1");
    let below_the_effect = middle_of(&window, HUE_SHIFT) + egui::vec2(0.0, 60.0);

    window.drag(from, below_the_effect, PointerButton::Primary);

    assert_eq!(the_chain_of(&window, owner), ["hue shift", "tag Tag 1"]);
    assert!(window.application().has_unsaved_changes());
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(clip)],
        "taking hold of a badge selects nothing"
    );
    let (effect, badge) = (window.rectangle_of(HUE_SHIFT), window.rectangle_of("Tag: Tag 1"));
    assert!(effect.top() < badge.top(), "it is shown where it is now");

    undo(&mut window);
    assert_eq!(
        the_chain_of(&window, owner),
        ["tag Tag 1", "hue shift"],
        "one step puts it back"
    );
    redo(&mut window);
    assert_eq!(the_chain_of(&window, owner), ["hue shift", "tag Tag 1"]);
}

#[test]
fn dragging_the_box_of_an_effect_by_its_heading_puts_it_before_a_badge_it_is_dragged_above() {
    let (mut window, clip, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let owner = Member::Clip(clip);
    let from = middle_of(&window, HUE_SHIFT);
    let above_the_badge = middle_of(&window, "Tag: Tag 1") - egui::vec2(0.0, 12.0);

    window.drag(from, above_the_badge, PointerButton::Primary);

    assert_eq!(the_chain_of(&window, owner), ["hue shift", "tag Tag 1"]);
}

#[test]
fn a_row_that_is_let_go_of_where_it_was_changes_nothing_and_is_not_an_edit() {
    let (mut window, clip, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let owner = Member::Clip(clip);
    let badge = middle_of(&window, "Tag: Tag 1");

    window.drag(badge, badge + egui::vec2(0.0, 6.0), PointerButton::Primary);

    assert_eq!(the_chain_of(&window, owner), ["tag Tag 1", "hue shift"]);
    assert!(!window.application().has_unsaved_changes());
}

/// The y of each line that shows where a row that is dragged will go.
fn lines_that_show_where_a_row_will_go(window: &Window) -> Vec<f32> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke } if stroke.color == theme::WHERE_IT_WILL_GO && stroke.width >= 3.0 => {
                Some(points[0].y)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_line_that_shows_where_a_row_will_go_is_in_the_room_between_the_rows_it_goes_between() {
    let (mut window, _, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let effect = middle_of(&window, HUE_SHIFT);
    let badge = middle_of(&window, "Tag: Tag 1");

    // The box of the effect is dragged to just above the middle of the badge: it will go before it, which is first.
    window.drag_and_hold(effect, badge - egui::vec2(0.0, 12.0), PointerButton::Primary);
    window.settle();

    let lines = lines_that_show_where_a_row_will_go(&window);
    assert_eq!(lines.len(), 1, "{lines:?}");
    // A badge is 30 points tall, and the room between the rows is 8: the line is in the middle of that room, above the badge.
    assert!(
        (lines[0] - (badge.y - 15.0 - 4.0)).abs() < 0.6,
        "{} and the badge's middle is {}",
        lines[0],
        badge.y
    );
}

#[test]
fn while_a_row_is_dragged_it_is_dimmed_and_a_line_shows_where_it_will_go_and_neither_is_there_after() {
    let (mut window, _, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let from = middle_of(&window, "Tag: Tag 1");
    let below_the_effect = middle_of(&window, HUE_SHIFT) + egui::vec2(0.0, 60.0);

    window.drag_and_hold(from, below_the_effect, PointerButton::Primary);
    window.settle();

    let lines: Vec<f32> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke } if stroke.color == theme::WHERE_IT_WILL_GO && stroke.width >= 3.0 => {
                Some(points[0].y)
            }
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 1, "one line: {lines:?}");
    assert!(
        lines[0] > window.rectangle_of(HUE_SHIFT).bottom(),
        "it is under the effect, which is where the badge will go"
    );
    let veils = window
        .shapes_drawn()
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::BEING_DRAGGED_VEIL))
        .count();
    assert_eq!(veils, 1, "what is dragged is dimmed");
    assert_eq!(
        the_chain_of(&window, Member::Clip(window.clip(0, 0))),
        ["tag Tag 1", "hue shift"],
        "nothing is changed until it is let go of"
    );

    window.release(below_the_effect, PointerButton::Primary);
    window.settle();
    let none_left = window.shapes_drawn().into_iter().all(|shape| match shape {
        Shape::LineSegment { stroke, .. } => stroke.color != theme::WHERE_IT_WILL_GO || stroke.width < 3.0,
        Shape::Rect(rectangle) => rectangle.fill != theme::BEING_DRAGGED_VEIL,
        _ => true,
    });
    assert!(none_left, "the line and the veil are gone");
}

#[test]
fn a_double_click_soon_after_a_click_somewhere_else_still_selects_what_the_badge_stands_for() {
    // (egui counts the second click of such a double click as a triple click, because of the click before it, and a badge must not care.)
    let (mut window, _, tag) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let somewhere_else = middle_of(&window, HUE_SHIFT);
    window.click(somewhere_else);
    assert_eq!(
        window.application().selected_items().len(),
        1,
        "(a click on the heading of an effect selects nothing)"
    );
    let badge = middle_of(&window, "Tag: Tag 1");

    window.double_click(badge);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);
}

#[test]
fn a_double_click_soon_after_a_click_somewhere_else_still_selects_a_member_of_a_group_from_its_list() {
    let (mut window, tag, clips) = super::group_box_tests::window_with_a_tag_of_the_first_two_clips_selected();
    let somewhere_else = middle_of(&window, "Tag");
    window.click(somewhere_else);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);
    let row = middle_of(&window, super::group_box_tests::FIRST_CLIP_LISTED);

    window.double_click(row);

    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(clips[0])]
    );
}

// --- the badge of the track ---

/// A window with the three clips of [`window_with_three_clips`], where their track has the hue shift, and the first clip is selected. Nothing
/// is unsaved.
fn window_with_a_track_that_has_an_effect_and_a_clip_of_it_selected() -> (Window, ClipId, TrackId) {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let track = window.application().project().tracks()[0].id();
    window
        .application_mut()
        .project
        .add_effect(AnimationTarget::Track(track), EffectKind::HueShift)
        .unwrap();
    select_only(&mut window, SelectionItem::Clip(clips[0]));
    window.application_mut().note_everything_is_saved();
    (window, clips[0], track)
}

const THE_TRACK_BADGE: &str = "Track: track 0 (1 effect)";

#[test]
fn a_clip_on_a_track_that_has_effects_has_a_grey_badge_that_names_the_track_and_says_how_many_effects_apply() {
    let (window, clip, _) = window_with_a_track_that_has_an_effect_and_a_clip_of_it_selected();

    assert!(is_shown(&window, THE_TRACK_BADGE));
    assert_eq!(the_chain_of(&window, Member::Clip(clip)), ["track track 0"]);
    let greys = window
        .shapes_drawn()
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::TRACK_BADGE_BACKGROUND))
        .count();
    assert_eq!(greys, 1, "grey, as the maintainer said");
    assert!(
        !window.grip_dots_drawn(GripDotArrangement::FOR_A_RACK).is_empty(),
        "it can be moved anywhere in the chain, so it has dots"
    );
}

#[test]
fn a_clip_on_a_track_that_has_no_effects_has_no_badge_of_its_track_and_a_track_has_none_of_itself() {
    let window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let mut window = window;
    select_only(&mut window, SelectionItem::Clip(clips[0]));
    assert!(
        window.harness.query_all_by_label_contains("Track: ").next().is_none(),
        "nothing of the track applies to the clip"
    );

    let (mut window, _, track) = window_with_a_track_that_has_an_effect_and_a_clip_of_it_selected();
    select_only(&mut window, SelectionItem::Track(track));
    assert!(is_shown(&window, HUE_SHIFT), "its own effect is in its chain");
    assert!(
        window.harness.query_all_by_label_contains("Track: ").next().is_none(),
        "a track does not have the badge of itself"
    );
}

#[test]
fn a_click_on_the_badge_of_the_track_selects_nothing_and_a_double_click_selects_the_track() {
    let (mut window, clip, track) = window_with_a_track_that_has_an_effect_and_a_clip_of_it_selected();
    let badge = middle_of(&window, THE_TRACK_BADGE);

    window.click(badge);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(clip)]);

    window.double_click(badge);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(track)]);
    assert!(is_shown(&window, "Track"), "the box of the track");
}

#[test]
fn the_badge_of_the_track_can_be_dragged_above_the_effects_of_the_clip_and_is_kept_there() {
    let (mut window, clip, _) = window_with_a_track_that_has_an_effect_and_a_clip_of_it_selected();
    window
        .application_mut()
        .project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    window.settle();
    let owner = Member::Clip(clip);
    assert_eq!(
        the_chain_of(&window, owner),
        ["hue shift", "track track 0"],
        "the effects of the clip come first and then those of its track, until it is moved"
    );
    let from = middle_of(&window, THE_TRACK_BADGE);
    let above_the_effect = middle_of(&window, HUE_SHIFT) - egui::vec2(0.0, 40.0);

    window.drag(from, above_the_effect, PointerButton::Primary);

    assert_eq!(the_chain_of(&window, owner), ["track track 0", "hue shift"]);
    undo(&mut window);
    assert_eq!(
        the_chain_of(&window, owner),
        ["hue shift", "track track 0"],
        "one step puts it back"
    );
}

// --- the group a clip is directly in, last ---

/// A window with the three clips of [`window_with_three_clips`] where the first two are in a clip group, and the first is selected. Nothing is
/// unsaved.
fn window_with_a_clip_that_is_in_a_clip_group() -> (Window, ClipId, GroupId) {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let group = window
        .application_mut()
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    select_only(&mut window, SelectionItem::Clip(clips[0]));
    window.application_mut().note_everything_is_saved();
    (window, clips[0], group)
}

const THE_CLIP_GROUP_BADGE: &str = "Clip group: Clip group 1";

#[test]
fn the_clip_group_a_clip_is_in_has_its_badge_last_in_the_chain_with_no_dots_because_it_is_not_moved() {
    let (mut window, clip, group) = window_with_a_clip_that_is_in_a_clip_group();
    window
        .application_mut()
        .project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    window.settle();

    assert_eq!(
        the_chain_of(&window, Member::Clip(clip)),
        ["hue shift", "clip group Clip group 1"],
        "after the effects of the clip: the effects of the group are applied to what the clip makes"
    );
    let badge = window.rectangle_of(THE_CLIP_GROUP_BADGE);
    assert!(
        badge.top() > window.rectangle_of("Add an effect").bottom() - 1.0,
        "under the plus button, apart"
    );
    let colour = colour_of(&window, group);
    let colour = egui::Color32::from_rgb(colour.red, colour.green, colour.blue);
    let filled: Vec<Rect> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == colour => Some(rectangle.rect),
            _ => None,
        })
        .collect();
    assert_eq!(filled.len(), 1, "in the colour of the group: {filled:?}");
    assert!(
        window
            .grip_dots_drawn(GripDotArrangement::FOR_A_RACK)
            .iter()
            .all(|dots| !filled[0].intersects(*dots)),
        "no dots on it"
    );
}

#[test]
fn the_badge_of_the_clip_group_cannot_be_dragged_and_nothing_can_be_dragged_past_it() {
    let (mut window, clip, _) = window_with_a_clip_that_is_in_a_clip_group();
    window
        .application_mut()
        .project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    window.settle();
    window.application_mut().note_everything_is_saved();
    let owner = Member::Clip(clip);
    let badge = middle_of(&window, THE_CLIP_GROUP_BADGE);

    window.drag(
        badge,
        middle_of(&window, HUE_SHIFT) - egui::vec2(0.0, 30.0),
        PointerButton::Primary,
    );
    assert_eq!(the_chain_of(&window, owner), ["hue shift", "clip group Clip group 1"]);
    assert!(
        !window.application().has_unsaved_changes(),
        "dragging it is not an edit"
    );

    let effect = middle_of(&window, HUE_SHIFT);
    window.drag(effect, badge + egui::vec2(0.0, 40.0), PointerButton::Primary);
    assert_eq!(
        the_chain_of(&window, owner),
        ["hue shift", "clip group Clip group 1"],
        "the group stays last: the effect is put last of what can be moved, which it is"
    );
}

#[test]
fn a_double_click_on_the_badge_of_the_clip_group_selects_the_clip_group() {
    let (mut window, clip, group) = window_with_a_clip_that_is_in_a_clip_group();
    let badge = middle_of(&window, THE_CLIP_GROUP_BADGE);

    window.click(badge);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(clip)]);
    window.double_click(badge);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(group)]);
    assert!(is_shown(&window, "Clip group"));
}

#[test]
fn a_track_that_is_in_a_track_group_has_the_badge_of_the_group_last_in_its_chain() {
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30)], &[(20, 30)]]));
    let tracks: Vec<TrackId> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    window
        .application_mut()
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Track(tracks[0]), Member::Track(tracks[1])],
        )
        .unwrap();
    select_only(&mut window, SelectionItem::Track(tracks[0]));

    assert_eq!(
        the_chain_of(&window, Member::Track(tracks[0])),
        ["track group Track group 1"]
    );
    assert!(is_shown(&window, "Track group: Track group 1"));
}

// --- the effects of a group ---

/// The effects that the group `group` has, as kinds.
fn effects_of_the_group(window: &Window, group: GroupId) -> Vec<EffectKind> {
    window
        .application()
        .project()
        .group(group)
        .unwrap()
        .effects()
        .iter()
        .map(|effect| effect.kind())
        .collect()
}

fn the_value_of_the_hue_shift_of_the_group(window: &Window, group: GroupId) -> Option<f32> {
    window
        .application()
        .project()
        .group(group)
        .unwrap()
        .effects()
        .iter()
        .find(|effect| effect.kind() == EffectKind::HueShift)
        .and_then(|effect| match effect.value_of(PropertyId::HueShift) {
            Some(PropertyValue::Number(degrees)) => Some(*degrees),
            _ => None,
        })
}

fn put_the_hue_shift_on_the_selected_group_with_the_plus_button(window: &mut Window) {
    let plus = middle_of(window, "Add an effect");
    window.click(plus);
    let item = middle_of(window, HUE_SHIFT);
    window.click(item);
}

#[test]
fn a_group_has_a_plus_button_that_puts_an_effect_on_it_with_a_box_of_its_own_and_a_value_that_is_not_animated() {
    let (mut window, tag, _) = super::group_box_tests::window_with_a_tag_of_the_first_two_clips_selected();
    assert!(is_shown(&window, "Add an effect"));
    assert!(!is_shown(&window, "Turn"), "no effect, no controls");

    put_the_hue_shift_on_the_selected_group_with_the_plus_button(&mut window);

    assert_eq!(effects_of_the_group(&window, tag), [EffectKind::HueShift]);
    assert_eq!(
        the_value_of_the_hue_shift_of_the_group(&window, tag),
        Some(0.0),
        "as it begins"
    );
    assert!(is_shown(&window, HUE_SHIFT), "its box");
    assert!(is_shown(&window, "Turn"), "and the control of its value");
    assert!(
        window.harness.query_all_by_label_contains("stopwatch").next().is_none(),
        "the values of the effects of a group have no keyframes"
    );
    assert!(
        window.rectangle_of(HUE_SHIFT).top() > window.rectangle_of("Tag").bottom(),
        "the box of the effect is under the box of the group"
    );

    window.set_slider("Turn", 90.0);
    assert_eq!(the_value_of_the_hue_shift_of_the_group(&window, tag), Some(90.0));

    undo(&mut window);
    assert_eq!(
        the_value_of_the_hue_shift_of_the_group(&window, tag),
        Some(0.0),
        "changing it is one step"
    );
    undo(&mut window);
    assert_eq!(effects_of_the_group(&window, tag), vec![], "and so is putting it on");
}

#[test]
fn the_menu_of_the_box_of_an_effect_of_a_group_deletes_the_effect() {
    let (mut window, tag, _) = super::group_box_tests::window_with_a_tag_of_the_first_two_clips_selected();
    put_the_hue_shift_on_the_selected_group_with_the_plus_button(&mut window);
    let heading = middle_of(&window, HUE_SHIFT);
    window.right_click(heading);

    window.harness.get_by_label("Delete effect").click();
    window.settle();

    assert_eq!(effects_of_the_group(&window, tag), vec![]);
    assert!(!is_shown(&window, HUE_SHIFT));
}

#[test]
fn a_clip_group_and_a_track_group_have_the_plus_button_too() {
    let (mut window, _, group) = window_with_a_clip_that_is_in_a_clip_group();
    select_only(&mut window, SelectionItem::Group(group));
    put_the_hue_shift_on_the_selected_group_with_the_plus_button(&mut window);
    assert_eq!(effects_of_the_group(&window, group), [EffectKind::HueShift]);

    let track = window.application().project().tracks()[0].id();
    let track_group = window
        .application_mut()
        .project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(track_group));
    put_the_hue_shift_on_the_selected_group_with_the_plus_button(&mut window);
    assert_eq!(effects_of_the_group(&window, track_group), [EffectKind::HueShift]);
}

#[test]
fn an_effect_of_a_group_is_dragged_by_its_heading_like_any_other_and_a_group_in_a_tag_has_the_badge_of_the_tag_in_its_chain()
 {
    let (mut window, _, group) = window_with_a_clip_that_is_in_a_clip_group();
    let clips = the_three_clips(&window);
    let tag = window
        .application_mut()
        .project
        .create_group(GroupKind::Tag, &[Member::Group(group), Member::Clip(clips[2])])
        .unwrap();
    select_only(&mut window, SelectionItem::Group(group));
    put_the_hue_shift_on_the_selected_group_with_the_plus_button(&mut window);
    assert_eq!(
        the_chain_of(&window, Member::Group(group)),
        ["tag Tag 1", "hue shift"],
        "the badge of the tag came first: it was there before the effect was"
    );

    let from = middle_of(&window, HUE_SHIFT);
    let above_the_badge = middle_of(&window, "Tag: Tag 1") - egui::vec2(0.0, 12.0);
    window.drag(from, above_the_badge, PointerButton::Primary);

    assert_eq!(the_chain_of(&window, Member::Group(group)), ["hue shift", "tag Tag 1"]);
    let badge = middle_of(&window, "Tag: Tag 1");
    window.double_click(badge);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(tag)]);
}

// --- the keyboard ---

/// Presses Tab many times, and says whether the keyboard focus was ever on something that has the point `point` on the screen in it.
fn tab_ever_stops_at_what_has(window: &mut Window, point: egui::Pos2) -> bool {
    let mut stopped = false;
    for _ in 0..60 {
        window.press_key(egui::Key::Tab);
        let focused = window.harness.ctx.memory(|memory| memory.focused());
        if let Some(id) = focused
            && let Some(response) = window.harness.ctx.read_response(id)
            && response.rect.contains(point)
        {
            stopped = true;
        }
    }
    stopped
}

#[test]
fn the_tab_key_does_not_stop_at_a_badge_or_at_the_heading_of_the_box_of_an_effect_which_have_nothing_to_do_for_the_keyboard()
 {
    let (mut window, clip, _) = window_with_a_clip_that_is_in_a_tag_and_has_an_effect();
    let track = window.application().project().tracks()[0].id();
    window
        .application_mut()
        .project
        .add_effect(AnimationTarget::Track(track), EffectKind::HueShift)
        .unwrap();
    window.settle();
    assert_eq!(
        the_chain_of(&window, Member::Clip(clip)),
        ["tag Tag 1", "hue shift", "track track 0"],
        "the setup: a badge of a tag, an effect and the badge of the track"
    );
    let points = [
        middle_of(&window, "Tag: Tag 1"),
        middle_of(&window, HUE_SHIFT),
        middle_of(&window, "Track: track 0 (1 effect)"),
    ];

    for point in points {
        assert!(
            !tab_ever_stops_at_what_has(&mut window, point),
            "the focus was on what is at {point:?}"
        );
    }
}

#[test]
fn the_tab_key_does_not_stop_at_the_badge_of_a_clip_group_which_is_not_moved_either() {
    let (mut window, _, _) = window_with_a_clip_that_is_in_a_clip_group();
    let badge = middle_of(&window, THE_CLIP_GROUP_BADGE);

    assert!(!tab_ever_stops_at_what_has(&mut window, badge));
}

#[test]
fn the_tab_key_does_not_stop_at_the_bar_of_the_box_of_a_group_either() {
    let (mut window, _, _) = super::group_box_tests::window_with_a_tag_of_the_first_two_clips_selected();
    let on_the_bar = middle_of(&window, "Tag");

    assert!(!tab_ever_stops_at_what_has(&mut window, on_the_bar));
}
