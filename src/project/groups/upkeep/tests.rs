// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the edits of clips and tracks do about groups: clips that are cut, copied or deleted are in and out of the groups, and an edit that
//! would break a rule is not made.

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out, video_source};
use crate::project::{
    Badge, ChainItemKey, ClipMove, ClipSide, GroupKind, GroupProblem, SpeedFactor, TextObject, TimelineFrameIndex,
    TrackSide,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn tag_badge(group: GroupId) -> ChainItemKey {
    ChainItemKey::Badge(Badge::Tag(group))
}

const TRACK_BADGE: ChainItemKey = ChainItemKey::Badge(Badge::Track);

fn chain_of_the_clip(project: &Project, clip: crate::project::ClipId) -> Vec<ChainItemKey> {
    project.chain_of_the_member(Member::Clip(clip)).unwrap().keys(true)
}

fn members_of(project: &Project, group: GroupId) -> Vec<Member> {
    project.group(group).unwrap().members().to_vec()
}

fn move_to(clip: crate::project::ClipId, to_track: crate::project::TrackId, start: u64) -> ClipMove {
    ClipMove {
        clip,
        to_track,
        start: frame(start),
    }
}

// --- deleting ---

#[test]
fn a_clip_that_is_deleted_is_taken_out_of_the_groups_it_is_in() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let (a, b) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[a, b, laid.clip_member(2, 0)])
        .unwrap();
    let clip_group = laid.project.create_group(GroupKind::ClipGroup, &[a, b]).unwrap();
    laid.project.delete_clip(laid.clip(0, 0)).unwrap();
    assert_eq!(members_of(&laid.project, tag), [b, laid.clip_member(2, 0)]);
    assert_eq!(members_of(&laid.project, clip_group), [b]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_group_that_has_nothing_left_when_its_last_clip_is_deleted_is_dropped_with_the_groups_that_only_it_was_in() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    let inner = laid.project.create_group(GroupKind::Tag, &[clip]).unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(inner)])
        .unwrap();
    let other = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(inner), laid.clip_member(1, 0)])
        .unwrap();
    laid.project.delete_clip(laid.clip(0, 0)).unwrap();
    assert!(laid.project.group(inner).is_none());
    assert!(laid.project.group(outer).is_none());
    assert_eq!(
        members_of(&laid.project, other),
        [laid.clip_member(1, 0)],
        "it had another in it too"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_track_that_is_deleted_is_taken_out_of_its_track_group_and_its_clips_out_of_theirs() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(1, 0), laid.clip_member(2, 0)])
        .unwrap();
    laid.project.delete_track(laid.tracks[1]).unwrap();
    assert_eq!(members_of(&laid.project, track_group), [laid.track_member(0)]);
    assert_eq!(members_of(&laid.project, tag), [laid.clip_member(2, 0)]);
    assert_eq!(laid.project.problems_with_the_groups(), []);

    laid.project.delete_track(laid.tracks[0]).unwrap();
    assert!(laid.project.group(track_group).is_none(), "its last track went");
}

#[test]
fn deleting_a_track_from_the_middle_of_a_track_group_keeps_the_group_together() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[laid.track_member(0), laid.track_member(1), laid.track_member(2)],
        )
        .unwrap();
    laid.project.delete_track(laid.tracks[1]).unwrap();
    assert_eq!(
        members_of(&laid.project, group),
        [laid.track_member(0), laid.track_member(2)]
    );
    assert_eq!(
        laid.project.problems_with_the_groups(),
        [],
        "the two that are left are next to each other now"
    );
}

// --- cutting ---

#[test]
fn the_second_half_of_a_clip_that_is_cut_is_in_the_groups_of_the_first_with_the_badges_in_the_same_places() {
    let mut laid = lay_out(&[&[(0, 20)], &[(0, 20)]]);
    let (a, b) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let first_tag = laid.project.create_group(GroupKind::Tag, &[a]).unwrap();
    laid.project
        .add_effect(
            crate::project::AnimationTarget::Clip(laid.clip(0, 0)),
            crate::project::EffectKind::HueShift,
        )
        .unwrap();
    let second_tag = laid.project.create_group(GroupKind::Tag, &[a, b]).unwrap();
    let clip_group = laid.project.create_group(GroupKind::ClipGroup, &[a, b]).unwrap();

    let cut = laid.project.split_at(frame(10), Some(&[laid.clip(0, 0)]));
    assert_eq!(cut.len(), 1);
    let (first_half, second_half) = (cut[0].1.first_half, cut[0].1.second_half);
    assert_eq!(first_half, laid.clip(0, 0));
    let second = Member::Clip(second_half);

    assert_eq!(
        members_of(&laid.project, first_tag),
        [a, second],
        "right after the first half"
    );
    assert_eq!(members_of(&laid.project, second_tag), [a, second, b]);
    assert_eq!(members_of(&laid.project, clip_group), [a, second, b]);
    assert_eq!(
        chain_of_the_clip(&laid.project, second_half),
        chain_of_the_clip(&laid.project, first_half),
        "the same chain: the badges are where they are in the first half's"
    );
    assert_eq!(
        chain_of_the_clip(&laid.project, second_half),
        [
            tag_badge(first_tag),
            ChainItemKey::Effect(crate::project::EffectInstanceId::FIRST),
            tag_badge(second_tag),
            TRACK_BADGE
        ]
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
    assert_eq!(
        laid.project.rectangle_of_the_group(clip_group).unwrap().tracks(),
        0..=1,
        "a cut changes no rectangle"
    );
}

#[test]
fn a_cut_where_no_clip_can_be_cut_leaves_the_groups_as_they_are() {
    let mut laid = lay_out(&[&[(10, 20)]]);
    laid.project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert!(
        laid.project.split_at(frame(10), None).is_empty(),
        "where the clip starts"
    );
    assert!(laid.project.split_at(frame(5), None).is_empty(), "where nothing is");
    assert_eq!(laid.project, before);
}

// --- copying a clip ---

#[test]
fn the_copy_of_a_clip_is_in_the_groups_of_the_clip_with_the_badges_where_the_clip_has_them() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 30)]]);
    let (a, b) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let tag = laid.project.create_group(GroupKind::Tag, &[a]).unwrap();
    let clip_group = laid.project.create_group(GroupKind::ClipGroup, &[a, b]).unwrap();
    let copy = laid.project.duplicate_clip(laid.clip(0, 0), ClipSide::Right).unwrap();
    assert_eq!(members_of(&laid.project, tag), [a, Member::Clip(copy)]);
    assert_eq!(members_of(&laid.project, clip_group), [a, Member::Clip(copy), b]);
    assert_eq!(chain_of_the_clip(&laid.project, copy), [tag_badge(tag), TRACK_BADGE]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_copy_that_would_make_the_rectangle_of_a_clip_group_take_in_a_clip_that_is_not_in_it_is_not_made() {
    // The group is the clips at frames 0 to 10 of the tracks 0 and 1; a clip that is not in it is on track 1 at frames 12 to 18, just
    // after it. A copy of the clip on track 0 to its right (frames 10 to 20) would make the rectangle reach frame 20, and so take it in.
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10), (12, 6)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert!(
        !laid.project.room_for_a_copy_of(laid.clip(0, 0), ClipSide::Right),
        "the menu says so"
    );
    assert_eq!(
        laid.project.duplicate_clip(laid.clip(0, 0), ClipSide::Right),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group: GroupId::new(0),
                clip: laid.clip(1, 1)
            }
        ))
    );
    assert_eq!(laid.project, before, "nothing was changed");
}

#[test]
fn there_is_room_for_a_copy_when_it_breaks_no_rule_and_none_when_there_is_none_on_the_track() {
    let mut laid = lay_out(&[&[(0, 10), (10, 10)], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    assert!(
        !laid.project.room_for_a_copy_of(laid.clip(0, 0), ClipSide::Right),
        "the clip after it is in the way"
    );
    assert!(
        laid.project.room_for_a_copy_of(laid.clip(0, 1), ClipSide::Right),
        "the rectangle grows to take in the copy, which is the only thing there"
    );
    assert!(
        !laid.project.room_for_a_copy_of(laid.clip(0, 0), ClipSide::Left),
        "it begins at the start of the timeline"
    );
}

// --- copying a track ---

#[test]
fn the_copy_of_a_track_is_in_the_track_group_the_track_is_in_and_its_clips_in_the_groups_of_their_originals() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let (b, c) = (laid.clip_member(1, 0), laid.clip_member(2, 0));
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let tag = laid.project.create_group(GroupKind::Tag, &[b, c]).unwrap();
    let copy = laid.project.duplicate_track(laid.tracks[1], TrackSide::Below).unwrap();

    assert_eq!(
        members_of(&laid.project, track_group),
        [laid.track_member(0), laid.track_member(1), Member::Track(copy)],
        "right after the track it is a copy of"
    );
    let clip_of_the_copy = laid.project.track(copy).unwrap().entries()[0].id();
    assert_eq!(members_of(&laid.project, tag), [b, Member::Clip(clip_of_the_copy), c]);
    assert_eq!(
        chain_of_the_clip(&laid.project, clip_of_the_copy),
        [tag_badge(tag), TRACK_BADGE]
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
    assert_eq!(laid.project.track_position(copy), Some(2), "below the track");
}

#[test]
fn the_copy_of_a_track_beside_a_track_group_that_the_track_is_not_in_is_not_in_it() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(1), laid.track_member(2)])
        .unwrap();
    let copy = laid.project.duplicate_track(laid.tracks[0], TrackSide::Below).unwrap();
    assert!(!laid.project.group(group).unwrap().has_as_a_member(Member::Track(copy)));
    assert_eq!(laid.project.track_position(copy), Some(1), "just before the group");
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn the_clips_of_the_copy_of_a_track_are_in_the_clip_groups_of_their_originals_when_that_breaks_no_rule() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let copy = laid.project.duplicate_track(laid.tracks[1], TrackSide::Above).unwrap();
    let clip_of_the_copy = laid.project.track(copy).unwrap().entries()[0].id();
    assert!(
        laid.project
            .group(clip_group)
            .unwrap()
            .has_as_a_member(Member::Clip(clip_of_the_copy))
    );
    assert_eq!(
        laid.project.track_position(copy),
        Some(1),
        "between the two tracks of the group, as its clip is"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

// --- moving clips ---

/// Three tracks: the clips of a clip group are on the first at frames 0 to 10 and on the second at frames 20 to 30, and so its rectangle
/// is frames 0 to 30 of those two tracks, in which the clips that are not in it are the one on the third track (to be moved there).
fn a_clip_group_with_a_clip_to_move() -> (Laid, GroupId) {
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    (laid, group)
}

#[test]
fn a_clip_that_is_in_a_clip_group_can_be_moved_inside_its_rectangle_and_the_rectangle_follows_it() {
    let (mut laid, group) = a_clip_group_with_a_clip_to_move();
    laid.project
        .move_clips(&[move_to(laid.clip(0, 0), laid.tracks[0], 15)])
        .unwrap();
    let rectangle = laid.project.rectangle_of_the_group(group).unwrap();
    assert_eq!(rectangle.first_frame, 15, "it starts where its first clip does now");
    assert_eq!(rectangle.end_frame, 30);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_that_is_in_a_clip_group_cannot_be_moved_so_that_the_rectangle_takes_in_a_clip_that_is_not_in_it() {
    let (mut laid, group) = a_clip_group_with_a_clip_to_move();
    // The clip on the third track is not in the rectangle, which reaches the first two tracks. The clip of the group that is on the second
    // track is moved to the third (at frames 20 to 30, where the other has nothing): then the rectangle reaches the third track too.
    let outsider = laid.clip(2, 0);
    let before = laid.project.clone();
    assert_eq!(
        laid.project.move_clips(&[move_to(laid.clip(1, 0), laid.tracks[2], 20)]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip: outsider }
        ))
    );
    assert_eq!(laid.project, before);

    laid.project
        .move_clips(&[move_to(laid.clip(1, 0), laid.tracks[1], 40)])
        .unwrap();
    assert_eq!(
        laid.project.rectangle_of_the_group(group).unwrap().end_frame,
        50,
        "on its own track it only makes the rectangle longer"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn clips_that_swap_places_are_checked_as_they_are_after_the_move_and_not_while_they_are_in_the_air() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    laid.project
        .move_clips(&[
            move_to(laid.clip(0, 0), laid.tracks[1], 20),
            move_to(laid.clip(1, 0), laid.tracks[0], 20),
        ])
        .unwrap();
    assert_eq!(laid.project.rectangle_of_the_group(clip_group).unwrap().first_frame, 20);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn moving_clips_when_there_are_no_groups_is_what_it_was() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    laid.project
        .move_clips(&[move_to(laid.clip(0, 0), laid.tracks[1], 20)])
        .unwrap();
    assert_eq!(
        laid.project
            .placement_of(laid.clip(0, 0))
            .unwrap()
            .first_timeline_frame
            .value(),
        20
    );
    assert_eq!(
        laid.project.move_clips(&[move_to(laid.clip(0, 0), laid.tracks[1], 5)]),
        Err(EditError::Clip(crate::project::ClipEditError::WouldOverlap {
            clip: laid.clip(0, 0)
        })),
        "the same error as ever: it is the overlap that is found, not a group (the clip that starts inside the other is named)"
    );
}

// --- moving a track ---

#[test]
fn a_track_that_is_not_in_a_track_group_cannot_be_moved_into_the_middle_of_it_and_one_that_is_cannot_be_moved_out_of_the_middle()
 {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(1), laid.track_member(2)])
        .unwrap();
    let before = laid.project.clone();
    assert!(
        matches!(
            laid.project.move_track(laid.tracks[0], 1),
            Err(EditError::BreaksTheRulesOfGroups(
                GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt { .. }
            ))
        ),
        "between the two tracks of the group"
    );
    assert!(
        laid.project.move_track(laid.tracks[1], 3).is_err(),
        "a track of the group out to the back leaves the other one apart"
    );
    assert_eq!(laid.project, before);

    laid.project.move_track(laid.tracks[3], 1).unwrap();
    assert_eq!(
        laid.project.track_position(laid.tracks[3]),
        Some(1),
        "next to the group, before it, is fine"
    );
    laid.project.move_track(laid.tracks[1], 0).unwrap_err();
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_track_cannot_be_moved_into_the_rectangle_of_a_clip_group_when_its_clips_would_be_inside_it() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert!(
        laid.project.move_track(laid.tracks[2], 1).is_err(),
        "its clip would be between the two"
    );
    assert_eq!(laid.project, before);
}

// --- how long things last ---

#[test]
fn a_clip_cannot_be_made_to_last_so_long_that_the_rectangle_of_its_group_takes_in_another() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10), (12, 6)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project.set_clip_duration(laid.clip(0, 0), 15),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group,
                clip: laid.clip(1, 1)
            }
        ))
    );
    assert_eq!(laid.project, before);
    assert_eq!(
        laid.project.set_clip_duration(laid.clip(0, 0), 11),
        Ok(11),
        "this far does not reach it"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

/// A video of 60 frames that lasts 30 on the back track, because it plays at twice its speed (or because its track does, if
/// `the_track_is_sped_up`), with a text object of the same clip group on the front track at frames 0 to 10 and one that is not in the
/// group on the front track at frames 30 to 40, just after the rectangle of the group (which is frames 0 to 30 of the two tracks).
fn a_video_that_lasts_30_frames_in_a_clip_group(
    the_track_is_sped_up: bool,
) -> (Project, crate::project::ClipId, crate::project::ClipId, GroupId) {
    let mut project = Project::empty();
    let video = project
        .import_source(video_source(1, 60), frame(0))
        .unwrap()
        .clip
        .expect("the video has frames");
    let twice = SpeedFactor::new(2).unwrap();
    if the_track_is_sped_up {
        let track = project.track_holding(video).unwrap().id();
        project.set_track_speed(track, twice).unwrap();
    } else {
        project.set_clip_speed(video, twice).unwrap();
    }
    let in_front = project.add_track_in_front();
    let (member, _) = project
        .insert_text(in_front, frame(0), TextObject::default(), 10)
        .unwrap();
    let (outsider, _) = project
        .insert_text(in_front, frame(30), TextObject::default(), 10)
        .unwrap();
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(video), Member::Clip(member)])
        .unwrap();
    assert_eq!(project.rectangle_of_the_group(group).unwrap().end_frame, 30);
    (project, video, outsider, group)
}

#[test]
fn a_clip_cannot_be_slowed_down_so_that_the_rectangle_of_its_group_takes_in_another_clip() {
    let (mut project, video, outsider, group) = a_video_that_lasts_30_frames_in_a_clip_group(false);
    let before = project.clone();
    assert_eq!(
        project.set_clip_speed(video, SpeedFactor::NORMAL),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip: outsider }
        )),
        "at normal speed it lasts 60 frames, and the clip on the front track at 30 to 40 is inside"
    );
    assert_eq!(project, before);
    project.set_clip_speed(video, SpeedFactor::new(4).unwrap()).unwrap();
    assert_eq!(project.problems_with_the_groups(), [], "faster is shorter");
}

#[test]
fn a_track_cannot_be_slowed_down_so_that_the_rectangle_of_a_clip_group_takes_in_another_clip() {
    let (mut project, video, outsider, group) = a_video_that_lasts_30_frames_in_a_clip_group(true);
    let track = project.track_holding(video).unwrap().id();
    let before = project.clone();
    assert_eq!(
        project.set_track_speed(track, SpeedFactor::NORMAL),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip: outsider }
        ))
    );
    assert_eq!(project, before);
    project.set_track_speed(track, SpeedFactor::new(4).unwrap()).unwrap();
    assert_eq!(project.problems_with_the_groups(), []);
}

// --- making clips ---

#[test]
fn a_text_object_cannot_be_put_inside_the_rectangle_of_a_clip_group_and_can_be_put_beside_it() {
    let mut laid = lay_out(&[&[(0, 10)], &[(20, 10)], &[]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project
            .insert_text(laid.tracks[0], frame(12), TextObject::default(), 5),
        None,
        "on a track of the group, between its clips"
    );
    assert_eq!(laid.project, before);
    assert!(
        laid.project
            .insert_text(laid.tracks[0], frame(30), TextObject::default(), 5)
            .is_some(),
        "after it"
    );
    assert!(
        laid.project
            .insert_text(laid.tracks[2], frame(12), TextObject::default(), 5)
            .is_some(),
        "on a track it does not reach"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_file_is_put_on_an_empty_track_that_is_not_in_the_rectangle_of_a_clip_group_or_on_a_new_track_in_front() {
    // The first empty track, the one in the middle, is in the rectangle of the group at frame 0: not there.
    let mut laid = lay_out(&[&[(0, 10)], &[], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
        .unwrap();
    let outcome = laid.project.import_source(video_source(1, 60), frame(0)).unwrap();
    assert!(outcome.created_track, "a new track, in front");
    assert_eq!(laid.project.track_position(outcome.track), Some(0));
    assert_eq!(laid.project.problems_with_the_groups(), []);

    // Further along than the rectangle reaches, the middle track will do.
    let mut laid = lay_out(&[&[(0, 10)], &[], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
        .unwrap();
    let outcome = laid.project.import_source(video_source(1, 60), frame(10)).unwrap();
    assert!(!outcome.created_track);
    assert_eq!(outcome.track, laid.tracks[1]);
}

#[test]
fn a_file_is_put_on_the_first_empty_track_when_there_are_no_groups_as_it_always_was() {
    let mut laid = lay_out(&[&[(0, 10)], &[], &[(0, 10)]]);
    let outcome = laid.project.import_source(video_source(1, 60), frame(0)).unwrap();
    assert!(!outcome.created_track);
    assert_eq!(outcome.track, laid.tracks[1]);
}

// --- counting again at another frame rate ---

#[test]
fn counting_again_at_another_frame_rate_is_refused_when_rounding_would_put_a_clip_inside_the_rectangle_of_a_group() {
    // At 30 frames a second: the group is a clip on track 0 at frames 1 to 3 and a clip on track 1 at frames 0 to 1, and a clip that is
    // not in it is on track 1 at frames 3 to 4, just after the rectangle. At 24 frames a second the clips are at 1 to 3, at 0 to 1 and at
    // 2 to 3: where the clip that is not in the group starts, the rectangle (frames 0 to 3) has not ended.
    let mut laid = lay_out(&[&[(1, 2)], &[(0, 1), (3, 1)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    assert_eq!(laid.project.problems_with_the_groups(), []);
    let rate = crate::project::FrameRate::new(24, 1).unwrap();

    let mut without_looking = laid.project.clone();
    without_looking.set_frame_rate(rate);
    assert_eq!(
        without_looking.problems_with_the_groups(),
        [GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
            group: GroupId::new(0),
            clip: laid.clip(1, 1)
        }],
        "this is what `set_frame_rate` does, which does not look"
    );

    let before = laid.project.clone();
    assert!(matches!(
        laid.project.try_set_frame_rate(rate),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt { .. }
        ))
    ));
    assert_eq!(laid.project, before, "the project keeps its frame rate");
}

#[test]
fn counting_again_at_another_frame_rate_is_made_when_no_rule_is_broken() {
    let mut laid = lay_out(&[&[(0, 30)], &[(0, 30)]]);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    laid.project
        .try_set_frame_rate(crate::project::FrameRate::new(60, 1).unwrap())
        .unwrap();
    assert_eq!(
        laid.project.frame_rate(),
        crate::project::FrameRate::new(60, 1).unwrap()
    );
    assert_eq!(
        laid.project
            .placement_of(laid.clip(0, 0))
            .unwrap()
            .end_timeline_frame
            .value(),
        60
    );
}

// --- looking before doing ---

#[test]
fn what_would_be_allowed_is_found_out_without_doing_it() {
    let (laid, _) = a_clip_group_with_a_clip_to_move();
    let outsider = laid.clip(2, 0);
    let before = laid.project.clone();
    // (The clip of the group that is on the second track, moved to the third: the rectangle would take in the clip that is not in it.)
    assert!(
        laid.project
            .would_be_allowed(|project| project.move_clips(&[move_to(laid.clip(1, 0), laid.tracks[2], 20)]))
            .is_err()
    );
    assert!(
        laid.project
            .would_be_allowed(|project| project.move_clips(&[move_to(outsider, laid.tracks[0], 30)]))
            .is_ok()
    );
    assert_eq!(laid.project, before, "the project itself was not touched either time");
}

#[test]
fn edits_that_cannot_break_a_rule_are_made_when_there_are_groups_as_they_are_when_there_are_none() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    laid.project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project.set_track_title(laid.tracks[0], "titles").unwrap();
    assert_eq!(laid.project.track(laid.tracks[0]).unwrap().title(), "titles");
    laid.project
        .add_effect(
            crate::project::AnimationTarget::Track(laid.tracks[1]),
            crate::project::EffectKind::HueShift,
        )
        .unwrap();
    let added = laid.project.add_track_in_front();
    assert_eq!(laid.project.track_position(added), Some(0));
    assert_eq!(laid.project.problems_with_the_groups(), []);
}
