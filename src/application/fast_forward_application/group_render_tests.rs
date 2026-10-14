//! Tests that the window can draw whatever groups there are: projects that were made by many edits in random order (with tags, clip groups and
//! track groups, groups in groups, effects and badges in the chains) are shown, with each group, each clip and each track selected in turn, so
//! that the outlines on the timeline and the box and the chain of effects in the properties panel are drawn for all of them.

use egui::Shape;

use super::group_box_tests::select_only;
use super::test_window::Window;
use crate::application::selection::SelectionItem;
use crate::project::groups::random_edits_tests::project_after_random_edits_with_more_clip_groups;
use crate::project::{GroupKind, Project};

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

#[test]
fn whatever_groups_there_are_the_window_draws_them_with_each_thing_selected_in_turn_and_nothing_that_is_drawn_has_a_point_that_is_not_finite()
 {
    let mut shown_with_groups = 0;
    let mut tags = 0;
    let mut clip_groups = 0;
    let mut track_groups = 0;
    let mut selected = 0;
    for seed in 1..=10u64 {
        let project = project_after_random_edits_with_more_clip_groups(seed, 120);
        if project.groups().is_empty() {
            continue;
        }
        shown_with_groups += 1;
        for group in project.groups() {
            match group.kind() {
                GroupKind::Tag => tags += 1,
                GroupKind::ClipGroup => clip_groups += 1,
                GroupKind::TrackGroup => track_groups += 1,
            }
        }
        let things = everything_that_can_be_selected(&project);
        let mut window = Window::showing(project);
        assert!(every_point_drawn_is_finite(&window), "seed {seed}: nothing selected");
        for item in things {
            select_only(&mut window, item);
            selected += 1;
            assert!(every_point_drawn_is_finite(&window), "seed {seed}: {item:?} selected");
            assert_eq!(
                window.application().project().problems_with_the_groups(),
                vec![],
                "seed {seed}: showing a project changes nothing in it"
            );
        }
    }
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
