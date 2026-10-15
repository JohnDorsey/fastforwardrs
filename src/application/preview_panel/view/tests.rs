// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use egui::{pos2, vec2};

use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// A panel 1000 points wide and 600 high with its top left corner at (100, 50): a 16 by 9 picture fits it 984 points wide (it
/// is held 8 points from the edges all round).
fn panel() -> Rect {
    Rect::from_min_size(pos2(100.0, 50.0), vec2(1000.0, 600.0))
}

/// A picture of 1920 by 1080 pixels, which is 0.5125 points to a pixel when it fits the panel.
const FULL_HD: (u32, u32) = (1920, 1080);

fn close(first: f32, second: f32) {
    assert!((first - second).abs() < 1e-2, "{first} is not {second}");
}

fn the_view_after_zooming_by(factors: &[f32], about: Pos2) -> PreviewView {
    factors.iter().fold(PreviewView::FITTING, |view, factor| {
        view.zoomed_by(*factor, about, panel(), FULL_HD)
    })
}

// --- fitting ---

#[test]
fn a_new_view_has_the_picture_as_big_as_it_can_be_in_the_panel_in_the_middle_of_it() {
    let view = PreviewView::default();
    assert!(view.is_fitting());
    assert_eq!(view.zoom(), 1.0);
    let rectangle = view.picture_rectangle(panel(), FULL_HD);
    close(rectangle.width(), 984.0);
    close(rectangle.height(), 553.5);
    assert_eq!(rectangle.center(), panel().center());
    assert_eq!(
        rectangle,
        fit_inside(panel().shrink(theme::SPACING), 1920, 1080),
        "what the preview showed before it could be zoomed"
    );
}

#[test]
fn a_view_that_is_zoomed_is_not_fitting() {
    assert!(!the_view_after_zooming_by(&[1.5], panel().center()).is_fitting());
    assert!(!the_view_after_zooming_by(&[0.7], panel().center()).is_fitting());
}

// --- zooming ---

#[test]
fn zooming_in_makes_the_picture_that_many_times_as_big_and_about_the_middle_keeps_it_there() {
    let view = the_view_after_zooming_by(&[2.0], panel().center());
    let rectangle = view.picture_rectangle(panel(), FULL_HD);
    close(rectangle.width(), 984.0 * 2.0);
    close(rectangle.height(), 553.5 * 2.0);
    assert_eq!(rectangle.center(), panel().center());
    assert_eq!(view.zoom(), 2.0);
}

#[test]
fn zooming_about_a_point_keeps_that_point_of_the_picture_under_it() {
    let mut random = DeterministicRandomNumbers::with_seed(4);
    let mut points_checked = 0;
    for round in 0..300 {
        // A point in the middle half of the panel, so that the picture goes on covering the panel when it is zoomed in a little.
        let about = pos2(
            panel().left() + 250.0 + random.next_below(500) as f32,
            panel().top() + 150.0 + random.next_below(300) as f32,
        );
        let first = 1.2 + random.next_below(8) as f32 / 10.0;
        let before = PreviewView::FITTING.zoomed_by(first, about, panel(), FULL_HD);
        let second = 1.0 + random.next_below(5) as f32 / 10.0;
        let after = before.zoomed_by(second, about, panel(), FULL_HD);

        for (from, to, what) in [
            (PreviewView::FITTING, before, "zoomed in"),
            (before, after, "zoomed in again"),
        ] {
            let fraction_in = |view: PreviewView| {
                let rectangle = view.picture_rectangle(panel(), FULL_HD);
                (
                    (about.x - rectangle.left()) / rectangle.width(),
                    (about.y - rectangle.top()) / rectangle.height(),
                )
            };
            let (x_before, y_before) = fraction_in(from);
            let (x_after, y_after) = fraction_in(to);
            assert!(
                (x_before - x_after).abs() < 1e-3 && (y_before - y_after).abs() < 1e-3,
                "round {round}, {what}: the point of the picture under {about:?} was ({x_before}, {y_before}) and is ({x_after}, {y_after})"
            );
            points_checked += 1;
        }
    }
    assert_eq!(points_checked, 600);
}

#[test]
fn zooming_out_goes_no_further_than_half_the_size_the_picture_has_when_it_fits() {
    let view = the_view_after_zooming_by(&[0.5, 0.5, 0.5, 0.5], panel().center());
    assert_eq!(view.zoom(), SMALLEST_ZOOM);
    let rectangle = view.picture_rectangle(panel(), FULL_HD);
    close(rectangle.width(), 984.0 / 2.0);
    close(rectangle.height(), 553.5 / 2.0);
    assert_eq!(rectangle.center(), panel().center(), "in the middle of the panel");
    let still = view.zoomed_by(0.1, pos2(200.0, 100.0), panel(), FULL_HD);
    assert_eq!(still, view, "asking for more changes nothing");
}

#[test]
fn zooming_in_goes_no_further_than_where_every_pixel_of_the_picture_can_be_seen_by_itself() {
    let view = the_view_after_zooming_by(&[2.0; 8], panel().center());
    close(view.points_per_pixel(panel(), FULL_HD), MOST_POINTS_PER_PIXEL);
    let still = view.zoomed_by(2.0, panel().center(), panel(), FULL_HD);
    assert_eq!(still, view);
    // The same limit is a different amount of zoom for a picture with fewer pixels, which is bigger when it fits.
    let small = (320, 180);
    let zoomed = (0..8).fold(PreviewView::FITTING, |view, _| {
        view.zoomed_by(2.0, panel().center(), panel(), small)
    });
    close(zoomed.points_per_pixel(panel(), small), MOST_POINTS_PER_PIXEL);
    assert!(zoomed.zoom() < view.zoom(), "{} and {}", zoomed.zoom(), view.zoom());
}

#[test]
fn when_a_limit_stops_the_zoom_the_point_of_the_picture_under_the_pointer_stays_under_it_for_the_zoom_that_was_made() {
    // A point to the right of the middle and above it, and a zoom that is a little short of the most there is (which is 15.6 for
    // a picture of this size in this panel).
    let about = pos2(panel().left() + 800.0, panel().top() + 200.0);
    let fraction_in = |view: PreviewView| {
        let rectangle = view.picture_rectangle(panel(), FULL_HD);
        (
            (about.x - rectangle.left()) / rectangle.width(),
            (about.y - rectangle.top()) / rectangle.height(),
        )
    };
    let near = PreviewView::FITTING.zoomed_by(15.0, about, panel(), FULL_HD);
    assert!(near.zoom() < 15.1, "{}", near.zoom());

    let stopped = near.zoomed_by(2.0, about, panel(), FULL_HD);

    assert!(
        stopped.zoom() > near.zoom() && stopped.zoom() < 2.0 * near.zoom(),
        "twice as much was asked for and the limit stopped it: {} and {}",
        near.zoom(),
        stopped.zoom()
    );
    let ((x_before, y_before), (x_after, y_after)) = (fraction_in(near), fraction_in(stopped));
    assert!(
        (x_before - x_after).abs() < 1e-4 && (y_before - y_after).abs() < 1e-4,
        "the point of the picture under the pointer was ({x_before}, {y_before}) and is ({x_after}, {y_after})"
    );
}

#[test]
fn a_picture_with_so_few_pixels_that_they_are_wider_than_that_when_it_fits_is_not_zoomed_in_but_can_be_zoomed_out() {
    let tiny = (16, 9);
    let view = PreviewView::FITTING.zoomed_by(3.0, panel().center(), panel(), tiny);
    assert!(
        view.is_fitting(),
        "a pixel is already {} points wide",
        view.points_per_pixel(panel(), tiny)
    );
    let out = PreviewView::FITTING.zoomed_by(0.5, panel().center(), panel(), tiny);
    assert_eq!(out.zoom(), SMALLEST_ZOOM);
}

#[test]
fn a_zoom_that_is_not_a_number_or_not_more_than_nothing_changes_nothing() {
    let view = the_view_after_zooming_by(&[1.5], pos2(300.0, 300.0));
    for factor in [0.0, -2.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            view.zoomed_by(factor, pos2(300.0, 300.0), panel(), FULL_HD),
            view,
            "{factor}"
        );
    }
}

#[test]
fn zooming_in_and_out_again_by_the_same_amounts_comes_back_to_where_the_picture_fits_exactly() {
    for factors in [[1.3, 1.0 / 1.3], [2.0, 0.5], [1.7, 1.0 / 1.7], [1.05, 1.0 / 1.05]] {
        let view = the_view_after_zooming_by(&factors, pos2(400.0, 250.0));
        assert!(view.is_fitting(), "{factors:?} gives {view:?}");
    }
    let (a, b) = (1.1_f32, 1.2_f32);
    let back = the_view_after_zooming_by(&[a, b, 1.0 / b, 1.0 / a], pos2(333.0, 444.0));
    assert!(back.is_fitting(), "{back:?}");
}

#[test]
fn zooming_out_from_a_zoom_in_that_was_off_to_one_side_brings_the_picture_back_to_the_middle() {
    let view = the_view_after_zooming_by(&[2.0], pos2(950.0, 600.0));
    assert_ne!(view.shift, Vec2::ZERO, "the zoom was not about the middle");
    let out = view.zoomed_by(0.5, pos2(950.0, 600.0), panel(), FULL_HD);
    assert!(out.is_fitting(), "{out:?}");
    let smaller = out.zoomed_by(0.7, pos2(950.0, 600.0), panel(), FULL_HD);
    assert_eq!(smaller.picture_rectangle(panel(), FULL_HD).center(), panel().center());
}

// --- moving the picture about ---

#[test]
fn a_picture_that_is_bigger_than_the_panel_is_moved_by_as_much_as_it_is_asked_to_as_far_as_it_covers_the_panel() {
    let zoomed = the_view_after_zooming_by(&[2.0], panel().center());
    let rectangle = |view: PreviewView| view.picture_rectangle(panel(), FULL_HD);
    let moved = zoomed.shifted_by(vec2(-30.0, 10.0), panel(), FULL_HD);
    close(rectangle(moved).left() - rectangle(zoomed).left(), -30.0);
    close(rectangle(moved).top() - rectangle(zoomed).top(), 10.0);

    // As far as it goes, it has its edges at those of the panel, and no further.
    let furthest_right = zoomed.shifted_by(vec2(10_000.0, 0.0), panel(), FULL_HD);
    close(rectangle(furthest_right).left(), panel().left());
    let furthest_left = zoomed.shifted_by(vec2(-10_000.0, 0.0), panel(), FULL_HD);
    close(rectangle(furthest_left).right(), panel().right());
    let furthest_down = zoomed.shifted_by(vec2(0.0, 10_000.0), panel(), FULL_HD);
    close(rectangle(furthest_down).top(), panel().top());
    let furthest_up = zoomed.shifted_by(vec2(0.0, -10_000.0), panel(), FULL_HD);
    close(rectangle(furthest_up).bottom(), panel().bottom());
}

#[test]
fn a_picture_that_is_not_bigger_than_the_panel_stays_in_the_middle_of_it_whatever_it_is_asked() {
    for view in [
        PreviewView::FITTING,
        the_view_after_zooming_by(&[0.7], panel().center()),
    ] {
        let moved = view.shifted_by(vec2(200.0, -150.0), panel(), FULL_HD);
        assert_eq!(moved, view);
        assert_eq!(moved.picture_rectangle(panel(), FULL_HD).center(), panel().center());
    }
}

#[test]
fn a_picture_that_is_bigger_than_the_panel_one_way_only_is_moved_that_way_only() {
    // 1.2 times as big as it fits is 1181 by 664 points: wider than the 1000 by 600 panel, and higher too. A tall panel instead:
    let tall = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1400.0));
    let zoomed = PreviewView::FITTING.zoomed_by(1.2, tall.center(), tall, FULL_HD);
    let moved = zoomed.shifted_by(vec2(10_000.0, 10_000.0), tall, FULL_HD);
    let rectangle = moved.picture_rectangle(tall, FULL_HD);
    close(rectangle.left(), tall.left());
    close(rectangle.center().y, tall.center().y);
}

#[test]
fn a_view_that_the_panel_has_got_smaller_under_is_brought_back_to_where_it_can_be() {
    let zoomed =
        the_view_after_zooming_by(&[3.0], panel().center()).shifted_by(vec2(-10_000.0, -10_000.0), panel(), FULL_HD);
    let smaller = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 200.0));
    let kept = zoomed.kept_where_it_can_be(smaller, FULL_HD);
    let rectangle = kept.picture_rectangle(smaller, FULL_HD);
    assert!(
        rectangle.expand(0.01).contains_rect(smaller),
        "it still covers the panel it is in: {rectangle:?} for {smaller:?}"
    );
    // And a panel that is so big that the picture is a lot bigger when it fits has the zoom brought back to where a pixel is no
    // wider than it can be.
    let big = Rect::from_min_size(pos2(0.0, 0.0), vec2(4000.0, 2400.0));
    let most_zoomed_in = the_view_after_zooming_by(&[100.0], panel().center());
    assert!(
        most_zoomed_in.points_per_pixel(big, FULL_HD) > MOST_POINTS_PER_PIXEL,
        "as the panel was"
    );
    let kept = most_zoomed_in.kept_where_it_can_be(big, FULL_HD);
    close(kept.points_per_pixel(big, FULL_HD), MOST_POINTS_PER_PIXEL);
}

// --- how the pictures are put on the screen ---

#[test]
fn a_pixel_that_is_exactly_two_pixels_of_the_screen_wide_is_shown_as_a_block_and_one_a_little_less_is_not() {
    // The picture fits the panel 984 points across: 492 pixels across is exactly two points to a pixel, and 493 is a little less.
    assert_eq!(
        PreviewView::FITTING.filter(panel(), (492, 277), 1.0),
        PictureFilter::Pixels
    );
    assert_eq!(
        PreviewView::FITTING.filter(panel(), (493, 277), 1.0),
        PictureFilter::Smooth
    );
    assert_eq!(
        PreviewView::FITTING.filter(panel(), (984, 553), 2.0),
        PictureFilter::Pixels,
        "a point is two pixels of the screen, and a pixel of the picture is a point wide"
    );
}

#[test]
fn the_pixels_are_blended_when_the_picture_fits_and_shown_as_blocks_when_a_pixel_is_two_pixels_of_the_screen_or_more() {
    assert_eq!(
        PreviewView::FITTING.filter(panel(), FULL_HD, 1.0),
        PictureFilter::Smooth
    );
    // A pixel of the picture is 0.5125 points when it fits.
    let a_little_in = the_view_after_zooming_by(&[3.0], panel().center());
    assert_eq!(
        a_little_in.filter(panel(), FULL_HD, 1.0),
        PictureFilter::Smooth,
        "1.5 points"
    );
    let further = the_view_after_zooming_by(&[4.0], panel().center());
    assert_eq!(
        further.filter(panel(), FULL_HD, 1.0),
        PictureFilter::Pixels,
        "2.05 points"
    );
    assert_eq!(
        a_little_in.filter(panel(), FULL_HD, 2.0),
        PictureFilter::Pixels,
        "on a screen with twice the pixels a point is two pixels, and 1.5 points is three"
    );
    assert_eq!(
        the_view_after_zooming_by(&[100.0], panel().center()).filter(panel(), FULL_HD, 1.0),
        PictureFilter::Pixels,
        "all the way in"
    );
}
