// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The outline of a clip: a rectangle with its corners rounded, and cutting what is drawn in a clip to it.
//!
//! A clip is drawn in pieces (the pictures of a video, the bars of a waveform), and every piece is cut
//! to the outline of the whole clip. Rounding each piece with a radius of its own is not the same:
//! a piece that is narrower than twice the radius (the last picture of a clip is often only a sliver)
//! can only be rounded as far as half of its own width, so it would stick out of the corners of the
//! clip, which are rounded as far as the clip's own radius.
//!
//! This is only geometry, as polygons to be filled.

use egui::{Pos2, Rect, pos2};

/// How many straight pieces a quarter of a rounded corner is made of.
const PIECES_PER_QUARTER_TURN: usize = 8;

/// A rectangle with its corners rounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ClipShape {
    whole: Rect,
    radius: f32,
}

impl ClipShape {
    /// The shape of the clip that fills `whole`, with corners rounded by `radius`, or as far as the
    /// clip allows: by no more than half of its width or its height.
    pub(super) fn new(whole: Rect, radius: f32) -> Self {
        let most_that_fits = (whole.width() / 2.0).min(whole.height() / 2.0).max(0.0);
        Self {
            whole,
            radius: radius.clamp(0.0, most_that_fits),
        }
    }

    /// How much the corners are rounded by: the radius asked for, or as much as the shape allows.
    pub(super) fn radius(&self) -> f32 {
        self.radius
    }

    /// Whether all of `area` is inside the shape, which it is if it is inside the rectangle and
    /// touches none of the squares at the corners that the rounding cuts into.
    pub(super) fn contains_all_of(&self, area: Rect) -> bool {
        if !self.whole.contains_rect(area) {
            return false;
        }
        self.radius <= 0.0
            || self
                .squares_at_the_corners()
                .iter()
                .all(|square| !overlaps(*square, area))
    }

    /// The part of `area` that is inside the shape, as a convex polygon whose points go clockwise on
    /// the screen (none if no part of `area` is inside). Pieces that together make a rectangle give
    /// parts that together make the shape, with no gap and no overlap between them.
    pub(super) fn part_of(&self, area: Rect) -> Vec<Pos2> {
        let area = area.intersect(self.whole);
        if area.width() <= 0.0 || area.height() <= 0.0 {
            return Vec::new();
        }
        let mut polygon = corners_of(area);
        if self.contains_all_of(area) {
            return polygon;
        }
        let outline = self.outline();
        for (index, from) in outline.iter().enumerate() {
            let to = outline[(index + 1) % outline.len()];
            if *from == to {
                continue;
            }
            polygon = keep_what_is_on_the_inside_of(&polygon, *from, to);
            if polygon.is_empty() {
                break;
            }
        }
        polygon
    }

    /// The outline as a polygon, clockwise on the screen, starting at the top left corner.
    fn outline(&self) -> Vec<Pos2> {
        let (left, top, right, bottom) = (
            self.whole.left(),
            self.whole.top(),
            self.whole.right(),
            self.whole.bottom(),
        );
        let radius = self.radius;
        // Angles are in degrees, measured from the right and going clockwise on the screen.
        let corners = [
            (pos2(left + radius, top + radius), 180.0),
            (pos2(right - radius, top + radius), 270.0),
            (pos2(right - radius, bottom - radius), 0.0),
            (pos2(left + radius, bottom - radius), 90.0),
        ];
        corners
            .iter()
            .flat_map(|(centre, first_angle)| {
                (0..=PIECES_PER_QUARTER_TURN).map(move |piece| {
                    let degrees = first_angle + 90.0 * piece as f32 / PIECES_PER_QUARTER_TURN as f32;
                    let radians = degrees.to_radians();
                    pos2(centre.x + radius * radians.cos(), centre.y + radius * radians.sin())
                })
            })
            .collect()
    }

    /// The four squares, one in each corner of the rectangle, that the rounding cuts into.
    fn squares_at_the_corners(&self) -> [Rect; 4] {
        let size = egui::vec2(self.radius, self.radius);
        [
            Rect::from_min_size(self.whole.left_top(), size),
            Rect::from_min_size(pos2(self.whole.right() - self.radius, self.whole.top()), size),
            Rect::from_min_size(
                pos2(self.whole.right() - self.radius, self.whole.bottom() - self.radius),
                size,
            ),
            Rect::from_min_size(pos2(self.whole.left(), self.whole.bottom() - self.radius), size),
        ]
    }
}

/// Whether two rectangles have some area in common (touching is not enough).
fn overlaps(first: Rect, second: Rect) -> bool {
    let common = first.intersect(second);
    common.width() > 0.0 && common.height() > 0.0
}

/// The corners of a rectangle, clockwise on the screen from the top left.
fn corners_of(rectangle: Rect) -> Vec<Pos2> {
    vec![
        rectangle.left_top(),
        rectangle.right_top(),
        rectangle.right_bottom(),
        rectangle.left_bottom(),
    ]
}

/// Which side of the line from `from` to `to` a point is on: positive on the inside (which is on the
/// right of the way along the line, as it is seen on the screen, for an outline that goes clockwise),
/// negative on the outside and zero on the line. Only the sign means anything.
fn side_of(from: Pos2, to: Pos2, point: Pos2) -> f32 {
    (to.x - from.x) * (point.y - from.y) - (to.y - from.y) * (point.x - from.x)
}

/// Cuts a convex polygon with the line from `from` to `to`, keeping what is on the inside of it.
fn keep_what_is_on_the_inside_of(polygon: &[Pos2], from: Pos2, to: Pos2) -> Vec<Pos2> {
    let mut kept = Vec::with_capacity(polygon.len() + 2);
    for (index, current) in polygon.iter().enumerate() {
        let next = polygon[(index + 1) % polygon.len()];
        let (side_of_current, side_of_next) = (side_of(from, to, *current), side_of(from, to, next));
        if side_of_current >= 0.0 {
            kept.push(*current);
        }
        if (side_of_current >= 0.0) != (side_of_next >= 0.0) {
            let fraction = side_of_current / (side_of_current - side_of_next);
            kept.push(pos2(
                current.x + (next.x - current.x) * fraction,
                current.y + (next.y - current.y) * fraction,
            ));
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    const RADIUS: f32 = 27.0;

    /// A clip 300 points wide and 104 tall, with its top left corner at (100, 50).
    fn clip() -> ClipShape {
        ClipShape::new(Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0)), RADIUS)
    }

    /// Whether `point` is in the shape, worked out from what a rounded rectangle is and not from the
    /// polygon (with room for the points of the polygon being worked out in f32).
    fn is_inside(shape: &ClipShape, point: Pos2) -> bool {
        let tolerance = 0.01;
        let whole = shape.whole;
        if point.x < whole.left() - tolerance
            || point.x > whole.right() + tolerance
            || point.y < whole.top() - tolerance
            || point.y > whole.bottom() + tolerance
        {
            return false;
        }
        let radius = shape.radius;
        let centre_x = point.x.clamp(whole.left() + radius, whole.right() - radius);
        let centre_y = point.y.clamp(whole.top() + radius, whole.bottom() - radius);
        pos2(centre_x, centre_y).distance(point) <= radius + tolerance
    }

    /// The area of a polygon, by the shoelace formula.
    fn area_of(polygon: &[Pos2]) -> f32 {
        let twice = polygon
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let next = polygon[(index + 1) % polygon.len()];
                point.x * next.y - next.x * point.y
            })
            .sum::<f32>();
        twice.abs() / 2.0
    }

    /// The area of the shape, by adding up the height of it at many places across.
    fn area_of_the_shape_between(shape: &ClipShape, left: f32, right: f32) -> f32 {
        let steps = 20_000;
        let width = (right - left) / steps as f32;
        (0..steps)
            .map(|step| {
                let x = left + (step as f32 + 0.5) * width;
                let into_the_corner = (shape.whole.left() + shape.radius - x)
                    .max(x - (shape.whole.right() - shape.radius))
                    .max(0.0);
                let cut_off = shape.radius - (shape.radius.powi(2) - into_the_corner.powi(2)).max(0.0).sqrt();
                (shape.whole.height() - 2.0 * cut_off) * width
            })
            .sum()
    }

    #[test]
    fn the_corners_are_rounded_by_the_radius_asked_for_or_as_far_as_a_small_clip_allows() {
        assert_eq!(clip().radius, RADIUS);
        let narrow = ClipShape::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 104.0)), RADIUS);
        assert_eq!(narrow.radius, 20.0, "half of its width");
        let short = ClipShape::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 30.0)), RADIUS);
        assert_eq!(short.radius, 15.0, "half of its height");
        let nothing = ClipShape::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0)), RADIUS);
        assert_eq!(nothing.radius, 0.0);
    }

    #[test]
    fn what_is_away_from_the_corners_is_all_inside_and_comes_back_as_it_is() {
        let middle = Rect::from_min_size(pos2(150.0, 50.0), vec2(100.0, 104.0));
        assert!(clip().contains_all_of(middle));
        assert_eq!(
            clip().part_of(middle),
            vec![
                middle.left_top(),
                middle.right_top(),
                middle.right_bottom(),
                middle.left_bottom()
            ]
        );
    }

    #[test]
    fn what_reaches_a_corner_is_not_all_inside() {
        let at_the_end = Rect::from_min_size(pos2(380.0, 50.0), vec2(20.0, 104.0));
        assert!(!clip().contains_all_of(at_the_end));
        let at_the_start_top = Rect::from_min_size(pos2(100.0, 50.0), vec2(10.0, 10.0));
        assert!(!clip().contains_all_of(at_the_start_top));
        let sticking_out = Rect::from_min_size(pos2(90.0, 80.0), vec2(30.0, 20.0));
        assert!(!clip().contains_all_of(sticking_out), "it is not inside the rectangle");
    }

    #[test]
    fn what_is_outside_the_clip_has_no_part_of_it() {
        assert!(
            clip()
                .part_of(Rect::from_min_size(pos2(500.0, 50.0), vec2(20.0, 104.0)))
                .is_empty()
        );
        assert!(
            clip()
                .part_of(Rect::from_min_size(pos2(100.0, 200.0), vec2(20.0, 10.0)))
                .is_empty()
        );
        assert!(
            clip()
                .part_of(Rect::from_min_size(pos2(150.0, 60.0), vec2(0.0, 10.0)))
                .is_empty()
        );
    }

    #[test]
    fn the_end_of_a_clip_is_cut_by_the_curve_of_its_corners_and_nothing_sticks_out_of_them() {
        let shape = clip();
        let last_sliver = Rect::from_min_size(pos2(380.0, 50.0), vec2(20.0, 104.0));
        let part = shape.part_of(last_sliver);
        assert!(part.len() > 4, "it has the curve of the corners: {part:?}");
        for point in &part {
            assert!(is_inside(&shape, *point), "{point:?} is outside the shape");
        }
        let expected = area_of_the_shape_between(&shape, 380.0, 400.0);
        assert!(
            (area_of(&part) - expected).abs() < 0.01 * expected,
            "{} is not about {expected}",
            area_of(&part)
        );
        assert!(area_of(&part) < 20.0 * 104.0, "less than the rectangle");
    }

    #[test]
    fn a_piece_narrower_than_the_radius_at_the_end_of_a_clip_is_cut_by_the_clips_curve_and_not_its_own() {
        // A tile 5 points wide, with its own radius it could only be rounded by 2.5 points.
        let shape = clip();
        let sliver = Rect::from_min_size(pos2(395.0, 50.0), vec2(5.0, 104.0));
        let part = shape.part_of(sliver);
        assert!(!part.is_empty());
        for point in &part {
            assert!(is_inside(&shape, *point), "{point:?}");
        }
        // The corner is a circle of 27 round (373, 77): 22 points from its middle across, where the sliver
        // begins, it is at 77 - sqrt(27 * 27 - 22 * 22), which is 11.35 points down from the top.
        let top_of_it = part.iter().map(|point| point.y).fold(f32::MAX, f32::min);
        assert!((top_of_it - 61.35).abs() < 0.3, "{top_of_it}");
    }

    #[test]
    fn everything_that_is_given_back_is_inside_the_shape_whatever_the_piece() {
        let shape = clip();
        let mut seed = 12345_u32;
        let mut random = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1 << 24) as f32
        };
        for _ in 0..300 {
            let left = 60.0 + random() * 380.0;
            let top = 20.0 + random() * 160.0;
            let area = Rect::from_min_size(pos2(left, top), vec2(1.0 + random() * 200.0, 1.0 + random() * 120.0));
            let part = shape.part_of(area);
            for point in &part {
                assert!(is_inside(&shape, *point), "{point:?} of {area:?}");
            }
            if !part.is_empty() {
                assert!(part.len() >= 3);
                assert!(area_of(&part) <= area.intersect(shape.whole).area() + 0.01);
            }
        }
    }

    #[test]
    fn pieces_that_make_up_the_clip_have_parts_that_make_up_the_shape_with_no_gap_and_no_overlap() {
        let shape = clip();
        let whole_area = area_of_the_shape_between(&shape, 100.0, 400.0);
        for cuts in [
            vec![250.0],
            vec![103.0, 390.0],
            vec![110.0, 111.0, 112.0, 395.0, 399.0],
            vec![127.0, 200.0, 373.0],
        ] {
            let mut edges = vec![100.0];
            edges.extend(cuts);
            edges.push(400.0);
            let total: f32 = edges
                .windows(2)
                .map(|pair| {
                    let piece = Rect::from_min_max(pos2(pair[0], 50.0), pos2(pair[1], 154.0));
                    area_of(&shape.part_of(piece))
                })
                .sum();
            assert!(
                (total - whole_area).abs() < 0.005 * whole_area,
                "{total} is not the {whole_area} of the shape"
            );
        }
    }

    #[test]
    fn a_clip_with_no_rounding_is_cut_to_its_rectangle_only() {
        let square = ClipShape::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 50.0)), 0.0);
        let part = square.part_of(Rect::from_min_size(pos2(-10.0, -10.0), vec2(30.0, 30.0)));
        assert_eq!(area_of(&part), 400.0);
        assert!(square.contains_all_of(Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 50.0))));
    }

    #[test]
    fn a_clip_narrower_than_the_rounding_is_a_pill_and_what_is_cut_to_it_stays_inside_it() {
        let pill = ClipShape::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(30.0, 104.0)), RADIUS);
        assert_eq!(pill.radius, 15.0);
        let part = pill.part_of(Rect::from_min_size(pos2(-20.0, -20.0), vec2(100.0, 200.0)));
        for point in &part {
            assert!(is_inside(&pill, *point), "{point:?}");
        }
        assert!(area_of(&part) < 30.0 * 104.0);
    }
}
