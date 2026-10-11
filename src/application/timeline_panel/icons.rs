//! The two little pictures of the timeline that are not clips: the settings gear and the plus.
//! They are drawn with shapes, so that no font is needed.

use egui::{Color32, Painter, Pos2, Shape, Stroke, pos2, vec2};

/// How many teeth the gear has.
const TEETH: usize = 8;

/// Draws a gear of the given overall `radius`, with a round hole in the middle that is `hole_colour`
/// (the colour of what is behind the gear).
pub(super) fn draw_gear(painter: &Painter, centre: Pos2, radius: f32, colour: Color32, hole_colour: Color32) {
    let body_radius = radius * 0.78;
    painter.circle_filled(centre, body_radius, colour);
    // Each tooth is a small four-cornered piece standing on the body, a little narrower at its tip.
    for tooth in 0..TEETH {
        let angle = std::f32::consts::TAU * tooth as f32 / TEETH as f32;
        let outwards = vec2(angle.cos(), angle.sin());
        let sideways = vec2(-outwards.y, outwards.x);
        let half_width_at_the_foot = radius * 0.24;
        let half_width_at_the_tip = radius * 0.16;
        let foot = centre + outwards * (body_radius * 0.9);
        let tip = centre + outwards * radius;
        painter.add(Shape::convex_polygon(
            vec![
                foot - sideways * half_width_at_the_foot,
                tip - sideways * half_width_at_the_tip,
                tip + sideways * half_width_at_the_tip,
                foot + sideways * half_width_at_the_foot,
            ],
            colour,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(centre, radius * 0.32, hole_colour);
}

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
