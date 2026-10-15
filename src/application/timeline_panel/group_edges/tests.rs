// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how an edge that is moved goes: where it can be put, and how it gets there.

use super::*;

#[test]
fn the_nearest_place_is_where_the_pointer_is_if_that_is_a_place_and_otherwise_the_end_of_the_nearest_range_of_places() {
    let places = [10..=20, 30..=40, 50..=60];

    assert_eq!(nearest_place(&places, 15), Some(15));
    assert_eq!(nearest_place(&places, 10), Some(10), "the first of a range");
    assert_eq!(nearest_place(&places, 40), Some(40), "the last of a range");
    assert_eq!(
        nearest_place(&places, 22),
        Some(20),
        "nearer the end of the range before"
    );
    assert_eq!(
        nearest_place(&places, 28),
        Some(30),
        "nearer the start of the range after"
    );
    assert_eq!(nearest_place(&places, 0), Some(10), "before them all");
    assert_eq!(nearest_place(&places, 500), Some(60), "after them all");
    assert_eq!(nearest_place(&[], 5), None, "nowhere to go");
}

#[test]
fn halfway_between_two_ranges_the_place_before_is_the_nearest() {
    let places = [10..=20, 30..=40];

    assert_eq!(
        nearest_place(&places, 25),
        Some(20),
        "5 from each: the first that was nearest is the one"
    );
}

#[test]
fn an_edge_goes_towards_where_it_goes_a_part_of_the_way_at_a_time_and_gets_there() {
    let mut shown = 100.0;
    let mut times = 0;
    while shown != 140.0 {
        let next = gone_towards(shown, 140.0, 1.0 / 60.0);
        assert!(next > shown && next <= 140.0, "{shown} to {next}");
        shown = next;
        times += 1;
        assert!(times < 200, "it never got there");
    }
    assert!(times > 3, "it is not there at once: {times} frames");
    assert!(times < 60, "it is quick: {times} frames");
}

#[test]
fn an_edge_goes_both_ways_and_is_not_moved_by_no_time_passing() {
    assert!(gone_towards(100.0, 60.0, 1.0 / 60.0) < 100.0);
    assert_eq!(gone_towards(100.0, 60.0, 0.0), 100.0);
    assert_eq!(gone_towards(100.0, 100.0, 0.5), 100.0);
}
