// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! **The null test of the sound engine** (phase 7, piece 7.1; AU-09; the design is `docs/engine-phase-7-design.md`, G7): the sound that the block graph makes is held to the
//! sound that the filter graph of ffmpeg makes, the graph that the preview and the export have used until now, from the very same specification, with the real ffmpeg.
//!
//! Sources are sound files with noise in them (a different number at every sample, so that a clip that is a sample out of place shows). The oracle is the filter graph
//! ([`build_audio_graph`] through an [`AudioStream`], which is how the preview gets its sound). The block graph is given, for each clip, the sound that
//! [`clip_feed_filters`] makes of it (an ffmpeg that runs for each clip: what the preview will do from piece 7.2 on), and does the rest itself.
//!
//! What is expected to be different, and is measured and not assumed: the volume that changes between points (ffmpeg changes it once for each chunk of sound it handles, the
//! block graph at every sample), and a frame rate that is not a whole number of samples to a frame (ffmpeg adds up the lengths of the items, each rounded up).

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use fastforwardrs::media::{
    AudioFormat, AudioStream, AudioStreamEvent, AudioStreamRequest, EngineAudioStream, FfmpegToolchain,
    render_sound_to_file,
};
use fastforwardrs::project::{FrameRate, SpedUpClipSound, SpeedFactor};
use support::sound_files::{FILE_SECONDS, steady_file, the_sources};
use support::{RandomNumbers, scratch_directory};

const TIME_ALLOWED: Duration = Duration::from_secs(120);

const STEREO_AT_48000: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channel_count: 2,
};

fn by_the_filter_graph(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
) -> Vec<f32> {
    by_the_filter_graph_as(toolchain, specification, sources, STEREO_AT_48000)
}

/// The same, in the kind of sound the speakers want.
fn by_the_filter_graph_as(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
    format: AudioFormat,
) -> Vec<f32> {
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification.clone(),
        format,
    };
    let stream = AudioStream::start(toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => samples.extend(more),
            Some(AudioStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None, "ffmpeg made the sound");
                return samples;
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "the stream did not finish in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

/// The sound of `specification` as the sound engine makes it for the export: into a file, by the graph with an ffmpeg for each clip (`render_sound_to_file`), read back.
/// (`block` is no longer asked for: the export makes its sound in blocks of its own, and the graph gives the same sound for any block, which the unit tests hold it to.)
fn by_the_block_graph(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
    _block: usize,
) -> Vec<f32> {
    let folder = scratch_directory();
    let file = folder.path().join("sound.f32");
    render_sound_to_file(
        toolchain,
        specification,
        sources.iter().map(|path| path.clone().into()).collect(),
        &file,
        &|| false,
    )
    .expect("the sound is made");
    let bytes = fs::read(&file).expect("the file of sound is there");
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|sample| f32::from_le_bytes(*sample))
        .collect()
}

/// The sound of `specification` as the sound engine makes it for the preview: the events of an [`EngineAudioStream`], put together.
fn by_the_engine_stream(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
) -> Vec<f32> {
    by_the_engine_stream_as(toolchain, specification, sources, STEREO_AT_48000)
}

/// The same, in the kind of sound the speakers want.
fn by_the_engine_stream_as(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
    format: AudioFormat,
) -> Vec<f32> {
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification.clone(),
        format,
    };
    let stream = EngineAudioStream::start(toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => samples.extend(more),
            Some(AudioStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None, "the engine made the sound");
                return samples;
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "the stream did not finish in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

/// How two sounds differ.
#[derive(Debug)]
struct Difference {
    /// Moments of sound (a sample of each channel).
    moments_of_the_oracle: usize,
    moments_of_the_block_graph: usize,
    /// The largest difference of any sample, of those that both have.
    largest: f32,
    /// Where it is, in moments.
    largest_at: usize,
    /// The root of the mean of the squares of the differences, over the root of the mean of the squares of the oracle's samples (so 0.001 is 60 decibels below the sound).
    relative_rms: f64,
}

fn compare(oracle: &[f32], block_graph: &[f32]) -> Difference {
    let common = oracle.len().min(block_graph.len());
    let (mut largest, mut largest_at) = (0.0_f32, 0);
    let (mut squares_of_differences, mut squares_of_oracle) = (0.0_f64, 0.0_f64);
    for place in 0..common {
        let difference = (oracle[place] - block_graph[place]).abs();
        if difference > largest {
            (largest, largest_at) = (difference, place / 2);
        }
        squares_of_differences += f64::from(difference).powi(2);
        squares_of_oracle += f64::from(oracle[place]).powi(2);
    }
    Difference {
        moments_of_the_oracle: oracle.len() / 2,
        moments_of_the_block_graph: block_graph.len() / 2,
        largest,
        largest_at,
        relative_rms: if squares_of_oracle > 0.0 {
            (squares_of_differences / squares_of_oracle).sqrt()
        } else {
            squares_of_differences.sqrt()
        },
    }
}

// --- made-up specifications ---

/// Which kinds of thing a made-up specification has.
#[derive(Clone, Copy, Debug)]
struct Kinds {
    /// Clips whose speed is more than 1.
    sped_up: bool,
    /// Clips whose volume changes between points.
    changing_volume: bool,
    /// Clips that go on past the end of their file, or stop short of the place they are given on the timeline.
    wrong_lengths: bool,
}

fn a_specification(
    seed: u64,
    frame_rate: FrameRate,
    kinds: Kinds,
    sped_up_clip_sound: SpedUpClipSound,
) -> SoundGraphSpecification {
    let mut random = RandomNumbers::with_seed(seed);
    let strip_count = 1 + random.next_below(3) as usize;
    let mut longest_strip = 0;
    let strips: Vec<SoundStripToExport> = (0..strip_count)
        .map(|_| {
            let mut frames = 0;
            let items = (0..1 + random.next_below(5))
                .map(|_| {
                    let length = 1 + random.next_below(40);
                    if random.next_below(3) == 0 {
                        frames += length;
                        return SoundItemToExport::Silence { frame_count: length };
                    }
                    let source_input = random.next_below(FILE_SECONDS.len() as u64) as usize;
                    let file_frames = (FILE_SECONDS[source_input] * frame_rate.frames_per_second()) as u64;
                    let speed = if kinds.sped_up && random.next_below(2) == 0 {
                        [2, 3, 5, 30][random.next_below(4) as usize]
                    } else {
                        1
                    };
                    let first_source_frame =
                        random.next_below(file_frames.saturating_sub(length * u64::from(speed)).max(1));
                    let mut end_source_frame = first_source_frame + length * u64::from(speed);
                    if kinds.wrong_lengths && random.next_below(4) == 0 {
                        end_source_frame += random.next_below(file_frames);
                    }
                    let mut timeline_frame_count = (end_source_frame - first_source_frame).div_ceil(u64::from(speed));
                    if kinds.wrong_lengths && random.next_below(4) == 0 {
                        timeline_frame_count = timeline_frame_count + 1 + random.next_below(6);
                    }
                    frames += timeline_frame_count;
                    let loudness = match random.next_below(if kinds.changing_volume { 3 } else { 2 }) {
                        0 => LoudnessToExport::UNCHANGED,
                        1 => LoudnessToExport::Constant(0.2 + random.next_below(18) as f64 / 10.0),
                        _ => LoudnessToExport::Changing(vec![
                            (0, random.next_below(20) as f32 - 15.0),
                            (timeline_frame_count / 2 + 1, random.next_below(20) as f32 - 15.0),
                            (timeline_frame_count + 1, random.next_below(20) as f32 - 15.0),
                        ]),
                    };
                    SoundItemToExport::Clip(SoundClipToExport {
                        source_input,
                        first_source_frame,
                        end_source_frame,
                        speed_factor: SpeedFactor::new(speed).unwrap(),
                        timeline_frame_count,
                        loudness,
                        first_timeline_frame: 0,
                        frames_of_the_clip_before: 0,
                        effects: Vec::new(),
                        sent_to: None,
                        tail_routes: Vec::new(),
                    })
                })
                .collect();
            longest_strip = longest_strip.max(frames);
            SoundStripToExport {
                items,
                buses: Vec::new(),
                loudness: Default::default(),
                muted: Vec::new(),
            }
        })
        .collect();
    SoundGraphSpecification {
        frame_rate,
        total_frame_count: longest_strip + random.next_below(4),
        strips,
        sped_up_clip_sound,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    }
}

/// How the two sounds differ for `specification`, both made from `sources`.
fn difference_for(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
) -> Difference {
    let oracle = by_the_filter_graph(toolchain, specification, sources);
    let ours = by_the_block_graph(toolchain, specification, sources, 512);
    compare(&oracle, &ours)
}

/// Runs `check` for each of `cases`, a few at a time (each is a handful of ffmpeg processes), and gives what each said.
fn in_parallel<Case: Sync, Output: Send>(cases: &[Case], check: impl Fn(&Case) -> Output + Sync) -> Vec<Output> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let outputs = std::sync::Mutex::new((0..cases.len()).map(|_| None).collect::<Vec<Option<Output>>>());
    std::thread::scope(|scope| {
        for _ in 0..4.min(cases.len()) {
            scope.spawn(|| {
                loop {
                    let number = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some(case) = cases.get(number) else { return };
                    let output = check(case);
                    outputs.lock().unwrap()[number] = Some(output);
                }
            });
        }
    });
    outputs.into_inner().unwrap().into_iter().map(Option::unwrap).collect()
}

/// A rounding of the last bit of a sample around 1: the filter graph mixes with 0.5 and doubles, the block graph does not, and a float multiplied by 0.5 and by 2 is exact
/// but a mix of three is not.
const ONE_ROUNDING_OF_A_SAMPLE: f32 = 4.0e-7;

#[test]
fn with_constant_volumes_the_block_graph_makes_the_sound_the_filter_graph_makes_to_within_a_rounding_of_a_sample() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    // Every frame rate at which a frame is a whole number of samples; every setting for sped-up clips; clips of normal speed, sped up, too long and too short for their place.
    let mut cases = Vec::new();
    for (numerator, kinds) in [
        (
            24,
            Kinds {
                sped_up: false,
                changing_volume: false,
                wrong_lengths: false,
            },
        ),
        (
            25,
            Kinds {
                sped_up: true,
                changing_volume: false,
                wrong_lengths: true,
            },
        ),
        (
            30,
            Kinds {
                sped_up: true,
                changing_volume: false,
                wrong_lengths: true,
            },
        ),
        (
            60,
            Kinds {
                sped_up: true,
                changing_volume: false,
                wrong_lengths: false,
            },
        ),
    ] {
        for setting in [
            SpedUpClipSound::Mute,
            SpedUpClipSound::MaintainPitch,
            SpedUpClipSound::IncreasePitch,
        ] {
            for seed in 0..4_u64 {
                cases.push((seed, FrameRate::new(numerator, 1).unwrap(), kinds, setting));
            }
        }
    }
    let results = in_parallel(&cases, |&(seed, frame_rate, kinds, setting)| {
        let specification = a_specification(seed * 7 + 3, frame_rate, kinds, setting);
        (
            specification.clone(),
            difference_for(&toolchain, &specification, &sources),
        )
    });
    let mut worst = 0.0_f32;
    for ((seed, frame_rate, kinds, setting), (specification, difference)) in cases.iter().zip(&results) {
        let name = format!("seed {seed}, {frame_rate:?}, {kinds:?}, {setting:?}: {specification:?}");
        assert_eq!(
            difference.moments_of_the_oracle, difference.moments_of_the_block_graph,
            "the two sounds are as long as each other: {name}"
        );
        assert!(
            difference.largest <= ONE_ROUNDING_OF_A_SAMPLE,
            "the sounds differ by {} at moment {}: {name}",
            difference.largest,
            difference.largest_at
        );
        worst = worst.max(difference.largest);
    }
    eprintln!(
        "the largest difference of any sample in {} made-up projects: {worst}",
        cases.len()
    );
}

#[test]
fn the_filter_graph_changes_the_volume_in_steps_of_a_chunk_of_sound_and_the_block_graph_at_every_sample() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let steady = steady_file(folder.path(), "steady.wav", 4.0, 0.5);
    let thirty = FrameRate::new(30, 1).unwrap();
    let specification = SoundGraphSpecification {
        frame_rate: thirty,
        total_frame_count: 60,
        strips: vec![SoundStripToExport {
            items: vec![SoundItemToExport::Clip(SoundClipToExport {
                source_input: 0,
                first_source_frame: 0,
                end_source_frame: 60,
                speed_factor: SpeedFactor::new(1).unwrap(),
                timeline_frame_count: 60,
                loudness: LoudnessToExport::Changing(vec![(0, 0.0), (60, -40.0)]),
                first_timeline_frame: 0,
                frames_of_the_clip_before: 0,
                effects: Vec::new(),
                sent_to: None,
                tail_routes: Vec::new(),
            })],
            buses: Vec::new(),
            loudness: Default::default(),
            muted: Vec::new(),
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    };
    let sources = [steady];
    let oracle = by_the_filter_graph(&toolchain, &specification, &sources);
    let ours = by_the_block_graph(&toolchain, &specification, &sources, 512);
    // 40 dB down in two seconds: 20 decibels a second. The sound is 0.5 all the way through, so the sound is the volume.
    let decibels_a_second = 20.0_f64;
    let volume_at = |second: f64| 0.5 * 10f64.powf(-decibels_a_second * second / 20.0);
    let mut lengths_of_the_steps = std::collections::BTreeSet::new();
    let mut largest_gap = 0.0_f64;
    let mut step_began = 0;
    for moment in 0..oracle.len() / 2 {
        // The block graph is the curve at every sample.
        let curve = volume_at(moment as f64 / 48_000.0);
        assert!((f64::from(ours[moment * 2]) - curve).abs() < 1e-6, "moment {moment}");
        // The filter graph is the curve at the start of the chunk of 4096 moments the moment is in.
        if moment > 0 && oracle[moment * 2] != oracle[(moment - 1) * 2] {
            lengths_of_the_steps.insert(moment - step_began);
            step_began = moment;
        }
        let at_the_start_of_its_chunk = volume_at((moment / 4096 * 4096) as f64 / 48_000.0);
        assert!(
            (f64::from(oracle[moment * 2]) - at_the_start_of_its_chunk).abs() < 1e-6,
            "moment {moment}: {} and {at_the_start_of_its_chunk}",
            oracle[moment * 2]
        );
        largest_gap = largest_gap.max((f64::from(oracle[moment * 2]) - curve).abs());
    }
    assert_eq!(
        lengths_of_the_steps,
        [4096].into(),
        "the steps are all 4096 moments long (the last one, which is cut off, is not counted)"
    );
    // How far the staircase is from the curve at the worst: a step of 4096 moments at 20 decibels a second is 1.7 decibels, which is 21 % of the sound at its start.
    let most_a_step_can_be_out = 0.5 * (1.0 - 10f64.powf(-decibels_a_second * 4096.0 / 48_000.0 / 20.0));
    assert!(largest_gap < most_a_step_can_be_out + 1e-6, "{largest_gap}");
    eprintln!("the staircase is at most {largest_gap} from the curve, in a sound of 0.5");
}

#[test]
fn with_volumes_that_change_the_sounds_differ_by_no_more_than_the_staircase_of_the_filter_graph_allows() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: true,
        changing_volume: true,
        wrong_lengths: true,
    };
    let cases: Vec<u64> = (0..16).collect();
    let results = in_parallel(&cases, |&seed| {
        let specification = a_specification(
            seed,
            FrameRate::new(30, 1).unwrap(),
            kinds,
            SpedUpClipSound::MaintainPitch,
        );
        (
            specification.clone(),
            difference_for(&toolchain, &specification, &sources),
        )
    });
    for (seed, (specification, difference)) in cases.iter().zip(&results) {
        assert_eq!(
            difference.moments_of_the_oracle, difference.moments_of_the_block_graph,
            "seed {seed}: the two sounds are as long as each other"
        );
        // The filter graph's volume is the curve at the start of a chunk of sound, which is at most this long (a file of this kind gives chunks of 4096; sped-up sound
        // comes out in other sizes, so the bound has room for four times as many), and the curve cannot have moved further than its steepest slope in that time.
        let mut steepest_decibels_a_second = 0.0_f64;
        for item in specification.strips.iter().flat_map(|strip| &strip.items) {
            if let SoundItemToExport::Clip(SoundClipToExport {
                loudness: LoudnessToExport::Changing(points),
                ..
            }) = item
            {
                for pair in points.windows(2) {
                    let seconds = (pair[1].0 - pair[0].0) as f64 / 30.0;
                    steepest_decibels_a_second =
                        steepest_decibels_a_second.max(f64::from((pair[1].1 - pair[0].1).abs()) / seconds);
                }
            }
        }
        let chunk_seconds = 4.0 * 4096.0 / 48_000.0;
        let most_the_volume_can_be_out = 10f64.powf(steepest_decibels_a_second * chunk_seconds / 20.0) - 1.0;
        // (Strips are added, and noise in one is not noise in another: the differences of three add up, in the worst case, three times as much.)
        let bound = 3.0 * most_the_volume_can_be_out + 1e-6;
        assert!(
            difference.relative_rms <= bound,
            "seed {seed}: the sounds differ by {} of the sound, and the staircase allows {bound} ({steepest_decibels_a_second} decibels a second at the steepest): {specification:?}",
            difference.relative_rms
        );
    }
}

#[test]
fn at_a_frame_rate_that_is_not_a_whole_number_of_samples_a_frame_the_clips_are_the_same_sound_and_the_old_graph_is_the_one_that_drifts()
 {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let frame_rate = FrameRate::new(30000, 1001).unwrap();
    let kinds = Kinds {
        sped_up: false,
        changing_volume: false,
        wrong_lengths: false,
    };
    for seed in 0..10_u64 {
        // One strip, so that each item can be found in each sound by itself.
        let mut specification = a_specification(seed + 100, frame_rate, kinds, SpedUpClipSound::MaintainPitch);
        specification.strips.truncate(1);
        specification.total_frame_count = specification.strips[0]
            .items
            .iter()
            .map(|item| match item {
                SoundItemToExport::Silence { frame_count } => *frame_count,
                SoundItemToExport::Clip(clip) => clip.timeline_frame_count,
            })
            .sum();
        let oracle = by_the_filter_graph(&toolchain, &specification, &sources);
        let ours = by_the_block_graph(&toolchain, &specification, &sources, 512);
        let exact_start = |frames: u64| fastforwardrs::sound_engine::sample_at_frame(frame_rate, frames) as usize;
        let mut frames_before = 0;
        let mut worst_drift = 0;
        for (number, item) in specification.strips[0].items.iter().enumerate() {
            let (frames, is_a_clip) = match item {
                SoundItemToExport::Silence { frame_count } => (*frame_count, false),
                SoundItemToExport::Clip(clip) => (clip.timeline_frame_count, true),
            };
            if is_a_clip {
                let start = exact_start(frames_before);
                let length = (exact_start(frames_before + frames) - start).min(4000);
                let ours_here = &ours[start * 2..(start + length) * 2];
                // The filter graph has each item as long as its length in seconds, in whole samples rounded up, and adds them up: so it is up to one sample further on for
                // each item before this one.
                let found_at = (0..=number + 1).find(|drift| {
                    oracle.len() >= (start + drift + length) * 2
                        && oracle[(start + drift) * 2..(start + drift + length) * 2]
                            .iter()
                            .zip(ours_here)
                            .all(|(a, b)| (a - b).abs() <= ONE_ROUNDING_OF_A_SAMPLE)
                });
                let drift = found_at.unwrap_or_else(|| {
                    panic!(
                        "seed {seed}, item {number}: the clip is not in the filter graph's sound within {} samples",
                        number + 1
                    )
                });
                worst_drift = worst_drift.max(drift);
            }
            frames_before += frames;
        }
        eprintln!(
            "seed {seed}: the filter graph's clips are at most {worst_drift} samples later than the exact places"
        );
    }
}

#[test]
fn the_preview_and_the_export_make_the_very_same_sound_and_it_is_the_sound_of_the_filter_graph_for_constant_volumes() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: true,
        changing_volume: false,
        wrong_lengths: true,
    };
    let seeds: Vec<u64> = (0..8).collect();
    let results = in_parallel(&seeds, |&seed| {
        let specification = a_specification(
            seed + 50,
            FrameRate::new(30, 1).unwrap(),
            kinds,
            SpedUpClipSound::MaintainPitch,
        );
        let for_the_export = by_the_block_graph(&toolchain, &specification, &sources, 512);
        let for_the_preview = by_the_engine_stream(&toolchain, &specification, &sources);
        let oracle = by_the_filter_graph(&toolchain, &specification, &sources);
        let difference = compare(&oracle, &for_the_preview);
        (for_the_export, for_the_preview, difference)
    });
    for (seed, (for_the_export, for_the_preview, difference)) in seeds.iter().zip(&results) {
        assert_eq!(
            for_the_preview, for_the_export,
            "seed {seed}: the stream and the file are one sound, bit for bit"
        );
        assert!(!for_the_export.is_empty());
        assert_eq!(
            difference.moments_of_the_oracle, difference.moments_of_the_block_graph,
            "seed {seed}"
        );
        assert!(
            difference.largest <= ONE_ROUNDING_OF_A_SAMPLE,
            "seed {seed}: {difference:?}"
        );
    }
}

#[test]
fn a_clip_whose_file_cannot_be_read_is_silence_and_the_sound_says_so_when_it_ends() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let mut sources = the_sources(folder.path());
    sources[1] = folder.path().join("not_a_file.wav");
    let specification = SoundGraphSpecification {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        total_frame_count: 60,
        strips: vec![SoundStripToExport {
            items: vec![
                SoundItemToExport::Clip(SoundClipToExport {
                    source_input: 0,
                    first_source_frame: 0,
                    end_source_frame: 30,
                    speed_factor: SpeedFactor::new(1).unwrap(),
                    timeline_frame_count: 30,
                    loudness: LoudnessToExport::UNCHANGED,
                    first_timeline_frame: 0,
                    frames_of_the_clip_before: 0,
                    effects: Vec::new(),
                    sent_to: None,
                    tail_routes: Vec::new(),
                }),
                SoundItemToExport::Clip(SoundClipToExport {
                    source_input: 1,
                    first_source_frame: 0,
                    end_source_frame: 30,
                    speed_factor: SpeedFactor::new(1).unwrap(),
                    timeline_frame_count: 30,
                    loudness: LoudnessToExport::UNCHANGED,
                    first_timeline_frame: 0,
                    frames_of_the_clip_before: 0,
                    effects: Vec::new(),
                    sent_to: None,
                    tail_routes: Vec::new(),
                }),
            ],
            buses: Vec::new(),
            loudness: Default::default(),
            muted: Vec::new(),
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    };
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification.clone(),
        format: STEREO_AT_48000,
    };
    let stream = EngineAudioStream::start(&toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let started = Instant::now();
    let complaint = loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(_)) => {}
            Some(AudioStreamEvent::Finished { error_output }) => break error_output,
            None => {
                assert!(started.elapsed() < TIME_ALLOWED);
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    };
    assert!(
        complaint.as_deref().is_some_and(|text| text.contains("not_a_file")),
        "the stream says what ffmpeg said: {complaint:?}"
    );

    // The export fails with the same words, and the sound of the first clip is not lost to the second's trouble.
    let file = folder.path().join("sound.f32");
    let problem = render_sound_to_file(
        &toolchain,
        &specification,
        sources.iter().map(|path| path.clone().into()).collect(),
        &file,
        &|| false,
    )
    .expect_err("the sound cannot be made");
    assert!(problem.to_string().contains("not_a_file"), "{problem}");
}

#[test]
fn the_sound_of_an_export_that_is_stopped_is_not_finished_and_the_stop_is_said() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: false,
        changing_volume: false,
        wrong_lengths: false,
    };
    let specification = a_specification(3, FrameRate::new(30, 1).unwrap(), kinds, SpedUpClipSound::MaintainPitch);
    let file = folder.path().join("sound.f32");
    let blocks_asked = std::sync::atomic::AtomicUsize::new(0);
    let result = render_sound_to_file(
        &toolchain,
        &specification,
        sources.iter().map(|path| path.clone().into()).collect(),
        &file,
        &|| blocks_asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 5,
    );
    assert!(
        matches!(result, Err(fastforwardrs::media::SoundRenderError::Cancelled)),
        "{result:?}"
    );
    assert!(
        fs::metadata(&file).unwrap().len() <= 5 * 2048 * 8,
        "no more than five blocks were written before it was stopped"
    );
}

#[test]
fn speakers_that_want_another_kind_of_sound_are_given_it_by_a_conversion_and_it_is_the_sound_the_filter_graph_gives_them()
 {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: true,
        changing_volume: false,
        wrong_lengths: true,
    };
    let formats = [
        AudioFormat {
            sample_rate: 44_100,
            channel_count: 2,
        },
        AudioFormat {
            sample_rate: 48_000,
            channel_count: 1,
        },
        AudioFormat {
            sample_rate: 96_000,
            channel_count: 2,
        },
        AudioFormat {
            sample_rate: 44_100,
            channel_count: 6,
        },
    ];
    for format in formats {
        for seed in 0..3_u64 {
            let specification = a_specification(
                seed + 70,
                FrameRate::new(30, 1).unwrap(),
                kinds,
                SpedUpClipSound::MaintainPitch,
            );
            let oracle = by_the_filter_graph_as(&toolchain, &specification, &sources, format);
            let ours = by_the_engine_stream_as(&toolchain, &specification, &sources, format);
            let channels = usize::from(format.channel_count);
            assert_eq!(oracle.len() % channels, 0);
            // The moments are counted in the kind of sound the speakers want. The two conversions are done by ffmpeg's resampler on sound that is the same, in the same
            // order, so they are the same sound, to the rounding of a float.
            let moments_oracle = oracle.len() / channels;
            let moments_ours = ours.len() / channels;
            assert!(
                moments_oracle == moments_ours,
                "{format:?} seed {seed}: {moments_oracle} moments and {moments_ours}"
            );
            let common = oracle.len().min(ours.len());
            let largest = oracle[..common]
                .iter()
                .zip(&ours[..common])
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert!(
                largest <= ONE_ROUNDING_OF_A_SAMPLE,
                "{format:?} seed {seed}: the sounds differ by {largest}"
            );
            eprintln!(
                "{format:?} seed {seed}: {moments_oracle} and {moments_ours} moments, the largest difference {largest}"
            );
        }
    }
}

#[test]
fn the_window_is_woken_when_the_first_sound_is_ready_and_when_the_sound_ends_and_not_otherwise() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: false,
        changing_volume: false,
        wrong_lengths: false,
    };
    // The sound as the speakers want it (stereo at 48000: made by the engine itself) and as they might (44100: through a conversion): the same two wake-ups.
    for format in [
        STEREO_AT_48000,
        AudioFormat {
            sample_rate: 44_100,
            channel_count: 2,
        },
    ] {
        let specification = a_specification(5, FrameRate::new(30, 1).unwrap(), kinds, SpedUpClipSound::MaintainPitch);
        let request = AudioStreamRequest {
            source_files: sources.iter().map(|path| path.clone().into()).collect(),
            sound: specification,
            format,
        };
        let wake_ups = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&wake_ups);
        let stream = EngineAudioStream::start(
            &toolchain,
            &request,
            Arc::new(move || {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
        )
        .expect("the stream starts");
        let started = Instant::now();
        loop {
            match stream.try_next_event() {
                Some(AudioStreamEvent::Finished { .. }) => break,
                Some(_) => {}
                None => {
                    assert!(started.elapsed() < TIME_ALLOWED);
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
        // (The wake-up that goes with the end comes just after the event: it is waited for, and then a moment more is allowed for any that should not come.)
        let waited = Instant::now();
        while wake_ups.load(std::sync::atomic::Ordering::SeqCst) < 2 && waited.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            wake_ups.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "{format:?}: once for the first sound and once for the end"
        );
    }
}

/// A sound file that nothing ever writes to: a named pipe, which ffmpeg waits for for ever when it opens it. (Where there are named pipes.)
#[cfg(unix)]
#[test]
fn an_export_that_is_stopped_while_the_sound_of_a_clip_never_comes_is_stopped_at_once() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let stuck = folder.path().join("stuck.wav");
    let made = std::process::Command::new("mkfifo").arg(&stuck).status();
    if !made.is_ok_and(|status| status.success()) {
        eprintln!("SKIPPED: there is no mkfifo");
        return;
    }
    let specification = SoundGraphSpecification {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        total_frame_count: 30,
        strips: vec![SoundStripToExport {
            items: vec![SoundItemToExport::Clip(SoundClipToExport {
                source_input: 0,
                first_source_frame: 0,
                end_source_frame: 30,
                speed_factor: SpeedFactor::new(1).unwrap(),
                timeline_frame_count: 30,
                loudness: LoudnessToExport::UNCHANGED,
                first_timeline_frame: 0,
                frames_of_the_clip_before: 0,
                effects: Vec::new(),
                sent_to: None,
                tail_routes: Vec::new(),
            })],
            buses: Vec::new(),
            loudness: Default::default(),
            muted: Vec::new(),
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    };
    let began = Instant::now();
    let result = render_sound_to_file(
        &toolchain,
        &specification,
        vec![stuck.clone().into()],
        &folder.path().join("sound.f32"),
        // (Stopped after a second, from outside: the sound of the clip is stuck, and no block is ever finished to ask at.)
        &|| began.elapsed() > Duration::from_secs(1),
    );
    // Whatever happened, the pipe is let go of, so that a failure is a failure and not a test that never ends.
    drop(fs::OpenOptions::new().read(true).write(true).open(&stuck));
    assert!(
        matches!(result, Err(fastforwardrs::media::SoundRenderError::Cancelled)),
        "{result:?}"
    );
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "it was stopped after {:?}",
        began.elapsed()
    );
}

#[test]
fn a_lead_in_is_cut_off_the_front_of_the_sound_by_the_block_graph_and_by_the_filter_graph_alike() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: false,
        changing_volume: false,
        wrong_lengths: false,
    };
    let mut cases = Vec::new();
    for seed in 0..4_u64 {
        for lead_in_frames in [1_u64, 7] {
            cases.push((seed, lead_in_frames));
        }
    }
    let results = in_parallel(&cases, |&(seed, lead_in_frames)| {
        let whole = a_specification(
            seed * 7 + 3,
            FrameRate::new(30, 1).unwrap(),
            kinds,
            SpedUpClipSound::MaintainPitch,
        );
        let mut with_a_lead_in = whole.clone();
        with_a_lead_in.lead_in_frames = lead_in_frames;
        (
            whole.total_frame_count,
            difference_for(&toolchain, &with_a_lead_in, &sources),
            by_the_block_graph(&toolchain, &whole, &sources, 512).len(),
            by_the_block_graph(&toolchain, &with_a_lead_in, &sources, 512).len(),
        )
    });
    for ((seed, lead_in_frames), (total, difference, whole_samples, cut_samples)) in cases.iter().zip(&results) {
        let name = format!("seed {seed}, a lead-in of {lead_in_frames} frames of {total}");
        assert_eq!(
            difference.moments_of_the_oracle, difference.moments_of_the_block_graph,
            "the two sounds are as long as each other: {name}"
        );
        assert!(
            difference.largest <= ONE_ROUNDING_OF_A_SAMPLE,
            "{name}: {}",
            difference.largest
        );
        assert_eq!(
            *whole_samples - *cut_samples,
            2 * 1600 * *lead_in_frames as usize,
            "the sound is shorter by the lead-in: {name}"
        );
    }
}

#[test]
fn the_volume_and_the_mutes_of_a_track_are_done_after_its_clips_are_joined_by_the_block_graph_and_by_the_filter_graph_alike()
 {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kinds = Kinds {
        sped_up: false,
        changing_volume: false,
        wrong_lengths: false,
    };
    let seeds: Vec<u64> = (0..6).collect();
    let results = in_parallel(&seeds, |&seed| {
        let mut with_a_volume = a_specification(
            seed * 5 + 1,
            FrameRate::new(30, 1).unwrap(),
            kinds,
            SpedUpClipSound::MaintainPitch,
        );
        for strip in &mut with_a_volume.strips {
            strip.loudness = LoudnessToExport::Constant(0.3 + seed as f64 / 10.0);
        }
        let mut with_mutes = with_a_volume.clone();
        with_mutes.strips[0].muted = vec![(2, 9), (20, 21)];
        (
            difference_for(&toolchain, &with_a_volume, &sources),
            by_the_filter_graph(&toolchain, &with_mutes, &sources),
            by_the_block_graph(&toolchain, &with_mutes, &sources, 512),
            by_the_block_graph(&toolchain, &with_a_volume, &sources, 512),
        )
    });
    for (seed, (difference, mutes_by_the_filter_graph, mutes_by_the_block_graph, without_mutes)) in
        seeds.iter().zip(&results)
    {
        assert_eq!(
            difference.moments_of_the_oracle, difference.moments_of_the_block_graph,
            "seed {seed}"
        );
        assert!(
            difference.largest <= ONE_ROUNDING_OF_A_SAMPLE,
            "a volume for the whole track, seed {seed}: {}",
            difference.largest
        );
        // The mutes: measured, not assumed. The filter graph turns a filter on and off a chunk of sound at a time, so the edges of a mute are not at the very sample.
        let edges_in_moments = [2 * 1600_i64, 9 * 1600, 20 * 1600, 21 * 1600];
        let mut largest_away_from_the_edges = 0.0_f32;
        let mut largest_at_the_edges = 0.0_f32;
        for (moment, (by_the_filter_graph, by_the_block_graph)) in mutes_by_the_filter_graph
            .chunks(2)
            .zip(mutes_by_the_block_graph.chunks(2))
            .enumerate()
        {
            let difference = (by_the_filter_graph[0] - by_the_block_graph[0]).abs();
            if edges_in_moments.iter().any(|edge| (moment as i64 - edge).abs() <= 4096) {
                largest_at_the_edges = largest_at_the_edges.max(difference);
            } else {
                largest_away_from_the_edges = largest_away_from_the_edges.max(difference);
            }
        }
        assert!(
            largest_away_from_the_edges <= ONE_ROUNDING_OF_A_SAMPLE,
            "mutes, seed {seed}: the two sounds are the same but for a chunk of sound at each edge of a mute: {largest_away_from_the_edges}"
        );
        assert!(
            largest_at_the_edges > 0.01,
            "(the control: the edges of the mutes are where the filter graph is a chunk out, or there is nothing to compare) seed {seed}: {largest_at_the_edges}"
        );
        assert_eq!(
            mutes_by_the_block_graph.len(),
            without_mutes.len(),
            "a mute does not change how long the sound is: seed {seed}"
        );
    }
}
