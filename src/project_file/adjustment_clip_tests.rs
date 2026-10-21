// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The adjustment clip in a project file (phase 5, piece 5.3; format 15): it is saved with how long it lasts, its effects and their keyframes and the badges of the tags it is in, and
//! read back as the same clip; a file of version 14 is read as it was and saved as 15; and what a file may not say of one is refused with the number of the clip.

use serde_json::json;

use super::tests::{assert_same_content, edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{EffectKind, GroupKind, Member, PropertyId};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};

/// A project that has a video, and in front of it a track with an adjustment clip from frame 20 to 120, that has a hue shift with a keyframe in it and a blur, and is in a tag.
fn a_project_with_an_adjustment_clip() -> (Project, crate::project::ClipId) {
    let mut project = edited_project(link_to_missing_video());
    let front = project.add_track_in_front();
    let (clip, _) = project
        .try_insert_adjustment_clip(front, crate::project::TimelineFrameIndex::new(20), 100)
        .unwrap();
    let effect = put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 30.0);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::effect_value(EffectKind::HUE_SHIFT, effect, 0),
        60,
        PropertyValue::Number(90.0),
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
fn an_adjustment_clip_is_saved_with_how_long_it_lasts_and_its_effects_and_read_back_the_same() {
    let (project, _) = a_project_with_an_adjustment_clip();

    let text = saved(&project);

    let document: serde_json::Value = serde_json::from_str(&text).unwrap();
    let written = &document["tracks"][0]["clips"][0];
    assert_eq!(written["kind"], "adjustment");
    assert_eq!(written["gap_before_frames"], 20);
    assert_eq!(written["duration_in_frames"], 100);
    assert_eq!(
        written["effects"].as_array().unwrap().len(),
        2,
        "the hue shift and the blur"
    );
    assert!(written["badges"].is_array(), "the badge of the tag it is in");
    let loaded = project_from_json_text(&text).unwrap();
    assert_same_content(&loaded, &project);
    let placement = loaded.tracks()[0].placement_at_index(0).unwrap();
    assert_eq!(placement.clip.kind(), crate::project::ClipKind::Adjustment);
    assert_eq!(placement.clip.duration_in_frames_it_says(), Some(100));
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const { assert!(FILE_FORMAT_VERSION >= 15, "the adjustment clip came with format 15") };
}

#[test]
fn a_file_of_version_14_has_no_adjustment_clip_is_read_as_it_was_and_saved_as_the_version_there_is() {
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    let fourteen = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(14));

    let loaded = project_from_json_text(&fourteen).unwrap();

    assert_same_content(&loaded, &project);
    assert!(saved(&loaded).contains(&version_line(FILE_FORMAT_VERSION)));
}

#[test]
fn a_file_from_a_newer_program_is_refused_so_that_an_older_program_does_not_read_an_adjustment_clip_as_something_else()
{
    let (project, _) = a_project_with_an_adjustment_clip();
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
fn what_a_file_says_of_an_adjustment_clip_that_cannot_be_is_refused_with_where_it_is() {
    let (project, _) = a_project_with_an_adjustment_clip();
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

    // It has no picture of its own, so nothing of one may be said of it.
    let reason = refuse(&|clip| clip["placement"] = json!({"area": {}}));
    assert!(!reason.is_empty(), "{reason}");
    let reason = refuse(&|clip| clip["source_identifier"] = json!(1));
    assert!(!reason.is_empty(), "{reason}");
}
