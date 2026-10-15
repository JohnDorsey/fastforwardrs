// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how the effects of clips and tracks are saved in a project file and read back, and what is refused when a file says
//! something that cannot be.

use super::tests::{
    assert_same_content, current_version_line, edited_project, link_to_missing_video, project_with_everything,
    reason_for_unreadable_project, saved, valid_text_with, version_line,
};
use super::*;
use crate::composition::test_projects::random_project_with_effects;
use crate::project::TimelineFrameIndex;
use crate::test_support::{DeterministicRandomNumbers, make_a_keyframe};

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

/// The JSON of the file of `project_with_everything`, after `change` has been made to it. (The captions track is the first of the
/// tracks and has the text object as its clip, the music track is the second and has the sound, and the track of the video is the
/// third and has two clips. See `project_with_everything`, which gives all of them a hue shift but the sound.)
fn the_file_of_everything_with(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    change(&mut file);
    serde_json::to_string_pretty(&file).unwrap()
}

fn reason_for_invalid_project(text: &str) -> String {
    match project_from_json_text(text) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("expected InvalidProject, got {other:?}"),
    }
}

// --- what the file says ---

#[test]
fn effects_are_saved_with_the_name_of_the_effect_and_the_values_it_has_when_they_are_not_what_it_begins_with() {
    let file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();

    // The first video has it as it begins, the second is turned 90 degrees.
    assert_eq!(
        file["tracks"][2]["clips"][0]["effects"],
        serde_json::json!([{"effect": "hue shift"}])
    );
    assert_eq!(
        file["tracks"][2]["clips"][1]["effects"],
        serde_json::json!([{"effect": "hue shift", "values": [90.0]}])
    );
    // The text object as it begins, and the track of the text turned back.
    assert_eq!(
        file["tracks"][0]["clips"][0]["effects"],
        serde_json::json!([{"effect": "hue shift"}])
    );
    assert_eq!(
        file["tracks"][0]["effects"],
        serde_json::json!([{"effect": "hue shift", "values": [-30.0]}])
    );
}

#[test]
fn what_has_no_effects_has_no_mention_of_them_in_its_file() {
    let file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    assert!(file["tracks"][1].get("effects").is_none(), "the music track");
    assert!(file["tracks"][1]["clips"][0].get("effects").is_none(), "the sound");
    assert!(file["tracks"][2].get("effects").is_none(), "the track of the videos");

    let plain = saved(&edited_project(link_to_missing_video()));
    assert!(!plain.contains("effects"), "{plain}");
}

#[test]
fn the_values_of_an_effect_that_are_turned_back_to_what_it_begins_with_are_left_out_again() {
    let mut project = edited_project(link_to_missing_video());
    let clip = project.tracks()[0].entries()[0].id();
    project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
        .unwrap();
    project
        .set_property_at(
            AnimationTarget::Clip(clip),
            PropertyId::HueShift,
            turn(45.0),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    assert!(saved(&project).contains("\"values\""));

    project
        .set_property_at(
            AnimationTarget::Clip(clip),
            PropertyId::HueShift,
            turn(0.0),
            TimelineFrameIndex::new(0),
        )
        .unwrap();

    let text = saved(&project);
    assert!(text.contains("\"effect\": \"hue shift\""), "{text}");
    assert!(!text.contains("\"values\""), "{text}");
}

// --- the trip through the file ---

#[test]
fn the_effects_of_clips_and_of_tracks_with_their_values_survive_a_trip_through_the_text() {
    let project = project_with_everything();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    let clip_at = |track: usize, clip: usize| AnimationTarget::Clip(loaded.tracks()[track].entries()[clip].id());
    let value_of_the_hue_shift_of = |target: AnimationTarget| {
        loaded
            .effects_of(target)
            .and_then(|effects| effects.value_of(PropertyId::HueShift))
    };
    assert_eq!(value_of_the_hue_shift_of(clip_at(2, 0)), Some(turn(0.0)));
    assert_eq!(value_of_the_hue_shift_of(clip_at(2, 1)), Some(turn(90.0)));
    assert_eq!(value_of_the_hue_shift_of(clip_at(0, 0)), Some(turn(0.0)));
    assert_eq!(
        value_of_the_hue_shift_of(AnimationTarget::Track(loaded.tracks()[0].id())),
        Some(turn(-30.0))
    );
    assert_eq!(value_of_the_hue_shift_of(clip_at(1, 0)), None, "the sound has none");
    assert_eq!(
        value_of_the_hue_shift_of(AnimationTarget::Track(loaded.tracks()[1].id())),
        None,
        "nor has the music track"
    );
}

#[test]
fn the_keyframes_of_the_value_of_an_effect_survive_a_trip_through_the_text_even_when_they_go_round_more_than_once() {
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let captions = AnimationTarget::Track(project.tracks()[0].id());
    make_a_keyframe(&mut project, video, PropertyId::HueShift, 15, turn(-90.0));
    make_a_keyframe(&mut project, video, PropertyId::HueShift, 75, turn(450.0));
    make_a_keyframe(&mut project, captions, PropertyId::HueShift, 130, turn(10.0));

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    let loaded_video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let hue_at =
        |frame: u64| loaded.property_value_at(loaded_video, PropertyId::HueShift, TimelineFrameIndex::new(frame));
    assert_eq!(hue_at(15), Some(turn(-90.0)));
    assert_eq!(hue_at(45), Some(turn(180.0)), "half way from -90 to 450");
    assert_eq!(
        hue_at(75),
        Some(turn(450.0)),
        "not taken round to a turn that is the same colour"
    );
    let rack_written: serde_json::Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(
        rack_written["tracks"][2]["clips"][0]["drawer"]
            .as_array()
            .unwrap()
            .iter()
            .find(|rack| rack["property"] == "hue shift")
            .unwrap()["keyframes"],
        serde_json::json!([{"frame": 15, "value": -90.0}, {"frame": 75, "value": 450.0}])
    );
}

#[test]
fn the_effects_of_clips_that_were_cut_and_of_a_track_that_was_copied_are_still_what_they_were_after_a_trip() {
    let mut project = project_with_everything();
    let video_track = project.tracks()[2].id();
    let first_video = project.tracks()[2].entries()[0].id();
    project.split_at(TimelineFrameIndex::new(100), Some(&[first_video]));
    let copy = project
        .duplicate_track(video_track, crate::project::TrackSide::Above)
        .unwrap();
    project
        .add_effect(AnimationTarget::Track(copy), EffectKind::HueShift)
        .unwrap();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn projects_made_at_random_with_effects_survive_a_trip_through_the_text() {
    let (mut with_effects, mut with_animated_effects) = (0, 0);
    for seed in 1..=120u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 41_000);
        let project = random_project_with_effects(&mut random);

        let text = saved(&project);
        let loaded = project_from_json_text(&text).unwrap_or_else(|error| panic!("seed {seed}: {error:?}\n{text}"));

        assert_same_content(&loaded, &project);
        assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new(), "seed {seed}");
        let has_one = |project: &Project| {
            project.tracks().iter().any(|track| {
                !track.effects().is_empty() || track.entries().iter().any(|entry| !entry.effects().is_empty())
            })
        };
        with_effects += usize::from(has_one(&loaded));
        with_animated_effects += usize::from(
            loaded
                .located_racks()
                .any(|located| located.rack.property().effect().is_some()),
        );
    }
    // (So that the projects made at random cannot stop having effects in them without this noticing.)
    assert!(with_effects > 100, "only {with_effects} projects had effects");
    assert!(
        with_animated_effects > 60,
        "only {with_animated_effects} projects had keyframes of effects"
    );
}

// --- files of earlier versions ---

#[test]
fn a_file_of_version_7_has_no_effects_and_is_saved_in_the_current_layout() {
    let text = valid_text_with(&current_version_line(), &version_line(7));

    let project = project_from_json_text(&text).unwrap();

    assert!(
        project.tracks().iter().all(|track| {
            track.effects().is_empty() && track.entries().iter().all(|entry| entry.effects().is_empty())
        })
    );
    assert!(saved(&project).contains(&current_version_line()));
    assert_same_content(&project, &project_from_json_text(&text).unwrap());
}

#[test]
fn a_file_of_version_7_that_has_everything_a_version_8_file_has_but_effects_loads_as_it_was() {
    // (The file of today of a project with everything, with the effects taken out and the version put back.)
    let text = the_file_of_everything_with(|file| {
        for track in file["tracks"].as_array_mut().unwrap() {
            track.as_object_mut().unwrap().remove("effects");
            for clip in track["clips"].as_array_mut().unwrap() {
                clip.as_object_mut().unwrap().remove("effects");
            }
        }
        file["file_format_version"] = serde_json::json!(7);
    });

    let project = project_from_json_text(&text).unwrap();

    assert_eq!(project.tracks().len(), 3);
    assert!(project.tracks().iter().all(|track| track.effects().is_empty()));
    assert!(saved(&project).contains(&current_version_line()));
}

// --- what is wrong with a file ---

#[test]
fn an_effect_that_there_is_not_is_refused_by_name_and_the_clip_or_track_is_named() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][2]["clips"][1]["effects"] = serde_json::json!([{"effect": "sepia"}]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"sepia\" is not an effect there is"), "{reason}");
    assert!(reason.contains("clip number 2 of track number 3"), "{reason}");

    let text = the_file_of_everything_with(|file| {
        file["tracks"][1]["effects"] = serde_json::json!([{"effect": "blur"}]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"blur\" is not an effect there is"), "{reason}");
    assert!(reason.contains("track number 2"), "{reason}");
}

#[test]
fn an_effect_that_is_put_on_twice_is_refused() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["effects"] =
            serde_json::json!([{"effect": "hue shift"}, {"effect": "hue shift", "values": [5.0]}]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("has the hue shift effect twice"), "{reason}");
    assert!(reason.contains("track number 1"), "{reason}");
}

#[test]
fn values_of_an_effect_that_are_too_few_or_too_many_or_of_the_wrong_kind_are_refused_by_what_is_wrong() {
    for (values, wanted_in_the_reason) in [
        (serde_json::json!([]), None),
        (
            serde_json::json!([1.0, 2.0]),
            Some("a list of 1, and the file's list is of 2"),
        ),
        (serde_json::json!(["blue"]), Some("words, where a number is wanted")),
        (
            serde_json::json!([true]),
            Some("true or false, where a number is wanted"),
        ),
        (
            serde_json::json!([[1.0, 2.0]]),
            Some("two numbers, where a number is wanted"),
        ),
    ] {
        let text = the_file_of_everything_with(|file| {
            file["tracks"][2]["clips"][0]["effects"] = serde_json::json!([{"effect": "hue shift", "values": values}]);
        });
        match wanted_in_the_reason {
            // (No values at all is how an effect that has the values it begins with is written.)
            None => assert!(project_from_json_text(&text).is_ok(), "{values}"),
            Some(wanted) => {
                let reason = reason_for_invalid_project(&text);
                assert!(reason.contains(wanted), "{values}: {reason}");
                assert!(reason.contains("the hue shift effect"), "{values}: {reason}");
            }
        }
    }
}

#[test]
fn a_sound_and_a_keyframe_crate_have_no_effects_to_be_given_in_a_file() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][1]["clips"][0]["effects"] = serde_json::json!([{"effect": "hue shift"}]);
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `effects`"), "{reason}");

    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["clips"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "kind": "crate",
                "gap_before_frames": 40,
                "duration_in_frames": 200,
                "effects": [{"effect": "hue shift"}]
            }));
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `effects`"), "{reason}");
}

#[test]
fn a_misspelt_field_of_an_effect_is_reported_instead_of_silently_ignored() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["effects"] = serde_json::json!([{"effect": "hue shift", "value": [5.0]}]);
    });
    let reason = reason_for_unreadable_project(&text);
    assert!(reason.contains("unknown field `value`"), "{reason}");
}

#[test]
fn keyframes_of_the_value_of_an_effect_that_what_they_are_of_does_not_have_are_refused() {
    // The sound's drawer holding the keyframes of the hue shift of the first video, which does not have the effect in this file.
    let text = the_file_of_everything_with(|file| {
        file["tracks"][2]["clips"][0].as_object_mut().unwrap().remove("effects");
        file["tracks"][2]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "hue shift", "keyframes": [{"frame": 5, "value": 10.0}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"hue shift\" cannot be animated"), "{reason}");
    assert!(reason.contains("it does not have the hue shift effect"), "{reason}");
    assert!(reason.contains("clip number 1 of track number 3"), "{reason}");

    // A track too.
    let text = the_file_of_everything_with(|file| {
        file["tracks"][0].as_object_mut().unwrap().remove("effects");
        file["tracks"][0]["drawer"] = serde_json::json!([
            {"property": "hue shift", "keyframes": [{"frame": 5, "value": 10.0}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("it does not have the hue shift effect"), "{reason}");
    assert!(reason.contains("track number 1"), "{reason}");

    // The keyframes may be kept in the drawer of another clip, and are still refused where what they are of lacks the effect.
    let text = the_file_of_everything_with(|file| {
        file["tracks"][2]["clips"][0].as_object_mut().unwrap().remove("effects");
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "hue shift", "target": {"track": 2, "clip": 0}, "keyframes": [{"frame": 5, "value": 10.0}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("it does not have the hue shift effect"), "{reason}");
}

#[test]
fn keyframes_of_an_effect_the_thing_has_are_accepted_wherever_they_are_kept() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "hue shift", "target": {"track": 2, "clip": 0}, "keyframes": [{"frame": 5, "value": 10.0}, {"frame": 21, "value": 100.0}]}
        ]);
    });

    let project = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    assert!(project.is_animated(video, PropertyId::HueShift));
    // (The drawer of the text, which starts at frame 120 of the timeline, so its keyframes are at 125 and 141: 133 is half way.)
    assert_eq!(
        project.property_value_at(video, PropertyId::HueShift, TimelineFrameIndex::new(133)),
        Some(turn(55.0))
    );
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
}

#[test]
fn an_effect_that_what_it_is_on_cannot_have_is_refused_whichever_effect_it_is() {
    // (With the effects there are, every one can be on everything that has an `effects` in its file, so this is the function that
    // reads them, told what can and what cannot be had.)
    let documents = || {
        vec![EffectDocument {
            effect: EffectKind::HueShift.name().to_owned(),
            values: Vec::new(),
        }]
    };

    let allowed = effects_from_documents(documents(), |_| true).unwrap();
    assert!(allowed.contains(EffectKind::HueShift));

    let reason = effects_from_documents(documents(), |_| false).unwrap_err();
    assert!(reason.contains("it cannot have the hue shift effect"), "{reason}");
}
