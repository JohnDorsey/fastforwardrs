use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::project::{BOOLEAN_SETTINGS, FrameRate, SourceVideoIdentifier, SpeedFactor, TimelineFrameIndex};

fn video_properties() -> SourceVideoProperties {
    SourceVideoProperties {
        frame_rate: FrameRate::new(30000, 1001).unwrap(),
        total_frame_count: 900,
        display_width_in_pixels: 1920,
        display_height_in_pixels: 1080,
        has_audio: true,
    }
}

/// A project edited the way a user would: split in three, with the middle clip sped up and the
/// fast-forward symbol moved.
fn edited_project(link: SourceVideoLink) -> Project {
    let mut project = Project::empty();
    project
        .import_source_video(SourceVideo {
            identifier: SourceVideoIdentifier::new(1),
            link,
            properties: video_properties(),
        })
        .unwrap();
    project.split_at(TimelineFrameIndex::new(300)).unwrap();
    project.split_at(TimelineFrameIndex::new(600)).unwrap();
    project.set_clip_speed(1, SpeedFactor::new(8).unwrap()).unwrap();
    project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.5, 0.25, 0.125));
    project
}

fn link_to_missing_video() -> SourceVideoLink {
    SourceVideoLink {
        absolute_path: PathBuf::from("/videos/holiday.mp4"),
        relative_path: Some(PathBuf::from("../videos/holiday.mp4")),
    }
}

#[test]
fn the_json_has_the_documented_pretty_human_readable_shape() {
    let text = project_to_json_text(&edited_project(link_to_missing_video()), Path::new("/projects")).unwrap();
    let expected = r#"{
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
    assert_eq!(text, expected);
}

#[test]
fn settings_that_follow_the_global_settings_are_not_saved_at_all() {
    let project = edited_project(link_to_missing_video());
    assert_eq!(project.settings(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    assert!(!text.contains("settings"), "{text}");
    assert!(
        !text.contains("autosave") && !text.contains("mute_sped_up_clips"),
        "{text}"
    );
}

#[test]
fn only_the_settings_a_project_has_set_for_itself_are_saved() {
    let mut project = edited_project(link_to_missing_video());
    project.set_settings(ProjectSettings {
        autosave: Some(true),
        mute_sped_up_clips: None,
    });
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    assert!(
        text.contains("  \"settings\": {\n    \"autosave\": true\n  }"),
        "{text}"
    );
    assert!(
        !text.contains("mute_sped_up_clips"),
        "the one that follows is left out: {text}"
    );

    // An "off" that the project chose is saved too: it is not the same as following the global one.
    project.set_settings(ProjectSettings {
        autosave: Some(false),
        mute_sped_up_clips: Some(true),
    });
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    assert!(text.contains("\"autosave\": false"), "{text}");
    assert!(text.contains("\"mute_sped_up_clips\": true"), "{text}");
}

#[test]
fn every_setting_in_the_table_survives_a_trip_through_the_file_in_every_state() {
    // Going through the table of settings (and not naming any setting here) means that a setting
    // added to the table but forgotten in the file format fails this test.
    for setting in BOOLEAN_SETTINGS {
        for chosen in [None, Some(false), Some(true)] {
            let mut project = edited_project(link_to_missing_video());
            let mut settings = project.settings();
            *(setting.project)(&mut settings) = chosen;
            project.set_settings(settings);

            let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
            let loaded = project_from_json_text(&text).unwrap();

            assert_eq!(
                loaded.settings(),
                settings,
                "{} set to {chosen:?}\n{text}",
                setting.label
            );
        }
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

#[test]
fn a_project_file_from_before_settings_existed_loads_and_follows_every_global_setting() {
    let text = valid_text_with("\"file_format_version\": 2", "\"file_format_version\": 1");
    let project = project_from_json_text(&text).unwrap();
    assert_eq!(project.settings(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    assert_eq!(project.video_track().clips().len(), 3);
}

#[test]
fn a_project_survives_a_trip_through_text_unchanged() {
    let project = edited_project(link_to_missing_video());
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    assert_eq!(project_from_json_text(&text).unwrap(), project);
}

#[test]
fn an_empty_project_survives_a_trip_through_text() {
    let text = project_to_json_text(&Project::empty(), Path::new("/projects")).unwrap();
    assert_eq!(project_from_json_text(&text).unwrap(), Project::empty());
}

#[test]
fn a_project_survives_a_trip_through_a_real_file_and_no_temporary_file_is_left_behind() {
    let folder = tempfile::tempdir().unwrap();
    let project_path = folder.path().join("holiday.fastforward.json");
    let project = edited_project(link_to_missing_video());

    write_project_file(&project, &project_path).unwrap();

    assert_eq!(read_project_file(&project_path).unwrap(), project);
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

    assert_eq!(read_project_file(&project_path).unwrap(), Project::empty());
}

#[test]
fn the_relative_path_is_worked_out_from_where_the_project_is_saved() {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir_all(folder.path().join("videos")).unwrap();
    fs::create_dir_all(folder.path().join("projects")).unwrap();
    let video_path = folder.path().join("videos").join("holiday.mp4");
    fs::write(&video_path, b"not really a video").unwrap();
    let project_path = folder.path().join("projects").join("summer.json");

    let project = edited_project(SourceVideoLink::from_absolute_path(video_path.clone()));
    write_project_file(&project, &project_path).unwrap();

    let loaded = read_project_file(&project_path).unwrap();
    let link = &loaded.source_videos()[0].link;
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

    let link = read_project_file(&project_path).unwrap().source_videos()[0]
        .link
        .clone();
    assert_eq!(link, link_to_missing_video());
}

#[test]
fn a_missing_video_with_no_known_relative_path_is_saved_with_a_null_one() {
    let link = SourceVideoLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4"));
    let text = project_to_json_text(&edited_project(link), Path::new("/projects")).unwrap();
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

/// Valid project text with one thing replaced, for the "bad file" tests below.
fn valid_text_with(replacing: &str, with: &str) -> String {
    let text = project_to_json_text(&edited_project(link_to_missing_video()), Path::new("/projects")).unwrap();
    assert!(
        text.contains(replacing),
        "test setup: {replacing:?} not found in the sample project"
    );
    text.replacen(replacing, with, 1)
}

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
    let error = project_from_json_text(&valid_text_with(
        "\"file_format_version\": 2",
        "\"file_format_version\": 3",
    ))
    .unwrap_err();
    assert!(
        matches!(error, ProjectFileError::NewerFileFormat { found: 3, supported: 2 }),
        "{error:?}"
    );
}

/// The reason given for a file that is not a readable project, failing the test for any other
/// kind of error.
fn reason_for_unreadable_project(text: &str) -> String {
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
fn a_clip_with_no_frames_is_rejected_and_the_clip_is_named() {
    let error = project_from_json_text(&valid_text_with(
        "\"end_source_frame\": 600",
        "\"end_source_frame\": 300",
    ))
    .unwrap_err();
    match error {
        ProjectFileError::InvalidProject { reason } => assert!(reason.contains("clip number 2"), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_clip_of_an_unknown_video_is_rejected() {
    let text = valid_text_with(
        "\"source_video_identifier\": 1,\n        \"first_source_frame\": 600",
        "\"source_video_identifier\": 9,\n        \"first_source_frame\": 600",
    );
    let error = project_from_json_text(&text).unwrap_err();
    match error {
        ProjectFileError::InvalidProject { reason } => assert!(reason.contains('9'), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
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

#[test]
fn a_project_with_two_videos_is_saved_and_loaded_even_though_only_one_can_be_edited_today() {
    let mut project = edited_project(link_to_missing_video());
    let text = project_to_json_text(&project, Path::new("/projects")).unwrap();
    // Add a second video by hand-editing the JSON, as a future version of the editor would save it.
    let second_video = r#"    },
    {
      "identifier": 2,
      "absolute_path": "/videos/second.mp4",
      "relative_path": null,
      "frame_rate": "30000/1001",
      "total_frame_count": 100,
      "display_width_in_pixels": 1920,
      "display_height_in_pixels": 1080,
      "has_audio": false
    }
  ],"#;
    let text_with_two_videos = text.replacen("    }\n  ],", second_video, 1);
    project = project_from_json_text(&text_with_two_videos).unwrap();
    assert_eq!(project.source_videos().len(), 2);
    assert_eq!(project.source_videos()[1].identifier, SourceVideoIdentifier::new(2));
    assert!(!project.source_videos()[1].properties.has_audio);
}
