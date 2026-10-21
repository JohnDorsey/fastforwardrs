// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::project::{
    Angle, AnimationTarget, CanvasArea, Clip, ClipId, EffectInstanceId, FrameRate, Gain, Placement, PropertyId,
    PropertyValue, SETTINGS, SourceImageProperties, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor, TextColour,
    TimelineFrameIndex, TrackHeight,
};

fn video_properties() -> SourceVideoProperties {
    SourceVideoProperties {
        frame_rate: FrameRate::new(30000, 1001).unwrap(),
        total_frame_count: 900,
        display_width_in_pixels: 1920,
        display_height_in_pixels: 1080,
        has_audio: true,
    }
}

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

/// The place of the clip with this number (counting from 0) on the project's only or front-most track.
fn clip_number(project: &Project, number: usize) -> ClipId {
    project.tracks()[0].entries()[number].id()
}

/// A project edited the way a user would: split in three, with the middle clip sped up and the
/// fast-forward symbol moved.
pub(super) fn edited_project(link: SourceMediaLink) -> Project {
    let mut project = Project::empty();
    project
        .import_source(
            SourceMedia::video(SourceMediaIdentifier::new(1), link, video_properties()),
            frame(0),
        )
        .unwrap();
    project.split_at(frame(300), None);
    project.split_at(frame(600), None);
    let middle = clip_number(&project, 1);
    project.set_clip_speed(middle, SpeedFactor::new(8).unwrap()).unwrap();
    project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.5, 0.25, 0.125));
    project
}

pub(super) fn link_to_missing_video() -> SourceMediaLink {
    SourceMediaLink {
        absolute_path: PathBuf::from("/videos/holiday.mp4"),
        relative_path: Some(PathBuf::from("../videos/holiday.mp4")),
        included_as: None,
    }
}

/// The text a project is saved as, for a project that is in a folder of its own.
pub(super) fn saved(project: &Project) -> String {
    project_to_json_text(project, Path::new("/projects")).unwrap()
}

/// Whether two projects are the same as far as a file can tell. (Their clips and tracks have
/// names that are made up when a project is loaded, so loading a saved project gives other names.)
pub(super) fn assert_same_content(loaded: &Project, original: &Project) {
    assert_eq!(saved(loaded), saved(original));
}

#[test]
fn the_json_has_the_documented_pretty_human_readable_shape() {
    let text = saved(&edited_project(link_to_missing_video()));
    let expected = r#"{
  "file_format": "fastforwardrs-project",
  "file_format_version": 14,
  "sources": [
    {
      "kind": "video",
      "identifier": 1,
      "absolute_path": "/videos/holiday.mp4",
      "relative_path": "../videos/holiday.mp4",
      "frame_rate": "30000/1001",
      "total_frame_count": 900,
      "display_width_in_pixels": 1920,
      "display_height_in_pixels": 1080,
      "has_audio": true
    }
  ],
  "tracks": [
    {
      "title": "track 0",
      "gain_decibels": 0.0,
      "speed_factor": 1,
      "muted": false,
      "clips": [
        {
          "kind": "video",
          "source_identifier": 1,
          "first_source_frame": 0,
          "end_source_frame": 300,
          "speed_factor": 1
        },
        {
          "kind": "video",
          "source_identifier": 1,
          "first_source_frame": 300,
          "end_source_frame": 600,
          "speed_factor": 8
        },
        {
          "kind": "video",
          "source_identifier": 1,
          "first_source_frame": 600,
          "end_source_frame": 900,
          "speed_factor": 1
        }
      ]
    }
  ],
  "speed_indicator": {
    "left_fraction_of_frame_width": 0.5,
    "top_fraction_of_frame_height": 0.25,
    "height_fraction_of_frame_height": 0.125
  }
}
"#;
    assert_eq!(text, expected);
}

#[test]
fn settings_that_follow_the_global_settings_are_not_saved_at_all() {
    let project = edited_project(link_to_missing_video());
    assert_eq!(project.settings(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    let text = saved(&project);
    assert!(!text.contains("settings"), "{text}");
    assert!(
        !text.contains("autosave") && !text.contains("sped_up_clip_sound"),
        "{text}"
    );
}

#[test]
fn only_the_settings_a_project_has_set_for_itself_are_saved() {
    let mut project = edited_project(link_to_missing_video());
    project.set_settings(ProjectSettings {
        autosave: Some(true),
        sped_up_clip_sound: None,
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    });
    let text = saved(&project);
    assert!(
        text.contains("  \"settings\": {\n    \"autosave\": true\n  }"),
        "{text}"
    );
    assert!(
        !text.contains("sped_up_clip_sound"),
        "the one that follows is left out: {text}"
    );

    // An "off" that the project chose is saved too: it is not the same as following the global one.
    project.set_settings(ProjectSettings {
        autosave: Some(false),
        sped_up_clip_sound: Some(SpedUpClipSound::IncreasePitch),
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    });
    let text = saved(&project);
    assert!(text.contains("\"autosave\": false"), "{text}");
    assert!(text.contains("\"sped_up_clip_sound\": \"increase_pitch\""), "{text}");
}

#[test]
fn every_setting_in_the_table_survives_a_trip_through_the_file_in_every_state() {
    // Going through the table of settings (and not naming any setting here) means that a setting
    // added to the table but forgotten in the file format fails this test.
    for setting in SETTINGS {
        let states = std::iter::once(None).chain((0..setting.option_labels().len()).map(Some));
        for chosen in states {
            let mut project = edited_project(link_to_missing_video());
            let mut settings = project.settings();
            setting.set_project_option(&mut settings, chosen);
            project.set_settings(settings);

            let text = saved(&project);
            let loaded = project_from_json_text(&text).unwrap();

            assert_eq!(
                loaded.settings(),
                settings,
                "{} set to {chosen:?}\n{text}",
                setting.label()
            );
        }
    }
}

/// What the line that says the version looks like in a file of the version this build writes.
pub(super) fn current_version_line() -> String {
    version_line(FILE_FORMAT_VERSION)
}

/// What the line that says the version looks like in a file of the version `version`.
pub(super) fn version_line(version: u32) -> String {
    format!("\"file_format_version\": {version}")
}

/// Valid project text with one thing replaced, for the "bad file" tests below.
pub(super) fn valid_text_with(replacing: &str, with: &str) -> String {
    let text = saved(&edited_project(link_to_missing_video()));
    assert!(
        text.contains(replacing),
        "test setup: {replacing:?} not found in the sample project"
    );
    text.replacen(replacing, with, 1)
}

/// Regression test for the change from a yes-or-no setting to a choice of three: a project file
/// written by the earlier version keeps meaning what it meant.
#[test]
fn the_old_yes_or_no_mute_setting_still_means_mute_or_the_sound_at_its_own_pitch() {
    for (old_value, expected) in [
        ("true", SpedUpClipSound::Mute),
        ("false", SpedUpClipSound::MaintainPitch),
    ] {
        let text = valid_text_with(
            "  \"speed_indicator\": {",
            &format!("  \"settings\": {{\n    \"mute_sped_up_clips\": {old_value}\n  }},\n  \"speed_indicator\": {{"),
        );
        let project = project_from_json_text(&text).unwrap();
        assert_eq!(
            project.settings(),
            ProjectSettings {
                autosave: None,
                sped_up_clip_sound: Some(expected),
                ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
            },
            "mute_sped_up_clips: {old_value}"
        );
        // Saved again, it says the same thing in the new way.
        let saved_again = saved(&project);
        assert!(!saved_again.contains("mute_sped_up_clips"), "{saved_again}");
        assert!(saved_again.contains("sped_up_clip_sound"), "{saved_again}");
    }
}

#[test]
fn a_misspelt_setting_is_reported_instead_of_silently_ignored() {
    let text = valid_text_with(
        "  \"speed_indicator\": {",
        "  \"settings\": {\n    \"autosve\": true\n  },\n  \"speed_indicator\": {",
    );
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `autosve`"), "{reason}");
}

// --- files from the earlier versions ---

/// A project file as the version before this one wrote it (format version 2): one video track.
const VERSION_2_TEXT: &str = r#"{
  "file_format": "fastforwardrs-project",
  "file_format_version": 2,
  "source_videos": [
    {
      "identifier": 1,
      "absolute_path": "/videos/holiday.mp4",
      "relative_path": "../videos/holiday.mp4",
      "frame_rate": "30000/1001",
      "total_frame_count": 900,
      "display_width_in_pixels": 1920,
      "display_height_in_pixels": 1080,
      "has_audio": true
    }
  ],
  "video_track": {
    "clips": [
      {
        "source_video_identifier": 1,
        "first_source_frame": 0,
        "end_source_frame": 300,
        "speed_factor": 1
      },
      {
        "source_video_identifier": 1,
        "first_source_frame": 300,
        "end_source_frame": 600,
        "speed_factor": 8
      },
      {
        "source_video_identifier": 1,
        "first_source_frame": 600,
        "end_source_frame": 900,
        "speed_factor": 1
      }
    ]
  },
  "speed_indicator": {
    "left_fraction_of_frame_width": 0.5,
    "top_fraction_of_frame_height": 0.25,
    "height_fraction_of_frame_height": 0.125
  }
}
"#;

#[test]
fn a_project_file_of_the_earlier_layout_loads_as_a_project_with_one_track_of_the_same_clips() {
    let loaded = project_from_json_text(VERSION_2_TEXT).unwrap();
    assert_same_content(&loaded, &edited_project(link_to_missing_video()));
    assert_eq!(loaded.tracks().len(), 1);
    assert_eq!(loaded.tracks()[0].title(), "track 0");
    assert_eq!(loaded.tracks()[0].entries().len(), 3);
}

#[test]
fn a_project_file_from_before_settings_existed_loads_and_follows_every_global_setting() {
    let text = VERSION_2_TEXT.replace("\"file_format_version\": 2", "\"file_format_version\": 1");
    let project = project_from_json_text(&text).unwrap();
    assert_eq!(project.settings(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    assert_eq!(project.tracks()[0].entries().len(), 3);
}

#[test]
fn an_earlier_file_with_settings_keeps_them_including_the_old_yes_or_no_one() {
    let text = VERSION_2_TEXT.replace(
        "  \"speed_indicator\": {",
        "  \"settings\": {\n    \"autosave\": true,\n    \"mute_sped_up_clips\": true\n  },\n  \"speed_indicator\": {",
    );
    let project = project_from_json_text(&text).unwrap();
    assert_eq!(
        project.settings(),
        ProjectSettings {
            autosave: Some(true),
            sped_up_clip_sound: Some(SpedUpClipSound::Mute),
            ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
        }
    );
}

#[test]
fn an_earlier_file_with_a_clip_that_has_no_frames_is_rejected_and_the_clip_is_named() {
    let text = VERSION_2_TEXT.replace("\"end_source_frame\": 600", "\"end_source_frame\": 300");
    match project_from_json_text(&text).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => assert!(reason.contains("clip number 2"), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn an_earlier_file_is_saved_in_the_current_layout_and_that_loads_to_the_same_project() {
    let loaded = project_from_json_text(VERSION_2_TEXT).unwrap();
    let text = saved(&loaded);
    assert!(text.contains(&current_version_line()), "{text}");
    assert!(text.contains("\"tracks\""), "{text}");
    assert!(!text.contains("video_track"), "{text}");
    assert_same_content(&project_from_json_text(&text).unwrap(), &loaded);
}

// --- the height of a track ---

#[test]
fn a_track_of_another_height_is_saved_with_it_and_comes_back_and_one_of_the_normal_height_has_none_in_the_file() {
    let mut project = edited_project(link_to_missing_video());
    let normal_text = saved(&project);
    assert!(!normal_text.contains("height_in_points"), "{normal_text}");

    let track = project.tracks()[0].id();
    project.set_track_height(track, TrackHeight::from_points(64.0)).unwrap();
    let text = saved(&project);
    assert!(text.contains("\"height_in_points\": 64.0"), "{text}");

    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(loaded.tracks()[0].height().points(), 64.0);
    assert_same_content(&loaded, &project);
}

#[test]
fn a_file_of_version_3_has_no_heights_and_its_tracks_are_of_the_normal_height() {
    let text = valid_text_with(&current_version_line(), &version_line(3));
    let project = project_from_json_text(&text).unwrap();
    assert!(
        project
            .tracks()
            .iter()
            .all(|track| track.height() == TrackHeight::DEFAULT)
    );
    assert_eq!(
        project.tracks()[0].entries().len(),
        3,
        "and everything else is read as before"
    );
}

#[test]
fn a_height_in_a_file_that_is_out_of_range_is_brought_to_the_nearest_one_that_is_allowed() {
    for (written, allowed) in [
        ("1000.0", TrackHeight::LARGEST_IN_POINTS),
        ("1.0", TrackHeight::SMALLEST_IN_POINTS),
        ("-20.0", TrackHeight::SMALLEST_IN_POINTS),
    ] {
        let text = valid_text_with(
            "\"muted\": false,",
            &format!("\"muted\": false,\n      \"height_in_points\": {written},"),
        );
        let project = project_from_json_text(&text).unwrap();
        assert_eq!(project.tracks()[0].height().points(), allowed, "{written}");
    }
}

// --- round trips ---

#[test]
fn a_project_survives_a_trip_through_text_unchanged() {
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    let loaded = project_from_json_text(&text).unwrap();
    assert_same_content(&loaded, &project);
    assert_eq!(loaded.sources(), project.sources());
    assert_eq!(loaded.speed_indicator_layout(), project.speed_indicator_layout());
    assert_eq!(loaded.total_frame_count(), project.total_frame_count());
}

#[test]
fn an_empty_project_survives_a_trip_through_text() {
    let text = saved(&Project::empty());
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(loaded.tracks().len(), 1);
    assert!(loaded.tracks()[0].is_empty());
    assert_eq!(loaded.tracks()[0].title(), "track 0");
    assert!(loaded.sources().is_empty());
}

#[test]
fn a_project_survives_a_trip_through_a_real_file_and_no_temporary_file_is_left_behind() {
    let folder = tempfile::tempdir().unwrap();
    let project_path = folder.path().join("holiday.fastforward.json");
    let project = edited_project(link_to_missing_video());

    write_project_file(&project, &project_path).unwrap();

    assert_same_content(&read_project_file(&project_path).unwrap(), &project);
    let leftover_files: Vec<_> = fs::read_dir(folder.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftover_files, vec!["holiday.fastforward.json".to_owned()]);
}

#[test]
fn saving_over_an_existing_project_replaces_it_completely() {
    let folder = tempfile::tempdir().unwrap();
    let project_path = folder.path().join("project.json");
    fs::write(
        &project_path,
        "something much longer than a small project file will ever be ".repeat(500),
    )
    .unwrap();

    write_project_file(&Project::empty(), &project_path).unwrap();

    assert_same_content(&read_project_file(&project_path).unwrap(), &Project::empty());
}

#[test]
fn the_relative_path_is_worked_out_from_where_the_project_is_saved() {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir_all(folder.path().join("videos")).unwrap();
    fs::create_dir_all(folder.path().join("projects")).unwrap();
    let video_path = folder.path().join("videos").join("holiday.mp4");
    fs::write(&video_path, b"not really a video").unwrap();
    let project_path = folder.path().join("projects").join("summer.json");

    let project = edited_project(SourceMediaLink::from_absolute_path(video_path.clone()));
    write_project_file(&project, &project_path).unwrap();

    let loaded = read_project_file(&project_path).unwrap();
    let link = &loaded.sources()[0].link;
    assert_eq!(link.absolute_path, video_path);
    assert_eq!(link.relative_path, Some(PathBuf::from("../videos/holiday.mp4")));
    // The relative path really leads from the project's folder to the video.
    let resolved = project_path
        .parent()
        .unwrap()
        .join(link.relative_path.as_ref().unwrap());
    assert!(resolved.exists(), "{resolved:?} should exist");
}

#[test]
fn a_video_that_cannot_be_found_keeps_the_relative_path_it_already_had() {
    let folder = tempfile::tempdir().unwrap();
    let project_path = folder.path().join("project.json");
    let project = edited_project(link_to_missing_video());

    write_project_file(&project, &project_path).unwrap();

    let link = read_project_file(&project_path).unwrap().sources()[0].link.clone();
    assert_eq!(link, link_to_missing_video());
}

#[test]
fn a_missing_video_with_no_known_relative_path_is_saved_with_a_null_one() {
    let link = SourceMediaLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4"));
    let text = saved(&edited_project(link));
    assert!(text.contains("\"relative_path\": null"), "{text}");
}

#[test]
fn saving_to_a_folder_that_does_not_exist_fails_cleanly() {
    let folder = tempfile::tempdir().unwrap();
    let project_path = folder.path().join("no_such_folder").join("project.json");
    let error = write_project_file(&Project::empty(), &project_path).unwrap_err();
    assert!(matches!(error, ProjectFileError::CouldNotWrite { .. }), "{error:?}");
    assert!(!folder.path().join("no_such_folder").exists());
}

#[test]
fn loading_a_file_that_does_not_exist_fails_cleanly() {
    let folder = tempfile::tempdir().unwrap();
    let error = read_project_file(&folder.path().join("missing.json")).unwrap_err();
    assert!(matches!(error, ProjectFileError::CouldNotRead { .. }), "{error:?}");
}

// --- bad files ---

#[test]
fn text_that_is_not_json_or_not_a_project_is_rejected_with_a_clear_reason() {
    for bad_text in ["", "not json", "[]", "{}", "{\"hello\": 1}"] {
        let error = project_from_json_text(bad_text).unwrap_err();
        assert!(
            matches!(error, ProjectFileError::NotAProjectFile { .. }),
            "{bad_text:?} gave {error:?}"
        );
    }
}

#[test]
fn json_that_is_not_a_fastforwardrs_project_is_rejected() {
    let error =
        project_from_json_text(&valid_text_with("\"fastforwardrs-project\"", "\"something-else\"")).unwrap_err();
    match error {
        ProjectFileError::NotAProjectFile { reason } => assert!(reason.contains("something-else"), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_project_from_a_newer_version_is_refused_rather_than_misread() {
    let newer = FILE_FORMAT_VERSION + 1;
    let error = project_from_json_text(&valid_text_with(&current_version_line(), &version_line(newer))).unwrap_err();
    assert!(
        matches!(
            error,
            ProjectFileError::NewerFileFormat { found, supported }
                if found == newer && supported == FILE_FORMAT_VERSION
        ),
        "{error:?}"
    );
}

// --- the frame rate of the project ---

#[test]
fn a_project_that_follows_its_first_video_has_no_frame_rate_in_its_file_and_one_that_was_given_one_has_it() {
    let mut project = edited_project(link_to_missing_video());
    assert_eq!(project.chosen_frame_rate(), None);
    let own_line = |text: &str| text.lines().any(|line| line.starts_with("  \"frame_rate\":"));
    assert!(
        !own_line(&saved(&project)),
        "no frame rate of the project is written, only those of the videos"
    );

    project.set_frame_rate(FrameRate::new(25, 1).unwrap());
    let text = saved(&project);
    assert!(
        text.contains(
            "
  \"frame_rate\": \"25/1\"
"
        ),
        "{text}"
    );
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(loaded.chosen_frame_rate(), Some(FrameRate::new(25, 1).unwrap()));
    assert_eq!(loaded.frame_rate(), FrameRate::new(25, 1).unwrap());
    assert_same_content(&loaded, &project);
}

#[test]
fn videos_of_different_frame_rates_survive_a_trip_through_the_file_with_the_edits_that_are_made_of_them() {
    let mut project = edited_project(link_to_missing_video());
    let mut other = video_properties();
    other.frame_rate = FrameRate::new(25, 1).unwrap();
    other.total_frame_count = 250;
    project
        .import_source(
            SourceMedia::video(SourceMediaIdentifier::new(9), link_to_missing_video(), other),
            frame(0),
        )
        .unwrap();
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    assert_same_content(&loaded, &project);
    assert_eq!(loaded.frame_rate(), project.frame_rate());
    assert_eq!(
        loaded.frame_count_of_source(SourceMediaIdentifier::new(9)),
        project.frame_count_of_source(SourceMediaIdentifier::new(9))
    );
}

#[test]
fn a_file_of_version_4_has_no_frame_rate_of_its_own_and_the_project_follows_its_first_video() {
    let text = valid_text_with(&current_version_line(), &version_line(4));
    let project = project_from_json_text(&text).unwrap();
    assert_eq!(project.chosen_frame_rate(), None);
    assert_eq!(
        project.frame_rate(),
        FrameRate::new(30000, 1001).unwrap(),
        "that of the video in the sample"
    );
}

/// The reason given for a file that is not a readable project, failing the test for any other
/// kind of error.
pub(super) fn reason_for_unreadable_project(text: &str) -> String {
    match project_from_json_text(text) {
        Err(ProjectFileError::NotAProjectFile { reason }) => reason,
        other => panic!("expected NotAProjectFile, got {other:?}"),
    }
}

#[test]
fn a_misspelt_field_is_reported_instead_of_silently_ignored() {
    let reason = reason_for_unreadable_project(&valid_text_with("\"speed_factor\": 8", "\"speed_facter\": 8"));
    assert!(reason.contains("unknown field `speed_facter`"), "{reason}");
}

#[test]
fn a_missing_field_is_reported_by_name() {
    // Remove the last field of the source video together with the comma before it, so that the
    // text is still valid JSON and the only problem is the missing field.
    let text = valid_text_with("1080,\n      \"has_audio\": true\n", "1080\n");
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("missing field `has_audio`"), "{reason}");
}

#[test]
fn a_speed_factor_of_zero_or_a_fraction_is_rejected() {
    let reason_for = |bad_speed: &str| {
        reason_for_unreadable_project(&valid_text_with(
            "\"speed_factor\": 8",
            &format!("\"speed_factor\": {bad_speed}"),
        ))
    };
    assert!(
        reason_for("0").contains("speed factor must be a whole number of at least 1"),
        "{}",
        reason_for("0")
    );
    assert!(reason_for("-3").contains("invalid value"), "{}", reason_for("-3"));
    assert!(reason_for("2.5").contains("invalid type"), "{}", reason_for("2.5"));
}

#[test]
fn an_invalid_frame_rate_is_rejected() {
    for bad_rate in ["0/1", "30/0", "fast", ""] {
        let reason = reason_for_unreadable_project(&valid_text_with("\"30000/1001\"", &format!("\"{bad_rate}\"")));
        assert!(reason.contains("frame rate"), "{bad_rate:?}: {reason}");
    }
}

#[test]
fn a_clip_with_no_frames_is_rejected_and_the_clip_and_track_are_named() {
    let error = project_from_json_text(&valid_text_with(
        "\"end_source_frame\": 600",
        "\"end_source_frame\": 300",
    ))
    .unwrap_err();
    match error {
        ProjectFileError::InvalidProject { reason } => {
            assert!(reason.contains("clip number 2 of track number 1"), "{reason}");
        }
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_clip_of_an_unknown_file_is_rejected() {
    let text = valid_text_with(
        "\"source_identifier\": 1,\n          \"first_source_frame\": 600",
        "\"source_identifier\": 9,\n          \"first_source_frame\": 600",
    );
    let error = project_from_json_text(&text).unwrap_err();
    match error {
        ProjectFileError::InvalidProject { reason } => assert!(reason.contains('9'), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_clip_of_the_wrong_kind_of_file_is_rejected() {
    // The first clip is a video clip; say it is a sound clip, of a source that is a video.
    let text = valid_text_with(
        "\"kind\": \"video\",\n          \"source_identifier\": 1,",
        "\"kind\": \"audio\",\n          \"source_identifier\": 1,",
    );
    match project_from_json_text(&text).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => assert!(reason.contains("not the kind of file"), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn an_unknown_kind_of_clip_or_source_is_rejected_by_name() {
    let reason = reason_for_unreadable_project(&valid_text_with(
        "\"kind\": \"video\",\n          \"source_identifier\": 1,",
        "\"kind\": \"hologram\",\n          \"source_identifier\": 1,",
    ));
    assert!(reason.contains("unknown variant `hologram`"), "{reason}");
    let reason = reason_for_unreadable_project(&valid_text_with(
        "\"kind\": \"video\",\n      \"identifier\"",
        "\"kind\": \"smell\",\n      \"identifier\"",
    ));
    assert!(reason.contains("unknown variant `smell`"), "{reason}");
}

#[test]
fn symbol_layout_values_that_are_out_of_range_are_corrected_not_rejected() {
    let text = valid_text_with(
        "\"left_fraction_of_frame_width\": 0.5",
        "\"left_fraction_of_frame_width\": 5.0",
    );
    let project = project_from_json_text(&text).unwrap();
    assert_eq!(project.speed_indicator_layout().left_fraction_of_frame_width(), 1.0);
}

// --- what is new in this layout: tracks, sound files, text ---

/// A project with a video, a sound file and a text object, on three tracks with settings of their own.
pub(super) fn project_with_everything() -> Project {
    let mut project = edited_project(link_to_missing_video());
    project
        .import_source(
            SourceMedia::audio(
                SourceMediaIdentifier::new(2),
                SourceMediaLink {
                    absolute_path: PathBuf::from("/music/song.mp3"),
                    relative_path: None,
                    included_as: None,
                },
                SourceAudioProperties {
                    duration_in_microseconds: 12_500_000,
                },
            ),
            frame(450),
        )
        .unwrap();
    let music_track = project.tracks()[0].id();
    let music = clip_number(&project, 0);
    project.set_clip_gain(music, Gain::from_decibels(-7.5)).unwrap();
    project.set_clip_muted(music, true).unwrap();
    project.set_track_title(music_track, "the music").unwrap();
    project.set_track_gain(music_track, Gain::from_decibels(3.0)).unwrap();
    project
        .set_track_speed(music_track, SpeedFactor::new(2).unwrap())
        .unwrap();
    project.set_track_muted(music_track, true).unwrap();

    let captions = project.add_track_in_front();
    project
        .insert_text(
            captions,
            frame(120),
            TextObject {
                text: "Hello\nthere \"world\"".to_owned(),
                font_family: "Arial".to_owned(),
                font_size: FontSize::from_points(54.0),
                colour: TextColour::new(255, 200, 0, 128),
                area: CanvasArea::new(0.25, 0.5, 0.5, 0.125),
                rotation: Angle::from_degrees(-12.5),
            },
            90,
        )
        .unwrap();
    let placed_video = project.tracks()[2].entries()[1].id();
    project
        .set_clip_placement(placed_video, a_place_for_the_video())
        .unwrap();

    // Effects: a hue shift on the first video (as it begins, and the animation tests give it keyframes), on the second video
    // (turned a quarter of the way round), on the text object (as it begins), and on the track of the text (turned back a
    // twelfth).
    let first_video = project.tracks()[2].entries()[0].id();
    let text = project.tracks()[0].entries()[0].id();
    let turned_hue_shifts = [
        (AnimationTarget::Clip(placed_video), 90.0),
        (AnimationTarget::Track(captions), -30.0),
    ];
    for target in [AnimationTarget::Clip(first_video), AnimationTarget::Clip(text)]
        .into_iter()
        .chain(turned_hue_shifts.map(|(target, _)| target))
    {
        project.add_effect(target, EffectKind::HUE_SHIFT).unwrap();
    }
    for (target, degrees) in turned_hue_shifts {
        project
            .set_property_at(target, HUE, PropertyValue::Number(degrees), frame(0))
            .unwrap();
    }
    project
}

/// Where the second video clip of [`project_with_everything`] is on the picture.
fn a_place_for_the_video() -> Placement {
    Placement::new(CanvasArea::new(0.125, 0.25, 0.5, 0.375), Angle::from_degrees(30.0))
}

#[test]
fn tracks_sound_files_text_gaps_and_settings_all_survive_a_trip_through_text() {
    let project = project_with_everything();
    let text = saved(&project);
    let loaded = project_from_json_text(&text).unwrap();

    assert_same_content(&loaded, &project);
    let titles: Vec<&str> = loaded.tracks().iter().map(Track::title).collect();
    assert_eq!(
        titles,
        vec!["track 2", "the music", "track 0"],
        "in the same order, front first"
    );
    assert_eq!(loaded.sources().len(), 2);
    assert_eq!(loaded.total_frame_count(), project.total_frame_count());
    let music = &loaded.tracks()[1];
    assert_eq!(music.gain(), Gain::from_decibels(3.0));
    assert_eq!(music.speed(), SpeedFactor::new(2).unwrap());
    assert!(music.is_muted());
    let media = music.entries()[0].clip().as_media().unwrap();
    assert_eq!(media.gain(), Gain::from_decibels(-7.5));
    assert!(media.is_muted());
    assert_eq!(
        music.entries()[0].gap_before_frames(),
        450,
        "the empty time before the sound is kept"
    );
    let caption = loaded.tracks()[0].entries()[0].clip().as_text().unwrap();
    assert_eq!(caption.object.text, "Hello\nthere \"world\"");
    assert_eq!(caption.object.font_family, "Arial");
    assert_eq!(caption.object.colour, TextColour::new(255, 200, 0, 128));
    assert_eq!(caption.duration_in_frames(), 90);
    assert_eq!(loaded.tracks()[0].entries()[0].gap_before_frames(), 120);
    assert_eq!(caption.object.area, CanvasArea::new(0.25, 0.5, 0.5, 0.125));
    assert_eq!(caption.object.rotation, Angle::from_degrees(-12.5));
    assert!((caption.object.font_size.points() - 54.0).abs() < 0.01);
    let video_clips: Vec<_> = loaded.tracks()[2]
        .entries()
        .iter()
        .map(|entry| entry.clip().placement())
        .collect();
    assert_eq!(
        video_clips,
        vec![None, Some(a_place_for_the_video()), None],
        "only the video that was moved says where it is"
    );
}

#[test]
fn a_text_that_is_turned_says_by_how_much_and_one_that_is_not_says_nothing_about_it() {
    let turned = saved(&project_with_everything());
    assert!(turned.contains("\"rotation_degrees\": -12.5"), "{turned}");

    let mut upright_project = project_with_everything();
    let caption = upright_project.tracks()[0].entries()[0].id();
    let area = upright_project
        .placement_of(caption)
        .unwrap()
        .clip
        .placement()
        .unwrap()
        .area;
    upright_project
        .set_clip_placement(caption, Placement::upright(area))
        .unwrap();
    let upright = saved(&upright_project);
    let captions_part = upright.split("\"kind\": \"text\"").nth(1).unwrap();
    let captions_part = captions_part.split("\"kind\"").next().unwrap();
    assert!(!captions_part.contains("rotation_degrees"), "{captions_part}");
}

#[test]
fn a_video_that_was_placed_says_where_and_how_far_turned_and_one_that_was_not_says_nothing_about_it() {
    let text = saved(&project_with_everything());
    let placement_part = text.split("\"placement\"").nth(1).expect("one video is placed");
    for expected in [
        "\"left_fraction_of_frame_width\": 0.125",
        "\"top_fraction_of_frame_height\": 0.25",
        "\"width_fraction_of_frame_width\": 0.5",
        "\"height_fraction_of_frame_height\": 0.375",
        "\"rotation_degrees\": 30.0",
    ] {
        assert!(
            placement_part.contains(expected),
            "{expected} is not in {placement_part}"
        );
    }
    assert_eq!(
        text.matches("\"placement\"").count(),
        1,
        "the other videos are not placed"
    );

    let without = saved(&edited_project(link_to_missing_video()));
    assert!(!without.contains("placement"), "{without}");
    assert!(!without.contains("rotation"), "{without}");
}

#[test]
fn a_placed_video_that_is_not_turned_is_written_without_an_angle() {
    let mut project = edited_project(link_to_missing_video());
    let video = clip_number(&project, 1);
    project
        .set_clip_placement(video, Placement::upright(CanvasArea::new(0.1, 0.1, 0.5, 0.5)))
        .unwrap();
    let text = saved(&project);
    assert!(text.contains("\"placement\""), "{text}");
    assert!(!text.contains("rotation_degrees"), "{text}");
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(
        loaded.tracks()[0].entries()[1].clip().placement(),
        Some(Placement::upright(CanvasArea::new(0.1, 0.1, 0.5, 0.5)))
    );
}

#[test]
fn files_written_before_clips_could_be_turned_or_placed_are_read_as_upright_and_fitted() {
    // Take the angle and the place out of a file that has them, which is what an older file looks like.
    let mut document: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    let mut taken_out = 0;
    for track in document["tracks"].as_array_mut().unwrap() {
        for clip in track["clips"].as_array_mut().unwrap() {
            let clip = clip.as_object_mut().unwrap();
            taken_out += usize::from(clip.remove("rotation_degrees").is_some());
            taken_out += usize::from(clip.remove("placement").is_some());
        }
    }
    assert_eq!(
        taken_out, 2,
        "the angle of the caption and the place of the video were there to be taken out"
    );

    let loaded = project_from_json_text(&document.to_string()).unwrap();

    let caption = loaded.tracks()[0].entries()[0].clip().as_text().unwrap();
    assert_eq!(caption.object.rotation, Angle::ZERO);
    assert!(
        loaded.tracks()[2]
            .entries()
            .iter()
            .all(|entry| entry.clip().placement().is_none()),
        "every video is fitted inside the picture"
    );
}

#[test]
fn an_angle_that_is_more_than_a_turn_is_kept_as_it_is_and_written_as_it_is() {
    let text = saved(&project_with_everything()).replace("\"rotation_degrees\": -12.5", "\"rotation_degrees\": 347.5");
    let loaded = project_from_json_text(&text).unwrap();
    let caption = loaded.tracks()[0].entries()[0].clip().as_text().unwrap();
    assert_eq!(caption.object.rotation.degrees(), 347.5);
    assert!(saved(&loaded).contains("\"rotation_degrees\": 347.5"));
}

#[test]
fn a_misspelt_field_of_a_placement_is_reported() {
    let text = saved(&project_with_everything()).replace(
        "\"placement\": {",
        "\"placement\": {
 \"rotaton_degrees\": 3.0,",
    );
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("rotaton_degrees"), "{reason}");
}

#[test]
fn a_clips_volume_and_muting_and_the_empty_time_before_it_are_only_written_when_they_matter() {
    let text = saved(&edited_project(link_to_missing_video()));
    for left_out in ["gain_decibels\": ", "gap_before_frames", "\"muted\": true"] {
        let in_clips_only: String = text.split("\"clips\"").nth(1).unwrap().to_owned();
        assert!(!in_clips_only.contains(left_out), "{left_out}: {in_clips_only}");
    }
    let text = saved(&project_with_everything());
    assert!(text.contains("\"gain_decibels\": -7.5"), "{text}");
    assert!(text.contains("\"gap_before_frames\": 120"), "{text}");
    assert!(text.contains("\"muted\": true"), "{text}");
}

#[test]
fn a_text_clip_is_written_with_its_words_font_size_colour_and_place() {
    let text = saved(&project_with_everything());
    for expected in [
        "\"kind\": \"text\"",
        "\"text\": \"Hello\\nthere \\\"world\\\"\"",
        "\"font_family\": \"Arial\"",
        "\"colour\": \"#FFC80080\"",
        "\"duration_in_frames\": 90",
        "\"left_fraction_of_frame_width\": 0.25",
        "\"top_fraction_of_frame_height\": 0.5",
        "\"width_fraction_of_frame_width\": 0.5",
        "\"height_fraction_of_frame_height\": 0.125",
        "\"font_size_fraction_of_frame_height\": 0.05",
        "\"kind\": \"audio\"",
        "\"duration_in_microseconds\": 12500000",
    ] {
        assert!(text.contains(expected), "{expected} is not in {text}");
    }
}

// --- pictures ---

/// A project with a video and, on a track in front of it, a picture, which was put somewhere if `placed`.
fn project_with_a_picture(placed: bool) -> Project {
    let mut project = edited_project(link_to_missing_video());
    project
        .import_source(
            SourceMedia::image(
                SourceMediaIdentifier::new(2),
                SourceMediaLink {
                    absolute_path: PathBuf::from("/pictures/logo.png"),
                    relative_path: Some(PathBuf::from("../pictures/logo.png")),
                    included_as: None,
                },
                SourceImageProperties {
                    display_width_in_pixels: 640,
                    display_height_in_pixels: 480,
                },
            ),
            frame(100),
        )
        .unwrap();
    let picture = project.tracks()[0].entries()[0].id();
    if placed {
        project
            .set_clip_placement(
                picture,
                Placement::new(CanvasArea::new(0.5, 0.125, 0.25, 0.25), Angle::from_degrees(-20.0)),
            )
            .unwrap();
    }
    project
}

#[test]
fn a_picture_and_where_it_is_survive_a_trip_through_text() {
    let project = project_with_a_picture(true);
    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    let picture = loaded.tracks()[0].entries()[0].clip().as_image().unwrap();
    assert_eq!(picture.source(), SourceMediaIdentifier::new(2));
    assert_eq!(picture.duration_in_frames(), 150);
    assert_eq!(
        picture.placement(),
        Some(Placement::new(
            CanvasArea::new(0.5, 0.125, 0.25, 0.25),
            Angle::from_degrees(-20.0)
        ))
    );
    assert_eq!(loaded.tracks()[0].entries()[0].gap_before_frames(), 100);
    assert_eq!(
        loaded
            .source(SourceMediaIdentifier::new(2))
            .unwrap()
            .properties
            .display_size_in_pixels(),
        Some((640, 480))
    );
}

#[test]
fn a_picture_is_written_with_its_file_its_size_and_how_long_it_is_shown_for() {
    let text = saved(&project_with_a_picture(true));
    for expected in [
        "\"kind\": \"image\"",
        "\"absolute_path\": \"/pictures/logo.png\"",
        "\"relative_path\": \"../pictures/logo.png\"",
        "\"display_width_in_pixels\": 640",
        "\"display_height_in_pixels\": 480",
        "\"source_identifier\": 2",
        "\"duration_in_frames\": 150",
        "\"rotation_degrees\": -20.0",
    ] {
        assert!(text.contains(expected), "{expected} is not in {text}");
    }
}

#[test]
fn a_picture_that_was_not_put_anywhere_says_nothing_about_where() {
    let text = saved(&project_with_a_picture(false));
    // (The source of the picture comes first, then its clip, and the clip is the first of the clips.)
    let picture_part = text
        .split("\"kind\": \"image\"")
        .nth(2)
        .expect("the clip of the picture");
    assert!(!picture_part.contains("placement"), "{picture_part}");
    assert!(!text.contains("rotation_degrees"), "{text}");
}

#[test]
fn a_picture_that_is_shown_for_no_time_or_that_uses_a_file_that_is_not_a_picture_is_rejected() {
    let no_time =
        saved(&project_with_a_picture(true)).replace("\"duration_in_frames\": 150", "\"duration_in_frames\": 0");
    match project_from_json_text(&no_time).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => {
            assert!(reason.contains("clip number 1 of track number 1"), "{reason}")
        }
        other => panic!("unexpected error {other:?}"),
    }
    // The picture's clip pointed at the video, which is source number 1.
    let wrong_file =
        saved(&project_with_a_picture(true)).replace("\"source_identifier\": 2", "\"source_identifier\": 1");
    match project_from_json_text(&wrong_file).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => assert!(reason.contains("not the kind of file"), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_project_with_two_videos_is_saved_and_loaded() {
    let mut project = edited_project(link_to_missing_video());
    project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(2),
                SourceMediaLink::from_absolute_path(PathBuf::from("/videos/second.mp4")),
                SourceVideoProperties {
                    total_frame_count: 100,
                    has_audio: false,
                    ..video_properties()
                },
            ),
            frame(900),
        )
        .unwrap();
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    assert_eq!(loaded.sources().len(), 2);
    assert_eq!(loaded.sources()[1].identifier, SourceMediaIdentifier::new(2));
    assert!(!loaded.sources()[1].properties.has_sound());
    assert_same_content(&loaded, &project);
}

#[test]
fn values_of_a_text_object_that_are_out_of_range_are_corrected_not_rejected() {
    let text = saved(&project_with_everything())
        .replace(
            "\"font_size_fraction_of_frame_height\": 0.05",
            "\"font_size_fraction_of_frame_height\": 90.0",
        )
        .replace(
            "\"left_fraction_of_frame_width\": 0.25",
            "\"left_fraction_of_frame_width\": 7.0",
        )
        .replace("\"gain_decibels\": -7.5", "\"gain_decibels\": -700.0");
    let loaded = project_from_json_text(&text).unwrap();
    let caption = loaded.tracks()[0].entries()[0].clip().as_text().unwrap();
    assert_eq!(
        caption.object.font_size.fraction_of_picture_height(),
        FontSize::LARGEST_FRACTION
    );
    assert_eq!(
        caption.object.area.left(),
        0.75,
        "moved back as far as it goes: its middle is at the edge of the picture"
    );
    let media = loaded.tracks()[1].entries()[0].clip().as_media().unwrap();
    assert_eq!(media.gain().decibels(), Gain::SMALLEST_DECIBELS);
}

#[test]
fn objects_that_reach_out_past_the_edges_of_the_picture_are_saved_and_loaded_as_they_are() {
    let mut project = project_with_everything();
    let caption = project.tracks()[0].entries()[0].id();
    let video = project.tracks()[2].entries()[1].id();
    project
        .set_clip_placement(
            caption,
            Placement::new(CanvasArea::new(0.75, 0.875, 0.5, 0.25), Angle::from_degrees(-12.5)),
        )
        .unwrap();
    project
        .set_clip_placement(
            video,
            Placement::new(CanvasArea::new(-0.25, -0.125, 0.5, 0.375), Angle::from_degrees(30.0)),
        )
        .unwrap();

    let text = saved(&project);
    let loaded = project_from_json_text(&text).unwrap();

    assert!(
        text.contains("\"left_fraction_of_frame_width\": -0.25"),
        "the left edge of the video is before the picture: {text}"
    );
    assert_same_content(&loaded, &project);
    let loaded_caption = loaded.tracks()[0].entries()[0].clip().as_text().unwrap();
    assert_eq!(loaded_caption.object.area, CanvasArea::new(0.75, 0.875, 0.5, 0.25));
    assert!(loaded_caption.object.area.right() > 1.0 && loaded_caption.object.area.bottom() > 1.0);
    let loaded_video = loaded.tracks()[2].entries()[1].clip().as_media().unwrap();
    assert_eq!(
        loaded_video.placement().map(|place| place.area),
        Some(CanvasArea::new(-0.25, -0.125, 0.5, 0.375))
    );
}

#[test]
fn a_text_clip_that_lasts_no_time_or_has_a_bad_colour_is_rejected() {
    let no_time = saved(&project_with_everything()).replace("\"duration_in_frames\": 90", "\"duration_in_frames\": 0");
    match project_from_json_text(&no_time).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => {
            assert!(reason.contains("clip number 1 of track number 1"), "{reason}");
        }
        other => panic!("unexpected error {other:?}"),
    }
    let bad_colour = saved(&project_with_everything()).replace("#FFC80080", "orange");
    let reason = reason_for_unreadable_project(&bad_colour);
    assert!(reason.contains("#RRGGBBAA"), "{reason}");
}

#[test]
fn a_misspelt_field_of_a_text_clip_is_reported() {
    let text = saved(&project_with_everything()).replace("\"font_family\"", "\"font_famly\"");
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("font_famly"), "{reason}");
}

#[test]
fn the_names_of_clips_and_tracks_are_made_up_afresh_when_a_project_is_loaded_and_never_saved() {
    let text = saved(&project_with_everything());
    assert!(!text.contains("\"clip\""), "{text}");
    let loaded = project_from_json_text(&text).unwrap();
    let positions: Vec<u32> = loaded.tracks().iter().map(|track| track.id().value()).collect();
    assert_eq!(positions, vec![0, 1, 2], "numbered in the order they are in the file");
    let mut next = loaded.clone();
    let new_track = next.add_track_in_front();
    assert!(loaded.tracks().iter().all(|track| track.id() != new_track));
}

// --- the documentation ---

/// The JSON of the example in the description of the project file (`docs/project-file-format.md`): the first block of code that is JSON.
fn the_example_in_the_description_of_the_project_file() -> String {
    let readme = include_str!("../../docs/project-file-format.md").replace("\r\n", "\n");
    let start = readme
        .find("```json\n")
        .expect("the description of the project file has an example of it")
        + "```json\n".len();
    let length = readme[start..].find("\n```").expect("the example ends");
    readme[start..start + length].to_owned()
}

#[test]
fn the_example_in_the_description_of_the_project_file_is_a_project_file_this_version_reads_and_has_what_it_says_it_has()
{
    let project = project_from_json_text(&the_example_in_the_description_of_the_project_file()).unwrap();

    assert_eq!(project.sources().len(), 3, "a video, a sound and a picture");
    assert_eq!(project.tracks().len(), 2);
    assert_eq!(project.tracks()[0].height().points(), 80.0);
    let caption = project.tracks()[0].entries()[0].clip().as_text().unwrap();
    assert_eq!(caption.object.rotation, Angle::from_degrees(-12.5));
    let picture = project.tracks()[0].entries()[1].clip().as_image().unwrap();
    assert_eq!(picture.source(), SourceMediaIdentifier::new(3));
    assert_eq!(picture.placement().unwrap().rotation, Angle::from_degrees(20.0));
    assert_eq!(project.tracks()[1].entries().len(), 3);
    assert_eq!(
        project.tracks()[0].entries().len(),
        3,
        "a text, a picture and a keyframe crate"
    );
    // The keyframes the example shows: the caption fades in, the first video moves, and the caption, while it is shown, makes
    // the first video quieter (a rack of that, which is kept in the drawer of the caption).
    let caption_animation = project.tracks()[0].entries()[0].own_animation();
    assert_eq!(caption_animation.keyframes_of(PropertyId::Colour).unwrap().len(), 2);
    let video = project.tracks()[1].entries()[0].id();
    let video_animation = project.tracks()[1].entries()[0].own_animation();
    assert_eq!(
        video_animation.value_at(PropertyId::Position, 60),
        Some(PropertyValue::Pair(0.375, 0.5))
    );
    assert!(
        video_animation.keyframes_of(PropertyId::Gain).is_none(),
        "the drawer of the video has no keyframes of its loudness"
    );
    let caption_drawer = project.tracks()[0].entries()[0].drawer();
    assert_eq!(
        caption_drawer.len(),
        3,
        "how it fades in, the loudness of the video, and the turn of its colours"
    );
    assert_eq!(
        caption_drawer.racks()[1].target(),
        AnimationTarget::Clip(video),
        "the rack kept in the drawer of the caption is of the video"
    );
    // The effects the example shows: the caption's colours go a whole turn round in 60 frames (with the keyframes in a rack of its
    // drawer), the picture's are turned a quarter of the way, and the track of the videos turns the colours of both of them.
    let caption = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let picture = AnimationTarget::Clip(project.tracks()[0].entries()[1].id());
    let videos_track = AnimationTarget::Track(project.tracks()[1].id());
    let hue_at =
        |target: AnimationTarget, frame: u64| project.property_value_at(target, HUE, TimelineFrameIndex::new(frame));
    assert_eq!(
        hue_at(caption, 90),
        Some(PropertyValue::Number(180.0)),
        "the caption starts at frame 60: half way is 30 frames in"
    );
    assert_eq!(
        hue_at(caption, 120),
        Some(PropertyValue::Number(360.0)),
        "a whole turn after 60 frames"
    );
    assert_eq!(hue_at(picture, 200), Some(PropertyValue::Number(90.0)));
    assert_eq!(hue_at(videos_track, 0), Some(PropertyValue::Number(15.0)));
    // (The track of the videos has two hue shifts, which have the names 0 and 1 in its list: the second is another effect, with a turn of its own.)
    assert_eq!(project.effects_of(videos_track).unwrap().len(), 2);
    assert_eq!(
        project.property_value_at(
            videos_track,
            PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(1), 0),
            TimelineFrameIndex::new(0)
        ),
        Some(PropertyValue::Number(40.0))
    );
    assert!(project.has_the_effect(caption, EffectInstanceId::FIRST));
    assert!(
        !project.has_the_effect(AnimationTarget::Clip(video), EffectInstanceId::FIRST),
        "the video has only its track's"
    );
    assert_eq!(
        project
            .animation_of(AnimationTarget::Clip(video))
            .value_at(PropertyId::Gain, 80),
        Some(PropertyValue::Number(-6.0)),
        "half way between frame 70 (0 dB) and frame 90 (-12 dB)"
    );
    // The crate, whose rack is of the track the video is on, and the rack of the track itself.
    let crate_entry = &project.tracks()[0].entries()[2];
    assert!(matches!(crate_entry.clip(), Clip::Crate(_)));
    assert_eq!(
        crate_entry.drawer().racks()[0].target(),
        AnimationTarget::Track(project.tracks()[1].id())
    );
    assert_eq!(project.tracks()[1].drawer().len(), 1);
    assert_eq!(project.tracks()[1].drawer().racks()[0].property(), PropertyId::Gain);
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
    // The groups the example shows: a tag of the caption and the first video that turns the colours of each by 30 degrees, a clip group of the
    // caption and the picture, and a track group of both tracks.
    use crate::project::{ChainRow, GroupKind, Member};
    let groups: Vec<(GroupKind, &str)> = project
        .groups()
        .iter()
        .map(|group| (group.kind(), group.name()))
        .collect();
    assert_eq!(
        groups,
        vec![
            (GroupKind::Tag, "intro"),
            (GroupKind::ClipGroup, "titles"),
            (GroupKind::TrackGroup, "everything")
        ]
    );
    assert_eq!(project.problems_with_the_groups(), vec![]);
    assert_eq!(project.groups()[0].effects().len(), 1);
    assert_eq!(
        project.groups()[0].colour(),
        crate::project::GroupColour::new(0x4B, 0xD2, 0x54)
    );
    let caption_id = project.tracks()[0].entries()[0].id();
    let first_video = project.tracks()[1].entries()[0].id();
    let describe = |rows: Vec<ChainRow<'_>>| -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                ChainRow::Effect(effect) => format!("effect {}", effect.kind().name()),
                ChainRow::TagBadge(group) => format!("tag {}", group.name()),
                ChainRow::TrackBadge { .. } => "track".to_owned(),
                ChainRow::ShapeBadge(group) => format!("{} {}", group.kind().name(), group.name()),
            })
            .collect()
    };
    assert_eq!(
        describe(project.chain_rows_of(Member::Clip(caption_id))),
        ["effect hue shift", "tag intro", "clip group titles"],
        "the caption: its own hue shift, then the effects of the tag, and last those of the group it is in"
    );
    assert_eq!(
        describe(project.chain_rows_of(Member::Clip(first_video))),
        ["track", "tag intro"],
        "the first video: the effects of its track first (which turn it by 15 degrees), and then those of the tag"
    );
    assert_eq!(
        project.chain_rows_of(Member::Track(project.tracks()[0].id())).len(),
        1,
        "the front track: the group of tracks"
    );
    // And what the program writes for it is what the example shows (the same project, whichever way it is told).
    let written = saved(&project);
    let again = project_from_json_text(&written).unwrap();
    assert_eq!(saved(&again), written);
}
