// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the ring of a clip goes at the edges of groups (phase 12, piece 12.3d; docs/engine-phase-12-design.md, section 7): the interpreter of the policies on made-up groups, and what the
//! plan says of the clips and the instances of a track's effects of a real project.

use super::tail_routes::{Emitter, GroupGeometry, tail_routes_of, window_routes_of};
use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::project::tracks::Clip;
use crate::project::{
    ClipTailEdge, ClipTailPolicy, EffectKind, GroupKind, GroupTailEdge, GroupTailPolicy, InstalledPlugin, Member,
    PropertyValue, Rectangle, TailLength, set_installed_plugins,
};
use crate::test_support::{
    an_installed_sound_plugin, put_an_effect_on, put_an_effect_on_a_group, the_installed_plugins_to_myself,
};

// --- the interpreter, on made-up groups ---

fn a_group(parent: Option<usize>) -> SoundGroup {
    SoundGroup {
        group: GroupId::new(0),
        effects: Vec::new(),
        parent,
    }
}

fn where_it_is(
    first_frame: u64,
    end_frame: u64,
    first_track: usize,
    last_track: usize,
    tail_edge: GroupTailEdge,
) -> GroupGeometry {
    GroupGeometry {
        rectangle: Rectangle {
            first_frame,
            end_frame,
            first_track,
            last_track,
        },
        tail_edge,
        is_a_clip_group: true,
    }
}

/// A clip on the track 0 whose ring begins at frame 30 and lasts 60 frames (to 90), that is sent to `sent_to`.
fn emitter(sent_to: Option<usize>, policy: ClipTailEdge) -> Emitter {
    Emitter {
        row: 0,
        ring_from: 30,
        ring_frames: 60,
        sent_to,
        policy,
    }
}

fn route(from_frame: u64, to: TailDestination) -> TailRoute {
    TailRoute { from_frame, to }
}

#[test]
fn a_ring_that_no_group_is_round_and_none_is_over_goes_where_its_clip_does() {
    assert_eq!(tail_routes_of(&[], &[], &emitter(None, ClipTailEdge::Cut)), Vec::new());
    let groups = [a_group(None)];
    let geometry = [where_it_is(0, 30, 1, 2, GroupTailEdge::Release)];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(None, ClipTailEdge::Cut)),
        Vec::new(),
        "a group that is on other tracks and ends before the ring begins is nothing to it"
    );
}

#[test]
fn a_ring_with_no_frames_has_no_routes_whatever_the_policies_say() {
    let groups = [a_group(None)];
    let geometry = [where_it_is(0, 40, 0, 1, GroupTailEdge::Cut)];
    let mut silent = emitter(Some(0), ClipTailEdge::Cut);
    silent.ring_frames = 0;
    assert_eq!(tail_routes_of(&groups, &geometry, &silent), Vec::new());
}

#[test]
fn keep_alive_is_what_happens_if_nobody_says_otherwise_and_the_ring_stays_in_the_group_it_was_in() {
    let groups = [a_group(None)];
    let geometry = [where_it_is(0, 40, 0, 1, GroupTailEdge::KeepAlive)];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Around)),
        Vec::new()
    );
}

#[test]
fn a_group_that_releases_lets_the_ring_go_at_its_edge_to_the_group_round_it_or_to_the_sum() {
    let geometry = [
        where_it_is(0, 40, 0, 1, GroupTailEdge::Release),
        where_it_is(0, 60, 0, 1, GroupTailEdge::Release),
    ];
    let alone = [a_group(None), a_group(None)];
    assert_eq!(
        tail_routes_of(&alone, &geometry, &emitter(Some(0), ClipTailEdge::Around)),
        [route(40, TailDestination::Sum)]
    );

    let nested = [a_group(Some(1)), a_group(None)];
    assert_eq!(
        tail_routes_of(&nested, &geometry, &emitter(Some(0), ClipTailEdge::Around)),
        [route(40, TailDestination::Group(1)), route(60, TailDestination::Sum)],
        "released from the inner group it is in the outer one, which releases it at its own edge"
    );
}

#[test]
fn what_an_inner_group_keeps_alive_is_not_the_business_of_the_group_round_it() {
    let groups = [a_group(Some(1)), a_group(None)];
    let geometry = [
        where_it_is(0, 40, 0, 1, GroupTailEdge::KeepAlive),
        where_it_is(0, 60, 0, 1, GroupTailEdge::Cut),
    ];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Around)),
        Vec::new(),
        "the ring is in the effects of the inner group and what they make is the group's"
    );

    let geometry = [
        where_it_is(0, 40, 0, 1, GroupTailEdge::Release),
        where_it_is(0, 60, 0, 1, GroupTailEdge::KeepAlive),
    ];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Around)),
        [route(40, TailDestination::Group(1))],
        "released by the inner group, kept by the outer"
    );
}

#[test]
fn a_group_that_cuts_ends_the_ring_at_its_edge() {
    let groups = [a_group(None)];
    let geometry = [where_it_is(0, 40, 0, 1, GroupTailEdge::Cut)];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Around)),
        [route(40, TailDestination::Cut)]
    );
}

#[test]
fn an_edge_that_the_ring_is_over_before_it_reaches_is_no_edge_of_it_and_one_where_it_begins_is() {
    let groups = [a_group(None)];
    let at = |end_frame| [where_it_is(0, end_frame, 0, 1, GroupTailEdge::Cut)];
    assert_eq!(
        tail_routes_of(&groups, &at(90), &emitter(Some(0), ClipTailEdge::Around)),
        Vec::new(),
        "the ring is over at frame 90, which is the edge: it does not reach it"
    );
    assert_eq!(
        tail_routes_of(&groups, &at(89), &emitter(Some(0), ClipTailEdge::Around)),
        [route(89, TailDestination::Cut)]
    );
    assert_eq!(
        tail_routes_of(&groups, &at(30), &emitter(Some(0), ClipTailEdge::Around)),
        [route(30, TailDestination::Cut)],
        "the group ends where the clip does and the ring begins"
    );
}

#[test]
fn a_clip_that_goes_around_a_group_it_enters_is_not_changed() {
    let groups = [a_group(None)];
    let geometry = [where_it_is(60, 80, 0, 1, GroupTailEdge::Cut)];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(None, ClipTailEdge::Around)),
        Vec::new()
    );
}

#[test]
fn a_clip_that_goes_into_a_group_it_enters_is_in_it_from_the_first_frame_of_the_group_until_its_edge_says_otherwise() {
    let groups = [a_group(None)];
    let routes_when_the_group = |tail_edge| {
        tail_routes_of(
            &groups,
            &[where_it_is(60, 80, 0, 1, tail_edge)],
            &emitter(None, ClipTailEdge::Into),
        )
    };
    assert_eq!(
        routes_when_the_group(GroupTailEdge::KeepAlive),
        [route(60, TailDestination::Group(0))],
        "it stays in the group, which keeps it alive"
    );
    assert_eq!(
        routes_when_the_group(GroupTailEdge::Release),
        [route(60, TailDestination::Group(0)), route(80, TailDestination::Sum)],
        "and leaves it at its edge"
    );
    assert_eq!(
        routes_when_the_group(GroupTailEdge::Cut),
        [route(60, TailDestination::Group(0)), route(80, TailDestination::Cut)],
        "or ends there"
    );
}

#[test]
fn a_clip_that_is_cut_where_it_enters_a_group_ends_there() {
    let groups = [a_group(None)];
    let geometry = [where_it_is(60, 80, 0, 1, GroupTailEdge::KeepAlive)];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(None, ClipTailEdge::Cut)),
        [route(60, TailDestination::Cut)]
    );
}

#[test]
fn a_group_that_the_ring_does_not_enter_is_nothing_to_it() {
    let groups = [a_group(None)];
    let routes = |first_frame, end_frame, first_track, last_track| {
        tail_routes_of(
            &groups,
            &[where_it_is(
                first_frame,
                end_frame,
                first_track,
                last_track,
                GroupTailEdge::Cut,
            )],
            &emitter(None, ClipTailEdge::Cut),
        )
    };
    assert_eq!(
        routes(60, 80, 1, 2),
        Vec::new(),
        "it is on other tracks: the ring stays on its own row"
    );
    assert_eq!(
        routes(10, 80, 0, 1),
        Vec::new(),
        "it began before the ring did: the ring does not enter it, it is over it"
    );
    assert_eq!(routes(90, 120, 0, 1), Vec::new(), "it begins when the ring is over");
    assert_eq!(
        routes(89, 120, 0, 1),
        [route(89, TailDestination::Cut)],
        "and one frame sooner it does not"
    );
    assert_eq!(
        routes(60, 80, 0, 0),
        [route(60, TailDestination::Cut)],
        "a rectangle that is only on its track is one"
    );
}

#[test]
fn a_group_that_the_clip_is_in_is_not_one_that_it_enters_and_the_edge_of_one_it_left_is_not_met_again() {
    let groups = [a_group(None), a_group(None)];
    let geometry = [
        where_it_is(0, 40, 0, 1, GroupTailEdge::Release),
        where_it_is(35, 80, 0, 1, GroupTailEdge::Release),
    ];
    // (The clip is in the group 0, which it leaves at frame 40; the group 1 begins at frame 35, while the ring is in the group 0.)
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Into)),
        [
            route(35, TailDestination::Group(1)),
            // (It is no longer in the group 0 at frame 40: that edge is not its.)
            route(80, TailDestination::Sum),
        ]
    );
}

#[test]
fn at_one_frame_the_edge_of_the_group_the_ring_is_in_comes_before_the_entering_of_another() {
    let groups = [a_group(None), a_group(None)];
    let geometry = [
        where_it_is(0, 40, 0, 0, GroupTailEdge::Release),
        where_it_is(40, 80, 0, 1, GroupTailEdge::Release),
    ];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Into)),
        [
            route(40, TailDestination::Sum),
            route(40, TailDestination::Group(1)),
            route(80, TailDestination::Sum),
        ]
    );
}

#[test]
fn nothing_is_changed_by_a_group_that_the_ring_enters_after_a_group_has_kept_it_alive() {
    let groups = [a_group(None), a_group(None)];
    let geometry = [
        where_it_is(0, 40, 0, 0, GroupTailEdge::KeepAlive),
        where_it_is(60, 80, 0, 1, GroupTailEdge::Cut),
    ];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(Some(0), ClipTailEdge::Cut)),
        Vec::new()
    );
}

#[test]
fn a_group_that_begins_where_the_ring_begins_is_one_that_it_enters() {
    let groups = [a_group(None)];
    let geometry = [where_it_is(30, 50, 0, 1, GroupTailEdge::KeepAlive)];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(None, ClipTailEdge::Into)),
        [route(30, TailDestination::Group(0))],
        "the clip ends at frame 30, where the group begins: the clip is not in it, and its ring is"
    );
}

#[test]
fn a_ring_that_enters_a_group_and_the_group_round_it_at_once_is_in_the_inner_one() {
    // The inner group (0) is in the outer (1); both begin at frame 60 and both reach the track of the clip.
    let groups = [a_group(Some(1)), a_group(None)];
    let geometry = [
        where_it_is(60, 70, 0, 1, GroupTailEdge::KeepAlive),
        where_it_is(60, 80, 0, 1, GroupTailEdge::KeepAlive),
    ];
    assert_eq!(
        tail_routes_of(&groups, &geometry, &emitter(None, ClipTailEdge::Into)),
        [route(60, TailDestination::Group(0))],
        "it is in the inner group, and so already in the outer one: it does not go out of the one to be in the other"
    );
}

// --- the windows of one instance for the track, on made-up groups ---

#[test]
fn a_window_inside_another_is_the_one_that_is_taken_while_it_lasts_and_the_outer_one_comes_back_if_it_is_lengthened() {
    // Row 0. The outer window is frames 90 to 230 and keeps its tails alive; the inner one is 120 to 150 and releases. The sound ends at frame 260.
    let geometry = [
        where_it_is(90, 230, 0, 1, GroupTailEdge::KeepAlive),
        where_it_is(120, 150, 0, 1, GroupTailEdge::Release),
    ];
    assert_eq!(
        window_routes_of(&geometry, 0, None, 260),
        [
            route(90, TailDestination::Group(0)),
            route(120, TailDestination::Group(1)),
            route(150, TailDestination::Group(0)),
            route(260, TailDestination::Sum),
        ]
    );
}

#[test]
fn of_two_windows_that_begin_at_one_frame_the_inner_one_is_taken() {
    let geometry = [
        where_it_is(90, 230, 0, 1, GroupTailEdge::Release),
        where_it_is(90, 120, 0, 1, GroupTailEdge::Release),
    ];
    assert_eq!(
        window_routes_of(&geometry, 0, None, 260),
        [
            route(90, TailDestination::Group(1)),
            route(120, TailDestination::Group(0)),
            route(230, TailDestination::Sum),
        ]
    );
}

#[test]
fn a_group_whose_rectangle_does_not_reach_the_track_takes_no_window_and_nor_does_a_track_group() {
    let mut track_group = where_it_is(0, 300, 0, 3, GroupTailEdge::KeepAlive);
    track_group.is_a_clip_group = false;
    let geometry = [where_it_is(90, 120, 1, 2, GroupTailEdge::Release), track_group];
    assert_eq!(window_routes_of(&geometry, 0, None, 260), Vec::new());
    assert_eq!(
        window_routes_of(&geometry, 1, Some(1), 260),
        [
            route(90, TailDestination::Group(0)),
            route(120, TailDestination::Group(1))
        ],
        "(on row 1 the group does take one, and outside it the sound goes where it was told to)"
    );
}

#[test]
fn windows_that_overlap_make_a_route_only_where_what_is_taken_changes() {
    // (The rules do not let two clip groups with effects that have clips on a track overlap in time; where they do not stop it, and for one whose clips are not on the track, they are
    // not made to give a route at every edge.)
    let geometry = [
        where_it_is(0, 40, 0, 1, GroupTailEdge::Release),
        where_it_is(10, 50, 0, 1, GroupTailEdge::Release),
    ];
    assert_eq!(
        window_routes_of(&geometry, 0, None, 100),
        [
            route(0, TailDestination::Group(0)),
            route(10, TailDestination::Group(1)),
            route(50, TailDestination::Sum),
        ]
    );
}

// --- what the plan says of a real project ---

/// A sound plugin that is installed, as a kind of effect.
fn a_kind() -> EffectKind {
    let plugin: InstalledPlugin =
        an_installed_sound_plugin("ladspa:a:routes", "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]);
    set_installed_plugins(vec![plugin]);
    EffectKind::installed_plugin_kinds()[0]
}

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

fn clip_at(project: &Project, track: usize, index: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(index).unwrap().id
}

fn the_clip(plan: &SoundPlan, strip: usize, index: usize) -> SoundClip {
    let SoundItem::Clip(clip) = &plan.strips[strip].items[index] else {
        panic!("a clip: {:?}", plan.strips[strip].items);
    };
    clip.clone()
}

/// Track 0: a clip of 30 frames (0 to 30) and one after it (30 to 60); track 1: a clip of 30 frames (0 to 30), and one 60 to 90.
fn two_tracks() -> Project {
    project_of(vec![
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (30, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
    ])
}

#[test]
fn the_ring_of_a_clip_in_a_group_that_releases_it_goes_to_the_sum_at_the_edge_of_the_group() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = two_tracks();
    let (first, other) = (clip_at(&project, 0, 0), clip_at(&project, 1, 0));
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Clip(first),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Clip(first), instance, TailLength::Milliseconds(1000))
        .unwrap();
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(other)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    assert_eq!(
        the_clip(&plan(&project), 0, 0).tail_routes,
        Vec::new(),
        "keep alive, which is what a group does if nobody says otherwise"
    );

    project
        .set_group_tail_policy(
            group,
            GroupTailPolicy {
                edge: GroupTailEdge::Release,
                ..GroupTailPolicy::default()
            },
        )
        .unwrap();
    assert_eq!(
        the_clip(&plan(&project), 0, 0).tail_routes,
        [route(30, TailDestination::Sum)],
        "the group ends at frame 30, where its last member does"
    );

    project
        .set_group_tail_policy(
            group,
            GroupTailPolicy {
                edge: GroupTailEdge::Cut,
                ..GroupTailPolicy::default()
            },
        )
        .unwrap();
    assert_eq!(
        the_clip(&plan(&project), 0, 0).tail_routes,
        [route(30, TailDestination::Cut)]
    );
}

#[test]
fn the_ring_of_a_clip_that_is_not_in_a_group_goes_into_the_group_whose_rectangle_it_reaches_after_the_clip() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    // Track 0: a clip (0 to 30) and nothing else. Track 1: nothing until a group of two clips (60 to 90) and (60 to 90) on tracks 0 and 1.
    let mut project = project_of(vec![
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (30, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![(60, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
    ]);
    let (ringing, in_group_on_0, in_group_on_1) = (
        clip_at(&project, 0, 0),
        clip_at(&project, 0, 1),
        clip_at(&project, 1, 0),
    );
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Clip(ringing),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Clip(ringing), instance, TailLength::Milliseconds(3000))
        .unwrap();
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(in_group_on_0), Member::Clip(in_group_on_1)],
        )
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    let rectangle = project.rectangle_of_the_group(group).unwrap();
    assert_eq!((rectangle.first_frame, rectangle.end_frame), (60, 90), "a premise");

    let routes_when_the_clip_says = |edge| {
        project.clone().set_clip_tail_policy_and_plan(
            ringing,
            ClipTailPolicy {
                edge,
                ..ClipTailPolicy::default()
            },
        )
    };
    assert_eq!(routes_when_the_clip_says(ClipTailEdge::Around), Vec::new());
    assert_eq!(
        routes_when_the_clip_says(ClipTailEdge::Into),
        [route(60, TailDestination::Group(0))]
    );
    assert_eq!(
        routes_when_the_clip_says(ClipTailEdge::Cut),
        [route(60, TailDestination::Cut)]
    );
}

trait PlansWithAPolicy {
    fn set_clip_tail_policy_and_plan(self, clip: crate::project::ClipId, policy: ClipTailPolicy) -> Vec<TailRoute>;
}

impl PlansWithAPolicy for Project {
    fn set_clip_tail_policy_and_plan(mut self, clip: crate::project::ClipId, policy: ClipTailPolicy) -> Vec<TailRoute> {
        self.set_clip_tail_policy(clip, policy).unwrap();
        the_clip(&plan(&self), 0, 0).tail_routes
    }
}

#[test]
fn the_ring_of_an_instance_of_the_effects_of_a_track_goes_by_the_policy_of_the_last_clip_it_hears() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = two_tracks();
    let track = project.tracks()[0].id();
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(track), instance, TailLength::Milliseconds(1000))
        .unwrap();
    let (first, second, other) = (
        clip_at(&project, 0, 0),
        clip_at(&project, 0, 1),
        clip_at(&project, 1, 0),
    );
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(first), Member::Clip(second), Member::Clip(other)],
        )
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    project
        .set_group_tail_policy(
            group,
            GroupTailPolicy {
                edge: GroupTailEdge::Release,
                ..GroupTailPolicy::default()
            },
        )
        .unwrap();

    let plan = plan(&project);

    assert_eq!(
        plan.strips[0].buses.len(),
        1,
        "a premise: the two clips of the track are one instance"
    );
    assert_eq!(
        plan.strips[0].buses[0].tail_routes,
        [route(60, TailDestination::Sum)],
        "the instance rings on after frame 60, where the group ends, and leaves the group there"
    );
    assert_eq!(
        the_clip(&plan, 0, 0).tail_routes,
        Vec::new(),
        "the clips that it hears have none of their own"
    );
}

#[test]
fn the_ring_of_an_instance_goes_into_or_ends_at_a_group_it_reaches_as_the_last_clip_it_hears_says_and_not_as_another_says()
 {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    // Row 1 has the two clips (0 to 30 and 30 to 60) that the effects of its track hear as one, and the ring of the instance is 30 frames long (to frame 90). The group is made of a
    // clip on row 0 and a clip on row 2, from frame 70 to 100, so its rectangle reaches across row 1, where there is nothing from frame 60 on.
    let mut project = project_of(vec![
        vec![(70, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![(70, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1)))],
    ]);
    let track = project.tracks()[1].id();
    let instance = put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    project
        .set_effect_tail_length(Member::Track(track), instance, TailLength::Milliseconds(1000))
        .unwrap();
    let (above, below) = (clip_at(&project, 0, 0), clip_at(&project, 2, 0));
    let (first, last) = (clip_at(&project, 1, 0), clip_at(&project, 1, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(above), Member::Clip(below)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    assert_eq!(
        plan(&project).strips[1].buses.len(),
        1,
        "a premise: one instance for the two clips"
    );
    let routes_when = |of_the_first: ClipTailEdge, of_the_last: ClipTailEdge| {
        let mut project = project.clone();
        project
            .set_clip_tail_policy(
                first,
                ClipTailPolicy {
                    edge: of_the_first,
                    ..ClipTailPolicy::default()
                },
            )
            .unwrap();
        project
            .set_clip_tail_policy(
                last,
                ClipTailPolicy {
                    edge: of_the_last,
                    ..ClipTailPolicy::default()
                },
            )
            .unwrap();
        plan(&project).strips[1].buses[0].tail_routes.clone()
    };

    assert_eq!(
        routes_when(ClipTailEdge::Cut, ClipTailEdge::Around),
        Vec::new(),
        "the first clip says cut, the last says around: the last decides"
    );
    assert_eq!(
        routes_when(ClipTailEdge::Around, ClipTailEdge::Cut),
        [route(70, TailDestination::Cut)]
    );
    assert_eq!(
        routes_when(ClipTailEdge::Around, ClipTailEdge::Into),
        [route(70, TailDestination::Group(0))]
    );
}

#[test]
fn the_clips_that_an_instance_hears_have_no_routes_of_their_own_though_their_own_effects_ring_on_and_their_group_releases_them()
 {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_kind();
    let mut project = two_tracks();
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        crate::project::AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    let (first, second, other) = (
        clip_at(&project, 0, 0),
        clip_at(&project, 0, 1),
        clip_at(&project, 1, 0),
    );
    for clip in [first, second] {
        let instance = put_an_effect_on(
            &mut project,
            crate::project::AnimationTarget::Clip(clip),
            kind,
            &[PropertyValue::Number(2.0)],
        );
        project
            .set_effect_tail_length(Member::Clip(clip), instance, TailLength::Milliseconds(1000))
            .unwrap();
    }
    let group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(first), Member::Clip(second), Member::Clip(other)],
        )
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, kind, &[PropertyValue::Number(3.0)]);
    project
        .set_group_tail_policy(
            group,
            GroupTailPolicy {
                edge: GroupTailEdge::Release,
                ..GroupTailPolicy::default()
            },
        )
        .unwrap();

    let plan = plan(&project);

    assert_eq!(
        plan.strips[0].buses.len(),
        1,
        "a premise: the two clips are one instance"
    );
    assert_eq!(the_clip(&plan, 0, 0).tail_routes, Vec::new());
    assert_eq!(
        the_clip(&plan, 0, 1).tail_routes,
        Vec::new(),
        "its own ring is in the instance, which is what leaves the group"
    );
    assert!(!plan.strips[0].buses[0].tail_routes.is_empty());
}
