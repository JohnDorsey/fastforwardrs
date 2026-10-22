// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shader of an effect in a project file (phase 6, piece 6.3; format 16): a snapshot of an effect that is a shader carries its language and its whole text, a project is read with
//! it, a shader that this version cannot run or does not know is kept as it was, and a file of version 15 has none.

use serde_json::{Value, json};

use super::tests::{assert_same_content, edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{EffectKind, MOST_BYTES_OF_A_SHADER};

const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage) * amount; }\n";

fn kind_of(name: &str, source: &str) -> EffectKind {
    let file = crate::isf::read(source).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, name, source)).unwrap()
}

fn a_project_with_a_shader() -> (Project, EffectKind) {
    let mut project = edited_project(link_to_missing_video());
    let kind = kind_of("tint.fs", TINT);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    (project, kind)
}

fn the_snapshot_of<'a>(document: &'a mut Value, key_begins: &str) -> &'a mut Value {
    document["plugins"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|snapshot| snapshot["key"].as_str().is_some_and(|key| key.starts_with(key_begins)))
        .expect("the snapshot is in the file")
}

#[test]
fn the_snapshot_of_a_shader_carries_its_language_and_its_whole_text() {
    let (project, kind) = a_project_with_a_shader();

    let text = saved(&project);

    let mut document: Value = serde_json::from_str(&text).unwrap();
    let snapshot = the_snapshot_of(&mut document, "isf:tint-");
    assert_eq!(snapshot["key"], kind.key());
    assert_eq!(snapshot["shader"], json!({ "language": "isf", "source": TINT }));
    assert_eq!(snapshot["params"][0]["id"], "amount");
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const { assert!(FILE_FORMAT_VERSION >= 16, "the shader of an effect came with format 16") };
}

#[test]
fn a_project_with_a_shader_is_read_back_with_the_same_kind_of_effect_and_the_same_values() {
    let (project, kind) = a_project_with_a_shader();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    let effect = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .next()
        .unwrap()
        .clone();
    assert_eq!(effect.kind(), kind, "the same kind, not another that looks like it");
    assert_eq!(effect.kind().shader_source().unwrap().text, TINT);
    assert!(
        effect.kind().is_installed(),
        "{:?}",
        crate::isf::why_not_usable(effect.kind().descriptor())
    );
    assert_eq!(effect.values(), [PropertyValue::Number(0.25)]);
}

#[test]
fn a_shader_whose_text_does_not_compile_is_kept_with_its_values_and_does_nothing() {
    let (project, _) = a_project_with_a_shader();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    // A file that was edited by hand: the text of the shader is not one.
    the_snapshot_of(&mut document, "isf:tint-")["shader"]["source"] = json!("void main() {}\n");

    let loaded = project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()).unwrap();

    let effect = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .next()
        .unwrap()
        .clone();
    assert!(!effect.kind().is_installed());
    assert_eq!(effect.values(), [PropertyValue::Number(0.25)], "the values are kept");
    // And it is written back as it was.
    let again: Value = serde_json::from_str(&saved(&loaded)).unwrap();
    let mut again = again;
    assert_eq!(
        the_snapshot_of(&mut again, "isf:tint-")["shader"]["source"],
        "void main() {}\n"
    );
}

#[test]
fn a_shader_of_a_language_this_version_does_not_know_is_kept_as_it_was_and_is_not_run() {
    let (project, _) = a_project_with_a_shader();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    let odd = json!({ "language": "hlsl", "source": "float4 main() { return 0; }", "entry": "main" });
    the_snapshot_of(&mut document, "isf:tint-")["shader"] = odd.clone();

    let loaded = project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()).unwrap();

    let effect = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .next()
        .unwrap()
        .clone();
    assert!(effect.kind().shader_source().is_none());
    assert!(!effect.kind().is_installed());
    let mut again: Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(
        the_snapshot_of(&mut again, "isf:tint-")["shader"],
        odd,
        "kept as it was"
    );
}

#[test]
fn a_shader_that_says_more_than_a_language_and_a_source_is_kept_as_it_was_too() {
    let (project, _) = a_project_with_a_shader();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    let more = json!({ "language": "isf", "source": TINT, "licence": "MIT" });
    the_snapshot_of(&mut document, "isf:tint-")["shader"] = more.clone();

    let loaded = project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()).unwrap();

    let mut again: Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(the_snapshot_of(&mut again, "isf:tint-")["shader"], more);
}

#[test]
fn a_shader_that_is_too_big_is_refused_with_the_number_of_the_snapshot() {
    let (project, _) = a_project_with_a_shader();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    the_snapshot_of(&mut document, "isf:tint-")["shader"]["source"] = json!("x".repeat(MOST_BYTES_OF_A_SHADER + 1));

    let refused = project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()).unwrap_err();

    let words = refused.to_string();
    assert!(
        words.contains("plugin number") && words.contains("the most there can be"),
        "{words}"
    );
}

#[test]
fn a_file_of_version_15_has_no_shaders_is_read_as_it_was_and_saved_as_the_version_there_is() {
    let project = edited_project(link_to_missing_video());
    let fifteen = saved(&project).replace(&version_line(FILE_FORMAT_VERSION), &version_line(15));

    let loaded = project_from_json_text(&fifteen).unwrap();

    assert_same_content(&loaded, &project);
    assert!(saved(&loaded).contains(&version_line(FILE_FORMAT_VERSION)));
    assert!(!saved(&loaded).contains("\"shader\""));
}

#[test]
fn the_effects_of_the_program_have_no_shader_in_their_snapshots() {
    let mut project = edited_project(link_to_missing_video());
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();
    assert!(!saved(&project).contains("\"shader\""));
}

#[test]
fn two_shaders_of_one_name_and_different_texts_are_two_kinds_in_a_file_and_stay_two() {
    let mut project = edited_project(link_to_missing_video());
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let other = TINT.replace("0.25", "0.75");
    for source in [TINT, other.as_str()] {
        project
            .add_effect(AnimationTarget::Clip(clip), kind_of("tint.fs", source))
            .unwrap();
    }

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    let kinds: Vec<EffectKind> = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .map(|effect| effect.kind())
        .collect();
    assert_eq!(kinds.len(), 2);
    assert_ne!(kinds[0], kinds[1]);
    assert_eq!(kinds[0].shader_source().unwrap().text, TINT);
    assert_eq!(kinds[1].shader_source().unwrap().text, other);
}
