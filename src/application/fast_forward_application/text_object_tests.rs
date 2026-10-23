// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of text objects: making one from the menu that opens on an empty part of a track, the
//! properties panel it is edited in, and moving and resizing it on the preview.

use egui::{Key, Modifiers, vec2};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{VIDEO, Window, project_of, video_source};
use crate::application::commands::Command;
use crate::application::preview_panel::{corner_handle_label, text_object_label};
use crate::project::{
    CanvasArea, Clip, ClipId, Corner, MediaClip, Project, SourceFrameIndex, SpeedFactor, SpeedIndicatorLayout,
    TextObject, TimelineFrameIndex, Track, TrackId,
};

/// What the box of the properties of a text object is headed with.
const TEXT_BOX_HEADING: &str = "Text object";

/// Makes a text object the way a person does: right-clicks the empty part of the strip of the
/// track at `position`, where timeline frame `frame` is, and chooses New, then Text object.
fn make_a_text_object_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    window.harness.get_by_label("Text object").click();
    window.settle();
}

/// The text clip of the track at `position` that is its `index`th clip, with its text object.
fn text_clip(window: &Window, position: usize, index: usize) -> (ClipId, TextObject, u64) {
    let location = window.clip(position, index);
    let placement = window.application().project().placement_of(location).unwrap();
    let text = placement.clip.as_text().expect("a text clip");
    let object = window
        .application()
        .project()
        .text_object_of(location)
        .expect("a text clip has a text object");
    (location, object, text.duration_in_frames())
}

/// A window with a 90 frame video and, beyond its end, a text object made from the menu.
fn window_with_a_text_object() -> Window {
    let mut window = Window::showing(project_of(90));
    make_a_text_object_at(&mut window, 0, 100);
    window
}

// --- making one ---

#[test]
fn right_clicking_an_empty_part_of_a_track_offers_new_and_in_it_a_text_object() {
    let mut window = Window::showing(project_of(90));
    let empty_part = window.middle_of_the_strip_at(0, 100);
    window.right_click(empty_part);
    assert!(
        window.harness.query_by_label_contains("New").is_some(),
        "the menu has New"
    );
    assert!(
        window.harness.query_by_label("Text object").is_none(),
        "and the text object is inside it"
    );
    window.harness.get_by_label_contains("New").click();
    window.settle();
    assert!(window.harness.query_by_label("Text object").is_some());
    assert!(
        !window.application().has_unsaved_changes(),
        "opening menus changes nothing"
    );
}

#[test]
fn right_clicking_a_clip_does_not_offer_new() {
    let mut window = Window::showing(project_of(90));
    let clip = window.middle_of_the_strip_at(0, 45);
    window.right_click(clip);
    assert!(window.harness.query_by_label_contains("New").is_none());
    assert_eq!(
        window.application().selected_clip(),
        Some(window.clip(0, 0)),
        "it was selected instead"
    );
}

#[test]
fn choosing_text_object_puts_a_text_clip_where_the_menu_was_opened_and_selects_it_which_shows_its_properties() {
    let mut window = Window::showing(project_of(90));
    assert_eq!(window.current_frame(), 0);

    make_a_text_object_at(&mut window, 0, 100);

    // The video is the first clip of the track, and the text came after it. The frame is as far
    // along as the pointer was, give or take the width of a frame.
    let (location, object, duration) = text_clip(&window, 0, 1);
    let placement = window.application().project().placement_of(location).unwrap();
    assert!(
        placement.first_timeline_frame.value().abs_diff(100) <= 1,
        "{}",
        placement.first_timeline_frame
    );
    assert_eq!(duration, 90, "three seconds at 30 frames a second");
    assert_eq!(object, TextObject::default());
    assert_eq!(window.application().selected_clip(), Some(location));
    assert!(window.application().has_unsaved_changes());
    assert!(
        window.harness.query_by_label(TEXT_BOX_HEADING).is_some(),
        "the properties to write it in are in the panel"
    );
    assert_eq!(
        window.current_frame(),
        placement.first_timeline_frame.value(),
        "the playhead went to the text object, so that it shows on the preview"
    );
    assert_eq!(
        window.application().project().total_frame_count(),
        placement.first_timeline_frame.value() + 90,
        "the timeline now reaches to the end of it"
    );
}

#[test]
fn the_playhead_stays_where_it_is_when_the_new_text_object_is_there_already() {
    // The video is on the track behind, and the new text goes on the empty track in front of it,
    // over frames of the video that the playhead can be on.
    let mut project = project_of(300);
    project.add_track_in_front();
    let mut window = Window::showing(project);
    window.move_current_frame_to(130);

    make_a_text_object_at(&mut window, 0, 100);

    let (location, _, _) = text_clip(&window, 0, 0);
    let placement = window.application().project().placement_of(location).unwrap();
    assert!(placement.contains(TimelineFrameIndex::new(130)));
    assert_eq!(window.current_frame(), 130);
}

#[test]
fn a_text_object_is_only_as_long_as_there_is_room_for_before_the_next_clip() {
    // A video clip of 60 frames, then 40 frames with nothing on them, then another video clip.
    let video_clip = |first: u64, end: u64| {
        Clip::Video(
            MediaClip::new(
                VIDEO,
                SourceFrameIndex::new(first),
                SourceFrameIndex::new(end),
                SpeedFactor::NORMAL,
            )
            .unwrap(),
        )
    };
    let track = Track::new(TrackId::new(0), "track 0").with_clips([(0, video_clip(0, 60)), (40, video_clip(60, 120))]);
    let project = Project::from_parts(vec![video_source(300)], vec![track], SpeedIndicatorLayout::DEFAULT).unwrap();
    let mut window = Window::showing(project);

    make_a_text_object_at(&mut window, 0, 70);

    let (location, _, duration) = text_clip(&window, 0, 1);
    let placement = window.application().project().placement_of(location).unwrap();
    assert_eq!(
        placement.end_timeline_frame.value(),
        100,
        "it ends where the next clip begins: {duration} frames from {}",
        placement.first_timeline_frame
    );
    assert!((25..40).contains(&duration), "{duration}");
    assert_eq!(window.clips().len(), 2, "and the video clips did not move");
}

#[test]
fn a_text_object_can_be_made_in_a_project_with_no_video_at_all_and_the_preview_then_shows_a_picture() {
    let mut window = Window::showing(Project::empty());
    assert!(window.application().project().has_no_clips());
    make_a_text_object_at(&mut window, 0, 0);

    assert_eq!(window.application().project().tracks()[0].entries().len(), 1);
    assert_eq!(window.application().project().total_frame_count(), 90);
    assert_eq!(window.application().project().picture_size(), (1920, 1080));
    assert!(window.harness.query_by_label(TEXT_BOX_HEADING).is_some());
    // The text can be grabbed on the preview, so the picture is there.
    let (location, _, _) = text_clip(&window, 0, 0);
    assert!(window.harness.query_by_label(&text_object_label(location)).is_some());
}

// --- its properties ---

#[test]
fn the_panel_has_the_words_the_font_the_size_the_colour_and_how_long_the_text_is_shown() {
    let mut window = window_with_a_text_object();
    window.application_mut().environment.font_families = Some(vec!["Arial".to_owned(), "Courier New".to_owned()]);
    window.settle();
    for label in ["Text", "Font", "Size", "Colour", "Shown for"] {
        assert!(window.harness.query_by_label(label).is_some(), "{label}");
    }
    assert_eq!(window.text_field_showing("Text").value().as_deref(), Some("Text"));
    assert!(
        window.harness.query_by_value("Default font").is_some(),
        "the font that is chosen"
    );
}

/// (A planted mistake that swapped the rows of the font and the size was not noticed by any test, so this one says what order a person sees them in. They are the rows that any effect's values have since
/// phase 9, drawn in the box of the text and not in a box of the chain, so that they come where they always did.)
#[test]
fn the_rows_of_a_text_are_in_the_order_the_words_the_font_the_size_the_colour_and_then_how_long_it_is_shown() {
    let mut window = window_with_a_text_object();
    window.application_mut().environment.font_families = Some(vec!["Arial".to_owned()]);
    window.settle();
    let heights: Vec<f32> = ["Text", "Font", "Size", "Colour", "Shown for"]
        .into_iter()
        .map(|label| window.rectangle_of(label).top())
        .collect();
    assert!(
        heights.windows(2).all(|pair| pair[0] < pair[1]),
        "from the top to the bottom: the words, the font, the size, the colour, how long: {heights:?}"
    );
}

/// (Another planted mistake that no test noticed: a text clip on the timeline that shows nothing of what it says.)
#[test]
fn the_clip_of_a_text_on_the_timeline_says_the_first_line_of_what_the_text_says() {
    let mut window = window_with_a_text_object();
    window.replace_text_in_the_field_showing(
        "Text",
        "Chapter one
and a second line",
    );
    let written: Vec<String> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    // (The box of lines in the panel writes all of the words in one piece, so a piece that is just the first line is what the clip says.)
    assert!(
        written.iter().any(|words| words == "Chapter one"),
        "the first line of what it says is written on the clip: {written:?}"
    );
}

#[test]
fn typing_in_the_panel_changes_the_words_of_the_text_object_and_marks_the_project_changed() {
    let mut window = window_with_a_text_object();
    window.application_mut().note_everything_is_saved();

    window.replace_text_in_the_field_showing("Text", "Hello there");

    let (_, object, _) = text_clip(&window, 0, 1);
    assert_eq!(object.text, "Hello there");
    assert!(window.application().has_unsaved_changes());
    assert_eq!(
        object.font_size,
        TextObject::default().font_size,
        "and only the words changed"
    );
}

#[test]
fn the_keys_are_not_shortcuts_while_the_words_are_being_typed_in() {
    let mut window = window_with_a_text_object();
    window.replace_text_in_the_field_showing("Text", "s is for sound");
    let (_, object, _) = text_clip(&window, 0, 1);
    assert_eq!(
        object.text, "s is for sound",
        "the S and the spaces went into the words"
    );
    assert!(!window.application().is_playing());
    assert_eq!(window.clips(), vec![(0, 90, 1)], "and nothing was cut");
    for key in [Key::Space, Key::S, Key::Delete] {
        window.press_key(key);
    }
    window.press_key_with(Modifiers::SHIFT, Key::S);
    assert!(
        !window.application().is_playing(),
        "the keys are the text's while it is typed in"
    );
    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        2,
        "nothing was deleted"
    );
}

#[test]
fn choosing_a_font_in_the_panel_sets_the_font_of_the_text_object() {
    let mut window = window_with_a_text_object();
    window.application_mut().environment.font_families = Some(vec!["Arial".to_owned(), "Courier New".to_owned()]);
    window.settle();
    window.harness.get_by_value("Default font").click();
    window.settle();
    window.harness.get_by_label("Courier New").click();
    window.settle();
    let (_, object, _) = text_clip(&window, 0, 1);
    assert_eq!(object.font_family, "Courier New");
    window.harness.get_by_value("Courier New").click();
    window.settle();
    window.harness.get_by_label("Default font").click();
    window.settle();
    assert_eq!(
        text_clip(&window, 0, 1).1.font_family,
        "",
        "the default font is no font at all"
    );
}

#[test]
fn the_size_is_shown_in_points_on_a_picture_1080_tall_and_changing_it_changes_the_object() {
    let mut window = window_with_a_text_object();
    let default_points = TextObject::default().font_size.points();
    let shown = window
        .harness
        .get_by_label("Size")
        .accesskit_node()
        .numeric_value()
        .unwrap();
    assert!((shown - f64::from(default_points)).abs() < 0.1, "{shown} points");

    window.set_slider("Size", 120.0);

    let (_, object, _) = text_clip(&window, 0, 1);
    assert!(
        (object.font_size.points() - 120.0).abs() < 0.01,
        "{}",
        object.font_size.points()
    );
    assert!((object.font_size.fraction_of_picture_height() - 120.0 / 1080.0).abs() < 1e-5);
}

#[test]
fn the_time_the_text_is_shown_for_can_be_changed_in_seconds_and_stops_at_the_next_clip() {
    let mut window = window_with_a_text_object();
    window.set_slider("Shown for", 5.0);
    assert_eq!(text_clip(&window, 0, 1).2, 150, "five seconds at 30 frames a second");
    window.set_slider("Shown for", 0.0);
    assert_eq!(text_clip(&window, 0, 1).2, 1, "at least one frame");
    assert_eq!(window.application().project().total_frame_count(), 100 + 1);
}

#[test]
fn selecting_the_text_clip_again_brings_its_properties_back_and_a_text_has_no_speed_or_loudness() {
    let mut window = window_with_a_text_object();
    window.press_key_outside_the_text(Key::Escape);
    assert!(
        window.harness.query_by_label(TEXT_BOX_HEADING).is_none(),
        "nothing is selected"
    );

    let (location, _, _) = text_clip(&window, 0, 1);
    let on_the_clip = window.middle_of_the_strip_at(0, 120);
    window.right_click(on_the_clip);

    assert!(window.harness.query_by_label(TEXT_BOX_HEADING).is_some());
    assert_eq!(window.application().selected_clip(), Some(location));
    for label in ["Speed", "Gain", "Mute"] {
        assert!(window.harness.query_by_label(label).is_none(), "a text has no {label}");
    }
}

#[test]
fn the_properties_go_when_the_text_clip_is_deleted() {
    let mut window = window_with_a_text_object();
    let (location, _, _) = text_clip(&window, 0, 1);
    window.do_this(Command::DeleteClips(vec![location]));
    window.settle();
    assert!(window.harness.query_by_label(TEXT_BOX_HEADING).is_none());
    assert_eq!(window.application().selected_clip(), None);
}

// --- on the preview ---

fn area_of(window: &Window) -> CanvasArea {
    text_clip(window, 0, 1).1.area
}

#[test]
fn a_text_object_is_on_the_preview_where_the_current_frame_is_in_it_and_only_then() {
    let mut window = window_with_a_text_object();
    let (location, _, _) = text_clip(&window, 0, 1);
    let label = text_object_label(location);
    assert!(
        window.harness.query_by_label(&label).is_some(),
        "the playhead is in the text clip"
    );
    window.move_current_frame_to(10);
    assert!(window.harness.query_by_label(&label).is_none(), "not at frame 10");
}

#[test]
fn the_selected_text_object_has_a_handle_at_each_of_its_four_corners_and_others_have_none() {
    let mut window = window_with_a_text_object();
    for corner in Corner::ALL {
        assert!(
            window.harness.query_by_label(&corner_handle_label(corner)).is_some(),
            "{corner:?}"
        );
    }
    // Deselect from the Edit menu (the keys are not shortcuts while the words of the text are typed in).
    window.harness.get_by_label("Edit").click();
    window.settle();
    window.harness.get_by_label_contains("Deselect").click();
    window.settle();
    assert_eq!(window.application().selected_clip(), None);
    for corner in Corner::ALL {
        assert!(
            window.harness.query_by_label(&corner_handle_label(corner)).is_none(),
            "{corner:?}"
        );
    }
}

#[test]
fn dragging_any_corner_of_the_selected_text_object_resizes_it_from_that_corner() {
    for corner in Corner::ALL {
        let mut window = window_with_a_text_object();
        let before = area_of(&window);
        let handle = window.rectangle_of(&corner_handle_label(corner)).center();
        // Drag the corner towards the middle of the text's area, which makes it smaller.
        let towards_the_middle = window.rectangle_of(&corner_handle_label(corner.opposite())).center();
        let to = handle + (towards_the_middle - handle) * 0.3;

        window.drag(handle, to, egui::PointerButton::Primary);

        let after = area_of(&window);
        assert!(
            after.width() < before.width() - 0.02,
            "{corner:?}: {before:?} then {after:?}"
        );
        assert!(
            after.height() < before.height() - 0.02,
            "{corner:?}: {before:?} then {after:?}"
        );
        let (fixed_x, fixed_y) = before.corner_position(corner.opposite());
        let (still_x, still_y) = after.corner_position(corner.opposite());
        assert!(
            (fixed_x - still_x).abs() < 1e-3 && (fixed_y - still_y).abs() < 1e-3,
            "the opposite corner stays where it was: {before:?} then {after:?}"
        );
        assert!(window.application().has_unsaved_changes());
    }
}

#[test]
fn a_text_object_cannot_be_made_bigger_than_the_picture_or_so_big_that_its_middle_leaves_it_or_too_small() {
    let mut window = window_with_a_text_object();
    let handle = window.rectangle_of(&corner_handle_label(Corner::BottomRight)).center();
    window.drag(handle, handle + vec2(4000.0, 4000.0), egui::PointerButton::Primary);
    let area = area_of(&window);
    assert!(area.width() <= 1.0 + 1e-4 && area.height() <= 1.0 + 1e-4, "{area:?}");
    assert!(
        area.left() + area.width() / 2.0 <= 1.0 + 1e-4 && area.top() + area.height() / 2.0 <= 1.0 + 1e-4,
        "its middle is still on the picture: {area:?}"
    );
    assert!(
        area.right() > 1.0 && area.bottom() > 1.0,
        "the corner did go out past the edges: {area:?}"
    );

    let handle = window.rectangle_of(&corner_handle_label(Corner::BottomRight)).center();
    let top_left = window.rectangle_of(&corner_handle_label(Corner::TopLeft)).center();
    window.drag(handle, top_left - vec2(500.0, 500.0), egui::PointerButton::Primary);
    let area = area_of(&window);
    assert!(area.width() >= CanvasArea::SMALLEST_SIDE_FRACTION - 1e-4, "{area:?}");
    assert!(area.height() >= CanvasArea::SMALLEST_SIDE_FRACTION - 1e-4, "{area:?}");
}

#[test]
fn a_text_object_dragged_far_past_the_edge_stops_with_its_middle_at_the_edge_and_can_be_dragged_back() {
    let mut window = window_with_a_text_object();
    let (location, _, _) = text_clip(&window, 0, 1);
    let body = window.rectangle_of(&text_object_label(location));
    let before = area_of(&window);

    window.drag(
        body.center(),
        body.center() + vec2(4000.0, 0.0),
        egui::PointerButton::Primary,
    );

    let out = area_of(&window);
    assert!(
        (out.left() + out.width() / 2.0 - 1.0).abs() < 1e-3,
        "its middle is on the right edge of the picture: {out:?}"
    );
    assert!(out.right() > 1.0, "so that half of it is past the edge: {out:?}");
    assert_eq!(
        (out.width(), out.height(), out.top()),
        (before.width(), before.height(), before.top())
    );

    // It is selected, so the whole of its box can be taken hold of, and it comes back the way it went.
    let top_left = window.rectangle_of(&corner_handle_label(Corner::TopLeft)).center();
    let bottom_right = window.rectangle_of(&corner_handle_label(Corner::BottomRight)).center();
    let middle = top_left + (bottom_right - top_left) * 0.5;
    window.drag(middle, middle - vec2(120.0, 0.0), egui::PointerButton::Primary);
    let back = area_of(&window);
    assert!(back.left() < out.left() - 0.05, "{out:?} then {back:?}");
}

#[test]
fn dragging_the_text_object_itself_moves_it_without_resizing_it() {
    let mut window = window_with_a_text_object();
    let (location, _, _) = text_clip(&window, 0, 1);
    let body = window.rectangle_of(&text_object_label(location));
    let before = area_of(&window);

    window.drag(
        body.center(),
        body.center() + vec2(0.0, -60.0),
        egui::PointerButton::Primary,
    );

    let after = area_of(&window);
    assert!(after.top() < before.top() - 0.05, "{before:?} then {after:?}");
    assert!((after.width() - before.width()).abs() < 1e-4 && (after.height() - before.height()).abs() < 1e-4);
    assert!((after.left() - before.left()).abs() < 1e-3, "straight up");
}

#[test]
fn clicking_a_text_object_on_the_preview_selects_its_clip_and_its_properties_are_in_the_panel() {
    let mut window = window_with_a_text_object();
    let (location, _, _) = text_clip(&window, 0, 1);
    window.application_mut().interface.selection.clear();
    window.settle();
    assert!(window.harness.query_by_label(TEXT_BOX_HEADING).is_none());
    let body = window.rectangle_of(&text_object_label(location));

    window.click(body.center());

    assert_eq!(window.application().selected_clip(), Some(location));
    assert!(window.harness.query_by_label(TEXT_BOX_HEADING).is_some());
}

#[test]
fn a_text_object_made_and_saved_comes_back_as_it_was() {
    let mut window = window_with_a_text_object();
    window.replace_text_in_the_field_showing("Text", "Chapter one");
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("with_text.fastforwardrs.json");
    window.application_mut().save_project_to(path.clone());

    let loaded = crate::project_file::read_project_file(&path).unwrap();
    let track: &Track = &loaded.tracks()[0];
    let placement = track.placement_at_index(1).unwrap();
    let Clip::Text(text) = placement.clip else {
        panic!("a text clip")
    };
    assert_eq!(loaded.text_object_of(placement.id).unwrap().text, "Chapter one");
    assert_eq!(text.duration_in_frames(), 90);
}
