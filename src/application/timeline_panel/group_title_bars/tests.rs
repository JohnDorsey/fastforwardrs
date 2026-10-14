//! Tests of where the name of a group is put, and of what the pointer is over.

use egui::{pos2, vec2};

use super::*;

/// The loop of a plain rectangle, as the loops of the outlines go: clockwise on the screen.
fn loop_of(rectangle: Rect) -> Vec<Pos2> {
    vec![
        rectangle.left_top(),
        rectangle.right_top(),
        rectangle.right_bottom(),
        rectangle.left_bottom(),
    ]
}

/// Where tabs can be: the strips and the ruler over them.
fn allowed() -> Rect {
    Rect::from_min_max(pos2(100.0, 20.0), pos2(900.0, 400.0))
}

#[test]
fn a_tab_is_over_the_top_edge_of_a_rectangle_in_the_middle_of_it_with_its_flat_side_on_the_edge() {
    let rectangle = Rect::from_min_max(pos2(200.0, 100.0), pos2(500.0, 200.0));
    let size = vec2(80.0, HEIGHT);

    let placed = place_a_tab(&[loop_of(rectangle)], size, allowed()).unwrap();

    assert_eq!(placed.rectangle, Rect::from_min_size(pos2(310.0, 100.0 - HEIGHT), size));
    assert!(!placed.hangs_below_the_edge);
}

#[test]
fn a_tab_that_is_wider_than_the_edge_goes_out_beyond_both_sides_of_it() {
    let rectangle = Rect::from_min_max(pos2(200.0, 100.0), pos2(260.0, 200.0));
    let size = vec2(120.0, HEIGHT);

    let placed = place_a_tab(&[loop_of(rectangle)], size, allowed()).unwrap();

    assert_eq!(placed.rectangle.center().x, 230.0, "in the middle of the edge");
    assert!(placed.rectangle.left() < rectangle.left() && placed.rectangle.right() > rectangle.right());
}

#[test]
fn a_tab_is_in_the_middle_of_the_part_of_the_edge_that_is_in_view() {
    // The rectangle is from 50 to 450, and the view begins at 100: the part that is in view is 100 to 450.
    let rectangle = Rect::from_min_max(pos2(50.0, 100.0), pos2(450.0, 200.0));
    let size = vec2(80.0, HEIGHT);

    let placed = place_a_tab(&[loop_of(rectangle)], size, allowed()).unwrap();

    assert_eq!(placed.rectangle.center().x, 275.0);
}

#[test]
fn a_tab_hangs_under_the_edge_where_there_is_no_room_over_it() {
    // The edge is 5 points under the top of the area that tabs can be in.
    let rectangle = Rect::from_min_max(pos2(200.0, 25.0), pos2(500.0, 100.0));
    let size = vec2(80.0, HEIGHT);

    let placed = place_a_tab(&[loop_of(rectangle)], size, allowed()).unwrap();

    assert!(placed.hangs_below_the_edge);
    assert_eq!(placed.rectangle.top(), 25.0);
    assert_eq!(placed.rectangle.bottom(), 25.0 + HEIGHT);
}

#[test]
fn a_tab_is_all_in_the_area_that_tabs_can_be_in_when_it_can_be() {
    // The edge is at the left end of the area, and a tab in the middle of what is in view would be partly out of it.
    let rectangle = Rect::from_min_max(pos2(60.0, 100.0), pos2(130.0, 200.0));
    let size = vec2(80.0, HEIGHT);

    let placed = place_a_tab(&[loop_of(rectangle)], size, allowed()).unwrap();

    assert!(placed.rectangle.left() >= allowed().left());
}

#[test]
fn a_group_with_no_top_edge_that_is_in_view_has_no_tab() {
    let size = vec2(80.0, HEIGHT);
    // Out of view to the left, to the right, above, and below.
    for rectangle in [
        Rect::from_min_max(pos2(0.0, 100.0), pos2(105.0, 200.0)),
        Rect::from_min_max(pos2(895.0, 100.0), pos2(1000.0, 200.0)),
        Rect::from_min_max(pos2(200.0, -300.0), pos2(500.0, -200.0)),
        Rect::from_min_max(pos2(200.0, 500.0), pos2(500.0, 600.0)),
    ] {
        assert_eq!(
            place_a_tab(&[loop_of(rectangle)], size, allowed()),
            None,
            "{rectangle:?}"
        );
    }
    assert_eq!(place_a_tab(&[], size, allowed()), None, "no outline");
}

#[test]
fn a_tab_goes_on_the_longest_top_edge_that_is_in_view_and_of_equal_ones_the_highest() {
    // An L: a tall narrow part at the left, 200 to 260, from 100 down, and a wide low part, 200 to 500, from 150: two top edges, one of 60
    // points at 100 and one of 240 at 150.
    let shape = vec![
        pos2(200.0, 100.0),
        pos2(260.0, 100.0),
        pos2(260.0, 150.0),
        pos2(500.0, 150.0),
        pos2(500.0, 200.0),
        pos2(200.0, 200.0),
    ];
    let size = vec2(80.0, HEIGHT);

    let placed = place_a_tab(&[shape], size, allowed()).unwrap();

    assert_eq!(
        placed.rectangle.bottom(),
        150.0,
        "on the wide part, which has room for all of the tab"
    );
    assert_eq!(placed.rectangle.center().x, (260.0 + 500.0) / 2.0);

    // Two edges that have room for the tab: the higher.
    let two = vec![
        pos2(200.0, 100.0),
        pos2(400.0, 100.0),
        pos2(400.0, 150.0),
        pos2(600.0, 150.0),
        pos2(600.0, 200.0),
        pos2(200.0, 200.0),
    ];
    assert_eq!(place_a_tab(&[two], size, allowed()).unwrap().rectangle.bottom(), 100.0);
}

#[test]
fn the_top_edges_are_the_straight_stretches_that_go_from_left_to_right_and_no_others() {
    let rectangle = Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0));

    assert_eq!(
        top_edges_of(&[loop_of(rectangle)]),
        vec![TopEdge {
            from_x: 0.0,
            to_x: 10.0,
            y: 0.0
        }],
        "the bottom goes the other way, and the sides are not across"
    );
}

#[test]
fn a_point_is_in_an_outline_when_it_is_inside_the_loops_and_not_when_it_is_outside_them_or_in_a_notch() {
    let shape = vec![
        pos2(0.0, 0.0),
        pos2(100.0, 0.0),
        pos2(100.0, 40.0),
        pos2(60.0, 40.0),
        pos2(60.0, 20.0),
        pos2(40.0, 20.0),
        pos2(40.0, 40.0),
        pos2(0.0, 40.0),
    ];

    assert!(outline_has(std::slice::from_ref(&shape), pos2(10.0, 10.0)));
    assert!(
        outline_has(std::slice::from_ref(&shape), pos2(50.0, 10.0)),
        "above the notch"
    );
    assert!(
        !outline_has(std::slice::from_ref(&shape), pos2(50.0, 30.0)),
        "in the notch"
    );
    assert!(!outline_has(std::slice::from_ref(&shape), pos2(-1.0, 10.0)));
    assert!(!outline_has(&[shape], pos2(10.0, 41.0)));
    assert!(!outline_has(&[], pos2(0.0, 0.0)));
}

#[test]
fn the_top_edges_leave_out_the_slopes_of_a_corner_that_is_cut_across() {
    // A rectangle with its top left corner cut across: a piece that goes right and up (it goes the right way, and is not level).
    let shape = vec![
        pos2(0.0, 10.0),
        pos2(10.0, 0.0),
        pos2(100.0, 0.0),
        pos2(100.0, 50.0),
        pos2(0.0, 50.0),
    ];

    assert_eq!(
        top_edges_of(&[shape]),
        vec![TopEdge {
            from_x: 10.0,
            to_x: 100.0,
            y: 0.0
        }]
    );
}

#[test]
fn a_tab_is_put_on_an_edge_that_has_just_as_much_in_view_as_is_worth_it_and_not_on_one_with_less() {
    let size = vec2(80.0, HEIGHT);
    let with_an_edge_that_is = |width: f32| loop_of(Rect::from_min_size(pos2(200.0, 100.0), vec2(width, 100.0)));

    assert!(place_a_tab(&[with_an_edge_that_is(LEAST_EDGE_IN_VIEW)], size, allowed()).is_some());
    assert_eq!(
        place_a_tab(&[with_an_edge_that_is(LEAST_EDGE_IN_VIEW - 0.5)], size, allowed()),
        None
    );
}

#[test]
fn an_edge_that_is_longer_than_a_tab_is_wide_is_no_better_than_one_that_the_tab_just_fits_on() {
    // The first is 100 points across at 100, the second 300 points across and lower: a tab that is 80 wide has room on both, and goes on the
    // higher. (Whichever of the two the outline has first.)
    let size = vec2(80.0, HEIGHT);
    let first = loop_of(Rect::from_min_size(pos2(200.0, 100.0), vec2(100.0, 100.0)));
    let second = loop_of(Rect::from_min_size(pos2(400.0, 150.0), vec2(300.0, 100.0)));

    for loops in [vec![first.clone(), second.clone()], vec![second, first]] {
        let placed = place_a_tab(&loops, size, allowed()).unwrap();

        assert_eq!(placed.rectangle.bottom(), 100.0, "{loops:?}");
    }
}

#[test]
fn of_edges_that_are_as_good_in_every_way_the_tab_goes_on_the_one_that_is_further_left() {
    let size = vec2(80.0, HEIGHT);
    let at_the_left = loop_of(Rect::from_min_size(pos2(200.0, 100.0), vec2(200.0, 100.0)));
    let at_the_right = loop_of(Rect::from_min_size(pos2(500.0, 100.0), vec2(200.0, 100.0)));

    for loops in [
        vec![at_the_left.clone(), at_the_right.clone()],
        vec![at_the_right, at_the_left],
    ] {
        let placed = place_a_tab(&loops, size, allowed()).unwrap();

        assert_eq!(placed.rectangle.center().x, 300.0, "{loops:?}");
    }
}

#[test]
fn edges_with_the_same_in_view_to_within_half_a_point_are_as_good_and_the_higher_is_chosen_and_a_longer_one_by_more_is_not()
 {
    // Tabs are wider than the edges here, so how much of an edge is in view is what tells them apart.
    let size = vec2(300.0, HEIGHT);
    let high = |width: f32| loop_of(Rect::from_min_size(pos2(200.0, 100.0), vec2(width, 100.0)));
    let low = |width: f32| loop_of(Rect::from_min_size(pos2(500.0, 150.0), vec2(width, 100.0)));

    // 0.3 points more in view on the lower one: not worth telling apart.
    for loops in [vec![high(100.0), low(100.3)], vec![low(100.3), high(100.0)]] {
        assert_eq!(
            place_a_tab(&loops, size, allowed()).unwrap().rectangle.bottom(),
            100.0,
            "{loops:?}"
        );
    }
    // 3 points less in view on the higher one: it is the lower, which has more in view.
    for loops in [vec![high(97.0), low(100.0)], vec![low(100.0), high(97.0)]] {
        assert_eq!(
            place_a_tab(&loops, size, allowed()).unwrap().rectangle.bottom(),
            150.0,
            "{loops:?}"
        );
    }
}

#[test]
fn a_tab_that_is_wider_than_the_area_it_can_be_in_is_at_the_left_end_of_it_and_so_is_one_that_is_as_wide() {
    let rectangle = Rect::from_min_max(pos2(300.0, 100.0), pos2(600.0, 200.0));

    for width in [allowed().width(), allowed().width() + 100.0] {
        let placed = place_a_tab(&[loop_of(rectangle)], vec2(width, HEIGHT), allowed()).unwrap();

        assert_eq!(placed.rectangle.left(), allowed().left(), "{width} wide");
        assert_eq!(placed.rectangle.width(), width);
    }
}

#[test]
fn a_tab_that_just_fits_over_the_edge_is_over_it_and_one_that_has_a_point_less_room_hangs_under_it() {
    let size = vec2(80.0, HEIGHT);
    let edge_at = |y: f32| loop_of(Rect::from_min_max(pos2(200.0, y), pos2(500.0, 300.0)));

    let fits = place_a_tab(&[edge_at(allowed().top() + HEIGHT)], size, allowed()).unwrap();
    assert!(!fits.hangs_below_the_edge);
    assert_eq!(fits.rectangle.top(), allowed().top());

    let does_not = place_a_tab(&[edge_at(allowed().top() + HEIGHT - 1.0)], size, allowed()).unwrap();
    assert!(does_not.hangs_below_the_edge);
}

#[test]
fn the_round_corners_of_a_tab_are_the_ones_that_are_away_from_the_edge_and_are_as_round_as_half_its_height() {
    let round = (HEIGHT / 2.0).round() as u8;
    assert!(round > 0, "the setup");

    let over_the_edge = rounding_of_a_tab(false);
    assert_eq!((over_the_edge.nw, over_the_edge.ne), (round, round), "round at the top");
    assert_eq!(
        (over_the_edge.sw, over_the_edge.se),
        (0, 0),
        "flat at the bottom, on the edge"
    );

    let under_the_edge = rounding_of_a_tab(true);
    assert_eq!(
        (under_the_edge.sw, under_the_edge.se),
        (round, round),
        "round at the bottom"
    );
    assert_eq!(
        (under_the_edge.nw, under_the_edge.ne),
        (0, 0),
        "flat at the top, on the edge"
    );
}

/// The outline of a clip group (or whatever kind) that is the rectangle, with `height` levels of groups in it.
fn outline_of(rectangle: Rect, height: usize) -> OutlineOfAGroup {
    OutlineOfAGroup {
        group: GroupId::new(1),
        colour: Color32::RED,
        kind: GroupKind::ClipGroup,
        height,
        loops: vec![loop_of(rectangle)],
    }
}

#[test]
fn of_two_groups_the_closer_is_the_one_with_fewer_groups_in_it_and_then_the_smaller_one() {
    let rectangle_of = |width: f32, height: f32| Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height));

    assert!(
        closeness_of(&outline_of(rectangle_of(1000.0, 1000.0), 0))
            < closeness_of(&outline_of(rectangle_of(10.0, 10.0), 1)),
        "fewer groups in it, even if it is the larger"
    );
    assert!(
        closeness_of(&outline_of(rectangle_of(10.0, 10.0), 2))
            > closeness_of(&outline_of(rectangle_of(1000.0, 1000.0), 1)),
        "and not the other way"
    );
    assert!(
        closeness_of(&outline_of(rectangle_of(40.0, 40.0), 1)) < closeness_of(&outline_of(rectangle_of(50.0, 40.0), 1)),
        "of those with as many in them, the smaller"
    );
    // How small is how much room it takes up (its area), and not how much it goes round: 100 by 10 takes up less than 40 by 40.
    assert!(
        closeness_of(&outline_of(rectangle_of(100.0, 10.0), 0))
            < closeness_of(&outline_of(rectangle_of(40.0, 40.0), 0)),
        "the area"
    );
}
