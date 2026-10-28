// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sharing of the sound effects of a track as one instance for the track (phase 12, piece 12.4; docs/engine-phase-12-design.md, section 5): *per track, with windows* and *per track,
//! indivisible*.

use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::project::tracks::Clip;
use crate::project::{
    EffectKind, GroupKind, GroupTailEdge, GroupTailPolicy, InstalledPlugin, Member, PropertyValue, TailLength,
    set_installed_plugins,
};
use crate::test_support::{
    an_installed_sound_plugin, put_an_effect_on, put_an_effect_on_a_group, the_installed_plugins_to_myself,
};

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

fn a_kind() -> EffectKind {
    let plugin: InstalledPlugin =
        an_installed_sound_plugin("ladspa:a:sharing", "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]);
    set_installed_plugins(vec![plugin]);
    EffectKind::installed_plugin_kinds()[0]
}

fn clip_at(project: &Project, track: usize, index: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(index).unwrap().id
}

fn route(from_frame: u64, to: TailDestination) -> TailRoute {
    TailRoute { from_frame, to }
}

fn ranges(strip: &SoundStrip) -> Vec<(usize, usize)> {
    strip.buses.iter().map(|bus| (bus.first_item, bus.last_item)).collect()
}

/// Track 0 has a clip (0 to 30), then 60 frames of nothing, then a clip (90 to 120), then 80 frames of nothing and a clip (200 to 230); track 1 has a clip (90 to 120) and one (200 to 230).
/// The effect of track 0 rings on for a second (30 frames).
fn a_track_with_gaps_and_an_effect(sharing: TrackSharing) -> Project {
    let mut project = project_of(vec![
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (60, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (80, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![
            (90, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (80, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
    ]);
    let track = project.tracks()[0].id();
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Track(track),
        a_kind(),
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(track), instance, TailLength::Milliseconds(1000))
        .unwrap();
    project
        .set_track_sharing(track, crate::project::EffectDomain::Sound, sharing)
        .unwrap();
    project
}

#[test]
fn with_one_copy_for_each_run_a_silence_longer_than_the_tail_ends_a_run_and_with_one_instance_for_the_track_it_does_not()
 {
    let _alone = the_installed_plugins_to_myself();

    assert_eq!(
        ranges(&plan(&a_track_with_gaps_and_an_effect(TrackSharing::PerRun)).strips[0]),
        [(0, 0), (2, 2), (4, 4)],
        "the silences are 60 and 80 frames and the tail is 30: each clip has an instance"
    );
    for sharing in [TrackSharing::PerTrackWithWindows, TrackSharing::PerTrackIndivisible] {
        let plan = plan(&a_track_with_gaps_and_an_effect(sharing));
        assert_eq!(
            ranges(&plan.strips[0]),
            [(0, 4)],
            "{sharing:?}: one instance, which hears the silences too"
        );
        assert_eq!(plan.strips[0].buses[0].sent_to, None);
    }
}

#[test]
fn the_clips_of_an_instance_for_the_whole_track_go_where_the_group_that_has_all_of_them_is() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = a_track_with_gaps_and_an_effect(TrackSharing::PerTrackIndivisible);
    let clips: Vec<Member> = (0..3).map(|index| Member::Clip(clip_at(&project, 0, index))).collect();
    let group = project.create_group(GroupKind::ClipGroup, &clips).unwrap();
    put_an_effect_on_a_group(&mut project, group, a_kind(), &[PropertyValue::Number(3.0)]);

    let plan = plan(&project);

    assert_eq!(plan.groups.len(), 1);
    assert_eq!(ranges(&plan.strips[0]), [(0, 4)]);
    assert_eq!(
        plan.strips[0].buses[0].sent_to,
        Some(0),
        "all of the track is in the group: the instance sends what it makes to it"
    );
}

/// A clip group of the clip of track 0 that is at `index` and the clip of track 1 that is at the same place, with a sound effect.
fn a_group_over_the_clips_at(project: &mut Project, index: usize) -> crate::project::GroupId {
    let (on_the_track, below) = (clip_at(project, 0, index + 1), clip_at(project, 1, index));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(on_the_track), Member::Clip(below)])
        .unwrap();
    put_an_effect_on_a_group(project, group, a_kind(), &[PropertyValue::Number(3.0)]);
    group
}

fn with_the_edge(project: &mut Project, group: crate::project::GroupId, edge: GroupTailEdge) {
    project
        .set_group_tail_policy(
            group,
            GroupTailPolicy {
                edge,
                ..GroupTailPolicy::default()
            },
        )
        .unwrap();
}

#[test]
fn a_group_takes_a_window_of_the_one_instance_for_the_time_of_its_rectangle_and_the_ring_after_it_if_it_keeps_its_tails_alive()
 {
    let _alone = the_installed_plugins_to_myself();
    let mut project = a_track_with_gaps_and_an_effect(TrackSharing::PerTrackWithWindows);
    let group = a_group_over_the_clips_at(&mut project, 0);

    let plan = plan(&project);

    assert_eq!(ranges(&plan.strips[0]), [(0, 4)], "one instance for the whole track");
    assert_eq!(
        plan.strips[0].buses[0].tail_routes,
        [route(90, TailDestination::Group(0)), route(260, TailDestination::Sum)],
        "the window is frames 90 to 120, the rectangle of the group, and it is lengthened to the end of the sound: the last clip ends at 230 and the ring is 30 frames more"
    );

    with_the_edge(&mut project, group, GroupTailEdge::Release);
    assert_eq!(
        crate::composition::plan_sound(&project, TimelineFrameIndex::new(0), |_| true).strips[0].buses[0].tail_routes,
        [route(90, TailDestination::Group(0)), route(120, TailDestination::Sum)],
        "a group that releases has the rectangle for its window"
    );

    with_the_edge(&mut project, group, GroupTailEdge::Cut);
    assert_eq!(
        crate::composition::plan_sound(&project, TimelineFrameIndex::new(0), |_| true).strips[0].buses[0].tail_routes,
        [route(90, TailDestination::Group(0)), route(120, TailDestination::Sum)],
        "a window cannot cut (what is after its edge is also what other clips make): it releases"
    );
}

#[test]
fn a_window_that_is_lengthened_ends_where_the_next_window_begins_and_does_not_come_back() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = a_track_with_gaps_and_an_effect(TrackSharing::PerTrackWithWindows);
    a_group_over_the_clips_at(&mut project, 0);
    a_group_over_the_clips_at(&mut project, 1);

    let plan = plan(&project);

    assert_eq!(
        plan.strips[0].buses[0].tail_routes,
        [
            route(90, TailDestination::Group(0)),
            route(200, TailDestination::Group(1)),
            route(260, TailDestination::Sum),
        ],
        "the first window is lengthened as far as the second begins, and the second as far as the sound goes"
    );
}

#[test]
fn outside_the_windows_the_instance_sends_what_it_makes_to_the_track_group_that_has_sound_effects_and_holds_the_track()
{
    let _alone = the_installed_plugins_to_myself();
    let mut project = a_track_with_gaps_and_an_effect(TrackSharing::PerTrackWithWindows);
    let (first, second) = (project.tracks()[0].id(), project.tracks()[1].id());
    let track_group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(first), Member::Track(second)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, track_group, a_kind(), &[PropertyValue::Number(4.0)]);
    a_group_over_the_clips_at(&mut project, 0);

    let plan = plan(&project);

    let place_of = |group| plan.groups.iter().position(|node| node.group == group).unwrap();
    let (outer, inner) = (place_of(track_group), 1 - place_of(track_group));
    assert_eq!(
        plan.groups[inner].parent,
        Some(outer),
        "a premise: the clip group is in the track group"
    );
    assert_eq!(plan.strips[0].buses[0].sent_to, Some(outer));
    assert_eq!(
        plan.strips[0].buses[0].tail_routes,
        [
            route(90, TailDestination::Group(inner)),
            route(260, TailDestination::Group(outer))
        ]
    );
}

#[test]
fn a_group_whose_rectangle_passes_over_the_track_without_a_clip_of_it_takes_a_window_too() {
    let _alone = the_installed_plugins_to_myself();
    // Row 1 has the clip with the effect (0 to 30); the group is made of a clip on row 0 and a clip on row 2, from 90 to 120, and reaches across row 1, where it has nothing.
    let mut project = project_of(vec![
        vec![(90, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
        vec![(90, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
    ]);
    let track = project.tracks()[1].id();
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Track(track),
        a_kind(),
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(track), instance, TailLength::Milliseconds(2000))
        .unwrap();
    project
        .set_track_sharing(
            track,
            crate::project::EffectDomain::Sound,
            TrackSharing::PerTrackWithWindows,
        )
        .unwrap();
    let (above, below) = (clip_at(&project, 0, 0), clip_at(&project, 2, 0));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(above), Member::Clip(below)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, a_kind(), &[PropertyValue::Number(3.0)]);
    with_the_edge(&mut project, group, GroupTailEdge::Release);

    let plan = plan(&project);

    assert_eq!(
        plan.strips[1].buses[0].tail_routes,
        [route(90, TailDestination::Group(0)), route(120, TailDestination::Sum)],
        "what the instance still rings with at frame 90 (its ring is three seconds) is what the window takes"
    );
}

#[test]
fn a_track_that_has_no_window_taken_has_no_routes_and_the_other_sharings_make_no_windows() {
    let _alone = the_installed_plugins_to_myself();
    for sharing in [
        TrackSharing::PerClip,
        TrackSharing::PerRun,
        TrackSharing::PerTrackIndivisible,
    ] {
        let mut project = a_track_with_gaps_and_an_effect(sharing);
        if sharing != TrackSharing::PerTrackIndivisible {
            a_group_over_the_clips_at(&mut project, 0);
        }
        let plan = plan(&project);
        assert!(
            plan.strips[0].buses.iter().all(|bus| bus.tail_routes.is_empty()),
            "{sharing:?}: no windows (and a group that keeps its tails alive does not change where a ring goes)"
        );
    }
}

#[test]
fn the_instance_sends_what_it_makes_outside_the_windows_where_the_track_is_and_not_where_its_first_clip_is_sent() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = project_of(vec![
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (60, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
    ]);
    let (first, second) = (project.tracks()[0].id(), project.tracks()[1].id());
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Track(first),
        a_kind(),
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(first), instance, TailLength::Milliseconds(1000))
        .unwrap();
    project
        .set_track_sharing(
            first,
            crate::project::EffectDomain::Sound,
            TrackSharing::PerTrackWithWindows,
        )
        .unwrap();
    let track_group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(first), Member::Track(second)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, track_group, a_kind(), &[PropertyValue::Number(4.0)]);
    // A group of the first clip of the track and the clip on the other track that is at the same time: the first clip is sent to it.
    let (on_the_track, below) = (clip_at(&project, 0, 0), clip_at(&project, 1, 0));
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(on_the_track), Member::Clip(below)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, clip_group, a_kind(), &[PropertyValue::Number(3.0)]);

    let plan = plan(&project);

    let place_of = |group| plan.groups.iter().position(|node| node.group == group).unwrap();
    let (outer, inner) = (place_of(track_group), place_of(clip_group));
    assert_eq!(
        the_first_clip_of(&plan.strips[0]).sent_to,
        Some(inner),
        "a premise: the first clip is sent to its group"
    );
    assert_eq!(
        plan.strips[0].buses[0].sent_to,
        Some(outer),
        "the instance sends what it makes, outside the windows, to the track group"
    );
}

fn the_first_clip_of(strip: &SoundStrip) -> &SoundClip {
    match &strip.items[0] {
        SoundItem::Clip(clip) => clip,
        SoundItem::Silence { .. } => panic!("a clip first"),
    }
}
