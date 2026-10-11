//! The ruler: a strip just above the tracks with marks along its bottom edge like the marks on a
//! ruler, the longest of them labelled with the time they are at (which marks there are depends on
//! how far the timeline is zoomed: see the geometry). Clicking or dragging in it moves the playhead
//! to that place.

use egui::{Align2, FontId, Painter, Rect, Response, Stroke, pos2};

use super::strip_view::StripView;
use crate::application::theme;
use crate::application::timeline_geometry::{RulerTickKind, time_label};
use crate::project::TimelineFrameIndex;

/// How tall the marks are, from the bottom edge of the ruler, in points: the labelled ones, the ones
/// between them, and the ones for frames.
const LABELLED_TICK_HEIGHT: f32 = 9.0;
const BETWEEN_TICK_HEIGHT: f32 = 4.0;
const FRAME_TICK_HEIGHT: f32 = 2.0;

/// How big the writing of the labels is, and how far it is from its mark, in points.
const LABEL_SIZE: f32 = 11.0;
const LABEL_DISTANCE_FROM_MARK: f32 = 4.0;

/// Fills the ruler's strip.
pub(super) fn draw_background(painter: &Painter, ruler_rectangle: Rect) {
    painter.rect_filled(ruler_rectangle, 0.0, theme::RULER_BACKGROUND);
}

/// Draws the marks along the bottom edge of the ruler, rising from the tracks, and the labels of the
/// longest ones. `view` is the view whose rectangle is the ruler.
pub(super) fn draw_ticks(painter: &Painter, view: &StripView, total_frame_count: u64) {
    let ruler_rectangle = view.rectangle;
    for tick in view
        .geometry
        .ruler_ticks(total_frame_count, view.scroll_offset, view.width())
    {
        // One point wide lines are crisp only when they are in the middle of a pixel.
        let x = painter.round_to_pixel_center(view.screen_x_of(tick.x));
        let height = match tick.kind {
            RulerTickKind::Labelled { .. } => LABELLED_TICK_HEIGHT,
            RulerTickKind::Between => BETWEEN_TICK_HEIGHT,
            RulerTickKind::Frame => FRAME_TICK_HEIGHT,
        };
        painter.line_segment(
            [
                pos2(x, ruler_rectangle.bottom() - height),
                pos2(x, ruler_rectangle.bottom()),
            ],
            Stroke::new(1.0, theme::RULER_TICK),
        );
        if let RulerTickKind::Labelled { seconds } = tick.kind {
            painter.text(
                pos2(
                    x + LABEL_DISTANCE_FROM_MARK,
                    ruler_rectangle.bottom() - LABELLED_TICK_HEIGHT - LABEL_DISTANCE_FROM_MARK,
                ),
                Align2::LEFT_BOTTOM,
                time_label(seconds),
                FontId::proportional(LABEL_SIZE),
                theme::RULER_LABEL,
            );
        }
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
