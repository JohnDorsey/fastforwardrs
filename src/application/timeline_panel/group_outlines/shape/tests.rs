use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// How round the corners that turn outwards and the corners that turn inwards are made, for the tests that give every piece the same radius.
#[derive(Clone, Copy)]
struct Rounding {
    convex: f32,
    concave: f32,
}

/// The outlines of `rectangles`, each of them a piece that is as round at its corners as `rounding.convex`.
fn outlines_of(rectangles: &[Rect], rounding: Rounding) -> Vec<Vec<Pos2>> {
    let pieces: Vec<Piece> = rectangles
        .iter()
        .map(|rectangle| Piece {
            rectangle: *rectangle,
            radius: rounding.convex,
        })
        .collect();
    super::outlines_of(&pieces, rounding.concave)
}

fn rectangle(left: f32, top: f32, right: f32, bottom: f32) -> Rect {
    Rect::from_min_max(pos2(left, top), pos2(right, bottom))
}

const SHARP: Rounding = Rounding {
    convex: 0.0,
    concave: 0.0,
};

/// The area that a loop goes round, which is positive for a loop that goes round clockwise on the screen (the shoelace sum, with the y that
/// goes down).
fn signed_area(points: &[Pos2]) -> f32 {
    let count = points.len();
    (0..count)
        .map(|at| {
            let (here, next) = (points[at], points[(at + 1) % count]);
            here.x * next.y - next.x * here.y
        })
        .sum::<f32>()
        / 2.0
}

/// Whether `point` is inside the loop (the even-odd rule: a line from it to the right crosses the loop an odd number of times).
fn is_inside(point: Pos2, points: &[Pos2]) -> bool {
    let count = points.len();
    let mut inside = false;
    for at in 0..count {
        let (a, b) = (points[at], points[(at + 1) % count]);
        if (a.y > point.y) != (b.y > point.y) {
            let crossing_x = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if crossing_x > point.x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Whether two straight pieces cross one another (not counting them touching at an end that they have in common).
fn cross(first: (Pos2, Pos2), second: (Pos2, Pos2)) -> bool {
    let side =
        |from: Pos2, to: Pos2, point: Pos2| (to.x - from.x) * (point.y - from.y) - (to.y - from.y) * (point.x - from.x);
    let (a, b) = first;
    let (c, d) = second;
    let (first_c, first_d) = (side(a, b, c), side(a, b, d));
    let (second_a, second_b) = (side(c, d, a), side(c, d, b));
    first_c * first_d < -1e-6 && second_a * second_b < -1e-6
}

/// Whether a loop crosses itself.
fn crosses_itself(points: &[Pos2]) -> bool {
    let count = points.len();
    for first in 0..count {
        for second in (first + 2)..count {
            if first == 0 && second == count - 1 {
                continue;
            }
            let pieces = (
                (points[first], points[(first + 1) % count]),
                (points[second], points[(second + 1) % count]),
            );
            if cross(pieces.0, pieces.1) {
                return true;
            }
        }
    }
    false
}

#[test]
fn nothing_has_no_outline_and_neither_do_rectangles_that_have_no_room_in_them() {
    assert!(outlines_of(&[], SHARP).is_empty());
    assert!(outlines_of(&[rectangle(5.0, 5.0, 5.0, 40.0), rectangle(1.0, 1.0, 9.0, 1.0)], SHARP).is_empty());
}

#[test]
fn one_rectangle_has_an_outline_that_is_its_four_corners_going_round_clockwise() {
    let loops = outlines_of(&[rectangle(10.0, 20.0, 110.0, 70.0)], SHARP);

    assert_eq!(loops.len(), 1);
    assert_eq!(loops[0].len(), 4);
    for corner in [pos2(10.0, 20.0), pos2(110.0, 20.0), pos2(110.0, 70.0), pos2(10.0, 70.0)] {
        assert!(loops[0].contains(&corner), "{corner:?} in {:?}", loops[0]);
    }
    assert_eq!(
        signed_area(&loops[0]),
        5000.0,
        "clockwise, and round exactly what it is round"
    );
}

#[test]
fn the_corners_of_a_rectangle_are_rounded_as_much_as_was_asked_and_the_outline_stays_inside_it() {
    let rounding = Rounding {
        convex: 12.0,
        concave: 0.0,
    };
    let loops = outlines_of(&[rectangle(10.0, 20.0, 110.0, 70.0)], rounding);

    assert_eq!(loops.len(), 1);
    let outline = &loops[0];
    assert_eq!(outline.len(), 4 * (PIECES_IN_A_QUARTER_TURN + 1));
    // It is a rectangle with the corners cut off round: the area is that less what four such corners take (a square of the radius, less a
    // quarter of a circle of it, at each).
    let corners_taken = 4.0 * (12.0 * 12.0 - std::f32::consts::PI * 12.0 * 12.0 / 4.0);
    // (The arcs are drawn as straight pieces that are a little inside the arc: so it is a little less, by under a point of the length of each.)
    let area = signed_area(outline);
    assert!(
        (area - (5000.0 - corners_taken)).abs() < 4.0,
        "{area} and {}",
        5000.0 - corners_taken
    );
    for point in outline {
        assert!(
            rectangle(10.0, 20.0, 110.0, 70.0).expand(0.01).contains(*point),
            "{point:?} is outside the rectangle"
        );
    }
    // The point of the rectangle's own corner is cut off: it is not inside.
    assert!(!is_inside(pos2(10.5, 20.5), outline));
    assert!(is_inside(pos2(60.0, 45.0), outline));
}

#[test]
fn a_corner_is_only_as_round_as_half_of_the_shorter_edge_next_to_it() {
    // 30 across: the corners can be 15 round at the most, whatever was asked for.
    let rounding = Rounding {
        convex: 100.0,
        concave: 0.0,
    };
    let loops = outlines_of(&[rectangle(0.0, 0.0, 30.0, 200.0)], rounding);

    assert_eq!(loops.len(), 1);
    // The loop reaches the edges: 15 down the long sides, where the rounding begins, and so is the shape of a capsule.
    assert!(is_inside(pos2(15.0, 1.0), &loops[0]));
    assert!(!is_inside(pos2(0.5, 0.5), &loops[0]));
    assert!(
        loops[0]
            .iter()
            .all(|point| (0.0..=30.0).contains(&point.x) && (0.0..=200.0).contains(&point.y))
    );
}

#[test]
fn corners_that_are_as_round_as_half_the_edge_between_them_meet_at_one_point_that_is_in_the_loop_once() {
    // 54 across: the two corners at the top are 27 round each and make a half circle, with no point twice in a row, and the loop does not
    // cross itself where they meet (it did, by a hair, when they were not made one).
    let rounding = Rounding {
        convex: 30.0,
        concave: 0.0,
    };

    let loops = outlines_of(&[rectangle(0.0, 0.0, 54.0, 110.0)], rounding);

    let outline = &loops[0];
    for at in 0..outline.len() {
        assert!(
            outline[at].distance(outline[(at + 1) % outline.len()]) > 0.001,
            "{at} of {outline:?}"
        );
    }
    assert!(!crosses_itself(outline));
    let highest = outline.iter().map(|point| point.y).fold(f32::INFINITY, f32::min);
    assert_eq!(highest, 0.0);
    assert_eq!(
        outline.iter().filter(|point| point.y == 0.0).count(),
        1,
        "one highest point, in the middle"
    );
}

#[test]
fn a_corner_is_as_round_as_the_piece_it_is_a_corner_of_and_the_pieces_that_share_it_are_all_gone_round() {
    // A narrow piece at the left end of a long one on the same row: the corner at the top left is the narrow piece's, and it is as round as
    // that is (9 at the most, with 18 across), though the long piece, if it were alone, would round its corners by 30.
    let narrow = Piece {
        rectangle: rectangle(0.0, 0.0, 18.0, 70.0),
        radius: 30.0,
    };
    let long = Piece {
        rectangle: rectangle(10.0, 0.0, 300.0, 70.0),
        radius: 30.0,
    };

    let loops = super::outlines_of(&[narrow, long], 0.0);

    // The arc at the top left begins 9 from the corner along the top and ends 9 down the left side.
    assert!(
        loops[0]
            .iter()
            .any(|point| (point.x - 9.0).abs() < 0.01 && point.y.abs() < 0.01),
        "{:?}",
        loops[0]
    );
    assert!(
        loops[0]
            .iter()
            .any(|point| point.x.abs() < 0.01 && (point.y - 9.0).abs() < 0.01),
        "{:?}",
        loops[0]
    );
    // The corner at the top right is the long piece's: 30 round.
    assert!(
        loops[0]
            .iter()
            .any(|point| (point.x - 270.0).abs() < 0.01 && point.y.abs() < 0.01)
    );

    // Two pieces that have the same corner: it is as round as the less round of them.
    let tight = Piece {
        rectangle: rectangle(0.0, 0.0, 100.0, 100.0),
        radius: 5.0,
    };
    let loose = Piece {
        rectangle: rectangle(0.0, 0.0, 200.0, 100.0),
        radius: 40.0,
    };
    let loops = super::outlines_of(&[tight, loose], 0.0);
    assert!(
        loops[0]
            .iter()
            .any(|point| (point.x - 5.0).abs() < 0.01 && point.y.abs() < 0.01),
        "{:?}",
        loops[0]
    );
}

#[test]
fn two_rectangles_that_overlap_make_one_shape_with_one_outline_whose_corners_are_those_of_the_shape() {
    // An L: a tall rectangle and a wide one that overlaps its lower part.
    let loops = outlines_of(
        &[rectangle(0.0, 0.0, 40.0, 100.0), rectangle(20.0, 60.0, 120.0, 100.0)],
        SHARP,
    );

    assert_eq!(loops.len(), 1);
    let mut corners = loops[0].clone();
    corners.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let mut wanted = vec![
        pos2(0.0, 0.0),
        pos2(40.0, 0.0),
        pos2(40.0, 60.0),
        pos2(120.0, 60.0),
        pos2(120.0, 100.0),
        pos2(0.0, 100.0),
    ];
    wanted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    assert_eq!(corners, wanted);
    assert_eq!(
        signed_area(&loops[0]),
        40.0 * 100.0 + 100.0 * 40.0 - 20.0 * 40.0,
        "the two, less where they are on each other"
    );
}

#[test]
fn rectangles_that_touch_along_an_edge_make_one_shape_too() {
    let loops = outlines_of(
        &[rectangle(0.0, 0.0, 50.0, 30.0), rectangle(50.0, 0.0, 90.0, 30.0)],
        SHARP,
    );

    assert_eq!(loops.len(), 1);
    assert_eq!(
        loops[0].len(),
        4,
        "the corners where they meet are not corners of the shape"
    );
    assert_eq!(signed_area(&loops[0]), 90.0 * 30.0);
}

#[test]
fn rectangles_that_are_apart_make_a_loop_each() {
    let loops = outlines_of(
        &[rectangle(0.0, 0.0, 20.0, 20.0), rectangle(100.0, 0.0, 120.0, 20.0)],
        SHARP,
    );

    assert_eq!(loops.len(), 2);
    for outline in &loops {
        assert_eq!(signed_area(outline), 400.0);
    }
}

#[test]
fn a_hole_in_the_shape_is_not_gone_round_and_only_the_outside_is() {
    // Four rectangles that make a frame round an empty place.
    let frame = [
        rectangle(0.0, 0.0, 100.0, 20.0),
        rectangle(0.0, 80.0, 100.0, 100.0),
        rectangle(0.0, 0.0, 20.0, 100.0),
        rectangle(80.0, 0.0, 100.0, 100.0),
    ];

    let loops = outlines_of(&frame, SHARP);

    assert_eq!(loops.len(), 1, "one loop, the outside: the hole has none");
    assert_eq!(
        signed_area(&loops[0]),
        100.0 * 100.0,
        "and it goes round the hole as well"
    );
}

#[test]
fn where_the_loop_turns_inwards_a_rounded_corner_fills_the_notch_in() {
    // An L again: the corner inside it, at (40, 60), is where the loop turns inwards.
    let rectangles = [rectangle(0.0, 0.0, 40.0, 100.0), rectangle(20.0, 60.0, 120.0, 100.0)];
    let sharp = outlines_of(&rectangles, SHARP);
    let rounding = Rounding {
        convex: 0.0,
        concave: 15.0,
    };

    let loops = outlines_of(&rectangles, rounding);

    assert_eq!(loops.len(), 1);
    assert!(
        signed_area(&loops[0]) > signed_area(&sharp[0]),
        "the shape gets bigger where the notch is filled: {} and {}",
        signed_area(&loops[0]),
        signed_area(&sharp[0])
    );
    // The place just beside the notch's corner, out in the notch, is in the filled shape; far out in the notch it is not.
    assert!(is_inside(pos2(41.5, 58.5), &loops[0]));
    assert!(!is_inside(pos2(41.5, 58.5), &sharp[0]));
    assert!(!is_inside(pos2(80.0, 30.0), &loops[0]));
}

#[test]
fn rectangles_that_touch_only_at_a_corner_are_gone_round_without_trouble() {
    let loops = outlines_of(
        &[rectangle(0.0, 0.0, 20.0, 20.0), rectangle(20.0, 20.0, 40.0, 40.0)],
        SHARP,
    );

    let total: f32 = loops.iter().map(|outline| signed_area(outline)).sum();
    assert_eq!(total, 800.0);
    assert!(loops.iter().all(|outline| !crosses_itself(outline)));
}

#[test]
fn the_edges_that_are_nearly_in_line_are_one_edge() {
    // The second rectangle is off by what arithmetic leaves: with no sliver between them, it is two rectangles that join.
    let loops = outlines_of(
        &[rectangle(0.0, 0.0, 50.0, 30.0), rectangle(49.999, 0.001, 90.0, 30.0)],
        SHARP,
    );

    assert_eq!(loops.len(), 1);
    assert_eq!(loops[0].len(), 4);
}

/// A random rectangle in a 300 by 200 place.
fn random_rectangle(random: &mut DeterministicRandomNumbers) -> Rect {
    let left = random.next_below(250) as f32;
    let top = random.next_below(160) as f32;
    let width = 6.0 + random.next_below(120) as f32;
    let height = 6.0 + random.next_below(80) as f32;
    rectangle(left, top, left + width, top + height)
}

/// The points round the edge of `rectangle` rounded at its corners by `radius` (as far as it has room for), a little inside it.
fn points_round_the_edge_of_a_rounded(rectangle: Rect, radius: f32) -> Vec<Pos2> {
    let radius = radius.min(rectangle.width() / 2.0).min(rectangle.height() / 2.0);
    let inner = rectangle.shrink(0.05);
    let mut points = Vec::new();
    for (corner, towards) in [
        (inner.left_top(), vec2(1.0, 1.0)),
        (inner.right_top(), vec2(-1.0, 1.0)),
        (inner.right_bottom(), vec2(-1.0, -1.0)),
        (inner.left_bottom(), vec2(1.0, -1.0)),
    ] {
        let centre = corner + towards * radius;
        for step in 0..=6 {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 6.0;
            // The arc from the side that is across to the side that is down, round the centre, on the corner's side of it.
            let away = vec2(-towards.x * angle.cos(), -towards.y * angle.sin());
            points.push(centre + away * (radius - 0.05));
        }
    }
    points.extend([
        inner.center_top(),
        inner.center_bottom(),
        inner.left_center(),
        inner.right_center(),
        inner.center(),
    ]);
    points
}

#[test]
fn whatever_rectangles_are_put_together_the_outline_is_simple_goes_round_clockwise_and_has_the_middle_of_each_of_them_inside_it()
 {
    let mut random = DeterministicRandomNumbers::with_seed(5);
    let rounding = Rounding {
        convex: 20.0,
        concave: 9.0,
    };
    for round in 0..300 {
        let rectangles: Vec<Rect> = (0..1 + random.next_below(6))
            .map(|_| random_rectangle(&mut random))
            .collect();
        let loops = outlines_of(&rectangles, rounding);
        let context = format!("round {round}: {rectangles:?}");

        assert!(!loops.is_empty(), "{context}");
        for outline in &loops {
            assert!(outline.len() >= 4, "{context}");
            assert!(signed_area(outline) > 0.0, "clockwise: {context}");
            assert!(!crosses_itself(outline), "{context}: {outline:?}");
        }
        // The rounded corners cut at most part of a rectangle's corner, never as deep as the radius across: what is that far in from each edge
        // is inside. (A rectangle that is not as wide and tall as twice the radius has nothing that far in.)
        for rectangle in &rectangles {
            let core = rectangle.shrink(rounding.convex);
            if core.width() < 0.0 || core.height() < 0.0 {
                continue;
            }
            for point in [
                core.left_top(),
                core.right_top(),
                core.right_bottom(),
                core.left_bottom(),
                core.center(),
            ] {
                assert!(
                    loops.iter().any(|outline| is_inside(point, outline)),
                    "{context}: {point:?} of {rectangle:?} is not inside any loop"
                );
            }
        }
        // With no rounding, the loops go round exactly the shape and its holes (never less than the area of the biggest rectangle).
        let sharp = outlines_of(&rectangles, SHARP);
        let total: f32 = sharp.iter().map(|outline| signed_area(outline)).sum();
        let biggest = rectangles.iter().map(|rectangle| rectangle.area()).fold(0.0, f32::max);
        assert!(total >= biggest - 1.0, "{context}");
    }
}

#[test]
fn the_outline_of_clips_on_rows_padded_and_rounded_as_the_clips_are_has_each_of_the_clips_whole_inside_it() {
    // The rows are as tall as a track is (the shortest and the tallest, and between) and 6 points apart, the clips are rounded by 27 (or as
    // much as their height allows), and the outline is 3 points out from them and as round as that.
    const PADDING: f32 = 3.0;
    const CLIP_RADIUS: f32 = 27.0;
    let rounding = Rounding {
        convex: CLIP_RADIUS + PADDING,
        concave: 8.0,
    };
    let mut random = DeterministicRandomNumbers::with_seed(17);
    for round in 0..300 {
        let row_height = [44.0, 70.0, 104.0, 156.0][random.next_below(4) as usize];
        let clips: Vec<Rect> = (0..1 + random.next_below(7))
            .map(|_| {
                let row = random.next_below(4) as f32;
                let top = row * (row_height + 6.0);
                let left = random.next_below(500) as f32;
                rectangle(left, top, left + 20.0 + random.next_below(220) as f32, top + row_height)
            })
            .collect();
        // (Each piece is as round as the clip is, and the padding more: a narrow clip is not as round as the others, and the outline is not
        // either where it goes round that clip.)
        let pieces: Vec<Piece> = clips
            .iter()
            .map(|clip| Piece {
                rectangle: clip.expand(PADDING),
                radius: CLIP_RADIUS + PADDING,
            })
            .collect();
        let loops = super::outlines_of(&pieces, rounding.concave);
        let context = format!("round {round}: {clips:?}");

        for outline in &loops {
            assert!(!crosses_itself(outline), "{context}");
        }
        for clip in &clips {
            for point in points_round_the_edge_of_a_rounded(*clip, CLIP_RADIUS) {
                assert!(
                    loops.iter().any(|outline| is_inside(point, outline)),
                    "{context}: {point:?} of {clip:?} is not inside any loop"
                );
            }
        }
    }
}

// --- spokes ---

#[test]
fn a_piece_on_its_own_or_none_needs_no_spoke_and_pieces_that_touch_need_none() {
    assert!(spokes_between(&[], 8.0).is_empty());
    assert!(spokes_between(&[vec![rectangle(0.0, 0.0, 10.0, 10.0)]], 8.0).is_empty());
    assert!(
        spokes_between(
            &[
                vec![rectangle(0.0, 0.0, 10.0, 10.0)],
                vec![rectangle(10.0, 0.0, 30.0, 10.0)]
            ],
            8.0
        )
        .is_empty()
    );
    assert!(
        spokes_between(
            &[
                vec![rectangle(0.0, 0.0, 10.0, 10.0)],
                vec![rectangle(5.0, 5.0, 30.0, 30.0)]
            ],
            8.0
        )
        .is_empty(),
        "on one another"
    );
    assert!(
        spokes_between(&[vec![], vec![rectangle(0.0, 0.0, 10.0, 10.0)]], 8.0).is_empty(),
        "a piece with nothing in it is not one"
    );
}

#[test]
fn pieces_side_by_side_are_joined_by_a_straight_spoke_across_the_rows_that_they_have_in_common() {
    let pieces = [
        vec![rectangle(0.0, 0.0, 20.0, 40.0)],
        vec![rectangle(100.0, 20.0, 130.0, 80.0)],
    ];

    let spokes = spokes_between(&pieces, 8.0);

    // The rows they have in common are 20 to 40, and the middle of those is 30; it goes from the middle of one (10) to the middle of the other (115).
    assert_eq!(spokes, vec![rectangle(10.0, 26.0, 115.0, 34.0)]);
}

#[test]
fn pieces_above_one_another_are_joined_by_a_straight_spoke_down_the_columns_that_they_have_in_common() {
    let pieces = [
        vec![rectangle(0.0, 0.0, 60.0, 20.0)],
        vec![rectangle(40.0, 100.0, 100.0, 120.0)],
    ];

    let spokes = spokes_between(&pieces, 6.0);

    assert_eq!(spokes, vec![rectangle(47.0, 10.0, 53.0, 110.0)]);
}

#[test]
fn pieces_that_are_not_in_the_same_rows_or_columns_are_joined_by_a_spoke_that_bends_once() {
    let pieces = [
        vec![rectangle(0.0, 0.0, 20.0, 20.0)],
        vec![rectangle(100.0, 100.0, 120.0, 120.0)],
    ];

    let spokes = spokes_between(&pieces, 6.0);

    assert_eq!(spokes.len(), 2);
    // Along the row of the first, from its middle to the column of the middle of the second, and down that column.
    assert_eq!(spokes[0], rectangle(7.0, 7.0, 113.0, 13.0));
    assert_eq!(spokes[1], rectangle(107.0, 7.0, 113.0, 113.0));
}

#[test]
fn the_shortest_set_of_joins_is_made_and_no_more() {
    // Three in a row: the first and the last are not joined to each other.
    let pieces = [
        vec![rectangle(0.0, 0.0, 10.0, 10.0)],
        vec![rectangle(50.0, 0.0, 60.0, 10.0)],
        vec![rectangle(100.0, 0.0, 110.0, 10.0)],
    ];

    let spokes = spokes_between(&pieces, 4.0);

    assert_eq!(spokes.len(), 2);
    assert!(spokes.iter().all(|spoke| spoke.width() <= 55.0), "{spokes:?}");
}

/// [`spokes_between`] as it was first written, which looks at each piece that is in against each piece that is not in, again every time one more
/// comes in: so slow (the time it takes goes up with the cube of the number of pieces: a tag of 300 clips took half a second, and one of 600 four
/// seconds, in a build for testing) that it is only what the faster way of finding the same spokes is compared with.
fn spokes_the_slow_way(components: &[Vec<Rect>], width: f32) -> Vec<Rect> {
    let components: Vec<&Vec<Rect>> = components.iter().filter(|component| !component.is_empty()).collect();
    if components.len() < 2 {
        return Vec::new();
    }
    let nearest = |first: &[Rect], second: &[Rect]| -> (f32, Rect, Rect) {
        let mut best: Option<(f32, Rect, Rect)> = None;
        for a in first {
            for b in second {
                let distance = distance_between(*a, *b);
                if best.is_none_or(|(so_far, _, _)| distance < so_far) {
                    best = Some((distance, *a, *b));
                }
            }
        }
        best.expect("a component has a rectangle")
    };
    let mut joined = vec![false; components.len()];
    joined[0] = true;
    let mut spokes = Vec::new();
    for _ in 1..components.len() {
        let mut best: Option<(f32, usize, Rect, Rect)> = None;
        for inside in (0..components.len()).filter(|index| joined[*index]) {
            for outside in (0..components.len()).filter(|index| !joined[*index]) {
                let (distance, a, b) = nearest(components[inside], components[outside]);
                if best.is_none_or(|(so_far, _, _, _)| distance < so_far) {
                    best = Some((distance, outside, a, b));
                }
            }
        }
        let (distance, coming_in, a, b) = best.expect("there is a piece that is not joined yet");
        joined[coming_in] = true;
        if distance > 0.0 {
            spokes.extend(spoke_between(a, b, width));
        }
    }
    spokes
}

/// A rectangle with its corners on a coarse grid, so that many pairs of rectangles are exactly as far apart as many others are.
fn rectangle_on_a_coarse_grid(random: &mut DeterministicRandomNumbers) -> Rect {
    let left = random.next_below(8) as f32 * 50.0;
    let top = random.next_below(8) as f32 * 50.0;
    let width = 10.0 + random.next_below(3) as f32 * 10.0;
    let height = 10.0 + random.next_below(3) as f32 * 10.0;
    rectangle(left, top, left + width, top + height)
}

#[test]
fn the_spokes_are_the_ones_that_the_slow_way_of_finding_them_makes_whatever_the_pieces_are_even_where_many_are_as_near_as_one_another()
 {
    let mut random = DeterministicRandomNumbers::with_seed(31);
    let mut sets_joined_by_several_spokes = 0;
    for round in 0..500 {
        let components: Vec<Vec<Rect>> = (0..3 + random.next_below(28))
            .map(|_| {
                (0..random.next_below(4))
                    .map(|_| rectangle_on_a_coarse_grid(&mut random))
                    .collect()
            })
            .collect();
        let spokes = spokes_between(&components, 6.0);
        assert_eq!(
            spokes,
            spokes_the_slow_way(&components, 6.0),
            "round {round}: {components:?}"
        );
        sets_joined_by_several_spokes += usize::from(spokes.len() >= 3);
    }
    // (Measured: 474 of the 500.)
    assert!(
        sets_joined_by_several_spokes >= 400,
        "the test is worth little if few of the sets of pieces are joined by several spokes: {sets_joined_by_several_spokes}"
    );
}

#[test]
fn a_great_many_pieces_are_joined_in_a_short_time() {
    // 600 pieces in 10 rows, each 7 wide and 10 apart (a tag of all the clips of a long project).
    let components: Vec<Vec<Rect>> = (0..600)
        .map(|number| {
            let (row, column) = (number / 60, number % 60);
            vec![rectangle(
                column as f32 * 10.0,
                row as f32 * 40.0,
                column as f32 * 10.0 + 7.0,
                row as f32 * 40.0 + 30.0,
            )]
        })
        .collect();
    let started = std::time::Instant::now();

    let spokes = spokes_between(&components, 8.0);

    // (The time was 4 seconds with the first way of finding them, in a build for testing, and is a few hundredths now.)
    assert_eq!(spokes.len(), 599);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "it took {:?}",
        started.elapsed()
    );
}

#[test]
fn whatever_pieces_there_are_the_spokes_join_them_into_a_shape_that_has_a_single_outline() {
    let mut random = DeterministicRandomNumbers::with_seed(11);
    for round in 0..300 {
        let pieces: Vec<Vec<Rect>> = (0..2 + random.next_below(6))
            .map(|_| {
                (0..1 + random.next_below(2))
                    .map(|_| random_rectangle(&mut random))
                    .collect()
            })
            .collect();
        let mut everything: Vec<Rect> = pieces.iter().flatten().copied().collect();
        everything.extend(spokes_between(&pieces, 6.0));

        let loops = outlines_of(&everything, SHARP);

        // (Each piece is on its own more rectangles that touch or overlap, or not: a piece of two that are apart is not joined by this, so
        // only one rectangle in each piece is looked at here.)
        let single_rectangle_pieces = pieces.iter().all(|piece| piece.len() == 1);
        if single_rectangle_pieces {
            assert_eq!(loops.len(), 1, "round {round}: {pieces:?}");
        }
        assert!(loops.iter().all(|outline| !crosses_itself(outline)), "round {round}");
    }
}
