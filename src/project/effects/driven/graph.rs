// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The graph of line segments that maps the value a mask gives to the value an effect uses (`docs/shader-masks-design.md`, 3 and 8): **the place of the mask's value in 0 to 1 (nothing of
//! its channel to all of it) becomes a height in 0 to 1 (the least of the value's range to the most)**. Until one is drawn the map is the straight line from the bottom left to the top right.
//!
//! A graph is **data**: [`MOST_POINTS_OF_A_GRAPH`] points at most, which the shader is given as [`NUMBERS_OF_A_GRAPH`] numbers of the call, so that dragging a point compiles nothing. The
//! first point is at the place 0 and the last at the place 1; the places in between go up. The graph is a small `Copy` value, and everything that edits it is a function that gives
//! the edited graph, so that the window that draws it holds nothing but what it shows.

pub use crate::isf::{MOST_POINTS_OF_A_GRAPH, NUMBERS_OF_A_GRAPH};

/// How near the place of a point may be to the place of its neighbour when a point is moved or added: a graph that is drawn does not have two points on top of each other, and a point
/// that is dragged does not jump over another.
pub const LEAST_DISTANCE_BETWEEN_PLACES: f32 = 0.01;

/// A graph of line segments: two points or more, from the place 0 to the place 1, with heights from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graph {
    /// The points, the places going up; those after the first `count` repeat the last (the places of the shader's numbers are filled to the end that way).
    points: [(f32, f32); MOST_POINTS_OF_A_GRAPH],
    count: usize,
}

impl Graph {
    /// The straight line: the value used is the value of the mask.
    pub fn straight() -> Self {
        Self::from_points(&[(0.0, 0.0), (1.0, 1.0)]).expect("the straight line is a graph")
    }

    /// The graph of `points` (the place, then the height, of each), if they make one: from two to [`MOST_POINTS_OF_A_GRAPH`] points, every number in 0 to 1, the first at the place 0, the last
    /// at the place 1, and the places going up.
    pub fn from_points(points: &[(f32, f32)]) -> Option<Self> {
        let count = points.len();
        if !(2..=MOST_POINTS_OF_A_GRAPH).contains(&count) {
            return None;
        }
        let in_the_square = |(place, height): &(f32, f32)| (0.0..=1.0).contains(place) && (0.0..=1.0).contains(height);
        if !points.iter().all(in_the_square)
            || points[0].0 != 0.0
            || points[count - 1].0 != 1.0
            || points.windows(2).any(|pair| pair[1].0 <= pair[0].0)
        {
            return None;
        }
        let mut filled = [(1.0, points[count - 1].1); MOST_POINTS_OF_A_GRAPH];
        filled[..count].copy_from_slice(points);
        Some(Self { points: filled, count })
    }

    /// The points of the graph, the place and the height of each, in the order of the places.
    pub fn points(&self) -> &[(f32, f32)] {
        &self.points[..self.count]
    }

    /// The numbers of a call that the shader reads the graph from: the places of the points, then their heights, the last point repeated to fill what is left.
    pub fn numbers(&self) -> [f32; NUMBERS_OF_A_GRAPH] {
        let mut numbers = [0.0; NUMBERS_OF_A_GRAPH];
        for (point, (place, height)) in self.points.iter().enumerate() {
            numbers[point] = *place;
            numbers[MOST_POINTS_OF_A_GRAPH + point] = *height;
        }
        numbers
    }

    /// The height of the graph at `place` (the end that it is beyond, for a place outside 0 to 1).
    pub fn height_at(&self, place: f32) -> f32 {
        let place = place.clamp(0.0, 1.0);
        for pair in self.points().windows(2) {
            if place <= pair[1].0 {
                let across = (place - pair[0].0) / (pair[1].0 - pair[0].0);
                return pair[0].1 + (pair[1].1 - pair[0].1) * across;
            }
        }
        self.points[self.count - 1].1
    }

    /// The graph with the point number `index` moved to `to` (a place and a height): in the square, and between its neighbours (a little short of them); an end keeps its place and moves in height
    /// only. A point that is not there, or a place that is not a number, leaves the graph as it is.
    #[must_use]
    pub fn with_a_point_moved(&self, index: usize, to: (f32, f32)) -> Self {
        if index >= self.count || !to.0.is_finite() || !to.1.is_finite() {
            return *self;
        }
        let place = if index == 0 {
            0.0
        } else if index == self.count - 1 {
            1.0
        } else {
            let (low, high) = (
                self.points[index - 1].0 + LEAST_DISTANCE_BETWEEN_PLACES,
                self.points[index + 1].0 - LEAST_DISTANCE_BETWEEN_PLACES,
            );
            if low > high {
                return *self;
            }
            to.0.clamp(low, high)
        };
        let mut points = self.points().to_vec();
        points[index] = (place, to.1.clamp(0.0, 1.0));
        Self::from_points(&points).unwrap_or(*self)
    }

    /// The graph with a point added at `place`, on the line where the graph is (so that the graph does not change), if there is room for one and the place is not too near another point.
    #[must_use]
    pub fn with_a_point_added_at(&self, place: f32) -> Option<Self> {
        if self.count >= MOST_POINTS_OF_A_GRAPH || !place.is_finite() || place <= 0.0 || place >= 1.0 {
            return None;
        }
        let after = self.points().iter().position(|point| point.0 > place)?;
        let (before_place, after_place) = (self.points[after - 1].0, self.points[after].0);
        if place - before_place < LEAST_DISTANCE_BETWEEN_PLACES || after_place - place < LEAST_DISTANCE_BETWEEN_PLACES {
            return None;
        }
        let mut points = self.points().to_vec();
        points.insert(after, (place, self.height_at(place)));
        Self::from_points(&points)
    }

    /// The graph without the point number `index`, if it is not an end (the first and the last stay, so a graph never has fewer than two points).
    #[must_use]
    pub fn with_a_point_removed(&self, index: usize) -> Option<Self> {
        if index == 0 || index + 1 >= self.count {
            return None;
        }
        let mut points = self.points().to_vec();
        points.remove(index);
        Self::from_points(&points)
    }
}

#[cfg(test)]
mod tests;
