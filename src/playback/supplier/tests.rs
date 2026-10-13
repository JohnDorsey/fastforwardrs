use super::*;
use crate::playback::scripted_streams::{Script, ScriptedStarter};
use crate::project::TrackId;

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

fn supplier() -> (PictureSupplier<ScriptedStarter>, Script) {
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
        }],
    }));
    (supplier, script)
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
    supplier.picture().map(|picture| picture.frame.rgba_pixels[0])
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
