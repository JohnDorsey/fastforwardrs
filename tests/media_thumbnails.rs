// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making timeline thumbnails with the real ffmpeg.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::{FfmpegToolchain, PictureSize, ThumbnailEvent, ThumbnailExtraction, ThumbnailRange};
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
    extract_range(toolchain, video_path, size, ThumbnailRange::EVERY_SECOND)
}

fn extract_range(
    toolchain: &FfmpegToolchain,
    video_path: &std::path::Path,
    size: PictureSize,
    range: ThumbnailRange,
) -> ExtractionResult {
    let extraction = ThumbnailExtraction::start_range(toolchain, video_path, size, range, Arc::new(|| {}))
        .expect("extraction should start");
    let started = Instant::now();
    let mut thumbnails = Vec::new();
    loop {
        match extraction.try_next_event() {
            Some(ThumbnailEvent::Thumbnail { index: second, picture }) => {
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

const SMALL: PictureSize = PictureSize {
    width_in_pixels: 160,
    height_in_pixels: 90,
};

/// The indexes and the source frames shown of the pictures that a range of the 20 second, 30 frames
/// a second counter video gives.
fn indexes_and_frames(toolchain: &FfmpegToolchain, range: ThumbnailRange) -> Vec<(usize, u64)> {
    let video = counter_video(toolchain);
    let result = extract_range(toolchain, &video.path, SMALL, range);
    assert_eq!(result.error_output, None);
    result
        .thumbnails
        .iter()
        .map(|&(index, frame_number, _)| (index, frame_number))
        .collect()
}

#[test]
fn finer_thumbnails_from_the_start_are_two_or_four_or_eight_a_second_each_showing_the_frame_at_its_moment() {
    let toolchain = toolchain_or_skip!();
    // Two a second: 0 s, 0.5 s, 1 s, 1.5 s, 2 s are frames 0, 15, 30, 45 and 60.
    assert_eq!(
        indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level: 1,
                first_index: 0,
                count: Some(5)
            }
        ),
        vec![(0, 0), (1, 15), (2, 30), (3, 45), (4, 60)]
    );
    // Eight a second: every 3.75 frames, each the frame that is on screen at its moment.
    assert_eq!(
        indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level: 3,
                first_index: 0,
                count: Some(5)
            }
        ),
        vec![(0, 0), (1, 3), (2, 7), (3, 11), (4, 15)],
        "at 0, 3.75, 7.5, 11.25 and 15 frames"
    );
}

#[test]
fn a_range_in_the_middle_of_the_video_starts_at_its_first_moment_whether_or_not_a_frame_starts_there() {
    let toolchain = toolchain_or_skip!();
    // Four a second, the 10th to the 13th: 2.5 s, 2.75 s, 3 s and 3.25 s, which are at 75, 82.5, 90 and
    // 97.5 frames in. A frame starts at 2.5 s and at 3 s but not at the other moments, which are in the
    // middle of frames 82 and 97.
    assert_eq!(
        indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level: 2,
                first_index: 10,
                count: Some(4)
            }
        ),
        vec![(10, 75), (11, 82), (12, 90), (13, 97)]
    );
    // The same when the first moment is in the middle of a frame (2.75 s is in the middle of frame 82):
    // that frame is the first picture, as it is on screen at that moment.
    assert_eq!(
        indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level: 2,
                first_index: 11,
                count: Some(3)
            }
        ),
        vec![(11, 82), (12, 90), (13, 97)]
    );
    // The same pictures as when everything from the start is made: it does not matter where the range begins.
    let from_the_start = indexes_and_frames(
        &toolchain,
        ThumbnailRange {
            level: 2,
            first_index: 0,
            count: Some(16),
        },
    );
    assert_eq!(from_the_start[10..14], [(10, 75), (11, 82), (12, 90), (13, 97)]);
}

#[test]
fn a_picture_is_the_same_whichever_range_it_was_made_in() {
    let toolchain = toolchain_or_skip!();
    let everything: Vec<Vec<(usize, u64)>> = (0..=4)
        .map(|level| {
            indexes_and_frames(
                &toolchain,
                ThumbnailRange {
                    level,
                    first_index: 0,
                    count: Some(16 << level),
                },
            )
        })
        .collect();
    assert_eq!(everything[0].len(), 16);
    assert_eq!(everything[4].len(), 256);
    // A range of any level starting at any moment, long or short, gives the same pictures as those
    // made from the start of the video (which have no seeking to go wrong). First some whose lead-in
    // begins in the middle of a frame (such as 2.3125 s, which is 69.375 frames in): ffmpeg has to be
    // told to pad the start of the pictures then, or it starts a slot late and all of them are one off.
    for (level, first_index, count) in [(4u8, 41usize, 5usize), (3, 13, 4), (4, 77, 6)] {
        let made = indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level,
                first_index,
                count: Some(count),
            },
        );
        let expected = &everything[level as usize][first_index..first_index + count];
        assert_eq!(
            made, expected,
            "{count} pictures of level {level} from number {first_index}"
        );
    }
    let mut random = support::RandomNumbers::with_seed(5);
    for _ in 0..14 {
        let level = random.next_below(5) as u8;
        let first_index = random.next_below(14 << level) as usize;
        let count = 1 + random.next_below(1 + (2 << level)) as usize;
        let made = indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level,
                first_index,
                count: Some(count),
            },
        );
        let expected = &everything[level as usize][first_index..first_index + count];
        assert_eq!(
            made, expected,
            "{count} pictures of level {level} from number {first_index}"
        );
    }
}

#[test]
fn a_range_that_runs_past_the_end_of_the_video_has_only_the_pictures_that_there_are() {
    let toolchain = toolchain_or_skip!();
    // A 20 second video has pictures of every second from 0 to 19.
    assert_eq!(
        indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level: 0,
                first_index: 18,
                count: Some(10)
            }
        ),
        vec![(18, 540), (19, 570)]
    );
    // And a range beyond the end has none, and ends all the same.
    assert_eq!(
        indexes_and_frames(
            &toolchain,
            ThumbnailRange {
                level: 2,
                first_index: 400,
                count: Some(8)
            }
        ),
        vec![]
    );
}

#[test]
fn a_range_has_no_more_pictures_than_it_asks_for() {
    let toolchain = toolchain_or_skip!();
    let pictures = indexes_and_frames(
        &toolchain,
        ThumbnailRange {
            level: 1,
            first_index: 6,
            count: Some(3),
        },
    );
    assert_eq!(pictures, vec![(6, 90), (7, 105), (8, 120)]);
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
