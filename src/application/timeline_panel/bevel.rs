//! The raised look of a clip: a light line along its top and left edges and a dark one along its
//! bottom and right edges, fading towards the middle, so that two clips of the same dark colour can
//! still be told apart where they meet.
//!
//! The left edge's light and the right edge's shade are thicker than the top's and the bottom's: it
//! is along those two that clips lie next to one another, and where two clips of one dark colour
//! meet is where the boundary between them has to show.
//!
//! This is only geometry, as lines to be stroked. A clip can be far longer than the window, so the
//! lines are cut to the stretch that can be seen: their ends out of view are never given.

use egui::{Color32, Pos2, Rect, pos2};

/// How far each line is inside the one before it, in points, and how thick it is.
pub(super) const LAYER_WIDTH: f32 = 1.0;

/// How many straight pieces a quarter of a rounded corner is drawn as.
const PIECES_PER_QUARTER_TURN: usize = 8;

/// How far past the ends of the view the lines are let go on, in points, so that the cut ends are not seen.
const MARGIN_BEYOND_THE_VIEW: f32 = 4.0;

/// One of the four edges of a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

impl Side {
    /// How opaque the line of each layer is along this edge, from the edge of the clip inwards: there
    /// is a layer, one point thick, for each number. The light along the top and the left edge is
    /// white, and the shade along the bottom and the right edge is black.
    fn alphas(self) -> &'static [u8] {
        match self {
            Self::Top => &[150, 76, 30],
            Self::Bottom => &[185, 105, 46],
            Self::Left => &[150, 118, 88, 60, 36, 16],
            Self::Right => &[185, 148, 112, 78, 48, 22],
        }
    }

    fn colour_with_alpha(self, alpha: u8) -> Color32 {
        match self {
            Self::Left | Self::Top => Color32::from_white_alpha(alpha),
            Self::Right | Self::Bottom => Color32::from_black_alpha(alpha),
        }
    }
}

/// The most layers any edge has.
const MOST_LAYERS: usize = 6;

/// One fading line of the bevel: where it runs, and how it is coloured.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct BevelLine {
    /// The points the line goes through. A clip that is partly out of view has pieces of line, so
    /// there may be more than one run of points.
    pub runs: Vec<Vec<Pos2>>,
    pub colour: Color32,
}

/// The lines that give a clip its raised look, from the outermost inwards: light ones along the top
/// and the left edge, and dark ones along the bottom and the right edge. `clip` is the whole clip
/// (which may stretch far beyond the window), rounded at its corners by `radius`, and only the
/// stretch of it between `view_left` and `view_right` is given.
pub(super) fn bevel_lines(clip: Rect, radius: f32, view_left: f32, view_right: f32) -> Vec<BevelLine> {
    let mut lines = Vec::new();
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
        for (side, chain) in chains_of_the_outline(inside, corner) {
            let Some(alpha) = side.alphas().get(layer) else {
                continue;
            };
            lines.push(BevelLine {
                runs: cut_to_the_view(
                    &chain,
                    view_left - MARGIN_BEYOND_THE_VIEW,
                    view_right + MARGIN_BEYOND_THE_VIEW,
                ),
                colour: side.colour_with_alpha(*alpha),
            });
        }
    }
    lines
}

/// The outline of the rounded rectangle `rectangle` in four chains of points, one for each edge, in
/// the order left, top, right, bottom. Each has the straight edge and, at each end of it, the half
/// of the rounded corner that is nearer to it, so that the chains go on from one another all the way
/// round: the left one from the middle of the bottom left corner to the middle of the top left, the
/// top one from there to the middle of the top right, and so on.
fn chains_of_the_outline(rectangle: Rect, radius: f32) -> [(Side, Vec<Pos2>); 4] {
    let top_left = pos2(rectangle.left() + radius, rectangle.top() + radius);
    let top_right = pos2(rectangle.right() - radius, rectangle.top() + radius);
    let bottom_right = pos2(rectangle.right() - radius, rectangle.bottom() - radius);
    let bottom_left = pos2(rectangle.left() + radius, rectangle.bottom() - radius);

    // Angles are in degrees, measured from the right and going clockwise on the screen.
    let chain = |first: (Pos2, f32, f32), second: (Pos2, f32, f32)| {
        let mut points = arc(first.0, radius, first.1, first.2);
        points.extend(arc(second.0, radius, second.1, second.2));
        points
    };
    [
        (Side::Left, chain((bottom_left, 135.0, 180.0), (top_left, 180.0, 225.0))),
        (Side::Top, chain((top_left, 225.0, 270.0), (top_right, 270.0, 315.0))),
        (Side::Right, chain((top_right, 315.0, 360.0), (bottom_right, 0.0, 45.0))),
        (
            Side::Bottom,
            chain((bottom_right, 45.0, 90.0), (bottom_left, 90.0, 135.0)),
        ),
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
    let mut current: Vec<Pos2> = Vec::new();
    for pair in chain.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        match clip_piece_to_the_view(from, to, left, right) {
            Some((start, end)) => {
                // A piece that starts where the one before it ended carries on the same run.
                if current.last() != Some(&start) {
                    if current.len() > 1 {
                        runs.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                    current.push(start);
                }
                current.push(end);
            }
            None => {
                if current.len() > 1 {
                    runs.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
        }
    }
    if current.len() > 1 {
        runs.push(current);
    }
    runs
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

    use super::*;

    fn clip() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0))
    }

    fn all_points(line: &BevelLine) -> impl Iterator<Item = Pos2> + '_ {
        line.runs.iter().flatten().copied()
    }

    /// The lines that are light (they are white) and those that are dark.
    fn light_and_dark(lines: &[BevelLine]) -> (Vec<&BevelLine>, Vec<&BevelLine>) {
        lines
            .iter()
            .partition(|line| line.colour == Color32::from_white_alpha(line.colour.a()))
    }

    /// How far the points of the lines reach each way: the left, the top, the right and the bottom.
    fn extent(lines: &[&BevelLine]) -> (f32, f32, f32, f32) {
        let points: Vec<Pos2> = lines.iter().flat_map(|line| all_points(line)).collect();
        let smallest = |value: fn(&Pos2) -> f32| points.iter().map(value).fold(f32::MAX, f32::min);
        let largest = |value: fn(&Pos2) -> f32| points.iter().map(value).fold(f32::MIN, f32::max);
        (
            smallest(|point| point.x),
            smallest(|point| point.y),
            largest(|point| point.x),
            largest(|point| point.y),
        )
    }

    const SIDES: [Side; 4] = [Side::Left, Side::Top, Side::Right, Side::Bottom];

    /// The lines of the layer `layer` (0 is the outermost): there is one for each edge that has that
    /// layer, in the order left, top, right, bottom. (Every edge has the first three layers.)
    fn lines_of_layer(lines: &[BevelLine], layer: usize) -> &[BevelLine] {
        let before: usize = (0..layer).map(edges_with_layer).sum();
        &lines[before..before + edges_with_layer(layer)]
    }

    fn edges_with_layer(layer: usize) -> usize {
        SIDES.into_iter().filter(|side| side.alphas().len() > layer).count()
    }

    /// The line along `side` in layer `layer`.
    fn line_of(lines: &[BevelLine], layer: usize, side: Side) -> &BevelLine {
        let position = SIDES
            .into_iter()
            .filter(|candidate| candidate.alphas().len() > layer)
            .position(|candidate| candidate == side)
            .expect("the edge has that layer");
        &lines_of_layer(lines, layer)[position]
    }

    #[test]
    fn light_runs_along_the_top_and_the_left_and_dark_along_the_bottom_and_the_right() {
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        let (light, dark) = light_and_dark(&lines);
        let (light_layers, dark_layers) = (
            Side::Left.alphas().len() + Side::Top.alphas().len(),
            Side::Right.alphas().len() + Side::Bottom.alphas().len(),
        );
        assert_eq!((light.len(), dark.len()), (light_layers, dark_layers));

        // (The straight edges between the points of the corners are not points themselves, so what
        // is looked at is how far the points reach each way.)
        let (left, top, right, bottom) = extent(&light);
        assert!(
            left < 101.0 && top < 51.0,
            "the light reaches the left edge and the top edge"
        );
        assert!(
            right < 395.0 && bottom < 150.0,
            "and nothing of the right edge or the bottom edge: {right} {bottom}"
        );

        let (left, top, right, bottom) = extent(&dark);
        assert!(
            right > 399.0 && bottom > 153.0,
            "the dark reaches the right edge and the bottom edge"
        );
        assert!(
            left > 105.0 && top > 55.0,
            "and nothing of the left edge or the top edge: {left} {top}"
        );
    }

    #[test]
    fn the_left_edge_is_lit_more_thickly_than_the_top_and_the_right_edge_is_shaded_more_thickly_than_the_bottom() {
        assert!(Side::Left.alphas().len() >= 2 * Side::Top.alphas().len());
        assert!(Side::Right.alphas().len() >= 2 * Side::Bottom.alphas().len());
        let strength = |side: Side| side.alphas().iter().map(|alpha| u32::from(*alpha)).sum::<u32>();
        assert!(strength(Side::Left) > strength(Side::Top) * 3 / 2);
        assert!(strength(Side::Right) > strength(Side::Bottom) * 3 / 2);
        for side in [Side::Left, Side::Top, Side::Right, Side::Bottom] {
            let alphas = side.alphas();
            assert!(
                alphas.windows(2).all(|pair| pair[0] > pair[1]),
                "{side:?} fades inwards"
            );
            assert!(alphas.len() <= MOST_LAYERS);
        }
        // What is drawn: layers 3 and on are along the left edge and the right edge only.
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        let extra = lines_of_layer(&lines, 3);
        assert_eq!(extra.len(), 2);
        let (light, dark) = light_and_dark(&lines[lines.len() - 2 * (MOST_LAYERS - 3)..]);
        let (left, top, right, bottom) = extent(&light);
        assert!(
            right < 130.0 && top > 52.0 && bottom < 153.0,
            "the extra light is only down the left edge, and not along the top: {left} {top} {right} {bottom}"
        );
        let (left, top, right, bottom) = extent(&dark);
        assert!(
            left > 370.0 && top > 52.0 && bottom < 152.0,
            "the extra shade is only down the right edge: {left} {top} {right} {bottom}"
        );
    }

    #[test]
    fn the_lines_go_on_from_one_another_all_the_way_round_the_outline() {
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        for layer in 0..3 {
            let [left, top, right, bottom] = lines_of_layer(&lines, layer) else {
                panic!("the first three layers have a line for every edge");
            };
            let end_of = |line: &BevelLine| *line.runs[0].last().unwrap();
            let start_of = |line: &BevelLine| *line.runs[0].first().unwrap();
            assert!(
                end_of(left).distance(start_of(top)) < 0.01,
                "layer {layer}: at the top left"
            );
            assert!(
                end_of(top).distance(start_of(right)) < 0.01,
                "layer {layer}: at the top right"
            );
            assert!(
                end_of(right).distance(start_of(bottom)) < 0.01,
                "layer {layer}: at the bottom right"
            );
            assert!(
                end_of(bottom).distance(start_of(left)) < 0.01,
                "layer {layer}: at the bottom left"
            );
        }
    }

    #[test]
    fn every_line_stays_inside_the_clip_and_each_is_inside_the_one_before() {
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        for line in &lines {
            for point in all_points(line) {
                assert!(clip().expand(0.01).contains(point), "{point:?}");
            }
        }
        let smallest_x = |line: &BevelLine| all_points(line).map(|point| point.x).fold(f32::MAX, f32::min);
        let smallest_y = |line: &BevelLine| all_points(line).map(|point| point.y).fold(f32::MAX, f32::min);
        let largest_x = |line: &BevelLine| all_points(line).map(|point| point.x).fold(f32::MIN, f32::max);
        let largest_y = |line: &BevelLine| all_points(line).map(|point| point.y).fold(f32::MIN, f32::max);
        for side in SIDES {
            for layer in 0..side.alphas().len() - 1 {
                let (outer, inner) = (line_of(&lines, layer, side), line_of(&lines, layer + 1, side));
                let goes_inwards = match side {
                    Side::Left => smallest_x(inner) > smallest_x(outer),
                    Side::Top => smallest_y(inner) > smallest_y(outer),
                    Side::Right => largest_x(inner) < largest_x(outer),
                    Side::Bottom => largest_y(inner) < largest_y(outer),
                };
                assert!(goes_inwards, "{side:?}: layer {} is inside layer {layer}", layer + 1);
            }
        }
    }

    #[test]
    fn a_clip_far_longer_than_the_view_only_has_lines_for_the_view() {
        let enormous = Rect::from_min_max(pos2(-5_000_000.0, 50.0), pos2(5_000_000.0, 154.0));
        let lines = bevel_lines(enormous, 27.0, 0.0, 1000.0);
        for line in &lines {
            for point in all_points(line) {
                assert!(
                    point.x >= -MARGIN_BEYOND_THE_VIEW - 0.01 && point.x <= 1000.0 + MARGIN_BEYOND_THE_VIEW + 0.01,
                    "{point:?}"
                );
            }
        }
        let along_the_top = line_of(&lines, 0, Side::Top);
        assert_eq!(along_the_top.runs.len(), 1, "one line along the top");
        assert!(all_points(along_the_top).all(|point| (point.y - 50.5).abs() < 0.01));
        let along_the_bottom = line_of(&lines, 0, Side::Bottom);
        assert_eq!(along_the_bottom.runs.len(), 1, "one line along the bottom");
        assert!(all_points(along_the_bottom).all(|point| (point.y - 153.5).abs() < 0.01));
    }

    #[test]
    fn a_clip_whose_start_is_in_view_has_its_left_edge_lit_and_one_whose_start_is_not_does_not() {
        let starts_in_view = Rect::from_min_max(pos2(100.0, 50.0), pos2(5_000.0, 154.0));
        let lit = bevel_lines(starts_in_view, 27.0, 0.0, 1000.0);
        assert!(
            all_points(line_of(&lit, 0, Side::Left)).any(|point| point.x < 101.0),
            "the left edge is there"
        );

        let started_long_ago = Rect::from_min_max(pos2(-300.0, 50.0), pos2(5_000.0, 154.0));
        let unlit = bevel_lines(started_long_ago, 27.0, 0.0, 1000.0);
        for layer in 0..Side::Left.alphas().len() {
            assert_eq!(
                all_points(line_of(&unlit, layer, Side::Left)).count(),
                0,
                "its left edge is out of view, so nothing of it is drawn"
            );
        }
        let along_the_top = line_of(&unlit, 0, Side::Top);
        assert!(all_points(along_the_top).count() > 1, "the top is in view");
        assert!(
            all_points(along_the_top)
                .all(|point| point.x > -MARGIN_BEYOND_THE_VIEW - 0.01 && (point.y - 50.5).abs() < 0.01),
            "what is seen of the light is straight along the top, and no corner is rounded in view"
        );
    }

    #[test]
    fn a_clip_too_small_for_the_rounding_is_rounded_as_far_as_it_can_be_and_still_makes_sense() {
        for (width, height) in [(10.0, 104.0), (104.0, 10.0), (3.0, 3.0), (1.0, 1.0), (0.2, 50.0)] {
            let tiny = Rect::from_min_size(pos2(100.0, 50.0), vec2(width, height));
            for line in bevel_lines(tiny, 27.0, 0.0, 1000.0) {
                for point in all_points(&line) {
                    assert!(
                        point.x.is_finite() && point.y.is_finite() && tiny.expand(0.01).contains(point),
                        "{width}x{height}: {point:?}"
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
