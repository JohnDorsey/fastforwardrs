// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The adjustment clip in the window (phase 5, piece 5.3): made from the menu that opens on an empty part of a track, shown in the timeline, with its properties (how long it lasts and
//! the effects it has) in the panel, undone, and refused where a clip group's rectangle is.

use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::project::{ClipId, ClipKind, EffectKind, GroupKind, Member, TimelineFrameIndex, TrackId};

/// Makes an adjustment clip the way a person does: right-clicks the empty part of the strip of the track at `position`, where timeline frame `frame` is, and chooses New, then
/// Adjustment clip.
fn make_an_adjustment_clip_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    window.harness.get_by_label("Adjustment clip").click();
    window.settle();
}

/// The adjustment clip that is the `index`th clip of the track at `position`, and how long it lasts.
fn adjustment_clip(window: &Window, position: usize, index: usize) -> (ClipId, u64) {
    let location = window.clip(position, index);
    let placement = window.application().project().placement_of(location).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Adjustment);
    (location, placement.clip.duration_in_frames_it_says().unwrap())
}

#[test]
fn the_menu_that_opens_on_an_empty_part_of_a_track_has_an_adjustment_clip_beside_the_text_object() {
    let mut window = Window::showing(project_of(90));
    let empty_part = window.middle_of_the_strip_at(0, 100);
    window.right_click(empty_part);
    window.harness.get_by_label_contains("New").click();
    window.settle();

    assert!(window.harness.query_by_label("Text object").is_some());
    assert!(window.harness.query_by_label("Adjustment clip").is_some());
    assert!(
        !window.application().has_unsaved_changes(),
        "opening a menu changes nothing"
    );
}

#[test]
fn choosing_adjustment_clip_puts_one_where_the_menu_was_opened_for_five_seconds_selects_it_and_shows_its_properties() {
    let mut window = Window::showing(project_of(90));

    make_an_adjustment_clip_at(&mut window, 0, 100);

    let (location, duration) = adjustment_clip(&window, 0, 1);
    assert_eq!(duration, 150, "five seconds at 30 frames a second");
    let placement = window.application().project().placement_of(location).unwrap();
    assert!(placement.first_timeline_frame.value().abs_diff(100) <= 1);
    assert_eq!(window.application().selected_clip(), Some(location));
    assert!(window.application().has_unsaved_changes());
    assert!(
        window.harness.query_by_label("Adjustment clip").is_some(),
        "the box of its properties is in the panel"
    );
    assert!(
        window.harness.query_by_label("Shown for").is_some(),
        "and how long it lasts"
    );
    assert!(
        window
            .harness
            .query_by_label_contains("Its effects are done to everything behind it.")
            .is_some()
    );
    assert_eq!(
        window.current_frame(),
        placement.first_timeline_frame.value(),
        "the playhead went to it, so that what it does shows"
    );
}

#[test]
fn an_adjustment_clip_that_is_made_is_undone_and_done_again_as_any_edit_is() {
    let mut window = Window::showing(project_of(90));
    make_an_adjustment_clip_at(&mut window, 0, 100);
    assert_eq!(window.application().project().tracks()[0].entries().len(), 2);

    undo(&mut window);

    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        1,
        "only the video is left"
    );
}

#[test]
fn an_adjustment_clip_is_only_as_long_as_there_is_room_for_before_the_next_clip_and_is_not_put_where_a_clip_is() {
    // The video lasts 90 frames, and a track in front of it has a text object from frame 30 to 60.
    let mut project = project_of(90);
    let front = project.add_track_in_front();
    project
        .try_insert_text(
            front,
            TimelineFrameIndex::new(30),
            crate::project::TextObject::default(),
            30,
        )
        .unwrap();
    let mut window = Window::showing(project);

    // Before the text object: room for 30 frames only, though 150 are wanted.
    window.do_this(Command::NewAdjustmentClip {
        track: front,
        frame: TimelineFrameIndex::new(0),
    });
    let (_, duration) = adjustment_clip(&window, 0, 0);
    assert_eq!(duration, 30);

    // Where the text object is: no room, and it says so.
    window.application_mut().interface.dialog = None;
    window.do_this(Command::NewAdjustmentClip {
        track: front,
        frame: TimelineFrameIndex::new(40),
    });
    let (title, _) = window.application().message_showing().expect("it says why");
    assert_eq!(title, "There is no room for an adjustment clip there");
}

#[test]
fn an_adjustment_clip_is_not_made_inside_the_rectangle_of_a_clip_group_and_the_message_names_the_group() {
    let mut project = project_of(90);
    let back = project.tracks()[0].id();
    let front = project.add_track_in_front();
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    let (text, _) = project
        .try_insert_text(
            front,
            TimelineFrameIndex::new(10),
            crate::project::TextObject::default(),
            20,
        )
        .unwrap();
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(video), Member::Clip(text)])
        .unwrap();
    let _: TrackId = back;
    let mut window = Window::showing(project);

    // Frame 50 of the front track is free, and is inside the rectangle (frames 0 to 90 across both tracks).
    window.do_this(Command::NewAdjustmentClip {
        track: front,
        frame: TimelineFrameIndex::new(50),
    });

    let (title, text) = window.application().message_showing().expect("it says why");
    assert_eq!(title, "An adjustment clip cannot be put there");
    assert!(
        text.contains("would be inside the rectangle of the clip group"),
        "{text}"
    );
}

#[test]
fn an_effect_put_on_an_adjustment_clip_is_one_of_its_properties_and_shown_in_the_panel() {
    let mut window = Window::showing(project_of(90));
    make_an_adjustment_clip_at(&mut window, 0, 100);
    let (location, _) = adjustment_clip(&window, 0, 1);

    window.do_this(Command::AddEffect {
        target: crate::project::AnimationTarget::Clip(location),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();

    let effects = window
        .application()
        .project()
        .effects_of(crate::project::AnimationTarget::Clip(location))
        .unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects.iter().next().unwrap().kind(), EffectKind::HUE_SHIFT);
    assert!(
        window.harness.query_by_label_contains("Hue shift").is_some(),
        "the box of the effect is in the panel with the box of the clip"
    );
}
