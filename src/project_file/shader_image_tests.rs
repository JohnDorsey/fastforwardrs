// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The images that a shader brings with it, in a project file (phase 6.4.4; format 17): the snapshot of such a shader has them as names and base 64, a project is read back with the
//! same images and the same kind of effect, a file of version 16 has none, and what is wrong with them is refused with where it is.

use serde_json::{Value, json};

use super::tests::{assert_same_content, edited_project, link_to_missing_video, saved, version_line};
use super::*;
use crate::project::{EffectKind, MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER, MOST_IMAGES_A_SHADER_BRINGS, ShaderImage};

const WITH_AN_IMAGE: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"IMPORTED\": { \"tex\": { \"PATH\": \"t.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage) * IMG_THIS_PIXEL(tex); }\n";

fn a_png() -> Vec<u8> {
    crate::rgba_picture::RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 1,
        rgba_pixels: vec![10, 20, 30, 255, 40, 50, 60, 255],
    }
    .to_png_bytes()
    .unwrap()
}

fn kind_with_an_image(bytes: Vec<u8>) -> EffectKind {
    let file = crate::isf::read(WITH_AN_IMAGE).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_with_images_of(
        &file,
        "textured.fs",
        WITH_AN_IMAGE,
        vec![ShaderImage::new("tex", bytes)],
    ))
    .unwrap()
}

fn a_project_with_a_shader_that_brings_an_image() -> (Project, EffectKind) {
    let mut project = edited_project(link_to_missing_video());
    let kind = kind_with_an_image(a_png());
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    (project, kind)
}

fn the_shader_of(document: &mut Value) -> &mut Value {
    &mut document["plugins"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|snapshot| {
            snapshot["key"]
                .as_str()
                .is_some_and(|key| key.starts_with("isf:textured-"))
        })
        .expect("the snapshot is in the file")["shader"]
}

#[test]
fn the_snapshot_of_a_shader_that_brings_an_image_has_the_name_and_the_bytes_in_base_64() {
    let (project, _) = a_project_with_a_shader_that_brings_an_image();

    let text = saved(&project);

    let mut document: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        *the_shader_of(&mut document),
        json!({
            "language": "isf",
            "source": WITH_AN_IMAGE,
            "images": [ { "name": "tex", "data": crate::base64::encode(&a_png()) } ],
        })
    );
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const { assert!(FILE_FORMAT_VERSION >= 17, "the images of a shader came with format 17") };
}

#[test]
fn a_project_with_a_shader_that_brings_an_image_is_read_back_with_the_image_and_the_same_kind_of_effect() {
    let (project, kind) = a_project_with_a_shader_that_brings_an_image();

    let loaded = project_from_json_text(&saved(&project)).unwrap();

    assert_same_content(&loaded, &project);
    let read_back = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .next()
        .unwrap()
        .kind();
    assert_eq!(
        read_back.key(),
        kind.key(),
        "the same kind: its key is made of the images too"
    );
    let images = &read_back.shader_source().unwrap().images;
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].name(), &**images[0].bytes()), ("tex", a_png().as_slice()));
    assert!(read_back.is_installed(), "and it is run: the image is a picture");
    assert_eq!(
        images[0].pixels().unwrap().rgba_pixels,
        vec![10, 20, 30, 255, 40, 50, 60, 255]
    );
}

#[test]
fn a_shader_that_brings_no_image_has_no_images_in_its_snapshot_and_a_file_of_version_16_reads_as_it_was() {
    let mut project = edited_project(link_to_missing_video());
    let file = crate::isf::read(WITH_AN_IMAGE).unwrap();
    let plain = EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "textured.fs", WITH_AN_IMAGE)).unwrap();
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    project.add_effect(AnimationTarget::Clip(clip), plain).unwrap();

    let text = saved(&project);

    let mut document: Value = serde_json::from_str(&text).unwrap();
    assert!(the_shader_of(&mut document).get("images").is_none());
    assert!(
        !plain.is_installed(),
        "(it needs the image, and was given none: it is kept, and does nothing)"
    );
    let sixteen = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(16));
    let loaded = project_from_json_text(&sixteen).unwrap();
    assert_same_content(&loaded, &project);
}

/// The project as a document, with `change` done to the `shader` of the snapshot of the shader that brings an image, read as a project.
fn read_with(change: impl FnOnce(&mut Value)) -> Result<Project, String> {
    let (project, _) = a_project_with_a_shader_that_brings_an_image();
    let mut document: Value = serde_json::from_str(&saved(&project)).unwrap();
    change(the_shader_of(&mut document));
    project_from_json_text(&document.to_string()).map_err(|error| error.to_string())
}

#[test]
fn images_that_are_not_what_they_should_be_refuse_the_file_and_say_which_effect_and_which_image() {
    let refused = |change: &dyn Fn(&mut Value)| read_with(|shader| change(shader)).map(|_| ()).unwrap_err();
    let said = refused(&|shader| shader["images"] = json!("not a list"));
    assert!(
        said.contains("the images of its shader")
            && said.contains("it is not a list")
            && said.contains("isf:textured-"),
        "{said}"
    );
    let said = refused(&|shader| shader["images"] = json!([42]));
    assert!(said.contains("image 1 is not a name and some data"), "{said}");
    let said = refused(&|shader| shader["images"] = json!([{ "name": "tex" }]));
    assert!(said.contains("image 1 is not a name and some data"), "{said}");
    let said = refused(&|shader| shader["images"] = json!([{ "name": "tex", "data": "AAAA", "more": 1 }]));
    assert!(said.contains("image 1 is not a name and some data"), "{said}");
    let said = refused(&|shader| shader["images"] = json!([{ "name": "tex", "data": 5 }]));
    assert!(said.contains("image 1 is not a name and some data"), "{said}");
    let said = refused(&|shader| shader["images"] = json!([{ "name": "tex", "data": "not base 64!" }]));
    assert!(said.contains("image 1:"), "{said}");
    let said = refused(&|shader| {
        shader["images"] = Value::Array(
            (0..=MOST_IMAGES_A_SHADER_BRINGS)
                .map(|_| json!({ "name": "tex", "data": "AAAA" }))
                .collect(),
        )
    });
    assert!(
        said.contains(&format!("{} are the most", MOST_IMAGES_A_SHADER_BRINGS)),
        "{said}"
    );
    let said = refused(&|shader| {
        shader["images"] =
            json!([{ "name": "tex", "data": "A".repeat(MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER.div_ceil(3) * 4 + 4) }])
    });
    assert!(said.contains("image 1 has more than"), "{said}");
}

#[test]
fn an_image_of_exactly_the_most_bytes_is_read_and_one_byte_more_is_not() {
    let at_most = crate::base64::encode(&vec![0u8; MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER]);
    let project = read_with(|shader| shader["images"] = json!([{ "name": "tex", "data": at_most }]));
    assert!(project.is_ok(), "{:?}", project.err());
    let over = crate::base64::encode(&vec![0u8; MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER + 1]);
    let said = read_with(|shader| shader["images"] = json!([{ "name": "tex", "data": over }]))
        .map(|_| ())
        .unwrap_err();
    assert!(said.contains("image 1 has more than"), "{said}");
}

#[test]
fn bytes_that_are_not_a_picture_are_read_and_the_effect_is_kept_and_does_nothing() {
    let loaded = read_with(|shader| {
        shader["images"] = json!([{ "name": "tex", "data": crate::base64::encode(b"not a picture") }])
    })
    .unwrap();
    let kind = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .next()
        .unwrap()
        .kind();
    assert!(!kind.is_installed());
    // (The key was made from the images the file had, and the file now has others: the project says why in the box of the effect.)
    assert!(crate::isf::why_not_usable(kind.descriptor()).is_some());
}

#[test]
fn as_many_images_as_a_shader_may_bring_are_read_and_one_more_is_not() {
    let images = |count: usize| Value::Array((0..count).map(|_| json!({ "name": "tex", "data": "AAAA" })).collect());
    let project = read_with(|shader| shader["images"] = images(MOST_IMAGES_A_SHADER_BRINGS));
    assert!(project.is_ok(), "{:?}", project.err());
    let said = read_with(|shader| shader["images"] = images(MOST_IMAGES_A_SHADER_BRINGS + 1))
        .map(|_| ())
        .unwrap_err();
    assert!(said.contains("are the most"), "{said}");
}
