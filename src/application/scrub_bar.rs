//! The slider along the bottom that spans the whole timeline. Dragging it moves quickly through
//! the video, and the current frame follows.

use egui::{PointerButton, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};

use super::commands::Command;
use super::scrub_slider::{frame_at_slider_fraction, slider_fraction_of_frame};
use super::theme;
use crate::project::TimelineFrameIndex;

/// The radius of the knob, in points.
const KNOB_RADIUS: f32 = 9.0;

/// How thick the groove is, in points.
const GROOVE_THICKNESS: f32 = 6.0;

/// Draws the slider across the full width of `ui`, and adds a command when it is dragged.
pub fn show(ui: &mut Ui, current_frame: TimelineFrameIndex, total_frame_count: u64, commands: &mut Vec<Command>) {
    // Not focusable, so that the space bar always means "cut here".
    let sense = if total_frame_count > 0 {
        Sense::CLICK | Sense::DRAG
    } else {
        Sense::hover()
    };
    let (response, painter) = ui.allocate_painter(vec2(ui.available_width(), theme::SCRUB_SLIDER_HEIGHT), sense);
    response.widget_info(|| {
        WidgetInfo::slider(
            total_frame_count > 0,
            f64::from(slider_fraction_of_frame(current_frame, total_frame_count)),
            "Position in the video",
        )
    });
    let rectangle = response.rect;

    // The groove runs from the middle of the knob at one end to the middle of the knob at the other.
    let left = rectangle.left() + KNOB_RADIUS;
    let right = (rectangle.right() - KNOB_RADIUS).max(left + 1.0);
    let middle_y = rectangle.center().y;

    if total_frame_count > 0
        && (response.dragged_by(PointerButton::Primary) || response.clicked_by(PointerButton::Primary))
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let fraction = (pointer.x - left) / (right - left);
        commands.push(Command::SeekTo(frame_at_slider_fraction(fraction, total_frame_count)));
    }

    let fraction = slider_fraction_of_frame(current_frame, total_frame_count);
    let knob_x = left + fraction * (right - left);
    let groove = egui::Rect::from_min_max(
        pos2(left, middle_y - GROOVE_THICKNESS / 2.0),
        pos2(right, middle_y + GROOVE_THICKNESS / 2.0),
    );
    painter.rect_filled(groove, GROOVE_THICKNESS / 2.0, theme::SLIDER_TRACK);
    if total_frame_count > 0 {
        let filled = egui::Rect::from_min_max(groove.min, pos2(knob_x, groove.max.y));
        painter.rect_filled(filled, GROOVE_THICKNESS / 2.0, theme::SLIDER_FILLED);
        painter.circle(
            pos2(knob_x, middle_y),
            KNOB_RADIUS,
            theme::SLIDER_KNOB,
            Stroke::new(1.0, theme::CLIP_BOUNDARY_DARK),
        );
    }
}
