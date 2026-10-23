// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The thread that describes the frames of a lead-up (EN-14): the frames it describes and in which order, the pictures it sends and lets go of, what it makes the pictures of the lead-up of, and how it
//! ends (to its end, with a failure, or cancelled). The streams of the videos are made up (no ffmpeg); the engine that takes what the thread gives is a model of what it needs, which says when a frame
//! is given before a picture it is made of. (That the states that such frames make are the ones the preview goes on from is for `engine::worker::lead_up` and the tests of the window.)

use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::*;
use crate::engine::{FrameColour, FrameDescription};
use crate::project::{Project, Settings, TextObject};
use crate::video_export::plan_export;
use crate::video_export::test_projects::{frame, picture_source, project_with_video};
use crate::video_export::test_streams::{Script, locations, rgba};

const A_LONG_TIME: Duration = Duration::from_secs(60);

/// A video of 300 frames at the back, a text over it from frame 50 to frame 80 (on a track of its own in front), and a picture file in front of that: the frames of the project.
fn a_video_with_a_text_and_a_picture_over_it() -> ExportPlan {
    let mut project: Project = project_with_video(320, 180, false);
    let front = project.add_track_in_front();
    project
        .insert_text(
            front,
            frame(50),
            TextObject {
                text: "hello".to_owned(),
                ..TextObject::default()
            },
            30,
        )
        .unwrap();
    project.import_source(picture_source(2, 100, 50), frame(200)).unwrap();
    let mut plan = plan_export(&project, Settings::DEFAULT).unwrap();
    plan.colour = FrameColour::default();
    plan
}

/// A script that gives 255 pictures to each stream that is started.
fn script_of_pictures() -> Script {
    let script = Script::default();
    for _ in 0..8 {
        script.state().to_say.push_back((0..=254).map(rgba).collect::<Vec<_>>());
    }
    script
}

fn a_picture_file() -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: 8,
        height_in_pixels: 4,
        rgba_pixels: vec![99; 8 * 4 * 4],
    }
}

/// How the frame on the screen is described: **a size of the frame and a colour that are not those of the plan nor the size of the pictures of the preview**, so that a describer that does not do as it is told
/// shows.
fn how_the_preview_describes() -> HowTheFramesAreDescribed {
    HowTheFramesAreDescribed {
        picture_size: (320, 180),
        render_size: (96, 54),
        colour: FrameColour {
            working: crate::engine::WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709),
            ..FrameColour::default()
        },
    }
}

fn made_from(plan: ExportPlan, frames: Range<u64>, script: &Script) -> WhatALeadUpIsMadeFrom<Script> {
    WhatALeadUpIsMadeFrom {
        plan,
        frames,
        how: how_the_preview_describes(),
        preview_picture_size: (160, 90),
        locations: locations(),
        planes: HashMap::new(),
        readings: HashMap::new(),
        images: HashMap::from([(SourceMediaIdentifier::new(2), a_picture_file())]),
        starter: script.clone(),
        text: Box::new(|| {
            Box::new(|_: &crate::project::TextObject, _: u32, _: u32| RgbaPicture {
                width_in_pixels: 4,
                height_in_pixels: 2,
                rgba_pixels: vec![200; 32],
            })
        }),
    }
}

/// Everything that the thread gives, until it has nothing more to give.
fn everything_given_by(started: &StartedLeadUp) -> Vec<LeadUpItem> {
    started.items.iter().collect()
}

fn wait_until_over(running: &RunningLeadUp) {
    let started = Instant::now();
    while !running.is_over() {
        assert!(started.elapsed() < A_LONG_TIME, "the thread did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn the_frames_in(items: &[LeadUpItem]) -> Vec<&FrameDescription> {
    items
        .iter()
        .filter_map(|item| match item {
            LeadUpItem::Frame(description) => Some(description),
            _ => None,
        })
        .collect()
}

#[test]
fn without_ffmpeg_no_run_of_a_video_is_started_and_the_words_say_why() {
    let starter = PreviewRunStreamStarter::new(None);
    let request = FrameStreamRequest {
        video: MediaLocation::from("holiday.mp4"),
        project_frame_rate: crate::project::FrameRate::new(30, 1).unwrap(),
        video_frame_rate: crate::project::FrameRate::new(30, 1).unwrap(),
        first_source_frame: crate::project::SourceFrameIndex::new(0),
        source_frame_count: 10,
        speed_factor: crate::project::SpeedFactor::new(1).unwrap(),
        video_display_size_in_pixels: (320, 180),
        picture_width_in_pixels: 160,
        picture_height_in_pixels: 90,
        delivery: crate::media::FrameDelivery::Rgba,
        ffmpeg_reads_planes_as: None,
    };
    let Err(error) = starter.start(&request, (160, 90)) else {
        panic!("a stream was started without ffmpeg");
    };
    assert!(error.to_string().contains("ffmpeg was not found"), "{error}");
}

#[test]
fn the_frames_of_a_lead_up_to_a_frame_are_as_many_before_it_as_a_state_has_gone_on_for_but_no_more_than_the_most() {
    assert_eq!(frames_of_the_lead_up_to(1000, 50), 950..1000);
    assert_eq!(frames_of_the_lead_up_to(1000, 300), 700..1000);
    assert_eq!(
        frames_of_the_lead_up_to(1000, 5000),
        700..1000,
        "a state that has gone on for longer is made from the last 300"
    );
    assert_eq!(
        frames_of_the_lead_up_to(100, 5000),
        0..100,
        "and none are before the first frame of the project"
    );
    assert!(
        frames_of_the_lead_up_to(1000, 0).is_empty(),
        "an effect at its first frame has nothing before it"
    );
    assert!(frames_of_the_lead_up_to(0, 0).is_empty());
    assert_eq!(
        MOST_FRAMES_OF_A_LEAD_UP, 300,
        "the length that the engine says a picture is not exact for"
    );
}

#[test]
fn the_frames_of_a_chase_start_where_those_of_a_lead_up_do_and_go_on_to_the_end_of_the_project() {
    assert_eq!(frames_of_a_chase_from(1000, 50, 5000), 950..5000);
    assert_eq!(
        frames_of_a_chase_from(1000, 5000, 5000),
        700..5000,
        "no more than 300 before it"
    );
    assert_eq!(
        frames_of_a_chase_from(100, 5000, 900),
        0..900,
        "and none before the first of the project"
    );
    assert_eq!(
        frames_of_a_chase_from(899, 0, 900),
        899..900,
        "an effect at its first frame has nothing before it, and the chase is the frame itself"
    );
    assert_eq!(
        frames_of_a_chase_from(10, 3, 5),
        7..11,
        "a frame that is past the end that the project has (it was cut short since) is in the chase all the same"
    );
    // Whatever the start, it is the start of the lead-up to the same frame.
    for (frame, most) in [(0, 0), (5, 3), (1000, 7), (1000, 300), (1000, 400)] {
        assert_eq!(
            frames_of_a_chase_from(frame, most, 9000).start,
            frames_of_the_lead_up_to(frame, most).start
        );
    }
}

#[test]
fn the_thread_gives_each_frame_after_the_pictures_it_is_made_of_and_a_picture_is_let_go_of_only_after_the_last_frame_that_uses_it()
 {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let started = start_a_lead_up(made_from(plan, 40..90, &script));
    let items = everything_given_by(&started);

    assert!(matches!(items.last(), Some(LeadUpItem::End)), "it ends with the end");
    assert_eq!(
        the_frames_in(&items).len(),
        50,
        "a frame for each of the frames of the lead-up"
    );
    // A model of the engine: it has the pictures that it was sent and not told to let go of, and a frame that is made of a picture that it does not have cannot be made.
    let mut engine_has: HashSet<PictureId> = HashSet::new();
    let mut frames_made = 0;
    let mut pictures_sent = 0;
    let mut pictures_let_go_of = 0;
    for item in &items {
        match item {
            LeadUpItem::Picture { id, .. } => {
                assert!(engine_has.insert(*id), "{id:?} is sent twice");
                pictures_sent += 1;
            }
            LeadUpItem::Forget(ids) => {
                assert!(!ids.is_empty(), "nothing is said when there is nothing to let go of");
                for id in ids {
                    assert!(
                        engine_has.remove(id),
                        "{id:?} is let go of and the engine has not got it"
                    );
                    pictures_let_go_of += 1;
                }
            }
            LeadUpItem::Frame(description) => {
                for id in description.pictures_used() {
                    assert!(
                        engine_has.contains(&id),
                        "frame {frames_made} is made of {id:?}, which the engine has not got"
                    );
                }
                frames_made += 1;
            }
            LeadUpItem::End => {}
            LeadUpItem::Failed(problem) => panic!("{problem}"),
        }
    }
    assert_eq!(frames_made, 50);
    // A picture of the video for each frame, and the text (seen at frames 50 to 79).
    assert_eq!(pictures_sent, 50 + 1);
    // Each picture of the video is let go of by the frame after it, and the text by the frame that it is not seen in: all but the pictures of the last frame (which the engine lets go of when it ends the lead-up).
    assert_eq!(
        engine_has.len(),
        1,
        "the picture of the last frame is the only one that is left: {engine_has:?}"
    );
    assert_eq!(pictures_let_go_of, pictures_sent - 1);
}

#[test]
fn every_picture_of_a_lead_up_has_an_id_of_the_lead_up_and_the_frames_are_made_as_the_preview_makes_them() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let started = start_a_lead_up(made_from(plan, 40..60, &script));
    let items = everything_given_by(&started);
    for item in &items {
        match item {
            LeadUpItem::Picture { id, .. } => assert!(id.is_in_the_lead_up(), "{id:?}"),
            LeadUpItem::Forget(ids) => assert!(ids.iter().all(|id| id.is_in_the_lead_up()), "{ids:?}"),
            LeadUpItem::Frame(description) => {
                assert!(
                    description.pictures_used().iter().all(|id| id.is_in_the_lead_up()),
                    "{:?}",
                    description.pictures_used()
                );
                assert_eq!(
                    description.render_size,
                    (96, 54),
                    "the size that the preview makes its frames at"
                );
                assert_eq!(description.colour, how_the_preview_describes().colour);
            }
            LeadUpItem::End | LeadUpItem::Failed(_) => {}
        }
    }
}

#[test]
fn the_picture_files_that_the_window_has_decoded_are_the_pictures_that_the_lead_up_is_made_of_and_a_text_is_drawn_for_the_picture_of_the_project()
 {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    // Frames that have the picture file in them (it begins at frame 200 on a track in front).
    let mut with_the_picture_file = made_from(plan, 205..210, &script);
    let seen_by_the_engine = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = std::sync::Arc::clone(&seen_by_the_engine);
    with_the_picture_file.text = Box::new(move || {
        Box::new(move |_: &crate::project::TextObject, width: u32, height: u32| {
            recorded.lock().unwrap().push((width, height));
            RgbaPicture {
                width_in_pixels: 4,
                height_in_pixels: 2,
                rgba_pixels: vec![200; 32],
            }
        })
    });
    let started = start_a_lead_up(with_the_picture_file);
    let items = everything_given_by(&started);
    let sizes_of_the_pictures: Vec<(u32, u32)> = items
        .iter()
        .filter_map(|item| match item {
            LeadUpItem::Picture { data, .. } => Some((data.width_in_pixels, data.height_in_pixels)),
            _ => None,
        })
        .collect();
    assert!(
        sizes_of_the_pictures.contains(&(8, 4)),
        "the picture file, as decoded by the window: {sizes_of_the_pictures:?}"
    );
    assert!(
        seen_by_the_engine.lock().unwrap().is_empty(),
        "no text is seen at those frames, so none is drawn"
    );
}

#[test]
fn the_stream_of_a_video_begins_where_the_lead_up_does_and_delivers_the_size_that_the_preview_delivers() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let started = start_a_lead_up(made_from(plan, 100..130, &script));
    let items = everything_given_by(&started);
    assert!(matches!(items.last(), Some(LeadUpItem::End)));
    let streams = script.state().started.clone();
    assert_eq!(
        streams.len(),
        1,
        "one stream, for the one video, and only for the lead-up"
    );
    let (request, delivered_size) = &streams[0];
    assert_eq!(
        request.first_source_frame.value(),
        100,
        "the picture that is seen at the first frame"
    );
    assert_eq!(request.source_frame_count, 30);
    assert_eq!(request.delivered_size(), *delivered_size);
    assert_eq!(
        (request.picture_width_in_pixels, request.picture_height_in_pixels),
        (160, 90),
        "inside the picture of the preview, which is what the preview delivers a video as big as"
    );
}

#[test]
fn a_lead_up_that_cannot_read_a_video_gives_the_failure_and_nothing_else() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    script.state().fail_to_start = true;
    let started = start_a_lead_up(made_from(plan, 0..30, &script));
    let items = everything_given_by(&started);
    assert!(
        matches!(items.as_slice(), [LeadUpItem::Failed(problem)] if problem.contains("cannot be read")),
        "{items:?}"
    );
}

#[test]
fn a_video_that_gives_no_picture_at_all_fails_the_lead_up_after_the_frames_that_could_be_made() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = Script::default();
    script.state().to_say.push_back(Vec::new());
    let started = start_a_lead_up(made_from(plan, 0..30, &script));
    let items = everything_given_by(&started);
    assert!(
        matches!(items.last(), Some(LeadUpItem::Failed(problem)) if problem.contains("gave no picture")),
        "{items:?}"
    );
    assert!(the_frames_in(&items).is_empty());
}

#[test]
fn a_mistake_of_the_program_in_the_thread_is_a_failure_with_words_and_not_a_lead_up_that_never_ends() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let mut made = made_from(plan, 40..60, &script);
    made.text = Box::new(|| panic!("a mistake of the test"));
    let started = start_a_lead_up(made);
    let items = everything_given_by(&started);
    assert!(
        matches!(items.last(), Some(LeadUpItem::Failed(problem)) if problem.contains("mistake in the program") && problem.contains("a mistake of the test")),
        "{items:?}"
    );
}

#[test]
fn a_lead_up_that_is_cancelled_while_the_engine_is_behind_ends_without_a_word() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let started = start_a_lead_up(made_from(plan, 0..200, &script));
    // Nobody takes anything: the channel fills, and the thread waits for room in it.
    std::thread::sleep(Duration::from_millis(100));
    assert!(!started.running.is_over(), "it waits for the engine");
    started.running.cancel();
    wait_until_over(&started.running);
    let given: Vec<LeadUpItem> = started.items.try_iter().collect();
    assert!(
        given
            .iter()
            .all(|item| !matches!(item, LeadUpItem::End | LeadUpItem::Failed(_))),
        "it says nothing of how it ended: {given:?}"
    );
    assert!(
        given.len() <= MOST_ITEMS_AHEAD,
        "no more than the room that it was given: {}",
        given.len()
    );
}

#[test]
fn a_lead_up_that_the_engine_has_let_go_of_ends_as_the_end_of_its_channel_is_gone() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let StartedLeadUp { items, running } = start_a_lead_up(made_from(plan, 0..200, &script));
    drop(items);
    wait_until_over(&running);
}

#[test]
fn a_lead_up_that_is_cancelled_while_a_picture_of_a_video_is_waited_for_ends_without_a_word() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = Script::default();
    // A stream that has nothing to say and does not end.
    script.state().never_ends = true;
    script.state().to_say.push_back(Vec::new());
    let started = start_a_lead_up(made_from(plan, 0..30, &script));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!started.running.is_over(), "it waits for the picture");
    started.running.cancel();
    wait_until_over(&started.running);
    assert_eq!(started.items.try_iter().count(), 0, "and says nothing");
}

#[test]
fn dropping_a_running_lead_up_cancels_it_and_lets_go_of_its_streams() {
    let plan = a_video_with_a_text_and_a_picture_over_it();
    let script = script_of_pictures();
    let StartedLeadUp { items, running } = start_a_lead_up(made_from(plan, 0..200, &script));
    std::thread::sleep(Duration::from_millis(50));
    let cancelled = running.cancelled.clone();
    drop(running);
    assert!(cancelled.load(Ordering::SeqCst));
    let started = Instant::now();
    while script.state().alive != 0 {
        assert!(started.elapsed() < A_LONG_TIME, "the decode process was not let go of");
        std::thread::sleep(Duration::from_millis(2));
    }
    drop(items);
}
