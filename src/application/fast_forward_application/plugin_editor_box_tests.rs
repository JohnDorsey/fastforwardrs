// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The button of a plugin's window in the box of the effect (phase 10, piece 10.3), without the program that shows the windows (the journey of `tests/plugin_editor_journey.rs` has it):
//! which boxes have the button, what clicking it asks for, and what the box says when the window cannot be opened.

use std::path::PathBuf;

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::project::{AnimationTarget, ClipId, EffectInstanceId, EffectKind, PluginAddress, set_installed_plugins};
use crate::test_support::{an_installed_sound_plugin, the_installed_plugins_to_myself};

const OPEN: &str = "Open editor";
const CLOSE: &str = "Close editor";

/// A plugin of CLAP that has a window (or not), as a kind of effect that is installed.
fn a_clap_plugin(name: &str, has_a_window: bool) -> EffectKind {
    let mut plugin = an_installed_sound_plugin(
        &format!("clap:{name}"),
        "Somebody",
        &[("amount", 1.0, Some((0.0, 4.0)))],
    );
    plugin.address = PluginAddress::Clap { id: name.to_owned() };
    plugin.has_a_window = has_a_window;
    plugin.descriptor.name = name.to_owned().into();
    plugin.library = PathBuf::from(format!("/plugins/{name}.clap"));
    set_installed_plugins(vec![plugin]);
    EffectKind::installed_plugin_kinds()[0]
}

fn window_with_the_plugin_on_the_clip(kind: EffectKind) -> (Window, ClipId) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind,
    });
    window.settle();
    (window, clip)
}

fn the_button_is_there(window: &Window, label: &str) -> bool {
    window
        .harness
        .query_by_role_and_label(egui::accesskit::Role::Button, label)
        .is_some()
}

#[test]
fn a_plugin_that_has_a_window_has_a_button_for_it_in_its_box_and_one_that_has_none_has_not() {
    let _alone = the_installed_plugins_to_myself();
    let (window, _) = window_with_the_plugin_on_the_clip(a_clap_plugin("with_a_window", true));
    assert!(the_button_is_there(&window, OPEN));
    assert!(!the_button_is_there(&window, CLOSE), "it is not open");

    let (window, _) = window_with_the_plugin_on_the_clip(a_clap_plugin("without_a_window", false));
    assert!(!the_button_is_there(&window, OPEN));
}

#[test]
fn a_plugin_has_the_button_on_a_track_too_and_an_effect_that_is_not_a_plugin_has_none() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("on_a_track", true);
    let (mut window, clip) = window_with_a_video();
    let track = window.application().project().track_holding(clip).unwrap().id();
    window.do_this(Command::select_track(
        track,
        crate::application::selection::SelectionChange::Replace,
    ));
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Track(track),
        kind,
    });
    window.settle();
    assert!(the_button_is_there(&window, OPEN));

    // (A hue shift is no plugin.)
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();
    assert!(!the_button_is_there(&window, OPEN));
}

#[test]
fn a_window_that_cannot_be_opened_says_why_in_the_box_until_it_is_asked_for_again() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("not_approved", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    let target = AnimationTarget::Clip(clip);
    let instance = EffectInstanceId::FIRST;

    // Nobody has said yes to the folder of the plugin.
    window.do_this(Command::OpenPluginEditor { target, instance });
    window.settle();
    let problem = window.application().problem_with_a_plugin_window(target, instance);
    assert!(
        problem
            .as_deref()
            .is_some_and(|why| why.contains("nobody has said yes")),
        "{problem:?}"
    );
    assert!(
        window.harness.query_by_label_contains("nobody has said yes").is_some(),
        "the box says it"
    );
    assert!(!window.application().is_a_plugin_window_open(target, instance));

    // The folder is approved, and the program that shows windows is not there.
    window
        .application_mut()
        .approve_the_plugin_folder_of_the_tests(PathBuf::from("/plugins/not_approved.clap"));
    window
        .application_mut()
        .show_plugin_windows_with_this_program(PathBuf::from("no-such-folder/no-such-program"), true);
    window.do_this(Command::OpenPluginEditor { target, instance });
    window.settle();
    let problem = window.application().problem_with_a_plugin_window(target, instance);
    assert!(
        problem.as_deref().is_some_and(|why| why.contains("could not be run")),
        "{problem:?}"
    );
    assert!(
        window.harness.query_by_label_contains("nobody has said yes").is_none(),
        "what was said before is not said any more"
    );
    assert!(!window.application().is_a_plugin_window_open(target, instance));
}

#[test]
fn a_plugin_that_has_no_window_is_not_opened_and_closing_what_is_not_open_does_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("without_a_window_at_all", false);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    let target = AnimationTarget::Clip(clip);
    let instance = EffectInstanceId::FIRST;

    window.do_this(Command::OpenPluginEditor { target, instance });
    window.do_this(Command::ClosePluginEditor { target, instance });
    window.settle();

    let problem = window.application().problem_with_a_plugin_window(target, instance);
    assert!(
        problem.as_deref().is_some_and(|why| why.contains("has no window")),
        "{problem:?}"
    );
    assert!(!window.application().is_a_plugin_window_open(target, instance));
}

#[test]
fn opening_a_window_is_no_edit_of_the_project() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("not_an_edit", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    window.application_mut().note_everything_is_saved();

    window.do_this(Command::OpenPluginEditor {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::FIRST,
    });
    window.settle();

    assert!(!window.application().has_unsaved_changes(), "a window is not an edit");
}

// --- what a window's edits do to the project, without a window ---

use std::time::{Duration, Instant};

use crate::plugin_editor::protocol::ParameterEdit;
use crate::plugin_editor::sessions::{WindowEvent, WindowKey};
use crate::project::{PropertyId, PropertyValue, TimelineFrameIndex};

/// What the one value of the plugin of `a_clap_plugin` is on the clip.
fn the_amount(window: &Window, clip: ClipId, kind: EffectKind) -> Option<PropertyValue> {
    window.application().project().property_value_at(
        AnimationTarget::Clip(clip),
        PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0),
        TimelineFrameIndex::new(0),
    )
}

/// The window says these things, at these times (the first of them at `start`). **The history merges edits by the times given** here, and not whenever they are made as the window
/// of the tests does.
fn the_window_says(window: &mut Window, clip: ClipId, says: &[(Duration, WindowEvent)]) {
    window.application_mut().document.history.merge_edits_by_time_again();
    let key = WindowKey {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::FIRST,
    };
    let start = Instant::now();
    let context = egui::Context::default();
    for (after, event) in says {
        window
            .application_mut()
            .do_what_a_plugin_editor_said(key, event.clone(), start + *after, &context);
    }
}

#[test]
fn what_a_hand_does_in_a_window_is_the_value_of_the_effect_and_the_edits_of_one_gesture_are_one_step_however_far_apart()
{
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("a_gesture", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    assert_eq!(the_amount(&window, clip, kind), Some(PropertyValue::Number(1.0)));
    let second = Duration::from_secs(1);

    // (The parameter id of the plugin of the tests is 0: see `an_installed_sound_plugin`.)
    the_window_says(
        &mut window,
        clip,
        &[
            (Duration::ZERO, WindowEvent::Edit(ParameterEdit::Begin { id: 0 })),
            (
                Duration::ZERO,
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 1.5 }),
            ),
            (
                second * 5,
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 2.5 }),
            ),
            (second * 5, WindowEvent::Edit(ParameterEdit::End { id: 0 })),
        ],
    );
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(2.5)),
        "the hand's value"
    );

    window.do_this(Command::Undo);
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(1.0)),
        "one undo takes the whole gesture, though five seconds went by in it"
    );
}

#[test]
fn a_gesture_that_has_ended_is_over_and_an_edit_a_long_time_after_it_is_another_step() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("after_a_gesture", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);

    the_window_says(
        &mut window,
        clip,
        &[
            (Duration::ZERO, WindowEvent::Edit(ParameterEdit::Begin { id: 0 })),
            (
                Duration::ZERO,
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 1.5 }),
            ),
            (Duration::ZERO, WindowEvent::Edit(ParameterEdit::End { id: 0 })),
            (
                Duration::from_secs(5),
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 2.5 }),
            ),
        ],
    );
    assert!(
        !window.application().interface.plugin_editors.a_gesture_is_open(),
        "it ended"
    );

    window.do_this(Command::Undo);
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(1.5)),
        "the gesture is one step, and the edit long after it is another"
    );
}

#[test]
fn edits_that_are_not_one_gesture_and_are_far_apart_are_two_steps() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("two_gestures", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);

    the_window_says(
        &mut window,
        clip,
        &[
            (
                Duration::ZERO,
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 1.5 }),
            ),
            (
                Duration::from_secs(5),
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 2.5 }),
            ),
        ],
    );

    window.do_this(Command::Undo);
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(1.5)),
        "the first edit is still there: they were two steps"
    );
}

#[test]
fn a_value_that_a_window_makes_is_what_its_control_says_and_one_that_is_not_a_value_of_the_plugin_is_ignored() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("clamped", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);

    // (The range of the value is 0 to 4: a number past it is kept to it, as the sound does.)
    the_window_says(
        &mut window,
        clip,
        &[(
            Duration::ZERO,
            WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 99.0 }),
        )],
    );
    assert_eq!(the_amount(&window, clip, kind), Some(PropertyValue::Number(4.0)));

    the_window_says(
        &mut window,
        clip,
        &[(
            Duration::ZERO,
            WindowEvent::Edit(ParameterEdit::Value { id: 77, value: 0.5 }),
        )],
    );
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(4.0)),
        "a parameter that the plugin has not is nothing"
    );
}

#[test]
fn a_window_ending_is_said_in_the_box_and_a_window_that_was_closed_is_not_a_problem() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("ending", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    let target = AnimationTarget::Clip(clip);

    the_window_says(&mut window, clip, &[(Duration::ZERO, WindowEvent::Closed)]);
    window.settle();
    assert!(
        window
            .application()
            .problem_with_a_plugin_window(target, EffectInstanceId::FIRST)
            .is_none()
    );

    the_window_says(
        &mut window,
        clip,
        &[(Duration::ZERO, WindowEvent::Failed("it crashed".to_owned()))],
    );
    window.settle();
    assert!(
        window.harness.query_by_label_contains("it crashed").is_some(),
        "the box says it"
    );
}

#[test]
fn a_folder_that_was_said_no_to_is_not_opened_and_the_box_says_so() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("declined", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    window
        .application_mut()
        .decline_the_plugin_folder_of_the_tests(PathBuf::from("/plugins/declined.clap"));

    window.do_this(Command::OpenPluginEditor {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::FIRST,
    });
    window.settle();

    assert!(window.harness.query_by_label_contains("was said no to").is_some());
}

// --- the state of the plugin, kept in the project (phase 10, piece 10.4) ---

/// The state that the effect of `a_clap_plugin` on the clip keeps, if it keeps one.
fn the_state_kept(window: &Window, clip: ClipId) -> Option<Vec<u8>> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .and_then(|chain| chain.instance(EffectInstanceId::FIRST))
        .and_then(|effect| effect.state().map(|state| state.to_vec()))
}

fn the_window_says_this_state(text: &[u8]) -> WindowEvent {
    WindowEvent::State(crate::base64::encode(text))
}

#[test]
fn a_state_that_a_window_says_is_kept_by_the_effect_and_is_one_step_with_the_edit_that_made_the_plugin_say_so() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("with_a_state", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    assert_eq!(the_state_kept(&window, clip), None);

    the_window_says(
        &mut window,
        clip,
        &[
            (
                Duration::from_secs(5),
                WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 2.5 }),
            ),
            (Duration::from_millis(5_300), WindowEvent::StateChanged),
            (
                Duration::from_millis(5_300),
                the_window_says_this_state(b"what the plugin saved"),
            ),
        ],
    );
    assert_eq!(the_state_kept(&window, clip), Some(b"what the plugin saved".to_vec()));
    assert_eq!(the_amount(&window, clip, kind), Some(PropertyValue::Number(2.5)));

    window.do_this(Command::Undo);
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(1.0)),
        "the value is back"
    );
    assert_eq!(
        the_state_kept(&window, clip),
        None,
        "and the state with it: they were one step"
    );
    window.do_this(Command::Redo);
    assert_eq!(the_state_kept(&window, clip), Some(b"what the plugin saved".to_vec()));
}

#[test]
fn a_state_that_nothing_made_the_plugin_say_is_a_step_of_its_own_and_a_state_that_the_effect_has_is_no_edit() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("alone_with_a_state", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);

    the_window_says(
        &mut window,
        clip,
        &[
            (Duration::from_secs(5), WindowEvent::StateChanged),
            (Duration::from_secs(5), the_window_says_this_state(b"a preset")),
            // (The same again, as a plugin says what it saved when its state changed: nothing is changed, and nothing is a step.)
            (Duration::from_secs(10), WindowEvent::StateChanged),
            (Duration::from_secs(10), the_window_says_this_state(b"a preset")),
        ],
    );
    assert_eq!(the_state_kept(&window, clip), Some(b"a preset".to_vec()));

    window.do_this(Command::Undo);
    assert_eq!(
        the_state_kept(&window, clip),
        None,
        "one step took the state: the second time it was said was no step"
    );
    assert!(
        window
            .application()
            .project()
            .effects_of(AnimationTarget::Clip(clip))
            .is_some_and(|chain| chain.instance(EffectInstanceId::FIRST).is_some()),
        "the effect is still there: only the state was undone"
    );
}

#[test]
fn a_state_that_is_too_big_for_a_project_is_not_kept_and_the_effect_goes_on_as_it_was() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("with_too_big_a_state", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    the_window_says(
        &mut window,
        clip,
        &[
            (Duration::ZERO, WindowEvent::StateChanged),
            (Duration::ZERO, the_window_says_this_state(b"small")),
        ],
    );

    let too_big = vec![0_u8; crate::project::MOST_BYTES_OF_A_PLUGIN_STATE + 1];
    the_window_says(
        &mut window,
        clip,
        &[
            (Duration::from_secs(5), WindowEvent::StateChanged),
            (Duration::from_secs(5), the_window_says_this_state(&too_big)),
        ],
    );

    assert_eq!(
        the_state_kept(&window, clip),
        Some(b"small".to_vec()),
        "what the effect had is kept"
    );
}

#[test]
fn a_state_that_nothing_said_had_changed_is_not_kept_and_so_leaves_what_can_be_redone_alone() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_clap_plugin("asked_not_changed", true);
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    the_window_says(
        &mut window,
        clip,
        &[(
            Duration::from_secs(5),
            WindowEvent::Edit(ParameterEdit::Value { id: 0, value: 2.5 }),
        )],
    );
    window.do_this(Command::Undo);
    assert_eq!(the_amount(&window, clip, kind), Some(PropertyValue::Number(1.0)));

    // (The window answers an asking of the program, or a value that the project told it: the plugin did not say that its state changed.)
    the_window_says(
        &mut window,
        clip,
        &[(Duration::from_secs(8), the_window_says_this_state(b"an answer"))],
    );

    assert_eq!(the_state_kept(&window, clip), None, "no person did that");
    window.do_this(Command::Redo);
    assert_eq!(
        the_amount(&window, clip, kind),
        Some(PropertyValue::Number(2.5)),
        "the redo is still there"
    );
}
