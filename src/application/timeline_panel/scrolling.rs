//! Where the strips are scrolled to, and how far they are zoomed.

use egui::{Rect, Ui};

use super::TimelineContent;
use super::view_state::TimelineViewState;
use crate::application::timeline_geometry::TimelineGeometry;

/// Brings the view up to date for this frame, and returns the scale the strips are drawn at.
///
/// * The mouse wheel with Ctrl held (or a pinch on a trackpad) zooms the strips in and out while the
///   pointer is over them, keeping the moment under the pointer where it is.
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

    let zoom_factor = f64::from(ui.input(|input| input.zoom_delta()));
    if zoom_factor != 1.0
        && let (Some(strips), Some(pointer)) = (state.strips_area_in_the_last_frame, pointer)
        && strips.contains(pointer)
    {
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
        if pointer_is_in(state.timeline_area_in_the_last_frame) {
            state.rows_scroll_offset -= scrolled.y;
        }
    } else if pointer_is_in(state.strips_area_in_the_last_frame) {
        // Scrolling either way (a wheel up and down, or a trackpad sideways) moves the strips.
        state.scroll_offset -= f64::from(scrolled.x + scrolled.y);
    }
    state.scroll_offset = geometry.clamp_scroll_offset(state.scroll_offset, view_width, total_frames);
    geometry
}
