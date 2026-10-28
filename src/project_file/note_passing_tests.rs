// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 22 of the project file added (phase 13, piece 13.5; docs/engine-phase-13-midi-design.md): `passes_notes_on` in the document of an effect, for a synthesizer that passes the notes
//! that reach it on, left out when it does not; and that a file of version 21, which has none, is read as a project where every synthesizer consumes the notes.

use serde_json::Value;

use super::tests::{edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{EffectKind, Member, set_installed_plugins};
use crate::test_support::{an_installed_synthesizer, put_an_effect_on, the_installed_plugins_to_myself};

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// A project (see `edited_project`) that has a synthesizer, installed, among the sound effects of its first track; and the instance, and the owner.
fn a_project_with_a_synthesizer() -> (Project, Member, crate::project::EffectInstanceId) {
    set_installed_plugins(vec![an_installed_synthesizer("clap:com.example.synth")]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = edited_project(link_to_missing_video());
    let track = project.tracks()[0].id();
    let instance = put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(1.5)],
    );
    (project, Member::Track(track), instance)
}

/// The documents of the effects of the first track.
fn the_effects_of_the_first_track(file: &Value) -> Vec<Value> {
    file["tracks"][0]["effects"].as_array().cloned().unwrap_or_default()
}

#[test]
fn a_synthesizer_that_consumes_the_notes_does_not_say_so_in_the_file_and_one_that_passes_them_on_does() {
    let _alone = the_installed_plugins_to_myself();
    let (mut project, owner, instance) = a_project_with_a_synthesizer();

    let consuming = as_json(&saved(&project));
    project.set_effect_passes_notes_on(owner, instance, true).unwrap();
    let passing = as_json(&saved(&project));

    let before = &the_effects_of_the_first_track(&consuming)[0];
    let after = &the_effects_of_the_first_track(&passing)[0];
    assert!(before.get("passes_notes_on").is_none(), "{before}");
    assert_eq!(after["passes_notes_on"], true);
}

#[test]
fn the_switch_is_read_back_as_it_was_where_the_plugin_is_gone_too_and_is_saved_again_as_it_was() {
    let _alone = the_installed_plugins_to_myself();
    let (mut project, owner, instance) = a_project_with_a_synthesizer();
    project.set_effect_passes_notes_on(owner, instance, true).unwrap();
    let text = saved(&project);

    let with_the_plugin = project_from_json_text(&text).unwrap();
    set_installed_plugins(Vec::new());
    let without = project_from_json_text(&text).unwrap();

    for loaded in [&with_the_plugin, &without] {
        let effect = loaded
            .chain_of_the_member(owner)
            .and_then(|chain| chain.instance(instance))
            .expect("the effect is there");
        assert!(effect.passes_notes_on());
        assert_eq!(saved(loaded), text, "nothing is lost");
    }
    assert!(
        !without
            .chain_of_the_member(owner)
            .unwrap()
            .instance(instance)
            .unwrap()
            .kind()
            .is_installed()
    );
}

#[test]
fn a_file_of_version_21_has_no_switch_and_its_synthesizers_consume_the_notes() {
    let _alone = the_installed_plugins_to_myself();
    let (project, owner, instance) = a_project_with_a_synthesizer();
    let text = saved(&project);
    let twenty_one = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(21));

    let loaded = project_from_json_text(&twenty_one).unwrap();

    let effect = loaded.chain_of_the_member(owner).unwrap().instance(instance).unwrap();
    assert!(!effect.passes_notes_on());
    assert_eq!(saved(&loaded), text);
}
