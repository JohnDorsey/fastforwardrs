// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making groups, naming them, putting things in and taking them out, taking them apart, and deleting them with all that is in them.

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{Badge, ChainItemKey, ClipId, EffectKind, GroupProblem, TrackId};

fn tag_badge(group: GroupId) -> ChainItemKey {
    ChainItemKey::Badge(Badge::Tag(group))
}

const TRACK_BADGE: ChainItemKey = ChainItemKey::Badge(Badge::Track);

/// What the chain of the clip `clip` has, in the order it is applied in, the badge of its track at the end if it is not kept elsewhere.
fn chain_of_the_clip(laid: &Laid, clip: Member) -> Vec<ChainItemKey> {
    laid.project
        .chain_of_the_member(clip)
        .unwrap()
        .keys(matches!(clip, Member::Clip(_)))
}

fn ids(groups: &[&Group]) -> Vec<GroupId> {
    groups.iter().map(|group| group.id()).collect()
}

/// Four tracks, each with a clip at frames 0 to 10 (`laid.clip(n, 0)`).
fn four_tracks() -> Laid {
    lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]])
}

// --- making a group ---

#[test]
fn a_tag_is_made_of_clips_that_have_clips_that_are_not_in_it_between_them_and_each_has_its_badge_last() {
    let mut laid = four_tracks();
    let (first, last) = (laid.clip_member(0, 0), laid.clip_member(3, 0));
    let tag = laid.project.create_group(GroupKind::Tag, &[first, last]).unwrap();

    let made = laid.project.group(tag).unwrap();
    assert_eq!(made.kind(), GroupKind::Tag);
    assert_eq!(made.members(), [first, last]);
    assert!(made.effects().has_nothing_in_it());
    assert_eq!(chain_of_the_clip(&laid, first), [tag_badge(tag), TRACK_BADGE]);
    assert_eq!(chain_of_the_clip(&laid, last), [tag_badge(tag), TRACK_BADGE]);
    assert_eq!(
        chain_of_the_clip(&laid, laid.clip_member(1, 0)),
        [TRACK_BADGE],
        "the clips between them are not in it, and have nothing of it"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_tag_can_be_made_of_one_clip() {
    let mut laid = four_tracks();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(2, 0)])
        .unwrap();
    assert_eq!(laid.project.group(tag).unwrap().members(), [laid.clip_member(2, 0)]);
}

#[test]
fn a_tag_can_be_made_of_clips_and_groups_of_any_kind_and_of_tags() {
    let mut laid = four_tracks();
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(2), laid.track_member(3)])
        .unwrap();
    let inner = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(
            GroupKind::Tag,
            &[
                Member::Group(clip_group),
                Member::Group(track_group),
                Member::Group(inner),
            ],
        )
        .unwrap();
    assert_eq!(laid.project.group(outer).unwrap().members().len(), 3);
    assert!(
        laid.project
            .chain_of_the_member(Member::Group(clip_group))
            .unwrap()
            .has_the_tag_badge(outer)
    );
    assert!(
        laid.project
            .chain_of_the_member(Member::Group(track_group))
            .unwrap()
            .has_the_tag_badge(outer)
    );
    assert!(
        laid.project
            .chain_of_the_member(Member::Group(inner))
            .unwrap()
            .has_the_tag_badge(outer)
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn nothing_is_made_into_a_group_and_a_clip_group_needs_two_things() {
    let mut laid = four_tracks();
    let before = laid.project.clone();
    for kind in GroupKind::ALL {
        assert_eq!(
            laid.project.create_group(kind, &[]),
            Err(EditError::NothingToMakeAGroupOf { kind })
        );
    }
    assert_eq!(
        laid.project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0)]),
        Err(EditError::TooFewToMakeAClipGroup)
    );
    assert_eq!(laid.project, before, "nothing changed");
    assert!(
        laid.project
            .create_group(GroupKind::TrackGroup, &[laid.track_member(0)])
            .is_ok(),
        "one track will do"
    );
}

#[test]
fn a_clip_group_is_refused_when_a_clip_that_is_not_in_it_would_be_inside_its_rectangle_and_nothing_changes() {
    let mut laid = four_tracks();
    let before = laid.project.clone();
    let outsider = laid.clip(1, 0);
    assert_eq!(
        laid.project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group: GroupId::new(0),
                clip: outsider
            }
        ))
    );
    assert_eq!(
        laid.project, before,
        "the group is not left behind, and the next one is given the same number"
    );
}

#[test]
fn a_clip_group_is_made_when_what_is_between_its_clips_is_beside_its_rectangle() {
    // The clip between them, on the middle track, is after the end of the rectangle.
    let mut laid = lay_out(&[&[(0, 10)], &[(10, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
        .unwrap();
    assert_eq!(laid.project.rectangle_of_the_group(group).unwrap().tracks(), 0..=2);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_group_can_have_clip_groups_in_it_and_a_clip_that_is_in_another_clip_group_cannot_be_in_it_as_well() {
    let mut laid = four_tracks();
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)])
        .unwrap();
    assert!(laid.project.is_in_the_shape(laid.clip_member(0, 0), outer));

    let before = laid.project.clone();
    assert_eq!(
        laid.project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(1, 0), laid.clip_member(3, 0)]),
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::InTwoShapes {
            member: laid.clip_member(1, 0),
            first: inner,
            second: GroupId::new(2)
        })),
        "a clip is in one clip group"
    );
    assert_eq!(laid.project, before);
}

#[test]
fn a_track_group_is_made_of_tracks_that_are_next_to_each_other_and_not_of_tracks_that_are_apart() {
    let mut laid = four_tracks();
    let before = laid.project.clone();
    assert_eq!(
        laid.project
            .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(2)]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt {
                group: GroupId::new(0),
                track: laid.tracks[1]
            }
        ))
    );
    assert_eq!(laid.project, before);
    let group = laid
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[laid.track_member(1), laid.track_member(0), laid.track_member(2)],
        )
        .unwrap();
    assert_eq!(
        laid.project.tracks_in(group),
        [laid.tracks[1], laid.tracks[0], laid.tracks[2]],
        "in the order they were given, whatever the order of the tracks is"
    );
}

#[test]
fn a_track_group_that_would_have_a_clip_group_across_its_edge_is_refused() {
    let mut laid = four_tracks();
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(1, 0), laid.clip_member(2, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert!(matches!(
        laid.project
            .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::ClipGroupReachesAcrossTheEdgeOfATrackGroup { .. }
        ))
    ));
    assert_eq!(laid.project, before);
    assert!(
        laid.project
            .create_group(
                GroupKind::TrackGroup,
                &[laid.track_member(0), laid.track_member(1), laid.track_member(2)]
            )
            .is_ok(),
        "the clip group is inside this one"
    );
}

#[test]
fn a_group_is_refused_what_a_group_of_its_kind_cannot_hold_and_the_same_thing_twice() {
    let mut laid = four_tracks();
    let before = laid.project.clone();
    let tag_with_a_track = laid.project.create_group(GroupKind::Tag, &[laid.track_member(0)]);
    assert!(matches!(
        tag_with_a_track,
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::CannotHold {
            kind: GroupKind::Tag,
            ..
        }))
    ));
    let track_group_with_a_clip = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.clip_member(0, 0)]);
    assert!(matches!(
        track_group_with_a_clip,
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::CannotHold {
            kind: GroupKind::TrackGroup,
            ..
        }))
    ));
    let clip_group_with_a_track = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.track_member(1)]);
    assert!(matches!(
        clip_group_with_a_track,
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::CannotHold {
            kind: GroupKind::ClipGroup,
            ..
        }))
    ));
    let twice = laid.clip_member(0, 0);
    assert!(matches!(
        laid.project.create_group(GroupKind::Tag, &[twice, twice]),
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::HasAMemberTwice { .. }))
    ));
    let no_such_clip = Member::Clip(ClipId::new(TrackId::new(99), 0));
    assert!(matches!(
        laid.project.create_group(GroupKind::Tag, &[no_such_clip]),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::HasAMemberThatIsNotThere { .. }
        ))
    ));
    assert_eq!(laid.project, before, "none of them left anything behind");
}

// --- names and colours ---

#[test]
fn a_new_group_is_named_with_its_kind_and_the_smallest_number_that_is_free_and_each_kind_counts_by_itself() {
    let mut laid = four_tracks();
    let first = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(1, 0)])
        .unwrap();
    let third = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(2, 0)])
        .unwrap();
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(3)])
        .unwrap();
    let name_of = |laid: &Laid, group| laid.project.group(group).unwrap().name().to_owned();
    assert_eq!(name_of(&laid, first), "Tag 1");
    assert_eq!(name_of(&laid, second), "Tag 2");
    assert_eq!(name_of(&laid, third), "Tag 3");
    assert_eq!(name_of(&laid, clip_group), "Clip group 1");
    assert_eq!(name_of(&laid, track_group), "Track group 1");

    laid.project.disband_group(second).unwrap();
    let again = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(3, 0)])
        .unwrap();
    assert_eq!(
        name_of(&laid, again),
        "Tag 2",
        "the number that is free again is used again"
    );

    laid.project.rename_group(first, "Tag 2").unwrap();
    assert_eq!(
        name_of(&laid, first),
        name_of(&laid, again),
        "names do not have to be different"
    );
    let another = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(3, 0)])
        .unwrap();
    assert_eq!(
        name_of(&laid, another),
        "Tag 1",
        "the number that no tag is named with now, which is the one that the first was named with"
    );
}

#[test]
fn new_groups_are_given_the_colours_of_the_palette_in_turn_a_colour_that_is_free_again_is_used_first_and_after_the_last_it_begins_again()
 {
    let mut laid = lay_out(&[&[
        (0, 10),
        (20, 10),
        (40, 10),
        (60, 10),
        (80, 10),
        (100, 10),
        (120, 10),
        (140, 10),
        (160, 10),
    ]]);
    let clips = laid.clips[0].clone();
    let mut groups = Vec::new();
    for clip in &clips[..8] {
        groups.push(
            laid.project
                .create_group(GroupKind::Tag, &[Member::Clip(*clip)])
                .unwrap(),
        );
    }
    for (group, colour) in groups.iter().zip(GroupColour::PALETTE) {
        assert_eq!(laid.project.group(*group).unwrap().colour(), colour);
    }
    let ninth = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[8])])
        .unwrap();
    assert_eq!(
        laid.project.group(ninth).unwrap().colour(),
        GroupColour::PALETTE[0],
        "all eight are taken: it begins again"
    );
    // (A clip can be in any number of tags.)
    for (number_of_groups_so_far, colour_wanted) in [(9, GroupColour::PALETTE[1]), (10, GroupColour::PALETTE[2])] {
        let another = laid
            .project
            .create_group(GroupKind::Tag, &[Member::Clip(clips[8])])
            .unwrap();
        assert_eq!(laid.project.groups().len(), number_of_groups_so_far + 1);
        assert_eq!(
            laid.project.group(another).unwrap().colour(),
            colour_wanted,
            "and it goes on in turn from there"
        );
    }

    laid.project.disband_group(groups[3]).unwrap();
    laid.project.disband_group(ninth).unwrap();
    let after = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clips[8])])
        .unwrap();
    assert_eq!(
        laid.project.group(after).unwrap().colour(),
        GroupColour::PALETTE[3],
        "the first colour that no group has"
    );
}

#[test]
fn a_group_is_renamed_and_given_another_colour_and_there_is_an_error_for_a_group_that_is_not_there() {
    let mut laid = four_tracks();
    let group = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project.rename_group(group, "titles").unwrap();
    laid.project.set_group_colour(group, GroupColour::new(1, 2, 3)).unwrap();
    assert_eq!(laid.project.group(group).unwrap().name(), "titles");
    assert_eq!(laid.project.group(group).unwrap().colour(), GroupColour::new(1, 2, 3));
    let none = GroupId::new(40);
    assert_eq!(
        laid.project.rename_group(none, "x"),
        Err(EditError::UnknownGroup { group: none })
    );
    assert_eq!(
        laid.project.set_group_colour(none, GroupColour::new(1, 2, 3)),
        Err(EditError::UnknownGroup { group: none })
    );
}

// --- adding and taking out ---

#[test]
fn a_clip_is_added_to_a_tag_last_and_its_badge_is_last_in_its_chain_too() {
    let mut laid = four_tracks();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let other = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(1, 0)])
        .unwrap();
    let joining = laid.clip_member(1, 0);
    laid.project.add_to_the_group(tag, joining).unwrap();
    assert_eq!(
        laid.project.group(tag).unwrap().members(),
        [laid.clip_member(0, 0), joining]
    );
    assert_eq!(
        chain_of_the_clip(&laid, joining),
        [tag_badge(other), tag_badge(tag), TRACK_BADGE]
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn something_that_is_in_a_group_already_is_not_added_again_and_a_group_that_is_not_there_has_nothing_added() {
    let mut laid = four_tracks();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project.add_to_the_group(tag, laid.clip_member(0, 0)),
        Err(EditError::AlreadyInTheGroup {
            group: tag,
            member: laid.clip_member(0, 0)
        })
    );
    assert_eq!(
        laid.project.add_to_the_group(GroupId::new(9), laid.clip_member(0, 0)),
        Err(EditError::UnknownGroup { group: GroupId::new(9) })
    );
    assert_eq!(laid.project, before);
}

#[test]
fn a_clip_is_added_to_a_clip_group_when_that_breaks_no_rule_and_is_refused_when_it_does() {
    let mut laid = four_tracks();
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    laid.project.add_to_the_group(group, laid.clip_member(2, 0)).unwrap();
    assert_eq!(laid.project.rectangle_of_the_group(group).unwrap().tracks(), 0..=2);

    let mut laid = four_tracks();
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert!(
        matches!(
            laid.project.add_to_the_group(group, laid.clip_member(3, 0)),
            Err(EditError::BreaksTheRulesOfGroups(
                GroupProblem::IsInsideAClipGroupWithoutBeingInIt { .. }
            ))
        ),
        "the clip on the track between would be inside"
    );
    assert_eq!(laid.project, before);
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(3, 0)])
        .unwrap();
    assert!(matches!(
        laid.project.add_to_the_group(group, Member::Group(tag)),
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::CannotHold { .. }))
    ));
}

#[test]
fn a_member_is_taken_out_of_a_tag_with_its_badge_and_the_others_keep_theirs() {
    let mut laid = four_tracks();
    let (first, second) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let tag = laid.project.create_group(GroupKind::Tag, &[first, second]).unwrap();
    laid.project.take_out_of_the_group(tag, first).unwrap();
    assert_eq!(laid.project.group(tag).unwrap().members(), [second]);
    assert_eq!(
        chain_of_the_clip(&laid, first),
        [TRACK_BADGE],
        "its chain is as it was before"
    );
    assert_eq!(chain_of_the_clip(&laid, second), [tag_badge(tag), TRACK_BADGE]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn taking_the_last_member_out_of_a_group_drops_the_group_and_so_the_groups_that_only_it_was_in() {
    let mut laid = four_tracks();
    let clip = laid.clip_member(0, 0);
    let inner = laid.project.create_group(GroupKind::Tag, &[clip]).unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(inner)])
        .unwrap();
    laid.project.take_out_of_the_group(inner, clip).unwrap();
    assert!(laid.project.group(inner).is_none());
    assert!(laid.project.group(outer).is_none(), "it had only that one in it");
    assert!(laid.project.groups().is_empty());
}

#[test]
fn a_member_that_is_not_in_a_group_is_not_taken_out_of_it() {
    let mut laid = four_tracks();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project.take_out_of_the_group(tag, laid.clip_member(1, 0)),
        Err(EditError::NotInTheGroup {
            group: tag,
            member: laid.clip_member(1, 0)
        })
    );
    assert_eq!(
        laid.project
            .take_out_of_the_group(GroupId::new(7), laid.clip_member(1, 0)),
        Err(EditError::UnknownGroup { group: GroupId::new(7) })
    );
    assert_eq!(laid.project, before);
}

#[test]
fn a_clip_at_the_edge_of_a_clip_group_can_be_taken_out_and_one_in_the_middle_cannot() {
    // Three clips, one above the other. The middle one is inside the rectangle of the group whether it is in it or not.
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[laid.clip_member(0, 0), laid.clip_member(1, 0), laid.clip_member(2, 0)],
        )
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project.take_out_of_the_group(group, laid.clip_member(1, 0)),
        Err(EditError::BreaksTheRulesOfGroups(
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt {
                group,
                clip: laid.clip(1, 0)
            }
        ))
    );
    assert_eq!(laid.project, before, "refused, and nothing is changed");

    laid.project
        .take_out_of_the_group(group, laid.clip_member(0, 0))
        .unwrap();
    assert_eq!(
        laid.project.group(group).unwrap().members(),
        [laid.clip_member(1, 0), laid.clip_member(2, 0)]
    );
    laid.project
        .take_out_of_the_group(group, laid.clip_member(2, 0))
        .unwrap();
    assert_eq!(
        laid.project.group(group).unwrap().members(),
        [laid.clip_member(1, 0)],
        "a group of one stays"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_track_at_the_edge_of_a_track_group_can_be_taken_out_and_one_in_the_middle_cannot() {
    let mut laid = four_tracks();
    let group = laid
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[laid.track_member(0), laid.track_member(1), laid.track_member(2)],
        )
        .unwrap();
    assert!(
        laid.project.take_out_of_the_group(group, laid.track_member(1)).is_err(),
        "it is between the others"
    );
    laid.project.take_out_of_the_group(group, laid.track_member(2)).unwrap();
    laid.project.take_out_of_the_group(group, laid.track_member(0)).unwrap();
    assert_eq!(laid.project.tracks_in(group), [laid.tracks[1]]);
}

// --- taking a group apart ---

#[test]
fn disbanding_a_tag_takes_away_the_tag_and_its_badges_and_leaves_everything_else_as_it_was() {
    let mut laid = four_tracks();
    let (first, second) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let kept = laid.project.create_group(GroupKind::Tag, &[first, second]).unwrap();
    let tag = laid.project.create_group(GroupKind::Tag, &[first, second]).unwrap();
    let first_clip = laid.clip(0, 0);
    laid.project
        .add_effect(crate::project::AnimationTarget::Clip(first_clip), EffectKind::HueShift)
        .unwrap();
    assert_eq!(
        chain_of_the_clip(&laid, first),
        [
            tag_badge(kept),
            tag_badge(tag),
            ChainItemKey::Effect(EffectKind::HueShift),
            TRACK_BADGE
        ]
    );

    laid.project.disband_group(tag).unwrap();
    assert!(laid.project.group(tag).is_none());
    assert_eq!(
        laid.project.group(kept).unwrap().members(),
        [first, second],
        "the other tag is as it was"
    );
    assert_eq!(
        chain_of_the_clip(&laid, first),
        [tag_badge(kept), ChainItemKey::Effect(EffectKind::HueShift), TRACK_BADGE],
        "only the badge of the tag that is gone is gone"
    );
    assert_eq!(chain_of_the_clip(&laid, second), [tag_badge(kept), TRACK_BADGE]);
    assert_eq!(
        laid.project.placement_of(first_clip).unwrap().effects.len(),
        1,
        "its own effect stays"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn disbanding_a_clip_group_that_is_in_a_clip_group_puts_its_clips_in_the_outer_one_in_its_place() {
    let mut laid = four_tracks();
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(1, 0), laid.clip_member(2, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[laid.clip_member(0, 0), Member::Group(inner), laid.clip_member(3, 0)],
        )
        .unwrap();
    laid.project.disband_group(inner).unwrap();
    assert_eq!(
        laid.project.group(outer).unwrap().members(),
        [
            laid.clip_member(0, 0),
            laid.clip_member(1, 0),
            laid.clip_member(2, 0),
            laid.clip_member(3, 0)
        ],
        "where the inner group was"
    );
    assert_eq!(
        laid.project.problems_with_the_groups(),
        [],
        "what was inside the outer group is inside still"
    );
}

#[test]
fn disbanding_a_tag_that_is_in_a_tag_puts_its_members_in_the_outer_tag_where_its_own_badge_was() {
    let mut laid = four_tracks();
    let (x, y, z) = (laid.clip_member(0, 0), laid.clip_member(1, 0), laid.clip_member(2, 0));
    let inner = laid.project.create_group(GroupKind::Tag, &[x, y]).unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(inner), z])
        .unwrap();
    // x has an effect after the badge of the inner tag, and the chain of the inner tag has the badge of the outer one.
    laid.project
        .add_effect(
            crate::project::AnimationTarget::Clip(laid.clip(0, 0)),
            EffectKind::HueShift,
        )
        .unwrap();
    assert_eq!(
        chain_of_the_clip(&laid, x),
        [
            tag_badge(inner),
            ChainItemKey::Effect(EffectKind::HueShift),
            TRACK_BADGE
        ]
    );

    laid.project.disband_group(inner).unwrap();
    assert_eq!(laid.project.group(outer).unwrap().members(), [x, y, z]);
    assert_eq!(
        chain_of_the_clip(&laid, x),
        [
            tag_badge(outer),
            ChainItemKey::Effect(EffectKind::HueShift),
            TRACK_BADGE
        ],
        "the badge of the outer tag is where the inner one was, before the effect"
    );
    assert_eq!(chain_of_the_clip(&laid, y), [tag_badge(outer), TRACK_BADGE]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn disbanding_a_clip_group_that_is_in_a_tag_puts_its_clips_in_the_tag_and_does_not_disturb_the_clips_that_were_in_it() {
    let mut laid = four_tracks();
    let (x, y, z) = (laid.clip_member(0, 0), laid.clip_member(1, 0), laid.clip_member(2, 0));
    let clip_group = laid.project.create_group(GroupKind::ClipGroup, &[x, y]).unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[z, Member::Group(clip_group)])
        .unwrap();
    laid.project.disband_group(clip_group).unwrap();
    assert_eq!(laid.project.group(tag).unwrap().members(), [z, x, y]);
    assert_eq!(chain_of_the_clip(&laid, x), [tag_badge(tag), TRACK_BADGE]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn disbanding_a_member_of_a_tag_with_clips_that_are_in_the_tag_already_does_not_put_them_in_twice() {
    let mut laid = four_tracks();
    let (x, y) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let clip_group = laid.project.create_group(GroupKind::ClipGroup, &[x, y]).unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[x, Member::Group(clip_group)])
        .unwrap();
    laid.project.disband_group(clip_group).unwrap();
    assert_eq!(
        laid.project.group(tag).unwrap().members(),
        [x, y],
        "x is where it was, and y comes where the group was"
    );
    assert_eq!(chain_of_the_clip(&laid, x), [tag_badge(tag), TRACK_BADGE]);
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn disbanding_a_track_group_that_is_in_a_tag_leaves_the_tag_without_it_for_a_tag_cannot_have_tracks() {
    let mut laid = four_tracks();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(track_group), laid.clip_member(3, 0)])
        .unwrap();
    laid.project.disband_group(track_group).unwrap();
    assert_eq!(laid.project.group(tag).unwrap().members(), [laid.clip_member(3, 0)]);

    let mut laid = four_tracks();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(track_group)])
        .unwrap();
    laid.project.disband_group(track_group).unwrap();
    assert!(laid.project.group(tag).is_none(), "it had nothing left in it");
    assert!(laid.project.groups().is_empty());
}

#[test]
fn disbanding_a_track_group_that_is_in_a_track_group_puts_its_tracks_in_the_outer_one() {
    let mut laid = four_tracks();
    let inner = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(1), laid.track_member(2)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), Member::Group(inner)])
        .unwrap();
    laid.project.disband_group(inner).unwrap();
    assert_eq!(
        laid.project.group(outer).unwrap().members(),
        [laid.track_member(0), laid.track_member(1), laid.track_member(2)]
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_group_that_is_not_there_is_not_disbanded() {
    let mut laid = four_tracks();
    assert_eq!(
        laid.project.disband_group(GroupId::new(3)),
        Err(EditError::UnknownGroup { group: GroupId::new(3) })
    );
}

// --- deleting a group and all that is in it ---

#[test]
fn deleting_a_clip_group_and_all_in_it_takes_its_clips_off_their_tracks_and_leaves_the_others() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(20, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    laid.project.delete_the_group_and_everything_in_it(group).unwrap();
    assert!(laid.project.group(group).is_none());
    assert!(laid.project.placement_of(laid.clip(0, 0)).is_none());
    assert!(laid.project.placement_of(laid.clip(1, 0)).is_none());
    assert!(
        laid.project.placement_of(laid.clip(2, 0)).is_some(),
        "the clip that was not in it stays"
    );
    assert_eq!(laid.project.tracks().len(), 3, "the tracks stay");
}

#[test]
fn deleting_a_tag_and_all_in_it_takes_away_the_clips_in_it_and_in_the_groups_in_it_and_those_groups_with_them() {
    let mut laid = four_tracks();
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(clip_group), laid.clip_member(3, 0)])
        .unwrap();
    laid.project.delete_the_group_and_everything_in_it(tag).unwrap();
    assert!(laid.project.groups().is_empty());
    for track in [0, 1, 3] {
        assert!(
            laid.project.placement_of(laid.clip(track, 0)).is_none(),
            "track {track}"
        );
    }
    assert!(laid.project.placement_of(laid.clip(2, 0)).is_some());
    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn deleting_a_track_group_and_all_in_it_takes_away_its_tracks_with_all_that_is_on_them() {
    let mut laid = four_tracks();
    let group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(1), laid.track_member(2)])
        .unwrap();
    let outer_tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(1, 0), laid.clip_member(0, 0)])
        .unwrap();
    laid.project.delete_the_group_and_everything_in_it(group).unwrap();
    assert_eq!(laid.project.tracks().len(), 2);
    assert!(laid.project.track(laid.tracks[1]).is_none() && laid.project.track(laid.tracks[2]).is_none());
    assert!(laid.project.track(laid.tracks[0]).is_some() && laid.project.track(laid.tracks[3]).is_some());
    assert!(laid.project.group(group).is_none());
    assert_eq!(
        laid.project.group(outer_tag).unwrap().members(),
        [laid.clip_member(0, 0)],
        "the clip of a track that went is out of the tag that it was in"
    );
}

#[test]
fn deleting_every_track_of_the_project_is_refused_because_a_project_always_has_a_track() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project.delete_the_group_and_everything_in_it(group),
        Err(EditError::WouldDeleteEveryTrack)
    );
    assert_eq!(laid.project, before);

    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(group)])
        .unwrap();
    let before = laid.project.clone();
    assert_eq!(
        laid.project.delete_the_group_and_everything_in_it(tag),
        Err(EditError::WouldDeleteEveryTrack),
        "a tag that has all the tracks in the groups in it"
    );
    assert_eq!(laid.project, before);
}

#[test]
fn a_group_that_is_not_there_is_not_deleted() {
    let mut laid = four_tracks();
    assert_eq!(
        laid.project.delete_the_group_and_everything_in_it(GroupId::new(3)),
        Err(EditError::UnknownGroup { group: GroupId::new(3) })
    );
}

#[test]
fn ids_of_groups_in_a_list_are_in_the_order_the_groups_were_made_in() {
    let mut laid = four_tracks();
    let first = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    assert_eq!(
        ids(&laid.project.groups_holding(laid.clip_member(0, 0))),
        [first, second]
    );
}
