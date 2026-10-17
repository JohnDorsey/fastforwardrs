// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The resources of a project, as the project holds them: adding, and which of them the effects use.

use std::path::PathBuf;
use std::sync::Arc;

use super::tests::{frame, video_source};
use super::*;
use crate::test_support::{a_kind_of_effect_that_uses_a_resource, a_kind_of_effect_the_program_does_not_have};

fn bytes(text: &str) -> Arc<[u8]> {
    Arc::from(text.as_bytes())
}

fn link() -> SourceMediaLink {
    SourceMediaLink::from_absolute_path(PathBuf::from("/tables/grade.cube"))
}

/// A project with one video on its own track, and what its clip is (for the effects to be put on).
fn project_with_a_clip() -> (Project, AnimationTarget) {
    let mut project = Project::empty();
    let video = project.import_source(video_source(1, 30, 600), frame(0)).unwrap();
    (project, AnimationTarget::Clip(video.clip.unwrap()))
}

fn uses_the_table_named(project: &mut Project, target: AnimationTarget, name: &str) {
    let kind = a_kind_of_effect_that_uses_a_resource();
    let instance = project.add_effect(target, kind).unwrap();
    project
        .set_property_at(
            target,
            PropertyId::effect_value(kind, instance, 0),
            PropertyValue::Words(name.to_owned()),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
}

#[test]
fn a_new_project_has_no_resources_and_one_that_is_added_is_there_by_the_name_it_was_given() {
    let mut project = Project::empty();
    assert!(project.resources().is_empty());

    let name = project
        .add_resource("grade.cube", ResourceKind::Cube, bytes("table"), link())
        .unwrap();

    assert_eq!(name, "grade.cube");
    assert_eq!(project.resources().len(), 1);
    assert_eq!(
        project.resources().get("grade.cube").unwrap().bytes().map(|b| &**b),
        Some(&b"table"[..])
    );
    // The same bytes under another name: the one that is there.
    assert_eq!(
        project.add_resource("other.cube", ResourceKind::Cube, bytes("table"), link()),
        Ok("grade.cube".to_owned())
    );
    assert_eq!(project.resources().len(), 1);
    assert!(matches!(
        project.add_resource("a/b.cube", ResourceKind::Cube, bytes("x"), link()),
        Err(ResourceProblem::BadName { .. })
    ));
}

#[test]
fn a_resource_is_in_use_when_an_effect_of_a_clip_a_track_or_a_group_names_it_in_a_value_that_is_a_resource() {
    let (mut project, clip) = project_with_a_clip();
    let track = AnimationTarget::Track(project.tracks()[0].id());
    for name in ["clip.cube", "track.cube", "group.cube", "unused.cube"] {
        project
            .add_resource(name, ResourceKind::Cube, bytes(name), link())
            .unwrap();
    }
    uses_the_table_named(&mut project, clip, "clip.cube");
    uses_the_table_named(&mut project, track, "track.cube");
    let AnimationTarget::Clip(clip_id) = clip else {
        unreachable!()
    };
    let clip_member = Member::Clip(clip_id);
    let tag = project.create_group(GroupKind::Tag, &[clip_member]).unwrap();
    let kind = a_kind_of_effect_that_uses_a_resource();
    let instance = project.add_effect_to_the_group(tag, kind).unwrap();
    project
        .set_effect_value_of_the_group(
            tag,
            PropertyId::effect_value(kind, instance, 0),
            &PropertyValue::Words("group.cube".to_owned()),
        )
        .unwrap();

    let used = project.resources_in_use();

    assert_eq!(
        used.iter().map(String::as_str).collect::<Vec<_>>(),
        ["clip.cube", "group.cube", "track.cube"]
    );
    let to_save = project.resources_to_save();
    assert_eq!(
        to_save.iter().map(Resource::name).collect::<Vec<_>>(),
        ["clip.cube", "track.cube", "group.cube"],
        "(in the order they were added; the one nobody uses is not saved)"
    );
    assert_eq!(project.resources().len(), 4, "(the project still has it)");
}

#[test]
fn words_that_are_not_a_resource_are_not_taken_for_one_and_nor_is_a_value_with_no_words() {
    let (mut project, clip) = project_with_a_clip();
    project
        .add_resource("grade.cube", ResourceKind::Cube, bytes("table"), link())
        .unwrap();
    // A value that is words but not a resource: it names the same thing and means nothing to the resources.
    let other = a_kind_of_effect_the_program_does_not_have(
        "test:words",
        &[("caption", PropertyValue::Words("grade.cube".to_owned()))],
    );
    project.add_effect(clip, other).unwrap();
    // A value that is a resource and says nothing yet: the effect was just put on.
    project
        .add_effect(clip, a_kind_of_effect_that_uses_a_resource())
        .unwrap();
    assert!(project.resources_in_use().is_empty());

    // A name that is not a resource of the project is in use all the same (the effect names it), and is not saved, for it is not there.
    uses_the_table_named(&mut project, clip, "gone.cube");
    assert_eq!(
        project.resources_in_use().into_iter().collect::<Vec<_>>(),
        ["gone.cube"]
    );
    assert!(project.resources_to_save().is_empty());
}

#[test]
fn a_resource_that_is_missing_is_still_in_the_list_and_can_be_given_its_bytes_and_kept_somewhere_else() {
    let mut project = Project::empty();
    let mut resources = Resources::default();
    resources
        .add_as_it_was_saved("grade.cube", ResourceKind::Cube, None, link())
        .unwrap();
    project = project.with_resources(resources);
    assert!(project.resources().get("grade.cube").unwrap().is_missing());

    assert!(project.give_the_bytes_of_a_resource("grade.cube", bytes("found")));
    assert!(!project.resources().get("grade.cube").unwrap().is_missing());

    let included = SourceMediaLink {
        included_as: Some("project-resources/0-grade.cube".to_owned()),
        ..link()
    };
    assert!(project.keep_a_resource_at("grade.cube", included.clone()));
    assert_eq!(project.resources().get("grade.cube").unwrap().link(), &included);
    assert!(!project.keep_a_resource_at("nothing.cube", included));
}

#[test]
fn the_resources_in_use_that_cannot_be_used_are_the_ones_the_project_does_not_have_and_the_ones_that_are_missing() {
    let (mut project, clip) = project_with_a_clip();
    let mut resources = Resources::default();
    resources
        .add_as_it_was_saved("missing.cube", ResourceKind::Cube, None, link())
        .unwrap();
    resources
        .add_as_it_was_saved("fine.cube", ResourceKind::Cube, Some(bytes("table")), link())
        .unwrap();
    resources
        .add_as_it_was_saved("unused.cube", ResourceKind::Cube, None, link())
        .unwrap();
    project = project.with_resources(resources);
    for name in ["fine.cube", "missing.cube", "absent.cube"] {
        uses_the_table_named(&mut project, clip, name);
    }

    assert_eq!(
        project.resources_in_use_that_are_missing(),
        ["absent.cube", "missing.cube"],
        "(in the order of the names; the one nobody uses does not count, and neither does the one that has its bytes)"
    );

    assert!(project.give_the_bytes_of_a_resource("missing.cube", bytes("found")));
    assert_eq!(project.resources_in_use_that_are_missing(), ["absent.cube"]);
}

#[test]
fn a_value_that_is_a_resource_is_looked_up_by_the_control_and_only_for_words() {
    let (mut project, _) = project_with_a_clip();
    project
        .add_resource("grade.cube", ResourceKind::Cube, bytes("table"), link())
        .unwrap();
    let control = ParameterControl::Resource(std::borrow::Cow::Borrowed("cube"));
    let words = PropertyValue::Words("grade.cube".to_owned());

    assert_eq!(
        project.resource_named_by(&control, &words).map(Resource::name),
        Some("grade.cube")
    );
    assert!(
        project
            .resource_named_by(&ParameterControl::Automatic, &words)
            .is_none()
    );
    assert!(
        project
            .resource_named_by(&control, &PropertyValue::Number(1.0))
            .is_none()
    );
    assert!(
        project
            .resource_named_by(&control, &PropertyValue::Words("other.cube".to_owned()))
            .is_none()
    );
}
