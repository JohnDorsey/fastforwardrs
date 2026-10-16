// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an export is going to do, as the plan says it: how long, how big, from which files, in which stretches of video, and with what sound. (How the
//! stretches are read and what is made of them is in the tests of the pieces that do it.)

use std::path::PathBuf;

use super::super::test_projects::{
    audio_source, clip_location, frame, picture_source, project_with_video, speed, video_source,
};
use super::*;
use crate::project::{
    Angle, AnimationTarget, CanvasArea, ClipId, Placement, PropertyId, PropertyValue, SourceMediaLink,
    SourceMediaProperties, SpedUpClipSound, SpeedIndicatorLayout, TextObject,
};
use crate::test_support::make_a_keyframe;

pub(super) fn plan(project: &Project) -> Result<ExportPlan, ExportPlanError> {
    plan_export(project, Settings::DEFAULT)
}

#[test]
fn a_project_with_nothing_on_its_tracks_cannot_be_exported() {
    assert_eq!(plan(&Project::empty()).unwrap_err(), ExportPlanError::NothingToExport);
    let mut with_a_file_but_no_clips = Project::empty();
    with_a_file_but_no_clips
        .import_source(video_source(1, 320, 180, 0, false), frame(0))
        .unwrap();
    assert_eq!(
        plan(&with_a_file_but_no_clips).unwrap_err(),
        ExportPlanError::NothingToExport,
        "a video with no frames has no clip"
    );
    assert_eq!(ExportPlanError::NothingToExport.heading(), "There is nothing to export");
}

#[test]
fn the_plan_reports_the_length_of_the_finished_video_in_frames_and_its_frame_rate() {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.total_frame_count, 100 + 50);
    assert_eq!(plan.frame_rate, FrameRate::new(30, 1).unwrap());
    assert_eq!(plan.frames_missing_a_picture, 0);
    assert_eq!(plan.pictures.total_frame_count, plan.total_frame_count);
}

#[test]
fn the_plan_carries_the_output_colour_space_of_the_settings_and_what_the_frames_are_encoded_as() {
    use crate::project::OutputColourSpace;
    let project = project_with_video(320, 180, false);
    for output in OutputColourSpace::ALL {
        let settings = Settings {
            output_colour_space: output,
            ..Settings::DEFAULT
        };
        let plan = plan_export(&project, settings).unwrap();
        assert_eq!(plan.output_colour_space, output);
        assert_eq!(plan.colour, colour_of_the_export(settings), "{output:?}");
    }
    assert_eq!(
        plan(&project).unwrap().output_colour_space,
        OutputColourSpace::Rec709AsPreviewed,
        "the default"
    );
}

#[test]
fn a_project_of_a_single_frame_is_planned_as_a_video_of_a_single_frame() {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, 320, 180, 1, false), frame(0))
        .unwrap();

    let plan = plan(&project).unwrap();

    assert_eq!(plan.total_frame_count, 1);
    assert_eq!(plan.pictures.sections.len(), 1);
    assert_eq!(plan.runs.len(), 1);
    assert_eq!(
        (plan.runs[0].first_timeline_frame, plan.runs[0].end_timeline_frame),
        (0, 1)
    );
    assert_eq!(plan.runs[0].picture_count(), 1);
}

#[test]
fn the_symbol_is_where_the_project_says_it_goes() {
    let mut project = project_with_video(320, 180, false);
    assert_eq!(plan(&project).unwrap().symbol_layout, SpeedIndicatorLayout::DEFAULT);
    let elsewhere = SpeedIndicatorLayout::new(0.5, 0.25, 0.2);
    assert_ne!(elsewhere, SpeedIndicatorLayout::DEFAULT);
    project.set_speed_indicator_layout(elsewhere);

    assert_eq!(plan(&project).unwrap().symbol_layout, elsewhere);
}

#[test]
fn odd_picture_sizes_are_rounded_down_to_even_ones() {
    let plan_for = |width, height| {
        let plan = plan(&project_with_video(width, height, false)).unwrap();
        (plan.picture_width_in_pixels, plan.picture_height_in_pixels)
    };
    assert_eq!(plan_for(321, 181), (320, 180));
    assert_eq!(plan_for(1, 1), (2, 2));
    assert_eq!(plan_for(320, 180), (320, 180));
    assert_eq!(
        plan(&project_with_video(321, 181, false)).unwrap().picture_size(),
        (320, 180)
    );
}

#[test]
fn a_project_with_no_video_at_all_exports_black_the_standard_size_and_has_no_run_and_no_source_file() {
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    project.insert_text(track, frame(0), TextObject::default(), 60).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.picture_size(), (1920, 1080));
    assert!(plan.source_files.is_empty());
    assert!(plan.runs.is_empty());
    assert!(plan.videos.is_empty() && plan.images.is_empty());
    assert!(!plan.has_audio && plan.sound.is_none());
}

// --- files that are shorter than the clips ---

/// A project edited on a video of 300 frames and then pointed at a shorter one: clips 0..100 at 1x, 100..200 at 4x and 200..300 at 1x, on a video of
/// `frame_count` frames.
fn project_on_a_replacement_video_of(frame_count: u64) -> Project {
    let mut project = project_with_video(320, 180, true);
    project.split_at(frame(100), None);
    project.split_at(frame(200), None);
    let middle = clip_location(&project, 0, 1);
    project.set_clip_speed(middle, speed(4)).unwrap();
    let SourceMediaProperties::Video(mut shorter) = video_source(1, 320, 180, frame_count, true).properties else {
        unreachable!()
    };
    shorter.total_frame_count = frame_count;
    project
        .relink_source(
            SourceMediaIdentifier::new(1),
            SourceMediaLink::from_absolute_path(PathBuf::from("/elsewhere/shorter.mp4")),
            SourceMediaProperties::Video(shorter),
        )
        .unwrap();
    project
}

#[test]
fn a_project_on_its_own_video_is_missing_no_pictures() {
    let plan = plan(&project_with_video(320, 180, true)).unwrap();
    assert_eq!(plan.total_frame_count, 300);
    assert_eq!(plan.frames_missing_a_picture, 0);
}

/// A clip that reaches past the end of its file does not make the finished video shorter (which would put everything after it out of step with the other tracks):
/// its frames are black, and silent, instead; and no run is read past the end of the file.
#[test]
fn clips_reaching_past_the_end_of_a_shorter_replacement_video_are_black_for_the_rest_and_the_length_is_kept() {
    // The video has 130 frames. Clip 1 (100..200 at 4x) keeps source frames 100, 104, ... 128: 8 of its 25 timeline frames. Clip 2 (200..300) has none.
    let project = project_on_a_replacement_video_of(130);
    assert_eq!(project.total_frame_count(), 100 + 25 + 100);
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.total_frame_count,
        100 + 25 + 100,
        "the project is as long as it was"
    );
    assert_eq!(plan.frames_missing_a_picture, 17 + 100);
    assert_eq!(plan.runs.len(), 2, "{:?}", plan.runs);
    let (first, second) = (&plan.runs[0], &plan.runs[1]);
    assert_eq!(
        (
            first.first_timeline_frame,
            first.end_timeline_frame,
            first.first_source_frame,
            first.source_frame_count
        ),
        (0, 100, 0, 100)
    );
    assert_eq!(
        (
            second.first_timeline_frame,
            second.end_timeline_frame,
            second.first_source_frame,
            second.source_frame_count
        ),
        (100, 108, 100, 30),
        "8 pictures, of the source frames 100, 104 ... 128: the frames 100 to 129, which are the end of the file, are read"
    );
    assert_eq!(second.picture_count(), 8);
}

#[test]
fn the_plan_is_missing_exactly_as_many_frames_as_there_are_frames_of_clips_beyond_the_end_of_the_video() {
    for frame_count in [1, 50, 99, 100, 101, 130, 200, 201, 299, 300, 1000] {
        let project = project_on_a_replacement_video_of(frame_count);
        let plan = plan(&project).unwrap();

        // Found one frame at a time: each frame whose source frame the video does not have.
        let placements = project.tracks()[0].placements();
        let missing: u64 = placements
            .map(|placement| {
                (0..placement.timeline_frame_count())
                    .filter(|&offset| {
                        placement
                            .source_frame_at(frame(placement.first_timeline_frame.value() + offset))
                            .unwrap()
                            .value()
                            >= frame_count
                    })
                    .count() as u64
            })
            .sum();
        assert_eq!(plan.frames_missing_a_picture, missing, "video of {frame_count} frames");
        assert_eq!(
            plan.total_frame_count,
            project.total_frame_count(),
            "video of {frame_count} frames"
        );
        // Every picture of every run is a picture that the file has, and the runs have as many pictures as there are frames with a picture.
        let pictures: u64 = plan.runs.iter().map(VideoRun::picture_count).sum();
        assert_eq!(
            pictures,
            plan.total_frame_count - missing,
            "video of {frame_count} frames"
        );
        for run in &plan.runs {
            assert!(run.first_source_frame + run.source_frame_count <= frame_count);
        }
    }
}

// --- sound ---

#[test]
fn the_setting_for_sped_up_sound_reaches_the_specification_of_the_sound() {
    let mut project = project_with_video(320, 180, true);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    for sound in SpedUpClipSound::ALL {
        let plan = plan_export(
            &project,
            Settings {
                sped_up_clip_sound: sound,
                ..Settings::DEFAULT
            },
        )
        .unwrap();
        assert_eq!(plan.sound.unwrap().specification.sped_up_clip_sound, sound);
    }
}

#[test]
fn sound_is_part_of_the_plan_only_when_something_on_the_tracks_has_it() {
    let with_sound = plan(&project_with_video(320, 180, true)).unwrap();
    assert!(with_sound.has_audio);
    let sound = with_sound.sound.unwrap();
    assert_eq!(sound.source_files, vec![SourceMediaIdentifier::new(1)]);
    assert_eq!(sound.specification.total_frame_count, 300);
    let without = plan(&project_with_video(320, 180, false)).unwrap();
    assert!(!without.has_audio);
    assert!(without.sound.is_none());
}

#[test]
fn a_muted_clip_or_track_leaves_the_sound_out() {
    let mut project = project_with_video(320, 180, true);
    let only_clip = clip_location(&project, 0, 0);
    project.set_clip_muted(only_clip, true).unwrap();
    assert!(!plan(&project).unwrap().has_audio);
    project.set_clip_muted(only_clip, false).unwrap();
    project.set_track_muted(project.tracks()[0].id(), true).unwrap();
    assert!(!plan(&project).unwrap().has_audio);
}

// --- several files and tracks ---

#[test]
fn the_source_files_are_every_file_that_anything_is_made_from_in_order_of_their_numbers() {
    let mut project = project_with_video(320, 180, true);
    project.import_source(audio_source(7, 2), frame(0)).unwrap();
    project.import_source(picture_source(4, 50, 50), frame(0)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.source_files,
        vec![
            SourceMediaIdentifier::new(1),
            SourceMediaIdentifier::new(4),
            SourceMediaIdentifier::new(7)
        ]
    );
    assert_eq!(
        plan.sound.unwrap().source_files,
        vec![SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(7)],
        "the sound is made from the video's sound and the sound file; the picture has none"
    );
    assert!(plan.videos.contains_key(&SourceMediaIdentifier::new(1)));
    assert_eq!(
        plan.videos[&SourceMediaIdentifier::new(1)].display_size_in_pixels,
        (320, 180)
    );
    assert_eq!(
        plan.videos[&SourceMediaIdentifier::new(1)].frame_rate,
        FrameRate::new(30, 1).unwrap()
    );
    assert!(plan.images.contains_key(&SourceMediaIdentifier::new(4)));
    assert_eq!(
        plan.images[&SourceMediaIdentifier::new(4)].display_size_in_pixels,
        (50, 50)
    );
}

#[test]
fn a_file_used_only_for_its_sound_is_still_a_source_file_and_has_no_run() {
    // A video with sound whose picture is hidden behind another video that is in front of it for its whole length.
    let mut project = project_with_video(320, 180, true);
    project
        .import_source(video_source(2, 320, 180, 300, false), frame(0))
        .unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.source_files,
        vec![SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(2)]
    );
    assert_eq!(plan.sound.unwrap().source_files, vec![SourceMediaIdentifier::new(1)]);
    assert_eq!(
        plan.runs.len(),
        1,
        "the picture of the one behind is not read: {:?}",
        plan.runs
    );
    assert_eq!(plan.runs[0].source, SourceMediaIdentifier::new(2));
}

// --- the runs of video ---

#[test]
fn one_clip_is_one_run_and_the_run_has_the_clips_frames_and_speed() {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.runs.len(), 2);
    let (first, second_run) = (&plan.runs[0], &plan.runs[1]);
    assert_eq!(first.clip, clip_location(&project, 0, 0));
    assert_eq!(second_run.clip, second);
    assert_eq!(
        (
            first.first_timeline_frame,
            first.end_timeline_frame,
            first.first_source_frame,
            first.source_frame_count
        ),
        (0, 100, 0, 100)
    );
    assert_eq!(first.speed, speed(1));
    assert_eq!(
        (
            second_run.first_timeline_frame,
            second_run.end_timeline_frame,
            second_run.first_source_frame,
            second_run.source_frame_count
        ),
        (100, 150, 100, 200)
    );
    assert_eq!(second_run.speed, speed(4));
    assert_eq!(second_run.picture_count(), 50);
    assert_eq!(second_run.source_frame_of_picture(0), 100);
    assert_eq!(second_run.source_frame_of_picture(49), 296);
    assert_eq!(plan.run_of(second, 100), Some(1));
    assert_eq!(plan.run_of(second, 149), Some(1));
    assert_eq!(plan.run_of(second, 150), None);
    assert_eq!(plan.run_of(second, 99), None);
}

#[test]
fn a_speed_that_does_not_divide_the_clip_reads_the_last_pictures_group_only_as_far_as_the_clip_goes() {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(10), None);
    let second = clip_location(&project, 0, 1);
    // 290 source frames at 7x: 42 pictures (the last group has 3 frames, 287 to 289).
    project.set_clip_speed(second, speed(7)).unwrap();
    let plan = plan(&project).unwrap();
    let run = plan.runs.iter().find(|run| run.clip == second).unwrap();
    assert_eq!(run.picture_count(), 42);
    assert_eq!(run.source_frame_count, 290);
    assert_eq!(run.source_frame_count.div_ceil(7), run.picture_count());
}

#[test]
fn a_text_over_the_video_cuts_the_timeline_into_sections_and_not_the_video_into_runs() {
    let mut project = project_with_video(320, 180, false);
    let front = project.add_track_in_front();
    project
        .insert_text(front, frame(50), TextObject::default(), 30)
        .unwrap();
    project
        .insert_text(front, frame(150), TextObject::default(), 20)
        .unwrap();
    let plan = plan(&project).unwrap();
    assert!(plan.pictures.sections.len() >= 5, "{:?}", plan.pictures.sections.len());
    assert_eq!(
        plan.runs.len(),
        1,
        "the video is one run however many sections it is in"
    );
    let run = &plan.runs[0];
    assert_eq!(
        (run.first_timeline_frame, run.end_timeline_frame, run.source_frame_count),
        (0, 300, 300)
    );
}

#[test]
fn a_video_that_is_hidden_for_a_while_by_another_in_front_of_it_is_a_run_before_and_a_run_after() {
    let mut project = project_with_video(320, 180, false);
    // A second video of 100 frames, in front, from frame 100, that fills the picture: the one behind it is not seen (and not read) from 100 to 200.
    project
        .import_source(video_source(2, 320, 180, 100, false), frame(100))
        .unwrap();
    let plan = plan(&project).unwrap();
    let behind = clip_location(&project, 1, 0);
    let back_runs: Vec<&VideoRun> = plan.runs.iter().filter(|run| run.clip == behind).collect();
    assert_eq!(back_runs.len(), 2, "{:?}", plan.runs);
    assert_eq!(
        (
            back_runs[0].first_timeline_frame,
            back_runs[0].end_timeline_frame,
            back_runs[0].first_source_frame
        ),
        (0, 100, 0)
    );
    assert_eq!(
        (
            back_runs[1].first_timeline_frame,
            back_runs[1].end_timeline_frame,
            back_runs[1].first_source_frame
        ),
        (200, 300, 200),
        "the pictures after the gap are the pictures that are at that time in the video"
    );
    assert_eq!(plan.runs.len(), 3);
    assert_eq!(plan.run_of(behind, 150), None);
}

#[test]
fn runs_are_in_the_order_they_begin() {
    let mut project = project_with_video(320, 180, false);
    project
        .import_source(video_source(2, 320, 180, 50, false), frame(40))
        .unwrap();
    let plan = plan(&project).unwrap();
    let firsts: Vec<u64> = plan.runs.iter().map(|run| run.first_timeline_frame).collect();
    let mut sorted = firsts.clone();
    sorted.sort_unstable();
    assert_eq!(firsts, sorted);
}

// --- how big a run is delivered ---

fn project_with_a_small_video(frames: u64) -> (Project, ClipId) {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, 640, 360, frames, false), frame(0))
        .unwrap();
    let clip = clip_location(&project, 0, 0);
    project
        .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.0, 0.0, 0.25, 0.25)))
        .unwrap();
    (project, clip)
}

#[test]
fn a_video_that_fills_the_picture_is_delivered_as_big_as_the_picture() {
    let plan = plan(&project_with_video(640, 360, false)).unwrap();
    assert_eq!(plan.runs[0].delivered_size_in_pixels, (640, 360));
}

#[test]
fn a_video_that_was_put_in_a_smaller_place_is_delivered_as_big_as_the_place() {
    let (project, _) = project_with_a_small_video(300);
    let plan = plan(&project).unwrap();
    assert_eq!(plan.runs[0].delivered_size_in_pixels, (160, 90));
}

#[test]
fn a_video_that_keyframes_make_bigger_and_smaller_is_delivered_as_big_as_the_biggest_it_gets() {
    let (mut project, clip) = project_with_a_small_video(300);
    // Size is the width and height of the box, as fractions of the picture: a quarter at the start, half at frame 100, a quarter again at frame 200.
    for (at, fraction) in [(0, 0.25), (100, 0.5), (200, 0.25)] {
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(clip),
            PropertyId::Size,
            at,
            PropertyValue::Pair(fraction, fraction),
        );
    }
    let plan = plan(&project).unwrap();
    assert_eq!(plan.runs[0].delivered_size_in_pixels, (320, 180));
}

#[test]
fn turning_a_video_does_not_change_how_big_it_is_delivered() {
    let (mut project, clip) = project_with_a_small_video(300);
    project
        .set_clip_placement(
            clip,
            Placement::new(CanvasArea::new(0.0, 0.0, 0.25, 0.25), Angle::from_degrees(30.0)),
        )
        .unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.runs[0].delivered_size_in_pixels, (160, 90));
}

#[test]
fn a_picture_file_is_decoded_as_big_as_the_biggest_place_it_has_and_a_text_has_no_file_at_all() {
    let mut project = project_with_video(640, 360, false);
    project.import_source(picture_source(2, 1000, 500), frame(0)).unwrap();
    let image_clip = clip_location(&project, 0, 0);
    project
        .set_clip_placement(image_clip, Placement::upright(CanvasArea::new(0.1, 0.1, 0.5, 0.5)))
        .unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.images[&SourceMediaIdentifier::new(2)].largest_box_in_pixels,
        (320, 180)
    );
    assert_eq!(plan.images.len(), 1);
}

#[test]
fn each_picture_file_is_decoded_as_big_as_the_biggest_place_that_it_has_itself_and_not_one_of_another_file() {
    let mut project = project_with_video(640, 360, false);
    project.import_source(picture_source(2, 1000, 500), frame(0)).unwrap();
    let big = clip_location(&project, 0, 0);
    project
        .set_clip_placement(big, Placement::upright(CanvasArea::new(0.1, 0.1, 0.5, 0.5)))
        .unwrap();
    project.import_source(picture_source(3, 400, 400), frame(0)).unwrap();
    let small = clip_location(&project, 0, 0);
    project
        .set_clip_placement(small, Placement::upright(CanvasArea::new(0.0, 0.0, 0.25, 0.25)))
        .unwrap();

    let plan = plan(&project).unwrap();

    assert_eq!(
        plan.images[&SourceMediaIdentifier::new(2)].largest_box_in_pixels,
        (320, 180)
    );
    assert_eq!(
        plan.images[&SourceMediaIdentifier::new(3)].largest_box_in_pixels,
        (160, 90)
    );
}

#[test]
fn a_picture_file_that_is_in_two_places_is_decoded_as_big_as_the_bigger_of_them_whichever_comes_first() {
    for (first, second) in [(0.75, 0.25), (0.25, 0.75)] {
        let mut project = project_with_video(640, 360, false);
        project.import_source(picture_source(2, 1000, 500), frame(0)).unwrap();
        project.split_at(frame(100), None);
        let (before, after) = (clip_location(&project, 0, 0), clip_location(&project, 0, 1));
        for (clip, size) in [(before, first), (after, second)] {
            project
                .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.0, 0.0, size, size)))
                .unwrap();
        }

        let plan = plan(&project).unwrap();

        assert_eq!(
            plan.images[&SourceMediaIdentifier::new(2)].largest_box_in_pixels,
            (480, 270),
            "{first} and then {second} of the picture"
        );
    }
}
