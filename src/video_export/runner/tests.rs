// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the export does with each thing the engine says, how it ends, and where its files go. (The export running, with the real engine and the real ffmpeg:
//! `tests/video_export_runs.rs`.)

use super::*;
use crate::engine::{FrameStatistics, RenderedFrame};

fn asked(serials_and_frames: &[(u64, u64)]) -> VecDeque<AskedFrame> {
    serials_and_frames
        .iter()
        .map(|&(serial, timeline_frame)| AskedFrame { serial, timeline_frame })
        .collect()
}

fn planes_of_nothing() -> RenderedPlanes {
    RenderedPlanes {
        width_in_pixels: 2,
        height_in_pixels: 2,
        planes: vec![1; 6],
    }
}

fn the_reason(ending: Result<Option<RenderedPlanes>, Ending>) -> String {
    match ending {
        Err(Ending::Failed(reason)) => reason,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn the_frame_that_is_next_is_given_on_and_taken_off_the_list_of_those_asked_for() {
    let mut waiting = asked(&[(7, 0), (8, 1), (9, 2)]);
    let planes = what_to_do_with(
        EngineEvent::ExportFrame {
            serial: 7,
            planes: planes_of_nothing(),
            statistics: FrameStatistics::default(),
        },
        &mut waiting,
    );
    assert_eq!(planes.unwrap(), Some(planes_of_nothing()));
    assert_eq!(waiting, asked(&[(8, 1), (9, 2)]));
}

#[test]
fn an_answer_that_is_not_for_the_frame_that_is_next_is_a_mistake_of_the_engine_and_ends_the_export() {
    for serial in [8, 9, 6, 100] {
        let mut waiting = asked(&[(7, 0), (8, 1), (9, 2)]);
        let reason = the_reason(what_to_do_with(
            EngineEvent::ExportFrame {
                serial,
                planes: planes_of_nothing(),
                statistics: FrameStatistics::default(),
            },
            &mut waiting,
        ));
        assert!(reason.contains("out of order"), "{reason}");
        assert_eq!(waiting.len(), 3, "nothing is taken off the list");
    }
    // An answer when nothing was asked for.
    let reason = the_reason(what_to_do_with(
        EngineEvent::ExportFrame {
            serial: 1,
            planes: planes_of_nothing(),
            statistics: FrameStatistics::default(),
        },
        &mut VecDeque::new(),
    ));
    assert!(reason.contains("out of order"), "{reason}");
}

#[test]
fn a_frame_that_could_not_be_made_ends_the_export_and_says_which_frame_and_why() {
    let mut waiting = asked(&[(7, 40), (8, 41)]);
    let reason = the_reason(what_to_do_with(
        EngineEvent::FrameFailed {
            serial: 8,
            problem: "the card said no".to_owned(),
        },
        &mut waiting,
    ));
    assert!(reason.contains("frame 41"), "{reason}");
    assert!(reason.contains("the card said no"), "{reason}");
    // (A serial that is not on the list is still a failure.)
    let reason = the_reason(what_to_do_with(
        EngineEvent::FrameFailed {
            serial: 99,
            problem: "x".to_owned(),
        },
        &mut waiting,
    ));
    assert!(reason.contains("a frame"), "{reason}");
}

#[test]
fn pictures_the_engine_does_not_have_end_the_export_and_say_how_many() {
    let mut waiting = asked(&[(3, 12)]);
    let reason = the_reason(what_to_do_with(
        EngineEvent::PicturesMissing {
            serial: 3,
            ids: vec![PictureId(1), PictureId(2)],
        },
        &mut waiting,
    ));
    assert!(
        reason.contains("2 of the pictures") && reason.contains("frame 12"),
        "{reason}"
    );
}

#[test]
fn a_picture_that_could_not_be_used_a_lost_device_and_a_stopped_engine_each_end_the_export_with_their_words() {
    let mut waiting = asked(&[(3, 12)]);
    let reason = the_reason(what_to_do_with(
        EngineEvent::PictureRejected {
            id: PictureId(5),
            problem: "too big for the card".to_owned(),
        },
        &mut waiting,
    ));
    assert!(reason.contains("too big for the card"), "{reason}");
    let reason = the_reason(what_to_do_with(
        EngineEvent::DeviceReplaced {
            why: "a frame did not finish in time".to_owned(),
            effects_switched_off: Vec::new(),
        },
        &mut waiting,
    ));
    assert!(
        reason.contains("lost") && reason.contains("a frame did not finish in time"),
        "{reason}"
    );
    let reason = the_reason(what_to_do_with(
        EngineEvent::Stopped {
            problem: "there is no adapter".to_owned(),
        },
        &mut waiting,
    ));
    assert!(
        reason.contains("stopped") && reason.contains("there is no adapter"),
        "{reason}"
    );
    assert_eq!(waiting.len(), 1);
}

#[test]
fn an_answer_for_the_preview_is_a_mistake_and_a_rejected_effect_is_not_the_exports_business() {
    let mut waiting = asked(&[(3, 12)]);
    let reason = the_reason(what_to_do_with(
        EngineEvent::Frame {
            serial: 3,
            frame: Arc::new(RenderedFrame {
                width_in_pixels: 1,
                height_in_pixels: 1,
                rgba_pixels: vec![0; 4],
            }),
            statistics: FrameStatistics::default(),
        },
        &mut waiting,
    ));
    assert!(reason.contains("preview"), "{reason}");
    assert_eq!(
        what_to_do_with(
            EngineEvent::EffectRejected {
                key: "x".to_owned(),
                problem: "y".to_owned()
            },
            &mut waiting
        )
        .unwrap(),
        None
    );
}

#[test]
fn a_cancel_is_an_ending_of_its_own_and_every_other_problem_is_a_failure_with_words() {
    assert_eq!(Ending::from(RunPictureProblem::Cancelled), Ending::Cancelled);
    assert_eq!(
        Ending::from(FrameProblem::Picture(RunPictureProblem::Cancelled)),
        Ending::Cancelled
    );
    let Ending::Failed(reason) = Ending::from(RunPictureProblem::NoPicture {
        video: "holiday.mp4".to_owned(),
        ffmpeg_said: Some("Invalid data".to_owned()),
    }) else {
        panic!("a failure");
    };
    assert!(
        reason.contains("holiday.mp4") && reason.contains("Invalid data"),
        "{reason}"
    );
    let Ending::Failed(reason) = Ending::from(FrameProblem::NotOnTheTimeline(9)) else {
        panic!("a failure");
    };
    assert!(reason.contains('9'), "{reason}");
}

// --- the files ---

#[test]
fn the_video_is_written_beside_the_destination_with_a_name_that_keeps_the_kind_of_file() {
    assert_eq!(
        temporary_output_path(Path::new("folder/video.mp4")),
        PathBuf::from("folder/video.exporting.mp4")
    );
    assert_eq!(
        temporary_output_path(Path::new("video.mkv")),
        PathBuf::from("video.exporting.mkv")
    );
    assert_eq!(
        temporary_output_path(Path::new("video")),
        PathBuf::from("video.exporting")
    );
    assert_eq!(
        temporary_output_path(Path::new("a.b.mov")),
        PathBuf::from("a.b.exporting.mov")
    );
}

#[test]
fn a_destination_is_the_same_file_as_a_source_however_it_is_spelled() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("holiday.mp4");
    fs::write(&source, b"x").unwrap();
    assert!(is_the_same_file(&source, &source));
    assert!(is_the_same_file(
        &source,
        &folder
            .path()
            .join("..")
            .join(folder.path().file_name().unwrap())
            .join("holiday.mp4")
    ));
    assert!(!is_the_same_file(&source, &folder.path().join("other.mp4")));
    assert!(!is_the_same_file(
        &folder.path().join("missing.mp4"),
        &folder.path().join("missing.mp4")
    ));
    // A destination that does not exist yet is the same as a source only if it would be at its place.
    let not_yet = folder.path().join("not_yet.mp4");
    assert!(!is_the_same_file(&source, &not_yet));
}

#[test]
fn a_file_is_called_by_its_name_in_a_message() {
    assert_eq!(name_of(&MediaLocation::from("some/folder/holiday.mp4")), "holiday.mp4");
}
