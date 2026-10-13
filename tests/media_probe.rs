//! Probing real videos with the real ffprobe.

mod support;

use std::process::Command;

use fastforwardrs::media::{MediaLocation, ProbeError, probe_source_media, probe_video};
use fastforwardrs::project::{FrameRate, SourceMediaProperties};
use support::{TestVideoSpecification, counter_video, scratch_directory, test_video};

#[test]
fn finds_ffmpeg_and_reports_its_version() {
    let toolchain = toolchain_or_skip!();
    assert!(toolchain.ffmpeg_path().to_string_lossy().contains("ffmpeg"));
    assert!(
        toolchain.version().banner.contains("ffmpeg version"),
        "{:?}",
        toolchain.version()
    );
    assert!(toolchain.version().major_version.is_some() || toolchain.version().banner.contains("N-"));
}

#[test]
fn probes_the_standard_test_video() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);

    let properties = probe_video(&toolchain, &video.path).unwrap();

    assert_eq!(properties.frame_rate, FrameRate::new(30, 1).unwrap());
    assert_eq!(properties.total_frame_count, 600);
    assert_eq!(properties.display_width_in_pixels, 320);
    assert_eq!(properties.display_height_in_pixels, 180);
    assert!(properties.has_audio);
}

/// Regression test: Matroska files state no frame count and their duration is a fraction of a
/// frame too long, so the video was believed to have one frame more than it does.
#[test]
fn a_matroska_file_gets_its_exact_frame_count() {
    let toolchain = toolchain_or_skip!();
    // The expected counts were checked by decoding the files (`ffprobe -count_frames`): 20 seconds
    // at 30 fps is 600 frames, and at 29.97 fps the generator makes 599.
    for (specification, expected_frames) in [
        (
            TestVideoSpecification {
                name: "counter_20_seconds_mkv",
                container: "mkv",
                ..TestVideoSpecification::COUNTER
            },
            600,
        ),
        (
            TestVideoSpecification {
                name: "ntsc_20_seconds_mkv",
                container: "mkv",
                frame_rate: "30000/1001",
                ..TestVideoSpecification::COUNTER
            },
            599,
        ),
    ] {
        let video = test_video(&toolchain, &specification);
        let properties = probe_video(&toolchain, &video.path).unwrap();
        assert_eq!(properties.total_frame_count, expected_frames, "{}", specification.name);
    }
}

#[test]
fn probes_a_video_without_audio() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "silent_3_seconds",
            duration_in_seconds: 3,
            with_audio: false,
            ..TestVideoSpecification::COUNTER
        },
    );
    let properties = probe_video(&toolchain, &video.path).unwrap();
    assert!(!properties.has_audio);
    assert_eq!(properties.total_frame_count, 90);
}

#[test]
fn probes_a_fractional_frame_rate_exactly() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "ntsc_4_seconds",
            frame_rate: "30000/1001",
            duration_in_seconds: 4,
            ..TestVideoSpecification::COUNTER
        },
    );
    let properties = probe_video(&toolchain, &video.path).unwrap();
    assert_eq!(properties.frame_rate, FrameRate::new(30000, 1001).unwrap());
    // 4 seconds of video at 29.97 fps: 120 frames (3.9 s would be 116.9 -> 117).
    assert!(
        (119..=120).contains(&properties.total_frame_count),
        "{}",
        properties.total_frame_count
    );
}

#[test]
fn a_video_with_a_rotation_flag_reports_its_upright_size() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let rotated_path = scratch.path().join("rotated.mp4");
    let status = Command::new(toolchain.ffmpeg_path())
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-display_rotation",
            "90",
            "-i",
        ])
        .arg(&video.path)
        .args(["-c", "copy"])
        .arg(&rotated_path)
        .status()
        .unwrap();
    assert!(status.success(), "could not make the rotated test video");

    let properties = probe_video(&toolchain, &rotated_path).unwrap();

    assert_eq!(
        (properties.display_width_in_pixels, properties.display_height_in_pixels),
        (180, 320),
        "a 320x180 video flagged as rotated by 90 degrees is shown 180x320"
    );
}

#[test]
fn a_video_with_non_square_pixels_reports_its_shown_size() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "wide_pixels_2_seconds",
            duration_in_seconds: 2,
            with_audio: false,
            extra_video_filter: Some("setsar=2/1"),
            ..TestVideoSpecification::COUNTER
        },
    );
    let properties = probe_video(&toolchain, &video.path).unwrap();
    assert_eq!(
        (properties.display_width_in_pixels, properties.display_height_in_pixels),
        (640, 180),
        "320x180 stored with pixels twice as wide as tall is shown 640x180"
    );
}

#[test]
fn a_file_that_does_not_exist_is_reported_with_its_path() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let missing_path = scratch.path().join("no_such_video.mp4");
    match probe_video(&toolchain, &missing_path) {
        Err(ProbeError::CouldNotOpen { location, message }) => {
            assert_eq!(location, MediaLocation::from(missing_path.as_path()));
            assert!(!message.is_empty());
        }
        other => panic!("expected CouldNotOpen, got {other:?}"),
    }
}

#[test]
fn a_file_that_is_not_a_video_is_reported_as_unopenable() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let text_path = scratch.path().join("notes.mp4");
    std::fs::write(&text_path, "this is definitely not a video file").unwrap();
    assert!(matches!(
        probe_video(&toolchain, &text_path),
        Err(ProbeError::CouldNotOpen { .. })
    ));
}

#[test]
fn a_file_with_unusual_characters_in_its_name_can_be_probed() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let oddly_named = scratch.path().join("holiday #1 (final) - café & friends [2026].mp4");
    std::fs::copy(&video.path, &oddly_named).unwrap();
    assert_eq!(probe_video(&toolchain, &oddly_named).unwrap().total_frame_count, 600);
}

#[test]
fn a_sound_file_is_probed_as_a_sound_that_lasts_as_long_as_it_does() {
    let toolchain = toolchain_or_skip!();
    let tone = support::tone_file(&toolchain, 440, 3);

    let properties = probe_source_media(&toolchain, &tone).unwrap();

    let SourceMediaProperties::Audio(sound) = properties else {
        panic!("a tone is a sound, got {properties:?}")
    };
    let seconds = sound.duration_in_microseconds as f64 / 1_000_000.0;
    assert!((seconds - 3.0).abs() < 0.05, "3 seconds of tone, probed as {seconds}");
    assert!(properties.has_sound());
}

#[test]
fn a_video_is_probed_as_a_video_by_the_probe_that_also_knows_sounds() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);

    let properties = probe_source_media(&toolchain, &video.path).unwrap();

    let SourceMediaProperties::Video(found) = properties else {
        panic!("a video is a video, got {properties:?}")
    };
    assert_eq!(found, probe_video(&toolchain, &video.path).unwrap());
}

#[test]
fn a_file_that_is_neither_is_reported_as_unopenable_by_the_probe_that_also_knows_sounds() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let not_media = folder.path().join("notes.wav");
    std::fs::write(&not_media, "this is text").unwrap();

    assert!(matches!(
        probe_source_media(&toolchain, &not_media),
        Err(ProbeError::CouldNotOpen { .. })
    ));
}
