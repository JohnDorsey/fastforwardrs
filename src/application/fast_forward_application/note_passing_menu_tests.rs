// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch "pass notes on" of a synthesizer in the menu of its box (phase 13, piece 13.5; docs/engine-phase-13-midi-design.md): it is in the menu of a synthesizer only, off to begin with, choosing
//! it is one step of undo, and the box says so while it is on and says nothing while it is off.
//!
//! What the items are called is written out in words here, and not taken from the constants of the code, so that these tests pin what the person sees.

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::naming::what_an_effect_is_called;
use crate::project::{AnimationTarget, EffectInstanceId, EffectKind, set_installed_plugins};
use crate::test_support::{an_installed_synthesizer, the_installed_plugins_to_myself};

fn is_shown(window: &Window, words: &str) -> bool {
    window.harness.query_by_label_contains(words).is_some()
}

fn passes_notes_on(window: &Window, clip: crate::project::ClipId) -> bool {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .instance(EffectInstanceId::FIRST)
        .unwrap()
        .passes_notes_on()
}

#[test]
fn the_menu_of_the_box_of_a_synthesizer_has_pass_notes_on_off_at_first_and_choosing_it_is_one_step_of_undo_and_the_box_says_so_while_it_is_on()
 {
    let _alone = the_installed_plugins_to_myself();
    set_installed_plugins(vec![an_installed_synthesizer("clap:com.example.synth")]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind,
    });
    window.settle();
    assert!(
        !is_shown(&window, "Passes the notes"),
        "the box says nothing while it is off"
    );
    let heading = window.rectangle_of(&what_an_effect_is_called(kind)).center();
    window.right_click(heading);
    assert!(is_shown(&window, "Pass notes on"));
    assert!(!is_shown(&window, "\u{2713} Pass notes on"), "off: not marked");

    window.harness.get_by_label_contains("Pass notes on").click();
    window.settle();

    assert!(passes_notes_on(&window, clip));
    assert!(is_shown(&window, "Passes the notes on to the next synthesizer"));
    undo(&mut window);
    assert!(!passes_notes_on(&window, clip));
    assert!(!is_shown(&window, "Passes the notes on to the next synthesizer"));
    redo(&mut window);
    assert!(passes_notes_on(&window, clip));
    let heading = window.rectangle_of(&what_an_effect_is_called(kind)).center();
    window.right_click(heading);
    assert!(is_shown(&window, "\u{2713} Pass notes on"), "on: marked");
    window.harness.get_by_label_contains("Pass notes on").click();
    window.settle();
    assert!(!passes_notes_on(&window, clip), "chosen again, it is off again");
}

#[test]
fn an_effect_that_is_not_a_synthesizer_has_no_such_item_in_its_menu() {
    let _alone = the_installed_plugins_to_myself();
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();

    let heading = window
        .rectangle_of(&what_an_effect_is_called(EffectKind::HUE_SHIFT))
        .center();
    window.right_click(heading);

    assert!(is_shown(&window, "Duplicate effect"), "the menu is open");
    assert!(!is_shown(&window, "Pass notes on"));
}
