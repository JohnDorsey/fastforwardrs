// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asking a project about its groups.

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{GroupColour, GroupProblem};

/// Five tracks with one clip on each, from the front: the clip of the track at place `n` is `laid.clip(n, 0)`.
fn five_tracks_with_a_clip_each() -> Laid {
    lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]])
}

#[test]
fn the_groups_are_listed_in_the_order_they_were_made_in_and_found_by_their_names() {
    let mut laid = five_tracks_with_a_clip_each();
    assert!(laid.project.groups().is_empty());
    let first = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(1, 0)])
        .unwrap();
    assert_eq!(
        laid.project.groups().iter().map(Group::id).collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(laid.project.group(second).map(Group::name), Some("Tag 2"));
    assert!(laid.project.group(GroupId::new(99)).is_none());
}

#[test]
fn the_project_has_what_a_member_is_when_it_is_there() {
    let mut laid = five_tracks_with_a_clip_each();
    let group = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    assert!(laid.project.has_the_member(laid.clip_member(4, 0)));
    assert!(laid.project.has_the_member(laid.track_member(2)));
    assert!(laid.project.has_the_member(Member::Group(group)));
    assert!(!laid.project.has_the_member(Member::Group(GroupId::new(9))));
    assert!(!laid.project.has_the_member(Member::Track(TrackId::new(99))));
    assert!(
        !laid
            .project
            .has_the_member(Member::Clip(ClipId::new(TrackId::new(0), 99)))
    );
}

#[test]
fn the_clip_group_or_track_group_a_thing_is_in_is_the_one_that_holds_it_and_a_tag_is_not_that() {
    let mut laid = five_tracks_with_a_clip_each();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(3), laid.track_member(4)])
        .unwrap();

    let project = &laid.project;
    assert_eq!(
        project.shape_group_of(laid.clip_member(0, 0)).map(Group::id),
        Some(clip_group)
    );
    assert_eq!(
        project.shape_group_of(laid.track_member(4)).map(Group::id),
        Some(track_group)
    );
    assert!(project.shape_group_of(laid.clip_member(2, 0)).is_none());
    assert!(project.shape_group_of(Member::Group(tag)).is_none());
    assert!(
        project.shape_group_of(laid.clip_member(4, 0)).is_none(),
        "a clip on a track of a track group is inside the group, not in it"
    );

    assert_eq!(
        project
            .tags_of(laid.clip_member(0, 0))
            .iter()
            .map(|group| group.id())
            .collect::<Vec<_>>(),
        [tag]
    );
    assert!(project.tags_of(laid.clip_member(2, 0)).is_empty());
    assert_eq!(
        project
            .groups_holding(laid.clip_member(0, 0))
            .iter()
            .map(|group| group.id())
            .collect::<Vec<_>>(),
        [tag, clip_group],
        "every group that has it in it, in the order the groups were made in"
    );
}

#[test]
fn a_thing_is_in_a_clip_group_when_it_is_in_it_or_in_a_clip_group_that_is_in_it() {
    let mut laid = five_tracks_with_a_clip_each();
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)])
        .unwrap();
    let project = &laid.project;
    assert!(project.is_in_the_shape(laid.clip_member(0, 0), inner));
    assert!(
        project.is_in_the_shape(laid.clip_member(0, 0), outer),
        "through the clip group it is in"
    );
    assert!(project.is_in_the_shape(laid.clip_member(2, 0), outer));
    assert!(
        !project.is_in_the_shape(laid.clip_member(2, 0), inner),
        "it is only in the outer one"
    );
    assert!(!project.is_in_the_shape(laid.clip_member(3, 0), outer));
    assert!(project.is_in_the_shape(Member::Group(inner), outer));
    assert!(!project.is_in_the_shape(Member::Group(outer), inner));
}

#[test]
fn a_clip_is_inside_a_clip_group_a_track_group_through_its_track_and_a_tag_by_being_in_a_group_in_it() {
    let mut laid = five_tracks_with_a_clip_each();
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(3), laid.track_member(4)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(clip_group), laid.clip_member(2, 0)])
        .unwrap();
    let project = &laid.project;

    assert!(project.clip_is_inside(laid.clip(0, 0), clip_group));
    assert!(!project.clip_is_inside(laid.clip(2, 0), clip_group));
    assert!(
        project.clip_is_inside(laid.clip(3, 0), track_group),
        "it is on a track of the group"
    );
    assert!(project.clip_is_inside(laid.clip(4, 0), track_group));
    assert!(!project.clip_is_inside(laid.clip(2, 0), track_group));
    assert!(
        project.clip_is_inside(laid.clip(1, 0), tag),
        "in a group that is in the tag"
    );
    assert!(project.clip_is_inside(laid.clip(2, 0), tag), "in the tag");
    assert!(!project.clip_is_inside(laid.clip(4, 0), tag));
    assert!(
        !project.clip_is_inside(laid.clip(0, 0), GroupId::new(99)),
        "no such group"
    );
}

#[test]
fn the_clips_in_a_group_are_those_in_it_and_in_the_groups_in_it_each_once_and_for_a_track_group_all_that_are_on_its_tracks()
 {
    let mut laid = lay_out(&[&[(0, 10), (20, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(2), laid.track_member(3)])
        .unwrap();
    let tag = laid
        .project
        .create_group(
            GroupKind::Tag,
            &[
                Member::Group(clip_group),
                laid.clip_member(1, 0),
                Member::Group(track_group),
            ],
        )
        .unwrap();
    let project = &laid.project;

    assert_eq!(project.clips_in(clip_group), [laid.clip(0, 0), laid.clip(1, 0)]);
    assert_eq!(project.clips_in(track_group), [laid.clip(2, 0), laid.clip(3, 0)]);
    assert_eq!(
        project.clips_in(tag),
        [laid.clip(0, 0), laid.clip(1, 0), laid.clip(2, 0), laid.clip(3, 0)],
        "the clip that is in the tag and in the clip group in it is there once"
    );
    assert!(project.clips_in(GroupId::new(99)).is_empty());
}

#[test]
fn the_tracks_in_a_group_are_those_in_it_and_in_the_track_groups_in_it() {
    let mut laid = five_tracks_with_a_clip_each();
    let inner = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(1), laid.track_member(2)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), Member::Group(inner)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(outer)])
        .unwrap();
    let project = &laid.project;

    assert_eq!(project.tracks_in(inner), [laid.tracks[1], laid.tracks[2]]);
    assert_eq!(
        project.tracks_in(outer),
        [laid.tracks[0], laid.tracks[1], laid.tracks[2]]
    );
    assert_eq!(
        project.tracks_in(tag),
        project.tracks_in(outer),
        "a tag has the tracks of the groups in it"
    );
    assert!(project.tracks_in(GroupId::new(99)).is_empty());
}

#[test]
fn the_groups_round_a_clip_are_the_clip_groups_it_is_in_and_then_the_track_groups_its_track_is_in_the_nearest_first() {
    // The clip groups are over the first three tracks, and so the track groups have to be at least that wide: a clip group cannot be
    // partly in a track group.
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let inner_clips = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer_clips = laid
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Group(inner_clips), laid.clip_member(2, 0)],
        )
        .unwrap();
    let inner_tracks = laid
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[laid.track_member(0), laid.track_member(1), laid.track_member(2)],
        )
        .unwrap();
    let outer_tracks = laid
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Group(inner_tracks), laid.track_member(3)],
        )
        .unwrap();
    let _tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();

    let project = &laid.project;
    assert_eq!(
        project.groups_round_the_clip(laid.clip(0, 0)),
        [inner_clips, outer_clips, inner_tracks, outer_tracks],
        "a tag is not round a clip"
    );
    assert_eq!(
        project.groups_round_the_clip(laid.clip(2, 0)),
        [outer_clips, inner_tracks, outer_tracks]
    );
    assert_eq!(project.groups_round_the_clip(laid.clip(3, 0)), [outer_tracks]);
    assert!(
        project
            .groups_round_the_clip(ClipId::new(TrackId::new(99), 0))
            .is_empty()
    );
}

#[test]
fn the_chain_of_a_thing_is_the_chain_of_a_clip_a_track_or_a_group_and_none_if_it_is_not_there() {
    let mut laid = five_tracks_with_a_clip_each();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project
        .add_effect_to_the_group(tag, crate::project::EffectKind::HueShift)
        .unwrap();
    let project = &laid.project;

    let of_the_clip = project.chain_of_the_member(laid.clip_member(0, 0)).unwrap();
    assert!(of_the_clip.has_the_tag_badge(tag));
    assert!(
        project
            .chain_of_the_member(laid.clip_member(1, 0))
            .unwrap()
            .has_nothing_in_it()
    );
    assert!(
        project
            .chain_of_the_member(laid.track_member(0))
            .unwrap()
            .has_nothing_in_it()
    );
    assert_eq!(project.chain_of_the_member(Member::Group(tag)).unwrap().len(), 1);
    assert!(project.chain_of_the_member(Member::Group(GroupId::new(9))).is_none());
    assert!(project.chain_of_the_member(Member::Track(TrackId::new(9))).is_none());
}

/// A project whose groups are in themselves, which the rules do not allow (and `with_groups` refuses): that what looks down through
/// the groups ends all the same.
#[test]
fn looking_down_through_groups_that_are_in_each_other_ends() {
    let mut laid = five_tracks_with_a_clip_each();
    let (first, second) = (GroupId::new(0), GroupId::new(1));
    laid.project.groups = vec![
        Group::new(first, GroupKind::Tag, "a", GroupColour::PALETTE[0])
            .with_members([Member::Group(second), laid.clip_member(0, 0)]),
        Group::new(second, GroupKind::Tag, "b", GroupColour::PALETTE[1])
            .with_members([Member::Group(first), laid.clip_member(1, 0)]),
    ];
    let project = &laid.project;
    let mut clips = project.clips_in(first);
    clips.sort();
    let mut both = [laid.clip(0, 0), laid.clip(1, 0)];
    both.sort();
    assert_eq!(clips, both, "each clip once, however many times round it is");
    assert!(project.tracks_in(first).is_empty());
    assert!(project.rectangle_of_the_group(first).is_some());
    assert!(
        project
            .problems_with_the_groups()
            .iter()
            .any(|problem| matches!(problem, GroupProblem::IsInItself { .. })),
        "and the rules say so"
    );
}
