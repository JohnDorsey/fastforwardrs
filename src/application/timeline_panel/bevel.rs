//! The raised look of a clip: a light line along its top and left edges and a dark one along its
//! bottom and right edges, fading towards the middle, so that two clips of the same dark colour can
//! still be told apart where they meet.
//!
//! This is only geometry, as lines to be stroked. A clip can be far longer than the window, so the
//! lines are cut to the stretch that can be seen: their ends out of view are never given.

use egui::{Color32, Pos2, Rect, pos2};

/// How many lines are drawn, each inside the last, fading.
const LAYER_COUNT: usize = 3;

/// How far each line is inside the one before it, in points, and how thick it is.
pub(super) const LAYER_WIDTH: f32 = 1.0;

/// How opaque the lines are, from the edge of the clip inwards.
const LIGHT_ALPHAS: [u8; LAYER_COUNT] = [150, 76, 30];
const SHADE_ALPHAS: [u8; LAYER_COUNT] = [185, 105, 46];

/// How many straight pieces a quarter of a rounded corner is drawn as.
const PIECES_PER_QUARTER_TURN: usize = 8;

/// How far past the ends of the view the lines are let go on, in points, so that the cut ends are not seen.
const MARGIN_BEYOND_THE_VIEW: f32 = 4.0;

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
    for layer in 0..LAYER_COUNT {
        let inset = (layer as f32 + 0.5) * LAYER_WIDTH;
        let inside = clip.shrink(inset);
        if inside.width() <= 0.0 || inside.height() <= 0.0 {
            break;
        }
        let corner = (radius - inset)
            .max(0.0)
            .min(inside.width() / 2.0)
            .min(inside.height() / 2.0);
        let (light, shade) = chains_of_the_outline(inside, corner);
        let visible = |chain: Vec<Pos2>| {
            cut_to_the_view(
                &chain,
                view_left - MARGIN_BEYOND_THE_VIEW,
                view_right + MARGIN_BEYOND_THE_VIEW,
            )
        };
        lines.push(BevelLine {
            runs: visible(light),
            colour: Color32::from_white_alpha(LIGHT_ALPHAS[layer]),
        });
        lines.push(BevelLine {
            runs: visible(shade),
            colour: Color32::from_black_alpha(SHADE_ALPHAS[layer]),
        });
    }
    lines
}

/// The outline of the rounded rectangle `rectangle` in two chains of points: the light one, from
/// the middle of the bottom left corner up the left edge and along the top edge to the middle of
/// the top right corner, and the shade one, which is all the rest of the way round.
fn chains_of_the_outline(rectangle: Rect, radius: f32) -> (Vec<Pos2>, Vec<Pos2>) {
    let top_left = pos2(rectangle.left() + radius, rectangle.top() + radius);
    let top_right = pos2(rectangle.right() - radius, rectangle.top() + radius);
    let bottom_right = pos2(rectangle.right() - radius, rectangle.bottom() - radius);
    let bottom_left = pos2(rectangle.left() + radius, rectangle.bottom() - radius);

    // Angles are in degrees, measured from the right and going clockwise on the screen.
    let mut light = arc(bottom_left, radius, 135.0, 180.0);
    light.extend(arc(top_left, radius, 180.0, 270.0));
    light.extend(arc(top_right, radius, 270.0, 315.0));

    let mut shade = arc(top_right, radius, 315.0, 360.0);
    shade.extend(arc(bottom_right, radius, 0.0, 90.0));
    shade.extend(arc(bottom_left, radius, 90.0, 135.0));
    (light, shade)
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

    /// The lines that are light (they start with a white colour) and those that are dark.
    fn light_and_dark(lines: &[BevelLine]) -> (Vec<&BevelLine>, Vec<&BevelLine>) {
        lines
            .iter()
            .partition(|line| line.colour == Color32::from_white_alpha(line.colour.a()))
    }

    #[test]
    fn light_runs_along_the_top_and_the_left_and_dark_along_the_bottom_and_the_right() {
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        let (light, dark) = light_and_dark(&lines);
        assert_eq!((light.len(), dark.len()), (LAYER_COUNT, LAYER_COUNT));

        // (The straight edges between the points of the corners are not points themselves, so what
        // is looked at is how far the points reach each way.)
        let extent = |line: &BevelLine| {
            let points: Vec<Pos2> = all_points(line).collect();
            let smallest = |value: fn(&Pos2) -> f32| points.iter().map(value).fold(f32::MAX, f32::min);
            let largest = |value: fn(&Pos2) -> f32| points.iter().map(value).fold(f32::MIN, f32::max);
            (
                smallest(|point| point.x),
                smallest(|point| point.y),
                largest(|point| point.x),
                largest(|point| point.y),
            )
        };
        let (left, top, right, bottom) = extent(light[0]);
        assert!(
            left < 101.0 && top < 51.0,
            "the light reaches the left edge and the top edge"
        );
        assert!(
            right < 395.0 && bottom < 150.0,
            "and nothing of the right edge or the bottom edge"
        );

        let (left, top, right, bottom) = extent(dark[0]);
        assert!(
            right > 399.0 && bottom > 153.0,
            "the dark reaches the right edge and the bottom edge"
        );
        assert!(
            left > 105.0 && top > 55.0,
            "and nothing of the left edge or the top edge"
        );
    }

    #[test]
    fn the_light_and_the_dark_meet_at_the_two_corners_that_are_neither() {
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        let (light, dark) = light_and_dark(&lines);
        let (light_run, dark_run) = (&light[0].runs[0], &dark[0].runs[0]);
        assert!(
            light_run.last().unwrap().distance(*dark_run.first().unwrap()) < 0.01,
            "at the top right"
        );
        assert!(
            dark_run.last().unwrap().distance(*light_run.first().unwrap()) < 0.01,
            "at the bottom left"
        );
    }

    #[test]
    fn every_line_stays_inside_the_clip_and_each_is_inside_the_one_before() {
        let lines = bevel_lines(clip(), 27.0, 0.0, 1000.0);
        for line in &lines {
            for point in all_points(line) {
                assert!(clip().expand(0.01).contains(point), "{point:?}");
            }
        }
        let outer: Vec<Pos2> = all_points(&lines[0]).collect();
        let inner: Vec<Pos2> = all_points(&lines[2]).collect();
        let top_of = |points: &[Pos2]| points.iter().map(|point| point.y).fold(f32::MAX, f32::min);
        assert!(top_of(&inner) > top_of(&outer), "the lines go inwards");
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
        let (light, dark) = light_and_dark(&lines);
        assert_eq!(light[0].runs.len(), 1, "one line along the top");
        assert!(all_points(light[0]).all(|point| (point.y - 50.5).abs() < 0.01));
        assert_eq!(dark[0].runs.len(), 1, "one line along the bottom");
        assert!(all_points(dark[0]).all(|point| (point.y - 153.5).abs() < 0.01));
    }

    #[test]
    fn a_clip_whose_start_is_in_view_has_its_left_edge_lit_and_one_whose_start_is_not_does_not() {
        let starts_in_view = Rect::from_min_max(pos2(100.0, 50.0), pos2(5_000.0, 154.0));
        let lit = bevel_lines(starts_in_view, 27.0, 0.0, 1000.0);
        assert!(
            all_points(&lit[0]).any(|point| point.x < 101.0),
            "the left edge is there"
        );

        let started_long_ago = Rect::from_min_max(pos2(-300.0, 50.0), pos2(5_000.0, 154.0));
        let unlit = bevel_lines(started_long_ago, 27.0, 0.0, 1000.0);
        assert!(
            all_points(&unlit[0]).all(|point| point.x > -MARGIN_BEYOND_THE_VIEW - 0.01),
            "its left edge is out of view, so nothing of it is drawn, and no corner is rounded in view"
        );
        assert!(
            all_points(&unlit[0]).all(|point| (point.y - 50.5).abs() < 0.01),
            "what is seen of the light is straight along the top"
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
