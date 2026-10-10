//! Making timeline thumbnails with the real ffmpeg.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::{FfmpegToolchain, PictureSize, ThumbnailEvent, ThumbnailExtraction};
use support::{TestVideoSpecification, counter_video, frame_number_in_picture, scratch_directory, test_video};

const TIME_ALLOWED: Duration = Duration::from_secs(60);

struct ExtractionResult {
    /// For each second in order: the source frame number shown in that second's thumbnail, and how
    /// many bytes the compressed thumbnail takes.
    thumbnails: Vec<(usize, u64, usize)>,
    error_output: Option<String>,
}

fn extract_everything(
    toolchain: &FfmpegToolchain,
    video_path: &std::path::Path,
    size: PictureSize,
) -> ExtractionResult {
    let extraction =
        ThumbnailExtraction::start(toolchain, video_path, size, Arc::new(|| {})).expect("extraction should start");
    let started = Instant::now();
    let mut thumbnails = Vec::new();
    loop {
        match extraction.try_next_event() {
            Some(ThumbnailEvent::Thumbnail { second, picture }) => {
                assert_eq!(picture.size(), size);
                let decoded = picture.decompress().unwrap();
                let frame_number = frame_number_in_picture(
                    &decoded.rgba_pixels,
                    decoded.width_in_pixels,
                    decoded.height_in_pixels,
                    4,
                );
                thumbnails.push((second, frame_number, picture.byte_count()));
            }
            Some(ThumbnailEvent::Finished { error_output }) => {
                return ExtractionResult {
                    thumbnails,
                    error_output,
                };
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "extraction did not finish in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[test]
fn makes_one_thumbnail_per_second_each_showing_the_frame_at_that_exact_second() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);

    let result = extract_everything(
        &toolchain,
        &video.path,
        PictureSize {
            width_in_pixels: 160,
            height_in_pixels: 90,
        },
    );

    assert_eq!(result.error_output, None);
    let seconds: Vec<usize> = result.thumbnails.iter().map(|&(second, _, _)| second).collect();
    assert_eq!(
        seconds,
        (0..20).collect::<Vec<usize>>(),
        "a 20 second video gets 20 thumbnails, in order"
    );
    let frame_numbers: Vec<u64> = result
        .thumbnails
        .iter()
        .map(|&(_, frame_number, _)| frame_number)
        .collect();
    assert_eq!(
        frame_numbers,
        (0..20).map(|second| second * 30).collect::<Vec<u64>>(),
        "second k shows frame 30k"
    );
}

#[test]
fn compressed_thumbnails_are_small() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let result = extract_everything(
        &toolchain,
        &video.path,
        PictureSize {
            width_in_pixels: 160,
            height_in_pixels: 90,
        },
    );
    let largest = result
        .thumbnails
        .iter()
        .map(|&(_, _, byte_count)| byte_count)
        .max()
        .unwrap();
    // Raw RGB would be 43,200 bytes each.
    assert!(largest < 8_000, "the largest 160x90 thumbnail takes {largest} bytes");
}

#[test]
fn thumbnails_follow_the_requested_size() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    for size in [
        PictureSize {
            width_in_pixels: 64,
            height_in_pixels: 36,
        },
        PictureSize {
            width_in_pixels: 96,
            height_in_pixels: 96,
        },
    ] {
        let result = extract_everything(&toolchain, &video.path, size);
        assert_eq!(result.thumbnails.len(), 20, "{size:?}");
    }
}

#[test]
fn a_video_that_is_not_a_whole_number_of_seconds_long_gets_a_thumbnail_for_its_last_partial_second() {
    let toolchain = toolchain_or_skip!();
    // 30 fps for 2 seconds is 60 frames: thumbnails at 0 s and 1 s. Make a 2.5 second video: the
    // 75 frames cover seconds 0, 1 and the start of 2, so there are three thumbnails.
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "two_and_a_half_seconds",
            duration_in_seconds: 3,
            with_audio: false,
            ..TestVideoSpecification::COUNTER
        },
    );
    let scratch = scratch_directory();
    let trimmed = scratch.path().join("trimmed.mp4");
    let status = std::process::Command::new(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
        .arg(&video.path)
        .args(["-frames:v", "75", "-c", "copy"])
        .arg(&trimmed)
        .status()
        .unwrap();
    assert!(status.success());

    let result = extract_everything(
        &toolchain,
        &trimmed,
        PictureSize {
            width_in_pixels: 80,
            height_in_pixels: 45,
        },
    );

    let frame_numbers: Vec<u64> = result
        .thumbnails
        .iter()
        .map(|&(_, frame_number, _)| frame_number)
        .collect();
    assert_eq!(frame_numbers, vec![0, 30, 60]);
}

#[test]
fn a_file_that_cannot_be_decoded_finishes_with_an_explanation_and_no_thumbnails() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let not_a_video = scratch.path().join("not_a_video.mp4");
    std::fs::write(&not_a_video, "just some text").unwrap();

    let result = extract_everything(
        &toolchain,
        &not_a_video,
        PictureSize {
            width_in_pixels: 160,
            height_in_pixels: 90,
        },
    );

    assert!(result.thumbnails.is_empty());
    assert!(result.error_output.is_some_and(|text| !text.is_empty()));
}

#[test]
fn dropping_an_extraction_early_stops_ffmpeg_promptly() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "long_60_seconds",
            width_in_pixels: 640,
            height_in_pixels: 360,
            duration_in_seconds: 60,
            with_audio: false,
            ..TestVideoSpecification::COUNTER
        },
    );
    let started = Instant::now();
    for _ in 0..5 {
        let extraction = ThumbnailExtraction::start(
            &toolchain,
            &video.path,
            PictureSize {
                width_in_pixels: 160,
                height_in_pixels: 90,
            },
            Arc::new(|| {}),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        drop(extraction);
    }
    assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
}
