// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures of the runs of an export (`video_export::PicturesOfTheRuns`), with the real ffmpeg: which picture of the video each frame of the project gets, for
//! every kind of edit that a project has, delivered as RGBA and as planes, at the size of the picture and smaller, and from videos of other frame rates.
//!
//! Every test video carries its own frame number in its picture (see `support`), so what a picture is can be read from the picture; what it should be is worked
//! out from the project without any of the export code (`support::expected_source_frames`, `support::picture_shown_in_frame`).

mod support;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use fastforwardrs::media::{
    FfmpegToolchain, FrameStreamError, FrameStreamRequest, MediaLocation, NativeYuv420p, probe_video,
    probe_video_stream_format,
};
use fastforwardrs::playback::SuppliedFrame;
use fastforwardrs::project::{
    CanvasArea, FrameRate, Placement, Project, Settings, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    TextObject, TimelineFrameIndex,
};
use fastforwardrs::video_export::{
    ExportPlan, FfmpegRunStreamStarter, PicturesOfTheRuns, RunStreamStarter, plan_export,
};
use support::{
    ClipOfTheVideo, TestVideoSpecification, counter_video, edited, expected_source_frames, frame_number_in_picture,
    picture_shown_in_frame, project_for, project_with_tracks_of_videos, test_video,
};

/// What was started, shared between a test and the starter it gave: the first source frame and the size of every run, in the order the runs were started.
type Started = Arc<Mutex<Vec<(u64, (u32, u32))>>>;

/// The starter that runs ffmpeg and remembers what it was asked to start.
#[derive(Clone)]
struct RecordingStarter {
    real: FfmpegRunStreamStarter,
    started: Started,
}

impl RunStreamStarter for RecordingStarter {
    type Stream = <FfmpegRunStreamStarter as RunStreamStarter>::Stream;

    fn start(
        &self,
        request: &FrameStreamRequest,
        delivered_size: (u32, u32),
    ) -> Result<Self::Stream, FrameStreamError> {
        self.started
            .lock()
            .unwrap()
            .push((request.first_source_frame.value(), delivered_size));
        self.real.start(request, delivered_size)
    }
}

/// What a run delivered.
#[derive(Debug, Default)]
struct RunSeen {
    /// The frame number carried by the picture of the run at each frame of the timeline that the run is seen at, in order.
    numbers: Vec<u64>,
    /// The `source_frame` that each of those pictures says it is of.
    source_frames_said: Vec<u64>,
    /// The sizes of the pictures, which must all be the same.
    sizes: Vec<(u32, u32)>,
    /// Whether the pictures came as planes.
    in_planes: bool,
}

struct Seen {
    runs: Vec<RunSeen>,
    started: Vec<(u64, (u32, u32))>,
    pictures_held: u64,
}

fn locations_of(project: &Project, plan: &ExportPlan) -> HashMap<SourceMediaIdentifier, MediaLocation> {
    plan.source_files
        .iter()
        .map(|&source| {
            (
                source,
                MediaLocation::from(project.source(source).unwrap().link.absolute_path.clone()),
            )
        })
        .collect()
}

fn planes_of(
    toolchain: &FfmpegToolchain,
    plan: &ExportPlan,
    locations: &HashMap<SourceMediaIdentifier, MediaLocation>,
) -> HashMap<SourceMediaIdentifier, NativeYuv420p> {
    plan.videos
        .keys()
        .filter_map(|&source| {
            let format = probe_video_stream_format(toolchain, locations[&source].clone()).unwrap();
            let (width, height) = plan.videos[&source].display_size_in_pixels;
            format.as_native_yuv420p(width, height).map(|native| (source, native))
        })
        .collect()
}

/// Takes every picture of every run of the plan of `project`, a frame after a frame as the export does, and reads what each carries.
fn pictures_of_the_runs(toolchain: &FfmpegToolchain, project: &Project, ask_for_planes: bool) -> Seen {
    let plan = plan_export(project, Settings::DEFAULT).expect("the project can be exported");
    let locations = locations_of(project, &plan);
    let planes = if ask_for_planes {
        planes_of(toolchain, &plan, &locations)
    } else {
        HashMap::new()
    };
    assert_eq!(
        !planes.is_empty(),
        ask_for_planes,
        "the test videos are stored the way that can be had as planes"
    );
    let started = Started::default();
    let starter = RecordingStarter {
        real: FfmpegRunStreamStarter {
            toolchain: Arc::new(toolchain.clone()),
        },
        started: Arc::clone(&started),
    };
    let mut pictures = PicturesOfTheRuns::new(starter, &plan, &locations, &planes).expect("the pictures can be had");
    let cancel = AtomicBool::new(false);
    let mut runs: Vec<RunSeen> = plan.runs.iter().map(|_| RunSeen::default()).collect();
    for timeline_frame in 0..plan.total_frame_count {
        pictures
            .start_the_runs_that_begin_soon(timeline_frame)
            .expect("the runs start");
        for (number, run) in plan.runs.iter().enumerate() {
            if !(run.first_timeline_frame <= timeline_frame && timeline_frame < run.end_timeline_frame) {
                continue;
            }
            let picture = pictures
                .picture(number, timeline_frame, &cancel)
                .unwrap_or_else(|problem| panic!("frame {timeline_frame} of run {number}: {problem}"));
            let seen = &mut runs[number];
            let (size, carried) = match &picture.frame {
                SuppliedFrame::Rgba(frame) => (
                    (frame.width_in_pixels, frame.height_in_pixels),
                    frame_number_in_picture(&frame.rgba_pixels, frame.width_in_pixels, frame.height_in_pixels, 4),
                ),
                SuppliedFrame::Yuv420p { frame, .. } => {
                    seen.in_planes = true;
                    (
                        (frame.width_in_pixels, frame.height_in_pixels),
                        frame_number_in_picture(&frame.y, frame.width_in_pixels, frame.height_in_pixels, 1),
                    )
                }
            };
            assert_eq!(picture.source, run.source);
            seen.numbers.push(carried);
            seen.source_frames_said.push(picture.source_frame.value());
            seen.sizes.push(size);
        }
    }
    let started = started.lock().unwrap().clone();
    Seen {
        runs,
        started,
        pictures_held: pictures.pictures_held(),
    }
}

/// The numbers a single run (of a project with one run) carried.
fn the_one_run(seen: &Seen) -> &RunSeen {
    assert_eq!(seen.runs.len(), 1, "{:?}", seen.runs);
    &seen.runs[0]
}

#[test]
fn every_frame_of_an_edited_project_gets_the_picture_the_project_says_in_rgba_and_in_planes() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Cut in four, and played at 1x, 3x, 1x and 5x: four runs, the speeds of which skip pictures.
    let project = edited(project_for(&toolchain, &video), &[150, 330, 375], &[1, 3, 1, 5]);
    let expected = expected_source_frames(&project);
    for in_planes in [false, true] {
        let seen = pictures_of_the_runs(&toolchain, &project, in_planes);
        assert_eq!(seen.runs.len(), 4);
        let numbers: Vec<u64> = seen.runs.iter().flat_map(|run| run.numbers.clone()).collect();
        assert_eq!(numbers, expected, "in planes: {in_planes}");
        let said: Vec<u64> = seen
            .runs
            .iter()
            .flat_map(|run| run.source_frames_said.clone())
            .collect();
        assert_eq!(
            said, expected,
            "what the pictures say they are of, in planes: {in_planes}"
        );
        assert!(seen.runs.iter().all(|run| run.in_planes == in_planes));
        assert!(
            seen.runs
                .iter()
                .flat_map(|run| &run.sizes)
                .all(|&size| size == (320, 180))
        );
        assert_eq!(seen.pictures_held, 0);
        assert_eq!(
            seen.started.len(),
            4,
            "one process for each run, and so nothing is read twice"
        );
    }
}

#[test]
fn a_text_over_a_video_does_not_cut_the_decoding_of_the_video() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    let expected = expected_source_frames(&project);
    let front = project.add_track_in_front();
    for (at, frames) in [(50, 30), (150, 20), (400, 60)] {
        project
            .insert_text(front, TimelineFrameIndex::new(at), TextObject::default(), frames)
            .unwrap();
    }
    let seen = pictures_of_the_runs(&toolchain, &project, true);
    assert_eq!(
        seen.started.len(),
        1,
        "the video has three texts over it and is one process: {:?}",
        seen.started
    );
    assert_eq!(the_one_run(&seen).numbers, expected);
}

#[test]
fn a_video_in_a_smaller_place_is_delivered_at_the_size_of_the_place_and_still_carries_its_numbers() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    project
        .set_clip_placement(
            project.tracks()[0].entries()[0].id(),
            Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5)),
        )
        .unwrap();
    let expected = expected_source_frames(&project);
    for in_planes in [false, true] {
        let seen = pictures_of_the_runs(&toolchain, &project, in_planes);
        let run = the_one_run(&seen);
        assert!(run.sizes.iter().all(|&size| size == (160, 90)), "{:?}", &run.sizes[..3]);
        assert_eq!(seen.started[0].1, (160, 90));
        assert_eq!(run.numbers, expected, "in planes: {in_planes}");
    }
}

/// A project of 30 frames a second that has the whole of `video` (of whatever frame rate) on its track. (A project made by importing a video into an empty
/// project runs at the rate of the video; this one has its rate chosen first.)
fn project_of_30_a_second_with(toolchain: &FfmpegToolchain, video: &support::TestVideo) -> Project {
    let mut project = Project::empty();
    project.set_frame_rate(FrameRate::new(30, 1).unwrap());
    project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                SourceMediaLink::from_absolute_path(video.path.clone()),
                probe_video(toolchain, &video.path).expect("the test video can be probed"),
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    assert_eq!(project.total_frame_count(), 90, "3 seconds at 30 a second");
    project
}

#[test]
fn a_video_of_25_pictures_a_second_in_a_project_of_30_gets_its_pictures_as_the_preview_does() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(&toolchain, &TestVideoSpecification::PAL_3_SECONDS);
    let project = project_of_30_a_second_with(&toolchain, &video);
    let expected: Vec<u64> = (0..90).map(|frame| picture_shown_in_frame(25, 30, frame)).collect();
    for in_planes in [false, true] {
        let seen = pictures_of_the_runs(&toolchain, &project, in_planes);
        assert_eq!(the_one_run(&seen).numbers, expected, "in planes: {in_planes}");
        assert_eq!(seen.pictures_held, 0);
    }
}

#[test]
fn a_video_of_24_pictures_a_second_in_a_project_of_30_gets_its_pictures_as_the_preview_does() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(&toolchain, &TestVideoSpecification::FILM_3_SECONDS);
    let project = project_of_30_a_second_with(&toolchain, &video);
    let expected: Vec<u64> = (0..90).map(|frame| picture_shown_in_frame(24, 30, frame)).collect();
    let seen = pictures_of_the_runs(&toolchain, &project, true);
    assert_eq!(the_one_run(&seen).numbers, expected);
}

#[test]
fn a_video_that_is_hidden_for_a_while_by_another_is_read_in_two_runs_that_skip_what_is_hidden() {
    let toolchain = toolchain_or_skip!();
    let behind = counter_video(&toolchain);
    let in_front = test_video(&toolchain, &TestVideoSpecification::PAL_3_SECONDS);
    let clip = |gap_before_frames, first_source_frame, end_source_frame| ClipOfTheVideo {
        gap_before_frames,
        first_source_frame,
        end_source_frame,
        speed: 1,
    };
    // The video in front fills the picture from frame 60 to frame 150, and the one behind it is not seen (and must not be read) there.
    let project = project_with_tracks_of_videos(
        &toolchain,
        &[&behind, &in_front],
        &[(1, &[clip(60, 0, 90)]), (0, &[clip(0, 0, 300)])],
    );
    let seen = pictures_of_the_runs(&toolchain, &project, true);
    assert_eq!(seen.runs.len(), 3, "{:?}", seen.runs);
    // The runs begin in this order: the one behind, from 0; the one in front, from 60; the one behind again, from 150.
    assert_eq!(seen.runs[0].numbers, (0..60).collect::<Vec<u64>>());
    assert_eq!(
        seen.runs[1].numbers,
        (0..90)
            .map(|slot| picture_shown_in_frame(25, 30, slot))
            .collect::<Vec<u64>>()
    );
    assert_eq!(seen.runs[2].numbers, (150..300).collect::<Vec<u64>>());
    assert_eq!(
        seen.started.iter().map(|(first, _)| *first).collect::<Vec<_>>(),
        vec![0, 0, 150],
        "the pictures of the video behind, from 60 to 150, are not read"
    );
    assert_eq!(seen.pictures_held, 0);
}

#[test]
fn two_videos_in_two_places_at_once_are_two_streams_that_each_carry_their_own_numbers() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let clip = |gap_before_frames, first_source_frame, end_source_frame, speed| ClipOfTheVideo {
        gap_before_frames,
        first_source_frame,
        end_source_frame,
        speed,
    };
    // The video in front, 1x then 2x, is in a quarter of the picture; the one behind fills it, at 1x.
    let mut project = project_with_tracks_of_videos(
        &toolchain,
        &[&video, &video],
        &[
            (1, &[clip(50, 200, 260, 1), clip(0, 260, 380, 2)]),
            (0, &[clip(0, 0, 300, 1)]),
        ],
    );
    let in_front = project.tracks()[0].entries()[0].id();
    let also_in_front = project.tracks()[0].entries()[1].id();
    for clip in [in_front, also_in_front] {
        project
            .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.5, 0.5, 0.5, 0.5)))
            .unwrap();
    }
    let seen = pictures_of_the_runs(&toolchain, &project, true);
    assert_eq!(seen.runs.len(), 3, "{:?}", seen.runs);
    // In the order they begin: the one behind (0 to 300), and the two in front (50 to 110, and 110 to 170).
    assert_eq!(seen.runs[0].numbers, (0..300).collect::<Vec<u64>>());
    assert_eq!(seen.runs[1].numbers, (200..260).collect::<Vec<u64>>());
    assert_eq!(seen.runs[2].numbers, (260..380).step_by(2).collect::<Vec<u64>>());
    assert!(seen.runs[0].sizes.iter().all(|&size| size == (320, 180)));
    assert!(seen.runs[1].sizes.iter().all(|&size| size == (160, 90)));
    assert!(seen.runs[2].sizes.iter().all(|&size| size == (160, 90)));
}

/// A video of 3 seconds at 30 a second (90 pictures), which has the same shape and rate as the counter video that is 20 seconds long.
const COUNTER_3_SECONDS: TestVideoSpecification = TestVideoSpecification {
    name: "counter_3_seconds",
    duration_in_seconds: 3,
    ..TestVideoSpecification::COUNTER
};

#[test]
fn a_project_pointed_at_a_shorter_video_gets_no_pictures_past_the_end_of_it_and_holds_nothing() {
    let toolchain = toolchain_or_skip!();
    let long = counter_video(&toolchain); // 600 pictures
    let short = test_video(&toolchain, &COUNTER_3_SECONDS);
    let mut project = project_for(&toolchain, &long);
    // The project was edited on the long video (600 frames) and is pointed at a shorter one, which the project knows is shorter.
    let properties = probe_video(&toolchain, &short.path).unwrap();
    assert_eq!(properties.total_frame_count, 90);
    project
        .relink_source(
            SourceMediaIdentifier::new(1),
            SourceMediaLink::from_absolute_path(short.path.clone()),
            fastforwardrs::project::SourceMediaProperties::Video(properties),
        )
        .unwrap();
    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    assert_eq!(plan.total_frame_count, 600);
    assert_eq!(plan.frames_missing_a_picture, 510);
    let seen = pictures_of_the_runs(&toolchain, &project, true);
    assert_eq!(the_one_run(&seen).numbers, (0..90).collect::<Vec<u64>>());
    assert_eq!(seen.pictures_held, 0);
}

#[test]
fn a_file_with_fewer_pictures_than_the_project_was_told_holds_its_last_picture_and_says_how_many_it_held() {
    let toolchain = toolchain_or_skip!();
    let short = test_video(&toolchain, &COUNTER_3_SECONDS);
    // The project is told that the file has 300 pictures, as it did when the project was made, and the file has since been replaced by one that has 90.
    let mut told = probe_video(&toolchain, &short.path).unwrap();
    told.total_frame_count = 300;
    let mut project = Project::empty();
    project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                SourceMediaLink::from_absolute_path(short.path.clone()),
                told,
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    let seen = pictures_of_the_runs(&toolchain, &project, true);
    let run = the_one_run(&seen);
    let expected: Vec<u64> = (0..90).chain(std::iter::repeat_n(89, 210)).collect();
    assert_eq!(run.numbers, expected);
    assert_eq!(seen.pictures_held, 210);
}
