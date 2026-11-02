// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const TRIANGLE: [(f32, f32); 3] = [(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)];

fn triangle() -> Graph {
    Graph::from_points(&TRIANGLE).expect("a triangle is a graph")
}

/// A graph of the given heights, the places equally far apart from 0 to 1.
fn evenly(heights: &[f32]) -> Graph {
    let last = (heights.len() - 1) as f32;
    let points: Vec<(f32, f32)> = heights
        .iter()
        .enumerate()
        .map(|(number, height)| (number as f32 / last, *height))
        .collect();
    Graph::from_points(&points).expect("a graph")
}

#[test]
fn a_graph_that_has_not_been_drawn_is_the_straight_line_from_nothing_to_all() {
    let graph = Graph::straight();

    assert_eq!(graph.points(), [(0.0, 0.0), (1.0, 1.0)]);
    for place in [0.0, 0.25, 0.5, 1.0] {
        assert_eq!(graph.height_at(place), place, "at {place}");
    }
}

#[test]
fn the_numbers_of_a_graph_are_the_places_of_its_points_and_then_their_heights_with_the_last_point_repeated_to_fill() {
    let numbers = triangle().numbers();

    assert_eq!(numbers.len(), NUMBERS_OF_A_GRAPH);
    assert_eq!(
        &numbers[..MOST_POINTS_OF_A_GRAPH],
        [0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "the places"
    );
    assert_eq!(
        &numbers[MOST_POINTS_OF_A_GRAPH..],
        [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        "the heights"
    );
}

#[test]
fn the_height_of_a_graph_is_on_the_line_between_the_two_points_that_the_place_is_between() {
    let graph = triangle();

    for (place, height) in [(0.0, 0.0), (0.25, 0.5), (0.5, 1.0), (0.75, 0.5), (1.0, 0.0)] {
        assert!((graph.height_at(place) - height).abs() < 1e-6, "at {place}");
    }
    // (A place outside 0 to 1 is the end that it is beyond.)
    assert_eq!(graph.height_at(-3.0), 0.0);
    assert_eq!(graph.height_at(7.0), 0.0);
}

#[test]
fn what_is_not_a_graph_is_refused() {
    let nan = f32::NAN;
    let refused: [(&str, &[(f32, f32)]); 9] = [
        ("no point", &[]),
        ("one point", &[(0.0, 0.5)]),
        ("it does not begin at the place 0", &[(0.1, 0.0), (1.0, 1.0)]),
        ("it does not end at the place 1", &[(0.0, 0.0), (0.9, 1.0)]),
        (
            "the places do not go up",
            &[(0.0, 0.0), (0.6, 1.0), (0.4, 0.0), (1.0, 1.0)],
        ),
        (
            "two points at one place",
            &[(0.0, 0.0), (0.5, 1.0), (0.5, 0.0), (1.0, 1.0)],
        ),
        ("a height above 1", &[(0.0, 0.0), (1.0, 1.5)]),
        ("a height below 0", &[(0.0, -0.1), (1.0, 1.0)]),
        ("a number that is not one", &[(0.0, 0.0), (nan, 0.5), (1.0, 1.0)]),
    ];
    for (why, points) in refused {
        assert!(Graph::from_points(points).is_none(), "{why}");
    }
    let evenly_spread = |count: usize| -> Vec<(f32, f32)> {
        (0..count)
            .map(|number| (number as f32 / (count - 1) as f32, 0.5))
            .collect()
    };
    assert!(
        Graph::from_points(&evenly_spread(MOST_POINTS_OF_A_GRAPH + 1)).is_none(),
        "more points than a graph can have"
    );
    assert!(
        Graph::from_points(&evenly_spread(MOST_POINTS_OF_A_GRAPH)).is_some(),
        "and as many as it can have is one"
    );
}

#[test]
fn moving_a_point_keeps_it_between_its_neighbours_and_inside_the_square() {
    let graph = evenly(&[0.0, 1.0, 0.0, 1.0]);

    // An inner point goes where it is put, and not past the places of its neighbours (a little short of them).
    let moved = graph.with_a_point_moved(1, (0.5, 0.25));
    assert_eq!(moved.points()[1], (0.5, 0.25));
    let too_far = graph.with_a_point_moved(1, (0.9, 2.0));
    let (place, height) = too_far.points()[1];
    assert!(
        place < graph.points()[2].0 && place > graph.points()[2].0 - 0.5,
        "{place}"
    );
    assert_eq!(height, 1.0, "no higher than 1");
    let too_far_back = graph.with_a_point_moved(2, (0.0, -1.0));
    let (place, height) = too_far_back.points()[2];
    assert!(place > graph.points()[1].0, "{place}");
    assert_eq!(height, 0.0, "no lower than 0");
    // The ends keep their places and move in height only.
    let end = graph.with_a_point_moved(0, (0.4, 0.8));
    assert_eq!(end.points()[0], (0.0, 0.8));
    let other_end = graph.with_a_point_moved(3, (0.2, 0.3));
    assert_eq!(other_end.points()[3], (1.0, 0.3));
    // Every graph made by moving is a graph.
    for made in [moved, too_far, too_far_back, end, other_end] {
        assert!(
            Graph::from_points(made.points()).is_some(),
            "{:?} is not a graph",
            made.points()
        );
    }
    // A point that is not there is left alone.
    assert_eq!(graph.with_a_point_moved(9, (0.5, 0.5)), graph);
}

#[test]
fn a_point_is_added_on_the_line_where_the_graph_is_and_does_not_change_the_graph() {
    let graph = triangle();

    let added = graph.with_a_point_added_at(0.25).expect("there is room");

    assert_eq!(added.points().len(), 4);
    assert_eq!(added.points()[1], (0.25, 0.5), "on the line, at the place asked for");
    for place in [0.0, 0.1, 0.25, 0.4, 0.5, 0.8, 1.0] {
        assert!(
            (added.height_at(place) - graph.height_at(place)).abs() < 1e-6,
            "the picture of the graph is the same at {place}"
        );
    }
}

#[test]
fn no_point_is_added_where_there_is_one_already_or_when_the_graph_is_full() {
    let graph = triangle();
    assert!(graph.with_a_point_added_at(0.5).is_none(), "on a point");
    assert!(graph.with_a_point_added_at(0.5 + 0.001).is_none(), "almost on a point");
    assert!(graph.with_a_point_added_at(0.0).is_none(), "on the end");
    assert!(graph.with_a_point_added_at(1.0).is_none(), "on the other end");

    let mut full = Graph::straight();
    for number in 1..MOST_POINTS_OF_A_GRAPH - 2 {
        full = full
            .with_a_point_added_at(number as f32 / (MOST_POINTS_OF_A_GRAPH - 1) as f32)
            .expect("there is room");
    }
    // (One short of full; one more fills it.)
    assert_eq!(full.points().len(), MOST_POINTS_OF_A_GRAPH - 1);
    let full = full.with_a_point_added_at(0.97).expect("the last place");
    assert_eq!(full.points().len(), MOST_POINTS_OF_A_GRAPH);
    assert!(full.with_a_point_added_at(0.01).is_none(), "no room for another");
}

#[test]
fn a_point_that_is_not_an_end_can_be_taken_away_and_the_graph_is_never_left_with_fewer_than_two() {
    let graph = triangle();

    let without = graph.with_a_point_removed(1).expect("an inner point");
    assert_eq!(without.points(), [(0.0, 0.0), (1.0, 0.0)]);
    assert!(graph.with_a_point_removed(0).is_none(), "an end stays");
    assert!(graph.with_a_point_removed(2).is_none(), "an end stays");
    assert!(graph.with_a_point_removed(7).is_none(), "there is no such point");
    assert!(without.with_a_point_removed(0).is_none());
}
