//! The rows of the timeline: for each track, its title box and its strip, one row under another.
//!
//! The rows are laid out one under another, as tall as their tracks are, which is what makes the rows area as
//! tall as all of them together (and scroll). They are *drawn* where they are laid out, except while tracks are
//! being moved about, when a row is shown somewhere else for as long as it is moving: see [`super::track_reordering`].
//! A track that is being dragged to another place in the list is drawn last, over the others, with a shadow
//! under it.

use egui::{Color32, CornerRadius, Rect, Sense, Shadow, Ui, pos2, vec2};

use super::strip_view::StripView;
use super::track_reordering::{follow_the_handles, plan_rows};
use super::{TimelineContent, TimelineViewState, header, strip, track_resize};
use crate::application::commands::Command;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::Track;

/// The shadow under the row of a track that is being dragged, which is lifted over the others.
const SHADOW_UNDER_THE_DRAGGED_ROW: Shadow = Shadow {
    offset: [0, 6],
    blur: 16,
    spread: 0,
    color: Color32::from_black_alpha(150),
};

/// Draws the rows in `ui`, which is the inside of the area they scroll in: the rows are as wide as `ui` is, the
/// title boxes at the left of them and then the strips, which are `strips_width` wide and start `strips_offset`
/// from the left edge of the row and look at the timeline the way `ruler_view` does. `rows_area_height` is how tall
/// the rows were together in the last frame. Gives the tracks with the view of the strip of each, in the order of
/// the tracks.
pub(super) fn show<'a>(
    ui: &mut Ui,
    content: &TimelineContent<'a>,
    (ruler_view, strips_offset, strips_width): (StripView, f32, f32),
    rows_area_height: f32,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) -> Vec<(&'a Track, StripView)> {
    let tracks = content.project.tracks();
    let can_delete = tracks.len() > 1;
    // A row is as tall as its track, and as the drawers that are open on it need under that.
    let heights: Vec<f32> = tracks
        .iter()
        .map(|track| track.height().points() + state.drawers.extra_height_of(track))
        .collect();
    let laid_out: Vec<Rect> = heights
        .iter()
        .map(|height| {
            ui.allocate_exact_size(vec2(ui.available_width(), *height), Sense::hover())
                .0
        })
        .collect();

    let rows_top = laid_out.first().map_or_else(|| ui.cursor().top(), |row| row.top());
    let planned = plan_rows(
        &tracks
            .iter()
            .zip(&heights)
            .map(|(track, height)| (track.id(), *height))
            .collect::<Vec<_>>(),
        theme::SPACE_BETWEEN_TRACKS,
        state.track_reorder.as_mut(),
        &mut state.row_motion,
        ui.input(|input| input.pointer.interact_pos())
            .map(|pointer| pointer.y - rows_top),
        ui.input(|input| input.stable_dt),
    );
    if planned.is_moving {
        ui.ctx().request_repaint();
    }
    let dragged = state
        .track_reorder
        .and_then(|reorder| tracks.iter().position(|track| track.id() == reorder.track));
    let drawing_order = (0..tracks.len())
        .filter(|position| Some(*position) != dragged)
        .chain(dragged);

    let mut views: Vec<Option<StripView>> = vec![None; tracks.len()];
    for position in drawing_order {
        let track = &tracks[position];
        // The box and the strip are as tall as the track is: what is under them is the room for the drawers.
        let height = track.height().points();
        let top = rows_top + planned.tops[position];
        let row = Rect::from_min_size(
            pos2(laid_out[position].left(), top),
            vec2(laid_out[position].width(), heights[position]),
        );
        let header_rectangle = Rect::from_min_size(row.min, vec2(theme::TRACK_HEADER_WIDTH, height));
        let strip_rectangle =
            Rect::from_min_size(pos2(row.left() + strips_offset, row.top()), vec2(strips_width, height));
        if Some(position) == dragged {
            let whole_row = Rect::from_min_max(row.min, pos2(strip_rectangle.right(), row.bottom()));
            ui.painter()
                .add(SHADOW_UNDER_THE_DRAGGED_ROW.as_shape(whole_row, CornerRadius::same(theme::CLIP_CORNER_RADIUS)));
        }
        let handles = header::show(
            ui,
            header_rectangle,
            track,
            (
                content.can_edit,
                can_delete,
                content.selection.contains_track(track.id()),
            ),
            commands,
        );
        follow_the_handles(
            ui,
            &handles,
            (track.id(), position, top),
            &mut state.track_reorder,
            commands,
        );
        let strip_view = ruler_view.shown_in(strip_rectangle);
        strip::show(ui, &strip_view, track, content, state, textures, commands);
        views[position] = Some(strip_view);
        track_resize::show(
            ui,
            header_rectangle,
            track,
            content.can_edit,
            rows_area_height,
            state,
            commands,
        );
    }
    tracks
        .iter()
        .zip(views)
        .filter_map(|(track, view)| Some((track, view?)))
        .collect()
}
