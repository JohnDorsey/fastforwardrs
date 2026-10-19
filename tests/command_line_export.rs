// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The export from the command line, with the real program (`fastforwardrs-export`), the real ffmpeg and the picture engine on a software graphics adapter
//! (`--adapter software`: nothing of it runs on a real graphics card in a test): what it writes to the error stream, its exit status, and the video it makes, for a
//! plain project file and for a project file with the video in it when the video is no longer where it was.

mod support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use fastforwardrs::media::MediaLocation;
use fastforwardrs::plugin_scan::{PluginSearchSettings, effect_of_a_plugin};
use fastforwardrs::plugin_trust::ApprovalStore;
use fastforwardrs::project::{
    AnimationTarget, EffectKind, PropertyId, PropertyValue, SourceMediaIdentifier, SourceMediaLink, TimelineFrameIndex,
    set_installed_plugins,
};
use fastforwardrs::project_file::archive::resource_entry_name;
use fastforwardrs::project_file::{HowToWrite, write_project_file, write_project_file_including};
use support::{
    TestVideoSpecification, decode_video, decoded_sound, expected_source_frames, loudness, project_for,
    scratch_directory, test_video,
};

/// The program, which Cargo builds for the tests of this package.
const PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-export");

/// A video of 3 seconds at 30 a second (90 pictures), 320 by 180.
const COUNTER_3_SECONDS: TestVideoSpecification = TestVideoSpecification {
    name: "counter_3_seconds",
    duration_in_seconds: 3,
    ..TestVideoSpecification::COUNTER
};

/// Runs the program with these arguments.
fn run(arguments: &[&Path]) -> Output {
    let mut command = Command::new(PROGRAM);
    command.args(arguments);
    command.output().expect("the program starts")
}

fn run_on_a_software_adapter(project: &Path, video: &Path) -> Output {
    let mut command = Command::new(PROGRAM);
    // (Settings from a file that is not there: the defaults, whatever the person who runs the test has set.)
    let no_settings = video.with_file_name("no_such_settings_file.json");
    command
        .arg("--adapter")
        .arg("software")
        .arg("--settings")
        .arg(no_settings)
        .arg(project)
        .arg(video);
    command.output().expect("the program starts")
}

fn said(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_plain_project_is_exported_with_its_progress_told_a_line_for_each_per_cent_and_a_status_of_0() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let project_file = scratch.path().join("trip.fastforwardrs.json");
    write_project_file(&project, &project_file).unwrap();
    let destination = scratch.path().join("trip.mp4");

    let _one_export_at_a_time = support::one_export_at_a_time();
    let output = run_on_a_software_adapter(&project_file, &destination);

    let words = said(&output);
    assert_eq!(output.status.code(), Some(0), "{words}");
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected_source_frames(&project)
    );
    let lines: Vec<&str> = words.lines().collect();
    assert!(
        lines
            .last()
            .is_some_and(|line| line.starts_with("The video was saved to ") && line.contains("trip.mp4")),
        "{words}"
    );
    let per_cents: Vec<u64> = lines
        .iter()
        .filter_map(|line| line.split('%').next()?.parse().ok())
        .collect();
    assert!(
        per_cents.windows(2).all(|pair| pair[0] < pair[1]),
        "each per cent once, going up: {per_cents:?}"
    );
    assert_eq!(per_cents.last(), Some(&100), "{words}");
    assert!(words.contains("(90 of 90 frames)"), "{words}");
    assert!(
        !words.contains("warning"),
        "nothing was held, so nothing is warned of: {words}"
    );
}

#[test]
fn an_export_that_held_pictures_says_so_in_a_warning_before_it_says_that_the_video_was_saved_and_the_status_is_still_0()
{
    let toolchain = toolchain_and_graphics_or_skip!();
    let short = test_video(&toolchain, &COUNTER_3_SECONDS);
    // The project was made when the file had 300 pictures; it has 90 now.
    let project =
        support::project_for_a_video_that_has_fewer_pictures_than_the_project_was_told(&toolchain, &short, 300);
    let scratch = scratch_directory();
    let project_file = scratch.path().join("trip.fastforwardrs.json");
    write_project_file(&project, &project_file).unwrap();
    let destination = scratch.path().join("trip.mp4");

    let _one_export_at_a_time = support::one_export_at_a_time();
    let output = run_on_a_software_adapter(&project_file, &destination);

    let words = said(&output);
    assert_eq!(output.status.code(), Some(0), "{words}");
    let lines: Vec<&str> = words.lines().collect();
    let warning = lines
        .iter()
        .position(|line| line.starts_with("warning: ") && line.contains("210 pictures"))
        .unwrap_or_else(|| panic!("the held pictures are told, as a warning: {words}"));
    let saved = lines
        .iter()
        .position(|line| line.starts_with("The video was saved to "))
        .unwrap_or_else(|| panic!("{words}"));
    assert!(
        warning < saved,
        "the warning comes before the line that says it is saved: {words}"
    );
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers().len(),
        300,
        "and the video is there, all of it"
    );
}

#[test]
fn an_export_that_fails_after_it_has_started_says_why_and_the_status_is_1() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let project_file = scratch.path().join("trip.fastforwardrs.json");
    write_project_file(&project, &project_file).unwrap();
    // The folder that the video is to be in is not there: ffmpeg is started, and cannot make the file.
    let destination = scratch.path().join("no_such_folder").join("trip.mp4");

    let _one_export_at_a_time = support::one_export_at_a_time();
    let output = run_on_a_software_adapter(&project_file, &destination);

    let words = said(&output);
    assert_eq!(output.status.code(), Some(1), "{words}");
    let reason = words
        .split_once("The export failed: ")
        .map(|(_, reason)| reason.trim())
        .unwrap_or_else(|| panic!("the failure is told: {words}"));
    assert!(!reason.is_empty(), "the reason is told: {words}");
    assert!(!reason.contains("pipe"), "ffmpeg's words are the reason: {words}");
    assert!(!destination.exists());
}

#[test]
fn a_project_file_with_the_video_in_it_is_exported_when_the_video_is_not_where_it_was() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let scratch = scratch_directory();
    let footage = scratch.path().join("footage");
    std::fs::create_dir_all(&footage).unwrap();
    let original = footage.join("holiday.mp4");
    std::fs::copy(&video.path, &original).unwrap();
    // The project, made from the original, whose link says that the video is in the project file.
    let mut project = project_for(
        &toolchain,
        &support::TestVideo {
            path: original.clone(),
            specification: video.specification.clone(),
        },
    );
    let identifier = SourceMediaIdentifier::new(1);
    let properties = project.source(identifier).unwrap().properties;
    let entry_name = resource_entry_name(identifier, "holiday.mp4");
    project
        .relink_source(
            identifier,
            SourceMediaLink {
                absolute_path: original.clone(),
                relative_path: None,
                included_as: Some(entry_name),
            },
            properties,
        )
        .unwrap();
    let archive = scratch.path().join("trip.ffrs");
    let files = HashMap::from([(identifier, MediaLocation::of_a_file(&original))]);
    write_project_file_including(&project, &archive, &files, HowToWrite::Anew).unwrap();
    // Gone, as on another computer.
    std::fs::remove_file(&original).unwrap();
    std::fs::remove_dir(&footage).unwrap();
    let destination = scratch.path().join("trip.mp4");

    let _one_export_at_a_time = support::one_export_at_a_time();
    let output = run_on_a_software_adapter(&archive, &destination);

    let words = said(&output);
    assert_eq!(output.status.code(), Some(0), "{words}");
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected_source_frames(&project)
    );
}

#[test]
fn a_project_whose_video_is_not_to_be_found_fails_with_a_status_of_1_names_the_file_and_writes_nothing() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let scratch = scratch_directory();
    let copy = scratch.path().join("gone.mp4");
    std::fs::copy(&video.path, &copy).unwrap();
    let project = project_for(
        &toolchain,
        &support::TestVideo {
            path: copy.clone(),
            specification: video.specification.clone(),
        },
    );
    std::fs::remove_file(&copy).unwrap();
    let project_file = scratch.path().join("trip.fastforwardrs.json");
    write_project_file(&project, &project_file).unwrap();
    let destination = scratch.path().join("trip.mp4");

    let output = run_on_a_software_adapter(&project_file, &destination);

    let words = said(&output);
    assert_eq!(output.status.code(), Some(1), "{words}");
    assert!(words.contains("The export failed"), "{words}");
    assert!(words.contains("gone.mp4"), "{words}");
    assert!(!destination.exists());
    let names: Vec<String> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["trip.fastforwardrs.json".to_owned()],
        "nothing else is left"
    );
}

#[test]
fn a_command_line_that_is_not_understood_gets_the_usage_and_a_status_of_2() {
    let output = run(&[Path::new("only_one_file.json")]);
    let words = said(&output);
    assert_eq!(output.status.code(), Some(2), "{words}");
    assert!(words.contains("Usage: fastforwardrs-export"), "{words}");
    let output = run(&[Path::new("--help")]);
    assert_eq!(output.status.code(), Some(0));
    assert!(said(&output).contains("--adapter software"));
}

#[test]
fn the_video_cannot_overwrite_a_file_the_project_is_made_from() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let scratch = scratch_directory();
    let copy: PathBuf = scratch.path().join("holiday.mp4");
    std::fs::copy(&video.path, &copy).unwrap();
    let project = project_for(
        &toolchain,
        &support::TestVideo {
            path: copy.clone(),
            specification: video.specification.clone(),
        },
    );
    let project_file = scratch.path().join("trip.fastforwardrs.json");
    write_project_file(&project, &project_file).unwrap();
    let original_bytes = std::fs::read(&copy).unwrap();

    let output = run_on_a_software_adapter(&project_file, &copy);

    let words = said(&output);
    assert_eq!(output.status.code(), Some(1), "{words}");
    assert!(words.contains("overwrite"), "{words}");
    assert_eq!(std::fs::read(&copy).unwrap(), original_bytes, "the video is as it was");
}

// --- plugins on the clips ---

/// Runs the program on a software adapter with the settings (and so the folders for plugins and the scans) beside `settings_file`, and `more_arguments` besides.
fn run_with_plugin_folders(project: &Path, video: &Path, settings_file: &Path, more_arguments: &[&Path]) -> Output {
    let mut command = Command::new(PROGRAM);
    command
        .arg("--adapter")
        .arg("software")
        .arg("--settings")
        .arg(settings_file)
        .args(more_arguments)
        .arg(project)
        .arg(video);
    command.output().expect("the program starts")
}

#[test]
fn a_project_with_a_plugin_on_a_clip_is_refused_until_its_folder_is_approved_and_then_the_sound_of_the_video_has_the_plugin_in_it()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let video = test_video(&toolchain, &COUNTER_3_SECONDS);
    let scratch = scratch_directory();
    let _one_export_at_a_time = support::one_export_at_a_time();

    // The plugins of the test library are in a folder of their own, which the file beside the settings says to look in.
    let plugins = scratch.path().join("plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    let library = plugins.join(format!("good.{}", fastforwardrs::plugin_scan::library_file_extension()));
    std::fs::copy(ladspa_test_plugins::library_file_for_tests(), &library).unwrap();
    let settings_file = scratch.path().join("settings.json");
    PluginSearchSettings {
        folders: vec![plugins.clone()],
        ..PluginSearchSettings::default()
    }
    .write(&settings_file.with_file_name("plugin_search.json"))
    .unwrap();

    // A project with the gain plugin at 0.5 on its clip, made here and written to its file (the plugin is installed in this program for that).
    let scan = ladspa_host::scan_library(&library, |_| {}).unwrap();
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_a_plugin(&library, plugin).ok())
            .collect(),
    );
    let kind = EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key().ends_with(":ffrs_test_gain"))
        .expect("the gain plugin");
    let plain = project_for(&toolchain, &video);
    let mut project = plain.clone();
    let clip = support::clip_number(&project, 0);
    let effect = project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    project
        .set_property_at(
            AnimationTarget::Clip(clip),
            PropertyId::effect_value(kind, effect, 0),
            PropertyValue::Number(0.5),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    set_installed_plugins(Vec::new());
    let project_file = scratch.path().join("with_a_plugin.fastforwardrs.json");
    write_project_file(&project, &project_file).unwrap();
    let plain_file = scratch.path().join("plain.fastforwardrs.json");
    write_project_file(&plain, &plain_file).unwrap();
    let approvals = scratch.path().join("plugin_approvals.json");

    // Nobody has approved the folder: refused, and nothing is exported.
    let destination = scratch.path().join("with_a_plugin.mp4");
    let refused = run_with_plugin_folders(
        &project_file,
        &destination,
        &settings_file,
        &[Path::new("--plugin-approvals"), &approvals],
    );
    let words = said(&refused);
    assert_eq!(refused.status.code(), Some(1), "{words}");
    assert!(
        words.contains("a command line cannot ask") && words.contains("has not been approved"),
        "{words}"
    );
    assert!(words.contains("test plugin ffrs_test_gain"), "{words}");
    assert!(!destination.exists());

    // The folder is approved (as the editor does it, in the file): the plugin is found, run, and heard.
    ApprovalStore::at(approvals.clone()).approve_folder(&plugins);
    let allowed = run_with_plugin_folders(
        &project_file,
        &destination,
        &settings_file,
        &[Path::new("--plugin-approvals"), &approvals],
    );
    let words = said(&allowed);
    assert_eq!(allowed.status.code(), Some(0), "{words}");
    assert!(!words.contains("warning"), "nothing was left out: {words}");
    let plain_destination = scratch.path().join("plain.mp4");
    let unchanged = run_with_plugin_folders(&plain_file, &plain_destination, &settings_file, &[]);
    assert_eq!(unchanged.status.code(), Some(0), "{}", said(&unchanged));

    let with = loudness(&decoded_sound(&toolchain, &destination, 2, 48_000));
    let without = loudness(&decoded_sound(&toolchain, &plain_destination, 2, 48_000));
    assert!(without > 0.02, "the test video has a tone to be changed: {without}");
    let ratio = with / without;
    assert!(
        (ratio - 0.5).abs() < 0.03,
        "the plugin halves the sound: {with} against {without} is {ratio}"
    );
}
