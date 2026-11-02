// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 23 of the project file added: `hidden` in the document of a track, for a track whose eye is closed (its pictures are left out of the picture of the project), left out when the
//! track is seen; and that a file of version 22, which has none, is read as a project whose tracks are all seen.

use serde_json::Value;

use super::tests::{edited_project, link_to_missing_video, saved, version_line};
use super::*;

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn a_track_that_is_seen_does_not_say_so_in_the_file_and_one_that_is_hidden_does() {
    let mut project = edited_project(link_to_missing_video());
    let track = project.tracks()[0].id();

    let seen = as_json(&saved(&project));
    project.set_track_hidden(track, true).unwrap();
    let hidden = as_json(&saved(&project));

    assert!(seen["tracks"][0].get("hidden").is_none(), "{}", seen["tracks"][0]);
    assert_eq!(hidden["tracks"][0]["hidden"], true);
}

#[test]
fn the_eye_of_each_track_is_read_back_as_it_was_and_is_saved_again_as_it_was() {
    let mut project = edited_project(link_to_missing_video());
    project.add_track_in_front();
    project.add_track_in_front();
    let tracks: Vec<_> = project.tracks().iter().map(|track| track.id()).collect();
    assert_eq!(tracks.len(), 3);
    // (The first and the third are hidden and the second is not: the right ones come back, in the right places.)
    for (number, track) in tracks.iter().enumerate() {
        project.set_track_hidden(*track, number % 2 == 0).unwrap();
    }
    let text = saved(&project);

    let loaded = project_from_json_text(&text).unwrap();

    let eyes: Vec<bool> = loaded.tracks().iter().map(|track| track.is_hidden()).collect();
    let expected: Vec<bool> = (0..tracks.len()).map(|number| number % 2 == 0).collect();
    assert_eq!(eyes, expected);
    assert_eq!(saved(&loaded), text, "nothing is lost");
}

#[test]
fn a_file_of_version_22_has_no_eye_and_all_its_tracks_are_seen() {
    let project = edited_project(link_to_missing_video());
    let text = saved(&project);
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const { assert!(FILE_FORMAT_VERSION >= 23, "the eye of a track came with format 23") };
    let twenty_two = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(22));

    let loaded = project_from_json_text(&twenty_two).unwrap();

    assert!(loaded.tracks().iter().all(|track| !track.is_hidden()));
    assert_eq!(
        saved(&loaded),
        text,
        "it is saved again as the newest version, with nothing different"
    );
}

#[test]
fn a_file_that_says_in_so_many_words_that_a_track_is_seen_is_read_as_seen() {
    let project = edited_project(link_to_missing_video());
    let mut document = as_json(&saved(&project));
    document["tracks"][0]["hidden"] = Value::Bool(false);

    let loaded = project_from_json_text(&document.to_string()).unwrap();

    assert!(!loaded.tracks()[0].is_hidden());
}
