//! Tests of what the strips and the clips on them look like, checked from the shapes the window
//! draws (so they need no graphics card).

use egui::epaint::{ColorMode, PathShape};
use egui::{Color32, CornerRadius, Pos2, Rect, Shape, pos2, vec2};

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

/// Whether `point` is inside the rectangle `rectangle` with its corners rounded as `radius` says.
fn rounded_rectangle_contains(rectangle: egui::Rect, radius: CornerRadius, point: egui::Pos2) -> bool {
    if !rectangle.contains(point) {
        return false;
    }
    let corners = [
        (rectangle.left_top(), radius.nw, 1.0, 1.0),
        (rectangle.right_top(), radius.ne, -1.0, 1.0),
        (rectangle.right_bottom(), radius.se, -1.0, -1.0),
        (rectangle.left_bottom(), radius.sw, 1.0, -1.0),
    ];
    corners.iter().all(|(corner, radius, towards_x, towards_y)| {
        let radius = f32::from(*radius);
        let centre = *corner + egui::vec2(towards_x * radius, towards_y * radius);
        let in_the_corner_square = (point.x - centre.x) * towards_x < 0.0 && (point.y - centre.y) * towards_y < 0.0;
        !in_the_corner_square || centre.distance(point) <= radius
    })
}

/// Whether something filled with `colour` is drawn at `point`: a rectangle, or one of the pieces of
/// polygons (with soft edges) that are drawn where a corner is cut.
fn is_drawn_in_at(shapes: &[&Shape], colour: egui::Color32, point: egui::Pos2) -> bool {
    shapes.iter().any(|shape| match shape {
        Shape::Rect(rectangle) if rectangle.fill == colour => {
            rounded_rectangle_contains(rectangle.rect, rectangle.corner_radius, point)
        }
        Shape::Mesh(mesh) => mesh.indices.chunks(3).any(|triangle| {
            let corners = [triangle[0], triangle[1], triangle[2]].map(|index| mesh.vertices[index as usize]);
            let solid = corners.iter().all(|corner| corner.color == colour);
            let [first, second, third] = corners.map(|corner| corner.pos);
            let side = |from: egui::Pos2, to: egui::Pos2| {
                (to.x - from.x) * (point.y - from.y) - (to.y - from.y) * (point.x - from.x)
            };
            let sides = [side(first, second), side(second, third), side(third, first)];
            solid && (sides.iter().all(|side| *side >= 0.0) || sides.iter().all(|side| *side <= 0.0))
        }),
        _ => false,
    })
}

/// Checks that what is drawn in the colour of the pictures not yet made fills the rounded end of a
/// clip of the first track, whose top and bottom are those of the strip: up to the curve and not past it.
/// `end_x` is where the end is, `towards_inside` is 1 for the start of the clip (the clip is to the
/// right of `end_x`) and -1 for the end of it.
fn assert_the_end_of_the_clip_is_filled_up_to_its_curve(window: &Window, end_x: f32, towards_inside: f32) {
    let strip = window.strip_of_track(0);
    let radius = f32::from(theme::CLIP_CORNER_RADIUS);
    let shapes = window.shapes_drawn();
    let drawn = |point: egui::Pos2| is_drawn_in_at(&shapes, theme::THUMBNAIL_PLACEHOLDER, point);
    // The corner circles are round (end_x + inside * radius, top + radius) and the same at the bottom.
    for (corner_y, towards_y) in [(strip.top(), 1.0), (strip.bottom(), -1.0)] {
        let centre = egui::pos2(end_x + towards_inside * radius, corner_y + towards_y * radius);
        let along_the_diagonal = egui::vec2(-towards_inside, -towards_y).normalized();
        let on_the_curve = centre + along_the_diagonal * radius;
        assert!(
            !drawn(on_the_curve + along_the_diagonal * 2.0),
            "2 points outside the curve of the corner is not drawn: {on_the_curve:?}"
        );
        assert!(
            drawn(on_the_curve - along_the_diagonal * 2.0),
            "2 points inside the curve of the corner is drawn: {on_the_curve:?}"
        );
    }
    let middle = strip.center().y;
    assert!(
        drawn(egui::pos2(end_x + towards_inside * 1.0, middle)),
        "the edge of the clip is drawn up to"
    );
}

#[test]
fn a_clip_whose_start_and_end_are_in_view_has_its_pictures_cut_to_its_rounded_corners_at_both_ends_and_only_there() {
    let mut window = Window::showing(project_of(60)); // two seconds: all of it is in view
    window.settle();
    let start = window.x_of_frame_on_the_strip(0, 0);
    let end = window.x_of_frame_on_the_strip(0, 60);
    assert_the_end_of_the_clip_is_filled_up_to_its_curve(&window, start, 1.0);
    assert_the_end_of_the_clip_is_filled_up_to_its_curve(&window, end, -1.0);

    // Where the picture of the first second meets the picture of the second there is no seam, and no
    // rounding: the two pictures are one strip.
    let strip = window.strip_of_track(0);
    let shapes = window.shapes_drawn();
    let middle_of_the_clip = window.x_of_frame_on_the_strip(0, 30);
    for across in [-3.0, -1.0, -0.2, 0.2, 1.0, 3.0] {
        for y in [strip.top() + 2.0, strip.center().y, strip.bottom() - 2.0] {
            assert!(
                is_drawn_in_at(
                    &shapes,
                    theme::THUMBNAIL_PLACEHOLDER,
                    egui::pos2(middle_of_the_clip + across, y)
                ),
                "{across} points across the meeting of the two pictures, {y} down"
            );
        }
    }
}

#[test]
fn a_last_picture_that_is_only_a_sliver_does_not_stick_out_of_the_rounded_end_of_its_clip() {
    // 61 frames: the third picture of the clip is shown for one frame, which is a few points wide, and a
    // few points is less than the radius of the corners.
    let mut window = Window::showing(project_of(61));
    window.settle();
    let end = window.x_of_frame_on_the_strip(0, 61);
    let before_the_end = window.x_of_frame_on_the_strip(0, 60);
    assert!(
        end - before_the_end < 8.0,
        "the setup: the last picture is a sliver: {}",
        end - before_the_end
    );
    assert_the_end_of_the_clip_is_filled_up_to_its_curve(&window, end, -1.0);
}

/// The lines of the bevel of the clips that were drawn: the thin lines that have no fill.
fn bevel_lines_drawn(window: &Window) -> Vec<&PathShape> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) if path.stroke.width == 1.0 && path.fill == Color32::TRANSPARENT => Some(path),
            _ => None,
        })
        .collect()
}

/// The colour that the line `path` has at `point`: its one colour, or, for a line that is lit more at some places than at others,
/// the colour it has there.
fn colour_of_the_line_at(path: &PathShape, point: Pos2) -> Color32 {
    match &path.stroke.color {
        ColorMode::Solid(colour) => *colour,
        ColorMode::UV(colour_at) => colour_at(Rect::NAN, point),
    }
}

/// How strongly lit a colour that is white (light) or black (dark) is, with a sign: positive for light, negative for dark.
fn signed_strength(colour: Color32) -> f32 {
    if colour.r() > 0 {
        f32::from(colour.a())
    } else {
        -f32::from(colour.a())
    }
}

#[test]
fn every_clip_has_a_raised_edge_drawn_over_it_light_above_and_dark_below() {
    let window = window_in_the_middle_of_a_long_clip();
    let strip = window.strip_of_track(0);
    let lines = bevel_lines_drawn(&window);
    let colours_at = |near_y: f32| -> Vec<Color32> {
        lines
            .iter()
            .filter(|path| path.points.iter().all(|point| (point.y - near_y).abs() < 4.0))
            .flat_map(|path| path.points.iter().map(|point| colour_of_the_line_at(path, *point)))
            .collect()
    };
    let at_the_top = colours_at(strip.top() + 1.0);
    let at_the_bottom = colours_at(strip.bottom() - 1.0);
    assert!(
        at_the_top.iter().any(|colour| colour.r() > 0 && colour.a() > 100),
        "a light line along the top: {at_the_top:?}"
    );
    assert!(
        at_the_top.iter().all(|colour| colour.r() > 0 || colour.a() == 0),
        "and nothing dark there: {at_the_top:?}"
    );
    assert!(
        at_the_bottom.iter().any(|colour| colour.r() == 0 && colour.a() > 100),
        "a dark line along the bottom: {at_the_bottom:?}"
    );
    assert!(
        at_the_bottom.iter().all(|colour| colour.r() == 0),
        "and nothing light there: {at_the_bottom:?}"
    );
}

/// The outermost layer of the bevel of the one clip of a window, which is all in view: its two lines (the clip is the first
/// thing drawn, and its layers are drawn from the outermost inwards), and the rectangle they go round, which is half a point
/// inside the clip.
fn outermost_layer_of_the_bevel_of_the_one_clip(window: &Window) -> (Vec<&PathShape>, Rect) {
    let outer: Vec<&PathShape> = bevel_lines_drawn(window).into_iter().take(2).collect();
    let points: Vec<Pos2> = outer.iter().flat_map(|path| path.points.iter().copied()).collect();
    (outer, Rect::from_points(&points))
}

#[test]
fn the_edge_of_a_clip_that_is_all_in_view_is_lit_smoothly_all_the_way_round_and_brightest_at_its_top_left_corner() {
    let window = Window::showing(project_of(60)); // two seconds: all of it is in view
    let (lines, outline) = outermost_layer_of_the_bevel_of_the_one_clip(&window);
    let radius = f32::from(theme::CLIP_CORNER_RADIUS) - 0.5;
    assert_eq!(
        lines.len(),
        2,
        "the outline is in two pieces, which meet where the light is nothing"
    );
    assert!(
        (outline.top() - (window.strip_of_track(0).top() + 0.5)).abs() < 0.01,
        "the setup: these are the lines that go half a point inside the clip: {outline:?}"
    );

    let mut brightest = (Pos2::ZERO, f32::MIN);
    for line in &lines {
        for pair in line.points.windows(2) {
            let (from, to) = (
                signed_strength(colour_of_the_line_at(line, pair[0])),
                signed_strength(colour_of_the_line_at(line, pair[1])),
            );
            let distance = pair[0].distance(pair[1]).max(1e-3);
            assert!(
                (to - from).abs() / distance < 20.0,
                "the light does not jump: from {:?} to {:?} it goes from {from} to {to}",
                pair[0],
                pair[1]
            );
        }
        for point in &line.points {
            let strength = signed_strength(colour_of_the_line_at(line, *point));
            if strength > brightest.1 {
                brightest = (*point, strength);
            }
        }
    }
    let top_left_corner = Rect::from_min_size(outline.left_top(), vec2(radius, radius));
    assert!(
        top_left_corner.expand(1.0).contains(brightest.0),
        "the brightest light, {}, is in the top left corner: {:?}, the corner is {top_left_corner:?}",
        brightest.1,
        brightest.0
    );
    assert!(
        brightest.1 > 200.0,
        "and it is brighter than the 150 of the edges: {}",
        brightest.1
    );

    // At the top right corner and the bottom left corner the light passes through nothing.
    for (name, corner) in [
        (
            "top right",
            Rect::from_min_size(pos2(outline.right() - radius, outline.top()), vec2(radius, radius)),
        ),
        (
            "bottom left",
            Rect::from_min_size(pos2(outline.left(), outline.bottom() - radius), vec2(radius, radius)),
        ),
    ] {
        let strengths: Vec<f32> = lines
            .iter()
            .flat_map(|line| {
                line.points
                    .iter()
                    .filter(|point| corner.expand(0.01).contains(**point))
                    .map(|point| signed_strength(colour_of_the_line_at(line, *point)))
            })
            .collect();
        assert!(!strengths.is_empty(), "{name}: the corner is drawn");
        assert!(
            strengths.iter().any(|strength| *strength > 40.0) && strengths.iter().any(|strength| *strength < -40.0),
            "{name}: it goes from light to dark in the corner: {strengths:?}"
        );
        assert!(
            strengths.iter().any(|strength| strength.abs() < 30.0),
            "{name}: and passes close to nothing between them: {strengths:?}"
        );
    }
}
