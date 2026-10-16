// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the command line understands and says. (The export running from it, with the real program: `tests/command_line_export.rs`.)

use super::*;
use crate::project::{Project, SourceMediaLink};
use crate::video_export::test_projects::{frame, project_with_video, video_source};

fn arguments(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn export_of(list: &[&str]) -> ExportRequest {
    match parse_arguments(arguments(list)).unwrap() {
        CommandLine::Export(request) => request,
        other => panic!("expected an export, got {other:?}"),
    }
}

// --- the arguments ---

#[test]
fn the_two_files_are_the_project_and_the_video_and_the_best_adapter_is_used_unless_another_is_asked_for() {
    let request = export_of(&["trip.json", "out.mp4"]);
    assert_eq!(request.project_file, PathBuf::from("trip.json"));
    assert_eq!(request.video_file, PathBuf::from("out.mp4"));
    assert_eq!(request.adapter, AdapterKind::Best);
    assert_eq!(request.archive_cache_folder, None, "the usual place");
    assert_eq!(request.global_settings_file, default_settings_file_path());
}

#[test]
fn the_adapter_can_be_chosen_before_between_or_after_the_files() {
    for list in [
        ["--adapter", "software", "trip.json", "out.mp4"],
        ["trip.json", "--adapter", "software", "out.mp4"],
        ["trip.json", "out.mp4", "--adapter", "software"],
    ] {
        let request = export_of(&list);
        assert_eq!(request.adapter, AdapterKind::Software, "{list:?}");
        assert_eq!(request.project_file, PathBuf::from("trip.json"), "{list:?}");
        assert_eq!(request.video_file, PathBuf::from("out.mp4"), "{list:?}");
    }
    assert_eq!(
        export_of(&["--adapter", "best", "trip.json", "out.mp4"]).adapter,
        AdapterKind::Best
    );
}

#[test]
fn the_settings_can_be_read_from_another_file_than_the_editors() {
    let request = export_of(&["--settings", "mine.json", "trip.json", "out.mp4"]);
    assert_eq!(request.global_settings_file, Some(PathBuf::from("mine.json")));
    assert_eq!(
        request.project_file,
        PathBuf::from("trip.json"),
        "the file after it is not taken for a project"
    );
    let problem = parse_arguments(arguments(&["trip.json", "out.mp4", "--settings"])).unwrap_err();
    assert!(problem.contains("--settings has to be followed"), "{problem}");
}

#[test]
fn asking_for_help_is_understood_wherever_it_is() {
    for list in [
        &["--help"][..],
        &["-h"],
        &["trip.json", "--help"],
        &["--help", "--adapter", "nonsense"],
    ] {
        assert_eq!(parse_arguments(arguments(list)), Ok(CommandLine::Help), "{list:?}");
    }
}

#[test]
fn what_is_not_understood_is_said() {
    for (list, words) in [
        (&[][..], "both needed"),
        (&["trip.json"], "both needed"),
        (&["a", "b", "c"], "only the project file and the video file"),
        (&["--adapter"], "has to be followed"),
        (&["--adapter", "fast", "a", "b"], "best or software, and not fast"),
        (&["--speed", "a", "b"], "--speed is not an option"),
    ] {
        let problem = parse_arguments(arguments(list)).unwrap_err();
        assert!(problem.contains(words), "{list:?}: {problem}");
    }
}

#[test]
fn a_command_line_that_is_not_understood_gets_the_usage_and_the_status_2_and_help_gets_the_usage_and_0() {
    let mut said = Vec::new();
    assert_eq!(run(arguments(&["only_one_file"]), &mut said), 2);
    let said = String::from_utf8(said).unwrap();
    assert!(said.contains("both needed"), "{said}");
    assert!(said.contains("Usage: fastforwardrs-export"), "{said}");

    let mut said = Vec::new();
    assert_eq!(run(arguments(&["--help"]), &mut said), 0);
    let said = String::from_utf8(said).unwrap();
    assert!(
        said.contains("--adapter best") && said.contains("--adapter software"),
        "{said}"
    );
    assert!(said.contains("The exit status is 0"), "{said}");
}

#[test]
fn a_project_that_cannot_be_read_fails_with_status_1_and_says_why() {
    let folder = tempfile::tempdir().unwrap();
    let mut said = Vec::new();
    let status = run(
        arguments(&[
            "--adapter",
            "software",
            &folder.path().join("missing.json").to_string_lossy(),
            &folder.path().join("out.mp4").to_string_lossy(),
        ]),
        &mut said,
    );
    let said = String::from_utf8(said).unwrap();
    // (Or ffmpeg is not found, which is said first: either way the export failed and says so.)
    assert_eq!(status, 1, "{said}");
    assert!(said.starts_with("The export failed: "), "{said}");
    assert!(!folder.path().join("out.mp4").exists());
}

// --- the progress ---

#[test]
fn a_line_is_told_for_each_per_cent_reached_and_none_is_told_twice() {
    let mut progress = ProgressTold::default();
    assert_eq!(progress.line_for(0, 200).as_deref(), Some("0% (0 of 200 frames)"));
    assert_eq!(progress.line_for(1, 200), None, "still 0%");
    assert_eq!(progress.line_for(2, 200).as_deref(), Some("1% (2 of 200 frames)"));
    assert_eq!(progress.line_for(3, 200), None);
    assert_eq!(progress.line_for(100, 200).as_deref(), Some("50% (100 of 200 frames)"));
    assert_eq!(progress.line_for(100, 200), None, "the same again");
    assert_eq!(progress.line_for(200, 200).as_deref(), Some("100% (200 of 200 frames)"));
    assert_eq!(progress.line_for(200, 200), None);
}

#[test]
fn a_video_of_no_frames_is_done_and_more_than_all_is_not_more_than_a_hundred() {
    let mut progress = ProgressTold::default();
    assert_eq!(progress.line_for(0, 0).as_deref(), Some("100% (0 of 0 frames)"));
    let mut progress = ProgressTold::default();
    assert_eq!(progress.line_for(250, 200).as_deref(), Some("100% (250 of 200 frames)"));
}

// --- finding the files ---

fn plan_of(project: &Project) -> ExportPlan {
    plan_export(project, Settings::DEFAULT).unwrap()
}

#[test]
fn a_file_is_looked_for_where_the_project_says_and_then_next_to_the_project_file() {
    let folder = tempfile::tempdir().unwrap();
    let project_file = folder.path().join("trip.json");
    let in_the_right_place = folder.path().join("holiday.mp4");
    std::fs::write(&in_the_right_place, b"x").unwrap();
    let moved_with_the_project = folder.path().join("moved.mp4");
    std::fs::write(&moved_with_the_project, b"x").unwrap();

    let mut project = Project::empty();
    // Source 1: where it was; source 2: not where it was, but next to the project file, which the relative path says.
    project
        .import_source(video_source(1, 320, 180, 100, false), frame(0))
        .unwrap();
    project
        .import_source(video_source(2, 320, 180, 100, false), frame(120))
        .unwrap();
    project
        .relink_source(
            crate::project::SourceMediaIdentifier::new(1),
            SourceMediaLink::from_absolute_path(in_the_right_place.clone()),
            video_source(1, 320, 180, 100, false).properties,
        )
        .unwrap();
    project
        .relink_source(
            crate::project::SourceMediaIdentifier::new(2),
            SourceMediaLink {
                absolute_path: PathBuf::from("/nowhere/at/all/moved.mp4"),
                relative_path: Some(PathBuf::from("moved.mp4")),
                included_as: None,
            },
            video_source(2, 320, 180, 100, false).properties,
        )
        .unwrap();
    let plan = plan_of(&project);
    assert_eq!(plan.source_files.len(), 2);

    let locations = locate_the_sources(&project, &plan, &project_file, None).unwrap();

    assert_eq!(
        locations,
        vec![
            MediaLocation::of_a_file(in_the_right_place),
            MediaLocation::of_a_file(folder.path().join("moved.mp4"))
        ]
    );
}

#[test]
fn every_file_that_cannot_be_found_is_named_not_only_the_first() {
    let folder = tempfile::tempdir().unwrap();
    let mut project = project_with_video(320, 180, false);
    project
        .import_source(video_source(2, 320, 180, 100, false), frame(320))
        .unwrap();
    let plan = plan_of(&project);
    assert_eq!(plan.source_files.len(), 2);
    let problem = locate_the_sources(&project, &plan, &folder.path().join("trip.json"), None).unwrap_err();
    assert!(problem.contains("1.mp4") && problem.contains("2.mp4"), "{problem}");
    assert!(problem.contains("cannot be used"), "{problem}");
}

#[test]
fn a_file_in_a_project_file_that_is_not_an_archive_cannot_be_found_and_the_reason_is_not_a_panic() {
    let folder = tempfile::tempdir().unwrap();
    let mut project = project_with_video(320, 180, false);
    project
        .relink_source(
            crate::project::SourceMediaIdentifier::new(1),
            SourceMediaLink {
                absolute_path: PathBuf::from("/videos/1.mp4"),
                relative_path: None,
                included_as: Some("resources/1/holiday.mp4".to_owned()),
            },
            video_source(1, 320, 180, 300, false).properties,
        )
        .unwrap();
    let not_an_archive = folder.path().join("trip.ffrs");
    std::fs::write(&not_an_archive, b"this is not a zip file").unwrap();
    let plan = plan_of(&project);
    let problem = locate_the_sources(&project, &plan, &not_an_archive, None).unwrap_err();
    assert!(problem.contains("1.mp4"), "{problem}");
}
