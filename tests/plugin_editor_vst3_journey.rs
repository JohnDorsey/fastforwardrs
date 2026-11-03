// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of a VST3 plugin in the real editor (piece 15.6 of `docs/engine-phase-15-vst3-design.md`), with the real scan program, the real editor-window program (headless: there is no
//! screen in a test) and the module of the VST3 test plugins, whose gain effect has a window. A person puts the plugin on a clip, opens its window from the box of the effect, and what
//! the window does becomes an edit of the project, as the panel's controls make it: one step of the history for a gesture, in the plain value that the panel shows; what the project says
//! reaches the window. A plugin that has no window has no button for one.
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
use support::{counter_video, scratch_directory};
use vst3_test_plugins as plugins;

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");
const EDITOR_PROGRAM: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-editor");

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

/// A folder with the module of the test plugins in it.
fn folder_with_the_test_module(parent: &Path) -> PathBuf {
    let folder = parent.join("plugins");
    std::fs::create_dir_all(&folder).unwrap();
    plugins::write_module(&folder);
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

/// A journey with the video, the plugins found, and the plugin called `name` (from the VST3 smaller menu) on the first clip, selected so that its box is in the panel. The folder of the
/// modules is approved if `approved`, and the test window presses `keys` when it is put in its parent.
fn journey_with_a_vst3_plugin_on_the_clip(
    toolchain: &fastforwardrs::media::FfmpegToolchain,
    scratch: &Path,
    name: &str,
    approved: bool,
    keys: &str,
) -> (Journey, AnimationTarget, EffectKind) {
    let video = counter_video(toolchain);
    let folder = folder_with_the_test_module(scratch);
    let mut journey = Journey::importing(toolchain, &video.path);
    look_for_plugins_in(&mut journey, &folder);
    if approved {
        let approvals = scratch.join("approvals.json");
        fastforwardrs::plugin_trust::ApprovalStore::at(approvals.clone()).approve_folder(&folder);
        journey.application_mut().keep_plugin_approvals_in(approvals);
    }
    journey
        .application_mut()
        .show_plugin_windows_with_this_program(PathBuf::from(EDITOR_PROGRAM), true);
    journey
        .application_mut()
        .add_to_the_environment_of_plugin_windows("FFRS_TEST_VIEW_KEYS", keys);
    journey.select_the_clip_at(0);
    let plus = journey.rectangle_of("Add an effect").center();
    journey.click(plus);
    journey.settle();
    journey.open_the_smaller_menu("VST3");
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

/// The gain of the effect (its first value), as the project has it.
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

const THE_GAIN_THAT_THE_WINDOW_MAKES: f32 = 3.0;

#[test]
fn the_window_of_a_vst3_plugin_makes_its_edit_one_step_of_the_project_in_the_plain_value_the_panel_shows() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, kind) =
        journey_with_a_vst3_plugin_on_the_clip(&toolchain, scratch.path(), "Test gain \u{e9}", true, "e");
    assert!(
        kind.installed_plugin().unwrap().has_a_window,
        "the scan says that it has a window"
    );
    assert_eq!(
        the_gain(&journey, target, kind),
        Some(PropertyValue::Number(1.0)),
        "the gain the plugin begins with"
    );

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

    // The window makes its edit when it is put in its parent: the project has it, as the plain 3 (and not the 0.75 that the controller says).
    draw_until(&mut journey, "the edit of the window", |journey| {
        the_gain(journey, target, kind) == Some(PropertyValue::Number(THE_GAIN_THAT_THE_WINDOW_MAKES))
    });
    assert!(
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
            .is_none()
    );

    // A gesture of three steps (begin, perform, end) is one step of the history.
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
        Some(PropertyValue::Number(THE_GAIN_THAT_THE_WINDOW_MAKES))
    );
}

#[test]
fn what_a_plugin_window_says_asks_the_editor_to_draw_a_frame_at_once_without_waiting_for_its_timer() {
    // A hand that drags a knob in the plugin's window is not over the editor's window, which draws only when something asks it to. Until 2026-11-03 the only thing that asked was a
    // timer of 30 milliseconds, and a slider that follows the knob moved in jumps of several edits (found by a person dragging a knob of Dragonfly Reverb).
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, _kind) =
        journey_with_a_vst3_plugin_on_the_clip(&toolchain, scratch.path(), "Test gain \u{e9}", true, "e");
    // (egui keeps the cause of each request for a frame, the place in the code that made it, and says them at the next frame, whether or not it called the integration for each. The
    // editor's timer makes one request in each frame that a window is open, always from the same place; any other request from the code of the windows is the window's own.)
    journey.wait_until_idle();
    journey
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
        .click();
    let mut places_that_asked = Vec::new();
    for _ in 0..40 {
        journey.harness.step();
        places_that_asked.extend(
            journey
                .harness
                .ctx
                .repaint_causes()
                .iter()
                .filter(|cause| cause.file.ends_with("plugin_editor_flow.rs"))
                .map(|cause| cause.line),
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // (By now the window has said all that it had to say: the last frame has the timer's request only.)
    let the_timer = *places_that_asked
        .last()
        .expect("the timer asks in each frame that a window is open");
    let asked_by_the_window = places_that_asked.iter().filter(|line| **line != the_timer).count();
    assert!(
        asked_by_the_window >= 1,
        "the window said things and the editor was asked for no frame by it, only by its timer: {places_that_asked:?}"
    );
    assert!(
        journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST),
        "(the window is the one that is open)"
    );
}

#[test]
fn the_state_that_a_window_says_changed_is_asked_for_and_kept_in_the_effect_with_the_hands_edit_in_it() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, kind) =
        journey_with_a_vst3_plugin_on_the_clip(&toolchain, scratch.path(), "Test gain \u{e9}", true, "ed");
    assert!(
        journey
            .application()
            .project()
            .effects_of(target)
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .state()
            .is_none(),
        "no state before the window"
    );

    open_the_window(&mut journey);
    draw_until(
        &mut journey,
        "the state the window said changed to be kept",
        |journey| {
            journey
                .application()
                .project()
                .effects_of(target)
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .state()
                .is_some()
        },
    );

    // The state is the plugin's own, with the edit in it: a plugin that is made from it has the gain the hand made.
    let state = journey
        .application()
        .project()
        .effects_of(target)
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .state()
        .cloned()
        .expect("the state is kept");
    let module = Path::new(&kind.installed_plugin().unwrap().library).to_owned();
    let opened = vst3_host::Module::open(&module).expect("the module opens");
    let class =
        vst3_host::ClassId::from_numbers(plugins::GAIN[0], plugins::GAIN[1], plugins::GAIN[2], plugins::GAIN[3]);
    let (mut instance, _) = vst3_host::Instance::new(&opened, class, 48_000.0, 512, None).expect("the plugin is made");
    instance.restore_state(&state).expect("the state is taken");
    assert_eq!(
        instance.plain_value(plugins::PARAMETER_GAIN),
        Some(f64::from(THE_GAIN_THAT_THE_WINDOW_MAKES))
    );
}

#[test]
fn a_plugin_that_has_no_window_has_no_button_for_one_and_a_plugin_whose_folder_nobody_said_yes_to_says_so() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (without_a_window, _, kind) = journey_with_a_vst3_plugin_on_the_clip(
        &toolchain,
        scratch.path(),
        "Test effect with a side chain \u{e9}",
        true,
        "",
    );
    assert!(!kind.installed_plugin().unwrap().has_a_window);
    assert!(
        without_a_window
            .harness
            .query_by_role_and_label(egui::accesskit::Role::Button, "Open editor")
            .is_none()
    );

    let other_scratch = scratch_directory();
    let (mut journey, target, _) =
        journey_with_a_vst3_plugin_on_the_clip(&toolchain, other_scratch.path(), "Test gain \u{e9}", false, "");
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

#[test]
fn a_window_that_crashes_is_a_problem_in_the_box_of_the_effect_and_the_editor_goes_on() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let scratch = scratch_directory();
    let (mut journey, target, _) =
        journey_with_a_vst3_plugin_on_the_clip(&toolchain, scratch.path(), "Test gain \u{e9}", true, "");
    journey
        .application_mut()
        .add_to_the_environment_of_plugin_windows("FFRS_TEST_PLUGIN", "crash_when_attached");

    open_the_window(&mut journey);

    draw_until(&mut journey, "the failure to be heard", |journey| {
        journey
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
            .is_some()
    });
    assert!(
        !journey
            .application()
            .is_a_plugin_window_open(target, EffectInstanceId::FIRST)
    );
}
