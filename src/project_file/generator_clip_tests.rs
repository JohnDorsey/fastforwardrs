// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The generator clip in a project file (phase 5, piece 5.4; format 15): it is saved with which generator it is, its colour, where it is, how long it lasts, its effects and their
//! keyframes (the colour has some too) and the badges of the tags it is in, and read back as the same clip; a file of version 14 is read as it was and saved as 15; and what a file
//! may not say of one is refused with where it is.

use serde_json::json;

use super::tests::{assert_same_content, edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{EffectKind, GeneratorKind, GroupKind, Member, PropertyId, TextColour};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

/// A project that has a video, and in front of it a track with a solid colour from frame 20 to 120 that is red and half clear, put at the top left, turned a little, with a hue
/// shift that has a keyframe in it, a blur, and a keyframe of its colour, and that is in a tag.
fn a_project_with_a_solid_colour() -> (Project, crate::project::ClipId) {
    let mut project = edited_project(link_to_missing_video());
    let front = project.add_track_in_front();
    let (clip, _) = project
        .try_insert_generator_clip(
            front,
            crate::project::TimelineFrameIndex::new(20),
            GeneratorKind::SolidColour,
            100,
        )
        .unwrap();
    project
        .set_generator_colour(clip, TextColour::new(255, 0, 0, 128))
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
    let effect = put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 30.0);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::effect_value(EffectKind::HUE_SHIFT, effect, 0),
        60,
        PropertyValue::Number(90.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Colour,
        70,
        PropertyValue::Colour(TextColour::new(0, 0, 255, 255)),
    );
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::BLUR)
        .unwrap();
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    project
        .create_group(GroupKind::Tag, &[Member::Clip(clip), Member::Clip(video)])
        .unwrap();
    (project, clip)
}

#[test]
fn a_generator_clip_is_saved_with_what_it_makes_and_how_long_it_lasts_and_read_back_the_same() {
    let (project, _) = a_project_with_a_solid_colour();

    let text = saved(&project);

    let document: serde_json::Value = serde_json::from_str(&text).unwrap();
    let written = &document["tracks"][0]["clips"][0];
    assert_eq!(written["kind"], "generator");
    assert_eq!(written["generator"], "builtin:solid_colour");
    assert_eq!(written["gap_before_frames"], 20);
    assert_eq!(written["duration_in_frames"], 100);
    assert_eq!(written["colour"], "#FF000080");
    assert_eq!(written["rotation_degrees"], 12.5);
    assert!(written["area"].is_object(), "where it is");
    assert_eq!(
        written["effects"].as_array().unwrap().len(),
        2,
        "the hue shift and the blur"
    );
    assert!(written["badges"].is_array(), "the badge of the tag it is in");
    let loaded = project_from_json_text(&text).unwrap();
    assert_same_content(&loaded, &project);
    let placement = loaded.tracks()[0].placement_at_index(0).unwrap();
    assert_eq!(placement.clip.kind(), crate::project::ClipKind::Generator);
    let generator = placement.clip.as_generator().unwrap();
    assert_eq!(generator.kind(), GeneratorKind::SolidColour);
    assert_eq!(generator.colour, TextColour::new(255, 0, 0, 128));
    assert_eq!(generator.duration_in_frames(), 100);
    assert_eq!(generator.placement().rotation.degrees(), 12.5);
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const { assert!(FILE_FORMAT_VERSION >= 15, "the generator clip came with format 15") };
}

#[test]
fn the_keyframes_of_the_colour_of_a_generator_clip_are_saved_and_read_back() {
    let (project, _) = a_project_with_a_solid_colour();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    // (A clip is named by where it is when a file is read, not by the name it had.)
    let clip = loaded.tracks()[0].placement_at_index(0).unwrap().id;
    let target = AnimationTarget::Clip(clip);
    assert!(loaded.is_animated(target, PropertyId::Colour));
    assert_eq!(
        loaded.generator_colour_at(clip, crate::project::TimelineFrameIndex::new(70)),
        Some(TextColour::new(0, 0, 255, 255))
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn a_solid_colour_that_is_the_whole_picture_and_upright_is_saved_with_no_turn() {
    let mut project = edited_project(link_to_missing_video());
    let front = project.add_track_in_front();
    project
        .try_insert_generator_clip(
            front,
            crate::project::TimelineFrameIndex::new(0),
            GeneratorKind::SolidColour,
            10,
        )
        .unwrap();

    let document: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();

    let written = &document["tracks"][0]["clips"][0];
    assert!(written.get("rotation_degrees").is_none(), "left out when it is upright");
    assert_eq!(written["colour"], "#808080FF", "the grey that it begins as");
}

#[test]
fn a_file_of_version_14_has_no_generator_clip_is_read_as_it_was_and_saved_as_the_version_there_is() {
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    let fourteen = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(14));

    let loaded = project_from_json_text(&fourteen).unwrap();

    assert_same_content(&loaded, &project);
    assert!(saved(&loaded).contains(&version_line(FILE_FORMAT_VERSION)));
}

#[test]
fn a_file_from_a_newer_program_is_refused_so_that_an_older_program_does_not_read_a_generator_clip_as_something_else() {
    let (project, _) = a_project_with_a_solid_colour();
    let text = saved(&project);
    let newer = text.replace(
        &version_line(FILE_FORMAT_VERSION),
        &version_line(FILE_FORMAT_VERSION + 1),
    );
    assert!(matches!(
        project_from_json_text(&newer),
        Err(ProjectFileError::NewerFileFormat { found, supported })
            if found == FILE_FORMAT_VERSION + 1 && supported == FILE_FORMAT_VERSION
    ));
}

#[test]
fn what_a_file_says_of_a_generator_clip_that_cannot_be_is_refused_with_where_it_is() {
    let (project, _) = a_project_with_a_solid_colour();
    let text = saved(&project);
    let refuse = |change: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut document: serde_json::Value = serde_json::from_str(&text).unwrap();
        change(&mut document["tracks"][0]["clips"][0]);
        match project_from_json_text(&serde_json::to_string_pretty(&document).unwrap()) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("the file was read"),
        }
    };

    let reason = refuse(&|clip| clip["duration_in_frames"] = json!(0));
    assert!(reason.contains("at least one frame"), "{reason}");

    let reason = refuse(&|clip| clip["generator"] = json!("builtin:plasma"));
    assert!(
        reason.contains("builtin:plasma") && reason.contains("not one that this program has"),
        "{reason}"
    );

    let reason = refuse(&|clip| clip["colour"] = json!("red"));
    assert!(!reason.is_empty(), "{reason}");

    // It is a picture the program makes: it has no file it is made from.
    let reason = refuse(&|clip| clip["source_identifier"] = json!(1));
    assert!(!reason.is_empty(), "{reason}");
}
