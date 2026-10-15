// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that the window can draw whatever groups there are: projects that were made by many edits in random order (with tags, clip groups and
//! track groups, groups in groups, effects and badges in the chains) are shown, with each group, each clip and each track selected in turn, so
//! that the outlines on the timeline and the box and the chain of effects in the properties panel are drawn for all of them.

use egui::Shape;

use super::group_box_tests::select_only;
use super::test_window::Window;
use crate::application::selection::SelectionItem;
use crate::project::groups::random_edits_tests::project_after_random_edits_with_more_clip_groups;
use crate::project::{GroupKind, Project};
use crate::test_support::check_every_case_at_once;

/// Whether every point of every line that was drawn in the last frame is a number that is not infinite.
fn every_point_drawn_is_finite(window: &Window) -> bool {
    window.shapes_drawn().into_iter().all(|shape| match shape {
        Shape::Path(path) => path
            .points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite()),
        Shape::LineSegment { points, .. } => points.iter().all(|point| point.x.is_finite() && point.y.is_finite()),
        _ => true,
    })
}

/// Everything of the project that can be selected on its own: its groups, its clips and its tracks.
fn everything_that_can_be_selected(project: &Project) -> Vec<SelectionItem> {
    let groups = project.groups().iter().map(|group| SelectionItem::Group(group.id()));
    let clips = project
        .tracks()
        .iter()
        .flat_map(|track| track.entries().iter().map(|entry| SelectionItem::Clip(entry.id())));
    let tracks = project.tracks().iter().map(|track| SelectionItem::Track(track.id()));
    groups.chain(clips).chain(tracks).collect()
}

/// What was shown for the project that one seed makes.
#[derive(Default)]
struct WhatWasShown {
    had_groups: bool,
    tags: usize,
    clip_groups: usize,
    track_groups: usize,
    selected: usize,
}

/// Shows the project that the seed `seed` makes in a window, with each thing in it selected in turn, and checks every frame.
fn show_the_project_of_the_seed(seed: u64) -> WhatWasShown {
    let project = project_after_random_edits_with_more_clip_groups(seed, 120);
    if project.groups().is_empty() {
        return WhatWasShown::default();
    }
    let mut shown = WhatWasShown {
        had_groups: true,
        ..WhatWasShown::default()
    };
    for group in project.groups() {
        match group.kind() {
            GroupKind::Tag => shown.tags += 1,
            GroupKind::ClipGroup => shown.clip_groups += 1,
            GroupKind::TrackGroup => shown.track_groups += 1,
        }
    }
    let things = everything_that_can_be_selected(&project);
    let mut window = Window::showing(project);
    assert!(every_point_drawn_is_finite(&window), "seed {seed}: nothing selected");
    for item in things {
        select_only(&mut window, item);
        shown.selected += 1;
        assert!(every_point_drawn_is_finite(&window), "seed {seed}: {item:?} selected");
        assert_eq!(
            window.application().project().problems_with_the_groups(),
            vec![],
            "seed {seed}: showing a project changes nothing in it"
        );
    }
    shown
}

#[test]
fn whatever_groups_there_are_the_window_draws_them_with_each_thing_selected_in_turn_and_nothing_that_is_drawn_has_a_point_that_is_not_finite()
 {
    // (Each project is shown in a window of its own, so they are shown several at once: it takes a window a good while to draw a frame.)
    let seeds: Vec<u64> = (1..=10).collect();
    let shown = check_every_case_at_once(&seeds, |seed| show_the_project_of_the_seed(*seed));

    let shown_with_groups = shown.iter().filter(|shown| shown.had_groups).count();
    let tags: usize = shown.iter().map(|shown| shown.tags).sum();
    let clip_groups: usize = shown.iter().map(|shown| shown.clip_groups).sum();
    let track_groups: usize = shown.iter().map(|shown| shown.track_groups).sum();
    let selected: usize = shown.iter().map(|shown| shown.selected).sum();
    // (Measured: 10 projects, with 52 tags, 30 clip groups and 16 track groups in them, and 368 things selected.)
    assert!(
        shown_with_groups >= 9,
        "{shown_with_groups} projects had groups in them"
    );
    assert!(
        tags >= 25 && clip_groups >= 15 && track_groups >= 8,
        "{tags} tags, {clip_groups} clip groups, {track_groups} track groups: the test is worth little if there are few of each kind"
    );
    assert!(selected >= 200, "{selected} things were selected");
}
