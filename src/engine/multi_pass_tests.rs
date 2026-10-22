// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shaders that take several passes (phase 6.4.1), made on the software adapter: a pass draws a buffer that the next one reads, a buffer is as big as its expression says, a buffer
//! of 8 bits is rounded to 8 bits and a float one is not, a buffer that nothing drew yet is clear, and the picture the effect works on is the same in every pass.

use super::effect_tests::{frame_of, whole_frame};
use super::foreign_effect_tests::{call, guarded};
use super::oracle_tests::{TestPicture, compare};
use super::testing::with_software_graphics;
use super::{
    BufferSpec, DEFAULT_MOST_PASSES_OF_A_LOOP, FrameRenderer, ImageSlot, Operator, PassSpec, RenderProblem,
    ShaderStructure, SizeExpression,
};

/// What a shader that reads the picture and one buffer declares of what it is given: the 68 numbers (the 4th is the number of the pass, and the last four begin with the size of
/// what the pass draws), the picture, and the buffer.
const THE_PICTURE_AND_A_BUFFER: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@group(0) @binding(3) var buffer: texture_2d<f32>;
@group(0) @binding(4) var buffer_sampler: sampler;
";

/// The same, for a shader with two buffers.
const THE_PICTURE_AND_TWO_BUFFERS: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@group(0) @binding(3) var first_buffer: texture_2d<f32>;
@group(0) @binding(4) var first_buffer_sampler: sampler;
@group(0) @binding(5) var second_buffer: texture_2d<f32>;
@group(0) @binding(6) var second_buffer_sampler: sampler;
";

/// The first pass draws a colour into the buffer; the second swaps its red and its blue.
const A_COLOUR_THROUGH_A_BUFFER: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return vec4<f32>(0.25, 0.5, 0.75, 1.0);
    }
    let held = textureLoad(buffer, vec2<i32>(position.xy), 0);
    return vec4<f32>(held.b, held.g, held.r, 1.0);
}
";

/// The first pass shows how big it is (red: across, green: down, in pixels out of 255) in the buffer; the second shows that in red and green and how big it is across in blue.
const THE_SIZES_OF_THE_PASSES: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return vec4<f32>(numbers[16].x / 255.0, numbers[16].y / 255.0, 0.0, 1.0);
    }
    let held = textureLoad(buffer, vec2<i32>(0, 0), 0);
    return vec4<f32>(held.r, held.g, numbers[16].x / 255.0, 1.0);
}
";

/// The first pass puts a number that is between two steps of 8 bits in the buffer; the second shows, a hundred times as big, how far from a half it came back.
const A_NUMBER_BETWEEN_TWO_STEPS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return vec4<f32>(0.50049, 0.0, 0.0, 1.0);
    }
    let held = textureLoad(buffer, vec2<i32>(0, 0), 0);
    return vec4<f32>((held.r - 0.5) * 100.0, 0.0, 0.0, 1.0);
}
";

/// The first pass reads the buffer it is drawing, which nothing has drawn, and adds 0.4 of red to what it read; the second shows the buffer.
const READING_THE_BUFFER_IT_DRAWS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        let own = textureLoad(buffer, vec2<i32>(position.xy), 0);
        return own + vec4<f32>(0.4, 0.0, 0.0, 1.0);
    }
    return textureLoad(buffer, vec2<i32>(position.xy), 0);
}
";

/// The first pass draws red in the first buffer, the second green in the second; the third gives the picture the effect works on, as it is.
const THE_PICTURE_AFTER_TWO_BUFFERS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0);
    }
    if numbers[0].w < 1.5 {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);
    }
    return textureLoad(picture, vec2<i32>(position.xy), 0);
}
";

/// The first pass copies the picture into the buffer; the second turns its colours to their opposites.
const INVERT_THROUGH_A_BUFFER: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return textureLoad(picture, vec2<i32>(position.xy), 0);
    }
    let held = textureLoad(buffer, vec2<i32>(position.xy), 0);
    return vec4<f32>(vec3<f32>(1.0) - held.rgb, held.a);
}
";

fn a_pass(target: Option<usize>) -> PassSpec {
    PassSpec {
        target,
        width: None,
        height: None,
    }
}

/// The picture and one buffer, drawn by the first of two passes, as the structure of a two-pass blur is.
fn a_buffer_and_two_passes(float: bool) -> ShaderStructure {
    ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Buffer(0)],
        buffers: vec![BufferSpec {
            persistent: false,
            float,
        }],
        passes: vec![a_pass(Some(0)), a_pass(None)],
        pass_index_number: Some(3),
    }
}

fn installed_as(renderer: &mut FrameRenderer, key: &str, source: &str, structure: &ShaderStructure) {
    renderer
        .install_effect_with_structure(key, &guarded(source, DEFAULT_MOST_PASSES_OF_A_LOOP), structure)
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
}

fn the_source(header: &str, stage: &str) -> String {
    format!("{header}\n{stage}")
}

#[test]
fn the_second_pass_reads_what_the_first_drew_into_the_buffer_and_the_passes_know_which_they_are() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_as(
            &mut renderer,
            "test:through",
            &the_source(THE_PICTURE_AND_A_BUFFER, A_COLOUR_THROUGH_A_BUFFER),
            &a_buffer_and_two_passes(false),
        );
        let picture = TestPicture::gradient(16, 12);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (16, 12),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:through", &[0.0; 4])])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        // (0.75, 0.5, 0.25 of 255, to the step of 8 bits.)
        let [red, green, blue, opacity] = frame.pixel(5, 5);
        assert!(
            (i32::from(red) - 191).abs() <= 1
                && (i32::from(green) - 128).abs() <= 1
                && (i32::from(blue) - 64).abs() <= 1,
            "{:?}",
            frame.pixel(5, 5)
        );
        assert_eq!(opacity, 255);
        assert!(
            frame.every_pixel_is(frame.pixel(0, 0)),
            "what the passes draw does not depend on the picture"
        );
        assert_eq!(
            statistics.passes_recorded,
            2 + 1 + 1,
            "two passes of the effect, the layer and the output"
        );
    });
}

#[test]
fn a_buffer_is_as_big_as_its_expressions_say_and_each_pass_is_told_how_big_it_is() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut structure = a_buffer_and_two_passes(false);
        let quarter = |side: SizeExpression| {
            Some(SizeExpression::Binary(
                Operator::Divide,
                Box::new(side),
                Box::new(SizeExpression::Number(4.0)),
            ))
        };
        structure.passes[0].width = quarter(SizeExpression::Width);
        structure.passes[0].height = quarter(SizeExpression::Height);
        installed_as(
            &mut renderer,
            "test:sizes",
            &the_source(THE_PICTURE_AND_A_BUFFER, THE_SIZES_OF_THE_PASSES),
            &structure,
        );
        let picture = TestPicture::gradient(64, 36);
        let (frame, _) = frame_of(
            &mut renderer,
            (64, 36),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:sizes", &[0.0; 4])])],
        );
        // The buffer is 16 by 9 (said by the first pass), and the last pass is 64 across (said in blue).
        assert!(frame.every_pixel_is([16, 9, 64, 255]), "{:?}", frame.pixel(0, 0));
    });
}

#[test]
fn a_buffer_of_eight_bits_is_rounded_to_eight_bits_and_a_float_buffer_is_not() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let source = the_source(THE_PICTURE_AND_A_BUFFER, A_NUMBER_BETWEEN_TWO_STEPS);
        installed_as(&mut renderer, "test:eight", &source, &a_buffer_and_two_passes(false));
        installed_as(&mut renderer, "test:float", &source, &a_buffer_and_two_passes(true));
        let picture = TestPicture::gradient(8, 8);
        let [eight_bit, float] = ["test:eight", "test:float"].map(|key| {
            let (frame, _) = frame_of(
                &mut renderer,
                (8, 8),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call(key, &[0.0; 4])])],
            );
            frame.pixel(0, 0)[0]
        });
        // 0.50049 rounded to a step of 8 bits is 128 of 255, which is 0.00196 above a half (0.196, 50 of 255, a hundred times as big); to a half float it is 0.500488, which is 0.000488
        // above (0.0488, 12 of 255).
        assert!((i32::from(eight_bit) - 50).abs() <= 1, "eight bits: {eight_bit}");
        assert!((i32::from(float) - 12).abs() <= 1, "float: {float}");
    });
}

#[test]
fn a_buffer_that_no_pass_drew_yet_is_read_as_clear() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_as(
            &mut renderer,
            "test:own",
            &the_source(THE_PICTURE_AND_A_BUFFER, READING_THE_BUFFER_IT_DRAWS),
            &a_buffer_and_two_passes(false),
        );
        let picture = TestPicture::gradient(8, 8);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:own", &[0.0; 4])])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        // Nothing plus 0.4 of red, over the whole picture: 102 of 255, and no green or blue.
        assert!(frame.every_pixel_is([102, 0, 0, 255]), "{:?}", frame.pixel(0, 0));
    });
}

#[test]
fn the_picture_the_effect_works_on_is_the_same_in_every_pass() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let structure = ShaderStructure {
            images: vec![ImageSlot::Picture, ImageSlot::Buffer(0), ImageSlot::Buffer(1)],
            buffers: vec![
                BufferSpec {
                    persistent: false,
                    float: false,
                },
                BufferSpec {
                    persistent: false,
                    float: true,
                },
            ],
            passes: vec![a_pass(Some(0)), a_pass(Some(1)), a_pass(None)],
            pass_index_number: Some(3),
        };
        installed_as(
            &mut renderer,
            "test:three",
            &the_source(THE_PICTURE_AND_TWO_BUFFERS, THE_PICTURE_AFTER_TWO_BUFFERS),
            &structure,
        );
        let picture = TestPicture::gradient(32, 24);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (32, 24),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:three", &[0.0; 4])])],
        );
        let difference = compare(&frame.rgba_pixels, &picture.rgba);
        assert!(difference.largest <= 1, "{difference:?}");
        assert_eq!(statistics.passes_recorded, 3 + 1 + 1);
    });
}

#[test]
fn the_effect_after_a_shader_of_several_passes_works_on_what_its_last_pass_drew() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_as(
            &mut renderer,
            "test:invert_through",
            &the_source(THE_PICTURE_AND_A_BUFFER, INVERT_THROUGH_A_BUFFER),
            &a_buffer_and_two_passes(false),
        );
        let picture = TestPicture::gradient(16, 12);
        let inverted_picture = super::foreign_effect_tests::inverted(&picture, 1.0);
        let (frame, _) = frame_of(
            &mut renderer,
            (16, 12),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:invert_through", &[0.0; 4])])],
        );
        let difference = compare(&frame.rgba_pixels, &inverted_picture);
        assert!(difference.largest <= 1, "{difference:?}");

        // Twice over, each pass of each call reads what it should: the picture is back as it was.
        let (twice, statistics) = frame_of(
            &mut renderer,
            (16, 12),
            &[(1, &picture)],
            vec![whole_frame(
                1,
                vec![
                    call("test:invert_through", &[0.0; 4]),
                    call("test:invert_through", &[0.0; 4]),
                ],
            )],
        );
        let difference = compare(&twice.rgba_pixels, &picture.rgba);
        assert!(difference.largest <= 2, "{difference:?}");
        assert_eq!(statistics.passes_recorded, 4 + 1 + 1);
    });
}

#[test]
fn a_structure_that_is_not_well_formed_is_refused_when_the_shader_is_given_and_the_effect_is_not_run() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // (Rewritten on purpose in phase 6.4.2, when a buffer that lasts began to be run: the example of a structure that is refused is a buffer drawn by two passes.)
        let mut two_writers = a_buffer_and_two_passes(false);
        two_writers.passes = vec![a_pass(Some(0)), a_pass(Some(0)), a_pass(None)];
        let refused = renderer
            .install_effect_with_structure(
                "test:lasting",
                &guarded(
                    &the_source(THE_PICTURE_AND_A_BUFFER, A_COLOUR_THROUGH_A_BUFFER),
                    DEFAULT_MOST_PASSES_OF_A_LOOP,
                ),
                &two_writers,
            )
            .unwrap_err();
        assert!(
            matches!(&refused, RenderProblem::BadEffectShader(why) if why.contains("more than one pass")),
            "{refused}"
        );
        // (And what was not given is not run: the frame is made, and says that the effect was left out.)
        let picture = TestPicture::gradient(8, 8);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:lasting", &[0.0; 4])])],
        );
        assert_eq!(statistics.effects_not_run, 1);
        assert!(compare(&frame.rgba_pixels, &picture.rgba).largest <= 1);
    });
}

/// A description of a frame with one layer that has the effect `key`.
fn a_frame_with_the_effect(key: &str) -> super::FrameDescription {
    let mut description = super::FrameDescription::black((8, 8));
    description.layers = vec![whole_frame(1, vec![call(key, &[0.0; 4])])];
    description
}

#[test]
fn a_shader_on_the_shelf_is_given_to_the_engine_with_what_it_is_made_of_when_a_frame_asks_for_it() {
    use super::{Engine, EngineRequest, publish_effect_shader_with};
    let key = "test:shelf_with_a_structure";
    let structure = a_buffer_and_two_passes(true);
    publish_effect_shader_with(
        key,
        &the_source(THE_PICTURE_AND_A_BUFFER, A_COLOUR_THROUGH_A_BUFFER),
        structure.clone(),
    );
    let (mut engine, _events, requests) = Engine::scripted_for_tests();

    engine.request_frame(a_frame_with_the_effect(key));

    let sent: Vec<EngineRequest> = requests.try_iter().collect();
    assert!(
        matches!(sent.as_slice(), [EngineRequest::InstallEffect { key: given, structure: told, .. }, EngineRequest::Render { .. }]
            if given == key && **told == structure),
        "the effect, with its structure, and then the frame: {}",
        sent.len()
    );
    // (Given once: a second frame asks for it again and the engine already has it.)
    engine.request_frame(a_frame_with_the_effect(key));
    let sent: Vec<EngineRequest> = requests.try_iter().collect();
    assert!(matches!(sent.as_slice(), [EngineRequest::Render { .. }]));
}

#[test]
fn a_shader_with_no_structure_said_is_given_as_one_pass_that_reads_one_picture() {
    use super::{Engine, EngineRequest, publish_effect_shader};
    let key = "test:shelf_with_no_structure";
    publish_effect_shader(
        key,
        &super::foreign_effect_tests::effect_source(super::foreign_effect_tests::INVERT),
    );
    let (mut engine, _events, requests) = Engine::scripted_for_tests();

    engine.request_frame(a_frame_with_the_effect(key));

    let sent: Vec<EngineRequest> = requests.try_iter().collect();
    assert!(
        matches!(sent.as_slice(), [EngineRequest::InstallEffect { structure, .. }, EngineRequest::Render { .. }]
            if structure.is_one_pass_of_one_picture()),
        "{}",
        sent.len()
    );
}

#[test]
fn a_structure_that_is_not_well_formed_is_refused_by_the_handle_and_nothing_is_sent() {
    use super::Engine;
    let (engine, _events, requests) = Engine::scripted_for_tests();
    let mut two_writers = a_buffer_and_two_passes(false);
    two_writers.passes = vec![a_pass(Some(0)), a_pass(Some(0)), a_pass(None)];
    let source = the_source(THE_PICTURE_AND_A_BUFFER, A_COLOUR_THROUGH_A_BUFFER);

    let refused = engine
        .compile_effect_shader_with_structure("test:handle_two_writers", &source, two_writers)
        .unwrap_err();

    assert!(refused.message.contains("more than one pass"), "{refused:?}");
    assert_eq!(requests.try_iter().count(), 0);
    // The same with a structure that is well formed is sent.
    engine
        .compile_effect_shader_with_structure("test:handle_two_passes", &source, a_buffer_and_two_passes(false))
        .unwrap();
    assert_eq!(requests.try_iter().count(), 1);
}

/// The first pass draws (1, 0, 0.2) in the first buffer and the second (0, 1, 0.4) in the second; the third makes its red from the first buffer, its green from the second and its
/// blue from both.
const READING_TWO_BUFFERS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return vec4<f32>(1.0, 0.0, 0.2, 1.0);
    }
    if numbers[0].w < 1.5 {
        return vec4<f32>(0.0, 1.0, 0.4, 1.0);
    }
    let first = textureLoad(first_buffer, vec2<i32>(position.xy), 0);
    let second = textureLoad(second_buffer, vec2<i32>(position.xy), 0);
    return vec4<f32>(first.r, second.g, first.b + second.b, 1.0);
}
";

#[test]
fn a_pass_reads_each_of_two_buffers_as_the_pass_that_drew_it_left_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let structure = ShaderStructure {
            images: vec![ImageSlot::Picture, ImageSlot::Buffer(0), ImageSlot::Buffer(1)],
            buffers: vec![
                BufferSpec {
                    persistent: false,
                    float: false,
                },
                BufferSpec {
                    persistent: false,
                    float: true,
                },
            ],
            passes: vec![a_pass(Some(0)), a_pass(Some(1)), a_pass(None)],
            pass_index_number: Some(3),
        };
        installed_as(
            &mut renderer,
            "test:two_buffers",
            &the_source(THE_PICTURE_AND_TWO_BUFFERS, READING_TWO_BUFFERS),
            &structure,
        );
        let picture = TestPicture::gradient(8, 8);
        let (frame, _) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:two_buffers", &[0.0; 4])])],
        );
        // Red from the first buffer, green from the second, and blue (0.2 and 0.4 of 255 added: 153).
        let [red, green, blue, opacity] = frame.pixel(3, 3);
        assert!(
            red == 255 && green == 255 && (i32::from(blue) - 153).abs() <= 1 && opacity == 255,
            "{:?}",
            frame.pixel(3, 3)
        );
    });
}

/// Draws a red that is a tenth of the number of the pass in each of the first six passes, and in the last the first buffer, which only the first pass drew, and a half of green.
const SEVEN_PICTURES_AND_THE_FIRST_BUFFER: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 5.5 {
        return vec4<f32>(numbers[0].w / 10.0, 0.0, 0.0, 1.0);
    }
    let held = textureLoad(buffer, vec2<i32>(0, 0), 0);
    return vec4<f32>(held.r, 0.5, 0.0, 1.0);
}
";

#[test]
fn a_shader_may_read_seven_pictures_and_no_more() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The picture and six buffers, drawn by the first six of seven passes; the seventh makes the picture.
        let mut structure = ShaderStructure {
            images: vec![ImageSlot::Picture],
            buffers: vec![
                BufferSpec {
                    persistent: false,
                    float: false
                };
                6
            ],
            passes: (0..6)
                .map(|buffer| a_pass(Some(buffer)))
                .chain([a_pass(None)])
                .collect(),
            pass_index_number: Some(3),
        };
        structure.images.extend((0..6).map(ImageSlot::Buffer));
        assert_eq!(structure.images.len(), 7);
        installed_as(
            &mut renderer,
            "test:seven",
            &the_source(THE_PICTURE_AND_A_BUFFER, SEVEN_PICTURES_AND_THE_FIRST_BUFFER),
            &structure,
        );
        let picture = TestPicture::gradient(8, 8);
        let (frame, statistics) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:seven", &[0.0; 4])])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        assert_eq!(statistics.passes_recorded, 7 + 1 + 1);
        // The first buffer was drawn by pass 0, which makes red 0: the shader shows that, and a half of green.
        assert!(frame.every_pixel_is([0, 128, 0, 255]), "{:?}", frame.pixel(0, 0));

        // An eighth picture is refused.
        structure.images.push(ImageSlot::Picture);
        let refused = renderer
            .install_effect_with_structure(
                "test:eight",
                &guarded(
                    &the_source(THE_PICTURE_AND_A_BUFFER, SEVEN_PICTURES_AND_THE_FIRST_BUFFER),
                    DEFAULT_MOST_PASSES_OF_A_LOOP,
                ),
                &structure,
            )
            .unwrap_err();
        assert!(
            matches!(&refused, RenderProblem::BadEffectShader(why) if why.contains("reads 8 pictures")),
            "{refused}"
        );
    });
}

/// The first pass draws nothing that matters; the second shows how big it is (red: across, green: down, in pixels out of 255) in the second buffer; the third shows that buffer.
const THE_SIZE_OF_A_PASS_IN_THE_MIDDLE: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if numbers[0].w < 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    if numbers[0].w < 1.5 {
        return vec4<f32>(numbers[16].x / 255.0, numbers[16].y / 255.0, 0.0, 1.0);
    }
    return textureLoad(second_buffer, vec2<i32>(0, 0), 0);
}
";

#[test]
fn a_pass_that_is_not_the_last_is_told_how_big_the_buffer_it_draws_is() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let half = |side: SizeExpression| {
            Some(SizeExpression::Binary(
                Operator::Divide,
                Box::new(side),
                Box::new(SizeExpression::Number(2.0)),
            ))
        };
        let quarter = |side: SizeExpression| {
            Some(SizeExpression::Binary(
                Operator::Divide,
                Box::new(side),
                Box::new(SizeExpression::Number(4.0)),
            ))
        };
        let structure = ShaderStructure {
            images: vec![ImageSlot::Picture, ImageSlot::Buffer(0), ImageSlot::Buffer(1)],
            buffers: vec![
                BufferSpec {
                    persistent: false,
                    float: false,
                };
                2
            ],
            passes: vec![
                PassSpec {
                    target: Some(0),
                    width: quarter(SizeExpression::Width),
                    height: quarter(SizeExpression::Height),
                },
                PassSpec {
                    target: Some(1),
                    width: half(SizeExpression::Width),
                    height: half(SizeExpression::Height),
                },
                a_pass(None),
            ],
            pass_index_number: Some(3),
        };
        installed_as(
            &mut renderer,
            "test:middle",
            &the_source(THE_PICTURE_AND_TWO_BUFFERS, THE_SIZE_OF_A_PASS_IN_THE_MIDDLE),
            &structure,
        );
        let picture = TestPicture::gradient(64, 36);
        let (frame, _) = frame_of(
            &mut renderer,
            (64, 36),
            &[(1, &picture)],
            vec![whole_frame(1, vec![call("test:middle", &[0.0; 4])])],
        );
        // The second buffer is 32 by 18, whatever the first is, and the frame is not.
        assert!(frame.every_pixel_is([32, 18, 0, 255]), "{:?}", frame.pixel(0, 0));
    });
}
