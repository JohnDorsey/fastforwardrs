// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The row above the tracks: the ruler, over the strips, and the round button that adds a track,
//! over the title boxes.

use egui::{Rect, Sense, Ui, WidgetInfo, WidgetType, pos2, vec2};

use super::strip_view::StripView;
use super::view_state::TimelineViewState;
use super::{ADD_TRACK_LABEL, RULER_LABEL, TimelineContent, ruler};
use crate::application::commands::Command;
use crate::application::plus_button;
use crate::application::theme;
use crate::application::timeline_geometry::TimelineGeometry;

/// Where the parts of the top row are on the screen.
pub(super) struct TopRow {
    /// The whole row.
    pub rectangle: Rect,
    /// The ruler, which is over the strips.
    pub ruler_rectangle: Rect,
    /// The stretch of the strips that the ruler, the strips and the playhead all show.
    pub ruler_view: StripView,
}

/// Draws the row and adds a command for each thing the user does in it. `strips_offset` is how far
/// from the left edge of the row the strips begin, and `strips_width` how wide they are.
pub(super) fn show(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    geometry: TimelineGeometry,
    (strips_offset, strips_width): (f32, f32),
    commands: &mut Vec<Command>,
) -> TopRow {
    let (top_row, _) = ui.allocate_exact_size(vec2(ui.available_width(), theme::TOP_ROW_HEIGHT), Sense::hover());
    let ruler_rectangle = Rect::from_min_size(
        pos2(top_row.left() + strips_offset, top_row.top()),
        vec2(strips_width, theme::TOP_ROW_HEIGHT),
    );
    show_add_track_button(ui, top_row, content.can_edit, commands);
    let ruler_response = ui.interact(ruler_rectangle, ui.id().with("ruler"), Sense::CLICK | Sense::DRAG);
    ruler_response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, content.can_edit, RULER_LABEL));
    let ruler_view = StripView::new(geometry, state.scroll_offset, ruler_rectangle);
    let ruler_painter = ui.painter_at(ruler_rectangle);
    ruler::draw_background(&ruler_painter, ruler_rectangle);
    let total_frames = content.project.total_frame_count();
    ruler::draw_ticks(&ruler_painter, &ruler_view, total_frames);
    if content.can_edit
        && let Some(frame) = ruler::frame_picked(&ruler_response, &ruler_view, total_frames)
    {
        commands.push(Command::SeekTo(frame));
        state.frame_picked_on_the_ruler = Some(frame);
    }
    TopRow {
        rectangle: top_row,
        ruler_rectangle,
        ruler_view,
    }
}

/// The big round button that adds a track, in the middle of the part of `top_row` that is over the
/// title boxes.
pub(super) fn show_add_track_button(ui: &mut Ui, top_row: Rect, can_edit: bool, commands: &mut Vec<Command>) {
    let centre = pos2(top_row.left() + theme::TRACK_HEADER_WIDTH / 2.0, top_row.center().y);
    let response = plus_button::show(
        ui,
        centre,
        "add track",
        ADD_TRACK_LABEL,
        can_edit,
        theme::TRACK_HEADER_BACKGROUND,
    );
    if can_edit && response.clicked() {
        commands.push(Command::AddTrack);
    }
}
