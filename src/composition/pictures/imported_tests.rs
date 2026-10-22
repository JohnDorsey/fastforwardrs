// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a shader that brings images with it (phase 6.4.4): its operation carries the pictures, in the order the file lists them, the same at every frame, and
//! the pictures of two shaders are not mistaken for each other.

use super::tests::{frame, long_video, plan};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{EffectKind, ShaderImage};

const WITH_TWO_IMAGES: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"IMPORTED\": { \"noise\": { \"PATH\": \"n.png\" }, \"ramp\": { \"PATH\": \"r.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(noise) + IMG_THIS_PIXEL(ramp); }\n";

fn a_png(level: u8) -> Vec<u8> {
    crate::rgba_picture::RgbaPicture {
        width_in_pixels: 3,
        height_in_pixels: 2,
        rgba_pixels: [level, 0, 0, 255].repeat(6),
    }
    .to_png_bytes()
    .unwrap()
}

fn kind_with_images(name: &str, levels: [u8; 2]) -> EffectKind {
    let file = crate::isf::read(WITH_TWO_IMAGES).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_with_images_of(
        &file,
        name,
        WITH_TWO_IMAGES,
        vec![
            ShaderImage::new("noise", a_png(levels[0])),
            ShaderImage::new("ramp", a_png(levels[1])),
        ],
    ))
    .unwrap()
}

fn images_of(operations: &[PictureOperation]) -> Vec<ImportedPicture> {
    match operations {
        [PictureOperation::Shader { images, .. }] => images.clone(),
        other => panic!("one operation of a shader was expected: {other:?}"),
    }
}

#[test]
fn the_operation_of_a_shader_carries_its_images_in_the_order_of_its_file_and_the_same_at_every_frame() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let kind = kind_with_images("textured.fs", [11, 22]);
    assert!(kind.is_installed());
    project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    let at_first = images_of(&layer.operations_at(frame(0)));
    let later = images_of(&layer.operations_at(frame(40)));

    assert_eq!(at_first.len(), 2);
    assert_eq!(at_first, later, "the same images (the same numbers) at every frame");
    assert_ne!(at_first[0].id, at_first[1].id, "two images, two numbers");
    // The pixels are those of the files, in the order the file lists them (the first has 11 of red, the second 22).
    assert_eq!(at_first[0].picture.rgba_pixels[0], 11);
    assert_eq!(at_first[1].picture.rgba_pixels[0], 22);
    assert_eq!(
        (
            at_first[0].picture.width_in_pixels,
            at_first[0].picture.height_in_pixels
        ),
        (3, 2)
    );
}

#[test]
fn the_images_of_two_shaders_are_told_apart_and_a_shader_that_brings_none_has_none() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))], vec![(0, long_video(0, 60, 1))]]);
    let first = project.tracks()[0].placement_at_index(0).unwrap().id;
    let second = project.tracks()[1].placement_at_index(0).unwrap().id;
    let one = kind_with_images("one.fs", [1, 2]);
    let other = kind_with_images("other.fs", [1, 2]);
    project.add_effect(AnimationTarget::Clip(first), one).unwrap();
    project.add_effect(AnimationTarget::Clip(second), other).unwrap();
    let plan = plan(&project);
    let ids: Vec<Vec<u64>> = plan
        .sections
        .iter()
        .flat_map(|section| &section.layers)
        .map(|layer| {
            images_of(&layer.operations_at(frame(5)))
                .iter()
                .map(|image| image.id)
                .collect()
        })
        .collect();
    assert_eq!(ids.len(), 2);
    for id in &ids[0] {
        assert!(!ids[1].contains(id), "{ids:?}");
    }

    // A shader with no image brings none.
    let mut plain_project = project_of(vec![vec![(0, long_video(0, 60, 1))]]);
    let clip = plain_project.tracks()[0].placement_at_index(0).unwrap().id;
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";
    let file = crate::isf::read(source).unwrap();
    let kind = EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "plain.fs", source)).unwrap();
    plain_project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    let plan = super::tests::plan(&plain_project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();
    assert!(images_of(&layer.operations_at(frame(5))).is_empty());
}
