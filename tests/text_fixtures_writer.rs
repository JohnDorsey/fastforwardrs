// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! THROWAWAY (phase 9.0): writes the project files in `tests/data/text/` that `text_objects_as_they_are` looks at, **with the program as it was before phase 9**.
//! It is deleted in the commit that has them (it names `PropertyId::Text` and the other text properties, which phase 9 takes away); the files are what is kept.
//!
//! ```text
//! cargo test --test text_fixtures_writer -- --ignored
//! ```

use std::path::{Path, PathBuf};

use fastforwardrs::project::{
    Angle, AnimationTarget, CanvasArea, ClipId, EffectKind, FontSize, GroupKind, Member, Project, PropertyId,
    PropertyValue, RackDestination, TextColour, TextObject, TimelineFrameIndex, TrackId,
};
use fastforwardrs::project_file::project_to_json_text;

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("text")
}

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn keyframe(project: &mut Project, clip: ClipId, property: PropertyId, at: u64, value: PropertyValue) {
    let target = AnimationTarget::Clip(clip);
    project.add_keyframe(target, property, frame(at)).unwrap();
    project.set_property_at(target, property, value, frame(at)).unwrap();
}

fn write(name: &str, project: &Project) {
    std::fs::create_dir_all(folder()).unwrap();
    let text = project_to_json_text(project, Path::new(".")).expect("the project can be written");
    std::fs::write(folder().join(format!("{name}.json")), text).unwrap();
}

fn a_bigger_text() -> TextObject {
    TextObject {
        text: "Hello,\nworld".to_owned(),
        font_family: "Arial".to_owned(),
        font_size: FontSize::from_points(40.0),
        colour: TextColour::new(255, 200, 0, 128),
        area: CanvasArea::new(0.1, 0.2, 0.5, 0.3),
        rotation: Angle::from_degrees(15.0),
    }
}

#[test]
#[ignore = "writes the files of tests/data/text"]
fn write_the_text_fixtures() {
    // 1. Two texts, one after the other on a track: a new one, and one that has been given every setting.
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    project.insert_text(track, frame(0), TextObject::default(), 60).unwrap();
    project.insert_text(track, frame(60), a_bigger_text(), 40).unwrap();
    write("plain", &project);

    // 2. A text whose words, font, size and colour change (a word is kept until the next keyframe; the others change evenly), and whose place and turning change.
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    let (clip, _) = project.insert_text(track, frame(10), a_bigger_text(), 80).unwrap();
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        10,
        PropertyValue::Words("One".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        40,
        PropertyValue::Words("Two\nlines".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        70,
        PropertyValue::Words("Three".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::FontFamily,
        10,
        PropertyValue::Words(String::new()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::FontFamily,
        55,
        PropertyValue::Words("Arial".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::FontSize,
        10,
        PropertyValue::Number(0.06),
    );
    keyframe(&mut project, clip, PropertyId::FontSize, 89, PropertyValue::Number(0.2));
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        10,
        PropertyValue::Colour(TextColour::WHITE),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        50,
        PropertyValue::Colour(TextColour::new(255, 0, 0, 255)),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        89,
        PropertyValue::Colour(TextColour::new(0, 0, 255, 64)),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        10,
        PropertyValue::Pair(0.3, 0.4),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        89,
        PropertyValue::Pair(0.7, 0.6),
    );
    keyframe(&mut project, clip, PropertyId::Size, 10, PropertyValue::Pair(0.5, 0.3));
    keyframe(&mut project, clip, PropertyId::Size, 89, PropertyValue::Pair(0.8, 0.5));
    keyframe(
        &mut project,
        clip,
        PropertyId::Rotation,
        10,
        PropertyValue::Number(15.0),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Rotation,
        89,
        PropertyValue::Number(-40.0),
    );
    write("animated", &project);

    // 3. A text with effects of the program on it: the first with a value that changes, and the others as they are put on.
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    let (clip, _) = project.insert_text(track, frame(0), a_bigger_text(), 60).unwrap();
    let target = AnimationTarget::Clip(clip);
    let hue = project.add_effect(target, EffectKind::HUE_SHIFT).unwrap();
    let hue_value = PropertyId::effect_value(EffectKind::HUE_SHIFT, hue, 0);
    project.add_keyframe(target, hue_value, frame(0)).unwrap();
    project
        .set_property_at(target, hue_value, PropertyValue::Number(0.0), frame(0))
        .unwrap();
    project.add_keyframe(target, hue_value, frame(59)).unwrap();
    project
        .set_property_at(target, hue_value, PropertyValue::Number(180.0), frame(59))
        .unwrap();
    let saturation = project.add_effect(target, EffectKind::SATURATION).unwrap();
    project
        .set_property_at(
            target,
            PropertyId::effect_value(EffectKind::SATURATION, saturation, 0),
            PropertyValue::Number(0.5),
            frame(0),
        )
        .unwrap();
    let opacity = project.add_effect(target, EffectKind::OPACITY).unwrap();
    project
        .set_property_at(
            target,
            PropertyId::effect_value(EffectKind::OPACITY, opacity, 0),
            PropertyValue::Number(0.6),
            frame(0),
        )
        .unwrap();
    let blur = project.add_effect(target, EffectKind::BLUR).unwrap();
    project
        .set_property_at(
            target,
            PropertyId::effect_value(EffectKind::BLUR, blur, 0),
            PropertyValue::Number(3.0),
            frame(0),
        )
        .unwrap();
    let brightness = project.add_effect(target, EffectKind::BRIGHTNESS_AND_CONTRAST).unwrap();
    for (slot, value) in [0.2, 1.3].into_iter().enumerate() {
        project
            .set_property_at(
                target,
                PropertyId::effect_value(EffectKind::BRIGHTNESS_AND_CONTRAST, brightness, slot as u8),
                PropertyValue::Number(value),
                frame(0),
            )
            .unwrap();
    }
    // (An effect on the track that the text is on: it is done to what the track makes.)
    let exposure = project
        .add_effect(AnimationTarget::Track(track), EffectKind::EXPOSURE)
        .unwrap();
    project
        .set_property_at(
            AnimationTarget::Track(track),
            PropertyId::effect_value(EffectKind::EXPOSURE, exposure, 0),
            PropertyValue::Number(0.5),
            frame(0),
        )
        .unwrap();
    write("with_effects", &project);

    // 4. A text whose keyframes are in racks that are taken out onto crates, and in racks that are merged.
    let mut project = Project::empty();
    let text_track = project.tracks()[0].id();
    let (clip, _) = project.insert_text(text_track, frame(0), a_bigger_text(), 90).unwrap();
    let crate_track: TrackId = project.add_track_in_front();
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        5,
        PropertyValue::Colour(TextColour::WHITE),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        60,
        PropertyValue::Colour(TextColour::new(0, 255, 0, 255)),
    );
    let target = AnimationTarget::Clip(clip);
    let rack = project
        .located_racks()
        .find(|located| located.owner == target && located.rack.property() == PropertyId::Colour)
        .map(|located| located.rack.id())
        .expect("the keyframes are in a rack of the drawer");
    project
        .move_racks(
            &[rack],
            RackDestination::NewCrate {
                track: crate_track,
                from_frame: 5,
                until_frame: 61,
            },
            0,
            None,
        )
        .unwrap();
    // The words in two racks, that are merged.
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        0,
        PropertyValue::Words("First".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        20,
        PropertyValue::Words("Second".to_owned()),
    );
    keyframe(&mut project, clip, PropertyId::FontSize, 30, PropertyValue::Number(0.1));
    keyframe(
        &mut project,
        clip,
        PropertyId::FontSize,
        50,
        PropertyValue::Number(0.15),
    );
    write("racks_and_crates", &project);

    // 5. A text that has been cut in two, with keyframes across the cut.
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    let (clip, _) = project.insert_text(track, frame(0), a_bigger_text(), 90).unwrap();
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        0,
        PropertyValue::Words("Before".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Text,
        45,
        PropertyValue::Words("After".to_owned()),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        0,
        PropertyValue::Colour(TextColour::new(10, 20, 30, 255)),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Colour,
        89,
        PropertyValue::Colour(TextColour::new(250, 240, 230, 255)),
    );
    project.split_at(frame(30), None);
    write("cut_in_two", &project);

    // 6. Texts in groups: a tag, a clip group and a track group, with the first with an effect.
    let mut project = Project::empty();
    let first_track = project.tracks()[0].id();
    let second_track = project.add_track_in_front();
    let (first, _) = project
        .insert_text(first_track, frame(0), TextObject::default(), 50)
        .unwrap();
    let (second, _) = project
        .insert_text(second_track, frame(10), a_bigger_text(), 50)
        .unwrap();
    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let hue = project.add_effect_to_the_group(tag, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(
            tag,
            PropertyId::effect_value(EffectKind::HUE_SHIFT, hue, 0),
            &PropertyValue::Number(90.0),
        )
        .unwrap();
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let saturation = project
        .add_effect_to_the_group(clip_group, EffectKind::SATURATION)
        .unwrap();
    project
        .set_effect_value_of_the_group(
            clip_group,
            PropertyId::effect_value(EffectKind::SATURATION, saturation, 0),
            &PropertyValue::Number(0.3),
        )
        .unwrap();
    let track_group = project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Track(first_track), Member::Track(second_track)],
        )
        .unwrap();
    let blur = project.add_effect_to_the_group(track_group, EffectKind::BLUR).unwrap();
    project
        .set_effect_value_of_the_group(
            track_group,
            PropertyId::effect_value(EffectKind::BLUR, blur, 0),
            &PropertyValue::Number(2.0),
        )
        .unwrap();
    write("in_groups", &project);
}
