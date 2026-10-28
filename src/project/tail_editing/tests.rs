// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sharing of a track and the tail policies of clips and groups (phase 12, piece 12.1): what they begin as, how they are set, and that they go where what they belong to goes
//! (a clip that is cut, copied or moved, a track that is copied). And that **nothing is made or seen differently for them yet**: the plans of the sound and of the picture are the same.

use super::*;
use crate::project::groups::test_projects::lay_out;
use crate::project::{
    ClipMove, ClipSide, ClipTailEdge, EffectSharing, GroupTailEdge, Member, TailDepth, TimelineFrameIndex, TrackSide,
};

fn an_unusual_clip_policy() -> ClipTailPolicy {
    ClipTailPolicy {
        edge: ClipTailEdge::Into,
        depth: TailDepth::InFront,
    }
}

fn an_unusual_group_policy() -> GroupTailPolicy {
    GroupTailPolicy {
        edge: GroupTailEdge::Release,
        depth: TailDepth::InFront,
    }
}

#[test]
fn everything_begins_as_the_design_says_a_run_for_each_medium_a_tail_that_goes_around_behind_and_a_group_that_keeps_it_alive()
 {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let defaults = EffectSharing::default();
    assert_eq!(
        (defaults.picture, defaults.sound),
        (TrackSharing::PerRun, TrackSharing::PerRun)
    );
    assert!(laid.project.tracks().iter().all(|track| track.sharing() == defaults));
    assert_eq!(
        laid.project.clip_tail_policy(laid.clip(0, 0)),
        Some(ClipTailPolicy::default())
    );
    assert_eq!(ClipTailPolicy::default().edge, ClipTailEdge::Around);
    assert_eq!(ClipTailPolicy::default().depth, TailDepth::Behind);
    assert_eq!(
        laid.project.group(group).unwrap().tail_policy(),
        GroupTailPolicy::default()
    );
    assert_eq!(GroupTailPolicy::default().edge, GroupTailEdge::KeepAlive);
}

#[test]
fn the_sharing_of_the_picture_effects_of_a_track_and_that_of_its_sound_effects_are_two_settings() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let (first, second) = (laid.tracks[0], laid.tracks[1]);
    laid.project
        .set_track_sharing(first, EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();
    laid.project
        .set_track_sharing(first, EffectDomain::Picture, TrackSharing::PerTrackWithWindows)
        .unwrap();
    laid.project
        .set_track_sharing(second, EffectDomain::Sound, TrackSharing::PerTrackIndivisible)
        .unwrap();

    let sharing = |track: TrackId| laid.project.track(track).unwrap().sharing();
    assert_eq!(sharing(first).of(EffectDomain::Sound), TrackSharing::PerClip);
    assert_eq!(
        sharing(first).of(EffectDomain::Picture),
        TrackSharing::PerTrackWithWindows
    );
    assert_eq!(
        sharing(second).of(EffectDomain::Sound),
        TrackSharing::PerTrackIndivisible
    );
    assert_eq!(
        sharing(second).of(EffectDomain::Picture),
        TrackSharing::PerRun,
        "the other medium of the other track is as it was"
    );
    assert_eq!(
        laid.project
            .set_track_sharing(TrackId::new(99), EffectDomain::Sound, TrackSharing::PerClip),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
}

#[test]
fn a_clip_has_a_tail_policy_of_its_own_and_a_clip_that_is_not_there_is_an_error() {
    let mut laid = lay_out(&[&[(0, 10), (10, 10)]]);
    laid.project
        .set_clip_tail_policy(laid.clip(0, 1), an_unusual_clip_policy())
        .unwrap();
    assert_eq!(
        laid.project.clip_tail_policy(laid.clip(0, 1)),
        Some(an_unusual_clip_policy())
    );
    assert_eq!(
        laid.project.clip_tail_policy(laid.clip(0, 0)),
        Some(ClipTailPolicy::default())
    );
    let missing = ClipId::new(laid.tracks[0], 99);
    assert!(matches!(
        laid.project.set_clip_tail_policy(missing, an_unusual_clip_policy()),
        Err(EditError::Clip(_))
    ));
    assert_eq!(laid.project.clip_tail_policy(missing), None);
}

#[test]
fn a_clip_that_is_cut_has_its_tail_policy_in_both_halves_and_a_copy_of_a_clip_has_it_too() {
    let mut laid = lay_out(&[&[(0, 30)]]);
    let clip = laid.clip(0, 0);
    laid.project
        .set_clip_tail_policy(clip, an_unusual_clip_policy())
        .unwrap();

    let cut = laid.project.split_at(TimelineFrameIndex::new(10), None);
    let (_, outcome) = cut[0];
    assert_eq!(
        laid.project.clip_tail_policy(outcome.first_half),
        Some(an_unusual_clip_policy())
    );
    assert_eq!(
        laid.project.clip_tail_policy(outcome.second_half),
        Some(an_unusual_clip_policy())
    );

    let copy = laid
        .project
        .duplicate_clip(outcome.second_half, ClipSide::Right)
        .unwrap();
    assert_eq!(laid.project.clip_tail_policy(copy), Some(an_unusual_clip_policy()));
}

#[test]
fn a_clip_that_is_moved_to_another_track_keeps_its_tail_policy_and_so_does_one_that_is_left_where_it_is_while_another_is_moved()
 {
    let mut laid = lay_out(&[&[(0, 10), (10, 10)], &[(0, 5)]]);
    let (moved, stays) = (laid.clip(0, 0), laid.clip(0, 1));
    laid.project
        .set_clip_tail_policy(moved, an_unusual_clip_policy())
        .unwrap();
    laid.project
        .set_clip_tail_policy(
            stays,
            ClipTailPolicy {
                edge: ClipTailEdge::Cut,
                depth: TailDepth::Behind,
            },
        )
        .unwrap();

    laid.project
        .move_clips(&[ClipMove {
            clip: moved,
            to_track: laid.tracks[1],
            start: TimelineFrameIndex::new(20),
        }])
        .unwrap();

    assert_eq!(laid.project.clip_tail_policy(moved), Some(an_unusual_clip_policy()));
    assert_eq!(
        laid.project.clip_tail_policy(stays).map(|policy| policy.edge),
        Some(ClipTailEdge::Cut)
    );
    assert!(
        laid.project
            .track(laid.tracks[1])
            .unwrap()
            .placement_of(moved)
            .is_some(),
        "it is on the other track"
    );
}

#[test]
fn a_copy_of_a_track_has_the_sharing_of_the_track_and_the_tail_policies_of_its_clips() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    laid.project
        .set_track_sharing(laid.tracks[0], EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();
    laid.project
        .set_clip_tail_policy(laid.clip(0, 0), an_unusual_clip_policy())
        .unwrap();

    let copy = laid.project.duplicate_track(laid.tracks[0], TrackSide::Below).unwrap();

    let copied = laid.project.track(copy).unwrap();
    assert_eq!(copied.sharing().sound, TrackSharing::PerClip);
    assert_eq!(copied.entries()[0].tail_policy(), an_unusual_clip_policy());
}

#[test]
fn a_clip_group_and_a_track_group_have_a_tail_policy_and_a_tag_has_none() {
    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(laid.clip(0, 0))])
        .unwrap();

    laid.project
        .set_group_tail_policy(clip_group, an_unusual_group_policy())
        .unwrap();
    laid.project
        .set_group_tail_policy(track_group, an_unusual_group_policy())
        .unwrap();

    assert_eq!(
        laid.project.group(clip_group).unwrap().tail_policy(),
        an_unusual_group_policy()
    );
    assert_eq!(
        laid.project.group(track_group).unwrap().tail_policy(),
        an_unusual_group_policy()
    );
    assert_eq!(
        laid.project.set_group_tail_policy(tag, an_unusual_group_policy()),
        Err(EditError::ATagHasNoEdge { group: tag })
    );
    assert_eq!(
        laid.project.group(tag).unwrap().tail_policy(),
        GroupTailPolicy::default()
    );
    assert_eq!(
        laid.project
            .set_group_tail_policy(GroupId::new(99), an_unusual_group_policy()),
        Err(EditError::UnknownGroup {
            group: GroupId::new(99)
        })
    );
}

#[test]
fn an_effect_has_a_tail_length_that_its_person_can_say_and_that_a_copy_of_it_has_too() {
    use crate::project::{EffectKind, LONGEST_TAIL_IN_MILLISECONDS};

    let mut laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let clip = laid.clip_member(0, 0);
    let clip_target = crate::project::AnimationTarget::Clip(laid.clip(0, 0));
    let instance = laid.project.add_effect(clip_target, EffectKind::HUE_SHIFT).unwrap();
    let tail_of = |project: &Project, instance| {
        project
            .chain_of_the_member(clip)
            .unwrap()
            .instance(instance)
            .unwrap()
            .tail_length()
    };
    assert_eq!(tail_of(&laid.project, instance), TailLength::Automatic);

    laid.project
        .set_effect_tail_length(clip, instance, TailLength::Milliseconds(2500))
        .unwrap();
    assert_eq!(tail_of(&laid.project, instance), TailLength::Milliseconds(2500));
    laid.project
        .set_effect_tail_length(clip, instance, TailLength::Milliseconds(u32::MAX))
        .unwrap();
    assert_eq!(
        tail_of(&laid.project, instance),
        TailLength::Milliseconds(LONGEST_TAIL_IN_MILLISECONDS),
        "ten minutes is the most"
    );
    laid.project
        .set_effect_tail_length(clip, instance, TailLength::None)
        .unwrap();
    let copy = laid.project.duplicate_effect(clip, instance).unwrap();
    assert_eq!(
        tail_of(&laid.project, copy),
        TailLength::None,
        "a copy has what the effect has"
    );

    // The track, a group and what is not there.
    let track = laid.track_member(0);
    let on_the_track = laid
        .project
        .add_effect(
            crate::project::AnimationTarget::Track(laid.tracks[0]),
            EffectKind::HUE_SHIFT,
        )
        .unwrap();
    laid.project
        .set_effect_tail_length(track, on_the_track, TailLength::Milliseconds(10))
        .unwrap();
    assert_eq!(
        laid.project
            .set_effect_tail_length(clip, EffectInstanceId::new(99), TailLength::None),
        Err(EditError::NotInTheChain {
            owner: clip,
            item: crate::project::ChainItemKey::Effect(EffectInstanceId::new(99))
        })
    );
    assert_eq!(
        laid.project
            .set_effect_tail_length(Member::Group(GroupId::new(9)), instance, TailLength::None),
        Err(EditError::UnknownGroup { group: GroupId::new(9) })
    );
}
