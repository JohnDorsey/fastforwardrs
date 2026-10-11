//! The ruler: a small strip of empty space just above the tracks, with a small line for every
//! second of the finished video (and finer ones between, when there is room), like the marks on a
//! ruler. Clicking or dragging in it moves the playhead to that place.

use egui::{Painter, Rect, Response, Stroke, pos2};

use super::strip_view::StripView;
use crate::application::theme;
use crate::application::timeline_geometry::RulerTickKind;
use crate::project::TimelineFrameIndex;

/// How tall the mark for a whole second is, and how tall the finer marks are, in points.
const SECOND_TICK_HEIGHT: f32 = 9.0;
const TENTH_OF_A_SECOND_TICK_HEIGHT: f32 = 4.0;

/// Fills the ruler's strip.
pub(super) fn draw_background(painter: &Painter, ruler_rectangle: Rect) {
    painter.rect_filled(ruler_rectangle, 0.0, theme::RULER_BACKGROUND);
}

/// Draws the marks along the bottom edge of the ruler, rising from the tracks. `view` is the view
/// whose rectangle is the ruler.
pub(super) fn draw_ticks(painter: &Painter, view: &StripView, total_frame_count: u64) {
    let ruler_rectangle = view.rectangle;
    for tick in view
        .geometry
        .ruler_ticks(total_frame_count, view.scroll_offset, view.width())
    {
        // One point wide lines are crisp only when they are in the middle of a pixel.
        let x = painter.round_to_pixel_center(view.screen_x_of(tick.x));
        let height = match tick.kind {
            RulerTickKind::Second => SECOND_TICK_HEIGHT,
            RulerTickKind::Tenth => TENTH_OF_A_SECOND_TICK_HEIGHT,
        };
        painter.line_segment(
            [
                pos2(x, ruler_rectangle.bottom() - height),
                pos2(x, ruler_rectangle.bottom()),
            ],
            Stroke::new(1.0, theme::RULER_TICK),
        );
    }
}

/// The frame the user picked by clicking, or by dragging along, the ruler during this frame of the
/// window. A press that has not been let go yet counts once it moves. `view` is the view whose
/// rectangle is the ruler, which `response` is the response of.
pub(super) fn frame_picked(
    response: &Response,
    view: &StripView,
    total_frame_count: u64,
) -> Option<TimelineFrameIndex> {
    if !(response.clicked_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Primary)) {
        return None;
    }
    let pointer = response.interact_pointer_pos()?;
    view.frame_picked_at(pointer.x, total_frame_count)
}
