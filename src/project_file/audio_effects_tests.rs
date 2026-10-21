// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects of a clip that is made from a sound file. They were put on in the window and heard, and were **not written to the project file** (the record of such a clip had no place for
//! them), so a plugin put on the first clip of a song was gone when the project was loaded again, without a word. Found with a `.ffrs` file of the maintainer's that had the snapshot of a
//! reverb among its `plugins` and no clip that used it.

use super::tests::{project_with_everything, saved};
use super::*;
use crate::project::{AnimationTarget, ClipId, EffectKind, PropertyId, PropertyValue};
use crate::test_support::{an_installed_sound_plugin, the_installed_plugins_to_myself};

/// The clip of the project that is made from a sound file.
fn the_sound_clip_of(project: &Project) -> ClipId {
    project
        .tracks()
        .iter()
        .flat_map(|track| track.entries())
        .find(|entry| matches!(entry.clip(), Clip::Audio(_)))
        .expect("the project has a clip made from a sound file")
        .id()
}

/// A reverb that is installed (made up: a sound effect with a mix and a size), so that the effect is of a type that the program has.
fn the_reverb_kind() -> EffectKind {
    let plugin = an_installed_sound_plugin(
        "clap:test.reverb",
        "Maker",
        &[("mix", 0.5, Some((0.0, 1.0))), ("size", 20.0, Some((10.0, 60.0)))],
    );
    crate::project::set_installed_plugins(vec![plugin]);
    EffectKind::installed_plugin_kinds()[0]
}

fn a_project_with_a_reverb_on_its_sound_clip() -> (Project, ClipId, EffectKind) {
    let kind = the_reverb_kind();
    let mut project = project_with_everything();
    let clip = the_sound_clip_of(&project);
    let instance = project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    project
        .set_property_at(
            AnimationTarget::Clip(clip),
            PropertyId::effect_value(kind, instance, 0),
            PropertyValue::Number(0.8),
            crate::project::TimelineFrameIndex::new(0),
        )
        .unwrap();
    (project, clip, kind)
}

#[test]
fn an_effect_on_a_clip_made_from_a_sound_file_is_in_the_file_and_is_there_when_the_file_is_loaded() {
    let _alone = the_installed_plugins_to_myself();
    let (project, _, kind) = a_project_with_a_reverb_on_its_sound_clip();

    let text = saved(&project);
    let loaded = project_from_json_text(&text).expect("the file is read");

    let clip = the_sound_clip_of(&loaded);
    let effects = loaded
        .effects_of(AnimationTarget::Clip(clip))
        .expect("the clip is there");
    let found: Vec<(EffectKind, Option<PropertyValue>)> = effects
        .iter()
        .map(|effect| (effect.kind(), effect.values().first().cloned()))
        .collect();
    assert_eq!(
        found,
        [(kind, Some(PropertyValue::Number(0.8)))],
        "the effect, with the value it was given"
    );
    // (And the file says so where the clip is: not only among the types of effect in `plugins`, where it was found.)
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let in_the_file = file["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|track| track["clips"].as_array().unwrap())
        .find(|clip| clip["kind"] == "audio")
        .unwrap()["effects"]
        .clone();
    assert_eq!(in_the_file[0]["effect"], "clap:test.reverb");
    assert_eq!(in_the_file[0]["values"]["mix"], 0.8);
}

#[test]
fn a_sound_clip_that_has_no_effects_has_no_effects_in_the_file_and_a_file_of_before_reads_as_before() {
    let _alone = the_installed_plugins_to_myself();
    let project = project_with_everything();

    let text = saved(&project);
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let audio = file["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|track| track["clips"].as_array().unwrap())
        .find(|clip| clip["kind"] == "audio")
        .unwrap()
        .clone();

    assert!(
        audio.get("effects").is_none(),
        "left out, as it is for the others that have none: {audio}"
    );
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(
        loaded
            .effects_of(AnimationTarget::Clip(the_sound_clip_of(&loaded)))
            .unwrap()
            .iter()
            .count(),
        0
    );
}

#[test]
fn a_project_with_an_effect_on_a_sound_clip_is_written_again_as_it_was() {
    let _alone = the_installed_plugins_to_myself();
    let (project, ..) = a_project_with_a_reverb_on_its_sound_clip();

    let first = saved(&project);
    let second = saved(&project_from_json_text(&first).unwrap());

    assert_eq!(first, second);
}

#[test]
fn a_picture_effect_on_a_sound_clip_is_refused_and_a_sound_effect_of_a_plugin_that_is_not_installed_is_kept() {
    let _alone = the_installed_plugins_to_myself();
    let (project, ..) = a_project_with_a_reverb_on_its_sound_clip();
    let text = saved(&project);

    // A picture effect, by hand, on the sound clip: what the program would never write, and does not make up.
    let mut file: serde_json::Value = serde_json::from_str(&text).unwrap();
    for track in file["tracks"].as_array_mut().unwrap() {
        for clip in track["clips"].as_array_mut().unwrap() {
            if clip["kind"] == "audio" {
                clip["effects"] =
                    serde_json::json!([{"effect": "builtin:hue_shift", "version": [1, 1], "instance": 0}]);
            }
        }
    }
    let refused = project_from_json_text(&file.to_string());
    assert!(refused.is_err(), "a hue shift cannot be on a sound");

    // The plugin gone from the list of what is installed: the effect is kept whole (a placeholder that says so), and the file is written back as it was.
    crate::project::set_installed_plugins(Vec::new());
    let loaded = project_from_json_text(&text).expect("the file is read without the plugin");
    let clip = the_sound_clip_of(&loaded);
    assert_eq!(
        loaded.effects_of(AnimationTarget::Clip(clip)).unwrap().iter().count(),
        1,
        "kept"
    );
    assert_eq!(saved(&loaded), text, "written back as it was");
}
