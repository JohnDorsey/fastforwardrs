// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stopping the sound of the preview (phase 7): when the stream is dropped, **the ffmpeg of every clip is stopped and waited for, in the background, and
//! `wait_for_sound_that_was_stopped` says when that is done**. On Windows a program that is still running has its file open, and the editor writes the project file again when
//! its window is closed (found by the CI: "Access is denied", when the first version of the sound engine only told its thread to stop). The editor waits for it before it does.
//! (The first version of the mend waited in the drop itself, which the real-time tests of the sound on Ubuntu found to put a gap of 60 ms in the sound each time a stream was
//! replaced by another: so the drop does not wait.)

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use fastforwardrs::media::{
    AudioFormat, AudioStreamEvent, AudioStreamRequest, EngineAudioStream, wait_for_sound_that_was_stopped,
};
use fastforwardrs::project::{FrameRate, SpedUpClipSound, SpeedFactor};
use support::scratch_directory;
use support::sound_files::the_sources;

/// A clip of the file of nine seconds (the third) that is heard for `frames` frames.
fn clip_of_the_long_file(frames: u64) -> SoundItemToExport {
    SoundItemToExport::Clip(SoundClipToExport {
        source_input: 2,
        first_source_frame: 0,
        end_source_frame: frames,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: frames,
        loudness: LoudnessToExport::Constant(0.5),
        first_timeline_frame: 0,
        frames_of_the_clip_before: 0,
        effects: Vec::new(),
    })
}

/// Starts the sound of two clips on two tracks (so both are heard at once), waits until the sound is coming and a clip's program is running, and drops the stream
/// **without having read the sound**, so that the thread of the graph is waiting to give it away. Gives how many programs run just after the drop, and how many ran before it
/// (`None` where there is no ffmpeg).
fn programs_running_just_after_dropping_the_stream(format: AudioFormat) -> Option<(usize, usize)> {
    let toolchain = support::toolchain()?;
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let specification = SoundGraphSpecification {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        total_frame_count: 540,
        strips: vec![
            SoundStripToExport {
                items: vec![clip_of_the_long_file(270)],
                buses: Vec::new(),
            },
            SoundStripToExport {
                items: vec![clip_of_the_long_file(270)],
                buses: Vec::new(),
            },
        ],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
    };
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification,
        format,
    };
    let stream = EngineAudioStream::start(&toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let running: Arc<AtomicUsize> = stream.running_programs();
    let began = Instant::now();
    // (Only looks: the sound that has come is not taken, so that the buffers fill and the graph's thread waits to hand sound over.)
    while running.load(Ordering::SeqCst) < 2 {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "the programs of the clips were not started"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(300));
    let before = running.load(Ordering::SeqCst);
    drop(stream);
    assert!(
        wait_for_sound_that_was_stopped(Duration::from_secs(20)),
        "the sound that was stopped was let go of within 20 seconds"
    );
    Some((running.load(Ordering::SeqCst), before))
}

#[test]
fn the_programs_of_the_clips_are_gone_when_the_stream_that_the_speakers_take_directly_is_dropped() {
    let Some((after, before)) = programs_running_just_after_dropping_the_stream(AudioFormat {
        sample_rate: 48_000,
        channel_count: 2,
    }) else {
        return;
    };
    assert_eq!(before, 2, "both clips had their programs running");
    assert_eq!(after, 0);
}

#[test]
fn the_programs_of_the_clips_are_gone_when_the_stream_that_is_changed_to_another_kind_of_sound_is_dropped() {
    let Some((after, before)) = programs_running_just_after_dropping_the_stream(AudioFormat {
        sample_rate: 44_100,
        channel_count: 2,
    }) else {
        return;
    };
    assert_eq!(before, 2, "both clips had their programs running");
    assert_eq!(after, 0);
}

#[test]
fn a_stream_that_was_read_to_its_end_is_dropped_with_no_program_left() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: SoundGraphSpecification {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: 15,
            strips: vec![SoundStripToExport {
                items: vec![clip_of_the_long_file(15)],
                buses: Vec::new(),
            }],
            sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
            lead_in_frames: 0,
        },
        format: AudioFormat {
            sample_rate: 48_000,
            channel_count: 2,
        },
    };
    let stream = EngineAudioStream::start(&toolchain, &request, Arc::new(|| {})).expect("the stream starts");
    let running = stream.running_programs();
    let began = Instant::now();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None);
                break;
            }
            Some(AudioStreamEvent::Samples(_)) => {}
            None => {
                assert!(began.elapsed() < Duration::from_secs(20), "the sound did not end");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
    drop(stream);
    assert!(
        wait_for_sound_that_was_stopped(Duration::from_secs(20)),
        "the sound that was stopped was let go of within 20 seconds"
    );
    assert_eq!(running.load(Ordering::SeqCst), 0);
}
