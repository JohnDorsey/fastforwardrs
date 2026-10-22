// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The frames of a lead-up on a seek (EN-14, option 1) made from a real video with the real ffmpeg: which picture of the video each frame of the lead-up is made of, for every kind of edit
//! that a project has, and how it ends when it is cancelled while ffmpeg is at work. (What the engine makes of the frames is in `engine::worker::lead_up`; what the window does with them is in
//! the tests of the window.)
//!
//! Every test video carries its own frame number in its picture (see `support`), so what a picture is can be read from the picture; what it should be is worked out from the project without any
//! of the lead-up code (`support::expected_source_frames`).

mod support;

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::engine::{FrameColour, LayerSource, LeadUpItem, PictureData, PictureId, PicturePixels};
use fastforwardrs::lead_up::{
    PreviewRunStreamStarter, StartedLeadUp, WhatALeadUpIsMadeFrom, frames_of_the_lead_up_to, start_a_lead_up,
};
use fastforwardrs::media::{FfmpegToolchain, MediaLocation};
use fastforwardrs::playback::preview_picture_size;
use fastforwardrs::project::{Project, Settings};
use fastforwardrs::video_export::{
    FontsOfTheComputer, HowTheFramesAreDescribed, plan_export_of_the_files_that_can_be_read,
};
use support::{counter_video, edited, expected_source_frames, frame_number_in_picture, project_for};

const A_LONG_TIME: Duration = Duration::from_secs(120);

/// The size that the test makes the frames of the lead-up at (what the preview makes its frames at is the size of the picture on the screen).
const RENDER_SIZE: (u32, u32) = (160, 90);

/// Starts the lead-up of `frames` of `project`, made as the window makes one.
fn start_the_lead_up_of(toolchain: &FfmpegToolchain, project: &Project, frames: Range<u64>) -> StartedLeadUp {
    let plan = plan_export_of_the_files_that_can_be_read(project, Settings::DEFAULT, |_| true).unwrap();
    let (picture_width, picture_height) = project.picture_size();
    let locations: HashMap<_, _> = project
        .sources()
        .iter()
        .map(|source| {
            (
                source.identifier,
                MediaLocation::from(source.link.absolute_path.clone()),
            )
        })
        .collect();
    start_a_lead_up(WhatALeadUpIsMadeFrom {
        plan,
        frames,
        how: HowTheFramesAreDescribed {
            picture_size: (picture_width, picture_height),
            render_size: RENDER_SIZE,
            colour: FrameColour::default(),
        },
        preview_picture_size: preview_picture_size(picture_width, picture_height),
        locations,
        planes: HashMap::new(),
        readings: HashMap::new(),
        images: HashMap::new(),
        starter: PreviewRunStreamStarter::new(Some(Arc::new(toolchain.clone()))),
        text: Box::new(|| Box::new(FontsOfTheComputer::new())),
    })
}

/// Everything that a lead-up gives, until it has nothing more to give (or the time is up, which fails the test).
fn everything_given_by(started: &StartedLeadUp) -> Vec<LeadUpItem> {
    let limit = Instant::now() + A_LONG_TIME;
    let mut items = Vec::new();
    loop {
        match started.items.recv_timeout(Duration::from_millis(50)) {
            Ok(item) => items.push(item),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(Instant::now() < limit, "the lead-up did not end: {} items", items.len());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return items,
        }
    }
}

/// The number of the frame of the video that each frame of the lead-up is made of, read from the picture, and the size of the picture (of the video, which is the layer at the back).
fn the_source_frames_that_the_frames_are_made_of(items: &[LeadUpItem]) -> Vec<(u64, (u32, u32))> {
    let mut pictures: HashMap<PictureId, Arc<PictureData>> = HashMap::new();
    let mut frames = Vec::new();
    for item in items {
        match item {
            LeadUpItem::Picture { id, data } => {
                pictures.insert(*id, Arc::clone(data));
            }
            LeadUpItem::Forget(ids) => {
                for id in ids {
                    pictures.remove(id);
                }
            }
            LeadUpItem::Frame(description) => {
                // (The video is the layer at the back; the stretches that play faster have the symbol over it.)
                assert!(
                    (1..=2).contains(&description.layers.len()),
                    "a video, and perhaps the symbol of the speed over it: {}",
                    description.layers.len()
                );
                let LayerSource::Picture(id) = description.layers[0].source else {
                    panic!("the layer is the picture of the video");
                };
                let picture = pictures
                    .get(&id)
                    .unwrap_or_else(|| panic!("{id:?} was not sent before it was used"));
                let PicturePixels::Rgba8 { straight_alpha } = &picture.pixels else {
                    panic!("the video was not asked for in planes");
                };
                frames.push((
                    frame_number_in_picture(straight_alpha, picture.width_in_pixels, picture.height_in_pixels, 4),
                    (picture.width_in_pixels, picture.height_in_pixels),
                ));
            }
            LeadUpItem::End => {}
            LeadUpItem::Failed(problem) => panic!("the lead-up failed: {problem}"),
        }
    }
    frames
}

#[test]
fn the_pictures_of_a_lead_up_are_the_source_frames_that_the_project_shows_at_its_frames_for_every_kind_of_edit() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let uncut = project_for(&toolchain, &video);
    // Cut at 60 and 150, the middle clip at 4 times the speed.
    let cut_and_sped_up = edited(project_for(&toolchain, &video), &[60, 150], &[1, 4, 1]);
    let cut = edited(project_for(&toolchain, &video), &[100], &[1, 1]);
    for (name, project, frames) in [
        ("uncut, from the middle", &uncut, 120..180),
        ("uncut, from the first frame", &uncut, 0..40),
        ("cut and sped up, over the cuts", &cut_and_sped_up, 40..100),
        ("cut and sped up, from inside the fast part", &cut_and_sped_up, 70..90),
        ("cut, over the cut", &cut, 90..110),
    ] {
        let started = start_the_lead_up_of(&toolchain, project, frames.clone());
        let items = everything_given_by(&started);
        assert!(
            matches!(items.last(), Some(LeadUpItem::End)),
            "{name}: it ends with the end"
        );
        let made_of = the_source_frames_that_the_frames_are_made_of(&items);
        let expected = expected_source_frames(project);
        assert_eq!(
            made_of.len() as u64,
            frames.end - frames.start,
            "{name}: a frame for each frame of the lead-up"
        );
        for (frame, (source_frame, size)) in frames.clone().zip(made_of) {
            assert_eq!(
                source_frame, expected[frame as usize],
                "{name}: the lead-up's frame {frame} is made of frame {source_frame} of the video and the project shows {}",
                expected[frame as usize]
            );
            assert_eq!(
                size,
                (320, 180),
                "{name}: as big as the preview makes it, which is as big as the video"
            );
        }
    }
}

#[test]
fn the_frames_of_a_lead_up_to_a_frame_of_a_real_project_begin_where_the_states_of_its_effects_begin() {
    let toolchain = toolchain_or_skip!();
    let project = project_for(&toolchain, &counter_video(&toolchain));
    // An effect that has gone on for 25 frames is made from the 25 frames before the frame, which are the first 25 of the clip's video.
    let frames = frames_of_the_lead_up_to(25, 25);
    assert_eq!(frames, 0..25);
    let started = start_the_lead_up_of(&toolchain, &project, frames);
    let made_of = the_source_frames_that_the_frames_are_made_of(&everything_given_by(&started));
    assert_eq!(
        made_of
            .iter()
            .map(|(source_frame, _)| *source_frame)
            .collect::<Vec<_>>(),
        (0..25).collect::<Vec<_>>()
    );
}

#[test]
fn a_lead_up_that_is_cancelled_while_ffmpeg_decodes_ends_without_a_word_and_does_not_leave_the_thread_running() {
    let toolchain = toolchain_or_skip!();
    let project = project_for(&toolchain, &counter_video(&toolchain));
    let started = start_the_lead_up_of(&toolchain, &project, 0..300);
    // Take a few of the things that it gives, and then let go of the lead-up: the engine is not going to take the rest.
    for _ in 0..3 {
        started
            .items
            .recv_timeout(A_LONG_TIME)
            .expect("the lead-up gives something");
    }
    started.running.cancel();
    let limit = Instant::now() + A_LONG_TIME;
    while !started.running.is_over() {
        assert!(Instant::now() < limit, "the thread did not end");
        std::thread::sleep(Duration::from_millis(5));
    }
    let given: Vec<LeadUpItem> = started.items.try_iter().collect();
    assert!(
        given
            .iter()
            .all(|item| !matches!(item, LeadUpItem::End | LeadUpItem::Failed(_))),
        "it says nothing of how it ended"
    );
}
