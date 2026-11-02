// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 24 of the project file added: `driven` in the document of an effect, the numbers of a shader that are read from a mask (a track), by the id of each number, with the place of
//! the track in the file's list of tracks, the channel, and what the number is where the mask has nothing; and that a file of version 23, which has none, is read as one where every number is
//! read from its slider. (`docs/shader-masks-design.md`.)

use serde_json::{Value, json};

use super::tests::{edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{DrivenValue, DrivenValues, EffectKind, MaskChannel, MaskSource, Member};

/// A shader with two numbers that have a range, `amount` (0 to 1) and `strength` (2 to 6), and a colour called `tint`.
const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"strength\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 6.0, \"DEFAULT\": 3.0 }, { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [1.0, 0.0, 0.0, 1.0] } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn tint() -> EffectKind {
    let file = crate::isf::read(TINT).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "tint.fs", TINT)).unwrap()
}

fn as_json(text: &str) -> Value {
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

fn the_effect_of_the_back_tracks_first_clip(document: &Value) -> &Value {
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
            DrivenValue {
                source: MaskSource::Track(loaded.tracks()[0].id()),
                channel: MaskChannel::Brightness,
                value_where_nothing: 0.25,
            }
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
