// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Presets of plugins in the box of an effect (piece 11.7 of `docs/engine-phase-11-presets-design.md`): the *Presets* menu in the menu of the box, which is there only for a plugin that has
//! presets, what choosing a preset does to the values of the effect and to undo, and what is marked.

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::naming::what_an_effect_is_called;
use crate::project::{
    AnimationTarget, ClipId, EffectInstanceId, EffectKind, EffectPreset, PropertyId, PropertyValue, TimelineFrameIndex,
    set_installed_plugins,
};
use crate::test_support::{an_installed_sound_plugin, the_installed_plugins_to_myself};

fn number(value: f32) -> PropertyValue {
    PropertyValue::Number(value)
}

/// A plugin with three values (`amount`, `tone` and `mix`, from 0 to 4, beginning as 1, 0 and 2) that has these presets, as an installed kind of effect.
fn a_plugin_with_presets(presets: Vec<EffectPreset>) -> EffectKind {
    let mut plugin = an_installed_sound_plugin(
        "lv2:urn_test_presets",
        "Somebody",
        &[
            ("amount", 1.0, Some((0.0, 4.0))),
            ("tone", 0.0, Some((0.0, 4.0))),
            ("mix", 2.0, Some((0.0, 4.0))),
        ],
    );
    plugin.descriptor.name = "presets plugin".into();
    plugin.presets = presets;
    set_installed_plugins(vec![plugin]);
    EffectKind::installed_plugin_kinds()[0]
}

fn a_preset(label: &str, values: &[(usize, f32)]) -> EffectPreset {
    EffectPreset {
        label: label.to_owned(),
        values: values.iter().map(|(slot, value)| (*slot, number(*value))).collect(),
        left_out: 0,
        has_state: false,
    }
}

fn the_usual_presets() -> Vec<EffectPreset> {
    vec![
        a_preset("Quiet", &[(0, 0.25), (1, 3.0)]),
        a_preset("Loud", &[(0, 4.0), (2, 4.0)]),
    ]
}

/// A window with the video selected and the plugin on it: the window, the clip, and the heading of the box of the effect (what to right-click).
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

fn open_the_menu_of_the_box(window: &mut Window, kind: EffectKind) {
    let heading = window.rectangle_of(&what_an_effect_is_called(kind)).center();
    window.right_click(heading);
}

fn the_values(window: &Window, clip: ClipId) -> Vec<PropertyValue> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .instance(EffectInstanceId::FIRST)
        .unwrap()
        .values()
        .to_vec()
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

#[test]
fn the_menu_of_the_box_of_a_plugin_that_has_presets_has_a_presets_item_and_one_that_has_none_has_not_and_nor_has_an_effect_of_the_program()
 {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_plugin_with_presets(the_usual_presets());
    let (mut window, _) = window_with_the_plugin_on_the_clip(kind);
    open_the_menu_of_the_box(&mut window, kind);
    assert!(is_shown(&window, "Presets \u{23f5}"), "a plugin that has presets");

    let kind = a_plugin_with_presets(Vec::new());
    let (mut window, _) = window_with_the_plugin_on_the_clip(kind);
    open_the_menu_of_the_box(&mut window, kind);
    assert!(is_shown(&window, "Delete effect"), "the menu is open");
    assert!(!is_shown(&window, "Presets \u{23f5}"), "a plugin that has none");

    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();
    open_the_menu_of_the_box(&mut window, EffectKind::HUE_SHIFT);
    assert!(is_shown(&window, "Delete effect"));
    assert!(!is_shown(&window, "Presets \u{23f5}"), "an effect of the program");
}

#[test]
fn choosing_a_preset_in_the_menu_sets_the_values_it_has_and_leaves_the_others() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_plugin_with_presets(the_usual_presets());
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    assert_eq!(the_values(&window, clip), vec![number(1.0), number(0.0), number(2.0)]);

    open_the_menu_of_the_box(&mut window, kind);
    window.open_the_smaller_menu("Presets");
    assert!(
        is_shown(&window, "Quiet") && is_shown(&window, "Loud"),
        "the presets are listed"
    );
    assert!(
        window
            .harness
            .query_by_label_contains("A value that has keyframes is kept")
            .is_some(),
        "and the menu says what is done with a value that has keyframes"
    );
    let quiet = window.rectangle_of("Quiet").center();
    window.click(quiet);

    assert_eq!(
        the_values(&window, clip),
        vec![number(0.25), number(3.0), number(2.0)],
        "the value that the preset does not have is as it was"
    );
}

#[test]
fn a_preset_is_one_step_of_undo_and_the_one_that_the_values_are_is_marked_in_the_menu() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_plugin_with_presets(the_usual_presets());
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);

    window.do_this(Command::ApplyEffectPreset {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::FIRST,
        preset: 1,
    });

    assert_eq!(the_values(&window, clip), vec![number(4.0), number(0.0), number(4.0)]);
    open_the_menu_of_the_box(&mut window, kind);
    window.open_the_smaller_menu("Presets");
    assert!(is_shown(&window, "\u{2713} Loud"), "the preset that is set is marked");
    assert!(is_shown(&window, "Quiet"), "and the other is not");

    window.do_this(Command::Undo);

    assert_eq!(
        the_values(&window, clip),
        vec![number(1.0), number(0.0), number(2.0)],
        "both values come back with one undo"
    );
}

#[test]
fn a_value_that_has_keyframes_is_left_as_it_is_when_a_preset_is_chosen_and_the_others_are_set() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_plugin_with_presets(the_usual_presets());
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    let target = AnimationTarget::Clip(clip);
    let amount = PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0);
    window.do_this(Command::AddKeyframe {
        target,
        property: amount,
        frame: TimelineFrameIndex::new(3),
    });

    window.do_this(Command::ApplyEffectPreset {
        target,
        instance: EffectInstanceId::FIRST,
        preset: 1,
    });

    let project = window.application().project();
    assert!(
        project.is_animated(target, amount),
        "the keyframes of the amount are there"
    );
    assert_eq!(
        project
            .effects_of(target)
            .unwrap()
            .instance(EffectInstanceId::FIRST)
            .unwrap()
            .values()[2],
        number(4.0),
        "the value that has none was set"
    );
}

#[test]
fn a_preset_that_also_has_a_state_says_that_only_its_values_are_used() {
    let _alone = the_installed_plugins_to_myself();
    let mut with_a_state = a_preset("Remembers", &[(0, 3.0)]);
    with_a_state.has_state = true;
    let kind = a_plugin_with_presets(vec![with_a_state]);
    let (mut window, _) = window_with_the_plugin_on_the_clip(kind);

    open_the_menu_of_the_box(&mut window, kind);
    window.open_the_smaller_menu("Presets");

    assert!(is_shown(&window, "Remembers (values only)"));
}

#[test]
fn a_preset_that_is_not_there_or_for_an_effect_that_is_not_there_changes_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let kind = a_plugin_with_presets(the_usual_presets());
    let (mut window, clip) = window_with_the_plugin_on_the_clip(kind);
    let before = the_values(&window, clip);

    window.do_this(Command::ApplyEffectPreset {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::FIRST,
        preset: 7,
    });
    window.do_this(Command::ApplyEffectPreset {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::new(9),
        preset: 0,
    });

    assert_eq!(the_values(&window, clip), before);
}
