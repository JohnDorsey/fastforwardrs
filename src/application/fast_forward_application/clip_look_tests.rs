//! Tests of what the strips and the clips on them look like, checked from the shapes the window
//! draws (so they need no graphics card).

use egui::{CornerRadius, Shape};

use super::test_window::{Window, project_of};
use crate::application::theme;

/// The strips of a 30 second video are far wider than the window, and the playhead is in the
/// middle, so that the clip goes on beyond both edges of the window.
fn window_in_the_middle_of_a_long_clip() -> Window {
    let mut window = Window::showing(project_of(900));
    window.move_current_frame_to(450);
    window
}

#[test]
fn the_strip_of_a_track_has_square_corners_and_a_line_along_its_top_and_its_bottom_and_none_down_its_sides() {
    let window = window_in_the_middle_of_a_long_clip();
    let strip = window.strip_of_track(0);
    let shapes = window.shapes_drawn();

    let background = shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.rect == strip && rectangle.fill == theme::TRACK_BACKGROUND => {
                Some(rectangle)
            }
            _ => None,
        })
        .expect("the background of the strip is drawn");
    assert_eq!(background.corner_radius, CornerRadius::ZERO);

    // A line round the whole strip would be seen over the clips that go on beyond the window, which
    // would look as if they ended there.
    let outlined = shapes.iter().any(|shape| {
        matches!(shape, Shape::Rect(rectangle)
            if rectangle.rect == strip && rectangle.stroke.color == theme::TRACK_BORDER && rectangle.stroke.width > 0.0)
    });
    assert!(!outlined, "the strip is not outlined");

    let across_the_whole_strip: Vec<[egui::Pos2; 2]> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke } if stroke.color == theme::TRACK_BORDER => Some(*points),
            _ => None,
        })
        .filter(|points| points[0].x == strip.left() && points[1].x == strip.right())
        .collect();
    assert!(
        across_the_whole_strip
            .iter()
            .any(|points| points[0].y > strip.top() && points[0].y < strip.top() + 1.0 && points[0].y == points[1].y),
        "along the top"
    );
    assert!(
        across_the_whole_strip.iter().any(|points| points[0].y < strip.bottom()
            && points[0].y > strip.bottom() - 1.0
            && points[0].y == points[1].y),
        "along the bottom"
    );
}

#[test]
fn a_clip_that_goes_on_beyond_both_edges_of_the_window_has_no_rounded_corner_in_the_window() {
    let window = window_in_the_middle_of_a_long_clip();
    let shapes = window.shapes_drawn();

    // (The pictures of the video are not made in these tests, so each tile of the clip is a plain
    // grey rectangle.)
    let tiles: Vec<&egui::epaint::RectShape> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::THUMBNAIL_PLACEHOLDER => Some(rectangle),
            _ => None,
        })
        .collect();
    assert!(tiles.len() > 3, "the clip is made of several tiles: {}", tiles.len());
    for tile in tiles {
        assert_eq!(
            tile.corner_radius,
            CornerRadius::ZERO,
            "neither the start nor the end of the clip is in view: {:?}",
            tile.rect
        );
    }
}

#[test]
fn a_clip_whose_start_and_end_are_in_view_is_rounded_at_both_ends_and_only_there() {
    let mut window = Window::showing(project_of(60)); // two seconds: all of it is in view
    window.settle();
    let shapes = window.shapes_drawn();
    let tiles: Vec<&egui::epaint::RectShape> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::THUMBNAIL_PLACEHOLDER => Some(rectangle),
            _ => None,
        })
        .collect();
    assert_eq!(tiles.len(), 2, "one tile for each second");
    let radius = theme::CLIP_CORNER_RADIUS;
    assert_eq!(tiles[0].corner_radius.nw, radius, "the start of the clip");
    assert_eq!(
        tiles[0].corner_radius.ne, 0,
        "but not where the first tile meets the second"
    );
    assert_eq!(tiles[1].corner_radius.ne, radius, "the end of the clip");
    assert_eq!(tiles[1].corner_radius.nw, 0);
}

#[test]
fn every_clip_has_a_raised_edge_drawn_over_it_light_above_and_dark_below() {
    let window = window_in_the_middle_of_a_long_clip();
    let strip = window.strip_of_track(0);
    let lines: Vec<&Shape> = window
        .shapes_drawn()
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Path(path) if path.stroke.width == 1.0 && path.fill == egui::Color32::TRANSPARENT))
        .collect();
    let colours_at = |near_y: f32| -> Vec<egui::Color32> {
        lines
            .iter()
            .filter_map(|shape| match shape {
                Shape::Path(path) if path.points.iter().all(|point| (point.y - near_y).abs() < 4.0) => {
                    match &path.stroke.color {
                        egui::epaint::ColorMode::Solid(colour) => Some(*colour),
                        egui::epaint::ColorMode::UV(_) => None,
                    }
                }
                _ => None,
            })
            .collect()
    };
    let at_the_top = colours_at(strip.top() + 1.0);
    let at_the_bottom = colours_at(strip.bottom() - 1.0);
    assert!(
        at_the_top.iter().any(|colour| colour.r() > 0 && colour.a() > 100),
        "a light line along the top: {at_the_top:?}"
    );
    assert!(
        at_the_bottom.iter().any(|colour| colour.r() == 0 && colour.a() > 100),
        "a dark line along the bottom: {at_the_bottom:?}"
    );
}
