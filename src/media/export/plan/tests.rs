// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use super::*;
use crate::project::{
    Angle, CanvasArea, Placement, SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaLink,
    SourceVideoProperties, SpedUpClipSound, SpeedIndicatorLayout, TextColour, fit_inside_picture,
};

pub(super) fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

pub(super) fn video_source(identifier: u32, width: u32, height: u32, frames: u64, has_audio: bool) -> SourceMedia {
    SourceMedia::video(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/videos/{identifier}.mp4"))),
        SourceVideoProperties {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: frames,
            display_width_in_pixels: width,
            display_height_in_pixels: height,
            has_audio,
        },
    )
}

pub(super) fn picture_source(identifier: u32, width: u32, height: u32) -> SourceMedia {
    SourceMedia::image(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/pictures/{identifier}.png"))),
        SourceImageProperties {
            display_width_in_pixels: width,
            display_height_in_pixels: height,
        },
    )
}

fn audio_source(identifier: u32, seconds: u64) -> SourceMedia {
    SourceMedia::audio(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/sounds/{identifier}.mp3"))),
        SourceAudioProperties {
            duration_in_microseconds: seconds * 1_000_000,
        },
    )
}

pub(super) fn project_with_video(width: u32, height: u32, has_audio: bool) -> Project {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, width, height, 300, has_audio), frame(0))
        .unwrap();
    project
}

/// A picture the size of the text's area, filled with white, standing in for rendered text.
pub(super) fn stand_in_for_text(object: &TextObject, picture_width: u32, picture_height: u32) -> RgbaPicture {
    let (width, height) = crate::text_rendering::area_size_in_pixels(object, picture_width, picture_height);
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: vec![255; (width * height * 4) as usize],
    }
}

pub(super) fn plan(project: &Project) -> Result<ExportPlan, ExportPlanError> {
    plan_export(project, Settings::DEFAULT, &stand_in_for_text)
}

fn plan_with(project: &Project, settings: Settings) -> ExportPlan {
    plan_export(project, settings, &stand_in_for_text).unwrap()
}

pub(super) fn clip_location(project: &Project, track_position: usize, clip_number: usize) -> ClipId {
    project.tracks()[track_position].entries()[clip_number].id()
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
}

#[test]
fn the_plan_reports_the_length_of_the_finished_video_in_frames() {
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.total_frame_count, 100 + 50);
    assert_eq!(plan.frame_rate, FrameRate::new(30, 1).unwrap());
    assert_eq!(plan.frames_missing_a_picture, 0);
}

#[test]
fn odd_picture_sizes_are_rounded_down_to_even_ones() {
    let plan_for = |width, height| {
        let plan = plan(&project_with_video(width, height, false)).unwrap();
        (plan.picture_width_in_pixels, plan.picture_height_in_pixels)
    };
    assert_eq!(plan_for(321, 181), (320, 180));
    assert_eq!(plan_for(1, 1), (2, 2));
}

#[test]
fn a_project_with_no_video_at_all_exports_black_the_standard_size() {
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    project.insert_text(track, frame(0), TextObject::default(), 60).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        (plan.picture_width_in_pixels, plan.picture_height_in_pixels),
        (1920, 1080)
    );
    assert!(plan.source_files.is_empty());
    assert!(
        plan.filter_graph.text.contains("color=c=black:s=1920x1080"),
        "{}",
        plan.filter_graph.text
    );
    // The text's own picture is the only input there is, so it is input 0.
    assert!(
        plan.filter_graph
            .text
            .starts_with("[0:v]split=1[still_picture_0_copy_0]"),
        "{}",
        plan.filter_graph.text
    );
    assert_eq!(plan.still_pictures.len(), 1);
}

// --- files that are shorter than the clips ---

/// A project edited on a video of 300 frames and then pointed at a shorter one: clips 0..100 at
/// 1x, 100..200 at 4x and 200..300 at 1x, on a video of `frame_count` frames.
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

use crate::project::SourceMediaProperties;

#[test]
fn a_project_on_its_own_video_is_missing_no_pictures() {
    let plan = plan(&project_with_video(320, 180, true)).unwrap();
    assert_eq!(plan.total_frame_count, 300);
    assert_eq!(plan.frames_missing_a_picture, 0);
}

/// A behaviour that changed when there came to be several tracks: a clip that reaches past the end
/// of its file no longer makes the finished video shorter (which would put everything after it
/// out of step with the other tracks). Its frames are black, and silent, instead.
#[test]
fn clips_reaching_past_the_end_of_a_shorter_replacement_video_are_black_for_the_rest_and_the_length_is_kept() {
    // The video has 130 frames. Clip 1 (100..200 at 4x) keeps source frames 100, 104, ... 128:
    // 8 of its 25 timeline frames. Clip 2 (200..300) has none.
    let project = project_on_a_replacement_video_of(130);
    assert_eq!(project.total_frame_count(), 100 + 25 + 100);

    let plan = plan(&project).unwrap();

    assert_eq!(
        plan.total_frame_count,
        100 + 25 + 100,
        "the project is as long as it was"
    );
    assert_eq!(plan.frames_missing_a_picture, 17 + 100);
    let text = &plan.filter_graph.text;
    assert!(text.contains("trim=start_frame=100:end_frame=130"), "{text}");
    assert_eq!(
        text.matches("color=c=black").count(),
        1,
        "one black stretch of 117 frames: {text}"
    );
    assert!(text.contains("trim=end_frame=117"), "{text}");
    assert!(text.contains("concat=n=3:v=1:a=0"), "{text}");
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
    }
}

#[test]
fn a_speed_only_used_by_clips_that_have_no_pictures_to_show_gets_no_symbol_picture() {
    // The 4x clip (100..200) is wholly past the end of a video of 100 frames.
    let plan = plan(&project_on_a_replacement_video_of(100)).unwrap();
    assert!(plan.still_pictures.is_empty());
}

// --- the fast-forward symbol ---

#[test]
fn only_accelerated_speeds_get_a_symbol_picture_and_each_speed_only_one() {
    let mut project = project_with_video(1280, 720, false);
    for at in [50, 100, 150, 200] {
        project.split_at(frame(at), None);
    }
    // Five clips: 1x, 8x, 1x, 8x, 2x.
    for (clip_number, factor) in [(1, 8), (3, 8), (4, 2)] {
        let location = clip_location(&project, 0, clip_number);
        project.set_clip_speed(location, speed(factor)).unwrap();
    }
    let plan = plan(&project).unwrap();
    let speeds: Vec<u32> = plan
        .still_pictures
        .iter()
        .map(|input| match input.kind {
            StillPictureKind::FastForwardSymbol(speed_factor) => speed_factor.value(),
            StillPictureKind::Text(_) => panic!("there is no text here"),
        })
        .collect();
    assert_eq!(
        speeds,
        vec![2, 8],
        "ordered by speed, 1x has none, 8x is shared by two clips"
    );
    assert!(
        plan.still_pictures
            .iter()
            .all(|input| input.picture.height_in_pixels > 0)
    );
}

#[test]
fn a_project_with_no_accelerated_clips_needs_no_symbol_pictures() {
    let mut project = project_with_video(1280, 720, false);
    project.split_at(frame(100), None);
    assert!(plan(&project).unwrap().still_pictures.is_empty());
}

#[test]
fn the_symbol_is_sized_and_placed_for_the_finished_picture() {
    let mut project = project_with_video(1920, 1080, false);
    project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.1, 0.2, 0.1));
    let only_clip = clip_location(&project, 0, 0);
    project.set_clip_speed(only_clip, speed(4)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.still_pictures[0].picture.height_in_pixels, 108);
    assert!(
        plan.filter_graph.text.contains("overlay=x=192:y=216:format=auto"),
        "{}",
        plan.filter_graph.text
    );
}

#[test]
fn the_symbol_is_the_product_of_the_clips_speed_and_the_tracks() {
    let mut project = project_with_video(640, 360, false);
    let only_clip = clip_location(&project, 0, 0);
    project.set_clip_speed(only_clip, speed(3)).unwrap();
    project.set_track_speed(project.tracks()[0].id(), speed(2)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.still_pictures[0].kind,
        StillPictureKind::FastForwardSymbol(speed(6))
    );
}

// --- sound ---

#[test]
fn the_setting_for_sped_up_sound_reaches_the_graph_and_only_changes_the_sound_of_sped_up_clips() {
    let mut project = project_with_video(320, 180, true);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    let with = |sound| {
        plan_with(
            &project,
            Settings {
                sped_up_clip_sound: sound,
                ..Settings::DEFAULT
            },
        )
        .filter_graph
        .text
    };

    let keeping = with(SpedUpClipSound::MaintainPitch);
    let muting = with(SpedUpClipSound::Mute);
    let raising = with(SpedUpClipSound::IncreasePitch);

    assert!(keeping.contains("atempo=4"), "{keeping}");
    assert!(!muting.contains("atempo") && !muting.contains("asetrate"), "{muting}");
    assert!(muting.contains("volume=0"), "{muting}");
    assert!(raising.contains("asetrate=192000,aresample=48000"), "{raising}");
    assert!(!raising.contains("atempo"), "{raising}");
    assert_eq!(
        keeping.matches("setpts=").count(),
        muting.matches("setpts=").count(),
        "the pictures are the same"
    );
}

#[test]
fn sound_is_part_of_the_plan_only_when_something_on_the_tracks_has_it() {
    assert!(plan(&project_with_video(320, 180, true)).unwrap().has_audio);
    assert!(
        plan(&project_with_video(320, 180, true))
            .unwrap()
            .filter_graph
            .audio_output_label
            .is_some()
    );
    assert!(!plan(&project_with_video(320, 180, false)).unwrap().has_audio);
    assert!(
        plan(&project_with_video(320, 180, false))
            .unwrap()
            .filter_graph
            .audio_output_label
            .is_none()
    );
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
fn a_sound_file_is_a_second_input_and_the_tracks_are_mixed() {
    let mut project = project_with_video(320, 180, true);
    project.import_source(audio_source(2, 4), frame(30)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.source_files,
        vec![SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(2)]
    );
    let text = &plan.filter_graph.text;
    assert!(text.contains("[0:v:0]fps="), "{text}");
    assert!(text.contains("[0:a:0]aformat="), "{text}");
    assert!(text.contains("[1:a:0]aformat="), "{text}");
    assert!(text.contains("amix=inputs=2:"), "{text}");
    assert!(!text.contains("[1:v"), "the sound file has no pictures: {text}");
    assert_eq!(plan.total_frame_count, 300);
}

#[test]
fn a_file_used_only_for_its_sound_is_still_an_input() {
    // A video with sound whose picture is hidden behind another video.
    let mut project = project_with_video(320, 180, false);
    project
        .import_source(video_source(2, 320, 180, 100, true), frame(0))
        .unwrap();
    // The new one is on a track in front, so the first video is hidden for its 100 frames, and
    // only the second's sound... is the one in front. Both are inputs.
    let plan = plan(&project).unwrap();
    assert_eq!(plan.source_files.len(), 2);
}

#[test]
fn the_inputs_are_the_source_files_in_order_of_their_names_and_then_the_still_pictures() {
    let mut project = project_with_video(320, 180, true);
    project.import_source(audio_source(7, 2), frame(0)).unwrap();
    let only_clip = clip_location(&project, 1, 0);
    project.set_clip_speed(only_clip, speed(3)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        plan.source_files,
        vec![SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(7)]
    );
    assert_eq!(plan.still_pictures.len(), 1);
    // The symbol picture is the third input: after the two source files.
    assert!(
        plan.filter_graph.text.contains("[2:v]split=1[still_picture_2_copy_0]"),
        "{}",
        plan.filter_graph.text
    );
}

// --- text ---

fn project_with_text(text_object: TextObject) -> (Project, ClipId) {
    let mut project = project_with_video(1280, 720, false);
    let front = project.add_track_in_front();
    let (location, _) = project.insert_text(front, frame(30), text_object, 60).unwrap();
    (project, location)
}

#[test]
fn text_in_front_of_the_video_is_laid_over_it_where_its_area_says_for_as_long_as_it_lasts() {
    let (project, location) = project_with_text(TextObject {
        text: "Hello".to_owned(),
        colour: TextColour::WHITE,
        area: CanvasArea::new(0.25, 0.5, 0.5, 0.25),
        ..TextObject::default()
    });
    let plan = plan(&project).unwrap();

    assert_eq!(plan.still_pictures.len(), 1);
    assert_eq!(plan.still_pictures[0].kind, StillPictureKind::Text(location));
    let picture = &plan.still_pictures[0].picture;
    assert_eq!(
        (picture.width_in_pixels, picture.height_in_pixels),
        (640, 180),
        "the size of the text's area"
    );
    let text = &plan.filter_graph.text;
    // Placed at 25% across and 50% down of 1280x720.
    assert!(text.contains("overlay=x=320:y=360:format=auto"), "{text}");
    // Over frames 30..90 only: the video is cut into three pieces.
    assert!(text.contains("concat=n=3:v=1:a=0"), "{text}");
    assert_eq!(text.matches("overlay=").count(), 1);
}

#[test]
fn text_is_made_for_the_finished_picture_size_and_once_however_many_pieces_it_is_shown_in() {
    let (mut project, location) = project_with_text(TextObject::default());
    // Cut the video under the text (and not the text) in three places.
    for at in [40, 50, 60] {
        let video_track = &project.tracks()[1];
        let video_clip = video_track.placement_covering(frame(at)).unwrap().id;
        assert_eq!(project.split_at(frame(at), Some(&[video_clip])).len(), 1);
    }
    let sizes = std::cell::RefCell::new(Vec::new());
    let counting = |object: &TextObject, width: u32, height: u32| {
        sizes.borrow_mut().push((width, height));
        stand_in_for_text(object, width, height)
    };
    let plan = plan_export(&project, Settings::DEFAULT, &counting).unwrap();
    assert_eq!(
        *sizes.borrow(),
        vec![(1280, 720)],
        "asked for once, for the finished picture's size"
    );
    assert_eq!(plan.still_pictures.len(), 1);
    assert_eq!(plan.still_pictures[0].kind, StillPictureKind::Text(location));
    assert_eq!(
        plan.filter_graph.text.matches("overlay=").count(),
        4,
        "laid over each of the four pieces the video under it is cut into"
    );
}

#[test]
fn a_symbol_goes_over_the_video_and_text_goes_over_the_symbol() {
    let (mut project, _) = project_with_text(TextObject::default());
    let video = clip_location(&project, 1, 0);
    project.set_clip_speed(video, speed(2)).unwrap();
    let plan = plan(&project).unwrap();
    let text = &plan.filter_graph.text;
    let symbol = text.find("still_picture_1_copy_0]overlay").unwrap();
    let words = text.find("still_picture_2_copy_0]overlay").unwrap();
    assert!(symbol < words, "{text}");
    assert!(
        text.contains("[clip_picture_0_with_1][still_picture_2_copy_0]overlay=")
            || text.contains("[clip_picture_1_with_1][still_picture_2_copy_0]overlay="),
        "{text}"
    );
}

// --- where things are on the picture ---

fn text_texts_among(plan: &ExportPlan) -> usize {
    plan.still_pictures
        .iter()
        .filter(|input| matches!(input.kind, StillPictureKind::Text(_)))
        .count()
}

#[test]
fn a_video_that_was_put_somewhere_smaller_is_exported_scaled_to_its_place_and_in_the_middle_of_black() {
    let mut project = project_with_video(1280, 720, false);
    let video = clip_location(&project, 0, 0);
    project
        .set_clip_placement(video, Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5)))
        .unwrap();
    let text = plan(&project).unwrap().filter_graph.text;
    assert!(text.contains("scale=640:360,setsar=1,pad=1280:720:320:180"), "{text}");
}

#[test]
fn a_video_that_was_put_partly_past_the_edge_is_exported_laid_over_black_at_a_place_before_the_picture() {
    let mut project = project_with_video(1280, 720, false);
    let video = clip_location(&project, 0, 0);
    // Half of it is before the left edge of the picture: 640 by 360 pixels, from -320 across and 180 down.
    project
        .set_clip_placement(video, Placement::upright(CanvasArea::new(-0.25, 0.25, 0.5, 0.5)))
        .unwrap();
    let text = plan(&project).unwrap().filter_graph.text;
    assert!(text.contains("color=c=black:s=1280x720"), "{text}");
    assert!(text.contains("scale=640:360,setsar=1[clip_layer_0_0]"), "{text}");
    assert!(text.contains("overlay=x=-320:y=180:format=auto"), "{text}");
    assert!(!text.contains("pad="), "{text}");
}

#[test]
fn text_put_partly_past_the_edges_is_laid_over_at_its_place_that_is_below_zero_or_beyond_the_picture() {
    let (project, _) = project_with_text(TextObject {
        area: CanvasArea::new(-0.25, 0.875, 0.5, 0.25),
        ..TextObject::default()
    });
    let plan = plan(&project).unwrap();
    assert_eq!(
        (
            plan.still_pictures[0].picture.width_in_pixels,
            plan.still_pictures[0].picture.height_in_pixels
        ),
        (640, 180),
        "the size of its area, the whole of it, though some of it is past the edge"
    );
    assert!(
        plan.filter_graph.text.contains("overlay=x=-320:y=630:format=auto"),
        "{}",
        plan.filter_graph.text
    );
}

#[test]
fn a_video_that_fills_the_picture_is_only_scaled_with_nothing_laid_over_and_no_black_made() {
    let text = plan(&project_with_video(1920, 1080, false)).unwrap().filter_graph.text;
    assert!(text.contains(",scale=1920:1080,setsar=1[finished_clip_0]"), "{text}");
    assert!(
        !text.contains("pad=") && !text.contains("overlay") && !text.contains("color=c=black"),
        "{text}"
    );
}

#[test]
fn a_video_of_another_shape_in_front_is_fitted_inside_the_picture_over_the_video_behind_it() {
    // A video as wide as the picture, with an upright one on a track in front.
    let mut project = project_with_video(1280, 720, false);
    project
        .import_source(video_source(2, 720, 1280, 100, false), frame(0))
        .unwrap();
    let fit = fit_inside_picture(720, 1280, 1280, 720);
    let plan = plan(&project).unwrap();
    let text = &plan.filter_graph.text;
    assert!(
        text.contains(&format!(
            ",scale={}:{},setsar=1[clip_layer_0_1]",
            fit.width_in_pixels, fit.height_in_pixels
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "overlay=x={}:y={}:format=auto",
            fit.left_in_pixels, fit.top_in_pixels
        )),
        "{text}"
    );
    assert_eq!(
        plan.source_files,
        vec![SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(2)],
        "both are seen, the one behind through the bars"
    );
}

#[test]
fn a_video_behind_one_that_fills_the_picture_is_not_read_at_all_if_it_has_no_sound() {
    let mut project = project_with_video(1280, 720, false);
    project
        .import_source(video_source(2, 1280, 720, 300, false), frame(0))
        .unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.source_files, vec![SourceMediaIdentifier::new(2)]);
}

#[test]
fn text_that_is_turned_is_turned_where_the_middle_of_its_area_is() {
    // The area is 640 by 180 pixels at (320, 360) of 1280x720, so its middle is at (640, 450).
    // Turned a quarter it is 180 across and 640 down: its top left corner is at (550, 130).
    let (project, _) = project_with_text(TextObject {
        area: CanvasArea::new(0.25, 0.5, 0.5, 0.25),
        rotation: Angle::from_degrees(90.0),
        ..TextObject::default()
    });
    let text = plan(&project).unwrap().filter_graph.text;
    assert!(
        text.contains("format=rgba,rotate=a=1.570796327:ow=180:oh=640:c=black@0"),
        "{text}"
    );
    assert!(text.contains("overlay=x=550:y=130:format=auto"), "{text}");
}

#[test]
fn text_that_is_not_turned_has_nothing_to_do_with_turning() {
    let (project, _) = project_with_text(TextObject::default());
    let text = plan(&project).unwrap().filter_graph.text;
    assert!(!text.contains("rotate"), "{text}");
}

#[test]
fn a_video_that_is_turned_is_laid_over_black_turned_with_clear_corners() {
    let mut project = project_with_video(1280, 720, false);
    let video = clip_location(&project, 0, 0);
    project
        .set_clip_placement(
            video,
            Placement::new(CanvasArea::new(0.25, 0.25, 0.5, 0.5), Angle::from_degrees(-30.0)),
        )
        .unwrap();
    let text = plan(&project).unwrap().filter_graph.text;
    assert!(text.contains("color=c=black:s=1280x720"), "{text}");
    assert!(
        text.contains("scale=640:360,setsar=1,format=rgba,rotate=a=-0.523598776:"),
        "{text}"
    );
    assert!(text.contains(":c=black@0[clip_layer_0_0]"), "{text}");
}

#[test]
fn the_symbol_is_the_one_of_the_front_most_video_so_a_sped_up_video_behind_a_normal_one_has_none() {
    let mut project = project_with_video(1280, 720, false);
    let behind = clip_location(&project, 0, 0);
    project.set_clip_speed(behind, speed(4)).unwrap();
    // A small video that is not sped up, on a track in front: the one behind shows round it.
    project
        .import_source(video_source(2, 1280, 720, 100, false), frame(0))
        .unwrap();
    let in_front = clip_location(&project, 0, 0);
    assert_ne!(
        project.track_holding(in_front).unwrap().id(),
        project.track_holding(behind).unwrap().id()
    );
    project
        .set_clip_placement(in_front, Placement::upright(CanvasArea::new(0.1, 0.1, 0.3, 0.3)))
        .unwrap();

    let plan = plan(&project).unwrap();

    assert!(plan.still_pictures.is_empty(), "{:?}", plan.still_pictures);
}

#[test]
fn the_symbol_is_laid_once_over_the_video_in_front_and_not_over_the_video_behind_it() {
    let mut project = project_with_video(1280, 720, false);
    // A small video that is sped up, on a track in front: the one behind shows round it.
    project
        .import_source(video_source(2, 1280, 720, 100, false), frame(0))
        .unwrap();
    let in_front = clip_location(&project, 0, 0);
    project.set_clip_speed(in_front, speed(4)).unwrap();
    project
        .set_clip_placement(in_front, Placement::upright(CanvasArea::new(0.1, 0.1, 0.3, 0.3)))
        .unwrap();

    let plan = plan(&project).unwrap();
    let text = &plan.filter_graph.text;

    // The symbol is the third input, after the two videos. It is used once, over what the video in front
    // makes with the video behind it, and not over the video behind it alone.
    assert!(text.contains("[2:v]split=1[still_picture_2_copy_0]"), "{text}");
    assert_eq!(
        text.matches("overlay=").count(),
        2,
        "the video in front over the one behind, and the symbol over both: {text}"
    );
    assert!(
        text.contains("[clip_picture_0_with_1][still_picture_2_copy_0]overlay="),
        "{text}"
    );
}

#[test]
fn text_behind_a_video_that_fills_the_picture_is_not_made_and_behind_one_that_does_not_it_is() {
    // The text first, so that the video that is imported after it goes on a track in front of it.
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    project.insert_text(track, frame(0), TextObject::default(), 60).unwrap();
    project
        .import_source(video_source(1, 1280, 720, 100, false), frame(0))
        .unwrap();
    assert_eq!(text_texts_among(&plan(&project).unwrap()), 0, "it cannot be seen");

    let video = clip_location(&project, 0, 0);
    project
        .set_clip_placement(video, Placement::upright(CanvasArea::new(0.1, 0.1, 0.3, 0.3)))
        .unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        text_texts_among(&plan),
        1,
        "the video is small, and the text shows round it"
    );
}

// --- pictures ---

#[test]
fn a_picture_is_an_input_of_its_own_scaled_to_where_it_is_with_nothing_rendered_for_it() {
    let mut project = project_with_video(1280, 720, false);
    project.import_source(picture_source(2, 800, 400), frame(0)).unwrap();
    let plan = plan(&project).unwrap();

    assert_eq!(
        plan.source_files,
        vec![SourceMediaIdentifier::new(1), SourceMediaIdentifier::new(2)],
        "the picture file is read like the video file is"
    );
    assert!(
        plan.still_pictures.is_empty(),
        "no picture is made for it: ffmpeg reads the file"
    );
    let fit = fit_inside_picture(800, 400, 1280, 720);
    assert_eq!(
        (fit.width_in_pixels, fit.height_in_pixels, fit.top_in_pixels),
        (1280, 640, 40)
    );
    let text = &plan.filter_graph.text;
    assert!(text.contains("[1:v]split=1[still_picture_1_copy_0]"), "{text}");
    assert!(
        text.contains("[still_picture_1_copy_0]scale=1280:640,setsar=1[clip_layer_0_1]"),
        "{text}"
    );
    assert!(text.contains("overlay=x=0:y=40:format=auto"), "{text}");
    assert!(!text.contains("[1:a"), "a picture has no sound: {text}");
}

#[test]
fn a_picture_that_is_turned_and_made_smaller_is_exported_that_way() {
    let mut project = project_with_video(1280, 720, false);
    let outcome = project.import_source(picture_source(2, 800, 400), frame(0)).unwrap();
    let picture = outcome.clip.unwrap();
    project
        .set_clip_placement(
            picture,
            Placement::new(CanvasArea::new(0.25, 0.25, 0.5, 0.25), Angle::from_degrees(90.0)),
        )
        .unwrap();
    let text = plan(&project).unwrap().filter_graph.text;
    // 640 by 180 pixels, turned a quarter: 180 across and 640 down, with its middle at (640, 270).
    assert!(
        text.contains("scale=640:180,setsar=1,format=rgba,rotate=a=1.570796327:ow=180:oh=640:c=black@0"),
        "{text}"
    );
    assert!(text.contains("overlay=x=550:y=-50:format=auto"), "{text}");
}

#[test]
fn a_project_of_nothing_but_a_picture_is_exported_at_the_standard_size_with_the_picture_fitted() {
    let mut project = Project::empty();
    project.import_source(picture_source(1, 400, 400), frame(0)).unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(
        (plan.picture_width_in_pixels, plan.picture_height_in_pixels),
        (1920, 1080)
    );
    assert_eq!(plan.total_frame_count, 150, "five seconds at 30 frames per second");
    assert!(!plan.has_audio);
    let fit = fit_inside_picture(400, 400, 1920, 1080);
    assert!(
        plan.filter_graph.text.contains(&format!(
            "scale={}:{},setsar=1",
            fit.width_in_pixels, fit.height_in_pixels
        )),
        "{}",
        plan.filter_graph.text
    );
}

#[test]
fn a_picture_behind_a_video_that_fills_the_picture_is_not_an_input() {
    let mut project = Project::empty();
    project.import_source(picture_source(1, 400, 400), frame(0)).unwrap();
    project
        .import_source(video_source(2, 1280, 720, 150, false), frame(0))
        .unwrap();
    let plan = plan(&project).unwrap();
    assert_eq!(plan.source_files, vec![SourceMediaIdentifier::new(2)]);
}

#[test]
fn a_turn_is_exported_as_the_angle_between_minus_180_and_180_that_looks_the_same() {
    let exported = |degrees: f32| {
        place_to_export(
            Placement::new(CanvasArea::WHOLE_PICTURE, Angle::from_degrees(degrees)),
            (1920, 1080),
        )
        .turn_in_degrees
    };
    assert_eq!(exported(0.0), 0.0);
    assert_eq!(
        exported(360.0),
        0.0,
        "a whole turn is not turned at all: the picture is laid over as it is and not turned a lot for nothing"
    );
    assert_eq!(exported(720.0), 0.0);
    assert!((exported(725.0) - 5.0).abs() < 1e-3);
    assert!((exported(-725.0) + 5.0).abs() < 1e-3);
    assert!((exported(270.0) + 90.0).abs() < 1e-3);
    assert!((exported(90.0) - 90.0).abs() < 1e-3);
}
