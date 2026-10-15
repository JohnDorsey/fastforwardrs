// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of zooming the timeline in and out with Ctrl and the mouse wheel, and of the ruler that
//! has to follow the zoom.

use egui::{Modifiers, Pos2, vec2};

use super::test_window::{Window, project_of};

/// How much a turn of the wheel by this many points makes the zoom change (egui makes a zoom of
/// `exp(delta / 200)` of a turn).
const ONE_NOTCH: f32 = 100.0;

/// A window showing a 30 second video, which is far wider than the window at the normal zoom.
fn window_with_a_long_video() -> Window {
    Window::showing(project_of(900))
}

/// How wide a second of the video is on the screen at the moment, in points.
fn width_of_a_second(window: &Window) -> f32 {
    window.x_of_frame_on_the_strip(0, 60) - window.x_of_frame_on_the_strip(0, 30)
}

fn zoom_with_the_wheel(window: &mut Window, position: Pos2, notches: f32) {
    window.scroll_the_wheel(position, vec2(0.0, notches * ONE_NOTCH), Modifiers::COMMAND);
}

/// The text of everything that was written in the last frame.
fn writing_on_the_screen(window: &Window) -> Vec<String> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn turning_the_wheel_with_control_held_zooms_in_and_turning_it_back_zooms_out_again() {
    let mut window = window_with_a_long_video();
    let normal = width_of_a_second(&window);
    let over_the_strip = window.middle_of_the_track_at(100);

    zoom_with_the_wheel(&mut window, over_the_strip, 1.0);
    let zoomed_in = width_of_a_second(&window);
    assert!(
        zoomed_in > normal * 1.5,
        "a second is wider: {normal} became {zoomed_in}"
    );

    zoom_with_the_wheel(&mut window, over_the_strip, -1.0);
    let back = width_of_a_second(&window);
    assert!(
        (back - normal).abs() < 0.5,
        "turning it back zooms back out: {normal}, then {zoomed_in}, then {back}"
    );
}

#[test]
fn zooming_in_keeps_the_frame_under_the_pointer_where_it_is_on_the_screen() {
    let mut window = window_with_a_long_video();
    let pointer = window.middle_of_the_track_at(100);

    zoom_with_the_wheel(&mut window, pointer, 1.0);

    let after = window.x_of_frame_on_the_strip(0, 100);
    assert!(
        (after - pointer.x).abs() < 1.5,
        "frame 100 was at {} and is at {after}",
        pointer.x
    );
    assert!(width_of_a_second(&window) > 250.0, "and it did zoom in");
}

#[test]
fn the_zoom_works_from_the_ruler_and_from_any_track_but_not_from_elsewhere_in_the_window() {
    let normal = width_of_a_second(&window_with_a_long_video());

    for (place, zooms) in [
        ("the ruler", true),
        ("a strip", true),
        ("the preview", false),
        ("the title boxes", false),
    ] {
        let mut window = window_with_a_long_video();
        let position = match place {
            "the ruler" => {
                let ruler = window.rectangle_of("Ruler");
                Pos2::new(ruler.center().x, ruler.center().y)
            }
            "a strip" => window.middle_of_the_track_at(100),
            "the preview" => Pos2::new(600.0, 150.0),
            _ => window.title_box_of_track(0).center(),
        };
        zoom_with_the_wheel(&mut window, position, 1.0);
        let changed = (width_of_a_second(&window) - normal).abs() > 1.0;
        assert_eq!(changed, zooms, "the wheel with Ctrl held over {place}");
    }
}

#[test]
fn the_wheel_without_control_scrolls_and_does_not_zoom() {
    let mut window = window_with_a_long_video();
    let normal = width_of_a_second(&window);
    let scroll_before = window.application().timeline_view.scroll_offset;

    let over_the_strip = window.middle_of_the_track_at(100);
    window.scroll_the_wheel(over_the_strip, vec2(0.0, -ONE_NOTCH), Modifiers::NONE);

    assert!((width_of_a_second(&window) - normal).abs() < 0.01);
    assert!(window.application().timeline_view.scroll_offset > scroll_before);
}

#[test]
fn zooming_has_limits_a_second_is_never_narrower_than_two_points_nor_a_frame_wider_than_forty() {
    let mut window = window_with_a_long_video();
    let over_the_strip = window.middle_of_the_track_at(100);

    zoom_with_the_wheel(&mut window, over_the_strip, 40.0);
    let frame_width = window.x_of_frame_on_the_strip(0, 101) - window.x_of_frame_on_the_strip(0, 100);
    assert!(
        (frame_width - 40.0).abs() < 0.01,
        "a frame is {frame_width} points wide at the most"
    );

    zoom_with_the_wheel(&mut window, over_the_strip, -80.0);
    assert!(
        (width_of_a_second(&window) - 2.0).abs() < 0.01,
        "a second is {} points wide at the least",
        width_of_a_second(&window)
    );
    assert_eq!(
        window.application().timeline_view.scroll_offset,
        0.0,
        "all of the strip fits, so there is nowhere to scroll to"
    );
}

#[test]
fn zooming_out_never_leaves_the_view_before_the_start_or_past_the_end_of_the_strip() {
    let mut window = window_with_a_long_video();
    window.move_current_frame_to(850);
    let near_the_end = window.middle_of_the_track_at(850);

    zoom_with_the_wheel(&mut window, near_the_end, -1.0);

    let strip = window.strip_of_track(0);
    let end_of_the_video = window.x_of_frame_on_the_strip(0, 900);
    assert!(window.application().timeline_view.scroll_offset >= 0.0);
    assert!(
        end_of_the_video >= strip.right() - 0.5 || window.x_of_frame_on_the_strip(0, 0) >= strip.left() - 0.5,
        "the strip is not scrolled past its end: it ends at {end_of_the_video}, the view at {}",
        strip.right()
    );
}

#[test]
fn the_ruler_the_playhead_and_the_strips_agree_after_zooming_so_clicking_the_ruler_picks_the_frame_it_is_over() {
    let mut window = window_with_a_long_video();
    let over_the_strip = window.middle_of_the_track_at(100);
    zoom_with_the_wheel(&mut window, over_the_strip, 1.0);

    let point = window.point_on_the_ruler_at(120);
    window.click(point);

    assert_eq!(window.current_frame(), 120);
    let playhead = window.x_of_the_drawn_playhead().expect("the playhead is in view");
    assert!(
        (playhead - window.x_of_frame_on_the_strip(0, 120)).abs() < 1.0,
        "the red line is at the start of frame 120 on the strip"
    );
}

#[test]
fn a_different_project_starts_at_the_normal_zoom() {
    let mut window = window_with_a_long_video();
    let normal = width_of_a_second(&window);
    let over_the_strip = window.middle_of_the_track_at(100);
    zoom_with_the_wheel(&mut window, over_the_strip, 2.0);
    assert!(width_of_a_second(&window) > normal * 2.0);

    window.application_mut().replace_project(project_of(900), None);
    window.settle();

    assert!((width_of_a_second(&window) - normal).abs() < 0.01);
}

// --- the ruler ---

#[test]
fn at_the_normal_zoom_the_ruler_is_labelled_every_second() {
    let window = window_with_a_long_video();
    let writing = writing_on_the_screen(&window);
    for label in ["0:00", "0:01", "0:02", "0:03"] {
        assert!(
            writing.contains(&label.to_owned()),
            "{label} is on the ruler: {writing:?}"
        );
    }
}

#[test]
fn zoomed_far_out_the_ruler_is_labelled_with_minutes_and_far_apart() {
    let mut window = Window::showing(project_of(3600)); // two minutes
    let over_the_strip = window.middle_of_the_track_at(100);
    zoom_with_the_wheel(&mut window, over_the_strip, -40.0);
    assert!((width_of_a_second(&window) - 2.0).abs() < 0.01);

    let writing = writing_on_the_screen(&window);
    for label in ["0:00", "1:00", "2:00"] {
        assert!(
            writing.contains(&label.to_owned()),
            "{label} is on the ruler: {writing:?}"
        );
    }
    assert!(
        !writing.contains(&"0:01".to_owned()),
        "the seconds are too close together to label now: {writing:?}"
    );
}

#[test]
fn the_labels_do_not_overlap_whatever_the_zoom() {
    let mut window = Window::showing(project_of(3600));
    let over_the_strip = window.middle_of_the_track_at(100);
    for notches in [-0.5, -0.5, -1.0, 2.0, 2.0, 1.0, 1.0] {
        zoom_with_the_wheel(&mut window, over_the_strip, notches);
        let ruler = window.rectangle_of("Ruler");
        let mut labels: Vec<egui::Rect> = window
            .shapes_drawn()
            .into_iter()
            .filter_map(|shape| match shape {
                egui::Shape::Text(text) if ruler.contains_rect(text.visual_bounding_rect()) => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .collect();
        labels.sort_by(|first, second| first.left().total_cmp(&second.left()));
        for pair in labels.windows(2) {
            assert!(
                pair[0].right() < pair[1].left(),
                "{:?} and {:?} overlap at {} points a second",
                pair[0],
                pair[1],
                width_of_a_second(&window)
            );
        }
    }
}
