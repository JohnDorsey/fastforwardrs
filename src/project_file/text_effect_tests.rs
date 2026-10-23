// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text effect in a project file (phase 9.1): its snapshot says the controls that a text needs by name, and a file that has it is given the effect of the program.

use serde_json::Value;

use super::tests::{assert_same_content, saved};
use super::*;
use crate::project::{AnimationTarget, EffectKind, ParameterControl, TextObject, TimelineFrameIndex};

/// A project with a text on its track that has the text effect on it too, which only a test does (nothing offers it; a text is made of it from 9.2).
fn a_text_that_has_the_text_effect() -> (Project, AnimationTarget) {
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    let (clip, _) = project
        .insert_text(track, TimelineFrameIndex::new(0), TextObject::default(), 60)
        .unwrap();
    let target = AnimationTarget::Clip(clip);
    project.add_effect(target, EffectKind::TEXT).unwrap();
    (project, target)
}

#[test]
fn the_snapshot_of_the_text_effect_names_its_controls_and_the_effect_comes_back_as_the_effect_of_the_program() {
    let (project, target) = a_text_that_has_the_text_effect();
    let text = saved(&project);
    let document: Value = serde_json::from_str(&text).unwrap();
    let snapshot = document["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|snapshot| snapshot["key"] == "builtin:text")
        .expect("the file has a snapshot of every type of effect that it uses");
    let params = snapshot["params"].as_array().unwrap();
    let ids: Vec<&str> = params.iter().map(|param| param["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["text", "font", "size", "colour"]);
    assert_eq!(params[0]["control"], "multiline");
    assert_eq!(params[1]["control"], "font");
    assert_eq!(params[2]["control"], "points_of_the_picture_height");
    assert!(
        params[3].get("control").is_none(),
        "the colour has the control that its kind has, which is not said"
    );
    assert_eq!(params[2]["range"], serde_json::json!([0.01, 0.5]));
    assert_eq!(params[2]["unit"], "pt");

    let loaded = project_from_json_text(&text).unwrap();
    assert_same_content(&loaded, &project);
    let clip = loaded.tracks()[0].entries()[0].id();
    let kind = loaded
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .expect("the effect is there")
        .kind();
    assert_eq!(
        kind,
        EffectKind::TEXT,
        "it is the effect of the program, and not one that a file brought"
    );
    let _ = target;
}

#[test]
fn a_file_that_gives_the_text_effect_to_something_that_is_not_a_text_is_refused() {
    let (project, _) = a_text_that_has_the_text_effect();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    // The effect is moved from the text to the track that it is on.
    let effects = document["tracks"][0]["clips"][0]
        .as_object_mut()
        .unwrap()
        .remove("effects")
        .expect("the text has the effect");
    document["tracks"][0]["effects"] = effects;
    let refused = project_from_json_text(&serde_json::to_string(&document).unwrap())
        .expect_err("a track cannot have the text effect");
    assert!(refused.to_string().contains("cannot have the text effect"), "{refused}");
}

#[test]
fn a_snapshot_of_another_effect_that_has_these_controls_is_read_with_them_and_written_as_it_was() {
    let (project, _) = a_text_that_has_the_text_effect();
    let text = saved(&project).replace("builtin:text", "isf:like_a_text");
    let loaded = project_from_json_text(&text).unwrap();
    let clip = loaded.tracks()[0].entries()[0].id();
    let kind = loaded
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .expect("the effect is there")
        .kind();
    assert_ne!(
        kind,
        EffectKind::TEXT,
        "it is not the effect of the program: it has another key"
    );
    let controls: Vec<ParameterControl> = kind
        .descriptor()
        .parameters
        .iter()
        .map(|parameter| parameter.control.clone())
        .collect();
    assert_eq!(
        controls,
        [
            ParameterControl::Multiline,
            ParameterControl::Font,
            ParameterControl::PointsOfThePictureHeight,
            ParameterControl::Automatic,
        ],
        "read by the names that the file gives them, and not kept as what this version does not know"
    );
    assert_eq!(saved(&loaded), text, "and written back as it was");
}
