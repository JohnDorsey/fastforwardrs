// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The block graph with sound that is made up: positions to the sample, lengths, the volume at every sample, the sum, that any size of block gives the same sound, that
//! feeds are opened when they are first heard, and where the effects go. (The graph against the filter graph of ffmpeg, which is what its sound is meant to be, is
//! `tests/sound_engine_null_test.rs`.)

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use crate::project::{FrameRate, SpedUpClipSound, SpeedFactor};
use crate::test_support::check_every_case_at_once;

mod groups;
mod routes;

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn clip(frames: u64, loudness: LoudnessToExport) -> SoundItemToExport {
    SoundItemToExport::Clip(SoundClipToExport {
        source_input: 0,
        first_source_frame: 0,
        end_source_frame: frames,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: frames,
        loudness,
        first_timeline_frame: 0,
        frames_of_the_clip_before: 0,
        effects: Vec::new(),
        sent_to: None,
        tail_routes: Vec::new(),
    })
}

fn silence(frames: u64) -> SoundItemToExport {
    SoundItemToExport::Silence { frame_count: frames }
}

fn specification(
    frame_rate: FrameRate,
    total_frames: u64,
    strips: Vec<Vec<SoundItemToExport>>,
) -> SoundGraphSpecification {
    SoundGraphSpecification {
        frame_rate,
        total_frame_count: total_frames,
        strips: strips
            .into_iter()
            .map(|items| SoundStripToExport {
                items,
                buses: Vec::new(),
                loudness: Default::default(),
                muted: Vec::new(),
            })
            .collect(),
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    }
}

/// What sample number `index` of the feed of item `item` of strip `strip` is: a different number for every sample of every clip, so that a sample in the wrong place cannot be
/// taken for the right one. The right channel is the left with the sign changed.
fn value_of(strip: usize, item: usize, index: usize) -> f32 {
    (strip + 1) as f32 * 100.0 + item as f32 + index as f32 * 0.001
}

/// Feeds that give `value_of`, for as many samples as they are told (a clip that is not told has more than it can use), and count the feeds that are open.
#[derive(Clone, Default)]
struct Ramps {
    lengths: HashMap<(usize, usize), usize>,
    opened: Arc<AtomicUsize>,
    open_now: Arc<AtomicUsize>,
    most_open_at_once: Arc<AtomicUsize>,
}

impl Ramps {
    fn with_length(mut self, strip: usize, item: usize, samples: usize) -> Self {
        self.lengths.insert((strip, item), samples);
        self
    }
}

struct Ramp {
    strip: usize,
    item: usize,
    length: usize,
    next: usize,
    open_now: Arc<AtomicUsize>,
}

impl Drop for Ramp {
    fn drop(&mut self) {
        self.open_now.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ClipFeed for Ramp {
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        let count = left.len().min(self.length - self.next);
        for place in 0..count {
            left[place] = value_of(self.strip, self.item, self.next + place);
            right[place] = -left[place];
        }
        self.next += count;
        count
    }
}

impl ClipFeeds for Ramps {
    fn open(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        let now = self.open_now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_open_at_once.fetch_max(now, Ordering::SeqCst);
        let length = self
            .lengths
            .get(&(request.strip, request.item))
            .copied()
            .unwrap_or(usize::MAX);
        Box::new(Ramp {
            strip: request.strip,
            item: request.item,
            length,
            next: 0,
            open_now: self.open_now.clone(),
        })
    }
}

fn graph_of(specification: SoundGraphSpecification, ramps: &Ramps) -> SoundGraph {
    SoundGraph::new(specification, Box::new(ramps.clone()), Box::new(NoInsertions))
}

/// Whether `a` and `b` are as long as each other and each sample is within `tolerance` of the other, relative to its size (but not to less than 1).
fn equal_within(a: &[f32], b: &[f32], tolerance: f32) -> Result<(), String> {
    if a.len() != b.len() {
        return Err(format!("{} samples and {}", a.len(), b.len()));
    }
    for (place, (a, b)) in a.iter().zip(b).enumerate() {
        if (a - b).abs() > tolerance * b.abs().max(1.0) {
            return Err(format!("sample {place}: {a} and {b}"));
        }
    }
    Ok(())
}

// --- where a frame is ---

#[test]
fn a_frame_is_where_its_moment_is_in_samples_exactly_where_that_is_a_whole_number() {
    for (rate, samples_a_frame) in [(24, 2000), (25, 1920), (30, 1600), (50, 960), (60, 800)] {
        let rate = FrameRate::new(rate, 1).unwrap();
        for frame in [0, 1, 2, 7, 1000, 123_456] {
            assert_eq!(
                sample_at_frame(rate, frame),
                frame * samples_a_frame,
                "{rate:?} {frame}"
            );
        }
    }
}

#[test]
fn a_frame_rate_that_is_not_a_whole_number_of_samples_a_frame_does_not_drift() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    // 30000 frames are exactly 1001 seconds, whatever number of items they are cut into.
    assert_eq!(sample_at_frame(rate, 30_000), 1001 * 48_000);
    assert_eq!(sample_at_frame(rate, 15_000), 1001 * 24_000);
    let mut earlier = 0;
    for frame in 0..5000_u64 {
        let sample = sample_at_frame(rate, frame);
        let ideal = frame as f64 * 1001.0 / 30000.0 * 48000.0;
        assert!(
            (sample as f64 - ideal).abs() <= 0.5 + 1e-9,
            "frame {frame}: {sample} and {ideal}"
        );
        assert!(sample >= earlier, "frame {frame}");
        earlier = sample;
    }
    // Half a sample is rounded up: frame 5 of 48000/... is at 5 * 1600.0 = exact, so use a rate where it is a half: 1 frame a 96000th of a second.
    let odd = FrameRate::new(96_000, 1).unwrap();
    assert_eq!(sample_at_frame(odd, 1), 1, "half a sample is a sample");
    assert_eq!(sample_at_frame(odd, 2), 1);
    assert_eq!(sample_at_frame(odd, 3), 2);
}

// --- how loud, at every sample ---

fn changing(points: &[(u64, f32)]) -> LoudnessToExport {
    LoudnessToExport::Changing(points.to_vec())
}

#[test]
fn a_constant_loudness_is_one_number_and_one_of_one_changes_nothing() {
    let curve = GainCurve::new(&LoudnessToExport::Constant(0.25), thirty());
    assert!(!curve.changes_nothing());
    let (mut left, mut right) = (vec![1.0, -2.0, 4.0], vec![0.5, 0.0, 8.0]);
    let mut curve = curve;
    curve.apply(0, &mut left, &mut right);
    assert_eq!((left, right), (vec![0.25, -0.5, 1.0], vec![0.125, 0.0, 2.0]));
    assert!(GainCurve::new(&LoudnessToExport::UNCHANGED, thirty()).changes_nothing());
}

#[test]
fn a_changing_loudness_goes_evenly_in_decibels_between_its_points_and_is_the_first_before_and_the_last_after() {
    // 0 dB at frame 30 (one second), -20 dB at frame 90 (three seconds).
    let curve = GainCurve::new(&changing(&[(30, 0.0), (90, -20.0)]), thirty());
    let at = |seconds: f64| curve.factor_at_sample((seconds * 48_000.0) as u64);
    assert_eq!(at(0.0), 1.0, "before the first point it is as at the first");
    assert_eq!(at(1.0), 1.0);
    assert!(
        (at(2.0) - 10f64.powf(-10.0 / 20.0)).abs() < 1e-12,
        "halfway is -10 dB: {}",
        at(2.0)
    );
    assert!((at(3.0) - 0.1).abs() < 1e-12);
    assert!((at(100.0) - 0.1).abs() < 1e-12, "after the last it is as at the last");
}

#[test]
fn a_loudness_with_three_points_has_a_segment_for_each_and_a_corner_where_they_meet() {
    // 0 dB at 0, -6 dB at one second, +6 dB at two seconds.
    let curve = GainCurve::new(&changing(&[(0, 0.0), (30, -6.0), (60, 6.0)]), thirty());
    let decibels = |seconds: f64| 20.0 * curve.factor_at_sample((seconds * 48_000.0) as u64).log10();
    assert!((decibels(0.5) - -3.0).abs() < 1e-9);
    assert!((decibels(1.0) - -6.0).abs() < 1e-9);
    assert!((decibels(1.5) - 0.0).abs() < 1e-9);
    assert!((decibels(2.0) - 6.0).abs() < 1e-9);
}

#[test]
fn the_gain_applied_to_a_stretch_is_the_gain_of_each_of_its_samples_whatever_the_stretches_are() {
    let curve = GainCurve::new(&changing(&[(0, -3.0), (10, 0.0), (25, -40.0), (60, 6.0)]), thirty());
    let total = 2 * 48_000_usize;
    let ones = vec![1.0_f32; total];
    for stretch in [1_usize, 5, 255, 512, 7000, total] {
        let (mut left, mut right) = (ones.clone(), ones.clone());
        let mut working = curve.clone();
        let mut first = 0;
        while first < total {
            let end = (first + stretch).min(total);
            working.apply(first as u64, &mut left[first..end], &mut right[first..end]);
            first = end;
        }
        for sample in (0..total).step_by(97) {
            let wanted = curve.factor_at_sample(sample as u64) as f32;
            assert_eq!(left[sample], wanted, "stretches of {stretch}, sample {sample}");
            assert_eq!(right[sample], wanted);
        }
    }
}

// --- the graph ---

#[test]
fn a_clip_is_heard_from_the_sample_its_frame_is_at_with_silence_before_it() {
    // Two frames of silence, then a clip of three frames: at 30 frames a second, 1600 samples a frame.
    let ramps = Ramps::default();
    let mut graph = graph_of(
        specification(
            thirty(),
            6,
            vec![vec![silence(2), clip(3, LoudnessToExport::UNCHANGED), silence(1)]],
        ),
        &ramps,
    );
    assert_eq!(graph.total_samples(), 6 * 1600);
    let (left, right) = graph.render_to_the_end(512);
    assert_eq!(left.len(), 6 * 1600);
    assert!(left[..3200].iter().all(|sample| *sample == 0.0));
    for index in [0, 1, 1599, 1600, 4799] {
        assert_eq!(left[3200 + index], value_of(0, 1, index), "sample {index} of the clip");
        assert_eq!(right[3200 + index], -value_of(0, 1, index));
    }
    assert!(left[8000..].iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_clip_is_cut_off_where_its_place_on_the_timeline_ends_and_followed_by_silence_where_its_file_ends_early() {
    let long = Ramps::default().with_length(0, 0, 100_000);
    let (left, _) = graph_of(
        specification(
            thirty(),
            2,
            vec![vec![
                clip(1, LoudnessToExport::UNCHANGED),
                clip(1, LoudnessToExport::UNCHANGED),
            ]],
        ),
        &long,
    )
    .render_to_the_end(512);
    assert_eq!(left.len(), 3200);
    assert_eq!(left[1599], value_of(0, 0, 1599));
    assert_eq!(
        left[1600],
        value_of(0, 1, 0),
        "the next clip begins where it should, however long the first one's sound was"
    );

    let short = Ramps::default().with_length(0, 0, 1000);
    let (left, _) = graph_of(
        specification(thirty(), 1, vec![vec![clip(1, LoudnessToExport::UNCHANGED)]]),
        &short,
    )
    .render_to_the_end(512);
    assert_eq!(left[999], value_of(0, 0, 999));
    assert!(
        left[1000..].iter().all(|sample| *sample == 0.0),
        "silence follows the sound that ran out"
    );
    assert_eq!(left.len(), 1600);
}

#[test]
fn the_tracks_are_added_and_a_track_that_ends_early_is_silent_after_its_end() {
    let ramps = Ramps::default();
    let (left, right) = graph_of(
        specification(
            thirty(),
            3,
            vec![
                vec![clip(3, LoudnessToExport::UNCHANGED)],
                vec![silence(1), clip(1, LoudnessToExport::UNCHANGED)],
            ],
        ),
        &ramps,
    )
    .render_to_the_end(512);
    assert_eq!(left.len(), 4800);
    assert_eq!(left[10], value_of(0, 0, 10), "the first track alone");
    let at = 1600 + 10;
    assert_eq!(left[at], value_of(0, 0, at) + value_of(1, 1, 10), "both");
    assert_eq!(right[at], -(value_of(0, 0, at) + value_of(1, 1, 10)));
    assert_eq!(left[3200 + 10], value_of(0, 0, 3200 + 10), "the second track has ended");
}

#[test]
fn the_volume_of_a_clip_is_applied_at_every_sample_from_the_start_of_the_clip() {
    let ramps = Ramps::default();
    let loudness = changing(&[(0, 0.0), (2, -20.0)]);
    let (left, _) = graph_of(
        specification(thirty(), 3, vec![vec![silence(1), clip(2, loudness.clone())]]),
        &ramps,
    )
    .render_to_the_end(333);
    let curve = GainCurve::new(&loudness, thirty());
    for index in [0_usize, 1, 800, 1599, 1600, 3199] {
        let wanted = value_of(0, 1, index) * curve.factor_at_sample(index as u64) as f32;
        assert_eq!(left[1600 + index], wanted, "sample {index} of the clip");
    }
}

#[test]
fn the_sound_is_the_same_for_any_size_of_block() {
    let ramps = Ramps::default().with_length(0, 1, 4000);
    let make = || {
        specification(
            FrameRate::new(30000, 1001).unwrap(),
            14,
            vec![
                vec![
                    clip(3, LoudnessToExport::Constant(0.7)),
                    silence(2),
                    clip(4, changing(&[(0, -6.0), (4, 3.0)])),
                    clip(5, LoudnessToExport::UNCHANGED),
                ],
                vec![silence(1), clip(9, changing(&[(0, 0.0), (3, -9.0), (9, 0.0)]))],
            ],
        )
    };
    let (whole_left, whole_right) = graph_of(make(), &ramps).render_to_the_end(10_000_000);
    assert_eq!(
        whole_left.len() as u64,
        sample_at_frame(FrameRate::new(30000, 1001).unwrap(), 14)
    );
    for block in [1, 7, 100, 512, 1601, 4096] {
        let (left, right) = graph_of(make(), &ramps).render_to_the_end(block);
        assert_eq!(left, whole_left, "blocks of {block}");
        assert_eq!(right, whole_right, "blocks of {block}");
    }
}

#[test]
fn the_sound_ends_at_the_end_and_a_sound_with_no_tracks_is_silence_as_long_as_the_sound() {
    let ramps = Ramps::default();
    let mut graph = graph_of(
        specification(thirty(), 2, vec![vec![clip(2, LoudnessToExport::UNCHANGED)]]),
        &ramps,
    );
    let (mut left, mut right) = (vec![9.0; 5000], vec![9.0; 5000]);
    assert_eq!(graph.render(&mut left, &mut right), 3200, "no more than there is");
    assert_eq!(graph.position(), 3200);
    assert_eq!(graph.render(&mut left, &mut right), 0, "and then none");

    let (left, right) = graph_of(specification(thirty(), 2, Vec::new()), &ramps).render_to_the_end(512);
    assert_eq!((left.len(), right.len()), (3200, 3200));
    assert!(left.iter().chain(&right).all(|sample| *sample == 0.0));
}

#[test]
fn a_feed_is_opened_when_its_clip_is_first_heard_and_let_go_of_when_the_clip_ends() {
    let ramps = Ramps::default();
    let mut graph = graph_of(
        specification(
            thirty(),
            8,
            vec![vec![
                clip(2, LoudnessToExport::UNCHANGED),
                silence(1),
                clip(2, LoudnessToExport::UNCHANGED),
                clip(3, LoudnessToExport::UNCHANGED),
            ]],
        ),
        &ramps,
    );
    let (mut left, mut right) = (vec![0.0; 1600], vec![0.0; 1600]);
    assert_eq!(
        ramps.opened.load(Ordering::SeqCst),
        0,
        "nothing is opened before sound is asked for"
    );
    graph.render(&mut left, &mut right);
    assert_eq!(
        (
            ramps.opened.load(Ordering::SeqCst),
            ramps.open_now.load(Ordering::SeqCst)
        ),
        (1, 1)
    );
    graph.render(&mut left, &mut right);
    assert_eq!(
        ramps.open_now.load(Ordering::SeqCst),
        0,
        "the first clip ended with the block that ended it"
    );
    graph.render_to_the_end(1600);
    assert_eq!(ramps.opened.load(Ordering::SeqCst), 3);
    assert_eq!(ramps.open_now.load(Ordering::SeqCst), 0);
    assert_eq!(
        ramps.most_open_at_once.load(Ordering::SeqCst),
        1,
        "one strip, one feed at a time"
    );
}

// --- the effects ---

/// Adds a number to every sample, in the order it is given them.
struct Adds(f32);

impl BlockProcessor for Adds {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for sample in left.iter_mut().chain(right.iter_mut()) {
            *sample += self.0;
        }
    }

    fn reset(&mut self) {}
}

/// Counts the samples it is given and writes the count into them: so what it makes does not depend on the size of the blocks only if it is given every sample once, in order.
#[derive(Default)]
struct Counts(u64);

impl BlockProcessor for Counts {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            *left += self.0 as f32;
            *right += self.0 as f32;
            self.0 += 1;
        }
    }

    fn reset(&mut self) {
        self.0 = 0;
    }
}

/// The same effects, made again, for every clip, every bus and every group.
struct FixedInsertions {
    for_clips: fn() -> Vec<Box<dyn BlockProcessor>>,
    for_buses: fn() -> Vec<Box<dyn BlockProcessor>>,
    for_groups: fn() -> Vec<Box<dyn BlockProcessor>>,
}

impl Insertions for FixedInsertions {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        (self.for_clips)()
    }

    fn for_bus(&mut self, _request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        (self.for_buses)()
    }

    fn for_group(&mut self, _request: &GroupRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        (self.for_groups)()
    }
}

#[test]
fn the_effects_of_a_clip_are_applied_before_its_volume_and_the_effects_of_a_track_after_the_clips_are_together() {
    let ramps = Ramps::default();
    let make = |insertions: FixedInsertions| {
        SoundGraph::new(
            specification(
                thirty(),
                2,
                vec![
                    vec![silence(1), clip(1, LoudnessToExport::Constant(0.5))],
                    vec![clip(2, LoudnessToExport::UNCHANGED)],
                ],
            ),
            Box::new(ramps.clone()),
            Box::new(insertions),
        )
    };
    // A clip's effect adds 1 and then the volume halves: (x + 1) / 2, not x / 2 + 1.
    let (left, _) = make(FixedInsertions {
        for_clips: || vec![Box::new(Adds(1.0))],
        for_buses: Vec::new,
        for_groups: Vec::new,
    })
    .render_to_the_end(512);
    let (plain, _) = graph_of(
        specification(
            thirty(),
            2,
            vec![
                vec![silence(1), clip(1, LoudnessToExport::Constant(0.5))],
                vec![clip(2, LoudnessToExport::UNCHANGED)],
            ],
        ),
        &ramps,
    )
    .render_to_the_end(512);
    let first_strip_clip = 1600 + 5;
    let second_strip_clips = [5, 1600 + 5];
    assert_eq!(
        left[first_strip_clip],
        (value_of(0, 1, 5) + 1.0) * 0.5 + (value_of(1, 0, 1605) + 1.0)
    );
    assert_eq!(
        left[second_strip_clips[0]],
        value_of(1, 0, 5) + 1.0,
        "an effect on every clip: the other strip's clip has it too"
    );
    assert_ne!(left[first_strip_clip], plain[first_strip_clip]);

    // The effects of a track are one instance for the items it hears, which hears the silence among them too: this one adds 100, once, before the strips are added: two strips make 200.
    let (left, _) = SoundGraph::new(
        specification_with_buses(
            thirty(),
            2,
            vec![
                (vec![silence(1), clip(1, LoudnessToExport::Constant(0.5))], vec![(0, 1)]),
                (vec![clip(2, LoudnessToExport::UNCHANGED)], vec![(0, 0)]),
            ],
        ),
        Box::new(ramps.clone()),
        Box::new(FixedInsertions {
            for_clips: Vec::new,
            for_buses: || vec![Box::new(Adds(100.0))],
            for_groups: Vec::new,
        }),
    )
    .render_to_the_end(512);
    assert_eq!(
        left[0],
        value_of(1, 0, 0) + 200.0,
        "the first strip is silent here, and has its 100 all the same"
    );
    assert_eq!(
        left[first_strip_clip],
        (value_of(0, 1, 5) * 0.5 + 100.0) + (value_of(1, 0, 1605) + 100.0)
    );
}

/// One strip of a clip of `frames` frames, heard by a bus (the effects of the track), with the volume and the mutes of the track.
fn strip_with_a_volume_and_mutes(
    frames: u64,
    loudness: LoudnessToExport,
    muted: Vec<(u64, u64)>,
) -> SoundGraphSpecification {
    let mut specification = specification_with_buses(
        thirty(),
        frames,
        vec![(vec![clip(frames, LoudnessToExport::UNCHANGED)], vec![(0, 0)])],
    );
    specification.strips[0].loudness = loudness;
    specification.strips[0].muted = muted;
    specification
}

#[test]
fn the_volume_of_a_track_is_done_after_its_effects() {
    let ramps = Ramps::default();
    let mut graph = SoundGraph::new(
        strip_with_a_volume_and_mutes(2, LoudnessToExport::Constant(0.5), Vec::new()),
        Box::new(ramps.clone()),
        Box::new(FixedInsertions {
            for_clips: Vec::new,
            for_buses: || vec![Box::new(Adds(100.0))],
            for_groups: Vec::new,
        }),
    );

    let (left, right) = graph.render_to_the_end(512);

    // The effect adds 100 and then the volume halves: (x + 100) / 2, not x / 2 + 100 (as a volume that came first would make it).
    for index in [0, 5, 1599, 1600, 3199] {
        assert_eq!(left[index], (value_of(0, 0, index) + 100.0) * 0.5, "left {index}");
        assert_eq!(right[index], (-value_of(0, 0, index) + 100.0) * 0.5, "right {index}");
    }
}

#[test]
fn a_volume_of_a_track_that_changes_is_counted_from_the_start_of_the_sound_in_whatever_blocks_it_is_made() {
    let ramps = Ramps::default();
    let points = [(0, -12.0), (1, 0.0), (3, 6.0)];
    // The same volume done to the clip, which begins where the sound does, and so counts from there too, is what it must be.
    let (on_the_clip_left, on_the_clip_right) = graph_of(
        specification(thirty(), 3, vec![vec![clip(3, changing(&points))]]),
        &ramps,
    )
    .render_to_the_end(512);
    for block in [1, 512, 1600, 5000] {
        let (left, right) = SoundGraph::new(
            strip_with_a_volume_and_mutes(3, changing(&points), Vec::new()),
            Box::new(ramps.clone()),
            Box::new(FixedInsertions {
                for_clips: Vec::new,
                for_buses: Vec::new,
                for_groups: Vec::new,
            }),
        )
        .render_to_the_end(block);
        assert_eq!(
            (left, right),
            (on_the_clip_left.clone(), on_the_clip_right.clone()),
            "blocks of {block}"
        );
    }
}

#[test]
fn a_track_that_is_muted_is_silent_after_its_effects_and_what_they_remember_goes_on_meanwhile_for_any_blocks() {
    let ramps = Ramps::default();
    let render = |block: usize| {
        SoundGraph::new(
            // The second of three frames is muted.
            strip_with_a_volume_and_mutes(3, LoudnessToExport::UNCHANGED, vec![(1, 2)]),
            Box::new(ramps.clone()),
            Box::new(FixedInsertions {
                for_clips: Vec::new,
                for_buses: || vec![Box::new(Counts::default())],
                for_groups: Vec::new,
            }),
        )
        .render_to_the_end(block)
    };

    let (left, right) = render(512);

    assert_eq!(left.len(), 4800);
    for (index, sample) in left.iter().enumerate() {
        // Counts writes its number of samples so far into every sample, whether or not the sample is heard.
        let heard = value_of(0, 0, index) + index as f32;
        let expected = if (1600..3200).contains(&index) { 0.0 } else { heard };
        assert_eq!(*sample, expected, "left {index}");
    }
    assert!(
        right[1600..3200].iter().all(|sample| *sample == 0.0),
        "silent on the right too"
    );
    assert_ne!(right[3200], 0.0, "and heard again after the mute");
    for block in [1, 700, 1600, 4800, 10_000] {
        assert_eq!(render(block), (left.clone(), right.clone()), "blocks of {block}");
    }
}

#[test]
fn an_effect_is_given_every_sample_of_its_clip_once_and_in_order_whatever_the_blocks_are_and_past_the_end_of_the_sound_of_the_file()
 {
    let ramps = Ramps::default().with_length(0, 0, 1000);
    let make = || {
        SoundGraph::new(
            specification(thirty(), 2, vec![vec![clip(2, LoudnessToExport::UNCHANGED)]]),
            Box::new(ramps.clone()),
            Box::new(FixedInsertions {
                for_clips: || vec![Box::<Counts>::default()],
                for_buses: Vec::new,
                for_groups: Vec::new,
            }),
        )
    };
    let (whole, _) = make().render_to_the_end(1_000_000);
    // The file has 1000 samples; the effect is given 3200 all the same, the last 2200 of them silent (its tail would be heard).
    assert_eq!(whole[1000], 1000.0, "sample 1000 of the clip: silence plus the count");
    assert_eq!(whole[3199], 3199.0);
    for block in [1, 13, 512, 1600] {
        assert_eq!(make().render_to_the_end(block).0, whole, "blocks of {block}");
    }
}

// --- the arithmetic against a plain evaluation ---

/// The sound of `specification` worked out in the plainest way: each item whole, then the strips added.
fn plainly(specification: &SoundGraphSpecification, ramps: &Ramps) -> Vec<f32> {
    let total = sample_at_frame(specification.frame_rate, specification.total_frame_count) as usize;
    let mut sum = vec![0.0_f32; total];
    for (strip_number, strip) in specification.strips.iter().enumerate() {
        let mut frames = 0;
        for (item_number, item) in strip.items.iter().enumerate() {
            let start = sample_at_frame(specification.frame_rate, frames) as usize;
            let SoundItemToExport::Clip(clip) = item else {
                if let SoundItemToExport::Silence { frame_count } = item {
                    frames += frame_count;
                }
                continue;
            };
            frames += clip.timeline_frame_count;
            let end = sample_at_frame(specification.frame_rate, frames) as usize;
            let curve = GainCurve::new(&clip.loudness, specification.frame_rate);
            let sound_length = ramps
                .lengths
                .get(&(strip_number, item_number))
                .copied()
                .unwrap_or(usize::MAX);
            for index in 0..(end - start).min(sound_length) {
                if start + index < total {
                    sum[start + index] += (f64::from(value_of(strip_number, item_number, index))
                        * curve.factor_at_sample(index as u64)) as f32;
                }
            }
        }
    }
    sum
}

#[test]
fn the_graph_is_the_plain_arithmetic_on_many_made_up_specifications() {
    let rates = [(24, 1), (30, 1), (30000, 1001), (25, 1), (60, 1)];
    let seeds: Vec<u64> = (0..60).collect();
    let failures: Vec<String> = check_every_case_at_once(&seeds, |&seed| {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = |most: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % most
        };
        let (numerator, denominator) = rates[next(rates.len() as u64) as usize];
        let frame_rate = FrameRate::new(numerator, denominator).unwrap();
        let strip_count = 1 + next(3) as usize;
        let mut ramps = Ramps::default();
        let mut longest = 0;
        let strips: Vec<Vec<SoundItemToExport>> = (0..strip_count)
            .map(|strip| {
                let mut frames = 0;
                let items: Vec<SoundItemToExport> = (0..next(6))
                    .map(|item| {
                        let length = 1 + next(12);
                        frames += length;
                        if next(3) == 0 {
                            silence(length)
                        } else {
                            if next(4) == 0 {
                                ramps = ramps.clone().with_length(strip, item as usize, next(20_000) as usize);
                            }
                            let loudness = match next(3) {
                                0 => LoudnessToExport::UNCHANGED,
                                1 => LoudnessToExport::Constant(0.1 + next(20) as f64 / 10.0),
                                _ => changing(&[(0, next(20) as f32 - 15.0), (length, next(20) as f32 - 15.0)]),
                            };
                            clip(length, loudness)
                        }
                    })
                    .collect();
                longest = longest.max(frames);
                items
            })
            .collect();
        let total = longest + next(3);
        let specification = specification(frame_rate, total, strips);
        let wanted = plainly(&specification, &ramps);
        let block = [1, 100, 512, 2000][next(4) as usize];
        let (left, right) = graph_of(specification, &ramps).render_to_the_end(block);
        equal_within(&left, &wanted, 2e-6).map_err(|why| format!("seed {seed}, blocks of {block}: {why}"))?;
        equal_within(&right, &wanted.iter().map(|sample| -sample).collect::<Vec<_>>(), 2e-6)
            .map_err(|why| format!("seed {seed}, right channel: {why}"))
    })
    .into_iter()
    .filter_map(Result::err)
    .collect();
    assert!(failures.is_empty(), "{failures:?}");
}

// --- sound that is in memory, and buffers of different lengths ---

#[test]
fn sound_in_memory_is_read_in_pieces_in_order_and_runs_out_where_it_ends() {
    let left: Vec<f32> = (0..10).map(|number| number as f32).collect();
    let right: Vec<f32> = left.iter().map(|sample| -sample - 100.0).collect();
    let mut sound = SoundInMemory::new(left.clone(), right.clone());
    assert_eq!((sound.len(), sound.is_empty()), (10, false));
    let (mut a, mut b) = ([9.0; 4], [9.0; 4]);
    assert_eq!(sound.read(&mut a, &mut b), 4);
    assert_eq!((&a[..], &b[..]), (&left[0..4], &right[0..4]));
    assert_eq!(sound.read(&mut a, &mut b), 4);
    assert_eq!((&a[..], &b[..]), (&left[4..8], &right[4..8]));
    let (mut c, mut d) = ([9.0; 4], [9.0; 4]);
    assert_eq!(sound.read(&mut c, &mut d), 2, "only two are left");
    assert_eq!((&c[..2], &d[..2]), (&left[8..10], &right[8..10]));
    assert_eq!((c[2], d[2]), (9.0, 9.0), "what it did not give is not touched");
    assert_eq!(sound.read(&mut c, &mut d), 0, "and then none");
    assert!(SoundInMemory::new(Vec::new(), Vec::new()).is_empty());
}

#[test]
fn sound_in_memory_that_is_given_interleaved_is_split_into_its_channels_and_a_half_moment_is_left_out() {
    let mut sound = SoundInMemory::from_interleaved(&[1.0, -1.0, 2.0, -2.0, 3.0]);
    assert_eq!(sound.len(), 2);
    let (mut left, mut right) = ([0.0; 3], [0.0; 3]);
    assert_eq!(sound.read(&mut left, &mut right), 2);
    assert_eq!((&left[..2], &right[..2]), (&[1.0, 2.0][..], &[-1.0, -2.0][..]));
}

#[test]
fn a_gain_whose_first_point_is_after_the_start_of_the_clip_is_the_first_point_before_it() {
    // 6 dB at one second (frame 30), -6 dB at two seconds; the clip begins at 0.
    let curve = GainCurve::new(&changing(&[(30, 6.0), (60, -6.0)]), thirty());
    let total = 3 * 48_000_usize;
    for stretch in [1_usize, 4096, total] {
        let (mut left, mut right) = (vec![1.0_f32; total], vec![1.0_f32; total]);
        let mut working = curve.clone();
        let mut first = 0;
        while first < total {
            let end = (first + stretch).min(total);
            working.apply(first as u64, &mut left[first..end], &mut right[first..end]);
            first = end;
        }
        for sample in (0..total).step_by(113) {
            assert_eq!(
                left[sample],
                curve.factor_at_sample(sample as u64) as f32,
                "stretches of {stretch}, sample {sample}"
            );
        }
        assert_eq!(
            left[0],
            10f64.powf(6.0 / 20.0) as f32,
            "before the first point: as at the first"
        );
        assert_eq!(
            left[total - 1],
            10f64.powf(-6.0 / 20.0) as f32,
            "after the last: as at the last"
        );
    }
}

#[test]
fn the_graph_makes_as_many_samples_as_the_shorter_of_two_buffers_that_are_not_as_long_as_each_other() {
    let mut graph = graph_of(
        specification(thirty(), 2, vec![vec![clip(2, LoudnessToExport::UNCHANGED)]]),
        &Ramps::default(),
    );
    let (mut left, mut right) = (vec![9.0; 100], vec![9.0; 40]);
    assert_eq!(graph.render(&mut left, &mut right), 40);
    assert_eq!(graph.position(), 40);
    assert!(
        left[40..].iter().all(|sample| *sample == 9.0),
        "the longer one is not touched past that"
    );
    assert_eq!(right[39], -value_of(0, 0, 39));
}

// --- feeds that are got ready ahead ---

use std::sync::Mutex;

/// Makes the same feeds `Ramps` does, and says which were made and in what order.
#[derive(Clone)]
struct RampSpawner {
    ramps: Ramps,
    spawned: Arc<Mutex<Vec<(usize, usize)>>>,
}

impl RampSpawner {
    fn new() -> Self {
        Self {
            ramps: Ramps::default(),
            spawned: Arc::default(),
        }
    }

    fn spawned(&self) -> Vec<(usize, usize)> {
        self.spawned.lock().unwrap().clone()
    }
}

impl ClipSpawner for RampSpawner {
    fn spawn(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        self.spawned.lock().unwrap().push((request.strip, request.item));
        self.ramps.open(request)
    }
}

/// Clips of one second each (30 frames) one after another on strip 0, and `second_strip` clips of a second each on strip 1 beginning at the given seconds.
fn clips_one_after_another(count: usize) -> SoundGraphSpecification {
    specification(
        thirty(),
        30 * count as u64,
        vec![(0..count).map(|_| clip(30, LoudnessToExport::UNCHANGED)).collect()],
    )
}

fn request_for<'a>(specification: &'a SoundGraphSpecification, strip: usize, item: usize) -> ClipRequest<'a> {
    let SoundItemToExport::Clip(clip) = &specification.strips[strip].items[item] else {
        panic!("a clip")
    };
    ClipRequest { strip, item, clip }
}

#[test]
fn nothing_is_started_until_the_graph_says_where_it_is_and_then_the_clips_that_begin_within_the_look_ahead_are_started_earliest_first()
 {
    let spawner = RampSpawner::new();
    let mut feeds = LookAheadFeeds::new(clips_one_after_another(10), spawner.clone());
    assert_eq!(
        spawner.spawned(),
        Vec::<(usize, usize)>::new(),
        "nothing before the graph says"
    );

    feeds.about_to_render(0);
    // Two seconds ahead: the clips that begin at 0 s and at 1 s (not the one at 2 s, which begins when the look-ahead ends).
    assert_eq!(spawner.spawned(), vec![(0, 0), (0, 1)]);
    assert_eq!(feeds.open_count(), 2);

    feeds.about_to_render(0);
    assert_eq!(spawner.spawned().len(), 2, "asking again starts nothing again");

    feeds.about_to_render(48_000);
    assert_eq!(
        spawner.spawned(),
        vec![(0, 0), (0, 1), (0, 2)],
        "a second on, the clip at 2 s is within reach"
    );
}

#[test]
fn a_clip_whose_feed_was_started_is_opened_with_that_feed_and_one_that_was_not_is_started_at_once() {
    let spawner = RampSpawner::new();
    let specification = clips_one_after_another(6);
    let mut feeds = LookAheadFeeds::new(specification.clone(), spawner.clone());
    feeds.about_to_render(0);
    assert_eq!(spawner.spawned().len(), 2);

    let mut first = feeds.open(&request_for(&specification, 0, 0));
    assert_eq!(
        spawner.spawned().len(),
        2,
        "the feed that was started is the one it gets"
    );
    let (mut left, mut right) = ([0.0; 3], [0.0; 3]);
    assert_eq!(first.read(&mut left, &mut right), 3);
    assert_eq!(left[0], value_of(0, 0, 0));

    let _late = feeds.open(&request_for(&specification, 0, 5));
    assert_eq!(
        spawner.spawned().len(),
        3,
        "a clip that nothing was ready for is started when it is asked for"
    );
    assert_eq!(spawner.spawned()[2], (0, 5));
}

#[test]
fn no_more_feeds_are_started_ahead_than_the_most_there_may_be_and_one_that_ends_makes_room() {
    let spawner = RampSpawner::new();
    // Six clips begin together on six strips.
    let strips = (0..6).map(|_| vec![clip(30, LoudnessToExport::UNCHANGED)]).collect();
    let specification = specification(thirty(), 30, strips);
    let mut feeds = LookAheadFeeds::with_limits(specification.clone(), spawner.clone(), 2.0, 4);
    feeds.about_to_render(0);
    assert_eq!(spawner.spawned().len(), 4, "four at most are started ahead");
    assert_eq!(feeds.open_count(), 4);

    // One is let go of (its clip ended): there is room for the next.
    drop(feeds.open(&request_for(&specification, 0, 0)));
    assert_eq!(feeds.open_count(), 3);
    feeds.about_to_render(0);
    assert_eq!(spawner.spawned().len(), 5);
    assert_eq!(feeds.open_count(), 4);
}

#[test]
fn the_sound_is_the_same_with_feeds_that_are_got_ready_ahead_and_every_clip_is_started_once() {
    let make = || {
        specification(
            thirty(),
            12,
            vec![
                vec![
                    clip(3, LoudnessToExport::UNCHANGED),
                    silence(2),
                    clip(4, LoudnessToExport::Constant(0.5)),
                    clip(3, LoudnessToExport::UNCHANGED),
                ],
                vec![silence(1), clip(9, changing(&[(0, 0.0), (9, -12.0)]))],
            ],
        )
    };
    let direct = graph_of(make(), &Ramps::default()).render_to_the_end(512);
    for block in [64, 512, 5000] {
        let spawner = RampSpawner::new();
        let ahead = LookAheadFeeds::with_limits(make(), spawner.clone(), 0.1, 2);
        let mut graph = SoundGraph::new(make(), Box::new(ahead), Box::new(NoInsertions));
        assert_eq!(graph.render_to_the_end(block), direct, "blocks of {block}");
        let mut spawned = spawner.spawned();
        spawned.sort_unstable();
        assert_eq!(
            spawned,
            vec![(0, 0), (0, 2), (0, 3), (1, 1)],
            "each clip once, and no silence"
        );
    }
}

#[test]
fn the_look_ahead_is_measured_from_where_each_clip_is_in_frames_to_the_sample() {
    // Clips of a second one after another; a look-ahead of 1.02 seconds from the start reaches the clip that begins at 1 s (sample 48000) and not the one at 2 s.
    let spawner = RampSpawner::new();
    let mut feeds = LookAheadFeeds::with_limits(clips_one_after_another(5), spawner.clone(), 1.02, 12);
    feeds.about_to_render(0);
    assert_eq!(spawner.spawned(), vec![(0, 0), (0, 1)]);
    // And just short of the clip at 1 s it is not reached: the look-ahead is 0.02 s short of it.
    let spawner = RampSpawner::new();
    let mut feeds = LookAheadFeeds::with_limits(clips_one_after_another(5), spawner.clone(), 0.98, 12);
    feeds.about_to_render(0);
    assert_eq!(spawner.spawned(), vec![(0, 0)]);
}

#[test]
fn a_clip_that_was_opened_without_having_been_started_is_not_started_again_when_the_look_ahead_reaches_it() {
    let spawner = RampSpawner::new();
    let specification = clips_one_after_another(5);
    let mut feeds = LookAheadFeeds::with_limits(specification.clone(), spawner.clone(), 0.5, 12);
    feeds.about_to_render(0);
    assert_eq!(
        spawner.spawned(),
        vec![(0, 0)],
        "only the first is within half a second"
    );
    // The second clip is asked for before the look-ahead has reached it: it is started then.
    let _second = feeds.open(&request_for(&specification, 0, 1));
    assert_eq!(spawner.spawned(), vec![(0, 0), (0, 1)]);
    // Now the look-ahead reaches it: it is not started again.
    feeds.about_to_render(40_000);
    assert_eq!(spawner.spawned(), vec![(0, 0), (0, 1)]);
    feeds.about_to_render(80_000);
    assert_eq!(
        spawner.spawned(),
        vec![(0, 0), (0, 1), (0, 2)],
        "and the next one is, as it should be"
    );
}

// --- latency ---

/// An effect that delays the sound by `latency` samples, and says so; and counts how many samples it was given.
struct Delays {
    latency: usize,
    memory: std::collections::VecDeque<(f32, f32)>,
    given: Arc<AtomicUsize>,
}

impl Delays {
    fn new(latency: usize, given: &Arc<AtomicUsize>) -> Self {
        Self {
            latency,
            memory: std::iter::repeat_n((0.0, 0.0), latency).collect(),
            given: Arc::clone(given),
        }
    }
}

impl BlockProcessor for Delays {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.given.fetch_add(left.len(), Ordering::SeqCst);
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            self.memory.push_back((*left, *right));
            (*left, *right) = self.memory.pop_front().expect("the memory is never empty");
        }
    }

    fn reset(&mut self) {
        self.memory = std::iter::repeat_n((0.0, 0.0), self.latency).collect();
    }

    fn latency(&self) -> usize {
        self.latency
    }
}

/// A delay that does not say it is one: the sound comes out late.
struct DelaysAndDoesNotSay(Delays);

impl BlockProcessor for DelaysAndDoesNotSay {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.0.process(left, right);
    }

    fn reset(&mut self) {
        self.0.reset();
    }
}

fn a_graph_with_delays(latencies: &'static [usize], given: &Arc<AtomicUsize>, ramps: &Ramps) -> SoundGraph {
    struct Delaying {
        latencies: &'static [usize],
        given: Arc<AtomicUsize>,
    }
    impl Insertions for Delaying {
        fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
            self.latencies
                .iter()
                .map(|latency| Box::new(Delays::new(*latency, &self.given)) as Box<dyn BlockProcessor>)
                .collect()
        }
    }
    SoundGraph::new(
        specification(
            thirty(),
            4,
            vec![vec![
                clip(2, LoudnessToExport::Constant(0.5)),
                silence(1),
                clip(1, LoudnessToExport::UNCHANGED),
            ]],
        ),
        Box::new(ramps.clone()),
        Box::new(Delaying {
            latencies,
            given: Arc::clone(given),
        }),
    )
}

#[test]
fn an_effect_with_a_latency_is_made_up_for_so_the_sound_is_where_it_would_be_without_it_for_any_blocks() {
    let ramps = Ramps::default().with_length(0, 0, 3000).with_length(0, 2, 900);
    let plain = graph_of(
        specification(
            thirty(),
            4,
            vec![vec![
                clip(2, LoudnessToExport::Constant(0.5)),
                silence(1),
                clip(1, LoudnessToExport::UNCHANGED),
            ]],
        ),
        &ramps,
    )
    .render_to_the_end(512)
    .0;
    for latencies in [&[64_usize][..], &[1], &[300, 7], &[2000]] {
        let latencies: &'static [usize] = Vec::leak(latencies.to_vec());
        for block in [1, 13, 512, 100_000] {
            let given = Arc::new(AtomicUsize::new(0));

            let made = a_graph_with_delays(latencies, &given, &ramps)
                .render_to_the_end(block)
                .0;

            assert_eq!(made, plain, "latencies {latencies:?}, blocks of {block}");
        }
    }
}

#[test]
fn an_effect_with_a_latency_is_given_that_many_samples_more_than_its_clip_has_and_one_without_is_given_only_those() {
    let ramps = Ramps::default();
    for (latencies, extra) in [(&[0_usize][..], 0), (&[64], 64), (&[300, 7], 307)] {
        let latencies: &'static [usize] = Vec::leak(latencies.to_vec());
        let given = Arc::new(AtomicUsize::new(0));

        a_graph_with_delays(latencies, &given, &ramps).render_to_the_end(512);

        // Two clips of 3200 and 1600 samples; every effect of the chain is given each sample once, and the extra ones for the latency of the chain, for each clip.
        let samples = 3200 + 1600;
        assert_eq!(
            given.load(Ordering::SeqCst),
            latencies.len() * samples + latencies.len() * 2 * extra,
            "latencies {latencies:?}"
        );
    }
}

#[test]
fn an_effect_that_delays_the_sound_and_does_not_say_so_is_heard_late_by_what_it_delays() {
    let ramps = Ramps::default().with_length(0, 0, 3200);
    let spec = || specification(thirty(), 2, vec![vec![clip(2, LoudnessToExport::UNCHANGED)]]);
    let plain = graph_of(spec(), &ramps).render_to_the_end(512).0;
    let given = Arc::new(AtomicUsize::new(0));
    struct Lying(Arc<AtomicUsize>);
    impl Insertions for Lying {
        fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
            vec![Box::new(DelaysAndDoesNotSay(Delays::new(10, &self.0)))]
        }
    }

    let made = SoundGraph::new(spec(), Box::new(ramps.clone()), Box::new(Lying(given)))
        .render_to_the_end(512)
        .0;

    assert_eq!(
        &made[..10],
        &[0.0; 10],
        "the first samples out are the silence it started with"
    );
    assert_eq!(
        &made[10..],
        &plain[..plain.len() - 10],
        "late by ten samples, because nothing was told"
    );
}

// --- sound from before a clip ---

/// Feeds that give the number of the sample as the sample: the first samples are the sound from before the clip, if the clip has any.
#[derive(Clone)]
struct Counted;

struct CountedFeed(usize);

impl ClipFeed for CountedFeed {
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            *left = self.0 as f32;
            *right = -(self.0 as f32);
            self.0 += 1;
        }
        left.len()
    }
}

impl ClipFeeds for Counted {
    fn open(&mut self, _request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        Box::new(CountedFeed(0))
    }
}

/// An effect that does nothing to the sound and writes down what it is told and given.
struct Listens {
    latency: usize,
    told_of_sound_from_before: Arc<std::sync::Mutex<Option<usize>>>,
    given: Arc<std::sync::Mutex<Vec<f32>>>,
    /// How many samples it was given in each call.
    calls: Arc<std::sync::Mutex<Vec<usize>>>,
}

impl BlockProcessor for Listens {
    fn process(&mut self, left: &mut [f32], _right: &mut [f32]) {
        self.calls.lock().unwrap().push(left.len());
        self.given.lock().unwrap().extend_from_slice(left);
    }

    fn reset(&mut self) {}

    fn latency(&self) -> usize {
        self.latency
    }

    fn about_to_be_given_sound_from_before_the_clip(&mut self, samples: usize) {
        *self.told_of_sound_from_before.lock().unwrap() = Some(samples);
        assert!(
            self.given.lock().unwrap().is_empty(),
            "it is told before it is given anything"
        );
    }
}

struct ListeningInsertions {
    latency: usize,
    told: Arc<std::sync::Mutex<Option<usize>>>,
    given: Arc<std::sync::Mutex<Vec<f32>>>,
    calls: Arc<std::sync::Mutex<Vec<usize>>>,
}

impl Insertions for ListeningInsertions {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        vec![Box::new(Listens {
            latency: self.latency,
            told_of_sound_from_before: Arc::clone(&self.told),
            given: Arc::clone(&self.given),
            calls: Arc::clone(&self.calls),
        })]
    }
}

/// A clip of `frames` frames at 30 a second that begins `first_source_frame` frames into its file, with an effect of a plugin that asks for `pre_roll_samples` of sound from before.
fn clip_that_has_sound_from_before(
    first_source_frame: u64,
    frames: u64,
    pre_roll_samples: usize,
    speed: u32,
) -> SoundItemToExport {
    use crate::project::{Animation, EffectInstanceId, set_installed_plugins};
    let mut plugin = crate::test_support::an_installed_sound_plugin("clap:test.remembers", "Maker", &[]);
    plugin.pre_roll_samples = pre_roll_samples;
    set_installed_plugins(vec![plugin]);
    SoundItemToExport::Clip(SoundClipToExport {
        source_input: 0,
        first_source_frame,
        end_source_frame: first_source_frame + frames * u64::from(speed),
        speed_factor: SpeedFactor::new(speed).unwrap(),
        timeline_frame_count: frames,
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame: 0,
        frames_of_the_clip_before: first_source_frame,
        effects: vec![crate::composition::LayerEffect {
            state: None,
            clock: Default::default(),
            instance: EffectInstanceId::FIRST,
            kind: crate::project::EffectKind::installed_plugin_kinds()[0],
            values: Vec::new(),
            animation: Animation::default(),
            tables: Vec::new(),
            tail_length: Default::default(),
            shared_by: None,
        }],
        sent_to: None,
        tail_routes: Vec::new(),
    })
}

#[test]
fn the_effects_of_a_clip_are_given_the_sound_from_before_it_first_and_it_is_not_heard_for_any_blocks_or_latency() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    // The clip is 3 frames, 4800 samples, and begins 10 frames into its file. 2000 samples asked for are 2 frames, 3200 samples; 6000 are 4 frames, 6400, which is more than the
    // graph gives to the effects at once.
    for (asked_for, from_before) in [(2000_usize, 3200_usize), (6000, 6400)] {
        for latency in [0_usize, 64] {
            for block in [1, 13, 512, 100_000] {
                let (told, given) = (Arc::default(), Arc::default());
                let calls: Arc<std::sync::Mutex<Vec<usize>>> = Arc::default();
                let mut graph = SoundGraph::new(
                    specification(
                        thirty(),
                        3,
                        vec![vec![clip_that_has_sound_from_before(10, 3, asked_for, 1)]],
                    ),
                    Box::new(Counted),
                    Box::new(ListeningInsertions {
                        latency,
                        told: Arc::clone(&told),
                        given: Arc::clone(&given),
                        calls: Arc::clone(&calls),
                    }),
                );

                let (heard, other) = graph.render_to_the_end(block);

                let what = format!("{asked_for} asked for, latency {latency}, blocks of {block}");
                assert_eq!(*told.lock().unwrap(), Some(from_before), "{what}");
                // The calls that make up the sound from before are pieces of at most 4096 samples, whatever the blocks are.
                let mut so_far = 0;
                for call in calls.lock().unwrap().iter() {
                    if so_far >= from_before {
                        break;
                    }
                    assert!(*call <= 4096, "a piece of {call}: {what}");
                    so_far += call;
                }
                assert_eq!(
                    so_far, from_before,
                    "the sound from before is made of whole calls: {what}"
                );
                // (The effect says it has a latency and does not delay the sound, so the sound that comes out is the latency ahead of the clip: it is not late, and the graph has
                // taken the samples that make up for the latency off the front, as it does for one that is.)
                let heard_expected: Vec<f32> = (0..4800).map(|n| (from_before + latency + n) as f32).collect();
                assert_eq!(
                    heard, heard_expected,
                    "what is heard begins after the sound from before the clip and the samples that make up for the latency: {what}"
                );
                assert_eq!(other, heard_expected.iter().map(|sample| -sample).collect::<Vec<_>>());
                let given = given.lock().unwrap();
                assert_eq!(
                    given[..from_before],
                    (0..from_before).map(|n| n as f32).collect::<Vec<_>>()[..],
                    "the sound from before, first: {what}"
                );
                assert_eq!(given[from_before], from_before as f32, "and then the clip: {what}");
                assert_eq!(
                    given.len(),
                    from_before + 4800 + latency,
                    "all of it once, and what makes up for the latency: {what}"
                );
            }
        }
    }
}

#[test]
fn a_clip_at_the_start_of_its_file_or_one_that_is_sped_up_or_with_a_plugin_that_remembers_nothing_is_told_of_no_sound_from_before()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    for (item, why) in [
        (
            clip_that_has_sound_from_before(0, 3, 2000, 1),
            "it begins at the start of its file",
        ),
        (clip_that_has_sound_from_before(10, 3, 2000, 2), "it is sped up"),
        (
            clip_that_has_sound_from_before(10, 3, 0, 1),
            "the plugin remembers nothing",
        ),
    ] {
        let (told, given) = (Arc::default(), Arc::default());
        let mut graph = SoundGraph::new(
            specification(thirty(), 3, vec![vec![item]]),
            Box::new(Counted),
            Box::new(ListeningInsertions {
                latency: 0,
                told: Arc::clone(&told),
                given: Arc::clone(&given),
                calls: Arc::default(),
            }),
        );

        let (heard, _) = graph.render_to_the_end(512);

        assert_eq!(*told.lock().unwrap(), None, "{why}");
        assert_eq!(
            heard,
            (0..4800).map(|n| n as f32).collect::<Vec<_>>(),
            "{why}: the clip is heard from its first sample"
        );
    }
}

// --- buses: one instance of the effects of a track that hears the clips of a run (phase 12, piece 12.2) ---

/// A strip of items, and the runs of them that one instance of effects hears (each a first and a last item).
type StripWithBuses = (Vec<SoundItemToExport>, Vec<(usize, usize)>);

/// A specification of strips with buses.
fn specification_with_buses(
    frame_rate: FrameRate,
    total_frames: u64,
    strips: Vec<StripWithBuses>,
) -> SoundGraphSpecification {
    SoundGraphSpecification {
        frame_rate,
        total_frame_count: total_frames,
        strips: strips
            .into_iter()
            .map(|(items, buses)| SoundStripToExport {
                // (A bus sends what it makes where the clips it hears are sent.)
                buses: buses
                    .into_iter()
                    .map(|(first_item, last_item)| crate::media::export::SoundBusToExport {
                        first_item,
                        last_item,
                        effects: Vec::new(),
                        sent_to: items[first_item..=last_item]
                            .iter()
                            .find_map(|item| match item {
                                SoundItemToExport::Clip(clip) => Some(clip.sent_to),
                                SoundItemToExport::Silence { .. } => None,
                            })
                            .flatten(),
                        tail_routes: Vec::new(),
                    })
                    .collect(),
                items,
                loudness: Default::default(),
                muted: Vec::new(),
            })
            .collect(),
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    }
}

/// Gives every sample the sum of it and all the samples that it was given before (each channel by itself): a state that goes on from one sample to the next, and so from one
/// clip to the next if it is one instance.
#[derive(Default)]
struct Sums {
    left: f32,
    right: f32,
}

impl BlockProcessor for Sums {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for sample in left {
            self.left += *sample;
            *sample = self.left;
        }
        for sample in right {
            self.right += *sample;
            *sample = self.right;
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Insertions that give the effects of a bus (and of nothing else, or of each clip too, if `for_clips`), and count the instances that are made and are alive.
struct BusInsertions {
    make: fn() -> Vec<Box<dyn BlockProcessor>>,
    for_clips: bool,
    made: Arc<AtomicUsize>,
    alive: Arc<AtomicUsize>,
}

/// Counts itself as alive while it is, and does what `Sums` does.
struct SumsThatAreCounted {
    inside: Sums,
    alive: Arc<AtomicUsize>,
}

impl BlockProcessor for SumsThatAreCounted {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.inside.process(left, right);
    }

    fn reset(&mut self) {
        self.inside.reset();
    }
}

impl Drop for SumsThatAreCounted {
    fn drop(&mut self) {
        self.alive.fetch_sub(1, Ordering::SeqCst);
    }
}

impl BusInsertions {
    fn new(make: fn() -> Vec<Box<dyn BlockProcessor>>, for_clips: bool) -> Self {
        Self {
            make,
            for_clips,
            made: Arc::default(),
            alive: Arc::default(),
        }
    }

    fn counted(&self) -> Vec<Box<dyn BlockProcessor>> {
        self.made.fetch_add(1, Ordering::SeqCst);
        self.alive.fetch_add(1, Ordering::SeqCst);
        vec![Box::new(SumsThatAreCounted {
            inside: Sums::default(),
            alive: Arc::clone(&self.alive),
        })]
    }
}

impl Insertions for BusInsertions {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        if self.for_clips { self.counted() } else { Vec::new() }
    }

    fn for_bus(&mut self, _request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        if self.for_clips { (self.make)() } else { self.counted() }
    }
}

fn running_sum_of(samples: impl IntoIterator<Item = f32>) -> Vec<f32> {
    let mut total = 0.0_f32;
    samples
        .into_iter()
        .map(|sample| {
            total += sample;
            total
        })
        .collect()
}

#[test]
fn the_clips_of_a_bus_are_heard_by_one_instance_whose_state_goes_on_from_one_clip_to_the_next() {
    let ramps = Ramps::default();
    let two_clips = || {
        vec![
            clip(1, LoudnessToExport::UNCHANGED),
            clip(1, LoudnessToExport::UNCHANGED),
        ]
    };
    let mut shared = SoundGraph::new(
        specification_with_buses(thirty(), 2, vec![(two_clips(), vec![(0, 1)])]),
        Box::new(ramps.clone()),
        Box::new(BusInsertions::new(Vec::new, false)),
    );
    let mut separate = SoundGraph::new(
        specification_with_buses(thirty(), 2, vec![(two_clips(), Vec::new())]),
        Box::new(ramps.clone()),
        Box::new(BusInsertions::new(Vec::new, true)),
    );

    let (shared_left, shared_right) = shared.render_to_the_end(512);
    let (separate_left, _) = separate.render_to_the_end(512);

    let feed = |item: usize| (0..1600).map(move |index| value_of(0, item, index));
    assert_eq!(
        shared_left,
        running_sum_of(feed(0).chain(feed(1))),
        "one instance heard the two clips one after the other, and its sum went on"
    );
    let right_feed = |item: usize| (0..1600).map(move |index| -value_of(0, item, index));
    assert_eq!(shared_right, running_sum_of(right_feed(0).chain(right_feed(1))));
    let mut restarted = running_sum_of(feed(0));
    restarted.extend(running_sum_of(feed(1)));
    assert_eq!(
        separate_left, restarted,
        "(the control: an instance for each clip begins again at each clip)"
    );
    assert_ne!(shared_left, separate_left);
}

#[test]
fn an_instance_of_a_bus_is_made_when_its_first_clip_is_first_heard_and_let_go_of_after_its_last_sample() {
    let ramps = Ramps::default();
    let insertions = BusInsertions::new(Vec::new, false);
    let (made, alive) = (Arc::clone(&insertions.made), Arc::clone(&insertions.alive));
    let mut graph = SoundGraph::new(
        specification_with_buses(
            thirty(),
            5,
            vec![(
                vec![
                    silence(1),
                    clip(1, LoudnessToExport::UNCHANGED),
                    clip(1, LoudnessToExport::UNCHANGED),
                    silence(1),
                    clip(1, LoudnessToExport::UNCHANGED),
                ],
                vec![(1, 2), (4, 4)],
            )],
        ),
        Box::new(ramps),
        Box::new(insertions),
    );
    let (mut left, mut right) = (vec![0.0; 1600], vec![0.0; 1600]);
    let mut seen = Vec::new();
    // A frame at a time (1600 samples): the instances alive after each, and how many were made.
    for _ in 0..5 {
        graph.render(&mut left, &mut right);
        seen.push((alive.load(Ordering::SeqCst), made.load(Ordering::SeqCst)));
    }

    assert_eq!(
        seen,
        [
            (0, 0),
            (1, 1), // the run of two clips is first heard
            (0, 1), // and let go of after its last sample, which is the last of the second frame
            (0, 1),
            (0, 2), // the last clip's run is made, heard and let go of in its frame
        ],
        "frame by frame: how many are alive, and how many were made so far"
    );
}

#[test]
fn the_sound_of_a_bus_is_the_same_for_any_size_of_block() {
    let ramps = Ramps::default().with_length(0, 1, 2000);
    let make = |block: usize| {
        SoundGraph::new(
            specification_with_buses(
                thirty(),
                4,
                vec![
                    (
                        vec![
                            clip(1, LoudnessToExport::Constant(0.5)),
                            clip(1, LoudnessToExport::UNCHANGED),
                            silence(1),
                            clip(1, LoudnessToExport::UNCHANGED),
                        ],
                        vec![(0, 1), (3, 3)],
                    ),
                    (vec![clip(4, LoudnessToExport::UNCHANGED)], Vec::new()),
                ],
            ),
            Box::new(ramps.clone()),
            Box::new(BusInsertions::new(Vec::new, false)),
        )
        .render_to_the_end(block)
    };
    let whole = make(1_000_000);
    for block in [1, 7, 512, 1599, 1600, 1601] {
        assert_eq!(make(block), whole, "blocks of {block}");
    }
}

#[test]
fn the_volume_of_each_clip_is_done_before_the_instance_hears_it_and_what_is_not_in_a_bus_is_as_it_was() {
    let ramps = Ramps::default();
    let mut graph = SoundGraph::new(
        specification_with_buses(
            thirty(),
            2,
            vec![
                (
                    vec![
                        clip(1, LoudnessToExport::Constant(0.5)),
                        clip(1, LoudnessToExport::UNCHANGED),
                    ],
                    vec![(0, 1)],
                ),
                (vec![clip(2, LoudnessToExport::Constant(0.25))], Vec::new()),
            ],
        ),
        Box::new(ramps.clone()),
        Box::new(BusInsertions::new(Vec::new, false)),
    );
    let (left, _) = graph.render_to_the_end(512);

    let heard_by_the_instance = (0..1600)
        .map(|index| value_of(0, 0, index) * 0.5)
        .chain((0..1600).map(|index| value_of(0, 1, index)));
    let other_track = (0..3200).map(|index| value_of(1, 0, index) * 0.25);
    let expected: Vec<f32> = running_sum_of(heard_by_the_instance)
        .into_iter()
        .zip(other_track)
        .map(|(bus, other)| bus + other)
        .collect();
    assert_eq!(
        left, expected,
        "the first clip's volume is before the sum; the other track is added as it is"
    );
}

#[test]
fn an_effect_of_a_bus_that_has_a_latency_is_made_up_for_across_the_clips_it_hears_for_any_blocks() {
    struct Delaying {
        latency: usize,
        given: Arc<AtomicUsize>,
    }
    impl Insertions for Delaying {
        fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
            Vec::new()
        }

        fn for_bus(&mut self, _request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
            vec![Box::new(Delays::new(self.latency, &self.given))]
        }
    }
    let ramps = Ramps::default().with_length(0, 1, 1000);
    let items = || {
        vec![
            clip(2, LoudnessToExport::Constant(0.5)),
            clip(1, LoudnessToExport::UNCHANGED),
            silence(1),
            clip(1, LoudnessToExport::UNCHANGED),
        ]
    };
    let plain = graph_of(specification(thirty(), 5, vec![items()]), &ramps)
        .render_to_the_end(512)
        .0;
    for latency in [1_usize, 64, 300, 2000] {
        for block in [1, 13, 512, 100_000] {
            let given = Arc::new(AtomicUsize::new(0));
            let made = SoundGraph::new(
                specification_with_buses(thirty(), 5, vec![(items(), vec![(0, 1), (3, 3)])]),
                Box::new(ramps.clone()),
                Box::new(Delaying {
                    latency,
                    given: Arc::clone(&given),
                }),
            )
            .render_to_the_end(block)
            .0;

            assert_eq!(made, plain, "latency {latency}, blocks of {block}");
        }
    }
}

// --- tails: an effect goes on after its clip, and after the last clip of a bus (phase 12, piece 12.2) ---

/// A sound effect, installed, that has the tail `tail` (and the clip that has it in its chain says so: the engine reads the tail from the specification and the effect itself is
/// made up by [`Echoes`]).
fn an_effect_that_rings_for(tail: crate::project::TailLength) -> crate::composition::LayerEffect {
    let plugin =
        crate::test_support::an_installed_sound_plugin("ladspa:a:rings", "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]);
    crate::project::set_installed_plugins(vec![plugin]);
    crate::composition::LayerEffect {
        clock: Default::default(),
        instance: crate::project::EffectInstanceId::FIRST,
        kind: crate::project::EffectKind::installed_plugin_kinds()[0],
        values: vec![crate::project::PropertyValue::Number(1.0)],
        animation: Default::default(),
        tables: Vec::new(),
        state: None,
        tail_length: tail,
        shared_by: None,
    }
}

/// An effect that has a tail without end: every sample it gives is the sample it is given and half of what it gave before (each channel by itself).
#[derive(Default)]
struct Echoes {
    left: f32,
    right: f32,
}

impl BlockProcessor for Echoes {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for sample in left {
            self.left = *sample + 0.5 * self.left;
            *sample = self.left;
        }
        for sample in right {
            self.right = *sample + 0.5 * self.right;
            *sample = self.right;
        }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// What `Echoes` gives for `input`.
fn echoed(input: impl IntoIterator<Item = f32>) -> Vec<f32> {
    let mut memory = 0.0_f32;
    input
        .into_iter()
        .map(|sample| {
            memory = sample + 0.5 * memory;
            memory
        })
        .collect()
}

/// Gives `Echoes` to the clips whose item number is in `items` (of any strip), to a bus too if `for_buses`, and counts the ones that are alive.
struct RingingInsertions {
    items: &'static [usize],
    for_buses: bool,
    alive: Arc<AtomicUsize>,
}

struct EchoesThatAreCounted(Echoes, Arc<AtomicUsize>);

impl BlockProcessor for EchoesThatAreCounted {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.0.process(left, right);
    }

    fn reset(&mut self) {
        self.0.reset();
    }
}

impl Drop for EchoesThatAreCounted {
    fn drop(&mut self) {
        self.1.fetch_sub(1, Ordering::SeqCst);
    }
}

impl RingingInsertions {
    fn new(items: &'static [usize], for_buses: bool) -> Self {
        Self {
            items,
            for_buses,
            alive: Arc::default(),
        }
    }

    fn one(&self) -> Vec<Box<dyn BlockProcessor>> {
        self.alive.fetch_add(1, Ordering::SeqCst);
        vec![Box::new(EchoesThatAreCounted(
            Echoes::default(),
            Arc::clone(&self.alive),
        ))]
    }
}

impl Insertions for RingingInsertions {
    fn for_clip(&mut self, request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        if self.items.contains(&request.item) {
            self.one()
        } else {
            Vec::new()
        }
    }

    fn for_bus(&mut self, _request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        if self.for_buses { self.one() } else { Vec::new() }
    }
}

/// A clip of `frames` frames that has `effect` in its chain.
fn a_clip_that_has(
    frames: u64,
    loudness: LoudnessToExport,
    effect: &crate::composition::LayerEffect,
) -> SoundItemToExport {
    let SoundItemToExport::Clip(mut clip) = clip(frames, loudness) else {
        unreachable!("a clip");
    };
    clip.effects = vec![effect.clone()];
    SoundItemToExport::Clip(clip)
}

const A_TAIL_OF_960: crate::project::TailLength = crate::project::TailLength::Milliseconds(20);

#[test]
fn the_effects_of_a_clip_go_on_after_it_for_their_tail_and_no_longer() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let ramps = Ramps::default();
    let mut graph = SoundGraph::new(
        specification(
            thirty(),
            3,
            vec![vec![
                a_clip_that_has(1, LoudnessToExport::UNCHANGED, &effect),
                silence(2),
            ]],
        ),
        Box::new(ramps),
        Box::new(RingingInsertions::new(&[0], false)),
    );

    let (left, right) = graph.render_to_the_end(512);

    let feed = (0..1600).map(|index| value_of(0, 0, index));
    let expected = echoed(feed.chain(std::iter::repeat_n(0.0, 960)));
    assert_eq!(
        &left[..1600 + 960],
        &expected[..],
        "the clip, and then the effect given silence for its tail of 960 samples"
    );
    assert!(
        left[1600 + 960..].iter().all(|sample| *sample == 0.0),
        "and then nothing: the tail ended"
    );
    assert_eq!(right.len(), left.len());
    assert!(
        expected[1600..].iter().any(|sample| *sample != 0.0),
        "(the control: the tail is something)"
    );
}

#[test]
fn the_tail_of_a_clip_is_made_as_loud_as_the_clip_and_is_added_to_the_clips_that_come_after_it() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let ramps = Ramps::default();
    let mut graph = SoundGraph::new(
        specification(
            thirty(),
            3,
            vec![vec![
                a_clip_that_has(1, LoudnessToExport::Constant(0.5), &effect),
                clip(1, LoudnessToExport::UNCHANGED),
            ]],
        ),
        Box::new(ramps),
        Box::new(RingingInsertions::new(&[0], false)),
    );

    let (left, _) = graph.render_to_the_end(512);

    let first_clip = echoed(
        (0..1600)
            .map(|index| value_of(0, 0, index))
            .chain(std::iter::repeat_n(0.0, 960)),
    );
    let mut expected: Vec<f32> = first_clip.iter().map(|sample| sample * 0.5).collect();
    expected.resize(3200, 0.0);
    for (index, sample) in (0..1600).map(|index| value_of(0, 1, index)).enumerate() {
        expected[1600 + index] += sample;
    }
    assert_eq!(
        &left[..3200],
        &expected[..3200],
        "the next clip's sound, with the ring of the one before it added to its first samples, at the volume of the one before"
    );
}

#[test]
fn the_effects_of_a_clip_that_ring_on_are_let_go_of_when_the_ring_has_ended_and_not_before() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let insertions = RingingInsertions::new(&[0], false);
    let alive = Arc::clone(&insertions.alive);
    let mut graph = SoundGraph::new(
        specification(
            thirty(),
            3,
            vec![vec![
                a_clip_that_has(1, LoudnessToExport::UNCHANGED, &effect),
                silence(2),
            ]],
        ),
        Box::new(Ramps::default()),
        Box::new(insertions),
    );
    let (mut left, mut right) = (vec![0.0; 800], vec![0.0; 800]);
    let mut seen = Vec::new();
    // Blocks of 800 samples: the clip is 1600, its ring 960 more, so the ring ends in the fourth block.
    for _ in 0..6 {
        graph.render(&mut left, &mut right);
        seen.push(alive.load(Ordering::SeqCst));
    }

    assert_eq!(seen, [1, 1, 1, 0, 0, 0]);
}

#[test]
fn the_sound_of_a_tail_is_the_same_for_any_size_of_block() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let make = |block: usize| {
        SoundGraph::new(
            specification(
                thirty(),
                4,
                vec![vec![
                    a_clip_that_has(1, LoudnessToExport::Constant(0.5), &effect),
                    silence(1),
                    a_clip_that_has(1, LoudnessToExport::UNCHANGED, &effect),
                ]],
            ),
            Box::new(Ramps::default().with_length(0, 2, 1000)),
            Box::new(RingingInsertions::new(&[0, 2], false)),
        )
        .render_to_the_end(block)
    };
    let whole = make(1_000_000);
    for block in [1, 7, 512, 1599, 1601, 960] {
        assert_eq!(make(block), whole, "blocks of {block}");
    }
}

#[test]
fn a_tail_that_would_be_heard_after_the_end_of_the_sound_is_cut_there() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(crate::project::TailLength::Milliseconds(5_000));
    let mut graph = SoundGraph::new(
        specification(
            thirty(),
            2,
            vec![vec![
                a_clip_that_has(1, LoudnessToExport::UNCHANGED, &effect),
                silence(1),
            ]],
        ),
        Box::new(Ramps::default()),
        Box::new(RingingInsertions::new(&[0], false)),
    );

    let (left, _) = graph.render_to_the_end(512);

    assert_eq!(
        left.len(),
        3200,
        "the sound is as long as the timeline, whatever rings on"
    );
}

#[test]
fn the_effects_of_a_bus_ring_on_after_its_last_clip_for_the_tail_of_the_effects_of_the_track() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let mut specification = specification_with_buses(
        thirty(),
        4,
        vec![(
            vec![
                clip(1, LoudnessToExport::UNCHANGED),
                clip(1, LoudnessToExport::UNCHANGED),
                silence(2),
            ],
            vec![(0, 1)],
        )],
    );
    specification.strips[0].buses[0].effects = vec![effect];
    let mut graph = SoundGraph::new(
        specification,
        Box::new(Ramps::default()),
        Box::new(RingingInsertions::new(&[], true)),
    );

    let (left, _) = graph.render_to_the_end(512);

    let feed = |item: usize| (0..1600).map(move |index| value_of(0, item, index));
    let expected = echoed(feed(0).chain(feed(1)).chain(std::iter::repeat_n(0.0, 960)));
    assert_eq!(&left[..3200 + 960], &expected[..]);
    assert!(left[3200 + 960..].iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_clip_of_a_bus_that_has_effects_of_its_own_that_ring_on_is_heard_by_the_bus_for_as_long_as_it_rings() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let own = an_effect_that_rings_for(A_TAIL_OF_960);
    let mut specification = specification_with_buses(
        thirty(),
        3,
        vec![(
            vec![a_clip_that_has(1, LoudnessToExport::UNCHANGED, &own), silence(2)],
            vec![(0, 0)],
        )],
    );
    // (The effects of the bus: none that ring.)
    specification.strips[0].buses[0].effects = Vec::new();
    let mut graph = SoundGraph::new(
        specification,
        Box::new(Ramps::default()),
        Box::new(RingingInsertions::new(&[0], false)),
    );

    let (left, _) = graph.render_to_the_end(512);

    let expected = echoed(
        (0..1600)
            .map(|index| value_of(0, 0, index))
            .chain(std::iter::repeat_n(0.0, 960)),
    );
    assert_eq!(
        &left[..1600 + 960],
        &expected[..],
        "the ring of the clip's own effects goes through the bus: the bus is heard until it has ended"
    );
}

// --- lead-in: sound that is made and not heard, so that the effects have heard what leads up to the place the sound is wanted from (phase 12, piece 12.2d) ---

#[test]
fn the_lead_in_is_made_and_not_heard_so_the_sound_is_the_end_of_the_sound_that_is_made_from_the_start_for_any_blocks() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let effect = an_effect_that_rings_for(A_TAIL_OF_960);
    let items = || {
        vec![
            a_clip_that_has(1, LoudnessToExport::UNCHANGED, &effect),
            silence(1),
            a_clip_that_has(1, LoudnessToExport::Constant(0.5), &effect),
        ]
    };
    let made = |lead_in_frames: u64, block: usize| {
        let mut specification = specification(thirty(), 3, vec![items()]);
        specification.lead_in_frames = lead_in_frames;
        let mut graph = SoundGraph::new(
            specification,
            Box::new(Ramps::default()),
            Box::new(RingingInsertions::new(&[0, 2], false)),
        );
        let total = (graph.total_samples(), graph.position());
        (graph.render_to_the_end(block), total)
    };
    let (from_the_start, total) = made(0, 512);
    assert_eq!(total, (4800, 0));

    for lead_in_frames in [1, 2] {
        for block in [1, 13, 512, 100_000] {
            let ((left, right), total) = made(lead_in_frames, block);
            let thrown_away = 1600 * lead_in_frames as usize;
            assert_eq!(
                left,
                from_the_start.0[thrown_away..],
                "{lead_in_frames} frames of lead-in, blocks of {block}"
            );
            assert_eq!(right, from_the_start.1[thrown_away..]);
            assert_eq!(
                total,
                (4800 - thrown_away as u64, 0),
                "the length is what is heard, and nothing has been made yet"
            );
        }
    }
}

#[test]
fn the_position_of_a_graph_that_has_a_lead_in_counts_what_is_heard() {
    let mut specification = specification(thirty(), 3, vec![vec![clip(3, LoudnessToExport::UNCHANGED)]]);
    specification.lead_in_frames = 1;
    let mut graph = graph_of(specification, &Ramps::default());
    let (mut left, mut right) = (vec![0.0; 100], vec![0.0; 100]);

    graph.render(&mut left, &mut right);

    assert_eq!(
        graph.position(),
        100,
        "the lead-in was made, and what is counted is the sound after it"
    );
    assert_eq!(
        left[0],
        value_of(0, 0, 1600),
        "and the first sample heard is the one that comes after the lead-in"
    );
}
