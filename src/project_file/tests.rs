use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::project::{
    CanvasArea, ClipLocation, FrameRate, Gain, SETTINGS, SourceMediaIdentifier, SpedUpClipSound, SpeedFactor,
    TextColour, TimelineFrameIndex,
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
fn clip_number(project: &Project, number: usize) -> ClipLocation {
    ClipLocation {
        track: project.tracks()[0].id(),
        clip: project.tracks()[0].entries()[number].id(),
    }
}

/// A project edited the way a user would: split in three, with the middle clip sped up and the
/// fast-forward symbol moved.
fn edited_project(link: SourceMediaLink) -> Project {
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

fn link_to_missing_video() -> SourceMediaLink {
    SourceMediaLink {
        absolute_path: PathBuf::from("/videos/holiday.mp4"),
        relative_path: Some(PathBuf::from("../videos/holiday.mp4")),
    }
}

/// The text a project is saved as, for a project that is in a folder of its own.
fn saved(project: &Project) -> String {
    project_to_json_text(project, Path::new("/projects")).unwrap()
}

/// Whether two projects are the same as far as a file can tell. (Their clips and tracks have
/// names that are made up when a project is loaded, so loading a saved project gives other names.)
fn assert_same_content(loaded: &Project, original: &Project) {
    assert_eq!(saved(loaded), saved(original));
}

#[test]
fn the_json_has_the_documented_pretty_human_readable_shape() {
    let text = saved(&edited_project(link_to_missing_video()));
    let expected = r#"{
  "file_format": "fastforwardrs-project",
  "file_format_version": 3,
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

/// Valid project text with one thing replaced, for the "bad file" tests below.
fn valid_text_with(replacing: &str, with: &str) -> String {
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
                sped_up_clip_sound: Some(expected)
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
            sped_up_clip_sound: Some(SpedUpClipSound::Mute)
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
    assert!(text.contains("\"file_format_version\": 3"), "{text}");
    assert!(text.contains("\"tracks\""), "{text}");
    assert!(!text.contains("video_track"), "{text}");
    assert_same_content(&project_from_json_text(&text).unwrap(), &loaded);
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
    let error = project_from_json_text(&valid_text_with(
        "\"file_format_version\": 3",
        "\"file_format_version\": 4",
    ))
    .unwrap_err();
    assert!(
        matches!(error, ProjectFileError::NewerFileFormat { found: 4, supported: 3 }),
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
fn project_with_everything() -> Project {
    let mut project = edited_project(link_to_missing_video());
    project
        .import_source(
            SourceMedia::audio(
                SourceMediaIdentifier::new(2),
                SourceMediaLink {
                    absolute_path: PathBuf::from("/music/song.mp3"),
                    relative_path: None,
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
            },
            90,
        )
        .unwrap();
    project
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
    assert!((caption.object.font_size.points() - 54.0).abs() < 0.01);
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
    assert!(caption.object.area.right() <= 1.0 + 1e-6, "{:?}", caption.object.area);
    let media = loaded.tracks()[1].entries()[0].clip().as_media().unwrap();
    assert_eq!(media.gain().decibels(), Gain::SMALLEST_DECIBELS);
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
