// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which sections of the picture plan a clip's video goes on over as one run (one decode process): when the pictures of the next section are the very pictures that follow
//! the ones of the section before, and in no other case. The plans here are made by hand, so that a clip can do what the plan of a project never makes it do (change its file
//! or its speed between two sections, skip a picture): what makes a run go on has to be exactly what is said here, whatever the planning does.

use super::*;
use crate::composition::{PictureSection, VisibleVideo};
use crate::project::{Animation, CanvasArea, FrameRate, Placement, TrackId};
use crate::video_export::test_projects::speed;

const PICTURE: (u32, u32) = (320, 180);

fn clip(number: u32) -> ClipId {
    ClipId::new(TrackId::new(0), number)
}

/// A layer that is the video of `clip_number`: the pictures of the file `source` from the source frame `first_source_frame` up to `end_source_frame`, at `factor` times normal speed.
fn video_layer(clip_number: u32, source: u32, first_source_frame: u64, end_source_frame: u64, factor: u32) -> Layer {
    Layer {
        clip: clip(clip_number),
        track: TrackId::new(0),
        content: LayerContent::Video(VisibleVideo {
            source: SourceMediaIdentifier::new(source),
            first_source_frame,
            end_source_frame,
            speed: speed(factor),
        }),
        placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
        animation: Animation::none(),
        effects: Vec::new(),
    }
}

/// A plan of these sections: `(first frame, end frame, layers)`.
fn plan_of(sections: Vec<(u64, u64, Vec<Layer>)>) -> PicturePlan {
    PicturePlan {
        frame_rate: FrameRate::new(30, 1).unwrap(),
        picture_size: PICTURE,
        total_frame_count: sections.last().map_or(0, |section| section.1),
        sections: sections
            .into_iter()
            .map(|(first_timeline_frame, end_timeline_frame, layers)| PictureSection {
                first_timeline_frame,
                end_timeline_frame,
                layers,
                groups: Vec::new(),
                masks: Vec::new(),
            })
            .collect(),
        frames_missing_a_picture: 0,
    }
}

fn runs_of(plan: &PicturePlan) -> Vec<VideoRun> {
    find_the_runs(plan, PICTURE)
}

#[test]
fn the_pictures_that_follow_those_of_the_section_before_are_read_on_by_the_same_run() {
    // Frames 0 to 10 show the source frames 0 to 10, and frames 10 to 30 the source frames 10 to 30: one file is read, from the first to the last.
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1)]),
        (10, 30, vec![video_layer(0, 1, 10, 30, 1)]),
    ]);

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.clip, clip(0));
    assert_eq!(run.source, SourceMediaIdentifier::new(1));
    assert_eq!((run.first_timeline_frame, run.end_timeline_frame), (0, 30));
    assert_eq!((run.first_source_frame, run.source_frame_count), (0, 30));
    assert_eq!(run.picture_count(), 30);
}

#[test]
fn at_a_speed_the_pictures_that_follow_are_as_many_source_frames_on() {
    // At 2x, 10 frames of the timeline are 20 source frames, so the next section's pictures begin at the source frame 20, and at 3x 30 frames of the timeline are
    // 90 source frames.
    for (factor, frames_of_the_first, frames_of_the_second) in [(2_u32, 10_u64, 20_u64), (3, 10, 20), (4, 7, 9)] {
        let first_ends_at = u64::from(factor) * frames_of_the_first;
        let second_ends_at = first_ends_at + u64::from(factor) * frames_of_the_second;
        let plan = plan_of(vec![
            (
                0,
                frames_of_the_first,
                vec![video_layer(0, 1, 0, first_ends_at, factor)],
            ),
            (
                frames_of_the_first,
                frames_of_the_first + frames_of_the_second,
                vec![video_layer(0, 1, first_ends_at, second_ends_at, factor)],
            ),
        ]);

        let runs = runs_of(&plan);

        assert_eq!(runs.len(), 1, "at {factor}x");
        assert_eq!(runs[0].source_frame_count, second_ends_at, "at {factor}x");
        assert_eq!(runs[0].end_timeline_frame, frames_of_the_first + frames_of_the_second);
        assert_eq!(runs[0].speed, speed(factor));
    }
}

#[test]
fn a_clip_that_goes_on_with_other_pictures_than_the_ones_that_follow_is_another_run() {
    // The second section skips a source frame: reading on from the first would give a picture of the wrong frame.
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1)]),
        (10, 30, vec![video_layer(0, 1, 11, 31, 1)]),
    ]);

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 2);
    assert_eq!((runs[0].first_timeline_frame, runs[0].end_timeline_frame), (0, 10));
    assert_eq!((runs[1].first_timeline_frame, runs[1].end_timeline_frame), (10, 30));
    assert_eq!(runs[1].first_source_frame, 11);
}

#[test]
fn a_clip_that_goes_on_in_another_file_is_another_run() {
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1)]),
        (10, 30, vec![video_layer(0, 2, 10, 30, 1)]),
    ]);

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].source, SourceMediaIdentifier::new(1));
    assert_eq!(runs[1].source, SourceMediaIdentifier::new(2));
}

#[test]
fn a_clip_that_goes_on_at_another_speed_is_another_run() {
    // (The second section's pictures begin where the ones of the first end, if the first speed is the one that counts.)
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1)]),
        (10, 30, vec![video_layer(0, 1, 10, 50, 2)]),
    ]);

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].speed, speed(1));
    assert_eq!(runs[1].speed, speed(2));
}

#[test]
fn a_clip_that_was_not_seen_in_a_section_is_not_read_on_after_it_though_its_pictures_would_follow() {
    // The video is hidden by another in front of it from frame 10 to frame 20 and is seen again after that, and its pictures (by this plan, which is made up) begin where
    // they ended: nothing was read for the frames in between, so what is read on would be pictures of the wrong frames.
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1)]),
        (10, 20, vec![video_layer(1, 2, 0, 10, 1)]),
        (20, 30, vec![video_layer(0, 1, 10, 20, 1)]),
    ]);

    let runs = runs_of(&plan);

    let of_the_clip: Vec<(u64, u64)> = runs
        .iter()
        .filter(|run| run.clip == clip(0))
        .map(|run| (run.first_timeline_frame, run.end_timeline_frame))
        .collect();
    assert_eq!(of_the_clip, vec![(0, 10), (20, 30)]);
    assert_eq!(runs.len(), 3);
}

#[test]
fn two_clips_at_once_are_two_runs_that_each_go_on_by_themselves() {
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1), video_layer(1, 2, 100, 110, 1)]),
        (
            10,
            20,
            vec![video_layer(0, 1, 10, 20, 1), video_layer(1, 2, 110, 120, 1)],
        ),
    ]);

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 2);
    assert_eq!(
        runs.iter()
            .map(|run| (run.clip, run.first_source_frame, run.source_frame_count))
            .collect::<Vec<_>>(),
        vec![(clip(0), 0, 20), (clip(1), 100, 20)]
    );
}

#[test]
fn a_picture_that_is_not_a_video_has_no_run_and_does_not_cut_the_video_under_it() {
    let mut a_text_over_the_video = video_layer(1, 2, 0, 0, 1);
    a_text_over_the_video.content = LayerContent::Text(crate::composition::LayerEffect {
        state: None,
        clock: Default::default(),
        instance: crate::project::EffectInstanceId::FIRST,
        kind: crate::project::EffectKind::TEXT,
        values: crate::project::values_of_a_text_object(&crate::project::TextObject::default()).to_vec(),
        animation: Default::default(),
        tables: vec![None; 4],
        tail_length: Default::default(),
        passes_notes_on: false,
        shared_by: None,
        driven: Default::default(),
    });
    let plan = plan_of(vec![
        (0, 10, vec![video_layer(0, 1, 0, 10, 1)]),
        (10, 20, vec![video_layer(0, 1, 10, 20, 1), a_text_over_the_video]),
        (20, 30, vec![video_layer(0, 1, 20, 30, 1)]),
    ]);

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].first_timeline_frame, runs[0].end_timeline_frame), (0, 30));
    assert_eq!(runs[0].source_frame_count, 30);
}

#[test]
fn a_plan_with_no_video_has_no_runs() {
    assert!(runs_of(&plan_of(Vec::new())).is_empty());
    assert!(runs_of(&plan_of(vec![(0, 10, Vec::new())])).is_empty());
}

// --- masks ---

/// A mask that is the video `layer`, which is the picture of its track over the frames `first..end` of the timeline.
fn a_mask_of(layer: Layer, first: u64, end: u64) -> crate::composition::MaskPicture {
    crate::composition::MaskPicture {
        which: crate::project::MaskUse::of_a_track(layer.track),
        layers_behind: 0,
        section: PictureSection {
            first_timeline_frame: first,
            end_timeline_frame: end,
            layers: vec![layer],
            groups: Vec::new(),
            masks: Vec::new(),
        },
    }
}

#[test]
fn the_video_of_a_mask_is_a_run_of_its_own_with_its_source_frames_counted_from_the_first_frame_of_the_section_that_has_it()
 {
    // A section of frames 40 to 70 that has a mask whose video is the picture of its track over frames 0 to 100: at the frame 40 it shows the source frame 500 + 40.
    let mut plan = plan_of(vec![(40, 70, vec![video_layer(0, 1, 40, 70, 1)])]);
    plan.sections[0].masks = vec![a_mask_of(video_layer(5, 2, 500, 600, 1), 0, 100)];

    let runs = runs_of(&plan);

    assert_eq!(
        runs.len(),
        2,
        "the video that is in the picture, and the one that is only a mask"
    );
    let the_mask = runs
        .iter()
        .find(|run| run.clip == clip(5))
        .expect("the mask's video has a run");
    assert_eq!(the_mask.source, SourceMediaIdentifier::new(2));
    assert_eq!((the_mask.first_timeline_frame, the_mask.end_timeline_frame), (40, 70));
    assert_eq!((the_mask.first_source_frame, the_mask.source_frame_count), (540, 30));
}

#[test]
fn the_pieces_a_section_is_cut_into_by_a_mask_go_on_as_one_run_for_the_video_of_the_mask_as_well() {
    // The mask's track shows one video over frames 0 to 100; the section is cut into three pieces, each of which has the whole of that stretch.
    let mask = || a_mask_of(video_layer(5, 2, 500, 600, 1), 0, 100);
    let mut plan = plan_of(vec![
        (0, 30, vec![video_layer(0, 1, 0, 30, 1)]),
        (30, 60, vec![video_layer(0, 1, 30, 60, 1)]),
        (60, 100, vec![video_layer(0, 1, 60, 100, 1)]),
    ]);
    for section in &mut plan.sections {
        section.masks = vec![mask()];
    }

    let runs = runs_of(&plan);

    assert_eq!(runs.len(), 2, "one run for each video, however many pieces: {runs:?}");
    let the_mask = runs.iter().find(|run| run.clip == clip(5)).unwrap();
    assert_eq!((the_mask.first_timeline_frame, the_mask.end_timeline_frame), (0, 100));
    assert_eq!((the_mask.first_source_frame, the_mask.source_frame_count), (500, 100));
}

#[test]
fn a_clip_that_is_in_the_picture_and_in_a_mask_has_one_run() {
    let mut plan = plan_of(vec![(0, 50, vec![video_layer(0, 1, 0, 50, 1)])]);
    plan.sections[0].masks = vec![a_mask_of(video_layer(0, 1, 0, 50, 1), 0, 50)];

    assert_eq!(runs_of(&plan).len(), 1);
}
