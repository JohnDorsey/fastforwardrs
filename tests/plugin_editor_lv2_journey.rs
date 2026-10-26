// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of an LV2 plugin in the real editor (phase 11, piece 11.5), with the real scan program, the real editor-window program (headless: there is no screen in a test) and the
//! LV2 test bundle, whose gain plugin has a window. A person puts the plugin on a clip, opens its window from the box of the effect, and what the window does becomes an edit of the
//! project, as the panel's controls make it: one step of the history for a gesture; what the project says reaches the window; the window closes when its plugin says so. A plugin that has
//! no window has no button for one.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod journey;
mod support;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use fastforwardrs::project::{
    AnimationTarget, EffectInstanceId, EffectKind, PropertyId, PropertyValue, TimelineFrameIndex, set_installed_plugins,
};
use journey::Journey;
use lv2_test_plugins::{UI_CLOSE_VALUE, UI_EDIT_VALUE};
use support::{counter_video, scratch_directory};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");
const EDITOR_PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

/// A folder of bundles with the bundle of the test plugins in it.
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
    let began = Instant::now();
    while journey.application().is_scanning_plugins() {
        assert!(began.elapsed() < Duration::from_secs(60), "the scan did not end");
        journey.keep_drawing_for(Duration::from_millis(50));
    }
}

/// A journey with the video, the plugins found, and the plugin called `name` (from the LV2 smaller menu) on the first clip, selected so that its box is in the panel. The folder of the
/// bundles is approved if `approved`.
fn journey_with_an_lv2_plugin_on_the_clip(
    toolchain: &fastforwardrs::media::FfmpegToolchain,
    scratch: &Path,
    name: &str,
    approved: bool,
) -> (Journey, AnimationTarget, EffectKind) {
    let video = counter_video(toolchain);
    let plugins = folder_with_the_test_bundle(scratch);
    let mut journey = Journey::importing(toolchain, &video.path);
    look_for_plugins_in(&mut journey, &plugins);
    if approved {
        let approvals = scratch.join("approvals.json");
        fastforwardrs::plugin_trust::ApprovalStore::at(approvals.clone()).approve_folder(&plugins);
        journey.application_mut().keep_plugin_approvals_in(approvals);
    }
    journey
        .application_mut()
        .show_plugin_windows_with_this_program(PathBuf::from(EDITOR_PROGRAM), true);
    journey.select_the_clip_at(0);
    let plus = journey.rectangle_of("Add an effect").center();
    journey.click(plus);
    journey.settle();
    journey.open_the_smaller_menu("LV2");
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, name)
        .click();
    journey.settle();
    let clip = journey.application().project().tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .id;
    let target = AnimationTarget::Clip(clip);
    let kind = journey
        .application()
        .project()
        .effects_of(target)
        .unwrap()
        .iter()
        .next()
        .expect("the plugin is on the clip")
        .kind();
    (journey, target, kind)
}

fn the_gain(journey: &Journey, target: AnimationTarget, kind: EffectKind) -> Option<PropertyValue> {
    journey.application().project().property_value_at(
        target,
        PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0),
        TimelineFrameIndex::new(0),
    )
}

fn draw_until(journey: &mut Journey, what: &str, done: impl Fn(&Journey) -> bool) {
    let began = Instant::now();
    while !done(journey) {
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "{what} did not happen in time"
        );
        journey.keep_drawing_for(Duration::from_millis(30));
    }
}

fn open_the_window(journey: &mut Journey) {
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
        .click();
    journey.settle();
}

#[test]
fn the_window_of_an_lv2_plugin_makes_its_edit_one_step_of_the_project_and_what_the_project_says_reaches_it() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, kind) =
        journey_with_an_lv2_plugin_on_the_clip(&toolchain, scratch.path(), "Test gain", true);
    assert!(
        kind.installed_plugin().unwrap().has_a_window,
        "the scan says that it has a window"
    );
    assert_eq!(the_gain(&journey, target, kind), Some(PropertyValue::Number(1.0)));

    open_the_window(&mut journey);
    assert!(
        journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST),
        "{:?}",
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
    );

    // The window makes its edit (it makes one when it is given its time): the project has it.
    draw_until(&mut journey, "the edit of the window", |journey| {
        the_gain(journey, target, kind) == Some(PropertyValue::Number(UI_EDIT_VALUE))
    });
    assert!(
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
            .is_none()
    );
    assert!(
        journey
            .harness
            .query_by_role_and_label(egui::accesskit::Role::Button, "Close editor")
            .is_some(),
        "the box says that the window can be closed"
    );

    // A gesture of three steps (touch, write, let go) is one step of the history.
    journey.press_key_with(Modifiers::COMMAND, Key::Z);
    journey.settle();
    assert_eq!(
        the_gain(&journey, target, kind),
        Some(PropertyValue::Number(1.0)),
        "one undo: the whole gesture"
    );
    journey.press_key_with(Modifiers::COMMAND, Key::Y);
    journey.settle();
    assert_eq!(
        the_gain(&journey, target, kind),
        Some(PropertyValue::Number(UI_EDIT_VALUE))
    );
}

#[test]
fn a_value_that_the_project_says_reaches_the_window_and_the_one_that_closes_it_closes_it_without_a_failure() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, kind) =
        journey_with_an_lv2_plugin_on_the_clip(&toolchain, scratch.path(), "Test gain", true);
    open_the_window(&mut journey);
    draw_until(&mut journey, "the edit of the window", |journey| {
        the_gain(journey, target, kind) == Some(PropertyValue::Number(UI_EDIT_VALUE))
    });

    // The value that the test window takes for a request to close: said by the project, as a drag of the control in the panel would say it.
    journey.application_mut().set_the_value_of_an_effect(
        target,
        PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0),
        PropertyValue::Number(UI_CLOSE_VALUE),
    );

    draw_until(&mut journey, "the window to close itself", |journey| {
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    });
    assert!(
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
            .is_none(),
        "it closed; it did not fail"
    );
}

#[test]
fn a_plugin_that_has_no_window_has_no_button_for_one_and_a_plugin_whose_folder_nobody_said_yes_to_says_so() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (without_a_window, _, kind) =
        journey_with_an_lv2_plugin_on_the_clip(&toolchain, scratch.path(), "Test controls", true);
    assert!(!kind.installed_plugin().unwrap().has_a_window);
    assert!(
        without_a_window
            .harness
            .query_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
            .is_none()
    );

    let other_scratch = scratch_directory();
    let (mut journey, target, _) =
        journey_with_an_lv2_plugin_on_the_clip(&toolchain, other_scratch.path(), "Test gain", false);
    open_the_window(&mut journey);
    assert!(
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    );
    let problem = journey
        .application()
        .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
        .expect("it says why");
    assert!(problem.contains("nobody has said yes"), "{problem}");
}
