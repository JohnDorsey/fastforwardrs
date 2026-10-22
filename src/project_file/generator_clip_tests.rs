// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The generator clip in a project file (phase 6, piece 6.5; format 16): it is saved with how long it lasts, where it is, which of its effects it makes its picture with, its
//! effects and their keyframes and the badges of the tags it is in, and read back as the same clip; a file of version 15 is read as it was; and what a file may not say of one is
//! refused with where it is.

use serde_json::{Value, json};

use super::tests::{assert_same_content, edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{EffectKind, GroupKind, Member, PropertyId};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

const SOLID: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"colour\", \"TYPE\": \"color\", \"DEFAULT\": [0.2, 0.4, 0.9, 1.0] } ] }*/\nvoid main() { gl_FragColor = colour; }\n";

fn a_generator() -> EffectKind {
    let file = crate::isf::read(SOLID).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "solid.fs", SOLID)).unwrap()
}

/// A project that has a video, and in front of it a track with a generator clip from frame 20 to 120 that is in the top left quarter and turned a little, has a hue shift with
/// a keyframe in it and a keyframe of the colour it makes, and is in a tag.
fn a_project_with_a_generator_clip() -> (Project, crate::project::ClipId) {
    let mut project = edited_project(link_to_missing_video());
    let front = project.add_track_in_front();
    let (clip, _) = project
        .try_insert_generator_clip(front, crate::project::TimelineFrameIndex::new(20), a_generator(), 100)
        .unwrap();
    project
        .set_clip_placement(
            clip,
            crate::project::Placement::new(
                CanvasArea::new(0.0, 0.0, 0.5, 0.25),
                crate::project::Angle::from_degrees(12.5),
            ),
        )
        .unwrap();
    let target = AnimationTarget::Clip(clip);
    let content = project
        .placement_of(clip)
        .unwrap()
        .clip
        .as_generator()
        .unwrap()
        .content();
    make_a_keyframe(
        &mut project,
        target,
        PropertyId::effect_value(a_generator(), content, 0),
        60,
        PropertyValue::Colour(crate::project::TextColour::new(255, 0, 0, 255)),
    );
    let turn = put_a_hue_shift_on(&mut project, target, 30.0);
    make_a_keyframe(
        &mut project,
        target,
        PropertyId::effect_value(EffectKind::HUE_SHIFT, turn, 0),
        70,
        PropertyValue::Number(90.0),
    );
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    project
        .create_group(GroupKind::Tag, &[Member::Clip(clip), Member::Clip(video)])
        .unwrap();
    (project, clip)
}

#[test]
fn a_generator_clip_is_saved_with_what_it_makes_where_it_is_and_how_long_it_lasts_and_read_back_the_same() {
    let (project, _) = a_project_with_a_generator_clip();

    let text = saved(&project);

    let document: Value = serde_json::from_str(&text).unwrap();
    let written = &document["tracks"][0]["clips"][0];
    assert_eq!(written["kind"], "generator");
    assert_eq!(written["gap_before_frames"], 20);
    assert_eq!(written["duration_in_frames"], 100);
    assert_eq!(written["content"], 0, "it is made by its effect number 0");
    assert_eq!(written["rotation_degrees"], 12.5);
    assert!(written["area"].is_object(), "where it is");
    assert_eq!(
        written["effects"].as_array().unwrap().len(),
        2,
        "what it makes, and the hue shift"
    );
    assert_eq!(written["effects"][0]["effect"], a_generator().key());
    assert!(written["badges"].is_array(), "the badge of the tag it is in");
    let loaded = project_from_json_text(&text).unwrap();
    assert_same_content(&loaded, &project);
    let placement = loaded.tracks()[0].placement_at_index(0).unwrap();
    let generator = placement.clip.as_generator().expect("it is still a generator");
    assert_eq!(generator.duration_in_frames(), 100);
    assert_eq!(generator.placement().rotation.degrees(), 12.5);
    assert_eq!(
        placement
            .effects
            .instance(generator.content())
            .map(|effect| effect.kind()),
        Some(a_generator()),
        "and it is made by the same effect"
    );
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
}

#[test]
fn the_keyframes_of_what_a_generator_makes_are_saved_and_read_back() {
    let (project, _) = a_project_with_a_generator_clip();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    let clip = loaded.tracks()[0].placement_at_index(0).unwrap().id;
    let content = loaded
        .placement_of(clip)
        .unwrap()
        .clip
        .as_generator()
        .unwrap()
        .content();
    assert!(loaded.is_animated(
        AnimationTarget::Clip(clip),
        PropertyId::effect_value(a_generator(), content, 0)
    ));
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn a_generator_that_is_the_whole_picture_and_upright_is_saved_with_no_turn() {
    let mut project = edited_project(link_to_missing_video());
    let front = project.add_track_in_front();
    project
        .try_insert_generator_clip(front, crate::project::TimelineFrameIndex::new(0), a_generator(), 10)
        .unwrap();

    let document: Value = serde_json::from_str(&saved(&project)).unwrap();

    assert!(document["tracks"][0]["clips"][0].get("rotation_degrees").is_none());
}

#[test]
fn a_file_of_version_15_has_no_generator_clip_is_read_as_it_was_and_saved_as_the_version_there_is() {
    let project = edited_project(link_to_missing_video());
    let fifteen = saved(&project).replace(&version_line(FILE_FORMAT_VERSION), &version_line(15));

    let loaded = project_from_json_text(&fifteen).unwrap();

    assert_same_content(&loaded, &project);
    assert!(saved(&loaded).contains(&version_line(FILE_FORMAT_VERSION)));
}

#[test]
fn what_a_file_says_of_a_generator_clip_that_cannot_be_is_refused_with_where_it_is() {
    let (project, _) = a_project_with_a_generator_clip();
    let text = saved(&project);
    let refuse = |change: &dyn Fn(&mut Value)| -> String {
        let mut document: Value = serde_json::from_str(&text).unwrap();
        change(&mut document["tracks"][0]["clips"][0]);
        match project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("the file was read"),
        }
    };

    let reason = refuse(&|clip| clip["duration_in_frames"] = json!(0));
    assert!(reason.contains("at least one frame"), "{reason}");

    // It is made by an effect that it does not have.
    let reason = refuse(&|clip| clip["content"] = json!(7));
    assert!(
        reason.contains("effect number 7") && reason.contains("does not have"),
        "{reason}"
    );

    // It has no file it is made from.
    let reason = refuse(&|clip| clip["source_identifier"] = json!(1));
    assert!(!reason.is_empty(), "{reason}");

    // And the text of its kind: a sound effect cannot be put on a clip that has nothing to hear.
    let reason = refuse(&|clip| clip["effects"][1]["effect"] = json!("ladspa:1/not_here"));
    assert!(!reason.is_empty(), "{reason}");
}

#[test]
fn a_generator_whose_shader_is_not_installed_is_kept_with_what_it_makes_and_its_values() {
    let (project, _) = a_project_with_a_generator_clip();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    // The shader is one that this version cannot run (a file edited by hand: the text is not what its key was made from).
    let snapshot = document["plugins"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|snapshot| {
            snapshot["key"]
                .as_str()
                .is_some_and(|key| key.starts_with("isf:solid-"))
        })
        .unwrap();
    snapshot["shader"]["source"] = json!("void main() {}\n");

    let loaded = project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()).unwrap();

    let placement = loaded.tracks()[0].placement_at_index(0).unwrap();
    let generator = placement.clip.as_generator().unwrap();
    let content = placement.effects.instance(generator.content()).unwrap();
    assert!(!content.kind().is_installed());
    assert_eq!(content.values().len(), 1, "its value is kept");
    assert_eq!(loaded.tracks()[0].placements().count(), 1);
}
