//! Tests of the drawers on clips: the button over the bottom edge of a clip, the drawer that slides open when it is
//! clicked, and how the rows make room for it.

use egui::{PointerButton, Pos2, Rect, Shape, epaint::RectShape, pos2, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::theme;
use crate::application::timeline_panel::drawer_button_label;
use crate::project::{TimelineFrameIndex, TrackHeight};

/// How tall a drawer is when it is open, which is what the tests of the rows making room look for.
const DRAWER_HEIGHT: f32 = 64.0;

/// A window with one video clip that is 2 seconds (60 frames) long, which is all in view.
fn window_with_a_short_clip() -> Window {
    Window::showing(project_of(60))
}

fn label_of_the_button_of(window: &Window, position: usize, clip_index: usize) -> String {
    drawer_button_label(&window.title_of_track(position), window.clip(position, clip_index))
}

/// Where the button of the drawer of the `clip_index`th clip of the track at `position` is, if it has one.
fn button_of(window: &Window, position: usize, clip_index: usize) -> Option<Rect> {
    window
        .harness
        .query_by_label(&label_of_the_button_of(window, position, clip_index))
        .map(|button| button.rect())
}

fn the_button_of(window: &Window, position: usize, clip_index: usize) -> Rect {
    button_of(window, position, clip_index).expect("the clip has a button for its drawer")
}

/// Where the clip that starts at `first_frame` and ends at `end_frame` on the track at `position` is on the screen.
fn clip_rectangle(window: &Window, position: usize, first_frame: u64, end_frame: u64) -> Rect {
    let strip = window.strip_of_track(position);
    Rect::from_min_max(
        pos2(window.x_of_frame_on_the_strip(position, first_frame), strip.top()),
        pos2(window.x_of_frame_on_the_strip(position, end_frame), strip.bottom()),
    )
}

/// Clicks the button of the drawer of the first clip of the track at `position`.
fn click_the_button(window: &mut Window, position: usize) {
    let centre = the_button_of(window, position, 0).center();
    window.click(centre);
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

// --- the button ---

#[test]
fn a_clip_has_a_wide_thin_button_over_its_bottom_edge_in_the_middle_of_it() {
    let window = window_with_a_short_clip();
    let clip = clip_rectangle(&window, 0, 0, 60);
    let button = the_button_of(&window, 0, 0);

    assert_close(
        button.width() / button.height(),
        5.0,
        "it is about 5 times as wide as it is tall",
    );
    assert_close(button.center().x, clip.center().x, "in the middle of the clip across");
    assert!(clip.contains_rect(button), "over the clip: {button:?} in {clip:?}");
    assert!(
        clip.bottom() - button.bottom() < 6.0,
        "at its bottom edge: {button:?} in {clip:?}"
    );
    assert!(button.height() < clip.height() / 4.0, "thin");
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

    let button = the_button_of(&window, 0, 0);

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

// --- opening and shutting ---

#[test]
fn clicking_the_button_slides_the_drawer_open_in_a_quick_animation_and_clicking_again_shuts_it() {
    let mut window = window_with_a_short_clip();
    assert!(
        drawers_drawn(&window).is_empty(),
        "shut to begin with: nothing is seen of it"
    );
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
    assert!(full > DRAWER_HEIGHT, "open: {heights:?}");
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
        clip.bottom() + DRAWER_HEIGHT,
        "it hangs down by its height",
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

/// A window with two tracks and the video's clip on the one in front, whose drawer the tests open: the row under
/// it is what makes room.
fn window_with_the_clip_on_the_front_track_of_two() -> Window {
    let mut window = window_with_a_short_clip();
    window.do_this(Command::AddTrack);
    let video_clip = window.clip(1, 0);
    let in_front = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveClips(vec![crate::project::ClipMove {
        clip: video_clip,
        to_track: in_front,
        start: TimelineFrameIndex::new(0),
    }]));
    assert_eq!(window.clips_of_track(0).len(), 1, "the clip is on the track in front");
    window
}

/// How far the top of the strip of the track under the first is from the top of the first one's.
fn distance_to_the_row_under(window: &Window) -> f32 {
    window.strip_of_track(1).top() - window.strip_of_track(0).top()
}

#[test]
fn the_row_under_a_drawer_that_opens_goes_further_off_by_the_height_of_the_drawer_and_comes_back_when_it_shuts() {
    let mut window = window_with_the_clip_on_the_front_track_of_two();
    let distance_before = distance_to_the_row_under(&window);
    let strip_height_before = window.strip_of_track(0).height();
    let title_before = window.title_box_of_track(0).top() - window.strip_of_track(0).top();

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);

    assert_close(
        distance_to_the_row_under(&window),
        distance_before + DRAWER_HEIGHT,
        "the row under it is further by the height of the drawer",
    );
    assert_close(
        window.strip_of_track(0).height(),
        strip_height_before,
        "the strip of the row that has the drawer is as tall as it was",
    );
    assert_close(
        window.title_box_of_track(0).top() - window.strip_of_track(0).top(),
        title_before,
        "and its title box is where it was beside it",
    );

    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_close(
        distance_to_the_row_under(&window),
        distance_before,
        "and it comes back when the drawer is shut",
    );
}

#[test]
fn the_rows_make_room_as_the_drawer_slides_and_not_all_at_once() {
    let mut window = window_with_the_clip_on_the_front_track_of_two();
    let distance_before = distance_to_the_row_under(&window);
    let centre = the_button_of(&window, 0, 0).center();
    window.move_pointer_to(centre);
    window.press(centre, PointerButton::Primary);
    window.release(centre, PointerButton::Primary);

    let mut distances = Vec::new();
    for _ in 0..30 {
        window.harness.step();
        distances.push(distance_to_the_row_under(&window));
    }

    assert!(
        distances.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
        "{distances:?}"
    );
    assert!(
        distances[0] < distance_before + DRAWER_HEIGHT / 2.0,
        "not at once: {distances:?}"
    );
    assert_close(
        *distances.last().unwrap(),
        distance_before + DRAWER_HEIGHT,
        "all the way",
    );
}

#[test]
fn each_clip_has_a_drawer_of_its_own_and_the_row_is_as_tall_as_the_tallest_that_is_open() {
    let mut window = window_with_a_short_clip();
    window.cut_at_frame(30);
    window.do_this(Command::AddTrack);
    let first_clip = window.clip(1, 0);
    let second_clip = window.clip(1, 1);
    let in_front = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveClips(vec![
        crate::project::ClipMove {
            clip: first_clip,
            to_track: in_front,
            start: TimelineFrameIndex::new(0),
        },
        crate::project::ClipMove {
            clip: second_clip,
            to_track: in_front,
            start: TimelineFrameIndex::new(30),
        },
    ]));
    let distance_before = distance_to_the_row_under(&window);

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
    assert_close(
        distance_to_the_row_under(&window),
        distance_before + DRAWER_HEIGHT,
        "and the row has made room for it",
    );

    // Open the first one too: the row is no taller.
    let centre = the_button_of(&window, 0, 0).center();
    window.click(centre);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 2);
    assert_close(
        distance_to_the_row_under(&window),
        distance_before + DRAWER_HEIGHT,
        "as tall as the tallest, not both",
    );
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
        window.strip_of_track(0).bottom() + DRAWER_HEIGHT / 2.0,
    );
    window.click(in_the_drawer);

    assert_eq!(
        window.application().selected_clips().len(),
        1,
        "still selected: the drawer took the click"
    );
}

#[test]
fn the_drawer_of_a_clip_that_is_deleted_is_forgotten_and_a_new_project_has_all_its_drawers_shut() {
    let mut window = window_with_a_short_clip();
    click_the_button(&mut window, 0);
    let_the_drawers_finish(&mut window);
    assert_eq!(drawers_drawn(&window).len(), 1);

    window.application_mut().start_empty_project();
    window.settle();
    let_the_drawers_finish(&mut window);

    assert!(drawers_drawn(&window).is_empty());
    assert_eq!(window.application().project().tracks().len(), 1);
}

#[test]
fn the_window_is_drawn_again_while_a_drawer_slides_and_not_when_it_is_still() {
    let mut window = window_with_a_short_clip();
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
        .position(|shape| matches!(shape, Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BUTTON_FILL))
        .expect("the button is drawn");
    assert!(drawer < clip, "the drawer is drawn at {drawer} and the clip at {clip}");
    assert!(clip < button, "and the button over the clip, at {button}");
}
