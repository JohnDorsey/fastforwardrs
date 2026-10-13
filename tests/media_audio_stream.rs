//! The sound of the live preview with the real ffmpeg: what the stream delivers has to be the sound
//! the export will have, from wherever the playhead is.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::composition::plan_sound;
use fastforwardrs::media::export::{
    ExportResult, VideoExport, plan_export, sound_graph_specification, sources_of_sound,
};
use fastforwardrs::media::{
    AudioFormat, AudioStream, AudioStreamEvent, AudioStreamRequest, FfmpegToolchain, MediaLocation,
};
use fastforwardrs::project::{
    AnimationTarget, Project, PropertyId, PropertyValue, Settings, SpedUpClipSound, TimelineFrameIndex,
};
use support::{
    TestVideoSpecification, clip_number, counter_video, decoded_sound, edited, loudness, pitch_in_hertz, project_for,
    project_with_a_video_and_a_tone_file, scratch_directory, test_video, tone_strength, transparent_text_picture,
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
            .map(|identifier| project.source(*identifier).unwrap().link.absolute_path.clone().into())
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
    let source_paths: Vec<MediaLocation> = plan
        .source_files
        .iter()
        .map(|identifier| project.source(*identifier).unwrap().link.absolute_path.clone().into())
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
    request.source_files = vec![video.path.with_file_name("there_is_no_such_video.mp4").into()];

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

// --- keyframes ---

/// Makes `property` of the first clip of `project` have `value` at timeline frame `frame`: a keyframe.
fn make_a_keyframe(project: &mut Project, property: PropertyId, frame: u64, value: PropertyValue) {
    let target = AnimationTarget::Clip(clip_number(project, 0));
    support::make_a_keyframe(project, target, property, frame, value);
}

/// The left channel of stereo samples.
fn left_channel(samples: &[f32]) -> Vec<f32> {
    samples.iter().step_by(2).copied().collect()
}

/// How many times louder than the sound at `from_seconds` the sound at `to_seconds` is (both stretches 0.2 seconds long).
fn how_much_louder_it_is_at(samples: &[f32], from_seconds: f64, to_seconds: f64) -> f64 {
    f64::from(loudness_of_stretch(samples, to_seconds, 0.2))
        / f64::from(loudness_of_stretch(samples, from_seconds, 0.2))
}

#[test]
fn a_clip_whose_gain_has_keyframes_is_as_loud_as_they_say_at_each_moment() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    // 0 decibels at the start, getting quieter by 4 decibels every second until -20 decibels at 5 seconds.
    make_a_keyframe(&mut project, PropertyId::Gain, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, PropertyId::Gain, 150, PropertyValue::Number(-20.0));

    let (samples, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

    assert_eq!(error_output, None);
    assert_eq!(samples.len() / 2, 20 * 48_000, "as long as before");
    // The stretches are 0.2 seconds long, and start at 1 second (gain -4.4 decibels at their middle), at 4 seconds
    // (-16.4) and at 7 seconds (-20, as it stays).
    let at_4_seconds = how_much_louder_it_is_at(&samples, 1.0, 4.0);
    let expected = 10f64.powf(-12.0 / 20.0);
    assert!(
        (at_4_seconds / expected - 1.0).abs() < 0.06,
        "{at_4_seconds} times as loud, not {expected}"
    );
    let at_7_seconds = how_much_louder_it_is_at(&samples, 1.0, 7.0);
    let expected = 10f64.powf(-15.6 / 20.0);
    assert!(
        (at_7_seconds / expected - 1.0).abs() < 0.06,
        "{at_7_seconds} times as loud, not {expected}"
    );
    let at_10_seconds = how_much_louder_it_is_at(&samples, 7.0, 10.0);
    assert!(
        (at_10_seconds - 1.0).abs() < 0.03,
        "{at_10_seconds}: after the last keyframe it stays as loud"
    );
}

#[test]
fn the_sound_that_starts_in_the_middle_of_a_change_of_gain_starts_as_loud_as_it_is_there() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    make_a_keyframe(&mut project, PropertyId::Gain, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, PropertyId::Gain, 150, PropertyValue::Number(-20.0));

    // From frame 75 (2.5 seconds), where it is -10 decibels, to frame 150, 2.5 seconds later, where it is -20.
    let (samples, error_output) =
        everything_from(&toolchain, &request_for(&project, 75, SpedUpClipSound::MaintainPitch));

    assert_eq!(error_output, None);
    // The first stretch is at -10.4 decibels (at 0.1 seconds), and the one at 3 seconds is -20.
    let ratio = how_much_louder_it_is_at(&samples, 0.0, 3.0);
    let expected = 10f64.powf(-9.6 / 20.0);
    assert!(
        (ratio / expected - 1.0).abs() < 0.06,
        "{ratio} times as loud, not {expected}"
    );
}

#[test]
fn a_clip_muted_by_keyframes_is_silent_for_that_stretch_and_the_sound_after_it_is_the_sound_of_that_time() {
    let toolchain = toolchain_or_skip!();
    // A video whose sound is a different pitch every second: 300 hertz in the first, 500 in the second, and so on.
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "stepped_tones_20_seconds",
            audio_expression: Some("0.3*sin(2*PI*(300+200*floor(t))*t)"),
            ..TestVideoSpecification::COUNTER
        },
    );
    let mut project = project_for(&toolchain, &video);
    // Silent from 3 seconds to 5 seconds.
    make_a_keyframe(&mut project, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, PropertyId::Muted, 90, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, PropertyId::Muted, 150, PropertyValue::Flag(false));

    let (samples, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

    assert_eq!(error_output, None);
    assert_eq!(samples.len() / 2, 20 * 48_000, "nothing moved");
    let left = left_channel(&samples);
    let heard_at = |seconds: f64| &left[(seconds * 48_000.0) as usize..((seconds + 0.1) * 48_000.0) as usize];
    let pitch_at = |seconds: f64| pitch_in_hertz(heard_at(seconds), 48_000);
    assert!(
        (pitch_at(1.2) / 500.0 - 1.0).abs() < 0.04,
        "second 1 is 500 hertz: {}",
        pitch_at(1.2)
    );
    assert!(loudness(heard_at(3.3)) < 0.001, "silent: {}", loudness(heard_at(3.3)));
    assert!(loudness(heard_at(4.8)) < 0.001, "silent: {}", loudness(heard_at(4.8)));
    assert!(
        (pitch_at(6.2) / 1500.0 - 1.0).abs() < 0.04,
        "after it is the sound of second 6, which is 1500 hertz: {}",
        pitch_at(6.2)
    );
}

#[test]
fn the_preview_sound_is_the_sound_the_export_has_when_clips_and_tracks_have_keyframes_for_it() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    make_a_keyframe(&mut project, PropertyId::Gain, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, PropertyId::Gain, 300, PropertyValue::Number(-18.0));
    make_a_keyframe(&mut project, PropertyId::Muted, 0, PropertyValue::Flag(false));
    make_a_keyframe(&mut project, PropertyId::Muted, 360, PropertyValue::Flag(true));
    make_a_keyframe(&mut project, PropertyId::Muted, 420, PropertyValue::Flag(false));
    let exported = sound_of_the_export_of(&toolchain, &project);

    let (previewed, error_output) =
        everything_from(&toolchain, &request_for(&project, 0, SpedUpClipSound::MaintainPitch));

    assert_eq!(error_output, None);
    assert!(
        (previewed.len() / 2).abs_diff(exported.len() / 2) < 2 * 1024,
        "preview {} moments, export {}",
        previewed.len() / 2,
        exported.len() / 2
    );
    let seconds = (previewed.len().min(exported.len()) / 2 / 48_000).saturating_sub(1);
    assert!(seconds >= 15, "enough to compare: {seconds} s");
    for second in 0..seconds {
        let in_the_preview = loudness_of_stretch(&previewed, second as f64, 1.0);
        let in_the_export = loudness_of_stretch(&exported, second as f64, 1.0);
        assert!(
            (in_the_preview - in_the_export).abs() < 0.01,
            "second {second}: {in_the_preview} against {in_the_export}"
        );
    }
    assert!(
        loudness_of_stretch(&previewed, 12.5, 1.0) < 0.001,
        "the gap from 12 to 14 seconds is silent"
    );
}
