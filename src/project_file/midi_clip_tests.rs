// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 21 of the project file added (phase 13, piece 13.3; docs/engine-phase-13-midi-design.md): the MIDI clip, which names a resource of the kind `midi` (a Standard MIDI File that
//! the project keeps, linked or included), and the flag in the snapshot of an effect that is a synthesizer.

use std::sync::Arc;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::archive::project_resource_entry_name;
use super::effects_tests::reason_for_invalid_project;
use super::tests::{edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{
    EffectKind, Gain, MidiClip, ResourceKind, SourceFrameIndex, SpeedFactor, TimelineFrameIndex, set_installed_plugins,
};
use crate::test_support::{
    a_midi_file, an_installed_sound_plugin, put_a_hue_shift_on, put_an_effect_on, the_installed_plugins_to_myself,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn json_of(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn a_midi_clip() -> MidiClip {
    MidiClip::new(
        "tune.mid",
        2,
        SourceFrameIndex::new(0),
        SourceFrameIndex::new(90),
        SpeedFactor::new(3).unwrap(),
    )
    .unwrap()
}

/// A project (see `edited_project`) that has a track in front with the MIDI clip `clip` from frame 10, and the resource `tune.mid` linked to the file at `path` (which is written).
fn a_project_with_a_midi_clip(clip: MidiClip, path: &Path) -> Project {
    fs::write(path, &*a_midi_file()).unwrap();
    let mut project = edited_project(link_to_missing_video());
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            a_midi_file(),
            SourceMediaLink::from_absolute_path(path.to_owned()),
        )
        .unwrap();
    let track = project.add_track_in_front();
    project.try_insert_midi_clip(track, frame(10), clip).unwrap();
    project
}

/// The document of the clip of the first track, the one in front.
fn the_document_of_the_midi_clip(file: &Value) -> &Value {
    file["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|track| track["clips"].as_array().unwrap())
        .find(|clip| clip["kind"] == "midi")
        .expect("the file has a MIDI clip")
}

#[test]
fn a_midi_clip_is_saved_with_the_name_of_its_file_its_track_and_its_run_and_read_back_as_it_was_with_its_file() {
    let folder = TempDir::new().unwrap();
    let clip = a_midi_clip().with_gain(Gain::from_decibels(-6.0)).with_muted(true);
    let project = a_project_with_a_midi_clip(clip.clone(), &folder.path().join("tune.mid"));
    let destination = folder.path().join("project.json");

    write_project_file(&project, &destination).unwrap();

    let file = json_of(&destination);
    assert_eq!(file["file_format_version"], 21);
    let document = the_document_of_the_midi_clip(&file);
    assert_eq!(document["resource"], "tune.mid");
    assert_eq!(document["midi_track"], 2);
    assert_eq!(document["first_source_frame"], 0);
    assert_eq!(document["end_source_frame"], 90);
    assert_eq!(document["speed_factor"], 3);
    assert_eq!(document["gap_before_frames"], 10);
    assert_eq!(document["gain_decibels"], -6.0);
    assert_eq!(document["muted"], true);
    assert_eq!(file["resources"][0]["name"], "tune.mid");
    assert_eq!(file["resources"][0]["kind"], "midi");

    let loaded = read_project_file(&destination).unwrap();
    let there = loaded
        .tracks()
        .iter()
        .flat_map(|track| track.entries())
        .find_map(|entry| entry.clip().as_midi().cloned())
        .expect("the clip is there");
    assert_eq!(there, clip);
    let resource = loaded.resources().get("tune.mid").unwrap();
    assert_eq!(resource.kind(), ResourceKind::Midi);
    assert_eq!(
        resource.midi_file().unwrap().tracks[0].notes.len(),
        1,
        "the file is read as music"
    );
    assert_eq!(saved(&loaded), saved(&project), "nothing is lost and nothing is added");
}

#[test]
fn a_midi_clip_that_has_the_default_volume_and_is_heard_says_neither() {
    let folder = TempDir::new().unwrap();
    let plain = a_midi_clip();
    let project = a_project_with_a_midi_clip(plain, &folder.path().join("tune.mid"));

    let file = as_json(&saved(&project));

    let document = the_document_of_the_midi_clip(&file);
    assert!(
        document.get("gain_decibels").is_none() && document.get("muted").is_none(),
        "{document}"
    );
    assert!(document.get("effects").is_none() && document.get("drawer").is_none() && document.get("badges").is_none());
}

#[test]
fn a_midi_file_that_is_included_in_the_project_file_is_in_the_archive_and_read_back_from_it() {
    let folder = TempDir::new().unwrap();
    let mut project = a_project_with_a_midi_clip(a_midi_clip(), &folder.path().join("tune.mid"));
    let entry = project_resource_entry_name(crc32fast::hash(&a_midi_file()), "tune.mid");
    assert!(project.keep_a_resource_at(
        "tune.mid",
        SourceMediaLink {
            absolute_path: PathBuf::from("/where/it/was/tune.mid"),
            relative_path: None,
            included_as: Some(entry.clone()),
        }
    ));
    let destination = folder.path().join("project.ffrs");

    let written =
        write_project_file_including(&project, &destination, &IncludedFiles::new(), HowToWrite::Anew).unwrap();

    let state = written.archive.expect("it is an archive");
    assert!(state.entries.contains_key(&entry), "{:?}", state.entries.keys());
    fs::remove_file(folder.path().join("tune.mid")).unwrap();
    let loaded = read_project_file(&destination).unwrap();
    let resource = loaded.resources().get("tune.mid").unwrap();
    assert!(resource.is_included());
    assert_eq!(
        resource.bytes().map(|bytes| bytes.to_vec()),
        Some(a_midi_file().to_vec())
    );
    assert_eq!(resource.midi_file().unwrap().tracks.len(), 1, "it is music still");
}

#[test]
fn a_project_whose_midi_file_is_gone_is_opened_with_the_clip_and_the_file_missing_and_saves_again_as_it_was() {
    let folder = TempDir::new().unwrap();
    let path = folder.path().join("tune.mid");
    let project = a_project_with_a_midi_clip(a_midi_clip(), &path);
    let destination = folder.path().join("project.json");
    write_project_file(&project, &destination).unwrap();
    fs::remove_file(&path).unwrap();

    let loaded = read_project_file(&destination).expect("a project whose music is gone is still opened");

    assert!(loaded.resources().get("tune.mid").unwrap().is_missing());
    assert_eq!(loaded.resources_in_use_that_are_missing(), vec!["tune.mid".to_owned()]);
    assert_eq!(
        loaded.resources().get("tune.mid").unwrap().midi_file().unwrap_err(),
        "the file is missing"
    );
    let found = loaded
        .tracks()
        .iter()
        .flat_map(|track| track.entries())
        .find_map(|entry| entry.clip().as_midi().cloned());
    assert_eq!(found, Some(a_midi_clip()));
    let again = folder.path().join("again.json");
    write_project_file(&loaded, &again).unwrap();
    assert_eq!(json_of(&again)["resources"][0]["name"], "tune.mid");
    assert_eq!(
        the_document_of_the_midi_clip(&json_of(&again)),
        the_document_of_the_midi_clip(&json_of(&destination))
    );
}

#[test]
fn a_midi_file_that_no_clip_plays_is_not_saved_and_one_that_a_clip_plays_is() {
    let folder = TempDir::new().unwrap();
    let mut project = a_project_with_a_midi_clip(a_midi_clip(), &folder.path().join("tune.mid"));
    project
        .add_resource(
            "spare.mid",
            ResourceKind::Midi,
            Arc::from(&b"something else"[..]),
            SourceMediaLink::from_absolute_path(folder.path().join("spare.mid")),
        )
        .unwrap();
    let destination = folder.path().join("project.json");

    write_project_file(&project, &destination).unwrap();

    let names: Vec<String> = json_of(&destination)["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["tune.mid"]);
}

#[test]
fn the_effects_of_a_midi_clip_are_saved_with_it_and_a_synthesizer_is_still_one_where_its_plugin_is_gone() {
    let _alone = the_installed_plugins_to_myself();
    let folder = TempDir::new().unwrap();
    let mut synthesizer = an_installed_sound_plugin("clap:com.example.synth", "A Maker", &[("gain", 1.0, None)]);
    synthesizer.descriptor.makes_sound_from_notes = true;
    synthesizer.audio_inputs = 0;
    synthesizer.audio_outputs = 2;
    set_installed_plugins(vec![synthesizer]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let mut project = a_project_with_a_midi_clip(a_midi_clip(), &folder.path().join("tune.mid"));
    let clip = project
        .tracks()
        .iter()
        .flat_map(|track| track.entries())
        .find(|entry| entry.clip().as_midi().is_some())
        .unwrap()
        .id();
    let instance = put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(clip),
        kind,
        &[PropertyValue::Number(1.5)],
    );
    let text = saved(&project);

    let file = as_json(&text);
    assert_eq!(
        the_document_of_the_midi_clip(&file)["effects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let snapshot = file["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|snapshot| snapshot["key"] == "clap:com.example.synth")
        .unwrap();
    assert_eq!(snapshot["makes_sound_from_notes"], true);
    set_installed_plugins(Vec::new());
    let loaded = project_from_json_text(&text).unwrap();
    let there = loaded
        .tracks()
        .iter()
        .flat_map(|track| track.entries())
        .find(|entry| entry.clip().as_midi().is_some())
        .unwrap();
    let effect = there.effects().instance(instance).unwrap();
    assert!(!effect.kind().is_installed());
    assert!(effect.kind().makes_sound_from_notes());
    assert_eq!(effect.values(), [PropertyValue::Number(1.5)]);
    // (The file of the music was not read, since the project was read from text and not from where it is: the tracks and the snapshots are what they were.)
    let again = as_json(&saved(&loaded));
    assert_eq!(again["tracks"], file["tracks"]);
    assert_eq!(again["plugins"], file["plugins"]);
}

#[test]
fn a_file_that_puts_a_picture_effect_on_a_midi_clip_is_refused_with_the_reason() {
    let folder = TempDir::new().unwrap();
    // (The effects of a video that has a hue shift, and the snapshots that go with them, are put on the MIDI clip of another file.)
    let mut with_a_hue_shift = edited_project(link_to_missing_video());
    let video = AnimationTarget::Clip(with_a_hue_shift.tracks()[0].entries()[0].id());
    put_a_hue_shift_on(&mut with_a_hue_shift, video, 30.0);
    let donor = as_json(&saved(&with_a_hue_shift));
    let effects = donor["tracks"][0]["clips"][0]["effects"].clone();
    let mut file = as_json(&saved(&a_project_with_a_midi_clip(
        a_midi_clip(),
        &folder.path().join("tune.mid"),
    )));
    file["plugins"] = donor["plugins"].clone();
    for track in file["tracks"].as_array_mut().unwrap() {
        for clip in track["clips"].as_array_mut().unwrap() {
            if clip["kind"] == "midi" {
                clip["effects"] = effects.clone();
            }
        }
    }

    let reason = reason_for_invalid_project(&file.to_string());

    assert!(reason.contains("hue_shift") || reason.contains("effect"), "{reason}");
}

#[test]
fn a_midi_clip_of_no_frames_or_with_a_field_that_is_not_known_is_refused() {
    let folder = TempDir::new().unwrap();
    let project = a_project_with_a_midi_clip(a_midi_clip(), &folder.path().join("tune.mid"));
    let change = |change: &dyn Fn(&mut Value)| {
        let mut file = as_json(&saved(&project));
        for track in file["tracks"].as_array_mut().unwrap() {
            for clip in track["clips"].as_array_mut().unwrap() {
                if clip["kind"] == "midi" {
                    change(clip);
                }
            }
        }
        file.to_string()
    };

    let empty = change(&|clip| clip["end_source_frame"] = json!(0));
    let odd = change(&|clip| clip["volume"] = json!(11));
    let no_track_number = change(&|clip| clip["midi_track"] = json!(70_000));

    assert!(reason_for_invalid_project(&empty).contains("at least one frame"));
    assert!(project_from_json_text(&odd).is_err(), "a field that no version has");
    assert!(
        project_from_json_text(&no_track_number).is_err(),
        "a track number that is not a number of 16 bits"
    );
}

#[test]
fn a_file_of_version_20_which_has_no_midi_clip_is_read_as_before() {
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    let twenty = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(20));

    let loaded = project_from_json_text(&twenty).unwrap();

    assert_eq!(saved(&loaded), text);
}
