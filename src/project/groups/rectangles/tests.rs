//! Where clips, tracks and groups are on the timeline, as rectangles.

use super::*;
use crate::project::GroupKind;
use crate::project::groups::test_projects::lay_out;

fn rectangle(first_frame: u64, end_frame: u64, first_track: usize, last_track: usize) -> Rectangle {
    Rectangle {
        first_frame,
        end_frame,
        first_track,
        last_track,
    }
}

// --- the rectangles themselves ---

#[test]
fn the_rectangle_of_frames_on_a_track_reaches_across_that_track_only() {
    let one = Rectangle::of_frames_on_a_track(10, 25, 3);
    assert_eq!(one, rectangle(10, 25, 3, 3));
    assert_eq!(one.tracks(), 3..=3);
    assert!(one.reaches_the_track_at(3) && !one.reaches_the_track_at(2) && !one.reaches_the_track_at(4));
}

#[test]
fn the_smallest_rectangle_that_has_two_rectangles_has_the_earliest_start_the_latest_end_and_all_their_tracks() {
    let first = rectangle(10, 20, 1, 2);
    let second = rectangle(5, 15, 4, 5);
    let both = first.holding(second);
    assert_eq!(both, rectangle(5, 20, 1, 5));
    assert_eq!(second.holding(first), both, "whichever way round");
    assert_eq!(first.holding(first), first);
    assert_eq!(both.tracks(), 1..=5);
}

#[test]
fn a_frame_is_in_a_rectangle_from_its_first_up_to_but_not_including_its_end() {
    let it = rectangle(10, 20, 0, 0);
    assert!(it.has_any_of_the_frames(10, 11), "its first frame");
    assert!(it.has_any_of_the_frames(19, 30), "its last frame");
    assert!(it.has_any_of_the_frames(0, 100), "all of it and more");
    assert!(it.has_any_of_the_frames(12, 14), "inside");
    assert!(!it.has_any_of_the_frames(20, 30), "starting where it ends is not in it");
    assert!(!it.has_any_of_the_frames(0, 10), "ending where it starts is not in it");
    assert!(!it.has_any_of_the_frames(0, 5));
    assert!(!it.has_any_of_the_frames(25, 30));
}

#[test]
fn two_rectangles_are_on_top_of_each_other_when_they_share_a_frame_of_a_track() {
    let it = rectangle(10, 20, 1, 3);
    assert!(
        it.is_on_top_of(&rectangle(15, 25, 3, 4)),
        "the last track and some frames"
    );
    assert!(it.is_on_top_of(&rectangle(0, 11, 0, 1)), "the first track and a frame");
    assert!(!it.is_on_top_of(&rectangle(20, 30, 1, 3)), "the frames are after");
    assert!(!it.is_on_top_of(&rectangle(10, 20, 4, 5)), "the tracks are below");
    assert!(!it.is_on_top_of(&rectangle(10, 20, 0, 0)), "the tracks are above");
    assert!(!it.is_on_top_of(&rectangle(0, 10, 0, 0)), "neither");
    assert!(it.is_on_top_of(&it));
}

// --- clips and tracks ---

#[test]
fn a_clip_is_where_it_lasts_on_the_track_it_is_on() {
    let laid = lay_out(&[&[(10, 20)], &[(5, 5), (30, 10)]]);
    let project = &laid.project;
    assert_eq!(
        project.rectangle_of_the_clip(laid.clip(0, 0)),
        Some(rectangle(10, 30, 0, 0))
    );
    assert_eq!(
        project.rectangle_of_the_clip(laid.clip(1, 0)),
        Some(rectangle(5, 10, 1, 1))
    );
    assert_eq!(
        project.rectangle_of_the_clip(laid.clip(1, 1)),
        Some(rectangle(30, 40, 1, 1))
    );
}

#[test]
fn a_clip_is_found_where_it_is_after_it_is_moved_to_another_track() {
    let mut laid = lay_out(&[&[(10, 20)], &[]]);
    let clip = laid.clip(0, 0);
    laid.project
        .move_clips(&[crate::project::ClipMove {
            clip,
            to_track: laid.tracks[1],
            start: crate::project::TimelineFrameIndex::new(50),
        }])
        .unwrap();
    assert_eq!(laid.project.rectangle_of_the_clip(clip), Some(rectangle(50, 70, 1, 1)));
}

#[test]
fn a_track_is_every_frame_of_the_project_on_that_track() {
    let laid = lay_out(&[&[(0, 10)], &[(20, 40)], &[]]);
    let project = &laid.project;
    assert_eq!(project.total_frame_count(), 60);
    assert_eq!(
        project.rectangle_of_the_track(laid.tracks[0]),
        Some(rectangle(0, 60, 0, 0))
    );
    assert_eq!(
        project.rectangle_of_the_track(laid.tracks[2]),
        Some(rectangle(0, 60, 2, 2)),
        "an empty track too"
    );
}

#[test]
fn what_is_not_in_the_project_has_no_rectangle() {
    let laid = lay_out(&[&[(0, 10)]]);
    assert_eq!(
        laid.project
            .rectangle_of_the_clip(crate::project::ClipId::new(TrackId::new(99), 0)),
        None
    );
    assert_eq!(
        laid.project
            .rectangle_of_the_clip(crate::project::ClipId::new(laid.tracks[0], 99)),
        None
    );
    assert_eq!(laid.project.rectangle_of_the_track(TrackId::new(99)), None);
    assert_eq!(laid.project.rectangle_of_the_group(GroupId::new(0)), None);
}

// --- groups ---

#[test]
fn a_clip_group_is_the_smallest_rectangle_that_has_all_its_clips() {
    let mut laid = lay_out(&[&[(40, 10)], &[], &[(10, 20)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
        .unwrap();
    assert_eq!(
        laid.project.rectangle_of_the_group(group),
        Some(rectangle(10, 50, 0, 2))
    );
}

#[test]
fn a_clip_group_has_the_rectangles_of_the_clip_groups_in_it() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(30, 10)]]);
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)])
        .unwrap();
    assert_eq!(laid.project.rectangle_of_the_group(inner), Some(rectangle(0, 10, 0, 1)));
    assert_eq!(laid.project.rectangle_of_the_group(outer), Some(rectangle(0, 40, 0, 2)));
}

#[test]
fn a_tag_has_the_rectangle_that_holds_everything_in_it_whatever_is_between() {
    let mut laid = lay_out(&[&[(0, 10)], &[(5, 5)], &[(50, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
        .unwrap();
    assert_eq!(
        laid.project.rectangle_of_the_group(group),
        Some(rectangle(0, 60, 0, 2)),
        "the clip between them, which is not in the tag, is inside the rectangle that holds them"
    );
}

#[test]
fn a_track_group_reaches_across_all_the_frames_of_the_project_whatever_its_clips_are() {
    let mut laid = lay_out(&[&[(30, 10)], &[(0, 5)], &[(0, 100)]]);
    let group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    assert_eq!(
        laid.project.rectangle_of_the_group(group),
        Some(rectangle(0, 100, 0, 1))
    );
}

#[test]
fn a_track_group_that_has_only_track_groups_in_it_reaches_across_all_the_frames_too() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 80)]]);
    let first = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(1)])
        .unwrap();
    let both = laid
        .project
        .create_group(GroupKind::TrackGroup, &[Member::Group(first), Member::Group(second)])
        .unwrap();
    assert_eq!(laid.project.rectangle_of_the_group(both), Some(rectangle(0, 80, 0, 1)));
}
