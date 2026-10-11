//! Decoding preview frames with the real ffmpeg, checked frame by frame.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::{DecodedFrame, FfmpegToolchain, FrameStream, FrameStreamEvent, FrameStreamRequest};
use fastforwardrs::project::{FrameRate, SourceFrameIndex, SpeedFactor};
use support::{
    TestVideo, TestVideoSpecification, counter_video, frame_number_in_picture, scratch_directory, test_video,
};

const TIME_ALLOWED_FOR_A_STREAM: Duration = Duration::from_secs(30);

fn request_for(
    video: &TestVideo,
    first_source_frame: u64,
    source_frame_count: u64,
    speed_factor: u32,
) -> FrameStreamRequest {
    FrameStreamRequest {
        video_path: video.path.clone(),
        frame_rate: FrameRate::new(30, 1).unwrap(),
        first_source_frame: SourceFrameIndex::new(first_source_frame),
        source_frame_count,
        speed_factor: SpeedFactor::new(speed_factor).unwrap(),
        video_display_size_in_pixels: (
            video.specification.width_in_pixels,
            video.specification.height_in_pixels,
        ),
        picture_width_in_pixels: video.specification.width_in_pixels,
        picture_height_in_pixels: video.specification.height_in_pixels,
    }
}

/// Runs a stream to its end and returns the source frame number of every delivered picture, plus
/// ffmpeg's complaint if there was one.
fn frame_numbers_delivered(toolchain: &FfmpegToolchain, request: &FrameStreamRequest) -> (Vec<u64>, Option<String>) {
    let stream = FrameStream::start(toolchain, request, Arc::new(|| {})).expect("the stream should start");
    let started = Instant::now();
    let mut frame_numbers = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(FrameStreamEvent::Frame(frame)) => {
                assert_eq!(
                    (frame.width_in_pixels, frame.height_in_pixels),
                    request.delivered_size()
                );
                assert_eq!(
                    frame.rgba_pixels.len(),
                    (frame.width_in_pixels * frame.height_in_pixels * 4) as usize
                );
                frame_numbers.push(frame_number_in_picture(
                    &frame.rgba_pixels,
                    frame.width_in_pixels,
                    frame.height_in_pixels,
                    4,
                ));
            }
            Some(FrameStreamEvent::Finished { error_output }) => return (frame_numbers, error_output),
            None => {
                assert!(
                    started.elapsed() < TIME_ALLOWED_FOR_A_STREAM,
                    "the stream did not finish in time"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[test]
fn a_normal_speed_clip_delivers_each_of_its_frames_in_order() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let (frame_numbers, error_output) = frame_numbers_delivered(&toolchain, &request_for(&video, 100, 30, 1));
    assert_eq!(frame_numbers, (100..130).collect::<Vec<u64>>());
    assert_eq!(error_output, None);
}

#[test]
fn a_sped_up_clip_delivers_every_nth_frame_starting_with_its_first() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &request_for(&video, 100, 100, 3));
    // 100 source frames at 3x: ceil(100 / 3) = 34 pictures: 100, 103, ..., 199.
    assert_eq!(
        frame_numbers,
        (0..34).map(|picture| 100 + picture * 3).collect::<Vec<u64>>()
    );
}

#[test]
fn a_speed_that_does_not_divide_the_clip_length_still_delivers_the_partial_last_group() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &request_for(&video, 7, 10, 4));
    assert_eq!(frame_numbers, vec![7, 11, 15]);
}

#[test]
fn a_clip_shorter_than_its_speed_factor_still_shows_its_first_frame() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &request_for(&video, 250, 3, 100));
    assert_eq!(frame_numbers, vec![250]);
}

#[test]
fn clips_are_exact_at_the_start_the_end_and_around_keyframes() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // The test video has a keyframe every 30 frames, so these straddle keyframes and both ends.
    for first_source_frame in [0u64, 1, 29, 30, 31, 299, 300, 301, 594, 595] {
        let (frame_numbers, error_output) =
            frame_numbers_delivered(&toolchain, &request_for(&video, first_source_frame, 5, 1));
        assert_eq!(
            frame_numbers,
            (first_source_frame..first_source_frame + 5).collect::<Vec<u64>>(),
            "clip starting at source frame {first_source_frame} ({error_output:?})"
        );
    }
}

#[test]
fn a_clip_that_runs_past_the_end_of_the_video_simply_ends_with_the_video() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &request_for(&video, 595, 100, 1));
    assert_eq!(frame_numbers, vec![595, 596, 597, 598, 599]);
}

#[test]
fn pictures_are_scaled_to_the_requested_size_and_keep_their_frame_number() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // (All of the shape of the video, so that it fills the picture.)
    for (width, height) in [(160, 90), (640, 360), (80, 45)] {
        let mut request = request_for(&video, 40, 4, 1);
        request.picture_width_in_pixels = width;
        request.picture_height_in_pixels = height;
        let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &request);
        assert_eq!(frame_numbers, vec![40, 41, 42, 43], "scaled to {width}x{height}");
    }
}

/// The first picture a stream delivers.
fn first_picture_delivered(toolchain: &FfmpegToolchain, request: &FrameStreamRequest) -> DecodedFrame {
    let stream = FrameStream::start(toolchain, request, Arc::new(|| {})).expect("the stream should start");
    let started = Instant::now();
    loop {
        match stream.try_next_event() {
            Some(FrameStreamEvent::Frame(frame)) => return frame,
            Some(FrameStreamEvent::Finished { error_output }) => panic!("no picture came: {error_output:?}"),
            None => {
                assert!(started.elapsed() < TIME_ALLOWED_FOR_A_STREAM, "no picture came in time");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[test]
fn a_video_of_another_shape_than_the_picture_is_delivered_as_big_as_fits_inside_it_in_its_own_shape() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain); // 320x180
    let mut request = request_for(&video, 40, 4, 1);
    request.picture_width_in_pixels = 200;
    request.picture_height_in_pixels = 200;

    let frame = first_picture_delivered(&toolchain, &request);

    // 16:9 in a square of 200: 200 wide and 112 tall, which is all of the picture there is: no black
    // is put round it (where it goes on the picture is for whoever draws it to say).
    let (width, height) = (frame.width_in_pixels as usize, frame.height_in_pixels as usize);
    assert_eq!((width, height), (200, 112));
    assert_eq!(request.delivered_size(), (200, 112));
    // It is the picture of source frame 40, with its frame number strip along its top.
    assert_eq!(frame_number_in_picture(&frame.rgba_pixels, 200, 112, 4), 40);
}

#[test]
fn a_fractional_frame_rate_video_is_decoded_exactly_too() {
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
    let mut request = request_for(&video, 50, 20, 1);
    request.frame_rate = FrameRate::new(30000, 1001).unwrap();
    let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &request);
    assert_eq!(frame_numbers, (50..70).collect::<Vec<u64>>());

    let mut sped_up = request_for(&video, 50, 20, 4);
    sped_up.frame_rate = FrameRate::new(30000, 1001).unwrap();
    let (frame_numbers, _) = frame_numbers_delivered(&toolchain, &sped_up);
    assert_eq!(frame_numbers, vec![50, 54, 58, 62, 66]);
}

/// Regression test: with the frame slot rounded up from the file's timestamps, a Matroska file
/// (which stores timestamps rounded to whole milliseconds) delivered duplicated and skipped
/// pictures. The pictures must be exact whatever the container.
#[test]
fn pictures_are_exact_in_files_whose_timestamps_are_rounded_to_milliseconds() {
    let toolchain = toolchain_or_skip!();
    for (specification, frame_rate) in [
        (
            TestVideoSpecification {
                name: "counter_20_seconds_mkv",
                container: "mkv",
                ..TestVideoSpecification::COUNTER
            },
            FrameRate::new(30, 1).unwrap(),
        ),
        (
            TestVideoSpecification {
                name: "ntsc_20_seconds_mkv",
                container: "mkv",
                frame_rate: "30000/1001",
                with_audio: false,
                ..TestVideoSpecification::COUNTER
            },
            FrameRate::new(30000, 1001).unwrap(),
        ),
    ] {
        let video = test_video(&toolchain, &specification);
        for first_source_frame in [0u64, 1, 2, 29, 30, 31, 100, 299, 301, 580] {
            for speed in [1u32, 3] {
                // 12 source frames from a frame that is at least 12 before the end of the video.
                let mut request = request_for(&video, first_source_frame, 12, speed);
                request.frame_rate = frame_rate;
                let (frame_numbers, error_output) = frame_numbers_delivered(&toolchain, &request);
                let expected: Vec<u64> = (0..12u64.div_ceil(u64::from(speed)))
                    .map(|picture| first_source_frame + picture * u64::from(speed))
                    .collect();
                assert_eq!(
                    frame_numbers, expected,
                    "{} starting at frame {first_source_frame}, speed {speed} ({error_output:?})",
                    specification.name
                );
            }
        }
    }
}

#[test]
fn a_variable_frame_rate_file_gives_the_picture_nearest_in_time_to_each_slot() {
    let toolchain = toolchain_or_skip!();
    let video = variable_frame_rate_video(&toolchain);
    // The video has 400 pictures over 20 s, so the project (and the request) run at 20 per second.
    let mut request = request_for(&video, 100, 40, 1);
    request.frame_rate = FrameRate::new(20, 1).unwrap();

    let (frame_numbers, error_output) = frame_numbers_delivered(&toolchain, &request);

    assert_eq!(frame_numbers.len(), 40, "{error_output:?}");
    for (slot, &frame_number) in frame_numbers.iter().enumerate() {
        let slot_seconds = (100 + slot) as f64 / 20.0;
        let picture_seconds = frame_number as f64 / 30.0;
        // Seeking into a variable frame rate file can start up to one gap late, so allow it.
        assert!(
            (picture_seconds - slot_seconds).abs() <= 0.1 + 0.05 + 1e-9,
            "slot {slot} at {slot_seconds} s shows source frame {frame_number} (at {picture_seconds} s)"
        );
    }
}

/// The numbered test video with most of its first half's pictures removed: 400 pictures over 20
/// seconds, every third one for the first 10 seconds and all of them for the second 10.
fn variable_frame_rate_video(toolchain: &FfmpegToolchain) -> TestVideo {
    test_video(
        toolchain,
        &TestVideoSpecification {
            name: "variable_frame_rate_20_seconds",
            with_audio: false,
            extra_video_filter: Some("select='lt(n,300)*not(mod(n,3))+gte(n,300)'"),
            extra_output_arguments: &["-fps_mode", "vfr"],
            ..TestVideoSpecification::COUNTER
        },
    )
}

#[test]
fn a_file_that_cannot_be_decoded_finishes_with_an_explanation_and_no_frames() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let not_a_video = scratch.path().join("not_a_video.mp4");
    std::fs::write(&not_a_video, "just some text").unwrap();
    let mut request = request_for(&video, 0, 10, 1);
    request.video_path = not_a_video;

    let (frame_numbers, error_output) = frame_numbers_delivered(&toolchain, &request);

    assert!(frame_numbers.is_empty());
    let error_output = error_output.expect("ffmpeg should have explained the failure");
    assert!(!error_output.is_empty());
}

#[test]
fn dropping_a_stream_stops_decoding_promptly_even_with_frames_waiting() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let started = Instant::now();
    for _ in 0..10 {
        let stream = FrameStream::start(&toolchain, &request_for(&video, 0, 600, 1), Arc::new(|| {})).unwrap();
        // Let the decoder fill its buffer so the reading thread is blocked waiting for room.
        std::thread::sleep(Duration::from_millis(40));
        drop(stream);
    }
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "starting and dropping streams took {:?}",
        started.elapsed()
    );
}

#[test]
fn the_wake_up_callback_is_called_when_frames_arrive() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let wake_up_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&wake_up_count);
    let stream = FrameStream::start(
        &toolchain,
        &request_for(&video, 0, 10, 1),
        Arc::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    )
    .unwrap();
    let started = Instant::now();
    while !matches!(stream.try_next_event(), Some(FrameStreamEvent::Finished { .. })) {
        assert!(started.elapsed() < TIME_ALLOWED_FOR_A_STREAM);
        std::thread::sleep(Duration::from_millis(2));
    }
    // Ten frames and the end-of-stream notice each ask the interface to wake up.
    assert!(wake_up_count.load(std::sync::atomic::Ordering::SeqCst) >= 11);
}
