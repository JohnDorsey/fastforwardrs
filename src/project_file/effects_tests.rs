// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how the effects of clips and tracks are saved in a project file and read back, and what is refused when a file says
//! something that cannot be.

use super::effects::{EffectDocument, KindsOfEffects, ValuesDocument, effects_from_documents};
use super::old_text_files_tests::as_a_file_of_a_version_before_18;
use super::tests::{
    assert_same_content, current_version_line, edited_project, link_to_missing_video, project_with_everything,
    reason_for_unreadable_project, saved, valid_text_with, version_line,
};
use super::*;
use crate::composition::test_projects::random_project_with_effects;
use crate::project::{HUE_SHIFT_KEY, PropertyId, TimelineFrameIndex};
use crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE;
use crate::test_support::{DeterministicRandomNumbers, make_a_keyframe};

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

/// The JSON of the file of `project_with_everything`, after `change` has been made to it. (The captions track is the first of the
/// tracks and has the text object as its clip, the music track is the second and has the sound, and the track of the video is the
/// third and has two clips. See `project_with_everything`, which gives all of them a hue shift but the sound.)
pub(super) fn the_file_of_everything_with(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    change(&mut file);
    serde_json::to_string_pretty(&file).unwrap()
}

pub(super) fn reason_for_invalid_project(text: &str) -> String {
    match project_from_json_text(text) {
        Err(ProjectFileError::InvalidProject { reason }) => reason,
        other => panic!("expected InvalidProject, got {other:?}"),
    }
}

// --- what the file says ---

#[test]
fn effects_are_saved_with_the_key_of_the_effect_and_the_values_it_has_when_they_are_not_what_it_begins_with_by_their_ids()
 {
    let file: serde_json::Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    let hue_shift = |extra: serde_json::Value| {
        let mut effect = serde_json::json!({"effect": "builtin:hue_shift", "version": [1, 1], "instance": 0});
        effect
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        effect
    };

    // The first video has it as it begins, the second is turned 90 degrees.
    assert_eq!(
        file["tracks"][2]["clips"][0]["effects"],
        serde_json::json!([hue_shift(serde_json::json!({}))])
    );
    assert_eq!(
        file["tracks"][2]["clips"][1]["effects"],
        serde_json::json!([hue_shift(serde_json::json!({"values": {"degrees": 90.0}}))])
    );
    // The text object: first the text effect that it is made of (with what it says, as it is not what a new text says), then its hue shift as it begins, which is the second effect of
    // its chain (so its `instance` is 1). And the track of the text turned back. (Rewritten on purpose in phase 9.4: the text was not an effect before.)
    let mut hue_shift_of_the_text = hue_shift(serde_json::json!({}));
    hue_shift_of_the_text["instance"] = serde_json::json!(1);
    assert_eq!(
        file["tracks"][0]["clips"][0]["effects"],
        serde_json::json!([
            {
                "effect": "builtin:text",
                "version": [1, 0],
                "instance": 0,
                "values": {
                    "text": "Hello\nthere \"world\"",
                    "font": "Arial",
                    "size": 0.05,
                    "colour": "#FFC80080"
                }
            },
            hue_shift_of_the_text
        ])
    );
    assert_eq!(
        file["tracks"][0]["effects"],
        serde_json::json!([hue_shift(serde_json::json!({"values": {"degrees": -30.0}}))])
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
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();
    project
        .set_property_at(AnimationTarget::Clip(clip), HUE, turn(45.0), TimelineFrameIndex::new(0))
        .unwrap();
    assert!(saved(&project).contains("\"values\""));

    project
        .set_property_at(AnimationTarget::Clip(clip), HUE, turn(0.0), TimelineFrameIndex::new(0))
        .unwrap();

    let text = saved(&project);
    assert!(text.contains("\"effect\": \"builtin:hue_shift\""), "{text}");
    assert!(!text.contains("\"values\""), "{text}");
}

// --- the trip through the file ---

#[test]
fn the_effects_of_clips_and_of_tracks_with_their_values_survive_a_trip_through_the_text() {
    let project = project_with_everything();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    let clip_at = |track: usize, clip: usize| AnimationTarget::Clip(loaded.tracks()[track].entries()[clip].id());
    let value_of_the_hue_shift_of =
        |target: AnimationTarget| loaded.effects_of(target).and_then(|effects| effects.value_of(HUE));
    // (The hue shift of the text is the second effect of its chain, the first being the text effect that it is made of.)
    let hue_shift_of_the_text = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::new(1), 0);
    assert_eq!(value_of_the_hue_shift_of(clip_at(2, 0)), Some(turn(0.0)));
    assert_eq!(value_of_the_hue_shift_of(clip_at(2, 1)), Some(turn(90.0)));
    assert_eq!(
        loaded
            .effects_of(clip_at(0, 0))
            .and_then(|effects| effects.value_of(hue_shift_of_the_text)),
        Some(turn(0.0))
    );
    assert_eq!(
        value_of_the_hue_shift_of(clip_at(0, 0)),
        None,
        "the first effect of the text is not a hue shift"
    );
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
    make_a_keyframe(&mut project, video, HUE, 15, turn(-90.0));
    make_a_keyframe(&mut project, video, HUE, 75, turn(450.0));
    make_a_keyframe(&mut project, captions, HUE, 130, turn(10.0));

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    let loaded_video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let hue_at = |frame: u64| loaded.property_value_at(loaded_video, HUE, TimelineFrameIndex::new(frame));
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
            .find(|rack| rack["property"] == "degrees")
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
        .add_effect(AnimationTarget::Track(copy), EffectKind::HUE_SHIFT)
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
                .any(|located| located.rack.property().is_a_value_of_an_effect()),
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
        // (A text of a version before 18 says what it says in the clip, and its chain has no text effect in it: it is that that has "no effects" in a file of version 7.)
        as_a_file_of_a_version_before_18(file);
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
fn two_effects_that_have_the_same_name_in_one_chain_are_refused_and_so_are_some_named_and_some_not() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["effects"] = serde_json::json!([
            {"effect": "hue shift", "instance": 3},
            {"effect": "hue shift", "instance": 3, "values": [5.0]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("two effects that are named effect 3"), "{reason}");
    assert!(reason.contains("track number 1"), "{reason}");

    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["effects"] = serde_json::json!([
            {"effect": "hue shift", "instance": 0},
            {"effect": "hue shift", "values": [5.0]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(
        reason.contains("some of its effects have a name in its chain and some have not"),
        "{reason}"
    );
    assert!(reason.contains("track number 1"), "{reason}");
}

#[test]
fn a_rack_of_keyframes_that_names_an_effect_that_the_thing_does_not_have_is_refused_by_the_name() {
    let text = the_file_of_everything_with(|file| {
        file["tracks"][2]["clips"][0]["drawer"] = serde_json::json!([
            {"property": "hue shift", "effect": 5, "keyframes": [{"frame": 5, "value": 10.0}]}
        ]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(reason.contains("\"hue shift\" cannot be animated"), "{reason}");
    assert!(
        reason.contains("it does not have effect 5 that has a value of that name"),
        "{reason}"
    );
    assert!(reason.contains("clip number 1 of track number 3"), "{reason}");

    // A value that the effect named does not have is as bad, and so is the name of a property that a clip has itself.
    for property in ["rotation", "turn"] {
        let text = the_file_of_everything_with(|file| {
            file["tracks"][2]["clips"][0]["drawer"] = serde_json::json!([
                {"property": property, "effect": 0, "keyframes": [{"frame": 5, "value": 10.0}]}
            ]);
        });
        let reason = reason_for_invalid_project(&text);
        assert!(
            reason.contains("it does not have effect 0 that has a value of that name"),
            "{property}: {reason}"
        );
    }
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
fn a_keyframe_crate_has_no_effects_to_be_given_in_a_file_and_a_sound_has_only_those_of_sound_plugins() {
    // (A sound clip has effects, those of sound plugins: `audio_effects_tests`. A picture effect is not one of them.)
    let text = the_file_of_everything_with(|file| {
        file["tracks"][1]["clips"][0]["effects"] = serde_json::json!([{"effect": "hue shift"}]);
    });
    let reason = reason_for_invalid_project(&text);
    assert!(
        reason.contains("clip number 1 of track number 2") && reason.contains("it cannot have the hue shift effect"),
        "the clip has a place for effects, and the effect is refused for what it is: {reason}"
    );

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
fn a_field_of_an_effect_that_this_version_does_not_know_is_kept_and_written_back_even_if_it_is_a_misspelling() {
    // (What a newer program wrote is kept, and so is what is misspelt: there is no telling them apart. The effect has its values still.)
    let text = the_file_of_everything_with(|file| {
        file["tracks"][0]["effects"] = serde_json::json!([{"effect": "hue shift", "value": [5.0], "values": [3.0]}]);
    });

    let project = project_from_json_text(&text).unwrap();

    let captions = AnimationTarget::Track(project.tracks()[0].id());
    let chain = project.effects_of(captions).unwrap();
    assert_eq!(chain.value_of(HUE), Some(turn(3.0)));
    assert_eq!(
        chain.iter().next().unwrap().unknown_fields().fields().get("value"),
        Some(&serde_json::json!([5.0]))
    );
    let written: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();
    assert_eq!(written["tracks"][0]["effects"][0]["value"], serde_json::json!([5.0]));
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
    assert!(project.is_animated(video, HUE));
    // (The drawer of the text, which starts at frame 120 of the timeline, so its keyframes are at 125 and 141: 133 is half way.)
    assert_eq!(
        project.property_value_at(video, HUE, TimelineFrameIndex::new(133)),
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
            effect: HUE_SHIFT_KEY.to_owned(),
            version: None,
            instance: None,
            values: ValuesDocument::default(),
            enabled: true,
            unknown: std::collections::BTreeMap::new(),
        }]
    };
    let kinds = KindsOfEffects::from_documents(Vec::new()).unwrap();

    let allowed = effects_from_documents(documents(), &kinds, |_| true).unwrap();
    assert_eq!(allowed.len(), 1);
    assert_eq!(allowed[0].kind(), EffectKind::HUE_SHIFT);

    let reason = effects_from_documents(documents(), &kinds, |_| false).unwrap_err();
    assert!(reason.contains("it cannot have the hue shift effect"), "{reason}");
}

// --- several effects of one kind ---

/// The value of the hue shift that is named `number` in its chain.
fn hue_of(number: u32) -> PropertyId {
    PropertyId::effect_value(EffectKind::HUE_SHIFT, crate::project::EffectInstanceId::new(number), 0)
}

/// The project of everything, with two more hue shifts on its first video (it has one, named 0, already): the effects 0, 1 and 2, turning by
/// 0, 60 and -45 degrees, and keyframes of the values of the first and the third.
fn project_with_three_hue_shifts_on_the_first_video() -> (Project, AnimationTarget) {
    let mut project = project_with_everything();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    for degrees in [60.0, -45.0] {
        let instance = project.add_effect(video, EffectKind::HUE_SHIFT).unwrap();
        project
            .set_property_at(
                video,
                hue_of(instance.number()),
                turn(degrees),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
    }
    make_a_keyframe(&mut project, video, hue_of(0), 15, turn(-90.0));
    make_a_keyframe(&mut project, video, hue_of(0), 75, turn(450.0));
    make_a_keyframe(&mut project, video, hue_of(2), 30, turn(100.0));
    (project, video)
}

#[test]
fn several_effects_of_one_kind_are_saved_each_with_its_name_and_values_and_come_back_as_they_were_in_the_same_order() {
    let (project, _) = project_with_three_hue_shifts_on_the_first_video();

    let file: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();

    assert_eq!(
        file["tracks"][2]["clips"][0]["effects"],
        serde_json::json!([
            {"effect": "builtin:hue_shift", "version": [1, 1], "instance": 0},
            {"effect": "builtin:hue_shift", "version": [1, 1], "instance": 1, "values": {"degrees": 60.0}},
            {"effect": "builtin:hue_shift", "version": [1, 1], "instance": 2, "values": {"degrees": -45.0}}
        ])
    );
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    assert_same_content(&loaded, &project);
    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let chain = loaded.effects_of(video).unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(
        chain
            .iter()
            .map(|effect| effect.instance().number())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(chain.value_of(hue_of(1)), Some(turn(60.0)));
    assert_eq!(chain.value_of(hue_of(2)), Some(turn(-45.0)));
}

#[test]
fn the_keyframes_of_each_of_several_effects_of_one_kind_are_racks_that_say_which_effect_they_are_of_and_come_back_to_it()
 {
    let (project, _) = project_with_three_hue_shifts_on_the_first_video();

    let file: serde_json::Value = serde_json::from_str(&saved(&project)).unwrap();

    let racks = file["tracks"][2]["clips"][0]["drawer"].as_array().unwrap();
    let keyframes_of_the_effect_named = |number: u32| {
        racks
            .iter()
            .find(|rack| rack["property"] == "degrees" && rack["effect"] == number)
            .map(|rack| rack["keyframes"].clone())
    };
    assert_eq!(
        keyframes_of_the_effect_named(0),
        Some(serde_json::json!([{"frame": 15, "value": -90.0}, {"frame": 75, "value": 450.0}]))
    );
    assert_eq!(keyframes_of_the_effect_named(1), None, "the second has none");
    assert_eq!(
        keyframes_of_the_effect_named(2),
        Some(serde_json::json!([{"frame": 30, "value": 100.0}]))
    );
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    let at = |number: u32, frame: u64| loaded.property_value_at(video, hue_of(number), TimelineFrameIndex::new(frame));
    assert_eq!(at(0, 45), Some(turn(180.0)), "half way from -90 to 450, of the first");
    assert_eq!(at(1, 45), Some(turn(60.0)), "the second does not move");
    assert_eq!(at(2, 5), Some(turn(100.0)), "the third has one keyframe: that");
}

#[test]
fn the_names_of_the_effects_are_kept_by_the_file_when_they_are_not_the_first_numbers() {
    let (mut project, video) = project_with_three_hue_shifts_on_the_first_video();
    project
        .remove_effect(video, crate::project::EffectInstanceId::new(0))
        .unwrap();
    project
        .remove_effect(video, crate::project::EffectInstanceId::new(1))
        .unwrap();
    assert_eq!(
        project
            .effects_of(video)
            .unwrap()
            .iter()
            .map(|effect| effect.instance().number())
            .collect::<Vec<_>>(),
        [2]
    );

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    let video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    assert_eq!(
        loaded
            .effects_of(video)
            .unwrap()
            .iter()
            .map(|effect| effect.instance().number())
            .collect::<Vec<_>>(),
        [2],
        "it is not given the name of the first"
    );
    assert_eq!(
        loaded.property_value_at(video, hue_of(2), TimelineFrameIndex::new(5)),
        Some(turn(100.0)),
        "and its keyframes are still its own"
    );
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
}

/// What the old files called the hue shift, and its one value: both `hue shift`.
const OLD_NAME_OF_THE_HUE_SHIFT: &str = "hue shift";

/// The text of `project` as the old files had it: the effects are called by name (`hue shift`, where a file of today has a key), their values are
/// a list, there are no snapshots of descriptors, and a rack of the keyframes of the value of an effect calls the value by its name. A file of
/// version 10 had no names for the effects in their chains and did not say which effect a rack was of (`with_instances` is `false`); a file
/// of version 11 as it was written while the effects were being given names of their own in their chains did (`true`). (A text says what it says in its clip, as it did before
/// version 18: see [`as_a_file_of_a_version_before_18`].)
fn saved_the_way_the_old_files_did(project: &Project, with_instances: bool) -> String {
    fn change(value: &mut serde_json::Value, with_instances: bool) {
        match value {
            serde_json::Value::Object(object) => {
                let is_a_rack = object.contains_key("property");
                let is_an_effect = object.get("effect").is_some_and(serde_json::Value::is_string);
                if is_a_rack && let Some(instance) = object.remove("effect") {
                    object.insert("property".to_owned(), serde_json::json!(OLD_NAME_OF_THE_HUE_SHIFT));
                    if with_instances {
                        object.insert("effect".to_owned(), instance);
                    }
                }
                if is_an_effect {
                    object.insert("effect".to_owned(), serde_json::json!(OLD_NAME_OF_THE_HUE_SHIFT));
                    object.remove("version");
                    if !with_instances {
                        object.remove("instance");
                    }
                    if let Some(serde_json::Value::Object(values)) = object.remove("values") {
                        let in_order: Vec<serde_json::Value> = values.get("degrees").into_iter().cloned().collect();
                        object.insert("values".to_owned(), serde_json::Value::Array(in_order));
                    }
                }
                object.values_mut().for_each(|inner| change(inner, with_instances));
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|inner| change(inner, with_instances)),
            _ => {}
        }
    }
    let mut file: serde_json::Value = serde_json::from_str(&saved(project)).unwrap();
    as_a_file_of_a_version_before_18(&mut file);
    file.as_object_mut().unwrap().remove("plugins");
    change(&mut file, with_instances);
    file["file_format_version"] = serde_json::json!(if with_instances { 11 } else { 10 });
    serde_json::to_string_pretty(&file).unwrap()
}

/// The project of everything with keyframes of the values of two of its hue shifts: the video's first, and the captions track's. (The text has no hue shift in it: a text that a file of a
/// version before 18 had is given a text effect that has the name that comes after those of its other effects, and a file of version 10 names the effects in the order they are in, so the
/// text effect of a text that has one has another name than the one that a text that was made today has; what these files are about is the effects that have names of their own.)
fn project_with_keyframes_of_hue_shifts() -> Project {
    let mut project = project_with_everything();
    let text = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    project.remove_effect(text, EffectInstanceId::new(1)).unwrap();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let captions = AnimationTarget::Track(project.tracks()[0].id());
    make_a_keyframe(&mut project, video, HUE, 15, turn(-90.0));
    make_a_keyframe(&mut project, video, HUE, 75, turn(450.0));
    make_a_keyframe(&mut project, captions, HUE, 130, turn(10.0));
    project
}

#[test]
fn a_file_of_version_10_names_no_effect_and_no_rack_names_one_and_is_read_as_the_project_it_was() {
    // (One hue shift on each of the clips and the track that have one, with keyframes of the values of two of them, as version 10 had them.)
    let project = project_with_keyframes_of_hue_shifts();
    let text = saved_the_way_the_old_files_did(&project, false);
    assert!(text.contains(&version_line(10)), "{text}");
    assert!(!text.contains("\"instance\""), "{text}");
    assert!(
        !text.contains("plugins") && !text.contains("builtin:"),
        "no key and no snapshot: {text}"
    );
    assert!(
        !text.contains("\"effect\": 0"),
        "no rack says which effect it is of: {text}"
    );
    assert!(
        text.contains("\"effect\": \"hue shift\""),
        "the effects are called by their names: {text}"
    );
    assert!(
        text.contains("\"values\": ["),
        "and have their values in a list: {text}"
    );

    let loaded = project_from_json_text(&text).unwrap();

    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    let loaded_video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    assert_eq!(
        loaded.property_value_at(loaded_video, HUE, TimelineFrameIndex::new(45)),
        Some(turn(180.0)),
        "the keyframes are those of the one hue shift there is"
    );
    assert!(
        saved(&loaded).contains(&current_version_line()),
        "it is saved in the layout of today"
    );
    assert!(
        saved(&loaded).contains("\"instance\": 0"),
        "with the effects named, in the order they were in"
    );
    assert!(
        saved(&loaded).contains("\"key\": \"builtin:hue_shift\""),
        "and a snapshot of what the effect is"
    );
}

#[test]
fn a_file_of_version_11_as_it_was_written_while_the_effects_were_being_named_is_read_as_the_project_it_was() {
    // (Effects that had names in their chains and were called by name, with their values in a list, and racks that said which effect they were of and
    // called the value by its name. Nothing was released like that, but a program that was being built did write it.)
    let mut project = project_with_keyframes_of_hue_shifts();
    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    let second = project.add_effect(video, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_property_at(video, hue_of(second.number()), turn(60.0), TimelineFrameIndex::new(0))
        .unwrap();
    make_a_keyframe(&mut project, video, hue_of(second.number()), 30, turn(100.0));
    let text = saved_the_way_the_old_files_did(&project, true);
    assert!(text.contains(&version_line(11)), "{text}");
    assert!(text.contains("\"instance\": 1"), "{text}");
    assert!(!text.contains("plugins") && !text.contains("builtin:"), "{text}");

    let loaded = project_from_json_text(&text).unwrap();

    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    let loaded_video = AnimationTarget::Clip(loaded.tracks()[2].entries()[0].id());
    assert_eq!(
        loaded.property_value_at(loaded_video, hue_of(1), TimelineFrameIndex::new(5)),
        Some(turn(100.0)),
        "the keyframes of the second are the second's"
    );
}

#[test]
fn a_file_of_version_10_that_has_two_effects_of_one_kind_names_them_in_the_order_they_are_in() {
    // (Version 10 did not allow it, but a file that was edited by hand may: nothing is lost by reading it, as the order is all there is.)
    let text = the_file_of_everything_with(|file| {
        file["file_format_version"] = serde_json::json!(10);
        file["tracks"][2]["clips"][0]["effects"] = serde_json::json!([
            {"effect": "hue shift", "values": [10.0]},
            {"effect": "hue shift", "values": [20.0]}
        ]);
    });

    let project = project_from_json_text(&text).unwrap();

    let video = AnimationTarget::Clip(project.tracks()[2].entries()[0].id());
    assert_eq!(project.effects_of(video).unwrap().len(), 2);
    assert_eq!(project.effects_of(video).unwrap().value_of(hue_of(0)), Some(turn(10.0)));
    assert_eq!(project.effects_of(video).unwrap().value_of(hue_of(1)), Some(turn(20.0)));
}
