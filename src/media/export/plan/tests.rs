use std::path::PathBuf;

use super::*;
use crate::project::{
    CanvasArea, SourceAudioProperties, SourceMedia, SourceMediaLink, SourceVideoProperties, SpedUpClipSound,
    SpeedIndicatorLayout, TextColour,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn video_source(identifier: u32, width: u32, height: u32, frames: u64, has_audio: bool) -> SourceMedia {
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

fn audio_source(identifier: u32, seconds: u64) -> SourceMedia {
    SourceMedia::audio(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/sounds/{identifier}.mp3"))),
        SourceAudioProperties {
            duration_in_microseconds: seconds * 1_000_000,
        },
    )
}

fn project_with_video(width: u32, height: u32, has_audio: bool) -> Project {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, width, height, 300, has_audio), frame(0))
        .unwrap();
    project
}

/// A picture the size of the text's area, filled with white, standing in for rendered text.
fn stand_in_for_text(object: &TextObject, picture_width: u32, picture_height: u32) -> RgbaPicture {
    let width = (object.area.width() * picture_width as f32).round() as u32;
    let height = (object.area.height() * picture_height as f32).round() as u32;
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: vec![255; (width * height * 4) as usize],
    }
}

fn plan(project: &Project) -> Result<ExportPlan, ExportPlanError> {
    plan_export(project, Settings::DEFAULT, &stand_in_for_text)
}

fn plan_with(project: &Project, settings: Settings) -> ExportPlan {
    plan_export(project, settings, &stand_in_for_text).unwrap()
}

fn clip_location(project: &Project, track_position: usize, clip_number: usize) -> ClipLocation {
    ClipLocation {
        track: project.tracks()[track_position].id(),
        clip: project.tracks()[track_position].entries()[clip_number].id(),
    }
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
    assert_eq!(plan.frames_black_for_lack_of_pictures, 0);
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
fn a_project_on_its_own_video_has_nothing_black_for_lack_of_pictures() {
    let plan = plan(&project_with_video(320, 180, true)).unwrap();
    assert_eq!(plan.total_frame_count, 300);
    assert_eq!(plan.frames_black_for_lack_of_pictures, 0);
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
    assert_eq!(plan.frames_black_for_lack_of_pictures, 17 + 100);
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
fn the_plan_has_exactly_as_many_frames_black_as_there_are_frames_of_clips_beyond_the_end_of_the_video() {
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
        assert_eq!(
            plan.frames_black_for_lack_of_pictures, missing,
            "video of {frame_count} frames"
        );
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
    project.set_track_speed(only_clip.track, speed(2)).unwrap();
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
    project.set_track_muted(only_clip.track, true).unwrap();
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
    assert!(text.contains("[0:v:0]scale="), "{text}");
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

fn project_with_text(text_object: TextObject) -> (Project, ClipLocation) {
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
        let video_clip = ClipLocation {
            track: video_track.id(),
            clip: video_track.placement_covering(frame(at)).unwrap().id,
        };
        assert_eq!(project.split_at(frame(at), Some(video_clip)).len(), 1);
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
