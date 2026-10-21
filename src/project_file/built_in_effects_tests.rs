// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects of the program that came with phase 4 (saturation, brightness and contrast, exposure and opacity) in a project file: each is saved with the values it
//! has and its keyframes and read back as the same effect, and a group that cannot have one in the program cannot have it in a file either.

use super::tests::{assert_same_content, project_with_everything, saved};
use super::*;
use crate::project::{AnimationTarget, EffectKind, GroupKind, MOST_EFFECTS_ON_ONE_THING, PropertyId};
use crate::test_support::{make_a_keyframe, put_an_effect_on};

/// Values that are not what the effect begins with, for each value of `kind`: a third of the way from the least to the most of a range, the last of the options of a choice,
/// the name of a table for a resource.
fn values_that_are_not_the_defaults(kind: EffectKind) -> Vec<PropertyValue> {
    kind.parameters()
        .iter()
        .map(|parameter| match (&parameter.control, parameter.range) {
            (crate::project::ParameterControl::Choice(options), _) => {
                PropertyValue::Words(options.last().expect("a choice has options").id.to_string())
            }
            (crate::project::ParameterControl::Resource(_), _) => PropertyValue::Words("a table.cube".to_owned()),
            (_, Some((least, most))) => PropertyValue::Number(least + (most - least) / 3.0),
            (_, None) => panic!("{kind}: a value that is not a number, a choice or a resource"),
        })
        .collect()
}

/// The place of the first value of `kind` that is a number with a range, which is the one that the tests animate, and its range.
fn the_first_number_with_a_range(kind: EffectKind) -> (u8, (f32, f32)) {
    kind.parameters()
        .iter()
        .enumerate()
        .find_map(|(place, parameter)| parameter.range.map(|range| (u8::try_from(place).unwrap(), range)))
        .expect("every effect of the program has a number with a range")
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
        let (place, (_, most)) = the_first_number_with_a_range(*kind);
        let property = PropertyId::effect_value(*kind, *instance, place);
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
            .keyframes_of(PropertyId::effect_value(
                kind,
                instance,
                the_first_number_with_a_range(kind).0,
            ))
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

// (Rewritten on purpose in phase 5: it said that a file with an opacity on a clip group is refused, because the opacity of a group was not what it would be done to the picture the
// members make together. The engine makes that picture now, so a clip group has an opacity, a blur and a crop in a file as a tag does.)
#[test]
fn a_tag_and_a_clip_group_can_have_an_opacity_and_a_blur_in_a_file() {
    use crate::project::Member;
    let mut project = project_with_everything();
    let clip = the_first_video(&project);
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap();
    project.add_effect_to_the_group(tag, EffectKind::OPACITY).unwrap();
    project.add_effect_to_the_group(tag, EffectKind::EXPOSURE).unwrap();
    assert_same_content(&project_from_json_text(&saved(&project)).unwrap(), &project);

    // The same effects on a clip group, a track group and a clip group in the project that has groups of every kind are read as they are.
    let mut groups = crate::project_file::groups_tests::a_project_with_groups_of_every_kind();
    for kind in [GroupKind::ClipGroup, GroupKind::TrackGroup] {
        let group = groups.groups().iter().find(|group| group.kind() == kind).unwrap().id();
        groups.add_effect_to_the_group(group, EffectKind::OPACITY).unwrap();
        groups.add_effect_to_the_group(group, EffectKind::BLUR).unwrap();
    }
    assert_same_content(&project_from_json_text(&saved(&groups)).unwrap(), &groups);
}

#[test]
fn a_file_with_sixty_four_effects_on_a_clip_is_read_and_one_with_sixty_five_is_refused_with_the_number() {
    let mut project = project_with_everything();
    let clip = the_first_video(&project);
    while !project.effects_of(AnimationTarget::Clip(clip)).unwrap().is_full() {
        project
            .add_effect(AnimationTarget::Clip(clip), EffectKind::EXPOSURE)
            .unwrap();
    }
    assert_eq!(
        project.effects_of(AnimationTarget::Clip(clip)).unwrap().len(),
        MOST_EFFECTS_ON_ONE_THING
    );
    assert_same_content(&project_from_json_text(&saved(&project)).unwrap(), &project);

    let mut file: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();
    let effects = file["tracks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|track| track["clips"].as_array_mut().into_iter().flatten())
        .find_map(|clip| {
            clip.get_mut("effects")
                .and_then(|effects| effects.as_array_mut())
                .filter(|effects| effects.len() == 64)
        })
        .expect("the clip with the effects is in the file");
    let mut one_more = effects[0].clone();
    one_more["instance"] = serde_json::json!(64);
    effects.push(one_more);
    let text = serde_json::to_string_pretty(&file).unwrap();
    let reason = match project_from_json_text(&text) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("{other:?}"),
    };
    assert!(
        reason.contains("65 effects") && reason.contains("the most one can have is 64"),
        "{reason}"
    );
}
