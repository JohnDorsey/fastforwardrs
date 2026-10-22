// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound plugins in the real editor, with the real scan program and the plugin library written for the tests (phase 7, piece 7.5): a person says where the plugins are, the scan
//! finds them in the background, a plugin is put on a clip from the menu, the project is saved, and it is loaded where the plugin is not (the effect is kept whole and says it
//! is not installed) and where it is (it works again).
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod journey;
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use fastforwardrs::project::{AnimationTarget, EffectKind, PropertyValue, set_installed_plugins};
use journey::Journey;
use ladspa_test_plugins::library_file_for_tests;
use support::{counter_video, scratch_directory};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

/// A folder with a copy of the library of the test plugins in it.
fn folder_with_the_test_plugins(parent: &Path) -> PathBuf {
    let folder = parent.join("plugins");
    fs::create_dir_all(&folder).unwrap();
    fs::copy(
        library_file_for_tests(),
        folder.join(format!("good.{}", fastforwardrs::plugin_scan::library_file_extension())),
    )
    .unwrap();
    folder
}

fn look_for_plugins_in(journey: &mut Journey, folder: &Path) {
    journey
        .application_mut()
        .use_this_plugin_scanner(PathBuf::from(SCANNER), None, false);
    journey.application_mut().add_plugin_folder(folder.to_owned());
    assert!(
        journey.application().is_scanning_plugins(),
        "the scan is going on in the background"
    );
    let began = Instant::now();
    while journey.application().is_scanning_plugins() {
        assert!(began.elapsed() < Duration::from_secs(60), "the scan did not end");
        journey.keep_drawing_for(Duration::from_millis(50));
    }
}

fn effects_of_the_first_clip(journey: &Journey) -> Vec<EffectKind> {
    let clip = journey.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    journey
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect()
}

#[test]
fn plugins_found_in_the_background_are_offered_put_on_a_clip_saved_and_kept_whole_where_they_are_not_installed() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let plugins = folder_with_the_test_plugins(scratch.path());

    let mut journey = Journey::importing(&toolchain, &video.path);
    look_for_plugins_in(&mut journey, &plugins);
    assert!(
        journey.application().plugin_scan_problems().is_empty(),
        "{:?}",
        journey.application().plugin_scan_problems()
    );
    assert_eq!(journey.application().plugin_libraries_found().len(), 1);
    assert_eq!(
        EffectKind::installed_plugin_kinds().len(),
        7,
        "seven of the nine test plugins can be effects"
    );

    // Put the gain on the clip, from the menu of the plus button.
    journey.select_the_clip_at(0);
    let plus = journey.rectangle_of("Add an effect").center();
    journey.click(plus);
    journey.settle();
    journey.open_the_smaller_menu("LADSPA");
    journey.harness.get_by_label("Test plugin ffrs_test_gain").click();
    journey.settle();
    let kinds = effects_of_the_first_clip(&journey);
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].key(), "ladspa:good:ffrs_test_gain");
    assert!(kinds[0].is_installed());
    let file = scratch.path().join("with_a_plugin.ffrs");
    assert!(journey.application_mut().save_project_to(file.clone()));
    assert!(fs::metadata(&file).is_ok(), "the project file was written");

    // Another editor, where the plugin is not installed: the effect is there, whole, and does nothing.
    set_installed_plugins(Vec::new());
    let mut elsewhere = Journey::start(&toolchain);
    elsewhere.application_mut().load_project_from(file.clone());
    elsewhere.wait_until_idle();
    let kinds_there = effects_of_the_first_clip(&elsewhere);
    assert_eq!(kinds_there.len(), 1);
    assert_eq!(kinds_there[0].key(), "ladspa:good:ffrs_test_gain");
    assert!(!kinds_there[0].is_installed(), "the plugin is not installed here");
    let clip = elsewhere.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    let effect_there = elsewhere
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .values()
        .to_vec();
    assert_eq!(effect_there, [PropertyValue::Number(1.0)], "its values are kept");
    let resaved = scratch.path().join("saved_again.ffrs");
    assert!(elsewhere.application_mut().save_project_to(resaved.clone()));
    let again = fastforwardrs::project_file::read_project_file(&resaved).unwrap();
    let original = fastforwardrs::project_file::read_project_file(&file).unwrap();
    assert_eq!(
        again
            .effects_of(AnimationTarget::Clip(clip))
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .kind()
            .descriptor(),
        original
            .effects_of(AnimationTarget::Clip(clip))
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .kind()
            .descriptor(),
        "saved again with the plugin missing, the effect is what it was"
    );

    // The plugins are found there too: the effect works.
    look_for_plugins_in(&mut elsewhere, &plugins);
    assert!(
        effects_of_the_first_clip(&elsewhere)[0].is_installed(),
        "found: it works again"
    );
}

#[test]
fn a_scan_that_is_asked_for_while_one_is_going_on_is_made_after_it_and_does_not_scan_what_did_not_change_again() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let plugins = folder_with_the_test_plugins(scratch.path());
    let mut journey = Journey::start(&toolchain);
    journey.application_mut().use_this_plugin_scanner(
        PathBuf::from(SCANNER),
        Some(scratch.path().join("scans.json")),
        false,
    );
    journey.application_mut().add_plugin_folder(plugins);
    // Asked for again at once: it waits for the first, and goes on after it.
    journey.application_mut().start_scanning_plugins();
    assert!(journey.application().is_scanning_plugins());
    let began = Instant::now();
    while journey.application().is_scanning_plugins() {
        assert!(began.elapsed() < Duration::from_secs(60), "the scans did not end");
        journey.keep_drawing_for(Duration::from_millis(50));
    }
    let libraries = journey.application().plugin_libraries_found();
    assert_eq!(libraries.len(), 1);
    assert!(
        libraries[0].from_cache,
        "the second scan found the library as the first left it, and did not scan it again"
    );
    assert_eq!(EffectKind::installed_plugin_kinds().len(), 7);
}

#[test]
fn a_scan_with_no_program_to_scan_with_finds_nothing_and_a_plugin_folder_is_still_kept() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let mut journey = Journey::start(&toolchain);
    journey.application_mut().add_plugin_folder(PathBuf::from("/nowhere"));
    // (Rewritten on purpose in phase 6.8: the folders are also looked at for shaders, which needs no program to scan with and is done on a thread of its own, so the scan goes on for a
    // moment: it is waited for, and what it finds is nothing.)
    let began = Instant::now();
    while journey.application().is_scanning_plugins() {
        assert!(began.elapsed() < Duration::from_secs(60), "the scan did not end");
        journey.keep_drawing_for(Duration::from_millis(50));
    }
    assert_eq!(
        journey.application().plugin_search_settings().folders,
        [PathBuf::from("/nowhere")]
    );
    assert!(EffectKind::installed_plugin_kinds().is_empty());
    assert!(EffectKind::installed_shader_kinds().is_empty());
}

// --- the sound of a plugin in an export made by the editor ---

/// Puts the plugin `Test plugin ffrs_test_switches` on the first clip (it multiplies the sound by a whole number: its value, which the project says) and gives the number.
fn put_the_multiplier_on_the_clip(journey: &mut Journey) -> f32 {
    journey.select_the_clip_at(0);
    let plus = journey.rectangle_of("Add an effect").center();
    journey.click(plus);
    journey.settle();
    journey.open_the_smaller_menu("LADSPA");
    journey.harness.get_by_label("Test plugin ffrs_test_switches").click();
    journey.settle();
    let clip = journey.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    let values = journey
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .values()
        .to_vec();
    match values.as_slice() {
        [PropertyValue::Flag(false), PropertyValue::Number(multiplier)] => *multiplier,
        other => panic!("the values of the plugin: {other:?}"),
    }
}

fn export_and_take_the_words(journey: &mut Journey, destination: &Path) -> String {
    journey.application_mut().export_video_to(destination.to_owned());
    journey.wait_until_idle();
    let (title, text) = journey
        .application()
        .message_showing()
        .expect("a message says how the export ended");
    assert_eq!(title, "Export finished", "{text}");
    text.to_owned()
}

#[test]
fn an_export_from_the_editor_has_the_plugin_in_its_sound_when_the_folder_is_approved_and_says_which_plugins_were_not_run_when_it_is_not()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let _one_export_at_a_time = support::one_export_at_a_time();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let plugins = folder_with_the_test_plugins(scratch.path());

    // The sound of the video as it is.
    let mut plain = Journey::importing(&toolchain, &video.path);
    let plain_words = export_and_take_the_words(&mut plain, &scratch.path().join("plain.mp4"));
    assert!(!plain_words.contains("were not run"), "{plain_words}");
    let plain_loudness = support::loudness(&support::decoded_sound(
        &toolchain,
        &scratch.path().join("plain.mp4"),
        2,
        48_000,
    ));
    assert!(plain_loudness > 0.02, "there is a tone to be changed: {plain_loudness}");

    // The plugin is on the clip and its folder is approved (as the editor's file of approvals says).
    let mut approved = Journey::importing(&toolchain, &video.path);
    look_for_plugins_in(&mut approved, &plugins);
    let multiplier = put_the_multiplier_on_the_clip(&mut approved);
    assert!(multiplier >= 2.0, "the plugin makes the sound louder: {multiplier}");
    let approvals = scratch.path().join("plugin_approvals.json");
    fastforwardrs::plugin_trust::ApprovalStore::at(approvals.clone()).approve_folder(&plugins);
    approved.application_mut().keep_plugin_approvals_in(approvals);
    let words = export_and_take_the_words(&mut approved, &scratch.path().join("with_the_plugin.mp4"));
    assert!(!words.contains("were not run"), "everything was run: {words}");
    let louder = support::loudness(&support::decoded_sound(
        &toolchain,
        &scratch.path().join("with_the_plugin.mp4"),
        2,
        48_000,
    ));
    let ratio = louder / plain_loudness;
    assert!(
        (ratio - multiplier).abs() < 0.05 * multiplier,
        "the plugin multiplies the sound by {multiplier}: {louder} against {plain_loudness} is {ratio}"
    );

    // Nobody has approved the folder: the sound is as it was, and the message at the end says so.
    let mut asked_nobody = Journey::importing(&toolchain, &video.path);
    look_for_plugins_in(&mut asked_nobody, &plugins);
    put_the_multiplier_on_the_clip(&mut asked_nobody);
    let words = export_and_take_the_words(&mut asked_nobody, &scratch.path().join("not_run.mp4"));
    assert!(
        words.contains("were not run, because nobody has said yes") && words.contains("test plugin ffrs_test_switches"),
        "{words}"
    );
    let unchanged = support::loudness(&support::decoded_sound(
        &toolchain,
        &scratch.path().join("not_run.mp4"),
        2,
        48_000,
    ));
    assert!(
        (unchanged / plain_loudness - 1.0).abs() < 0.05,
        "the sound went through the plugin unchanged: {unchanged} against {plain_loudness}"
    );
}
