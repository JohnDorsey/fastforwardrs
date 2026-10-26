// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of a plugin in the real editor (phase 10, piece 10.3), with the real scan program, the real editor-window program (headless: there is no screen in a test) and the
//! CLAP plugin written for the tests that has a window and a state. A person puts the plugin on a clip, opens its window from the box of the effect, and what the window does
//! becomes edits of the project, as the panel's controls make them: one step of the history for a gesture; what the project says reaches the window; the window closes when the
//! effect goes. A plugin whose folder nobody has said yes to is not shown, and the box says why.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod journey;
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use clap_test_plugins::{EDITOR_EDIT_VALUE, library_file_for_tests};
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use fastforwardrs::project::{
    AnimationTarget, EffectInstanceId, EffectKind, PropertyId, PropertyValue, TimelineFrameIndex, set_installed_plugins,
};
use journey::Journey;
use support::{counter_video, scratch_directory};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");
const EDITOR_PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

fn folder_with_the_test_plugins(parent: &Path) -> PathBuf {
    let folder = parent.join("plugins");
    fs::create_dir_all(&folder).unwrap();
    fs::copy(library_file_for_tests(), folder.join("good.clap")).unwrap();
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

/// A journey with the video, the plugins found, and the editor plugin on the first clip (selected, so that its box is in the panel). The folder is approved if `approved`.
fn journey_with_the_editor_plugin_on_the_clip(
    toolchain: &fastforwardrs::media::FfmpegToolchain,
    scratch: &Path,
    approved: bool,
) -> (Journey, AnimationTarget, EffectKind) {
    let video = counter_video(toolchain);
    let plugins = folder_with_the_test_plugins(scratch);
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
    journey.open_the_smaller_menu("CLAP");
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Editor")
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

// (The editor's own base 64 is private: this is the little of it that the tests need.)
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> String {
    let mut bytes = Vec::new();
    let (mut group, mut bits) = (0_u32, 0);
    for letter in text.bytes().filter(|letter| *letter != b'=') {
        let value = ALPHABET
            .iter()
            .position(|candidate| *candidate == letter)
            .expect("base 64") as u32;
        group = (group << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((group >> bits) as u8);
            group &= (1 << bits) - 1;
        }
    }
    String::from_utf8(bytes).expect("the state is text")
}

fn the_state_of_the_window(journey: &mut Journey, target: AnimationTarget) -> String {
    journey
        .application_mut()
        .ask_a_plugin_window_for_its_state(target, EffectInstanceId::FIRST);
    draw_until(journey, "the state of the window", |journey| {
        journey
            .application()
            .the_state_a_plugin_window_last_said(target, EffectInstanceId::FIRST)
            .is_some()
    });
    decode(
        &journey
            .application()
            .the_state_a_plugin_window_last_said(target, EffectInstanceId::FIRST)
            .unwrap(),
    )
}

#[test]
fn a_window_opened_from_the_box_of_a_plugin_makes_its_edit_one_step_of_the_project_and_hears_what_the_project_says() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, kind) = journey_with_the_editor_plugin_on_the_clip(&toolchain, scratch.path(), true);
    assert_eq!(the_gain(&journey, target, kind), Some(PropertyValue::Number(1.0)));
    assert!(
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    );

    // The button of the box opens the window.
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
        .click();
    journey.settle();
    assert!(
        journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    );

    // The window makes its edit (it makes one when it is shown): the project has it.
    draw_until(&mut journey, "the edit of the window", |journey| {
        the_gain(journey, target, kind) == Some(PropertyValue::Number(EDITOR_EDIT_VALUE as f32))
    });
    assert!(
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
            .is_none()
    );
    // The box says that the window can be closed.
    assert!(
        journey
            .harness
            .query_by_role_and_label(egui::accesskit::Role::Button, "Close editor")
            .is_some()
    );

    // A gesture of three steps (begin, value, end) is one step of the history: one undo takes the value back to what it was before the window.
    journey.press_key_with(Modifiers::COMMAND, Key::Z);
    journey.settle();
    assert_eq!(
        the_gain(&journey, target, kind),
        Some(PropertyValue::Number(1.0)),
        "one undo: the whole gesture"
    );

    // And the window is told: the project says 1, and the window has 1.
    // (The window is told in a frame or two: not at the very moment of the undo.)
    journey.keep_drawing_for(Duration::from_millis(150));
    let state = the_state_of_the_window(&mut journey, target);
    assert!(state.contains("gain=1 "), "{state}");

    journey.press_key_with(Modifiers::COMMAND, Key::Y);
    journey.settle();
    assert_eq!(
        the_gain(&journey, target, kind),
        Some(PropertyValue::Number(EDITOR_EDIT_VALUE as f32))
    );
    journey.keep_drawing_for(Duration::from_millis(150));
    assert!(the_state_of_the_window(&mut journey, target).contains("gain=0.75 "));
}

#[test]
fn the_button_closes_the_window_and_taking_the_effect_off_closes_it_too() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, _) = journey_with_the_editor_plugin_on_the_clip(&toolchain, scratch.path(), true);
    let open = |journey: &mut Journey| {
        journey
            .harness
            .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
            .click();
        journey.settle();
    };

    open(&mut journey);
    assert!(
        journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    );
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Close editor")
        .click();
    draw_until(&mut journey, "the window to close", |journey| {
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    });

    // It can be opened again; and when the effect is deleted the window goes with it.
    open(&mut journey);
    assert!(
        journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    );
    let heading = journey.rectangle_of("Editor").center();
    journey.right_click(heading);
    journey.settle();
    journey.press_button("Delete effect");
    journey.settle();
    draw_until(&mut journey, "the window to close with its effect", |journey| {
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    });
}

#[test]
fn a_plugin_whose_folder_nobody_has_said_yes_to_is_not_shown_and_the_box_says_why() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, _) = journey_with_the_editor_plugin_on_the_clip(&toolchain, scratch.path(), false);

    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
        .click();
    journey.settle();

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
    assert!(
        journey.harness.query_by_label_contains("nobody has said yes").is_some(),
        "the box says it"
    );

    // Somebody says yes, and the window is asked for again: it opens, and what was said of the last time is not said any more.
    let approvals = scratch.path().join("approvals.json");
    fastforwardrs::plugin_trust::ApprovalStore::at(approvals.clone()).approve_folder(&scratch.path().join("plugins"));
    journey.application_mut().keep_plugin_approvals_in(approvals);
    journey.settle();
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
        .click();
    journey.settle();
    assert!(
        journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST),
        "{:?}",
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
    );
    assert_eq!(
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST),
        None,
        "asked for again, and it opened: no problem"
    );
}

#[test]
fn the_window_is_opened_with_the_values_that_the_project_has_and_closes_when_another_project_is_opened() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, kind) = journey_with_the_editor_plugin_on_the_clip(&toolchain, scratch.path(), true);
    journey.application_mut().set_the_value_of_an_effect(
        target,
        PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0),
        PropertyValue::Number(2.5),
    );
    journey.settle();
    assert_eq!(the_gain(&journey, target, kind), Some(PropertyValue::Number(2.5)));

    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
        .click();
    journey.settle();

    // The plugin had 2.5 when its window was shown (the window then made its own edit).
    let state = the_state_of_the_window(&mut journey, target);
    assert!(state.contains("gain_when_shown=2.5"), "{state}");

    // Another project is opened (the file of this one): the window of the project that was is closed.
    let file = scratch.path().join("with_the_window.ffrs");
    assert!(journey.application_mut().save_project_to(file.clone()));
    journey.application_mut().load_project_from(file);
    draw_until(&mut journey, "the window to close with its project", |journey| {
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    });
}
