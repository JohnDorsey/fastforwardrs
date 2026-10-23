// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whole journeys through the real editor with a real video and the real ffmpeg: importing,
//! editing with the mouse, saving, loading into a fresh editor, losing the video, and exporting.
//!
//! The window is driven with simulated input (see the `journey` module), and the places a file
//! dialog would choose are given to the editor directly. Every journey ends by checking what the
//! editor holds against what the project file says, or what the exported video contains.

mod journey;
mod support;

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, PointerButton, vec2};
use egui_kittest::kittest::Queryable;
use fastforwardrs::colour_effects::ColourMatrix;
use fastforwardrs::colour_space::TransferFunction;
use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::{
    AnimationTarget, Clip, EffectInstanceId, EffectKind, Project, SourceMediaIdentifier, TimelineFrameIndex,
};
use journey::Journey;
use support::{
    TestVideoSpecification, counter_video, decode_video, expected_source_frames, is_black, picture_shown_in_frame,
    red_and_blue_picture, scratch_directory, stream_durations, test_video,
};

/// The size of the pictures the test videos are exported at.
const VIDEO_WIDTH: u32 = 320;
const VIDEO_HEIGHT: u32 = 180;

/// The first file a project imports is its source file number 1.
const THE_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

/// `path` in a form in which two paths to the same file are equal.
fn same_file_as(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|error| panic!("{} cannot be resolved: {error}", path.display()))
}

fn read_project_file_as_json(path: &Path) -> (String, serde_json::Value) {
    let text = std::fs::read_to_string(path).expect("the project file was written");
    let json = serde_json::from_str(&text).expect("the project file is JSON");
    (text, json)
}

/// What is on the tracks of a project, without the names that are only made up while it is open:
/// for each track in order its title, and for each clip the empty frames before it and the clip.
fn what_is_on_the_tracks(project: &Project) -> Vec<(String, Vec<(u64, Clip)>)> {
    project
        .tracks()
        .iter()
        .map(|track| {
            (
                track.title().to_owned(),
                track
                    .entries()
                    .iter()
                    .map(|entry| (entry.gap_before_frames(), entry.clip().clone()))
                    .collect(),
            )
        })
        .collect()
}

/// Cuts at 60 and 150 with the keyboard and sets the middle clip to 10x in the properties panel.
fn cut_into_three_with_the_middle_at_ten_times(journey: &mut Journey) {
    journey.cut_at_frame(60);
    journey.cut_at_frame(150);
    assert_eq!(journey.clips(), vec![(0, 60, 1), (60, 150, 1), (150, 600, 1)]);
    journey.set_the_speed_of_the_clip_at(105, 10);
    assert_eq!(journey.clips(), vec![(0, 60, 1), (60, 150, 10), (150, 600, 1)]);
}

fn export_to(journey: &mut Journey, destination: &Path) {
    let _one_export_at_a_time = support::one_export_at_a_time();
    journey.application_mut().export_video_to(destination.to_owned());
    assert!(
        journey.application().is_working_in_the_background(),
        "the export started"
    );
    journey.wait_until_idle();
    let (title, text) = journey
        .application()
        .message_showing()
        .expect("a message says how the export ended");
    assert_eq!(title, "Export finished", "{text}");
    assert!(
        text.contains(&destination.display().to_string()),
        "the message says where the video went: {text}"
    );
}

fn assert_exports_exactly_what_the_project_describes(
    toolchain: &FfmpegToolchain,
    journey: &mut Journey,
    destination: &Path,
) {
    export_to(journey, destination);
    let expected = expected_source_frames(journey.application().project());
    assert_eq!(
        expected.len() as u64,
        journey.application().project().total_frame_count()
    );
    let exported = decode_video(toolchain, destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.frame_numbers(), expected);
}

#[test]
fn an_export_in_the_window_of_a_video_that_has_fewer_pictures_than_the_project_was_told_says_how_many_pictures_were_held()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    // The project was made when the file had 300 pictures; it has 90 now (see `support`).
    let short = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "counter_3_seconds",
            duration_in_seconds: 3,
            ..TestVideoSpecification::COUNTER
        },
    );
    let project =
        support::project_for_a_video_that_has_fewer_pictures_than_the_project_was_told(&toolchain, &short, 300);
    let scratch = scratch_directory();
    let project_path = scratch.path().join("shortened.fastforwardrs.json");
    fastforwardrs::project_file::write_project_file(&project, &project_path).unwrap();
    let mut journey = Journey::start(&toolchain);
    journey.application_mut().load_project_from(project_path);
    journey.wait_until_idle();
    assert_eq!(journey.application().message_showing(), None, "the file is there");

    export_to(&mut journey, &scratch.path().join("held.mp4"));

    let (title, text) = journey
        .application()
        .message_showing()
        .expect("the end of the export is told");
    assert_eq!(title, "Export finished");
    assert!(text.contains("210 pictures of videos were not there"), "{text}");
    assert!(
        !text.contains("are missing the picture of a video"),
        "that is told of what the plan knew beforehand: {text}"
    );
}

#[test]
fn importing_a_video_makes_one_clip_and_a_picture_for_every_second() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);

    let journey = Journey::importing(&toolchain, &video.path);

    assert_eq!(journey.clips(), vec![(0, 600, 1)]);
    assert_eq!(
        journey.application().project().tracks().len(),
        1,
        "the video went on the track the new project has"
    );
    assert!(
        journey.application().has_unsaved_changes(),
        "a new project has not been saved"
    );
    assert_eq!(journey.application().project_file_path(), None);
    assert_eq!(journey.application().message_showing(), None);
    assert_eq!(
        journey.application().thumbnail_progress(THE_VIDEO),
        (20, 20),
        "one picture for each of the 20 seconds"
    );
    let source = &journey.application().project().sources()[0];
    assert_eq!(source.link.absolute_path, std::path::absolute(&video.path).unwrap());
    let properties = source.properties.video().expect("a video");
    assert_eq!(properties.total_frame_count, 600);
    assert_eq!(
        (properties.display_width_in_pixels, properties.display_height_in_pixels),
        (VIDEO_WIDTH, VIDEO_HEIGHT)
    );
    assert!(properties.has_audio);
    assert!(
        journey.harness.query_by_label("00:00:00.00").is_some(),
        "the time starts at zero"
    );
}

#[test]
fn the_preview_shows_the_picture_of_whichever_frame_is_current_even_through_sped_up_clips() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    cut_into_three_with_the_middle_at_ten_times(&mut journey);
    // 60 frames, then the 90 of the sped up clip become 9, then the last 450 are 450: 519 in all.
    let total_frames = journey.application().project().total_frame_count();
    assert_eq!(total_frames, 60 + 9 + 450);

    for fraction in [0.0_f32, 0.07, 0.11, 0.13, 0.5, 0.93, 1.0] {
        journey.click_the_scrub_slider_at(fraction);

        let current = journey.application().current_frame();
        let target = (fraction * (total_frames - 1) as f32).round() as i64;
        assert!(
            (current.value() as i64 - target).abs() <= 1,
            "the slider at {fraction} is frame {target}, but the current frame is {current}"
        );
        let wanted_source_frame = journey
            .source_frame_of_the_video_in_front_at(current)
            .expect("the current frame is on the timeline")
            .value();
        journey.wait_for_the_preview_to_show_the_current_frame();
        assert_eq!(
            journey.frame_number_in_the_preview(),
            wanted_source_frame,
            "the picture the preview shows is source frame {wanted_source_frame} (timeline frame {current})"
        );
    }
}

#[test]
fn a_whole_journey_import_edit_save_load_export() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);

    // Edit with the mouse and the keyboard.
    cut_into_three_with_the_middle_at_ten_times(&mut journey);
    assert!(journey.application().has_unsaved_changes());

    // Save.
    let project_path = scratch.path().join("journey.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    assert_eq!(journey.application().message_showing(), None);
    assert!(
        !journey.application().has_unsaved_changes(),
        "saving clears the unsaved changes"
    );
    assert_eq!(journey.application().project_file_path(), Some(project_path.as_path()));
    let (text, saved) = read_project_file_as_json(&project_path);
    assert!(
        text.lines().count() > 20,
        "the file is pretty: a value to a line\n{text}"
    );
    assert!(
        text.len() < 5_000,
        "the video is not inside the file ({} bytes)",
        text.len()
    );
    assert_eq!(saved["tracks"][0]["title"], "track 0");
    assert_eq!(saved["tracks"][0]["clips"][1]["kind"], "video");
    assert_eq!(saved["tracks"][0]["clips"][1]["speed_factor"], 10);
    assert_eq!(saved["tracks"][0]["clips"][1]["first_source_frame"], 60);
    assert_eq!(saved["sources"][0]["kind"], "video");
    assert_eq!(
        saved["sources"][0]["absolute_path"],
        std::path::absolute(&video.path).unwrap().to_string_lossy().as_ref()
    );
    assert!(saved["sources"][0]["relative_path"].is_string(), "{text}");

    // Load into a fresh editor: everything is as it was.
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path.clone());
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    assert!(fresh.application().every_source_file_is_available());
    assert!(!fresh.application().missing_sources_warning_is_showing());
    assert_eq!(fresh.clips(), journey.clips());
    assert_eq!(
        what_is_on_the_tracks(fresh.application().project()),
        what_is_on_the_tracks(journey.application().project())
    );
    assert_eq!(
        fresh.application().project().speed_indicator_layout(),
        journey.application().project().speed_indicator_layout()
    );
    assert_eq!(
        fresh.application().project().sources()[0].properties,
        journey.application().project().sources()[0].properties
    );
    assert!(
        !fresh.application().has_unsaved_changes(),
        "a loaded project has nothing unsaved"
    );
    assert_eq!(fresh.application().project_file_path(), Some(project_path.as_path()));
    assert_eq!(fresh.application().thumbnail_progress(THE_VIDEO), (20, 20));

    // Export from the loaded project, and check every picture.
    let exported_path = scratch.path().join("journey.mp4");
    assert_exports_exactly_what_the_project_describes(&toolchain, &mut fresh, &exported_path);
    assert!(
        !scratch.path().join("journey.exporting.mp4").exists(),
        "the temporary file is gone once the export is finished"
    );
}

#[test]
fn a_project_and_its_video_moved_together_are_found_again_by_the_relative_path() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let original_folder = scratch.path().join("original");
    std::fs::create_dir_all(original_folder.join("footage")).unwrap();
    let video_copy = original_folder.join("footage").join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();

    let mut journey = Journey::importing(&toolchain, &video_copy);
    journey.cut_at_frame(90);
    let clips_before = journey.clips();
    let project_path = original_folder.join("holiday.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text, saved) = read_project_file_as_json(&project_path);
    assert_eq!(saved["sources"][0]["relative_path"], "footage/holiday.mp4", "{text}");
    drop(journey);

    // The whole folder moves somewhere else: the absolute path is now wrong, the relative one is right.
    let new_folder = scratch.path().join("moved to a new place");
    std::fs::rename(&original_folder, &new_folder).unwrap();
    let mut reopened = Journey::start(&toolchain);
    reopened
        .application_mut()
        .load_project_from(new_folder.join("holiday.fastforwardrs.json"));
    reopened.wait_until_idle();

    assert_eq!(reopened.application().message_showing(), None);
    assert!(
        reopened.application().every_source_file_is_available(),
        "found through the relative path"
    );
    assert!(!reopened.application().missing_sources_warning_is_showing());
    assert_eq!(reopened.clips(), clips_before);
    assert_eq!(reopened.application().thumbnail_progress(THE_VIDEO), (20, 20));
    assert_eq!(
        same_file_as(&reopened.application().project().sources()[0].link.absolute_path),
        same_file_as(&new_folder.join("footage").join("holiday.mp4")),
        "the project now remembers where the video really is"
    );
}

#[test]
fn a_project_whose_video_cannot_be_found_warns_keeps_every_edit_and_can_be_pointed_at_the_video_again() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let video_copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();

    let mut journey = Journey::importing(&toolchain, &video_copy);
    journey.cut_at_frame(90);
    journey.cut_at_frame(180);
    journey.set_the_speed_of_the_clip_at(135, 3);
    let edited_clips = journey.clips();
    assert_eq!(edited_clips, vec![(0, 90, 1), (90, 180, 3), (180, 600, 1)]);
    let project_path = scratch.path().join("project.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    drop(journey);

    // The video goes missing.
    let moved_away = scratch.path().join("elsewhere").join("holiday (renamed).mp4");
    std::fs::create_dir_all(moved_away.parent().unwrap()).unwrap();
    std::fs::rename(&video_copy, &moved_away).unwrap();

    let mut reopened = Journey::start(&toolchain);
    reopened.application_mut().load_project_from(project_path);
    reopened.wait_until_idle();

    // The warning is up and says what is needed. Nothing was discarded.
    assert!(!reopened.application().every_source_file_is_available());
    assert!(reopened.application().missing_sources_warning_is_showing());
    assert!(
        reopened
            .harness
            .query_by_label_contains("must be chosen again")
            .is_some()
    );
    assert_eq!(reopened.clips(), edited_clips, "no edit was discarded");
    assert!(
        !reopened.application().has_unsaved_changes(),
        "nothing has changed since loading"
    );

    // The program stays interactive: the track can still be cut while the video is missing.
    reopened.cut_at_frame(30);
    let clips_after_the_cut = vec![(0, 30, 1), (30, 90, 1), (90, 180, 3), (180, 600, 1)];
    assert_eq!(
        reopened.clips(),
        clips_after_the_cut,
        "editing still works while the video is missing"
    );
    assert!(reopened.application().has_unsaved_changes());

    // Choosing the video again brings it back, with all the edits (including the one made meanwhile).
    reopened
        .application_mut()
        .relink_source_from(THE_VIDEO, moved_away.clone());
    reopened.wait_until_idle();
    assert_eq!(reopened.application().message_showing(), None);
    assert!(reopened.application().every_source_file_is_available());
    assert!(!reopened.application().missing_sources_warning_is_showing());
    assert!(
        reopened
            .harness
            .query_by_label_contains("must be chosen again")
            .is_none()
    );
    assert_eq!(reopened.clips(), clips_after_the_cut);
    assert_eq!(reopened.application().thumbnail_progress(THE_VIDEO), (20, 20));
    assert_eq!(
        same_file_as(&reopened.application().project().sources()[0].link.absolute_path),
        same_file_as(&moved_away)
    );

    // And the project is whole: its export has exactly the frames the edits describe.
    assert_exports_exactly_what_the_project_describes(&toolchain, &mut reopened, &scratch.path().join("relinked.mp4"));
}

#[test]
fn a_replacement_video_of_another_frame_rate_that_lasts_as_long_takes_the_place_of_the_old_one_with_the_edits_where_they_were()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let video_copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();
    let mut journey = Journey::importing(&toolchain, &video_copy);
    journey.cut_at_frame(90);
    let project_path = scratch.path().join("project.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    drop(journey);
    std::fs::remove_file(&video_copy).unwrap();

    let mut reopened = Journey::start(&toolchain);
    reopened.application_mut().load_project_from(project_path);
    reopened.wait_until_idle();
    assert!(reopened.application().missing_sources_warning_is_showing());

    // Another 20 seconds, but of 500 pictures at 25 a second where there were 600 at 30.
    let replacement = test_video(&toolchain, &TestVideoSpecification::PAL_20_SECONDS);
    reopened
        .application_mut()
        .relink_source_from(THE_VIDEO, replacement.path.clone());
    reopened.wait_until_idle();

    assert_eq!(
        reopened.application().message_showing(),
        None,
        "it lasts as long as the old one did, and that is what the edits care about"
    );
    assert!(reopened.application().every_source_file_is_available());
    assert_eq!(
        reopened.clips(),
        vec![(0, 90, 1), (90, 600, 1)],
        "the edits are untouched"
    );
    assert_eq!(
        reopened.application().project().frame_rate().to_string(),
        "30/1",
        "and so is the frame rate of the project: it does not follow the file"
    );

    // The export has in each of the 600 frames the picture of the new file that is nearest in time.
    let destination = scratch.path().join("relinked.mp4");
    export_to(&mut reopened, &destination);
    let expected: Vec<u64> = expected_source_frames(reopened.application().project())
        .into_iter()
        .map(|frame| picture_shown_in_frame(25, 30, frame))
        .collect();
    assert_eq!(expected.len(), 600);
    let exported = decode_video(&toolchain, &destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.frame_numbers(), expected);
}

#[test]
fn a_shorter_replacement_video_keeps_the_edits_and_the_export_is_black_where_the_video_has_nothing() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let video_copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();
    let mut journey = Journey::importing(&toolchain, &video_copy);
    journey.cut_at_frame(100);
    journey.cut_at_frame(200);
    journey.set_the_speed_of_the_clip_at(150, 3);
    let edited_clips = vec![(0, 100, 1), (100, 200, 3), (200, 600, 1)];
    assert_eq!(journey.clips(), edited_clips);
    let project_path = scratch.path().join("project.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    drop(journey);
    std::fs::remove_file(&video_copy).unwrap();

    // The video is replaced by one that is only 5 seconds (150 frames) long: the second clip
    // (frames 100 to 200) reaches past its end, and the third is wholly beyond it.
    let shorter_video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "counter_5_seconds",
            duration_in_seconds: 5,
            ..TestVideoSpecification::COUNTER
        },
    );
    let mut reopened = Journey::start(&toolchain);
    reopened.application_mut().load_project_from(project_path);
    reopened.wait_until_idle();
    reopened
        .application_mut()
        .relink_source_from(THE_VIDEO, shorter_video.path.clone());
    reopened.wait_until_idle();

    let (title, text) = reopened.application().message_showing().expect("the lengths differ");
    assert_eq!(title, "The file is not the same length");
    assert!(text.contains("600 frames") && text.contains("150"), "{text}");
    assert_eq!(reopened.clips(), edited_clips, "all the edits are kept");

    // The timeline is as long as ever, so that everything on other tracks would stay where it is:
    // 100 frames, then the 100 source frames of the sped up clip are 34, then 400.
    let timeline_frames = reopened.application().project().total_frame_count();
    assert_eq!(timeline_frames, 100 + 34 + 400);

    // The export has a picture for every frame of the timeline: the frames of the clips that exist,
    // and black where the video has nothing.
    let exported_path = scratch.path().join("shorter.mp4");
    export_to(&mut reopened, &exported_path);
    let project = reopened.application().project();
    let expected: Vec<u64> = expected_source_frames(project);
    let exported = decode_video(&toolchain, &exported_path, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.pictures.len() as u64, timeline_frames);
    let exported_numbers = exported.frame_numbers();
    let frames_that_exist = expected.iter().take_while(|&&source_frame| source_frame < 150).count();
    assert_eq!(frames_that_exist, 100 + 17, "frames 0 to 99, then 100, 103, ... 148");
    assert_eq!(exported_numbers[..frames_that_exist], expected[..frames_that_exist]);
    for (timeline_frame, picture) in exported.pictures.iter().enumerate().skip(frames_that_exist) {
        assert!(
            is_black(picture),
            "timeline frame {timeline_frame} has no picture in the video: black"
        );
    }

    // ...the message says how much of the project is missing a picture...
    let (_, text) = reopened.application().message_showing().expect("the export finished");
    let black = timeline_frames - frames_that_exist as u64;
    assert!(
        text.contains(&format!(
            "{black} frames of the project are missing the picture of a video"
        )),
        "{black} frames are missing a picture: {text}"
    );

    // ...and the sound is as long as the picture.
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &exported_path);
    let (video_seconds, audio_seconds) = (video_seconds.unwrap(), audio_seconds.unwrap());
    assert!(
        (video_seconds - timeline_frames as f64 / 30.0).abs() < 0.01,
        "{timeline_frames} frames at 30 per second: {video_seconds}"
    );
    assert!(
        (audio_seconds - video_seconds).abs() < 0.06,
        "the sound is {audio_seconds} seconds long and the picture {video_seconds}"
    );
}

#[test]
fn a_file_that_is_not_a_video_or_a_sound_is_refused_with_a_message_and_the_project_stays_empty() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let not_a_video = scratch.path().join("notes.mp4");
    std::fs::write(&not_a_video, "this is not a video").unwrap();
    let mut journey = Journey::start(&toolchain);

    journey.application_mut().link_and_import_file_from(not_a_video);
    journey.wait_until_idle();

    let (title, text) = journey.application().message_showing().expect("a message explains");
    assert_eq!(title, "The file could not be opened");
    assert!(text.contains("notes.mp4"), "{text}");
    assert!(journey.application().project().has_no_clips());
    assert!(journey.application().project().sources().is_empty());
}

#[test]
fn a_file_that_is_not_a_project_is_refused_with_a_message_and_nothing_is_lost() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.cut_at_frame(150);
    let not_a_project = scratch.path().join("shopping list.json");
    std::fs::write(&not_a_project, r#"{ "milk": 2, "eggs": 12 }"#).unwrap();

    journey.application_mut().load_project_from(not_a_project);
    journey.settle();

    let (title, _) = journey.application().message_showing().expect("a message explains");
    assert_eq!(title, "The project could not be loaded");
    assert_eq!(
        journey.clips(),
        vec![(0, 150, 1), (150, 600, 1)],
        "the project being edited is untouched"
    );
    assert!(journey.application().has_unsaved_changes());
    assert!(journey.application().every_source_file_is_available());
}

#[test]
fn a_new_project_forgets_everything_about_the_old_one() {
    let toolchain = toolchain_or_skip!();
    let first_video = counter_video(&toolchain);
    let second_video = test_video(&toolchain, &TestVideoSpecification::PAL_3_SECONDS);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &first_video.path);
    cut_into_three_with_the_middle_at_ten_times(&mut journey);
    journey
        .application_mut()
        .save_project_to(scratch.path().join("first.fastforwardrs.json"));

    journey.application_mut().start_empty_project();
    assert!(journey.application().project().sources().is_empty());
    assert_eq!(journey.application().project_file_path(), None);
    journey
        .application_mut()
        .link_and_import_file_from(second_video.path.clone());
    journey.wait_until_idle();

    assert_eq!(journey.application().message_showing(), None);
    assert_eq!(
        journey.clips(),
        vec![(0, 75, 1)],
        "a single clip over the whole new video"
    );
    assert_eq!(journey.application().project().sources().len(), 1);
    assert_eq!(
        journey.application().project_file_path(),
        None,
        "the new project has no file yet"
    );
    assert!(journey.application().has_unsaved_changes());
    assert_eq!(journey.application().current_frame(), TimelineFrameIndex::new(0));
    assert_eq!(journey.application().thumbnail_progress(THE_VIDEO), (3, 3));
    assert_eq!(
        journey.application().project().frame_rate().to_string(),
        "25/1",
        "the first video of a project decides the frame rate"
    );
    // The preview is of the new video, not the old one.
    journey.wait_for_the_preview_to_show_the_current_frame();
    assert_eq!(journey.frame_number_in_the_preview(), 0);
    journey.click_the_scrub_slider_at(1.0);
    journey.wait_for_the_preview_to_show_the_current_frame();
    assert_eq!(journey.frame_number_in_the_preview(), 74);
}

#[test]
fn exporting_over_the_source_video_is_refused_with_a_message_and_the_video_is_untouched() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let video_copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();
    let original_bytes = std::fs::read(&video_copy).unwrap();
    let mut journey = Journey::importing(&toolchain, &video_copy);

    journey.application_mut().export_video_to(video_copy.clone());
    journey.settle();

    let (title, text) = journey.application().message_showing().expect("a message explains");
    assert_eq!(title, "The export could not be started");
    assert!(!text.is_empty());
    assert!(!journey.application().is_working_in_the_background());
    assert_eq!(
        std::fs::read(&video_copy).unwrap(),
        original_bytes,
        "the source video is untouched"
    );
}

/// How loud ffmpeg's `sine` source is, which is the sound of the test videos and tone files.
const TONE_AMPLITUDE: f64 = 0.125;

#[test]
fn a_sound_file_imported_at_the_playhead_goes_on_a_track_of_its_own_with_a_waveform_and_is_heard_in_the_export() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain); // 20 seconds with a 440 hertz tone
    let tone = support::tone_file(&toolchain, 330, 4);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);

    // Import the sound two seconds in (where the playhead is).
    journey.move_playhead_to_frame(60);
    journey.import(&tone);

    let project = journey.application().project();
    assert_eq!(
        project.tracks().len(),
        2,
        "the sound has a track of its own, in front of the video's"
    );
    let sound_clip = project.tracks()[0].placement_at_index(0).unwrap();
    assert!(matches!(sound_clip.clip, Clip::Audio(_)));
    assert_eq!(
        (
            sound_clip.first_timeline_frame.value(),
            sound_clip.timeline_frame_count()
        ),
        (60, 120),
        "four seconds, starting where the playhead was"
    );
    assert_eq!(project.sources().len(), 2);
    assert_eq!(journey.clips_of_track(1), vec![(0, 600, 1)], "the video is as it was");
    let sound_source = project.sources()[1].identifier;
    let peaks = journey.application().waveform_peak_count(sound_source);
    assert!(
        peaks.abs_diff(400) <= 2,
        "a hundred peaks a second for four seconds: {peaks}"
    );

    // It is saved as a sound, at the place it was put, and comes back with its waveform.
    let project_path = scratch.path().join("with_sound.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text, saved) = read_project_file_as_json(&project_path);
    assert_eq!(saved["sources"][1]["kind"], "audio", "{text}");
    assert_eq!(saved["tracks"][0]["clips"][0]["kind"], "audio", "{text}");
    assert_eq!(saved["tracks"][0]["clips"][0]["gap_before_frames"], 60, "{text}");
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path);
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    assert!(fresh.application().every_source_file_is_available());
    assert_eq!(
        what_is_on_the_tracks(fresh.application().project()),
        what_is_on_the_tracks(journey.application().project())
    );
    assert!(fresh.application().waveform_peak_count(sound_source).abs_diff(400) <= 2);

    // The export has both sounds where they belong: the video's tone all through, the file's from 2 s to 6 s.
    let exported_path = scratch.path().join("with_sound.mp4");
    export_to(&mut fresh, &exported_path);
    let sound = support::decoded_sound(&toolchain, &exported_path, 1, 48_000);
    let window = |from: f64, to: f64| &sound[(from * 48_000.0) as usize..(to * 48_000.0) as usize];
    let strength = |window: &[f32], hertz: f64| support::tone_strength(window, hertz, 48_000);
    let before = window(0.5, 1.8);
    assert!((strength(before, 440.0) - TONE_AMPLITUDE).abs() < 0.1 * TONE_AMPLITUDE);
    assert!(strength(before, 330.0) < 0.02, "the file has not started");
    let during = window(2.5, 5.5);
    assert!(
        (strength(during, 330.0) - TONE_AMPLITUDE).abs() < 0.1 * TONE_AMPLITUDE,
        "the file is heard at its full size"
    );
    assert!(
        (strength(during, 440.0) - TONE_AMPLITUDE).abs() < 0.1 * TONE_AMPLITUDE,
        "and so is the video"
    );
    let after = window(6.3, 7.0);
    assert!(strength(after, 330.0) < 0.02, "the file has ended");
    assert!((strength(after, 440.0) - TONE_AMPLITUDE).abs() < 0.1 * TONE_AMPLITUDE);
}

/// How many pixels of a decoded picture (RGB, `width` across) inside `area` (left, top, right,
/// bottom) are nearly white.
fn bright_pixels_in(picture: &[u8], width: usize, area: (usize, usize, usize, usize)) -> usize {
    let (left, top, right, bottom) = area;
    (top..bottom)
        .flat_map(|row| (left..right).map(move |column| (row * width + column) * 3))
        .filter(|&first| picture[first..first + 3].iter().all(|channel| *channel > 215))
        .count()
}

#[test]
fn a_text_object_made_with_the_mouse_is_saved_and_drawn_over_the_picture_in_the_export() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    // Keep four seconds of the video, so that the export is small.
    journey.cut_at_frame(120);
    let second_clip = journey.point_on_the_track_at_frame(121);
    journey.click(second_clip);
    journey.press_key(egui::Key::Delete);
    assert_eq!(journey.clips(), vec![(0, 120, 1)]);

    // A text object on a track in front of the video, from 1 s for 2 s, made as a person makes it.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 30);
    journey.replace_text_in_the_field_showing("Text", "HELLO WORLD");
    journey.set_slider("Size", 100.0);
    journey.set_slider("Shown for", 2.0);
    journey.wait_until_idle();
    let text_track = journey.application().project().tracks()[0].clone();
    let text = text_track.placement_at_index(0).unwrap();
    assert!(text.first_timeline_frame.value().abs_diff(30) <= 1);
    let Clip::Text(text_clip) = text.clip else {
        panic!("a text clip")
    };
    // (What a text says is the value of its text effect since phase 9: the project says what it is.)
    assert_eq!(
        journey.application().project().text_object_of(text.id).unwrap().text,
        "HELLO WORLD"
    );
    assert_eq!(text_clip.duration_in_frames(), 60);
    let text_middle = text.first_timeline_frame.value() as usize + 30;

    // Saved and loaded, the text is as it was.
    let project_path = scratch.path().join("with_text.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text_of_file, saved) = read_project_file_as_json(&project_path);
    assert_eq!(saved["tracks"][0]["clips"][0]["kind"], "text", "{text_of_file}");
    // (What a text says is the `text` value of its text effect in the file since format 18.)
    assert_eq!(
        saved["tracks"][0]["clips"][0]["effects"][0]["values"]["text"], "HELLO WORLD",
        "{text_of_file}"
    );
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path);
    fresh.wait_until_idle();
    assert_eq!(
        what_is_on_the_tracks(fresh.application().project()),
        what_is_on_the_tracks(journey.application().project())
    );

    // The export has the text over the video where its area is, and the video as it is elsewhere.
    let with_text = scratch.path().join("with_text.mp4");
    export_to(&mut fresh, &with_text);
    // Then without the text object, for comparison: the message that the export is finished is
    // closed, and the text object is selected on the timeline and deleted.
    fresh.harness.get_by_label("OK").click();
    fresh.settle();
    let on_the_text = fresh.point_on_the_strip_at_frame(0, text_middle as u64);
    fresh.click(on_the_text);
    fresh.press_key(egui::Key::Delete);
    assert!(fresh.application().project().tracks()[0].is_empty());
    let without_text = scratch.path().join("without_text.mp4");
    export_to(&mut fresh, &without_text);

    let (width, height) = (VIDEO_WIDTH as usize, VIDEO_HEIGHT as usize);
    let picture_with = &decode_video(&toolchain, &with_text, VIDEO_WIDTH, VIDEO_HEIGHT).pictures[text_middle];
    let picture_without = &decode_video(&toolchain, &without_text, VIDEO_WIDTH, VIDEO_HEIGHT).pictures[text_middle];
    // The area of a new text object is 0.1 to 0.9 across and 0.74 to 0.90 down the picture.
    let area = (
        (0.1 * width as f32) as usize,
        (0.74 * height as f32) as usize,
        (0.9 * width as f32) as usize,
        (0.9 * height as f32) as usize,
    );
    let inside_with = bright_pixels_in(picture_with, width, area);
    let inside_without = bright_pixels_in(picture_without, width, area);
    assert!(
        inside_with > inside_without + 30,
        "the letters are white: {inside_with} bright pixels with the text and {inside_without} without"
    );
    let above = (0, 0, width, area.1.saturating_sub(4));
    assert_eq!(
        bright_pixels_in(picture_with, width, above),
        bright_pixels_in(picture_without, width, above),
        "above the area the picture is the same"
    );
}

#[test]
fn a_video_made_smaller_and_dragged_past_the_edge_keeps_its_place_in_the_file_and_is_cut_off_there_in_the_export() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    // Keep two seconds of the video, so that the export is small.
    journey.cut_at_frame(60);
    let second_clip = journey.point_on_the_track_at_frame(61);
    journey.click(second_clip);
    journey.press_key(Key::Delete);
    assert_eq!(journey.clips(), vec![(0, 60, 1)]);
    journey.move_playhead_to_frame(10);
    journey.wait_for_the_preview_to_show_the_current_frame();

    // Picked on the preview, made half the size by its bottom right corner, and then dragged to the left as
    // far as it goes (with the pointer far outside the window).
    let handle = |journey: &Journey, which: &str| {
        journey
            .rectangle_of(&format!("Resize handle at the {which} corner"))
            .center()
    };
    let on_the_video = journey.rectangle_of("Video, clip 0 of track 0").center();
    journey.click(on_the_video);
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    journey.drag(
        PointerButton::Primary,
        bottom_right,
        top_left + (bottom_right - top_left) * 0.5,
    );
    journey.wait_until_idle();
    let (top_left, bottom_right) = (handle(&journey, "top left"), handle(&journey, "bottom right"));
    let middle = top_left + (bottom_right - top_left) * 0.5;
    journey.drag(PointerButton::Primary, middle, middle - vec2(4000.0, 0.0));
    journey.wait_until_idle();

    let track = journey.application().project().tracks()[0].clone();
    let Clip::Video(media) = track.placement_at_index(0).unwrap().clip else {
        panic!("a video clip")
    };
    let area = media.placement().expect("it was put somewhere").area;
    assert!(
        (area.left() + 0.25).abs() < 0.01 && area.top().abs() < 0.01,
        "half of it is before the left edge of the picture: {area:?}"
    );
    assert!(
        (area.width() - 0.5).abs() < 0.01 && (area.height() - 0.5).abs() < 0.01,
        "{area:?}"
    );

    // Saved, the file says where it is, and loaded into a fresh editor, it is there.
    let project_path = scratch.path().join("past_the_edge.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text_of_file, saved) = read_project_file_as_json(&project_path);
    let saved_left = saved["tracks"][0]["clips"][0]["placement"]["area"]["left_fraction_of_frame_width"]
        .as_f64()
        .unwrap_or_else(|| panic!("the place of the video is in the file: {text_of_file}"));
    assert!((saved_left + 0.25).abs() < 0.01, "{text_of_file}");
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path);
    fresh.wait_until_idle();
    assert_eq!(
        what_is_on_the_tracks(fresh.application().project()),
        what_is_on_the_tracks(journey.application().project())
    );

    // The export has the right half of the video in the top left quarter of the picture, and black elsewhere.
    let destination = scratch.path().join("past_the_edge.mp4");
    export_to(&mut fresh, &destination);
    let decoded = decode_video(&toolchain, &destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(decoded.pictures.len(), 60);
    let picture = &decoded.pictures[10];
    let pixel = |x: usize, y: usize| {
        let first = (y * VIDEO_WIDTH as usize + x) * 3;
        &picture[first..first + 3]
    };
    // (The picture is grey, except for the strip along its top with the number of the frame in it.)
    for (x, y) in [(10, 40), (40, 40), (70, 40), (40, 80)] {
        assert!(!is_black(pixel(x, y)), "({x}, {y}) is the video");
    }
    for (x, y) in [(100, 40), (200, 40), (300, 40), (40, 100), (40, 150), (200, 150)] {
        assert!(is_black(pixel(x, y)), "({x}, {y}) is not the video");
    }
}

#[test]
fn of_two_videos_at_once_the_preview_shows_the_one_in_front_and_the_other_where_it_would_have_been_when_that_ends() {
    let toolchain = toolchain_or_skip!();
    let long_video = counter_video(&toolchain); // 600 frames
    let short_video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "counter_5_seconds",
            duration_in_seconds: 5,
            ..TestVideoSpecification::COUNTER
        },
    ); // 150 frames
    let mut journey = Journey::importing(&toolchain, &long_video.path);
    journey.move_playhead_to_frame(60);
    journey.import(&short_video.path);
    // The short one is on a track in front, from timeline frame 60 to 210.
    assert_eq!(journey.application().project().tracks().len(), 2);
    assert_eq!(journey.clips_of_track(0), vec![(0, 150, 1)]);

    for (timeline_frame, shown_frame, which) in [
        (30, 30, "the long video, before the short one begins"),
        (100, 40, "the short one, 40 frames into it"),
        (209, 149, "the last picture of the short one"),
        (210, 210, "the long video again, where it would have been"),
        (400, 400, "the long video, long after"),
    ] {
        journey.move_playhead_to_frame(timeline_frame);
        journey.wait_for_the_preview_to_show_the_current_frame();
        assert_eq!(
            journey.frame_number_in_the_preview(),
            shown_frame,
            "at timeline frame {timeline_frame} the preview shows {which}"
        );
    }
}

#[test]
fn a_video_of_another_frame_rate_than_the_projects_is_added_for_as_long_as_it_lasts_and_the_project_keeps_its_rate() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain); // 30 frames a second
    let other_frame_rate = test_video(&toolchain, &TestVideoSpecification::PAL_3_SECONDS); // 75 pictures at 25
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.cut_at_frame(150);

    // The playhead is at frame 150, and that is where the new video goes.
    journey.import(&other_frame_rate.path);

    assert_eq!(journey.application().message_showing(), None);
    assert_eq!(
        journey.application().project().frame_rate().to_string(),
        "30/1",
        "the first video decides the frame rate, and the second does not change it"
    );
    assert_eq!(
        journey.application().project().tracks().len(),
        2,
        "on a track of its own, in front"
    );
    assert_eq!(
        journey.clips(),
        vec![(0, 90, 1)],
        "75 pictures at 25 a second last 3 seconds, which are 90 frames of the project"
    );
    assert_eq!(journey.timeline_spans_of_track(0), vec![(150, 240)]);
    assert_eq!(
        journey.clips_of_track(1),
        vec![(0, 150, 1), (150, 600, 1)],
        "the first video is as it was"
    );
    assert_eq!(
        journey.application().thumbnail_progress(SourceMediaIdentifier::new(2)),
        (3, 3),
        "one picture for every second of the new video"
    );

    // The preview shows, in each frame, the picture of the video in front that is nearest in time.
    for timeline_frame in [149u64, 150, 151, 153, 154, 189, 239, 240] {
        let expected = match timeline_frame {
            150..240 => picture_shown_in_frame(25, 30, timeline_frame - 150),
            _ => timeline_frame,
        };
        journey.move_playhead_to_frame(timeline_frame);
        journey.wait_for_the_preview_to_show_the_current_frame();
        assert_eq!(
            journey.frame_number_in_the_preview(),
            expected,
            "the preview at timeline frame {timeline_frame}"
        );
    }

    // And so does the export, which is as long as the first video, with the sound of both.
    let destination = scratch.path().join("both.mp4");
    export_to(&mut journey, &destination);
    let expected: Vec<u64> = (0..600u64)
        .map(|timeline_frame| match timeline_frame {
            150..240 => picture_shown_in_frame(25, 30, timeline_frame - 150),
            _ => timeline_frame,
        })
        .collect();
    let exported = decode_video(&toolchain, &destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.frame_numbers(), expected);
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!((video_seconds.unwrap() - 20.0).abs() < 0.01, "{video_seconds:?}");
    assert!((audio_seconds.unwrap() - 20.0).abs() < 0.05, "{audio_seconds:?}");
}

#[test]
fn choosing_another_frame_rate_in_the_project_settings_keeps_every_clip_as_long_in_time_and_the_export_runs_at_that_rate()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain); // 20 seconds at 30 frames a second: 600 frames
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.cut_at_frame(90); // 3 seconds
    journey.cut_at_frame(300); // 10 seconds
    assert_eq!(journey.clips(), vec![(0, 90, 1), (90, 300, 1), (300, 600, 1)]);

    journey.choose_the_frame_rate_of_the_project("30 fps", "24 fps");

    assert_eq!(journey.application().project().frame_rate().to_string(), "24/1");
    assert_eq!(
        journey.clips(),
        vec![(0, 72, 1), (72, 240, 1), (240, 480, 1)],
        "the cuts are at the same moments: 3, 10 and 20 seconds"
    );
    assert_eq!(journey.application().project().total_frame_count(), 480);
    assert!(journey.application().has_unsaved_changes());

    // The export is of 480 frames at 24 a second, each the picture of the video nearest in time.
    let destination = scratch.path().join("at_24.mp4");
    export_to(&mut journey, &destination);
    let exported = decode_video(&toolchain, &destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(
        exported.frame_numbers(),
        (0..480)
            .map(|slot| picture_shown_in_frame(30, 24, slot))
            .collect::<Vec<u64>>()
    );
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!((video_seconds.unwrap() - 20.0).abs() < 0.01, "{video_seconds:?}");
    assert!((audio_seconds.unwrap() - 20.0).abs() < 0.05, "{audio_seconds:?}");
    let streams = support::probe_json(&toolchain, &destination, &["-show_streams"]);
    let video_stream = streams["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .expect("the file has a video stream");
    assert_eq!(video_stream["r_frame_rate"], "24/1");

    // Taking the choice back is one step of the history, and the clips are as they were. (The message that the
    // export is finished is closed first: the keys do nothing while it is up.)
    journey.harness.get_by_label("OK").click();
    journey.settle();
    journey.press_key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(journey.application().project().frame_rate().to_string(), "30/1");
    assert_eq!(journey.clips(), vec![(0, 90, 1), (90, 300, 1), (300, 600, 1)]);
}

#[test]
fn importing_a_file_is_an_edit_that_undo_takes_back_and_redo_puts_back_with_its_pictures_still_there() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain); // 20 seconds at 30 frames a second
    let mut journey = Journey::start(&toolchain);
    assert!(!journey.application().can_undo(), "nothing has been done yet");

    journey.import(&video.path);
    assert_eq!(journey.clips(), vec![(0, 600, 1)]);
    assert!(journey.application().can_undo(), "the import is an edit");

    // Undoing takes the video off the timeline again, and the project back to what it was.
    journey.press_key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(journey.clips(), Vec::new());
    assert!(journey.application().project().sources().is_empty());
    assert!(!journey.application().can_undo());
    assert!(journey.application().can_redo());

    // Redoing brings the clip back, with the file still open and its pictures still made.
    journey.press_key_with(Modifiers::COMMAND, Key::Y);
    assert_eq!(journey.clips(), vec![(0, 600, 1)]);
    assert_eq!(journey.application().project().sources().len(), 1);
    assert!(journey.application().every_source_file_is_available());
    assert_eq!(journey.application().thumbnail_progress(THE_VIDEO), (20, 20));
    journey.wait_for_the_preview_to_show_the_current_frame();
    assert_eq!(journey.frame_number_in_the_preview(), 0);
}

/// Draws frames until the pictures the timeline wanted have been made. (The first frames after a
/// change are when it says what it wants.)
fn wait_for_the_finer_thumbnails(journey: &mut Journey) {
    for _ in 0..3 {
        journey.wait_until_idle();
        journey.settle();
    }
    assert_eq!(
        journey.application().finer_thumbnails_still_wanted(THE_VIDEO),
        0,
        "everything the timeline wanted has been made"
    );
}

#[test]
fn a_short_track_and_a_zoomed_in_timeline_get_finer_thumbnails_made_for_what_is_on_screen_only() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain); // 20 seconds, 640 by 360
    let mut journey = Journey::importing(&toolchain, &video.path);
    assert_eq!(
        journey.application().finer_thumbnails_made(THE_VIDEO),
        0,
        "at the normal size and zoom the picture of every second is all there is need of"
    );

    // Half as tall a track has pictures half as wide, which are half a second apart.
    let boundary = journey.boundary_under_the_track(0);
    journey.drag(PointerButton::Primary, boundary, boundary - vec2(0.0, 52.0));
    assert!((journey.strip_of_track(0).height() - 52.0).abs() < 0.5);
    wait_for_the_finer_thumbnails(&mut journey);
    let after_shortening = journey.application().finer_thumbnails_made(THE_VIDEO);
    assert!(
        (4..=60).contains(&after_shortening),
        "pictures were made for the part on screen, and no more than that: {after_shortening}"
    );

    // Zooming in a lot (the pointer over the strip) makes the pictures a quarter of a second apart or less.
    let over_the_strip = journey.point_on_the_track_at_frame(0);
    journey.scroll_the_wheel(over_the_strip, vec2(0.0, 150.0), Modifiers::COMMAND);
    wait_for_the_finer_thumbnails(&mut journey);
    let after_zooming = journey.application().finer_thumbnails_made(THE_VIDEO);
    assert!(
        after_zooming > after_shortening,
        "zooming in needed pictures of a finer level too: {after_shortening}, then {after_zooming}"
    );
    assert!(
        after_zooming < 20 * 16,
        "and still not for the whole video: {after_zooming} of the {} there could be",
        20 * 16
    );
}

#[test]
fn a_picture_imported_from_a_file_is_decoded_for_the_preview_put_in_front_of_the_video_and_drawn_in_the_export() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    // A picture twice as wide as it is tall, red on its left half and blue on its right.
    let picture_file = red_and_blue_picture(&toolchain, scratch.path(), "red_and_blue.png", 64, 32);
    let mut journey = Journey::importing(&toolchain, &video.path);

    journey.import(&picture_file);

    let picture_source = SourceMediaIdentifier::new(2);
    assert!(
        journey.application().image_file_is_decoded(picture_source),
        "the picture was decoded for the preview"
    );
    let project = journey.application().project();
    assert_eq!(project.tracks().len(), 2, "on a track of its own, in front");
    let picture = project.tracks()[0].entries()[0]
        .clip()
        .as_image()
        .expect("a picture clip");
    assert_eq!(picture.source(), picture_source);
    assert_eq!(
        picture.duration_in_frames(),
        150,
        "five seconds at 30 frames per second"
    );

    // Exported: the 64 by 32 picture is fitted into the 320 by 180 one, 320 across and 160 down with 10
    // above it and 10 below, so its left half is red and its right half is blue over the video.
    let exported_to = scratch.path().join("with_a_picture.mp4");
    export_to(&mut journey, &exported_to);
    let decoded = decode_video(&toolchain, &exported_to, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(decoded.pictures.len(), 600);
    let at = |x: usize, y: usize| {
        let first = (y * VIDEO_WIDTH as usize + x) * 3;
        let picture = &decoded.pictures[75];
        [picture[first], picture[first + 1], picture[first + 2]]
    };
    let [red, green, blue] = at(60, 90);
    assert!(
        red > 200 && green < 60 && blue < 60,
        "red on the left: {red} {green} {blue}"
    );
    let [red, green, blue] = at(260, 90);
    assert!(
        blue > 200 && red < 60 && green < 60,
        "blue on the right: {red} {green} {blue}"
    );
    // After its five seconds the picture is gone and the video is all there is.
    let later = &decoded.pictures[200];
    let first = (90 * VIDEO_WIDTH as usize + 60) * 3;
    assert!(
        !(later[first] > 200 && later[first + 1] < 60 && later[first + 2] < 60),
        "the picture is not there at frame 200"
    );
}

/// Presses the stopwatch of `property` (named as the editor names it: "position", "gain") at the playhead, to make a
/// keyframe there, the way a person does: with the pointer over the control, which brings the buttons.
fn make_a_keyframe_with_the_stopwatch(journey: &mut Journey, property: &str) {
    let stopwatch = journey.rectangle_of(&format!("Make a keyframe of {property} here"));
    journey.click(stopwatch.center());
}

#[test]
fn a_video_animated_with_the_stopwatch_and_the_panel_is_saved_with_its_keyframes_and_exported_as_it_moves_and_fades() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    // Keep three seconds of the video, so that the export is small.
    journey.cut_at_frame(90);
    let second_clip = journey.point_on_the_track_at_frame(91);
    journey.click(second_clip);
    journey.press_key(Key::Delete);
    assert_eq!(journey.clips(), vec![(0, 90, 1)]);

    // The video is made half as wide and tall as the picture, in the middle of it.
    journey.select_the_clip_at(10);
    journey.set_slider("w", 50.0);
    journey.set_slider("h", 50.0);

    // It moves from there to a place further right during the first second: a keyframe for its place with the stopwatch at the
    // start, and the next one made by moving it to the right (which is a change of an animated place, so a keyframe).
    journey.move_playhead_to_frame(0);
    make_a_keyframe_with_the_stopwatch(&mut journey, "position");
    journey.move_playhead_to_frame(30);
    journey.set_slider("x", 75.0);
    // And it gets quieter, from what it is to 20 decibels quieter by 2 seconds.
    journey.move_playhead_to_frame(0);
    make_a_keyframe_with_the_stopwatch(&mut journey, "gain");
    journey.move_playhead_to_frame(60);
    journey.set_slider("Gain", -20.0);

    let clip = journey.application().project().tracks()[0].entries()[0].id();
    let keyframe_frames = |journey: &Journey, property: fastforwardrs::project::PropertyId| -> Vec<u64> {
        journey
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .own_animation()
            .keyframes_of(property)
            .map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame).collect())
            .unwrap_or_default()
    };
    assert_eq!(
        keyframe_frames(&journey, fastforwardrs::project::PropertyId::Position),
        vec![0, 30]
    );
    assert_eq!(
        keyframe_frames(&journey, fastforwardrs::project::PropertyId::Gain),
        vec![0, 60]
    );
    assert_eq!(
        keyframe_frames(&journey, fastforwardrs::project::PropertyId::Size),
        Vec::<u64>::new(),
        "the size was set once for the whole clip"
    );

    // Saved, the file says what is animated, and loaded into a fresh editor it is the same.
    let project_path = scratch.path().join("animated.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text_of_file, saved) = read_project_file_as_json(&project_path);
    assert_eq!(
        saved["file_format_version"],
        fastforwardrs::project_file::FILE_FORMAT_VERSION,
        "{text_of_file}"
    );
    // (The keyframes are in racks, in the drawer of the clip.)
    let rack_of = |property: &str| {
        saved["tracks"][0]["clips"][0]["drawer"]
            .as_array()
            .and_then(|racks| racks.iter().find(|rack| rack["property"] == property))
            .cloned()
            .unwrap_or_else(|| panic!("no rack of {property}: {text_of_file}"))
    };
    assert_eq!(rack_of("position")["keyframes"][1]["frame"], 30, "{text_of_file}");
    assert_eq!(rack_of("gain")["keyframes"][1]["value"], -20.0, "{text_of_file}");
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path);
    fresh.wait_until_idle();
    assert_eq!(
        fresh
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .own_animation(),
        journey
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .own_animation(),
        "the keyframes come back exactly"
    );

    // The export has the video where the keyframes say at each frame, and quieter as the keyframes say.
    let animated = scratch.path().join("animated.mp4");
    export_to(&mut fresh, &animated);
    let (width, height) = (VIDEO_WIDTH as usize, VIDEO_HEIGHT as usize);
    let decoded = decode_video(&toolchain, &animated, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(decoded.pictures.len(), 90);
    for (frame, expected_left) in [(0, 80.0), (15, 120.0), (30, 160.0), (75, 160.0)] {
        let (left, _, right, _) = support::box_of_what_is_not_black(&decoded.pictures[frame], width, height);
        assert!(
            (left as f64 - expected_left).abs() <= 3.0,
            "frame {frame}: the video starts {left} pixels across, not {expected_left}"
        );
        assert!(
            (right - left + 1).abs_diff(160) <= 3,
            "frame {frame}: it is 160 pixels wide"
        );
    }
    let sound = support::decoded_sound(&toolchain, &animated, 1, 48_000);
    let loudness_around = |seconds: f64| {
        let first = ((seconds - 0.1) * 48_000.0) as usize;
        support::loudness(&sound[first..first + 9_600])
    };
    // -5 decibels half way down to the keyframe at 1 second... the gain is 0 at 0 s and -20 at 2 s: -5 at 0.5 s and -20 from 2 s on.
    let ratio = f64::from(loudness_around(2.5) / loudness_around(0.5));
    let expected = 10f64.powf((-20.0 + 5.0) / 20.0);
    assert!(
        (ratio / expected - 1.0).abs() < 0.08,
        "{ratio} times as loud, not {expected}"
    );
}

/// The words that name a rack on the screen, as the screen reader (and the tests) are told them.
fn name_of_the_rack(rack: fastforwardrs::project::RackId) -> String {
    format!("Rack {} of {}", rack.number(), rack.origin())
}

#[test]
fn keyframes_kept_in_the_drawer_of_a_text_lower_the_video_wherever_the_text_or_a_copy_of_it_is() {
    use fastforwardrs::project::{AnimationTarget, PropertyId};

    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain); // 20 seconds with a 440 hertz tone
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);
    // Keep five seconds of the video (150 frames), so that all of it can be seen on the timeline.
    journey.cut_at_frame(150);
    let rest = journey.point_on_the_track_at_frame(151);
    journey.click(rest);
    journey.press_key(Key::Delete);
    assert_eq!(journey.clips(), vec![(0, 150, 1)]);

    // A text object in front of it, from frame 5, which lasts three seconds.
    journey.add_a_track();
    journey.make_a_text_object_at(0, 5);
    journey.harness.run_steps(40);
    let (text, video_clip) = {
        let project = journey.application().project();
        (
            project.tracks()[0].entries()[0].id(),
            project.tracks()[1].entries()[0].id(),
        )
    };
    assert_eq!(
        journey.application().project().placement_of(text).map(|placement| (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        )),
        Some((5, 95))
    );

    // The video's loudness, as the keyframes of the video: 0 decibels, then -20 from frame 25 to 55, then 0 again at 65.
    // (They are in a rack in the drawer of the video, which the stopwatch opens.)
    let on_the_video = journey.point_on_the_strip_at_frame(1, 10);
    journey.click(on_the_video);
    journey.move_playhead_to_frame(15);
    make_a_keyframe_with_the_stopwatch(&mut journey, "gain");
    journey.move_playhead_to_frame(25);
    journey.set_slider("Gain", -20.0);
    journey.move_playhead_to_frame(55);
    make_a_keyframe_with_the_stopwatch(&mut journey, "gain");
    journey.move_playhead_to_frame(65);
    journey.set_slider("Gain", 0.0);
    journey.harness.run_steps(40);
    let loudness_of_the_video = AnimationTarget::Clip(video_clip);
    let frames_of_the_loudness = |journey: &Journey| -> Vec<u64> {
        journey
            .application()
            .project()
            .keyframes_of(loudness_of_the_video, PropertyId::Gain)
            .iter()
            .map(|keyframe| keyframe.frame)
            .collect()
    };
    assert_eq!(frames_of_the_loudness(&journey), vec![15, 25, 55, 65]);
    let rack = {
        let project = journey.application().project();
        let racks = project
            .drawer_of(loudness_of_the_video)
            .expect("the video has a drawer")
            .racks();
        assert_eq!(racks.len(), 1, "one rack, of the loudness, in the drawer of the video");
        racks[0].id()
    };

    // The rack is taken out of the drawer of the video and put in that of the text (which is opened with its button): they
    // are the keyframes of the video that go wherever the text goes.
    let title_of_the_front_track = journey.application().project().tracks()[0].title().to_owned();
    let button = journey.rectangle_of(&format!(
        "Drawer of clip {} of {title_of_the_front_track}",
        text.number()
    ));
    journey.move_pointer_to(journey.strip_of_track(0).center());
    journey.click(button.center());
    journey.harness.run_steps(40);
    let on_the_rack = journey.rectangle_of(&name_of_the_rack(rack));
    let from = egui::pos2(journey.point_on_the_strip_at_frame(1, 40).x, on_the_rack.center().y);
    let into_the_drawer_of_the_text = egui::pos2(from.x, journey.strip_of_track(0).bottom() + 14.0);
    journey.drag(PointerButton::Primary, from, into_the_drawer_of_the_text);
    {
        let project = journey.application().project();
        assert_eq!(
            project
                .drawer_of(AnimationTarget::Clip(text))
                .map(|drawer| drawer.len()),
            Some(1),
            "the rack is in the drawer of the text"
        );
        assert_eq!(
            project.drawer_of(loudness_of_the_video).map(|drawer| drawer.len()),
            Some(0)
        );
    }
    assert_eq!(
        frames_of_the_loudness(&journey),
        vec![15, 25, 55, 65],
        "the video has the same keyframes, at the same times"
    );

    // The text is copied to the right, with the menu of the clip: the copy has a copy of the rack, of the same video.
    let on_the_text = journey.point_on_the_strip_at_frame(0, 50);
    journey.right_click(on_the_text);
    journey.harness.get_by_label("Duplicate to the right").click();
    journey.settle();
    assert_eq!(
        journey.application().project().tracks()[0]
            .placements()
            .map(|placement| (
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value()
            ))
            .collect::<Vec<_>>(),
        vec![(5, 95), (95, 185)]
    );
    assert_eq!(
        frames_of_the_loudness(&journey),
        vec![15, 25, 55, 65, 105, 115, 145, 155]
    );

    // Saved, the rack is in the drawer of each text, and says which clip it is the keyframes of.
    let project_path = scratch.path().join("lowering.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text_of_file, saved) = read_project_file_as_json(&project_path);
    for clip in 0..2 {
        let racks = &saved["tracks"][0]["clips"][clip]["drawer"];
        assert_eq!(racks[0]["property"], "gain", "{text_of_file}");
        assert_eq!(racks[0]["target"]["track"], 1, "{text_of_file}");
        assert_eq!(racks[0]["target"]["clip"], 0, "{text_of_file}");
    }
    assert!(
        saved["tracks"][1]["clips"][0].get("drawer").is_none(),
        "{text_of_file}: the video has none of its own"
    );

    // Loaded into a fresh editor it is the same, and the export has the video quieter where each text is than where it is not.
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path);
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    let fresh_video = fresh.application().project().tracks()[1].entries()[0].id();
    assert_eq!(
        fresh
            .application()
            .project()
            .keyframes_of(AnimationTarget::Clip(fresh_video), PropertyId::Gain)
            .iter()
            .map(|keyframe| (keyframe.frame, keyframe.value.clone()))
            .collect::<Vec<_>>(),
        journey
            .application()
            .project()
            .keyframes_of(loudness_of_the_video, PropertyId::Gain)
            .iter()
            .map(|keyframe| (keyframe.frame, keyframe.value.clone()))
            .collect::<Vec<_>>(),
        "the loudness of the video is the same when the file is loaded"
    );
    let exported_path = scratch.path().join("lowering.mp4");
    export_to(&mut fresh, &exported_path);
    let sound = support::decoded_sound(&toolchain, &exported_path, 1, 48_000);
    let loudness_around = |seconds: f64| {
        let first = ((seconds - 0.1) * 48_000.0) as usize;
        f64::from(support::loudness(&sound[first..first + 9_600]))
    };
    let loud = loudness_around(0.25);
    let quiet = 10f64.powf(-20.0 / 20.0);
    for (seconds, times_as_loud, what) in [
        (1.2, quiet, "the first text is there: 20 decibels quieter"),
        (2.7, 1.0, "between the texts: as loud as it was"),
        (4.2, quiet, "the copy of the text is there: 20 decibels quieter again"),
    ] {
        let ratio = loudness_around(seconds) / loud;
        assert!(
            (ratio / times_as_loud - 1.0).abs() < 0.08,
            "{what}: at {seconds} s it is {ratio} times as loud as it was, and should be {times_as_loud}"
        );
    }
}

/// A video of 60 seconds (1800 frames), a small picture so that it is quick to make.
fn video_of_a_minute(toolchain: &FfmpegToolchain) -> support::TestVideo {
    test_video(
        toolchain,
        &TestVideoSpecification {
            name: "animated_export_60_seconds",
            duration_in_seconds: 60,
            with_audio: false,
            ..TestVideoSpecification::COUNTER
        },
    )
}

/// What the export used to refuse (a project that needed more than 1500 stretches in which everything is in one place, "too much animation to export") is
/// exported now, since the export draws every frame as it is: a video that goes a pixel across and back at every one of 1560 frames is a different place in each, and
/// its left edge is where the keyframes say in every frame.
#[test]
fn an_animation_with_a_different_place_for_every_one_of_more_than_fifteen_hundred_frames_is_exported_and_every_frame_is_where_its_keyframes_say()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = video_of_a_minute(&toolchain);
    let scratch = scratch_directory();
    const FRAMES: u64 = 1560;
    let mut project = support::project_with_tracks_of_videos(
        &toolchain,
        &[&video],
        &[(
            0,
            &[support::ClipOfTheVideo {
                gap_before_frames: 0,
                first_source_frame: 0,
                end_source_frame: FRAMES,
                speed: 1,
            }],
        )],
    );
    let clip = support::clip_number(&project, 0);
    project
        .set_clip_placement(
            clip,
            fastforwardrs::project::Placement::upright(fastforwardrs::project::CanvasArea::new(0.0, 0.0, 0.25, 0.25)),
        )
        .unwrap();
    for frame in 0..FRAMES {
        // (Where the middle of the video is: a quarter of the picture is 80 pixels across and 45 down, so its left edge is at the picture's left edge when its
        // middle is 40 pixels in, and one pixel in from it when its middle is 41.)
        let across = if frame % 2 == 0 { 0.125 } else { 0.125 + 1.0 / 320.0 };
        support::make_a_keyframe(
            &mut project,
            fastforwardrs::project::AnimationTarget::Clip(clip),
            fastforwardrs::project::PropertyId::Position,
            frame,
            fastforwardrs::project::PropertyValue::Pair(across, 0.125),
        );
    }
    let project_path = scratch.path().join("many_places.fastforwardrs.json");
    fastforwardrs::project_file::write_project_file(&project, &project_path).unwrap();
    let mut journey = Journey::start(&toolchain);
    journey.application_mut().load_project_from(project_path);
    journey.wait_until_idle();
    let destination = scratch.path().join("many_places.mp4");

    export_to(&mut journey, &destination);

    let exported = decode_video(&toolchain, &destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.pictures.len() as u64, FRAMES);
    for (frame, picture) in exported.pictures.iter().enumerate() {
        if frame % 7 != 0 && frame > 10 && frame + 10 < FRAMES as usize {
            continue;
        }
        let (left, _top, right, bottom) =
            support::box_of_what_is_not_black(picture, VIDEO_WIDTH as usize, VIDEO_HEIGHT as usize);
        let one_pixel_in = usize::from(frame % 2 == 1);
        // (The top is not looked at: the strip at the top of the test pictures that carries the number of the frame is black where its number has a 0.)
        assert_eq!(
            (left, right - left + 1, bottom),
            (one_pixel_in, 80, 44),
            "frame {frame}: where the video is"
        );
    }
}

#[test]
fn a_hue_shift_put_on_with_the_plus_button_and_turned_with_its_slider_is_saved_with_the_clip_and_exported_turned() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let orange = (200u8, 100u8, 50u8);
    let video = support::flat_colour_video(&toolchain, scratch.path(), orange);
    let mut journey = Journey::importing(&toolchain, &video);

    // Put on with the plus button of the properties panel, and turned a third of the way round with its slider.
    journey.select_the_clip_at(10);
    let plus = journey.rectangle_of("Add an effect");
    journey.click(plus.center());
    journey.open_the_smaller_menu("Built-in");
    let item = journey.rectangle_of("Hue shift");
    journey.click(item.center());
    journey.set_slider("Turn", 120.0);

    // Saved with the clip, and a fresh editor has it.
    let project_path = scratch.path().join("turned.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text_of_file, saved) = read_project_file_as_json(&project_path);
    assert_eq!(
        saved["tracks"][0]["clips"][0]["effects"],
        serde_json::json!([{"effect": "builtin:hue_shift", "version": [1, 1], "instance": 0, "values": {"degrees": 120.0}}]),
        "{text_of_file}"
    );
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path);
    fresh.wait_until_idle();
    let clip = fresh.application().project().tracks()[0].entries()[0].id();
    assert!(
        fresh
            .application()
            .project()
            .has_the_effect(AnimationTarget::Clip(clip), EffectInstanceId::FIRST)
    );

    // The export has the colours turned as the arithmetic of the effect says, in every frame.
    let exported = scratch.path().join("turned.mp4");
    export_to(&mut fresh, &exported);
    let decoded = decode_video(&toolchain, &exported, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(decoded.pictures.len(), 60);
    let mut turned = [orange.0, orange.1, orange.2, 255];
    ColourMatrix::hue_rotation(120.0).apply_to_rgba_pixels(&mut turned);
    for frame in [0, 30, 59] {
        let middle = (VIDEO_HEIGHT as usize / 2 * VIDEO_WIDTH as usize + VIDEO_WIDTH as usize / 2) * 3;
        let found = &decoded.pictures[frame][middle..middle + 3];
        for (channel, (found, wanted)) in found.iter().zip(turned).enumerate() {
            assert!(
                found.abs_diff(wanted) <= 10,
                "frame {frame}, channel {channel}: {found} where the turned colour {turned:?} has {wanted}"
            );
        }
    }
}

#[test]
fn an_exposure_and_a_brightness_put_on_with_the_plus_button_are_exported_as_the_light_and_the_codes_they_make() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let orange = (200u8, 100u8, 50u8);
    let video = support::flat_colour_video(&toolchain, scratch.path(), orange);
    let mut journey = Journey::importing(&toolchain, &video);

    // One stop less light, which is done to the light, and then a tenth of the steps less brightness, which is done to the codes of the screen.
    journey.select_the_clip_at(10);
    for (menu_item, slider, value) in [
        ("Exposure", "Amount", -1.0),
        ("Brightness and contrast", "Brightness", -0.1),
    ] {
        let plus = journey.rectangle_of("Add an effect");
        journey.click(plus.center());
        journey.open_the_smaller_menu("Built-in");
        let item = journey.rectangle_of(menu_item);
        journey.click(item.center());
        journey.set_slider(slider, value);
    }
    let clip = journey.application().project().tracks()[0].entries()[0].id();
    let kinds: Vec<_> = journey
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect();
    assert_eq!(kinds, vec![EffectKind::EXPOSURE, EffectKind::BRIGHTNESS_AND_CONTRAST]);

    let exported = scratch.path().join("darker.mp4");
    export_to(&mut journey, &exported);
    let decoded = decode_video(&toolchain, &exported, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(decoded.pictures.len(), 60);

    // Each colour: its light, halved; the code of that; and a tenth of the steps less.
    let wanted = [orange.0, orange.1, orange.2].map(|code| {
        let light = TransferFunction::Srgb.decode_to_light(f64::from(code) / 255.0) * 0.5;
        (TransferFunction::Srgb.encode_from_light(light) - 0.1).max(0.0) * 255.0
    });
    for frame in [0, 30, 59] {
        let middle = (VIDEO_HEIGHT as usize / 2 * VIDEO_WIDTH as usize + VIDEO_WIDTH as usize / 2) * 3;
        let found = &decoded.pictures[frame][middle..middle + 3];
        for (channel, (found, wanted)) in found.iter().zip(wanted).enumerate() {
            assert!(
                (f64::from(*found) - wanted).abs() <= 10.0,
                "frame {frame}, channel {channel}: {found} where {wanted:.1} was wanted (all of them {wanted:?})"
            );
        }
    }
}

#[test]
fn a_crop_put_on_with_the_plus_button_is_exported_with_the_left_half_cut_off() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let orange = (200u8, 100u8, 50u8);
    let video = support::flat_colour_video(&toolchain, scratch.path(), orange);
    let mut journey = Journey::importing(&toolchain, &video);

    journey.select_the_clip_at(10);
    let plus = journey.rectangle_of("Add an effect");
    journey.click(plus.center());
    journey.open_the_smaller_menu("Built-in");
    let item = journey.rectangle_of("Crop");
    journey.click(item.center());
    journey.set_slider("Left", 0.5);

    let exported = scratch.path().join("cropped.mp4");
    export_to(&mut journey, &exported);
    let decoded = decode_video(&toolchain, &exported, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(decoded.pictures.len(), 60);

    // The left half shows nothing (the black the frame is made on), the right half is the video as it was, and the edge is in the middle of the picture to the pixel.
    let pixel = |frame: usize, across: usize| {
        let first = (VIDEO_HEIGHT as usize / 2 * VIDEO_WIDTH as usize + across) * 3;
        &decoded.pictures[frame][first..first + 3]
    };
    let width = VIDEO_WIDTH as usize;
    for frame in [0, 30, 59] {
        for across in [2, width / 4, width / 2 - 4] {
            assert!(
                pixel(frame, across).iter().all(|colour| *colour <= 24),
                "frame {frame}, column {across}: {:?} where black was wanted",
                pixel(frame, across)
            );
        }
        for across in [width / 2 + 4, width * 3 / 4, width - 3] {
            let found = pixel(frame, across);
            for (channel, (found, wanted)) in found.iter().zip([orange.0, orange.1, orange.2]).enumerate() {
                assert!(
                    found.abs_diff(wanted) <= 10,
                    "frame {frame}, column {across}, channel {channel}: {found} where {wanted} was wanted"
                );
            }
        }
    }
}

#[test]
fn a_video_included_in_the_project_file_travels_with_it_and_plays_and_exports_when_the_original_is_gone() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let footage = scratch.path().join("footage");
    std::fs::create_dir_all(&footage).unwrap();
    let original = footage.join("holiday.mp4");
    std::fs::copy(&video.path, &original).unwrap();
    let original_bytes = std::fs::read(&original).unwrap();

    // Cut, include the video from the menu of its clip, and save as a project archive.
    let mut journey = Journey::importing(&toolchain, &original);
    cut_into_three_with_the_middle_at_ten_times(&mut journey);
    let archive_path = scratch.path().join("trip.ffrs");
    journey.application_mut().save_project_to(archive_path.clone());
    journey.right_click(journey.point_on_the_track_at_frame(100));
    journey.press_button("Include in project file");
    assert!(
        !journey.application().has_unsaved_changes(),
        "the video is put in the project file as it is included, and the project is saved with it"
    );
    assert_eq!(journey.application().message_showing(), None);
    assert!(
        journey.application().project().sources()[0].link.is_included(),
        "the video is included in the project file"
    );
    let archive_size = std::fs::metadata(&archive_path).unwrap().len();
    assert!(
        archive_size > original_bytes.len() as u64,
        "the video is in the archive ({archive_size} bytes, and the video has {})",
        original_bytes.len()
    );

    // The video the project was made from is gone, as it is on another computer. (The first editor is closed first: a file that a
    // program has open cannot be deleted on every system.)
    let clips_of_the_first_editor = journey.clips();
    drop(journey);
    delete_when_nothing_has_it_open(&original);
    std::fs::remove_dir(&footage).unwrap();

    // A fresh editor takes the video out of the archive into its cache, and has all that was there: the clips, the pictures
    // for the timeline, and the video to play and to export.
    let cache = scratch.path().join("cache of files taken out");
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().keep_archive_files_in(cache.clone());
    fresh.application_mut().load_project_from(archive_path.clone());
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    assert!(fresh.application().every_source_file_is_available());
    assert!(!fresh.application().missing_sources_warning_is_showing());
    assert_eq!(fresh.clips(), clips_of_the_first_editor);
    assert_eq!(fresh.application().thumbnail_progress(THE_VIDEO), (20, 20));
    assert!(
        fresh.application().project().sources()[0].link.is_included(),
        "it is included still: that is not changed by loading"
    );
    assert!(!fresh.application().has_unsaved_changes());
    assert!(
        walk_files(&cache).is_empty(),
        "nothing was taken out of the project file: the video is read where it is in it"
    );
    let location = fresh
        .application()
        .location_of_the_source(THE_VIDEO)
        .expect("the video is there");
    let stretch = location.stretch().expect("it is a stretch of the project file");
    assert_eq!(location.file(), std::path::absolute(&archive_path).unwrap());
    assert_eq!(
        stretch.length,
        original_bytes.len() as u64,
        "all of the video, as it was"
    );
    assert_exports_exactly_what_the_project_describes(
        &toolchain,
        &mut fresh,
        &scratch.path().join("from the archive.mp4"),
    );
    fresh.press_button("OK"); // (the message that says the export is finished)

    // Taking it out of the project file puts the video back where it was (the folder is made first: it was taken away),
    // and the project file is written without it.
    std::fs::create_dir_all(&footage).unwrap();
    fresh.right_click(fresh.point_on_the_track_at_frame(100));
    fresh.press_button("Remove from project file");
    assert_eq!(fresh.application().message_showing(), None);
    assert_eq!(
        std::fs::read(&original).unwrap(),
        original_bytes,
        "put back where it was"
    );
    assert!(!fresh.application().project().sources()[0].link.is_included());
    assert!(fresh.application().every_source_file_is_available());
    fresh.application_mut().save_project_to(archive_path.clone());
    let state = fastforwardrs::project_file::archive::state_of_archive(&archive_path).unwrap();
    assert!(
        !state.entries.keys().any(|name| name.starts_with("resources/")),
        "the video is no longer in the project file: {:?}",
        state.entries.keys().collect::<Vec<_>>()
    );
    assert!(
        state.bytes_of_garbage >= original_bytes.len() as u64,
        "its bytes are in the file still, as what is no use, until the project file is written anew"
    );

    // A third editor finds the video where it is again, and has the same clips.
    let mut third = Journey::start(&toolchain);
    third.application_mut().load_project_from(archive_path);
    third.wait_until_idle();
    assert_eq!(third.application().message_showing(), None);
    assert!(third.application().every_source_file_is_available());
    assert_eq!(third.clips(), clips_of_the_first_editor);
}

#[test]
fn a_video_that_is_embedded_and_imported_is_in_the_project_file_at_once_and_is_read_there_and_the_original_is_not_needed()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let footage = scratch.path().join("footage");
    std::fs::create_dir_all(&footage).unwrap();
    let original = footage.join("holiday.mp4");
    std::fs::copy(&video.path, &original).unwrap();
    let original_bytes = std::fs::read(&original).unwrap();
    let archive_path = scratch.path().join("trip.ffrs");

    // A project whose file is an archive; the video is embedded as it is imported.
    let mut journey = Journey::start(&toolchain);
    assert!(journey.application_mut().save_project_to(archive_path.clone()));
    journey.application_mut().embed_and_import_file_from(original.clone());
    journey.wait_until_idle();
    assert_eq!(journey.application().message_showing(), None);
    assert!(journey.application().every_source_file_is_available());
    assert!(journey.application().project().sources()[0].link.is_included());
    assert!(
        !journey.application().has_unsaved_changes(),
        "the project is saved with the video in it"
    );
    let state = fastforwardrs::project_file::archive::state_of_archive(&archive_path).unwrap();
    assert!(
        state.bytes_of_included_files() >= original_bytes.len() as u64,
        "the video is in the project file"
    );
    let location = journey
        .application()
        .location_of_the_source(THE_VIDEO)
        .expect("the video is there");
    assert_eq!(
        location.stretch().expect("a stretch of the project file").length,
        original_bytes.len() as u64
    );
    assert_eq!(journey.application().thumbnail_progress(THE_VIDEO), (20, 20));

    // The original is gone, as on another computer: a fresh editor has the video from the project file alone.
    let clips_of_the_first_editor = journey.clips();
    drop(journey);
    delete_when_nothing_has_it_open(&original);
    std::fs::remove_dir(&footage).unwrap();
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(archive_path);
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    assert!(fresh.application().every_source_file_is_available());
    assert_eq!(fresh.clips(), clips_of_the_first_editor);
    assert_eq!(fresh.application().thumbnail_progress(THE_VIDEO), (20, 20));
    assert_exports_exactly_what_the_project_describes(
        &toolchain,
        &mut fresh,
        &scratch.path().join("from the archive.mp4"),
    );
}

/// A window that has a project file with a video embedded in it (the video is read in the project file, so on Windows it is open
/// in it) and as much garbage as a sound that was embedded and taken out of it again, with the menu of its clip, and the project saved
/// with the menu. (The file cannot be written again yet, with the video read in it.) The sound is about 4.8 megabytes: more than is
/// worth writing the file again for.
fn window_with_a_video_read_in_a_project_file_that_has_garbage_in_it(
    toolchain: &FfmpegToolchain,
    video: &Path,
    archive_path: &Path,
) -> Journey {
    let tone = support::tone_file(toolchain, 330, 50);
    let mut journey = Journey::start(toolchain);
    assert!(journey.application_mut().save_project_to(archive_path.to_owned()));
    journey.application_mut().embed_and_import_file_from(video.to_owned());
    journey.wait_until_idle();
    journey.application_mut().embed_and_import_file_from(tone);
    journey.wait_until_idle();
    assert_eq!(journey.application().message_showing(), None);
    journey.right_click(journey.point_on_the_track_at_frame(100));
    journey.press_button("Remove from project file");
    assert_eq!(journey.application().message_showing(), None);
    journey.press_button("File");
    journey.press_button("Save project");
    journey.wait_until_idle();
    assert_eq!(journey.application().message_showing(), None);
    let state = fastforwardrs::project_file::archive::state_of_archive(archive_path).unwrap();
    assert!(
        state.bytes_of_garbage >= 4_000_000,
        "the sound that was taken out is still in the file: {state:?}"
    );
    journey.wait_for_the_preview_to_show_the_current_frame();
    journey
}

#[test]
fn closing_the_window_writes_the_project_file_again_without_what_was_taken_out_of_it_though_a_video_is_read_in_it() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let archive_path = scratch.path().join("trip.ffrs");
    let mut journey =
        window_with_a_video_read_in_a_project_file_that_has_garbage_in_it(&toolchain, &video.path, &archive_path);
    let length_before = std::fs::metadata(&archive_path).unwrap().len();

    // Closing lets go of the video, writes the file again, and lets the window close.
    let commands = journey.ask_to_close();
    journey.wait_until_idle();

    assert_eq!(journey.application().message_showing(), None);
    assert!(
        !commands.contains(&egui::ViewportCommand::CancelClose),
        "it was little to copy: {commands:?}"
    );
    let after = fastforwardrs::project_file::archive::state_of_archive(&archive_path).unwrap();
    assert_eq!(after.bytes_of_garbage, 0);
    assert!(
        std::fs::metadata(&archive_path).unwrap().len() + 4_000_000 <= length_before,
        "the file is shorter by what was garbage"
    );
    drop(journey);

    // The video is read where it is now, in what is left: a fresh editor has its pictures.
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(archive_path.clone());
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    assert!(fresh.application().every_source_file_is_available());
    assert_eq!(fresh.application().thumbnail_progress(THE_VIDEO), (20, 20));
    fresh.wait_for_the_preview_to_show_the_current_frame();
    assert_eq!(fresh.frame_number_in_the_preview(), 0);
    let location = fresh.application().location_of_the_source(THE_VIDEO).unwrap();
    assert_eq!(
        location.stretch().unwrap().length,
        std::fs::metadata(&video.path).unwrap().len()
    );
}

/// Makes a project file for looking at with a real window of the program (`trip.ffrs` in the folder that the variable names): a video
/// embedded in it, and the garbage that a sound that was embedded and taken out of it leaves, so that closing the window writes the file
/// again. The file is left there. (Run with `--ignored`.)
#[test]
#[ignore = "makes a file for a real window to be looked at with; run with --ignored and FASTFORWARDRS_REAL_WINDOW_FOLDER set"]
fn write_a_project_file_that_has_garbage_for_a_real_window() {
    let toolchain = toolchain_or_skip!();
    let folder = PathBuf::from(
        std::env::var_os("FASTFORWARDRS_REAL_WINDOW_FOLDER").expect("set FASTFORWARDRS_REAL_WINDOW_FOLDER to a folder"),
    );
    std::fs::create_dir_all(&folder).unwrap();
    let video = counter_video(&toolchain);
    let archive_path = folder.join("trip.ffrs");
    let _ = std::fs::remove_file(&archive_path);
    let journey =
        window_with_a_video_read_in_a_project_file_that_has_garbage_in_it(&toolchain, &video.path, &archive_path);
    drop(journey);
    let state = fastforwardrs::project_file::archive::state_of_archive(&archive_path).unwrap();
    println!(
        "{}: {} bytes, of which {} are garbage",
        archive_path.display(),
        std::fs::metadata(&archive_path).unwrap().len(),
        state.bytes_of_garbage
    );
}

#[test]
fn closing_the_window_while_the_sound_plays_from_the_project_file_lets_go_of_the_sound_too() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let archive_path = scratch.path().join("trip.ffrs");
    let mut journey =
        window_with_a_video_read_in_a_project_file_that_has_garbage_in_it(&toolchain, &video.path, &archive_path);
    let speakers = journey::speakers::RecordingSpeakers::new();
    journey.application_mut().play_sound_through(speakers.opener());
    journey.press_button("Play");
    journey.keep_drawing_for(std::time::Duration::from_millis(600));
    assert!(
        journey.application().sound_is_running(),
        "the sound is being decoded from the project file"
    );

    let commands = journey.ask_to_close();
    journey.wait_until_idle();

    assert_eq!(
        journey.application().message_showing(),
        None,
        "the program that decodes the sound has not got the project file open any more"
    );
    assert!(!commands.contains(&egui::ViewportCommand::CancelClose), "{commands:?}");
    let after = fastforwardrs::project_file::archive::state_of_archive(&archive_path).unwrap();
    assert_eq!(after.bytes_of_garbage, 0);
}

#[test]
fn closing_the_window_while_an_export_is_going_on_does_not_write_the_project_file_again() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let _one_export_at_a_time = support::one_export_at_a_time();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let archive_path = scratch.path().join("trip.ffrs");
    let mut journey =
        window_with_a_video_read_in_a_project_file_that_has_garbage_in_it(&toolchain, &video.path, &archive_path);
    let length_before = std::fs::metadata(&archive_path).unwrap().len();
    journey
        .application_mut()
        .export_video_to(scratch.path().join("export.mp4"));
    assert!(
        journey.application().is_working_in_the_background(),
        "the export started"
    );

    let commands = journey.ask_to_close();

    assert_eq!(journey.application().message_showing(), None);
    assert!(!commands.contains(&egui::ViewportCommand::CancelClose), "{commands:?}");
    assert_eq!(
        std::fs::metadata(&archive_path).unwrap().len(),
        length_before,
        "the project file is as it was: the export is reading it"
    );
}

/// Deletes `path`, trying for a little while: a program that has just been closed may take a moment to let go of a file.
fn delete_when_nothing_has_it_open(path: &Path) {
    let started = std::time::Instant::now();
    loop {
        match std::fs::remove_file(path) {
            Ok(()) => return,
            Err(error) if started.elapsed() < std::time::Duration::from_secs(10) => {
                let _ = error;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => panic!("{} could not be deleted: {error}", path.display()),
        }
    }
}

/// Every file under `folder`, however deep.
fn walk_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return files;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk_files(&path));
        } else {
            files.push(path);
        }
    }
    files
}
