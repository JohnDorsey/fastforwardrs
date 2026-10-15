// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of clips that are put in the rectangle of a clip group joining it, and of what a drag remembers so that they are out of it again
//! when they are dragged out.

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{ClipId, GroupProblem};

fn frame(number: u64) -> crate::project::TimelineFrameIndex {
    crate::project::TimelineFrameIndex::new(number)
}

fn move_to(clip: ClipId, to_track: crate::project::TrackId, start: u64) -> ClipMove {
    ClipMove {
        clip,
        to_track,
        start: frame(start),
    }
}

fn members_of(project: &Project, group: GroupId) -> Vec<Member> {
    project.group(group).unwrap().members().to_vec()
}

/// Three tracks: the clips of a clip group are on the first at frames 0 to 10 and on the second at frames 20 to 30, so its rectangle is
/// frames 0 to 30 of those two tracks; and on the third track is the clip that is not in it, to be moved. Gives the project, the group, and the
/// two clips of it and the other.
fn a_clip_group_and_a_clip_that_is_not_in_it() -> (Laid, GroupId, [ClipId; 3]) {
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let clips = [laid.clip(0, 0), laid.clip(1, 0), laid.clip(2, 0)];
    (laid, group, clips)
}

#[test]
fn a_clip_that_is_not_in_a_clip_group_is_put_in_it_when_it_is_moved_into_its_rectangle() {
    let (mut laid, group, [first, second, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();

    // Between the clips, on the first track: frames 12 to 22 are in the rectangle (0 to 30 of the first two tracks).
    laid.project
        .move_clips(&[move_to(outsider, laid.tracks[0], 12)])
        .unwrap();

    assert_eq!(
        members_of(&laid.project, group),
        [Member::Clip(first), Member::Clip(second), Member::Clip(outsider)],
        "it is in the group, last"
    );
    assert_eq!(
        laid.project
            .rectangle_of_the_group(group)
            .map(|rectangle| (rectangle.first_frame, rectangle.end_frame)),
        Some((0, 30)),
        "inside the rectangle, so that it is what it was"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_is_moved_so_that_it_is_partly_in_the_rectangle_is_in_the_group_and_the_rectangle_has_all_of_it() {
    let (mut laid, group, [_, _, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();

    // Frames 25 to 35 of the first track: five of them are in the rectangle.
    laid.project
        .move_clips(&[move_to(outsider, laid.tracks[0], 25)])
        .unwrap();

    assert!(
        laid.project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
    assert_eq!(
        laid.project
            .rectangle_of_the_group(group)
            .map(|rectangle| (rectangle.first_frame, rectangle.end_frame)),
        Some((0, 35))
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_is_moved_beside_the_rectangle_or_to_a_track_it_does_not_reach_is_not_put_in_the_group() {
    let (mut laid, group, [_, _, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();

    laid.project
        .move_clips(&[move_to(outsider, laid.tracks[0], 30)])
        .unwrap();
    assert!(
        !laid
            .project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider)),
        "frame 30 is the first after the rectangle"
    );
    laid.project
        .move_clips(&[move_to(outsider, laid.tracks[2], 5)])
        .unwrap();
    assert!(
        !laid
            .project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider)),
        "the third track is not one that the rectangle reaches"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_would_make_the_rectangle_take_in_a_clip_that_is_not_in_it_by_being_put_in_the_group_is_not_moved() {
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10), (32, 8)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let (outsider, in_the_way) = (laid.clip(2, 0), laid.clip(1, 1));
    let before = laid.project.clone();

    // Frames 25 to 35 of the first track: it would be in the group, and the rectangle would be 0 to 35, which the clip on the second track
    // that is from 32 to 40 would be in.
    assert_eq!(
        laid.project.move_clips(&[move_to(outsider, laid.tracks[0], 25)]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group,
                clip: in_the_way
            }
        ))
    );
    assert_eq!(laid.project, before, "nothing is changed");
    // Where the rectangle is not made longer, it is joined.
    laid.project
        .move_clips(&[move_to(outsider, laid.tracks[0], 22)])
        .unwrap();
    assert!(
        laid.project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
}

#[test]
fn a_clip_that_is_in_another_clip_group_does_not_join_this_one_and_is_not_moved_into_it() {
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10)], &[(0, 10), (20, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let other = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(2, 0), laid.clip_member(2, 1)])
        .unwrap();
    let in_the_other = laid.clip(2, 0);
    // (The other group has its clips on the third track at 0 to 10 and 20 to 30, so its rectangle is those frames of that track alone.)
    let before = laid.project.clone();

    assert_eq!(
        laid.project.move_clips(&[move_to(in_the_other, laid.tracks[0], 12)]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group,
                clip: in_the_other
            }
        ))
    );
    assert_eq!(laid.project, before);
    assert_eq!(members_of(&laid.project, other).len(), 2);
}

#[test]
fn a_clip_that_is_moved_into_the_rectangle_of_a_clip_group_that_is_in_another_is_put_in_the_innermost() {
    // An inner group of two clips (frames 0 to 30 of the first two tracks), in an outer group with a clip that is further along the first
    // track (40 to 50), so that the outer rectangle is frames 0 to 50 of those two tracks.
    let mut laid = lay_out(&[&[(0, 10), (40, 10)], &[(20, 10)], &[(0, 10), (60, 10)]]);
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(0, 1)])
        .unwrap();
    let outsider = laid.clip(2, 0);

    // On the second track at 0 to 10: in both rectangles, and in the inner group.
    laid.project
        .move_clips(&[move_to(outsider, laid.tracks[1], 0)])
        .unwrap();
    assert!(
        laid.project
            .group(inner)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
    assert!(
        !laid
            .project
            .group(outer)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
    assert!(
        laid.project.clip_is_inside(outsider, outer),
        "so it is inside the outer too"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);

    // On the second track at 32 to 42: in the outer rectangle (0 to 50) and not the inner one (0 to 30): it is in the outer group.
    laid.project
        .move_clips_remembering(
            &[move_to(outsider, laid.tracks[1], 32)],
            &[ClipGroupBeforeTheDrag {
                clip: outsider,
                group: None,
            }],
        )
        .unwrap();
    assert!(
        !laid
            .project
            .group(inner)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
    assert!(
        laid.project
            .group(outer)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_was_in_no_clip_group_when_the_drag_began_is_out_of_the_one_it_joined_when_it_is_moved_out_of_it() {
    let (mut laid, group, [first, second, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();
    let before_the_drag = laid.project.clip_groups_before_a_drag_of(&[outsider]);
    assert_eq!(
        before_the_drag,
        [ClipGroupBeforeTheDrag {
            clip: outsider,
            group: None
        }]
    );

    laid.project
        .move_clips_remembering(&[move_to(outsider, laid.tracks[0], 12)], &before_the_drag)
        .unwrap();
    assert!(
        laid.project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
    // Dragged on, past the end of the rectangle: it is out of the group again, and the rectangle is what it was.
    laid.project
        .move_clips_remembering(&[move_to(outsider, laid.tracks[0], 40)], &before_the_drag)
        .unwrap();

    assert_eq!(
        members_of(&laid.project, group),
        [Member::Clip(first), Member::Clip(second)]
    );
    assert_eq!(
        laid.project
            .rectangle_of_the_group(group)
            .map(|rectangle| (rectangle.first_frame, rectangle.end_frame)),
        Some((0, 30))
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_is_dragged_through_a_clip_group_is_in_it_while_it_is_in_its_rectangle_and_not_after() {
    let (mut laid, group, [_, _, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();
    let before_the_drag = laid.project.clip_groups_before_a_drag_of(&[outsider]);
    // Along the first track, frame by frame, from before the rectangle to after it (it is at 12 to 22 where there is room, and 10 to 20 is
    // the first place; before that, it is beside the first clip).
    let mut was_in_it = Vec::new();
    for start in [10, 12, 20, 25, 28, 29, 30, 35, 50] {
        laid.project
            .move_clips_remembering(&[move_to(outsider, laid.tracks[0], start)], &before_the_drag)
            .unwrap();
        was_in_it.push((
            start,
            laid.project
                .group(group)
                .unwrap()
                .has_as_a_member(Member::Clip(outsider)),
        ));
    }
    assert_eq!(
        was_in_it,
        [
            (10, true),
            (12, true),
            (20, true),
            (25, true),
            (28, true),
            (29, true),
            (30, false),
            (35, false),
            (50, false)
        ]
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_was_in_a_clip_group_when_the_drag_began_stays_in_it_wherever_it_is_dragged() {
    let (mut laid, group, [first, ..]) = a_clip_group_and_a_clip_that_is_not_in_it();
    let before_the_drag = laid.project.clip_groups_before_a_drag_of(&[first]);
    assert_eq!(
        before_the_drag,
        [ClipGroupBeforeTheDrag {
            clip: first,
            group: Some(group)
        }]
    );

    // Far from the rest of the group, on its own track: the rectangle follows it, as it always has.
    laid.project
        .move_clips_remembering(&[move_to(first, laid.tracks[0], 60)], &before_the_drag)
        .unwrap();

    assert!(laid.project.group(group).unwrap().has_as_a_member(Member::Clip(first)));
    assert_eq!(
        laid.project
            .rectangle_of_the_group(group)
            .map(|rectangle| (rectangle.first_frame, rectangle.end_frame)),
        Some((20, 70))
    );
}

#[test]
fn a_move_that_is_not_made_leaves_the_clip_in_the_group_it_was_in_and_what_was_joined_as_it_was() {
    let (mut laid, group, [_, _, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();
    let before_the_drag = laid.project.clip_groups_before_a_drag_of(&[outsider]);
    laid.project
        .move_clips_remembering(&[move_to(outsider, laid.tracks[0], 12)], &before_the_drag)
        .unwrap();
    let before = laid.project.clone();

    // On top of the clip that is on the first track at 0 to 10: the move is not made, and the clip is not taken out of the group either.
    assert!(
        laid.project
            .move_clips_remembering(&[move_to(outsider, laid.tracks[0], 5)], &before_the_drag)
            .is_err()
    );
    assert_eq!(laid.project, before);
    assert!(
        laid.project
            .group(group)
            .unwrap()
            .has_as_a_member(Member::Clip(outsider))
    );
}

#[test]
fn what_each_clip_was_in_is_told_for_the_clips_that_are_there_and_a_clip_that_is_not_is_left_out() {
    let (laid, group, [first, _, outsider]) = a_clip_group_and_a_clip_that_is_not_in_it();
    let gone = ClipId::new(crate::project::TrackId::new(70), 3);

    assert_eq!(
        laid.project.clip_groups_before_a_drag_of(&[first, gone, outsider]),
        [
            ClipGroupBeforeTheDrag {
                clip: first,
                group: Some(group)
            },
            ClipGroupBeforeTheDrag {
                clip: outsider,
                group: None
            }
        ]
    );
}

#[test]
fn clips_that_are_moved_in_a_project_that_has_no_clip_groups_are_moved_as_ever_and_tags_are_not_looked_at() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let clip = laid.clip(0, 0);

    laid.project.move_clips(&[move_to(clip, laid.tracks[1], 20)]).unwrap();

    assert_eq!(members_of(&laid.project, tag).len(), 2);
    assert_eq!(
        laid.project.groups().len(),
        1,
        "no clip group is made by a move, and a tag does not have clips put in it by where they are"
    );
}
