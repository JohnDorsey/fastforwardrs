// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the ring of a clip, or of an instance of a track's effects, goes at the edges of groups, in the block graph (phase 12, piece 12.3d; docs/engine-phase-12-design.md, section 7): it
//! is made once, in order, and each place it goes to takes its share, with a crossfade over the edge fade where the share changes.

use super::groups::{GroupInsertions, Scales};
use super::*;
use crate::composition::{TailDestination, TailRoute};

fn route(from_frame: u64, to: TailDestination) -> TailRoute {
    TailRoute { from_frame, to }
}

/// A clip of `frames` frames that has the effect that rings on in its chain, is sent to `sent_to`, and whose ring goes where `routes` say.
fn a_ringing_clip(
    frames: u64,
    effect: &crate::composition::LayerEffect,
    sent_to: Option<usize>,
    routes: Vec<TailRoute>,
) -> SoundItemToExport {
    let SoundItemToExport::Clip(mut clip) = a_clip_that_has(frames, LoudnessToExport::UNCHANGED, effect) else {
        unreachable!("a clip");
    };
    clip.sent_to = sent_to;
    clip.tail_routes = routes;
    SoundItemToExport::Clip(clip)
}

/// A clip with nothing done to it that is sent to the group `group`.
fn a_member(frames: u64, group: usize) -> SoundItemToExport {
    let SoundItemToExport::Clip(mut clip) = clip(frames, LoudnessToExport::UNCHANGED) else {
        unreachable!("a clip");
    };
    clip.sent_to = Some(group);
    SoundItemToExport::Clip(clip)
}

/// The specification: strip 0 is the ringing clip, strip 1 holds the member of the group 0 that the group is there for (so that the group hears something: it is made when its first
/// member is heard), and the group halves what it is given. The fade is `fade_milliseconds`.
fn the_ringing_clip_and_a_group(
    clip_in_strip_0: SoundItemToExport,
    total_frames: u64,
    fade_milliseconds: u32,
    member_frames: u64,
) -> SoundGraphSpecification {
    let mut specification = specification_with_buses(
        thirty(),
        total_frames,
        vec![
            (vec![clip_in_strip_0], Vec::new()),
            (vec![a_member(member_frames, 0)], Vec::new()),
        ],
    );
    specification.groups = vec![SoundGroupToExport {
        effects: Vec::new(),
        parent: None,
    }];
    specification.edge_fade_milliseconds = fade_milliseconds;
    specification
}

use crate::media::export::SoundGroupToExport;

/// What the group does: halves what it is given.
fn halving() -> GroupInsertionsWithEchoes {
    GroupInsertionsWithEchoes {
        inside: GroupInsertions::new(|_| vec![Box::new(Scales(0.5))]),
        items: &[0],
    }
}

/// Gives the echo to the clips of strip 0 and what the group insertions give to the groups and buses.
struct GroupInsertionsWithEchoes {
    inside: GroupInsertions,
    /// The items that have an echo (of strip 0).
    items: &'static [usize],
}

impl Insertions for GroupInsertionsWithEchoes {
    fn for_clip(&mut self, request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        if request.strip == 0 && self.items.contains(&request.item) {
            vec![Box::<Echoes>::default()]
        } else {
            Vec::new()
        }
    }

    fn for_bus(&mut self, request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        self.inside.for_bus(request)
    }

    fn for_group(&mut self, request: &GroupRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        self.inside.for_group(request)
    }
}

/// The sound of strip 0, the clip whose noise is the ramp, and of its echo for `ring_samples` more samples: what `Echoes` gives.
fn the_echo_of_the_clip(frames: u64, ring_samples: usize) -> Vec<f32> {
    echoed(
        (0..frames as usize * 1600)
            .map(|index| value_of(0, 0, index))
            .chain(std::iter::repeat_n(0.0, ring_samples)),
    )
}

fn the_left_of(specification: SoundGraphSpecification, insertions: impl Insertions + Send + 'static) -> Vec<f32> {
    let (left, right) =
        SoundGraph::new(specification, Box::new(Ramps::default()), Box::new(insertions)).render_to_the_end(512);
    // (The noise on the right is the noise on the left with the sign changed, and everything done to it is the same for both channels: the right is the left, negated, all the way.)
    assert!(
        left.iter().zip(&right).all(|(left, right)| *right == -*left),
        "the right channel is the left one with the sign changed"
    );
    left
}

#[test]
fn a_ring_that_a_group_releases_goes_to_the_sum_from_the_edge_of_the_group_and_what_the_group_does_is_done_to_what_was_before_it()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    // The clip is a frame, a member of the group; the group ends where it does, at sample 1600; the ring (960 samples) leaves the group there.
    let left = the_left_of(
        the_ringing_clip_and_a_group(
            a_ringing_clip(1, &effect, Some(0), vec![route(1, TailDestination::Sum)]),
            3,
            0,
            1,
        ),
        halving(),
    );

    let echo = the_echo_of_the_clip(1, 960);
    let member = |index: usize| value_of(1, 0, index);
    for (index, sample) in left.iter().enumerate() {
        // What the group is given is what the clip makes and what the member makes, added, and the group halves it. The ring after the edge is the echo of the clip, as it is.
        let expected = if index < 1600 {
            (echo[index] + member(index)) * 0.5
        } else if index < 1600 + 960 {
            echo[index]
        } else {
            0.0
        };
        assert_eq!(*sample, expected, "sample {index}");
    }
}

#[test]
fn a_ring_that_is_cut_ends_at_the_edge_and_the_effects_that_made_it_are_let_go_of_then() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let insertions = GroupInsertionsWithEchoes {
        inside: GroupInsertions::new(|_| Vec::new()),
        items: &[0],
    };

    let left = the_left_of(
        the_ringing_clip_and_a_group(
            a_ringing_clip(1, &effect, Some(0), vec![route(1, TailDestination::Cut)]),
            3,
            0,
            1,
        ),
        insertions,
    );

    let echo = the_echo_of_the_clip(1, 0);
    for (index, sample) in left.iter().enumerate() {
        let expected = if index < 1600 {
            echo[index] + value_of(1, 0, index)
        } else {
            0.0
        };
        assert_eq!(*sample, expected, "sample {index}");
    }
}

#[test]
fn a_ring_that_goes_into_a_group_is_done_to_by_its_effects_from_the_frame_it_goes_in() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    // The clip is not in the group (it is sent to the sum); its ring goes into the group from the frame 1 on, which begins at sample 1600, as the clip ends.
    let mut specification = the_ringing_clip_and_a_group(
        a_ringing_clip(1, &effect, None, vec![route(1, TailDestination::Group(0))]),
        4,
        0,
        3,
    );
    // (The member of the group is a long one, so the group is there for the whole of the ring.)
    specification.strips[1].items = vec![a_member(3, 0)];

    let left = the_left_of(specification, halving());

    let echo = the_echo_of_the_clip(1, 960);
    for (index, sample) in left.iter().enumerate() {
        let member = if index < 4800 { value_of(1, 0, index) * 0.5 } else { 0.0 };
        let ring = if index < 1600 {
            echo[index]
        } else if index < 1600 + 960 {
            echo[index] * 0.5
        } else {
            0.0
        };
        assert_eq!(*sample, ring + member, "sample {index}");
    }
}

#[test]
fn the_share_of_a_ring_changes_over_the_edge_fade_so_that_what_goes_to_both_adds_up_to_all_of_it() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    // 10 ms is 480 samples: over them the ring goes less and less to the group (which halves it) and more and more to the sum.
    let left = the_left_of(
        the_ringing_clip_and_a_group(
            a_ringing_clip(1, &effect, Some(0), vec![route(1, TailDestination::Sum)]),
            3,
            10,
            1,
        ),
        halving(),
    );

    let echo = the_echo_of_the_clip(1, 960);
    for index in [1600, 1601, 1800, 1839, 2079, 2080, 2500] {
        let x = ((index - 1600) as f32 / 480.0).min(1.0);
        // (The member of the group ends where the ring leaves, so the group is there for the fade only because the ring is still going to it: the ring is shared between the group,
        // which halves it, and the sum, which does not.)
        let expected = echo[index] * ((1.0 - x) * 0.5 + x);
        assert!(
            (left[index] - expected).abs() <= expected.abs() * 1e-5 + 1e-4,
            "sample {index}: {} and {expected}",
            left[index]
        );
    }
}

#[test]
fn a_ring_that_is_cut_fades_out_over_the_edge_fade() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(crate::project::TailLength::Milliseconds(100));
    let left = the_left_of(
        the_ringing_clip_and_a_group(
            a_ringing_clip(1, &effect, None, vec![route(1, TailDestination::Cut)]),
            4,
            10,
            1,
        ),
        GroupInsertionsWithEchoes {
            inside: GroupInsertions::new(|_| Vec::new()),
            items: &[0],
        },
    );

    let echo = the_echo_of_the_clip(1, 4800);
    for index in [1600, 1601, 1840, 2079] {
        let weight = 1.0 - (index - 1600) as f32 / 480.0;
        let expected = echo[index] * weight;
        assert!(
            (left[index] - expected).abs() <= expected.abs() * 1e-5 + 1e-4,
            "sample {index}: {} and {expected}",
            left[index]
        );
    }
    assert_eq!(left[2080], 0.0, "and nothing after the fade");
    assert_eq!(left[3000], 0.0);
}

#[test]
fn the_sound_with_routes_is_the_same_for_any_size_of_block_and_a_group_with_a_latency_does_not_move_a_ring_that_leaves_it()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let make = |routes: Vec<TailRoute>, latency: usize| {
        SoundGraph::new(
            the_ringing_clip_and_a_group(a_ringing_clip(1, &effect, Some(0), routes), 3, 0, 2),
            Box::new(Ramps::default()),
            Box::new(GroupInsertionsWithEchoes {
                inside: GroupInsertions::new(move |_| {
                    if latency == 0 {
                        Vec::new()
                    } else {
                        vec![Box::new(Delays::new(latency, &Arc::new(AtomicUsize::new(0))))]
                    }
                }),
                items: &[0],
            }),
        )
    };
    // A group that only delays, and says so, gives back what it was given where it was: with it or without it, with a ring that leaves it or one that stays, the sound is the same.
    let without_routes = make(Vec::new(), 0).render_to_the_end(512);
    for routes in [Vec::new(), vec![route(1, TailDestination::Sum)]] {
        for latency in [0, 64, 700] {
            for block in [1, 13, 512, 1600, 100_000] {
                let made = make(routes.clone(), latency).render_to_the_end(block);
                assert_eq!(
                    made,
                    without_routes,
                    "{} routes, a latency of {latency}, blocks of {block}",
                    routes.len()
                );
            }
        }
    }
}

#[test]
fn the_instance_of_a_tracks_effects_goes_by_its_routes_as_a_clip_does() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    // Two clips that the track's effects hear as one, sent to the group; the instance rings on after them and its ring leaves the group where they end (frame 2).
    let mut specification = specification_with_buses(
        thirty(),
        4,
        vec![
            (vec![a_member(1, 0), a_member(1, 0)], vec![(0, 1)]),
            (vec![a_member(3, 0)], Vec::new()),
        ],
    );
    specification.groups = vec![SoundGroupToExport {
        effects: Vec::new(),
        parent: None,
    }];
    specification.edge_fade_milliseconds = 0;
    specification.strips[0].buses[0].effects = vec![effect];
    specification.strips[0].buses[0].tail_routes = vec![route(2, TailDestination::Sum)];
    let mut insertions = GroupInsertions::new(|_| vec![Box::new(Scales(0.5))]);
    insertions.for_buses = || vec![Box::<Echoes>::default()];

    let (left, _) =
        SoundGraph::new(specification, Box::new(Ramps::default()), Box::new(insertions)).render_to_the_end(512);

    // The instance hears the two clips, one after the other: the ramps of the items 0 and 1 of strip 0.
    let heard = (0..1600)
        .map(|index| value_of(0, 0, index))
        .chain((0..1600).map(|index| value_of(0, 1, index)))
        .chain(std::iter::repeat_n(0.0, 960));
    let echo = echoed(heard);
    for (index, sample) in left.iter().enumerate() {
        let member = if index < 4800 { value_of(1, 0, index) } else { 0.0 };
        // Until frame 2 (sample 3200) the instance's sound is in the group, which halves it, and so is the member's; after it the ring is not.
        let expected = if index < 3200 {
            (echo[index] + member) * 0.5
        } else if index < 3200 + 960 {
            echo[index] + member * 0.5
        } else {
            member * 0.5
        };
        assert_eq!(*sample, expected, "sample {index}");
    }
}

#[test]
fn a_group_that_a_ring_has_left_is_let_go_of_when_its_members_are_over_and_one_that_keeps_it_alive_is_not() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let alive_after_the_first_frame = |routes: Vec<TailRoute>| {
        let insertions = GroupInsertionsWithEchoes {
            inside: GroupInsertions::new(|_| vec![Box::new(Scales(0.5))]),
            items: &[0],
        };
        let alive = Arc::clone(&insertions.inside.alive);
        let mut graph = SoundGraph::new(
            the_ringing_clip_and_a_group(a_ringing_clip(1, &effect, Some(0), routes), 3, 0, 1),
            Box::new(Ramps::default()),
            Box::new(insertions),
        );
        let (mut left, mut right) = (vec![0.0; 1600], vec![0.0; 1600]);
        graph.render(&mut left, &mut right);
        alive.load(Ordering::SeqCst)
    };

    assert_eq!(
        alive_after_the_first_frame(vec![route(1, TailDestination::Sum)]),
        0,
        "the members are a frame long and the ring has left: nothing is kept for it"
    );
    assert_eq!(
        alive_after_the_first_frame(Vec::new()),
        1,
        "(the control: a group that keeps the ring alive is there for the ring)"
    );
}

/// Counts the effects it has made that are alive.
struct EchoesThatAreCounted(Echoes, Arc<AtomicUsize>);

impl BlockProcessor for EchoesThatAreCounted {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.0.process(left, right);
    }

    fn reset(&mut self) {}
}

impl Drop for EchoesThatAreCounted {
    fn drop(&mut self) {
        self.1.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Gives an echo that is counted to the clips of strip 0.
struct GivesCountedEchoes(Arc<AtomicUsize>);

impl Insertions for GivesCountedEchoes {
    fn for_clip(&mut self, request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        if request.strip != 0 {
            return Vec::new();
        }
        self.0.fetch_add(1, Ordering::SeqCst);
        vec![Box::new(EchoesThatAreCounted(Echoes::default(), Arc::clone(&self.0)))]
    }
}

#[test]
fn the_effects_of_a_ring_that_is_cut_are_let_go_of_where_it_has_faded_out_and_not_when_its_tail_would_have_ended() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(crate::project::TailLength::Milliseconds(100));
    let alive_after_two_thousand_samples = |routes: Vec<TailRoute>| {
        let counted = Arc::new(AtomicUsize::new(0));
        let mut graph = SoundGraph::new(
            the_ringing_clip_and_a_group(a_ringing_clip(1, &effect, None, routes), 4, 0, 1),
            Box::new(Ramps::default()),
            Box::new(GivesCountedEchoes(Arc::clone(&counted))),
        );
        let (mut left, mut right) = (vec![0.0; 2000], vec![0.0; 2000]);
        graph.render(&mut left, &mut right);
        counted.load(Ordering::SeqCst)
    };

    assert_eq!(
        alive_after_two_thousand_samples(vec![route(1, TailDestination::Cut)]),
        0,
        "the ring is cut at sample 1600 and nothing of it is heard after that"
    );
    assert_eq!(
        alive_after_two_thousand_samples(Vec::new()),
        1,
        "(the control: a ring that goes on is not let go of until its tail has ended, 4800 samples after the clip)"
    );
}
