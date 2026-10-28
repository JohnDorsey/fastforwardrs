// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Groups that have sound effects, in the block graph (phase 12, piece 12.3; docs/engine-phase-12-design.md, sections 2 and 8): what is sent to a group is added and given to one instance
//! of its effects, which comes after the effects of the clips, their volume and the effects of their track, and goes into the sum of the group round it or of all the sound.

use super::*;
use crate::media::export::SoundGroupToExport;

/// A clip of `frames` frames that is sent to the group `group`.
fn clip_sent_to(frames: u64, group: usize) -> SoundItemToExport {
    let SoundItemToExport::Clip(mut clip) = clip(frames, LoudnessToExport::UNCHANGED) else {
        unreachable!("a clip");
    };
    clip.sent_to = Some(group);
    SoundItemToExport::Clip(clip)
}

/// A specification with groups: for each, the group that holds it, and its effects as the specification says them (which only matters for how long they ring on).
fn specification_with_groups(
    total_frames: u64,
    strips: Vec<StripWithBuses>,
    groups: Vec<(Option<usize>, Vec<crate::composition::LayerEffect>)>,
) -> SoundGraphSpecification {
    let mut specification = specification_with_buses(thirty(), total_frames, strips);
    specification.groups = groups
        .into_iter()
        .map(|(parent, effects)| SoundGroupToExport { effects, parent })
        .collect();
    specification
}

/// Multiplies every sample by a number.
struct Scales(f32);

impl BlockProcessor for Scales {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for sample in left.iter_mut().chain(right.iter_mut()) {
            *sample *= self.0;
        }
    }

    fn reset(&mut self) {}
}

/// Counts itself as alive while it is, and does what the processor it holds does.
struct Counted(Box<dyn BlockProcessor>, Arc<AtomicUsize>);

impl BlockProcessor for Counted {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.0.process(left, right);
    }

    fn reset(&mut self) {
        self.0.reset();
    }

    fn latency(&self) -> usize {
        self.0.latency()
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.1.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Gives the effects that `for_group` makes for the group it is told, and `for_buses` for every bus, and counts the instances that are made and are alive.
struct GroupInsertions {
    for_group: Box<dyn FnMut(usize) -> Vec<Box<dyn BlockProcessor>> + Send>,
    for_buses: fn() -> Vec<Box<dyn BlockProcessor>>,
    made: Arc<AtomicUsize>,
    alive: Arc<AtomicUsize>,
}

impl GroupInsertions {
    fn new(for_group: impl FnMut(usize) -> Vec<Box<dyn BlockProcessor>> + Send + 'static) -> Self {
        Self {
            for_group: Box::new(for_group),
            for_buses: Vec::new,
            made: Arc::default(),
            alive: Arc::default(),
        }
    }

    fn counted(&self, processors: Vec<Box<dyn BlockProcessor>>) -> Vec<Box<dyn BlockProcessor>> {
        processors
            .into_iter()
            .map(|processor| {
                self.made.fetch_add(1, Ordering::SeqCst);
                self.alive.fetch_add(1, Ordering::SeqCst);
                Box::new(Counted(processor, Arc::clone(&self.alive))) as Box<dyn BlockProcessor>
            })
            .collect()
    }
}

impl Insertions for GroupInsertions {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        Vec::new()
    }

    fn for_bus(&mut self, _request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        (self.for_buses)()
    }

    fn for_group(&mut self, request: &GroupRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        let processors = (self.for_group)(request.group);
        self.counted(processors)
    }
}

/// What `for_group` is asked: the first clip of the group, as the frame it begins at, for each group that is asked for.
struct AsksForTheFirstClip(Arc<std::sync::Mutex<Vec<(usize, u64)>>>);

impl Insertions for AsksForTheFirstClip {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        Vec::new()
    }

    fn for_group(&mut self, request: &GroupRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        self.0
            .lock()
            .unwrap()
            .push((request.group, request.first_clip.first_timeline_frame));
        Vec::new()
    }
}

fn graph(specification: SoundGraphSpecification, insertions: impl Insertions + Send + 'static) -> SoundGraph {
    SoundGraph::new(specification, Box::new(Ramps::default()), Box::new(insertions))
}

#[test]
fn what_is_sent_to_a_group_is_added_and_given_to_its_effects_once_and_the_rest_is_added_after_them() {
    let strips = || {
        vec![
            (vec![clip_sent_to(2, 0)], Vec::new()),
            (vec![clip_sent_to(2, 0)], Vec::new()),
            (vec![clip(2, LoudnessToExport::UNCHANGED)], Vec::new()),
        ]
    };

    let (left, right) = graph(
        specification_with_groups(2, strips(), vec![(None, Vec::new())]),
        GroupInsertions::new(|_| vec![Box::new(Adds(100.0))]),
    )
    .render_to_the_end(512);

    for index in [0, 5, 1599, 1600, 3199] {
        // The two strips sent to the group are added and the effect adds its 100 to the sum, once; the third strip is added as it is.
        assert_eq!(
            left[index],
            (value_of(0, 0, index) + value_of(1, 0, index) + 100.0) + value_of(2, 0, index),
            "left {index}"
        );
        assert_eq!(
            right[index],
            (-value_of(0, 0, index) - value_of(1, 0, index) + 100.0) - value_of(2, 0, index),
            "right {index}"
        );
    }
}

#[test]
fn a_group_whose_effects_do_nothing_gives_what_it_was_given_as_if_it_was_not_there() {
    let strips = || {
        vec![
            (vec![clip_sent_to(2, 0)], Vec::new()),
            (vec![clip(2, LoudnessToExport::Constant(0.5))], Vec::new()),
        ]
    };
    let plain = graph(
        specification_with_buses(
            thirty(),
            2,
            vec![
                (vec![clip(2, LoudnessToExport::UNCHANGED)], Vec::new()),
                (vec![clip(2, LoudnessToExport::Constant(0.5))], Vec::new()),
            ],
        ),
        NoInsertions,
    )
    .render_to_the_end(512);

    let grouped = graph(
        specification_with_groups(2, strips(), vec![(None, Vec::new())]),
        GroupInsertions::new(|_| Vec::new()),
    )
    .render_to_the_end(512);

    assert_eq!(grouped, plain);
}

#[test]
fn the_volume_and_the_mutes_of_a_track_are_done_to_what_it_sends_before_the_effects_of_the_group_hear_it() {
    let mut specification = specification_with_groups(
        3,
        vec![(vec![clip_sent_to(3, 0)], Vec::new())],
        vec![(None, Vec::new())],
    );
    specification.strips[0].loudness = LoudnessToExport::Constant(0.5);
    specification.strips[0].muted = vec![(1, 2)];

    let (left, _) = graph(
        specification,
        GroupInsertions::new(|_| vec![Box::new(Counts::default())]),
    )
    .render_to_the_end(512);

    for (index, sample) in left.iter().enumerate() {
        // The volume is before the effect, so it is halved before the count is added; the mute too, so the effect hears silence there and its count goes on all the same.
        let heard = if (1600..3200).contains(&index) {
            0.0
        } else {
            value_of(0, 0, index) * 0.5
        };
        assert_eq!(*sample, heard + index as f32, "left {index}");
    }
}

#[test]
fn a_group_in_a_group_gives_its_sum_to_the_one_round_it_and_the_effects_are_done_inside_out() {
    let (left, _) = graph(
        specification_with_groups(
            2,
            vec![(vec![clip_sent_to(2, 0)], Vec::new())],
            vec![(Some(1), Vec::new()), (None, Vec::new())],
        ),
        GroupInsertions::new(|group| {
            if group == 0 {
                vec![Box::new(Adds(10.0))]
            } else {
                vec![Box::new(Scales(2.0))]
            }
        }),
    )
    .render_to_the_end(512);

    for index in [0, 7, 1600, 3199] {
        assert_eq!(
            left[index],
            (value_of(0, 0, index) + 10.0) * 2.0,
            "the inner group adds 10 and then the outer group doubles: {index}"
        );
    }
}

#[test]
fn the_effects_of_a_track_come_before_the_effects_of_the_group_that_the_clips_it_hears_are_sent_to() {
    let mut insertions = GroupInsertions::new(|_| vec![Box::new(Scales(2.0))]);
    insertions.for_buses = || vec![Box::new(Adds(1.0))];

    let (left, _) = graph(
        specification_with_groups(
            2,
            vec![(vec![clip_sent_to(1, 0), clip_sent_to(1, 0)], vec![(0, 1)])],
            vec![(None, Vec::new())],
        ),
        insertions,
    )
    .render_to_the_end(512);

    for index in [0, 1599, 1600, 3199] {
        let item = index / 1600;
        assert_eq!(
            left[index],
            (value_of(0, item, index - item * 1600) + 1.0) * 2.0,
            "the bus adds 1 and then the group doubles: {index}"
        );
    }
}

#[test]
fn the_effects_of_a_group_are_made_when_the_first_of_what_it_hears_is_heard_and_let_go_of_after_the_last() {
    let insertions = GroupInsertions::new(|_| vec![Box::new(Adds(1.0))]);
    let (made, alive) = (Arc::clone(&insertions.made), Arc::clone(&insertions.alive));
    let mut graph = graph(
        specification_with_groups(
            4,
            vec![(vec![silence(2), clip_sent_to(2, 0)], Vec::new())],
            vec![(None, Vec::new())],
        ),
        insertions,
    );
    let (mut left, mut right) = (vec![0.0; 1600], vec![0.0; 1600]);

    graph.render(&mut left, &mut right);
    assert_eq!(made.load(Ordering::SeqCst), 0, "nothing it hears has begun");
    graph.render(&mut left, &mut right);
    assert_eq!(
        made.load(Ordering::SeqCst),
        0,
        "two frames of silence is not hearing the clip"
    );
    graph.render(&mut left, &mut right);
    assert_eq!(
        (made.load(Ordering::SeqCst), alive.load(Ordering::SeqCst)),
        (1, 1),
        "the clip begins"
    );
    graph.render(&mut left, &mut right);
    assert_eq!(
        (made.load(Ordering::SeqCst), alive.load(Ordering::SeqCst)),
        (1, 0),
        "the clip has ended, and nothing rings on: the instance is let go of"
    );
}

#[test]
fn a_group_that_hears_nothing_makes_no_instance_of_its_effects() {
    let insertions = GroupInsertions::new(|_| vec![Box::new(Adds(1.0))]);
    let made = Arc::clone(&insertions.made);
    let (left, _) = graph(
        specification_with_groups(
            2,
            vec![(vec![clip(2, LoudnessToExport::UNCHANGED)], Vec::new())],
            vec![(None, Vec::new())],
        ),
        insertions,
    )
    .render_to_the_end(512);

    assert_eq!(made.load(Ordering::SeqCst), 0);
    assert_eq!(
        left[10],
        value_of(0, 0, 10),
        "and the sound that goes to no group is as it was"
    );
}

#[test]
fn the_effects_of_a_group_go_on_after_its_members_for_their_tail_and_no_longer_and_the_tail_is_cut_where_the_sound_ends()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let make = |total_frames: u64| {
        graph(
            specification_with_groups(
                total_frames,
                vec![(vec![clip_sent_to(1, 0)], Vec::new())],
                vec![(None, vec![effect.clone()])],
            ),
            GroupInsertions::new(|_| vec![Box::new(Echoes::default())]),
        )
    };

    let (left, right) = make(3).render_to_the_end(512);

    let input = (0..1600)
        .map(|index| value_of(0, 0, index))
        .chain(std::iter::repeat_n(0.0, 960));
    let expected: Vec<f32> = echoed(input)
        .into_iter()
        .chain(std::iter::repeat_n(0.0, 4800 - 1600 - 960))
        .collect();
    assert_eq!(
        left, expected,
        "the clip, the echo of it for the tail of the effect, and then silence"
    );
    assert_ne!(right[1600], 0.0, "the right channel rings on too");

    let (cut, _) = make(1).render_to_the_end(512);
    assert_eq!(cut.len(), 1600, "a ring past the end of the sound is cut there");
}

#[test]
fn the_effects_of_a_group_that_have_a_latency_are_made_up_for_so_the_sound_is_where_it_would_be_without_them_for_any_blocks()
 {
    // (Two members that begin at different times, and one thing that goes to the sum: the sound that is asked for ahead is what puts the later member where it is.)
    let strips = || {
        vec![
            (vec![clip_sent_to(1, 0)], Vec::new()),
            (vec![silence(1), clip_sent_to(2, 0)], Vec::new()),
            (vec![clip(3, LoudnessToExport::UNCHANGED)], Vec::new()),
        ]
    };
    let plain = graph(
        specification_with_groups(3, strips(), vec![(None, Vec::new())]),
        GroupInsertions::new(|_| Vec::new()),
    )
    .render_to_the_end(512);
    for latency in [1_usize, 64, 300, 2000] {
        for block in [1, 13, 512, 100_000] {
            let insertions =
                GroupInsertions::new(move |_| vec![Box::new(Delays::new(latency, &Arc::new(AtomicUsize::new(0))))]);

            let made = graph(
                specification_with_groups(3, strips(), vec![(None, Vec::new())]),
                insertions,
            )
            .render_to_the_end(block);

            assert_eq!(made, plain, "a latency of {latency} samples, blocks of {block}");
        }
    }
}

#[test]
fn a_group_with_a_latency_in_a_group_with_a_latency_makes_up_for_both_and_what_comes_out_is_where_it_is() {
    let strips = || {
        vec![
            (vec![silence(1), clip_sent_to(1, 0)], Vec::new()),
            (vec![clip_sent_to(1, 0)], Vec::new()),
            (vec![silence(2), clip_sent_to(1, 1)], Vec::new()),
            (vec![clip(3, LoudnessToExport::UNCHANGED)], Vec::new()),
        ]
    };
    let groups = || vec![(Some(1), Vec::new()), (None, Vec::new())];
    let plain = graph(
        specification_with_groups(3, strips(), groups()),
        GroupInsertions::new(|_| Vec::new()),
    )
    .render_to_the_end(512);
    for block in [1, 13, 512, 100_000] {
        let made = graph(
            specification_with_groups(3, strips(), groups()),
            GroupInsertions::new(|group| {
                let given = Arc::new(AtomicUsize::new(0));
                vec![Box::new(Delays::new(if group == 0 { 100 } else { 37 }, &given))]
            }),
        )
        .render_to_the_end(block);

        assert_eq!(made, plain, "blocks of {block}");
    }
}

#[test]
fn the_sound_with_groups_is_the_same_for_any_size_of_block() {
    let make = || {
        graph(
            specification_with_groups(
                3,
                vec![
                    (vec![silence(1), clip_sent_to(2, 0)], Vec::new()),
                    (vec![clip_sent_to(2, 1)], Vec::new()),
                    (vec![clip(3, LoudnessToExport::Constant(0.5))], Vec::new()),
                ],
                vec![(Some(1), Vec::new()), (None, Vec::new())],
            ),
            GroupInsertions::new(|group| {
                if group == 0 {
                    vec![Box::<Counts>::default()]
                } else {
                    vec![Box::<Echoes>::default()]
                }
            }),
        )
    };
    let whole = make().render_to_the_end(1_000_000);

    for block in [1, 13, 512, 1600, 4799] {
        assert_eq!(make().render_to_the_end(block), whole, "blocks of {block}");
    }
}

#[test]
fn the_effects_of_a_group_are_asked_for_with_the_clip_that_it_hears_first_and_so_is_each_group_round_it() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    // (The clip that begins later is in the strip that is first, and the one that begins first is in the second: it is by when they begin, not by where they are.)
    let mut specification = specification_with_groups(
        4,
        vec![
            (vec![silence(2), clip_sent_to(1, 0)], Vec::new()),
            (vec![silence(1), clip_sent_to(1, 0)], Vec::new()),
        ],
        vec![(Some(1), Vec::new()), (None, Vec::new())],
    );
    for (strip, first_frame) in [(0, 2), (1, 1)] {
        let SoundItemToExport::Clip(clip) = &mut specification.strips[strip].items[1] else {
            unreachable!("a clip");
        };
        clip.first_timeline_frame = first_frame;
    }

    graph(specification, AsksForTheFirstClip(Arc::clone(&asked))).render_to_the_end(512);

    let mut asked = asked.lock().unwrap().clone();
    asked.sort_unstable();
    assert_eq!(
        asked,
        [(0, 1), (1, 1)],
        "each group is asked for once, with the clip that begins at frame 1"
    );
}
