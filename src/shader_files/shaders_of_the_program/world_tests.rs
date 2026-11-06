// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders that come with the program that say a number can be read from a mask **laid in space** (`"WORLD_SPACE": true`): that each of them compiles for every shape, and that **a
//! mask of one colour over the whole of space is the slider**: with a mask of no red the numbers are what their sliders are at the least of their ranges, and with a mask of all the red
//! they are at the most. (A number that reads the mask in the wrong place, or that is used before the shader says where in space it is, would be its own number where the mask has nothing.)

use std::time::Duration;

use super::*;
use crate::engine::testing::with_software_graphics;
use crate::engine::{AdapterChoice, EngineDevice};
use crate::engine::{
    CORNERS_OF_THE_WHOLE_PICTURE, DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, FrameDescription, FrameRenderer,
    GroupDescription, LayerDescription, LayerSource, PictureData, PictureId, RenderedFrame, guard_wgsl,
};
use crate::isf::{
    DrivenInput, IsfInputKind, MOST_NUMBERS_OF_A_CALL, Mapping, MaskChannel, NUMBERS_BEFORE_THE_INPUTS,
    NUMBERS_OF_A_WORLD_PLACEMENT, Space, WorldShape, compile_with_driven_inputs, numbers_of_the_values, read,
};
use crate::project::{EffectKind, PropertyValue, WorldPlacement};
use crate::shader_files::{ShaderOnARenderer, a_scene_to_look_at, write_as_png};

const SIZE: (u32, u32) = (48, 27);

/// The shaders of the program that have a number that says it can be read in space.
fn shaders_that_work_in_space() -> Vec<&'static ShaderOfTheProgram> {
    SHADERS_OF_THE_PROGRAM
        .iter()
        .filter(|shader| read(shader.text).unwrap().inputs.iter().any(|input| input.world_space))
        .collect()
}

#[test]
fn the_shaders_that_work_in_space_are_the_three_that_make_a_shape_in_space_and_each_says_how_big_it_is() {
    let names: Vec<&str> = shaders_that_work_in_space()
        .iter()
        .map(|shader| shader.file_name)
        .collect();

    assert_eq!(names, ["Mandelbulb.fs", "Folded Sponge.fs", "Mandelbox.fs"]);
    for shader in shaders_that_work_in_space() {
        let file = read(shader.text).unwrap();
        let size = file
            .world_size
            .unwrap_or_else(|| panic!("{} says no WORLD_SIZE", shader.file_name));
        // The size is the diameter of the sphere that holds the shape: twice SCENE_RADIUS, which the shader says.
        let radius: f32 = shader
            .text
            .lines()
            .find_map(|line| line.strip_prefix("#define SCENE_RADIUS "))
            .unwrap_or_else(|| panic!("{} has no SCENE_RADIUS", shader.file_name))
            .trim()
            .parse()
            .unwrap();
        assert!(
            (size - 2.0 * radius).abs() < 0.2,
            "{}: WORLD_SIZE {size}, SCENE_RADIUS {radius}",
            shader.file_name
        );
    }
}

#[test]
fn every_number_that_says_world_space_is_one_that_the_shader_uses_only_where_it_says_where_it_is() {
    // The shader says where it is as the first thing the function of the shape does; a number that says WORLD_SPACE is used only in that function (and, as a name, in the header).
    for shader in shaders_that_work_in_space() {
        let file = read(shader.text).unwrap();
        let start = shader
            .text
            .find("float scene_distance(vec3 point, out float trap)")
            .unwrap();
        let end = start + shader.text[start..].find("\n}\n").unwrap();
        let function = &shader.text[start..end];
        assert!(
            function.contains("isf_world_position = point;"),
            "{}: the function of the shape does not say where it is",
            shader.file_name
        );
        for input in file.inputs.iter().filter(|input| input.world_space) {
            let uses_in_the_text = shader
                .text
                .match_indices(&input.name)
                .filter(|(at, _)| {
                    let before = shader.text[..*at].chars().next_back();
                    let after = shader.text[at + input.name.len()..].chars().next();
                    !before.is_some_and(|character| character.is_alphanumeric() || character == '_')
                        && !after.is_some_and(|character| character.is_alphanumeric() || character == '_')
                })
                .count();
            let uses_in_the_function = function
                .match_indices(&input.name)
                .filter(|(at, _)| {
                    let before = function[..*at].chars().next_back();
                    !before.is_some_and(|character| character.is_alphanumeric() || character == '_')
                })
                .count();
            // (One mention in the header's list of inputs, and the rest in the function.)
            assert_eq!(
                uses_in_the_text,
                1 + uses_in_the_function,
                "{}: the number {} is used outside the function that says where the shader is",
                shader.file_name,
                input.name
            );
            assert!(
                uses_in_the_function >= 1,
                "{}: {} is not used at all",
                shader.file_name,
                input.name
            );
        }
    }
}

/// A mask of all of space with this much red.
fn mask_with_red(red: u8) -> Vec<GroupDescription> {
    vec![GroupDescription {
        layers: vec![LayerDescription {
            source: LayerSource::Solid([red, 0, 0, 255]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        }],
    }]
}

/// The frame of the shader `text`, installed on `renderer` as the variant that reads `driven` from the mask `mask` (which covers all of space), with the numbers of the values `values` and
/// the placements of the masks after them.
fn frame_with_the_numbers_read_in_space(
    renderer: &mut FrameRenderer,
    key: &str,
    text: &str,
    driven: &[DrivenInput],
    values: &[PropertyValue],
    placement: &WorldPlacement,
    mask: Vec<GroupDescription>,
) -> RenderedFrame {
    let file = read(text).unwrap();
    let compiled = compile_with_driven_inputs(&file, driven).unwrap_or_else(|problems| panic!("{key}: {problems:?}"));
    let structure = compiled
        .structure
        .clone()
        .unwrap_or_else(|problems| panic!("{key}: {problems:?}"));
    let guarded = guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP)
        .unwrap_or_else(|problem| panic!("{key}: {problem:?}"));
    renderer
        .install_effect_with_structure(key, &guarded, &structure)
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
    let mut numbers = vec![1.3, 1.0 / 30.0, 39.0, 0.0];
    numbers.extend(numbers_of_the_values(values));
    for _ in driven {
        numbers.extend(placement.numbers());
    }
    let scene = PictureData::rgba8(SIZE.0, SIZE.1, a_scene_to_look_at(SIZE.0, SIZE.1)).unwrap();
    renderer.upload_picture(PictureId(1), &scene).unwrap();
    let mut description = FrameDescription::black(SIZE);
    description.layers = vec![LayerDescription {
        source: LayerSource::Picture(PictureId(1)),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects: vec![EffectCall {
            key: key.to_owned(),
            values: numbers,
            table: None,
            state: None,
            images: Vec::new(),
            masks: mask,
        }],
    }];
    let (frame, statistics) = renderer
        .render(&description, Duration::from_secs(120))
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
    assert_eq!(
        statistics.effects_not_run, 0,
        "{key}: the engine did not run the effect"
    );
    frame
}

fn pixels_that_differ(first: &RenderedFrame, second: &RenderedFrame) -> usize {
    first
        .rgba_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(second.rgba_pixels.as_chunks::<4>().0)
        .filter(|(one, other)| {
            one.iter()
                .zip(other.iter())
                .any(|(one, other)| one.abs_diff(*other) >= 3)
        })
        .count()
}

/// Whether the adapter is one of those that the software adapter of Windows (WARP) is: it loses its device (the pixels of the frame cannot be got back) on **these shaders with a number read from
/// a mask**, over the picture or in space alike (found by trying: the same shader with no mask, or with a mask for a number that the shader uses outside its loops, draws). A real card
/// and lavapipe draw them, so the test runs where they are.
fn is_the_software_adapter_that_cannot_draw_these(device: &EngineDevice) -> bool {
    let name = device.adapter_information.name.to_lowercase();
    name.contains("basic render") || name.contains("warp")
}

fn a_mask_of_one_colour_over_all_of_space_is_the_slider_on(device: &EngineDevice) {
    for (number, shader) in shaders_that_work_in_space().into_iter().enumerate() {
        let file = read(shader.text).unwrap();
        let mut renderer = FrameRenderer::new(device.clone());
        let installed = ShaderOnARenderer::install(&mut renderer, 900 + number as u64, shader.file_name, shader.text)
            .unwrap_or_else(|why| panic!("{}: {why}", shader.file_name));
        let kind: EffectKind = installed.kind();
        // As many of the numbers that work in space as there is room for among the numbers of a call: a place takes nine.
        let numbers_of_the_inputs: usize = file.inputs.iter().map(|input| input.kind.numbers()).sum();
        let room =
            (MOST_NUMBERS_OF_A_CALL - NUMBERS_BEFORE_THE_INPUTS - numbers_of_the_inputs) / NUMBERS_OF_A_WORLD_PLACEMENT;
        let names: Vec<&str> = file
            .inputs
            .iter()
            .filter(|input| input.world_space)
            .map(|input| input.name.as_str())
            .take(room)
            .collect();
        assert!(!names.is_empty(), "{}: no room for a place in space", shader.file_name);
        let placement = WorldPlacement {
            width: file.world_size.unwrap(),
            height: file.world_size.unwrap(),
            ..WorldPlacement::default()
        };
        let range_of = |name: &str| match file.inputs.iter().find(|input| input.name == name).unwrap().kind {
            IsfInputKind::Float { minimum, maximum, .. } => (minimum, maximum),
            _ => panic!("{name} is not a float"),
        };
        for (red, end_of_the_range) in [(0u8, 0usize), (255u8, 1usize)] {
            // The sliders at that end of their ranges.
            let changed: Vec<(String, String)> = names
                .iter()
                .map(|name| {
                    let (least, most) = range_of(name);
                    (
                        (*name).to_owned(),
                        format!("{}", if end_of_the_range == 0 { least } else { most }),
                    )
                })
                .collect();
            let changed: Vec<(&str, &str)> = changed.iter().map(|(id, text)| (id.as_str(), text.as_str())).collect();
            let values = installed.values_with(&changed).unwrap();
            let by_the_sliders = installed
                .draw(&mut renderer, SIZE, 1.3, &values, &the_scene_of_these_tests())
                .unwrap_or_else(|why| panic!("{}: {why}", shader.file_name));
            for shape in WorldShape::ALL {
                let driven: Vec<DrivenInput> = names
                    .iter()
                    .map(|name| DrivenInput {
                        name: (*name).to_owned(),
                        mask: 0,
                        channel: MaskChannel::Red,
                        mapping: Mapping::Straight,
                        space: Space::World(shape),
                    })
                    .collect();
                let key = format!("world:{}:{shape:?}:{red}", shader.file_name);
                let by_the_mask = frame_with_the_numbers_read_in_space(
                    &mut renderer,
                    &key,
                    shader.text,
                    &driven,
                    &kind.default_values(),
                    &placement,
                    mask_with_red(red),
                );
                assert_eq!(
                    pixels_that_differ(&by_the_sliders, &by_the_mask),
                    0,
                    "{}: with a mask of {red} of red over all of space ({shape:?}) the numbers {names:?} are not what their sliders are at {}",
                    shader.file_name,
                    if end_of_the_range == 0 {
                        "the least of their ranges"
                    } else {
                        "the most"
                    }
                );
            }
        }
    }
}

#[test]
fn a_mask_of_one_colour_over_all_of_space_is_the_slider_at_the_end_of_its_range_for_every_shape_and_every_number_that_works_in_space()
 {
    with_software_graphics!(graphics, {
        if is_the_software_adapter_that_cannot_draw_these(&graphics.device) {
            eprintln!(
                "SKIPPED: {} loses its device on these shaders with a number read from a mask",
                graphics.device.adapter_information.name
            );
            return;
        }
        a_mask_of_one_colour_over_all_of_space_is_the_slider_on(&graphics.device);
    });
}

#[test]
#[ignore = "by hand: it draws on the best graphics card of this computer, which the tests must not do by themselves (SF-01)"]
fn a_mask_of_one_colour_over_all_of_space_is_the_slider_on_the_best_graphics_card_of_this_computer() {
    let device = EngineDevice::create(&AdapterChoice::BestAvailable).expect("a graphics card");
    eprintln!("drawing on {}", device.adapter_information.name);
    a_mask_of_one_colour_over_all_of_space_is_the_slider_on(&device);
}

fn the_scene_of_these_tests() -> PictureData {
    PictureData::rgba8(SIZE.0, SIZE.1, a_scene_to_look_at(SIZE.0, SIZE.1)).unwrap()
}

/// Pictures for looking at: each shader that works in space, with its first number that says so read from **a ramp from no red on the left to all the red on the right**, laid in space on
/// each of the shapes (and, for the first, the slider as it is, for comparing). Written into the folder that `FASTFORWARDRS_WORLD_PICTURES` names. By hand, on the best graphics card.
#[test]
#[ignore = "by hand: it draws on the best graphics card of this computer and writes pictures into the folder that FASTFORWARDRS_WORLD_PICTURES names"]
fn write_pictures_of_the_shaders_that_work_in_space_with_a_ramp_laid_on_each_shape() {
    let folder = std::path::PathBuf::from(std::env::var("FASTFORWARDRS_WORLD_PICTURES").expect("the folder"));
    std::fs::create_dir_all(&folder).unwrap();
    let device = EngineDevice::create(&AdapterChoice::BestAvailable).expect("a graphics card");
    let size = (640u32, 360u32);
    // A ramp: 256 pixels across, the red going from 0 to 255, and a mask is the group of this one picture over all of it.
    let mut ramp = Vec::new();
    for _ in 0..4 {
        for across in 0..256u32 {
            ramp.extend([across as u8, 0, 0, 255]);
        }
    }
    let ramp_picture = PictureData::rgba8(256, 4, ramp).unwrap();
    for (number, shader) in shaders_that_work_in_space().into_iter().enumerate() {
        let file = read(shader.text).unwrap();
        let mut renderer = FrameRenderer::new(device.clone());
        let installed =
            ShaderOnARenderer::install(&mut renderer, 700 + number as u64, shader.file_name, shader.text).unwrap();
        let kind = installed.kind();
        let scene = PictureData::rgba8(size.0, size.1, a_scene_to_look_at(size.0, size.1)).unwrap();
        let plain = installed
            .draw(&mut renderer, size, 1.3, &kind.default_values(), &scene)
            .unwrap();
        let stem = shader.file_name.trim_end_matches(".fs").replace(' ', "_");
        write_as_png(&plain, &folder.join(format!("{stem}_the_slider.png"))).unwrap();
        for input in file.inputs.iter().filter(|input| input.world_space) {
            for shape in WorldShape::ALL {
                let driven = [DrivenInput {
                    name: input.name.clone(),
                    mask: 0,
                    channel: MaskChannel::Red,
                    mapping: Mapping::Straight,
                    space: Space::World(shape),
                }];
                let compiled = compile_with_driven_inputs(&file, &driven).unwrap();
                let structure = compiled.structure.clone().unwrap();
                let guarded = guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap();
                let key = format!("ramp:{stem}:{}:{shape:?}", input.name);
                renderer
                    .install_effect_with_structure(&key, &guarded, &structure)
                    .unwrap();
                renderer.upload_picture(PictureId(77), &ramp_picture).unwrap();
                renderer.upload_picture(PictureId(1), &scene).unwrap();
                let mut numbers = vec![1.3, 1.0 / 30.0, 39.0, 0.0];
                numbers.extend(numbers_of_the_values(&kind.default_values()));
                // The shape is as big as the shader's shape, a little less tall than wide for the plane, so that the ramp is seen to run across it.
                let placement = WorldPlacement {
                    width: file.world_size.unwrap(),
                    height: file.world_size.unwrap(),
                    ..WorldPlacement::default()
                };
                numbers.extend(placement.numbers());
                let mut description = FrameDescription::black(size);
                description.layers = vec![LayerDescription {
                    source: LayerSource::Picture(PictureId(1)),
                    corners: CORNERS_OF_THE_WHOLE_PICTURE,
                    effects: vec![EffectCall {
                        key,
                        values: numbers,
                        table: None,
                        state: None,
                        images: Vec::new(),
                        masks: vec![GroupDescription {
                            layers: vec![LayerDescription {
                                source: LayerSource::Picture(PictureId(77)),
                                corners: CORNERS_OF_THE_WHOLE_PICTURE,
                                effects: Vec::new(),
                            }],
                        }],
                    }],
                }];
                let (frame, _) = renderer.render(&description, Duration::from_secs(120)).unwrap();
                write_as_png(&frame, &folder.join(format!("{stem}_{}_{shape:?}.png", input.name))).unwrap();
            }
        }
    }
}
