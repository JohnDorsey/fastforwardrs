//! The sound of the live preview with the real ffmpeg: what the stream delivers has to be the sound
//! the export will have, from wherever the playhead is.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::export::{ExportResult, VideoExport, clips_to_export, plan_export};
use fastforwardrs::media::{AudioFormat, AudioStream, AudioStreamEvent, AudioStreamRequest, FfmpegToolchain};
use fastforwardrs::project::{Project, Settings, TimelineFrameIndex};
use support::{TestVideo, counter_video, decoded_sound, edited, loudness, project_for, scratch_directory};

const TIME_ALLOWED: Duration = Duration::from_secs(60);

const STEREO_AT_48000: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channel_count: 2,
};

/// Asks for the sound of `project` from timeline frame `from_frame` on.
fn request_for(project: &Project, video: &TestVideo, from_frame: u64, mute_sped_up_clips: bool) -> AudioStreamRequest {
    let source_video = &project.source_videos()[0];
    AudioStreamRequest {
        video_path: video.path.clone(),
        frame_rate: source_video.properties.frame_rate,
        clips: clips_to_export(
            &project.video_track().clips_from(TimelineFrameIndex::new(from_frame)),
            source_video.properties.total_frame_count,
        ),
        mute_sped_up_clips,
        format: STEREO_AT_48000,
    }
}

/// Everything a stream delivers, and what it said when it ended.
fn everything_from(toolchain: &FfmpegToolchain, request: &AudioStreamRequest) -> (Vec<f32>, Option<String>) {
    let stream = AudioStream::start(toolchain, request, Arc::new(|| {})).expect("the stream starts");
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => samples.extend(more),
            Some(AudioStreamEvent::Finished { error_output }) => return (samples, error_output),
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "the stream did not finish in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

/// How loud the sound is in the stretch from `from_seconds` for `length_in_seconds`, away from the
/// ends of the stretch (where one clip joins the next).
fn loudness_of_stretch(samples: &[f32], from_seconds: f64, length_in_seconds: f64) -> f32 {
    let moment = |seconds: f64| (seconds * 48_000.0) as usize * 2;
    loudness(&samples[moment(from_seconds)..moment(from_seconds + length_in_seconds)])
}

#[test]
fn the_sound_of_an_unedited_project_is_all_of_the_videos_sound_at_the_format_asked_for() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);

    let (samples, error_output) = everything_from(&toolchain, &request_for(&project, &video, 0, false));

    assert_eq!(error_output, None);
    assert_eq!(
        samples.len(),
        20 * 48_000 * 2,
        "20 seconds of stereo at 48000 moments a second"
    );
    assert!(
        loudness(&samples) > 0.05,
        "it is the sound, not silence: {}",
        loudness(&samples)
    );
}

#[test]
fn mono_at_another_rate_is_delivered_as_asked() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let mut request = request_for(&project, &video, 0, false);
    request.format = AudioFormat {
        sample_rate: 44_100,
        channel_count: 1,
    };

    let (samples, error_output) = everything_from(&toolchain, &request);

    assert_eq!(error_output, None);
    assert_eq!(samples.len(), 20 * 44_100);
}

#[test]
fn an_edited_timeline_sounds_as_long_as_it_looks_and_every_clip_has_sound() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Clips: 0..300 at 1x (10 s), 300..420 at 4x (1 s), 420..600 at 1x (6 s).
    let project = edited(project_for(&toolchain, &video), &[300, 420], &[1, 4, 1]);
    assert_eq!(project.video_track().total_frame_count(), 300 + 30 + 180);

    let (samples, error_output) = everything_from(&toolchain, &request_for(&project, &video, 0, false));

    assert_eq!(error_output, None);
    let moments = samples.len() / 2;
    let expected_moments = (300 + 30 + 180) * 48_000 / 30;
    assert!(
        moments.abs_diff(expected_moments) < 48_000 / 50,
        "{moments} moments, expected {expected_moments}"
    );
    assert!(loudness_of_stretch(&samples, 1.0, 8.0) > 0.05, "the first clip");
    assert!(loudness_of_stretch(&samples, 10.1, 0.8) > 0.05, "the sped up clip");
    assert!(loudness_of_stretch(&samples, 11.2, 5.0) > 0.05, "the last clip");
}

#[test]
fn with_sped_up_clips_muted_the_sped_up_clip_is_silent_and_everything_else_is_not() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = edited(project_for(&toolchain, &video), &[300, 420], &[1, 4, 1]);

    let (samples, error_output) = everything_from(&toolchain, &request_for(&project, &video, 0, true));

    assert_eq!(error_output, None);
    assert!(
        loudness_of_stretch(&samples, 1.0, 8.0) > 0.05,
        "the first clip keeps its sound"
    );
    assert!(
        loudness_of_stretch(&samples, 10.1, 0.8) < 0.001,
        "the sped up clip is silent"
    );
    assert!(
        loudness_of_stretch(&samples, 11.2, 5.0) > 0.05,
        "the last clip keeps its sound"
    );
    assert_eq!(samples.len() / 2, (300 + 30 + 180) * 48_000 / 30, "and nothing moved");
}

#[test]
fn the_sound_from_the_start_of_a_clip_is_exactly_the_end_of_the_sound_from_the_start() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Starting where a clip starts, nothing is cut mid-way, so the two can be compared sample by sample.
    let project = edited(project_for(&toolchain, &video), &[300, 420], &[1, 4, 1]);
    let (whole, _) = everything_from(&toolchain, &request_for(&project, &video, 0, false));

    let (from_the_sped_up_clip, error_output) = everything_from(&toolchain, &request_for(&project, &video, 300, false));

    assert_eq!(error_output, None);
    let start_in_whole = 300 * 48_000 / 30 * 2;
    assert_eq!(
        from_the_sped_up_clip.len(),
        whole.len() - start_in_whole,
        "the same length"
    );
    let largest_difference = from_the_sped_up_clip
        .iter()
        .zip(&whole[start_in_whole..])
        .map(|(from_there, from_the_start)| (from_there - from_the_start).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        largest_difference < 1e-4,
        "the largest difference is {largest_difference}"
    );
}

#[test]
fn the_sound_from_the_middle_of_a_normal_speed_clip_is_the_end_of_the_sound_from_the_start() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let (whole, _) = everything_from(&toolchain, &request_for(&project, &video, 0, false));

    // Frame 450 is 15 seconds in.
    let (from_the_middle, error_output) = everything_from(&toolchain, &request_for(&project, &video, 450, false));

    assert_eq!(error_output, None);
    let start_in_whole = 15 * 48_000 * 2;
    assert_eq!(from_the_middle.len(), whole.len() - start_in_whole);
    let largest_difference = from_the_middle
        .iter()
        .zip(&whole[start_in_whole..])
        .map(|(from_there, from_the_start)| (from_there - from_the_start).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        largest_difference < 1e-4,
        "the largest difference is {largest_difference}"
    );
}

#[test]
fn from_the_middle_of_a_sped_up_clip_the_rest_of_the_clip_and_what_follows_is_there_in_full_length() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = edited(project_for(&toolchain, &video), &[300, 420], &[1, 4, 1]);

    // Timeline frame 315 is 15 frames into the sped up clip, which has 30: half a second remains.
    let (samples, error_output) = everything_from(&toolchain, &request_for(&project, &video, 315, false));

    assert_eq!(error_output, None);
    let expected_moments = (15 + 180) * 48_000 / 30;
    assert!(
        (samples.len() / 2).abs_diff(expected_moments) < 48_000 / 50,
        "{} moments, expected {expected_moments}",
        samples.len() / 2
    );
    assert!(
        loudness_of_stretch(&samples, 0.05, 0.4) > 0.05,
        "the rest of the sped up clip has sound"
    );
    assert!(
        loudness_of_stretch(&samples, 1.0, 5.0) > 0.05,
        "and so does what follows"
    );
}

#[test]
fn the_preview_sound_is_the_sound_the_export_has_clip_for_clip() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = edited(
        project_for(&toolchain, &video),
        &[200, 350, 380, 500],
        &[1, 3, 1, 25, 2],
    );
    let scratch = scratch_directory();
    let exported_path = scratch.path().join("exported.mp4");
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let export = VideoExport::start(&toolchain, &plan, &video.path, &exported_path, Arc::new(|| {})).unwrap();
    let started = Instant::now();
    loop {
        if let Some(fastforwardrs::media::export::ExportEvent::Finished(result)) = export.try_next_event() {
            assert_eq!(result, ExportResult::Completed);
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(120));
        std::thread::sleep(Duration::from_millis(5));
    }
    let exported = decoded_sound(&toolchain, &exported_path, 2, 48_000);

    let (previewed, error_output) = everything_from(&toolchain, &request_for(&project, &video, 0, false));

    assert_eq!(error_output, None);
    // The export is compressed (AAC) and the preview is not, so the two can differ slightly in
    // detail, and the export starts a hair later; but the length and how loud each second is must agree.
    assert!(
        (previewed.len() / 2).abs_diff(exported.len() / 2) < 2 * 1024,
        "preview {} moments, export {}",
        previewed.len() / 2,
        exported.len() / 2
    );
    let seconds = (previewed.len().min(exported.len()) / 2 / 48_000).saturating_sub(1);
    assert!(seconds >= 10, "enough to compare: {seconds} s");
    for second in 0..seconds {
        let in_the_preview = loudness_of_stretch(&previewed, second as f64, 1.0);
        let in_the_export = loudness_of_stretch(&exported, second as f64, 1.0);
        assert!(
            (in_the_preview - in_the_export).abs() < 0.03,
            "second {second}: {in_the_preview} against {in_the_export}"
        );
    }
}

#[test]
fn a_video_that_cannot_be_read_ends_the_stream_with_ffmpegs_complaint() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let mut request = request_for(&project, &video, 0, false);
    request.video_path = video.path.with_file_name("there_is_no_such_video.mp4");

    let (samples, error_output) = everything_from(&toolchain, &request);

    assert!(samples.is_empty());
    assert!(error_output.is_some_and(|text| !text.is_empty()), "ffmpeg says why");
}

#[test]
fn stopping_a_stream_early_is_quick_and_clean() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let request = request_for(&project, &video, 0, false);

    let started = Instant::now();
    {
        let stream = AudioStream::start(&toolchain, &request, Arc::new(|| {})).unwrap();
        // Wait for the first of the sound, then walk away with nearly all of it unread.
        while stream.try_next_event().is_none() {
            assert!(started.elapsed() < TIME_ALLOWED);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "dropping the stream did not wait for the rest"
    );
}
