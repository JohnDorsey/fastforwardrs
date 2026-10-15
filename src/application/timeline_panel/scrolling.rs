// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the strips are scrolled to, and how far they are zoomed, and how far the rows are zoomed up and down.

use egui::{Context, Rect, Ui};

use super::rows::RowsLayout;
use super::view_history::ViewAction;
use super::view_state::TimelineViewState;
use super::{TimelineContent, tallest_rows_area};
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::Project;

/// Brings the view up to date for this frame, and returns the scale the strips are drawn at.
///
/// * The mouse wheel with Ctrl held (or a pinch on a trackpad) zooms the strips in and out while the
///   pointer is over them, keeping the moment under the pointer where it is. (With Alt held as well it zooms the rows up and
///   down instead: see [`zoom_the_rows`].)
/// * The strips are kept scrolled so that the current frame is in the middle of the window whenever
///   the current frame changes, and the mouse wheel scrolls them in between. (The one exception is a
///   frame the user has just picked on the ruler: they pointed at it where it is, so the strips
///   stay still under their pointer.)
/// * With Alt held the wheel scrolls the rows up and down instead.
pub(super) fn update_view(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    view_width: f64,
) -> TimelineGeometry {
    let total_frames = content.project.total_frame_count();
    let mut geometry = state.geometry(content.geometry);
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let pointer_is_in = |area: Option<Rect>| area.zip(pointer).is_some_and(|(area, pointer)| area.contains(pointer));

    // (Along the strips only: the zoom that is up and down alone, with Alt held, does not zoom them along.)
    let zoom_factor = f64::from(ui.input(|input| input.zoom_delta_2d()).x);
    if zoom_factor != 1.0
        && let (Some(strips), Some(pointer)) = (state.strips_area_in_the_last_frame, pointer)
        && strips.contains(pointer)
    {
        state.note_that_the_user_changes_the_view(ViewAction::Zooming);
        let anchor_in_the_view = f64::from(pointer.x - strips.left());
        let (zoomed, scroll_offset) = geometry.zoomed_about(state.scroll_offset, zoom_factor, anchor_in_the_view);
        geometry = zoomed;
        state.scroll_offset = scroll_offset;
        state.points_per_second = Some(zoomed.points_per_second());
    }

    let picked_on_the_ruler = state.frame_picked_on_the_ruler.take() == Some(content.current_frame);
    if state.last_followed_frame != Some(content.current_frame) {
        if !picked_on_the_ruler {
            state.scroll_offset = geometry.scroll_offset_to_centre(content.current_frame, view_width, total_frames);
        }
        state.last_followed_frame = Some(content.current_frame);
    }
    let (scrolled, alt_is_held) = ui.input(|input| (input.smooth_scroll_delta, input.modifiers.alt));
    if alt_is_held {
        // With Alt held the wheel scrolls the rows up and down, over the title boxes as well.
        if pointer_is_in(state.timeline_area_in_the_last_frame) && scrolled.y != 0.0 {
            state.note_that_the_user_changes_the_view(ViewAction::Scrolling);
            state.rows_scroll_offset -= scrolled.y;
        }
    } else if pointer_is_in(state.strips_area_in_the_last_frame) && scrolled.x + scrolled.y != 0.0 {
        // Scrolling either way (a wheel up and down, or a trackpad sideways) moves the strips.
        state.note_that_the_user_changes_the_view(ViewAction::Scrolling);
        state.scroll_offset -= f64::from(scrolled.x + scrolled.y);
    }
    state.scroll_offset = geometry.clamp_scroll_offset(state.scroll_offset, view_width, total_frames);
    geometry
}

/// Zooms the rows up or down when the wheel is turned over the timeline with Ctrl and Alt held (or a pinch that is up and down
/// alone is made), by as much as the wheel is turned, and keeps the part of the rows that is under the pointer where it is on the
/// screen. This is done before the panel the timeline is on is made as tall as it is going to be (see [`super::prepare`]), so that
/// the panel is as tall as the rows are in the frame that they are zoomed in.
///
/// Nothing is done while a track is being made taller or shorter or is being moved about in the list: what is drawn is where
/// the pointer has put it, by sums that the zoom would make wrong.
pub(super) fn zoom_the_rows(context: &Context, project: &Project, state: &mut TimelineViewState) {
    if state.track_resize.is_some() || state.track_reorder.is_some() {
        return;
    }
    let zoom = context.input(|input| input.zoom_delta_2d());
    // (Ctrl with the wheel zooms along as well as up and down, which is a zoom of the strips: the zoom of the rows is the one that
    // is up and down alone.)
    if zoom.x != 1.0 || zoom.y == 1.0 {
        return;
    }
    let pointer = context.input(|input| input.pointer.hover_pos());
    let (Some(timeline), Some(view), Some(pointer)) = (
        state.timeline_area_in_the_last_frame,
        state.rows_view_in_the_last_frame,
        pointer,
    ) else {
        return;
    };
    if !timeline.contains(pointer) {
        return;
    }
    let tallest_track = project
        .tracks()
        .iter()
        .map(|track| track.height().points())
        .fold(0.0, f32::max);
    let zoomed = state.vertical_zoom.zoomed_by(zoom.y, tallest_track);
    if zoomed == state.vertical_zoom {
        return;
    }
    let before = RowsLayout::of(project, &state.drawers, state.vertical_zoom);
    let after = RowsLayout::of(project, &state.drawers, zoomed);
    let pointer_below_the_top_of_the_view = pointer.y - view.top();
    let point_under_the_pointer =
        before.where_the_point_goes_in(&after, pointer_below_the_top_of_the_view + state.rows_scroll_offset);
    // The rows are scrolled as far as keeps the point where it is, but not further than they scroll: when all of them are in the
    // area (because they fit in it) they are not scrolled at all, whatever the point does.
    let most_they_scroll = (after.height_of_everything_in_the_area() - tallest_rows_area()).max(0.0);
    state.note_that_the_user_changes_the_view(ViewAction::Zooming);
    state.rows_scroll_offset =
        (point_under_the_pointer - pointer_below_the_top_of_the_view).clamp(0.0, most_they_scroll);
    state.vertical_zoom = zoomed;
}
