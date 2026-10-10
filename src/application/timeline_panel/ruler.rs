//! The ruler: a small strip of empty space just above the video track, with a small line for every
//! second of the finished video (and finer ones between, when there is room), like the marks on a
//! ruler. Clicking or dragging in it moves the playhead to that place.

use egui::{Painter, Rect, Response, Stroke, pos2};

use crate::application::theme;
use crate::application::timeline_geometry::{RulerTickKind, TimelineGeometry};
use crate::project::TimelineFrameIndex;

/// How tall the mark for a whole second is, and how tall the finer marks are, in points.
const SECOND_TICK_HEIGHT: f32 = 9.0;
const TENTH_OF_A_SECOND_TICK_HEIGHT: f32 = 4.0;

/// Fills the ruler's strip.
pub(super) fn draw_background(painter: &Painter, ruler_rectangle: Rect) {
    painter.rect_filled(ruler_rectangle, 0.0, theme::RULER_BACKGROUND);
}

/// Draws the marks along the bottom edge of the ruler, rising from the video track.
pub(super) fn draw_ticks(
    painter: &Painter,
    geometry: TimelineGeometry,
    total_frame_count: u64,
    scroll_offset: f64,
    ruler_rectangle: Rect,
) {
    let view_width = f64::from(ruler_rectangle.width());
    for tick in geometry.ruler_ticks(total_frame_count, scroll_offset, view_width) {
        // One point wide lines are crisp only when they are in the middle of a pixel.
        let x = painter.round_to_pixel_center(ruler_rectangle.left() + (tick.x - scroll_offset) as f32);
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
/// window. A press that has not been let go yet counts once it moves.
pub(super) fn frame_picked(
    response: &Response,
    geometry: TimelineGeometry,
    total_frame_count: u64,
    scroll_offset: f64,
) -> Option<TimelineFrameIndex> {
    if !(response.clicked_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Primary)) {
        return None;
    }
    let pointer = response.interact_pointer_pos()?;
    let x_along_the_strip = f64::from(pointer.x - response.rect.left()) + scroll_offset;
    geometry.frame_picked_at_x(x_along_the_strip, total_frame_count)
}
