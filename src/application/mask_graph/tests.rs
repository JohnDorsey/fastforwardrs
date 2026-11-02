// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn square() -> Rect {
    Rect::from_min_size(pos2(100.0, 40.0), vec2(200.0, 200.0))
}

fn triangle() -> Graph {
    Graph::from_points(&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]).unwrap()
}

#[test]
fn the_bottom_left_of_the_square_is_the_place_0_at_the_height_0_and_the_top_right_is_1_and_1() {
    let rect = square();

    assert_eq!(place_on_the_screen(rect, (0.0, 0.0)), pos2(100.0, 240.0));
    assert_eq!(place_on_the_screen(rect, (1.0, 1.0)), pos2(300.0, 40.0));
    assert_eq!(place_on_the_screen(rect, (0.5, 0.25)), pos2(200.0, 190.0));
}

#[test]
fn a_point_of_the_screen_is_the_place_and_height_it_is_the_screen_place_of() {
    let rect = square();

    for point in [(0.0, 0.0), (1.0, 1.0), (0.3, 0.8), (0.75, 0.1)] {
        let (place, height) = point_under(rect, place_on_the_screen(rect, point));
        assert!(
            (place - point.0).abs() < 1e-6 && (height - point.1).abs() < 1e-6,
            "{point:?}"
        );
    }
    // (Outside the square it goes on, and is brought inside by what moves a point.)
    let (place, height) = point_under(rect, pos2(50.0, 300.0));
    assert!(place < 0.0 && height < 0.0);
}

#[test]
fn a_handle_is_found_near_a_point_of_the_graph_and_the_nearest_one_wins() {
    let rect = square();
    let graph = triangle();
    let at_the_top = place_on_the_screen(rect, (0.5, 1.0));

    assert_eq!(handle_at(rect, &graph, at_the_top), Some(1));
    assert_eq!(
        handle_at(rect, &graph, at_the_top + vec2(3.0, 4.0)),
        Some(1),
        "near enough"
    );
    assert_eq!(handle_at(rect, &graph, at_the_top + vec2(0.0, 40.0)), None, "too far");
    assert_eq!(
        handle_at(rect, &graph, place_on_the_screen(rect, (0.0, 0.0)) + vec2(1.0, -1.0)),
        Some(0)
    );
    assert_eq!(
        handle_at(rect, &graph, place_on_the_screen(rect, (1.0, 0.0))),
        Some(2),
        "the last point"
    );

    // Two points that are close: the one the pointer is nearer to.
    let close = Graph::from_points(&[(0.0, 0.0), (0.5, 0.5), (0.52, 0.5), (1.0, 1.0)]).unwrap();
    let nearer_the_second = place_on_the_screen(rect, (0.52, 0.5)) + vec2(1.0, 0.0);
    assert_eq!(handle_at(rect, &close, nearer_the_second), Some(2));
}

#[test]
fn a_point_of_the_screen_is_near_the_line_when_it_is_close_to_a_segment_and_not_when_it_is_off_it() {
    let rect = square();
    let graph = triangle();
    let on_the_way_up = place_on_the_screen(rect, (0.25, 0.5));

    assert!(is_near_the_line(rect, &graph, on_the_way_up));
    assert!(is_near_the_line(rect, &graph, on_the_way_up + vec2(0.0, 3.0)));
    assert!(!is_near_the_line(rect, &graph, on_the_way_up + vec2(0.0, 40.0)));
    // (Under the peak, far from either side.)
    assert!(!is_near_the_line(rect, &graph, place_on_the_screen(rect, (0.5, 0.2))));
}

#[test]
fn the_graph_of_a_straight_map_is_the_straight_line_and_a_graph_that_is_the_straight_line_is_a_straight_map() {
    assert_eq!(map_of(Graph::straight()), MaskMap::Straight);
    assert_eq!(map_of(triangle()), MaskMap::Graph(triangle()));
    assert_eq!(MaskMap::Straight.graph_or_straight(), Graph::straight());
}
