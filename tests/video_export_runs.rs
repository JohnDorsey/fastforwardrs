// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The export made by the picture engine (`video_export::VideoExport`), end to end, with the real ffmpeg and the engine on a software graphics adapter (so that nothing
//! of it runs on a real graphics card, SF-01): the frames of the finished file are the frames the project says, in the picture the preview shows.
//!
//! Every test video carries its own frame number in its picture (see `support`), so what a frame of the finished file is can be read from the file.

mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::engine::AdapterChoice;
use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::{Project, Settings, TextObject};
use fastforwardrs::rgba_picture::RgbaPicture;
use fastforwardrs::video_export::{ExportEvent, ExportOptions, ExportResult, VideoExport, plan_export};
use support::{
    TestVideoSpecification, decode_video, expected_source_frames, project_for, scratch_directory, test_video,
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
            Some(ExportEvent::Finished(result)) => return Outcome { result, progress },
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
    assert!(reason.contains("holiday.mp4"), "{reason}");
    assert_eq!(files_in(scratch.path()), Vec::<String>::new());
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
