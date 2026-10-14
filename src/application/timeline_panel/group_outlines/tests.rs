//! Tests of where the outlines of groups are, worked out from where the rows are on the screen.

use egui::{Rect, pos2, vec2};

use super::*;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::FrameRate;
use crate::project::groups::test_projects::lay_out;

/// The rows of a project laid out as they would be on a screen: the strips begin at x = 200 and are 1000 wide, 4 points are a frame, each track is
/// as tall as the tracks are, with the room that is between tracks between them, the first track's strip beginning at y = 50, and the boxes of the titles
/// begin at x = 40.
struct Screen {
    views: Vec<StripView>,
}

fn screen_of(project: &Project) -> Screen {
    // 120 points a second at 30 frames a second: 4 points a frame.
    let geometry = TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 100, 100, 120.0);
    let height = crate::project::TrackHeight::DEFAULT_IN_POINTS;
    let views = (0..project.tracks().len())
        .map(|position| {
            let top = 50.0 + position as f32 * (height + theme::SPACE_BETWEEN_TRACKS);
            StripView::new(
                geometry,
                0.0,
                Rect::from_min_size(pos2(200.0, top), vec2(1000.0, height)),
            )
        })
        .collect();
    Screen { views }
}

fn outlines_of_the_groups_of(project: &Project, screen: &Screen) -> Vec<OutlineOfAGroup> {
    let views: Vec<(&Track, StripView)> = project.tracks().iter().zip(screen.views.iter().copied()).collect();
    outlines_of_the_groups(
        project,
        WhereTheRowsAre {
            views: &views,
            left_of_the_title_boxes: 40.0,
        },
    )
}

fn bounds_of(points: &[Pos2]) -> Rect {
    Rect::from_points(points)
}

fn frame_x(frame: u64) -> f32 {
    200.0 + 4.0 * frame as f32
}

#[test]
fn a_project_that_has_no_groups_has_no_outlines() {
    let laid = lay_out(&[&[(0, 10)], &[(5, 10)]]);
    let screen = screen_of(&laid.project);

    assert!(outlines_of_the_groups_of(&laid.project, &screen).is_empty());
}

#[test]
fn a_clip_group_has_a_loop_that_goes_round_the_rectangle_of_frames_and_tracks_that_holds_its_members_a_little_way_out_of_it()
 {
    let mut laid = lay_out(&[&[(10, 20)], &[(15, 30)], &[(60, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let screen = screen_of(&laid.project);

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    assert_eq!(outlines.len(), 1);
    let outline = &outlines[0];
    assert_eq!(outline.group, group);
    assert_eq!(outline.loops.len(), 1);
    // From frame 10 (the first clip) to frame 45 (the end of the second), across the first two strips, 3 points out from them.
    let strips_top = screen.views[0].rectangle.top();
    let strips_bottom = screen.views[1].rectangle.bottom();
    assert_eq!(
        bounds_of(&outline.loops[0]),
        Rect::from_min_max(
            pos2(frame_x(10) - 3.0, strips_top - 3.0),
            pos2(frame_x(45) + 3.0, strips_bottom + 3.0)
        )
    );
    assert_eq!(outline.colour, colour_of(laid.project.group(group).unwrap()));
}

#[test]
fn the_corners_of_the_outline_of_a_clip_group_are_as_round_as_the_corners_of_the_clips_with_the_padding_more() {
    let mut laid = lay_out(&[&[(10, 20)], &[(15, 30)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let screen = screen_of(&laid.project);

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    let loop_points = &outlines[0].loops[0];
    let bounds = bounds_of(loop_points);
    let radius = f32::from(theme::CLIP_CORNER_RADIUS) + 3.0;
    // The very corner of the rectangle that has the outline in it is not on the outline: it is cut off round.
    for corner_of_the_rectangle in [
        bounds.left_top(),
        bounds.right_top(),
        bounds.right_bottom(),
        bounds.left_bottom(),
    ] {
        let a_little_in = corner_of_the_rectangle + (bounds.center() - corner_of_the_rectangle).normalized() * 3.0;
        assert!(!is_inside(a_little_in, loop_points), "{a_little_in:?}");
    }
    // And a radius in from both sides of a corner (the middle of its arc) is inside, and a point on the arc is on the line.
    let middle_of_the_top_left_arc = bounds.left_top() + vec2(radius, radius);
    assert!(is_inside(middle_of_the_top_left_arc, loop_points));
    assert!(
        loop_points
            .iter()
            .any(|point| ((*point - middle_of_the_top_left_arc).length() - radius).abs() < 0.05),
        "a point of the outline is on the circle that the corner is an arc of"
    );
}

#[test]
fn a_clip_group_that_has_a_clip_group_in_it_is_four_points_further_out_than_the_one_in_it() {
    let mut laid = lay_out(&[&[(10, 20)], &[(15, 30)], &[(60, 10)]]);
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)])
        .unwrap();
    let screen = screen_of(&laid.project);

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    assert_eq!(outlines.len(), 2);
    assert_eq!(
        outlines[0].group, outer,
        "the outer one is drawn first, so the inner one is over it"
    );
    assert_eq!(outlines[0].height, 1);
    assert_eq!(outlines[1].height, 0);
    let inner_bounds = bounds_of(&outlines[1].loops[0]);
    let outer_bounds = bounds_of(&outlines[0].loops[0]);
    assert!(
        outer_bounds.contains_rect(inner_bounds.expand(3.99)),
        "{outer_bounds:?} round {inner_bounds:?}"
    );
    // The outer one reaches the clip on the third track, and is 7 points out from it (3 and 4).
    assert_eq!(outer_bounds.max.y, screen.views[2].rectangle.bottom() + 7.0);
    assert_eq!(
        outer_bounds.min.x,
        frame_x(10) - 7.0,
        "and 4 further out than the inner one at the left"
    );
}

#[test]
fn a_track_group_goes_across_the_whole_timeline_and_over_the_boxes_of_the_titles_of_its_tracks() {
    let mut laid = lay_out(&[&[(10, 20)], &[(15, 30)], &[(60, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let screen = screen_of(&laid.project);

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    assert_eq!(outlines[0].group, group);
    let bounds = bounds_of(&outlines[0].loops[0]);
    // From the left of the title boxes, to the end of the project (the third track ends at frame 70), across the first two strips.
    assert_eq!(bounds.min.x, 40.0 - 3.0);
    assert_eq!(bounds.max.x, frame_x(70) + 3.0);
    assert_eq!(bounds.min.y, screen.views[0].rectangle.top() - 3.0);
    assert_eq!(bounds.max.y, screen.views[1].rectangle.bottom() + 3.0);
}

#[test]
fn a_tag_goes_round_clips_that_are_apart_with_a_loop_that_joins_them_and_has_each_of_them_inside_it() {
    let mut laid = lay_out(&[&[(0, 10)], &[(100, 10)], &[(30, 10)]]);
    let tag = laid
        .project
        .create_group(
            GroupKind::Tag,
            &[laid.clip_member(0, 0), laid.clip_member(1, 0), laid.clip_member(2, 0)],
        )
        .unwrap();
    let screen = screen_of(&laid.project);

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    assert_eq!(outlines[0].group, tag);
    assert_eq!(
        outlines[0].loops.len(),
        1,
        "one loop, round all of them and what joins them"
    );
    let loop_points = &outlines[0].loops[0];
    for (track, frame) in [(0, 5u64), (1, 105), (2, 35)] {
        let middle = pos2(frame_x(frame), screen.views[track].rectangle.center().y);
        assert!(
            is_inside(middle, loop_points),
            "the clip on track {track} is inside the loop"
        );
    }
    // The loop is not the rectangle that holds them all: the place on the top track above the clip on the second, far from the clip on the top
    // track and from the spokes, is not inside it (and it is inside the rectangle that holds them).
    let in_the_hull_and_far_from_the_clips = pos2(frame_x(105), screen.views[0].rectangle.center().y);
    assert!(bounds_of(loop_points).contains(in_the_hull_and_far_from_the_clips));
    assert!(!is_inside(in_the_hull_and_far_from_the_clips, loop_points));
}

#[test]
fn a_tag_that_has_a_clip_group_in_it_goes_round_the_outline_of_the_group_and_not_only_the_clips() {
    let mut laid = lay_out(&[&[(10, 20)], &[(10, 20)], &[(200, 10)]]);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(clip_group), laid.clip_member(2, 0)])
        .unwrap();
    let screen = screen_of(&laid.project);

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    let of = |group: GroupId| outlines.iter().find(|outline| outline.group == group).unwrap();
    assert_eq!(of(tag).height, 1);
    assert_eq!(of(clip_group).height, 0);
    let inner = bounds_of(&of(clip_group).loops[0]);
    let loop_points = &of(tag).loops[0];
    for corner in [
        inner.left_top(),
        inner.right_top(),
        inner.right_bottom(),
        inner.left_bottom(),
    ] {
        // (The corners are rounded, so a point a little in from each is looked at.)
        let nudged = corner + (inner.center() - corner).normalized() * 12.0;
        assert!(is_inside(nudged, loop_points), "{nudged:?}");
    }
    assert!(
        bounds_of(loop_points).min.x <= inner.min.x - 4.0 + 0.01,
        "4 points out from the outline of the clip group"
    );
}

#[test]
fn a_group_whose_tracks_are_not_in_the_rows_has_no_outline_and_no_mistake_is_made() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let screen = screen_of(&laid.project);
    let views: Vec<(&Track, StripView)> = laid
        .project
        .tracks()
        .iter()
        .zip(screen.views.iter().copied())
        .take(1)
        .collect();

    let outlines = outlines_of_the_groups(
        &laid.project,
        WhereTheRowsAre {
            views: &views,
            left_of_the_title_boxes: 40.0,
        },
    );

    assert_eq!(outlines.len(), 1);
    assert!(outlines[0].loops.is_empty());
}

#[test]
fn the_padding_grows_by_four_points_for_each_level_of_groups_in_groups() {
    assert_eq!(padding_at(0), 3.0);
    assert_eq!(padding_at(1), 7.0);
    assert_eq!(padding_at(2), 11.0);
}

/// Whether `point` is inside the loop (the even-odd rule).
fn is_inside(point: Pos2, points: &[Pos2]) -> bool {
    let count = points.len();
    let mut inside = false;
    for at in 0..count {
        let (a, b) = (points[at], points[(at + 1) % count]);
        if (a.y > point.y) != (b.y > point.y) {
            let crossing_x = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if crossing_x > point.x {
                inside = !inside;
            }
        }
    }
    inside
}

#[test]
fn the_outline_of_a_tag_that_has_every_clip_of_a_long_project_in_it_is_worked_out_in_a_short_time() {
    // 10 tracks of 60 clips each, all in one tag. (Outlines are worked out each time the timeline is drawn.)
    let clips: Vec<(u64, u64)> = (0..60).map(|number| (number * 10, 7)).collect();
    let layout: Vec<&[(u64, u64)]> = (0..10).map(|_| clips.as_slice()).collect();
    let mut laid = lay_out(&layout);
    let members: Vec<Member> = (0..10)
        .flat_map(|track| (0..60).map(move |clip| (track, clip)))
        .map(|(track, clip)| laid.clip_member(track, clip))
        .collect();
    laid.project.create_group(GroupKind::Tag, &members).unwrap();
    let screen = screen_of(&laid.project);
    let started = std::time::Instant::now();

    let outlines = outlines_of_the_groups_of(&laid.project, &screen);

    assert_eq!(outlines.len(), 1);
    assert_eq!(outlines[0].loops.len(), 1);
    // (In a build for testing it took 4.3 seconds when the spokes were found by looking at every piece that is in against every piece that is
    // not each time a piece came in, and 0.07 seconds when each piece that is not in keeps the nearest of those that are.)
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "it took {:?}",
        started.elapsed()
    );
}
