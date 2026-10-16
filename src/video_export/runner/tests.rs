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

// --- how it ends, from what ffmpeg said ---

/// The status that a program that has ended with `code` has.
fn an_exit_status(code: i32) -> std::process::ExitStatus {
    let mut program = if cfg!(windows) {
        let mut program = std::process::Command::new("cmd");
        program.args(["/C", &format!("exit {code}")]);
        program
    } else {
        let mut program = std::process::Command::new("sh");
        program.args(["-c", &format!("exit {code}")]);
        program
    };
    program.status().expect("a program that ends with a status can be run")
}

#[test]
fn when_the_encoder_is_stopped_a_failure_has_what_ffmpeg_said_added_and_a_cancel_stays_a_cancel() {
    assert_eq!(
        the_ending_with_what_ffmpeg_said(Ending::Cancelled, "ffmpeg complained about being killed"),
        Ending::Cancelled
    );
    assert_eq!(
        the_ending_with_what_ffmpeg_said(Ending::failed("the frame 7 could not be made"), ""),
        Ending::failed("the frame 7 could not be made")
    );
    assert_eq!(
        the_ending_with_what_ffmpeg_said(Ending::failed("the frame 7 could not be made"), "Error opening output"),
        Ending::failed("the frame 7 could not be made: Error opening output")
    );
}

#[test]
fn when_a_frame_could_not_be_given_to_the_encoder_ffmpegs_words_are_the_reason_and_the_broken_pipe_only_when_it_said_nothing()
 {
    let broken = io::Error::new(io::ErrorKind::BrokenPipe, "the pipe is broken");
    assert_eq!(
        the_ending_when_a_frame_could_not_be_given(&broken, "Unable to open the output".to_owned()),
        Ending::failed("Unable to open the output"),
        "(and not a word of the pipe)"
    );
    assert_eq!(
        the_ending_when_a_frame_could_not_be_given(&broken, String::new()),
        Ending::failed("the video could not be written: the pipe is broken")
    );
}

#[test]
fn an_export_is_well_ended_when_ffmpeg_says_it_finished_and_otherwise_ffmpegs_words_are_the_reason() {
    let said = |status: io::Result<std::process::ExitStatus>, words: &str| {
        the_ending_of_the_file(EncoderEnd {
            status,
            error_output: words.to_owned(),
        })
    };
    assert_eq!(said(Ok(an_exit_status(0)), ""), Ok(()));
    assert_eq!(
        said(Ok(an_exit_status(0)), "a warning"),
        Ok(()),
        "what it says is not the reason when it finished"
    );
    assert_eq!(
        said(Ok(an_exit_status(1)), "Conversion failed!"),
        Err(Ending::failed("Conversion failed!"))
    );
    let Err(Ending::Failed(reason)) = said(Ok(an_exit_status(3)), "") else {
        panic!("a failure that said nothing is still a failure");
    };
    assert!(reason.starts_with("ffmpeg stopped with"), "{reason}");
    let Err(Ending::Failed(reason)) = said(Err(io::Error::other("no such process")), "") else {
        panic!("a process that cannot be waited for is a failure");
    };
    assert_eq!(reason, "ffmpeg could not be waited for: no such process");
}

// --- a mistake of the program ---

#[test]
fn what_a_panic_said_is_told_in_its_words_whether_it_was_a_text_a_formatted_text_or_not_words_at_all() {
    let said = |payload: Box<dyn std::any::Any + Send>| what_a_panic_said(payload.as_ref());
    assert_eq!(said(Box::new("a text")), "a text");
    assert_eq!(said(Box::new(format!("a formatted {}", 7))), "a formatted 7");
    assert_eq!(said(Box::new(7_u32)), "it did not say what");
    // (What `panic!` makes of its arguments: a literal is a `&str` and a formatted message is a `String`.)
    let literal = std::panic::catch_unwind(|| panic!("a literal")).unwrap_err();
    let formatted = std::panic::catch_unwind(|| panic!("a formatted {}", 7)).unwrap_err();
    assert_eq!(said(literal), "a literal");
    assert_eq!(said(formatted), "a formatted 7");
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

#[test]
fn nothing_is_said_when_no_picture_was_held() {
    assert_eq!(super::words_about_held_pictures(0), None);
}

#[test]
fn the_words_about_held_pictures_give_the_number_and_say_what_stood_in_for_them_and_why_it_may_be() {
    let one = super::words_about_held_pictures(1).expect("one picture held is told");
    assert!(one.starts_with("1 picture of a video was not there"), "{one}");
    let many = super::words_about_held_pictures(210).expect("210 pictures held are told");
    assert!(many.starts_with("210 pictures of videos were not there"), "{many}");
    for words in [&one, &many] {
        assert!(
            words.contains("last picture"),
            "it says what stands in for them: {words}"
        );
        assert!(words.contains("fewer pictures"), "it says what was found: {words}");
        assert!(words.contains("damaged"), "it says what may be the reason: {words}");
    }
}

// --- how the colours of the videos are read ---

fn a_plan_of_two_videos(colour_of_the_second: crate::project::InputColourOverride) -> ExportPlan {
    use crate::project::{Settings, TimelineFrameIndex};
    use crate::video_export::plan_export;
    use crate::video_export::test_projects::{project_with_video, video_source};
    let mut project = project_with_video(1920, 1080, false);
    project
        .import_source(video_source(2, 320, 180, 100, false), TimelineFrameIndex::new(300))
        .unwrap();
    project.set_source_colour(SourceMediaIdentifier::new(2), colour_of_the_second);
    plan_export(&project, Settings::DEFAULT).unwrap()
}

fn formats_of_both(
    first: VideoStreamFormat,
    second: VideoStreamFormat,
) -> HashMap<SourceMediaIdentifier, VideoStreamFormat> {
    HashMap::from([
        (SourceMediaIdentifier::new(1), first),
        (SourceMediaIdentifier::new(2), second),
    ])
}

fn eight_bit_four_two_zero() -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        ..VideoStreamFormat::default()
    }
}

#[test]
fn the_export_wants_the_planes_of_a_video_that_is_stored_as_the_engine_can_take_them_and_not_of_one_that_is_not() {
    let plan = a_plan_of_two_videos(crate::project::InputColourOverride::AUTOMATIC);
    let other_way = VideoStreamFormat {
        pixel_format: Some("yuv444p".to_owned()),
        ..VideoStreamFormat::default()
    };

    let readings = readings_of_the_videos(&plan, &formats_of_both(eight_bit_four_two_zero(), other_way));

    assert!(
        readings[&SourceMediaIdentifier::new(1)].planes.is_some(),
        "the video that is 8 bits, 4:2:0, is given to the engine as ffmpeg decodes it"
    );
    assert_eq!(readings[&SourceMediaIdentifier::new(1)].ffmpeg_is_told, None);
    assert_eq!(readings[&SourceMediaIdentifier::new(2)].planes, None);
    assert!(
        readings[&SourceMediaIdentifier::new(2)].ffmpeg_is_told.is_some(),
        "the other is made into colours by ffmpeg, which is told how"
    );
}

#[test]
fn each_video_of_the_export_is_read_with_the_size_of_its_own_picture_and_what_the_user_says_of_it() {
    use crate::colour_space::{StandardPrimaries, VideoMatrix};
    let said = crate::project::InputColourOverride {
        matrix: Some(VideoMatrix::Bt2020),
        primaries: Some(StandardPrimaries::DisplayP3),
        ..crate::project::InputColourOverride::AUTOMATIC
    };
    let plan = a_plan_of_two_videos(said);

    let readings = readings_of_the_videos(
        &plan,
        &formats_of_both(eight_bit_four_two_zero(), eight_bit_four_two_zero()),
    );

    // The first is 1920 by 1080 and the user said nothing of it: BT.709 by its size. The second is 320 by 180 and the user said BT.2020 and the primaries of P3.
    assert_eq!(
        readings[&SourceMediaIdentifier::new(1)]
            .planes
            .map(|planes| planes.matrix),
        Some(VideoMatrix::Bt709)
    );
    assert_eq!(
        readings[&SourceMediaIdentifier::new(2)]
            .planes
            .map(|planes| planes.matrix),
        Some(VideoMatrix::Bt2020)
    );
    assert_eq!(
        readings[&SourceMediaIdentifier::new(2)].picture_colour.primaries,
        StandardPrimaries::DisplayP3
    );
    assert_eq!(
        readings[&SourceMediaIdentifier::new(1)].picture_colour,
        crate::engine::PictureColour::SRGB
    );
}
