//! Whole journeys through the real editor with a real video and the real ffmpeg: importing,
//! editing with the mouse, saving, loading into a fresh editor, losing the video, and exporting.
//!
//! The window is driven with simulated input (see the `journey` module), and the places a file
//! dialog would choose are given to the editor directly. Every journey ends by checking what the
//! editor holds against what the project file says, or what the exported video contains.

mod journey;
mod support;

use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;
use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::TimelineFrameIndex;
use journey::Journey;
use support::{
    TestVideoSpecification, counter_video, decode_video, expected_source_frames, scratch_directory, stream_durations,
    test_video,
};

/// The size of the pictures the test videos are exported at.
const VIDEO_WIDTH: u32 = 320;
const VIDEO_HEIGHT: u32 = 180;

/// `path` in a form in which two paths to the same file are equal.
fn same_file_as(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|error| panic!("{} cannot be resolved: {error}", path.display()))
}

fn read_project_file_as_json(path: &Path) -> (String, serde_json::Value) {
    let text = std::fs::read_to_string(path).expect("the project file was written");
    let json = serde_json::from_str(&text).expect("the project file is JSON");
    (text, json)
}

/// Cuts at 60 and 150 with the mouse and sets the middle clip to 10x by dragging up.
fn cut_into_three_with_the_middle_at_ten_times(journey: &mut Journey) {
    journey.cut_at_frame(60);
    journey.cut_at_frame(150);
    assert_eq!(journey.clips(), vec![(0, 60, 1), (60, 150, 1), (150, 600, 1)]);
    journey.drag_the_speed_slider_up(105, 200.0);
    assert_eq!(journey.clips(), vec![(0, 60, 1), (60, 150, 10), (150, 600, 1)]);
}

fn export_to(journey: &mut Journey, destination: &Path) {
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
        journey.application().project().video_track().total_frame_count()
    );
    let exported = decode_video(toolchain, destination, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.frame_numbers(), expected);
}

#[test]
fn importing_a_video_makes_one_clip_and_a_picture_for_every_second() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);

    let journey = Journey::importing(&toolchain, &video.path);

    assert_eq!(journey.clips(), vec![(0, 600, 1)]);
    assert!(
        journey.application().has_unsaved_changes(),
        "a new project has not been saved"
    );
    assert_eq!(journey.application().project_file_path(), None);
    assert_eq!(journey.application().message_showing(), None);
    assert_eq!(
        journey.application().thumbnail_progress(),
        (20, 20),
        "one picture for each of the 20 seconds"
    );
    let source_video = &journey.application().project().source_videos()[0];
    assert_eq!(
        source_video.link.absolute_path,
        std::path::absolute(&video.path).unwrap()
    );
    assert_eq!(source_video.properties.total_frame_count, 600);
    assert_eq!(
        (
            source_video.properties.display_width_in_pixels,
            source_video.properties.display_height_in_pixels
        ),
        (VIDEO_WIDTH, VIDEO_HEIGHT)
    );
    assert!(source_video.properties.has_audio);
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
    let total_frames = journey.application().project().video_track().total_frame_count();
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
            .application()
            .project()
            .video_track()
            .locate(current)
            .expect("the current frame is on the timeline")
            .source_frame
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
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &video.path);

    // Edit with the mouse.
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
    assert_eq!(saved["video_track"]["clips"][1]["speed_factor"], 10);
    assert_eq!(saved["video_track"]["clips"][1]["first_source_frame"], 60);
    assert_eq!(
        saved["source_videos"][0]["absolute_path"],
        std::path::absolute(&video.path).unwrap().to_string_lossy().as_ref()
    );
    assert!(saved["source_videos"][0]["relative_path"].is_string(), "{text}");

    // Load into a fresh editor: everything is as it was.
    let mut fresh = Journey::start(&toolchain);
    fresh.application_mut().load_project_from(project_path.clone());
    fresh.wait_until_idle();
    assert_eq!(fresh.application().message_showing(), None);
    assert!(fresh.application().source_video_is_available());
    assert!(!fresh.application().missing_source_warning_is_showing());
    assert_eq!(fresh.clips(), journey.clips());
    assert_eq!(
        fresh.application().project().video_track(),
        journey.application().project().video_track()
    );
    assert_eq!(
        fresh.application().project().speed_indicator_layout(),
        journey.application().project().speed_indicator_layout()
    );
    assert_eq!(
        fresh.application().project().source_videos()[0].properties,
        journey.application().project().source_videos()[0].properties
    );
    assert!(
        !fresh.application().has_unsaved_changes(),
        "a loaded project has nothing unsaved"
    );
    assert_eq!(fresh.application().project_file_path(), Some(project_path.as_path()));
    assert_eq!(fresh.application().thumbnail_progress(), (20, 20));

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
    assert_eq!(
        saved["source_videos"][0]["relative_path"], "footage/holiday.mp4",
        "{text}"
    );
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
        reopened.application().source_video_is_available(),
        "found through the relative path"
    );
    assert!(!reopened.application().missing_source_warning_is_showing());
    assert_eq!(reopened.clips(), clips_before);
    assert_eq!(reopened.application().thumbnail_progress(), (20, 20));
    assert_eq!(
        same_file_as(&reopened.application().project().source_videos()[0].link.absolute_path),
        same_file_as(&new_folder.join("footage").join("holiday.mp4")),
        "the project now remembers where the video really is"
    );
}

#[test]
fn a_project_whose_video_cannot_be_found_warns_keeps_every_edit_and_can_be_pointed_at_the_video_again() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let video_copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();

    let mut journey = Journey::importing(&toolchain, &video_copy);
    journey.cut_at_frame(90);
    journey.cut_at_frame(180);
    journey.drag_the_speed_slider_up(135, 100.0);
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
    assert!(!reopened.application().source_video_is_available());
    assert!(reopened.application().missing_source_warning_is_showing());
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

    // The program stays interactive: the video track can still be cut while the video is missing.
    reopened.cut_at_frame(30);
    let clips_after_the_cut = vec![(0, 30, 1), (30, 90, 1), (90, 180, 3), (180, 600, 1)];
    assert_eq!(
        reopened.clips(),
        clips_after_the_cut,
        "editing still works while the video is missing"
    );
    assert!(reopened.application().has_unsaved_changes());

    // Choosing the video again brings it back, with all the edits (including the one made meanwhile).
    reopened.application_mut().import_video_from(moved_away.clone());
    reopened.wait_until_idle();
    assert_eq!(reopened.application().message_showing(), None);
    assert!(reopened.application().source_video_is_available());
    assert!(!reopened.application().missing_source_warning_is_showing());
    assert!(
        reopened
            .harness
            .query_by_label_contains("must be chosen again")
            .is_none()
    );
    assert_eq!(reopened.clips(), clips_after_the_cut);
    assert_eq!(reopened.application().thumbnail_progress(), (20, 20));
    assert_eq!(
        same_file_as(&reopened.application().project().source_videos()[0].link.absolute_path),
        same_file_as(&moved_away)
    );

    // And the project is whole: its export has exactly the frames the edits describe.
    assert_exports_exactly_what_the_project_describes(&toolchain, &mut reopened, &scratch.path().join("relinked.mp4"));
}

#[test]
fn a_replacement_video_with_a_different_frame_rate_is_refused_and_the_edits_stay() {
    let toolchain = toolchain_or_skip!();
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
    assert!(reopened.application().missing_source_warning_is_showing());

    let wrong_frame_rate = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "ntsc_4_seconds",
            frame_rate: "30000/1001",
            duration_in_seconds: 4,
            ..TestVideoSpecification::COUNTER
        },
    );
    reopened
        .application_mut()
        .import_video_from(wrong_frame_rate.path.clone());
    reopened.wait_until_idle();

    let (title, text) = reopened.application().message_showing().expect("a message explains");
    assert_eq!(title, "That video cannot be used");
    assert!(text.contains("frames per second"), "{text}");
    assert!(
        !reopened.application().source_video_is_available(),
        "the editor is still waiting for the right video"
    );
    assert_eq!(
        reopened.clips(),
        vec![(0, 90, 1), (90, 600, 1)],
        "the edits are untouched"
    );
}

#[test]
fn a_shorter_replacement_video_keeps_the_edits_and_the_export_has_only_the_frames_that_exist_with_matching_sound() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let video_copy = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &video_copy).unwrap();
    let mut journey = Journey::importing(&toolchain, &video_copy);
    journey.cut_at_frame(100);
    journey.cut_at_frame(200);
    journey.drag_the_speed_slider_up(150, 100.0);
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
    reopened.application_mut().import_video_from(shorter_video.path.clone());
    reopened.wait_until_idle();

    let (title, text) = reopened.application().message_showing().expect("the lengths differ");
    assert_eq!(title, "The video is not the same length");
    assert!(text.contains("600 frames") && text.contains("150"), "{text}");
    assert_eq!(reopened.clips(), edited_clips, "all the edits are kept");

    // The export has the frames of the clips that exist, and not one more...
    let exported_path = scratch.path().join("shorter.mp4");
    export_to(&mut reopened, &exported_path);
    let project = reopened.application().project();
    let timeline_frames = project.video_track().total_frame_count();
    let expected: Vec<u64> = expected_source_frames(project)
        .into_iter()
        .filter(|&source_frame| source_frame < 150)
        .collect();
    assert_eq!(expected.len(), 100 + 17, "frames 0 to 99, then 100, 103, ... 148");
    let exported = decode_video(&toolchain, &exported_path, VIDEO_WIDTH, VIDEO_HEIGHT);
    assert_eq!(exported.frame_numbers(), expected);

    // ...the message says how much of the project is not in it...
    let (_, text) = reopened.application().message_showing().expect("the export finished");
    let left_out = timeline_frames - expected.len() as u64;
    assert!(
        text.contains(&format!("{left_out} frames of the project are not in it")),
        "{left_out} frames were left out: {text}"
    );

    // ...and the sound ends where the picture does (it used to run on for over half a second).
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &exported_path);
    let (video_seconds, audio_seconds) = (video_seconds.unwrap(), audio_seconds.unwrap());
    assert!(
        (video_seconds - 3.9).abs() < 0.01,
        "117 frames at 30 per second: {video_seconds}"
    );
    assert!(
        (audio_seconds - video_seconds).abs() < 0.06,
        "the sound is {audio_seconds} seconds long and the picture {video_seconds}"
    );
}

#[test]
fn a_file_that_is_not_a_video_is_refused_with_a_message_and_the_project_stays_empty() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let not_a_video = scratch.path().join("notes.mp4");
    std::fs::write(&not_a_video, "this is not a video").unwrap();
    let mut journey = Journey::start(&toolchain);

    journey.application_mut().import_video_from(not_a_video);
    journey.wait_until_idle();

    let (title, text) = journey.application().message_showing().expect("a message explains");
    assert_eq!(title, "The video could not be opened");
    assert!(text.contains("notes.mp4"), "{text}");
    assert!(!journey.application().source_video_is_available());
    assert!(journey.application().project().video_track().is_empty());
    assert!(journey.application().project().source_videos().is_empty());
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
    assert!(journey.application().source_video_is_available());
}

#[test]
fn a_new_project_forgets_everything_about_the_old_one() {
    let toolchain = toolchain_or_skip!();
    let first_video = counter_video(&toolchain);
    let second_video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "pal_3_seconds",
            frame_rate: "25",
            duration_in_seconds: 3,
            ..TestVideoSpecification::COUNTER
        },
    );
    let scratch = scratch_directory();
    let mut journey = Journey::importing(&toolchain, &first_video.path);
    cut_into_three_with_the_middle_at_ten_times(&mut journey);
    journey
        .application_mut()
        .save_project_to(scratch.path().join("first.fastforwardrs.json"));

    journey.application_mut().start_empty_project();
    assert!(journey.application().project().source_videos().is_empty());
    assert_eq!(journey.application().project_file_path(), None);
    journey.application_mut().import_video_from(second_video.path.clone());
    journey.wait_until_idle();

    assert_eq!(journey.application().message_showing(), None);
    assert_eq!(
        journey.clips(),
        vec![(0, 75, 1)],
        "a single clip over the whole new video"
    );
    assert_eq!(journey.application().project().source_videos().len(), 1);
    assert_eq!(
        journey.application().project_file_path(),
        None,
        "the new project has no file yet"
    );
    assert!(journey.application().has_unsaved_changes());
    assert_eq!(journey.application().current_frame(), TimelineFrameIndex::new(0));
    assert_eq!(journey.application().thumbnail_progress(), (3, 3));
    assert_eq!(
        journey
            .application()
            .project()
            .timeline_frame_rate()
            .map(|rate| rate.to_string()),
        Some("25/1".to_owned())
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
    let toolchain = toolchain_or_skip!();
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
