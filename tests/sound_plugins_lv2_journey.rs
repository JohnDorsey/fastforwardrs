// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! LV2 plugins in the real editor, with the real scan program and the LV2 bundle written for the tests (phase 11, pieces 11.2 and 11.3): a person says where the bundles are, the
//! scan finds them in the background, a plugin is put on a clip from the *LV2* smaller menu, the project is saved, and it is loaded where the plugin is not (the effect is kept whole
//! and says it is not installed) and where it is (it works again).
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod journey;
mod support;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use fastforwardrs::project::{AnimationTarget, EffectKind, PluginAddress, PropertyValue, set_installed_plugins};
use journey::Journey;
use support::{counter_video, scratch_directory};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

/// A folder of bundles (as `LV2_PATH` names) with the bundle of the test plugins in it.
fn folder_with_the_test_bundle(parent: &Path) -> PathBuf {
    let folder = parent.join("lv2");
    std::fs::create_dir_all(&folder).unwrap();
    lv2_test_plugins::write_bundle(&folder);
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
fn lv2_plugins_found_in_the_background_are_offered_put_on_a_clip_saved_and_kept_whole_where_they_are_not_installed() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let video = counter_video(&toolchain);
    let plugins = folder_with_the_test_bundle(scratch.path());

    let mut journey = Journey::importing(&toolchain, &video.path);
    look_for_plugins_in(&mut journey, &plugins);
    assert!(
        journey.application().plugin_scan_problems().is_empty(),
        "{:?}",
        journey.application().plugin_scan_problems()
    );
    assert_eq!(
        journey.application().plugin_libraries_found().len(),
        1,
        "the bundle is the one library"
    );
    assert_eq!(
        EffectKind::installed_plugin_kinds().len(),
        8,
        "eight of the ten test plugins can be effects (the instrument is a synthesizer; not the one that requires log:log, and not the one that gives nothing)"
    );
    assert!(
        EffectKind::installed_plugin_kinds()
            .iter()
            .all(|kind| kind.key().starts_with("lv2:urn_fastforwardrs_test_"))
    );

    // Put the gain on the clip, from the smaller menu of LV2 in the menu of the plus button.
    journey.select_the_clip_at(0);
    let plus = journey.rectangle_of("Add an effect").center();
    journey.click(plus);
    journey.settle();
    journey.open_the_smaller_menu("LV2");
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Test gain")
        .click();
    journey.settle();
    let kinds = effects_of_the_first_clip(&journey);
    assert_eq!(kinds.len(), 1);
    assert_eq!(kinds[0].key(), "lv2:urn_fastforwardrs_test_gain");
    assert!(kinds[0].is_installed());
    let installed = kinds[0].installed_plugin().unwrap();
    assert_eq!(
        installed.address,
        PluginAddress::Lv2 {
            uri: lv2_test_plugins::GAIN.to_owned()
        }
    );
    assert_eq!(
        installed.value_handles,
        [2],
        "the number of the port that the plugin gave it"
    );
    let file = scratch.path().join("with_an_lv2_plugin.ffrs");
    assert!(journey.application_mut().save_project_to(file.clone()));

    // Another editor, where the plugin is not installed: the effect is there, whole, and does nothing.
    set_installed_plugins(Vec::new());
    let mut elsewhere = Journey::start(&toolchain);
    elsewhere.application_mut().load_project_from(file.clone());
    elsewhere.wait_until_idle();
    let kinds_there = effects_of_the_first_clip(&elsewhere);
    assert_eq!(kinds_there.len(), 1);
    assert_eq!(kinds_there[0].key(), "lv2:urn_fastforwardrs_test_gain");
    assert!(!kinds_there[0].is_installed(), "the plugin is not installed here");
    let clip = elsewhere.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    let values_there = elsewhere
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .values()
        .to_vec();
    assert_eq!(values_there, [PropertyValue::Number(1.0)], "its values are kept");

    // The plugins are found there too: the effect works again.
    look_for_plugins_in(&mut elsewhere, &plugins);
    assert!(
        effects_of_the_first_clip(&elsewhere)[0].is_installed(),
        "found: it is installed again"
    );
}
