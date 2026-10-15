// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Effects that were given to the engine as shaders, made on the software adapter: what the shader says is what is drawn, the numbers of a call are the
//! shader's numbers, a loop that never ends is made to end by the guard, and a shader that the graphics library will not take is refused when it is given.
//! (What the guard does to the tree of a shader, and what it refuses, is in `guard_tests`; what the engine does when a frame does not finish is in
//! `watchdog_tests`.)

use std::time::{Duration, Instant};

use super::effect_tests::{flat, frame_of, matrices, matrix_effect, noisy, render, whole_frame};
use super::oracle_tests::{TestPicture, compare, upright};
use super::shaders;
use super::testing::with_software_graphics;
use super::{
    DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, FrameDescription, FrameRenderer, GuardedShader, LayerDescription,
    LayerSource, PictureId, RenderProblem, RenderedFrame, guard_wgsl,
};
use crate::colour_effects::ColourMatrix;

const A_LONG_TIME: Duration = Duration::from_secs(60);

/// What every effect shader declares of what it is given (see `effect_shaders`).
const THE_BINDINGS: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 16>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
";

/// Turns the colours of the picture to their opposites by the amount that the first number of the call says (1 is all the way).
pub(super) const INVERT: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let premultiplied = textureLoad(picture, vec2<i32>(position.xy), 0);
    let opacity = premultiplied.a;
    var colour = vec3<f32>(0.0);
    if opacity > 0.0 {
        colour = premultiplied.rgb / opacity;
    }
    let mixed = mix(colour, vec3<f32>(1.0) - colour, numbers[0].x);
    return vec4<f32>(mixed * opacity, opacity);
}
";

/// Makes every pixel the colour of three of the numbers it was given: the first as green, the sixth as blue and the very last of the sixty-four as red.
pub(super) const SHOW_THE_NUMBERS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(numbers[15].w, numbers[0].x, numbers[1].y, 1.0);
}
";

/// Counts the passes of a loop that has no way out, and shows the count: the lowest 8 bits of it as red, the next 8 as green, the next 8 as blue.
pub(super) const COUNT_THE_PASSES_OF_A_LOOP_THAT_NEVER_ENDS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var passes = 0u;
    loop {
        passes = passes + 1u;
    }
    return vec4<f32>(f32(passes & 255u) / 255.0, f32((passes >> 8u) & 255u) / 255.0, f32((passes >> 16u) & 255u) / 255.0, 1.0);
}
";

/// The same, but the loop leaves of its own accord when it has made as many passes as the first number of the call says.
const COUNT_THE_PASSES_UP_TO_A_NUMBER: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let wanted = u32(numbers[0].x);
    var passes = 0u;
    loop {
        passes = passes + 1u;
        if passes >= wanted {
            break;
        }
    }
    return vec4<f32>(f32(passes & 255u) / 255.0, f32((passes >> 8u) & 255u) / 255.0, f32((passes >> 16u) & 255u) / 255.0, 1.0);
}
";

/// The mean of the pixel and the two on each side of it along the row, read with the step of the reading worked out by the card (the way that a loop of an
/// ordinary effect reads a picture).
const BLUR_ALONG_THE_ROW: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(picture));
    var sum = vec4<f32>(0.0);
    for (var tap = -2; tap <= 2; tap++) {
        sum = sum + textureSample(picture, picture_sampler, (position.xy + vec2<f32>(f32(tap), 0.0)) / size);
    }
    return sum / 5.0;
}
";

/// Wants more numbers than a call can give.
pub(super) const TOO_MANY_NUMBERS: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(numbers[16].x, 0.0, 0.0, 1.0);
}
";

/// Wants a picture besides the one it is given.
const A_RESOURCE_IT_IS_NOT_GIVEN: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 16>;
@group(0) @binding(3) var another_picture: texture_2d<f32>;
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(another_picture, vec2<i32>(position.xy), 0) * numbers[0].x;
}
";

/// Wants a picture that is not flat.
const ANOTHER_KIND_OF_PICTURE: &str = "
@group(0) @binding(1) var picture: texture_cube<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureSample(picture, picture_sampler, vec3<f32>(1.0, 0.0, 0.0));
}
";

/// A shader that has a vertex stage and no fragment stage.
const NO_FRAGMENT_STAGE: &str = "
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
";

pub(super) fn effect_source(stage: &str) -> String {
    format!("{THE_BINDINGS}\n{stage}")
}

pub(super) fn guarded(source: &str, most_passes_of_a_loop: u32) -> GuardedShader {
    guard_wgsl(source, most_passes_of_a_loop).unwrap_or_else(|problem| panic!("{problem}\n{source}"))
}

fn installed(renderer: &mut FrameRenderer, key: &str, source: &str) {
    renderer
        .install_effect(key, &guarded(source, DEFAULT_MOST_PASSES_OF_A_LOOP))
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
}

pub(super) fn call(key: &str, numbers: &[f32]) -> EffectCall {
    EffectCall {
        key: key.to_owned(),
        values: numbers.to_vec(),
    }
}

/// What the picture is after the colours are turned to their opposites by `amount`, worked out on the processor (the picture is all opaque).
pub(super) fn inverted(picture: &TestPicture, amount: f32) -> Vec<u8> {
    picture
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[red, green, blue, opacity]| {
            let mixed = |colour: &u8| {
                let colour = f32::from(*colour) / 255.0;
                ((colour + (1.0 - 2.0 * colour) * amount) * 255.0).round() as u8
            };
            [mixed(red), mixed(green), mixed(blue), *opacity]
        })
        .collect()
}

/// The number of passes that a frame made by `COUNT_THE_PASSES...` says, if every pixel says the same.
pub(super) fn the_passes_it_says(frame: &RenderedFrame) -> u32 {
    let [red, green, blue, opacity] = frame.pixel(0, 0);
    assert_eq!(opacity, 255);
    assert!(
        frame.every_pixel_is([red, green, blue, 255]),
        "every pixel makes the same number of passes"
    );
    u32::from(red) + 256 * u32::from(green) + 65_536 * u32::from(blue)
}

#[test]
fn an_effect_that_a_shader_runs_does_what_the_shader_says_with_the_numbers_of_the_call() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:invert", &effect_source(INVERT));
        let picture = TestPicture::gradient(32, 24);
        for amount in [0.0_f32, 0.25, 0.5, 1.0] {
            let (frame, statistics) = frame_of(
                &mut renderer,
                (32, 24),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call("test:invert", &[amount])])],
            );
            assert_eq!(statistics.effects_not_run, 0);
            let difference = compare(&frame.rgba_pixels, &inverted(&picture, amount));
            assert!(difference.largest <= 1, "amount {amount}: {difference:?}");
        }
        // (The pictures that are made are not all alike: all the way is the opposite of what none of the way is.)
        let (none, _) = render(
            &mut renderer,
            (32, 24),
            vec![whole_frame(1, vec![call("test:invert", &[0.0])])],
        );
        let (all, _) = render(
            &mut renderer,
            (32, 24),
            vec![whole_frame(1, vec![call("test:invert", &[1.0])])],
        );
        assert!(compare(&none.rgba_pixels, &all.rgba_pixels).largest > 200);
    });
}

#[test]
fn the_numbers_of_a_call_come_to_the_shader_in_order_with_zeros_after_them_and_all_sixty_four_get_there() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:numbers", &effect_source(SHOW_THE_NUMBERS));
        let picture = flat(4, 4, [9, 9, 9, 255]);
        // (Numbers that are not halves of a step of the 255 steps of a colour, so that the rounding of what the card does is not what is looked at.)
        let (two, _) = frame_of(
            &mut renderer,
            (4, 4),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:numbers", &[0.2, 0.4])])],
        );
        // The first number (0.2) is the green of the shader, the sixth and the sixty-fourth are zeros that nobody gave.
        assert!(two.every_pixel_is([0, 51, 0, 255]), "{:?}", two.pixel(0, 0));

        let mut sixty_four = vec![0.0_f32; 64];
        sixty_four[0] = 0.4;
        sixty_four[5] = 0.6;
        sixty_four[63] = 0.8;
        let (all, statistics) = render(
            &mut renderer,
            (4, 4),
            vec![whole_frame(1, vec![call("test:numbers", &sixty_four)])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        assert!(all.every_pixel_is([204, 102, 153, 255]), "{:?}", all.pixel(0, 0));

        // And a call with none at all gives it zeros.
        let (none, _) = render(
            &mut renderer,
            (4, 4),
            vec![whole_frame(1, vec![call("test:numbers", &[])])],
        );
        assert!(none.every_pixel_is([0, 0, 0, 255]), "{:?}", none.pixel(0, 0));
    });
}

#[test]
fn effects_are_run_in_the_order_they_were_asked_for_whether_a_shader_runs_them_or_the_engine() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:invert", &effect_source(INVERT));
        let picture = flat(4, 4, [100, 150, 200, 255]);
        let darker = || {
            matrix_effect(ColourMatrix::from_rows([
                [0.5, 0.0, 0.0],
                [0.0, 0.5, 0.0],
                [0.0, 0.0, 0.5],
            ]))
        };
        // The opposite of the colours, and then half of that...
        let (first, statistics) = frame_of(
            &mut renderer,
            (4, 4),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:invert", &[1.0]), darker()])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        assert_eq!(
            statistics.passes_recorded,
            2 + 1 + 1,
            "an effect is a pass, and so is the engine's"
        );
        let near = |frame: &RenderedFrame, expected: [u8; 3]| {
            let pixel = frame.pixel(1, 1);
            (0..3).all(|number| pixel[number].abs_diff(expected[number]) <= 1) && pixel[3] == 255
        };
        // (155, 105 and 55, halved: 77.5, 52.5 and 27.5, which a step either way is as near to.)
        assert!(near(&first, [77, 52, 27]), "{:?}", first.pixel(1, 1));
        // ...is not half of the colours, and then the opposite of that.
        let (second, _) = render(
            &mut renderer,
            (4, 4),
            vec![whole_frame(1, vec![darker(), call("test:invert", &[1.0])])],
        );
        // (50, 75 and 100, and the opposite of them: 205, 180 and 155.)
        assert!(near(&second, [205, 180, 155]), "{:?}", second.pixel(1, 1));
        assert!(
            compare(&first.rgba_pixels, &second.rgba_pixels).largest > 100,
            "the order makes the pictures different"
        );
    });
}

#[test]
fn an_effect_works_on_the_picture_of_the_layer_and_not_on_the_whole_frame() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:invert", &effect_source(INVERT));
        // A picture of 16 by 12 pixels, put on the lower right quarter of a frame of 32 by 24, which it covers one pixel to a pixel.
        let picture = TestPicture::gradient(16, 12);
        renderer
            .upload_picture(PictureId(1), &picture.as_picture_data())
            .unwrap();
        let mut description = FrameDescription::black((32, 24));
        description.layers.push(LayerDescription {
            source: LayerSource::Picture(PictureId(1)),
            corners: upright(0.5, 0.5, 0.5, 0.5),
            effects: vec![call("test:invert", &[1.0])],
        });
        let (frame, _) = renderer.render(&description, A_LONG_TIME).unwrap();
        let opposite = inverted(&picture, 1.0);
        let mut expected = [0, 0, 0, 255].repeat(32 * 24);
        for down in 0..12usize {
            for across in 0..16usize {
                let from = (down * 16 + across) * 4;
                let to = ((12 + down) * 32 + 16 + across) * 4;
                expected[to..to + 4].copy_from_slice(&opposite[from..from + 4]);
            }
        }
        let difference = compare(&frame.rgba_pixels, &expected);
        assert!(difference.largest <= 1, "{difference:?}");
    });
}

#[test]
fn a_loop_that_never_ends_ends_after_exactly_the_passes_that_the_guard_was_told_and_the_frame_is_made_in_under_a_second()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = flat(8, 8, [10, 20, 30, 255]);
        let source = effect_source(COUNT_THE_PASSES_OF_A_LOOP_THAT_NEVER_ENDS);
        for most_passes in [1_u32, 5, 800, 70_000, DEFAULT_MOST_PASSES_OF_A_LOOP] {
            renderer
                .install_effect("test:never_ends", &guarded(&source, most_passes))
                .unwrap();
            let started = Instant::now();
            let (frame, statistics) = frame_of(
                &mut renderer,
                (8, 8),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call("test:never_ends", &[])])],
            );
            let took = started.elapsed();
            assert_eq!(statistics.effects_not_run, 0);
            assert_eq!(
                the_passes_it_says(&frame),
                most_passes,
                "the loop is let make the passes the guard was told, and no more"
            );
            assert!(
                took < Duration::from_secs(1),
                "{most_passes} passes of a loop that would never end took {took:?}"
            );
        }
    });
}

#[test]
fn a_loop_that_ends_of_its_own_accord_before_the_limit_is_left_as_it_was_and_one_that_would_not_is_cut_at_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = flat(8, 8, [10, 20, 30, 255]);
        let source = effect_source(COUNT_THE_PASSES_UP_TO_A_NUMBER);
        renderer.install_effect("test:counts", &guarded(&source, 800)).unwrap();
        for (wanted, made) in [
            (1.0_f32, 1),
            (200.0, 200),
            (799.0, 799),
            (800.0, 800),
            (801.0, 800),
            (50_000.0, 800),
        ] {
            let (frame, _) = frame_of(
                &mut renderer,
                (8, 8),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call("test:counts", &[wanted])])],
            );
            assert_eq!(the_passes_it_says(&frame), made, "{wanted} passes were wanted");
        }
        // With no limit that anybody will meet, the loop that ends of its own accord is made as many passes as it wants.
        renderer
            .install_effect("test:counts", &guarded(&source, u32::MAX))
            .unwrap();
        let (frame, _) = render(
            &mut renderer,
            (8, 8),
            vec![whole_frame(1, vec![call("test:counts", &[50_000.0])])],
        );
        assert_eq!(the_passes_it_says(&frame), 50_000);
    });
}

#[test]
fn a_shader_that_reads_a_picture_in_a_loop_is_given_its_counter_and_still_makes_the_picture_that_the_processor_makes() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:blur", &effect_source(BLUR_ALONG_THE_ROW));
        let picture = noisy(24, 16, 11, false);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (24, 16),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:blur", &[])])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        // The mean of the five that are nearest along the row, the one at the edge standing for those past it.
        let mut expected = Vec::new();
        for down in 0..16usize {
            for across in 0..24isize {
                for channel in 0..3 {
                    let sum: u32 = (-2..=2)
                        .map(|step| {
                            let across = (across + step).clamp(0, 23) as usize;
                            u32::from(picture.rgba[(down * 24 + across) * 4 + channel])
                        })
                        .sum();
                    expected.push((sum as f32 / 5.0).round() as u8);
                }
                expected.push(255);
            }
        }
        let difference = compare(&frame.rgba_pixels, &expected);
        assert!(difference.largest <= 2, "{difference:?}");
    });
}

#[test]
fn a_shader_that_the_library_will_not_take_is_refused_when_it_is_given_in_words_and_the_renderer_goes_on() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = flat(4, 4, [100, 150, 200, 255]);
        let refused = [
            (
                "it wants more numbers than a call can give",
                TOO_MANY_NUMBERS.to_owned(),
            ),
            (
                "it wants a picture the effect is not given",
                A_RESOURCE_IT_IS_NOT_GIVEN.to_owned(),
            ),
            ("it wants another kind of picture", ANOTHER_KIND_OF_PICTURE.to_owned()),
        ];
        for (reason, source) in refused {
            let problem = renderer
                .install_effect("test:refused", &guarded(&source, DEFAULT_MOST_PASSES_OF_A_LOOP))
                .unwrap_err();
            assert!(
                matches!(problem, RenderProblem::BadEffectShader(_)),
                "{reason}: {problem:?}"
            );
            assert!(
                problem.to_string().contains("the graphics library would not take it"),
                "{reason}: {problem}"
            );
            // It is not in the engine, so a frame that asks for it is made without it.
            let (frame, statistics) = frame_of(
                &mut renderer,
                (4, 4),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call("test:refused", &[1.0])])],
            );
            assert_eq!(statistics.effects_not_run, 1, "{reason}");
            assert!(frame.every_pixel_is([100, 150, 200, 255]), "{reason}");
        }
        assert_eq!(
            graphics.device.take_trouble(),
            Vec::new(),
            "the library's complaints were caught, and none went to the handler of the window"
        );
        // And what is not refused still works.
        installed(&mut renderer, "test:invert", &effect_source(INVERT));
        let (frame, _) = render(
            &mut renderer,
            (4, 4),
            vec![whole_frame(1, vec![call("test:invert", &[1.0])])],
        );
        assert!(frame.every_pixel_is([155, 105, 55, 255]), "{:?}", frame.pixel(0, 0));
    });
}

#[test]
fn a_shader_with_no_fragment_entry_point_and_a_key_that_is_the_engines_own_are_refused_in_words() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let no_fragment = renderer
            .install_effect("test:vertex_only", &guarded(NO_FRAGMENT_STAGE, 10))
            .unwrap_err();
        assert!(
            matches!(&no_fragment, RenderProblem::BadEffectShader(words) if words.contains("fs_main") && words.contains("vs_main")),
            "{no_fragment}"
        );
        for key in ["engine:colour_matrix", "engine:another", "engine:"] {
            let belongs_to_the_engine = renderer
                .install_effect(key, &guarded(&effect_source(INVERT), 10))
                .unwrap_err();
            assert!(
                matches!(&belongs_to_the_engine, RenderProblem::BadEffectShader(words) if words.contains(key) && words.contains("engine")),
                "{key}: {belongs_to_the_engine}"
            );
        }
        // The engine's own colour matrix is still its own.
        let picture = flat(4, 4, [100, 150, 200, 255]);
        let swap = ColourMatrix::from_rows([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (4, 4),
            &[(1, &picture)],
            vec![whole_frame(1, vec![matrix_effect(swap)])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        assert!(frame.every_pixel_is([200, 150, 100, 255]));
    });
}

#[test]
fn an_effect_that_is_not_installed_is_left_out_and_counted_until_it_is_and_a_new_shader_under_the_key_replaces_the_old()
{
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = flat(4, 4, [100, 150, 200, 255]);
        let (before, statistics) = frame_of(
            &mut renderer,
            (4, 4),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:effect", &[1.0])])],
        );
        assert_eq!(statistics.effects_not_run, 1);
        assert_eq!(statistics.passes_recorded, 2);
        assert!(before.every_pixel_is([100, 150, 200, 255]));

        installed(&mut renderer, "test:effect", &effect_source(INVERT));
        let layers = || vec![whole_frame(1, vec![call("test:effect", &[1.0])])];
        let (inverted, statistics) = render(&mut renderer, (4, 4), layers());
        assert_eq!(statistics.effects_not_run, 0);
        assert_eq!(statistics.passes_recorded, 3);
        assert!(inverted.every_pixel_is([155, 105, 55, 255]));

        installed(&mut renderer, "test:effect", &effect_source(SHOW_THE_NUMBERS));
        let (replaced, _) = render(&mut renderer, (4, 4), layers());
        assert!(
            replaced.every_pixel_is([0, 255, 0, 255]),
            "the other shader's picture: {:?}",
            replaced.pixel(0, 0)
        );
    });
}

#[test]
fn a_call_with_more_numbers_than_there_can_be_or_one_that_is_not_a_number_is_left_out_and_counted() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:numbers", &effect_source(SHOW_THE_NUMBERS));
        let picture = flat(4, 4, [100, 150, 200, 255]);
        let layers = |numbers: &[f32]| vec![whole_frame(1, vec![call("test:numbers", numbers)])];
        let (sixty_four, statistics) = frame_of(&mut renderer, (4, 4), &[(1, &picture)], layers(&[0.0; 64]));
        assert_eq!(statistics.effects_not_run, 0);
        assert!(sixty_four.every_pixel_is([0, 0, 0, 255]));
        for (name, numbers) in [
            ("sixty-five numbers", vec![0.0; 65]),
            ("a number that is not a number", vec![1.0, f32::NAN]),
            ("a number that goes on for ever", vec![f32::INFINITY]),
            ("one that goes on for ever the other way", vec![f32::NEG_INFINITY]),
        ] {
            let (frame, statistics) = render(&mut renderer, (4, 4), layers(&numbers));
            assert_eq!(statistics.effects_not_run, 1, "{name}");
            assert_eq!(statistics.passes_recorded, 2, "{name}");
            assert!(frame.every_pixel_is([100, 150, 200, 255]), "{name}");
        }
    });
}

#[test]
fn the_effects_of_a_frame_that_shaders_run_are_named_once_each_in_the_order_they_come_in_and_no_others() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:invert", &effect_source(INVERT));
        installed(&mut renderer, "test:numbers", &effect_source(SHOW_THE_NUMBERS));
        let swap = matrix_effect(ColourMatrix::from_rows([
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ]));
        let mut description = FrameDescription::black((8, 8));
        description.layers.push(LayerDescription {
            source: LayerSource::Picture(PictureId(1)),
            corners: upright(0.0, 0.0, 1.0, 1.0),
            effects: vec![
                call("test:numbers", &[]),
                swap,
                call("test:invert", &[1.0]),
                call("test:not_installed", &[]),
                call("test:numbers", &[0.5]),
                call("test:invert", &[0.0; 65]),
            ],
        });
        description.layers.push(LayerDescription {
            source: LayerSource::Solid([1, 2, 3, 255]),
            corners: upright(0.0, 0.0, 1.0, 1.0),
            effects: vec![call("test:invert", &[1.0])],
        });
        assert_eq!(
            renderer.shader_effects_in(&description),
            vec!["test:numbers".to_owned(), "test:invert".to_owned()]
        );
        assert_eq!(
            renderer.shader_effects_in(&FrameDescription::black((8, 8))),
            Vec::<String>::new()
        );
    });
}

#[test]
fn the_engines_own_colour_matrix_shader_given_to_the_engine_as_an_effect_makes_the_picture_that_the_engines_own_effect_makes()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The engine's own shader, text and all, goes the road that the shaders of strangers go: it is read, checked, given counters (it has no loop), written out
        // again and made a pipeline from. If that road changed what a shader means, the effect that is made this way would not be the one that is made the usual way.
        let own = guarded(shaders::COLOUR_MATRIX, DEFAULT_MOST_PASSES_OF_A_LOOP);
        assert_eq!(own.loops_capped, 0);
        renderer.install_effect("test:own_colour_matrix", &own).unwrap();
        let pictures = [
            ("gradient", TestPicture::gradient(32, 24)),
            ("random colours and opacities", noisy(24, 16, 9, true)),
        ];
        for (picture_name, picture) in &pictures {
            let size = (picture.width, picture.height);
            for (matrix_name, matrix) in matrices() {
                // The shader's matrix is three columns of three numbers, each in four numbers' room (the last is room): that is where the numbers of a call go.
                let rows = matrix.rows();
                let columns: Vec<f32> = (0..3)
                    .flat_map(|column| [rows[0][column], rows[1][column], rows[2][column], 0.0])
                    .collect();
                let (built_in, _) = frame_of(
                    &mut renderer,
                    size,
                    &[(1, picture)],
                    vec![whole_frame(1, vec![matrix_effect(matrix)])],
                );
                let (given, statistics) = render(
                    &mut renderer,
                    size,
                    vec![whole_frame(1, vec![call("test:own_colour_matrix", &columns)])],
                );
                assert_eq!(statistics.effects_not_run, 0, "{picture_name} [{matrix_name}]");
                let difference = compare(&given.rgba_pixels, &built_in.rgba_pixels);
                assert_eq!(difference.largest, 0, "{picture_name} [{matrix_name}]: {difference:?}");
            }
        }
    });
}
