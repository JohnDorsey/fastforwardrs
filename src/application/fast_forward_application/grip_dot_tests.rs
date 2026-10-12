//! Tests of the grip dots, the little bumps that say that a track's box, a clip or a rack can be taken hold of and dragged,
//! and of the box of a track that has them on it.

use egui::{Shape, vec2};

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::theme;
use crate::application::timeline_panel::{GripDotArrangement, HEIGHT_OF_THE_ROOM_FOR_A_TITLE, VERTICES_IN_A_GRIP_DOT};
use crate::project::{TimelineFrameIndex, TrackHeight};

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 0.6, "{what}: {actual} is not {expected}");
}

fn window_with_two_tracks() -> Window {
    Window::showing(project_with_a_video_and_a_sound(300, 10))
}

/// The dots of the boxes of the tracks that were drawn, the front-most track's first. (The rows are drawn from the last to
/// the first, so that what hangs from a row is over the rows under it.)
fn dots_of_the_track_boxes(window: &Window) -> Vec<egui::Rect> {
    let mut drawn = window.grip_dots_drawn(GripDotArrangement::FOR_A_TRACK_BOX);
    drawn.sort_by(|first, second| first.top().total_cmp(&second.top()));
    drawn
}

// --- on the box of a track ---

#[test]
fn the_box_of_every_track_has_six_dots_in_two_rows_of_three_at_the_middle_of_its_top() {
    let window = window_with_two_tracks();

    let drawn = dots_of_the_track_boxes(&window);

    assert_eq!(drawn.len(), 2, "one set for each track");
    for (position, dots) in drawn.iter().enumerate() {
        let track_box = window.box_of_track(position);
        assert_close(dots.center().x, track_box.center().x, "in the middle across");
        assert!(
            dots.top() > track_box.top() && dots.top() < track_box.top() + 6.0,
            "at the top of the box: {dots:?} in {track_box:?}"
        );
        // (The dots fade out a little beyond the room they take.)
        let room = GripDotArrangement::FOR_A_TRACK_BOX.size();
        assert!(
            dots.width() > room.x && dots.width() < room.x + 2.0,
            "as wide as three dots: {dots:?}"
        );
        assert!(
            dots.height() > room.y && dots.height() < room.y + 2.0,
            "and as high as two: {dots:?}"
        );
    }
}

#[test]
fn the_title_of_a_short_track_is_below_its_dots_and_inside_its_box() {
    let mut window = window_with_two_tracks();
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackHeight {
        track,
        height: TrackHeight::from_points(0.0),
    });
    assert_close(
        window.strip_of_track(0).height(),
        TrackHeight::SMALLEST_IN_POINTS,
        "the least height",
    );

    let dots = dots_of_the_track_boxes(&window)[0];
    let title = window.title_box_of_track(0);
    let track_box = window.box_of_track(0);

    assert!(title.top() >= dots.bottom(), "{title:?} is under {dots:?}");
    assert!(
        title.bottom() <= track_box.bottom() && title.top() >= track_box.top(),
        "{title:?} is inside {track_box:?}"
    );
}

#[test]
fn the_title_of_a_tall_track_has_the_room_it_needs_in_the_middle_of_its_box_as_it_had() {
    let window = window_with_two_tracks();
    for position in 0..2 {
        // (The text field is at the top of the room it has, which is a little taller than it is.)
        assert_close(
            window.title_box_of_track(position).top(),
            window.box_of_track(position).center().y - HEIGHT_OF_THE_ROOM_FOR_A_TITLE / 2.0,
            "the room for the title is in the middle up and down",
        );
    }
}

#[test]
fn the_box_of_a_track_is_round_at_its_left_corners_and_square_at_the_two_that_face_the_strip() {
    let window = window_with_two_tracks();

    let boxes: Vec<_> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::TRACK_HEADER_BACKGROUND => Some(rectangle.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(boxes.len(), 2);
    for track_box in boxes {
        assert_eq!(track_box.corner_radius.ne, 0, "{track_box:?}");
        assert_eq!(track_box.corner_radius.se, 0);
        assert!(track_box.corner_radius.nw > 0);
        assert_eq!(track_box.corner_radius.nw, track_box.corner_radius.sw);
    }
}

#[test]
fn the_outline_of_a_selected_track_has_the_same_corners_as_its_box() {
    let mut window = window_with_two_tracks();
    window.select_the_track_at(0);

    let outlines: Vec<_> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.stroke.color == theme::SELECTION_OUTLINE => Some(rectangle.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(outlines.len(), 1, "{outlines:?}");
    assert_eq!((outlines[0].corner_radius.ne, outlines[0].corner_radius.se), (0, 0));
    assert!(outlines[0].corner_radius.nw > 0);
}

// --- on a clip ---

fn window_with_a_clip_that_is_all_in_view() -> Window {
    Window::showing(project_of(60))
}

#[test]
fn a_clip_has_ten_dots_in_two_rows_of_five_at_the_middle_of_its_top_edge() {
    let window = window_with_a_clip_that_is_all_in_view();
    let strip = window.strip_of_track(0);
    let clip_left = window.x_of_frame_on_the_strip(0, 0);
    let clip_right = window.x_of_frame_on_the_strip(0, 60);

    let drawn = window.grip_dots_drawn(GripDotArrangement::FOR_A_CLIP);

    assert_eq!(drawn.len(), 1);
    let dots = drawn[0];
    assert_close(
        dots.center().x,
        (clip_left + clip_right) / 2.0,
        "in the middle of the clip across",
    );
    assert!(
        dots.top() > strip.top() && dots.top() < strip.top() + 8.0,
        "at the top edge: {dots:?} in {strip:?}"
    );
    assert!(dots.width() > 2.0 * dots.height(), "two rows of five");
}

#[test]
fn the_dots_of_a_clip_that_goes_beyond_the_view_are_in_the_middle_of_the_part_that_can_be_seen() {
    let window = Window::showing(project_of(300));
    let strip = window.strip_of_track(0);

    let dots = window.grip_dots_drawn(GripDotArrangement::FOR_A_CLIP);

    assert_eq!(dots.len(), 1);
    assert_close(
        dots[0].center().x,
        strip.center().x,
        "in the middle of what can be seen",
    );
}

#[test]
fn every_clip_has_its_own_dots_and_one_too_narrow_for_them_has_none() {
    let mut window = window_with_a_clip_that_is_all_in_view();
    window.cut_at_frame(40);
    window.cut_at_frame(41); // a clip of one frame, which is only a few points wide
    assert_eq!(window.clips().len(), 3);

    let dots = window.grip_dots_drawn(GripDotArrangement::FOR_A_CLIP);

    assert_eq!(dots.len(), 2, "the two that are wide enough");
    assert_close(
        dots[0].center().x,
        (window.x_of_frame_on_the_strip(0, 0) + window.x_of_frame_on_the_strip(0, 40)) / 2.0,
        "the first",
    );
    assert_close(
        dots[1].center().x,
        (window.x_of_frame_on_the_strip(0, 41) + window.x_of_frame_on_the_strip(0, 60)) / 2.0,
        "the last",
    );
}

#[test]
fn the_dots_of_a_clip_are_drawn_over_it_and_under_the_outline_that_shows_it_is_selected() {
    let mut window = window_with_a_clip_that_is_all_in_view();
    window.click(window.middle_of_the_track_at(30) + vec2(0.0, 10.0));
    assert_eq!(window.application().selected_clips().len(), 1);

    let shapes = window.shapes_drawn();
    let positions_of = |matches: &dyn Fn(&Shape) -> bool| -> Vec<usize> {
        shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| matches(shape))
            .map(|(index, _)| index)
            .collect()
    };
    let dots = positions_of(
        &|shape| matches!(shape, Shape::Mesh(mesh) if mesh.vertices.len() == GripDotArrangement::FOR_A_CLIP.count() * VERTICES_IN_A_GRIP_DOT),
    );
    // (The video of the test window cannot be read, so the clip is drawn as a grey place for its pictures.)
    let clip = positions_of(
        &|shape| matches!(shape, Shape::Mesh(mesh) if mesh.vertices.first().map(|vertex| vertex.color) == Some(theme::THUMBNAIL_PLACEHOLDER)),
    );
    let outline = positions_of(
        &|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.stroke.color == theme::SELECTION_OUTLINE),
    );
    // (A picture of the video is drawn for every second of the clip, and the clip lasts two.)
    assert_eq!((dots.len(), clip.len(), outline.len()), (1, 2, 1));
    assert!(
        clip[1] < dots[0],
        "the clip is drawn up to {} and its dots at {}",
        clip[1],
        dots[0]
    );
    assert!(
        dots[0] < outline[0],
        "and its dots under the outline, which is at {}",
        outline[0]
    );
}

#[test]
fn clips_of_every_kind_have_their_dots() {
    let mut window = window_with_two_tracks();
    // A sound on the front track and a video behind it.
    assert_eq!(window.grip_dots_drawn(GripDotArrangement::FOR_A_CLIP).len(), 2);
    window.do_this(Command::AddTrack);
    let in_front = window.application().project().tracks()[0].id();
    window.do_this(Command::NewTextObject {
        track: in_front,
        frame: TimelineFrameIndex::new(10),
    });
    assert_eq!(
        window.grip_dots_drawn(GripDotArrangement::FOR_A_CLIP).len(),
        3,
        "the text too"
    );
}
