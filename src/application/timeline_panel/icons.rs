//! The little picture of the timeline that is not a clip: the plus. It is drawn with shapes, so that no
//! font is needed.

use egui::{Color32, Painter, Pos2, Stroke, pos2};

/// Draws a plus sign whose arms reach `arm_length` from `centre`.
pub(super) fn draw_plus(painter: &Painter, centre: Pos2, arm_length: f32, thickness: f32, colour: Color32) {
    let stroke = Stroke::new(thickness, colour);
    painter.line_segment(
        [
            pos2(centre.x - arm_length, centre.y),
            pos2(centre.x + arm_length, centre.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(centre.x, centre.y - arm_length),
            pos2(centre.x, centre.y + arm_length),
        ],
        stroke,
    );
}
