// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 19 of the project file added (phase 10, piece 10.4): the `state` of an effect, which its plugin saved, in base 64; and that a file of version 18, which has none, is
//! read as it was.

use std::sync::Arc;

use serde_json::{Value, json};

use super::tests::{project_with_everything, saved, version_line};
use super::*;
use crate::project::{
    AnimationTarget, EffectInstanceId, EffectKind, MOST_BYTES_OF_A_PLUGIN_STATE, set_installed_plugins,
};
use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// A project with a plugin effect on its first video, with this state, and the plugin installed (the guard keeps others from changing which plugins there are).
fn a_project_with_a_plugin_that_has_a_state(
    state: Option<&[u8]>,
) -> (Project, AnimationTarget, EffectInstanceId, EffectKind) {
    let plugin = an_installed_sound_plugin("clap:state_test:one", "Somebody", &[("amount", 1.0, Some((0.0, 4.0)))]);
    set_installed_plugins(vec![plugin]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let instance = put_an_effect_on(&mut project, video, kind, &[PropertyValue::Number(2.0)]);
    project
        .set_effect_state(video.into(), instance, state.map(Arc::<[u8]>::from))
        .unwrap();
    (project, video, instance, kind)
}

/// The first video of a project that was read (its clips are named afresh when it is: the names of the project that was saved are not its).
fn the_first_video_of(project: &Project) -> AnimationTarget {
    AnimationTarget::Clip(project.tracks()[2].entries()[0].id())
}

fn the_effect_of(project: &Project, target: AnimationTarget, instance: EffectInstanceId) -> &Effect {
    project.effects_of(target).unwrap().instance(instance).unwrap()
}

#[test]
fn the_state_of_an_effect_is_written_in_base_64_and_read_back_the_same_and_an_effect_with_none_has_no_field() {
    let _alone = the_installed_plugins_to_myself();
    let bytes: Vec<u8> = (0..=255).collect();
    let (project, _, instance, _) = a_project_with_a_plugin_that_has_a_state(Some(&bytes));

    let text = saved(&project);

    let file = as_json(&text);
    let effects = &file["tracks"][2]["clips"][0]["effects"];
    let the_plugin = effects
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["effect"] == "clap:state_test:one")
        .unwrap();
    assert_eq!(the_plugin["state"], json!(crate::base64::encode(&bytes)));
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const { assert!(FILE_FORMAT_VERSION >= 19, "the state of an effect came with format 19") };
    let loaded = project_from_json_text(&text).unwrap();
    let there = the_effect_of(&loaded, the_first_video_of(&loaded), instance);
    assert_eq!(there.state().map(|state| state.to_vec()), Some(bytes));
    assert_eq!(as_json(&saved(&loaded)), file, "nothing is lost, and nothing is added");

    let (without, _, _, _) = a_project_with_a_plugin_that_has_a_state(None);
    let file = as_json(&saved(&without));
    let effects = file["tracks"][2]["clips"][0]["effects"].as_array().unwrap();
    assert!(
        effects.iter().all(|effect| effect.get("state").is_none()),
        "an effect with no state has no field for it: {effects:?}"
    );
}

#[test]
fn a_file_of_version_18_has_no_state_is_read_as_it_was_and_saved_as_the_version_there_is() {
    let _alone = the_installed_plugins_to_myself();
    let (project, _, instance, _) = a_project_with_a_plugin_that_has_a_state(None);
    let text = saved(&project);
    let eighteen = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(18));

    let loaded = project_from_json_text(&eighteen).unwrap();

    assert!(
        the_effect_of(&loaded, the_first_video_of(&loaded), instance)
            .state()
            .is_none()
    );
    assert_eq!(as_json(&saved(&loaded)), as_json(&text));
}

#[test]
fn the_state_of_an_effect_of_a_plugin_that_is_not_here_is_kept_and_written_back_as_it_was() {
    let _alone = the_installed_plugins_to_myself();
    let (project, _, instance, _) = a_project_with_a_plugin_that_has_a_state(Some(b"what only the plugin reads"));
    let text = saved(&project);

    set_installed_plugins(Vec::new());
    let loaded = project_from_json_text(&text).unwrap();

    let there = the_effect_of(&loaded, the_first_video_of(&loaded), instance);
    assert!(!there.kind().is_installed());
    assert_eq!(
        there.state().map(|state| state.to_vec()),
        Some(b"what only the plugin reads".to_vec())
    );
    assert_eq!(as_json(&saved(&loaded)), as_json(&text));
}

#[test]
fn a_state_that_is_not_base_64_or_is_too_big_is_refused_and_says_which_effect() {
    let _alone = the_installed_plugins_to_myself();
    let (project, _, _, _) = a_project_with_a_plugin_that_has_a_state(Some(b"x"));
    let file_with = |state: String| {
        let mut file = as_json(&saved(&project));
        for effect in file["tracks"][2]["clips"][0]["effects"].as_array_mut().unwrap() {
            if effect["effect"] == "clap:state_test:one" {
                effect["state"] = json!(state);
            }
        }
        file.to_string()
    };

    let error = project_from_json_text(&file_with("this is !! not base 64".to_owned())).unwrap_err();
    assert!(error.to_string().contains("the state of the"), "{error}");

    let at_most = file_with(crate::base64::encode(&vec![0_u8; MOST_BYTES_OF_A_PLUGIN_STATE]));
    assert!(project_from_json_text(&at_most).is_ok(), "the biggest that is kept");
    let over = file_with(crate::base64::encode(&vec![0_u8; MOST_BYTES_OF_A_PLUGIN_STATE + 1]));
    let error = project_from_json_text(&over).unwrap_err();
    assert!(error.to_string().contains("keeps at most"), "{error}");
}
