// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures of the runs, with streams that are made up (no ffmpeg): when each run is started, what it is asked for, what is made of what comes, and what is done
//! when it comes late, too soon, or not at all. (With the real ffmpeg: `tests/video_export_pictures.rs`.)

use super::*;
use crate::colour_space::VideoMatrix;
use crate::project::{CanvasArea, FrameRate, Placement, Project, Settings};
use crate::video_export::plan_export;
use crate::video_export::test_projects::{
    clip_location, frame, project_with_video, speed, video_source, video_source_at,
};
use crate::video_export::test_streams::{Script, locations, planes, rgba, the_first_byte_of};
use std::collections::VecDeque;

fn plan_of(project: &Project) -> ExportPlan {
    plan_export(project, Settings::DEFAULT).unwrap()
}

fn nobody_cancels() -> AtomicBool {
    AtomicBool::new(false)
}

/// A project of two videos: a long one (300 frames), and one of 100 frames from frame 200 on a track in front.
fn two_videos() -> (ExportPlan, Script) {
    let mut project = project_with_video(320, 180, false);
    project
        .import_source(video_source(2, 320, 180, 100, false), frame(200))
        .unwrap();
    let plan = plan_of(&project);
    // What the tests that use it count on (the short video is opaque and fills the picture, so the long one is not a layer of the picture behind it, and its
    // run ends where the short one begins).
    assert_eq!(
        plan.runs
            .iter()
            .map(|run| (run.source, run.first_timeline_frame, run.end_timeline_frame))
            .collect::<Vec<_>>(),
        vec![
            (SourceMediaIdentifier::new(1), 0, 200),
            (SourceMediaIdentifier::new(2), 200, 300)
        ]
    );
    (plan, Script::default())
}

/// A project of one video of 300 frames that is cut at `cut` (and not changed otherwise): two runs, one for each of the clips that the cut makes.
fn one_video_cut_at(cut: u64) -> ExportPlan {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(cut), None);
    let plan = plan_of(&project);
    assert_eq!(
        plan.runs
            .iter()
            .map(|run| (run.first_timeline_frame, run.end_timeline_frame, run.first_source_frame))
            .collect::<Vec<_>>(),
        vec![(0, cut, 0), (cut, 300, cut)]
    );
    plan
}

/// A project of one video of 300 frames that is cut at frame 100, where the second part plays at 4x: two runs, 0..100 at 1x and 100..150 at 4x.
fn one_video_cut_where_it_speeds_up() -> ExportPlan {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    let plan = plan_of(&project);
    assert_eq!(
        plan.runs
            .iter()
            .map(|run| (run.first_timeline_frame, run.end_timeline_frame, run.first_source_frame))
            .collect::<Vec<_>>(),
        vec![(0, 100, 0), (100, 150, 100)]
    );
    plan
}

#[test]
fn a_run_is_started_a_little_before_its_first_frame_and_only_once() {
    let (plan, script) = two_videos();
    let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
    assert_eq!(
        script.state().started.len(),
        0,
        "nothing is started before it is asked for"
    );
    pictures.start_the_runs_that_begin_soon(0).unwrap();
    assert_eq!(
        script.state().started.len(),
        1,
        "the first video; the second is 200 frames away"
    );
    pictures.start_the_runs_that_begin_soon(0).unwrap();
    pictures.start_the_runs_that_begin_soon(10).unwrap();
    assert_eq!(script.state().started.len(), 1, "and it is not started again");
    // The second run begins at frame 200: it is started when it is FRAMES_AHEAD_THAT_A_RUN_IS_STARTED frames away.
    pictures
        .start_the_runs_that_begin_soon(200 - FRAMES_AHEAD_THAT_A_RUN_IS_STARTED - 1)
        .unwrap();
    assert_eq!(script.state().started.len(), 1);
    pictures
        .start_the_runs_that_begin_soon(200 - FRAMES_AHEAD_THAT_A_RUN_IS_STARTED)
        .unwrap();
    assert_eq!(script.state().started.len(), 2);
    let started = script.state().started.clone();
    assert_eq!(started[0].0.video, MediaLocation::from("holiday.mp4"));
    assert_eq!(started[1].0.video, MediaLocation::from("birthday.mp4"));
}

#[test]
fn a_run_that_has_ended_is_not_started() {
    let plan = one_video_cut_where_it_speeds_up();
    let script = Script::default();
    let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
    // Frame 100 is the first frame after the first run (0..100) and the first of the second (100..150); and so is every frame after it.
    for timeline_frame in [100, 120] {
        let script = Script::default();
        let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
        pictures.start_the_runs_that_begin_soon(timeline_frame).unwrap();
        let started = script.state().started.clone();
        assert_eq!(started.len(), 1, "at frame {timeline_frame}");
        assert_eq!(
            started[0].0.first_source_frame.value(),
            100,
            "the second run, not the first, at frame {timeline_frame}"
        );
    }
    // And the run that is over at frame 150 (the end of the project) is not started at all, nor are the ones after the end.
    pictures.start_the_runs_that_begin_soon(150).unwrap();
    assert_eq!(script.state().started.len(), 0);
}

#[test]
fn the_request_of_a_run_says_which_source_frames_at_what_speed_and_how_big_the_pictures_are() {
    let plan = one_video_cut_where_it_speeds_up();
    let script = Script::default();
    let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
    pictures.start_the_runs_that_begin_soon(99).unwrap();
    let started = script.state().started.clone();
    assert_eq!(
        started.len(),
        2,
        "the first run is at frame 99, and the second is one frame away"
    );
    let (request, delivered_size) = &started[1];
    assert_eq!(request.first_source_frame.value(), 100);
    assert_eq!(request.source_frame_count, 200);
    assert_eq!(request.speed_factor, speed(4));
    assert_eq!(request.project_frame_rate, plan.frame_rate);
    assert_eq!(request.video_frame_rate, FrameRate::new(30, 1).unwrap());
    assert_eq!(request.video_display_size_in_pixels, (320, 180));
    assert_eq!(
        (request.picture_width_in_pixels, request.picture_height_in_pixels),
        (320, 180)
    );
    assert_eq!(
        request.delivery,
        FrameDelivery::Rgba,
        "no planes were offered for this video"
    );
    assert_eq!(*delivered_size, (320, 180));
}

#[test]
fn the_request_has_the_frame_rate_of_the_video_and_the_frame_rate_of_the_project_and_they_are_not_the_same_thing() {
    // A project of 30 frames a second (chosen first), and a video of 25 a second in it: 75 pictures are 90 frames.
    let mut project = Project::empty();
    project.set_frame_rate(FrameRate::new(30, 1).unwrap());
    project
        .import_source(
            video_source_at(1, 320, 180, 75, FrameRate::new(25, 1).unwrap(), false),
            frame(0),
        )
        .unwrap();
    let plan = plan_of(&project);
    assert_eq!(plan.total_frame_count, 90);
    let script = Script::default();
    let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
    pictures.start_the_runs_that_begin_soon(0).unwrap();
    let started = script.state().started.clone();
    assert_eq!(started[0].0.video_frame_rate, FrameRate::new(25, 1).unwrap());
    assert_eq!(started[0].0.project_frame_rate, FrameRate::new(30, 1).unwrap());
}

#[test]
fn a_video_in_a_smaller_place_is_asked_for_as_big_as_the_place_while_the_request_still_says_how_big_the_picture_is() {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, 640, 360, 300, false), frame(0))
        .unwrap();
    let clip = clip_location(&project, 0, 0);
    project
        .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.0, 0.0, 0.25, 0.25)))
        .unwrap();
    let plan = plan_of(&project);
    let script = Script::default();
    let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
    pictures.start_the_runs_that_begin_soon(0).unwrap();
    let started = script.state().started.clone();
    let (request, delivered_size) = &started[0];
    assert_eq!(*delivered_size, (160, 90));
    assert_eq!(request.video_display_size_in_pixels, (640, 360));
    assert_eq!(
        (request.picture_width_in_pixels, request.picture_height_in_pixels),
        (640, 360),
        "the size of the finished picture, which is not the size the video is delivered at"
    );
}

#[test]
fn a_video_that_can_be_had_in_planes_is_asked_for_in_planes_and_the_pictures_say_what_they_stand_for() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().to_say.push_back(vec![planes(10), planes(20)]);
    let native = NativeYuv420p {
        matrix: VideoMatrix::Bt709,
        full_range: false,
    };
    let mut pictures = PicturesOfTheRuns::new(
        script.clone(),
        &plan,
        &locations(),
        &HashMap::from([(SourceMediaIdentifier::new(1), native)]),
    )
    .unwrap();
    let picture = pictures.picture(0, 0, &nobody_cancels()).unwrap();
    assert_eq!(script.state().started[0].0.delivery, FrameDelivery::Yuv420p);
    let SuppliedFrame::Yuv420p { colour, .. } = &picture.frame else {
        panic!("{picture:?}")
    };
    assert_eq!(*colour, native);
}

#[test]
fn the_pictures_of_a_run_come_in_order_with_the_source_frames_of_the_run_and_a_serial_number_each_that_no_other_has() {
    let plan = one_video_cut_where_it_speeds_up();
    let script = Script::default();
    script.state().to_say = VecDeque::from([
        (0..100).map(rgba).collect(),
        (0..50).map(|number| rgba(number as u8 + 100)).collect(),
    ]);
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    let mut serials = Vec::new();
    for timeline_frame in 0..100 {
        let picture = pictures.picture(0, timeline_frame, &cancel).unwrap();
        assert_eq!(the_first_byte_of(&picture), timeline_frame as u8);
        assert_eq!(picture.source, SourceMediaIdentifier::new(1));
        assert_eq!(picture.source_frame.value(), timeline_frame);
        serials.push(picture.serial_number);
    }
    for timeline_frame in 100..150 {
        let picture = pictures.picture(1, timeline_frame, &cancel).unwrap();
        assert_eq!(the_first_byte_of(&picture), timeline_frame as u8);
        assert_eq!(picture.source_frame.value(), 100 + (timeline_frame - 100) * 4, "at 4x");
        serials.push(picture.serial_number);
    }
    let mut sorted = serials.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 150, "every picture has a serial number of its own");
    assert!(serials.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(pictures.pictures_held(), 0);
}

#[test]
fn the_same_frame_asked_for_again_is_the_very_same_picture_also_after_the_last() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().to_say.push_back((0..=255).map(rgba).collect::<Vec<_>>());
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    let first = pictures.picture(0, 0, &cancel).unwrap();
    assert!(Arc::ptr_eq(&first, &pictures.picture(0, 0, &cancel).unwrap()));
    for timeline_frame in 1..299 {
        pictures.picture(0, timeline_frame, &cancel).unwrap();
    }
    let last = pictures.picture(0, 299, &cancel);
    assert!(
        last.is_ok(),
        "the run has 300 pictures and the stream had 256: the last stands in"
    );
    let last = last.unwrap();
    assert!(Arc::ptr_eq(&last, &pictures.picture(0, 299, &cancel).unwrap()));
}

#[test]
fn a_frame_that_is_not_asked_for_is_passed_over_and_the_run_goes_on_with_the_frame_after() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().to_say.push_back((0..20).map(rgba).collect());
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    assert_eq!(the_first_byte_of(&pictures.picture(0, 0, &cancel).unwrap()), 0);
    let skipped_to = pictures.picture(0, 5, &cancel).unwrap();
    assert_eq!(the_first_byte_of(&skipped_to), 5);
    assert_eq!(skipped_to.source_frame.value(), 5);
    assert_eq!(the_first_byte_of(&pictures.picture(0, 6, &cancel).unwrap()), 6);
    assert_eq!(pictures.pictures_held(), 0);
}

#[test]
fn frames_that_are_not_asked_for_after_the_last_picture_of_a_stream_are_counted_as_held_too() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().to_say.push_back((0..3).map(rgba).collect());
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    assert_eq!(the_first_byte_of(&pictures.picture(0, 0, &cancel).unwrap()), 0);
    // Frames 3 to 7 have no picture, though only frame 7 is asked for.
    assert_eq!(the_first_byte_of(&pictures.picture(0, 7, &cancel).unwrap()), 2);
    assert_eq!(pictures.pictures_held(), 5);
}

#[test]
fn a_stream_that_ends_before_its_run_does_has_its_last_picture_held_for_the_rest_and_the_holding_is_counted() {
    let plan = one_video_cut_at(10);
    let script = Script::default();
    script.state().to_say = VecDeque::from([vec![rgba(1), rgba(2), rgba(3)], vec![rgba(9)]]);
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    let bytes: Vec<u8> = (0..10)
        .map(|timeline_frame| the_first_byte_of(&pictures.picture(0, timeline_frame, &cancel).unwrap()))
        .collect();
    assert_eq!(bytes, vec![1, 2, 3, 3, 3, 3, 3, 3, 3, 3]);
    assert_eq!(pictures.pictures_held(), 7);
    // Another run is not touched by it.
    assert_eq!(the_first_byte_of(&pictures.picture(1, 10, &cancel).unwrap()), 9);
    assert_eq!(the_first_byte_of(&pictures.picture(1, 11, &cancel).unwrap()), 9);
    assert_eq!(pictures.pictures_held(), 7 + 1);
}

#[test]
fn a_stream_with_no_picture_at_all_is_a_mistake_and_says_what_ffmpeg_said() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().to_say.push_back(vec![FrameStreamEvent::Finished {
        error_output: Some("Invalid data found when processing input".to_owned()),
    }]);
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let problem = pictures.picture(0, 0, &nobody_cancels()).unwrap_err();
    assert!(
        matches!(&problem, RunPictureProblem::NoPicture { video, ffmpeg_said: Some(words) }
        if video == "holiday.mp4" && words.contains("Invalid data")),
        "{problem:?}"
    );
    let words = problem.to_string();
    assert!(
        words.contains("holiday.mp4") && words.contains("Invalid data found"),
        "{words}"
    );
}

#[test]
fn a_stream_that_ends_with_nothing_said_and_no_picture_is_still_a_mistake() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script
        .state()
        .to_say
        .push_back(vec![FrameStreamEvent::Finished { error_output: None }]);
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let problem = pictures.picture(0, 0, &nobody_cancels()).unwrap_err();
    assert!(
        matches!(&problem, RunPictureProblem::NoPicture { ffmpeg_said: None, .. }),
        "{problem:?}"
    );
    assert_eq!(problem.to_string(), "ffmpeg gave no picture of holiday.mp4");
}

#[test]
fn a_stream_that_vanishes_without_saying_it_has_ended_is_a_mistake_too() {
    // The thread that reads the pictures can end without a word if it is stopped from outside; what is left to say is that the reading stopped.
    let plan = plan_of(&project_with_video(320, 180, false));
    let mut pictures = PicturesOfTheRuns::new(Script::default(), &plan, &locations(), &HashMap::new()).unwrap();
    let problem = pictures.picture(0, 0, &nobody_cancels()).unwrap_err();
    assert!(
        matches!(&problem, RunPictureProblem::NoPicture { ffmpeg_said: Some(words), .. } if words.contains("stopped")),
        "{problem:?}"
    );
}

#[test]
fn a_stream_that_vanishes_after_some_pictures_holds_the_last_like_one_that_says_it_has_ended() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().to_say.push_back(vec![rgba(5)]);
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    assert_eq!(the_first_byte_of(&pictures.picture(0, 0, &cancel).unwrap()), 5);
    assert_eq!(the_first_byte_of(&pictures.picture(0, 1, &cancel).unwrap()), 5);
    assert_eq!(pictures.pictures_held(), 1);
}

#[test]
fn a_run_that_cannot_be_started_is_a_mistake_that_says_which_video() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().fail_to_start = true;
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    let problem = pictures.start_the_runs_that_begin_soon(0).unwrap_err();
    assert!(
        matches!(&problem, RunPictureProblem::CouldNotStart { video, .. } if video == "holiday.mp4"),
        "{problem:?}"
    );
    let problem = pictures.picture(0, 0, &nobody_cancels()).unwrap_err();
    assert!(
        matches!(problem, RunPictureProblem::CouldNotStart { .. }),
        "{problem:?}"
    );
}

#[test]
fn a_video_with_no_known_place_or_no_details_cannot_be_read_and_the_words_say_so() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let problem = PicturesOfTheRuns::new(Script::default(), &plan, &HashMap::new(), &HashMap::new())
        .err()
        .unwrap();
    assert!(
        matches!(&problem, RunPictureProblem::CouldNotStart { why, .. } if why.contains("does not know where")),
        "{problem:?}"
    );
}

#[test]
fn a_picture_that_does_not_come_in_time_is_a_mistake() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().never_ends = true;
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    pictures.wait_for_a_picture_no_longer_than(Duration::from_millis(30));
    let started = Instant::now();
    let problem = pictures.picture(0, 0, &nobody_cancels()).unwrap_err();
    assert!(
        matches!(&problem, RunPictureProblem::TookTooLong { limit, .. } if *limit == Duration::from_millis(30)),
        "{problem:?}"
    );
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(30),
        "it waits for as long as the limit, and not less: {waited:?}"
    );
    assert!(waited < Duration::from_secs(5), "and not for ever: {waited:?}");
}

#[test]
fn cancelling_ends_the_wait_for_a_picture_at_once() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().never_ends = true;
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    // A short limit, so that a wait that did not look at the cancel would be seen to end the other way (and a test of the code that does not look would not
    // take two minutes).
    pictures.wait_for_a_picture_no_longer_than(Duration::from_millis(200));
    let cancelled = AtomicBool::new(true);
    let problem = pictures.picture(0, 0, &cancelled).unwrap_err();
    assert!(matches!(problem, RunPictureProblem::Cancelled), "{problem:?}");
}

#[test]
fn a_cancel_that_comes_while_a_picture_is_awaited_ends_the_wait() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().never_ends = true;
    let mut pictures = PicturesOfTheRuns::new(script, &plan, &locations(), &HashMap::new()).unwrap();
    pictures.wait_for_a_picture_no_longer_than(Duration::from_secs(60));
    let cancelled = AtomicBool::new(false);
    let started = Instant::now();
    let problem = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            cancelled.store(true, Ordering::SeqCst);
        });
        pictures.picture(0, 0, &cancelled).unwrap_err()
    });
    assert!(matches!(problem, RunPictureProblem::Cancelled), "{problem:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?} is not the 60 s of the limit",
        started.elapsed()
    );
}

#[test]
fn the_stream_of_a_run_is_let_go_of_with_its_last_picture_and_not_before() {
    let plan = one_video_cut_at(3);
    let script = Script::default();
    script.state().to_say = VecDeque::from([(0..3).map(rgba).collect(), (0..3).map(rgba).collect()]);
    script.state().never_ends = true;
    let mut pictures = PicturesOfTheRuns::new(script.clone(), &plan, &locations(), &HashMap::new()).unwrap();
    let cancel = nobody_cancels();
    pictures.start_the_runs_that_begin_soon(0).unwrap();
    assert_eq!(script.state().alive, 2);
    pictures.picture(0, 0, &cancel).unwrap();
    pictures.picture(0, 1, &cancel).unwrap();
    assert_eq!(script.state().alive, 2, "the run has a picture left");
    pictures.picture(0, 2, &cancel).unwrap();
    assert_eq!(script.state().alive, 1, "its last was taken");
    drop(pictures);
    assert_eq!(script.state().alive, 0, "and the rest go with the pictures");
}
