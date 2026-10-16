// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The export made by the picture engine (`video_export::VideoExport`), end to end, with the real ffmpeg and the engine on a software graphics adapter (so that nothing
//! of it runs on a real graphics card, SF-01): the frames of the finished file are the frames the project says, in the picture the preview shows.
//!
//! Every test video carries its own frame number in its picture (see `support`), so what a frame of the finished file is can be read from the file.

mod support;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fastforwardrs::engine::AdapterChoice;
use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::{Project, Settings, TextObject, TimelineFrameIndex};
use fastforwardrs::rgba_picture::RgbaPicture;
use fastforwardrs::video_export::{ExportEvent, ExportOptions, ExportResult, VideoExport, plan_export};
use support::{
    ClipOfTheVideo, TestVideoSpecification, decode_video, expected_source_frames, project_for,
    project_with_tracks_of_videos, scratch_directory, test_video,
};

use support::{one_export_at_a_time, there_is_graphics_to_export_with};

macro_rules! ready_or_skip {
    () => {{
        let toolchain = toolchain_or_skip!();
        if !there_is_graphics_to_export_with() {
            return;
        }
        toolchain
    }};
}

/// What the export did.
struct Outcome {
    result: ExportResult,
    progress: Vec<u64>,
    /// How many pictures of videos the export had to make by holding the last picture that a video gave (see `VideoExport::pictures_held`).
    pictures_held: u64,
}

const TIME_ALLOWED: Duration = Duration::from_secs(300);

fn text_in_flat_grey(_: &TextObject, width: u32, height: u32) -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: [128, 128, 128, 255].repeat((width * height) as usize),
    }
}

fn options() -> ExportOptions {
    ExportOptions::new(AdapterChoice::SoftwareOnly).with_text_pictures(text_in_flat_grey)
}

/// Exports `project` to `destination` and waits for the end.
fn export(toolchain: &FfmpegToolchain, project: &Project, destination: &Path) -> Outcome {
    let _one_at_a_time = one_export_at_a_time();
    let plan = plan_export(project, Settings::DEFAULT).expect("the project can be exported");
    let locations: Vec<_> = plan
        .source_files
        .iter()
        .map(|source| project.source(*source).unwrap().link.absolute_path.clone().into())
        .collect();
    let export =
        VideoExport::start(toolchain, &plan, &locations, destination, options(), Arc::new(|| {})).expect("it starts");
    wait_for(&export)
}

fn wait_for(export: &VideoExport) -> Outcome {
    let started = Instant::now();
    let mut progress = Vec::new();
    loop {
        match export.try_next_event() {
            Some(ExportEvent::Progress { frames_written }) => progress.push(frames_written),
            Some(ExportEvent::Finished(result)) => {
                return Outcome {
                    result,
                    progress,
                    pictures_held: export.pictures_held(),
                };
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "the export did not finish in time");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// A video of 3 seconds at 30 a second (90 pictures), 320 by 180.
const COUNTER_3_SECONDS: TestVideoSpecification = TestVideoSpecification {
    name: "counter_3_seconds",
    duration_in_seconds: 3,
    ..TestVideoSpecification::COUNTER
};

#[test]
fn a_video_exported_unedited_has_every_frame_of_the_video_in_order() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("unedited.mp4");
    let outcome = export(&toolchain, &project, &destination);
    assert_eq!(outcome.result, ExportResult::Completed);
    let exported = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(exported.frame_numbers(), expected_source_frames(&project));
    assert_eq!(outcome.progress.last(), Some(&90), "{:?}", outcome.progress);
    assert_eq!(
        outcome.pictures_held, 0,
        "every picture of the video was the video's own"
    );
}

#[test]
fn a_file_with_fewer_pictures_than_the_project_was_told_is_exported_with_its_last_picture_held_and_the_export_says_how_many_it_held()
 {
    let toolchain = ready_or_skip!();
    let short = test_video(&toolchain, &COUNTER_3_SECONDS);
    // The project was made when the file had 300 pictures; it has 90 now.
    let project =
        support::project_for_a_video_that_has_fewer_pictures_than_the_project_was_told(&toolchain, &short, 300);
    let scratch = scratch_directory();
    let destination = scratch.path().join("held.mp4");

    let outcome = export(&toolchain, &project, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    assert_eq!(
        outcome.pictures_held, 210,
        "300 frames were asked for and the file has 90 pictures"
    );
    let exported = decode_video(&toolchain, &destination, 320, 180);
    let expected: Vec<u64> = (0..90).chain(std::iter::repeat_n(89, 210)).collect();
    assert_eq!(
        exported.frame_numbers(),
        expected,
        "the last picture stands in for the ones that were not there"
    );
}

#[test]
fn a_project_of_a_single_frame_is_exported_as_a_video_of_a_single_frame() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let one_frame = ClipOfTheVideo {
        gap_before_frames: 0,
        first_source_frame: 7,
        end_source_frame: 8,
        speed: 1,
    };
    let project = project_with_tracks_of_videos(&toolchain, &[&video], &[(0, &[one_frame])]);
    assert_eq!(project.total_frame_count(), 1, "a premise");
    let scratch = scratch_directory();
    let destination = scratch.path().join("one_frame.mp4");

    let outcome = export(&toolchain, &project, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let exported = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(exported.frame_numbers(), vec![7]);
    assert_eq!(outcome.progress.last(), Some(&1), "{:?}", outcome.progress);
}

// --- how an export ends ---

/// The names of the files in `folder`.
fn files_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn start(toolchain: &FfmpegToolchain, project: &Project, destination: &Path) -> VideoExport {
    let plan = plan_export(project, Settings::DEFAULT).expect("the project can be exported");
    let locations: Vec<_> = plan
        .source_files
        .iter()
        .map(|source| project.source(*source).unwrap().link.absolute_path.clone().into())
        .collect();
    VideoExport::start(toolchain, &plan, &locations, destination, options(), Arc::new(|| {})).expect("it starts")
}

#[test]
fn a_finished_export_replaces_the_file_that_was_at_the_destination_and_leaves_nothing_else() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("video.mp4");
    std::fs::write(&destination, b"an older video").unwrap();
    let outcome = export(&toolchain, &project, &destination);
    assert_eq!(outcome.result, ExportResult::Completed);
    assert_eq!(files_in(scratch.path()), ["video.mp4"], "the temporary file is gone");
    assert_ne!(std::fs::read(&destination).unwrap(), b"an older video");
    assert_eq!(decode_video(&toolchain, &destination, 320, 180).pictures.len(), 90);
}

#[test]
fn a_cancelled_export_ends_as_cancelled_leaves_nothing_and_does_not_touch_the_file_that_was_there() {
    let toolchain = ready_or_skip!();
    let video = support::counter_video(&toolchain); // 600 frames: far more than is done by the time the first progress comes
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("video.mp4");
    std::fs::write(&destination, b"an older video").unwrap();
    let _one_at_a_time = one_export_at_a_time();
    let export = start(&toolchain, &project, &destination);
    let started = Instant::now();
    loop {
        match export.try_next_event() {
            Some(ExportEvent::Progress { .. }) => break,
            Some(ExportEvent::Finished(result)) => {
                panic!("the export finished before it could be cancelled: {result:?}")
            }
            None => {
                assert!(started.elapsed() < TIME_ALLOWED, "no progress was reported");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    export.cancel();
    let cancelled_at = Instant::now();
    let outcome = wait_for(&export);
    assert_eq!(outcome.result, ExportResult::Cancelled);
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(20),
        "cancelling takes a moment and not the rest of the export: {:?}",
        cancelled_at.elapsed()
    );
    assert_eq!(files_in(scratch.path()), ["video.mp4"]);
    assert_eq!(std::fs::read(&destination).unwrap(), b"an older video");
}

#[test]
fn an_export_that_is_dropped_is_cancelled_and_cleans_up() {
    let toolchain = ready_or_skip!();
    let video = support::counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("video.mp4");
    let _one_at_a_time = one_export_at_a_time();
    let export = start(&toolchain, &project, &destination);
    std::thread::sleep(Duration::from_millis(700));
    drop(export);
    assert_eq!(
        files_in(scratch.path()),
        Vec::<String>::new(),
        "no half-written file is left"
    );
}

#[test]
fn a_file_that_cannot_be_read_fails_the_export_with_its_name_and_leaves_nothing() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let scratch = scratch_directory();
    let copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &copy).unwrap();
    let project = project_for(
        &toolchain,
        &support::TestVideo {
            path: copy.clone(),
            specification: video.specification.clone(),
        },
    );
    std::fs::remove_file(&copy).unwrap();
    let destination = scratch.path().join("video.mp4");
    let outcome = export(&toolchain, &project, &destination);
    let ExportResult::Failed { reason } = outcome.result else {
        panic!("expected a failure, got {:?}", outcome.result);
    };
    assert!(
        reason.starts_with("holiday.mp4 cannot be read"),
        "the first words say which file: {reason}"
    );
    assert_eq!(files_in(scratch.path()), Vec::<String>::new());
}

#[test]
fn a_mistake_of_the_program_in_the_middle_of_an_export_ends_it_with_a_message_and_leaves_nothing() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let mut project = project_for(&toolchain, &video);
    let front = project.add_track_in_front();
    project
        .insert_text(front, TimelineFrameIndex::new(10), TextObject::default(), 30)
        .unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("never_made.mp4");
    let _one_at_a_time = one_export_at_a_time();
    let plan = plan_export(&project, Settings::DEFAULT).expect("the project can be exported");
    let locations: Vec<_> = plan
        .source_files
        .iter()
        .map(|source| project.source(*source).unwrap().link.absolute_path.clone().into())
        .collect();
    // The text is drawn by something that panics when it is asked: a mistake of the program, made up for the test, in the middle of the export (frame 10).
    let options = ExportOptions::new(AdapterChoice::SoftwareOnly).with_text_pictures(
        |_: &TextObject, _: u32, _: u32| -> RgbaPicture { panic!("a mistake made up by the test") },
    );
    let export =
        VideoExport::start(&toolchain, &plan, &locations, &destination, options, Arc::new(|| {})).expect("it starts");

    let outcome = wait_for(&export);

    let ExportResult::Failed { reason } = outcome.result else {
        panic!("the export did not fail: {:?}", outcome.result);
    };
    assert!(
        reason.contains("mistake in the program") && reason.contains("a mistake made up by the test"),
        "{reason}"
    );
    drop(export);
    assert_eq!(
        files_in(scratch.path()),
        Vec::<String>::new(),
        "neither the video nor the temporary file is left"
    );
}

#[test]
fn a_destination_that_cannot_be_written_fails_the_export_with_the_words_of_ffmpeg_and_leaves_nothing() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("no_such_folder").join("video.mp4");
    let outcome = export(&toolchain, &project, &destination);
    let ExportResult::Failed { reason } = outcome.result else {
        panic!("expected a failure, got {:?}", outcome.result);
    };
    assert!(!reason.is_empty());
    assert!(
        !reason.contains("pipe"),
        "ffmpeg's words are the reason, and not the way the program found out that it had stopped: {reason}"
    );
    assert!(
        reason.to_lowercase().contains("no such file")
            || reason.contains("cannot find")
            || reason.contains("could not"),
        "{reason}"
    );
    assert_eq!(files_in(scratch.path()), Vec::<String>::new());
}

#[test]
fn the_destination_cannot_be_a_file_the_project_is_made_from_and_the_number_of_files_has_to_be_right() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let source: fastforwardrs::media::MediaLocation = video.path.clone().into();
    let result = VideoExport::start(&toolchain, &plan, &[source], &video.path, options(), Arc::new(|| {}));
    assert!(
        matches!(&result, Err(fastforwardrs::video_export::ExportError::DestinationIsASourceFile { path }) if path == &video.path),
        "{:?}",
        result.err()
    );
    let result = VideoExport::start(&toolchain, &plan, &[], Path::new("x.mp4"), options(), Arc::new(|| {}));
    assert!(
        matches!(
            result,
            Err(fastforwardrs::video_export::ExportError::WrongNumberOfSourceFiles { needed: 1, given: 0 })
        ),
        "{:?}",
        result.err()
    );
}

#[test]
fn a_list_of_files_that_is_longer_than_the_plans_is_refused_as_one_that_is_shorter_is() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let source: fastforwardrs::media::MediaLocation = video.path.clone().into();
    let scratch = scratch_directory();

    let result = VideoExport::start(
        &toolchain,
        &plan,
        &[source.clone(), source],
        &scratch.path().join("never.mp4"),
        options(),
        Arc::new(|| {}),
    );

    assert!(
        matches!(
            result,
            Err(fastforwardrs::video_export::ExportError::WrongNumberOfSourceFiles { needed: 1, given: 2 })
        ),
        "{:?}",
        result.err()
    );
    assert_eq!(files_in(scratch.path()), Vec::<String>::new());
}

#[test]
fn each_source_file_of_the_plan_is_read_from_its_own_file_when_the_files_are_not_the_same() {
    let toolchain = ready_or_skip!();
    let short = test_video(&toolchain, &COUNTER_3_SECONDS);
    let long = support::counter_video(&toolchain);
    // At the back the first 30 frames of the short video (the file 1); in front, after a gap of 30 frames, 30 frames of the long video (the file 2) from its frame 300
    // on, which the short one does not have: each picture says which file it is from.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&short, &long],
        &[
            (
                1,
                &[ClipOfTheVideo {
                    gap_before_frames: 30,
                    first_source_frame: 300,
                    end_source_frame: 330,
                    speed: 1,
                }],
            ),
            (
                0,
                &[ClipOfTheVideo {
                    gap_before_frames: 0,
                    first_source_frame: 0,
                    end_source_frame: 30,
                    speed: 1,
                }],
            ),
        ],
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("two_files.mp4");

    let outcome = export(&toolchain, &project, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let exported = decode_video(&toolchain, &destination, 320, 180);
    let expected: Vec<u64> = (0..30).chain(300..330).collect();
    assert_eq!(exported.frame_numbers(), expected);
}

#[test]
fn the_window_is_woken_up_for_every_progress_that_is_told_and_once_more_at_the_end() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("woken.mp4");
    let _one_at_a_time = one_export_at_a_time();
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let locations: Vec<_> = plan
        .source_files
        .iter()
        .map(|source| project.source(*source).unwrap().link.absolute_path.clone().into())
        .collect();
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let export = VideoExport::start(
        &toolchain,
        &plan,
        &locations,
        &destination,
        options(),
        Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .expect("it starts");

    let outcome = wait_for(&export);
    drop(export); // (waits for the thread, which wakes the window as the last thing it does)

    assert_eq!(outcome.result, ExportResult::Completed);
    assert!(!outcome.progress.is_empty(), "a premise: progress was told");
    assert_eq!(woken.load(Ordering::SeqCst), outcome.progress.len() + 1);
}

#[test]
fn a_finished_video_that_cannot_be_moved_into_place_is_a_failure_that_says_so_and_leaves_what_was_there() {
    let toolchain = ready_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    // The destination is a folder that has something in it, which nothing can be moved over.
    let destination = scratch.path().join("video.mp4");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep.txt"), b"mine").unwrap();

    let outcome = export(&toolchain, &project, &destination);

    let ExportResult::Failed { reason } = outcome.result else {
        panic!("expected a failure, got {:?}", outcome.result);
    };
    assert!(reason.contains("could not be moved"), "{reason}");
    assert_eq!(
        files_in(scratch.path()),
        ["video.mp4"],
        "the finished video is not left beside it"
    );
    assert_eq!(std::fs::read(destination.join("keep.txt")).unwrap(), b"mine");
}

/// A sound file that nothing ever writes to: a named pipe, which ffmpeg waits for for ever when it opens it. (Where there are named pipes.)
#[cfg(unix)]
#[test]
fn cancelling_an_export_ends_it_though_ffmpeg_is_stuck_and_not_reading_the_frames() {
    let toolchain = ready_or_skip!();
    let project = support::project_with_a_video_and_a_tone_file(&toolchain);
    let scratch = scratch_directory();
    let stuck = scratch.path().join("stuck.wav");
    let made = std::process::Command::new("mkfifo").arg(&stuck).status();
    if !made.is_ok_and(|status| status.success()) {
        eprintln!("SKIPPED: there is no mkfifo");
        return;
    }
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    // The file of the sound (the source 2) is the one nothing writes to.
    let locations: Vec<fastforwardrs::media::MediaLocation> = plan
        .source_files
        .iter()
        .map(|source| {
            if source.value() == 2 {
                stuck.clone().into()
            } else {
                project.source(*source).unwrap().link.absolute_path.clone().into()
            }
        })
        .collect();
    let destination = scratch.path().join("video.mp4");
    let _one_at_a_time = one_export_at_a_time();
    let export =
        VideoExport::start(&toolchain, &plan, &locations, &destination, options(), Arc::new(|| {})).expect("it starts");
    // ffmpeg waits at the sound, the frames pile up in the pipe and the export waits to write the next: it has to be given time to get there.
    std::thread::sleep(Duration::from_secs(3));

    export.cancel();
    let cancelled_at = Instant::now();
    let mut result = None;
    while cancelled_at.elapsed() < Duration::from_secs(30) {
        if let Some(ExportEvent::Finished(finished)) = export.try_next_event() {
            result = Some(finished);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    // Whatever happened, ffmpeg is let go of (the pipe is opened for reading and writing at once, which does not wait for anyone, and shut again: ffmpeg's wait for
    // it ends), so that a failure of this test is a failure and not an export that never ends, which dropping it would wait for.
    drop(std::fs::OpenOptions::new().read(true).write(true).open(&stuck));
    assert_eq!(
        result,
        Some(ExportResult::Cancelled),
        "the export ended as cancelled within 30 seconds"
    );
    drop(export);
    assert_eq!(files_in(scratch.path()), ["stuck.wav"], "nothing is left but the pipe");
}

// --- the colours of the file ---

/// A project of one video, the whole of it on the timeline.
fn project_of_the_video(toolchain: &FfmpegToolchain, source: &Path) -> Project {
    let mut project = Project::empty();
    project
        .import_source(
            fastforwardrs::project::SourceMedia {
                identifier: fastforwardrs::project::SourceMediaIdentifier::new(1),
                link: fastforwardrs::project::SourceMediaLink::from_absolute_path(source.to_owned()),
                properties: fastforwardrs::media::probe_source_media(toolchain, source).unwrap(),
                colour: fastforwardrs::project::InputColourOverride::AUTOMATIC,
            },
            fastforwardrs::project::TimelineFrameIndex::new(0),
        )
        .unwrap();
    project
}

#[test]
fn the_file_says_bt709_in_the_limited_range_whatever_the_first_video_said_and_has_its_colours_within_a_few_levels_of_it()
 {
    // (Rewritten on purpose in phase 3: the file used to say what the first video said, and nothing at all when that said nothing. It says what the output colour space
    // is now, and the colours of a video that is not BT.709 are converted, so that the picture of the file is still the picture of the video.)
    use support::yuv_videos::{BT709_LIMITED, FOUR_COLOURS, SMPTE170M_LIMITED, UNTAGGED, video_made_by_geq};
    let toolchain = ready_or_skip!();
    let scratch = scratch_directory();
    // (what the video says; what the file is to say is the same for all)
    let (matrix, range) = (Some("bt709"), Some("tv"));
    let cases = [
        ("untagged", UNTAGGED),
        ("bt709", BT709_LIMITED),
        ("smpte170m", SMPTE170M_LIMITED),
    ];
    for (name, tags) in cases {
        let source = video_made_by_geq(
            &toolchain,
            scratch.path(),
            name,
            (320, 180),
            FOUR_COLOURS,
            "yuv420p",
            tags,
        );
        let project = project_of_the_video(&toolchain, &source);
        let destination = scratch.path().join(format!("{name}.mp4"));

        let outcome = export(&toolchain, &project, &destination);

        assert_eq!(outcome.result, ExportResult::Completed, "{name}");
        let json = support::probe_json(&toolchain, &destination, &["-show_streams"]);
        let video = &json["streams"][0];
        let said = |key: &str| video[key].as_str().map(str::to_owned);
        if fastforwardrs::media::export::VideoEncoder::choose(&toolchain)
            == fastforwardrs::media::export::VideoEncoder::Libx264
        {
            assert_eq!(said("color_space").as_deref(), matrix, "{name}: matrix");
            assert_eq!(said("color_range").as_deref(), range, "{name}: range");
            assert_eq!(said("color_primaries").as_deref(), Some("bt709"), "{name}: primaries");
            assert_eq!(said("color_transfer").as_deref(), Some("bt709"), "{name}: transfer");
        }
        // The colours: decoded by ffmpeg with the matrix each file says, the picture of the file is the picture of the video.
        let before = decode_video(&toolchain, &source, 320, 180);
        let after = decode_video(&toolchain, &destination, 320, 180);
        assert_eq!(before.pictures.len(), 1);
        assert_eq!(after.pictures.len(), 1, "{name}");
        let (largest, sum) = before.pictures[0]
            .iter()
            .zip(&after.pictures[0])
            .fold((0u8, 0u64), |(largest, sum), (a, b)| {
                (largest.max(a.abs_diff(*b)), sum + u64::from(a.abs_diff(*b)))
            });
        let mean = sum as f64 / before.pictures[0].len() as f64;
        eprintln!("{name}: the largest difference is {largest}, the mean is {mean:.3}");
        assert!(
            largest <= 6,
            "{name}: the largest difference between the colours is {largest}"
        );
        assert!(mean < 1.0, "{name}: the mean difference between the colours is {mean}");
    }
}

#[test]
fn a_high_definition_video_that_says_nothing_of_its_colours_is_read_as_bt709_so_it_comes_out_as_the_same_planes_that_say_so()
 {
    // (Phase 3, decision C7: it was BT.601 at every size, because that is what ffmpeg does with the colours of a file that says nothing; players use BT.709 for a picture
    // of more than 1024 across or 576 down, and so does the program now.) The same planes are made twice, once saying nothing and once saying `bt709`; ffmpeg's own
    // decoding of the second is the truth, and the export of the first has to come out as that.
    use support::yuv_videos::{BT709_LIMITED, FOUR_COLOURS, UNTAGGED, video_made_by_geq};
    let toolchain = ready_or_skip!();
    let scratch = scratch_directory();
    let size = (1280, 720);
    let says_nothing = video_made_by_geq(
        &toolchain,
        scratch.path(),
        "says_nothing",
        size,
        FOUR_COLOURS,
        "yuv420p",
        UNTAGGED,
    );
    let says_bt709 = video_made_by_geq(
        &toolchain,
        scratch.path(),
        "says_bt709",
        size,
        FOUR_COLOURS,
        "yuv420p",
        BT709_LIMITED,
    );
    let destination = scratch.path().join("high_definition.mp4");

    let outcome = export(
        &toolchain,
        &project_of_the_video(&toolchain, &says_nothing),
        &destination,
    );

    assert_eq!(outcome.result, ExportResult::Completed);
    let truth = decode_video(&toolchain, &says_bt709, size.0, size.1);
    let made = decode_video(&toolchain, &destination, size.0, size.1);
    assert_eq!((truth.pictures.len(), made.pictures.len()), (1, 1));
    let differences: Vec<u8> = truth.pictures[0]
        .iter()
        .zip(&made.pictures[0])
        .map(|(a, b)| a.abs_diff(*b))
        .collect();
    let largest = *differences.iter().max().unwrap();
    let mean = differences.iter().map(|&difference| f64::from(difference)).sum::<f64>() / differences.len() as f64;
    eprintln!("high definition, says nothing: the largest difference is {largest}, the mean is {mean:.3}");
    assert!(largest <= 6, "the largest difference between the colours is {largest}");
    assert!(mean < 1.0, "the mean difference between the colours is {mean}");
    // And the colours are not those that BT.601 would make of the same planes (the premise that the test can tell the two apart: the mean difference of the export
    // is a third of what BT.601 would be from the truth, or less).
    let wrongly_tagged = video_made_by_geq(
        &toolchain,
        scratch.path(),
        "says_bt601",
        size,
        FOUR_COLOURS,
        "yuv420p",
        support::yuv_videos::SMPTE170M_LIMITED,
    );
    let as_bt601 = decode_video(&toolchain, &wrongly_tagged, size.0, size.1);
    let differences_if_bt601: Vec<u8> = truth.pictures[0]
        .iter()
        .zip(&as_bt601.pictures[0])
        .map(|(a, b)| a.abs_diff(*b))
        .collect();
    let mean_if_bt601 = differences_if_bt601
        .iter()
        .map(|&difference| f64::from(difference))
        .sum::<f64>()
        / differences_if_bt601.len() as f64;
    eprintln!("a premise: read as BT.601 the mean difference would be {mean_if_bt601:.3}");
    assert!(
        mean_if_bt601 > 3.0 * mean,
        "a premise: BT.601 and BT.709 make colours of these planes that are {mean_if_bt601:.3} apart on average, and the export is {mean:.3} from the truth"
    );
}

#[test]
fn what_the_user_says_of_the_matrix_of_a_video_is_the_matrix_its_colours_are_made_with_in_the_export() {
    // (Phase 3, piece 3.6.) A high-definition video that says nothing is read as BT.709; the user says that it is BT.601, and the export comes out as ffmpeg's own decoding of
    // the same planes saying so. The control: the same planes, the user saying nothing, come out as BT.709 does.
    use fastforwardrs::colour_space::VideoMatrix;
    use fastforwardrs::project::{InputColourOverride, SourceMediaIdentifier};
    use support::yuv_videos::{BT709_LIMITED, FOUR_COLOURS, SMPTE170M_LIMITED, UNTAGGED, video_made_by_geq};
    let toolchain = ready_or_skip!();
    let scratch = scratch_directory();
    let size = (1280, 720);
    let says_nothing = video_made_by_geq(
        &toolchain,
        scratch.path(),
        "says_nothing",
        size,
        FOUR_COLOURS,
        "yuv420p",
        UNTAGGED,
    );
    let as_bt601 = video_made_by_geq(
        &toolchain,
        scratch.path(),
        "as_bt601",
        size,
        FOUR_COLOURS,
        "yuv420p",
        SMPTE170M_LIMITED,
    );
    let as_bt709 = video_made_by_geq(
        &toolchain,
        scratch.path(),
        "as_bt709",
        size,
        FOUR_COLOURS,
        "yuv420p",
        BT709_LIMITED,
    );
    let mean_difference = |a: &[u8], b: &[u8]| -> f64 {
        a.iter().zip(b).map(|(a, b)| f64::from(a.abs_diff(*b))).sum::<f64>() / a.len() as f64
    };

    let mut project = project_of_the_video(&toolchain, &says_nothing);
    assert!(project.set_source_colour(
        SourceMediaIdentifier::new(1),
        InputColourOverride {
            matrix: Some(VideoMatrix::Bt601),
            ..InputColourOverride::AUTOMATIC
        }
    ));
    let chosen = scratch.path().join("chosen.mp4");
    assert_eq!(export(&toolchain, &project, &chosen).result, ExportResult::Completed);
    let left_alone = scratch.path().join("left_alone.mp4");
    assert_eq!(
        export(
            &toolchain,
            &project_of_the_video(&toolchain, &says_nothing),
            &left_alone
        )
        .result,
        ExportResult::Completed
    );

    let truth_601 = decode_video(&toolchain, &as_bt601, size.0, size.1);
    let truth_709 = decode_video(&toolchain, &as_bt709, size.0, size.1);
    let chosen = decode_video(&toolchain, &chosen, size.0, size.1);
    let left_alone = decode_video(&toolchain, &left_alone, size.0, size.1);
    let (chosen_601, chosen_709) = (
        mean_difference(&chosen.pictures[0], &truth_601.pictures[0]),
        mean_difference(&chosen.pictures[0], &truth_709.pictures[0]),
    );
    let (alone_601, alone_709) = (
        mean_difference(&left_alone.pictures[0], &truth_601.pictures[0]),
        mean_difference(&left_alone.pictures[0], &truth_709.pictures[0]),
    );
    eprintln!("chosen BT.601: {chosen_601:.3} from the 601 reading, {chosen_709:.3} from the 709 one");
    eprintln!("left alone: {alone_601:.3} from the 601 reading, {alone_709:.3} from the 709 one");
    assert!(
        chosen_601 < 1.0 && chosen_709 > 3.0,
        "chosen as BT.601: {chosen_601} and {chosen_709}"
    );
    assert!(
        alone_709 < 1.0 && alone_601 > 3.0,
        "left alone, BT.709: {alone_601} and {alone_709}"
    );
}
