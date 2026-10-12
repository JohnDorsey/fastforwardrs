//! Tests of the drawers on clips and on tracks: the button at the bottom edge, which is there when something says it should
//! be, the drawer that slides open when it is clicked and pushes the rows under it down (or lies over them, when it was opened with
//! Shift held), the rows that scroll to bring a drawer that is opening into view, and what it is that a drawer takes.

use egui::{PointerButton, Pos2, Rect, Shape, epaint::RectShape, pos2, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::theme;
use crate::application::timeline_panel::{drawer_button_label, track_drawer_button_label};
use crate::project::{AnimationTarget, PropertyId, TimelineFrameIndex, TrackHeight};

/// How tall a drawer with nothing in it is when it is open: room above where the racks are, and the room for the rounding
/// of the corners under that.
const EMPTY_DRAWER_HEIGHT: f32 = theme::SPACE_ABOVE_THE_RACKS + theme::CLIP_CORNER_RADIUS as f32;

/// A window with one video clip that is 2 seconds (60 frames) long, which is all in view.
fn window_with_a_short_clip() -> Window {
    Window::showing(project_of(60))
}

fn label_of_the_button_of(window: &Window, position: usize, clip_index: usize) -> String {
    drawer_button_label(&window.title_of_track(position), window.clip(position, clip_index))
}

/// Where the button of the drawer of the `clip_index`th clip of the track at `position` is, if it has one: where it can be
/// taken hold of, which is where it is drawn when it is.
fn button_of(window: &Window, position: usize, clip_index: usize) -> Option<Rect> {
    window
        .harness
        .query_by_label(&label_of_the_button_of(window, position, clip_index))
        .map(|button| button.rect())
}

fn the_button_of(window: &Window, position: usize, clip_index: usize) -> Rect {
    button_of(window, position, clip_index).expect("the clip has a button for its drawer")
}

/// Where the button of the drawer of the track at `position` is.
fn the_button_of_the_track(window: &Window, position: usize) -> Rect {
    window
        .harness
        .get_by_label(&track_drawer_button_label(&window.title_of_track(position)))
        .rect()
}

/// Where the clip that starts at `first_frame` and ends at `end_frame` on the track at `position` is on the screen.
fn clip_rectangle(window: &Window, position: usize, first_frame: u64, end_frame: u64) -> Rect {
    let strip = window.strip_of_track(position);
    Rect::from_min_max(
        pos2(window.x_of_frame_on_the_strip(position, first_frame), strip.top()),
        pos2(window.x_of_frame_on_the_strip(position, end_frame), strip.bottom()),
    )
}

/// The buttons that were drawn in the last frame.
fn buttons_drawn(window: &Window) -> Vec<RectShape> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle)
                if rectangle.fill == theme::DRAWER_BUTTON_FILL
                    || rectangle.fill == theme::DRAWER_BUTTON_FILL_HOVERED =>
            {
                Some(rectangle.clone())
            }
            _ => None,
        })
        .collect()
}

/// The drawers that were drawn in the last frame.
fn drawers_drawn(window: &Window) -> Vec<RectShape> {
    window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND => Some(rectangle.clone()),
            _ => None,
        })
        .collect()
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 0.6, "{what}: {actual} is not {expected}");
}

fn let_the_drawers_finish(window: &mut Window) {
    window.harness.run_steps(40);
}

/// How far along the screen the middle of the first clip of the track at `position` is, which is all in view.
fn the_middle_of_the_first_clip_of(window: &Window, position: usize) -> f32 {
    let placement = window.application().project().tracks()[position]
        .placement_at_index(0)
        .expect("the track has a clip");
    (window.x_of_frame_on_the_strip(position, placement.first_timeline_frame.value())
        + window.x_of_frame_on_the_strip(position, placement.end_timeline_frame.value()))
        / 2.0
}

/// Puts the pointer over the middle of the first clip of the track at `position`, above where its button is.
fn hover_over_the_clip(window: &mut Window, position: usize) {
    let strip = window.strip_of_track(position);
    window.move_pointer_to(Pos2::new(
        the_middle_of_the_first_clip_of(window, position),
        strip.top() + 20.0,
    ));
    window.harness.run_steps(2);
}

fn move_the_pointer_away(window: &mut Window) {
    window.move_pointer_to(Pos2::new(5.0, 5.0));
    window.harness.run_steps(2);
}

/// Clicks the button of the drawer of the first clip of the track at `position`.
fn click_the_button(window: &mut Window, position: usize) {
    let centre = the_button_of(window, position, 0).center();
    window.click(centre);
}

/// Clicks the button of the drawer of the first clip of the track at `position` with Shift held, which opens it over the rows
/// under it and not by pushing them down.
fn shift_click_the_button(window: &mut Window, position: usize) {
    let centre = the_button_of(window, position, 0).center();
    window.click_with_shift(centre);
}

/// Puts a keyframe on the loudness of the clip, so that its drawer has a rack in it.
fn put_a_rack_in_the_drawer_of_the_first_clip_of(window: &mut Window, position: usize) {
    let clip = window.clip(position, 0);
    window.do_this(Command::AddKeyframe {
        target: AnimationTarget::Clip(clip),
        property: PropertyId::Gain,
        frame: TimelineFrameIndex::new(5),
    });
}

/// A window with a sound on the track in front and a video on the one behind it, whose clips are all in view.
fn window_with_two_tracks() -> Window {
    Window::showing(project_with_a_video_and_a_sound(60, 2))
}

// --- the button ---

#[test]
fn a_clip_has_no_button_to_see_until_it_is_hovered_over_and_has_it_only_while_it_is() {
    let mut window = window_with_a_short_clip();
    assert!(buttons_drawn(&window).is_empty(), "nothing is hovered over");

    hover_over_the_clip(&mut window, 0);
    assert_eq!(buttons_drawn(&window).len(), 1, "the pointer is over the clip");

    move_the_pointer_away(&mut window);
    assert!(buttons_drawn(&window).is_empty(), "and it goes when the pointer does");
}

#[test]
fn the_button_of_a_clip_is_wide_and_thin_in_the_middle_of_it_and_its_bottom_edge_is_the_clips() {
    let mut window = window_with_a_short_clip();
    hover_over_the_clip(&mut window, 0);
    let clip = clip_rectangle(&window, 0, 0, 60);
    let drawn = buttons_drawn(&window);
    assert_eq!(drawn.len(), 1);
    let button = drawn[0].rect;

    assert_close(
        button.width() / button.height(),
        5.0,
        "it is about 5 times as wide as it is tall",
    );
    assert_close(button.center().x, clip.center().x, "in the middle of the clip across");
    assert!(clip.contains_rect(button), "over the clip: {button:?} in {clip:?}");
    assert_close(
        button.bottom(),
        clip.bottom(),
        "its bottom edge is the bottom edge of the clip",
    );
    assert!(button.height() < clip.height() / 4.0, "thin");
    assert_eq!(
        button,
        the_button_of(&window, 0, 0),
        "where it is drawn is where it can be taken hold of"
    );
}

#[test]
fn the_two_bottom_corners_of_the_button_are_square_and_the_two_at_the_top_are_round() {
    let mut window = window_with_a_short_clip();
    hover_over_the_clip(&mut window, 0);

    let drawn = buttons_drawn(&window);

    assert_eq!(drawn.len(), 1);
    let corners = drawn[0].corner_radius;
    assert_eq!((corners.sw, corners.se), (0, 0), "square at the bottom");
    assert!(
        corners.nw > 0 && corners.nw == corners.ne,
        "round at the top: {corners:?}"
    );
}

#[test]
fn at_the_least_height_of_a_track_the_button_covers_less_than_half_of_the_clip() {
    let mut window = window_with_a_short_clip();
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackHeight {
        track,
        height: TrackHeight::from_points(0.0),
    });
    let clip = clip_rectangle(&window, 0, 0, 60);
    assert_close(
        clip.height(),
        TrackHeight::SMALLEST_IN_POINTS,
        "the least height of a track",
    );
    hover_over_the_clip(&mut window, 0);

    let button = buttons_drawn(&window)[0].rect;

    assert!(
        button.height() < clip.height() / 2.0,
        "{button:?} is not less than half of {clip:?}"
    );
    assert!(clip.contains_rect(button));
    assert_close(
        button.width() / button.height(),
        5.0,
        "and about 5 times as wide as it is tall",
    );
}

#[test]
fn every_clip_has_its_own_button_in_the_middle_of_it_and_one_too_narrow_for_it_has_none() {
    let mut window = window_with_a_short_clip();
    window.cut_at_frame(40);
    window.cut_at_frame(41); // a clip of one frame, which is only a few points wide
    assert_eq!(window.clips().len(), 3);

    let first = the_button_of(&window, 0, 0);
    let third = the_button_of(&window, 0, 2);
    assert_close(
        first.center().x,
        clip_rectangle(&window, 0, 0, 40).center().x,
        "the first",
    );
    assert_close(
        third.center().x,
        clip_rectangle(&window, 0, 41, 60).center().x,
        "the third",
    );
    assert_eq!(
        button_of(&window, 0, 1),
        None,
        "the one of a single frame has no button: it could not be hit"
    );
}

#[test]
fn the_button_of_a_clip_that_goes_beyond_the_view_is_in_the_part_that_can_be_seen() {
    let window = Window::showing(project_of(300));
    let strip = window.strip_of_track(0);
    let button = the_button_of(&window, 0, 0);
    assert!(strip.contains_rect(button), "{button:?} is not inside {strip:?}");
    assert_close(
        button.center().x,
        strip.center().x,
        "in the middle of what can be seen of the clip",
    );
}

#[test]
fn the_button_stays_while_the_drawer_is_open_wherever_the_pointer_is_and_goes_when_it_has_shut() {
    let mut window = window_with_a_short_clip();
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    move_the_pointer_away(&mut window);
    assert_eq!(buttons_drawn(&window).len(), 1, "the drawer is open");

    hover_over_the_clip(&mut window, 0);
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    move_the_pointer_away(&mut window);

    assert!(
        buttons_drawn(&window).is_empty(),
        "the drawer is shut and the pointer is not here"
    );
}

#[test]
fn the_button_stays_while_there_is_something_in_the_drawer_and_goes_when_what_was_in_it_is_taken_out() {
    let mut window = window_with_a_short_clip();
    put_a_rack_in_the_drawer_of_the_first_clip_of(&mut window, 0);
    let_the_drawers_finish(&mut window);
    // (The drawer was opened by the keyframe: it is shut by hand, so that it is only what is in it that keeps the button.)
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    move_the_pointer_away(&mut window);
    assert!(drawers_drawn(&window).is_empty(), "the drawer is shut");
    assert_eq!(buttons_drawn(&window).len(), 1, "a rack is in the drawer");

    let clip = window.clip(0, 0);
    window.do_this(Command::RemoveKeyframe {
        target: AnimationTarget::Clip(clip),
        property: PropertyId::Gain,
        frame: TimelineFrameIndex::new(5),
    });
    move_the_pointer_away(&mut window);
    assert!(buttons_drawn(&window).is_empty(), "the drawer is empty again");
}

// --- opening and shutting ---

#[test]
fn clicking_the_button_slides_the_drawer_open_in_a_quick_animation_and_clicking_again_shuts_it() {
    let mut window = window_with_a_short_clip();
    assert!(
        drawers_drawn(&window).is_empty(),
        "shut to begin with: nothing is seen of it"
    );
    hover_over_the_clip(&mut window, 0);
    let centre = the_button_of(&window, 0, 0).center();
    window.move_pointer_to(centre);
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);

    let mut heights = Vec::new();
    for _ in 0..30 {
        window.harness.step();
        heights.push(
            drawers_drawn(&window)
                .first()
                .map_or(0.0, |drawer| drawer.rect.height()),
        );
    }
    let full = *heights.last().unwrap();
    assert!(full > EMPTY_DRAWER_HEIGHT, "open: {heights:?}");
    assert!(
        heights.windows(2).all(|pair| pair[1] >= pair[0]),
        "it only opens: {heights:?}"
    );
    let frames_to_open = heights.iter().position(|height| (*height - full).abs() < 0.01).unwrap() + 1;
    assert!(
        (6..=20).contains(&frames_to_open),
        "a quick animation, not {frames_to_open} frames: {heights:?}"
    );
    assert!(
        heights[0] < full / 2.0,
        "it has not opened in a single frame: {heights:?}"
    );

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert!(drawers_drawn(&window).is_empty(), "shut again");
}

#[test]
fn a_drawer_is_as_wide_as_its_clip_hangs_from_its_bottom_edge_and_is_rounded_where_the_clip_is() {
    let mut window = window_with_a_short_clip();
    hover_over_the_clip(&mut window, 0);
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    let clip = clip_rectangle(&window, 0, 0, 60);
    let drawers = drawers_drawn(&window);
    assert_eq!(drawers.len(), 1);
    let drawer = &drawers[0];
    assert_close(drawer.rect.left(), clip.left(), "its left edge is the clip's");
    assert_close(drawer.rect.right(), clip.right(), "and its right edge");
    assert_close(
        drawer.rect.bottom(),
        clip.bottom() + EMPTY_DRAWER_HEIGHT,
        "it hangs down by its height, which has no rack in it",
    );
    let radius = f32::from(theme::CLIP_CORNER_RADIUS);
    assert_close(
        drawer.rect.top(),
        clip.bottom() - radius,
        "and reaches up behind the clip into the room outside its rounded corners",
    );
    assert_eq!(drawer.corner_radius.nw, 0, "the corners that face the clip are square");
    assert_eq!(drawer.corner_radius.ne, 0);
    assert_eq!(
        drawer.corner_radius.sw,
        theme::CLIP_CORNER_RADIUS,
        "the outer two are as round as the clip's"
    );
    assert_eq!(drawer.corner_radius.se, theme::CLIP_CORNER_RADIUS);
}

#[test]
fn a_drawer_with_a_rack_in_it_is_taller_by_the_height_of_the_rack() {
    let mut window = window_with_a_short_clip();
    // (The keyframe opens the drawer it is in.)
    put_a_rack_in_the_drawer_of_the_first_clip_of(&mut window, 0);
    let_the_drawers_finish(&mut window);

    let drawers = drawers_drawn(&window);

    assert_eq!(drawers.len(), 1);
    assert_close(
        drawers[0].rect.bottom(),
        clip_rectangle(&window, 0, 0, 60).bottom() + EMPTY_DRAWER_HEIGHT + theme::RACK_HEIGHT,
        "room for a rack more",
    );
}

#[test]
fn a_drawer_of_a_clip_at_the_least_height_has_the_corners_that_clip_has() {
    let mut window = window_with_a_short_clip();
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackHeight {
        track,
        height: TrackHeight::from_points(0.0),
    });
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    let drawers = drawers_drawn(&window);
    // The corners of a clip as short as that are as round as half of its height.
    let expected = (TrackHeight::SMALLEST_IN_POINTS / 2.0) as u8;
    assert_eq!(drawers[0].corner_radius.sw, expected);
    assert_eq!(drawers[0].corner_radius.se, expected);
}

// --- what a drawer does to the rows ---

/// How far the top of the strip of the track under the first is from the top of the first one's.
fn distance_to_the_row_under(window: &Window) -> f32 {
    window.strip_of_track(1).top() - window.strip_of_track(0).top()
}

#[test]
fn the_row_under_a_drawer_that_is_opened_with_shift_held_stays_where_it_is_the_drawer_hangs_over_it() {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);
    let strip_before = window.strip_of_track(0);
    let ruler_before = window.rectangle_of("Ruler");

    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    assert_eq!(drawers_drawn(&window).len(), 1, "it is open");
    assert_close(
        distance_to_the_row_under(&window),
        distance_before,
        "the row under it did not move",
    );
    assert_eq!(window.strip_of_track(0), strip_before, "nor did the row it hangs from");
    assert_eq!(
        window.rectangle_of("Ruler"),
        ruler_before,
        "and the timeline is as tall as it was"
    );
    let drawer = &drawers_drawn(&window)[0];
    assert!(
        drawer.rect.bottom() > window.strip_of_track(1).top() + 10.0,
        "the drawer reaches over the strip of the row under it: {:?} over {:?}",
        drawer.rect,
        window.strip_of_track(1)
    );
}

#[test]
fn the_row_under_does_not_move_at_any_moment_while_a_drawer_that_is_opened_with_shift_held_slides() {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);
    hover_over_the_clip(&mut window, 0);
    let centre = the_button_of(&window, 0, 0).center();
    window.move_pointer_to(centre);
    window.hold_shift();
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);
    window.let_go_of_shift();

    for _ in 0..30 {
        window.harness.step();
        assert_close(
            distance_to_the_row_under(&window),
            distance_before,
            "the row under it, while the drawer slides",
        );
    }
}

#[test]
fn a_drawer_that_lies_over_the_rows_under_it_is_drawn_over_the_clips_of_the_row_under_it() {
    let mut window = window_with_two_tracks();
    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    let shapes = window.shapes_drawn();
    let drawer = shapes
        .iter()
        .position(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND))
        .expect("the drawer is drawn");
    // (The video of the test window cannot be read, so its clip is drawn as grey places for its pictures.)
    let last_picture_of_the_video = shapes
        .iter()
        .rposition(|shape| {
            matches!(shape, Shape::Mesh(mesh) if mesh.vertices.first().map(|vertex| vertex.color) == Some(theme::THUMBNAIL_PLACEHOLDER))
        })
        .expect("the video of the row under is drawn");
    assert!(
        last_picture_of_the_video < drawer,
        "the video is drawn up to {last_picture_of_the_video} and the drawer, over it, at {drawer}"
    );
}

#[test]
fn clicking_where_a_drawer_hangs_over_the_row_under_it_takes_hold_of_the_drawer_and_not_of_what_is_under_it() {
    let mut window = window_with_two_tracks();
    let in_the_video_under_the_drawer = Pos2::new(
        the_middle_of_the_first_clip_of(&window, 0),
        window.strip_of_track(0).bottom() + 20.0,
    );
    // Before the drawer is open, it is the video of the row under that is there.
    window.click(in_the_video_under_the_drawer);
    assert_eq!(window.application().selected_clips().len(), 1, "the video is selected");
    // (Clicking an empty place on a strip lets go of it.)
    window.click(Pos2::new(
        window.strip_of_track(0).right() - 20.0,
        window.strip_of_track(0).center().y,
    ));
    assert_eq!(window.application().selected_clips(), vec![]);

    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    window.click(in_the_video_under_the_drawer);

    assert_eq!(
        window.application().selected_clips(),
        vec![],
        "the click went to the drawer, which does nothing"
    );
}

#[test]
fn the_rows_have_room_under_the_last_one_for_a_drawer_that_hangs_below_it() {
    let mut window = window_with_a_short_clip();
    let ruler_before = window.rectangle_of("Ruler");
    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    let (clip_rectangle_of_the_drawer, drawer) = window
        .clipped_shapes_drawn()
        .into_iter()
        .find_map(|(clip_rectangle, shape)| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND => {
                Some((clip_rectangle, rectangle.clone()))
            }
            _ => None,
        })
        .expect("the drawer is drawn");

    assert!(
        clip_rectangle_of_the_drawer.bottom() >= drawer.rect.bottom() - 0.01,
        "all of the drawer is drawn: it is cut off at {} and goes down to {}",
        clip_rectangle_of_the_drawer.bottom(),
        drawer.rect.bottom()
    );
    assert!(
        window.rectangle_of("Ruler").top() < ruler_before.top(),
        "the timeline has grown upward to have the room"
    );
}

#[test]
fn each_clip_has_a_drawer_of_its_own() {
    let mut window = window_with_a_short_clip();
    window.cut_at_frame(30);

    hover_over_the_clip(&mut window, 0);
    let centre = the_button_of(&window, 0, 1).center();
    window.click(centre);
    let_the_drawers_finish(&mut window);

    let drawers = drawers_drawn(&window);
    assert_eq!(drawers.len(), 1, "only the second clip's drawer is open");
    assert_close(
        drawers[0].rect.left(),
        clip_rectangle(&window, 0, 30, 60).left(),
        "it is under the second clip",
    );

    let centre = the_button_of(&window, 0, 0).center();
    window.click(centre);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 2, "and the first one's too");
}

// --- the drawer of a track ---

#[test]
fn a_track_has_a_button_at_the_bottom_edge_of_its_box_in_the_middle_of_it_that_is_there_while_the_box_is_hovered() {
    let mut window = window_with_two_tracks();
    assert!(buttons_drawn(&window).is_empty(), "nothing is hovered over");

    let track_box = window.box_of_track(0);
    window.move_pointer_to(Pos2::new(track_box.left() + 10.0, track_box.top() + 20.0));
    window.harness.run_steps(2);

    let drawn = buttons_drawn(&window);
    assert_eq!(drawn.len(), 1, "the box of the first track is hovered over");
    let button = drawn[0].rect;
    assert_close(
        button.center().x,
        track_box.center().x,
        "in the middle of the box across",
    );
    assert_close(button.bottom(), track_box.bottom(), "its bottom edge is the box's");
    assert_close(button.width() / button.height(), 5.0, "as wide as the button of a clip");
    assert_eq!(button, the_button_of_the_track(&window, 0));
    assert_eq!((drawn[0].corner_radius.sw, drawn[0].corner_radius.se), (0, 0));

    move_the_pointer_away(&mut window);
    assert!(buttons_drawn(&window).is_empty());
}

#[test]
fn the_drawer_of_a_track_slides_open_as_wide_as_its_strip_under_it_with_square_corners_over_the_row_under() {
    let mut window = window_with_two_tracks();
    let strip = window.strip_of_track(0);
    let row_under = window.strip_of_track(1);
    let button = the_button_of_the_track(&window, 0).center();
    window.click_with_shift(button);
    let_the_drawers_finish(&mut window);

    let drawers = drawers_drawn(&window);

    assert_eq!(drawers.len(), 1);
    let drawer = &drawers[0];
    assert_close(drawer.rect.left(), strip.left(), "as wide as the strip");
    assert_close(drawer.rect.right(), strip.right(), "to its right edge too");
    assert_close(drawer.rect.top(), strip.bottom(), "it hangs from its bottom edge");
    assert_close(
        drawer.rect.bottom(),
        strip.bottom() + EMPTY_DRAWER_HEIGHT,
        "by its height",
    );
    assert_eq!(drawer.corner_radius, egui::CornerRadius::ZERO);
    assert_eq!(window.strip_of_track(1), row_under, "the row under is where it was");
}

#[test]
fn the_drawer_of_a_track_and_the_drawers_of_its_clips_are_opened_and_shut_apart() {
    let mut window = window_with_two_tracks();
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 1, "only the track's");

    hover_over_the_clip(&mut window, 0);
    let clip_button = the_button_of(&window, 0, 0).center();
    window.click(clip_button);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 2, "and the clip's too");

    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    assert_eq!(
        drawers_drawn(&window).len(),
        1,
        "the clip's stays when the track's is shut"
    );
}

#[test]
fn the_drawer_of_a_track_hangs_below_the_drawers_of_its_clips_that_are_open_and_follows_them_as_they_slide() {
    let mut window = window_with_two_tracks();
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    let drawer_of_the_track = drawers_drawn(&window)[0].clone();
    assert_close(
        drawer_of_the_track.rect.top(),
        window.strip_of_track(0).bottom(),
        "right under the strip while no clip's drawer is open",
    );

    hover_over_the_clip(&mut window, 0);
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    // (The timeline has grown upward, to have room for the drawers that pushed the row under them down.)
    let strip = window.strip_of_track(0);
    let drawers = drawers_drawn(&window);
    assert_eq!(drawers.len(), 2);
    let of_the_track = drawers
        .iter()
        .find(|drawer| drawer.corner_radius == egui::CornerRadius::ZERO && drawer.rect.width() > 500.0)
        .expect("the drawer of the track is as wide as the strip");
    assert_close(
        of_the_track.rect.top(),
        strip.bottom() + EMPTY_DRAWER_HEIGHT,
        "it hangs from where the drawer of the clip ends, and does not cover it",
    );
    assert_close(
        of_the_track.rect.bottom(),
        strip.bottom() + 2.0 * EMPTY_DRAWER_HEIGHT,
        "by its own height",
    );
}

#[test]
fn the_drawer_of_a_track_moves_down_with_the_drawers_of_its_clips_as_they_slide_open() {
    let mut window = window_with_two_tracks();
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    hover_over_the_clip(&mut window, 0);
    let centre = the_button_of(&window, 0, 0).center();
    window.move_pointer_to(centre);
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);

    let mut gaps_below_the_strip = Vec::new();
    for _ in 0..12 {
        window.harness.step();
        // (The timeline grows upward as the drawers that push the rows under them down slide open: the strip moves.)
        let strip = window.strip_of_track(0);
        let drawers = drawers_drawn(&window);
        let of_the_clip = drawers
            .iter()
            .find(|drawer| drawer.corner_radius.sw > 0)
            .expect("the drawer of the clip is sliding open");
        let of_the_track = drawers
            .iter()
            .find(|drawer| drawer.corner_radius == egui::CornerRadius::ZERO && drawer.rect.width() > 500.0)
            .expect("the drawer of the track is open");
        assert_close(
            of_the_track.rect.top(),
            of_the_clip.rect.bottom(),
            "the drawer of the track hangs from where the drawer of the clip is now, at every moment",
        );
        gaps_below_the_strip.push(of_the_track.rect.top() - strip.bottom());
    }
    assert!(
        gaps_below_the_strip.windows(2).all(|pair| pair[1] >= pair[0])
            && gaps_below_the_strip[0] < gaps_below_the_strip[11],
        "it goes down as the other opens: {gaps_below_the_strip:?}"
    );
}

#[test]
fn the_button_of_a_track_stays_while_its_drawer_is_open_or_has_something_in_it() {
    let mut window = window_with_two_tracks();
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    move_the_pointer_away(&mut window);
    assert_eq!(buttons_drawn(&window).len(), 1, "open");

    // (The timeline grew upward when the drawer pushed the row under it down: the button is where it is now.)
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    move_the_pointer_away(&mut window);
    assert!(buttons_drawn(&window).is_empty(), "shut");

    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::AddKeyframe {
        target: AnimationTarget::Track(track),
        property: PropertyId::Gain,
        frame: TimelineFrameIndex::new(5),
    });
    move_the_pointer_away(&mut window);
    assert_eq!(buttons_drawn(&window).len(), 1, "a rack is in it");
}

#[test]
fn dragging_from_the_button_of_a_track_neither_moves_the_track_nor_makes_it_taller() {
    let mut window = window_with_two_tracks();
    let button = the_button_of_the_track(&window, 0).center();
    let order_before: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    let height_before = window.application().project().tracks()[0].height();

    window.drag(button, button + vec2(0.0, 60.0), PointerButton::Primary);

    let order_after: Vec<_> = window
        .application()
        .project()
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect();
    assert_eq!(order_after, order_before);
    assert_eq!(window.application().project().tracks()[0].height(), height_before);
}

#[test]
fn clicking_the_very_bottom_of_the_button_of_a_track_opens_the_drawer_and_does_not_start_to_resize_the_track() {
    let mut window = window_with_two_tracks();
    let button = the_button_of_the_track(&window, 0);

    window.click(Pos2::new(button.center().x, button.bottom() - 0.5));
    let_the_drawers_finish(&mut window);

    assert_eq!(drawers_drawn(&window).len(), 1, "the drawer is open");
}

// --- what it is and what it is not ---

#[test]
fn opening_a_drawer_does_not_select_the_clip_move_it_or_change_the_project() {
    let mut window = window_with_a_short_clip();
    window.application_mut().note_everything_is_saved();
    let project_before = window.application().project().clone();

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    assert_eq!(window.application().selected_clips(), vec![], "nothing was selected");
    assert_eq!(window.application().project(), &project_before);
    assert!(!window.application().has_unsaved_changes());
    assert!(!window.application().can_undo(), "it is not something to undo");
}

#[test]
fn dragging_from_the_button_does_not_drag_the_clip() {
    let mut window = window_with_a_short_clip();
    hover_over_the_clip(&mut window, 0);
    let button = the_button_of(&window, 0, 0).center();

    window.drag(button, button + vec2(80.0, 0.0), PointerButton::Primary);

    assert_eq!(window.clips().len(), 1);
    let placement = window.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap();
    assert_eq!(placement.first_timeline_frame.value(), 0, "the clip did not move");
}

#[test]
fn clicking_in_an_open_drawer_does_not_select_anything_or_let_go_of_what_is_selected() {
    let mut window = window_with_a_short_clip();
    window.click(Pos2::new(
        window.x_of_frame_on_the_strip(0, 30),
        window.strip_of_track(0).top() + 20.0,
    ));
    assert_eq!(window.application().selected_clips().len(), 1, "the clip is selected");
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_eq!(
        window.application().selected_clips().len(),
        1,
        "opening the drawer left it selected"
    );

    let in_the_drawer = Pos2::new(
        window.x_of_frame_on_the_strip(0, 30),
        window.strip_of_track(0).bottom() + EMPTY_DRAWER_HEIGHT / 2.0,
    );
    window.click(in_the_drawer);

    assert_eq!(
        window.application().selected_clips().len(),
        1,
        "still selected: the drawer took the click"
    );
}

#[test]
fn the_drawers_of_a_clip_that_is_deleted_and_of_a_track_are_forgotten_and_a_new_project_has_all_its_drawers_shut() {
    let mut window = window_with_a_short_clip();
    click_the_button(&mut window, 0);
    let track_button = the_button_of_the_track(&window, 0).center();
    window.click(track_button);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 2);

    window.application_mut().start_empty_project();
    window.settle();
    let_the_drawers_finish(&mut window);

    assert!(drawers_drawn(&window).is_empty());
    assert_eq!(window.application().project().tracks().len(), 1);
}

#[test]
fn the_window_is_drawn_again_while_a_drawer_slides_and_not_when_it_is_still() {
    let mut window = window_with_a_short_clip();
    hover_over_the_clip(&mut window, 0);
    let centre = the_button_of(&window, 0, 0).center();
    window.move_pointer_to(centre);
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);
    window.harness.step();
    assert!(window.repaint_is_wanted_at_once(), "it is sliding");
    let_the_drawers_finish(&mut window);
    window.move_pointer_to(Pos2::new(5.0, 5.0));
    window.harness.run_steps(3);
    assert!(!window.repaint_is_wanted_at_once(), "and then it is still");
}

#[test]
fn a_drawer_is_drawn_behind_its_clip_so_that_what_reaches_up_into_the_clip_is_not_seen() {
    let mut window = window_with_a_short_clip();
    hover_over_the_clip(&mut window, 0);
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    let shapes = window.shapes_drawn();
    let drawer = shapes
        .iter()
        .position(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND))
        .expect("the drawer is drawn");
    // (The video of the test window cannot be read, so the clip is drawn as a grey place for its pictures.)
    let clip = shapes
        .iter()
        .position(|shape| {
            matches!(shape, Shape::Mesh(mesh) if mesh.vertices.first().map(|vertex| vertex.color) == Some(theme::THUMBNAIL_PLACEHOLDER))
        })
        .expect("the clip is drawn");
    let button = shapes
        .iter()
        .position(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BUTTON_FILL || rectangle.fill == theme::DRAWER_BUTTON_FILL_HOVERED))
        .expect("the button is drawn");
    assert!(drawer < clip, "the drawer is drawn at {drawer} and the clip at {clip}");
    assert!(clip < button, "and the button over the clip, at {button}");
}

// --- a drawer that pushes the rows under it ---

#[test]
fn a_drawer_opened_by_clicking_its_button_pushes_the_row_under_it_down_by_as_far_as_it_hangs_and_the_row_comes_back_when_it_shuts()
 {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    assert_eq!(drawers_drawn(&window).len(), 1, "it is open");
    assert_close(
        distance_to_the_row_under(&window),
        distance_before + EMPTY_DRAWER_HEIGHT,
        "the row under it is as much further down as the drawer is tall",
    );
    let drawer = &drawers_drawn(&window)[0];
    assert!(
        drawer.rect.bottom() <= window.strip_of_track(1).top() + 0.6,
        "the drawer is between the strip it hangs from and the row under it: {:?} over {:?}",
        drawer.rect,
        window.strip_of_track(1)
    );

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_close(
        distance_to_the_row_under(&window),
        distance_before,
        "shut, it gives the room back",
    );
}

#[test]
fn the_row_under_goes_down_at_every_moment_as_a_drawer_slides_open_and_comes_back_as_it_slides_shut() {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);
    hover_over_the_clip(&mut window, 0);
    let centre = the_button_of(&window, 0, 0).center();
    window.move_pointer_to(centre);
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);

    let mut pushed_by = Vec::new();
    for _ in 0..20 {
        window.harness.step();
        pushed_by.push(distance_to_the_row_under(&window) - distance_before);
    }
    assert!(
        pushed_by.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
        "it only goes down: {pushed_by:?}"
    );
    assert!(
        pushed_by[0] > 0.0 && pushed_by[0] < EMPTY_DRAWER_HEIGHT,
        "it has begun but not finished: {pushed_by:?}"
    );
    assert_close(*pushed_by.last().unwrap(), EMPTY_DRAWER_HEIGHT, "and arrives");

    click_the_button(&mut window, 0);
    let mut shutting = Vec::new();
    for _ in 0..20 {
        window.harness.step();
        shutting.push(distance_to_the_row_under(&window) - distance_before);
    }
    assert!(
        shutting.windows(2).all(|pair| pair[1] <= pair[0] + 0.01),
        "it only comes back: {shutting:?}"
    );
    assert_close(*shutting.last().unwrap(), 0.0, "all the way");
}

#[test]
fn a_drawer_that_is_opened_to_show_a_keyframe_that_was_put_in_it_pushes_the_row_under_it_too() {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);

    put_a_rack_in_the_drawer_of_the_first_clip_of(&mut window, 0);
    let_the_drawers_finish(&mut window);

    assert_close(
        distance_to_the_row_under(&window),
        distance_before + EMPTY_DRAWER_HEIGHT + theme::RACK_HEIGHT,
        "as far as the drawer with a rack in it hangs",
    );
}

#[test]
fn the_drawer_of_a_track_pushes_the_rows_under_it_when_it_is_opened_with_a_click_and_lies_over_them_when_it_is_opened_with_shift()
 {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);
    let button = the_button_of_the_track(&window, 0).center();

    window.click(button);
    let_the_drawers_finish(&mut window);
    assert_close(
        distance_to_the_row_under(&window),
        distance_before + EMPTY_DRAWER_HEIGHT,
        "pushed by a click",
    );

    // Shut with a click, whichever way it opened; and opened again with Shift held, it lies over the row.
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    assert_close(distance_to_the_row_under(&window), distance_before, "shut");
    let button = the_button_of_the_track(&window, 0).center();
    window.click_with_shift(button);
    let_the_drawers_finish(&mut window);
    assert_close(
        distance_to_the_row_under(&window),
        distance_before,
        "not pushed when it is opened with Shift held",
    );
    assert_eq!(drawers_drawn(&window).len(), 1, "though it is open");
}

#[test]
fn a_drawer_that_was_opened_with_shift_held_is_shut_by_a_plain_click_and_one_that_pushed_by_a_click_with_shift_held() {
    let mut window = window_with_two_tracks();
    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 1);
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert!(drawers_drawn(&window).is_empty(), "shut by a click");

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 1);
    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert!(
        drawers_drawn(&window).is_empty(),
        "shift decides how a drawer opens, not whether it shuts"
    );
}

#[test]
fn a_drawer_of_the_track_that_pushes_makes_the_row_taller_by_the_drawers_of_its_clips_too_as_they_hang_over_the_rows_it_pushes()
 {
    let mut window = window_with_two_tracks();
    let distance_before = distance_to_the_row_under(&window);
    // The drawer of the clip, with Shift held: it lies over the rows under it. The drawer of the track, with a click: it hangs
    // from the end of the clip's, and pushes, so the room of the clip's drawer is in the row too.
    hover_over_the_clip(&mut window, 0);
    shift_click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_close(
        distance_to_the_row_under(&window),
        distance_before,
        "the clip's drawer alone pushes nothing",
    );
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);

    assert_close(
        distance_to_the_row_under(&window),
        distance_before + 2.0 * EMPTY_DRAWER_HEIGHT,
        "both drawers are in the row",
    );
}

// --- bringing a drawer that is opening into view ---

/// A window with several tracks, so that the rows scroll, the one at the back (which has the video) being the last, and the rows
/// scrolled down as far as they go.
fn window_with_many_tracks_scrolled_to_the_bottom() -> Window {
    let mut project = project_of(60);
    for _ in 0..6 {
        project.add_track_in_front();
    }
    let mut window = Window::showing(project);
    window.application_mut().timeline_view.rows_scroll_offset = 1.0e6;
    window.harness.run_steps(5);
    window
}

/// How far down the rows can be seen: where the view of them ends at the bottom.
fn the_bottom_of_what_can_be_seen_of_the_rows(window: &Window) -> f32 {
    window
        .application()
        .timeline_view
        .timeline_area_in_the_last_frame
        .expect("the timeline was drawn")
        .bottom()
}

/// Where the first drawer that was drawn is, and where what it is drawn in is cut off.
fn the_drawer_and_where_it_is_cut_off(window: &Window) -> (Rect, Rect) {
    window
        .clipped_shapes_drawn()
        .into_iter()
        .find_map(|(clip_rectangle, shape)| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND => {
                Some((rectangle.rect, clip_rectangle))
            }
            _ => None,
        })
        .expect("a drawer is drawn")
}

#[test]
fn the_rows_scroll_down_to_bring_a_drawer_into_view_when_it_is_opened_on_a_clip_in_the_bottom_track() {
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let last = window.application().project().tracks().len() - 1;
    let scroll_before = window.application().timeline_view.rows_scroll_offset;
    assert!(
        scroll_before > 0.0,
        "the setup: the rows are scrolled down, and there is more of them above"
    );

    hover_over_the_clip(&mut window, last);
    click_the_button(&mut window, last);
    let_the_drawers_finish(&mut window);

    let (drawer, cut_off_at) = the_drawer_and_where_it_is_cut_off(&window);
    assert!(
        cut_off_at.bottom() >= drawer.bottom() - 0.01,
        "all of the drawer is in view: it goes down to {} and is cut off at {}",
        drawer.bottom(),
        cut_off_at.bottom()
    );
    assert!(
        window.application().timeline_view.rows_scroll_offset > scroll_before,
        "the rows were scrolled down to bring it into view"
    );
}

#[test]
fn a_drawer_that_is_brought_into_view_with_rows_under_it_is_scrolled_to_with_some_room_under_it() {
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let tracks = window.application().project().tracks();
    let the_one_over_the_last = tracks.len() - 2;
    let track = tracks[the_one_over_the_last].id();
    // Scrolled up until the bottom of the strip of the track that is over the last is a little way above the bottom of what is
    // in view, and the drawer of that track, which hangs under the strip, goes beyond it: with the last track under it, the rows
    // can be scrolled as far as that takes.
    let bottom_of_the_view = the_bottom_of_what_can_be_seen_of_the_rows(&window);
    let bottom_of_the_strip = window.strip_of_track(the_one_over_the_last).bottom();
    window.application_mut().timeline_view.rows_scroll_offset -= bottom_of_the_view - 10.0 - bottom_of_the_strip;
    window.harness.run_steps(5);
    let scroll_before = window.application().timeline_view.rows_scroll_offset;

    window
        .application_mut()
        .timeline_view
        .open_the_drawer_of(AnimationTarget::Track(track));
    let_the_drawers_finish(&mut window);

    assert!(
        window.application().timeline_view.rows_scroll_offset > scroll_before + 10.0,
        "the drawer did not fit in what was in view, and the rows were scrolled down to it"
    );
    let (drawer, _) = the_drawer_and_where_it_is_cut_off(&window);
    assert_close(
        the_bottom_of_what_can_be_seen_of_the_rows(&window) - drawer.bottom(),
        theme::MARGIN_UNDER_A_DRAWER_BROUGHT_INTO_VIEW,
        "the room under the drawer, which is not at the very edge of what is in view",
    );
}

#[test]
fn a_drawer_that_lies_over_the_rows_is_brought_into_view_too_when_it_hangs_below_the_last_row() {
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let last = window.application().project().tracks().len() - 1;

    hover_over_the_clip(&mut window, last);
    shift_click_the_button(&mut window, last);
    let_the_drawers_finish(&mut window);

    let (drawer, cut_off_at) = the_drawer_and_where_it_is_cut_off(&window);
    assert!(
        cut_off_at.bottom() >= drawer.bottom() - 0.01,
        "it goes down to {} and is cut off at {}",
        drawer.bottom(),
        cut_off_at.bottom()
    );
}

#[test]
fn the_drawer_of_the_bottom_track_is_brought_into_view_as_well() {
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let last = window.application().project().tracks().len() - 1;
    let button = the_button_of_the_track(&window, last).center();

    window.click(button);
    let_the_drawers_finish(&mut window);

    let (drawer, cut_off_at) = the_drawer_and_where_it_is_cut_off(&window);
    assert!(
        cut_off_at.bottom() >= drawer.bottom() - 0.01,
        "it goes down to {} and is cut off at {}",
        drawer.bottom(),
        cut_off_at.bottom()
    );
}

#[test]
fn the_rows_scroll_down_as_the_drawer_slides_so_that_its_bottom_is_in_view_at_every_moment_it_can_be() {
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let last = window.application().project().tracks().len() - 1;
    hover_over_the_clip(&mut window, last);
    let centre = the_button_of(&window, last, 0).center();
    window.move_pointer_to(centre);
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);

    let mut offsets = Vec::new();
    for _ in 0..20 {
        window.harness.step();
        offsets.push(window.application().timeline_view.rows_scroll_offset);
    }

    // (Never back up, except by what the rows are held to by the end of them: a fraction of a point.)
    assert!(
        offsets.windows(2).all(|pair| pair[1] >= pair[0] - 0.5),
        "it only goes down: {offsets:?}"
    );
    assert!(
        offsets[0] < *offsets.last().unwrap(),
        "and it goes on while the drawer slides: {offsets:?}"
    );
}

#[test]
fn a_drawer_that_is_in_view_when_it_opens_does_not_scroll_the_rows_and_one_above_the_view_does_not_scroll_them_up() {
    // At the top: the drawer of the first track is in view.
    let mut project = project_of(60);
    for _ in 0..6 {
        project.add_track_in_front();
    }
    let mut window = Window::showing(project);
    let button = the_button_of_the_track(&window, 0).center();
    window.click(button);
    let_the_drawers_finish(&mut window);
    assert_eq!(
        window.application().timeline_view.rows_scroll_offset,
        0.0,
        "nothing was scrolled"
    );

    // Scrolled to the bottom: a drawer that is opened on a track that cannot be seen is not brought into view by going up.
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let scroll_before = window.application().timeline_view.rows_scroll_offset;
    let top_track = window.application().project().tracks()[0].id();
    window
        .application_mut()
        .timeline_view
        .open_the_drawer_of(AnimationTarget::Track(top_track));
    let_the_drawers_finish(&mut window);
    assert_eq!(
        window.application().timeline_view.rows_scroll_offset,
        scroll_before,
        "the rows were not scrolled up to it"
    );
}

#[test]
fn the_rows_are_left_alone_once_the_drawer_has_stopped_sliding_so_that_scrolling_back_up_stays_scrolled() {
    let mut window = window_with_many_tracks_scrolled_to_the_bottom();
    let last = window.application().project().tracks().len() - 1;
    hover_over_the_clip(&mut window, last);
    click_the_button(&mut window, last);
    let_the_drawers_finish(&mut window);

    window.application_mut().timeline_view.rows_scroll_offset = 0.0;
    window.harness.run_steps(20);

    assert_eq!(
        window.application().timeline_view.rows_scroll_offset,
        0.0,
        "scrolled up by hand, the rows are not taken down again to the drawer"
    );
}
