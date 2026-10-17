// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of the source files in a project file (version 12; the design is `docs/engine-phase-3-design.md`, decisions C5 and C16): what the user says a source file's
//! colours are is saved with the source, and a file of version 11, which has nothing of the kind, is read as one in which the user said nothing.

use std::path::PathBuf;

use super::tests::*;
use super::*;
use crate::colour_space::{StandardPrimaries, TransferFunction, VideoMatrix, VideoRange};
use crate::project::{
    FrameRate, InputColourOverride, SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaIdentifier,
    SourceMediaLink, SourceVideoProperties, SpedUpClipSound, TimelineFrameIndex,
};

/// A project file as the program wrote it when its version was 11, kept as it was (MG-03): what `saved(&edited_project(link_to_missing_video()))` was, with the settings
/// that a project could have then. Nothing in it is about colour.
const A_FILE_OF_VERSION_11: &str = r#"{
  "file_format": "fastforwardrs-project",
  "file_format_version": 11,
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
  },
  "settings": {
    "autosave": false,
    "sped_up_clip_sound": "increase_pitch"
  }
}
"#;

fn everything_said() -> InputColourOverride {
    InputColourOverride {
        matrix: Some(VideoMatrix::Bt2020),
        range: Some(VideoRange::Full),
        primaries: Some(StandardPrimaries::DisplayP3),
        curve: Some(TransferFunction::Gamma22),
    }
}

fn link(name: &str) -> SourceMediaLink {
    SourceMediaLink {
        absolute_path: PathBuf::from(format!("/videos/{name}")),
        relative_path: Some(PathBuf::from(format!("../videos/{name}"))),
        included_as: None,
    }
}

/// A project of a video, a picture file and a sound.
fn project_of_a_video_a_picture_and_a_sound() -> Project {
    let mut project = Project::empty();
    project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                link("holiday.mp4"),
                SourceVideoProperties {
                    frame_rate: FrameRate::new(30, 1).unwrap(),
                    total_frame_count: 90,
                    display_width_in_pixels: 640,
                    display_height_in_pixels: 360,
                    has_audio: false,
                },
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    project
        .import_source(
            SourceMedia::image(
                SourceMediaIdentifier::new(2),
                link("cat.png"),
                SourceImageProperties {
                    display_width_in_pixels: 100,
                    display_height_in_pixels: 50,
                },
            ),
            TimelineFrameIndex::new(90),
        )
        .unwrap();
    project
        .import_source(
            SourceMedia::audio(
                SourceMediaIdentifier::new(3),
                link("song.wav"),
                SourceAudioProperties {
                    duration_in_microseconds: 1_000_000,
                },
            ),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    project
}

#[test]
fn this_build_writes_a_version_that_has_the_colours_of_the_source_files() {
    // (The colours of the source files are the change that makes version 12; so are the three colour settings, which version 11 files do not have: an older program
    // that was given a file of version 12 says that the file is from a newer version, and does not fail on a field it has not heard of.)
    // (Rewritten on purpose in phase 4: the version is 13 now, which has all of version 12 and the shown-ness of the effects; what version 12 added is held by the tests below.)
    assert!(saved(&edited_project(link_to_missing_video())).contains(&version_line(FILE_FORMAT_VERSION)));
}

#[test]
fn a_file_of_version_11_loads_as_one_in_which_the_user_said_nothing_of_any_colour_and_the_settings_it_had_are_kept() {
    let project = project_from_json_text(A_FILE_OF_VERSION_11).expect("a file of version 11 loads");

    assert_eq!(project.sources().len(), 1);
    assert!(project.sources()[0].colour.is_automatic());
    let settings = project.settings();
    assert_eq!(settings.autosave, Some(false));
    assert_eq!(settings.sped_up_clip_sound, Some(SpedUpClipSound::IncreasePitch));
    assert_eq!(
        (
            settings.display_transform,
            settings.working_colour_space,
            settings.output_colour_space
        ),
        (None, None, None),
        "the colour settings follow the global ones, as the project had no word of them"
    );
    assert_eq!(project.tracks()[0].entries().len(), 3, "and the project is what it was");
}

#[test]
fn a_file_of_version_11_that_is_loaded_and_saved_is_the_same_file_with_the_current_version_line() {
    let project = project_from_json_text(A_FILE_OF_VERSION_11).unwrap();

    let again = project_to_json_text(&project, Path::new("/projects")).unwrap();

    assert_eq!(
        again,
        A_FILE_OF_VERSION_11.replace(&version_line(11), &version_line(FILE_FORMAT_VERSION)),
        "nothing is added to a file for a colour that nobody said"
    );
}

#[test]
fn what_the_user_says_of_the_colours_of_a_source_is_saved_with_that_source_and_comes_back() {
    let mut project = project_of_a_video_a_picture_and_a_sound();
    assert!(project.set_source_colour(SourceMediaIdentifier::new(1), everything_said()));
    assert!(project.set_source_colour(
        SourceMediaIdentifier::new(2),
        InputColourOverride {
            primaries: Some(StandardPrimaries::Rec2020),
            ..InputColourOverride::AUTOMATIC
        }
    ));

    let text = saved(&project);
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();

    assert_eq!(
        file["sources"][0]["colour"],
        serde_json::json!({"matrix": "bt2020", "range": "full", "primaries": "display_p3", "curve": "gamma22"})
    );
    assert_eq!(
        file["sources"][1]["colour"],
        serde_json::json!({"primaries": "rec2020"}),
        "only the parts that were said"
    );
    assert!(file["sources"][2].get("colour").is_none(), "a sound has no colours");
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(
        loaded.source_colour(SourceMediaIdentifier::new(1)),
        Some(everything_said())
    );
    assert_eq!(
        loaded.source_colour(SourceMediaIdentifier::new(2)),
        Some(InputColourOverride {
            primaries: Some(StandardPrimaries::Rec2020),
            ..InputColourOverride::AUTOMATIC
        })
    );
}

#[test]
fn a_source_that_the_user_said_nothing_of_has_no_colour_in_its_file() {
    let text = saved(&project_of_a_video_a_picture_and_a_sound());
    assert!(!text.contains("colour"), "{text}");
}

#[test]
fn every_way_of_saying_some_of_the_four_parts_survives_a_trip_through_the_file() {
    for bits in 0..16_u32 {
        let said = InputColourOverride {
            matrix: (bits & 1 != 0).then_some(VideoMatrix::Bt601),
            range: (bits & 2 != 0).then_some(VideoRange::Limited),
            primaries: (bits & 4 != 0).then_some(StandardPrimaries::AcesCg),
            curve: (bits & 8 != 0).then_some(TransferFunction::Gamma26),
        };
        let mut project = project_of_a_video_a_picture_and_a_sound();
        project.set_source_colour(SourceMediaIdentifier::new(1), said);
        let text = saved(&project);
        assert_eq!(
            text.contains("\"colour\""),
            !said.is_automatic(),
            "{said:?}: a colour that says nothing is not written\n{text}"
        );
        let loaded = project_from_json_text(&text).unwrap();
        assert_eq!(
            loaded.source_colour(SourceMediaIdentifier::new(1)),
            Some(said),
            "{said:?}"
        );
    }
}

#[test]
fn a_colour_that_names_something_the_program_has_not_got_is_refused_with_the_name_it_could_not_read() {
    let mut project = project_of_a_video_a_picture_and_a_sound();
    project.set_source_colour(SourceMediaIdentifier::new(1), everything_said());
    let text = saved(&project).replace("display_p3", "ntsc_1953");

    let reason = reason_for_unreadable_project(&text);

    assert!(reason.contains("ntsc_1953"), "{reason}");
}

#[test]
fn a_colour_that_names_a_part_that_does_not_exist_is_refused_rather_than_ignored() {
    let mut project = project_of_a_video_a_picture_and_a_sound();
    project.set_source_colour(SourceMediaIdentifier::new(1), everything_said());
    let text = saved(&project).replace("\"curve\"", "\"gamma\"");

    let reason = reason_for_unreadable_project(&text);

    assert!(reason.contains("gamma"), "{reason}");
}
