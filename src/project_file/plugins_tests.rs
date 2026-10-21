// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the types of effect that a project file has: the snapshot of the descriptor of each in `plugins`; an effect of a type that the program
//! does not have, which is kept with all its values and keyframes, and written back as it was; what this version does not know, which is kept;
//! when a snapshot is of an effect of the program and when it is not; and what is refused.

use serde_json::{Value, json};

use super::effects_tests::{reason_for_invalid_project, the_file_of_everything_with};
use super::tests::{project_with_everything, saved};
use super::*;
use crate::project::{Effect, EffectInstanceId, GroupKind, Member, PropertyId, TimelineFrameIndex};
use crate::test_support::a_kind_of_effect_the_program_does_not_have;

/// The snapshot of the hue shift that the program writes.
fn the_snapshot_of_the_hue_shift() -> Value {
    json!({
        "key": "builtin:hue_shift",
        "version": [1, 1],
        "name": "hue shift",
        "description": "Turns the colours round the wheel of colours",
        "category": "Colour",
        "domain": "picture",
        "changes_each_pixel_by_itself": true,
        "params": [{
            "id": "degrees", "name": "hue shift", "label": "Turn", "type": "number",
            "default": 0.0, "range": [-180.0, 180.0], "unit": "°", "decimals": 1
        }]
    })
}

/// The snapshot of an effect that the program does not have, with a value of each kind, and with things that this version of the program does not
/// know in it at the level of the effect and at that of a value.
fn a_snapshot_of(key: &str) -> Value {
    json!({
        "key": key,
        "version": [2, 3],
        "name": "made up",
        "description": "Not an effect of the program",
        "domain": "picture",
        "changes_each_pixel_by_itself": true,
        "params": [
            {"id": "amount", "name": "amount", "label": "Amount", "type": "number", "default": 0.5,
             "range": [0.0, 1.0], "decimals": 2, "easing": "logarithmic"},
            {"id": "point", "name": "point", "label": "Point", "type": "pair", "default": [0.5, 0.25]},
            {"id": "tint", "name": "tint", "label": "Tint", "type": "colour", "default": "#FF8000FF"},
            {"id": "on", "name": "on", "label": "On", "type": "flag", "default": true},
            {"id": "caption", "name": "caption", "label": "Caption", "type": "words", "default": "hello"}
        ],
        "inputs": [{"kind": "source"}]
    })
}

/// The file of the project of everything that has this snapshot among its `plugins` (after the snapshot of the hue shift, which is there
/// already, as the key of the hue shift comes before those of the others), and `change` made to it.
fn the_file_with_a_snapshot_and(snapshot: Value, change: impl FnOnce(&mut Value)) -> String {
    the_file_of_everything_with(|file| {
        file["plugins"].as_array_mut().unwrap().push(snapshot);
        change(file);
    })
}

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

// --- the snapshots a file has ---

#[test]
fn a_file_that_has_effects_has_a_snapshot_of_each_type_of_effect_that_it_uses_once() {
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    project.add_effect(video, EffectKind::HUE_SHIFT).unwrap();

    let file = as_json(&saved(&project));

    assert_eq!(
        file["plugins"],
        json!([the_snapshot_of_the_hue_shift()]),
        "one snapshot, though there are hue shifts on several things, and two on one"
    );
}

#[test]
fn a_file_that_has_no_effects_has_no_plugins() {
    let file = as_json(&saved(&super::tests::edited_project(
        super::tests::link_to_missing_video(),
    )));

    assert!(file.get("plugins").is_none(), "{file}");
}

#[test]
fn a_type_of_effect_that_only_a_group_uses_has_its_snapshot_too() {
    let mut project = project_with_everything();
    let clip = project.tracks()[2].entries()[0].id();
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap();
    let only_a_group_uses_it = a_kind_of_effect_the_program_does_not_have(
        "isf:only_a_group_has_this",
        &[("amount", PropertyValue::Number(0.5))],
    );
    project.add_effect_to_the_group(tag, only_a_group_uses_it).unwrap();
    for track in project.tracks().to_owned() {
        for entry in track.entries() {
            for instance in entry.effects().iter().map(Effect::instance).collect::<Vec<_>>() {
                project
                    .remove_effect(AnimationTarget::Clip(entry.id()), instance)
                    .unwrap();
            }
        }
        for instance in track.effects().iter().map(Effect::instance).collect::<Vec<_>>() {
            project
                .remove_effect(AnimationTarget::Track(track.id()), instance)
                .unwrap();
        }
    }

    let file = as_json(&saved(&project));

    let keys: Vec<&str> = file["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|plugin| plugin["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["isf:only_a_group_has_this"]);
}

// --- an effect that the program does not have ---

/// The file of the project of everything in which a made-up type of effect (see [`a_snapshot_of`]) is on the first video (after a hue shift), on
/// the music track, and in a rack of keyframes in the drawer of the video, with the values by their ids, and what this version does not know at
/// the level of an effect. It is written as the program would write it.
fn the_file_with_effects_of_a_type_the_program_does_not_have(key: &str) -> String {
    the_file_with_a_snapshot_and(a_snapshot_of(key), |file| {
        file["tracks"][2]["clips"][0]["effects"] = json!([
            {"effect": key, "version": [2, 3], "instance": 0,
             "values": {"amount": 0.75, "tint": "#102030FF", "caption": "bye", "point": [0.0, 1.0], "on": false},
             "state": {"seed": 4}},
            {"effect": "builtin:hue_shift", "version": [1, 1], "instance": 1}
        ]);
        file["tracks"][2]["clips"][0]["drawer"] = json!([
            {"property": "amount", "effect": 0, "keyframes": [{"frame": 10, "value": 0.25}, {"frame": 30, "value": 0.75}]}
        ]);
        file["tracks"][1]["effects"] = json!([{"effect": key, "version": [2, 3], "instance": 0}]);
    })
}

#[test]
fn an_effect_of_a_type_the_program_does_not_have_is_kept_with_its_values_and_keyframes_and_the_file_is_written_back_as_it_was()
 {
    let key = "isf:the_file_brings_this_one";
    let original = the_file_with_effects_of_a_type_the_program_does_not_have(key);

    let loaded = project_from_json_text(&original).unwrap();

    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let chain = loaded.effects_of(video).unwrap();
    assert_eq!(chain.len(), 2);
    let foreign = chain.instance(EffectInstanceId::FIRST).unwrap();
    assert_eq!(foreign.kind().key(), key);
    assert_eq!(foreign.kind().version(), (2, 3));
    assert!(!foreign.kind().is_installed(), "the program does not have it");
    assert_eq!(
        foreign.values(),
        [
            PropertyValue::Number(0.75),
            PropertyValue::Pair(0.0, 1.0),
            PropertyValue::Colour(crate::project::TextColour::new(0x10, 0x20, 0x30, 0xFF)),
            PropertyValue::Flag(false),
            PropertyValue::Words("bye".to_owned()),
        ],
        "each value is what the file says, of the kind that the snapshot says, in the place the snapshot has it"
    );
    assert_eq!(
        foreign.unknown_fields().fields().get("state"),
        Some(&json!({"seed": 4})),
        "what the file says of the effect that the program does not know is kept"
    );
    assert_eq!(
        foreign.kind().descriptor().unknown_fields.fields().get("inputs"),
        Some(&json!([{"kind": "source"}])),
        "and of its type"
    );
    assert_eq!(
        foreign.kind().parameters()[0].unknown_fields.fields().get("easing"),
        Some(&json!("logarithmic")),
        "and of a value of it"
    );
    let amount = foreign.property_with_id("amount").unwrap();
    assert_eq!(
        loaded.property_value_at(video, amount, TimelineFrameIndex::new(20)),
        Some(PropertyValue::Number(0.5)),
        "its keyframes are the project's, and go from the one to the other as any do"
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    assert_eq!(
        foreign.kind().colour_matrix(foreign.values()),
        None,
        "it changes no colour"
    );
    assert!(
        chain.instance(EffectInstanceId::new(1)).unwrap().kind().is_installed(),
        "the hue shift next to it is the program's"
    );

    assert_eq!(
        as_json(&saved(&loaded)),
        as_json(&original),
        "nothing is lost, and nothing is added: it is the file it was"
    );
}

#[test]
fn what_is_kept_of_an_effect_the_program_does_not_have_is_kept_by_an_edit_of_the_effect_next_to_it_and_by_a_copy_of_its_track()
 {
    let original = the_file_with_effects_of_a_type_the_program_does_not_have("isf:the_file_brings_this_one_too");
    let mut project = project_from_json_text(&original).unwrap();
    let video_track = project.tracks()[2].id();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    project
        .set_property_at(
            video,
            PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(1), 0),
            PropertyValue::Number(30.0),
            TimelineFrameIndex::new(0),
        )
        .unwrap();

    let copy = project
        .duplicate_track(video_track, crate::project::TrackSide::Above)
        .expect("a track can be copied");

    let written = as_json(&saved(&project));
    let first_clip_of = |position: usize| &written["tracks"][position]["clips"][0];
    let original_effects = &as_json(&original)["tracks"][2]["clips"][0]["effects"];
    assert_eq!(
        first_clip_of(3)["effects"][0],
        original_effects[0],
        "the effect that this version does not have is as it was"
    );
    assert_eq!(
        first_clip_of(3)["effects"][1]["values"],
        json!({"degrees": 30.0}),
        "and the effect next to it is as it was changed"
    );
    assert_eq!(
        first_clip_of(2)["effects"],
        first_clip_of(3)["effects"],
        "a copy of the track has both effects of the clip, as they are"
    );
    let position_of_the_copy = project.tracks().iter().position(|track| track.id() == copy).unwrap();
    let first_clip_of_the_copy = &project.tracks()[position_of_the_copy].entries()[0];
    assert_eq!(first_clip_of_the_copy.effects().len(), 2);
    assert_eq!(
        first_clip_of_the_copy
            .effects()
            .iter()
            .next()
            .unwrap()
            .unknown_fields()
            .fields()
            .get("state"),
        Some(&json!({"seed": 4}))
    );
}

#[test]
fn a_value_that_a_file_gives_an_effect_that_is_what_it_begins_with_is_left_out_when_the_file_is_written_again() {
    let key = "isf:begins_with_what_is_said";
    let text = the_file_with_a_snapshot_and(a_snapshot_of(key), |file| {
        file["tracks"][1]["effects"] = json!([
            {"effect": key, "version": [2, 3], "instance": 0, "values": {"amount": 0.5, "on": false}}
        ]);
    });

    let project = project_from_json_text(&text).unwrap();

    let written = as_json(&saved(&project));
    assert_eq!(
        written["tracks"][1]["effects"][0]["values"],
        json!({"on": false}),
        "the amount is 0.5, which is what it begins as, and is not written"
    );
}

#[test]
fn the_values_of_an_effect_are_found_by_their_ids_and_not_by_their_places_in_the_snapshot() {
    let key = "isf:ids_not_places";
    let in_order = a_snapshot_of(key);
    let mut reversed = a_snapshot_of(key);
    reversed["params"].as_array_mut().unwrap().reverse();
    let file_with = |snapshot: Value| {
        the_file_with_a_snapshot_and(snapshot, |file| {
            file["tracks"][2]["clips"][0]["effects"] = json!([
                {"effect": key, "version": [2, 3], "instance": 0, "values": {"amount": 0.75, "caption": "bye"}}
            ]);
            file["tracks"][2]["clips"][0]["drawer"] = json!([
                {"property": "amount", "effect": 0, "keyframes": [{"frame": 10, "value": 0.25}]}
            ]);
        })
    };

    let first = project_from_json_text(&file_with(in_order)).unwrap();
    let second = project_from_json_text(&file_with(reversed)).unwrap();

    for project in [&first, &second] {
        let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
        let effect = project.effects_of(video).unwrap().iter().next().unwrap();
        let amount = effect.property_with_id("amount").unwrap();
        let caption = effect.property_with_id("caption").unwrap();
        assert_eq!(effect.value_of(caption), Some(&PropertyValue::Words("bye".to_owned())));
        assert_eq!(
            project.property_value_at(video, amount, TimelineFrameIndex::new(0)),
            Some(PropertyValue::Number(0.25)),
            "the keyframe is of the amount, whichever place it is in"
        );
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
    }
    let places = |project: &Project| {
        let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
        project
            .effects_of(video)
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .kind()
            .slot_of_the_value_with_id("amount")
    };
    assert_eq!(
        (places(&first), places(&second)),
        (Some(0), Some(4)),
        "the places are not the same, and the ids are"
    );
}

// --- when the program stands in for a snapshot ---

#[test]
fn a_snapshot_of_the_hue_shift_of_another_minor_version_that_has_nothing_the_program_has_not_makes_the_effects_of_the_program()
 {
    let mut older = the_snapshot_of_the_hue_shift();
    older["version"] = json!([1, 7]);
    older["params"][0]["label"] = json!("What an earlier program called it");
    let text = the_file_of_everything_with(|file| {
        file["plugins"] = json!([older]);
        file["tracks"][0]["effects"][0]["version"] = json!([1, 7]);
        file["tracks"][2]["clips"][0]["effects"][0]["version"] = json!([1, 7]);
        file["tracks"][2]["clips"][1]["effects"][0]["version"] = json!([1, 7]);
        file["tracks"][0]["clips"][0]["effects"][0]["version"] = json!([1, 7]);
    });

    let project = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let effect = project.effects_of(video).unwrap().iter().next().unwrap();
    assert!(
        effect.kind().is_installed(),
        "it is the effect of the program, which does what it does"
    );
    assert_eq!(effect.kind(), EffectKind::HUE_SHIFT);
    let written = as_json(&saved(&project));
    assert_eq!(
        written["plugins"],
        json!([the_snapshot_of_the_hue_shift()]),
        "it is written as the program has it now"
    );
    assert_eq!(written["tracks"][2]["clips"][0]["effects"][0]["version"], json!([1, 1]));
}

#[test]
fn a_snapshot_of_the_hue_shift_of_another_major_version_is_an_effect_the_program_does_not_have_and_is_kept_as_it_is() {
    let snapshot = json!({
        "key": "builtin:hue_shift", "version": [2, 0], "name": "hue shift", "domain": "picture",
        "params": [{"id": "angle", "name": "angle", "label": "Angle", "type": "number", "default": 10.0}]
    });
    let effects = json!([
        {"effect": "builtin:hue_shift", "version": [2, 0], "instance": 0, "values": {"angle": 12.0}}
    ]);
    let text = the_file_of_everything_with(|file| {
        file["plugins"] = json!([snapshot.clone()]);
        file["tracks"][1]["effects"] = effects.clone();
        file["tracks"][0].as_object_mut().unwrap().remove("effects");
        file["tracks"][0]["clips"][0].as_object_mut().unwrap().remove("effects");
        file["tracks"][2]["clips"][0].as_object_mut().unwrap().remove("effects");
        file["tracks"][2]["clips"][1].as_object_mut().unwrap().remove("effects");
    });

    let project = project_from_json_text(&text).unwrap();

    let music = AnimationTarget::Track(project.tracks()[1].id());
    let effect = project.effects_of(music).unwrap().iter().next().unwrap();
    assert!(!effect.kind().is_installed());
    assert_ne!(effect.kind(), EffectKind::HUE_SHIFT);
    assert_eq!(effect.values(), [PropertyValue::Number(12.0)]);
    let written = as_json(&saved(&project));
    assert_eq!(written["plugins"], json!([snapshot]));
    assert_eq!(written["tracks"][1]["effects"], effects);
}

#[test]
fn an_effect_of_the_program_whose_snapshot_the_file_does_not_have_is_the_effect_of_the_program() {
    let text = the_file_of_everything_with(|file| {
        file.as_object_mut().unwrap().remove("plugins");
    });

    let project = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(project.tracks()[2].entries()[1].id());
    let effect = project.effects_of(video).unwrap().iter().next().unwrap();
    assert_eq!(effect.kind(), EffectKind::HUE_SHIFT);
    assert_eq!(
        effect.value_of(PropertyId::effect_value(
            EffectKind::HUE_SHIFT,
            EffectInstanceId::FIRST,
            0
        )),
        Some(&PropertyValue::Number(90.0))
    );
    assert!(saved(&project).contains("\"plugins\""), "and the snapshot is written");
}

// --- what is refused ---

#[test]
fn what_is_wrong_with_the_plugins_of_a_file_is_refused_and_the_reason_says_which_and_what() {
    let key = "isf:what_is_wrong";
    let with_the_snapshot_changed = |change: &dyn Fn(&mut Value)| {
        let mut snapshot = a_snapshot_of(key);
        change(&mut snapshot);
        reason_for_invalid_project(&the_file_with_a_snapshot_and(snapshot, |_| {}))
    };

    let reason = with_the_snapshot_changed(&|snapshot| snapshot["key"] = json!("no namespace"));
    assert!(
        reason.contains("plugin number 2") && reason.contains("is not a namespace, a colon and an id"),
        "{reason}"
    );
    let reason = with_the_snapshot_changed(&|snapshot| snapshot["params"][1]["id"] = json!("amount"));
    assert!(reason.contains("two values with the id \"amount\""), "{reason}");
    let reason = with_the_snapshot_changed(&|snapshot| snapshot["params"][0]["default"] = json!("a word"));
    assert!(
        reason.contains("the value \"amount\"") && reason.contains("where a number is wanted"),
        "{reason}"
    );
    let reason = with_the_snapshot_changed(&|snapshot| snapshot["params"][0]["range"] = json!([1.0, 0.0]));
    assert!(reason.contains("not a least that is no more than a most"), "{reason}");
    let reason = with_the_snapshot_changed(&|snapshot| snapshot["name"] = json!(""));
    assert!(reason.contains("has no name"), "{reason}");

    // The same key and version twice.
    let twice = the_file_of_everything_with(|file| {
        let plugins = file["plugins"].as_array_mut().unwrap();
        plugins.push(a_snapshot_of(key));
        plugins.push(a_snapshot_of(key));
    });
    let reason = reason_for_invalid_project(&twice);
    assert!(
        reason.contains("\"isf:what_is_wrong\" version 2.3 is described twice"),
        "{reason}"
    );
}

#[test]
fn an_effect_that_a_file_has_wrong_is_refused_and_the_clip_or_track_is_named() {
    let key = "isf:the_effects_are_wrong";
    let reason_for = |effects: Value| {
        reason_for_invalid_project(&the_file_with_a_snapshot_and(a_snapshot_of(key), |file| {
            file["tracks"][1]["effects"] = effects;
        }))
    };

    let reason = reason_for(json!([{"effect": "isf:nobody_described_this", "instance": 0}]));
    assert!(
        reason.contains("is not described in `plugins`, and the program does not have it"),
        "{reason}"
    );
    assert!(reason.contains("track number 2"), "{reason}");

    let reason = reason_for(json!([{"effect": key, "version": [3, 0], "instance": 0}]));
    assert!(
        reason.contains("is not described in `plugins`"),
        "a snapshot of another version is not the one it says: {reason}"
    );

    let reason = reason_for(json!([{"effect": "builtin:hue_shift", "version": [2, 0], "instance": 0}]));
    assert!(reason.contains("is not described in `plugins`"), "{reason}");

    let reason = reason_for(json!([{"effect": key, "version": [2, 3], "instance": 0, "values": {"nothing": 1.0}}]));
    assert!(reason.contains("has no value with the id \"nothing\""), "{reason}");

    let reason = reason_for(json!([{"effect": key, "version": [2, 3], "instance": 0, "values": {"amount": "much"}}]));
    assert!(reason.contains("words, where a number is wanted"), "{reason}");

    let reason = reason_for(json!([{"effect": key, "version": [2, 3], "instance": 0, "values": [0.5]}]));
    assert!(reason.contains("a list, which only an old file has"), "{reason}");

    let reason = reason_for(json!([
        {"effect": key, "version": [2, 3], "instance": 0},
        {"effect": key, "version": [2, 3], "instance": 0}
    ]));
    assert!(reason.contains("two effects that are named effect 0"), "{reason}");
}

#[test]
fn an_effect_that_does_not_say_its_version_is_the_one_snapshot_of_its_key_and_is_refused_when_there_are_several() {
    let key = "isf:which_version";
    let mut other_version = a_snapshot_of(key);
    other_version["version"] = json!([1, 0]);
    let just_the_key = json!([{"effect": key, "instance": 0, "values": {"amount": 0.75}}]);

    let one = the_file_with_a_snapshot_and(a_snapshot_of(key), |file| {
        file["tracks"][1]["effects"] = just_the_key.clone()
    });
    let project = project_from_json_text(&one).unwrap();
    let music = AnimationTarget::Track(project.tracks()[1].id());
    assert_eq!(
        project
            .effects_of(music)
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .kind()
            .version(),
        (2, 3)
    );

    let two = the_file_of_everything_with(|file| {
        let plugins = file["plugins"].as_array_mut().unwrap();
        plugins.push(a_snapshot_of(key));
        plugins.push(other_version);
        file["tracks"][1]["effects"] = just_the_key;
    });
    let reason = reason_for_invalid_project(&two);
    assert!(
        reason.contains("does not say which version it is of, and the file has 2 of them"),
        "{reason}"
    );
}

#[test]
fn a_rack_that_names_a_value_the_effect_has_not_is_refused_by_the_id() {
    let key = "isf:the_racks_are_wrong";
    let text = the_file_with_a_snapshot_and(a_snapshot_of(key), |file| {
        file["tracks"][2]["clips"][0]["effects"] = json!([{"effect": key, "version": [2, 3], "instance": 0}]);
        file["tracks"][2]["clips"][0]["drawer"] = json!([
            {"property": "volume", "effect": 0, "keyframes": [{"frame": 10, "value": 0.25}]}
        ]);
    });

    let reason = reason_for_invalid_project(&text);

    assert!(reason.contains("\"volume\" cannot be animated"), "{reason}");
    assert!(
        reason.contains("it does not have effect 0 that has a value of that name"),
        "{reason}"
    );
}

#[test]
fn a_value_that_a_snapshot_does_not_name_or_label_is_named_by_its_id_and_labelled_by_its_name() {
    let key = "isf:names_and_labels";
    let snapshot = json!({
        "key": key, "version": [1, 1], "name": "names and labels", "domain": "picture",
        "params": [
            {"id": "plain", "type": "number", "default": 1.0},
            {"id": "named", "name": "what it is called", "type": "number", "default": 2.0},
            {"id": "both", "name": "called", "label": "What its control says", "type": "number", "default": 3.0}
        ]
    });
    let text = the_file_with_a_snapshot_and(snapshot, |file| {
        file["tracks"][1]["effects"] = json!([{"effect": key, "version": [1, 1], "instance": 0}]);
    });

    let project = project_from_json_text(&text).unwrap();

    let music = AnimationTarget::Track(project.tracks()[1].id());
    let kind = project.effects_of(music).unwrap().iter().next().unwrap().kind();
    let names_and_labels: Vec<(&str, &str)> = kind
        .parameters()
        .iter()
        .map(|parameter| (&*parameter.name, &*parameter.label))
        .collect();
    assert_eq!(
        names_and_labels,
        [
            ("plain", "plain"),
            ("what it is called", "what it is called"),
            ("called", "What its control says")
        ]
    );
}

// --- an effect that is a sound plugin (phase 7, piece 7.5) ---

#[test]
fn a_sound_plugin_on_a_video_with_keyframes_is_written_with_its_snapshot_read_back_whole_and_installed_when_the_plugin_is()
 {
    use crate::project::{AnimationTarget, EffectKind, set_installed_plugins};
    use crate::test_support::{
        an_installed_sound_plugin, make_a_keyframe, put_an_effect_on, the_installed_plugins_to_myself,
    };

    let _alone = the_installed_plugins_to_myself();
    let plugin = an_installed_sound_plugin(
        "ladspa:tap_reverb:tap_reverb",
        "Tom Szilagyi",
        &[("decay_time", 2500.0, Some((0.0, 10_000.0))), ("dry_level", -2.0, None)],
    );
    set_installed_plugins(vec![plugin.clone()]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let instance = put_an_effect_on(
        &mut project,
        video,
        kind,
        &[PropertyValue::Number(1200.0), PropertyValue::Number(-6.0)],
    );
    let decay = PropertyId::effect_value(kind, instance, 0);
    make_a_keyframe(&mut project, video, decay, 10, PropertyValue::Number(100.0));
    make_a_keyframe(&mut project, video, decay, 50, PropertyValue::Number(900.0));

    let text = saved(&project);
    let file = as_json(&text);
    let snapshot = file["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|snapshot| snapshot["key"] == "ladspa:tap_reverb:tap_reverb")
        .expect("the type of effect has its snapshot in the file");
    assert_eq!(snapshot["domain"], "sound");
    assert_eq!(snapshot["category"], "Tom Szilagyi");
    assert!(
        snapshot["changes_each_pixel_by_itself"] != true,
        "a sound effect changes no pixel"
    );
    assert_eq!(snapshot["params"][0]["id"], "decay_time");
    assert_eq!(snapshot["params"][0]["range"], json!([0.0, 10_000.0]));

    // The plugin is not here when the file is read: the effect is kept whole, with its keyframes, and the file is what it was.
    set_installed_plugins(Vec::new());
    let loaded = project_from_json_text(&text).unwrap();
    let there = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let chain = loaded.effects_of(there).unwrap();
    let effect = chain.instance(instance).unwrap();
    assert_eq!(effect.kind().key(), "ladspa:tap_reverb:tap_reverb");
    assert!(!effect.kind().is_installed());
    assert_eq!(
        effect.values(),
        [PropertyValue::Number(1200.0), PropertyValue::Number(-6.0)]
    );
    let decay_there = effect.property_with_id("decay_time").unwrap();
    assert_eq!(
        loaded.property_value_at(there, decay_there, TimelineFrameIndex::new(30)),
        Some(PropertyValue::Number(500.0)),
        "the keyframes are the project's"
    );
    assert_eq!(as_json(&saved(&loaded)), file, "nothing is lost, and nothing is added");

    // The plugin is found: the same effect works.
    set_installed_plugins(vec![plugin]);
    assert!(effect.kind().is_installed());
    let loaded_with_the_plugin = project_from_json_text(&text).unwrap();
    let again = loaded_with_the_plugin
        .effects_of(AnimationTarget::Clip(
            loaded_with_the_plugin.tracks()[2].entries()[0].id(),
        ))
        .unwrap()
        .instance(instance)
        .unwrap()
        .kind();
    assert!(again.is_installed());
    assert_eq!(
        again,
        EffectKind::installed_plugin_kinds()[0],
        "the plugin itself is the kind when it is found first"
    );
}

// --- conformance (phase 7, piece 7.7): what a project has of a sound plugin that is not what it was is kept, byte for byte ---

#[test]
fn an_effect_of_a_plugin_whose_ports_changed_is_kept_whole_and_the_file_is_written_back_as_it_was_and_the_plugin_that_has_not_changed_is_the_kind()
 {
    use crate::project::{AnimationTarget, EffectKind, set_installed_plugins};
    use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

    let _alone = the_installed_plugins_to_myself();
    let before = an_installed_sound_plugin(
        "ladspa:tap_reverb:tap_reverb",
        "Tom Szilagyi",
        &[("decay_time", 2500.0, None), ("dry_level", -2.0, None)],
    );
    set_installed_plugins(vec![before.clone()]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let instance = put_an_effect_on(
        &mut project,
        video,
        kind,
        &[PropertyValue::Number(1200.0), PropertyValue::Number(-6.0)],
    );
    let text = saved(&project);

    // The plugin of that name is found again, but its ports are not the same (a value was renamed, so the snapshot has one that it has not): it cannot stand in.
    let after = an_installed_sound_plugin(
        "ladspa:tap_reverb:tap_reverb",
        "Tom Szilagyi",
        &[("decay_time", 2500.0, None), ("wet_level", 0.0, None)],
    );
    set_installed_plugins(vec![after]);
    let loaded = project_from_json_text(&text).unwrap();
    let there = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let effect = loaded.effects_of(there).unwrap().instance(instance).unwrap().clone();
    assert!(
        !effect.kind().is_installed(),
        "a plugin whose ports changed is another plugin"
    );
    assert_eq!(
        effect.values(),
        [PropertyValue::Number(1200.0), PropertyValue::Number(-6.0)],
        "and the effect has its values"
    );
    assert_eq!(
        as_json(&saved(&loaded)),
        as_json(&text),
        "nothing is lost, and nothing is added"
    );

    // The plugin as it was is found: the same file makes the effect work.
    set_installed_plugins(vec![before]);
    let loaded_again = project_from_json_text(&text).unwrap();
    let kind_again = loaded_again
        .effects_of(AnimationTarget::Clip(loaded_again.tracks()[2].entries()[0].id()))
        .unwrap()
        .instance(instance)
        .unwrap()
        .kind();
    assert!(kind_again.is_installed());
    assert_eq!(as_json(&saved(&loaded_again)), as_json(&text));
}

// --- conformance (phase 7, piece 7.8.6): what a project has of a CLAP plugin that is gone or is not what it was is kept, byte for byte ---

/// A CLAP plugin as the scan makes it an effect, with a parameter of each kind that becomes a value of its own kind: a number on a logarithmic slider, a whole number, a choice
/// (an enum, whose options are named by the numbers) and a switch (a bypass).
fn a_clap_plugin_with_a_value_of_each_kind(parameter_ids: [u32; 4]) -> crate::project::InstalledPlugin {
    use clap_host::scan::{AudioPort, Parameter, ScannedPlugin};
    let port = AudioPort {
        id: 0,
        name: "Main".to_owned(),
        channel_count: 2,
        port_type: Some("stereo".to_owned()),
        is_main: true,
    };
    let parameter =
        |id: u32, name: &str, minimum: f64, maximum: f64, default: f64, flags: u32, texts: &[&str]| Parameter {
            id,
            name: name.to_owned(),
            module: String::new(),
            minimum,
            maximum,
            default,
            flags,
            texts: texts.iter().map(|text| (*text).to_owned()).collect(),
        };
    const STEPPED: u32 = 1;
    const BYPASS: u32 = 1 << 4;
    const ENUM: u32 = 1 << 16;
    let scanned = ScannedPlugin {
        description: clap_host::PluginDescription {
            id: "com.example.multi".to_owned(),
            name: "Multi".to_owned(),
            vendor: "Example Maker".to_owned(),
            ..clap_host::PluginDescription::default()
        },
        audio_inputs: vec![port.clone()],
        audio_outputs: vec![port],
        parameters: vec![
            parameter(parameter_ids[0], "Cutoff", 20.0, 20_000.0, 1_000.0, 0, &[]),
            parameter(parameter_ids[1], "Voices", 1.0, 8.0, 2.0, STEPPED, &[]),
            parameter(
                parameter_ids[2],
                "Mode",
                0.0,
                2.0,
                1.0,
                STEPPED | ENUM,
                &["soft", "hard", "wild"],
            ),
            parameter(parameter_ids[3], "Bypass", 0.0, 1.0, 0.0, STEPPED | BYPASS, &[]),
        ],
        latency: 0,
        tail: Some(48_000),
        has_state: false,
        start: clap_host::start_check::StartCheck::NotChecked,
        problem: None,
    };
    crate::plugin_scan::effect_of_a_clap_plugin(std::path::Path::new("/plugins/multi.clap"), &scanned)
        .expect("an effect")
}

#[test]
fn a_clap_effect_with_a_number_a_whole_number_a_choice_and_a_switch_is_kept_whole_where_the_plugin_is_gone_and_written_back_byte_for_byte()
 {
    use crate::project::{AnimationTarget, EffectKind, set_installed_plugins};
    use crate::test_support::{make_a_keyframe, put_an_effect_on, the_installed_plugins_to_myself};

    let _alone = the_installed_plugins_to_myself();
    let plugin = a_clap_plugin_with_a_value_of_each_kind([10, 11, 12, 13]);
    set_installed_plugins(vec![plugin.clone()]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    assert_eq!(kind.key(), "clap:com.example.multi");
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let values = [
        PropertyValue::Number(440.0),
        PropertyValue::Number(4.0),
        PropertyValue::Words("2".to_owned()),
        PropertyValue::Flag(true),
    ];
    let instance = put_an_effect_on(&mut project, video, kind, &values);
    let cutoff = PropertyId::effect_value(kind, instance, 0);
    make_a_keyframe(&mut project, video, cutoff, 10, PropertyValue::Number(100.0));
    make_a_keyframe(&mut project, video, cutoff, 50, PropertyValue::Number(9_000.0));
    let text = saved(&project);
    let file = as_json(&text);
    let snapshot = file["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|snapshot| snapshot["key"] == "clap:com.example.multi")
        .expect("the type of effect has its snapshot in the file");
    assert_eq!(snapshot["category"], "Example Maker");
    assert_eq!(
        snapshot["params"]
            .as_array()
            .unwrap()
            .iter()
            .map(|parameter| parameter["id"].clone())
            .collect::<Vec<_>>(),
        [json!("param10"), json!("param11"), json!("param12"), json!("param13")],
        "a value is named by the id the plugin gave the parameter, and not by its name"
    );
    assert_eq!(
        snapshot["params"][2]["type"], "words",
        "a choice is words, an option named by its number"
    );

    // The plugin is not here when the file is read: the effect is kept whole, and the file is what it was.
    set_installed_plugins(Vec::new());
    let loaded = project_from_json_text(&text).unwrap();
    let there = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let effect = loaded.effects_of(there).unwrap().instance(instance).unwrap().clone();
    assert_eq!(effect.kind().key(), "clap:com.example.multi");
    assert!(!effect.kind().is_installed());
    assert_eq!(effect.values(), values);
    assert_eq!(as_json(&saved(&loaded)), file, "nothing is lost, and nothing is added");
    assert_eq!(saved(&loaded), text, "byte for byte");

    // The plugin is found: the same effect works, and the file is the same again.
    set_installed_plugins(vec![plugin]);
    let found = project_from_json_text(&text).unwrap();
    let kind_again = found
        .effects_of(AnimationTarget::Clip(found.tracks()[2].entries()[0].id()))
        .unwrap()
        .instance(instance)
        .unwrap()
        .kind();
    assert!(kind_again.is_installed());
    assert_eq!(saved(&found), text, "byte for byte");
}

#[test]
fn a_clap_effect_of_a_plugin_whose_parameter_ids_changed_is_another_plugin_and_the_effect_is_kept_whole() {
    use crate::project::{AnimationTarget, EffectKind, set_installed_plugins};
    use crate::test_support::{put_an_effect_on, the_installed_plugins_to_myself};

    let _alone = the_installed_plugins_to_myself();
    let before = a_clap_plugin_with_a_value_of_each_kind([10, 11, 12, 13]);
    set_installed_plugins(vec![before.clone()]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let values = [
        PropertyValue::Number(440.0),
        PropertyValue::Number(4.0),
        PropertyValue::Words("0".to_owned()),
        PropertyValue::Flag(false),
    ];
    let instance = put_an_effect_on(&mut project, video, kind, &values);
    let text = saved(&project);

    // The plugin of the same id is found again, but one of its parameters has another id (a new version of it): it cannot stand in, whatever the names are.
    set_installed_plugins(vec![a_clap_plugin_with_a_value_of_each_kind([10, 11, 99, 13])]);
    let loaded = project_from_json_text(&text).unwrap();
    let effect = loaded
        .effects_of(AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id()))
        .unwrap()
        .instance(instance)
        .unwrap()
        .clone();
    assert!(
        !effect.kind().is_installed(),
        "a plugin whose parameter ids changed is another plugin"
    );
    assert_eq!(effect.values(), values, "and the effect has its values");
    assert_eq!(
        saved(&loaded),
        text,
        "nothing is lost, and nothing is added: byte for byte"
    );

    // A plugin that has the same ids and more besides can stand in (the snapshot is a part of it).
    set_installed_plugins(vec![before]);
    let again = project_from_json_text(&text).unwrap();
    assert!(
        again
            .effects_of(AnimationTarget::Clip(again.tracks()[2].entries()[0].id()))
            .unwrap()
            .instance(instance)
            .unwrap()
            .kind()
            .is_installed()
    );
    assert_eq!(saved(&again), text);
}

#[test]
fn a_clap_effect_whose_plugin_kind_of_value_changed_is_another_plugin_too() {
    use crate::project::{AnimationTarget, EffectKind, set_installed_plugins};
    use crate::test_support::{put_an_effect_on, the_installed_plugins_to_myself};

    let _alone = the_installed_plugins_to_myself();
    set_installed_plugins(vec![a_clap_plugin_with_a_value_of_each_kind([10, 11, 12, 13])]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let instance = put_an_effect_on(
        &mut project,
        video,
        kind,
        &[
            PropertyValue::Number(440.0),
            PropertyValue::Number(4.0),
            PropertyValue::Words("1".to_owned()),
            PropertyValue::Flag(true),
        ],
    );
    let text = saved(&project);

    // The parameter 12 was an enum (words) and is now a number: the value is not the kind it was.
    let mut changed = a_clap_plugin_with_a_value_of_each_kind([10, 11, 12, 13]);
    let parameters = changed.descriptor.parameters.to_mut();
    parameters[2].default = PropertyValue::Number(1.0);
    parameters[2].control = crate::project::ParameterControl::Automatic;
    parameters[2].range = Some((0.0, 2.0));
    set_installed_plugins(vec![changed]);
    let loaded = project_from_json_text(&text).unwrap();
    let effect = loaded
        .effects_of(AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id()))
        .unwrap()
        .instance(instance)
        .unwrap()
        .clone();

    assert!(!effect.kind().is_installed());
    assert_eq!(saved(&loaded), text, "byte for byte");
}
