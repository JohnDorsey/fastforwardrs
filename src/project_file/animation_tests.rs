//! Tests of how keyframes are saved in a project file and read back.

use super::tests::{
    assert_same_content, current_version_line, project_with_everything, reason_for_unreadable_project, saved,
    valid_text_with, version_line,
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
    let text_track = project.tracks()[0].id();
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

    // (The hue shift of the video is turned a turn and a quarter between its keyframes, which is more than the colours go round.)
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::HueShift,
        15,
        PropertyValue::Number(-90.0),
    );
    make_a_keyframe(
        &mut project,
        clip(video),
        PropertyId::HueShift,
        75,
        PropertyValue::Number(450.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Track(text_track),
        PropertyId::HueShift,
        130,
        PropertyValue::Number(10.0),
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
fn keyframes_for_a_turn_that_are_whole_turns_apart_come_back_as_they_were_saved() {
    let mut project = project_with_everything();
    let video = project.tracks()[2].entries()[0].id();
    for (frame, degrees) in [(0, 0.0), (60, 1080.0)] {
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(video),
            PropertyId::Rotation,
            frame,
            PropertyValue::Number(degrees),
        );
    }

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    let video = loaded.tracks()[2].entries()[0].id();
    assert_eq!(
        loaded.property_value_at(
            AnimationTarget::Clip(video),
            PropertyId::Rotation,
            crate::project::TimelineFrameIndex::new(30)
        ),
        Some(PropertyValue::Number(540.0)),
        "half way is a turn and a half"
    );
}

#[test]
fn a_project_that_has_no_keyframes_has_nothing_about_them_in_its_file() {
    let text = saved(&project_with_everything());
    assert!(!text.contains("animation"), "{text}");
    assert!(!text.contains("drawer"), "{text}");
    assert!(!text.contains("keyframes"), "{text}");
}

#[test]
fn keyframes_are_written_as_racks_in_the_drawer_of_what_has_them_by_the_name_of_the_property_with_a_frame_and_a_value_each()
 {
    let text = saved(&project_with_keyframes());
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();

    let video = &file["tracks"][2]["clips"][0]["drawer"];
    assert_eq!(
        rack_of(video, "gain")["keyframes"],
        serde_json::json!([{"frame": 0, "value": 0.0}, {"frame": 30, "value": -6.0}])
    );
    assert_eq!(
        rack_of(video, "position")["keyframes"],
        serde_json::json!([{"frame": 10, "value": [0.25, 0.5]}, {"frame": 90, "value": [0.75, 0.5]}])
    );
    assert_eq!(
        rack_of(video, "size")["keyframes"],
        serde_json::json!([{"frame": 0, "value": [1.0, 1.0]}])
    );
    assert_eq!(
        rack_of(video, "rotation")["keyframes"],
        serde_json::json!([{"frame": 20, "value": -45.5}])
    );
    assert_eq!(
        rack_of(&file["tracks"][1]["clips"][0]["drawer"], "muted")["keyframes"],
        serde_json::json!([{"frame": 0, "value": false}, {"frame": 60, "value": true}])
    );
    let text_clip = &file["tracks"][0]["clips"][0]["drawer"];
    assert_eq!(
        rack_of(text_clip, "colour")["keyframes"],
        serde_json::json!([{"frame": 0, "value": "#FF0000FF"}, {"frame": 50, "value": "#0000FF80"}])
    );
    assert_eq!(
        rack_of(text_clip, "text")["keyframes"],
        serde_json::json!([{"frame": 0, "value": "One \"two\"\nthree"}])
    );
    assert_eq!(
        rack_of(text_clip, "font")["keyframes"],
        serde_json::json!([{"frame": 5, "value": "Arial"}])
    );
    assert_eq!(
        rack_of(&file["tracks"][1]["drawer"], "gain")["keyframes"],
        serde_json::json!([{"frame": 0, "value": 2.0}, {"frame": 500, "value": -12.0}])
    );
    assert_eq!(
        rack_of(&file["tracks"][1]["drawer"], "muted")["keyframes"],
        serde_json::json!([{"frame": 600, "value": true}])
    );
    assert!(
        file["tracks"][2].get("drawer").is_none(),
        "a track that has no keyframes has no drawer in its file"
    );
    assert!(
        !text.contains("\"animation\""),
        "the animation objects of the files before this are not written"
    );
}

/// The rack of the keyframes of `property` in `drawer` (a list of racks, as the file has it).
fn rack_of<'a>(drawer: &'a serde_json::Value, property: &str) -> &'a serde_json::Value {
    drawer
        .as_array()
        .expect("a drawer is a list of racks")
        .iter()
        .find(|rack| rack["property"] == property)
        .unwrap_or_else(|| panic!("no rack of {property} in {drawer}"))
}

fn rack_of_mut<'a>(drawer: &'a mut serde_json::Value, property: &str) -> &'a mut serde_json::Value {
    drawer
        .as_array_mut()
        .expect("a drawer is a list of racks")
        .iter_mut()
        .find(|rack| rack["property"] == property)
        .unwrap_or_else(|| panic!("no rack of {property}"))
}

#[test]
fn a_rack_does_not_say_what_it_is_of_when_it_is_of_the_clip_or_the_track_that_has_the_drawer_it_is_in() {
    let file: serde_json::Value = serde_json::from_str(&saved(&project_with_keyframes())).unwrap();
    for drawer in [
        &file["tracks"][2]["clips"][0]["drawer"],
        &file["tracks"][0]["clips"][0]["drawer"],
        &file["tracks"][1]["drawer"],
    ] {
        for rack in drawer.as_array().unwrap() {
            assert!(rack.get("target").is_none(), "{rack}");
        }
    }
}

#[test]
fn racks_kept_in_the_drawer_of_another_clip_or_of_a_track_say_what_they_are_of_by_where_it_is_in_the_file_and_come_back()
 {
    // The caption (the first clip of the first track) holds a rack of how loud the video is (the first clip of the third
    // track), and the drawer of the first track holds a rack of whether the music track (the second) is silent.
    let mut file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
        {"property": "gain", "target": {"track": 2, "clip": 0}, "keyframes": [{"frame": 3, "value": -4.0}, {"frame": 9, "value": 0.0}]}
    ]);
    file["tracks"][0]["drawer"] = serde_json::json!([
        {"property": "muted", "target": {"track": 1}, "keyframes": [{"frame": 100, "value": true}]}
    ]);
    let text = serde_json::to_string_pretty(&file).unwrap();

    let loaded = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let music = AnimationTarget::Track(loaded.tracks()[1].id());
    let frame = crate::project::TimelineFrameIndex::new;
    assert_eq!(
        loaded.property_value_at(video, PropertyId::Gain, frame(120 + 3)),
        Some(PropertyValue::Number(-4.0)),
        "the caption starts at frame 120"
    );
    assert!(loaded.is_animated(music, PropertyId::Muted));
    assert!(
        loaded.tracks()[2].entries()[0].drawer().is_empty(),
        "the video's own drawer has nothing in it"
    );
    let written: serde_json::Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(
        written["tracks"][0]["clips"][0]["drawer"],
        file["tracks"][0]["clips"][0]["drawer"]
    );
    assert_eq!(written["tracks"][0]["drawer"], file["tracks"][0]["drawer"]);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

/// The file of the project that has everything, with one more thing at the end of the clips of the track in front: `crate_clip`.
fn file_with_this_crate_after_the_text_object(crate_clip: serde_json::Value) -> serde_json::Value {
    let mut file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    file["tracks"][0]["clips"].as_array_mut().unwrap().push(crate_clip);
    file
}

/// How long the keyframe crate that is the last clip of the track in front lasts, and where.
fn the_crate_at_the_end_of_the_front_track(project: &Project) -> (u64, u64, usize) {
    let entry = project.tracks()[0].entries().last().unwrap();
    let crate::project::Clip::Crate(crate_clip) = entry.clip() else {
        panic!("the last clip of the track in front is not a crate: {:?}", entry.clip());
    };
    (
        entry.gap_before_frames(),
        crate_clip.duration_in_frames(),
        entry.drawer().len(),
    )
}

#[test]
fn a_keyframe_crate_is_written_with_how_long_it_lasts_and_its_drawer_and_comes_back() {
    // (A crate lasts from its first keyframe to its last, so this one, with keyframes at its first frame and at its 51st, lasts 51.)
    let file = file_with_this_crate_after_the_text_object(serde_json::json!({
        "kind": "crate",
        "gap_before_frames": 40,
        "duration_in_frames": 51,
        "drawer": [{"property": "rotation", "target": {"track": 2, "clip": 0}, "keyframes": [
            {"frame": 0, "value": 90.0},
            {"frame": 50, "value": 180.0}
        ]}]
    }));
    let text = serde_json::to_string_pretty(&file).unwrap();

    let loaded = project_from_json_text(&text).unwrap();

    assert_eq!(the_crate_at_the_end_of_the_front_track(&loaded), (40, 51, 1));
    let written: serde_json::Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(
        written["tracks"][0]["clips"].as_array().unwrap().last(),
        file["tracks"][0]["clips"].as_array().unwrap().last()
    );
}

#[test]
fn a_keyframe_crate_saved_by_a_version_that_let_it_last_longer_than_its_keyframes_is_loaded_as_long_as_they_need() {
    let file = file_with_this_crate_after_the_text_object(serde_json::json!({
        "kind": "crate",
        "gap_before_frames": 40,
        "duration_in_frames": 200,
        "drawer": [{"property": "rotation", "target": {"track": 2, "clip": 0}, "keyframes": [
            {"frame": 10, "value": 90.0},
            {"frame": 30, "value": 180.0}
        ]}]
    }));
    let text = serde_json::to_string_pretty(&file).unwrap();

    let loaded = project_from_json_text(&text).unwrap();

    // It begins where its first keyframe was (10 frames after where it began) and ends after its last (21 frames later): it is what the
    // file of a crate that is made so says.
    assert_eq!(the_crate_at_the_end_of_the_front_track(&loaded), (50, 21, 1));
    let the_same_made_so = file_with_this_crate_after_the_text_object(serde_json::json!({
        "kind": "crate",
        "gap_before_frames": 50,
        "duration_in_frames": 21,
        "drawer": [{"property": "rotation", "target": {"track": 2, "clip": 0}, "keyframes": [
            {"frame": 0, "value": 90.0},
            {"frame": 20, "value": 180.0}
        ]}]
    }));
    let the_same_loaded = project_from_json_text(&serde_json::to_string_pretty(&the_same_made_so).unwrap()).unwrap();
    assert_same_content(&loaded, &the_same_loaded);
    // The keyframes did not move: they are at the frames of the timeline that the file said.
    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let where_the_crate_began = project_with_everything().tracks()[0].total_frame_count() + 40;
    let frame = crate::project::TimelineFrameIndex::new;
    assert_eq!(
        loaded.property_value_at(video, PropertyId::Rotation, frame(where_the_crate_began + 10)),
        Some(PropertyValue::Number(90.0))
    );
    assert_eq!(
        loaded.property_value_at(video, PropertyId::Rotation, frame(where_the_crate_began + 30)),
        Some(PropertyValue::Number(180.0))
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn a_keyframe_crate_saved_with_nothing_in_it_is_not_in_the_project_that_is_loaded() {
    let original = project_with_everything();
    let file = file_with_this_crate_after_the_text_object(serde_json::json!({
        "kind": "crate",
        "gap_before_frames": 40,
        "duration_in_frames": 200,
        "drawer": []
    }));
    let text = serde_json::to_string_pretty(&file).unwrap();

    let loaded = project_from_json_text(&text).unwrap();

    assert_same_content(&loaded, &original);
}

#[test]
fn numbers_of_keyframes_are_written_as_short_as_the_other_numbers_of_the_file_are() {
    let text = saved(&project_with_keyframes());
    // (A font size of 0.1 of the picture's height, which has a fraction that does not fit a binary number.)
    assert!(text.contains("\"value\": 0.1\n"), "{text}");
}

#[test]
fn a_file_of_the_version_before_keyframes_loads_as_a_project_with_nothing_animated() {
    let text = valid_text_with(&current_version_line(), &version_line(5));
    let project = project_from_json_text(&text).unwrap();
    assert!(project.tracks()[0].own_animation().is_empty());
    assert!(
        project.tracks()[0]
            .entries()
            .iter()
            .all(|entry| entry.own_animation().is_empty())
    );
}

/// The file of a version before 7, which has the keyframes of what a clip or a track is in an `animation` object in it, made from
/// the file of `project` (which has racks of the keyframes of what has the drawer they are in, and no others).
fn written_the_way_version_6_did(project: &Project) -> String {
    let mut file: serde_json::Value = serde_json::from_str(&saved(project)).unwrap();
    let to_animation = |holder: &mut serde_json::Value| {
        let Some(drawer) = holder.as_object_mut().and_then(|holder| holder.remove("drawer")) else {
            return;
        };
        let mut animation = serde_json::Map::new();
        for rack in drawer.as_array().unwrap() {
            assert!(
                rack.get("target").is_none(),
                "only racks of what has the drawer: {rack}"
            );
            animation.insert(rack["property"].as_str().unwrap().to_owned(), rack["keyframes"].clone());
        }
        holder["animation"] = serde_json::Value::Object(animation);
    };
    for track in file["tracks"].as_array_mut().unwrap() {
        to_animation(track);
        for clip in track["clips"].as_array_mut().unwrap() {
            to_animation(clip);
        }
    }
    file["file_format_version"] = serde_json::json!(6);
    serde_json::to_string_pretty(&file).unwrap()
}

#[test]
fn a_file_of_version_6_has_the_keyframes_in_an_animation_object_which_is_read_as_a_rack_in_the_drawer_of_what_has_them()
{
    let project = project_with_keyframes();
    let old_file = written_the_way_version_6_did(&project);
    assert!(old_file.contains("\"animation\""), "{old_file}");
    assert!(!old_file.contains("\"drawer\""), "{old_file}");

    let loaded = project_from_json_text(&old_file).unwrap();

    // (The racks of a clip are in the order of the table of properties when they come from an animation object, which is the
    // order they were made in or not: what is the same is what each drawer has.)
    assert_eq!(
        with_the_racks_in_order(&saved(&loaded)),
        with_the_racks_in_order(&saved(&project))
    );
    assert!(
        saved(&loaded).contains(&current_version_line()),
        "it is written the way the version of today writes it"
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

/// The JSON of a file with the racks of every drawer sorted by the name of their property (and what they are of).
fn with_the_racks_in_order(text: &str) -> serde_json::Value {
    fn sort_the_drawers(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::Array(racks)) = map.get_mut("drawer") {
                    racks.sort_by_key(|rack| (rack["property"].to_string(), rack["target"].to_string()));
                }
                map.values_mut().for_each(sort_the_drawers);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(sort_the_drawers),
            _ => {}
        }
    }
    let mut file: serde_json::Value = serde_json::from_str(text).unwrap();
    sort_the_drawers(&mut file);
    file
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

#[test]
fn keyframes_of_something_that_cannot_be_animated_are_refused_by_name() {
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "gain")["property"] = serde_json::json!("loudness");
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"loudness\""), "{reason}");
    assert!(reason.contains("not something that can be animated"), "{reason}");
    assert!(
        reason.contains("clip number 1 of track number 3"),
        "the clip is named: {reason}"
    );
    assert!(reason.contains("rack number"), "and the rack: {reason}");
}

#[test]
fn keyframes_of_something_the_clip_or_the_track_does_not_have_are_refused() {
    // A video has no words.
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "rotation")["property"] = serde_json::json!("text");
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"text\" cannot be animated"), "{reason}");

    // A sound has no place on the picture.
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][1]["clips"][0]["drawer"], "muted")["property"] = serde_json::json!("position");
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"position\" cannot be animated"), "{reason}");

    // A track has no place on the picture either.
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][1]["drawer"], "gain")["property"] = serde_json::json!("position");
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"position\" cannot be animated"), "{reason}");
    assert!(reason.contains("track number 2"), "{reason}");

    // What a rack is of is what the property is checked against, not what has the drawer: a sound is not on the picture, whichever
    // drawer a rack of its place is in.
    let text = saved_with(|file| {
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "position", "target": {"track": 1, "clip": 0}, "keyframes": [{"frame": 1, "value": [0.5, 0.5]}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"position\" cannot be animated"), "{reason}");
}

#[test]
fn a_rack_of_what_is_not_in_the_file_is_refused_and_says_which() {
    let text = saved_with(|file| {
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "gain", "target": {"track": 9}, "keyframes": [{"frame": 1, "value": 0.0}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("there is no track 9"), "{reason}");
    assert!(reason.contains("clip number 1 of track number 1"), "{reason}");

    let text = saved_with(|file| {
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "gain", "target": {"track": 2, "clip": 7}, "keyframes": [{"frame": 1, "value": 0.0}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("there is no clip 7 on track 2"), "{reason}");
}

#[test]
fn a_drawer_that_has_two_racks_of_one_property_of_one_thing_is_loaded_with_both_and_they_are_saved_as_they_were() {
    let text = saved_with(|file| {
        let rack = rack_of(&file["tracks"][2]["clips"][0]["drawer"], "gain").clone();
        file["tracks"][2]["clips"][0]["drawer"]
            .as_array_mut()
            .unwrap()
            .push(rack);
    });

    let loaded = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    assert_eq!(
        loaded
            .drawer_of(video)
            .unwrap()
            .racks_for(video, PropertyId::Gain)
            .count(),
        2,
        "they are not merged"
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved(&loaded)).unwrap(),
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        "what is read is what is saved"
    );
}

#[test]
fn the_animation_object_of_a_file_of_the_version_before_is_a_rack_of_its_own_beside_a_rack_of_the_same_property_in_the_drawer()
 {
    let text = saved_with(|file| {
        file["tracks"][2]["clips"][0]["animation"] = serde_json::json!({"gain": [{"frame": 1, "value": 0.0}]});
    });

    let loaded = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let racks: Vec<usize> = loaded
        .drawer_of(video)
        .unwrap()
        .racks_for(video, PropertyId::Gain)
        .map(|rack| rack.len())
        .collect();
    assert_eq!(
        racks,
        vec![1, 2],
        "the rack that came from the animation object comes first, and the rack of the drawer with its two keyframes under it"
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn a_rack_with_no_keyframes_is_refused() {
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "gain")["keyframes"] = serde_json::json!([]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("no keyframes of \"gain\""), "{reason}");
}

#[test]
fn a_value_that_is_not_the_kind_the_property_has_is_refused_and_says_what_was_wanted() {
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "gain")["keyframes"][1]["value"] =
            serde_json::json!("quiet");
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("the keyframe of \"gain\" at frame 30"), "{reason}");
    assert!(reason.contains("words, where a number is wanted"), "{reason}");

    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][1]["clips"][0]["drawer"], "muted")["keyframes"][0]["value"] =
            serde_json::json!(1.0);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("a number, where true or false is wanted"), "{reason}");

    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][0]["clips"][0]["drawer"], "colour")["keyframes"][1]["value"] =
            serde_json::json!("blue");
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("#RRGGBBAA"), "{reason}");

    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "position")["keyframes"][0]["value"] =
            serde_json::json!(0.5);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("a number, where two numbers"), "{reason}");
}

#[test]
fn a_pair_of_numbers_that_has_three_is_not_a_value_at_all() {
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "position")["keyframes"][0]["value"] =
            serde_json::json!([0.25, 0.5, 1.0]);
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("did not match any variant"), "{reason}");
}

#[test]
fn a_misspelt_field_of_a_keyframe_a_rack_or_a_target_is_reported() {
    let text = saved_with(|file| {
        let keyframe = &mut rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "gain")["keyframes"][1];
        let value = keyframe.as_object_mut().unwrap().remove("frame").unwrap();
        keyframe["frames"] = value;
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `frames`"), "{reason}");

    let text = saved_with(|file| {
        let rack = rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "gain");
        let keyframes = rack.as_object_mut().unwrap().remove("keyframes").unwrap();
        rack["keyframe"] = keyframes;
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `keyframe`"), "{reason}");

    let text = saved_with(|file| {
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "gain", "target": {"track": 2, "clipp": 0}, "keyframes": [{"frame": 1, "value": 0.0}]}
        ]);
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `clipp`"), "{reason}");
}

#[test]
fn a_crate_that_lasts_no_time_is_refused() {
    let text = saved_with(|file| {
        file["tracks"][0]["clips"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "kind": "crate",
                "duration_in_frames": 0
            }));
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("at least one frame"), "{reason}");
}

#[test]
fn keyframes_after_the_end_of_a_clip_are_brought_in_to_its_last_frame_when_a_file_is_read() {
    // The first video clip lasts 300 frames: the second keyframe of its gain, at frame 1000, is where it is not.
    let text = saved_with(|file| {
        rack_of_mut(&mut file["tracks"][2]["clips"][0]["drawer"], "gain")["keyframes"][1]["frame"] =
            serde_json::json!(1000);
    });
    let project = project_from_json_text(&text).unwrap();
    let video = project.tracks()[2].entries()[0].id();
    let keyframes = project
        .placement_of(video)
        .unwrap()
        .own_animation()
        .keyframes_of(PropertyId::Gain)
        .unwrap()
        .iter()
        .map(|keyframe| (keyframe.frame, keyframe.value.clone()))
        .collect::<Vec<_>>();
    // The one in the clip is kept, and the one that is not is at the last frame of the clip, with its value.
    assert_eq!(keyframes.len(), 2, "{keyframes:?}");
    assert_eq!(keyframes[0], (0, PropertyValue::Number(0.0)));
    assert_eq!(
        keyframes[1],
        (299, PropertyValue::Number(-6.0)),
        "the last frame of the clip"
    );
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn keyframes_after_the_end_of_the_clip_that_has_the_drawer_are_brought_in_even_if_they_are_of_another_clip() {
    // The caption lasts 90 frames, and its drawer has a rack of the video's rotation that has a keyframe at frame 100.
    let text = saved_with(|file| {
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "rotation", "target": {"track": 2, "clip": 0}, "keyframes": [{"frame": 10, "value": 0.0}, {"frame": 100, "value": 90.0}]}
        ]);
    });
    let project = project_from_json_text(&text).unwrap();
    let caption = &project.tracks()[0].entries()[0];
    let rack = &caption.drawer().racks()[0];
    assert_eq!(
        rack.keyframes()
            .iter()
            .map(|keyframe| (keyframe.frame, keyframe.value.clone()))
            .collect::<Vec<_>>(),
        vec![(10, PropertyValue::Number(0.0)), (89, PropertyValue::Number(90.0))],
        "the one that was past the end is at the last frame of the caption, with its value"
    );
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
}
