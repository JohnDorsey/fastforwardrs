// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the user is told when something to do with groups cannot be done: which rule it would break, in words, naming the things that are in
//! the way (a clip that would be inside the rectangle of a clip group, a track that is between the tracks of a track group).
//!
//! The rules are the project's (see [`crate::project::GroupProblem`]); here they are only put into words. The things are named as they are named
//! in the lists of the properties panel (see [`describe_a_member`]).

use super::naming::{SourceLooks, describe_a_member};
use crate::project::{EditError, GroupId, GroupProblem, Member, Project};

/// What a group is called in a message: its kind and its name ("the clip group \"Intro\""). A group that is not in the project is
/// "a group".
fn the_group(project: &Project, group: GroupId) -> String {
    match project.group(group) {
        Some(group) => format!("the {} \"{}\"", group.kind().name(), group.name()),
        None => "a group".to_owned(),
    }
}

/// What a thing is called in a message, in the words of the lists of the properties panel, with quotes ("Video clip: holiday.mp4 (at 0:04)").
fn the_member(project: &Project, source_looks: &SourceLooks<'_>, member: Member) -> String {
    match describe_a_member(project, source_looks, member) {
        Some(description) => format!("\"{description}\""),
        None => "something".to_owned(),
    }
}

/// The rule that `problem` is about, in words.
fn in_words(project: &Project, source_looks: &SourceLooks<'_>, problem: &GroupProblem) -> String {
    let member = |member: Member| the_member(project, source_looks, member);
    match *problem {
        GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip } => format!(
            "{} would be inside the rectangle of {}, where only the clips that are in that group may be.",
            member(Member::Clip(clip)),
            the_group(project, group)
        ),
        GroupProblem::InTwoShapes {
            member: what,
            first,
            second: _,
        } => {
            let kind = project.group(first).map_or("group", |group| group.kind().name());
            format!(
                "{} is in {} already, and a thing can be in one {kind} only.",
                member(what),
                the_group(project, first)
            )
        }
        GroupProblem::CannotHold {
            group: _,
            kind,
            member: what,
        } => format!(
            "A {kind} cannot have {} in it: a tag can have clips and groups, a clip group clips and clip groups, and a track group \
             tracks and track groups.",
            member(what)
        ),
        GroupProblem::HasAMemberTwice { .. } => "The same thing was chosen twice.".to_owned(),
        GroupProblem::HasAMemberThatIsNotThere { .. } => {
            "Something that was chosen is not in the project any more.".to_owned()
        }
        GroupProblem::IsInItself { group } => format!(
            "A group cannot be in itself, which {} would be.",
            the_group(project, group)
        ),
        GroupProblem::HasNothingInIt { .. } => "A group has to have something in it.".to_owned(),
        GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt { group, track } => format!(
            "{} is between the tracks of {}, and is not in it. The tracks of a track group are next to each other.",
            member(Member::Track(track)),
            the_group(project, group)
        ),
        GroupProblem::ClipGroupReachesAcrossTheEdgeOfATrackGroup {
            clip_group,
            track_group,
        } => format!(
            "{} would be partly inside {} and partly outside it: a clip group has to be all inside a track group, or all outside it.",
            the_group(project, clip_group),
            the_group(project, track_group)
        ),
        GroupProblem::HasNoBadgeOfTheGroupItIsIn { .. }
        | GroupProblem::HasTheBadgeOfAGroupItIsNotIn { .. }
        | GroupProblem::HasTheBadgeOfATrackAndIsNotAClip { .. } => {
            format!("The groups and the chains of effects would not agree with each other ({problem}).")
        }
    }
}

/// Why `error` happened, in words, if it is something that groups make happen (a rule that groups have, something about a group that was not
/// there, a group that was asked for what it cannot be): `None` for every other error, which is for the caller to say.
pub fn the_reason(project: &Project, source_looks: &SourceLooks<'_>, error: &EditError) -> Option<String> {
    match error {
        EditError::BreaksTheRulesOfGroups(problem) => Some(in_words(project, source_looks, problem)),
        EditError::WouldDeleteEveryTrack => {
            Some("That would delete every track, and a project always has a track.".to_owned())
        }
        EditError::TooFewToMakeAClipGroup => {
            Some("A clip group is made of two things at the least: select more than one clip.".to_owned())
        }
        EditError::NothingToMakeAGroupOf { kind } => Some(format!("Select something to make the {kind} of.")),
        EditError::AlreadyInTheGroup { group, member } => Some(format!(
            "{} is in {} already.",
            the_member(project, source_looks, *member),
            the_group(project, *group)
        )),
        EditError::NotInTheGroup { group, member } => Some(format!(
            "{} is not in {}.",
            the_member(project, source_looks, *member),
            the_group(project, *group)
        )),
        EditError::UnknownGroup { .. } => Some("That group is not in the project any more.".to_owned()),
        EditError::NotAClipGroup { group } => Some(format!(
            "{} has no edges to move: it is not a clip group.",
            the_group(project, *group)
        )),
        EditError::TheEdgeWouldCutAcross { member } => Some(format!(
            "The edge cannot go there: it would cut across {}.",
            the_member(project, source_looks, *member)
        )),
        EditError::NothingWouldBeLeftInTheGroup { group } => {
            Some(format!("Nothing would be left in {}.", the_group(project, *group)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::project::groups::test_projects::lay_out;
    use crate::project::{GroupKind, TrackId};

    fn the_looks() -> SourceLooks<'static> {
        HashMap::new()
    }

    #[test]
    fn a_clip_that_would_be_inside_a_clip_group_is_named_as_the_lists_name_it_with_the_group_if_there_is_one() {
        // (The middle clip starts a little later than the others, so that it is the one that the message names, and not one of the members.)
        let mut laid = lay_out(&[&[(0, 10)], &[(3, 4)], &[(0, 10)]]);
        let error = laid
            .project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
            .unwrap_err();
        // (The group was not made, so it is not in the project to be named: it is "a group" until it is.)
        assert_eq!(
            the_reason(&laid.project, &the_looks(), &error).unwrap(),
            "\"Text object: Text (at 00:00:00.03)\" would be inside the rectangle of a group, where only the clips that are in that group may be."
        );

        // A group that is there is named, with its kind: here a clip is refused for being taken out of the middle of it.
        let group = laid
            .project
            .create_group(
                GroupKind::ClipGroup,
                &[laid.clip_member(0, 0), laid.clip_member(1, 0), laid.clip_member(2, 0)],
            )
            .unwrap();
        laid.project.rename_group(group, "Intro").unwrap();
        let error = laid
            .project
            .take_out_of_the_group(group, laid.clip_member(1, 0))
            .unwrap_err();
        assert_eq!(
            the_reason(&laid.project, &the_looks(), &error).unwrap(),
            "\"Text object: Text (at 00:00:00.03)\" would be inside the rectangle of the clip group \"Intro\", where only the clips that are in that group may be."
        );
    }

    #[test]
    fn what_a_group_cannot_hold_and_what_is_in_one_already_are_said_with_the_kind_and_the_name() {
        let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
        let first = laid
            .project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
            .unwrap();
        laid.project.rename_group(first, "Intro").unwrap();
        let error = laid
            .project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
            .unwrap_err();
        let reason = the_reason(&laid.project, &the_looks(), &error).unwrap();
        assert!(
            reason.contains("is in the clip group \"Intro\" already, and a thing can be in one clip group only"),
            "{reason}"
        );

        let error = laid
            .project
            .create_group(GroupKind::Tag, &[laid.track_member(0)])
            .unwrap_err();
        let reason = the_reason(&laid.project, &the_looks(), &error).unwrap();
        assert!(reason.starts_with("A tag cannot have \"Track: "), "{reason}");
    }

    #[test]
    fn a_track_in_the_way_of_a_track_group_and_a_clip_group_across_its_edge_are_said() {
        let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
        let error = laid
            .project
            .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(2)])
            .unwrap_err();
        let reason = the_reason(&laid.project, &the_looks(), &error).unwrap();
        assert!(
            reason.contains("is between the tracks of a group, and is not in it"),
            "{reason}"
        );

        laid.project
            .create_group(GroupKind::ClipGroup, &[laid.clip_member(1, 0), laid.clip_member(2, 0)])
            .unwrap();
        let error = laid
            .project
            .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
            .unwrap_err();
        let reason = the_reason(&laid.project, &the_looks(), &error).unwrap();
        assert!(
            reason.contains("would be partly inside") && reason.contains("partly outside it"),
            "{reason}"
        );
    }

    #[test]
    fn the_other_things_that_groups_refuse_are_said_and_what_has_nothing_to_do_with_groups_is_not() {
        let laid = lay_out(&[&[(0, 10)]]);
        let said = |error: EditError| the_reason(&laid.project, &the_looks(), &error);
        assert!(
            said(EditError::WouldDeleteEveryTrack)
                .unwrap()
                .contains("always has a track")
        );
        assert!(
            said(EditError::TooFewToMakeAClipGroup)
                .unwrap()
                .contains("more than one clip")
        );
        assert_eq!(
            said(EditError::NothingToMakeAGroupOf { kind: GroupKind::Tag }).unwrap(),
            "Select something to make the tag of."
        );
        assert!(
            said(EditError::UnknownGroup { group: GroupId::new(3) })
                .unwrap()
                .contains("not in the project")
        );
        assert!(
            said(EditError::AlreadyInTheGroup {
                group: GroupId::new(0),
                member: Member::Track(TrackId::new(0))
            })
            .unwrap()
            .contains("is in a group already")
        );
        assert_eq!(said(EditError::NoRoomThere), None);
        assert_eq!(said(EditError::UnknownTrack { track: TrackId::new(1) }), None);
    }

    #[test]
    fn every_rule_of_groups_has_words_that_say_what_it_is() {
        let laid = lay_out(&[&[(0, 10)]]);
        let (clip, group) = (laid.clip(0, 0), GroupId::new(0));
        for problem in [
            GroupProblem::HasAMemberThatIsNotThere {
                group,
                member: Member::Clip(clip),
            },
            GroupProblem::HasAMemberTwice {
                group,
                member: Member::Clip(clip),
            },
            GroupProblem::CannotHold {
                group,
                kind: GroupKind::Tag,
                member: Member::Track(laid.tracks[0]),
            },
            GroupProblem::InTwoShapes {
                member: Member::Clip(clip),
                first: group,
                second: GroupId::new(1),
            },
            GroupProblem::IsInItself { group },
            GroupProblem::HasNothingInIt { group },
            GroupProblem::HasNoBadgeOfTheGroupItIsIn {
                member: Member::Clip(clip),
                group,
            },
            GroupProblem::HasTheBadgeOfAGroupItIsNotIn {
                owner: Member::Clip(clip),
                group,
            },
            GroupProblem::HasTheBadgeOfATrackAndIsNotAClip {
                owner: Member::Track(laid.tracks[0]),
            },
            GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group, clip },
            GroupProblem::IsBetweenTheTracksOfATrackGroupWithoutBeingInIt {
                group,
                track: laid.tracks[0],
            },
            GroupProblem::ClipGroupReachesAcrossTheEdgeOfATrackGroup {
                clip_group: group,
                track_group: GroupId::new(1),
            },
        ] {
            let words = in_words(&laid.project, &the_looks(), &problem);
            assert!(words.len() > 20 && words.ends_with('.'), "{problem:?}: {words:?}");
        }
    }
}
