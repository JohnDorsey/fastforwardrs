// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! VST3 plugins in the real editor, with the real scan program, the real program that runs plugins and the VST3 module written for the tests (piece 15.4 of
//! `docs/engine-phase-15-vst3-design.md`): a person says where the plugins are, the scan finds them in the background, a plugin is put on a clip from the *VST3* smaller menu, the
//! project is saved, and it is loaded where the plugin is not (the effect is kept whole and says it is not installed) and where it is (it works again); **and an export from the editor has
//! the plugin in its sound when the folder is approved, and says which plugins were not run when it is not**.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod journey;
mod support;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use fastforwardrs::plugin_scan::key_of_a_vst3_plugin;
use fastforwardrs::project::{AnimationTarget, EffectKind, PluginAddress, PropertyValue, set_installed_plugins};
use journey::Journey;
use support::{counter_video, scratch_directory};
use vst3_test_plugins as vst3_plugins;

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

fn the_id_of(numbers: [u32; 4]) -> vst3_host::ClassId {
    vst3_host::ClassId::from_numbers(numbers[0], numbers[1], numbers[2], numbers[3])
}

/// A folder with the module of the test plugins in it (`test.vst3`, a folder of its own).
fn folder_with_the_test_module(parent: &Path) -> PathBuf {
    let folder = parent.join("plugins");
    std::fs::create_dir_all(&folder).unwrap();
    vst3_plugins::write_module(&folder);
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

fn values_of_the_first_effect_of_the_first_clip(journey: &Journey) -> Vec<PropertyValue> {
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
        .next()
        .unwrap()
        .values()
        .to_vec()
}

/// Puts the plugin that is called `label` in the menu on the first clip, from the smaller menu of VST3 in the menu of the plus button.
fn put_the_plugin_on_the_clip(journey: &mut Journey, label: &str) {
    journey.select_the_clip_at(0);
    let plus = journey.rectangle_of("Add an effect").center();
    journey.click(plus);
    journey.settle();
    journey.open_the_smaller_menu("VST3");
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, label)
        .click();
    journey.settle();
}

#[test]
fn vst3_plugins_found_in_the_background_are_offered_put_on_a_clip_saved_and_kept_whole_where_they_are_not_installed() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let plugins = folder_with_the_test_module(scratch.path());

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
        "seven of the eleven test plugins can be effects (two of them instruments); not the four that misbehave"
    );
    assert!(
        EffectKind::installed_plugin_kinds()
            .iter()
            .all(|kind| kind.key().starts_with("vst3:5445535"))
    );

    // Put the gain on the clip, from the smaller menu of VST3 in the menu of the plus button.
    put_the_plugin_on_the_clip(&mut journey, "Test gain \u{e9}");
    let kinds = effects_of_the_first_clip(&journey);
    assert_eq!(kinds.len(), 1);
    let gain_id = the_id_of(vst3_plugins::GAIN);
    assert_eq!(kinds[0].key(), key_of_a_vst3_plugin(gain_id));
    assert!(kinds[0].is_installed());
    let installed = kinds[0].installed_plugin().unwrap();
    assert_eq!(installed.address, PluginAddress::Vst3 { class_id: gain_id });
    assert_eq!(
        installed.value_handles,
        [
            vst3_plugins::PARAMETER_GAIN,
            vst3_plugins::PARAMETER_MODE,
            vst3_plugins::PARAMETER_BYPASS
        ],
        "the parameter ids the plugin gave its values"
    );
    let values_here = values_of_the_first_effect_of_the_first_clip(&journey);
    assert_eq!(
        values_here[0],
        PropertyValue::Number(1.0),
        "the gain the plugin begins with"
    );
    let file = scratch.path().join("with_a_vst3_plugin.ffrs");
    assert!(journey.application_mut().save_project_to(file.clone()));

    // Another editor, where the plugin is not installed: the effect is there, whole, and does nothing.
    set_installed_plugins(Vec::new());
    let mut elsewhere = Journey::start(&toolchain);
    elsewhere.application_mut().load_project_from(file.clone());
    elsewhere.wait_until_idle();
    let kinds_there = effects_of_the_first_clip(&elsewhere);
    assert_eq!(kinds_there.len(), 1);
    assert_eq!(kinds_there[0].key(), key_of_a_vst3_plugin(gain_id));
    assert!(!kinds_there[0].is_installed(), "the plugin is not installed here");
    assert_eq!(
        values_of_the_first_effect_of_the_first_clip(&elsewhere),
        values_here,
        "its values are kept"
    );
    let clip = elsewhere.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    let resaved = scratch.path().join("saved_again.ffrs");
    assert!(elsewhere.application_mut().save_project_to(resaved.clone()));
    let describe = |path: &Path| {
        fastforwardrs::project_file::read_project_file(path)
            .unwrap()
            .effects_of(AnimationTarget::Clip(clip))
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .kind()
            .descriptor()
            .clone()
    };
    assert_eq!(
        describe(&resaved),
        describe(&file),
        "saved again with the plugin missing, the effect is what it was"
    );

    // The plugins are found there too: the effect works again.
    look_for_plugins_in(&mut elsewhere, &plugins);
    assert!(
        effects_of_the_first_clip(&elsewhere)[0].is_installed(),
        "found: it is installed again"
    );
}

// --- the sound of a plugin in an export made by the editor ---

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
fn an_export_from_the_editor_has_the_vst3_plugin_in_its_sound_when_the_folder_is_approved_and_says_which_plugins_were_not_run_when_it_is_not()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let _one_export_at_a_time = support::one_export_at_a_time();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let plugins = folder_with_the_test_module(scratch.path());
    // (The effect with a side chain makes the sound that reaches it half as loud, whatever its values are: it has none to be set.)
    let halving = "Test effect with a side chain \u{e9}";

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
    put_the_plugin_on_the_clip(&mut approved, halving);
    let approvals = scratch.path().join("plugin_approvals.json");
    fastforwardrs::plugin_trust::ApprovalStore::at(approvals.clone()).approve_folder(&plugins);
    approved.application_mut().keep_plugin_approvals_in(approvals);
    let words = export_and_take_the_words(&mut approved, &scratch.path().join("with_the_plugin.mp4"));
    assert!(!words.contains("were not run"), "everything was run: {words}");
    let halved = support::loudness(&support::decoded_sound(
        &toolchain,
        &scratch.path().join("with_the_plugin.mp4"),
        2,
        48_000,
    ));
    let ratio = halved / plain_loudness;
    assert!(
        (ratio - 0.5).abs() < 0.03,
        "the plugin halves the sound: {halved} against {plain_loudness} is {ratio}"
    );

    // Nobody has approved the folder: the sound is as it was, and the message at the end says so.
    let mut asked_nobody = Journey::importing(&toolchain, &video.path);
    look_for_plugins_in(&mut asked_nobody, &plugins);
    put_the_plugin_on_the_clip(&mut asked_nobody, halving);
    let words = export_and_take_the_words(&mut asked_nobody, &scratch.path().join("not_run.mp4"));
    assert!(
        words.contains("were not run, because nobody has said yes") && words.contains("test effect with a side chain"),
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
