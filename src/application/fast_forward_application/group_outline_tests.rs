//! Tests of the outlines of groups as they are drawn in the window: where, in what colour, how thick, over what and under what, and where they
//! are cut off.

use egui::epaint::ColorMode;
use egui::{Color32, PointerButton, Pos2, Rect, Shape};

use super::clip_drag_tests::{in_frame, project_of_tracks, window_with_three_clips};
use super::group_box_tests::{colour_of, select_only, the_three_clips};
use super::test_window::Window;
use crate::application::selection::SelectionItem;
use crate::application::theme;
use crate::project::{GroupId, GroupKind, Member};

/// One outline as it was drawn: where it was cut off, its points and how thick it is.
pub(super) struct DrawnOutline {
    pub(super) cut_off_at: Rect,
    pub(super) points: Vec<Pos2>,
    pub(super) thickness: f32,
    /// Where in the order of what was drawn it was.
    pub(super) place: usize,
}

/// Every closed line of `colour` that was drawn in the last frame.
pub(super) fn outlines_drawn_in(window: &Window, colour: Color32) -> Vec<DrawnOutline> {
    window
        .clipped_shapes_drawn()
        .into_iter()
        .enumerate()
        .filter_map(|(place, (cut_off_at, shape))| match shape {
            Shape::Path(path) if path.closed && path.stroke.color == ColorMode::Solid(colour) => Some(DrawnOutline {
                cut_off_at,
                points: path.points.clone(),
                thickness: path.stroke.width,
                place,
            }),
            _ => None,
        })
        .collect()
}

pub(super) fn colour_of_group(window: &Window, group: GroupId) -> Color32 {
    let colour = colour_of(window, group);
    Color32::from_rgb(colour.red, colour.green, colour.blue)
}

pub(super) fn bounds_of(outline: &DrawnOutline) -> Rect {
    Rect::from_points(&outline.points)
}

/// The window of `window_with_three_clips` with the first two clips in a clip group.
pub(super) fn window_with_a_clip_group_of_the_first_two_clips() -> (Window, GroupId) {
    let mut window = window_with_three_clips();
    let clips = the_three_clips(&window);
    let group = window
        .application_mut()
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();
    (window, group)
}

#[test]
fn a_clip_group_has_an_outline_round_its_clips_in_its_colour_two_points_thick() {
    let (window, group) = window_with_a_clip_group_of_the_first_two_clips();

    let outlines = outlines_drawn_in(&window, colour_of_group(&window, group));

    assert_eq!(outlines.len(), 1, "one loop");
    let outline = &outlines[0];
    assert_eq!(outline.thickness, 2.0);
    // The clips are at 20 to 50 and 65 to 95: the outline is 3 points out from them, along the strip of the one track.
    let bounds = bounds_of(outline);
    let (left, right) = (
        window.x_of_frame_on_the_strip(0, 20),
        window.x_of_frame_on_the_strip(0, 95),
    );
    let strip = window.strip_of_track(0);
    assert!((bounds.min.x - (left - 3.0)).abs() < 0.3, "{bounds:?} and {left}");
    assert!((bounds.max.x - (right + 3.0)).abs() < 0.3, "{bounds:?} and {right}");
    assert!((bounds.min.y - (strip.top() - 3.0)).abs() < 0.3);
    assert!((bounds.max.y - (strip.bottom() + 3.0)).abs() < 0.3);
}

#[test]
fn the_outline_of_a_group_that_is_selected_is_three_points_thick() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    select_only(&mut window, SelectionItem::Group(group));

    let outlines = outlines_drawn_in(&window, colour_of_group(&window, group));

    assert_eq!(outlines.len(), 1);
    assert_eq!(outlines[0].thickness, 3.0);

    let the_last_clip = the_three_clips(&window)[2];
    select_only(&mut window, SelectionItem::Clip(the_last_clip));
    assert_eq!(
        outlines_drawn_in(&window, colour_of_group(&window, group))[0].thickness,
        2.0,
        "and thinner when it is not"
    );
}

#[test]
fn an_outline_is_drawn_over_the_strips_and_the_clips_and_under_the_playhead() {
    let (window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let outline = &outlines_drawn_in(&window, colour_of_group(&window, group))[0];

    let drawn = window.clipped_shapes_drawn();
    let strip_backgrounds: Vec<usize> = drawn
        .iter()
        .enumerate()
        .filter(|(_, (_, shape))| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::TRACK_BACKGROUND))
        .map(|(place, _)| place)
        .collect();
    assert!(!strip_backgrounds.is_empty(), "the setup: the strip is drawn");
    assert!(
        strip_backgrounds.iter().all(|place| *place < outline.place),
        "over the strips, and so over the clips on them"
    );
    let playhead = drawn
        .iter()
        .position(|(_, shape)| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == theme::PLAYHEAD))
        .expect("the playhead is drawn");
    assert!(outline.place < playhead, "under the playhead");
}

#[test]
fn an_outline_of_a_clip_group_is_cut_off_at_the_strips_and_does_not_go_over_the_boxes_of_the_titles() {
    let (window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let outline = &outlines_drawn_in(&window, colour_of_group(&window, group))[0];
    let strip = window.strip_of_track(0);

    assert!(
        outline.cut_off_at.left() >= strip.left() - 0.5,
        "{:?} and {strip:?}",
        outline.cut_off_at
    );
    assert!(outline.cut_off_at.right() <= strip.right() + 0.5);
}

#[test]
fn the_top_of_an_outline_round_the_first_row_is_not_cut_off_where_it_goes_a_little_way_above_the_row() {
    let (window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let outline = &outlines_drawn_in(&window, colour_of_group(&window, group))[0];

    // (The group is on the one track, which is the first row: its outline goes 3 points above the strip, over the bottom of the ruler.)
    assert!(bounds_of(outline).min.y < window.strip_of_track(0).top());
    assert!(
        outline.cut_off_at.top() <= bounds_of(outline).min.y - 1.0,
        "{:?} and {:?}",
        outline.cut_off_at,
        bounds_of(outline)
    );
    assert!(
        outline.cut_off_at.top() >= window.rectangle_of("Ruler").top(),
        "but not over more of the timeline than the ruler"
    );
    // (As far above the first row as the outline of a group that has two levels of groups in it goes, 3 + 2 * 4 points, and the line, 3.)
    assert!(
        (outline.cut_off_at.top() - (window.strip_of_track(0).top() - 14.0)).abs() < 0.5,
        "{:?} and the first row at {}",
        outline.cut_off_at,
        window.strip_of_track(0).top()
    );
}

#[test]
fn an_outline_of_a_track_group_reaches_left_over_the_boxes_of_the_titles_and_is_not_cut_off_there() {
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30)], &[(20, 30)]]));
    let tracks: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    let group = window
        .application_mut()
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Track(tracks[0]), Member::Track(tracks[1])],
        )
        .unwrap();
    window.settle();

    let outlines = outlines_drawn_in(&window, colour_of_group(&window, group));

    assert_eq!(outlines.len(), 1);
    let bounds = bounds_of(&outlines[0]);
    let first_title_box = window.box_of_track(0);
    assert!(
        bounds.min.x <= first_title_box.left() - 2.0,
        "{bounds:?} and the box {first_title_box:?}"
    );
    assert!(
        outlines[0].cut_off_at.left() <= bounds.min.x + 0.5,
        "it is not cut off at the strips"
    );
    assert!(bounds.min.y < window.strip_of_track(0).top() && bounds.max.y > window.strip_of_track(1).bottom());
}

#[test]
fn a_tag_has_an_outline_that_goes_round_clips_that_are_on_different_tracks() {
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30)], &[(100, 30)]]));
    let clips = [window.clip(0, 0), window.clip(1, 0)];
    let tag = window
        .application_mut()
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[0]), Member::Clip(clips[1])])
        .unwrap();
    window.settle();

    let outlines = outlines_drawn_in(&window, colour_of_group(&window, tag));

    assert_eq!(outlines.len(), 1);
    let bounds = bounds_of(&outlines[0]);
    assert!(bounds.contains(window.middle_of_the_strip_at(0, 30)));
    assert!(bounds.contains(window.middle_of_the_strip_at(1, 115)));
    assert!(bounds.min.x < window.x_of_frame_on_the_strip(0, 20));
}

#[test]
fn the_outlines_are_not_drawn_while_a_track_is_being_dragged_to_another_place() {
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30)], &[(20, 30)]]));
    let tracks: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    let group = window
        .application_mut()
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Track(tracks[0]), Member::Track(tracks[1])],
        )
        .unwrap();
    window.settle();
    assert_eq!(
        outlines_drawn_in(&window, colour_of_group(&window, group)).len(),
        1,
        "the setup"
    );
    let handle = window.box_of_track(0);

    // The box of the front track is taken hold of by its top, where the grip dots are, and dragged down a little.
    let from = Pos2::new(handle.center().x, handle.top() + 8.0);
    window.drag_and_hold(from, from + egui::vec2(0.0, 30.0), PointerButton::Primary);
    window.settle();

    assert!(outlines_drawn_in(&window, colour_of_group(&window, group)).is_empty());
    window.release(from + egui::vec2(0.0, 30.0), PointerButton::Primary);
    window.settle();
    assert_eq!(
        outlines_drawn_in(&window, colour_of_group(&window, group)).len(),
        1,
        "and they come back"
    );
}

#[test]
fn the_outline_of_a_clip_group_follows_its_clips_when_they_are_dragged() {
    let (mut window, group) = window_with_a_clip_group_of_the_first_two_clips();
    let before = bounds_of(&outlines_drawn_in(&window, colour_of_group(&window, group))[0]);

    // The last clip is not in the group: the first two are dragged together by a drag of the selection of both (the clips at 20 and 65).
    let clips = the_three_clips(&window);
    window.do_this(crate::application::commands::Command::select_clips(
        vec![clips[0], clips[1]],
        crate::application::selection::SelectionChange::Replace,
    ));
    window.drag(
        in_frame(&window, 0, 35),
        in_frame(&window, 0, 40),
        PointerButton::Primary,
    );

    let after = bounds_of(&outlines_drawn_in(&window, colour_of_group(&window, group))[0]);
    let moved_by = after.min.x - before.min.x;
    assert!(moved_by > 1.0, "{before:?} and {after:?}");
    assert!((after.width() - before.width()).abs() < 0.5, "it is as wide as it was");
}
