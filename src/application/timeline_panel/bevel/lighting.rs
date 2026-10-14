//! How lit the lines of a clip's bevel are, at each place along them. The light comes from the top left.
//!
//! Along the straight part of the left edge and the top edge a line is white, and along the right edge and the bottom edge it is
//! black, as strongly as the table of the edge (see [`Side::alphas`]) says for the layer the line is in. Round a corner the light
//! does not jump from the one edge's to the other's: it goes from one to the other as the way the outline faces turns, smoothly.
//! So at the top right corner and the bottom left corner, where an edge that faces the light turns into one that faces away from it
//! (or the other way round), a line fades from light through nothing to dark, and at the top left corner, which faces the light
//! straight on, it is brighter than along either of the edges that meet there.
//!
//! What is worked out here is a strength with a sign (positive for white, negative for black) from where a point is on the
//! outline, and nothing else, so that it can be done for each point of a line as it is drawn (see [`Lighting::colour_at`]).

use egui::{Color32, Pos2, Rect};

/// One of the four edges of a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

impl Side {
    pub(super) const ALL: [Side; 4] = [Self::Left, Self::Top, Self::Right, Self::Bottom];

    /// How opaque the line of each layer is along this edge, from the edge of the clip inwards: there
    /// is a layer, one point thick, for each number. The light along the top and the left edge is
    /// white, and the shade along the bottom and the right edge is black.
    ///
    /// The left edge's light and the right edge's shade are thicker than the top's and the bottom's: it
    /// is along those two that clips lie next to one another, and where two clips of one dark colour
    /// meet is where the boundary between them has to show.
    pub(super) fn alphas(self) -> &'static [u8] {
        match self {
            Self::Top => &[150, 76, 30],
            Self::Bottom => &[185, 105, 46],
            Self::Left => &[150, 118, 88, 60, 36, 16],
            Self::Right => &[185, 148, 112, 78, 48, 22],
        }
    }

    /// How strong the line of layer `layer` is along this edge, with a sign: positive if it is white, negative if it is black.
    /// (An edge that has no such layer has nothing.)
    pub(super) fn signed_alpha_in_layer(self, layer: usize) -> f32 {
        let alpha = f32::from(self.alphas().get(layer).copied().unwrap_or(0));
        match self {
            Self::Left | Self::Top => alpha,
            Self::Right | Self::Bottom => -alpha,
        }
    }
}

/// The most layers any edge has.
pub(super) const MOST_LAYERS: usize = 6;

/// How much brighter than the brighter of its two edges the light is in the middle of the top left corner, as a fraction of how
/// bright that is: the top left corner is where the highlight is at its brightest, which is 1.45 times as bright as the brighter
/// of the edges that meet there.
const TOP_LEFT_GLINT: f32 = 0.45;

/// A point whose diamond distance (across plus down) from the middle of a rounded corner is less than this is not on a corner that
/// has any rounding to speak of.
const SMALLEST_ROUNDING: f32 = 0.01;

/// How lit one layer of the bevel is, at every point of the rounded rectangle it runs round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// How strong the line is along the straight part of the left, top, right and bottom edges, with a sign (see
    /// [`Side::signed_alpha_in_layer`]).
    along_the_edges: [f32; 4],
    /// The rounded rectangle that the layer runs round.
    outline: Rect,
    corner_radius: f32,
}

impl Lighting {
    /// The light on layer `layer` (0 is the outermost), which runs round the rectangle `outline` with its corners rounded by
    /// `corner_radius`.
    pub(super) fn of_layer(layer: usize, outline: Rect, corner_radius: f32) -> Self {
        Self {
            along_the_edges: Side::ALL.map(|side| side.signed_alpha_in_layer(layer)),
            outline,
            corner_radius,
        }
    }

    /// The colour a line of this layer has at `point` on it: white or black, and as opaque as the light is strong there.
    pub fn colour_at(&self, point: Pos2) -> Color32 {
        let signed_alpha = self.signed_alpha_at(point);
        // (Rounded by adding a half, which is much quicker than rounding, in a loop that is run for every point of every line.)
        let alpha = (signed_alpha.abs() + 0.5).min(255.0) as u8;
        if signed_alpha >= 0.0 {
            Color32::from_white_alpha(alpha)
        } else {
            Color32::from_black_alpha(alpha)
        }
    }

    /// How strong the light is at `point` of the outline, with a sign: positive for white and negative for black.
    ///
    /// How far round a corner the point is is told by where it is from the middle of the corner: the way it is from there runs from
    /// straight along one edge (0) to straight along the other (1), and is a half on the diagonal. (That is worked out from the
    /// across and the down parts of it, `down / (across + down)`, which is much quicker than working out an angle, and is as good
    /// for this: all that it has to be is a smooth way of saying how far round the corner the point is.)
    pub fn signed_alpha_at(&self, point: Pos2) -> f32 {
        let [left, top, right, bottom] = self.along_the_edges;
        let radius = self.corner_radius;
        // The middles of the corners are the corners of this rectangle, which is as small as a line, or as a point, when the
        // rounding is as much as the outline's size allows. (Not `clamp`, which panics if the rounding of a number makes the
        // rectangle inside out by the least bit.)
        let (core_left, core_right) = (self.outline.left() + radius, self.outline.right() - radius);
        let (core_top, core_bottom) = (self.outline.top() + radius, self.outline.bottom() - radius);
        let across = point.x - point.x.max(core_left).min(core_right);
        let down = point.y - point.y.max(core_top).min(core_bottom);
        let (across_size, down_size) = (across.abs(), down.abs());
        let total = across_size + down_size;
        if total < SMALLEST_ROUNDING {
            return self.at_the_nearest_edge(point);
        }
        match (across < 0.0, down < 0.0) {
            (true, true) => {
                let way = down_size / total;
                blend(left, top, way) + glint(left, top, way)
            }
            (false, true) => blend(top, right, across_size / total),
            (false, false) => blend(right, bottom, down_size / total),
            (true, false) => blend(bottom, left, across_size / total),
        }
    }

    /// The strength along the edge that `point` is nearest to: for an outline with no rounding to its corners.
    fn at_the_nearest_edge(&self, point: Pos2) -> f32 {
        let distances = [
            (point.x - self.outline.left()).abs(),
            (point.y - self.outline.top()).abs(),
            (self.outline.right() - point.x).abs(),
            (self.outline.bottom() - point.y).abs(),
        ];
        let nearest = (1..4).fold(0, |nearest, edge| {
            if distances[edge] < distances[nearest] {
                edge
            } else {
                nearest
            }
        });
        self.along_the_edges[nearest]
    }
}

/// `from` when `way` is 0 and `to` when it is 1, going from one to the other smoothly (slowly at first and at last).
fn blend(from: f32, to: f32, way: f32) -> f32 {
    from + (to - from) * way * way * (3.0 - 2.0 * way)
}

/// How much brighter the light is `way` of the way round the top left corner, between the left edge's strength `left` and the top
/// edge's `top`, than it is by the blend of the two: nothing at either edge, and, in the middle, as much as makes the light
/// there [`TOP_LEFT_GLINT`] brighter than the brighter edge is.
fn glint(left: f32, top: f32, way: f32) -> f32 {
    let hump = 16.0 * way * way * (1.0 - way) * (1.0 - way);
    let in_the_middle = left.max(top).max(0.0) * (1.0 + TOP_LEFT_GLINT);
    let by_the_blend_alone = (left + top) / 2.0;
    (in_the_middle - by_the_blend_alone).max(0.0) * hump
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;

    /// A clip 300 points wide and 104 tall with its corners rounded by 25 points.
    fn outline() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0))
    }

    const RADIUS: f32 = 25.0;

    fn lighting_of(layer: usize) -> Lighting {
        Lighting::of_layer(layer, outline(), RADIUS)
    }

    /// The middles of the corners of `outline()` round which they are rounded: top left, top right, bottom right, bottom left.
    fn middles_of_the_corners() -> [Pos2; 4] {
        let outline = outline();
        [
            pos2(outline.left() + RADIUS, outline.top() + RADIUS),
            pos2(outline.right() - RADIUS, outline.top() + RADIUS),
            pos2(outline.right() - RADIUS, outline.bottom() - RADIUS),
            pos2(outline.left() + RADIUS, outline.bottom() - RADIUS),
        ]
    }

    /// The point on the arc of a corner of `outline()` that is `degrees` round it, counted from the right and going clockwise on the
    /// screen as the angles of the arcs of the bevel are.
    fn on_the_corner(middle: Pos2, degrees: f32) -> Pos2 {
        let radians = degrees.to_radians();
        middle + vec2(radians.cos(), radians.sin()) * RADIUS
    }

    /// Points along the straight line from `from` to `to` (without `to`), `step` points from one another or a little less.
    fn along_the_straight_line(from: Pos2, to: Pos2, step: f32) -> Vec<Pos2> {
        let pieces = (from.distance(to) / step).ceil().max(1.0) as usize;
        (0..pieces)
            .map(|piece| from + (to - from) * (piece as f32 / pieces as f32))
            .collect()
    }

    /// Points along a quarter of a corner of `outline()` that begins at `from_degrees` (without the point it ends at), `step`
    /// points from one another or a little less.
    fn along_the_arc(middle: Pos2, from_degrees: f32, step: f32) -> Vec<Pos2> {
        let pieces = (RADIUS * std::f32::consts::FRAC_PI_2 / step).ceil().max(1.0) as usize;
        (0..pieces)
            .map(|piece| on_the_corner(middle, from_degrees + 90.0 * piece as f32 / pieces as f32))
            .collect()
    }

    /// Points all the way round the outline, a little way from one another (`step` points along it), in order from the middle
    /// of the left edge, clockwise on the screen.
    fn all_the_way_round(step: f32) -> Vec<Pos2> {
        let [top_left, top_right, bottom_right, bottom_left] = middles_of_the_corners();
        let outline = outline();
        let mut points = Vec::new();
        points.extend(along_the_straight_line(
            pos2(outline.left(), bottom_left.y),
            pos2(outline.left(), top_left.y),
            step,
        ));
        points.extend(along_the_arc(top_left, 180.0, step));
        points.extend(along_the_straight_line(
            pos2(top_left.x, outline.top()),
            pos2(top_right.x, outline.top()),
            step,
        ));
        points.extend(along_the_arc(top_right, 270.0, step));
        points.extend(along_the_straight_line(
            pos2(outline.right(), top_right.y),
            pos2(outline.right(), bottom_right.y),
            step,
        ));
        points.extend(along_the_arc(bottom_right, 0.0, step));
        points.extend(along_the_straight_line(
            pos2(bottom_right.x, outline.bottom()),
            pos2(bottom_left.x, outline.bottom()),
            step,
        ));
        points.extend(along_the_arc(bottom_left, 90.0, step));
        points
    }

    #[test]
    fn along_the_straight_part_of_each_edge_a_line_is_as_strong_as_the_table_of_the_edge_says_for_its_layer() {
        let outline = outline();
        for layer in 0..MOST_LAYERS {
            let lighting = lighting_of(layer);
            let along = [
                (Side::Left, pos2(outline.left(), outline.center().y)),
                (Side::Top, pos2(outline.center().x, outline.top())),
                (Side::Right, pos2(outline.right(), outline.center().y)),
                (Side::Bottom, pos2(outline.center().x, outline.bottom())),
            ];
            for (side, point) in along {
                assert_eq!(
                    lighting.signed_alpha_at(point),
                    side.signed_alpha_in_layer(layer),
                    "{side:?}, layer {layer}"
                );
            }
        }
    }

    #[test]
    fn the_light_edges_are_white_and_the_shaded_edges_are_black_and_an_edge_has_nothing_in_a_layer_it_does_not_have() {
        for side in Side::ALL {
            for layer in 0..MOST_LAYERS {
                let strength = side.signed_alpha_in_layer(layer);
                match side.alphas().get(layer) {
                    Some(alpha) => assert_eq!(strength.abs(), f32::from(*alpha), "{side:?} {layer}"),
                    None => assert_eq!(strength, 0.0, "{side:?} has no layer {layer}"),
                }
                match side {
                    Side::Left | Side::Top => assert!(strength >= 0.0, "{side:?} is light"),
                    Side::Right | Side::Bottom => assert!(strength <= 0.0, "{side:?} is dark"),
                }
            }
        }
    }

    #[test]
    fn the_colour_is_white_where_the_light_is_positive_and_black_where_it_is_negative_and_as_opaque_as_it_is_strong() {
        let outline = outline();
        let lighting = lighting_of(0);
        assert_eq!(
            lighting.colour_at(pos2(outline.center().x, outline.top())),
            Color32::from_white_alpha(150),
            "the top edge"
        );
        assert_eq!(
            lighting.colour_at(pos2(outline.center().x, outline.bottom())),
            Color32::from_black_alpha(185),
            "the bottom edge"
        );
        assert_eq!(
            lighting.colour_at(pos2(outline.left(), outline.center().y)),
            Color32::from_white_alpha(150)
        );
        assert_eq!(
            lighting.colour_at(pos2(outline.right(), outline.center().y)),
            Color32::from_black_alpha(185)
        );
    }

    #[test]
    fn the_light_in_the_middle_of_the_top_left_corner_is_a_good_deal_brighter_than_along_either_edge_and_nowhere_else_is()
     {
        let [top_left, ..] = middles_of_the_corners();
        let middle_of_the_corner = on_the_corner(top_left, 225.0);
        for layer in 0..MOST_LAYERS {
            let lighting = lighting_of(layer);
            let brighter_edge = Side::Left
                .signed_alpha_in_layer(layer)
                .max(Side::Top.signed_alpha_in_layer(layer));
            assert!(
                (lighting.signed_alpha_at(middle_of_the_corner) - brighter_edge * 1.45).abs() < 0.01,
                "layer {layer}: {} in the corner, and {brighter_edge} along the brighter edge",
                lighting.signed_alpha_at(middle_of_the_corner)
            );
            for point in all_the_way_round(0.25) {
                let is_in_the_top_left_corner = point.x < top_left.x && point.y < top_left.y;
                if !is_in_the_top_left_corner {
                    assert!(
                        lighting.signed_alpha_at(point) <= brighter_edge + 1e-3,
                        "layer {layer}: {point:?} is as bright as the corner: {}",
                        lighting.signed_alpha_at(point)
                    );
                }
            }
        }
    }

    #[test]
    fn the_top_left_corner_is_at_its_brightest_in_the_middle_and_no_brighter_than_the_edges_where_it_meets_them() {
        let [top_left, ..] = middles_of_the_corners();
        let lighting = lighting_of(0);
        let (at_the_left_edge, at_the_top_edge) = (
            lighting.signed_alpha_at(on_the_corner(top_left, 180.0)),
            lighting.signed_alpha_at(on_the_corner(top_left, 270.0)),
        );
        assert_eq!(at_the_left_edge, Side::Left.signed_alpha_in_layer(0));
        assert_eq!(at_the_top_edge, Side::Top.signed_alpha_in_layer(0));
        // (Round the corner from the left edge, 180 degrees, to the top edge, 270: the middle is 225.)
        let strengths: Vec<f32> = (0..=90)
            .map(|degrees| lighting.signed_alpha_at(on_the_corner(top_left, 180.0 + degrees as f32)))
            .collect();
        assert!(
            strengths[..=45].windows(2).all(|pair| pair[1] >= pair[0] - 1e-3),
            "it rises to the middle: {strengths:?}"
        );
        assert!(
            strengths[45..].windows(2).all(|pair| pair[1] <= pair[0] + 1e-3),
            "and falls after it: {strengths:?}"
        );
        assert!(
            strengths[45] > strengths[0] && strengths[45] > strengths[90],
            "and the middle is brighter than both ends"
        );
    }

    #[test]
    fn at_the_top_right_and_bottom_left_corners_a_line_fades_through_nothing_from_light_to_dark_and_back() {
        let [_, top_right, _, bottom_left] = middles_of_the_corners();
        for layer in 0..3 {
            let lighting = lighting_of(layer);
            // Going clockwise round the top right corner the light goes from the top's (light) to the right edge's (dark).
            let round_the_top_right: Vec<f32> = (0..=90)
                .map(|degrees| lighting.signed_alpha_at(on_the_corner(top_right, 270.0 + degrees as f32)))
                .collect();
            assert!(
                round_the_top_right[0] > 0.0 && round_the_top_right[90] < 0.0,
                "layer {layer}"
            );
            assert!(
                round_the_top_right.windows(2).all(|pair| pair[1] <= pair[0] + 1e-3),
                "layer {layer}: it only gets darker round the top right corner"
            );
            let crossings = round_the_top_right
                .windows(2)
                .filter(|pair| pair[0] > 0.0 && pair[1] <= 0.0)
                .count();
            assert_eq!(crossings, 1, "layer {layer}: it is nothing in one place");

            // And the bottom left corner is the other way: from the bottom's (dark) to the left edge's (light).
            let round_the_bottom_left: Vec<f32> = (0..=90)
                .map(|degrees| lighting.signed_alpha_at(on_the_corner(bottom_left, 90.0 + degrees as f32)))
                .collect();
            assert!(
                round_the_bottom_left[0] < 0.0 && round_the_bottom_left[90] > 0.0,
                "layer {layer}"
            );
            assert!(
                round_the_bottom_left.windows(2).all(|pair| pair[1] >= pair[0] - 1e-3),
                "layer {layer}: it only gets lighter round the bottom left corner"
            );
        }
    }

    #[test]
    fn nowhere_on_the_outline_does_the_light_jump_in_a_step_of_a_quarter_of_a_point() {
        // (The abrupt change from white to black at the top right corner and the bottom left corner is what this is the
        // opposite of: it was a change of 335 in the first layer from one point to the next.)
        for layer in 0..MOST_LAYERS {
            let lighting = lighting_of(layer);
            let points = all_the_way_round(0.25);
            let strengths: Vec<f32> = points.iter().map(|point| lighting.signed_alpha_at(*point)).collect();
            let most_it_changes = strengths
                .windows(2)
                .zip(points.windows(2))
                .map(|(pair, places)| (pair[1] - pair[0]).abs() / places[0].distance(places[1]).max(1e-3))
                .fold(0.0_f32, f32::max);
            assert!(
                most_it_changes < 20.0,
                "layer {layer}: the light changes by {most_it_changes} per point of the outline at the most"
            );
        }
    }

    #[test]
    fn a_corner_between_two_shaded_edges_goes_from_the_one_to_the_other_without_ever_being_light() {
        let [_, _, bottom_right, _] = middles_of_the_corners();
        for layer in 0..3 {
            let lighting = lighting_of(layer);
            for degrees in 0..=90 {
                let strength = lighting.signed_alpha_at(on_the_corner(bottom_right, degrees as f32));
                assert!(strength <= 0.0, "layer {layer}: {degrees} degrees: {strength}");
            }
        }
    }

    #[test]
    fn a_corner_that_is_not_rounded_has_each_edge_as_strong_as_its_table_says_all_along_it() {
        let outline = outline();
        let sharp = Lighting::of_layer(0, outline, 0.0);
        for (side, point) in [
            (Side::Left, pos2(outline.left(), outline.top() + 10.0)),
            (Side::Top, pos2(outline.left() + 10.0, outline.top())),
            (Side::Right, pos2(outline.right(), outline.bottom() - 10.0)),
            (Side::Bottom, pos2(outline.right() - 10.0, outline.bottom())),
        ] {
            assert_eq!(sharp.signed_alpha_at(point), side.signed_alpha_in_layer(0), "{side:?}");
        }
    }

    #[test]
    fn an_outline_that_is_rounded_as_much_as_it_can_be_is_lit_all_round_without_a_gap() {
        // 104 points tall with the rounding at half of that: the sides are half circles, and the top and bottom are straight.
        let pill = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 104.0));
        let lighting = Lighting::of_layer(0, pill, 52.0);
        assert_eq!(lighting.signed_alpha_at(pos2(150.0, 0.0)), 150.0, "the top");
        assert_eq!(lighting.signed_alpha_at(pos2(150.0, 104.0)), -185.0, "the bottom");
        assert!(
            lighting.signed_alpha_at(pos2(0.0, 52.0)) > 100.0,
            "the left end is light"
        );
        assert!(
            lighting.signed_alpha_at(pos2(300.0, 52.0)) < -100.0,
            "and the right end is dark"
        );

        // A clip that is narrower than it is tall is a circle at each end, and a rounding that is more than it allows is cut
        // down to what it does (the rectangle that the corners are round is then a point, or a line, and may be a hair inside out).
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 104.0));
        let lighting = Lighting::of_layer(0, narrow, 5.0);
        for point in [
            pos2(5.0, 0.0),
            pos2(0.0, 52.0),
            pos2(10.0, 52.0),
            pos2(5.0, 104.0),
            pos2(1.0, 1.0),
        ] {
            assert!(lighting.signed_alpha_at(point).is_finite(), "{point:?}");
        }
    }

    #[test]
    fn the_opacity_of_a_colour_is_the_strength_of_the_light_rounded_to_the_nearest_step_and_not_cut_off() {
        // (Round the corners the strength has all the numbers in between the ones of the tables; cut off, an opacity would be nearly a step
        // less than the strength, for a good half of the points.)
        for layer in 0..MOST_LAYERS {
            let lighting = lighting_of(layer);
            for point in all_the_way_round(0.25) {
                let strength = lighting.signed_alpha_at(point);
                let colour = lighting.colour_at(point);
                assert!(
                    (f32::from(colour.a()) - strength.abs()).abs() <= 0.5 + 1e-3,
                    "layer {layer}: at {point:?} the strength is {strength} and the opacity {}",
                    colour.a()
                );
            }
        }
    }

    #[test]
    fn a_colour_stops_at_all_the_way_opaque_where_the_glint_would_take_it_further() {
        // (Light so strong along the edges that the glint at the top left corner is more than there is: it must not wrap round.)
        let overbright = Lighting {
            along_the_edges: [250.0, 250.0, -250.0, -250.0],
            outline: outline(),
            corner_radius: RADIUS,
        };
        let in_the_middle_of_the_top_left_corner = on_the_corner(middles_of_the_corners()[0], 225.0);
        assert!(
            overbright.signed_alpha_at(in_the_middle_of_the_top_left_corner) > 300.0,
            "the setup"
        );
        assert_eq!(
            overbright.colour_at(in_the_middle_of_the_top_left_corner),
            Color32::from_white_alpha(255),
            "it stops at all the way opaque"
        );
    }
}
