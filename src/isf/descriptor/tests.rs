// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an ISF file is to the program: its kind of effect, its values, and the numbers of its values in a call.

use super::*;
use crate::isf::read;

const COLOR_CONTROLS: &str = include_str!("../fixtures/color_controls.fs");
const PLASMA: &str = include_str!("../fixtures/plasma.fs");
const WIPE: &str = include_str!("../fixtures/wipe_transition.fs");
const BLUR: &str = include_str!("../fixtures/gaussian_blur.fs");
const OLD_STYLE: &str = include_str!("../fixtures/old_style.fs");
const POSTERIZE: &str = include_str!("../fixtures/posterize.fs");
const VIGNETTE: &str = include_str!("../fixtures/vignette.fs");
const KALEIDOSCOPE: &str = include_str!("../fixtures/kaleidoscope.fs");
const BAD_SYNTAX: &str = include_str!("../fixtures/bad_syntax.fs");

fn descriptor_from(file_name: &str, source: &str) -> EffectDescriptor {
    descriptor_of(&read(source).unwrap(), file_name, source)
}

#[test]
fn the_key_is_isf_the_name_and_the_first_characters_of_the_digest_of_the_text() {
    let key = key_of("color_controls.fs", COLOR_CONTROLS);

    let (name, digest) = key.strip_prefix("isf:").unwrap().rsplit_once('-').unwrap();
    assert_eq!(name, "color_controls");
    assert_eq!(
        digest.len(),
        12,
        "twelve characters of the digest, which a file of a project has in the keys it names"
    );
    assert_eq!(digest.len(), CHARACTERS_OF_THE_DIGEST_IN_A_KEY);
    assert!(
        digest
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    );
    let whole = Sha256Digest::of_bytes(COLOR_CONTROLS.as_bytes()).to_hex();
    assert!(whole.starts_with(digest));
}

#[test]
fn one_text_is_one_key_wherever_it_is_read_and_two_texts_of_one_name_are_two_keys() {
    assert_eq!(key_of("a.fs", PLASMA), key_of("a.fs", PLASMA));
    assert_ne!(
        key_of("a.fs", PLASMA),
        key_of("a.fs", &format!("{PLASMA} ")),
        "one more character"
    );
    assert_ne!(key_of("a.fs", PLASMA), key_of("b.fs", PLASMA), "another name");
    assert_eq!(
        key_of("a.fs", PLASMA),
        key_of("a.isf", PLASMA),
        "the ending of the name is not part of it"
    );
}

#[test]
fn what_a_name_that_is_odd_is_made_of() {
    let name = |file_name: &str| {
        let key = key_of(file_name, "x");
        key.strip_prefix("isf:").unwrap().rsplit_once('-').unwrap().0.to_owned()
    };
    assert_eq!(name("My Cool Shader.fs"), "my_cool_shader");
    assert_eq!(name("Wipe-Left_to right.isf"), "wipe_left_to_right");
    assert_eq!(name(""), "shader");
    assert_eq!(name("...fs"), "shader");
    assert_eq!(name("übung.fs"), "bung", "letters that are not ASCII are not in a key");
    assert_eq!(name("noending"), "noending");
}

#[test]
fn a_filter_is_a_picture_effect_with_a_value_for_each_input_that_is_not_an_image() {
    let descriptor = descriptor_from("Color Controls.fs", COLOR_CONTROLS);

    assert_eq!(descriptor.problem(), None);
    assert_eq!(descriptor.name, "color controls");
    assert_eq!(descriptor.category, "Color Adjustment");
    assert!(descriptor.description.starts_with("brightness, contrast"));
    assert_eq!(descriptor.domain, EffectDomain::Picture);
    assert!(
        !descriptor.changes_each_pixel_by_itself,
        "a shader can do what it likes to opacity"
    );
    let ids: Vec<&str> = descriptor.parameters.iter().map(|parameter| &*parameter.id).collect();
    assert_eq!(ids, ["brightness", "contrast", "saturation", "invert", "tint"]);

    let brightness = &descriptor.parameters[0];
    assert_eq!(brightness.default, PropertyValue::Number(0.0));
    assert_eq!(brightness.range, Some((-1.0, 1.0)));
    assert_eq!(brightness.control, ParameterControl::Slider);
    assert_eq!(brightness.label, "brightness");
    let invert = &descriptor.parameters[3];
    assert_eq!(
        (&invert.default, &invert.control),
        (&PropertyValue::Flag(false), &ParameterControl::Toggle)
    );
    assert_eq!(
        descriptor.parameters[4].default,
        PropertyValue::Colour(TextColour::new(255, 255, 255, 255))
    );
}

#[test]
fn a_choice_has_an_option_for_each_value_called_by_the_value_and_begins_as_its_default() {
    let descriptor = descriptor_from("posterize.fs", POSTERIZE);

    assert_eq!(descriptor.problem(), None);
    let levels = &descriptor.parameters[0];
    let ParameterControl::Choice(options) = &levels.control else {
        panic!("a choice was expected: {:?}", levels.control);
    };
    let found: Vec<(&str, &str)> = options.iter().map(|option| (&*option.id, &*option.label)).collect();
    assert_eq!(found, [("2", "2"), ("4", "4"), ("8", "8"), ("16", "16")]);
    assert_eq!(levels.default, PropertyValue::Words("4".into()));
    assert_eq!(levels.value_it_begins_as(), PropertyValue::Words("4".into()));
    assert_eq!(descriptor.parameters[1].default, PropertyValue::Flag(true));
}

#[test]
fn a_point_is_a_pair_and_a_generator_has_the_values_of_its_inputs_and_no_picture() {
    let vignette = descriptor_from("vignette.fs", VIGNETTE);
    // (In pixels of a picture 1080 high, as ISF has a point; the file of the spike had it as a fraction, and was changed on purpose in phase 6.)
    assert_eq!(vignette.parameters[0].default, PropertyValue::Pair(960.0, 540.0));
    assert_eq!(vignette.parameters[0].range, None);

    let plasma = descriptor_from("plasma.fs", PLASMA);
    assert_eq!(plasma.parameters.len(), 4);
    assert_eq!(
        plasma.category, "Shader",
        "a file with no categories is in the heading that says what it is"
    );
}

#[test]
fn an_input_that_is_a_button_is_a_switch_that_is_off_and_sound_is_no_value() {
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"go\", \"LABEL\": \"Go!\", \"TYPE\": \"event\" }, { \"NAME\": \"sound\", \"TYPE\": \"audio\" } ] }*/\nvoid main() {}\n";
    let descriptor = descriptor_from("b.fs", source);
    assert_eq!(descriptor.parameters.len(), 1);
    assert_eq!(descriptor.parameters[0].default, PropertyValue::Flag(false));
    assert_eq!(descriptor.parameters[0].label, "Go!");
    assert_eq!(descriptor.parameters[0].name, "go!");
}

#[test]
fn every_file_of_the_spike_is_a_descriptor_the_program_can_rely_on() {
    for (name, source) in [
        ("color_controls.fs", COLOR_CONTROLS),
        ("plasma.fs", PLASMA),
        ("wipe_transition.fs", WIPE),
        ("gaussian_blur.fs", BLUR),
        ("old_style.fs", OLD_STYLE),
        ("posterize.fs", POSTERIZE),
        ("vignette.fs", VIGNETTE),
        ("kaleidoscope.fs", KALEIDOSCOPE),
        ("bad_syntax.fs", BAD_SYNTAX),
    ] {
        let descriptor = descriptor_from(name, source);
        assert_eq!(descriptor.problem(), None, "{name}");
        // The kind that the program makes of it is an interned one, which it can be given.
        assert!(
            crate::project::EffectKind::from_snapshot(descriptor.clone()).is_ok(),
            "{name}"
        );
    }
}

#[test]
fn the_values_a_kind_begins_with_are_as_many_numbers_as_the_inputs_take() {
    for source in [
        COLOR_CONTROLS,
        PLASMA,
        WIPE,
        BLUR,
        OLD_STYLE,
        POSTERIZE,
        VIGNETTE,
        KALEIDOSCOPE,
    ] {
        let file = read(source).unwrap();
        let kind = crate::project::EffectKind::from_snapshot(descriptor_of(&file, "x.fs", source)).unwrap();
        assert_eq!(
            numbers_of_the_values(&kind.default_values()).len(),
            file.numbers_of_the_inputs()
        );
    }
}

#[test]
fn the_defaults_of_a_filter_are_these_numbers_in_the_order_of_its_inputs() {
    let file = read(COLOR_CONTROLS).unwrap();
    let kind = crate::project::EffectKind::from_snapshot(descriptor_of(&file, "c.fs", COLOR_CONTROLS)).unwrap();

    assert_eq!(
        numbers_of_the_values(&kind.default_values()),
        [0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        "brightness, contrast, saturation, invert, and the four numbers of the tint"
    );
}

#[test]
fn each_kind_of_value_is_the_numbers_it_is() {
    let numbers = numbers_of_the_values(&[
        PropertyValue::Number(-2.5),
        PropertyValue::Pair(10.0, 20.0),
        PropertyValue::Colour(TextColour::new(255, 0, 51, 102)),
        PropertyValue::Flag(true),
        PropertyValue::Flag(false),
        PropertyValue::Words("16".into()),
        PropertyValue::Words(" -3 ".into()),
        PropertyValue::Words("not a number".into()),
    ]);

    assert_eq!(
        numbers,
        [-2.5, 10.0, 20.0, 1.0, 0.0, 0.2, 0.4, 1.0, 0.0, 16.0, -3.0, 0.0]
    );
    assert_eq!(numbers_of_the_values(&[]), Vec::<f32>::new());
}

#[test]
fn the_colour_of_a_default_is_rounded_to_the_nearest_of_the_256_and_not_cut() {
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"c\", \"TYPE\": \"color\", \"DEFAULT\": [0.9, 0.5, 0.1, 1.0] } ] }*/
void main() {}
";

    let descriptor = descriptor_from("c.fs", source);

    // 229.5, 127.5 and 25.5 are the nearest to 230, 128 and 26: cut off they would be 229, 127 and 25.
    assert_eq!(
        descriptor.parameters[0].default,
        PropertyValue::Colour(TextColour::new(230, 128, 26, 255))
    );
    // And one that is out of the 0 to 1 is the least or the most there is.
    let outside = "/*{ \"INPUTS\": [ { \"NAME\": \"c\", \"TYPE\": \"color\", \"DEFAULT\": [-1.0, 2.0, 0.0, 1.0] } ] }*/
void main() {}
";
    assert_eq!(
        descriptor_from("c.fs", outside).parameters[0].default,
        PropertyValue::Colour(TextColour::new(0, 255, 0, 255))
    );
}

#[test]
fn a_key_is_of_the_text_it_was_made_from_and_of_no_other() {
    let key = key_of("x.fs", PLASMA);
    assert!(key_is_of_the_text(&key, PLASMA));
    assert!(!key_is_of_the_text(&key, &format!("{PLASMA} ")));
    assert!(!key_is_of_the_text("isf:no_digest", PLASMA));
    assert!(!key_is_of_the_text("", PLASMA));
}

#[test]
fn the_options_of_a_choice_are_called_by_their_values_in_a_file_and_by_their_labels_to_a_person() {
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"size\", \"TYPE\": \"long\", \"VALUES\": [5, 6], \"LABELS\": [\"small\", \"big\"], \"DEFAULT\": 6 } ] }*/\nvoid main() {}\n";

    let descriptor = descriptor_from("s.fs", source);

    let ParameterControl::Choice(options) = &descriptor.parameters[0].control else {
        panic!("a choice was expected");
    };
    let found: Vec<(&str, &str)> = options.iter().map(|option| (&*option.id, &*option.label)).collect();
    assert_eq!(found, [("5", "small"), ("6", "big")]);
    assert_eq!(descriptor.parameters[0].default, PropertyValue::Words("6".into()));
    assert_eq!(descriptor.problem(), None);
}

#[test]
fn the_images_a_shader_brings_are_a_part_of_its_key_and_a_shader_with_none_has_the_key_it_always_had() {
    use crate::project::ShaderImage;
    let first = ShaderImage::new("tex", vec![1, 2, 3]);
    let other_bytes = ShaderImage::new("tex", vec![1, 2, 4]);
    let other_name = ShaderImage::new("map", vec![1, 2, 3]);
    assert_eq!(
        key_of_a_source_with_images("a.fs", PLASMA, &[]),
        key_of("a.fs", PLASMA),
        "none: as it was"
    );
    let key = key_of_a_source_with_images("a.fs", PLASMA, std::slice::from_ref(&first));
    assert_ne!(key, key_of("a.fs", PLASMA));
    assert_ne!(key, key_of_a_source_with_images("a.fs", PLASMA, &[other_bytes]));
    assert_ne!(key, key_of_a_source_with_images("a.fs", PLASMA, &[other_name]));
    assert_ne!(
        key_of_a_source_with_images("a.fs", PLASMA, &[first.clone(), first.clone()]),
        key,
        "two images are not one"
    );
    assert_eq!(
        key,
        key_of_a_source_with_images("a.fs", PLASMA, std::slice::from_ref(&first))
    );
    assert!(key_is_of_the_source(&key, PLASMA, std::slice::from_ref(&first)));
    assert!(!key_is_of_the_source(&key, PLASMA, &[]));
    assert!(!key_is_of_the_source(&key, "other text", &[first]));
}

#[test]
fn a_descriptor_made_with_images_carries_them_in_its_shader() {
    use crate::project::ShaderImage;
    let images = vec![ShaderImage::new("tex", vec![7, 7, 7])];
    let descriptor = descriptor_with_images_of(&read(PLASMA).unwrap(), "a.fs", PLASMA, images.clone());
    assert_eq!(descriptor.shader.as_ref().unwrap().images, images);
    assert!(key_is_of_the_source(&descriptor.key, PLASMA, &images));
    assert!(descriptor_from("a.fs", PLASMA).shader.unwrap().images.is_empty());
}
