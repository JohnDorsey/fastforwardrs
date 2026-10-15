// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rules that groups keep: one test for each, with a project that breaks only that rule.
//!
//! The groups that break rules are put into the project as they are (the editing would refuse to make them): see [`Laid::put_in`].

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{Badge, GroupColour};

fn problems(laid: &Laid) -> Vec<GroupProblem> {
    laid.project.problems_with_the_groups()
}

fn group(number: u32) -> GroupId {
    GroupId::new(number)
}

// --- nothing, and what is as it should be ---

#[test]
fn a_project_with_no_groups_has_nothing_wrong_with_its_groups() {
    let laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    assert!(problems(&laid).is_empty());
    assert_eq!(laid.project.first_problem_with_the_groups(), None);
}

#[test]
fn groups_that_are_made_by_the_editing_in_every_way_it_allows_break_no_rule() {
    // Five tracks; the clip groups are over the first three, the track groups cover the first four.
    let mut laid = lay_out(&[&[(0, 10), (50, 10)], &[(5, 10)], &[(20, 10)], &[(0, 100)], &[(0, 30)]]);
    let project = &mut laid.project;
    let (a, b, c) = (laid.clips[0][0], laid.clips[1][0], laid.clips[2][0]);
    let inner = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let outer = project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), Member::Clip(c)])
        .unwrap();
    let inner_tracks = project
        .create_group(
            GroupKind::TrackGroup,
            &[
                Member::Track(laid.tracks[0]),
                Member::Track(laid.tracks[1]),
                Member::Track(laid.tracks[2]),
            ],
        )
        .unwrap();
    let outer_tracks = project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Group(inner_tracks), Member::Track(laid.tracks[3])],
        )
        .unwrap();
    // Tags: of clips that have clips between them, of groups of all the kinds, and of tags.
    let tag = project
        .create_group(
            GroupKind::Tag,
            &[
                Member::Clip(laid.clips[0][1]),
                Member::Group(outer),
                Member::Group(outer_tracks),
            ],
        )
        .unwrap();
    project
        .create_group(GroupKind::Tag, &[Member::Group(tag), Member::Clip(laid.clips[4][0])])
        .unwrap();
    assert_eq!(project.problems_with_the_groups(), []);
}

// --- what is in a group ---

#[test]
fn a_group_that_has_nothing_in_it_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)]]);
    let empty = laid.put_in(GroupKind::Tag, &[]);
    assert_eq!(problems(&laid), [GroupProblem::HasNothingInIt { group: empty }]);
}

#[test]
fn a_group_that_has_what_the_project_does_not_have_in_it_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)]]);
    let no_clip = Member::Clip(crate::project::ClipId::new(laid.tracks[0], 99));
    let no_track = Member::Track(crate::project::TrackId::new(99));
    let no_group = Member::Group(group(77));
    let tag = laid.put_in(GroupKind::Tag, &[no_clip, laid.clip_member(0, 0), no_group]);
    let track_group = laid.put_in(GroupKind::TrackGroup, &[no_track]);
    assert_eq!(
        problems(&laid),
        [
            GroupProblem::HasAMemberThatIsNotThere {
                group: tag,
                member: no_clip
            },
            GroupProblem::HasAMemberThatIsNotThere {
                group: tag,
                member: no_group
            },
            GroupProblem::HasAMemberThatIsNotThere {
                group: track_group,
                member: no_track
            },
        ]
    );
}

#[test]
fn a_group_that_has_a_thing_in_it_twice_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    let tag = laid.put_in(GroupKind::Tag, &[clip, clip]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::HasAMemberTwice {
            group: tag,
            member: clip
        }]
    );
}

#[test]
fn a_group_that_has_what_a_group_of_its_kind_cannot_hold_in_it_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let (clip, track) = (laid.clip_member(0, 0), laid.track_member(1));
    let tag = laid.put_in(GroupKind::Tag, &[clip]);
    let clip_group_of_tag = laid.put_in(GroupKind::ClipGroup, &[clip, Member::Group(tag)]);
    let tag_with_a_track = laid.put_in(GroupKind::Tag, &[track]);
    let track_group_with_a_clip = laid.put_in(GroupKind::TrackGroup, &[track, clip]);
    let clip_group_of_a_track_group = laid.put_in(GroupKind::ClipGroup, &[Member::Group(track_group_with_a_clip)]);
    let found = problems(&laid);

    let cannot_hold =
        |group: GroupId, kind: GroupKind, member: Member| GroupProblem::CannotHold { group, kind, member };
    for expected in [
        cannot_hold(clip_group_of_tag, GroupKind::ClipGroup, Member::Group(tag)),
        cannot_hold(tag_with_a_track, GroupKind::Tag, track),
        cannot_hold(track_group_with_a_clip, GroupKind::TrackGroup, clip),
        cannot_hold(
            clip_group_of_a_track_group,
            GroupKind::ClipGroup,
            Member::Group(track_group_with_a_clip),
        ),
    ] {
        assert!(found.contains(&expected), "{expected:?} is not among {found:?}");
    }
    assert!(
        found.iter().all(|problem| matches!(
            problem,
            GroupProblem::CannotHold { .. } | GroupProblem::InTwoShapes { .. }
        )),
        "nothing else is wrong: {found:?}"
    );
}

#[test]
fn a_tag_can_hold_groups_of_every_kind_and_a_clip_group_or_a_track_group_only_its_own() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip_group = laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(0, 0)]);
    let track_group = laid.put_in(GroupKind::TrackGroup, &[laid.track_member(1)]);
    let inner_tag = laid.put_in(GroupKind::Tag, &[laid.clip_member(1, 0)]);
    laid.put_in(
        GroupKind::Tag,
        &[
            Member::Group(clip_group),
            Member::Group(track_group),
            Member::Group(inner_tag),
        ],
    );
    assert_eq!(problems(&laid), []);
}

#[test]
fn a_thing_that_is_in_two_clip_groups_or_two_track_groups_breaks_a_rule_and_one_in_a_tag_and_a_clip_group_does_not() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    let first = laid.put_in(GroupKind::ClipGroup, &[clip]);
    let second = laid.put_in(GroupKind::ClipGroup, &[clip]);
    laid.put_in(GroupKind::Tag, &[clip]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::InTwoShapes {
            member: clip,
            first,
            second
        }],
        "the tag is no matter"
    );

    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let track = laid.track_member(0);
    let first = laid.put_in(GroupKind::TrackGroup, &[track]);
    let second = laid.put_in(GroupKind::TrackGroup, &[track]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::InTwoShapes {
            member: track,
            first,
            second
        }]
    );
}

#[test]
fn a_group_that_is_in_itself_breaks_a_rule_however_many_groups_there_are_between() {
    let mut laid = lay_out(&[&[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    // Three tags, each in the next, the last in the first.
    let (first, second, third) = (group(0), group(1), group(2));
    laid.put_in(GroupKind::Tag, &[clip, Member::Group(second)]);
    laid.put_in(GroupKind::Tag, &[Member::Group(third)]);
    laid.put_in(GroupKind::Tag, &[Member::Group(first)]);
    let found = problems(&laid);
    assert!(
        found
            .iter()
            .any(|problem| matches!(problem, GroupProblem::IsInItself { .. })),
        "{found:?}"
    );

    let mut laid = lay_out(&[&[(0, 10)]]);
    let alone = laid.put_in(GroupKind::Tag, &[Member::Group(group(0))]);
    assert!(
        problems(&laid).contains(&GroupProblem::IsInItself { group: alone }),
        "a group that has itself in it"
    );
}

// --- the badges ---

#[test]
fn a_member_of_a_tag_that_has_no_badge_of_it_in_its_chain_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    let tag = group(0);
    laid.project
        .groups
        .push(Group::new(tag, GroupKind::Tag, "tag", GroupColour::PALETTE[0]).with_members([clip]));
    assert_eq!(
        problems(&laid),
        [GroupProblem::HasNoBadgeOfTheGroupItIsIn {
            member: clip,
            group: tag
        }]
    );
    laid.project.put_the_badge_of_a_tag_in_the_chain_of(clip, tag);
    assert_eq!(problems(&laid), []);
}

#[test]
fn a_badge_of_a_tag_that_what_has_the_chain_is_not_in_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let (in_it, not_in_it) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let tag = laid.put_in(GroupKind::Tag, &[in_it]);
    laid.project.put_the_badge_of_a_tag_in_the_chain_of(not_in_it, tag);
    assert_eq!(
        problems(&laid),
        [GroupProblem::HasTheBadgeOfAGroupItIsNotIn {
            owner: not_in_it,
            group: tag
        }]
    );

    // A badge of a group that is not there, and of one that is not a tag.
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip_group = laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(0, 0)]);
    let clip = laid.clip_member(1, 0);
    laid.project.put_the_badge_of_a_tag_in_the_chain_of(clip, clip_group);
    laid.project.put_the_badge_of_a_tag_in_the_chain_of(clip, group(55));
    let found = problems(&laid);
    assert!(found.contains(&GroupProblem::HasTheBadgeOfAGroupItIsNotIn {
        owner: clip,
        group: clip_group
    }));
    assert!(found.contains(&GroupProblem::HasTheBadgeOfAGroupItIsNotIn {
        owner: clip,
        group: group(55)
    }));
}

#[test]
fn a_track_or_a_group_that_keeps_the_badge_of_a_track_breaks_a_rule_and_a_clip_does_not() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let tag = laid.put_in(GroupKind::Tag, &[laid.clip_member(0, 0)]);
    let (clip, track) = (laid.clip_member(0, 0), laid.track_member(1));
    for owner in [clip, track, Member::Group(tag)] {
        let chain = laid.project.chain_of_the_member_mut(owner).unwrap();
        chain.add(crate::project::EffectKind::HueShift);
        assert!(chain.move_before(
            crate::project::ChainItemKey::Badge(Badge::Track),
            Some(crate::project::ChainItemKey::Effect(
                crate::project::EffectKind::HueShift
            )),
            true
        ));
    }
    let found = problems(&laid);
    assert_eq!(
        found,
        [
            GroupProblem::HasTheBadgeOfATrackAndIsNotAClip { owner: track },
            GroupProblem::HasTheBadgeOfATrackAndIsNotAClip {
                owner: Member::Group(tag)
            },
        ],
        "the clip may"
    );
}

// --- clip groups: what is inside the rectangle ---

/// Three tracks: the clip group's clips are `a` (track 0, frames 0 to 10) and `b` (track 1, frames 20 to 30), so its rectangle is frames 0
/// to 30 of tracks 0 and 1, and track 2 is below it.
fn a_clip_group_over_two_tracks(others: [&[(u64, u64)]; 3]) -> (Laid, GroupId) {
    let mut layout: Vec<Vec<(u64, u64)>> = others.iter().map(|clips| clips.to_vec()).collect();
    layout[0].insert(0, (0, 10));
    layout[1].insert(0, (20, 10));
    let layout_as_slices: Vec<&[(u64, u64)]> = layout.iter().map(Vec::as_slice).collect();
    let mut laid = lay_out(&layout_as_slices);
    let group = laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)]);
    (laid, group)
}

#[test]
fn a_clip_that_is_inside_the_rectangle_of_a_clip_group_and_is_not_in_it_breaks_a_rule() {
    // On track 1, between frames 0 and 20 where the rectangle reaches across both tracks.
    let (laid, group) = a_clip_group_over_two_tracks([&[], &[(5, 5)], &[]]);
    let outsider = laid.clip(1, 1);
    assert_eq!(
        problems(&laid),
        [GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip: outsider }]
    );

    // On track 0, after the clip that is in the group, but before the end of the rectangle.
    let (laid, group) = a_clip_group_over_two_tracks([&[(15, 5)], &[], &[]]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
            group,
            clip: laid.clip(0, 1)
        }]
    );
}

#[test]
fn a_clip_that_is_partly_inside_the_rectangle_is_inside_it() {
    let (laid, group) = a_clip_group_over_two_tracks([&[(28, 10)], &[], &[]]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
            group,
            clip: laid.clip(0, 1)
        }],
        "it starts in the last two frames of the rectangle, and goes on past its end"
    );
}

#[test]
fn a_clip_that_is_beside_below_above_or_after_the_rectangle_of_a_clip_group_is_not_inside_it() {
    // Touching the end of the rectangle (at frame 30) on both of its tracks, on the track below it, and before it on a track that the
    // rectangle does reach (the rectangle starts at frame 0, so nothing can be before it: the clip is after).
    let (laid, _) = a_clip_group_over_two_tracks([&[(30, 10)], &[(30, 5)], &[(0, 100)]]);
    assert_eq!(problems(&laid), []);
}

#[test]
fn a_clip_that_is_in_a_clip_group_in_a_clip_group_is_in_the_outer_one() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(20, 10)]]);
    let inner = laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)]);
    laid.put_in(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)]);
    assert_eq!(
        problems(&laid),
        [],
        "the clips of the inner group are in the rectangle of the outer, and are in it"
    );

    // The same, without the inner group: the clips of the outer group are not all in it.
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(20, 10)]]);
    laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
            group: group(0),
            clip: laid.clip(1, 0)
        }]
    );
}

#[test]
fn two_clip_groups_may_have_rectangles_that_are_on_top_of_each_other_where_there_is_no_clip() {
    // The first group: a clip on track 0 at frames 0 to 10, and one on track 1 at frames 0 to 4. The rectangle is frames 0 to 10 of
    // tracks 0 and 1. The second group: a clip on track 1 at frames 6 to 16, and one on track 2 at frames 6 to 16. Its rectangle is
    // frames 6 to 16 of tracks 1 and 2. The two share frames 6 to 10 of track 1, where there is no clip.
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 4), (12, 4)], &[(6, 10)]]);
    laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)]);
    laid.put_in(GroupKind::ClipGroup, &[laid.clip_member(1, 1), laid.clip_member(2, 0)]);
    let (first, second) = (
        laid.project.rectangle_of_the_group(group(0)).unwrap(),
        laid.project.rectangle_of_the_group(group(1)).unwrap(),
    );
    assert!(first.is_on_top_of(&second), "they do overlap");
    assert_eq!(problems(&laid), []);
}

// --- track groups: the tracks next to each other ---

#[test]
fn a_track_between_the_tracks_of_a_track_group_that_is_not_in_it_breaks_a_rule() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let track_group = laid.put_in(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(2)]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt {
            group: track_group,
            track: laid.tracks[1]
        }]
    );
}

#[test]
fn the_tracks_of_a_track_group_are_next_to_each_other_when_the_track_groups_in_it_are_counted() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let inner = laid.put_in(GroupKind::TrackGroup, &[laid.track_member(1), laid.track_member(2)]);
    laid.put_in(
        GroupKind::TrackGroup,
        &[laid.track_member(0), Member::Group(inner), laid.track_member(3)],
    );
    assert_eq!(problems(&laid), []);

    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let inner = laid.put_in(GroupKind::TrackGroup, &[laid.track_member(1)]);
    let outer = laid.put_in(GroupKind::TrackGroup, &[laid.track_member(3), Member::Group(inner)]);
    assert_eq!(
        problems(&laid),
        [GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt {
            group: outer,
            track: laid.tracks[2]
        }]
    );
}

// --- clip groups and track groups ---

/// A clip on each of four tracks, and a clip group of the clips on the tracks `clips.0` to `clips.1` and a track group of the tracks
/// `tracks.0` to `tracks.1`.
fn a_clip_group_and_a_track_group(clips: (usize, usize), tracks: (usize, usize)) -> Vec<GroupProblem> {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let clip_members: Vec<Member> = (clips.0..=clips.1).map(|track| laid.clip_member(track, 0)).collect();
    let track_members: Vec<Member> = (tracks.0..=tracks.1).map(|track| laid.track_member(track)).collect();
    laid.put_in(GroupKind::ClipGroup, &clip_members);
    laid.put_in(GroupKind::TrackGroup, &track_members);
    problems(&laid)
}

#[test]
fn a_clip_group_inside_a_track_group_or_apart_from_it_breaks_no_rule() {
    assert_eq!(a_clip_group_and_a_track_group((0, 1), (0, 1)), [], "the same tracks");
    assert_eq!(a_clip_group_and_a_track_group((1, 2), (0, 3)), [], "inside");
    assert_eq!(a_clip_group_and_a_track_group((0, 1), (2, 3)), [], "apart");
    assert_eq!(
        a_clip_group_and_a_track_group((2, 3), (0, 1)),
        [],
        "apart, the other way round"
    );
}

#[test]
fn a_clip_group_that_is_across_the_edge_of_a_track_group_or_has_it_between_its_tracks_breaks_a_rule() {
    let expected = [GroupProblem::ClipGroupReachesAcrossTheEdgeOfATrackGroup {
        clip_group: group(0),
        track_group: group(1),
    }];
    assert_eq!(
        a_clip_group_and_a_track_group((0, 1), (1, 2)),
        expected,
        "one track in common"
    );
    assert_eq!(
        a_clip_group_and_a_track_group((1, 3), (0, 1)),
        expected,
        "the other way round"
    );
    assert_eq!(
        a_clip_group_and_a_track_group((0, 3), (1, 2)),
        expected,
        "the track group is between the first track of the clip group and its last"
    );
}

// --- the order the rules are looked at in ---

#[test]
fn where_things_are_is_not_looked_at_while_what_is_in_what_does_not_make_sense() {
    // The outsider is inside the rectangle, but the group has a member that is not there, and so there is no rectangle to speak of.
    let (mut laid, group_with_an_outsider) = a_clip_group_over_two_tracks([&[(15, 5)], &[], &[]]);
    let missing = Member::Clip(crate::project::ClipId::new(laid.tracks[2], 99));
    laid.project
        .group_mut(group_with_an_outsider)
        .unwrap()
        .members_mut()
        .push(missing);
    assert_eq!(
        problems(&laid),
        [GroupProblem::HasAMemberThatIsNotThere {
            group: group_with_an_outsider,
            member: missing
        }]
    );
}

#[test]
fn the_first_problem_is_the_first_of_the_problems() {
    let mut laid = lay_out(&[&[(0, 10)]]);
    let first = laid.put_in(GroupKind::Tag, &[]);
    laid.put_in(GroupKind::Tag, &[]);
    assert_eq!(problems(&laid).len(), 2);
    assert_eq!(
        laid.project.first_problem_with_the_groups(),
        Some(GroupProblem::HasNothingInIt { group: first })
    );
}

// --- giving a project its groups ---

#[test]
fn a_project_is_given_groups_that_keep_the_rules_and_refuses_groups_that_do_not() {
    let laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    // (A tag has to have its badge in the chain of each of its members: a project that was made by the editing, and saved and loaded.)
    let mut made = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let tag = made.project.create_group(GroupKind::Tag, &[clip]).unwrap();
    let groups = made.project.groups().to_vec();
    assert_eq!(groups.len(), 1);

    let bad = vec![Group::new(group(4), GroupKind::Tag, "bad", GroupColour::PALETTE[0])];
    assert_eq!(
        laid.project.clone().with_groups(bad).unwrap_err(),
        crate::project::InconsistentProject::GroupsAreWrong {
            problem: GroupProblem::HasNothingInIt { group: group(4) }
        }
    );

    let project = made.project.clone().with_groups(groups).unwrap();
    assert_eq!(project.groups().len(), 1);
    assert_eq!(project, made.project, "the same project");
    assert_eq!(project.group(tag).map(Group::name), Some("Tag 1"));
}

#[test]
fn the_number_the_next_group_is_given_is_one_more_than_any_there_is_after_groups_are_given_to_a_project() {
    let mut made = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    made.project
        .create_group(GroupKind::Tag, &[made.clip_member(0, 0)])
        .unwrap();
    let mut groups = made.project.groups().to_vec();
    // The group is given another number than it was made with: the next one has to be after it.
    let renumbered = Group::new(group(10), GroupKind::Tag, "ten", GroupColour::PALETTE[2])
        .with_members(groups[0].members().iter().copied())
        .with_effects(groups[0].effects().clone());
    groups[0] = renumbered;
    let mut project = lay_out(&[&[(0, 10)], &[(0, 10)]]).project;
    // (The chain of the clip needs the badge of the group with its new number.)
    let clip = project.tracks()[0].entries()[0].id();
    project.put_the_badge_of_a_tag_in_the_chain_of(Member::Clip(clip), group(10));
    let mut project = project.with_groups(groups).unwrap();
    let next = project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap();
    assert_eq!(next, group(11));
}

// --- what the problems say ---

#[test]
fn a_problem_says_which_group_it_is_about_and_what_is_wrong_in_words() {
    let clip = Member::Clip(crate::project::ClipId::new(crate::project::TrackId::new(0), 3));
    let all = [
        (
            GroupProblem::HasAMemberThatIsNotThere {
                group: group(2),
                member: clip,
            },
            Some(group(2)),
        ),
        (
            GroupProblem::HasAMemberTwice {
                group: group(2),
                member: clip,
            },
            Some(group(2)),
        ),
        (
            GroupProblem::CannotHold {
                group: group(2),
                kind: GroupKind::Tag,
                member: Member::Track(crate::project::TrackId::new(1)),
            },
            Some(group(2)),
        ),
        (
            GroupProblem::InTwoShapes {
                member: clip,
                first: group(1),
                second: group(2),
            },
            Some(group(2)),
        ),
        (GroupProblem::IsInItself { group: group(2) }, Some(group(2))),
        (GroupProblem::HasNothingInIt { group: group(2) }, Some(group(2))),
        (
            GroupProblem::HasNoBadgeOfTheGroupItIsIn {
                member: clip,
                group: group(2),
            },
            Some(group(2)),
        ),
        (
            GroupProblem::HasTheBadgeOfAGroupItIsNotIn {
                owner: clip,
                group: group(2),
            },
            Some(group(2)),
        ),
        (GroupProblem::HasTheBadgeOfATrackAndIsNotAClip { owner: clip }, None),
        (
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group: group(2),
                clip: crate::project::ClipId::new(crate::project::TrackId::new(0), 3),
            },
            Some(group(2)),
        ),
        (
            GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt {
                group: group(2),
                track: crate::project::TrackId::new(4),
            },
            Some(group(2)),
        ),
        (
            GroupProblem::ClipGroupReachesAcrossTheEdgeOfATrackGroup {
                clip_group: group(2),
                track_group: group(3),
            },
            Some(group(2)),
        ),
    ];
    for (problem, about) in all {
        assert_eq!(problem.group(), about, "{problem:?}");
        let words = problem.to_string();
        assert!(words.len() >= 15, "{problem:?} has something to say: {words:?}");
        if let Some(about) = about {
            assert!(words.contains(&about.to_string()), "{words:?} names {about}");
        }
    }
}
