// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of moving the edges of a clip group: what it takes in and lets go of, and the places an edge can be put at.

use super::*;
use crate::project::GroupProblem;
use crate::project::groups::random_edits_tests::project_after_random_edits_with_more_clip_groups;
use crate::project::groups::test_projects::{Laid, lay_out};

fn members_of(project: &Project, group: GroupId) -> Vec<Member> {
    project.group(group).unwrap().members().to_vec()
}

fn frames_of(project: &Project, group: GroupId) -> (u64, u64) {
    let rectangle = project.rectangle_of_the_group(group).unwrap();
    (rectangle.first_frame, rectangle.end_frame)
}

/// Two tracks, and a clip group of `A` (on the first, from frame 0 to 10) and `B` (on the second, from 20 to 30), so that its rectangle is
/// frames 0 to 30 of both tracks, and clips that are not in it: `C` (on the first, 40 to 50), `D` (on the second, 60 to 70) and `E` (on the
/// first, 80 to 90). Gives the project, the group, and the clips in the order `A`, `B`, `C`, `D`, `E`.
fn a_clip_group_with_clips_beside_it() -> (Laid, GroupId, [ClipId; 5]) {
    let mut laid = lay_out(&[&[(0, 10), (40, 10), (80, 10)], &[(20, 10), (60, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let clips = [
        laid.clip(0, 0),
        laid.clip(1, 0),
        laid.clip(0, 1),
        laid.clip(1, 1),
        laid.clip(0, 2),
    ];
    (laid, group, clips)
}

#[test]
fn moving_the_right_edge_past_clips_takes_them_in_and_the_rectangle_fits_all_of_them() {
    let (mut laid, group, [a, b, c, ..]) = a_clip_group_with_clips_beside_it();

    laid.project.set_the_frames_of_the_clip_group(group, 0, 50).unwrap();

    assert_eq!(
        members_of(&laid.project, group),
        [Member::Clip(a), Member::Clip(b), Member::Clip(c)]
    );
    assert_eq!(frames_of(&laid.project, group), (0, 50));
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn moving_the_right_edge_to_where_there_is_nothing_to_take_in_changes_nothing() {
    let (mut laid, group, _) = a_clip_group_with_clips_beside_it();
    let before = laid.project.clone();

    // Frame 35 is in the empty stretch after the rectangle (it ends at 30), and the next clip begins at 40.
    laid.project.set_the_frames_of_the_clip_group(group, 0, 35).unwrap();

    assert_eq!(laid.project, before, "it fits what is in it, which is what it was");
}

#[test]
fn an_edge_that_would_cut_across_a_clip_is_not_moved_there() {
    let (mut laid, group, [_, b, c, ..]) = a_clip_group_with_clips_beside_it();
    let before = laid.project.clone();

    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(group, 0, 45),
        Err(EditError::TheEdgeWouldCutAcross {
            member: Member::Clip(c)
        })
    );
    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(group, 0, 25),
        Err(EditError::TheEdgeWouldCutAcross {
            member: Member::Clip(b)
        }),
        "a clip that is in the group is cut across too"
    );
    assert_eq!(laid.project, before);
}

#[test]
fn moving_an_edge_in_past_a_clip_lets_it_go() {
    let (mut laid, group, [a, _, ..]) = a_clip_group_with_clips_beside_it();

    // The right edge to frame 15, in the empty stretch between A and B: B is outside the frames.
    laid.project.set_the_frames_of_the_clip_group(group, 0, 15).unwrap();
    assert_eq!(members_of(&laid.project, group), [Member::Clip(a)]);
    assert_eq!(
        frames_of(&laid.project, group),
        (0, 10),
        "and the rectangle fits what is left"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);

    // And the left edge, from where it is, to the same stretch: A goes and B is left.
    let (mut laid, group, [a, b, ..]) = a_clip_group_with_clips_beside_it();
    laid.project.set_the_frames_of_the_clip_group(group, 15, 30).unwrap();
    assert_eq!(members_of(&laid.project, group), [Member::Clip(b)]);
    assert!(laid.project.shape_group_of(Member::Clip(a)).is_none());
}

#[test]
fn nothing_can_be_left_with_nothing_in_the_group() {
    let (mut laid, group, _) = a_clip_group_with_clips_beside_it();
    let before = laid.project.clone();

    // (The frames 12 to 18 have no clip of the tracks it reaches across.)
    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(group, 12, 18),
        Err(EditError::NothingWouldBeLeftInTheGroup { group })
    );
    assert_eq!(laid.project, before);
}

#[test]
fn a_clip_group_that_is_inside_the_frames_is_taken_in_as_a_whole_and_one_that_would_be_cut_is_not() {
    let (mut laid, group, [_, _, c, d, _]) = a_clip_group_with_clips_beside_it();
    // A clip group of C (first track, 40 to 50) and D (second track, 60 to 70): its rectangle is frames 40 to 70 of both tracks.
    let group_of_c_and_d = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(c), Member::Clip(d)])
        .unwrap();
    assert_eq!(frames_of(&laid.project, group_of_c_and_d), (40, 70));

    // The edge at 70 takes C and D in, as the one group that they are in.
    laid.project.set_the_frames_of_the_clip_group(group, 0, 70).unwrap();
    assert!(
        laid.project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Group(group_of_c_and_d))
    );
    assert!(!laid.project.group(group).unwrap().has_as_a_member(Member::Clip(c)));
    assert_eq!(frames_of(&laid.project, group), (0, 70));
    assert_eq!(laid.project.problems_with_the_groups(), []);

    // At 50, it would cut across the group of C and D (C is inside, and D is not): that is not done.
    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(group, 0, 50),
        Err(EditError::TheEdgeWouldCutAcross {
            member: Member::Group(group_of_c_and_d)
        })
    );
}

#[test]
fn a_clip_group_that_is_taken_in_by_the_edge_of_another_and_would_make_its_rectangle_take_in_a_clip_is_not_taken_in() {
    // A clip group of A (first track, 0 to 10) and B (second, 20 to 30); another of C (second track, 40 to 50) and F (third track, 40 to 50),
    // which reaches across the second and third tracks; and a clip W on the third track at 5 to 8, which is not in either, and would be inside
    // the first group's rectangle if it took in the second (the rectangle would reach the third track).
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10), (40, 10)], &[(5, 3), (40, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(1, 1), laid.clip_member(2, 1)])
        .unwrap();
    let w = laid.clip(2, 0);
    let before = laid.project.clone();

    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(group, 0, 50),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip: w }
        ))
    );
    assert_eq!(laid.project, before);
}

#[test]
fn what_is_let_go_of_stays_in_the_clip_group_round_the_group_as_far_as_it_is_still_inside_its_rectangle() {
    // The group of A (first track, 0 to 10) and B (second, 20 to 30) is in another with a clip Y (second, 40 to 50), so that the outer rectangle
    // is 0 to 50 of both tracks.
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10), (40, 10)]]);
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(1, 1)])
        .unwrap();
    let b = laid.clip(1, 0);
    assert_eq!(frames_of(&laid.project, outer), (0, 50));

    // The inner group lets go of B (its right edge is moved to 15): B is still inside the rectangle of the outer group, and is in it.
    laid.project.set_the_frames_of_the_clip_group(inner, 0, 15).unwrap();

    assert_eq!(members_of(&laid.project, inner), [laid.clip_member(0, 0)]);
    assert!(laid.project.group(outer).unwrap().has_as_a_member(Member::Clip(b)));
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn what_is_taken_in_leaves_the_clip_group_round_the_group_that_it_was_directly_in_and_what_is_let_go_of_leaves_both_outside_the_rectangle()
 {
    // The group of A (first track, 0 to 10) and B (second, 20 to 30) is in another with a clip Z (first, 40 to 50), so that the outer rectangle
    // is 0 to 50 of both tracks.
    let mut laid = lay_out(&[&[(0, 10), (40, 10)], &[(20, 10)]]);
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(0, 1)])
        .unwrap();
    let z = laid.clip(0, 1);

    // The inner group takes Z in (its right edge goes to 50): it is in the inner group, and out of the outer one that it was directly in.
    laid.project.set_the_frames_of_the_clip_group(inner, 0, 50).unwrap();
    assert!(laid.project.group(inner).unwrap().has_as_a_member(Member::Clip(z)));
    assert!(!laid.project.group(outer).unwrap().has_as_a_member(Member::Clip(z)));
    assert_eq!(laid.project.problems_with_the_groups(), []);

    // And lets it go (the edge is moved back to 35): it is outside the rectangle of the outer group, which is what the inner one is again, so
    // it is in neither.
    laid.project.set_the_frames_of_the_clip_group(inner, 0, 35).unwrap();
    assert!(laid.project.shape_group_of(Member::Clip(z)).is_none());
    assert_eq!(members_of(&laid.project, outer), [Member::Group(inner)]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn what_is_let_go_of_goes_to_the_nearest_clip_group_round_the_group_and_not_to_one_further_out() {
    // Four clips on one track: A (0 to 10), B (15 to 25), C (30 to 40) and D (45 to 55). The first group has A and B, the second has the first
    // and C, the third has the second and D.
    let mut laid = lay_out(&[&[(0, 10), (15, 10), (30, 10), (45, 10)]]);
    let first = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(0, 1)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(first), laid.clip_member(0, 2)])
        .unwrap();
    let third = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(second), laid.clip_member(0, 3)])
        .unwrap();
    let b = laid.clip(0, 1);

    // The first lets go of B (its right edge goes to 12). B is still inside the rectangle of the second, 0 to 40, and so is in it: not in the
    // third, which would leave it inside the rectangle of the second without being in it.
    laid.project.set_the_frames_of_the_clip_group(first, 0, 12).unwrap();

    assert!(laid.project.group(second).unwrap().has_as_a_member(Member::Clip(b)));
    assert!(!laid.project.group(third).unwrap().has_as_a_member(Member::Clip(b)));
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_is_in_groups_in_one_another_beside_the_group_is_taken_in_or_cut_across_as_the_largest_of_them() {
    // The group is A (first track, 0 to 10) and Z (second track, 0 to 10). Beside it, on the first track: C (20 to 30) and D (35 to 45), in
    // a clip group Y, which is in a clip group X with E (50 to 60).
    let mut laid = lay_out(&[&[(0, 10), (20, 10), (35, 10), (50, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let y = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 1), laid.clip_member(0, 2)])
        .unwrap();
    let x = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(y), laid.clip_member(0, 3)])
        .unwrap();
    let before = laid.project.clone();

    // The edge at 46 would take in all of Y but cut across X, which is what C and D are in as far as this group is concerned.
    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(group, 0, 46),
        Err(EditError::TheEdgeWouldCutAcross {
            member: Member::Group(x)
        })
    );
    assert_eq!(laid.project, before);

    // At 61 it takes in X, as one: not Y and X.
    laid.project.set_the_frames_of_the_clip_group(group, 0, 61).unwrap();
    assert_eq!(
        members_of(&laid.project, group),
        [laid.clip_member(0, 0), laid.clip_member(1, 0), Member::Group(x)]
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_group_that_is_not_a_clip_group_or_is_not_there_has_no_edges_to_move() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let nowhere = GroupId::new(90);

    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(tag, 0, 20),
        Err(EditError::NotAClipGroup { group: tag })
    );
    assert_eq!(
        laid.project.set_the_frames_of_the_clip_group(nowhere, 0, 20),
        Err(EditError::UnknownGroup { group: nowhere })
    );
    assert_eq!(
        laid.project
            .places_for_an_edge_of_the_clip_group(tag, ClipGroupEdge::Left),
        vec![]
    );
    assert_eq!(
        laid.project
            .places_for_an_edge_of_the_clip_group(nowhere, ClipGroupEdge::Right),
        vec![]
    );
}

#[test]
fn the_places_for_the_right_edge_are_between_the_clips_and_at_the_edges_of_them_and_not_across_them() {
    let (laid, group, _) = a_clip_group_with_clips_beside_it();

    assert_eq!(
        laid.project
            .places_for_an_edge_of_the_clip_group(group, ClipGroupEdge::Right),
        vec![10..=20, 30..=40, 50..=60, 70..=80, 90..=u64::MAX],
        "after A, after B (where it is), after C, after D and after E, and the empty stretch each is followed by"
    );
}

#[test]
fn the_places_for_the_left_edge_are_the_same_the_other_way() {
    // Two tracks: P (first, 0 to 10) and R (second, 20 to 30) are not in the clip group of Q (first, 40 to 50) and S (second, 60 to 70).
    let mut laid = lay_out(&[&[(0, 10), (40, 10)], &[(20, 10), (60, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 1), laid.clip_member(1, 1)])
        .unwrap();
    assert_eq!(frames_of(&laid.project, group), (40, 70));

    assert_eq!(
        laid.project
            .places_for_an_edge_of_the_clip_group(group, ClipGroupEdge::Left),
        vec![0..=0, 10..=20, 30..=40, 50..=60],
        "before P (it is taken in, with R), after P, after R (where it is), after Q: and none across a clip, or at the end"
    );
}

/// Whether the clip group `group` can be given the frames `first_frame` to `end_frame`, by trying it.
fn can_be_given(project: &Project, group: GroupId, first_frame: u64, end_frame: u64) -> bool {
    project
        .clone()
        .set_the_frames_of_the_clip_group(group, first_frame, end_frame)
        .is_ok()
}

#[test]
fn the_places_for_an_edge_are_the_frames_that_the_group_can_be_given_found_by_trying_each_frame_in_random_projects() {
    let mut compared = 0;
    let mut places_that_were_not_where_it_is = 0;
    for seed in 1..=60u64 {
        let project = project_after_random_edits_with_more_clip_groups(seed, 120);
        let last_frame = project.total_frame_count() + 3;
        for group in project
            .groups()
            .iter()
            .filter(|group| group.kind() == GroupKind::ClipGroup)
        {
            let Some(rectangle) = project.rectangle_of_the_group(group.id()) else {
                continue;
            };
            for edge in [ClipGroupEdge::Left, ClipGroupEdge::Right] {
                let places = project.places_for_an_edge_of_the_clip_group(group.id(), edge);
                for frame in 0..=last_frame {
                    let (first, end) = match edge {
                        ClipGroupEdge::Left => (frame, rectangle.end_frame),
                        ClipGroupEdge::Right => (rectangle.first_frame, frame),
                    };
                    let is_a_place = places.iter().any(|range| range.contains(&frame));
                    let by_trying = first < end && can_be_given(&project, group.id(), first, end);
                    assert_eq!(
                        is_a_place,
                        by_trying,
                        "seed {seed}, {} {edge:?} edge at {frame}: {places:?}",
                        group.id()
                    );
                    compared += 1;
                    places_that_were_not_where_it_is += usize::from(
                        is_a_place
                            && frame
                                != match edge {
                                    ClipGroupEdge::Left => rectangle.first_frame,
                                    ClipGroupEdge::Right => rectangle.end_frame,
                                },
                    );
                }
            }
        }
    }
    assert!(compared >= 3000, "{compared} places were looked at");
    assert!(
        places_that_were_not_where_it_is >= 500,
        "the test is worth little if the edges can seldom be put anywhere else: {places_that_were_not_where_it_is}"
    );
}
