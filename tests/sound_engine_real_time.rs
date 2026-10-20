// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound engine against the clock (phase 7): sound that is made faster than it is heard, and does not fall behind the speakers. Like the other tests that are measured
//! against time (`application_sound`, `application_playback`), a failure of these is a warning in the CI and not a red build: a runner with two slow processors cannot be asked.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use fastforwardrs::media::{AudioFormat, AudioStreamEvent, AudioStreamRequest, EngineAudioStream};
use fastforwardrs::project::{FrameRate, SpedUpClipSound, SpeedFactor};
use support::sound_files::the_sources;
use support::{RandomNumbers, scratch_directory};

const STEREO_AT_48000: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channel_count: 2,
};

#[test]
fn a_project_of_two_hundred_short_clips_is_made_faster_than_it_is_heard_and_never_falls_behind_the_speakers() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    // Two hundred clips of 6 frames (a fifth of a second) one after another on one track, cut from different places of the files: 40 seconds.
    let mut random = RandomNumbers::with_seed(77);
    let clips: Vec<SoundItemToExport> = (0..200)
        .map(|_| {
            let first = random.next_below(150);
            SoundItemToExport::Clip(SoundClipToExport {
                source_input: random.next_below(3) as usize,
                first_source_frame: first,
                end_source_frame: first + 6,
                speed_factor: SpeedFactor::new(1).unwrap(),
                timeline_frame_count: 6,
                loudness: LoudnessToExport::Constant(0.5),
                first_timeline_frame: 0,
                frames_of_the_clip_before: 0,
                effects: Vec::new(),
            })
        })
        .collect();
    let specification = SoundGraphSpecification {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        total_frame_count: 1200,
        strips: vec![SoundStripToExport { items: clips }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
    };
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification,
        format: STEREO_AT_48000,
    };
    let began = Instant::now();
    let stream = EngineAudioStream::start(&toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    // The speakers play what has come at the pace of time, from when the first sound came, and are given sound ahead: so at any time the sound that has come
    // has to be at least what has been played, or there was a gap. The sound is taken as it comes (the speakers' queue is not modelled).
    let mut first_sound_at = None;
    let mut moments_received = 0_usize;
    let mut least_ahead_in_seconds = f64::MAX;
    let mut finished = false;
    while moments_received < 30 * 48_000 && !finished {
        while let Some(event) = stream.try_next_event() {
            match event {
                AudioStreamEvent::Samples(samples) => {
                    first_sound_at.get_or_insert_with(|| began.elapsed());
                    moments_received += samples.len() / 2;
                }
                AudioStreamEvent::Finished { error_output } => {
                    assert_eq!(error_output, None);
                    finished = true;
                }
            }
        }
        if let Some(first) = first_sound_at {
            let played = (began.elapsed().saturating_sub(first)).as_secs_f64();
            least_ahead_in_seconds = least_ahead_in_seconds.min(moments_received as f64 / 48_000.0 - played);
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "not enough sound in a minute"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let first = first_sound_at.expect("there was sound");
    eprintln!(
        "200 clips: the first sound after {first:?}; {} seconds of sound received in {:?}; never less than {least_ahead_in_seconds:.2} seconds ahead of the speakers",
        moments_received as f64 / 48_000.0,
        began.elapsed()
    );
    assert!(first < Duration::from_secs(3), "the first sound took {first:?}");
    assert!(
        least_ahead_in_seconds > -0.1,
        "the sound fell {least_ahead_in_seconds:.2} seconds behind the speakers (a gap is any time behind at all; 0.1 seconds is the slack of the polling)"
    );
}
