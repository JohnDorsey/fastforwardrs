// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two rules of phase 12 about the sharing of a track's effects (docs/engine-phase-12-design.md, sections 5.3 and 5.4): **a clip group that has an effect that is switched on cannot take
//! part of a track whose effects of that medium are indivisible**, and **two clip groups that are not in one another cannot both have clips on a track whose effects of that medium give each
//! group a window, if they overlap in time**. Each is about one medium: the picture effects of a group and the sound effects of a track do not meet.

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{EditError, EffectDomain, EffectKind, TimelineFrameIndex, TrackSharing, set_installed_plugins};
use crate::test_support::{an_installed_sound_plugin, the_installed_plugins_to_myself};

/// Three clips on the first track and one on the second; a clip group of the first clip of each. (The first track has clips that are not in it.)
fn a_group_that_takes_part_of_a_track() -> (Laid, GroupId) {
    let mut laid = lay_out(&[&[(0, 10), (10, 10), (20, 10)], &[(0, 10)]]);
    let group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    (laid, group)
}

fn indivisible_picture(laid: &mut Laid, track: usize) -> Result<(), EditError> {
    laid.project.set_track_sharing(
        laid.tracks[track],
        EffectDomain::Picture,
        TrackSharing::PerTrackIndivisible,
    )
}

fn cuts(group: GroupId, track: TrackId, domain: EffectDomain) -> EditError {
    EditError::BreaksTheRulesOfGroups(GroupProblem::CutsAnIndivisibleTrack { group, track, domain })
}

// --- a track that is indivisible ---

#[test]
fn a_track_cannot_be_made_indivisible_while_a_clip_group_with_an_effect_takes_part_of_it() {
    let (mut laid, group) = a_group_that_takes_part_of_a_track();
    laid.project
        .add_effect_to_the_group(group, EffectKind::HUE_SHIFT)
        .unwrap();

    assert_eq!(
        indivisible_picture(&mut laid, 0),
        Err(cuts(group, laid.tracks[0], EffectDomain::Picture))
    );
    assert_eq!(
        laid.project.track(laid.tracks[0]).unwrap().sharing().picture,
        TrackSharing::PerRun,
        "the track is as it was"
    );
    assert_eq!(laid.project.problems_with_the_groups(), []);
    // The other track has all its clips in the group; the other medium has no effect on the group; and a track that gives windows is not the same.
    assert_eq!(indivisible_picture(&mut laid, 1), Ok(()));
    assert_eq!(
        laid.project
            .set_track_sharing(laid.tracks[0], EffectDomain::Sound, TrackSharing::PerTrackIndivisible),
        Ok(())
    );
    assert_eq!(
        laid.project
            .set_track_sharing(laid.tracks[0], EffectDomain::Picture, TrackSharing::PerTrackWithWindows),
        Ok(())
    );
}

#[test]
fn a_clip_group_cannot_be_given_an_effect_that_makes_it_take_part_of_an_indivisible_track_and_the_effect_is_not_given()
{
    let (mut laid, group) = a_group_that_takes_part_of_a_track();
    indivisible_picture(&mut laid, 0).unwrap();

    assert_eq!(
        laid.project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT),
        Err(cuts(group, laid.tracks[0], EffectDomain::Picture))
    );
    assert_eq!(laid.project.group(group).unwrap().effects().iter().count(), 0);
}

#[test]
fn an_effect_of_a_group_that_is_switched_off_does_not_count_and_switching_it_on_is_refused_where_it_would_cut() {
    let (mut laid, group) = a_group_that_takes_part_of_a_track();
    let instance = laid
        .project
        .add_effect_to_the_group(group, EffectKind::HUE_SHIFT)
        .unwrap();
    laid.project
        .set_effect_enabled(Member::Group(group), instance, false)
        .unwrap();

    assert_eq!(
        indivisible_picture(&mut laid, 0),
        Ok(()),
        "a group that does nothing cuts nothing"
    );
    assert_eq!(
        laid.project.set_effect_enabled(Member::Group(group), instance, true),
        Err(cuts(group, laid.tracks[0], EffectDomain::Picture))
    );
    assert!(
        !laid
            .project
            .group(group)
            .unwrap()
            .effects()
            .instance(instance)
            .unwrap()
            .is_enabled()
    );
}

#[test]
fn a_clip_group_that_has_all_the_clips_of_a_track_does_not_cut_it_and_neither_does_a_tag_or_a_track_group() {
    let mut laid = lay_out(&[&[(0, 10), (10, 10)], &[(0, 20)]]);
    let all = laid
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[laid.clip_member(0, 0), laid.clip_member(0, 1), laid.clip_member(1, 0)],
        )
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let tracks = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    indivisible_picture(&mut laid, 0).unwrap();
    indivisible_picture(&mut laid, 1).unwrap();

    laid.project
        .add_effect_to_the_group(all, EffectKind::HUE_SHIFT)
        .unwrap();
    laid.project
        .add_effect_to_the_group(tag, EffectKind::HUE_SHIFT)
        .unwrap();
    laid.project
        .add_effect_to_the_group(tracks, EffectKind::HUE_SHIFT)
        .unwrap();

    assert_eq!(laid.project.problems_with_the_groups(), []);
}

#[test]
fn a_clip_cannot_be_put_on_an_indivisible_track_where_a_group_with_an_effect_has_all_the_clips_and_the_clip_would_not_be_in_it()
 {
    let mut laid = lay_out(&[&[(0, 10), (10, 10)], &[(0, 20)]]);
    let all = laid
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[laid.clip_member(0, 0), laid.clip_member(0, 1), laid.clip_member(1, 0)],
        )
        .unwrap();
    laid.project
        .add_effect_to_the_group(all, EffectKind::HUE_SHIFT)
        .unwrap();
    indivisible_picture(&mut laid, 0).unwrap();

    let before = laid.project.clone();
    let put = laid
        .project
        .insert_a_text_clip_that_has_no_text_effect(laid.tracks[0], TimelineFrameIndex::new(40), 10);

    assert!(
        put.is_none(),
        "a clip that is not in the group would make the group take part of the track"
    );
    assert_eq!(laid.project, before, "and the project is as it was");
}

#[test]
fn the_sound_effects_of_a_group_and_the_sound_sharing_of_a_track_are_a_pair_of_their_own() {
    let _alone = the_installed_plugins_to_myself();
    let (mut laid, group) = a_group_that_takes_part_of_a_track();
    set_installed_plugins(vec![an_installed_sound_plugin(
        "clap:sharing:one",
        "Somebody",
        &[("amount", 1.0, Some((0.0, 4.0)))],
    )]);
    let sound = EffectKind::installed_plugin_kinds()[0];
    // (A clip group cannot be given a sound effect yet, which a later piece allows; the rule is about the effects a group has, whichever way it came to have them.)
    laid.project.group_mut(group).unwrap().effects_mut().add(sound);

    assert_eq!(
        laid.project
            .set_track_sharing(laid.tracks[0], EffectDomain::Sound, TrackSharing::PerTrackIndivisible),
        Err(cuts(group, laid.tracks[0], EffectDomain::Sound))
    );
    assert_eq!(
        indivisible_picture(&mut laid, 0),
        Ok(()),
        "the group has no picture effect"
    );
}

// --- windows ---

/// The layout the grid of three tracks found: the groups `a` (tracks 1 and 2, frames 10 to 30) and `b` (tracks 2 and 3, frames 20 to 40) have clips on the second track and
/// overlap in time, with no clip where they overlap on it.
fn two_groups_whose_windows_overlap() -> (Laid, GroupId, GroupId) {
    let mut laid = lay_out(&[&[(20, 10)], &[(0, 10), (10, 10), (30, 10)], &[(20, 10)]]);
    let a = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 1)])
        .unwrap();
    let b = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(1, 2), laid.clip_member(2, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(a, EffectKind::HUE_SHIFT).unwrap();
    laid.project.add_effect_to_the_group(b, EffectKind::HUE_SHIFT).unwrap();
    (laid, a, b)
}

#[test]
fn two_groups_with_effects_that_overlap_in_time_cannot_both_take_windows_of_one_track() {
    let (mut laid, a, b) = two_groups_whose_windows_overlap();
    let middle = laid.tracks[1];

    assert_eq!(
        laid.project
            .set_track_sharing(middle, EffectDomain::Picture, TrackSharing::PerTrackWithWindows),
        Err(EditError::BreaksTheRulesOfGroups(GroupProblem::OverlappingWindows {
            first: a,
            second: b,
            track: middle,
            domain: EffectDomain::Picture,
        }))
    );
    for allowed in [
        TrackSharing::PerClip,
        TrackSharing::PerRun,
        TrackSharing::PerTrackIndivisible,
    ] {
        // (Indivisible is refused for another reason: each group has some of the clips of the track.)
        let outcome = laid.project.set_track_sharing(middle, EffectDomain::Picture, allowed);
        assert_eq!(
            outcome.is_ok(),
            allowed != TrackSharing::PerTrackIndivisible,
            "{allowed:?}: {outcome:?}"
        );
    }
    assert_eq!(
        laid.project
            .set_track_sharing(middle, EffectDomain::Sound, TrackSharing::PerTrackWithWindows),
        Ok(()),
        "the groups have no sound effects: their windows of the sound do not overlap"
    );
}

#[test]
fn groups_that_do_not_overlap_in_time_or_are_in_one_another_can_take_windows_of_one_track() {
    // Apart in time: the first has the first two clips, the second the last two (the last clip of the first track is in neither).
    let mut laid = lay_out(&[&[(0, 10), (10, 10), (30, 10), (40, 10)], &[(0, 10), (40, 10)]]);
    let first = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(0, 1)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 2), laid.clip_member(0, 3)])
        .unwrap();
    laid.project
        .add_effect_to_the_group(first, EffectKind::HUE_SHIFT)
        .unwrap();
    laid.project
        .add_effect_to_the_group(second, EffectKind::HUE_SHIFT)
        .unwrap();
    assert_eq!(
        laid.project
            .set_track_sharing(laid.tracks[0], EffectDomain::Picture, TrackSharing::PerTrackWithWindows),
        Ok(())
    );

    // One in the other: an outer group that holds the inner and one more clip.
    let mut nested = lay_out(&[&[(0, 10), (10, 10), (20, 10)]]);
    let inner = nested
        .project
        .create_group(
            GroupKind::ClipGroup,
            &[nested.clip_member(0, 0), nested.clip_member(0, 1)],
        )
        .unwrap();
    let outer = nested
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), nested.clip_member(0, 2)])
        .unwrap();
    nested
        .project
        .add_effect_to_the_group(inner, EffectKind::HUE_SHIFT)
        .unwrap();
    nested
        .project
        .add_effect_to_the_group(outer, EffectKind::HUE_SHIFT)
        .unwrap();
    assert_eq!(
        nested.project.set_track_sharing(
            nested.tracks[0],
            EffectDomain::Picture,
            TrackSharing::PerTrackWithWindows
        ),
        Ok(()),
        "the window of the inner group is in that of the outer one: which takes it first is clear"
    );
}

#[test]
fn a_group_that_is_given_an_effect_where_it_would_overlap_the_window_of_another_is_refused() {
    let (mut laid, _, b) = two_groups_whose_windows_overlap();
    laid.project
        .set_track_sharing(laid.tracks[1], EffectDomain::Picture, TrackSharing::PerTrackWithWindows)
        .expect_err("it is refused while both have effects");
    // Take the effect off one, make the track give windows, and the effect cannot be put back.
    let instance = laid
        .project
        .group(b)
        .unwrap()
        .effects()
        .iter()
        .next()
        .unwrap()
        .instance();
    laid.project.remove_effect_from_the_group(b, instance).unwrap();
    laid.project
        .set_track_sharing(laid.tracks[1], EffectDomain::Picture, TrackSharing::PerTrackWithWindows)
        .unwrap();

    let given = laid.project.add_effect_to_the_group(b, EffectKind::HUE_SHIFT);

    assert!(
        matches!(
            given,
            Err(EditError::BreaksTheRulesOfGroups(
                GroupProblem::OverlappingWindows { .. }
            ))
        ),
        "{given:?}"
    );
}
