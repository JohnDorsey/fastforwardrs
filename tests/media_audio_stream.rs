//! The sound of the live preview with the real ffmpeg: what the stream delivers has to be the sound
//! the export will have, from wherever the playhead is.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::composition::plan_sound;
use fastforwardrs::media::export::{
    ExportResult, VideoExport, plan_export, sound_graph_specification, sources_of_sound,
};
use fastforwardrs::media::{AudioFormat, AudioStream, AudioStreamEvent, AudioStreamRequest, FfmpegToolchain};
use fastforwardrs::project::{Project, Settings, SpedUpClipSound, TimelineFrameIndex};
use support::{
    counter_video, decoded_sound, edited, loudness, project_for, project_with_a_video_and_a_tone_file,
    scratch_directory, tone_strength, transparent_text_picture,
};

const TIME_ALLOWED: Duration = Duration::from_secs(60);

const STEREO_AT_48000: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channel_count: 2,
};

/// Asks for the sound of `project` from timeline frame `from_frame` on.
fn request_for(project: &Project, from_frame: u64, sped_up_clip_sound: SpedUpClipSound) -> AudioStreamRequest {
    let plan = plan_sound(project, TimelineFrameIndex::new(from_frame), |_| true);
    let sources: Vec<_> = sources_of_sound(&plan).into_iter().collect();
    AudioStreamRequest {
        source_files: sources
            .iter()
            .map(|identifier| project.source(*identifier).unwrap().link.absolute_path.clone())
            .collect(),
        sound: sound_graph_specification(&plan, &sources, sped_up_clip_sound),
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

    let (samples, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

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
    let mut request = request_for(&project, 0, SpedUpClipSound::MaintainPitch);
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
    assert_eq!(project.total_frame_count(), 300 + 30 + 180);

    let (samples, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

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

    let (samples, error_output) = everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::Mute));

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
    let (whole, _) = everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

    let (from_the_sped_up_clip, error_output) =
        everything_from(&toolchain, &request_for(&project, 300, SpedUpClipSound::MaintainPitch));

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
    let (whole, _) = everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

    // Frame 450 is 15 seconds in.
    let (from_the_middle, error_output) =
        everything_from(&toolchain, &request_for(&project, 450, SpedUpClipSound::MaintainPitch));

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
    let (samples, error_output) =
        everything_from(&toolchain, &request_for(&project, 315, SpedUpClipSound::MaintainPitch));

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

/// The sound of the video that exporting `project` makes, as stereo samples at 48000 a second.
fn sound_of_the_export_of(toolchain: &FfmpegToolchain, project: &Project) -> Vec<f32> {
    let scratch = scratch_directory();
    let exported_path = scratch.path().join("exported.mp4");
    let plan = plan_export(project, Settings::DEFAULT, &transparent_text_picture).unwrap();
    let source_paths: Vec<_> = plan
        .source_files
        .iter()
        .map(|identifier| project.source(*identifier).unwrap().link.absolute_path.clone())
        .collect();
    let export = VideoExport::start(toolchain, &plan, &source_paths, &exported_path, Arc::new(|| {})).unwrap();
    let started = Instant::now();
    loop {
        if let Some(fastforwardrs::media::export::ExportEvent::Finished(result)) = export.try_next_event() {
            assert_eq!(result, ExportResult::Completed);
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(120));
        std::thread::sleep(Duration::from_millis(5));
    }
    decoded_sound(toolchain, &exported_path, 2, 48_000)
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
    let exported = sound_of_the_export_of(&toolchain, &project);

    let (previewed, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

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
    let mut request = request_for(&project, 0, SpedUpClipSound::MaintainPitch);
    request.source_files = vec![video.path.with_file_name("there_is_no_such_video.mp4")];

    let (samples, error_output) = everything_from(&toolchain, &request);

    assert!(samples.is_empty());
    assert!(error_output.is_some_and(|text| !text.is_empty()), "ffmpeg says why");
}

#[test]
fn stopping_a_stream_early_is_quick_and_clean() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let request = request_for(&project, 0, SpedUpClipSound::MaintainPitch);

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

/// How loud ffmpeg's `sine` source is, which is the sound of the test videos and tone files.
const TONE_AMPLITUDE: f32 = 0.125;

/// Stereo samples as one channel, the way ffmpeg mixes two channels into one: a mono sound that was
/// given to both channels (each at 0.707 of its size) comes out as it was.
fn mono_of(stereo: &[f32]) -> Vec<f32> {
    stereo
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0] + pair[1]) * std::f32::consts::FRAC_1_SQRT_2)
        .collect()
}

/// How strong the tone of `hertz` is between `from_seconds` and `to_seconds` of a mono sound at 48000 a second.
fn tone_between(mono: &[f32], hertz: f64, from_seconds: f64, to_seconds: f64) -> f32 {
    let moment = |seconds: f64| (seconds * 48_000.0) as usize;
    tone_strength(&mono[moment(from_seconds)..moment(to_seconds)], hertz, 48_000) as f32
}

fn assert_full_strength(strength: f32, context: &str) {
    assert!(
        (strength - TONE_AMPLITUDE).abs() < 0.1 * TONE_AMPLITUDE,
        "{context}: {strength}"
    );
}

#[test]
fn the_preview_sound_of_a_video_and_a_sound_file_on_two_tracks_is_both_mixed_from_wherever_it_starts() {
    let toolchain = toolchain_or_skip!();
    // The video's 440 hertz tone for 10 seconds, and in front of it a 330 hertz file from 5 s to 9 s.
    let project = project_with_a_video_and_a_tone_file(&toolchain);

    let (from_the_start, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));
    assert_eq!(error_output, None);
    let sound = mono_of(&from_the_start);
    assert!(
        sound.len().abs_diff(480_000) < 2 * 1024,
        "ten seconds: {} moments",
        sound.len()
    );
    assert_full_strength(tone_between(&sound, 440.0, 1.0, 4.0), "the video alone");
    assert!(tone_between(&sound, 330.0, 1.0, 4.0) < 0.02, "the file has not begun");
    assert_full_strength(tone_between(&sound, 440.0, 5.5, 8.5), "the video, with the file");
    assert_full_strength(tone_between(&sound, 330.0, 5.5, 8.5), "the file, with the video");
    assert!(tone_between(&sound, 330.0, 9.3, 9.9) < 0.02, "the file has ended");
    assert_full_strength(tone_between(&sound, 440.0, 9.3, 9.9), "the video after the file");

    // From seven seconds in, in the middle of the file: both at once, from the very start, then the video.
    let (from_the_middle, error_output) =
        everything_from(&toolchain, &request_for(&project, 210, SpedUpClipSound::MaintainPitch));
    assert_eq!(error_output, None);
    let rest = mono_of(&from_the_middle);
    assert!(
        rest.len().abs_diff(3 * 48_000) < 2 * 1024,
        "three seconds are left: {} moments",
        rest.len()
    );
    assert_full_strength(
        tone_between(&rest, 330.0, 0.1, 1.9),
        "the file is heard from the first moment",
    );
    assert_full_strength(tone_between(&rest, 440.0, 0.1, 1.9), "and the video");
    assert!(
        tone_between(&rest, 330.0, 2.3, 2.9) < 0.02,
        "the file ends two seconds on"
    );
    assert_full_strength(tone_between(&rest, 440.0, 2.3, 2.9), "the video goes on");
}

#[test]
fn the_preview_sound_of_two_tracks_is_the_sound_the_export_has_second_by_second() {
    let toolchain = toolchain_or_skip!();
    let project = project_with_a_video_and_a_tone_file(&toolchain);
    let exported = sound_of_the_export_of(&toolchain, &project);
    let (previewed, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));
    assert_eq!(error_output, None);
    assert!((previewed.len() / 2).abs_diff(exported.len() / 2) < 2 * 1024);
    for second in 0..9 {
        let in_the_preview = loudness_of_stretch(&previewed, second as f64, 1.0);
        let in_the_export = loudness_of_stretch(&exported, second as f64, 1.0);
        assert!(
            (in_the_preview - in_the_export).abs() < 0.03,
            "second {second}: {in_the_preview} against {in_the_export}"
        );
    }
}
