// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A file written before format 18, in which a text clip says what it says in the clip itself (phase 9.4), is read as a text made of its text effect: the effect is the first of the
//! chain of the clip, it is given the name that comes after the names of the other effects of the clip (so that they, and the racks of keyframes that name them, stay what they were), and
//! the keyframes of what the text said are those of the values of the effect.
//!
//! The files here are not kept as files: they are made from the file that the program writes today, taking the text clips back to the shape that the version before 18 wrote them in
//! ([`as_a_file_of_a_version_before_18`]), which the tests of other old files use too, so that the text clip that the project of everything has does not stop them being what they are.

use serde_json::{Value, json};

use super::tests::{project_with_everything, saved};
use super::*;
use crate::project::{AnimationTarget, EffectInstanceId, EffectKind, PropertyId, TextObject, TimelineFrameIndex};
use crate::test_support::{COLOUR_OF_A_TEXT, SIZE_OF_A_TEXT, TEXT_OF_A_TEXT, make_a_keyframe};

/// What a rack of the values of a text effect called its property in a file of a version before 18, for the id of the value in the effect.
fn the_old_name_of_a_value_of_the_text(id: &str) -> &str {
    match id {
        "size" => "font size",
        other => other,
    }
}

/// `file` (the JSON of a file that this version wrote) changed to what the version before 18 would have written for the same project: every text clip says what it says, its font, its size
/// and its colour in the clip (and has no `content`), has no text effect in its chain, and the badges of its chain and the racks of its keyframes (which have no `effect` when they are of
/// what the clip said) are as they were. The `file_format_version` is 17.
pub(super) fn as_a_file_of_a_version_before_18(file: &mut Value) {
    // Which clips are texts, and the name of the text effect that each is made of.
    let mut contents: Vec<(usize, usize, u64)> = Vec::new();
    for (track_number, track) in file["tracks"].as_array().unwrap().iter().enumerate() {
        for (clip_number, clip) in track["clips"].as_array().unwrap().iter().enumerate() {
            if clip["kind"] == "text" {
                contents.push((
                    track_number,
                    clip_number,
                    clip["content"].as_u64().expect("a text has content"),
                ));
            }
        }
    }
    let content_of = |track: usize, clip: usize| {
        contents
            .iter()
            .find(|(of_track, of_clip, _)| (*of_track, *of_clip) == (track, clip))
            .map(|(.., content)| *content)
    };
    // The racks: one that is of the value of the text effect of a text clip is of a property of the clip, with the name that it had.
    for (track_number, track) in file["tracks"].as_array_mut().unwrap().iter_mut().enumerate() {
        // (The track itself, and each of its clips: each can have a drawer.)
        for holder_place in std::iter::once(None).chain((0..track["clips"].as_array().unwrap().len()).map(Some)) {
            let holder = match holder_place {
                None => &mut *track,
                Some(clip_number) => &mut track["clips"][clip_number],
            };
            let Some(racks) = holder.get_mut("drawer").and_then(Value::as_array_mut) else {
                continue;
            };
            for rack in racks {
                let (target_track, target_clip) = match rack.get("target") {
                    Some(target) => (
                        usize::try_from(target["track"].as_u64().unwrap()).unwrap(),
                        target
                            .get("clip")
                            .map(|clip| usize::try_from(clip.as_u64().unwrap()).unwrap()),
                    ),
                    None => (track_number, holder_place),
                };
                let Some(clip) = target_clip else {
                    continue;
                };
                let (Some(content), Some(effect)) = (content_of(target_track, clip), rack.get("effect")) else {
                    continue;
                };
                if effect.as_u64() == Some(content) {
                    let id = rack["property"].as_str().unwrap().to_owned();
                    rack["property"] = json!(the_old_name_of_a_value_of_the_text(&id));
                    rack.as_object_mut().unwrap().remove("effect");
                }
            }
        }
    }
    // The text clips themselves.
    for (track_number, clip_number, content) in contents {
        let clip = &mut file["tracks"][track_number]["clips"][clip_number];
        clip.as_object_mut().unwrap().remove("content");
        let effects = clip["effects"].as_array_mut().expect("a text has its text effect");
        let place = effects
            .iter()
            .position(|effect| effect["effect"] == "builtin:text" && effect["instance"].as_u64() == Some(content))
            .expect("the text effect is in the chain");
        let text_effect = effects.remove(place);
        if effects.is_empty() {
            clip.as_object_mut().unwrap().remove("effects");
        }
        let value = |id: &str, default: Value| text_effect["values"].get(id).cloned().unwrap_or(default);
        clip["text"] = value("text", json!(""));
        clip["font_family"] = value("font", json!(""));
        clip["font_size_fraction_of_frame_height"] = value("size", json!(0.06));
        clip["colour"] = value("colour", json!("#FFFFFFFF"));
        if let Some(badges) = clip.get_mut("badges").and_then(Value::as_array_mut) {
            for badge in badges {
                let before = badge.get("effects_before").and_then(Value::as_u64).unwrap_or(0);
                if before > u64::try_from(place).unwrap() {
                    if before == 1 {
                        badge.as_object_mut().unwrap().remove("effects_before");
                    } else {
                        badge["effects_before"] = json!(before - 1);
                    }
                }
            }
        }
    }
    // (No effect is of the text effect any more: the file does not say what it is.)
    if !file["tracks"].to_string().contains("builtin:text")
        && let Some(plugins) = file.get_mut("plugins").and_then(Value::as_array_mut)
    {
        plugins.retain(|snapshot| snapshot["key"] != "builtin:text");
        if plugins.is_empty() {
            file.as_object_mut().unwrap().remove("plugins");
        }
    }
    file["file_format_version"] = json!(17);
}

/// The project that the program makes of `project` when it reads the file that the version before 18 wrote for it: the same, but its texts are made of a text effect that has the name
/// that comes after those of the other effects of its clip, and is the first of its chain, which is not the name that the text effect of `project` has (the first that a chain gives
/// out). What a test of an old file compares what it read with when the project that it is made from has a text in it.
pub(super) fn as_read_from_the_file_of_a_version_before_18(project: &Project) -> Project {
    let mut file: Value = serde_json::from_str(&saved(project)).unwrap();
    as_a_file_of_a_version_before_18(&mut file);
    project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap()
}

/// The file of [`project_with_everything`] as the version before 18 wrote it.
fn the_old_file_of_everything() -> String {
    let mut file: Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    as_a_file_of_a_version_before_18(&mut file);
    serde_json::to_string_pretty(&file).unwrap()
}

#[test]
fn the_text_clips_of_an_old_file_say_what_they_say_in_the_clip_and_have_no_text_effect() {
    // (A check of the file that the other tests are made from: it is what a file of the version before 18 was.)
    let text = the_old_file_of_everything();
    let file: Value = serde_json::from_str(&text).unwrap();
    let caption = &file["tracks"][0]["clips"][0];
    assert_eq!(caption["kind"], "text");
    assert_eq!(caption["text"], "Hello\nthere \"world\"");
    assert_eq!(caption["font_family"], "Arial");
    assert_eq!(caption["font_size_fraction_of_frame_height"], 0.05);
    assert_eq!(caption["colour"], "#FFC80080");
    assert!(caption.get("content").is_none());
    assert!(!text.contains("builtin:text"), "{text}");
    assert_eq!(
        caption["effects"],
        json!([{"effect": "builtin:hue_shift", "version": [1, 1], "instance": 1}]),
        "only the hue shift of the text is left, as it was named"
    );
    assert_eq!(file["file_format_version"], 17);
}

#[test]
fn a_text_clip_of_an_old_file_becomes_a_text_that_is_made_of_a_text_effect_the_first_of_its_chain() {
    let project = project_with_everything();
    let loaded = project_from_json_text(&the_old_file_of_everything()).unwrap();

    let caption = loaded.tracks()[0].entries()[0].id();
    assert_eq!(
        loaded.text_object_of(caption),
        project.text_object_of(project.tracks()[0].entries()[0].id()),
        "it says what it said, in the font that it had, in the size and the colour that it had, where it was"
    );
    let chain: Vec<EffectKind> = loaded
        .effects_of(AnimationTarget::Clip(caption))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect();
    assert_eq!(
        chain,
        [EffectKind::TEXT, EffectKind::HUE_SHIFT],
        "the text effect first, and what it had after it"
    );
    let content = loaded.tracks()[0].entries()[0].clip().as_text().unwrap().content();
    let hue_shift = loaded
        .effects_of(AnimationTarget::Clip(caption))
        .unwrap()
        .iter()
        .nth(1)
        .unwrap()
        .instance();
    assert_eq!(
        hue_shift,
        EffectInstanceId::new(1),
        "the effects that it had keep their names, so that the racks that name them still do"
    );
    assert_eq!(
        content,
        EffectInstanceId::new(2),
        "the name that comes after those of the effects that it had"
    );
    assert!(
        saved(&loaded).contains(&super::tests::current_version_line()),
        "and it is saved as a file of today"
    );
}

#[test]
fn the_keyframes_of_what_a_text_said_in_an_old_file_are_the_keyframes_of_the_values_of_its_text_effect() {
    let mut project = project_with_everything();
    let caption = project.tracks()[0].entries()[0].id();
    let target = AnimationTarget::Clip(caption);
    make_a_keyframe(
        &mut project,
        target,
        COLOUR_OF_A_TEXT,
        130,
        PropertyValue::Colour(TextColour::new(1, 2, 3, 4)),
    );
    make_a_keyframe(
        &mut project,
        target,
        COLOUR_OF_A_TEXT,
        190,
        PropertyValue::Colour(TextColour::new(5, 6, 7, 8)),
    );
    make_a_keyframe(
        &mut project,
        target,
        TEXT_OF_A_TEXT,
        120,
        PropertyValue::Words("Before".to_owned()),
    );
    make_a_keyframe(
        &mut project,
        target,
        TEXT_OF_A_TEXT,
        150,
        PropertyValue::Words("Later".to_owned()),
    );
    make_a_keyframe(&mut project, target, SIZE_OF_A_TEXT, 160, PropertyValue::Number(0.2));
    let mut file: Value = serde_json::from_str(&saved(&project)).unwrap();
    as_a_file_of_a_version_before_18(&mut file);
    let old_properties: Vec<String> = file["tracks"][0]["clips"][0]["drawer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rack| rack["property"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        ["colour", "text", "font size"]
            .iter()
            .all(|name| old_properties.iter().any(|property| property == name)),
        "the racks of the old file have the names that the old files gave: {old_properties:?}"
    );
    assert!(
        file["tracks"][0]["clips"][0]["drawer"]
            .to_string()
            .contains("\"font size\"")
    );

    let loaded = project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap();

    let loaded_caption = AnimationTarget::Clip(loaded.tracks()[0].entries()[0].id());
    let content = loaded.tracks()[0].entries()[0].clip().as_text().unwrap().content();
    let [text, _font, size, colour] = crate::project::properties_of_the_text_effect(content);
    let at = |property: PropertyId, frame: u64| {
        loaded.property_value_at(loaded_caption, property, TimelineFrameIndex::new(frame))
    };
    assert_eq!(
        at(colour, 160),
        Some(PropertyValue::Colour(TextColour::new(3, 4, 5, 6))),
        "half way between the colours"
    );
    assert_eq!(
        at(text, 140),
        Some(PropertyValue::Words("Before".to_owned())),
        "the words stay until the next keyframe"
    );
    assert_eq!(at(text, 150), Some(PropertyValue::Words("Later".to_owned())));
    assert_eq!(at(size, 160), Some(PropertyValue::Number(0.2)));
    assert_eq!(loaded.problems_with_the_racks(), Vec::<String>::new());
    assert_eq!(
        loaded
            .text_object_at(loaded.tracks()[0].entries()[0].id(), TimelineFrameIndex::new(165))
            .map(|object| object.text),
        Some("Later".to_owned())
    );
}

#[test]
fn a_badge_that_was_before_the_first_effect_of_a_text_in_an_old_file_comes_after_the_text_effect() {
    // The caption has the badge of a tag after its hue shift in the file of today (`effects_before` is 2: the text effect and the hue shift). In the file of the version before 18 it
    // was 1, and the text effect that is made for it comes first, so it is 2 again; one that was at 0, before the hue shift, is 1: after the text effect.
    let mut file: Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    file["groups"] = json!([{
        "kind": "tag",
        "name": "intro",
        "colour": "#4BD254",
        "members": [{"clip": {"track": 0, "clip": 0}}]
    }]);
    file["tracks"][0]["clips"][0]["badges"] = json!([{"of": {"tag": 0}, "effects_before": 2}]);
    as_a_file_of_a_version_before_18(&mut file);
    assert_eq!(
        file["tracks"][0]["clips"][0]["badges"],
        json!([{"of": {"tag": 0}, "effects_before": 1}])
    );
    let loaded = project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap();
    let saved_again: Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(
        saved_again["tracks"][0]["clips"][0]["badges"],
        json!([{"of": {"tag": 0}, "effects_before": 2}]),
        "after the text effect and the hue shift, as it was after the hue shift"
    );

    // One that was before the first effect (`effects_before` left out) comes after the text effect, and before the hue shift.
    file["tracks"][0]["clips"][0]["badges"] = json!([{"of": {"tag": 0}}]);
    let loaded = project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap();
    let saved_again: Value = serde_json::from_str(&saved(&loaded)).unwrap();
    assert_eq!(
        saved_again["tracks"][0]["clips"][0]["badges"],
        json!([{"of": {"tag": 0}, "effects_before": 1}]),
        "after the text effect (which comes first), before the hue shift"
    );
}

#[test]
fn an_old_text_that_has_a_bad_colour_or_no_words_is_not_a_project_file_and_an_old_one_that_names_a_missing_font_size_gets_a_new_text_size()
 {
    // The fields that the old files had are read as they were: a colour that is not one is a file that cannot be read, as it was.
    let text = the_old_file_of_everything().replace("#FFC80080", "orange");
    match project_from_json_text(&text).unwrap_err() {
        ProjectFileError::NotAProjectFile { reason } => assert!(reason.contains("#RRGGBBAA"), "{reason}"),
        other => panic!("unexpected error {other:?}"),
    }
    // A text that says nothing of what it says (and has no content) is refused with the clip named.
    let mut file: Value = serde_json::from_str(&the_old_file_of_everything()).unwrap();
    file["tracks"][0]["clips"][0].as_object_mut().unwrap().remove("text");
    match project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => {
            assert!(reason.contains("clip number 1 of track number 1"), "{reason}");
            assert!(reason.contains("what it says"), "{reason}");
        }
        other => panic!("unexpected error {other:?}"),
    }
    // One that has no size is the size of a new text, and one that has no font or colour is the font and colour of a new text.
    let mut file: Value = serde_json::from_str(&the_old_file_of_everything()).unwrap();
    for field in ["font_size_fraction_of_frame_height", "font_family", "colour"] {
        file["tracks"][0]["clips"][0].as_object_mut().unwrap().remove(field);
    }
    let loaded = project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap();
    let caption = loaded.text_object_of(loaded.tracks()[0].entries()[0].id()).unwrap();
    let new = TextObject::default();
    assert_eq!(
        (caption.font_size, caption.font_family, caption.colour),
        (new.font_size, new.font_family, new.colour)
    );
}

#[test]
fn a_new_file_whose_text_names_a_content_that_it_does_not_have_or_that_is_not_a_text_effect_is_refused() {
    let mut file: Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    file["tracks"][0]["clips"][0]["content"] = json!(7);
    match project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()).unwrap_err() {
        ProjectFileError::InvalidProject { reason } => {
            assert!(reason.contains("clip number 1 of track number 1"), "{reason}");
            assert!(reason.contains("not a text effect"), "{reason}");
        }
        other => panic!("unexpected error {other:?}"),
    }
    // The hue shift is an effect that the clip has, and is not a text effect.
    let mut file: Value = serde_json::from_str(&saved(&project_with_everything())).unwrap();
    file["tracks"][0]["clips"][0]["content"] = json!(1);
    assert!(matches!(
        project_from_json_text(&serde_json::to_string_pretty(&file).unwrap()),
        Err(ProjectFileError::InvalidProject { .. })
    ));
}
