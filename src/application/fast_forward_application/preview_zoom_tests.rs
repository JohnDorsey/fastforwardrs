//! Tests of zooming the preview: the wheel, moving the picture about with the middle button and the wheel, the limits, the button that
//! makes the picture fit the panel again, and the mouse doing what it does to layers on a picture that is zoomed.

use egui::{Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::preview_panel::{FIT_BUTTON_LABEL, video_label};
use crate::application::selection::SelectionChange;
use crate::project::{CanvasArea, ClipId, TextObject, TimelineFrameIndex};

/// The picture of the project is 1280 by 720 pixels.
const PICTURE_WIDTH_IN_PIXELS: f32 = 1280.0;

/// How much the wheel is turned to zoom by the factor 2, in points: what `egui` makes of it (its speed is 1/200 for each point).
fn wheel_for_twice_as_big() -> f32 {
    200.0 * 2.0_f32.ln()
}

fn window_with_a_video() -> (Window, ClipId) {
    let project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    (Window::showing(project), video)
}

/// Where the picture is drawn, which is where the video is, as it fills all of it.
fn picture_rectangle(window: &Window, video: ClipId) -> Rect {
    window.rectangle_of(&video_label(video))
}

fn the_fit_button(window: &Window) -> Option<Rect> {
    window
        .harness
        .query_by_label(FIT_BUTTON_LABEL)
        .map(|button| button.rect())
}

fn turn_the_wheel(window: &mut Window, at: Pos2, up_by: f32) {
    window.scroll_the_wheel(at, vec2(0.0, up_by), Modifiers::NONE);
}

/// A point of the picture, as fractions of its width and height.
fn point_of_the_picture(picture: Rect, across: f32, down: f32) -> Pos2 {
    picture.left_top() + picture.size() * vec2(across, down)
}

fn fractions_of(picture: Rect, point: Pos2) -> (f32, f32) {
    (
        (point.x - picture.left()) / picture.width(),
        (point.y - picture.top()) / picture.height(),
    )
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} is not {expected}"
    );
}

// --- zooming ---

#[test]
fn the_picture_fits_the_panel_to_begin_with_and_there_is_no_button_to_make_it_fit() {
    let (window, video) = window_with_a_video();
    let picture = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();
    assert!(panel.contains_rect(picture.expand(-0.5)), "{picture:?} in {panel:?}");
    assert!(
        (picture.width() - (panel.width() - 16.0)).abs() < 1.5
            || (picture.height() - (panel.height() - 16.0)).abs() < 1.5,
        "it is as big as it can be in the panel, with a margin: {picture:?} in {panel:?}"
    );
    assert_eq!(the_fit_button(&window), None);
}

#[test]
fn the_wheel_over_the_preview_zooms_in_about_the_pointer_which_stays_over_the_same_place_in_the_picture() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let pointer = point_of_the_picture(before, 0.3, 0.45);

    turn_the_wheel(&mut window, pointer, wheel_for_twice_as_big());

    let after = picture_rectangle(&window, video);
    assert_close(after.width(), before.width() * 2.0, 1.0, "twice as wide");
    assert_close(after.height(), before.height() * 2.0, 1.0, "twice as high");
    let (across, down) = fractions_of(after, pointer);
    assert_close(across, 0.3, 0.02, "the place under the pointer, across");
    assert_close(down, 0.45, 0.02, "the place under the pointer, down");
}

#[test]
fn zooming_in_and_out_again_by_as_much_brings_the_picture_back_to_where_it_was_and_the_button_goes() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let pointer = point_of_the_picture(before, 0.6, 0.5);

    turn_the_wheel(&mut window, pointer, wheel_for_twice_as_big());
    assert!(the_fit_button(&window).is_some());
    turn_the_wheel(&mut window, pointer, -wheel_for_twice_as_big());

    let after = picture_rectangle(&window, video);
    assert_close(after.left(), before.left(), 0.6, "left");
    assert_close(after.top(), before.top(), 0.6, "top");
    assert_close(after.width(), before.width(), 0.6, "width");
    assert_eq!(the_fit_button(&window), None, "it fits again");
}

#[test]
fn the_wheel_zooms_out_as_far_as_half_the_size_the_picture_has_when_it_fits_and_no_further() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();

    turn_the_wheel(&mut window, before.center(), -wheel_for_twice_as_big() * 3.0);

    let after = picture_rectangle(&window, video);
    assert_close(after.width(), before.width() / 2.0, 1.0, "half as wide");
    assert_close(after.height(), before.height() / 2.0, 1.0, "half as high");
    assert_close(
        after.center().x,
        panel.center().x,
        1.0,
        "in the middle of the panel, across",
    );
    assert_close(
        after.center().y,
        panel.center().y,
        1.0,
        "in the middle of the panel, down",
    );
    assert!(the_fit_button(&window).is_some(), "it does not fit now");
}

#[test]
fn the_wheel_zooms_in_as_far_as_every_pixel_of_the_picture_can_be_seen_and_no_further() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);

    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big() * 8.0);

    let after = picture_rectangle(&window, video);
    assert_close(
        after.width() / PICTURE_WIDTH_IN_PIXELS,
        8.0,
        0.05,
        "points to a pixel of the picture",
    );
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big());
    assert_close(
        picture_rectangle(&window, video).width(),
        after.width(),
        0.5,
        "turning it more changes nothing",
    );
}

#[test]
fn the_wheel_with_control_held_zooms_too_as_it_does_on_the_timeline() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);

    window.scroll_the_wheel(before.center(), vec2(0.0, 80.0), Modifiers::COMMAND);

    assert!(picture_rectangle(&window, video).width() > before.width() * 1.3);
}

#[test]
fn the_wheel_over_another_panel_does_not_zoom_the_preview() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let on_the_timeline = window.the_timeline_panel().center();

    turn_the_wheel(&mut window, on_the_timeline, 200.0);
    window.scroll_the_wheel(on_the_timeline, vec2(0.0, 80.0), Modifiers::COMMAND);

    assert_eq!(picture_rectangle(&window, video), before);
}

#[test]
fn a_new_project_has_the_picture_fitting_whatever_the_old_one_was_zoomed_to() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big());
    assert!(the_fit_button(&window).is_some());

    window.show_another_project(project_of(300));

    assert_eq!(the_fit_button(&window), None);
    let after = picture_rectangle(&window, window.application().project().tracks()[0].entries()[0].id());
    assert_close(after.width(), before.width(), 0.5, "as wide as it fits");
}

// --- the button ---

#[test]
fn the_button_that_makes_the_picture_fit_is_in_the_bottom_right_corner_of_the_preview_when_the_picture_does_not() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();

    for wheel in [wheel_for_twice_as_big(), -wheel_for_twice_as_big() * 3.0] {
        turn_the_wheel(&mut window, before.center(), wheel);
        let button = the_fit_button(&window).expect("the picture does not fit");
        assert!(panel.contains_rect(button), "{button:?} is in {panel:?}");
        assert!(
            panel.right() - button.right() < 20.0 && panel.bottom() - button.bottom() < 20.0,
            "in the bottom right corner: {button:?} of {panel:?}"
        );
        assert!(
            button.center().x > panel.center().x && button.center().y > panel.center().y,
            "the right and the lower half"
        );
        // Put it back before the next.
        window.click(button.center());
        assert_eq!(the_fit_button(&window), None);
    }
}

#[test]
fn pressing_the_button_makes_the_picture_fit_the_panel_again_whatever_zoom_it_has_and_wherever_it_has_been_moved_to() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();
    turn_the_wheel(
        &mut window,
        point_of_the_picture(before, 0.8, 0.8),
        wheel_for_twice_as_big() * 2.0,
    );
    window.drag(
        panel.center(),
        panel.center() - vec2(120.0, 80.0),
        PointerButton::Middle,
    );
    assert!(picture_rectangle(&window, video).width() > before.width() * 3.0);

    let button = the_fit_button(&window).expect("a button").center();
    window.click(button);

    let after = picture_rectangle(&window, video);
    assert_close(after.left(), before.left(), 0.6, "left");
    assert_close(after.top(), before.top(), 0.6, "top");
    assert_close(after.width(), before.width(), 0.6, "width");
    assert_eq!(the_fit_button(&window), None);
    assert!(
        !window.application().has_unsaved_changes(),
        "how the picture is seen is not an edit of the project"
    );
}

#[test]
fn pressing_the_button_does_not_press_what_is_under_it_which_would_be_picking_or_dragging_a_layer() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big() * 2.0);
    let selected_before = window.application().selected_clips();

    let button = the_fit_button(&window).expect("a button").center();
    window.click(button);

    assert_eq!(
        window.application().selected_clips(),
        selected_before,
        "the video under it was not picked"
    );
}

// --- moving the picture about ---

#[test]
fn the_middle_button_takes_hold_of_a_picture_that_is_zoomed_in_and_moves_it_with_the_pointer() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big());
    let zoomed = picture_rectangle(&window, video);

    window.drag(
        panel.center(),
        panel.center() + vec2(-70.0, 30.0),
        PointerButton::Middle,
    );

    let moved = picture_rectangle(&window, video);
    assert_close(moved.left() - zoomed.left(), -70.0, 1.5, "across");
    assert_close(moved.top() - zoomed.top(), 30.0, 1.5, "down");
    assert_close(moved.width(), zoomed.width(), 0.5, "it is the same size");
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn the_picture_cannot_be_moved_further_than_the_panel_is_covered_by_it_and_is_not_moved_at_all_when_it_fits() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();

    window.drag(
        panel.center(),
        panel.center() + vec2(100.0, 50.0),
        PointerButton::Middle,
    );
    assert_eq!(
        picture_rectangle(&window, video),
        before,
        "it fits: there is nothing to move"
    );

    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big());
    for _ in 0..4 {
        window.drag(
            panel.center() - vec2(200.0, 100.0),
            panel.center() + vec2(200.0, 100.0),
            PointerButton::Middle,
        );
    }
    let furthest = picture_rectangle(&window, video);
    assert_close(furthest.left(), panel.left(), 1.0, "its left edge is at the panel's");
    assert_close(furthest.top(), panel.top(), 1.0, "its top edge is at the panel's");
}

#[test]
fn the_wheel_with_shift_held_moves_the_picture_across_and_with_alt_held_up_and_down() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big() * 2.0);
    let zoomed = picture_rectangle(&window, video);

    window.scroll_the_wheel(zoomed.center(), vec2(0.0, 40.0), Modifiers::SHIFT);
    let across = picture_rectangle(&window, video);
    assert_close(across.width(), zoomed.width(), 0.5, "not zoomed");
    assert_close(across.top(), zoomed.top(), 0.5, "not moved up or down");
    assert!(
        (across.left() - zoomed.left()).abs() > 10.0,
        "moved across: {across:?} from {zoomed:?}"
    );

    window.scroll_the_wheel(across.center(), vec2(0.0, 40.0), Modifiers::ALT);
    let down = picture_rectangle(&window, video);
    assert_close(down.width(), across.width(), 0.5, "not zoomed");
    assert_close(down.left(), across.left(), 0.5, "not moved across");
    assert!(
        (down.top() - across.top()).abs() > 10.0,
        "moved up or down: {down:?} from {across:?}"
    );
}

// --- the mouse on layers that are on a picture that is zoomed ---

fn window_with_a_text_object_in_the_middle_selected() -> (Window, ClipId) {
    let mut project = project_of(300);
    let front = project.add_track_in_front();
    let in_the_middle = TextObject {
        area: CanvasArea::new(0.4, 0.4, 0.2, 0.2),
        ..TextObject::default()
    };
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), in_the_middle, 200)
        .unwrap();
    let mut window = Window::showing(project);
    window.do_this(Command::select_clips(vec![text], SelectionChange::Replace));
    (window, text)
}

fn area_of(window: &Window, clip: ClipId) -> CanvasArea {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .placement()
        .expect("a layer")
        .area
}

#[test]
fn a_layer_dragged_on_a_picture_that_is_zoomed_in_moves_by_what_the_pointer_goes_as_a_fraction_of_the_zoomed_picture() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let picture = window.rectangle_of(&crate::application::preview_panel::text_object_label(text));
    let middle = picture.center();

    // At first, as the picture fits.
    let start = area_of(&window, text);
    window.drag(middle, middle + vec2(60.0, 0.0), PointerButton::Primary);
    let moved_when_fitting = area_of(&window, text).left() - start.left();
    assert!(moved_when_fitting > 0.0);

    // After zooming in about the middle of the picture, the same distance of the pointer is half as much of the picture.
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let panel_middle = window.the_preview_panel().center();
    turn_the_wheel(&mut window, panel_middle, wheel_for_twice_as_big());
    let start = area_of(&window, text);
    let picture = window.rectangle_of(&crate::application::preview_panel::text_object_label(text));
    window.drag(
        picture.center(),
        picture.center() + vec2(60.0, 0.0),
        PointerButton::Primary,
    );
    let moved_when_zoomed = area_of(&window, text).left() - start.left();

    assert_close(
        moved_when_zoomed,
        moved_when_fitting / 2.0,
        moved_when_fitting * 0.05,
        "half as much of the picture, for the same distance of the pointer",
    );
}

#[test]
fn clicking_a_layer_on_a_picture_that_is_zoomed_in_picks_the_layer_that_is_where_the_pointer_is() {
    let (mut window, text) = window_with_a_text_object_in_the_middle_selected();
    let video = window.application().project().tracks()[1].entries()[0].id();
    window.do_this(Command::select_clips(vec![video], SelectionChange::Replace));
    let panel_middle = window.the_preview_panel().center();
    turn_the_wheel(&mut window, panel_middle, wheel_for_twice_as_big());
    let text_box = window.rectangle_of(&crate::application::preview_panel::text_object_label(text));

    window.click(text_box.center());

    assert_eq!(window.application().selected_clip(), Some(text));
    // And a click where the video is and the text is not picks the video.
    window.click(pos2(panel_middle.x - 200.0, panel_middle.y - 60.0));
    assert_eq!(window.application().selected_clip(), Some(video));
}

#[test]
fn a_picture_that_is_zoomed_in_and_moved_as_far_as_it_goes_still_covers_the_panel_when_the_window_gets_smaller() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big() * 2.0);
    for _ in 0..4 {
        window.drag(
            panel.center() - vec2(300.0, 150.0),
            panel.center() + vec2(300.0, 150.0),
            PointerButton::Middle,
        );
    }
    assert_close(
        picture_rectangle(&window, video).left(),
        panel.left(),
        1.0,
        "as far to the right as it goes",
    );

    window.harness.set_size(vec2(1000.0, 700.0));
    window.settle();

    let panel = window.the_preview_panel();
    let picture = picture_rectangle(&window, video);
    assert!(
        picture.expand(1.0).contains_rect(panel),
        "the picture {picture:?} covers the panel {panel:?}, which is smaller than it was"
    );
}

#[test]
fn a_drag_with_the_middle_button_that_began_where_the_picture_is_not_does_not_move_it() {
    let (mut window, video) = window_with_a_video();
    let before = picture_rectangle(&window, video);
    let panel = window.the_preview_panel();
    turn_the_wheel(&mut window, before.center(), wheel_for_twice_as_big());
    // A drag that does take hold of it first, so that whatever remembers that is set.
    window.drag(panel.center(), panel.center() + vec2(-20.0, 0.0), PointerButton::Middle);
    let zoomed = picture_rectangle(&window, video);

    // The next begins over the timeline and goes up over the picture.
    let on_the_timeline = window.the_timeline_panel().center();
    window.drag(on_the_timeline, panel.center(), PointerButton::Middle);

    assert_eq!(picture_rectangle(&window, video), zoomed);
}
