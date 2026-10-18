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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
    clip_feed_filters,
};
use fastforwardrs::media::{AudioFormat, AudioStream, AudioStreamEvent, AudioStreamRequest, FfmpegToolchain};
use fastforwardrs::project::{FrameRate, SpedUpClipSound, SpeedFactor};
use fastforwardrs::sound_engine::{
    ClipFeed, ClipFeeds, ClipRequest, NoInsertions, SAMPLE_RATE, SoundGraph, SoundInMemory,
};
use support::{RandomNumbers, scratch_directory};

const TIME_ALLOWED: Duration = Duration::from_secs(120);

const STEREO_AT_48000: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channel_count: 2,
};

/// A stereo WAV file of 32-bit floating point samples, 48000 a second, that has `value(channel, sample)` for the samples.
fn write_wav(path: &Path, seconds: f64, value: impl Fn(usize, usize) -> f32) {
    let samples = (seconds * f64::from(SAMPLE_RATE)) as usize;
    let data_bytes = samples * 2 * 4;
    let mut bytes = Vec::with_capacity(44 + data_bytes);
    bytes.extend(b"RIFF");
    bytes.extend(((36 + data_bytes) as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(3_u16.to_le_bytes()); // IEEE floating point
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(SAMPLE_RATE.to_le_bytes());
    bytes.extend((SAMPLE_RATE * 8).to_le_bytes());
    bytes.extend(8_u16.to_le_bytes());
    bytes.extend(32_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((data_bytes as u32).to_le_bytes());
    for sample in 0..samples {
        for channel in 0..2 {
            bytes.extend(value(channel, sample).to_le_bytes());
        }
    }
    fs::write(path, bytes).expect("the sound file is written");
}

/// A file of noise: a number in -0.5..0.5 at every sample, different in the two channels, the same for the same `seed`.
fn noise_file(folder: &Path, name: &str, seconds: f64, seed: u64) -> PathBuf {
    let path = folder.join(name);
    let mut random = RandomNumbers::with_seed(seed);
    let mut numbers: Vec<f32> = Vec::new();
    for _ in 0..(seconds * f64::from(SAMPLE_RATE)) as usize * 2 {
        numbers.push(random.next_below(1_000_000) as f32 / 1_000_000.0 - 0.5);
    }
    write_wav(&path, seconds, |channel, sample| numbers[sample * 2 + channel]);
    path
}

/// A file that is the same number all the way through.
fn steady_file(folder: &Path, name: &str, seconds: f64, value: f32) -> PathBuf {
    let path = folder.join(name);
    write_wav(&path, seconds, |_, _| value);
    path
}

fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
    left.iter()
        .zip(right)
        .flat_map(|(left, right)| [*left, *right])
        .collect()
}

/// The sound of `specification` as the filter graph of ffmpeg makes it (the oracle), interleaved.
fn by_the_filter_graph(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
) -> Vec<f32> {
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification.clone(),
        format: STEREO_AT_48000,
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

/// Feeds made by an ffmpeg that runs for each clip, with the filters that the filter graph has for it before the volume.
struct FeedsFromFfmpeg {
    ffmpeg: PathBuf,
    sources: Vec<PathBuf>,
    frame_rate: FrameRate,
    sped_up_clip_sound: SpedUpClipSound,
}

impl ClipFeeds for FeedsFromFfmpeg {
    fn open(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        let filters = clip_feed_filters(self.frame_rate, request.clip, self.sped_up_clip_sound);
        let output = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(&self.sources[request.clip.source_input])
            .args(["-af", &filters, "-f", "f32le", "-ar", "48000", "-ac", "2", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("ffmpeg runs");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let samples: Vec<f32> = output
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        Box::new(SoundInMemory::from_interleaved(&samples))
    }
}

/// The sound of `specification` as the block graph makes it, interleaved.
fn by_the_block_graph(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
    block: usize,
) -> Vec<f32> {
    let feeds = FeedsFromFfmpeg {
        ffmpeg: toolchain.ffmpeg_path().to_owned(),
        sources: sources.to_vec(),
        frame_rate: specification.frame_rate,
        sped_up_clip_sound: specification.sped_up_clip_sound,
    };
    let mut graph = SoundGraph::new(specification.clone(), Box::new(feeds), Box::new(NoInsertions));
    let (left, right) = graph.render_to_the_end(block);
    interleave(&left, &right)
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

const FILE_SECONDS: [f64; 3] = [6.0, 3.0, 9.0];

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
                    })
                })
                .collect();
            longest_strip = longest_strip.max(frames);
            SoundStripToExport { items }
        })
        .collect();
    SoundGraphSpecification {
        frame_rate,
        total_frame_count: longest_strip + random.next_below(4),
        strips,
        sped_up_clip_sound,
    }
}

fn the_sources(folder: &Path) -> Vec<PathBuf> {
    FILE_SECONDS
        .iter()
        .enumerate()
        .map(|(number, seconds)| noise_file(folder, &format!("noise_{number}.wav"), *seconds, 1000 + number as u64))
        .collect()
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
            })],
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
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
