// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 28 of the project file added: `space` in a `driven` number, where the mask is laid (over the picture, or in space on a plane, a sphere or a cylinder put somewhere in space), and
//! that a file of version 27, which has none, is read as one where every mask is over the picture. (`docs/shader-masks-design.md`.)

use serde_json::{Value, json};

use super::driven_value_tests::{as_json, the_effect_of_the_back_tracks_first_clip};
use super::tests::{edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{DrivenValue, EffectKind, MaskChannel, MaskSpace, Member, WorldPlacement, WorldShape};

/// A shader with `reach` (place 0) that says `WORLD_SPACE`, and `amount` (place 1) that does not.
const WORKS_IN_SPACE: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"reach\", \"TYPE\": \"float\", \"MIN\": 1.0, \"MAX\": 9.0, \"DEFAULT\": 4.0, \"WORLD_SPACE\": true }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 } ] }*/\nvoid main() { isf_world_position = vec3(isf_FragNormCoord, 0.0); gl_FragColor = vec4(vec3(reach * amount), 1.0); }\n";

fn works_in_space() -> EffectKind {
    let file = crate::isf::read(WORKS_IN_SPACE).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "space.fs", WORKS_IN_SPACE)).unwrap()
}

fn a_placement() -> WorldPlacement {
    WorldPlacement {
        shape: WorldShape::Cylinder,
        position: [1.5, -2.0, 0.25],
        yaw_pitch_roll_degrees: [30.0, -10.0, 45.0],
        width: 4.5,
        height: 2.5,
    }
}

/// A project of two tracks: the one in front (place 0 in the file), and the back one (place 1) whose first clip has the shader that works in space with `reach` read from the front track by
/// red, laid as `space` says.
fn project_with_a_value_in_space(space: MaskSpace) -> Project {
    let mut project = edited_project(link_to_missing_video());
    let back = project.tracks()[0].id();
    let front = project.add_track_in_front();
    let clip = project.track(back).unwrap().placement_at_index(0).unwrap().id;
    let instance = project
        .add_effect(crate::project::AnimationTarget::Clip(clip), works_in_space())
        .unwrap();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            0,
            Some(DrivenValue::from_a_track(front, MaskChannel::Red, 6.0).with_the_space(space)),
        )
        .unwrap();
    project
}

fn text_with_the_space_changed(space: MaskSpace, change: impl FnOnce(&mut Value)) -> String {
    let mut document = as_json(&saved(&project_with_a_value_in_space(space)));
    change(&mut document["tracks"][1]["clips"][0]["effects"][0]["driven"]["reach"]);
    document.to_string()
}

#[test]
fn a_mask_laid_in_space_is_written_as_its_shape_and_how_the_shape_is_put_and_one_over_the_picture_writes_nothing() {
    let in_space = as_json(&saved(&project_with_a_value_in_space(MaskSpace::World(a_placement()))));
    let over_the_picture = as_json(&saved(&project_with_a_value_in_space(MaskSpace::Screen)));

    assert_eq!(
        the_effect_of_the_back_tracks_first_clip(&in_space)["driven"]["reach"]["space"],
        json!({ "world": {
            "shape": "cylinder",
            "position": [1.5, -2.0, 0.25],
            "yaw_pitch_roll_degrees": [30.0, -10.0, 45.0],
            "width": 4.5,
            "height": 2.5,
        } })
    );
    assert!(
        the_effect_of_the_back_tracks_first_clip(&over_the_picture)["driven"]["reach"]
            .get("space")
            .is_none()
    );
}

#[test]
fn every_shape_is_written_by_its_word_comes_back_as_it_was_and_is_saved_again_as_it_was() {
    for (shape, word) in [
        (WorldShape::Plane, "plane"),
        (WorldShape::Sphere, "sphere"),
        (WorldShape::Cylinder, "cylinder"),
    ] {
        let space = MaskSpace::World(a_placement().with_the_shape(shape));
        let text = saved(&project_with_a_value_in_space(space));

        let loaded = project_from_json_text(&text).unwrap();

        let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
        assert_eq!(effect.driven_values().of(0).unwrap().space, space, "{word}");
        assert_eq!(saved(&loaded), text, "{word}: nothing is lost");
        assert_eq!(
            the_effect_of_the_back_tracks_first_clip(&as_json(&text))["driven"]["reach"]["space"]["world"]["shape"],
            word
        );
    }
}

#[test]
fn a_file_of_version_27_has_every_mask_over_the_picture_and_is_read_as_it_always_was() {
    let text = saved(&project_with_a_value_in_space(MaskSpace::Screen));
    const { assert!(FILE_FORMAT_VERSION >= 28, "the places in space came with format 28") };
    let twenty_seven = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(27));

    let loaded = project_from_json_text(&twenty_seven).unwrap();

    let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
    assert_eq!(effect.driven_values().of(0).unwrap().space, MaskSpace::Screen);
    assert_eq!(saved(&loaded), text);
}

#[test]
fn a_value_that_its_shader_does_not_say_works_in_space_is_refused_when_the_file_lays_its_mask_there() {
    let mut document = as_json(&saved(&project_with_a_value_in_space(MaskSpace::World(a_placement()))));
    let effect = &mut document["tracks"][1]["clips"][0]["effects"][0];
    let one = effect["driven"]["reach"].take();
    effect["driven"] = json!({ "amount": one });

    let reason = project_from_json_text(&document.to_string()).unwrap_err().to_string();

    assert!(reason.contains("amount") && reason.contains("in space"), "{reason}");
}

#[test]
fn a_place_in_space_that_is_not_one_a_shape_can_be_put_at_is_refused() {
    for (what, change) in [
        ("a width of nothing", json!({ "width": 0.0 })),
        ("a negative height", json!({ "height": -1.0 })),
        ("a middle out of reach", json!({ "position": [0.0, 3.0e6, 0.0] })),
        ("a size out of reach", json!({ "width": 3.0e6 })),
    ] {
        let text = text_with_the_space_changed(MaskSpace::World(a_placement()), |one| {
            for (key, value) in change.as_object().unwrap() {
                one["space"]["world"][key] = value.clone();
            }
        });

        let reason = project_from_json_text(&text).unwrap_err().to_string();

        assert!(
            reason.contains("reach") && reason.contains("place in space"),
            "{what}: {reason}"
        );
    }
}

#[test]
fn a_shape_this_version_does_not_know_a_field_it_does_not_know_and_a_kind_of_space_it_does_not_know_are_each_refused() {
    for (what, change) in [
        (
            "a shape",
            json!({ "world": { "shape": "torus", "position": [0, 0, 0], "yaw_pitch_roll_degrees": [0, 0, 0], "width": 1, "height": 1 } }),
        ),
        (
            "a field",
            json!({ "world": { "shape": "plane", "position": [0, 0, 0], "yaw_pitch_roll_degrees": [0, 0, 0], "width": 1, "height": 1, "depth": 1 } }),
        ),
        (
            "a number short",
            json!({ "world": { "shape": "plane", "position": [0, 0], "yaw_pitch_roll_degrees": [0, 0, 0], "width": 1, "height": 1 } }),
        ),
        ("a kind of space", json!({ "volume": { "shape": "plane" } })),
        (
            "a missing size",
            json!({ "world": { "shape": "plane", "position": [0, 0, 0], "yaw_pitch_roll_degrees": [0, 0, 0], "width": 1 } }),
        ),
    ] {
        let text = text_with_the_space_changed(MaskSpace::World(a_placement()), |one| one["space"] = change);

        assert!(project_from_json_text(&text).is_err(), "{what} was not refused");
    }
}

#[test]
fn a_file_whose_places_in_space_have_no_room_among_the_numbers_of_the_shader_is_refused() {
    // The four of the shader and 58 numbers fill 62 of the 64, so the nine of a place do not fit.
    let floats: Vec<String> = (0..58)
        .map(|number| {
            format!(
                "{{ \"NAME\": \"p{number}\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true }}"
            )
        })
        .collect();
    let text = format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}, {} ] }}*/\nvoid main() {{ isf_world_position = vec3(0.0); gl_FragColor = IMG_THIS_PIXEL(inputImage) * p0; }}\n",
        floats.join(", ")
    );
    let file = crate::isf::read(&text).unwrap();
    let kind = EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "many.fs", &text)).unwrap();
    let mut project = edited_project(link_to_missing_video());
    let back = project.tracks()[0].id();
    let front = project.add_track_in_front();
    let clip = project.track(back).unwrap().placement_at_index(0).unwrap().id;
    let instance = project
        .add_effect(crate::project::AnimationTarget::Clip(clip), kind)
        .unwrap();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            0,
            Some(DrivenValue::from_a_track(front, MaskChannel::Red, 0.5)),
        )
        .unwrap();
    let mut document = as_json(&saved(&project));
    document["tracks"][1]["clips"][0]["effects"][0]["driven"]["p0"]["space"] = json!({ "world": {
        "shape": "plane", "position": [0, 0, 0], "yaw_pitch_roll_degrees": [0, 0, 0], "width": 1, "height": 1 } });

    let reason = project_from_json_text(&document.to_string()).unwrap_err().to_string();

    assert!(reason.contains("room"), "{reason}");
}
