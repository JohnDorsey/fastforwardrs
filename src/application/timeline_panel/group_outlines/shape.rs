//! The shape of the outline of a group: the line that goes round a set of rectangles that are put together, as a loop of yarn goes round
//! things it is tied round, and that is rounded off where it turns.
//!
//! The rectangles are where the members of the group are (padded by a little, so that the loop is not on them), and the spokes that join the
//! members that are far from one another (see [`spokes_between`]): they are put together to make **one shape** (the places that any of the
//! rectangles is on), and the outline is the edge of that shape. It is the **outer** edge only: a hole in the shape (a place that the rectangles
//! go round and are not on) is not gone round, so there is one loop for the shape and no more. Where the loop turns **outwards** (at the
//! corner of a rectangle) it is rounded off by a fillet that is as round as it was said to be, and where it turns **inwards** (where two
//! rectangles that are not as wide meet) it is rounded off by one that fills the corner in, as yarn that cannot bend tightly does: so a notch
//! that is narrower than the yarn bends is not gone into.
//!
//! The shape is found exactly (and not by drawing it in pixels): the edges of all the rectangles cut the plane into a grid of cells that are
//! each wholly in the shape or wholly out of it, and the edge of the shape is the edge between the cells that are in it and the cells that are out
//! of it and have no way in to the shape, followed all the way round. That takes the same time at any size and however far the rectangles are
//! scrolled off.

use std::cmp::Ordering;
use std::collections::HashMap;

use egui::{Pos2, Rect, pos2, vec2};

/// How many straight pieces a quarter turn of a rounded corner is drawn as.
const PIECES_IN_A_QUARTER_TURN: usize = 8;

/// The finest that coordinates are told apart by, in points: two edges that are less than this far apart are made one edge, so that rectangles
/// that are meant to line up (and are off by what arithmetic leaves) have no sliver of shape between them.
const FINEST_COORDINATE: f32 = 0.25;

/// One of the rectangles that an outline goes round, and how round its corners are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Piece {
    pub rectangle: Rect,
    /// How round the corners of the rectangle are, in points, where the outline turns outwards at one of them. (As round as the rectangle is
    /// wide and tall enough for: half of the shorter side at the most. A corner of the outline that is a corner of more than one of the
    /// rectangles is as round as the one that is the least round, so that it goes round all of them.)
    pub radius: f32,
}

/// The loops that go round the shape that the pieces make together, one for each part of it that is on its own (a shape that is made of
/// pieces that are joined has one). Each loop goes round clockwise on the screen (with the y that goes down), and is a list of points that
/// are joined by straight lines, the last to the first. A piece that has no room in it is not in the shape. Where the outline turns inwards,
/// where two pieces meet that are not as wide, the corner is rounded off by `concave_radius` (filled in).
///
/// A corner that turns outwards is a corner of one of the pieces, and is rounded as that piece is, so that the outline goes round the piece
/// as close to it as the outline of the piece alone would. (Rounding every corner of the shape by the same radius would take a corner of a
/// piece that is not as round as that off the piece.)
pub(super) fn outlines_of(pieces: &[Piece], concave_radius: f32) -> Vec<Vec<Pos2>> {
    let pieces: Vec<Piece> = pieces
        .iter()
        .map(|piece| Piece {
            rectangle: snapped(piece.rectangle),
            radius: piece.radius,
        })
        .filter(|piece| piece.rectangle.width() > 0.0 && piece.rectangle.height() > 0.0)
        .collect();
    if pieces.is_empty() {
        return Vec::new();
    }
    let rectangles: Vec<Rect> = pieces.iter().map(|piece| piece.rectangle).collect();
    let grid = Grid::of(&rectangles);
    grid.loops()
        .into_iter()
        .map(|corners| rounded(&corners, &pieces, concave_radius))
        .collect()
}

/// The rectangle with each of its edges moved to the nearest multiple of [`FINEST_COORDINATE`].
fn snapped(rectangle: Rect) -> Rect {
    let snap = |coordinate: f32| (coordinate / FINEST_COORDINATE).round() * FINEST_COORDINATE;
    Rect::from_min_max(
        pos2(snap(rectangle.min.x), snap(rectangle.min.y)),
        pos2(snap(rectangle.max.x), snap(rectangle.max.y)),
    )
}

/// A corner of a cell of the grid: how many lines of the grid across and down it is at.
type Vertex = (usize, usize);

/// The cells that the edges of some rectangles cut the plane into, and which of them are in the shape the rectangles make.
struct Grid {
    /// The places across where there is an edge of a rectangle, from the left, each once.
    xs: Vec<f32>,
    /// The same down.
    ys: Vec<f32>,
    /// Whether each cell is in the shape or is a hole in it, row by row: one cell for each pair of lines that are next to each other.
    in_the_shape: Vec<bool>,
}

impl Grid {
    fn of(rectangles: &[Rect]) -> Self {
        let mut xs: Vec<f32> = rectangles
            .iter()
            .flat_map(|rectangle| [rectangle.min.x, rectangle.max.x])
            .collect();
        let mut ys: Vec<f32> = rectangles
            .iter()
            .flat_map(|rectangle| [rectangle.min.y, rectangle.max.y])
            .collect();
        for lines in [&mut xs, &mut ys] {
            lines.sort_by(f32::total_cmp);
            lines.dedup();
        }
        let (columns, rows) = (xs.len() - 1, ys.len() - 1);
        let mut covered = vec![false; columns * rows];
        for rectangle in rectangles {
            let first_column = xs.partition_point(|x| *x < rectangle.min.x);
            let end_column = xs.partition_point(|x| *x < rectangle.max.x);
            let first_row = ys.partition_point(|y| *y < rectangle.min.y);
            let end_row = ys.partition_point(|y| *y < rectangle.max.y);
            for row in first_row..end_row {
                for column in first_column..end_column {
                    covered[row * columns + column] = true;
                }
            }
        }
        let mut grid = Self {
            xs,
            ys,
            in_the_shape: covered,
        };
        grid.fill_the_holes();
        grid
    }

    fn columns(&self) -> usize {
        self.xs.len() - 1
    }

    fn rows(&self) -> usize {
        self.ys.len() - 1
    }

    /// Whether the cell at `column` and `row` is in the shape. (A cell that is beyond the grid is not.)
    fn is_in_the_shape(&self, column: isize, row: isize) -> bool {
        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
            return false;
        };
        column < self.columns() && row < self.rows() && self.in_the_shape[row * self.columns() + column]
    }

    /// Makes every cell that is out of the shape and cannot be got to from outside the grid without going through the shape a cell that is in
    /// it: the holes are filled.
    fn fill_the_holes(&mut self) {
        let (columns, rows) = (self.columns() as isize, self.rows() as isize);
        // The cells that can be got to from outside, found from the cells at the edge of the grid and the cells outside it.
        let mut reached = vec![false; self.in_the_shape.len()];
        let mut to_look_from: Vec<(isize, isize)> = Vec::new();
        for column in 0..columns {
            to_look_from.extend([(column, 0), (column, rows - 1)]);
        }
        for row in 0..rows {
            to_look_from.extend([(0, row), (columns - 1, row)]);
        }
        while let Some((column, row)) = to_look_from.pop() {
            if column < 0 || row < 0 || column >= columns || row >= rows {
                continue;
            }
            let at = (row * columns + column) as usize;
            if reached[at] || self.in_the_shape[at] {
                continue;
            }
            reached[at] = true;
            to_look_from.extend([
                (column - 1, row),
                (column + 1, row),
                (column, row - 1),
                (column, row + 1),
            ]);
        }
        for (cell, outside) in self.in_the_shape.iter_mut().zip(reached) {
            *cell = !outside;
        }
    }

    /// The corners of the loops that go round the shape: each loop clockwise, as a list of the places where it turns.
    fn loops(&self) -> Vec<Vec<Pos2>> {
        // The pieces of the edge of the shape, each along the side of a cell that is in the shape and has one that is out of it on the other
        // side, from one corner of the cell to the next, going round clockwise with the shape on the right.
        let mut pieces: Vec<(Vertex, Vertex)> = Vec::new();
        for row in 0..self.rows() {
            for column in 0..self.columns() {
                if !self.is_in_the_shape(column as isize, row as isize) {
                    continue;
                }
                let (c, r) = (column as isize, row as isize);
                if !self.is_in_the_shape(c, r - 1) {
                    pieces.push(((column, row), (column + 1, row)));
                }
                if !self.is_in_the_shape(c + 1, r) {
                    pieces.push(((column + 1, row), (column + 1, row + 1)));
                }
                if !self.is_in_the_shape(c, r + 1) {
                    pieces.push(((column + 1, row + 1), (column, row + 1)));
                }
                if !self.is_in_the_shape(c - 1, r) {
                    pieces.push(((column, row + 1), (column, row)));
                }
            }
        }
        let mut starting_at: HashMap<Vertex, Vec<usize>> = HashMap::new();
        for (index, (from, _)) in pieces.iter().enumerate() {
            starting_at.entry(*from).or_default().push(index);
        }
        let mut used = vec![false; pieces.len()];
        let mut loops = Vec::new();
        for first in 0..pieces.len() {
            if used[first] {
                continue;
            }
            let mut vertices: Vec<Vertex> = vec![pieces[first].0];
            let mut current = first;
            loop {
                used[current] = true;
                let (from, to) = pieces[current];
                if to == pieces[first].0 {
                    break;
                }
                vertices.push(to);
                // Where two loops touch at a corner there are two ways on: the one that turns right most keeps to the edge of the same piece.
                let way_on = starting_at[&to]
                    .iter()
                    .copied()
                    .filter(|candidate| !used[*candidate])
                    .max_by_key(|candidate| turn(from, to, pieces[*candidate].1));
                match way_on {
                    Some(next) => current = next,
                    None => break,
                }
            }
            loops.push(self.corners_of(&vertices));
        }
        loops
    }

    /// The places where a loop of corners of the grid turns, as points on the screen: the corners where it goes on straight are left out.
    fn corners_of(&self, vertices: &[Vertex]) -> Vec<Pos2> {
        let point = |(column, row): Vertex| pos2(self.xs[column], self.ys[row]);
        let count = vertices.len();
        (0..count)
            .filter(|at| {
                let (before, here, after) = (
                    vertices[(at + count - 1) % count],
                    vertices[*at],
                    vertices[(at + 1) % count],
                );
                turn(before, here, after) != 0
            })
            .map(|at| point(vertices[at]))
            .collect()
    }
}

/// How the way from `from` through `through` to `to` turns, along the lines of a grid with the lines that go down counted downwards: 1 for a
/// turn to the right (clockwise on the screen), -1 to the left, 0 for none (or for going back on itself).
fn turn(from: Vertex, through: Vertex, to: Vertex) -> i32 {
    let (first_x, first_y) = (through.0 as i64 - from.0 as i64, through.1 as i64 - from.1 as i64);
    let (second_x, second_y) = (to.0 as i64 - through.0 as i64, to.1 as i64 - through.1 as i64);
    (first_x * second_y - first_y * second_x).signum() as i32
}

/// The loop of `corners` (which go round clockwise, and turn only at right angles) with every corner rounded off: those that turn outwards as
/// the pieces that they are corners of are round, and those that turn inwards by `concave_radius`, each as round as the edges on either side
/// of it have room for.
fn rounded(corners: &[Pos2], pieces: &[Piece], concave_radius: f32) -> Vec<Pos2> {
    let count = corners.len();
    let mut points = Vec::with_capacity(count * (PIECES_IN_A_QUARTER_TURN + 1));
    for at in 0..count {
        let (before, corner, after) = (
            corners[(at + count - 1) % count],
            corners[at],
            corners[(at + 1) % count],
        );
        let (to_the_corner, from_the_corner) = (corner - before, after - corner);
        let (coming_in, going_out) = (to_the_corner.normalized(), from_the_corner.normalized());
        let turns_right = coming_in.x * going_out.y - coming_in.y * going_out.x > 0.0;
        let wanted = if turns_right {
            radius_of_the_corner_of_the_pieces_at(corner, pieces)
        } else {
            concave_radius
        };
        let radius = wanted
            .min(to_the_corner.length() / 2.0)
            .min(from_the_corner.length() / 2.0);
        if radius < 0.01 {
            points.push(corner);
            continue;
        }
        let start = corner - coming_in * radius;
        // The middle of the arc is a radius from the start of it, to the right of the way in if the corner turns right and to the left if it
        // turns left. (A right turn of the screen's y that goes down is a turn of an angle that goes up.)
        let to_the_right = vec2(-coming_in.y, coming_in.x);
        let centre = if turns_right {
            start + to_the_right * radius
        } else {
            start - to_the_right * radius
        };
        let first_angle = (start - centre).angle();
        let sweep = if turns_right {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        for piece in 0..=PIECES_IN_A_QUARTER_TURN {
            let angle = first_angle + sweep * piece as f32 / PIECES_IN_A_QUARTER_TURN as f32;
            points.push(centre + vec2(angle.cos(), angle.sin()) * radius);
        }
    }
    without_repeats(points)
}

/// How round the corner of the outline at `corner`, which turns outwards, is to be: as round as the least round of the pieces that have a
/// corner there (and as round as each has room for). A corner that no piece has (which does not happen for one that turns outwards, but
/// is not worth a mistake) is as round as the least round of them all.
fn radius_of_the_corner_of_the_pieces_at(corner: Pos2, pieces: &[Piece]) -> f32 {
    let room = |piece: &Piece| {
        piece
            .radius
            .min(piece.rectangle.width() / 2.0)
            .min(piece.rectangle.height() / 2.0)
    };
    let is_a_corner_of = |piece: &Piece| {
        let rectangle = piece.rectangle;
        [rectangle.min.x, rectangle.max.x].contains(&corner.x) && [rectangle.min.y, rectangle.max.y].contains(&corner.y)
    };
    pieces
        .iter()
        .filter(|piece| is_a_corner_of(piece))
        .map(room)
        .reduce(f32::min)
        .or_else(|| pieces.iter().map(room).reduce(f32::min))
        .unwrap_or(0.0)
}

/// The points of a loop with each that is where the one before it is left out (two corners that are as round as the edge between them is long
/// end and begin at one place, which the arithmetic of each puts a hair apart, and a piece that is no longer than that has no direction).
fn without_repeats(points: Vec<Pos2>) -> Vec<Pos2> {
    const SAME_PLACE: f32 = 0.001;
    let mut kept: Vec<Pos2> = Vec::with_capacity(points.len());
    for point in points {
        if kept.last().is_none_or(|last| last.distance(point) > SAME_PLACE) {
            kept.push(point);
        }
    }
    while kept.len() > 1 && kept[0].distance(kept[kept.len() - 1]) <= SAME_PLACE {
        kept.pop();
    }
    kept
}

/// The rectangles, `width` wide, that join the groups of rectangles in `components` into one: a spoke for each of the joins of the shortest set
/// of them that joins all (the pieces that are nearest one another are joined first, and so no piece is joined by more than it takes).
/// Pieces that are on one another or touch need no spoke, and neither do those that are joined through others.
///
/// A spoke is straight between two rectangles that are in the same rows or the same columns (it is as far across the rows, or down the
/// columns, as the two have in common, in the middle of that), and bends once between two that are not (from the middle of one along its
/// row to the column of the middle of the other, and down that column). It goes from the middle of one rectangle to the middle of the other,
/// so that it is on both.
pub(super) fn spokes_between(components: &[Vec<Rect>], width: f32) -> Vec<Rect> {
    let components: Vec<&Vec<Rect>> = components.iter().filter(|component| !component.is_empty()).collect();
    if components.len() < 2 {
        return Vec::new();
    }
    // The pair of rectangles that are nearest one another, of two components, and how near.
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
    // (Prim's way of finding the shortest set of joins: start with one piece and keep adding the piece that is nearest to those that are in.
    // What is kept for each piece that is not in yet is the nearest of those that are in, so that when one piece comes in it is only looked at
    // against the pieces that are not in, once: looking at each piece that is in against each that is not, every time, takes the cube of the
    // number of pieces, which was seconds for the tag of all the clips of a long project. Where pieces are as near as one another, the pair
    // that has the piece that is first in the list of pieces is the one that is taken, and then the one that is not in that is first: as it
    // is where all of the pairs are looked at in turn and the first of the nearest is kept.)
    let mut joined = vec![false; components.len()];
    let mut nearest_in: Vec<Option<NearestPieceThatIsIn>> = vec![None; components.len()];
    let mut spokes = Vec::new();
    let mut coming_in = 0;
    for _ in 1..components.len() {
        joined[coming_in] = true;
        for outside in (0..components.len()).filter(|index| !joined[*index]) {
            let (distance, a, b) = nearest(components[coming_in], components[outside]);
            let is_nearer = nearest_in[outside].is_none_or(|so_far| {
                distance < so_far.distance || (distance == so_far.distance && coming_in < so_far.inside)
            });
            if is_nearer {
                nearest_in[outside] = Some(NearestPieceThatIsIn {
                    distance,
                    inside: coming_in,
                    a,
                    b,
                });
            }
        }
        // The piece that is not in yet and is nearest to one that is, and which that is.
        let (outside, found) = (0..components.len())
            .filter(|index| !joined[*index])
            .map(|index| (index, nearest_in[index].expect("it was looked at when a piece came in")))
            .min_by(|(first_index, first), (second_index, second)| {
                first
                    .distance
                    .partial_cmp(&second.distance)
                    .unwrap_or(Ordering::Equal)
                    .then(first.inside.cmp(&second.inside))
                    .then(first_index.cmp(second_index))
            })
            .expect("there is a piece that is not joined yet");
        coming_in = outside;
        if found.distance > 0.0 {
            spokes.extend(spoke_between(found.a, found.b, width));
        }
    }
    spokes
}

/// For a piece that is not in yet (while the spokes are being found): the nearest of the pieces that are in, which one that is (its place in the
/// list of pieces), how near, and the two rectangles that are nearest one another, one of each.
#[derive(Clone, Copy)]
struct NearestPieceThatIsIn {
    distance: f32,
    inside: usize,
    a: Rect,
    b: Rect,
}

/// How far apart two rectangles are: 0 if they are on one another or touch, and otherwise as far as the nearest points of them.
fn distance_between(first: Rect, second: Rect) -> f32 {
    let across = (first.min.x - second.max.x).max(second.min.x - first.max.x).max(0.0);
    let down = (first.min.y - second.max.y).max(second.min.y - first.max.y).max(0.0);
    across.hypot(down)
}

/// The spoke (one rectangle, or two when it bends) that joins `from` and `to`, which are not on one another.
fn spoke_between(from: Rect, to: Rect, width: f32) -> Vec<Rect> {
    let half = width / 2.0;
    let overlap_across = (from.min.x.max(to.min.x), from.max.x.min(to.max.x));
    let overlap_down = (from.min.y.max(to.min.y), from.max.y.min(to.max.y));
    let (start, end) = (from.center(), to.center());
    if overlap_across.0 < overlap_across.1 {
        // One is above the other, with some of the same columns: straight down the middle of those, from the middle of one to the middle
        // of the other.
        let x = (overlap_across.0 + overlap_across.1) / 2.0;
        vec![Rect::from_min_max(
            pos2(x - half, start.y.min(end.y)),
            pos2(x + half, start.y.max(end.y)),
        )]
    } else if overlap_down.0 < overlap_down.1 {
        let y = (overlap_down.0 + overlap_down.1) / 2.0;
        vec![Rect::from_min_max(
            pos2(start.x.min(end.x), y - half),
            pos2(start.x.max(end.x), y + half),
        )]
    } else {
        // Along the row of the first to the column of the second, and down that.
        vec![
            Rect::from_min_max(
                pos2(start.x.min(end.x) - half, start.y - half),
                pos2(start.x.max(end.x) + half, start.y + half),
            ),
            Rect::from_min_max(
                pos2(end.x - half, start.y.min(end.y) - half),
                pos2(end.x + half, start.y.max(end.y) + half),
            ),
        ]
    }
}

#[cfg(test)]
mod tests;
