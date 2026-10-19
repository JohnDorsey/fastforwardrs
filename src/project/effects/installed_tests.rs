// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins that are installed, as types of effect: when a snapshot in a project is one of them, when it is not, and what changes while the program runs.

use std::path::PathBuf;

use super::*;
use crate::project::ClipKind;
use crate::project::PropertyValue;
use crate::test_support::{
    an_installed_sound_plugin, the_descriptor_of_an_effect_the_program_does_not_have, the_installed_plugins_to_myself,
};

fn snapshot_of(plugin: &InstalledPlugin) -> EffectDescriptor {
    plugin.descriptor.clone()
}

#[test]
fn nothing_is_installed_to_begin_with_and_what_is_put_in_the_list_is_in_it_in_order() {
    let _alone = the_installed_plugins_to_myself();
    assert!(installed_plugins().is_empty());
    let first = an_installed_sound_plugin("ladspa:a:first", "Maker A", &[("gain", 1.0, Some((0.0, 4.0)))]);
    let second = an_installed_sound_plugin("ladspa:a:second", "Maker B", &[]);
    set_installed_plugins(vec![first.clone(), second.clone()]);
    let keys: Vec<String> = installed_plugins()
        .iter()
        .map(|plugin| plugin.descriptor.key.to_string())
        .collect();
    assert_eq!(keys, ["ladspa:a:first", "ladspa:a:second"]);
    assert_eq!(*installed_plugins()[0], first);
    set_installed_plugins(Vec::new());
    assert!(installed_plugins().is_empty());
}

#[test]
fn the_same_plugin_put_in_the_list_again_is_the_same_kind_of_effect() {
    let _alone = the_installed_plugins_to_myself();
    let plugin = an_installed_sound_plugin("ladspa:a:same", "Maker", &[("gain", 1.0, None)]);
    set_installed_plugins(vec![plugin.clone()]);
    let before = EffectKind::installed_plugin_kinds();
    set_installed_plugins(Vec::new());
    set_installed_plugins(vec![plugin]);
    assert_eq!(
        EffectKind::installed_plugin_kinds(),
        before,
        "found again, it is what it was"
    );
}

#[test]
fn a_snapshot_that_a_plugin_can_stand_in_for_is_that_plugin_and_is_installed_for_as_long_as_the_plugin_is() {
    let _alone = the_installed_plugins_to_myself();
    let plugin = an_installed_sound_plugin("ladspa:a:stands_in", "Maker", &[("gain", 1.0, None)]);
    // A project that was opened before the plugins were found: its effect is of a kind the program does not have.
    let before_the_scan = EffectKind::from_snapshot(snapshot_of(&plugin)).unwrap();
    assert!(!before_the_scan.is_installed());
    assert!(before_the_scan.installed_plugin().is_none());

    set_installed_plugins(vec![plugin.clone()]);
    assert!(
        before_the_scan.is_installed(),
        "the plugin was found: the effect works now, as it is"
    );
    assert_eq!(
        before_the_scan.installed_plugin().map(|found| found.library.clone()),
        Some(plugin.library.clone())
    );
    // A project that is opened after is given the kind of the plugin itself.
    let after_the_scan = EffectKind::from_snapshot(snapshot_of(&plugin)).unwrap();
    assert!(after_the_scan.is_installed());
    assert_eq!(after_the_scan.descriptor(), &plugin.descriptor);
    assert_eq!(
        after_the_scan,
        EffectKind::installed_plugin_kinds()[0],
        "the kind is the plugin's own, not one that only looks like it"
    );
    assert_ne!(
        before_the_scan, after_the_scan,
        "(the one that was made before the scan is the file's: another kind of the same effect)"
    );

    set_installed_plugins(Vec::new());
    assert!(
        !after_the_scan.is_installed(),
        "the plugin is gone: the effect is kept, and is not installed"
    );
    assert!(!before_the_scan.is_installed());
}

#[test]
fn a_snapshot_of_a_plugin_that_has_other_values_or_another_major_version_is_not_the_plugin_that_is_installed() {
    let _alone = the_installed_plugins_to_myself();
    let plugin = an_installed_sound_plugin("ladspa:a:changed", "Maker", &[("gain", 1.0, None)]);
    set_installed_plugins(vec![plugin.clone()]);

    let mut with_another_value = snapshot_of(&plugin);
    with_another_value.parameters = the_descriptor_of_an_effect_the_program_does_not_have(
        "ladspa:a:changed",
        &[
            ("gain", PropertyValue::Number(1.0)),
            ("extra", PropertyValue::Number(0.0)),
        ],
    )
    .parameters;
    let kind = EffectKind::from_snapshot(with_another_value).unwrap();
    assert!(!kind.is_installed(), "it has a value that the plugin has not");

    let mut with_another_kind_of_value = snapshot_of(&plugin);
    with_another_kind_of_value.parameters = the_descriptor_of_an_effect_the_program_does_not_have(
        "ladspa:a:changed",
        &[("gain", PropertyValue::Flag(true))],
    )
    .parameters;
    assert!(
        !EffectKind::from_snapshot(with_another_kind_of_value)
            .unwrap()
            .is_installed()
    );

    let mut another_major_version = snapshot_of(&plugin);
    another_major_version.version = (2, 0);
    assert!(!EffectKind::from_snapshot(another_major_version).unwrap().is_installed());
}

#[test]
fn what_is_offered_is_what_the_program_has_and_then_the_plugins_in_the_order_they_were_found() {
    let _alone = the_installed_plugins_to_myself();
    assert_eq!(EffectKind::offered(), EffectKind::BUILT_IN.to_vec());
    let one = an_installed_sound_plugin("ladspa:a:one", "Maker", &[]);
    let two = an_installed_sound_plugin("ladspa:a:two", "Maker", &[]);
    set_installed_plugins(vec![two, one]);
    let offered = EffectKind::offered();
    assert_eq!(&offered[..EffectKind::BUILT_IN.len()], EffectKind::BUILT_IN);
    let keys: Vec<&str> = offered[EffectKind::BUILT_IN.len()..]
        .iter()
        .map(|kind| kind.key())
        .collect();
    assert_eq!(keys, ["ladspa:a:two", "ladspa:a:one"]);
}

#[test]
fn of_two_plugins_with_one_key_the_first_in_the_list_is_the_one_that_stands_in() {
    let _alone = the_installed_plugins_to_myself();
    let mut first = an_installed_sound_plugin("ladspa:a:twice", "Maker", &[("gain", 1.0, None)]);
    first.library = PathBuf::from("/mine/twice.so");
    let mut second = first.clone();
    second.library = PathBuf::from("/usr/lib/ladspa/twice.so");
    set_installed_plugins(vec![first.clone(), second]);
    let found = installed_plugin_for(&first.descriptor).unwrap();
    assert_eq!(found.library, PathBuf::from("/mine/twice.so"));
}

#[test]
fn a_sound_plugin_can_be_put_on_a_sound_and_on_a_video_and_on_nothing_seen() {
    let _alone = the_installed_plugins_to_myself();
    set_installed_plugins(vec![an_installed_sound_plugin("ladspa:a:where", "Maker", &[])]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    assert!(kind.applies_to_a_clip_of_kind(ClipKind::Audio));
    assert!(kind.applies_to_a_clip_of_kind(ClipKind::Video));
    assert!(!kind.applies_to_a_clip_of_kind(ClipKind::Image));
    assert!(!kind.applies_to_a_clip_of_kind(ClipKind::Text));
    assert!(kind.applies_to_a_track());
}
