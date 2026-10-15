// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the names of groups on the timeline, in bars that are shown while the pointer is over a group: where they are, what they are like,
//! and what clicking and dragging them does.

use egui::epaint::{RectShape, TextShape};
use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Shape};

use super::clip_drag_tests::{in_frame, project_of_tracks, window_with_three_clips};
use super::group_box_tests::{select_only, the_three_clips};
use super::group_outline_tests::{
    bounds_of, colour_of_group, outlines_drawn_in, window_with_a_clip_group_of_the_first_two_clips,
};
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::timeline_panel::{GripDotArrangement, HEIGHT_OF_A_GROUP_TITLE_BAR};
use crate::project::{GroupColour, GroupId, GroupKind, Member};

/// The bars that were drawn in the last frame, in the colour of `group`, as the shapes they were drawn as: a bar is a filled rectangle that is
/// as tall as one is.
fn bar_shapes_drawn_in(window: &Window, group: GroupId) -> Vec<RectShape> {
    let colour = colour_of_group(window, group);
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle)
                if rectangle.fill == colour && (rectangle.rect.height() - HEIGHT_OF_A_GROUP_TITLE_BAR).abs() < 0.01 =>
            {
                Some(rectangle.clone())
            }
            _ => None,
        })
        .collect()
}

fn bars_drawn_in(window: &Window, group: GroupId) -> Vec<Rect> {
    bar_shapes_drawn_in(window, group)
        .into_iter()
        .map(|shape| shape.rect)
        .collect()
}

/// The words as they were written in the last frame: where, and in what colour.
fn writing_of(window: &Window, words: &str) -> TextShape {
    window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text() == words => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{words:?} was not written"))
}

/// Whether `words` were written in the last frame.
fn were_written(window: &Window, words: &str) -> bool {
    window
        .shapes_drawn()
        .into_iter()
        .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == words))
}

fn name_of(window: &Window, group: GroupId) -> String {
    window.application().project().group(group).unwrap().name().to_owned()
}

/// A point on the first clip, which is in a clip group of the first two (see [`window_with_a_clip_group_of_the_first_two_clips`]).
fn on_the_first_clip(window: &Window) -> Pos2 {
    in_frame(window, 0, 35)
}

fn centre_of_the_bar(window: &Window, group: GroupId) -> Pos2 {
    bars_drawn_in(window, group)[0].center()
}

fn starts_on_track(window: &Window, position: usize) -> Vec<u64> {
    window.application().project().tracks()[position]
        .placements()
        .map(|placement| placement.first_timeline_frame.value())
        .collect()
}

#[test]
fn the_name_of_a_group_is_not_shown_until_the_pointer_is_over_the_group_and_goes_when_the_pointer_does() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    assert!(bars_drawn_in(&window, group).is_empty(), "the pointer is not over it");

    window.move_pointer_to(on_the_first_clip(&window));
    assert_eq!(bars_drawn_in(&window, group).len(), 1);
    assert!(were_written(&window, &name_of(&window, group)));

    // Over the clip that is not in the group.
    window.move_pointer_to(in_frame(&window, 0, 125));
    assert!(bars_drawn_in(&window, group).is_empty());
    assert!(!were_written(&window, &name_of(&window, group)));
}

#[test]
fn the_name_of_a_clip_group_is_in_a_bar_in_its_colour_on_the_middle_of_its_top_edge_with_dots_at_its_left_end() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));

    let bar = bars_drawn_in(&window, group)[0];

    let outline = bounds_of(&outlines_drawn_in(&window, colour_of_group(&window, group))[0]);
    assert!(
        (bar.center().x - outline.center().x).abs() < 1.5,
        "in the middle: {bar:?} and {outline:?}"
    );
    assert!(
        (bar.bottom() - outline.top()).abs() < 0.5,
        "on the edge, over it: {bar:?} and {outline:?}"
    );
    assert!(
        bar.width() < outline.width(),
        "it does not run the whole width of a group that is as wide as this"
    );
    let dots = window.grip_dots_drawn(GripDotArrangement::FOR_A_RACK);
    assert!(
        dots.iter()
            .any(|dot| bar.contains_rect(*dot) && dot.center().x < bar.center().x),
        "dots in the bar, at the left: {dots:?}"
    );
}

#[test]
fn a_bar_for_a_name_that_is_wider_than_the_group_goes_out_beyond_both_sides_of_it() {
    // Two clips of 5 frames, 3 frames apart: the group is 13 frames wide, which is less than the name and its dots need.
    let mut window = Window::showing(project_of_tracks(&[&[(20, 5), (3, 5)]]));
    let clips = [window.clip(0, 0), window.clip(0, 1)];
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();
    window.move_pointer_to(in_frame(&window, 0, 22));

    let bar = bars_drawn_in(&window, group)[0];

    let outline = bounds_of(&outlines_drawn_in(&window, colour_of_group(&window, group))[0]);
    assert!(bar.width() > outline.width(), "{bar:?} and {outline:?}");
    assert!(
        bar.left() < outline.left() && bar.right() > outline.right(),
        "out at both sides"
    );
    assert!((bar.center().x - outline.center().x).abs() < 1.5, "in the middle");
}

#[test]
fn a_name_that_is_longer_than_a_bar_is_wide_enough_for_is_cut_off_with_an_ellipsis() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.do_this(crate::application::commands::Command::RenameGroup {
        group,
        name: "A very long name for a group, which is longer than any name that is written in a bar in the timeline can be, as a name that long would take up the room of everything else".to_owned(),
    });
    window.settle();
    window.move_pointer_to(on_the_first_clip(&window));

    let bar = bars_drawn_in(&window, group)[0];

    assert!(bar.width() < 300.0, "{bar:?}");
    let words = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text().starts_with("A very long") => Some(text.clone()),
            _ => None,
        })
        .expect("the words are written");
    assert!(words.galley.elided, "the words are cut off");
    let last_row = words.galley.rows.last().expect("the words have a row");
    assert_eq!(
        last_row.row.glyphs.last().map(|glyph| glyph.chr),
        Some('\u{2026}'),
        "and what is left of them ends with an ellipsis"
    );
}

#[test]
fn clicking_the_bar_selects_the_group_and_clicking_it_with_control_held_adds_it_to_what_is_selected() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let clips = the_three_clips(&window);
    select_only(&mut window, SelectionItem::Clip(clips[2]));
    window.move_pointer_to(on_the_first_clip(&window));
    let bar = centre_of_the_bar(&window, group);

    window.click(bar);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(group)]);

    // With another thing selected: control adds the group, and clicks it away again.
    select_only(&mut window, SelectionItem::Clip(clips[2]));
    window.move_pointer_to(on_the_first_clip(&window));
    let bar = centre_of_the_bar(&window, group);
    window.click_with_control(bar);
    assert!(
        window
            .application()
            .selected_items()
            .contains(&SelectionItem::Group(group))
    );
    assert!(
        window
            .application()
            .selected_items()
            .contains(&SelectionItem::Clip(clips[2]))
    );
}

#[test]
fn dragging_the_bar_moves_the_group_and_all_its_clips_along_the_timeline() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));
    let from = centre_of_the_bar(&window, group);
    let frames_along = 8.0;
    let to = Pos2::new(
        from.x + frames_along * (window.x_of_frame_on_the_strip(0, 1) - window.x_of_frame_on_the_strip(0, 0)),
        from.y,
    );

    window.drag(from, to, PointerButton::Primary);
    window.settle();

    // The clips were at 20 and 65, and have gone 8 frames, both, as one.
    let starts = starts_on_track(&window, 0);
    assert_eq!(
        starts[1] - starts[0],
        45,
        "they are as far apart as they were: {starts:?}"
    );
    assert!((27..=28).contains(&starts[0]), "about 8 frames later: {starts:?}");
    assert_eq!(starts[2], 110, "the clip that is not in the group did not move");
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Group(group)],
        "the group is selected by it"
    );
}

#[test]
fn dragging_the_bar_down_to_another_track_where_there_is_room_moves_the_group_to_it() {
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30), (15, 30)], &[]]));
    let clips = [window.clip(0, 0), window.clip(0, 1)];
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();
    window.move_pointer_to(in_frame(&window, 0, 35));
    let from = centre_of_the_bar(&window, group);
    let to = Pos2::new(from.x, window.strip_of_track(1).center().y);

    window.drag(from, to, PointerButton::Primary);
    window.settle();

    assert_eq!(
        starts_on_track(&window, 0),
        Vec::<u64>::new(),
        "both are on the second track now"
    );
    assert_eq!(
        starts_on_track(&window, 1),
        vec![20, 65],
        "at the same frames, as far apart"
    );
    assert_eq!(window.application().project().problems_with_the_groups(), vec![]);
}

#[test]
fn a_group_dragged_by_its_bar_stops_against_a_clip_that_is_in_the_way() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));
    let from = centre_of_the_bar(&window, group);
    let to = Pos2::new(from.x + 200.0, from.y);

    window.drag(from, to, PointerButton::Primary);
    window.settle();

    // The third clip is at 110: the second can go as far as 80 to 110, 15 frames later than it was.
    assert_eq!(starts_on_track(&window, 0), vec![35, 80, 110]);
}

#[test]
fn the_name_of_a_tag_is_shown_over_one_of_its_clips_and_dragging_it_moves_the_clips_that_are_in_it() {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let tag = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[0]), Member::Clip(clips[2])])
        .unwrap();
    window.settle();
    window.move_pointer_to(in_frame(&window, 0, 35));
    assert_eq!(bars_drawn_in(&window, tag).len(), 1);
    assert!(were_written(&window, &name_of(&window, tag)));
    let from = centre_of_the_bar(&window, tag);
    let to = Pos2::new(
        from.x - 3.0 * (window.x_of_frame_on_the_strip(0, 1) - window.x_of_frame_on_the_strip(0, 0)),
        from.y,
    );

    window.drag(from, to, PointerButton::Primary);
    window.settle();

    // The first clip goes back as far as 17 (3 frames earlier), the third goes with it, and the one between, which is not in the tag, stays.
    assert_eq!(starts_on_track(&window, 0), vec![17, 65, 107]);
}

#[test]
fn a_track_group_has_no_bar_and_a_group_that_is_not_under_the_pointer_has_none() {
    let mut window = window_with_three_clips();
    let track = window.application().project().tracks()[0].id();
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();
    window.settle();

    window.move_pointer_to(in_frame(&window, 0, 35));

    assert!(bars_drawn_in(&window, group).is_empty());
    assert!(!were_written(&window, &name_of(&window, group)));
}

#[test]
fn of_groups_one_in_another_the_name_that_is_shown_is_the_innermost_that_the_pointer_is_over() {
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
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), Member::Clip(clips[2])])
        .unwrap();
    window.settle();

    window.move_pointer_to(in_frame(&window, 0, 35));
    assert_eq!(
        bars_drawn_in(&window, inner).len(),
        1,
        "over a clip that is in the inner group"
    );
    assert!(bars_drawn_in(&window, outer).is_empty());

    window.move_pointer_to(in_frame(&window, 0, 125));
    assert_eq!(
        bars_drawn_in(&window, outer).len(),
        1,
        "over the clip that is only in the outer one"
    );
    assert!(bars_drawn_in(&window, inner).is_empty());
}

#[test]
fn no_name_is_shown_while_a_clip_is_being_dragged() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let from = in_frame(&window, 0, 125);

    window.drag_and_hold(from, in_frame(&window, 0, 70), PointerButton::Primary);
    window.move_pointer_to(on_the_first_clip(&window));

    assert!(bars_drawn_in(&window, group).is_empty());
    window.release(on_the_first_clip(&window), PointerButton::Primary);
}

#[test]
fn the_group_that_is_dragged_by_its_bar_goes_on_showing_the_bar_where_the_group_is() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));
    let from = centre_of_the_bar(&window, group);

    window.drag_and_hold(from, Pos2::new(from.x + 30.0, from.y), PointerButton::Primary);
    window.settle();

    let bar = bars_drawn_in(&window, group);
    assert_eq!(bar.len(), 1, "the bar is where the group is now");
    let outline = bounds_of(&outlines_drawn_in(&window, colour_of_group(&window, group))[0]);
    assert!((bar[0].center().x - outline.center().x).abs() < 1.5);
    window.release(Pos2::new(from.x + 30.0, from.y), PointerButton::Primary);
}

#[test]
fn the_dots_of_a_bar_are_at_its_left_end_and_the_name_is_after_them_with_room_between_and_as_much_room_at_the_right_end_as_at_the_left()
 {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));
    let bar = bars_drawn_in(&window, group)[0];
    let dots: Vec<Rect> = window
        .grip_dots_drawn(GripDotArrangement::FOR_A_RACK)
        .into_iter()
        .filter(|dot| bar.contains_rect(*dot))
        .collect();
    assert!(!dots.is_empty(), "the setup: dots in the bar");
    let dots_left = dots.iter().map(|dot| dot.left()).fold(f32::INFINITY, f32::min);
    let dots_right = dots.iter().map(|dot| dot.right()).fold(f32::NEG_INFINITY, f32::max);

    let writing = writing_of(&window, &name_of(&window, group));

    assert!(
        writing.pos.x >= dots_right + 3.0,
        "the name begins after the dots, with room between: {} and {dots_right}",
        writing.pos.x
    );
    let room_at_the_left = dots_left - bar.left();
    let room_at_the_right = bar.right() - (writing.pos.x + writing.galley.size().x);
    assert!(
        (room_at_the_left - room_at_the_right).abs() < 2.0,
        "the bar is as far from what is in it at one end as at the other: {room_at_the_left} and {room_at_the_right}"
    );
    assert!(
        writing.pos.y >= bar.top() && writing.pos.y + writing.galley.size().y <= bar.bottom(),
        "the name is in the bar, up and down"
    );
}

#[test]
fn a_bar_over_the_top_edge_of_its_group_is_a_capsule_round_at_the_top_and_flat_where_it_is_on_the_edge() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));

    let shapes = bar_shapes_drawn_in(&window, group);

    let rounding = shapes[0].corner_radius;
    assert!(
        f32::from(rounding.nw) * 2.0 >= HEIGHT_OF_A_GROUP_TITLE_BAR - 1.5,
        "the ends are as round as half the height: {rounding:?}"
    );
    assert_eq!(rounding.ne, rounding.nw, "both the same");
    assert_eq!((rounding.sw, rounding.se), (0, 0), "flat at the bottom, on the edge");
}

#[test]
fn the_name_in_a_bar_is_written_in_white_on_a_dark_colour_and_in_black_on_a_light_one() {
    for (colour, writing) in [
        (GroupColour::new(20, 30, 90), Color32::WHITE),
        (GroupColour::new(250, 245, 200), Color32::BLACK),
    ] {
        let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
        window.do_this(Command::SetGroupColour { group, colour });
        window.settle();
        window.move_pointer_to(on_the_first_clip(&window));

        let written = writing_of(&window, &name_of(&window, group));

        assert_eq!(written.galley.job.sections[0].format.color, writing, "on {colour:?}");
    }
}

#[test]
fn no_bar_is_shown_for_a_pointer_that_is_over_the_title_boxes_though_the_group_has_been_scrolled_under_them() {
    // A long timeline, scrolled until the group (frames 20 to 335) has all but its last frames under the title boxes.
    let mut window = Window::showing(project_of_tracks(&[&[(20, 300), (15, 300), (15, 30), (600, 30)]]));
    let clips = [window.clip(0, 0), window.clip(0, 1)];
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();
    let points_to_frame_320 = window.x_of_frame_on_the_strip(0, 320) - window.x_of_frame_on_the_strip(0, 0);
    window.application_mut().interface.timeline_view.scroll_offset = f64::from(points_to_frame_320);
    window.settle();

    window.move_pointer_to(in_frame(&window, 0, 330));
    assert_eq!(
        bars_drawn_in(&window, group).len(),
        1,
        "the setup: it is shown where the group can be seen"
    );

    let over_the_title_box = Pos2::new(
        window.title_box_of_track(0).center().x,
        window.strip_of_track(0).center().y,
    );
    window.move_pointer_to(over_the_title_box);

    assert!(bars_drawn_in(&window, group).is_empty());
    assert!(!were_written(&window, &name_of(&window, group)));
}

#[test]
fn while_a_group_is_dragged_by_its_bar_only_its_bar_is_shown_and_not_that_of_a_smaller_group_somewhere_else() {
    // (The smaller group, on the second track at frames 100 to 145, is in view: the window shows the first 170 frames or so.)
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30), (15, 30)], &[(100, 20), (5, 20)]]));
    let [first, second] = [window.clip(0, 0), window.clip(0, 1)];
    let [third, fourth] = [window.clip(1, 0), window.clip(1, 1)];
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let smaller = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(third), Member::Clip(fourth)])
        .unwrap();
    window.settle();
    window.move_pointer_to(in_frame(&window, 0, 35));
    let from = centre_of_the_bar(&window, group);

    window.drag_and_hold(from, Pos2::new(from.x + 30.0, from.y), PointerButton::Primary);
    window.settle();

    assert_eq!(bars_drawn_in(&window, group).len(), 1, "the one that is dragged");
    assert!(were_written(&window, &name_of(&window, group)));
    assert!(
        bars_drawn_in(&window, smaller).is_empty() && !were_written(&window, &name_of(&window, smaller)),
        "and not the other"
    );
    window.release(Pos2::new(from.x + 30.0, from.y), PointerButton::Primary);
}

#[test]
fn the_pointer_is_a_hand_over_a_bar_and_is_the_hand_that_holds_while_the_group_is_dragged_by_it() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.move_pointer_to(on_the_first_clip(&window));
    let from = centre_of_the_bar(&window, group);

    window.move_pointer_to(from);
    assert_eq!(window.harness.output().platform_output.cursor_icon, CursorIcon::Grab);

    window.drag_and_hold(from, Pos2::new(from.x + 30.0, from.y), PointerButton::Primary);
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        CursorIcon::Grabbing,
        "holding it"
    );
    window.release(Pos2::new(from.x + 30.0, from.y), PointerButton::Primary);
}

/// A window with a clip group of the first two of three clips on the first track, and nothing on the second.
fn window_with_a_clip_group_over_an_empty_track() -> (Window, GroupId) {
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30), (15, 30), (15, 30)], &[]]));
    let clips = [window.clip(0, 0), window.clip(0, 1)];
    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();
    (window, group)
}

fn on_the_right_edge_of_the_outline(window: &Window, group: GroupId) -> Pos2 {
    let outline = bounds_of(&outlines_drawn_in(window, colour_of_group(window, group))[0]);
    Pos2::new(outline.max.x, window.strip_of_track(0).center().y)
}

#[test]
fn no_bar_is_shown_while_a_rectangle_is_drawn_round_clips_or_a_track_is_made_taller_or_an_edge_is_moved() {
    // A rectangle: from an empty place on the second track to the first clip of the group.
    let (mut window, group) = window_with_a_clip_group_over_an_empty_track();
    window.drag_and_hold(
        in_frame(&window, 1, 100),
        in_frame(&window, 0, 35),
        PointerButton::Primary,
    );
    window.move_pointer_to(on_the_first_clip(&window));
    assert!(bars_drawn_in(&window, group).is_empty(), "a rectangle");
    window.release(on_the_first_clip(&window), PointerButton::Primary);

    // The boundary under the title box of the second track.
    let (mut window, group) = window_with_a_clip_group_over_an_empty_track();
    let boundary = Pos2::new(
        window.title_box_of_track(1).center().x,
        window.strip_of_track(1).bottom() + crate::application::theme::SPACE_BETWEEN_TRACKS / 2.0,
    );
    window.drag_and_hold(boundary, boundary + egui::vec2(0.0, 10.0), PointerButton::Primary);
    window.move_pointer_to(on_the_first_clip(&window));
    assert!(bars_drawn_in(&window, group).is_empty(), "a track made taller");
    window.release(on_the_first_clip(&window), PointerButton::Primary);

    // An edge of the group: the right one, which is at the end of the second clip.
    let (mut window, group) = window_with_a_clip_group_over_an_empty_track();
    let right_edge = on_the_right_edge_of_the_outline(&window, group);
    window.drag_and_hold(right_edge, right_edge + egui::vec2(10.0, 0.0), PointerButton::Primary);
    window.move_pointer_to(on_the_first_clip(&window));
    assert!(bars_drawn_in(&window, group).is_empty(), "an edge moved");
    window.release(on_the_first_clip(&window), PointerButton::Primary);
}

#[test]
fn no_bar_is_shown_while_a_message_is_up_and_nothing_can_be_edited() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Something".to_owned(),
        text: "Something happened.".to_owned(),
    });
    window.settle();

    window.move_pointer_to(on_the_first_clip(&window));

    assert!(bars_drawn_in(&window, group).is_empty());
}

#[test]
fn a_plain_click_on_the_bar_of_a_group_that_is_selected_with_another_thing_selects_only_the_group() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let clips = the_three_clips(&window);
    select_only(&mut window, SelectionItem::Clip(clips[2]));
    window.do_this(Command::SelectItems {
        items: vec![SelectionItem::Group(group)],
        change: SelectionChange::Toggle,
    });
    assert_eq!(
        window.application().selected_items().len(),
        2,
        "the setup: both are selected"
    );
    window.move_pointer_to(on_the_first_clip(&window));
    let bar = centre_of_the_bar(&window, group);

    window.click(bar);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Group(group)]);
}
