// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Images that a shader brings with it (phase 6.4.4), made on the software adapter: the call names the pictures, the shader reads them as the second picture it is given, a call that
//! does not name them all is not run, and the frame says that it uses them (so that whoever sends the pictures knows to send them).

use super::effect_tests::{frame_of, render, whole_frame};
use super::foreign_effect_tests::{call, guarded};
use super::oracle_tests::{TestPicture, compare};
use super::testing::with_software_graphics;
use super::{
    DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, FrameDescription, FrameRenderer, ImageSlot, PassSpec, PictureData,
    PictureId, RenderProblem, ShaderStructure,
};

const THE_PICTURE_AND_AN_IMAGE: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@group(0) @binding(3) var imported: texture_2d<f32>;
@group(0) @binding(4) var imported_sampler: sampler;
";

/// Shows the image that the shader brings, where it is (the pixel of the image that is where the pixel of the frame is), and ignores the picture.
const SHOW_THE_IMAGE: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(imported, vec2<i32>(position.xy), 0);
}
";

fn one_pass_with_an_image() -> ShaderStructure {
    ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Imported(0)],
        buffers: Vec::new(),
        passes: vec![PassSpec {
            target: None,
            width: None,
            height: None,
        }],
        pass_index_number: Some(3),
    }
}

fn installed(renderer: &mut FrameRenderer, key: &str, structure: &ShaderStructure) {
    renderer
        .install_effect_with_structure(
            key,
            &guarded(
                &format!("{THE_PICTURE_AND_AN_IMAGE}\n{SHOW_THE_IMAGE}"),
                DEFAULT_MOST_PASSES_OF_A_LOOP,
            ),
            structure,
        )
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
}

/// A call of `key` that names these pictures as the images the shader brings.
fn bringing(key: &str, images: &[u64]) -> EffectCall {
    EffectCall {
        images: images.iter().map(|id| PictureId(*id)).collect(),
        ..call(key, &[0.0; 4])
    }
}

/// An image of 4 by 4: red at the top left, green at the top right, blue at the bottom left, white at the bottom right, all opaque.
fn four_quarters() -> PictureData {
    let mut rgba = Vec::new();
    for row in 0..4 {
        for column in 0..4 {
            rgba.extend(match (row < 2, column < 2) {
                (true, true) => [255, 0, 0, 255],
                (true, false) => [0, 255, 0, 255],
                (false, true) => [0, 0, 255, 255],
                (false, false) => [255, 255, 255, 255],
            });
        }
    }
    PictureData::rgba8(4, 4, rgba).unwrap()
}

#[test]
fn a_shader_reads_the_image_that_the_call_names_as_the_picture_it_brings() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:image", &one_pass_with_an_image());
        renderer.upload_picture(PictureId(2), &four_quarters()).unwrap();
        let picture = TestPicture::gradient(4, 4);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (4, 4),
            &[(1, &picture)],
            vec![whole_frame(1, vec![bringing("test:image", &[2])])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        assert_eq!(frame.pixel(0, 0), [255, 0, 0, 255], "top left");
        assert_eq!(frame.pixel(3, 0), [0, 255, 0, 255], "top right");
        assert_eq!(frame.pixel(0, 3), [0, 0, 255, 255], "bottom left");
        assert_eq!(frame.pixel(3, 3), [255, 255, 255, 255], "bottom right");
    });
}

#[test]
fn the_image_is_read_as_its_codes_and_not_as_light() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:image", &one_pass_with_an_image());
        let orange = PictureData::rgba8(2, 2, [200, 100, 50, 255].repeat(4)).unwrap();
        renderer.upload_picture(PictureId(2), &orange).unwrap();
        let picture = TestPicture::gradient(2, 2);
        let (frame, _) = frame_of(
            &mut renderer,
            (2, 2),
            &[(1, &picture)],
            vec![whole_frame(1, vec![bringing("test:image", &[2])])],
        );
        // (In the compatibility mode nothing is converted: the codes come out as they went in.)
        assert!(frame.every_pixel_is([200, 100, 50, 255]), "{:?}", frame.pixel(0, 0));
    });
}

#[test]
fn a_call_that_does_not_name_as_many_images_as_the_shader_brings_is_not_run() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:image", &one_pass_with_an_image());
        renderer.upload_picture(PictureId(2), &four_quarters()).unwrap();
        let picture = TestPicture::gradient(4, 4);
        for wrong in [vec![], vec![2, 2]] {
            let (frame, statistics) = frame_of(
                &mut renderer,
                (4, 4),
                &[(1, &picture)],
                vec![whole_frame(1, vec![bringing("test:image", &wrong)])],
            );
            assert_eq!(statistics.effects_not_run, 1, "{wrong:?}");
            assert!(
                compare(&frame.rgba_pixels, &picture.rgba).largest <= 1,
                "the picture is as it was, with the effect left out: {wrong:?}"
            );
        }
    });
}

#[test]
fn a_frame_whose_effect_names_a_picture_that_the_engine_does_not_have_is_not_made_and_says_which() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:image", &one_pass_with_an_image());
        renderer
            .upload_picture(PictureId(1), &TestPicture::gradient(4, 4).as_picture_data())
            .unwrap();
        let mut description = FrameDescription::black((4, 4));
        description.layers = vec![whole_frame(1, vec![bringing("test:image", &[9])])];
        let problem = renderer
            .render(&description, std::time::Duration::from_secs(60))
            .unwrap_err();
        assert!(
            matches!(&problem, RenderProblem::PicturesMissing(ids) if ids == &[PictureId(9)]),
            "{problem}"
        );
        // Sent, it is made.
        renderer.upload_picture(PictureId(9), &four_quarters()).unwrap();
        let (frame, _) = render(&mut renderer, (4, 4), description.layers);
        assert_eq!(frame.pixel(0, 0), [255, 0, 0, 255]);
    });
}

#[test]
fn the_pictures_a_frame_uses_are_those_of_its_layers_and_those_that_its_shaders_bring_each_once() {
    let mut description = FrameDescription::black((4, 4));
    description.layers = vec![
        whole_frame(
            1,
            vec![bringing("test:image", &[5, 6]), bringing("test:other", &[6, 1, 7])],
        ),
        whole_frame(2, vec![bringing("test:image", &[5])]),
    ];
    assert_eq!(
        description.pictures_used(),
        [1, 5, 6, 7, 2].map(PictureId),
        "in the order they are met in, each once"
    );
    // The images are a part of what the frame is: another picture is another frame.
    let mut other = description.clone();
    other.layers[0].effects[0].images[1] = PictureId(8);
    assert_ne!(other.fingerprint(), description.fingerprint());
}

#[test]
fn the_pictures_of_a_shader_that_is_in_a_group_are_used_by_the_frame() {
    let mut description = FrameDescription::black((4, 4));
    description.layers = vec![super::LayerDescription {
        source: super::LayerSource::Group(super::GroupDescription {
            layers: vec![whole_frame(3, vec![bringing("test:image", &[4])])],
        }),
        corners: super::CORNERS_OF_THE_WHOLE_PICTURE,
        effects: vec![bringing("test:image", &[5])],
    }];
    // (The group's own effect first, then what is in the group.)
    assert_eq!(description.pictures_used(), [5, 3, 4].map(PictureId));
}

const THE_PICTURE_AND_TWO_IMAGES: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@group(0) @binding(3) var first: texture_2d<f32>;
@group(0) @binding(4) var first_sampler: sampler;
@group(0) @binding(5) var second: texture_2d<f32>;
@group(0) @binding(6) var second_sampler: sampler;
";

/// The red of the first image and the green of the second, and the blue of the second image's red.
const MIX_TWO_IMAGES: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let one = textureLoad(first, vec2<i32>(0, 0), 0);
    let two = textureLoad(second, vec2<i32>(0, 0), 0);
    return vec4<f32>(one.r, two.g, two.r, 1.0);
}
";

#[test]
fn a_shader_that_brings_two_images_reads_each_as_the_picture_the_call_names_in_its_place() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let structure = ShaderStructure {
            images: vec![ImageSlot::Picture, ImageSlot::Imported(0), ImageSlot::Imported(1)],
            buffers: Vec::new(),
            passes: vec![PassSpec {
                target: None,
                width: None,
                height: None,
            }],
            pass_index_number: Some(3),
        };
        renderer
            .install_effect_with_structure(
                "test:two_images",
                &guarded(
                    &format!("{THE_PICTURE_AND_TWO_IMAGES}\n{MIX_TWO_IMAGES}"),
                    DEFAULT_MOST_PASSES_OF_A_LOOP,
                ),
                &structure,
            )
            .unwrap();
        // The first has 200 of red and 10 of green; the second 100 of red and 150 of green.
        renderer
            .upload_picture(PictureId(2), &PictureData::rgba8(1, 1, vec![200, 10, 0, 255]).unwrap())
            .unwrap();
        renderer
            .upload_picture(PictureId(3), &PictureData::rgba8(1, 1, vec![100, 150, 0, 255]).unwrap())
            .unwrap();
        let picture = TestPicture::gradient(2, 2);
        let (frame, _) = frame_of(
            &mut renderer,
            (2, 2),
            &[(1, &picture)],
            vec![whole_frame(1, vec![bringing("test:two_images", &[2, 3])])],
        );
        assert!(frame.every_pixel_is([200, 150, 100, 255]), "{:?}", frame.pixel(0, 0));
        // And with the two named the other way round, the other way round.
        let (swapped, _) = render(
            &mut renderer,
            (2, 2),
            vec![whole_frame(1, vec![bringing("test:two_images", &[3, 2])])],
        );
        assert!(swapped.every_pixel_is([100, 10, 200, 255]), "{:?}", swapped.pixel(0, 0));
    });
}
