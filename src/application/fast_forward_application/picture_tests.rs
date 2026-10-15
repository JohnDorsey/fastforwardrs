// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of pictures (image files) on the timeline and the preview: how they are put on a track, how
//! the clip is drawn and its box, and picking, resizing and turning one on the preview.

use std::path::PathBuf;

use egui::{PointerButton, Pos2, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use super::{SourceFileState, SourceStatus};
use crate::application::commands::Command;
use crate::application::preview_panel::{corner_handle_label, picture_label};
use crate::application::selection::SelectionChange;
use crate::project::{
    Angle, ClipId, ClipKind, Corner, Placement, SourceImageProperties, SourceMedia, SourceMediaIdentifier,
    SourceMediaLink, TimelineFrameIndex,
};

const PICTURE_FILE: SourceMediaIdentifier = SourceMediaIdentifier::new(3);

/// A picture file twice as wide as it is tall.
pub(super) fn picture_source() -> SourceMedia {
    SourceMedia::image(
        PICTURE_FILE,
        SourceMediaLink::from_absolute_path(PathBuf::from("logo.png")),
        SourceImageProperties {
            display_width_in_pixels: 800,
            display_height_in_pixels: 400,
        },
    )
}

/// A window with a video of 300 frames and, on a track in front of it, a picture shown for 5 seconds
/// from frame 0, which is selected.
fn window_with_a_picture_selected() -> (Window, ClipId) {
    let mut project = project_of(300);
    let outcome = project
        .import_source(picture_source(), TimelineFrameIndex::new(0))
        .unwrap();
    assert!(outcome.created_track, "the video is on the only track");
    let picture = outcome.clip.expect("a picture makes a clip");
    let mut window = Window::showing(project);
    window.do_this(Command::select_clips(vec![picture], SelectionChange::Replace));
    (window, picture)
}

fn placement_of(window: &Window, clip: ClipId) -> Option<Placement> {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .placement()
}

fn corner_position(window: &Window, corner: Corner) -> Pos2 {
    window.rectangle_of(&corner_handle_label(corner)).center()
}

#[test]
fn a_picture_is_a_clip_of_its_own_on_a_track_in_front_shown_for_five_seconds() {
    let (window, picture) = window_with_a_picture_selected();
    let project = window.application().project();
    assert_eq!(
        project.tracks()[0].id(),
        project.track_holding(picture).unwrap().id(),
        "in front"
    );
    let placement = project.placement_of(picture).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Image);
    assert_eq!(placement.timeline_frame_count(), 150);
    assert_eq!(
        placement.clip.placement(),
        None,
        "fitted inside the picture until it is moved"
    );
}

#[test]
fn a_picture_is_on_the_preview_fitted_inside_the_picture_with_a_handle_at_each_corner_when_it_is_selected() {
    let (window, picture) = window_with_a_picture_selected();
    let body = window.rectangle_of(&picture_label(picture));
    // Twice as wide as tall, in a picture that is 16 to 9: as wide as the picture, and 8/9 as tall.
    assert!(
        (body.width() / body.height() - 2.0).abs() < 0.02,
        "the shape of the picture file: {body:?}"
    );
    for corner in Corner::ALL {
        assert!(
            window.harness.query_by_label(&corner_handle_label(corner)).is_some(),
            "{corner:?}"
        );
    }
}

#[test]
fn clicking_a_picture_on_the_preview_selects_its_clip_when_nothing_is() {
    let (mut window, picture) = window_with_a_picture_selected();
    window.do_this(Command::Deselect);
    let body = window.rectangle_of(&picture_label(picture));

    window.click(body.center());

    assert_eq!(window.application().selected_clip(), Some(picture));
}

#[test]
fn a_corner_of_a_picture_that_is_dragged_scales_it_and_it_keeps_the_shape_of_the_file() {
    let (mut window, picture) = window_with_a_picture_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let top_left = corner_position(&window, Corner::TopLeft);

    window.drag(
        corner,
        corner + (top_left - corner) * 0.5 + vec2(0.0, 30.0),
        PointerButton::Primary,
    );

    let after = placement_of(&window, picture).expect("it was put somewhere by that");
    assert!(after.area.width() < 0.7, "smaller: {after:?}");
    let shape = after.area.width() * 1280.0 / (after.area.height() * 720.0);
    assert!((shape - 2.0).abs() < 0.03, "still twice as wide as it is tall: {shape}");
    assert_eq!(after.rotation, Angle::ZERO);
}

#[test]
fn a_picture_that_was_made_smaller_can_be_moved_and_turned_from_outside_a_corner() {
    let (mut window, picture) = window_with_a_picture_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let top_left = corner_position(&window, Corner::TopLeft);
    window.drag(corner, corner + (top_left - corner) * 0.5, PointerButton::Primary);
    let before = placement_of(&window, picture).unwrap();

    let middle = corner_position(&window, Corner::TopLeft)
        + (corner_position(&window, Corner::BottomRight) - corner_position(&window, Corner::TopLeft)) * 0.5;
    window.drag(middle, middle + vec2(50.0, 20.0), PointerButton::Primary);
    let moved = placement_of(&window, picture).unwrap();
    assert!(
        moved.area.left() > before.area.left() + 0.02,
        "{before:?} then {moved:?}"
    );

    let corner = corner_position(&window, Corner::TopRight);
    let middle = corner_position(&window, Corner::TopLeft)
        + (corner_position(&window, Corner::BottomRight) - corner_position(&window, Corner::TopLeft)) * 0.5;
    let start = corner + (corner - middle).normalized() * 18.0;
    let (sine, cosine) = 35.0f32.to_radians().sin_cos();
    let offset = start - middle;
    let end = middle + vec2(offset.x * cosine - offset.y * sine, offset.x * sine + offset.y * cosine);
    window.drag(start, end, PointerButton::Primary);
    let turned = placement_of(&window, picture).unwrap();
    assert!((turned.rotation.degrees() - 35.0).abs() < 0.5, "{turned:?}");
}

#[test]
fn selecting_a_picture_clip_shows_in_the_panel_for_how_long_it_is_shown_and_not_speed_or_sound() {
    let (mut window, picture) = window_with_a_picture_selected();
    window.do_this(Command::Deselect);

    window.click(window.middle_of_the_strip_at(0, 75));

    assert_eq!(window.application().selected_clip(), Some(picture));
    assert!(window.harness.query_by_label("Picture clip").is_some());
    assert!(window.harness.query_by_label("Shown for").is_some());
    for label in ["Speed", "Gain", "Mute"] {
        assert!(
            window.harness.query_by_label(label).is_none(),
            "a picture has no {label}"
        );
    }
    assert!(
        window.harness.query_by_label("logo.png").is_some(),
        "the box says which file the clip is from"
    );
}

#[test]
fn changing_for_how_long_a_picture_is_shown_changes_the_clip_and_one_undo_takes_it_back() {
    let (mut window, picture) = window_with_a_picture_selected();
    window.click(window.middle_of_the_strip_at(0, 75));

    window.set_slider("Shown for", 2.0);

    assert_eq!(
        window
            .application()
            .project()
            .placement_of(picture)
            .unwrap()
            .timeline_frame_count(),
        60,
        "2 seconds at 30 frames per second"
    );
    window.set_slider("Shown for", 0.0);
    assert_eq!(
        window
            .application()
            .project()
            .placement_of(picture)
            .unwrap()
            .timeline_frame_count(),
        1,
        "at least a frame"
    );
}

#[test]
fn a_picture_clip_can_be_cut_at_the_playhead_and_the_halves_are_pictures_in_the_same_place() {
    let (mut window, picture) = window_with_a_picture_selected();
    window.cut_at_frame(60);
    let track = window.application().project().track_holding(picture).unwrap();
    let durations: Vec<u64> = track
        .placements()
        .map(|placement| placement.timeline_frame_count())
        .collect();
    assert_eq!(durations, vec![60, 90]);
    assert!(
        track
            .entries()
            .iter()
            .all(|entry| entry.clip().kind() == ClipKind::Image)
    );
}

#[test]
fn a_picture_whose_file_cannot_be_found_is_a_missing_clip_and_its_box_offers_to_choose_the_file_again() {
    let (mut window, _picture) = window_with_a_picture_selected();
    window.application_mut().source_files.insert(
        PICTURE_FILE,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "there is no file there".to_owned(),
        }),
    );
    window.settle();
    window.right_click(window.middle_of_the_strip_at(0, 75));
    assert!(window.harness.query_by_label("Choose the file again").is_some());
    assert!(
        window.harness.query_by_label("Shown for").is_some(),
        "how long it is shown for can still be changed"
    );
}

#[test]
fn a_project_with_a_picture_is_saved_and_comes_back_with_it_where_it_was_put() {
    let (mut window, picture) = window_with_a_picture_selected();
    let corner = corner_position(&window, Corner::BottomRight);
    let top_left = corner_position(&window, Corner::TopLeft);
    window.drag(corner, corner + (top_left - corner) * 0.5, PointerButton::Primary);
    let placed = placement_of(&window, picture).unwrap();

    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("with_a_picture.fastforwardrs.json");
    window.application_mut().save_project_to(path.clone());
    let loaded = crate::project_file::read_project_file(&path).unwrap();

    let loaded_picture = loaded.tracks()[0].entries()[0]
        .clip()
        .as_image()
        .expect("a picture clip");
    assert_eq!(loaded_picture.source(), PICTURE_FILE);
    assert_eq!(loaded_picture.duration_in_frames(), 150);
    assert_eq!(loaded_picture.placement(), Some(placed));
}
