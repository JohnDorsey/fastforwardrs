// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::borrow::Cow;

use super::*;
use crate::media::FrameDelivery;
use crate::playback::scripted_streams::{Script, ScriptedStarter};
use crate::project::TrackId;

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

fn supplier() -> (PictureSupplier<ScriptedStarter>, Script) {
    supplier_of_a_video_that_is(None, false)
}

/// A supplier of the pictures of one video, which is stored as `format` says (if anyone knows), and for a preview that wants pictures in planes
/// or does not.
fn supplier_of_a_video_that_is(
    format: Option<VideoStreamFormat>,
    planes_wanted: bool,
) -> (PictureSupplier<ScriptedStarter>, Script) {
    supplier_of_a_video_that_is_said_to_be(format, planes_wanted, InputColourOverride::AUTOMATIC)
}

/// [`supplier_of_a_video_that_is`] when the user says that the colours of the video are `said`.
fn supplier_of_a_video_that_is_said_to_be(
    format: Option<VideoStreamFormat>,
    planes_wanted: bool,
    said: InputColourOverride,
) -> (PictureSupplier<ScriptedStarter>, Script) {
    let script = Script::default();
    let mut supplier = PictureSupplier::new(ScriptedStarter(script.clone()));
    supplier.set_setup(Some(PreviewSetup {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        picture_width_in_pixels: 64,
        picture_height_in_pixels: 36,
        videos: vec![PreviewVideo {
            source: VIDEO,
            location: MediaLocation::from("holiday.mp4"),
            display_size_in_pixels: (1280, 720),
            frame_rate: FrameRate::new(25, 1).unwrap(),
            format,
            colour: said,
        }],
        planes_wanted,
    }));
    (supplier, script)
}

/// What ffprobe says of an ordinary video: 8 bits to a number, 4:2:0.
fn eight_bit_four_two_zero(colour_space: Option<&str>, colour_range: Option<&str>) -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        colour_space: colour_space.map(str::to_owned),
        colour_range: colour_range.map(str::to_owned),
        ..VideoStreamFormat::default()
    }
}

/// A picture wanted from a clip that ends at source frame 200, with plenty of pictures left.
fn wanted(source_frame: u64) -> WantedPicture {
    wanted_with(source_frame, 1, 200, 100, None)
}

fn wanted_with(
    source_frame: u64,
    speed: u32,
    clip_end: u64,
    pictures_left: u64,
    next_video: Option<UpcomingVideo>,
) -> WantedPicture {
    WantedPicture {
        track: TrackId::new(0),
        source: VIDEO,
        source_frame: SourceFrameIndex::new(source_frame),
        speed_factor: SpeedFactor::new(speed).unwrap(),
        range_end_source_frame: SourceFrameIndex::new(clip_end),
        pictures_left,
        next_video,
    }
}

fn upcoming(first: u64, end: u64, speed: u32) -> UpcomingVideo {
    UpcomingVideo {
        source: VIDEO,
        first_source_frame: SourceFrameIndex::new(first),
        end_source_frame: SourceFrameIndex::new(end),
        speed_factor: SpeedFactor::new(speed).unwrap(),
    }
}

fn shown_frame_number(supplier: &PictureSupplier<ScriptedStarter>) -> Option<u8> {
    supplier.picture().map(|picture| picture.frame.rgba().rgba_pixels[0])
}

fn after(start: Instant, milliseconds: u64) -> Instant {
    start + Duration::from_millis(milliseconds)
}

#[test]
fn the_first_request_starts_a_stream_at_the_wanted_picture_that_runs_to_the_end_of_the_clip() {
    let (mut supplier, script) = supplier();
    let report = supplier.supply(Some(wanted(50)), false, Instant::now());

    assert_eq!(
        report,
        SupplyReport {
            shows_the_wanted_picture: false,
            ask_again_in: None
        }
    );
    assert_eq!(
        script.requests(),
        vec![FrameStreamRequest {
            video: MediaLocation::from("holiday.mp4"),
            project_frame_rate: FrameRate::new(30, 1).unwrap(),
            video_frame_rate: FrameRate::new(25, 1).unwrap(),
            first_source_frame: SourceFrameIndex::new(50),
            source_frame_count: 150,
            speed_factor: SpeedFactor::NORMAL,
            video_display_size_in_pixels: (1280, 720),
            picture_width_in_pixels: 64,
            picture_height_in_pixels: 36,
            delivery: FrameDelivery::Rgba,
        }]
    );
}

#[test]
fn a_picture_that_arrives_is_shown() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), false, start);
    script.deliver(0, 1);

    let report = supplier.supply(Some(wanted(50)), false, after(start, 10));

    assert!(report.shows_the_wanted_picture);
    assert_eq!(shown_frame_number(&supplier), Some(50));
    assert_eq!(supplier.picture().unwrap().source_frame, SourceFrameIndex::new(50));
}

#[test]
fn playing_forward_keeps_reading_the_same_stream() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 3);
    for (step, frame) in [50u64, 51, 52].into_iter().enumerate() {
        let report = supplier.supply(Some(wanted(frame)), true, after(start, 33 * (step as u64 + 1)));
        assert!(report.shows_the_wanted_picture, "frame {frame}");
        assert_eq!(shown_frame_number(&supplier), Some(frame as u8));
    }
    assert_eq!(script.started_count(), 1);
}

#[test]
fn when_the_decoder_is_ahead_the_wanted_picture_is_shown_and_older_ones_are_skipped() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 6); // pictures 50..=55 are ready

    supplier.supply(Some(wanted(53)), true, after(start, 100));
    assert_eq!(
        shown_frame_number(&supplier),
        Some(53),
        "pictures 50, 51 and 52 were skipped"
    );
    supplier.supply(Some(wanted(54)), true, after(start, 133));
    assert_eq!(shown_frame_number(&supplier), Some(54));
    assert_eq!(script.started_count(), 1);
}

#[test]
fn when_the_decoder_is_a_little_behind_the_last_picture_stays_and_nothing_restarts() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 2); // only 50 and 51

    let report = supplier.supply(Some(wanted(55)), true, after(start, 100));

    assert!(!report.shows_the_wanted_picture);
    assert_eq!(
        shown_frame_number(&supplier),
        Some(51),
        "the latest picture there is stays on show"
    );
    assert_eq!(script.started_count(), 1);

    script.deliver(0, 10);
    assert!(
        supplier
            .supply(Some(wanted(55)), true, after(start, 133))
            .shows_the_wanted_picture
    );
    assert_eq!(shown_frame_number(&supplier), Some(55));
}

#[test]
fn a_decoder_that_is_hopelessly_behind_is_skipped_past_with_a_new_stream() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), true, after(start, 10));

    // Playback is now 40 pictures ahead of what the decoder has delivered.
    supplier.supply(Some(wanted(90)), true, after(start, 1000));

    assert_eq!(script.started_count(), 2);
    assert_eq!(script.requests()[1].first_source_frame, SourceFrameIndex::new(90));
    assert_eq!(script.requests()[1].source_frame_count, 110);
}

#[test]
fn going_backwards_starts_a_new_stream_at_the_earlier_picture() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), false, start);
    script.deliver(0, 11);
    supplier.supply(Some(wanted(60)), false, after(start, 100));
    assert_eq!(shown_frame_number(&supplier), Some(60));

    supplier.supply(Some(wanted(40)), false, after(start, 500));

    assert_eq!(script.started_count(), 2);
    assert_eq!(script.requests()[1].first_source_frame, SourceFrameIndex::new(40));
}

#[test]
fn asking_for_the_picture_already_shown_needs_nothing_at_all() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), false, start);
    script.deliver(0, 5);
    supplier.supply(Some(wanted(50)), false, after(start, 10));
    let serial_number = supplier.picture().unwrap().serial_number;

    for step in 1..=5 {
        let report = supplier.supply(Some(wanted(50)), false, after(start, 10 + step * 100));
        assert!(report.shows_the_wanted_picture);
    }

    assert_eq!(script.started_count(), 1);
    assert_eq!(
        supplier.picture().unwrap().serial_number,
        serial_number,
        "no new picture was taken"
    );
}

#[test]
fn every_new_picture_gets_a_new_serial_number() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 3);
    let mut serial_numbers = Vec::new();
    for (step, frame) in [50u64, 51, 52].into_iter().enumerate() {
        supplier.supply(Some(wanted(frame)), true, after(start, 33 * (step as u64 + 1)));
        serial_numbers.push(supplier.picture().unwrap().serial_number);
    }
    assert!(
        serial_numbers.windows(2).all(|pair| pair[0] < pair[1]),
        "{serial_numbers:?}"
    );
}

#[test]
fn streams_are_not_started_more_often_than_the_minimum_interval_and_the_supplier_says_when_to_ask_again() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(10)), false, start);
    assert_eq!(script.started_count(), 1);

    // A jump 10 ms later, which the first stream is nowhere near delivering.
    let report = supplier.supply(Some(wanted(150)), false, after(start, 10));
    assert_eq!(script.started_count(), 1, "too soon after the last start");
    assert!(!report.shows_the_wanted_picture);
    assert_eq!(
        report.ask_again_in,
        Some(MINIMUM_TIME_BETWEEN_STREAM_STARTS - Duration::from_millis(10))
    );

    // Asked again when it said to: now it starts.
    let report = supplier.supply(Some(wanted(150)), false, after(start, 50));
    assert_eq!(script.started_count(), 2);
    assert_eq!(report.ask_again_in, None);
    assert_eq!(script.requests()[1].first_source_frame, SourceFrameIndex::new(150));
}

#[test]
fn dragging_through_many_pictures_quickly_starts_far_fewer_streams_than_requests() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    // A slider dragged across 190 pictures at one request per millisecond (200 ms in all).
    for step in 0..190u64 {
        supplier.supply(Some(wanted(5 + step)), false, after(start, step));
    }
    assert!(
        script.started_count() <= 6,
        "{} streams started",
        script.started_count()
    );
    assert!(script.started_count() >= 1);
}

#[test]
fn a_different_speed_needs_a_different_stream() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(100)), false, start);
    script.deliver(0, 5);
    supplier.supply(Some(wanted(100)), false, after(start, 10));

    supplier.supply(Some(wanted_with(104, 4, 200, 25, None)), false, after(start, 200));

    assert_eq!(script.started_count(), 2);
    assert_eq!(script.requests()[1].speed_factor, SpeedFactor::new(4).unwrap());
    assert_eq!(script.requests()[1].first_source_frame, SourceFrameIndex::new(104));
}

#[test]
fn a_picture_that_is_not_one_the_stream_would_deliver_needs_a_new_stream() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    // A 4x stream from frame 100 delivers frames 100, 104, 108, ...
    supplier.supply(Some(wanted_with(100, 4, 200, 25, None)), false, start);
    script.deliver(0, 5);
    supplier.supply(Some(wanted_with(100, 4, 200, 25, None)), false, after(start, 10));

    supplier.supply(Some(wanted_with(106, 4, 200, 23, None)), false, after(start, 200));

    assert_eq!(script.started_count(), 2, "106 is not 100 plus a multiple of 4");
    assert_eq!(script.requests()[1].first_source_frame, SourceFrameIndex::new(106));
}

#[test]
fn a_sped_up_stream_delivers_every_nth_picture_to_playback() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted_with(100, 4, 200, 25, None)), true, start);
    script.deliver(0, 4); // frames 100, 104, 108, 112
    for (step, frame) in [100u64, 104, 108, 112].into_iter().enumerate() {
        supplier.supply(
            Some(wanted_with(frame, 4, 200, 25 - step as u64, None)),
            true,
            after(start, 33 * (step as u64 + 1)),
        );
        assert_eq!(shown_frame_number(&supplier), Some(frame as u8));
    }
    assert_eq!(script.started_count(), 1);
}

#[test]
fn a_stream_started_before_a_split_carries_on_across_the_split_without_restarting() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    // The stream was started when the clip was [50, 200). The clip has since been split at 150, so
    // the picture now wanted belongs to a clip [150, 200) with the same speed.
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 101); // frames 50 to 150

    let report = supplier.supply(Some(wanted_with(150, 1, 200, 50, None)), true, after(start, 200));

    assert!(report.shows_the_wanted_picture);
    assert_eq!(shown_frame_number(&supplier), Some(150));
    assert_eq!(script.started_count(), 1);
}

#[test]
fn near_the_end_of_a_clip_while_playing_the_next_video_is_got_ready() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    let next = Some(upcoming(200, 400, 4));
    supplier.supply(Some(wanted_with(179, 1, 200, 21, next)), true, start);
    assert_eq!(script.started_count(), 1, "21 pictures left is still too early");

    supplier.supply(Some(wanted_with(180, 1, 200, 20, next)), true, after(start, 33));

    assert_eq!(script.started_count(), 2);
    let request = &script.requests()[1];
    assert_eq!(request.first_source_frame, SourceFrameIndex::new(200));
    assert_eq!(request.source_frame_count, 200);
    assert_eq!(request.speed_factor, SpeedFactor::new(4).unwrap());
}

#[test]
fn the_next_video_is_got_ready_only_once() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    let next = Some(upcoming(200, 400, 4));
    for step in 0..10u64 {
        supplier.supply(
            Some(wanted_with(180 + step, 1, 200, 20 - step, next)),
            true,
            after(start, 33 * step),
        );
    }
    assert_eq!(script.started_count(), 2);
}

#[test]
fn crossing_into_the_next_video_uses_the_stream_that_was_got_ready() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    let next = Some(upcoming(200, 400, 4));
    supplier.supply(Some(wanted_with(180, 1, 200, 20, next)), true, start);
    script.deliver(0, 20);
    script.deliver(1, 3); // frames 200, 204, 208 of the next clip are ready
    supplier.supply(Some(wanted_with(199, 1, 200, 1, next)), true, after(start, 600));
    assert_eq!(shown_frame_number(&supplier), Some(199));

    let report = supplier.supply(Some(wanted_with(200, 4, 400, 50, None)), true, after(start, 633));

    assert!(
        report.shows_the_wanted_picture,
        "the first picture of the next clip was already there"
    );
    assert_eq!(shown_frame_number(&supplier), Some(200));
    assert_eq!(script.started_count(), 2, "no new stream was needed");
}

#[test]
fn a_next_video_the_current_stream_already_covers_is_not_started_separately() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    // The stream runs from 50 to 200, and the clip after the one being shown starts at 150 with the
    // same speed (the clip was split after the stream was started).
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 100);
    supplier.supply(
        Some(wanted_with(140, 1, 150, 10, Some(upcoming(150, 200, 1)))),
        true,
        after(start, 200),
    );
    assert_eq!(script.started_count(), 1);
}

#[test]
fn pausing_lets_go_of_the_stream_that_was_got_ready_and_playing_again_gets_it_ready_afresh() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    let next = Some(upcoming(200, 400, 4));
    supplier.supply(Some(wanted_with(180, 1, 200, 20, next)), true, start);
    assert_eq!(script.started_count(), 2);

    supplier.supply(Some(wanted_with(180, 1, 200, 20, next)), false, after(start, 100));
    supplier.supply(Some(wanted_with(181, 1, 200, 19, next)), true, after(start, 200));

    assert_eq!(script.started_count(), 3);
}

#[test]
fn paused_playback_never_gets_the_next_video_ready() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    let next = Some(upcoming(200, 400, 4));
    supplier.supply(Some(wanted_with(199, 1, 200, 1, next)), false, start);
    assert_eq!(script.started_count(), 1);
}

#[test]
fn a_stream_that_ends_before_the_wanted_picture_is_not_restarted_over_and_over() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), false, start);
    script.end(0, Some("could not read the file"));

    for step in 1..=5 {
        let report = supplier.supply(Some(wanted(50)), false, after(start, 1000 * step));
        assert!(!report.shows_the_wanted_picture);
    }

    assert_eq!(script.started_count(), 1);
    assert_eq!(supplier.last_problem(), Some("could not read the file"));

    // A different picture is worth another try.
    supplier.supply(Some(wanted(60)), false, after(start, 10_000));
    assert_eq!(script.started_count(), 2);
}

#[test]
fn a_stream_that_cannot_be_started_is_reported_and_not_retried_for_the_same_picture() {
    let (mut supplier, script) = supplier();
    script.make_starting_fail();
    let start = Instant::now();

    for step in 0..4 {
        supplier.supply(Some(wanted(50)), false, after(start, 1000 * step));
    }

    assert_eq!(script.start_attempts(), 1);
    assert!(supplier.last_problem().is_some_and(|problem| !problem.is_empty()));
}

#[test]
fn with_nothing_wanted_the_streams_are_dropped_but_the_last_picture_stays() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), false, start);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, after(start, 10));

    let report = supplier.supply(None, false, after(start, 20));

    assert!(!report.shows_the_wanted_picture);
    assert_eq!(shown_frame_number(&supplier), Some(50));
    // Wanting the picture that is still on show needs no stream.
    assert!(
        supplier
            .supply(Some(wanted(50)), false, after(start, 500))
            .shows_the_wanted_picture
    );
    assert_eq!(script.started_count(), 1);
}

#[test]
fn a_picture_of_a_video_the_preview_does_not_show_is_not_supplied() {
    let (mut supplier, script) = supplier();
    let mut other_video = wanted(50);
    other_video.source = SourceMediaIdentifier::new(2);
    let report = supplier.supply(Some(other_video), false, Instant::now());
    assert!(!report.shows_the_wanted_picture);
    assert_eq!(script.started_count(), 0);
}

#[test]
fn changing_or_removing_the_source_forgets_everything() {
    let (mut supplier, script) = supplier();
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), false, start);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, after(start, 10));
    assert!(supplier.picture().is_some());

    supplier.set_setup(None);

    assert!(supplier.picture().is_none());
    assert!(supplier.setup().is_none());
    let report = supplier.supply(Some(wanted(50)), false, after(start, 1000));
    assert!(!report.shows_the_wanted_picture);
    assert_eq!(script.started_count(), 1, "nothing was started without a source");
}

#[test]
fn pictures_are_asked_for_in_planes_only_for_a_video_that_is_stored_that_way_and_only_when_planes_are_wanted() {
    let stored_another_way = |pixel_format: &str| VideoStreamFormat {
        pixel_format: Some(pixel_format.to_owned()),
        ..VideoStreamFormat::default()
    };
    for (format, planes_wanted, expected) in [
        (Some(eight_bit_four_two_zero(None, None)), true, FrameDelivery::Yuv420p),
        (
            Some(eight_bit_four_two_zero(Some("bt709"), Some("tv"))),
            true,
            FrameDelivery::Yuv420p,
        ),
        (Some(eight_bit_four_two_zero(None, None)), false, FrameDelivery::Rgba),
        (
            Some(eight_bit_four_two_zero(Some("bt709"), Some("pc"))),
            false,
            FrameDelivery::Rgba,
        ),
        (None, true, FrameDelivery::Rgba),
        (None, false, FrameDelivery::Rgba),
        (Some(stored_another_way("yuv422p")), true, FrameDelivery::Rgba),
        (Some(stored_another_way("yuv420p10le")), true, FrameDelivery::Rgba),
        (Some(stored_another_way("rgb24")), true, FrameDelivery::Rgba),
        (
            Some(eight_bit_four_two_zero(Some("bt2020c"), None)),
            true,
            FrameDelivery::Rgba,
        ),
    ] {
        let (mut supplier, script) = supplier_of_a_video_that_is(format.clone(), planes_wanted);
        supplier.supply(Some(wanted(50)), false, Instant::now());
        assert_eq!(
            script.requests()[0].delivery,
            expected,
            "the format {format:?} when planes are wanted: {planes_wanted}"
        );
    }
}

#[test]
fn a_picture_in_planes_is_supplied_with_the_colour_its_video_says_it_has() {
    for (colour_space, colour_range, matrix, full_range) in [
        // (Rewritten on purpose in phase 3: this video is 1280 by 720, and a file that says nothing is BT.709 when its picture is high definition. It was BT.601.)
        (None, None, VideoMatrix::Bt709, false),
        (Some("bt709"), Some("tv"), VideoMatrix::Bt709, false),
        (Some("bt709"), Some("pc"), VideoMatrix::Bt709, true),
        (Some("bt2020nc"), None, VideoMatrix::Bt2020, false),
        (Some("smpte170m"), Some("pc"), VideoMatrix::Bt601, true),
    ] {
        let (mut supplier, script) =
            supplier_of_a_video_that_is(Some(eight_bit_four_two_zero(colour_space, colour_range)), true);
        let now = Instant::now();
        supplier.supply(Some(wanted(50)), false, now);
        script.deliver(0, 1);
        let report = supplier.supply(Some(wanted(50)), false, now);

        assert!(report.shows_the_wanted_picture);
        let picture = supplier.picture().expect("the picture arrived");
        match &picture.frame {
            SuppliedFrame::Yuv420p { frame, colour } => {
                assert_eq!(
                    frame.y,
                    vec![50],
                    "the brightness is the frame number the script gave it"
                );
                assert_eq!(
                    *colour,
                    NativeYuv420p { matrix, full_range },
                    "{colour_space:?} {colour_range:?}"
                );
            }
            other => panic!("the picture is not in planes: {other:?}"),
        }
        assert_eq!(picture.source_frame, SourceFrameIndex::new(50));
    }
}

/// What ffprobe says of a video that says what the arguments say of its primaries and its transfer function (and nothing of its matrix).
fn eight_bit_four_two_zero_with(primaries: Option<&str>, transfer: Option<&str>) -> VideoStreamFormat {
    VideoStreamFormat {
        colour_primaries: primaries.map(str::to_owned),
        colour_transfer: transfer.map(str::to_owned),
        ..eight_bit_four_two_zero(Some("bt709"), None)
    }
}

#[test]
fn every_picture_comes_with_the_primaries_and_the_curve_its_video_says_or_the_ones_that_are_assumed_for_it() {
    use crate::colour_space::{StandardPrimaries, TransferFunction};
    use crate::engine::PictureColour;
    for (format, planes_wanted, expected) in [
        (
            eight_bit_four_two_zero_with(Some("bt709"), Some("bt709")),
            true,
            PictureColour::SRGB,
        ),
        (
            eight_bit_four_two_zero_with(Some("bt2020"), Some("bt2020-10")),
            true,
            PictureColour {
                primaries: StandardPrimaries::Rec2020,
                curve: TransferFunction::Srgb,
            },
        ),
        (
            eight_bit_four_two_zero_with(Some("smpte432"), Some("linear")),
            true,
            PictureColour {
                primaries: StandardPrimaries::DisplayP3,
                curve: TransferFunction::Linear,
            },
        ),
        // (A picture that ffmpeg made RGBA of is the colour its file says as well: ffmpeg does not change primaries or curves.)
        (
            eight_bit_four_two_zero_with(Some("smpte170m"), Some("smpte170m")),
            false,
            PictureColour {
                primaries: StandardPrimaries::Bt601Ntsc,
                curve: TransferFunction::Srgb,
            },
        ),
        (eight_bit_four_two_zero_with(None, None), true, PictureColour::SRGB),
    ] {
        let (mut supplier, script) = supplier_of_a_video_that_is(Some(format.clone()), planes_wanted);
        let now = Instant::now();
        supplier.supply(Some(wanted(50)), false, now);
        script.deliver(0, 1);
        supplier.supply(Some(wanted(50)), false, now);
        assert_eq!(
            supplier.picture().expect("the picture arrived").colour,
            expected,
            "{format:?}, planes wanted: {planes_wanted}"
        );
    }
}

#[test]
fn a_picture_of_a_video_whose_format_is_not_known_yet_is_srgb() {
    let (mut supplier, script) = supplier_of_a_video_that_is(None, true);
    let now = Instant::now();
    supplier.supply(Some(wanted(50)), false, now);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, now);
    assert_eq!(
        supplier.picture().expect("the picture arrived").colour,
        crate::engine::PictureColour::SRGB
    );
}

#[test]
fn a_stream_of_planes_goes_on_like_any_other_and_shows_the_picture_that_was_wanted_of_it() {
    let (mut supplier, script) = supplier_of_a_video_that_is(Some(eight_bit_four_two_zero(None, None)), true);
    let start = Instant::now();
    supplier.supply(Some(wanted(50)), true, start);
    script.deliver(0, 5);
    supplier.supply(Some(wanted(52)), true, start);
    assert_eq!(script.started_count(), 1, "the same stream goes on");
    let picture = supplier.picture().unwrap();
    let SuppliedFrame::Yuv420p { frame, .. } = &picture.frame else {
        panic!("not in planes");
    };
    assert_eq!(frame.y, vec![52]);
}

#[test]
fn a_picture_in_planes_can_be_had_as_rgba_and_one_that_is_rgba_is_not_copied_to_be_had_so() {
    // Y 81, U 90, V 240 is pure red in BT.601 limited range (the numbers the standard gives).
    let red = SuppliedFrame::Yuv420p {
        frame: DecodedYuvFrame {
            width_in_pixels: 2,
            height_in_pixels: 2,
            y: vec![81; 4],
            u: vec![90],
            v: vec![240],
        },
        colour: NativeYuv420p {
            matrix: VideoMatrix::Bt601,
            full_range: false,
        },
    };
    let as_rgba = red.rgba();
    assert!(matches!(as_rgba, Cow::Owned(_)));
    assert_eq!((as_rgba.width_in_pixels, as_rgba.height_in_pixels), (2, 2));
    assert_eq!(as_rgba.rgba_pixels.len(), 16);
    for pixel in as_rgba.rgba_pixels.as_chunks::<4>().0 {
        assert!(
            pixel[0] >= 254 && pixel[1] <= 2 && pixel[2] <= 2 && pixel[3] == 255,
            "{pixel:?}"
        );
    }
    assert_eq!(red.size_in_pixels(), (2, 2));

    let already = SuppliedFrame::from(DecodedFrame {
        width_in_pixels: 1,
        height_in_pixels: 1,
        rgba_pixels: vec![1, 2, 3, 4],
    });
    assert!(matches!(already.rgba(), Cow::Borrowed(_)));
    assert_eq!(already.rgba().rgba_pixels, vec![1, 2, 3, 4]);
    assert_eq!(already.size_in_pixels(), (1, 1));
}

#[test]
fn a_picture_in_planes_of_an_odd_size_can_be_had_as_rgba() {
    for (width, height) in [(1, 1), (3, 1), (1, 3), (3, 3), (5, 3)] {
        let (chroma_width, chroma_height) = (usize::div_ceil(width, 2), usize::div_ceil(height, 2));
        let picture = SuppliedFrame::Yuv420p {
            frame: DecodedYuvFrame {
                width_in_pixels: width as u32,
                height_in_pixels: height as u32,
                y: vec![128; width * height],
                u: vec![128; chroma_width * chroma_height],
                v: vec![128; chroma_width * chroma_height],
            },
            colour: NativeYuv420p {
                matrix: VideoMatrix::Bt709,
                full_range: true,
            },
        };
        let rgba = picture.rgba();
        assert_eq!(rgba.rgba_pixels.len(), width * height * 4, "{width}x{height}");
        // Full range grey: 128 in all three planes is a grey of 128.
        for pixel in rgba.rgba_pixels.as_chunks::<4>().0 {
            assert_eq!(*pixel, [128, 128, 128, 255], "{width}x{height}");
        }
    }
}

#[test]
fn every_matrix_and_range_a_video_can_say_is_the_same_one_for_the_engine() {
    for (native, expected) in [
        (VideoMatrix::Bt601, YuvMatrix::Bt601),
        (VideoMatrix::Bt709, YuvMatrix::Bt709),
        (VideoMatrix::Bt2020, YuvMatrix::Bt2020),
    ] {
        for (full_range, range) in [(false, YuvRange::Limited), (true, YuvRange::Full)] {
            assert_eq!(
                colour_for_the_engine(NativeYuv420p {
                    matrix: native,
                    full_range
                }),
                YuvColour {
                    matrix: expected,
                    range
                }
            );
        }
    }
}

#[test]
fn what_the_user_says_of_the_colours_of_a_video_is_what_its_pictures_come_with_whatever_the_file_says() {
    use crate::colour_space::{StandardPrimaries, TransferFunction, VideoRange};
    use crate::engine::PictureColour;
    let said = InputColourOverride {
        matrix: Some(VideoMatrix::Bt601),
        range: Some(VideoRange::Full),
        primaries: Some(StandardPrimaries::DisplayP3),
        curve: Some(TransferFunction::Gamma22),
    };
    let chosen_codes = PictureColour {
        primaries: StandardPrimaries::DisplayP3,
        curve: TransferFunction::Gamma22,
    };
    // In planes: the matrix and the range that the planes are made into colours with are the chosen ones, and so are the primaries and the curve of the colours.
    let (mut supplier, script) =
        supplier_of_a_video_that_is_said_to_be(Some(eight_bit_four_two_zero(Some("bt709"), Some("tv"))), true, said);
    let now = Instant::now();
    supplier.supply(Some(wanted(50)), false, now);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, now);
    let picture = supplier.picture().expect("the picture arrived");
    match &picture.frame {
        SuppliedFrame::Yuv420p { colour, .. } => assert_eq!(
            *colour,
            NativeYuv420p {
                matrix: VideoMatrix::Bt601,
                full_range: true
            }
        ),
        other => panic!("the picture is not in planes: {other:?}"),
    }
    assert_eq!(picture.colour, chosen_codes);

    // Not in planes (ffmpeg made the colours): the primaries and the curve are still the chosen ones.
    let (mut supplier, script) =
        supplier_of_a_video_that_is_said_to_be(Some(eight_bit_four_two_zero(Some("bt709"), Some("tv"))), false, said);
    supplier.supply(Some(wanted(50)), false, now);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, now);
    assert_eq!(supplier.picture().expect("the picture arrived").colour, chosen_codes);

    // Before the format of the file is known: only the primaries and the curve that the user chose can be used, over what is assumed of a file that says nothing.
    let (mut supplier, script) = supplier_of_a_video_that_is_said_to_be(None, false, said);
    supplier.supply(Some(wanted(50)), false, now);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, now);
    assert_eq!(supplier.picture().expect("the picture arrived").colour, chosen_codes);

    // And what is not said is as it was: the same video, the user saying nothing.
    let (mut supplier, script) = supplier_of_a_video_that_is_said_to_be(
        Some(eight_bit_four_two_zero(Some("bt709"), Some("tv"))),
        true,
        InputColourOverride::AUTOMATIC,
    );
    supplier.supply(Some(wanted(50)), false, now);
    script.deliver(0, 1);
    supplier.supply(Some(wanted(50)), false, now);
    assert_eq!(
        supplier.picture().expect("the picture arrived").colour,
        PictureColour::SRGB
    );
}
