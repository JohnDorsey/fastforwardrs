// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The eye in the box of a track, after the title on its line: **open** while the track is seen (an almond with a round pupil), **closed** while it is hidden (a lid that curves down,
//! with lashes). Clicking it opens or closes it. It is drawn with lines and a circle and not with a character, so that it looks the same whatever the fonts of the computer have.

use egui::{Color32, Id, Rect, Response, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType, pos2};

/// How wide and how tall the place for the eye is, in points. (The drawing is smaller, in the middle of it.)
pub const SIZE: f32 = 22.0;

/// How many straight pieces the curves are drawn with.
const PIECES_PER_CURVE: usize = 16;

/// How thick the lines of the eye are, in points.
const LINE_WIDTH: f32 = 1.6;

/// Points on the curve `y = lift * (1 - t * t)` for `t` from -1 to 1 (a parabola: it is highest in the middle and comes down to a point at each end), across `half_width` on each side of
/// `centre_x`, with `y` measured from `base_y` upward on the screen (so a larger `lift` is higher up, and a negative one is lower down).
fn arch(centre_x: f32, base_y: f32, half_width: f32, lift: f32) -> Vec<egui::Pos2> {
    (0..=PIECES_PER_CURVE)
        .map(|piece| {
            let t = -1.0 + 2.0 * piece as f32 / PIECES_PER_CURVE as f32;
            pos2(centre_x + t * half_width, base_y - lift * (1.0 - t * t))
        })
        .collect()
}

/// What an eye is drawn with in `rectangle`: if `open`, the almond and the pupil; if not, the lid and three lashes. Everything is inside `rectangle`, with room for the lines' width.
pub fn shapes(rectangle: Rect, open: bool, colour: Color32) -> Vec<Shape> {
    let stroke = Stroke::new(LINE_WIDTH, colour);
    let middle = rectangle.center();
    let half_width = rectangle.width() * 0.40;
    let half_height = rectangle.height() * 0.20;
    if open {
        let upper = arch(middle.x, middle.y, half_width, half_height);
        let mut lower = arch(middle.x, middle.y, half_width, -half_height);
        lower.reverse();
        // (The two curves meet at the corners of the eye: each corner is put in once.)
        let outline: Vec<_> = upper.into_iter().chain(lower.into_iter().skip(1)).collect();
        vec![
            Shape::closed_line(outline[..outline.len() - 1].to_vec(), stroke),
            Shape::circle_filled(middle, half_height * 0.62, colour),
        ]
    } else {
        // The lid bows downward a little in the middle, and three lashes hang from it. (Made to read at the size of the place, 22 points, as it is drawn on a screen: a lid that is deep, or lashes
        // that begin where the lid slopes steeply, cross it and run together. The sizes were tried at that size, on the pixels that are drawn.)
        let lid_base = middle.y - half_height * 0.45;
        let sag = half_height * 0.36;
        let lid = arch(middle.x, lid_base, half_width, -sag);
        let mut shapes = vec![Shape::line(lid, stroke)];
        for t in [-0.55_f32, 0.0, 0.55] {
            let x = middle.x + t * half_width;
            let lid_y = lid_base + sag * (1.0 - t * t);
            shapes.push(Shape::line_segment(
                [
                    pos2(x, lid_y),
                    pos2(x + t * half_height * 0.27, lid_y + half_height * 0.82),
                ],
                stroke,
            ));
        }
        shapes
    }
}

/// Draws the eye in `rectangle` (open if not `hidden`) and gives what a click on it is sensed by. `label` is what a screen reader calls it (and what the tests find it by), and
/// `hint` is what hovering over it says. It cannot be clicked when `can_edit` is false.
pub fn show(
    ui: &mut Ui,
    rectangle: Rect,
    id: Id,
    hidden: bool,
    can_edit: bool,
    (label, hint): (&str, &str),
) -> Response {
    let response = ui.interact(rectangle, id, if can_edit { Sense::click() } else { Sense::hover() });
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can_edit, label));
    if can_edit && response.hovered() {
        ui.painter()
            .rect_filled(rectangle, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let colour = if hidden {
        ui.visuals().weak_text_color()
    } else {
        ui.visuals().text_color()
    };
    for shape in shapes(rectangle, !hidden, colour) {
        ui.painter().add(shape);
    }
    response.on_hover_text(hint)
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    fn place() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(SIZE, SIZE))
    }

    #[test]
    fn an_open_eye_is_an_outline_with_a_pupil_and_a_closed_one_is_a_lid_with_lashes_and_no_pupil() {
        let open = shapes(place(), true, Color32::WHITE);
        let closed = shapes(place(), false, Color32::WHITE);

        assert!(open.iter().any(|shape| matches!(shape, Shape::Circle(_))), "the pupil");
        assert!(
            open.iter()
                .any(|shape| matches!(shape, Shape::Path(path) if path.closed)),
            "the almond is closed"
        );
        assert!(
            !closed.iter().any(|shape| matches!(shape, Shape::Circle(_))),
            "no pupil when it is shut"
        );
        assert!(
            !closed
                .iter()
                .any(|shape| matches!(shape, Shape::Path(path) if path.closed))
        );
        assert_eq!(
            closed
                .iter()
                .filter(|shape| matches!(shape, Shape::LineSegment { .. }))
                .count(),
            3,
            "three lashes"
        );
    }

    #[test]
    fn both_eyes_are_drawn_inside_the_place_they_are_given_with_the_width_of_the_lines_too() {
        for open in [true, false] {
            for shape in shapes(place(), open, Color32::WHITE) {
                let drawn = shape.visual_bounding_rect();
                assert!(
                    place().expand(0.5).contains_rect(drawn),
                    "{drawn:?} is outside {:?} (open: {open})",
                    place()
                );
            }
        }
    }

    #[test]
    fn the_almond_is_wider_than_it_is_high_and_is_in_the_middle() {
        let almond = shapes(place(), true, Color32::WHITE)
            .into_iter()
            .find(|shape| matches!(shape, Shape::Path(_)))
            .unwrap()
            .visual_bounding_rect();
        assert!(
            almond.width() > almond.height() * 1.6,
            "an almond, not a ball: {almond:?}"
        );
        assert!((almond.center().x - place().center().x).abs() < 0.5);
        assert!((almond.center().y - place().center().y).abs() < 0.5);
    }

    #[test]
    fn the_place_for_the_eye_fits_in_the_line_of_a_title() {
        const { assert!(SIZE <= super::super::header::TITLE_HEIGHT) };
    }
}
