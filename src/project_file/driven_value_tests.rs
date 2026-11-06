// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 24 of the project file added: `driven` in the document of an effect, the numbers of a shader that are read from a mask (a track), by the id of each number, with the place of
//! the track in the file's list of tracks, the channel, and what the number is where the mask has nothing; and that a file of version 23, which has none, is read as one where every number is
//! read from its slider. (`docs/shader-masks-design.md`.)

use serde_json::{Value, json};

use super::tests::{edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{DrivenValue, DrivenValues, EffectKind, MaskChannel, Member};

/// A shader with two numbers that have a range, `amount` (0 to 1) and `strength` (2 to 6), and a colour called `tint`.
const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"strength\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 6.0, \"DEFAULT\": 3.0 }, { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [1.0, 0.0, 0.0, 1.0] } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn tint() -> EffectKind {
    let file = crate::isf::read(TINT).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "tint.fs", TINT)).unwrap()
}

pub(super) fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// A project of two tracks: the one in front (place 0 in the file), and the back one (place 1) whose first clip has a shader with `amount` read from the front track, by brightness, and
/// 0.25 where the front track has nothing.
fn project_with_a_value_read_from_the_front_track() -> (Project, TrackId, TrackId) {
    let mut project = edited_project(link_to_missing_video());
    let back = project.tracks()[0].id();
    let front = project.add_track_in_front();
    let clip = project.track(back).unwrap().placement_at_index(0).unwrap().id;
    let instance = project
        .add_effect(crate::project::AnimationTarget::Clip(clip), tint())
        .unwrap();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            0,
            Some(DrivenValue::from_a_track(front, MaskChannel::Brightness, 0.25)),
        )
        .unwrap();
    (project, front, back)
}

pub(super) fn the_effect_of_the_back_tracks_first_clip(document: &Value) -> &Value {
    &document["tracks"][1]["clips"][0]["effects"][0]
}

#[test]
fn a_number_read_from_a_mask_is_written_by_the_id_of_the_number_and_the_place_of_the_track() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();

    let document = as_json(&saved(&project));

    assert_eq!(
        the_effect_of_the_back_tracks_first_clip(&document)["driven"],
        json!({ "amount": { "source": { "track": 0 }, "channel": "brightness", "where_nothing": 0.25 } })
    );
    // (The slider is as it was put on: nothing about it is written.)
    assert!(
        the_effect_of_the_back_tracks_first_clip(&document)
            .get("values")
            .is_none()
    );
}

#[test]
fn a_project_that_reads_nothing_from_a_mask_has_no_driven_anywhere_in_its_file() {
    let text = saved(&edited_project(link_to_missing_video()));

    assert!(!text.contains("driven"), "{text}");
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
}

#[test]
fn the_numbers_read_from_masks_come_back_as_they_were_and_are_saved_again_as_they_were() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let text = saved(&project);

    let loaded = project_from_json_text(&text).unwrap();

    // (A project that is loaded has its tracks numbered by their places in the file: the front track is 0 and the back track 1.)
    assert_eq!(loaded.tracks()[0].id(), TrackId::new(0));
    let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
    assert_eq!(
        effect.driven_values(),
        &DrivenValues::from_places(vec![(
            0,
            DrivenValue::from_a_track(loaded.tracks()[0].id(), MaskChannel::Brightness, 0.25)
        )])
        .unwrap()
    );
    assert_eq!(saved(&loaded), text, "nothing is lost");
}

#[test]
fn two_numbers_of_one_effect_read_from_two_tracks_survive_a_trip_through_the_file() {
    let (mut project, _, back) = project_with_a_value_read_from_the_front_track();
    let middle = project.add_track_in_front();
    let clip = project.track(back).unwrap().placement_at_index(0).unwrap().id;
    let instance = project
        .chain_of_the_member(Member::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .instance();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            1,
            Some(DrivenValue::from_a_track(middle, MaskChannel::Red, 5.5)),
        )
        .unwrap();
    let text = saved(&project);

    let loaded = project_from_json_text(&text).unwrap();

    let effect = loaded.tracks()[2].entries()[0].effects().iter().next().unwrap().clone();
    assert_eq!(effect.driven_values().iter().count(), 2);
    assert_eq!(saved(&loaded), text);
}

#[test]
fn a_file_of_version_23_has_no_numbers_read_from_masks_and_is_saved_again_as_the_newest_version() {
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    const { assert!(FILE_FORMAT_VERSION >= 24, "numbers read from masks came with format 24") };
    let twenty_three = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(23));

    let loaded = project_from_json_text(&twenty_three).unwrap();

    assert_eq!(saved(&loaded), text);
}

fn text_with_the_driven_document_changed(change: impl FnOnce(&mut Value)) -> String {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let mut document = as_json(&saved(&project));
    change(&mut document["tracks"][1]["clips"][0]["effects"][0]["driven"]);
    document.to_string()
}

#[test]
fn a_number_that_the_effect_does_not_have_is_refused_with_its_id() {
    let text = text_with_the_driven_document_changed(|driven| {
        let one = driven["amount"].take();
        *driven = json!({ "no_such_number": one });
    });

    let reason = project_from_json_text(&text).unwrap_err().to_string();

    assert!(reason.contains("no_such_number"), "{reason}");
}

#[test]
fn a_value_that_is_not_a_number_with_a_range_is_refused() {
    let text = text_with_the_driven_document_changed(|driven| {
        let one = driven["amount"].take();
        *driven = json!({ "tint": one });
    });

    let reason = project_from_json_text(&text).unwrap_err().to_string();

    assert!(
        reason.contains("tint") && reason.contains("cannot be read from a mask"),
        "{reason}"
    );
}

#[test]
fn a_track_that_the_file_does_not_have_is_refused() {
    let text = text_with_the_driven_document_changed(|driven| driven["amount"]["source"]["track"] = json!(7));

    let reason = project_from_json_text(&text).unwrap_err().to_string();

    assert!(reason.contains("track") && reason.contains("does not have"), "{reason}");
}

#[test]
fn tracks_that_are_made_from_each_other_are_refused() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let mut document = as_json(&saved(&project));
    // The back track's first clip reads the front track. Give the front track a clip of its own that reads the back track: each is made from the other.
    let mut front_clip = document["tracks"][1]["clips"][0].clone();
    front_clip["effects"][0]["driven"]["amount"]["source"]["track"] = json!(1);
    document["tracks"][0]["clips"] = json!([front_clip]);

    let reason = project_from_json_text(&document.to_string()).unwrap_err().to_string();

    assert!(reason.contains("made from itself"), "{reason}");
}

#[test]
fn a_kind_of_source_this_version_does_not_know_a_channel_it_does_not_know_and_a_field_it_does_not_know_are_each_refused()
 {
    for (what, change) in [
        (
            "a source that is not a track",
            Box::new(|driven: &mut Value| driven["amount"]["source"] = json!({ "group": 0 }))
                as Box<dyn Fn(&mut Value)>,
        ),
        (
            "a channel it does not know",
            Box::new(|driven: &mut Value| driven["amount"]["channel"] = json!("alpha")),
        ),
        (
            "a field it does not know",
            Box::new(|driven: &mut Value| driven["amount"]["mystery"] = json!(1)),
        ),
        (
            "no number for where the mask has nothing",
            Box::new(|driven: &mut Value| {
                driven["amount"].as_object_mut().unwrap().remove("where_nothing");
            }),
        ),
    ] {
        let text = text_with_the_driven_document_changed(|driven| change(driven));
        assert!(project_from_json_text(&text).is_err(), "{what}");
    }
}

// ---- version 25: the graph that maps the mask's value (docs/shader-masks-design.md, 8)

use crate::project::{Graph, MaskMap};

/// The project of `project_with_a_value_read_from_the_front_track`, with a triangle for the map of the number.
fn project_with_a_graph() -> (Project, Graph) {
    let (mut project, front, back) = project_with_a_value_read_from_the_front_track();
    let triangle = Graph::from_points(&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.25)]).unwrap();
    let clip = project.track(back).unwrap().placement_at_index(0).unwrap().id;
    let instance = project
        .chain_of_the_member(Member::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .instance();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            0,
            Some(
                DrivenValue::from_a_track(front, MaskChannel::Brightness, 0.25).with_the_map(MaskMap::Graph(triangle)),
            ),
        )
        .unwrap();
    (project, triangle)
}

#[test]
fn a_graph_is_written_as_its_points_and_a_straight_map_writes_none() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let straight = as_json(&saved(&project));
    assert!(
        the_effect_of_the_back_tracks_first_clip(&straight)["driven"]["amount"]
            .get("graph")
            .is_none()
    );

    let (project, _) = project_with_a_graph();
    let with_a_graph = as_json(&saved(&project));

    assert_eq!(
        the_effect_of_the_back_tracks_first_clip(&with_a_graph)["driven"]["amount"]["graph"],
        json!([[0.0, 0.0], [0.5, 1.0], [1.0, 0.25]])
    );
}

#[test]
fn a_graph_is_read_back_as_it_was_and_nothing_is_lost() {
    let (project, triangle) = project_with_a_graph();
    let text = saved(&project);

    let loaded = project_from_json_text(&text).unwrap();

    let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
    let driven = effect.driven_values().iter().next().unwrap().1;
    assert_eq!(driven.map, MaskMap::Graph(triangle));
    assert_eq!(saved(&loaded), text);
}

#[test]
fn a_file_of_version_24_has_no_graph_and_every_number_is_mapped_straight() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let text = saved(&project);
    const { assert!(FILE_FORMAT_VERSION >= 25, "the graph came with format 25") };
    let twenty_four = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(24));

    let loaded = project_from_json_text(&twenty_four).unwrap();

    let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
    assert_eq!(effect.driven_values().iter().next().unwrap().1.map, MaskMap::Straight);
    assert_eq!(saved(&loaded), text);
}

#[test]
fn a_graph_that_is_not_one_is_refused_with_the_reason() {
    let bad: [(&str, Value); 5] = [
        ("one point", json!([[0.0, 0.5]])),
        ("it does not end at the place 1", json!([[0.0, 0.0], [0.9, 1.0]])),
        (
            "the places do not go up",
            json!([[0.0, 0.0], [0.6, 1.0], [0.4, 0.0], [1.0, 1.0]]),
        ),
        ("a height above 1", json!([[0.0, 0.0], [1.0, 2.0]])),
        (
            "nine points",
            json!([
                [0.0, 0.0],
                [0.1, 0.0],
                [0.2, 0.0],
                [0.3, 0.0],
                [0.4, 0.0],
                [0.5, 0.0],
                [0.6, 0.0],
                [0.7, 0.0],
                [1.0, 0.0]
            ]),
        ),
    ];
    for (why, graph) in bad {
        let (project, _) = project_with_a_graph();
        let mut document = as_json(&saved(&project));
        document["tracks"][1]["clips"][0]["effects"][0]["driven"]["amount"]["graph"] = graph;

        let reason = project_from_json_text(&document.to_string()).unwrap_err().to_string();

        assert!(reason.contains("amount") && reason.contains("graph"), "{why}: {reason}");
    }
}

#[test]
fn a_graph_that_has_no_room_among_the_numbers_of_the_shader_is_refused() {
    // A shader with 58 numbers has 4 + 58 = 62 of the 64 a call can have: the 16 of a graph do not fit.
    let floats: Vec<String> = (0..58)
        .map(|number| {
            format!("{{ \"NAME\": \"p{number}\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5 }}")
        })
        .collect();
    let text = format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}, {} ] }}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage) * p0; }}\n",
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
    document["tracks"][1]["clips"][0]["effects"][0]["driven"]["p0"]["graph"] = json!([[0.0, 0.0], [1.0, 1.0]]);

    let reason = project_from_json_text(&document.to_string()).unwrap_err().to_string();

    assert!(reason.contains("no room"), "{reason}");
}

#[test]
fn an_effect_with_many_values_and_no_graph_is_not_refused_for_want_of_room() {
    // (Found 2026-11-02: a plugin with hundreds of values was refused by the check of room for graphs.)
    let mut project = edited_project(link_to_missing_video());
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let kind = {
        let floats: Vec<String> = (0..60)
            .map(|number| format!("{{ \"NAME\": \"p{number}\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0 }}"))
            .collect();
        let text = format!(
            "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}, {} ] }}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage); }}\n",
            floats.join(", ")
        );
        let file = crate::isf::read(&text).unwrap();
        EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "huge.fs", &text)).unwrap()
    };
    project
        .add_effect(crate::project::AnimationTarget::Clip(clip), kind)
        .unwrap();

    assert!(project_from_json_text(&saved(&project)).is_ok());
}

// ---- version 26: whether the transitions of the mask see the real backdrop

#[test]
fn a_value_that_sees_the_real_backdrop_writes_nothing_of_it_and_one_that_does_not_writes_false() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let seeing = as_json(&saved(&project));
    assert!(
        the_effect_of_the_back_tracks_first_clip(&seeing)["driven"]["amount"]
            .get("real_backdrop")
            .is_none()
    );

    let (mut project, front, back) = project_with_a_value_read_from_the_front_track();
    let clip = project.track(back).unwrap().placement_at_index(0).unwrap().id;
    let instance = project
        .chain_of_the_member(Member::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .instance();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            0,
            Some(DrivenValue::from_a_track(front, MaskChannel::Brightness, 0.25).with_the_real_backdrop(false)),
        )
        .unwrap();
    let text = saved(&project);
    let not_seeing = as_json(&text);

    assert_eq!(
        the_effect_of_the_back_tracks_first_clip(&not_seeing)["driven"]["amount"]["real_backdrop"],
        json!(false)
    );
    let loaded = project_from_json_text(&text).unwrap();
    let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
    assert!(!effect.driven_values().iter().next().unwrap().1.real_backdrop);
    assert_eq!(saved(&loaded), text, "nothing is lost");
}

#[test]
fn a_file_of_version_25_has_no_setting_and_every_number_sees_the_real_backdrop() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let text = saved(&project);
    const { assert!(FILE_FORMAT_VERSION >= 26, "the setting came with format 26") };
    let twenty_five = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(25));

    let loaded = project_from_json_text(&twenty_five).unwrap();

    let effect = loaded.tracks()[1].entries()[0].effects().iter().next().unwrap().clone();
    assert!(effect.driven_values().iter().next().unwrap().1.real_backdrop);
    assert_eq!(saved(&loaded), text);
}

// ---- version 27: a clip group or a track group as the source

use crate::project::{GroupKind, MaskSource};

/// The project that has groups of every kind, with one value read from a group of the kind `kind`: from the clip group by the text object (track 0), or from the track group by the first video
/// (track 2). (Both at once would be a ring: the video is in the clip group, and the track group has the text's track.)
fn project_that_reads_a_group(kind: GroupKind) -> Project {
    let mut project = super::groups_tests::a_project_with_groups_of_every_kind();
    let group = project
        .groups()
        .iter()
        .find(|group| group.kind() == kind)
        .map(|group| group.id())
        .unwrap();
    let (clip, source) = match kind {
        GroupKind::ClipGroup => (project.tracks()[0].entries()[1].id(), MaskSource::ClipGroup(group)),
        _ => (project.tracks()[2].entries()[0].id(), MaskSource::TrackGroup(group)),
    };
    let instance = project
        .add_effect(crate::project::AnimationTarget::Clip(clip), tint())
        .unwrap();
    project
        .set_driven_value(
            Member::Clip(clip),
            instance,
            0,
            Some(DrivenValue {
                source,
                ..DrivenValue::from_a_track(project.tracks()[0].id(), MaskChannel::Red, 0.25)
            }),
        )
        .unwrap_or_else(|why| panic!("{source:?}: {why:?}"));
    project
}

/// Every `driven` object in `document`, wherever it is.
fn every_driven_object(document: &Value, found: &mut Vec<Value>) {
    match document {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "driven" {
                    found.extend(value.as_object().into_iter().flat_map(|one| one.values().cloned()));
                } else {
                    every_driven_object(value, found);
                }
            }
        }
        Value::Array(list) => list.iter().for_each(|value| every_driven_object(value, found)),
        _ => {}
    }
}

#[test]
fn a_group_as_the_source_is_written_as_its_kind_and_its_place_among_the_groups_and_comes_back_as_it_was() {
    for (kind, name) in [
        (GroupKind::ClipGroup, "clip_group"),
        (GroupKind::TrackGroup, "track_group"),
    ] {
        let project = project_that_reads_a_group(kind);
        let text = saved(&project);
        let place = project.groups().iter().position(|group| group.kind() == kind).unwrap();

        let mut written = Vec::new();
        every_driven_object(&as_json(&text), &mut written);

        assert_eq!(written.len(), 1, "{kind:?}");
        assert_eq!(written[0]["source"], json!({ name: place }), "{kind:?}");
        let loaded = project_from_json_text(&text).unwrap();
        assert_eq!(saved(&loaded), text, "{kind:?}: nothing is lost");
    }
}

#[test]
fn a_group_that_the_file_does_not_have_or_that_is_of_the_other_kind_is_refused_as_a_source() {
    let project = project_that_reads_a_group(GroupKind::ClipGroup);
    let place = project
        .groups()
        .iter()
        .position(|group| group.kind() == GroupKind::ClipGroup)
        .unwrap();
    let other_kind = project
        .groups()
        .iter()
        .position(|group| group.kind() == GroupKind::TrackGroup)
        .unwrap();
    let compact = serde_json::to_string(&as_json(&saved(&project))).unwrap();
    let from = format!("{{\"clip_group\":{place}}}");
    assert!(compact.contains(&from), "the source is in the file: {compact}");
    for (why, to) in [
        ("no such group", "{\"clip_group\":99}".to_owned()),
        (
            "a track group named as a clip group",
            format!("{{\"clip_group\":{other_kind}}}"),
        ),
    ] {
        let reason = project_from_json_text(&compact.replacen(&from, &to, 1))
            .unwrap_err()
            .to_string();

        assert!(reason.contains("group"), "{why}: {reason}");
    }
}

#[test]
fn a_file_of_version_26_has_only_tracks_as_sources_and_is_read_as_it_always_was() {
    let (project, ..) = project_with_a_value_read_from_the_front_track();
    let text = saved(&project);
    const { assert!(FILE_FORMAT_VERSION >= 27, "the groups as sources came with format 27") };
    let twenty_six = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(26));

    let loaded = project_from_json_text(&twenty_six).unwrap();

    assert_eq!(saved(&loaded), text);
}
