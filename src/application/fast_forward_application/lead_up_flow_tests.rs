// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the window gives the thread that describes the frames of a lead-up (EN-14): the project as the window shows it, planned; the files where the window found them; how the colours of a video
//! are read, as the preview reads them; the picture files that the window has decoded; the sizes and the colour of the frame on the screen. The streams of the videos are made up (no ffmpeg), and the
//! window has no engine: these are about what the window gathers, and the lead-up that is made of it is looked at only as far as it says what it was asked for.

use super::test_window::{VIDEO, Window, project_of, video_source_shown_at};
use super::{SourceFileState, SourceStatus};
use crate::application::preview_panel::LeadUpWanted;
use crate::colour_space::VideoMatrix;
use crate::engine::{FrameColour, LeadUpItem};
use crate::lead_up::{WhatALeadUpIsMadeFrom, start_a_lead_up};
use crate::media::{FrameDelivery, MediaLocation, NativeYuv420p, VideoStreamFormat};
use crate::playback::preview_picture_size;
use crate::project::{Project, TimelineFrameIndex};
use crate::rgba_picture::RgbaPicture;
use crate::video_export::test_streams::{Script, planes};

/// A colour that is not the compatibility mode, which a frame that the window shows is made in unless it says so.
fn another_colour() -> FrameColour {
    FrameColour {
        working: crate::engine::WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709),
        ..FrameColour::default()
    }
}

fn wanted(most_frames_a_state_has_gone_on_for: u64) -> LeadUpWanted {
    LeadUpWanted {
        generation: 1,
        render_size: (96, 54),
        colour: another_colour(),
        most_frames_a_state_has_gone_on_for,
        is_a_chase: false,
    }
}

/// The same, as a chase (the project plays).
fn a_chase(most_frames_a_state_has_gone_on_for: u64) -> LeadUpWanted {
    LeadUpWanted {
        is_a_chase: true,
        ..wanted(most_frames_a_state_has_gone_on_for)
    }
}

fn made_from(window: &Window, wanted: &LeadUpWanted, frame: u64) -> Result<WhatALeadUpIsMadeFrom<Script>, String> {
    window
        .application()
        .what_the_lead_up_is_made_from(wanted, frame, Script::default())
}

fn a_video_that_is_stored_in_planes() -> VideoStreamFormat {
    VideoStreamFormat {
        pixel_format: Some("yuv420p".to_owned()),
        colour_space: Some("bt709".to_owned()),
        colour_range: Some("tv".to_owned()),
        ..VideoStreamFormat::default()
    }
}

#[test]
fn the_frames_are_the_frames_before_the_one_on_the_screen_as_many_as_the_states_have_gone_on_for_and_no_more_than_the_most()
 {
    let window = Window::showing(project_of(900));
    assert_eq!(made_from(&window, &wanted(8), 8).unwrap().frames, 0..8);
    assert_eq!(made_from(&window, &wanted(50), 500).unwrap().frames, 450..500);
    assert_eq!(
        made_from(&window, &wanted(5000), 500).unwrap().frames,
        200..500,
        "no more than 300"
    );
    assert!(
        made_from(&window, &wanted(8), 0)
            .err()
            .is_some_and(|why| why.contains("no frames before")),
        "there is nothing before the first frame"
    );
}

#[test]
fn the_frames_of_a_chase_go_on_from_before_the_frame_on_the_screen_to_the_end_of_the_project() {
    let window = Window::showing(project_of(900));
    assert_eq!(made_from(&window, &a_chase(50), 500).unwrap().frames, 450..900);
    assert_eq!(
        made_from(&window, &a_chase(5000), 500).unwrap().frames,
        200..900,
        "no more than 300 before it"
    );
    assert_eq!(
        made_from(&window, &a_chase(8), 0).unwrap().frames,
        0..900,
        "a chase of a frame that has nothing before it is not refused: it is the frames that follow"
    );
    // (And one that is not a chase is, as it was.)
    assert!(made_from(&window, &wanted(8), 0).is_err());
}

#[test]
fn the_frames_are_described_as_the_frame_on_the_screen_is_at_its_size_in_its_colour_and_for_the_picture_of_the_project()
{
    // A video that is an odd number of pixels across and down: the picture of the project is that, and the picture of the export is that made even.
    let mut project = Project::empty();
    project
        .import_source(video_source_shown_at(300, 1281, 721), TimelineFrameIndex::new(0))
        .unwrap();
    assert_eq!(project.picture_size(), (1281, 721), "a premise");
    let window = Window::showing(project);
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert_eq!(
        made.how.render_size,
        (96, 54),
        "the size that the frame on the screen was made at"
    );
    assert_eq!(made.how.colour, another_colour(), "and its colour");
    assert_eq!(
        made.how.picture_size,
        (1281, 721),
        "text is drawn for the picture of the project, as the preview draws it, and not for the even picture of the export"
    );
    assert_eq!(made.plan.picture_size(), (1280, 720), "(the plan is the export's)");
    assert_eq!(
        made.preview_picture_size,
        preview_picture_size(1281, 721),
        "the videos are delivered inside the picture that the preview decodes for"
    );
    assert_ne!(made.preview_picture_size, (1281, 721));
}

#[test]
fn the_files_are_where_the_window_found_them_and_a_file_that_cannot_be_used_has_no_pictures_in_the_plan() {
    let mut window = Window::showing(project_of(300));
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert_eq!(
        made.locations,
        std::collections::HashMap::from([(VIDEO, MediaLocation::from("holiday.mp4"))])
    );
    assert_eq!(made.plan.runs.len(), 1, "the video is seen");

    // The file cannot be found: the project that the window shows has no pictures of it, and neither has the lead-up.
    window.application_mut().sources.files.insert(
        VIDEO,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "it is gone".to_owned(),
        }),
    );
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert!(made.plan.runs.is_empty(), "no run of a video that cannot be read");
    assert!(made.locations.is_empty());
}

#[test]
fn the_picture_files_are_those_that_the_window_has_decoded_and_not_those_it_has_not() {
    let mut project = project_of(300);
    let decoded = super::picture_tests::picture_source();
    let decoded_identifier = decoded.identifier;
    project.import_source(decoded, TimelineFrameIndex::new(0)).unwrap();
    let mut window = Window::showing(project);
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert!(made.images.is_empty(), "the picture file has not been decoded yet");
    let picture = RgbaPicture {
        width_in_pixels: 4,
        height_in_pixels: 2,
        rgba_pixels: [10, 20, 30, 255].repeat(8),
    };
    window
        .application_mut()
        .sources
        .files
        .get_mut(&decoded_identifier)
        .expect("the window has the file")
        .picture
        .picture = Some(picture.clone());
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert_eq!(made.images.len(), 1);
    assert_eq!(made.images[&decoded_identifier], picture);
}

#[test]
fn how_the_colours_of_a_video_are_read_is_what_the_preview_makes_of_what_the_window_knows_of_it() {
    let mut window = Window::showing(project_of(300));
    let in_planes = NativeYuv420p {
        matrix: VideoMatrix::Bt709,
        full_range: false,
    };
    // Nothing is known of how the video is stored: ffmpeg makes it RGBA.
    window.application_mut().playback.planes_wanted = true;
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert!(made.planes.is_empty());
    assert!(made.readings[&VIDEO].planes.is_none());
    // It is known, and the preview wants planes (as it does when the engine makes it): so does the lead-up.
    window
        .application_mut()
        .sources
        .files
        .get_mut(&VIDEO)
        .unwrap()
        .video_format
        .known = Some(a_video_that_is_stored_in_planes());
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert_eq!(made.planes.get(&VIDEO), Some(&in_planes));
    assert_eq!(made.readings[&VIDEO].planes, Some(in_planes));
    // The preview does not want planes (the old drawing): neither does the lead-up.
    window.application_mut().playback.planes_wanted = false;
    let made = made_from(&window, &wanted(8), 8).unwrap();
    assert!(made.planes.is_empty());
}

#[test]
fn a_project_that_cannot_be_planned_cannot_have_a_lead_up_and_says_why() {
    let window = Window::showing(Project::empty());
    let why = made_from(&window, &wanted(8), 8)
        .err()
        .expect("an empty project has none");
    assert!(why.contains("nothing to export"), "{why}");
}

#[test]
fn what_the_window_gathers_makes_a_lead_up_of_the_pictures_of_the_video_in_planes_as_the_preview_has_them() {
    let mut window = Window::showing(project_of(300));
    window.application_mut().playback.planes_wanted = true;
    window
        .application_mut()
        .sources
        .files
        .get_mut(&VIDEO)
        .unwrap()
        .video_format
        .known = Some(a_video_that_is_stored_in_planes());
    let script = Script::default();
    script.state().to_say.push_back((0..40).map(planes).collect::<Vec<_>>());
    let made = window
        .application()
        .what_the_lead_up_is_made_from(&wanted(30), 100, script.clone())
        .unwrap();
    let started = start_a_lead_up(made);
    let items: Vec<LeadUpItem> = started.items.iter().collect();
    assert!(matches!(items.last(), Some(LeadUpItem::End)), "{items:?}");
    assert_eq!(
        items.iter().filter(|item| matches!(item, LeadUpItem::Frame(_))).count(),
        30
    );
    let streams = script.state().started.clone();
    assert_eq!(streams.len(), 1);
    let (request, delivered) = &streams[0];
    assert_eq!(
        request.delivery,
        FrameDelivery::Yuv420p,
        "in planes, as the preview asks for it"
    );
    assert_eq!(
        request.first_source_frame.value(),
        70,
        "the picture seen at the first frame of the lead-up"
    );
    assert_eq!(request.source_frame_count, 30);
    assert_eq!(*delivered, request.delivered_size());
    // (The picture of a video that is made in planes is a picture in planes.)
    assert!(items.iter().any(|item| matches!(
        item,
        LeadUpItem::Picture { data, .. } if matches!(data.pixels, crate::engine::PicturePixels::Yuv420p8 { .. })
    )));
}

#[test]
fn a_lead_up_that_the_window_cannot_make_is_said_to_the_preview_and_is_not_wanted_again_for_that_frame() {
    use crate::application::preview_panel::EnginePreview;
    use crate::engine::Engine;
    let mut window = Window::showing(project_of(300));
    let (engine, _events, _requests) = Engine::scripted_for_tests();
    let wanted = wanted(8);
    window.application_mut().interface.engine_preview =
        Some(EnginePreview::wanting_a_lead_up_for_tests(engine, wanted, 77));
    assert_eq!(
        window.engine_preview().unwrap().the_lead_up_that_is_wanted(),
        Some(wanted),
        "a premise"
    );

    window
        .application_mut()
        .tell_the_preview_what_came_of_the_lead_up_it_wanted(&wanted, Err("it cannot be made".to_owned()));

    let preview = window.engine_preview().unwrap();
    assert_eq!(preview.the_lead_up_that_is_wanted(), None, "it was answered");
    assert!(!preview.a_lead_up_is_being_made());
    assert!(
        preview
            .last_problem()
            .is_some_and(|problem| problem.contains("it cannot be made")),
        "{:?}",
        preview.last_problem()
    );
}

#[test]
fn a_lead_up_that_the_window_has_made_is_started_in_the_preview_and_the_engine_is_given_it() {
    use crate::application::preview_panel::EnginePreview;
    use crate::engine::{Engine, EngineRequest};
    let mut window = Window::showing(project_of(300));
    let (engine, _events, requests) = Engine::scripted_for_tests();
    let wanted = wanted(8);
    window.application_mut().interface.engine_preview =
        Some(EnginePreview::wanting_a_lead_up_for_tests(engine, wanted, 77));
    let script = Script::default();
    script.state().to_say.push_back((0..10).map(planes).collect::<Vec<_>>());
    let made = window
        .application()
        .what_the_lead_up_is_made_from(&wanted, 8, script)
        .map(start_a_lead_up);

    window
        .application_mut()
        .tell_the_preview_what_came_of_the_lead_up_it_wanted(&wanted, made);

    assert!(window.engine_preview().unwrap().a_lead_up_is_being_made());
    assert!(matches!(
        requests.try_recv(),
        Ok(EngineRequest::StartTheLeadUp { generation: 1, .. })
    ));
}
