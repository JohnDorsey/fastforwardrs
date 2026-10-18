// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the command line understands and says. (The export running from it, with the real program: `tests/command_line_export.rs`.)

use super::*;
use crate::plugin_trust::{ApprovalStore, this_platform};
use crate::project::{Project, Settings, SourceMediaLink};
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
fn anything_that_begins_with_a_dash_is_an_option_and_not_a_file_and_a_file_that_begins_with_one_can_be_given_with_its_folder()
 {
    for option in ["-x", "-adapter", "-", "---", "-1"] {
        let problem = parse_arguments(arguments(&[option, "a", "b"])).unwrap_err();
        assert!(
            problem.contains(&format!("{option} is not an option")),
            "{option}: {problem}"
        );
    }
    let request = export_of(&["./-trip.json", "./-out.mp4"]);
    assert_eq!(request.project_file, PathBuf::from("./-trip.json"));
    assert_eq!(request.video_file, PathBuf::from("./-out.mp4"));
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

// --- what the export is made with, and what it warns of ---

#[test]
fn the_adapter_that_is_asked_for_is_the_one_the_export_is_made_on() {
    for (asked_for, said) in [
        ("best", "the best graphics adapter the computer has"),
        ("software", "a software adapter"),
    ] {
        let request = export_of(&["--adapter", asked_for, "trip.json", "out.mp4"]);
        assert_eq!(
            format!("{:?}", export_options_of(&request).adapter),
            said,
            "{asked_for}"
        );
    }
}

#[test]
fn the_export_is_made_with_the_settings_of_the_file_that_was_named_and_the_projects_own_on_top_of_them() {
    use crate::project::{ProjectSettings, SpedUpClipSound};
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("settings.json");
    crate::settings_file::write_settings_file(
        Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::Mute,
            ..Settings::DEFAULT
        },
        &file,
    )
    .unwrap();
    let mut project = Project::empty();
    let request = |file: Option<PathBuf>| ExportRequest {
        global_settings_file: file,
        ..export_of(&["trip.json", "out.mp4"])
    };

    assert_eq!(
        settings_of_the_export(&request(Some(file.clone())), &project),
        Ok(Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::Mute,
            ..Settings::DEFAULT
        }),
        "the file that was named"
    );
    assert_eq!(
        settings_of_the_export(&request(None), &project),
        Ok(Settings::DEFAULT),
        "none was named"
    );
    assert_eq!(
        settings_of_the_export(&request(Some(folder.path().join("never_made.json"))), &project),
        Ok(Settings::DEFAULT),
        "a file that is not there: the settings were never changed"
    );
    project.set_settings(ProjectSettings {
        autosave: None,
        sped_up_clip_sound: Some(SpedUpClipSound::IncreasePitch),
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    });
    assert_eq!(
        settings_of_the_export(&request(Some(file.clone())), &project),
        Ok(Settings {
            autosave: true,
            sped_up_clip_sound: SpedUpClipSound::IncreasePitch,
            ..Settings::DEFAULT
        }),
        "what the project says of itself is on top of the file's, and the rest is the file's"
    );
    std::fs::write(&file, b"this is not settings").unwrap();
    assert!(
        settings_of_the_export(&request(Some(file)), &project).is_err(),
        "a file that is not a settings file is a reason to stop and not a reason to use the defaults"
    );
}

/// A project of one video whose clip and track have effects that the program does not have (made as a project file brings them), and one that it has.
fn project_with_effects_the_program_does_not_have() -> Project {
    use crate::project::{AnimationTarget, EffectKind};
    use crate::test_support::a_kind_of_effect_the_program_does_not_have;
    let mut project = project_with_video(320, 180, false);
    let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let track = AnimationTarget::Track(project.tracks()[0].id());
    let made_up = a_kind_of_effect_the_program_does_not_have("isf:a_made_up_effect", &[]);
    project.add_effect(clip, made_up).unwrap();
    project.add_effect(track, made_up).unwrap();
    project.add_effect(clip, EffectKind::HUE_SHIFT).unwrap();
    project
}

#[test]
fn an_effect_that_the_program_does_not_have_is_warned_of_by_its_name_once_and_one_that_it_has_is_not() {
    assert_eq!(
        warnings_about(&project_with_effects_the_program_does_not_have()),
        [
            "the effect a made up effect (isf:a_made_up_effect, version 1.0) is not in this program, so the video does not have what it does"
        ]
    );
    assert!(warnings_about(&project_with_video(320, 180, false)).is_empty());
}

#[test]
fn the_warnings_are_given_before_anything_else_is_done_when_the_project_is_exported() {
    if FfmpegToolchain::locate().is_err() {
        eprintln!("SKIPPED: there is no ffmpeg");
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let project_file = folder.path().join("trip.json");
    crate::project_file::write_project_file(&project_with_effects_the_program_does_not_have(), &project_file).unwrap();
    let request = ExportRequest {
        archive_cache_folder: Some(folder.path().join("cache")),
        // (A software adapter, though the export is stopped before an adapter is made: no test may use a real graphics card.)
        ..export_of(&[
            "--adapter",
            "software",
            &project_file.to_string_lossy(),
            &folder.path().join("out.mp4").to_string_lossy(),
        ])
    };
    let mut reports = Vec::new();

    // (The video is not where the project says, so the export is stopped after the warnings, and no graphics adapter is made.)
    let result = export_project(&request, &mut |report| reports.push(report));

    assert!(result.is_err(), "{result:?}");
    assert_eq!(
        reports,
        [Report::Warning(
            "the effect a made up effect (isf:a_made_up_effect, version 1.0) is not in this program, so the video does not have what it does"
                .to_owned()
        )]
    );
}

#[test]
fn a_warning_is_said_on_the_error_stream_as_a_line_that_begins_with_warning_before_the_failure_that_follows_it() {
    if FfmpegToolchain::locate().is_err() {
        eprintln!("SKIPPED: there is no ffmpeg");
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let project_file = folder.path().join("trip.json");
    crate::project_file::write_project_file(&project_with_effects_the_program_does_not_have(), &project_file).unwrap();
    let mut said = Vec::new();

    // (The video is not where the project says, so the export is stopped before any adapter is made.)
    let status = run(
        arguments(&[
            "--adapter",
            "software",
            &project_file.to_string_lossy(),
            &folder.path().join("out.mp4").to_string_lossy(),
        ]),
        &mut said,
    );

    let said = String::from_utf8(said).unwrap();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(status, 1, "{said}");
    assert_eq!(
        lines[0],
        "warning: the effect a made up effect (isf:a_made_up_effect, version 1.0) is not in this program, so the video does not have what it does",
        "{said}"
    );
    assert!(lines[1].starts_with("The export failed: "), "{said}");
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

// --- plugins: a command line cannot ask ---

#[test]
fn plugins_are_not_allowed_unless_the_command_line_says_so_and_the_approvals_are_those_of_the_editor_unless_it_names_a_file()
 {
    let plain = export_of(&["trip.json", "out.mp4"]);
    assert!(!plain.allow_plugins);
    assert_eq!(plain.plugin_approvals_file, ApprovalStore::default_file());

    let allowed = export_of(&["--allow-plugins", "trip.json", "out.mp4"]);
    assert!(allowed.allow_plugins);
    let elsewhere = export_of(&["trip.json", "--plugin-approvals", "my approvals.json", "out.mp4"]);
    assert_eq!(
        elsewhere.plugin_approvals_file,
        Some(PathBuf::from("my approvals.json"))
    );
    assert!(!elsewhere.allow_plugins);

    let reason = parse_arguments(arguments(&["trip.json", "out.mp4", "--plugin-approvals"])).unwrap_err();
    assert!(
        reason.contains("--plugin-approvals has to be followed by the name of a file"),
        "{reason}"
    );
    assert!(USAGE.contains("--allow-plugins") && USAGE.contains("--plugin-approvals"));
}

fn embedded_library(name: &str) -> crate::project::PluginLibrary {
    crate::project::PluginLibrary::new(
        name,
        crate::project::PluginLibraryKind::Ladspa,
        crate::project::PluginLibrarySource::Embedded(vec![
            crate::project::EmbeddedBuild::new(&this_platform(), Arc::from(b"the library".as_slice())).unwrap(),
        ]),
    )
    .unwrap()
}

fn linked_library(name: &str, path: &Path) -> crate::project::PluginLibrary {
    crate::project::PluginLibrary::new(
        name,
        crate::project::PluginLibraryKind::Ladspa,
        crate::project::PluginLibrarySource::Linked(crate::project::LinkedLibrary {
            absolute_path: path.to_owned(),
            relative_path: None,
        }),
    )
    .unwrap()
}

/// A project file in `folder` for a project that has `libraries`, and a request to export it with the approvals in `approvals`.
fn request_for_a_project_with(
    folder: &Path,
    libraries: Vec<crate::project::PluginLibrary>,
    approvals: Option<PathBuf>,
    allow: bool,
) -> ExportRequest {
    let mut project = project_with_video(320, 180, false);
    for library in libraries {
        project.add_plugin_library(library).unwrap();
    }
    let project_file = folder.join("trip.ffrs");
    crate::project_file::write_project_file(&project, &project_file).unwrap();
    ExportRequest {
        archive_cache_folder: Some(folder.join("cache")),
        allow_plugins: allow,
        plugin_approvals_file: approvals,
        ..export_of(&[
            "--adapter",
            "software",
            &project_file.to_string_lossy(),
            &folder.join("out.mp4").to_string_lossy(),
        ])
    }
}

#[test]
fn a_project_with_a_plugin_inside_it_is_refused_with_the_plugin_listed_and_nothing_is_started() {
    let folder = tempfile::tempdir().unwrap();
    let request = request_for_a_project_with(folder.path(), vec![embedded_library("amp_1181.dll")], None, false);
    let mut reports = Vec::new();

    let result = export_project(&request, &mut |report| reports.push(report));

    let reason = result.unwrap_err();
    assert!(reason.contains("a command line cannot ask"), "{reason}");
    assert!(reason.contains("  - amp_1181.dll (inside the project file"), "{reason}");
    assert!(reason.contains("--allow-plugins"), "{reason}");
    assert!(reports.is_empty(), "nothing was said to have begun");
    assert!(!folder.path().join("out.mp4").exists());
}

#[test]
fn a_plugin_file_that_has_not_been_approved_is_refused_and_one_that_has_is_not() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("tap_reverb.so");
    std::fs::write(&file, "bytes").unwrap();
    let approvals = folder.path().join("plugin_approvals.json");
    let request = request_for_a_project_with(
        folder.path(),
        vec![linked_library("tap_reverb.so", &file)],
        Some(approvals.clone()),
        false,
    );

    let refused = export_project(&request, &mut |_| {}).unwrap_err();
    assert!(
        refused.contains("tap_reverb.so") && refused.contains("has not been approved"),
        "{refused}"
    );

    // Approved in the editor (the same file, by path and contents): the command line no longer refuses it. (The export then goes on to look for ffmpeg and the video, which
    // may not be there: what is checked is that it is not the plugin that stops it.)
    ApprovalStore::at(approvals).approve(&file, crate::digest::Sha256Digest::of_bytes(b"bytes"));
    let result = export_project(&request, &mut |_| {});
    if let Err(reason) = result {
        assert!(!reason.contains("a command line cannot ask"), "{reason}");
    }
}

#[test]
fn with_allow_plugins_the_plugins_do_not_stop_the_export() {
    let folder = tempfile::tempdir().unwrap();
    let request = request_for_a_project_with(folder.path(), vec![embedded_library("amp_1181.dll")], None, true);

    let result = export_project(&request, &mut |_| {});

    if let Err(reason) = result {
        assert!(!reason.contains("a command line cannot ask"), "{reason}");
    }
}

#[test]
fn a_project_without_plugins_is_not_refused_for_them() {
    let folder = tempfile::tempdir().unwrap();
    let request = request_for_a_project_with(folder.path(), Vec::new(), None, false);

    let result = export_project(&request, &mut |_| {});

    if let Err(reason) = result {
        assert!(!reason.contains("a command line cannot ask"), "{reason}");
    }
}

#[test]
fn a_refusal_is_told_on_the_error_stream_and_the_status_says_the_export_failed() {
    let folder = tempfile::tempdir().unwrap();
    let request = request_for_a_project_with(folder.path(), vec![embedded_library("amp_1181.dll")], None, false);
    let mut said = Vec::new();

    let status = run(
        arguments(&[
            "--adapter",
            "software",
            &request.project_file.to_string_lossy(),
            &request.video_file.to_string_lossy(),
        ]),
        &mut said,
    );

    assert_eq!(status, 1);
    let text = String::from_utf8_lossy(&said);
    assert!(text.contains("amp_1181.dll"), "{text}");
}
