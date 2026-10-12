//! Tests of how keyframes are saved in a project file and read back.

use super::tests::{
    CURRENT_VERSION_LINE, assert_same_content, project_with_everything, reason_for_unreadable_project, saved,
    valid_text_with,
};
use super::*;
use crate::project::{AnimationTarget, TextColour};
use crate::test_support::make_a_keyframe;

/// The project that has everything, with keyframes on a video, a sound, a text object and two tracks.
fn project_with_keyframes() -> Project {
    let mut project = project_with_everything();
    // The video clips are on the track at the back (position 2), the sound on the middle one, the text in front. The frames
    // given here are those of the timeline (the sound starts at frame 450 and the text at frame 120), and the frames in the
    // file are counted from there.
    let video = project.tracks()[2].entries()[0].id();
    let sound = project.tracks()[1].entries()[0].id();
    let text = project.tracks()[0].entries()[0].id();
    let music_track = project.tracks()[1].id();
    let clip = AnimationTarget::Clip;

    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::Gain,
        0,
        PropertyValue::Number(0.0),
    );
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::Gain,
        30,
        PropertyValue::Number(-6.0),
    );
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::Position,
        10,
        PropertyValue::Pair(0.25, 0.5),
    );
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::Position,
        90,
        PropertyValue::Pair(0.75, 0.5),
    );
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::Size,
        0,
        PropertyValue::Pair(1.0, 1.0),
    );
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::Rotation,
        20,
        PropertyValue::Number(-45.5),
    );

    make_a_keyframe(
        &mut project,
        clip(sound),
        PropertyId::Muted,
        450,
        PropertyValue::Flag(false),
    );
    make_a_keyframe(
        &mut project,
        clip(sound),
        PropertyId::Muted,
        510,
        PropertyValue::Flag(true),
    );

    make_a_keyframe(
        &mut project,
        clip(text),
        PropertyId::Colour,
        120,
        PropertyValue::Colour(TextColour::new(255, 0, 0, 255)),
    );
    make_a_keyframe(
        &mut project,
        clip(text),
        PropertyId::Colour,
        170,
        PropertyValue::Colour(TextColour::new(0, 0, 255, 128)),
    );
    make_a_keyframe(
        &mut project,
        clip(text),
        PropertyId::Text,
        120,
        PropertyValue::Words("One \"two\"\nthree".to_owned()),
    );
    make_a_keyframe(
        &mut project,
        clip(text),
        PropertyId::FontFamily,
        125,
        PropertyValue::Words("Arial".to_owned()),
    );
    make_a_keyframe(
        &mut project,
        clip(text),
        PropertyId::FontSize,
        125,
        PropertyValue::Number(0.1),
    );

    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(music_track),
        PropertyId::Gain,
        0,
        PropertyValue::Number(2.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(music_track),
        PropertyId::Gain,
        500,
        PropertyValue::Number(-12.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(music_track),
        PropertyId::Muted,
        600,
        PropertyValue::Flag(true),
    );
    project
}

#[test]
fn keyframes_of_every_kind_of_clip_and_of_a_track_survive_a_trip_through_the_text() {
    let project = project_with_keyframes();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    // (The ids of the tracks and clips are made up afresh when a project is loaded, so it is the saved texts that are the same.)
    assert_same_content(&loaded, &project);
    let video = loaded.tracks()[2].entries()[0].id();
    assert!(loaded.is_animated(AnimationTarget::Clip(video), PropertyId::Rotation));
    assert!(loaded.is_animated(AnimationTarget::Track(loaded.tracks()[1].id()), PropertyId::Muted));
}

/// What keeps the test above honest: the project it saves and loads has keyframes for every property of the table, so that a
/// property that is added to the table is not left out of the trip through the file because nobody thought of it.
#[test]
fn the_project_that_makes_the_trip_through_the_file_has_keyframes_for_every_property_there_is() {
    let project = project_with_keyframes();
    let targets: Vec<AnimationTarget> = project
        .tracks()
        .iter()
        .flat_map(|track| {
            std::iter::once(AnimationTarget::Track(track.id()))
                .chain(track.entries().iter().map(|entry| AnimationTarget::Clip(entry.id())))
        })
        .collect();

    for property in PropertyId::ALL {
        assert!(
            targets.iter().any(|&target| project.is_animated(target, property)),
            "nothing in `project_with_keyframes` has keyframes for {property:?}, so the file is not tried with them"
        );
    }
}

#[test]
fn a_project_that_has_no_keyframes_has_nothing_about_them_in_its_file() {
    let text = saved(&project_with_everything());
    assert!(!text.contains("animation"), "{text}");
}

#[test]
fn keyframes_are_written_by_the_name_of_the_property_with_a_frame_and_a_value_each_in_the_order_of_the_frames() {
    let text = saved(&project_with_keyframes());
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();

    let video = &file["tracks"][2]["clips"][0]["animation"];
    assert_eq!(
        video["gain"],
        serde_json::json!([{"frame": 0, "value": 0.0}, {"frame": 30, "value": -6.0}])
    );
    assert_eq!(
        video["position"],
        serde_json::json!([{"frame": 10, "value": [0.25, 0.5]}, {"frame": 90, "value": [0.75, 0.5]}])
    );
    assert_eq!(video["size"], serde_json::json!([{"frame": 0, "value": [1.0, 1.0]}]));
    assert_eq!(video["rotation"], serde_json::json!([{"frame": 20, "value": -45.5}]));
    assert_eq!(
        file["tracks"][1]["clips"][0]["animation"]["muted"],
        serde_json::json!([{"frame": 0, "value": false}, {"frame": 60, "value": true}])
    );
    let text_clip = &file["tracks"][0]["clips"][0]["animation"];
    assert_eq!(
        text_clip["colour"],
        serde_json::json!([{"frame": 0, "value": "#FF0000FF"}, {"frame": 50, "value": "#0000FF80"}])
    );
    assert_eq!(
        text_clip["text"],
        serde_json::json!([{"frame": 0, "value": "One \"two\"\nthree"}])
    );
    assert_eq!(text_clip["font"], serde_json::json!([{"frame": 5, "value": "Arial"}]));
    assert_eq!(
        file["tracks"][1]["animation"]["gain"],
        serde_json::json!([{"frame": 0, "value": 2.0}, {"frame": 500, "value": -12.0}])
    );
    assert_eq!(
        file["tracks"][1]["animation"]["muted"],
        serde_json::json!([{"frame": 600, "value": true}])
    );
    assert!(
        file["tracks"][2].get("animation").is_none(),
        "a track that has no keyframes has none in its file"
    );
}

#[test]
fn numbers_of_keyframes_are_written_as_short_as_the_other_numbers_of_the_file_are() {
    let text = saved(&project_with_keyframes());
    // (A font size of 0.1 of the picture's height, which has a fraction that does not fit a binary number.)
    assert!(text.contains("\"value\": 0.1\n"), "{text}");
}

#[test]
fn a_file_of_the_version_before_keyframes_loads_as_a_project_with_nothing_animated() {
    let text = valid_text_with(CURRENT_VERSION_LINE, "\"file_format_version\": 5");
    let project = project_from_json_text(&text).unwrap();
    assert!(project.tracks()[0].animation().is_empty());
    assert!(
        project.tracks()[0]
            .entries()
            .iter()
            .all(|entry| entry.animation().is_empty())
    );
}

// --- what is wrong with a file ---

/// The file of [`project_with_keyframes`] after `change` has been made to the JSON of it. (The video that has keyframes for
/// how loud it is and where it is is the first clip of the track number 3, the sound the first clip of the track number 2, the
/// text object the first clip of the track number 1, and the track number 2 has keyframes of its own.)
fn saved_with(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut file: serde_json::Value = serde_json::from_str(&saved(&project_with_keyframes())).unwrap();
    change(&mut file);
    serde_json::to_string_pretty(&file).unwrap()
}

fn reason_for_invalid_project(text: &str) -> String {
    match project_from_json_text(text) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("expected InvalidProject, got {other:?}"),
    }
}

/// Renames the key `from` of the object `object` to `to`.
fn rename(object: &mut serde_json::Value, from: &str, to: &str) {
    let map = object.as_object_mut().expect("an object");
    let value = map.remove(from).expect("it has the key");
    map.insert(to.to_owned(), value);
}

#[test]
fn keyframes_of_something_that_cannot_be_animated_are_refused_by_name() {
    let text = saved_with(|file| rename(&mut file["tracks"][2]["clips"][0]["animation"], "gain", "loudness"));
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"loudness\""), "{reason}");
    assert!(reason.contains("not something that can be animated"), "{reason}");
    assert!(
        reason.contains("clip number 1 of track number 3"),
        "the clip is named: {reason}"
    );
}

#[test]
fn keyframes_of_something_the_clip_or_the_track_does_not_have_are_refused() {
    // A video has no words.
    let text = saved_with(|file| rename(&mut file["tracks"][2]["clips"][0]["animation"], "rotation", "text"));
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"text\" cannot be animated for a video"), "{reason}");

    // A sound has no place on the picture.
    let text = saved_with(|file| rename(&mut file["tracks"][1]["clips"][0]["animation"], "muted", "position"));
    let reason = reason_for_invalid_project(&text);
    assert!(
        reason.contains("\"position\" cannot be animated for a sound"),
        "{reason}"
    );

    // A track has no place on the picture either.
    let text = saved_with(|file| rename(&mut file["tracks"][1]["animation"], "gain", "position"));
    let reason = reason_for_invalid_project(&text);
    assert!(
        reason.contains("\"position\" cannot be animated for a track"),
        "{reason}"
    );
    assert!(reason.contains("track number 2"), "{reason}");
}

#[test]
fn a_value_that_is_not_the_kind_the_property_has_is_refused_and_says_what_was_wanted() {
    let text =
        saved_with(|file| file["tracks"][2]["clips"][0]["animation"]["gain"][1]["value"] = serde_json::json!("quiet"));
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("the keyframe of \"gain\" at frame 30"), "{reason}");
    assert!(reason.contains("words, where a number is wanted"), "{reason}");

    let text =
        saved_with(|file| file["tracks"][1]["clips"][0]["animation"]["muted"][0]["value"] = serde_json::json!(1.0));
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("a number, where true or false is wanted"), "{reason}");

    let text =
        saved_with(|file| file["tracks"][0]["clips"][0]["animation"]["colour"][1]["value"] = serde_json::json!("blue"));
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("#RRGGBBAA"), "{reason}");

    let text =
        saved_with(|file| file["tracks"][2]["clips"][0]["animation"]["position"][0]["value"] = serde_json::json!(0.5));
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("a number, where two numbers"), "{reason}");
}

#[test]
fn a_pair_of_numbers_that_has_three_is_not_a_value_at_all() {
    let text = saved_with(|file| {
        file["tracks"][2]["clips"][0]["animation"]["position"][0]["value"] = serde_json::json!([0.25, 0.5, 1.0]);
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("did not match any variant"), "{reason}");
}

#[test]
fn a_misspelt_field_of_a_keyframe_is_reported() {
    let text = saved_with(|file| {
        rename(
            &mut file["tracks"][2]["clips"][0]["animation"]["gain"][1],
            "frame",
            "frames",
        )
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `frames`"), "{reason}");
}

#[test]
fn keyframes_after_the_end_of_a_clip_are_cut_off_when_a_file_is_read_and_the_curve_is_kept_up_to_the_end() {
    // The first video clip lasts 300 frames: the second keyframe of its gain, at frame 1000, is where it is not.
    let text =
        saved_with(|file| file["tracks"][2]["clips"][0]["animation"]["gain"][1]["frame"] = serde_json::json!(1000));
    let project = project_from_json_text(&text).unwrap();
    let video = project.tracks()[2].entries()[0].id();
    let keyframes = project
        .placement_of(video)
        .unwrap()
        .animation
        .keyframes_of(PropertyId::Gain)
        .unwrap()
        .iter()
        .map(|keyframe| (keyframe.frame, keyframe.value.clone()))
        .collect::<Vec<_>>();
    // The one in the clip is kept, and the clip ends where the gain is on the way from 0 to -6 decibels that is 299/1000 of the
    // way there.
    assert_eq!(keyframes.len(), 2, "{keyframes:?}");
    assert_eq!(keyframes[0], (0, PropertyValue::Number(0.0)));
    assert_eq!(keyframes[1].0, 299, "the last frame of the clip");
    let PropertyValue::Number(decibels) = keyframes[1].1 else {
        panic!("a number");
    };
    assert!((decibels - (-6.0 * 299.0 / 1000.0)).abs() < 1e-4, "{decibels}");
}
