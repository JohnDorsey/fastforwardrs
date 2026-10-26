// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The state that a plugin saved, kept with its effect (phase 10, piece 10.4): who can have one, what keeps it and what takes it away.

use std::sync::Arc;

use super::tests::{frame, video_source};
use super::*;
use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

fn bytes(text: &str) -> Option<Arc<[u8]>> {
    Some(Arc::from(text.as_bytes()))
}

/// A project with a video, and a plugin effect (with one value) on it; the plugin is installed.
fn a_video_with_a_plugin_effect() -> (Project, AnimationTarget, EffectInstanceId, EffectKind) {
    set_installed_plugins(vec![an_installed_sound_plugin(
        "clap:state_model:one",
        "Somebody",
        &[("amount", 1.0, Some((0.0, 4.0)))],
    )]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = Project::empty();
    let video = project
        .import_source(video_source(1, 30, 600), frame(0))
        .unwrap()
        .clip
        .unwrap();
    let target = AnimationTarget::Clip(video);
    let instance = put_an_effect_on(&mut project, target, kind, &[PropertyValue::Number(2.0)]);
    (project, target, instance, kind)
}

fn the_state(project: &Project, target: AnimationTarget, instance: EffectInstanceId) -> Option<Vec<u8>> {
    project
        .effects_of(target)
        .unwrap()
        .instance(instance)
        .unwrap()
        .state()
        .map(|state| state.to_vec())
}

#[test]
fn an_effect_has_no_state_until_it_is_given_one_and_none_again_when_it_is_taken_away() {
    let _alone = the_installed_plugins_to_myself();
    let (mut project, target, instance, _) = a_video_with_a_plugin_effect();
    assert_eq!(the_state(&project, target, instance), None);

    project.set_effect_state(target.into(), instance, bytes("one")).unwrap();
    assert_eq!(the_state(&project, target, instance), Some(b"one".to_vec()));

    project.set_effect_state(target.into(), instance, bytes("two")).unwrap();
    assert_eq!(the_state(&project, target, instance), Some(b"two".to_vec()), "replaced");

    project.set_effect_state(target.into(), instance, None).unwrap();
    assert_eq!(the_state(&project, target, instance), None);
}

#[test]
fn a_state_is_kept_by_what_changes_the_values_and_is_copied_with_the_effect_and_is_not_the_values() {
    let _alone = the_installed_plugins_to_myself();
    let (mut project, target, instance, kind) = a_video_with_a_plugin_effect();
    project
        .set_effect_state(target.into(), instance, bytes("kept"))
        .unwrap();
    let amount = PropertyId::effect_value(kind, instance, 0);

    project
        .set_property_at(target, amount, PropertyValue::Number(3.0), TimelineFrameIndex::new(0))
        .unwrap();
    assert_eq!(
        the_state(&project, target, instance),
        Some(b"kept".to_vec()),
        "a value changed"
    );

    let copy = project.duplicate_effect(target.into(), instance).unwrap();
    assert_eq!(
        the_state(&project, target, copy),
        Some(b"kept".to_vec()),
        "the copy has it"
    );
    project
        .set_effect_state(target.into(), copy, bytes("the copy's own"))
        .unwrap();
    assert_eq!(
        the_state(&project, target, instance),
        Some(b"kept".to_vec()),
        "the state of the copy is its own"
    );

    assert!(
        Effect::new(kind) != Effect::new(kind).with_state(bytes("x")),
        "an effect with a state is another effect than one without"
    );
}

#[test]
fn the_fingerprint_of_a_state_is_the_same_for_the_same_bytes_and_another_for_others_and_for_none_whichever_way_it_is_given()
 {
    let _alone = the_installed_plugins_to_myself();
    let (_, _, _, kind) = a_video_with_a_plugin_effect();
    let none = Effect::new(kind).state_fingerprint();
    let one = Effect::new(kind).with_state(bytes("one")).state_fingerprint();
    let two = Effect::new(kind).with_state(bytes("two")).state_fingerprint();

    assert_ne!(one, none);
    assert_ne!(one, two);
    assert_eq!(Effect::new(kind).with_state(bytes("one")).state_fingerprint(), one);
    assert_eq!(
        Effect::new(kind)
            .with_state(bytes("one"))
            .with_state(None)
            .state_fingerprint(),
        none,
        "a state that is taken away"
    );
}

#[test]
fn a_state_goes_to_what_has_the_effect_and_nothing_else_and_as_big_a_state_as_a_plugin_makes_is_kept() {
    let _alone = the_installed_plugins_to_myself();
    let (mut project, target, instance, _) = a_video_with_a_plugin_effect();

    assert_eq!(
        project.set_effect_state(target.into(), EffectInstanceId::new(9), bytes("x")),
        Err(EditError::NotInTheChain {
            owner: target.into(),
            item: ChainItemKey::Effect(EffectInstanceId::new(9))
        }),
        "no such effect"
    );
    assert_eq!(the_state(&project, target, instance), None, "nothing was changed");
    // (There was a limit of 16 MiB until 2026-10-26, and a sampler's state is bigger: nothing is refused for its size.)
    let big: Arc<[u8]> = vec![7_u8; 40 * 1024 * 1024].into();
    project
        .set_effect_state(target.into(), instance, Some(Arc::clone(&big)))
        .unwrap();
    assert_eq!(
        the_state(&project, target, instance).map(|state| state.len()),
        Some(big.len())
    );
}
