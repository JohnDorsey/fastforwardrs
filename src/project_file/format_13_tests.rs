// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 13 of the project file added (phase 4, piece 4.1): the category of a type of effect, the scale and the control of a value, and the flag of an effect that is
//! switched off; and that a file of version 12, which has none of them, is read as it was.

use serde_json::{Value, json};

use super::tests::saved;
use super::*;
use crate::project::{AnimationTarget, EffectInstanceId, Member, ParameterControl, ValueScale};

/// A project file as the program wrote it when its version was 12, kept as it was (MG-03): a video of three clips, a hue shift on the first of them, turned 45 degrees. Its
/// snapshot of the hue shift is of version 1.0 and says nothing of a category.
const A_FILE_OF_VERSION_12: &str = r#"{
  "file_format": "fastforwardrs-project",
  "file_format_version": 12,
  "sources": [
    {
      "kind": "video",
      "identifier": 1,
      "absolute_path": "/videos/holiday.mp4",
      "relative_path": "../videos/holiday.mp4",
      "frame_rate": "30000/1001",
      "total_frame_count": 900,
      "display_width_in_pixels": 1920,
      "display_height_in_pixels": 1080,
      "has_audio": true
    }
  ],
  "plugins": [
    {
      "key": "builtin:hue_shift",
      "version": [1, 0],
      "name": "hue shift",
      "description": "Turns the colours round the wheel of colours",
      "domain": "picture",
      "changes_each_pixel_by_itself": true,
      "params": [
        {
          "id": "degrees",
          "name": "hue shift",
          "label": "Turn",
          "type": "number",
          "default": 0.0,
          "range": [-180.0, 180.0],
          "unit": "°",
          "decimals": 1
        }
      ]
    }
  ],
  "tracks": [
    {
      "title": "track 0",
      "gain_decibels": 0.0,
      "speed_factor": 1,
      "muted": false,
      "clips": [
        {
          "kind": "video",
          "source_identifier": 1,
          "first_source_frame": 0,
          "end_source_frame": 300,
          "speed_factor": 1,
          "effects": [
            {"effect": "builtin:hue_shift", "version": [1, 0], "instance": 0, "values": {"degrees": 45.0}}
          ]
        },
        {
          "kind": "video",
          "source_identifier": 1,
          "first_source_frame": 300,
          "end_source_frame": 600,
          "speed_factor": 8
        },
        {
          "kind": "video",
          "source_identifier": 1,
          "first_source_frame": 600,
          "end_source_frame": 900,
          "speed_factor": 1
        }
      ]
    }
  ],
  "speed_indicator": {
    "left_fraction_of_frame_width": 0.5,
    "top_fraction_of_frame_height": 0.25,
    "height_fraction_of_frame_height": 0.125
  }
}
"#;

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn this_build_writes_a_version_that_has_what_version_13_added() {
    // (Rewritten on purpose in phase 7: this build writes 14, which has everything that 13 added and the plugin libraries.)
    const { assert!(FILE_FORMAT_VERSION >= 13) };
    assert!(
        saved(&project_from_json_text(A_FILE_OF_VERSION_12).unwrap())
            .contains(&format!("\"file_format_version\": {FILE_FORMAT_VERSION}"))
    );
}

#[test]
fn a_file_of_version_12_loads_with_its_effect_switched_on_and_installed_and_with_what_it_had() {
    let project = project_from_json_text(A_FILE_OF_VERSION_12).expect("a file of version 12 loads");

    let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let effect = project.effects_of(clip).unwrap().iter().next().unwrap();
    assert_eq!(
        effect.kind(),
        EffectKind::HUE_SHIFT,
        "its snapshot of version 1.0 is the hue shift of the program"
    );
    assert!(effect.is_enabled());
    assert_eq!(effect.values(), [PropertyValue::Number(45.0)]);
}

#[test]
fn a_file_of_version_12_that_is_loaded_and_saved_is_written_as_the_program_has_the_hue_shift_now_and_nothing_else_changes()
 {
    let project = project_from_json_text(A_FILE_OF_VERSION_12).unwrap();

    let again = as_json(&saved(&project));

    let mut expected = as_json(A_FILE_OF_VERSION_12);
    expected["file_format_version"] = json!(FILE_FORMAT_VERSION);
    expected["plugins"][0]["version"] = json!([1, 1]);
    expected["plugins"][0]["category"] = json!("Colour");
    expected["tracks"][0]["clips"][0]["effects"][0]["version"] = json!([1, 1]);
    assert_eq!(again, expected);
}

#[test]
fn an_effect_that_is_switched_off_says_so_in_its_file_and_one_that_is_on_says_nothing_of_it() {
    let mut project = project_from_json_text(A_FILE_OF_VERSION_12).unwrap();
    let clip = project.tracks()[0].entries()[0].id();
    let on = as_json(&saved(&project));
    assert!(
        on["tracks"][0]["clips"][0]["effects"][0].get("enabled").is_none(),
        "{on}"
    );

    project
        .set_effect_enabled(Member::Clip(clip), EffectInstanceId::FIRST, false)
        .unwrap();
    let off_text = saved(&project);
    let off = as_json(&off_text);

    assert_eq!(off["tracks"][0]["clips"][0]["effects"][0]["enabled"], json!(false));
    let loaded = project_from_json_text(&off_text).unwrap();
    let effect = loaded
        .effects_of(AnimationTarget::Clip(loaded.tracks()[0].entries()[0].id()))
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert!(!effect.is_enabled());
    assert_eq!(
        effect.values(),
        [PropertyValue::Number(45.0)],
        "and its values are kept"
    );
    assert_eq!(as_json(&saved(&loaded)), off, "written as it was read");
}

/// The snapshot of an effect that the program does not have, whose values say every kind of control and a scale.
fn a_snapshot_with_every_control() -> Value {
    json!({
        "key": "isf:every_control",
        "version": [1, 0],
        "name": "every control",
        "description": "Made up",
        "category": "Looks",
        "domain": "picture",
        "params": [
            {"id": "radius", "name": "radius", "label": "Radius", "type": "number", "default": 1.0, "range": [0.1, 100.0], "scale": "logarithmic", "control": "slider"},
            {"id": "steps", "name": "steps", "label": "Steps", "type": "number", "default": 4.0, "range": [1.0, 16.0], "control": "integer"},
            {"id": "mirror", "name": "mirror", "label": "Mirror", "type": "flag", "default": false, "control": "toggle"},
            {"id": "mode", "name": "mode", "label": "Mode", "type": "words", "default": "soft",
             "control": {"choice": [{"id": "soft", "label": "Soft"}, {"id": "hard", "label": "Hard"}]}},
            {"id": "table", "name": "table", "label": "Table", "type": "words", "default": "", "control": {"resource": "cube"}}
        ]
    })
}

fn the_file_with(snapshot: Value) -> String {
    let mut file = as_json(A_FILE_OF_VERSION_12);
    file["file_format_version"] = json!(13);
    file["plugins"] = json!([snapshot.clone()]);
    file["tracks"][0]["clips"][0]["effects"] =
        json!([{"effect": "isf:every_control", "version": [1, 0], "instance": 0}]);
    serde_json::to_string_pretty(&file).unwrap()
}

#[test]
fn the_category_the_scale_and_the_control_of_every_value_are_read_from_a_snapshot_and_written_back_as_they_were() {
    let original = the_file_with(a_snapshot_with_every_control());

    let project = project_from_json_text(&original).unwrap();

    let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let kind = project.effects_of(clip).unwrap().iter().next().unwrap().kind();
    let descriptor = kind.descriptor();
    assert_eq!(descriptor.category, "Looks");
    let controls: Vec<&ParameterControl> = descriptor.parameters.iter().map(|value| &value.control).collect();
    assert_eq!(controls[0], &ParameterControl::Slider);
    assert_eq!(controls[1], &ParameterControl::Integer);
    assert_eq!(controls[2], &ParameterControl::Toggle);
    match controls[3] {
        ParameterControl::Choice(options) => {
            assert_eq!(
                options
                    .iter()
                    .map(|option| (&*option.id, &*option.label))
                    .collect::<Vec<_>>(),
                vec![("soft", "Soft"), ("hard", "Hard")]
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(controls[4], &ParameterControl::Resource("cube".into()));
    assert_eq!(descriptor.parameters[0].scale, ValueScale::Logarithmic);
    assert_eq!(descriptor.parameters[1].scale, ValueScale::Linear);

    // (Rewritten on purpose in phase 7: the file that is written says the version of this build, 14, and nothing else of it changed.)
    let mut expected = as_json(&original);
    expected["file_format_version"] = json!(FILE_FORMAT_VERSION);
    assert_eq!(
        as_json(&saved(&project)),
        expected,
        "nothing is lost, and nothing is added"
    );
}

#[test]
fn a_scale_or_a_control_that_this_version_does_not_know_is_kept_and_written_back_as_it_was() {
    let mut snapshot = a_snapshot_with_every_control();
    snapshot["params"][0]["scale"] = json!({"steps": 12});
    snapshot["params"][1]["control"] = json!({"curve_editor": {"points": 5}});
    snapshot["params"][2]["control"] = json!("a_dial_that_is_not_known_yet");
    let original = the_file_with(snapshot);

    let project = project_from_json_text(&original).expect("a file from a newer program that has more is not refused");

    let clip = AnimationTarget::Clip(project.tracks()[0].entries()[0].id());
    let descriptor = project
        .effects_of(clip)
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .kind()
        .descriptor();
    assert_eq!(
        descriptor.parameters[1].control,
        ParameterControl::Automatic,
        "used as the control that its kind has"
    );
    assert_eq!(descriptor.parameters[0].scale, ValueScale::Linear);
    let mut expected = as_json(&original);
    expected["file_format_version"] = json!(FILE_FORMAT_VERSION);
    assert_eq!(as_json(&saved(&project)), expected);
}

#[test]
fn a_snapshot_whose_controls_do_not_fit_its_values_is_refused_and_the_reason_says_which() {
    type AChangeToTheSnapshot = fn(&mut Value);
    let cases: [(AChangeToTheSnapshot, &str); 3] = [
        (
            |snapshot| snapshot["params"][3]["default"] = json!("not_an_option"),
            "begins as an option that it has not",
        ),
        (
            |snapshot| snapshot["params"][3]["control"] = json!({"choice": []}),
            "has no options",
        ),
        (
            |snapshot| snapshot["params"][0]["range"] = json!([0.0, 10.0]),
            "needs a range that is above 0",
        ),
    ];
    for (change, wanted) in cases {
        let mut snapshot = a_snapshot_with_every_control();
        change(&mut snapshot);
        let reason = match project_from_json_text(&the_file_with(snapshot)) {
            Err(ProjectFileError::InvalidProject { reason }) => reason,
            other => panic!("{wanted}: {other:?}"),
        };
        assert!(reason.contains(wanted), "{wanted}: {reason}");
    }
}
