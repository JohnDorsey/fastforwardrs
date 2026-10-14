//! The raised look of a clip: a light line along its top and left edges and a dark one along its
//! bottom and right edges, fading towards the middle, so that two clips of the same dark colour can
//! still be told apart where they meet.
//!
//! The line of each layer goes right round the clip as one outline, and how lit it is at each point of it is worked out from where
//! the point is (see [`Lighting`]): along an edge it is as strong as the table of the edge says, and round a corner it goes
//! smoothly from the strength of the one edge to that of the other, so that where a lit edge turns into a shaded one, at the top
//! right and the bottom left, there is no sudden change from light to dark, and the top left corner, which faces the light, is
//! where the light is at its brightest.
//!
//! The left edge's light and the right edge's shade are thicker than the top's and the bottom's: it is along those two that
//! clips lie next to one another, and where two clips of one dark colour meet is where the boundary between them has to show.
//!
//! This is only geometry, as lines to be stroked. A clip can be far longer than the window, so the
//! lines are cut to the stretch that can be seen: their ends out of view are never given.

mod lighting;

use egui::{Pos2, Rect, pos2};

pub(super) use self::lighting::Lighting;
use self::lighting::MOST_LAYERS;

/// How far each line is inside the one before it, in points, and how thick it is.
pub(super) const LAYER_WIDTH: f32 = 1.0;

/// How many straight pieces a quarter of a rounded corner is drawn as. (The more there are, the more smoothly the light changes
/// round the corner, which it does by a good deal: from light to dark in the space of one quarter.)
const PIECES_PER_QUARTER_TURN: usize = 12;

/// How far past the ends of the view the lines are let go on, in points, so that the cut ends are not seen.
const MARGIN_BEYOND_THE_VIEW: f32 = 4.0;

/// One layer of the bevel: the line that goes round the clip a little way inside its edge, and how lit it is.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct BevelLayer {
    /// The points the line goes through. A clip that is partly out of view has pieces of line, so there may be more than one run
    /// of points. (And a clip that is all in view has two, which are the halves of the outline that meet at the top right and
    /// the bottom left corner, where the light is nothing, so that the place where they meet is not seen.)
    pub runs: Vec<Vec<Pos2>>,
    pub lighting: Lighting,
}

/// The layers of lines that give a clip its raised look, from the outermost inwards. `clip` is the whole clip (which may stretch
/// far beyond the window), rounded at its corners by `radius`, and only the stretch of it between `view_left` and `view_right` is
/// given.
pub(super) fn bevel_layers(clip: Rect, radius: f32, view_left: f32, view_right: f32) -> Vec<BevelLayer> {
    let mut layers = Vec::new();
    for layer in 0..MOST_LAYERS {
        let inset = (layer as f32 + 0.5) * LAYER_WIDTH;
        let inside = clip.shrink(inset);
        if inside.width() <= 0.0 || inside.height() <= 0.0 {
            break;
        }
        let corner = (radius - inset)
            .max(0.0)
            .min(inside.width() / 2.0)
            .min(inside.height() / 2.0);
        let runs = halves_of_the_outline(inside, corner)
            .into_iter()
            .flat_map(|half| {
                cut_to_the_view(
                    &half,
                    view_left - MARGIN_BEYOND_THE_VIEW,
                    view_right + MARGIN_BEYOND_THE_VIEW,
                )
            })
            .collect();
        layers.push(BevelLayer {
            runs,
            lighting: Lighting::of_layer(layer, inside, corner),
        });
    }
    layers
}

/// The outline of the rounded rectangle `rectangle` in two chains of points: the one that goes up the left edge and along the
/// top, from the middle of the bottom left corner to the middle of the top right corner, and the one that goes on from there,
/// down the right edge and back along the bottom, to the middle of the bottom left corner. (The light is nothing at the middles
/// of those two corners, so that where the chains end is where nothing is seen of the ends.)
fn halves_of_the_outline(rectangle: Rect, radius: f32) -> [Vec<Pos2>; 2] {
    let top_left = pos2(rectangle.left() + radius, rectangle.top() + radius);
    let top_right = pos2(rectangle.right() - radius, rectangle.top() + radius);
    let bottom_right = pos2(rectangle.right() - radius, rectangle.bottom() - radius);
    let bottom_left = pos2(rectangle.left() + radius, rectangle.bottom() - radius);

    // Each part is the middle of a corner and the angle each end of it is at, in degrees, measured from the right and going
    // clockwise on the screen. The straight edges are the lines from the end of one part to the start of the next.
    let chain = |parts: [(Pos2, f32, f32); 3]| -> Vec<Pos2> {
        parts
            .into_iter()
            .flat_map(|(middle, from_degrees, to_degrees)| arc(middle, radius, from_degrees, to_degrees))
            .collect()
    };
    [
        chain([
            (bottom_left, 135.0, 180.0),
            (top_left, 180.0, 270.0),
            (top_right, 270.0, 315.0),
        ]),
        chain([
            (top_right, 315.0, 360.0),
            (bottom_right, 0.0, 90.0),
            (bottom_left, 90.0, 135.0),
        ]),
    ]
}

/// Points along the arc of a circle round `centre`, from one angle to another.
fn arc(centre: Pos2, radius: f32, from_degrees: f32, to_degrees: f32) -> Vec<Pos2> {
    let quarter_turns = (to_degrees - from_degrees) / 90.0;
    let pieces = ((PIECES_PER_QUARTER_TURN as f32 * quarter_turns).ceil() as usize).max(1);
    (0..=pieces)
        .map(|piece| {
            let degrees = from_degrees + (to_degrees - from_degrees) * piece as f32 / pieces as f32;
            let radians = degrees.to_radians();
            pos2(centre.x + radius * radians.cos(), centre.y + radius * radians.sin())
        })
        .collect()
}

/// The runs of `chain` that lie between `left` and `right` across the screen: the parts of its
/// straight pieces that stick out are cut off, and a chain that leaves and comes back is in two runs.
fn cut_to_the_view(chain: &[Pos2], left: f32, right: f32) -> Vec<Vec<Pos2>> {
    let mut runs: Vec<Vec<Pos2>> = Vec::new();
    // (A run that has begun has at least two points: the beginning of a piece and the end of it.)
    let mut current: Vec<Pos2> = Vec::new();
    for pair in chain.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        match clip_piece_to_the_view(from, to, left, right) {
            Some((start, end)) => {
                // A piece that starts where the one before it ended carries on the same run.
                if current.last() != Some(&start) {
                    runs.extend(end_the_run(&mut current));
                    current.push(start);
                }
                current.push(end);
            }
            None => runs.extend(end_the_run(&mut current)),
        }
    }
    runs.extend(end_the_run(&mut current));
    runs
}

/// The run that has been made so far, if there is one, which is then begun again.
fn end_the_run(current: &mut Vec<Pos2>) -> Option<Vec<Pos2>> {
    (!current.is_empty()).then(|| std::mem::take(current))
}

/// The part of the straight piece from `from` to `to` that is between `left` and `right`, if any.
fn clip_piece_to_the_view(from: Pos2, to: Pos2, left: f32, right: f32) -> Option<(Pos2, Pos2)> {
    if (from.x < left && to.x < left) || (from.x > right && to.x > right) {
        return None;
    }
    let along = |x: f32| -> Pos2 {
        if (to.x - from.x).abs() < f32::EPSILON {
            from
        } else {
            let fraction = (x - from.x) / (to.x - from.x);
            pos2(x, from.y + (to.y - from.y) * fraction)
        }
    };
    let start = if from.x < left {
        along(left)
    } else if from.x > right {
        along(right)
    } else {
        from
    };
    let end = if to.x < left {
        along(left)
    } else if to.x > right {
        along(right)
    } else {
        to
    };
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::lighting::Side;
    use super::*;

    fn clip() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0))
    }

    const RADIUS: f32 = 27.0;

    fn all_points(layer: &BevelLayer) -> impl Iterator<Item = Pos2> + '_ {
        layer.runs.iter().flatten().copied()
    }

    /// The smallest rectangle that has all the points of the layer in it.
    fn bounds_of(layer: &BevelLayer) -> Rect {
        Rect::from_points(&all_points(layer).collect::<Vec<_>>())
    }

    /// Whether the line of the layer goes through `point`: it is on one of the straight pieces between two of its points.
    fn the_line_goes_through(layer: &BevelLayer, point: Pos2) -> bool {
        layer.runs.iter().any(|run| {
            run.windows(2).any(|pair| {
                let along = pair[1] - pair[0];
                let fraction = ((point - pair[0]).dot(along) / along.length_sq().max(1e-6)).clamp(0.0, 1.0);
                (pair[0] + along * fraction).distance(point) < 0.01
            })
        })
    }

    /// How strong the light is at the point `point` of the layer, with a sign.
    fn strength_at(layer: &BevelLayer, point: Pos2) -> f32 {
        layer.lighting.signed_alpha_at(point)
    }

    #[test]
    fn a_clip_has_one_layer_for_each_line_of_its_bevel_and_a_clip_too_small_has_fewer() {
        assert_eq!(bevel_layers(clip(), RADIUS, 0.0, 1000.0).len(), MOST_LAYERS);
        let thin = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 4.0));
        assert_eq!(
            bevel_layers(thin, RADIUS, 0.0, 1000.0).len(),
            2,
            "the layers that are inside a clip 4 points tall are 0.5 and 1.5 points in, and the one at 2.5 would have no height"
        );
        let nothing = Rect::from_min_size(pos2(100.0, 50.0), vec2(0.5, 4.0));
        assert!(bevel_layers(nothing, RADIUS, 0.0, 1000.0).is_empty());
    }

    #[test]
    fn a_layer_that_would_have_no_height_or_no_width_at_all_is_not_made_either() {
        // (5 points tall: the layers are 0.5 and 1.5 points in, which leave 4 and 2 points, and 2.5 points in leaves none.)
        let flat = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 5.0));
        assert_eq!(bevel_layers(flat, RADIUS, 0.0, 1000.0).len(), 2);
        let narrow = Rect::from_min_size(pos2(100.0, 50.0), vec2(5.0, 104.0));
        assert_eq!(bevel_layers(narrow, RADIUS, 0.0, 1000.0).len(), 2);
    }

    #[test]
    fn the_corners_of_each_line_are_round_the_same_middles_as_the_corners_of_the_clip_and_the_light_is_made_for_the_line()
     {
        let clip = clip();
        let middle_of_the_top_left_corner = pos2(clip.left() + RADIUS, clip.top() + RADIUS);
        let towards_the_corner = vec2(225.0_f32.to_radians().cos(), 225.0_f32.to_radians().sin());
        for (number, layer) in bevel_layers(clip, RADIUS, 0.0, 1000.0).iter().enumerate() {
            let inset = number as f32 + 0.5;
            // A line that is `inset` inside the clip is round the same middle, with a radius that is less by that much.
            let on_the_arc = middle_of_the_top_left_corner + towards_the_corner * (RADIUS - inset);
            assert!(
                the_line_goes_through(layer, on_the_arc),
                "layer {number}: the arc is {inset} points inside that of the clip"
            );
            assert_eq!(
                layer.lighting,
                Lighting::of_layer(number, clip.shrink(inset), RADIUS - inset),
                "layer {number}: the light is worked out for the line, and not for the clip"
            );
        }
    }

    #[test]
    fn a_line_that_is_further_in_than_the_corners_are_round_has_square_corners_and_none_that_bend_outwards() {
        let clip = clip();
        let barely_rounded = 1.0;
        for (number, layer) in bevel_layers(clip, barely_rounded, 0.0, 1000.0)
            .iter()
            .enumerate()
            .skip(2)
        {
            let inside_the_line = clip.shrink(number as f32 + 0.5).expand(0.01);
            for point in all_points(layer) {
                assert!(
                    inside_the_line.contains(point),
                    "layer {number}: {point:?} is outside the line's own rectangle, {inside_the_line:?}"
                );
            }
        }
    }

    #[test]
    fn light_runs_along_the_top_and_the_left_and_dark_along_the_bottom_and_the_right() {
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        for (number, layer) in layers.iter().enumerate() {
            // (A line is a little way inside the edge of the clip: half a point, and a point more for each layer before it.)
            let inset = number as f32 + 0.5;
            let inside = clip().shrink(inset);
            for (side, point) in [
                (Side::Left, pos2(inside.left(), inside.center().y)),
                (Side::Top, pos2(inside.center().x, inside.top())),
                (Side::Right, pos2(inside.right(), inside.center().y)),
                (Side::Bottom, pos2(inside.center().x, inside.bottom())),
            ] {
                assert!(
                    the_line_goes_through(layer, point),
                    "layer {number} has a line along the {side:?} edge, through {point:?}"
                );
                assert_eq!(
                    strength_at(layer, point),
                    side.signed_alpha_in_layer(number),
                    "layer {number}: the light along the {side:?} edge"
                );
            }
        }
        let reached = bounds_of(&layers[0]);
        let half_a_point_inside = clip().shrink(0.5);
        for (reached, expected, edge) in [
            (reached.left(), half_a_point_inside.left(), "left"),
            (reached.top(), half_a_point_inside.top(), "top"),
            (reached.right(), half_a_point_inside.right(), "right"),
            (reached.bottom(), half_a_point_inside.bottom(), "bottom"),
        ] {
            assert!(
                (reached - expected).abs() < 1e-3,
                "the lines reach the {edge} edge of the clip, half a point inside it: {reached} and {expected}"
            );
        }
    }

    #[test]
    fn the_tables_of_the_edges_say_that_the_left_edge_is_lit_more_thickly_than_the_top_and_the_right_edge_is_shaded_more_thickly_than_the_bottom()
     {
        assert!(Side::Left.alphas().len() >= 2 * Side::Top.alphas().len());
        assert!(Side::Right.alphas().len() >= 2 * Side::Bottom.alphas().len());
        let strength = |side: Side| side.alphas().iter().map(|alpha| u32::from(*alpha)).sum::<u32>();
        assert!(strength(Side::Left) > strength(Side::Top) * 3 / 2);
        assert!(strength(Side::Right) > strength(Side::Bottom) * 3 / 2);
        for side in Side::ALL {
            let alphas = side.alphas();
            assert!(
                alphas.windows(2).all(|pair| pair[0] > pair[1]),
                "{side:?} fades inwards"
            );
            assert!(alphas.len() <= MOST_LAYERS);
        }
    }

    #[test]
    fn the_layers_that_only_the_left_and_the_right_edge_have_are_not_lit_along_the_top_and_the_bottom() {
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        for (number, layer) in layers.iter().enumerate().skip(Side::Top.alphas().len()) {
            let inside = clip().shrink(number as f32 + 0.5);
            assert_eq!(
                strength_at(layer, pos2(inside.center().x, inside.top())),
                0.0,
                "layer {number} along the top"
            );
            assert_eq!(
                strength_at(layer, pos2(inside.center().x, inside.bottom())),
                0.0,
                "layer {number} along the bottom"
            );
            assert!(
                strength_at(layer, pos2(inside.left(), inside.center().y)) > 0.0,
                "layer {number} along the left edge"
            );
            assert!(
                strength_at(layer, pos2(inside.right(), inside.center().y)) < 0.0,
                "layer {number} along the right edge"
            );
        }
    }

    #[test]
    fn the_two_runs_of_a_layer_go_on_from_one_another_all_the_way_round_the_outline() {
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        for (number, layer) in layers.iter().enumerate() {
            let [lit, shaded] = &layer.runs[..] else {
                panic!(
                    "layer {number}: a clip that is all in view is two runs: {}",
                    layer.runs.len()
                );
            };
            assert!(
                lit.last().unwrap().distance(*shaded.first().unwrap()) < 0.01,
                "layer {number}: at the top right"
            );
            assert!(
                shaded.last().unwrap().distance(*lit.first().unwrap()) < 0.01,
                "layer {number}: at the bottom left"
            );
        }
    }

    #[test]
    fn the_light_is_all_but_nothing_where_the_two_runs_meet_so_that_the_place_is_not_seen() {
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        // (At the middles of the top right and bottom left corners the light is the mean of two edges' strengths, which are the
        // top's and the right's, and the bottom's and the left's.)
        for (number, layer) in layers.iter().enumerate().take(3) {
            let [lit, shaded] = &layer.runs[..] else { panic!() };
            assert!(
                strength_at(layer, *lit.last().unwrap()).abs() <= 45.0,
                "layer {number} at the top right: {}",
                strength_at(layer, *lit.last().unwrap())
            );
            assert!(
                strength_at(layer, *shaded.last().unwrap()).abs() <= 45.0,
                "layer {number} at the bottom left: {}",
                strength_at(layer, *shaded.last().unwrap())
            );
        }
    }

    #[test]
    fn no_two_points_that_are_next_to_each_other_on_a_line_are_lit_so_differently_that_the_change_is_abrupt() {
        // (What was there before the light went smoothly round the corners was a change from white to black of 335 in the
        // outermost layer, from one point of a line to the next, at the top right and bottom left corners.)
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        for (number, layer) in layers.iter().enumerate() {
            for run in &layer.runs {
                for pair in run.windows(2) {
                    let change = (strength_at(layer, pair[1]) - strength_at(layer, pair[0])).abs();
                    let distance = pair[0].distance(pair[1]).max(1e-3);
                    assert!(
                        change / distance < 20.0,
                        "layer {number}: from {:?} to {:?} the light changes by {change} in {distance} points",
                        pair[0],
                        pair[1]
                    );
                }
            }
        }
    }

    #[test]
    fn the_light_changes_by_no_more_than_a_seventh_of_its_whole_range_from_one_point_of_a_line_to_the_next() {
        // (The light goes round a corner from one edge's to the next one's, and a corner is drawn as straight pieces: the more of them, the
        // smaller the step in the light from one end of a piece to the other, which is the smoothness that was asked for. The most of it, from
        // the shade along the right to the light along the top, is 335 in the outermost layer, and a dozen pieces to a quarter turn make steps of
        // 42 at the most.)
        for (number, layer) in bevel_layers(clip(), RADIUS, 0.0, 1000.0).iter().enumerate() {
            let range = Side::Left
                .signed_alpha_in_layer(number)
                .max(Side::Top.signed_alpha_in_layer(number))
                - Side::Right
                    .signed_alpha_in_layer(number)
                    .min(Side::Bottom.signed_alpha_in_layer(number));
            for run in &layer.runs {
                for pair in run.windows(2) {
                    let step = (strength_at(layer, pair[1]) - strength_at(layer, pair[0])).abs();
                    assert!(
                        step <= range / 7.0 + 1.0,
                        "layer {number}: the light is {} and then {}, a step of {step} in a range of {range}",
                        strength_at(layer, pair[0]),
                        strength_at(layer, pair[1])
                    );
                }
            }
        }
    }

    #[test]
    fn the_light_is_at_its_brightest_at_the_top_left_corner_of_the_lines() {
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        let top_left_corner = Rect::from_min_size(clip().left_top(), vec2(RADIUS, RADIUS));
        for (number, layer) in layers.iter().enumerate().take(3) {
            let (brightest_point, brightest) = all_points(layer).map(|point| (point, strength_at(layer, point))).fold(
                (pos2(0.0, 0.0), f32::MIN),
                |best, each| if each.1 > best.1 { each } else { best },
            );
            assert!(
                top_left_corner.expand(1.0).contains(brightest_point),
                "layer {number}: the brightest of the lines, {brightest}, is at {brightest_point:?}, in the top left corner"
            );
            let along_the_top = Side::Top.signed_alpha_in_layer(number);
            let along_the_left = Side::Left.signed_alpha_in_layer(number);
            assert!(
                brightest > along_the_top.max(along_the_left) * 1.3,
                "layer {number}: and it is brighter there than along either edge: {brightest}"
            );
        }
    }

    #[test]
    fn every_line_stays_inside_the_clip_and_each_is_inside_the_one_before() {
        let layers = bevel_layers(clip(), RADIUS, 0.0, 1000.0);
        for layer in &layers {
            for point in all_points(layer) {
                assert!(clip().expand(0.01).contains(point), "{point:?}");
            }
        }
        for pair in layers.windows(2) {
            assert!(
                bounds_of(&pair[0])
                    .shrink(LAYER_WIDTH - 0.01)
                    .contains_rect(bounds_of(&pair[1])),
                "each line is a point inside the one before: {:?} and {:?}",
                bounds_of(&pair[0]),
                bounds_of(&pair[1])
            );
        }
    }

    #[test]
    fn a_clip_far_longer_than_the_view_only_has_lines_for_the_view() {
        let enormous = Rect::from_min_max(pos2(-5_000_000.0, 50.0), pos2(5_000_000.0, 154.0));
        let layers = bevel_layers(enormous, RADIUS, 0.0, 1000.0);
        for layer in &layers {
            for point in all_points(layer) {
                assert!(
                    point.x >= -MARGIN_BEYOND_THE_VIEW - 0.01 && point.x <= 1000.0 + MARGIN_BEYOND_THE_VIEW + 0.01,
                    "{point:?}"
                );
            }
        }
        let outermost = &layers[0];
        let [along_the_top, along_the_bottom] = &outermost.runs[..] else {
            panic!("a line along the top and a line along the bottom: {:?}", outermost.runs);
        };
        for (run, name) in [(along_the_top, "top"), (along_the_bottom, "bottom")] {
            let (least, most) = run.iter().fold((f32::MAX, f32::MIN), |(least, most), point| {
                (least.min(point.x), most.max(point.x))
            });
            assert!(
                (least + MARGIN_BEYOND_THE_VIEW).abs() < 0.01 && (most - 1000.0 - MARGIN_BEYOND_THE_VIEW).abs() < 0.01,
                "the line along the {name} goes on past the view by the margin at both ends, so that the cut ends are not seen: {least} to {most}"
            );
        }
        assert!(along_the_top.iter().all(|point| (point.y - 50.5).abs() < 0.01));
        assert!(along_the_bottom.iter().all(|point| (point.y - 153.5).abs() < 0.01));
        assert_eq!(
            (
                strength_at(outermost, along_the_top[0]),
                strength_at(outermost, along_the_bottom[0])
            ),
            (150.0, -185.0),
            "light along the top and shade along the bottom"
        );
    }

    #[test]
    fn a_clip_whose_start_is_in_view_has_its_left_edge_lit_and_one_whose_start_is_not_does_not() {
        let starts_in_view = Rect::from_min_max(pos2(100.0, 50.0), pos2(5_000.0, 154.0));
        let lit = bevel_layers(starts_in_view, RADIUS, 0.0, 1000.0);
        assert!(
            all_points(&lit[0]).any(|point| point.x < 101.0),
            "the left edge is there"
        );

        let started_long_ago = Rect::from_min_max(pos2(-300.0, 50.0), pos2(5_000.0, 154.0));
        let unlit = bevel_layers(started_long_ago, RADIUS, 0.0, 1000.0);
        for (number, layer) in unlit.iter().enumerate() {
            assert!(
                all_points(layer).all(|point| point.x > -MARGIN_BEYOND_THE_VIEW - 0.01),
                "layer {number}: its left edge is out of view, so nothing of it is drawn"
            );
        }
        let outermost = &unlit[0];
        assert!(all_points(outermost).count() > 1, "the top is in view");
        let straight_along_the_top = outermost.runs.iter().all(|run| {
            run.iter()
                .all(|point| (point.y - 50.5).abs() < 0.01 || (point.y - 153.5).abs() < 0.01)
        });
        assert!(
            straight_along_the_top,
            "what is seen of the light is straight along the top (and the shade along the bottom), and no corner is rounded in view"
        );
    }

    #[test]
    fn a_clip_too_small_for_the_rounding_is_rounded_as_far_as_it_can_be_and_still_makes_sense() {
        for (width, height) in [(10.0, 104.0), (104.0, 10.0), (3.0, 3.0), (1.0, 1.0), (0.2, 50.0)] {
            let tiny = Rect::from_min_size(pos2(100.0, 50.0), vec2(width, height));
            for layer in bevel_layers(tiny, RADIUS, 0.0, 1000.0) {
                for point in all_points(&layer) {
                    assert!(
                        point.x.is_finite() && point.y.is_finite() && tiny.expand(0.01).contains(point),
                        "{width}x{height}: {point:?}"
                    );
                    let strength = strength_at(&layer, point);
                    assert!(
                        strength.is_finite() && strength.abs() <= 255.0,
                        "{width}x{height}: {point:?}: {strength}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_chain_that_leaves_the_view_and_comes_back_is_in_two_runs_each_ending_at_the_edge() {
        let chain = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(50.0, 0.0),
            pos2(10.0, 5.0),
            pos2(0.0, 5.0),
        ];
        let runs = cut_to_the_view(&chain, 0.0, 20.0);
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert_eq!(runs[0], vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(20.0, 0.0)]);
        assert_eq!(runs[1], vec![pos2(20.0, 3.75), pos2(10.0, 5.0), pos2(0.0, 5.0)]);
    }

    #[test]
    fn a_piece_that_comes_into_the_view_from_either_side_or_goes_right_across_it_is_cut_where_it_crosses_the_edges() {
        assert_eq!(
            cut_to_the_view(&[pos2(-10.0, 0.0), pos2(10.0, 10.0)], 0.0, 20.0),
            vec![vec![pos2(0.0, 5.0), pos2(10.0, 10.0)]],
            "from before the left edge"
        );
        assert_eq!(
            cut_to_the_view(&[pos2(30.0, 0.0), pos2(10.0, 10.0)], 0.0, 20.0),
            vec![vec![pos2(20.0, 5.0), pos2(10.0, 10.0)]],
            "from past the right edge"
        );
        assert_eq!(
            cut_to_the_view(&[pos2(-10.0, 0.0), pos2(30.0, 40.0)], 0.0, 20.0),
            vec![vec![pos2(0.0, 10.0), pos2(20.0, 30.0)]],
            "right across it"
        );
    }

    #[test]
    fn a_chain_wholly_in_view_is_left_as_it_is_and_one_wholly_out_of_view_is_nothing() {
        let inside = [pos2(1.0, 1.0), pos2(5.0, 2.0), pos2(9.0, 3.0)];
        assert_eq!(cut_to_the_view(&inside, 0.0, 20.0), vec![inside.to_vec()]);
        let outside = [pos2(30.0, 1.0), pos2(35.0, 2.0)];
        assert!(cut_to_the_view(&outside, 0.0, 20.0).is_empty());
        assert!(cut_to_the_view(&[], 0.0, 20.0).is_empty());
        assert!(
            cut_to_the_view(&[pos2(5.0, 5.0)], 0.0, 20.0).is_empty(),
            "a single point is no line"
        );
    }
}
