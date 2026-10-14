//! Tests of the row at the top of the timeline panel: the buttons, the time, and the slider that fills the rest of the row.

use egui::Rect;
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::project::{FrameRate, TimelineFrameIndex, TrackHeight, format_timecode};

const SLIDER: &str = "Position in the video";

/// How far from the middle of the row the middle of something on it may be, in points (they are all centred on the row, and the
/// edges of the panel are put on whole points).
const THE_ROUNDING_OF_THE_ROW: f32 = 0.55;

/// How far inside the edges of the timeline panel (at the sides) what it holds begins and ends, in points.
const THE_ROOM_INSIDE_THE_EDGES_OF_THE_PANEL: f32 = 8.0;

/// How much room there is between the last row and the bottom of the timeline panel at the most, in points: the 6 of the panel's own
/// margin and the 8 that are left under the last row for the boundary under it to be taken hold of. (The slider that used to be there
/// took 32 and more.)
const THE_MOST_ROOM_UNDER_THE_LAST_ROW: f32 = 15.0;

fn the_timecode_shown(window: &Window) -> Rect {
    let shown = format_timecode(
        TimelineFrameIndex::new(window.current_frame()),
        FrameRate::from_whole_frames_per_second(30).unwrap(),
    );
    window.rectangle_of(&shown)
}

#[test]
fn the_slider_is_on_the_line_of_the_play_buttons_and_the_time_and_is_right_of_all_of_them() {
    let window = Window::showing(project_of(900));
    let slider = window.rectangle_of(SLIDER);
    let time = the_timecode_shown(&window);

    for name in ["Stop", "Play", "Pause"] {
        let button = window.rectangle_of(name);
        assert!(
            (button.center().y - slider.center().y).abs() <= THE_ROUNDING_OF_THE_ROW,
            "the {name} button {button:?} is on the line of the slider {slider:?}"
        );
        assert!(
            button.right() <= slider.left(),
            "the {name} button is left of the slider"
        );
    }
    assert!(
        (time.center().y - slider.center().y).abs() <= THE_ROUNDING_OF_THE_ROW,
        "the time {time:?} is on the line of the slider {slider:?}"
    );
    assert!(
        time.right() < slider.left(),
        "the time ends at {} and the slider begins at {}",
        time.right(),
        slider.left()
    );
}

#[test]
fn the_slider_fills_the_row_to_the_right_edge_of_the_timeline_panel() {
    let window = Window::showing(project_of(900));
    let slider = window.rectangle_of(SLIDER);
    let panel = window.the_timeline_panel();

    assert_eq!(
        panel.right() - slider.right(),
        THE_ROOM_INSIDE_THE_EDGES_OF_THE_PANEL,
        "only the room inside the edge of the panel is right of the slider: {panel:?} and {slider:?}"
    );
    assert!(
        slider.width() > 600.0,
        "it is wide, in a window this big: {}",
        slider.width()
    );
}

#[test]
fn the_slider_is_over_the_timeline_and_the_rows_are_the_last_thing_in_the_panel() {
    let window = Window::showing(project_of(900));
    let slider = window.rectangle_of(SLIDER);
    let ruler = window.rectangle_of("Ruler");
    let panel = window.the_timeline_panel();

    assert!(
        slider.bottom() <= ruler.top(),
        "the slider {slider:?} is above the ruler {ruler:?}"
    );
    let last_strip = window.strip_of_track(0);
    assert!(
        panel.bottom() - last_strip.bottom() < THE_MOST_ROOM_UNDER_THE_LAST_ROW,
        "under the last row there is only the room inside the edge of the panel and the little there is for the boundary under \
         the row to be taken hold of: {panel:?}, {last_strip:?}"
    );
}

#[test]
fn every_row_that_fits_is_shown_whole_though_nothing_is_under_the_rows_to_make_the_panel_as_tall_as_they_are() {
    // (The rows are in an area that is as tall as they are, and so is the panel, which is as tall as what was in it in the frame
    // before: an area that could only be as tall as the panel was in the frame before stayed as short as the first frame made it
    // once the slider that used to be under the rows, and made the panel grow in each frame, was not there.)
    let mut window = Window::showing(project_with_a_video_and_a_sound(60, 2));
    window.do_this(Command::AddTrack);
    assert_eq!(window.application().project().tracks().len(), 3);

    let bottom_of_the_view = window.the_bottom_of_what_can_be_seen_of_the_rows();
    for position in 0..3 {
        let strip = window.strip_of_track(position);
        assert_eq!(
            strip.height(),
            TrackHeight::DEFAULT_IN_POINTS,
            "the strip of track {position} is as tall as a track is"
        );
        assert!(
            strip.bottom() <= bottom_of_the_view,
            "the strip of track {position} {strip:?} is in the view, which ends at {bottom_of_the_view}"
        );
    }
}

#[test]
fn what_can_be_seen_of_the_rows_ends_just_under_the_last_row_when_there_are_fewer_rows_than_the_room_has_for() {
    // (The room for the rows is as tall as three rows are: with two, the part of it that can be seen is as tall as they are, and not as the
    // room, which would be a stretch of the window under the panel that the pointer would count as over the timeline.)
    let window = Window::showing(project_with_a_video_and_a_sound(60, 2));
    assert_eq!(window.application().project().tracks().len(), 2);

    let last_row = window.strip_of_track(1);
    let bottom_of_the_view = window.the_bottom_of_what_can_be_seen_of_the_rows();

    assert!(
        bottom_of_the_view >= last_row.bottom(),
        "{bottom_of_the_view} and {last_row:?}"
    );
    assert!(
        bottom_of_the_view - last_row.bottom() <= THE_MOST_ROOM_UNDER_THE_LAST_ROW,
        "{bottom_of_the_view} and {last_row:?}"
    );
}

#[test]
fn the_slider_still_goes_from_the_first_frame_at_its_left_end_to_the_last_at_its_right_end() {
    let mut window = Window::showing(project_of(901));
    let slider = window.rectangle_of(SLIDER);

    window.click(egui::Pos2::new(slider.right() - 9.0, slider.center().y));
    assert_eq!(window.current_frame(), 900);
    window.click(egui::Pos2::new(slider.left() + 9.0, slider.center().y));
    assert_eq!(window.current_frame(), 0);
}

/// How far along the slider the knob is, as a fraction (0 at its left end and 1 at its right end), as a screen reader is told.
fn how_far_along_the_slider_the_knob_is(window: &Window) -> f64 {
    window
        .harness
        .get_by_label(SLIDER)
        .accesskit_node()
        .numeric_value()
        .expect("a slider has a value")
}

#[test]
fn the_knob_of_the_slider_is_at_its_left_end_at_the_first_frame_and_at_its_right_end_at_the_last() {
    let mut window = Window::showing(project_of(901));
    assert_eq!(how_far_along_the_slider_the_knob_is(&window), 0.0, "at the first frame");

    window.move_current_frame_to(450);
    assert_eq!(how_far_along_the_slider_the_knob_is(&window), 0.5, "half way");
    window.move_current_frame_to(900);
    assert_eq!(
        how_far_along_the_slider_the_knob_is(&window),
        1.0,
        "at the last frame, which is the 901st"
    );
}

#[test]
fn the_slider_is_as_tall_as_its_knob_is_round_so_that_the_knob_is_not_cut() {
    let window = Window::showing(project_of(900));
    let slider = window.rectangle_of(SLIDER);

    assert!(
        slider.height() >= 18.0,
        "the knob is 9 points in radius and is in the middle of the slider: {slider:?}"
    );
}

#[test]
fn the_slider_does_not_move_while_the_time_changes() {
    // The time is as wide as it ever is (hours, minutes, seconds and frames have all the digits they get), so the slider that is
    // right of it does not jump about as the playhead moves.
    let mut window = Window::showing(project_of(9_000));
    let before = window.rectangle_of(SLIDER);

    for frame in [1, 29, 30, 599, 600, 8_999] {
        window.move_current_frame_to(frame);
        assert_eq!(window.rectangle_of(SLIDER), before, "at frame {frame}");
    }
}

#[test]
fn a_slider_that_has_no_video_to_go_through_is_there_and_does_nothing() {
    let mut window = Window::showing(crate::project::Project::empty());
    let slider = window.rectangle_of(SLIDER);
    assert!(slider.width() > 600.0, "{slider:?}");

    window.click(slider.center());

    assert_eq!(window.current_frame(), 0);
}
