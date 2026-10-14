//! Moving the left or the right edge of a clip group, by taking hold of it where it is drawn: the clips that are inside the frames the edge is
//! then put at are in the group, and the group lets go of those that are not (see [`crate::project::Project::set_the_frames_of_the_clip_group`]).
//!
//! The pointer is a double arrow pointing left and right while it is over an edge. Taking hold of the edge and moving the pointer moves it: it
//! goes along with the pointer where it can be put (not across a clip, or a group of clips, or where the rules of groups would not have it), and
//! where the pointer is across one it goes, quickly, to the nearest place that it can be put, and is held there until the pointer is at a place
//! that it can be put again. Nothing is changed until the button is let go: then the group is given the frames, as one edit, and the edge
//! goes, quickly, to where the group has it then, which is as far as the clips that are in it (the rectangle is the smallest that has all of
//! them), and not where it was let go if that was further than that.

use std::ops::RangeInclusive;

use egui::{CursorIcon, PointerButton, Pos2, Rect, Sense, Ui, pos2};

use super::group_outlines::OutlineOfAGroup;
use super::strip_view::StripView;
use super::view_state::GroupEdgeMove;
use super::{TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::project::{ClipGroupEdge, GroupKind};

/// How far to the side of an edge it is taken hold of, in points: the edge is taken hold of in a stripe that is twice as wide.
const HALF_THE_WIDTH_OF_THE_STRIPE: f32 = 3.0;

/// How far from the top and the bottom of a group its edges are not taken hold of, where its corners are round, in points.
const CORNERS: f32 = 10.0;

/// How long it takes a shown edge to go most of the way to where it goes, in seconds.
const SECONDS_TO_GO_THERE: f32 = 0.045;

/// How far from the pointer a shown edge that follows it can be before it is not a following but a going, in frames.
const MOST_IT_FOLLOWS_BEHIND: f64 = 8.0;

/// The frame of `places` that is the nearest to `wanted`: `wanted` itself if it is in one of them. `None` if there are no places.
pub(super) fn nearest_place(places: &[RangeInclusive<u64>], wanted: u64) -> Option<u64> {
    places
        .iter()
        .map(|range| wanted.clamp(*range.start(), *range.end()))
        .min_by_key(|place| place.abs_diff(wanted))
}

/// Where a shown edge is after `seconds` of going towards `target`, from `shown`: most of the way, and all of it when it is near.
pub(super) fn gone_towards(shown: f64, target: f64, seconds: f32) -> f64 {
    let next = shown + (target - shown) * (1.0 - f64::from(-seconds / SECONDS_TO_GO_THERE).exp());
    if (next - target).abs() < 0.02 { target } else { next }
}

/// Moves the edge that is being moved, if one is: follows the pointer, and when the button is let go gives the group the frames (a command is
/// added) and goes to where the group has the edge then. `first_view` is any of the strips, for where frames are on the screen.
pub(super) fn keep_moving(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    first_view: Option<&StripView>,
    commands: &mut Vec<Command>,
) {
    let Some(moving) = state.group_edge_move.as_mut() else {
        return;
    };
    let context = ui.ctx();
    let seconds = context.input(|input| input.stable_dt);
    let (Some(view), Some(rectangle)) = (first_view, content.project.rectangle_of_the_group(moving.group)) else {
        state.group_edge_move = None;
        return;
    };
    let where_the_group_has_it = match moving.edge {
        ClipGroupEdge::Left => rectangle.first_frame,
        ClipGroupEdge::Right => rectangle.end_frame,
    };
    if moving.is_settling {
        moving.shown = gone_towards(moving.shown, where_the_group_has_it as f64, seconds);
        if moving.shown == where_the_group_has_it as f64 {
            state.group_edge_move = None;
        } else {
            context.request_repaint();
        }
        return;
    }
    if !content.can_edit {
        state.group_edge_move = None;
        return;
    }
    context.set_cursor_icon(CursorIcon::ResizeHorizontal);
    if let Some(pointer) = context.input(|input| input.pointer.interact_pos()) {
        let wanted = view.fractional_frame_at(pointer.x).max(0.0);
        let rounded = wanted.round() as u64;
        if let Some(place) = nearest_place(&moving.places, rounded) {
            moving.held_at = place;
        }
        let range_it_is_in = moving.places.iter().find(|range| range.contains(&rounded));
        moving.shown = match range_it_is_in {
            Some(range) if (wanted - moving.shown).abs() <= MOST_IT_FOLLOWS_BEHIND => {
                wanted.clamp(*range.start() as f64, *range.end() as f64)
            }
            _ => gone_towards(moving.shown, moving.held_at as f64, seconds),
        };
        if moving.shown != wanted {
            context.request_repaint();
        }
    }
    if !context.input(|input| input.pointer.primary_down()) {
        if moving.held_at != where_the_group_has_it {
            let (first_frame, end_frame) = match moving.edge {
                ClipGroupEdge::Left => (moving.held_at, rectangle.end_frame),
                ClipGroupEdge::Right => (rectangle.first_frame, moving.held_at),
            };
            commands.push(Command::SetTheFramesOfAClipGroup {
                group: moving.group,
                first_frame,
                end_frame,
            });
        }
        moving.is_settling = true;
        context.request_repaint();
    }
}

/// The stripes where the edges of the clip groups of `outlines` can be taken hold of, and what is done there: the pointer is a double arrow
/// over one, and when the button is pressed on one and the pointer moves the edge is taken hold of.
pub(super) fn show_the_handles(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    outlines: &[OutlineOfAGroup],
    strips_area: Rect,
) {
    if !content.can_edit || state.item_drag.is_some() || state.is_in_a_gesture_other_than_item_dragging() {
        return;
    }
    for outline in outlines.iter().filter(|outline| outline.kind == GroupKind::ClipGroup) {
        let points: Vec<Pos2> = outline.loops.iter().flatten().copied().collect();
        if points.is_empty() {
            continue;
        }
        let bounds = Rect::from_points(&points);
        // (A row is never shorter than the least a track can be, so there is a stretch of the edge between the corners.)
        let (top, bottom) = (bounds.top() + CORNERS, bounds.bottom() - CORNERS);
        for edge in [ClipGroupEdge::Left, ClipGroupEdge::Right] {
            let x = match edge {
                ClipGroupEdge::Left => bounds.left(),
                ClipGroupEdge::Right => bounds.right(),
            };
            let stripe = Rect::from_min_max(
                pos2(x - HALF_THE_WIDTH_OF_THE_STRIPE, top),
                pos2(x + HALF_THE_WIDTH_OF_THE_STRIPE, bottom),
            )
            .intersect(strips_area);
            if stripe.width() <= 0.0 || stripe.height() <= 0.0 {
                continue;
            }
            let response = ui.interact(
                stripe,
                ui.id()
                    .with(("an edge of a clip group", outline.group, edge == ClipGroupEdge::Left)),
                Sense::drag(),
            );
            if response.hovered() || response.is_pointer_button_down_on() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }
            if response.drag_started_by(PointerButton::Primary)
                && let Some(rectangle) = content.project.rectangle_of_the_group(outline.group)
            {
                let frame = match edge {
                    ClipGroupEdge::Left => rectangle.first_frame,
                    ClipGroupEdge::Right => rectangle.end_frame,
                };
                state.group_edge_move = Some(GroupEdgeMove {
                    group: outline.group,
                    edge,
                    places: content
                        .project
                        .places_for_an_edge_of_the_clip_group(outline.group, edge),
                    shown: frame as f64,
                    held_at: frame,
                    is_settling: false,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
