// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects of the program that came with phase 4 (saturation, brightness and contrast, exposure and opacity) in a project file: each is saved with the values it
//! has and its keyframes and read back as the same effect, and a group that cannot have one in the program cannot have it in a file either.

use super::tests::{assert_same_content, project_with_everything, saved};
use super::*;
use crate::project::{AnimationTarget, EffectKind, GroupKind, PropertyId};
use crate::test_support::{make_a_keyframe, put_an_effect_on};

/// Values that are not what the effect begins with, for each value of `kind`: a third of the way from the least to the most of its range.
fn values_that_are_not_the_defaults(kind: EffectKind) -> Vec<PropertyValue> {
    kind.parameters()
        .iter()
        .map(|parameter| {
            let (least, most) = parameter.range.expect("the effects of phase 4 have ranges");
            PropertyValue::Number(least + (most - least) / 3.0)
        })
        .collect()
}

fn the_first_video(project: &Project) -> ClipId {
    project.tracks()[2].entries()[0].id()
}

#[test]
fn every_effect_of_the_program_is_saved_with_its_values_and_its_keyframes_and_read_back_the_same() {
    let mut project = project_with_everything();
    let clip = the_first_video(&project);
    let target = AnimationTarget::Clip(clip);
    let mut instances = Vec::new();
    for kind in EffectKind::BUILT_IN {
        let values = values_that_are_not_the_defaults(kind);
        instances.push((kind, put_an_effect_on(&mut project, target, kind, &values)));
    }
    // One value of each is animated too.
    for (kind, instance) in &instances {
        let property = PropertyId::effect_value(*kind, *instance, 0);
        let (_, most) = kind.parameters()[0].range.unwrap();
        make_a_keyframe(&mut project, target, property, 10, PropertyValue::Number(most));
        make_a_keyframe(&mut project, target, property, 20, PropertyValue::Number(most / 2.0));
    }

    let text = saved(&project);
    let loaded = project_from_json_text(&text).expect("a file with the effects of the program loads");

    assert_same_content(&loaded, &project);
    let loaded_clip = AnimationTarget::Clip(the_first_video(&loaded));
    let kinds: Vec<EffectKind> = loaded
        .effects_of(loaded_clip)
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect();
    assert_eq!(
        kinds,
        [vec![EffectKind::HUE_SHIFT], EffectKind::BUILT_IN.to_vec()].concat(),
        "(the first video has a hue shift already: see `project_with_everything`)"
    );
    for (kind, instance) in instances {
        let effect = loaded
            .effects_of(loaded_clip)
            .unwrap()
            .iter()
            .find(|effect| effect.instance() == instance)
            .expect("the effect is there under the name it had");
        assert_eq!(effect.kind(), kind);
        assert!(
            effect.kind().is_installed(),
            "{kind}: it is the program's own effect, not a stranger"
        );
        assert_eq!(
            effect.values(),
            values_that_are_not_the_defaults(kind).as_slice(),
            "{kind}"
        );
        let animated = loaded
            .animation_of(loaded_clip)
            .keyframes_of(PropertyId::effect_value(kind, instance, 0))
            .map(|keyframes| keyframes.len());
        assert_eq!(animated, Some(2), "{kind}: its keyframes");
    }
}

#[test]
fn the_snapshot_of_each_effect_in_the_file_has_its_category_and_binds_to_the_effect_of_the_program() {
    let mut project = project_with_everything();
    let target = AnimationTarget::Clip(the_first_video(&project));
    for kind in EffectKind::BUILT_IN {
        put_an_effect_on(&mut project, target, kind, &values_that_are_not_the_defaults(kind));
    }

    let file: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();

    let plugins = file["plugins"].as_array().unwrap();
    for kind in EffectKind::BUILT_IN {
        let snapshot = plugins
            .iter()
            .find(|plugin| plugin["key"] == kind.key())
            .unwrap_or_else(|| panic!("no snapshot of {kind}: {plugins:?}"));
        assert_eq!(snapshot["category"], kind.descriptor().category.as_ref(), "{kind}");
        assert_eq!(
            snapshot["version"],
            serde_json::json!([kind.version().0, kind.version().1])
        );
        assert_eq!(
            snapshot["params"].as_array().unwrap().len(),
            kind.value_count(),
            "{kind}: a value for each"
        );
    }
}

#[test]
fn a_tag_can_have_an_opacity_in_a_file_and_a_clip_group_or_a_track_group_cannot() {
    use crate::project::Member;
    let mut project = project_with_everything();
    let clip = the_first_video(&project);
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap();
    project.add_effect_to_the_group(tag, EffectKind::OPACITY).unwrap();
    project.add_effect_to_the_group(tag, EffectKind::EXPOSURE).unwrap();
    assert_same_content(&project_from_json_text(&saved(&project)).unwrap(), &project);

    // A file that says a clip group has an opacity is refused, with the number of the group and the effect.
    let mut file: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();
    let group = file["groups"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|group| group["kind"] == "tag")
        .expect("the tag is in the file");
    group["kind"] = serde_json::json!("clip_group");
    let reason = match project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("{other:?}"),
    };
    assert!(
        reason.contains("group number 1") && reason.contains("opacity"),
        "{reason}"
    );
}
