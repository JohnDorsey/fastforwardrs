// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::mpsc;

use super::*;

fn request(first_source_frame: u64, source_frame_count: u64, speed_factor: u32) -> FrameStreamRequest {
    FrameStreamRequest {
        video: MediaLocation::from("holiday.mp4"),
        project_frame_rate: FrameRate::new(30, 1).unwrap(),
        video_frame_rate: FrameRate::new(30, 1).unwrap(),
        first_source_frame: SourceFrameIndex::new(first_source_frame),
        source_frame_count,
        speed_factor: SpeedFactor::new(speed_factor).unwrap(),
        video_display_size_in_pixels: (1280, 720),
        picture_width_in_pixels: 640,
        picture_height_in_pixels: 360,
        delivery: FrameDelivery::Rgba,
        ffmpeg_reads_planes_as: None,
    }
}

fn arguments_as_text(request: &FrameStreamRequest) -> Vec<String> {
    frame_stream_arguments(request)
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

fn filters_of(request: &FrameStreamRequest) -> String {
    let arguments = arguments_as_text(request);
    arguments[arguments.iter().position(|argument| argument == "-vf").unwrap() + 1].clone()
}

#[test]
fn the_command_line_seeks_half_a_frame_early_and_reads_exactly_the_clip() {
    // Frames 300 up to (not including) 600 at 30 fps: start at 9.983333 s (half a frame
    // before frame 300 at 10 s, to the next microsecond), read until half a frame before frame 600 at
    // 20 s. The first picture is then 16667 microseconds after the start of what is read, and 100
    // less than that is what the pictures are moved back by.
    let arguments = arguments_as_text(&request(300, 300, 1));
    assert_eq!(
        arguments,
        [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-ss",
            "9.983333",
            "-t",
            "10.000000",
            "-i",
            "holiday.mp4",
            "-an",
            "-sn",
            "-dn",
            "-vf",
            "settb=AVTB,setpts=PTS-(16567),fps=30/1:round=near:start_time=0,setpts=N*1/(30*TB),scale=640:360:flags=fast_bilinear,format=rgba",
            "-frames:v",
            "300",
            "-f",
            "rawvideo",
            "pipe:1",
        ]
    );
}

#[test]
fn a_stream_can_be_asked_to_deliver_another_size_with_another_scaling_and_nothing_else_of_the_command_line_changes() {
    let request = request(300, 300, 1);
    let preview = arguments_as_text(&request);
    let export: Vec<String> = frame_stream_arguments_delivering(&request, (200, 100), "bicubic")
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    assert_eq!(export.len(), preview.len());
    let differences: Vec<(&String, &String)> = preview.iter().zip(&export).filter(|(a, b)| a != b).collect();
    assert_eq!(differences.len(), 1, "{differences:?}");
    assert!(
        differences[0]
            .0
            .contains(",scale=640:360:flags=fast_bilinear,format=rgba"),
        "{differences:?}"
    );
    assert!(
        differences[0].1.contains(",scale=200:100:flags=bicubic,format=rgba"),
        "{differences:?}"
    );
    assert_eq!(
        differences[0]
            .0
            .replace("scale=640:360:flags=fast_bilinear", "scale=200:100:flags=bicubic"),
        *differences[0].1
    );
    // The preview's own command line is the one for the size that fits inside its picture, scaled quickly.
    assert_eq!(
        frame_stream_arguments(&request),
        frame_stream_arguments_delivering(&request, request.delivered_size(), "fast_bilinear")
    );
}

#[test]
fn a_clip_at_the_very_start_cannot_seek_before_zero_and_stops_half_a_frame_before_its_end() {
    let arguments = arguments_as_text(&request(0, 90, 1));
    assert_eq!(arguments[5], "0.000000");
    // Frames 0..90 end half a frame before 3 s.
    assert_eq!(arguments[7], "2.983333");
    // Reading starts at the very first picture, so there is no half frame to take back: the first
    // frame is at the start of what is read, and only the bias moves the pictures.
    assert!(
        filters_of(&request(0, 90, 1)).starts_with("settb=AVTB,setpts=PTS-(-100),fps=30/1:round=near:start_time=0,"),
        "{}",
        filters_of(&request(0, 90, 1))
    );
}

#[test]
fn a_video_whose_pictures_last_longer_than_a_frame_of_the_project_is_read_from_as_far_back_as_the_picture_on_screen_began()
 {
    let mut slower = request(33, 12, 1);
    slower.video_frame_rate = FrameRate::new(25, 1).unwrap();
    let arguments = arguments_as_text(&slower);
    // Frame 33 is at 1.1 s. A picture of the video lasts 40 ms and a frame of the project 33.33 ms,
    // so the picture on screen then began up to half a frame and 6.67 ms (23.33 ms, which is made
    // 23.334) before: reading starts at 1.076666 s, and the first frame is 23334 microseconds later.
    assert_eq!(arguments[5], "1.076666");
    assert!(
        filters_of(&slower).starts_with("settb=AVTB,setpts=PTS-(23234),fps=30/1:round=near:start_time=0,"),
        "{}",
        filters_of(&slower)
    );
    // Frame 45 is at 1.5 s: the reading ends half a frame before it, at 1.483333 s (as it does for a
    // video that is as fast as the project: only the start is earlier).
    assert_eq!(arguments[7], "0.406667", "from 1.076666 s to 1.483333 s");
}

#[test]
fn a_video_whose_pictures_are_as_short_as_the_frames_of_the_project_or_shorter_is_read_the_same_whatever_its_rate() {
    let same = arguments_as_text(&request(300, 300, 1));
    for faster in [30, 50, 60, 120] {
        let mut request = request(300, 300, 1);
        request.video_frame_rate = FrameRate::new(faster, 1).unwrap();
        assert_eq!(
            arguments_as_text(&request),
            same,
            "a video of {faster} pictures a second"
        );
    }
}

#[test]
fn the_reading_starts_early_enough_and_ends_late_enough_for_every_pair_of_frame_rates_and_never_starts_before_the_file()
{
    let rates = [
        (12, 1),
        (24000, 1001),
        (24, 1),
        (25, 1),
        (30000, 1001),
        (30, 1),
        (60, 1),
    ];
    for (project_numerator, project_denominator) in rates {
        for (video_numerator, video_denominator) in rates {
            for first_source_frame in [0, 1, 2, 7, 33, 300, 12_345] {
                let mut request = request(first_source_frame, 40, 1);
                request.project_frame_rate = FrameRate::new(project_numerator, project_denominator).unwrap();
                request.video_frame_rate = FrameRate::new(video_numerator, video_denominator).unwrap();
                let project_frame_rate = request.project_frame_rate;
                let description = format!(
                    "a video of {} in a project of {project_frame_rate}, from frame {first_source_frame}",
                    request.video_frame_rate
                );
                let timing = StreamTiming::of(&request);

                // Where the first frame is in what is read is where it is in the file, less where the
                // reading starts.
                let first_frame_at = project_frame_rate.microseconds_at_frame(first_source_frame);
                assert_eq!(
                    timing.start_in_file + timing.first_frame_after_the_start,
                    first_frame_at,
                    "{description}"
                );

                // The reading starts half a frame before it, and as much before that as a picture of the
                // video lasts longer than a frame (unless that would be before the file).
                let frame = 1_000_000.0 / project_frame_rate.frames_per_second();
                let picture = 1_000_000.0 / request.video_frame_rate.frames_per_second();
                let longer_than_a_frame = (picture - frame).max(0.0);
                assert!(
                    timing.start_in_file == 0
                        || timing.first_frame_after_the_start as f64 >= frame / 2.0 + longer_than_a_frame,
                    "{description}: the reading starts {} before the first frame",
                    timing.first_frame_after_the_start
                );

                // It ends half a frame before the end of the frames wanted.
                let end_at = project_frame_rate.microseconds_at_frame(first_source_frame + 40) as f64;
                let reads_to = (timing.start_in_file + timing.duration_read) as f64;
                assert!(
                    (reads_to - (end_at - frame / 2.0)).abs() <= 1.0,
                    "{description}: reads to {reads_to}, the frames end at {end_at}"
                );
            }
        }
    }
}

#[test]
fn an_accelerated_clip_keeps_every_nth_picture_after_resampling_and_a_normal_one_keeps_all() {
    let accelerated = filters_of(&request(300, 300, 8));
    assert!(
        accelerated.contains("fps=30/1:round=near:start_time=0,select='not(mod(n,8))',setpts=N*1/(30*TB),"),
        "{accelerated}"
    );
    assert!(!filters_of(&request(300, 300, 1)).contains("select"));
}

#[test]
fn the_stream_ends_after_the_pictures_wanted_which_are_one_in_every_speed_factor_of_the_frames() {
    let frames_wanted = |request: &FrameStreamRequest| {
        let arguments = arguments_as_text(request);
        let position = arguments.iter().position(|argument| argument == "-frames:v")?;
        Some(arguments[position + 1].clone())
    };
    assert_eq!(frames_wanted(&request(100, 30, 1)).as_deref(), Some("30"));
    assert_eq!(
        frames_wanted(&request(100, 100, 3)).as_deref(),
        Some("34"),
        "100 frames at 3x: 0, 3, ..., 99"
    );
    assert_eq!(frames_wanted(&request(100, 99, 3)).as_deref(), Some("33"));
    assert_eq!(frames_wanted(&request(100, 1, 8)).as_deref(), Some("1"));
    assert_eq!(
        frames_wanted(&request(100, 0, 1)),
        None,
        "none wanted, so nothing is asked for, which would be all"
    );
}

#[test]
fn resampling_uses_the_nearest_slot_never_round_up() {
    for speed in [1, 4] {
        let filters = filters_of(&request(300, 300, speed));
        assert!(
            filters.contains("round=near") && !filters.contains("round=up"),
            "{filters}"
        );
    }
}

#[test]
fn fractional_frame_rates_are_passed_as_exact_fractions() {
    let mut request = request(0, 100, 1);
    request.project_frame_rate = FrameRate::new(30000, 1001).unwrap();
    request.video_frame_rate = FrameRate::new(30000, 1001).unwrap();
    let filters = filters_of(&request);
    assert!(filters.contains("fps=30000/1001:round=near:start_time=0"), "{filters}");
    assert!(filters.contains("setpts=N*1001/(30000*TB)"), "{filters}");
}

#[test]
fn a_video_of_the_shape_of_the_picture_is_scaled_straight_to_it() {
    let filters = filters_of(&request(0, 100, 1));
    assert!(
        filters.ends_with(",scale=640:360:flags=fast_bilinear,format=rgba"),
        "{filters}"
    );
    assert!(!filters.contains("pad"), "{filters}");
}

#[test]
fn a_video_of_another_shape_is_delivered_as_big_as_fits_inside_the_picture_in_its_own_shape_with_no_black_round_it() {
    let mut upright = request(0, 100, 1);
    upright.video_display_size_in_pixels = (720, 1280);
    // 9:16 in 640x360: as tall as the picture, 202.5 wide, which is made 202 (even).
    let filters = filters_of(&upright);
    assert!(
        filters.ends_with(",scale=202:360:flags=fast_bilinear,format=rgba"),
        "{filters}"
    );
    assert!(
        !filters.contains("pad"),
        "where it goes on the picture is for the drawing to say"
    );
    assert_eq!(upright.delivered_size(), (202, 360));
}

#[test]
fn what_is_delivered_is_the_fit_of_the_video_in_the_picture_whatever_the_shapes() {
    for video in [(1280, 720), (720, 1280), (1000, 1000), (3840, 1080), (320, 180)] {
        for picture in [(640, 360), (360, 640), (500, 500), (64, 36)] {
            let mut request = request(0, 100, 1);
            request.video_display_size_in_pixels = video;
            request.picture_width_in_pixels = picture.0;
            request.picture_height_in_pixels = picture.1;
            let (width, height) = request.delivered_size();
            let fit = fit_inside_picture(video.0, video.1, picture.0, picture.1);
            assert_eq!(
                (width, height),
                (fit.width_in_pixels, fit.height_in_pixels),
                "{video:?} in {picture:?}"
            );
            assert!(
                width <= picture.0 && height <= picture.1,
                "{video:?} in {picture:?}: {width}x{height}"
            );
            assert!(
                filters_of(&request).contains(&format!("scale={width}:{height}:")),
                "{video:?} in {picture:?}"
            );
        }
    }
}

use crate::media::do_nothing_when_woken;

#[test]
fn a_zero_sized_picture_is_refused_before_ffmpeg_is_started() {
    let toolchain = FfmpegToolchain::unchecked_for_tests(
        PathBuf::from("no-such-ffmpeg"),
        PathBuf::from("no-such-ffprobe"),
        Some(9),
    );
    for (width, height) in [(0, 360), (640, 0), (0, 0)] {
        let mut request = request(0, 10, 1);
        request.picture_width_in_pixels = width;
        request.picture_height_in_pixels = height;
        let result = FrameStream::start(&toolchain, &request, do_nothing_when_woken());
        assert!(
            matches!(result, Err(FrameStreamError::EmptyPicture)),
            "{width}x{height}"
        );
    }
}

#[test]
fn a_picture_to_be_delivered_with_no_width_or_no_height_is_refused_before_ffmpeg_is_started() {
    let toolchain = FfmpegToolchain::unchecked_for_tests(
        PathBuf::from("no-such-ffmpeg"),
        PathBuf::from("no-such-ffprobe"),
        Some(9),
    );
    for delivered in [(0, 360), (640, 0), (0, 0)] {
        let result = FrameStream::start_delivering(
            &toolchain,
            &request(0, 10, 1),
            delivered,
            "bicubic",
            do_nothing_when_woken(),
        );
        assert!(
            matches!(result, Err(FrameStreamError::EmptyPicture)),
            "{delivered:?}: {:?}",
            result.err()
        );
    }
}

#[test]
fn a_missing_ffmpeg_is_reported_as_not_startable() {
    let toolchain = FfmpegToolchain::unchecked_for_tests(
        PathBuf::from("no-such-ffmpeg"),
        PathBuf::from("no-such-ffprobe"),
        Some(9),
    );
    let result = FrameStream::start(&toolchain, &request(0, 10, 1), do_nothing_when_woken());
    assert!(matches!(result, Err(FrameStreamError::CouldNotStart(_))));
}

#[test]
fn frames_are_read_whole_in_order_and_the_end_is_reported() {
    let frame_size = FrameSize {
        width_in_pixels: 2,
        height_in_pixels: 1,
        delivery: FrameDelivery::Rgba,
    };
    let bytes: Vec<u8> = (0..16).collect(); // two 8-byte frames
    let (sender, receiver) = mpsc::sync_channel(10);
    read_frames(&bytes[..], frame_size, &sender, &do_nothing_when_woken(), || {
        "decoder said no".to_owned()
    });
    let events: Vec<FrameStreamEvent> = receiver.try_iter().collect();
    assert_eq!(events.len(), 3);
    match (&events[0], &events[1], &events[2]) {
        (
            FrameStreamEvent::Frame(first),
            FrameStreamEvent::Frame(second),
            FrameStreamEvent::Finished { error_output },
        ) => {
            assert_eq!(first.rgba_pixels, (0..8).collect::<Vec<u8>>());
            assert_eq!(second.rgba_pixels, (8..16).collect::<Vec<u8>>());
            assert_eq!(error_output.as_deref(), Some("decoder said no"));
        }
        other => panic!("unexpected events {other:?}"),
    }
}

#[test]
fn a_partial_frame_at_the_end_of_the_data_is_dropped() {
    let frame_size = FrameSize {
        width_in_pixels: 2,
        height_in_pixels: 1,
        delivery: FrameDelivery::Rgba,
    };
    let bytes: Vec<u8> = (0..12).collect(); // one whole frame and half of another
    let (sender, receiver) = mpsc::sync_channel(10);
    read_frames(&bytes[..], frame_size, &sender, &do_nothing_when_woken(), String::new);
    let events: Vec<FrameStreamEvent> = receiver.try_iter().collect();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[1], FrameStreamEvent::Finished { error_output: None }));
}

#[test]
fn the_reader_stops_quietly_when_nobody_is_listening_any_more() {
    let frame_size = FrameSize {
        width_in_pixels: 2,
        height_in_pixels: 1,
        delivery: FrameDelivery::Rgba,
    };
    let bytes: Vec<u8> = (0..64).collect();
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    // Must return rather than block or panic.
    read_frames(&bytes[..], frame_size, &sender, &do_nothing_when_woken(), String::new);
}

fn size_in_planes(width_in_pixels: u32, height_in_pixels: u32) -> FrameSize {
    FrameSize {
        width_in_pixels,
        height_in_pixels,
        delivery: FrameDelivery::Yuv420p,
    }
}

#[test]
fn asking_for_planes_changes_nothing_in_the_command_line_but_the_pixel_format_that_the_filters_end_with() {
    let rgba_request = request(300, 300, 1);
    let planes_request = FrameStreamRequest {
        delivery: FrameDelivery::Yuv420p,
        ..request(300, 300, 1)
    };
    let rgba_arguments = arguments_as_text(&rgba_request);
    let planes_arguments = arguments_as_text(&planes_request);
    assert_eq!(rgba_arguments.len(), planes_arguments.len());
    let differences: Vec<_> = rgba_arguments
        .iter()
        .zip(&planes_arguments)
        .filter(|(rgba, planes)| rgba != planes)
        .collect();
    assert_eq!(differences.len(), 1, "{differences:?}");
    let (rgba_filters, planes_filters) = differences[0];
    assert!(
        rgba_filters.ends_with(",scale=640:360:flags=fast_bilinear,format=rgba"),
        "{rgba_filters}"
    );
    assert!(
        planes_filters.ends_with(",scale=640:360:flags=fast_bilinear,format=yuv420p"),
        "{planes_filters}"
    );
    assert_eq!(
        rgba_filters.strip_suffix("format=rgba"),
        planes_filters.strip_suffix("format=yuv420p"),
        "everything before the pixel format is the same"
    );
}

#[test]
fn a_stream_that_delivers_rgba_can_be_told_the_matrix_and_the_range_of_the_planes_it_makes_colours_of() {
    let told = |matrix, range| FrameStreamRequest {
        ffmpeg_reads_planes_as: Some(HowToReadColours { matrix, range }),
        ..request(300, 300, 1)
    };
    for (matrix, range, options) in [
        (
            Some(VideoMatrix::Bt709),
            Some(VideoRange::Limited),
            ":in_color_matrix=bt709:in_range=limited",
        ),
        (
            Some(VideoMatrix::Bt601),
            Some(VideoRange::Full),
            ":in_color_matrix=bt601:in_range=full",
        ),
        (Some(VideoMatrix::Bt2020), None, ":in_color_matrix=bt2020"),
        (None, Some(VideoRange::Full), ":in_range=full"),
        (None, None, ""),
    ] {
        let filters = filters_of(&told(matrix, range));
        assert!(
            filters.ends_with(&format!(",scale=640:360{options}:flags=fast_bilinear,format=rgba")),
            "{matrix:?} {range:?}: {filters}"
        );
    }
    assert_eq!(
        filters_of(&request(300, 300, 1)),
        filters_of(&told(None, None)),
        "told nothing is as not told"
    );
}

#[test]
fn a_stream_that_delivers_planes_is_not_told_how_to_make_colours_because_it_makes_none() {
    let planes = FrameStreamRequest {
        delivery: FrameDelivery::Yuv420p,
        ffmpeg_reads_planes_as: Some(HowToReadColours {
            matrix: Some(VideoMatrix::Bt709),
            range: Some(VideoRange::Limited),
        }),
        ..request(300, 300, 1)
    };
    let filters = filters_of(&planes);
    assert!(
        !filters.contains("in_color_matrix") && !filters.contains("in_range"),
        "{filters}"
    );
    assert!(
        filters.ends_with(",scale=640:360:flags=fast_bilinear,format=yuv420p"),
        "{filters}"
    );
}

#[test]
fn a_picture_takes_as_many_bytes_in_the_pipe_as_its_delivery_makes_it() {
    let rgba = |width_in_pixels, height_in_pixels| FrameSize {
        width_in_pixels,
        height_in_pixels,
        delivery: FrameDelivery::Rgba,
    };
    assert_eq!(rgba(3, 5).byte_count(), 60, "four bytes a pixel");
    assert_eq!(rgba(640, 360).byte_count(), 921_600);
    // A plane of brightness, and two of colour differences that have a pixel for every square of four (rounded up).
    for (width, height, bytes) in [
        (4, 2, 12),
        (2, 2, 6),
        (3, 2, 10),
        (2, 3, 10),
        (3, 3, 17),
        (5, 3, 27),
        (1, 1, 3),
        (640, 360, 345_600),
    ] {
        assert_eq!(size_in_planes(width, height).byte_count(), bytes, "{width} by {height}");
    }
}

#[test]
fn the_three_planes_of_a_picture_are_cut_out_of_its_bytes_in_order() {
    let bytes: Vec<u8> = (0..17).collect();
    let FrameStreamEvent::YuvFrame(frame) = size_in_planes(3, 3).into_event(bytes) else {
        panic!("a picture in planes was asked for");
    };
    assert_eq!((frame.width_in_pixels, frame.height_in_pixels), (3, 3));
    assert_eq!(frame.y, (0..9).collect::<Vec<u8>>());
    assert_eq!(
        frame.u,
        (9..13).collect::<Vec<u8>>(),
        "2 by 2: the last single column and row have one each"
    );
    assert_eq!(frame.v, (13..17).collect::<Vec<u8>>());
}

#[test]
fn pictures_in_planes_are_read_whole_in_order_and_a_partial_one_at_the_end_is_dropped() {
    let bytes: Vec<u8> = (0..15).collect(); // pictures of 2 by 2 take 6 bytes: two and half of a third
    let (sender, receiver) = mpsc::sync_channel(10);
    read_frames(
        &bytes[..],
        size_in_planes(2, 2),
        &sender,
        &do_nothing_when_woken(),
        String::new,
    );
    let events: Vec<FrameStreamEvent> = receiver.try_iter().collect();
    assert_eq!(events.len(), 3, "{events:?}");
    match (&events[0], &events[1], &events[2]) {
        (
            FrameStreamEvent::YuvFrame(first),
            FrameStreamEvent::YuvFrame(second),
            FrameStreamEvent::Finished { error_output: None },
        ) => {
            assert_eq!(
                (&first.y[..], &first.u[..], &first.v[..]),
                (&[0, 1, 2, 3][..], &[4][..], &[5][..])
            );
            assert_eq!(
                (&second.y[..], &second.u[..], &second.v[..]),
                (&[6, 7, 8, 9][..], &[10][..], &[11][..])
            );
        }
        other => panic!("unexpected events {other:?}"),
    }
}
