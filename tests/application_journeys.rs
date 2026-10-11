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
use fastforwardrs::project::{Clip, Project, SourceMediaIdentifier, TimelineFrameIndex};
use journey::Journey;
use support::{
    TestVideoSpecification, counter_video, decode_video, expected_source_frames, is_black, scratch_directory,
    stream_durations, test_video,
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

/// Cuts at 60 and 150 with the keyboard and sets the middle clip to 10x in its box of settings.
fn cut_into_three_with_the_middle_at_ten_times(journey: &mut Journey) {
    journey.cut_at_frame(60);
    journey.cut_at_frame(150);
    assert_eq!(journey.clips(), vec![(0, 60, 1), (60, 150, 1), (150, 600, 1)]);
    journey.set_the_speed_of_the_clip_at(105, 10);
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
        journey.application().project().total_frame_count()
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
    let toolchain = toolchain_or_skip!();
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
    let toolchain = toolchain_or_skip!();
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
    assert!(reopened.application().missing_sources_warning_is_showing());

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
        .relink_source_from(THE_VIDEO, wrong_frame_rate.path.clone());
    reopened.wait_until_idle();

    let (title, text) = reopened.application().message_showing().expect("a message explains");
    assert_eq!(title, "That file cannot be used");
    assert!(text.contains("frames per second"), "{text}");
    assert!(
        !reopened.application().every_source_file_is_available(),
        "the editor is still waiting for the right video"
    );
    assert_eq!(
        reopened.clips(),
        vec![(0, 90, 1), (90, 600, 1)],
        "the edits are untouched"
    );
}

#[test]
fn a_shorter_replacement_video_keeps_the_edits_and_the_export_is_black_where_the_video_has_nothing() {
    let toolchain = toolchain_or_skip!();
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

    // ...the message says how much of the project is black...
    let (_, text) = reopened.application().message_showing().expect("the export finished");
    let black = timeline_frames - frames_that_exist as u64;
    assert!(
        text.contains(&format!("{black} frames of the project are black")),
        "{black} frames are black: {text}"
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

    journey.application_mut().import_file_from(not_a_video);
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
    assert!(journey.application().project().sources().is_empty());
    assert_eq!(journey.application().project_file_path(), None);
    journey.application_mut().import_file_from(second_video.path.clone());
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

/// How loud ffmpeg's `sine` source is, which is the sound of the test videos and tone files.
const TONE_AMPLITUDE: f64 = 0.125;

#[test]
fn a_sound_file_imported_at_the_playhead_goes_on_a_track_of_its_own_with_a_waveform_and_is_heard_in_the_export() {
    let toolchain = toolchain_or_skip!();
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
    let toolchain = toolchain_or_skip!();
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
    assert_eq!(text_clip.object.text, "HELLO WORLD");
    assert_eq!(text_clip.duration_in_frames(), 60);
    let text_middle = text.first_timeline_frame.value() as usize + 30;

    // Saved and loaded, the text is as it was.
    let project_path = scratch.path().join("with_text.fastforwardrs.json");
    journey.application_mut().save_project_to(project_path.clone());
    let (text_of_file, saved) = read_project_file_as_json(&project_path);
    assert_eq!(saved["tracks"][0]["clips"][0]["kind"], "text", "{text_of_file}");
    assert_eq!(saved["tracks"][0]["clips"][0]["text"], "HELLO WORLD", "{text_of_file}");
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
fn a_video_with_another_frame_rate_than_the_projects_cannot_be_imported_and_the_project_stays_as_it_is() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain); // 30 frames a second
    let other_frame_rate = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "pal_3_seconds",
            frame_rate: "25",
            duration_in_seconds: 3,
            ..TestVideoSpecification::COUNTER
        },
    );
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.cut_at_frame(150);
    let before = what_is_on_the_tracks(journey.application().project());

    journey
        .application_mut()
        .import_file_from(other_frame_rate.path.clone());
    journey.wait_until_idle();

    let (title, text) = journey.application().message_showing().expect("a message explains");
    assert_eq!(title, "The file could not be imported");
    assert!(text.contains("frames per second"), "{text}");
    assert_eq!(
        what_is_on_the_tracks(journey.application().project()),
        before,
        "nothing was added"
    );
    assert_eq!(journey.application().project().sources().len(), 1);
}
