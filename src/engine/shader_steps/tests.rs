// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::engine::shader_structure::{BufferSpec, ImageSlot, Operator, PassSpec, SizeExpression};
use crate::engine::{BLUR_KEY, COLOUR_MATRIX_KEY};

fn base_shape() -> TextureShape {
    TextureShape {
        width_in_pixels: 640,
        height_in_pixels: 360,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    }
}

/// The picture that the layers are put together in: not the size of the picture that the effects work on, so that a test can tell which of the two a step draws.
fn frame_shape() -> TextureShape {
    TextureShape {
        width_in_pixels: 1280,
        height_in_pixels: 720,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    }
}

fn call(key: &str, values: Vec<f32>) -> EffectCall {
    EffectCall {
        key: key.to_owned(),
        values,
        table: None,
        state: None,
        images: Vec::new(),
    }
}

fn first(call: usize) -> EffectStep {
    EffectStep::Call {
        call,
        converts_into_display: false,
        converts_out_of_display: false,
    }
}

fn later(call: usize, pass: usize) -> EffectStep {
    EffectStep::LaterPass { call, pass }
}

/// The two-pass blur: the picture and a buffer a quarter as wide and as high as the picture (not float); the first pass draws the buffer.
fn quarter_buffer_blur() -> Arc<ShaderStructure> {
    let quarter = |side: SizeExpression| {
        Some(SizeExpression::Binary(
            Operator::Divide,
            Box::new(side),
            Box::new(SizeExpression::Number(4.0)),
        ))
    };
    Arc::new(ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Buffer(0)],
        buffers: vec![BufferSpec {
            persistent: false,
            float: false,
        }],
        passes: vec![
            PassSpec {
                target: Some(0),
                width: quarter(SizeExpression::Width),
                height: quarter(SizeExpression::Height),
            },
            PassSpec {
                target: None,
                width: None,
                height: None,
            },
        ],
        pass_index_number: Some(3),
    })
}

fn only_the_blur(key: &str) -> Option<Arc<ShaderStructure>> {
    (key == "test:blur").then(quarter_buffer_blur)
}

#[test]
fn a_step_that_is_not_a_pass_of_a_shader_draws_a_picture_of_the_size_of_the_one_the_effects_work_on() {
    let effects = [call(COLOUR_MATRIX_KEY, vec![]), call(BLUR_KEY, vec![3.0])];
    let steps = [
        first(0),
        first(1),
        later(1, 1),
        EffectStep::Conversion { into_display: false },
    ];
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_blur,
        base_shape(),
        (640, 360),
        frame_shape(),
        4096,
    );
    assert_eq!(plan.len(), 4);
    for step in &plan {
        assert_eq!(
            *step,
            StepPlan {
                shape: base_shape(),
                size: (640, 360),
                shader: None
            }
        );
    }
}

#[test]
fn the_steps_of_a_two_pass_shader_draw_the_buffer_and_then_the_picture_and_say_what_they_read() {
    let effects = [call("test:blur", vec![0.0, 0.0, 0.0, 0.0])];
    let steps = [first(0), later(0, 1)];
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_blur,
        base_shape(),
        (640, 360),
        frame_shape(),
        4096,
    );

    // The first pass draws the buffer: a quarter of the picture, 8 bits of each colour.
    assert_eq!(plan[0].size, (160, 90));
    assert_eq!(
        plan[0].shape,
        TextureShape {
            width_in_pixels: 160,
            height_in_pixels: 90,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: base_shape().usage,
        }
    );
    assert_eq!(
        plan[0].shader,
        Some(ShaderStep {
            pass: 0,
            first_step: 0,
            backdrop_step: None,
            images: 2,
            target_format: TargetFormat::EightBit,
            target_buffer: Some(0),
            reads: vec![ImageSource::Picture, ImageSource::Clear],
        })
    );

    // The second draws the picture the effect makes, as big as the picture it works on and in the format of pictures, and reads the buffer the first drew.
    assert_eq!(plan[1].size, (640, 360));
    assert_eq!(plan[1].shape, base_shape());
    assert_eq!(
        plan[1].shader,
        Some(ShaderStep {
            pass: 1,
            first_step: 0,
            backdrop_step: None,
            images: 2,
            target_format: TargetFormat::Half,
            target_buffer: None,
            reads: vec![ImageSource::Picture, ImageSource::DrawnByPass(0)],
        })
    );
}

#[test]
fn the_passes_of_a_shader_know_where_the_first_of_their_own_call_is_among_the_steps_of_the_layer() {
    // A colour matrix, then the shader of two passes, then another effect, then the shader again (a second call of it, with a picture of another size).
    let effects = [
        call(COLOUR_MATRIX_KEY, vec![]),
        call("test:blur", vec![0.0; 4]),
        call(BLUR_KEY, vec![1.0]),
        call("test:blur", vec![0.0; 4]),
    ];
    let steps = [
        first(0),
        first(1),
        later(1, 1),
        first(2),
        later(2, 1),
        first(3),
        later(3, 1),
    ];
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_blur,
        base_shape(),
        (640, 360),
        frame_shape(),
        4096,
    );
    let firsts: Vec<Option<usize>> = plan
        .iter()
        .map(|step| step.shader.as_ref().map(|shader| shader.first_step))
        .collect();
    assert_eq!(firsts, vec![None, Some(1), Some(1), None, None, Some(5), Some(5)]);
    let passes: Vec<Option<usize>> = plan
        .iter()
        .map(|step| step.shader.as_ref().map(|shader| shader.pass))
        .collect();
    assert_eq!(passes, vec![None, Some(0), Some(1), None, None, Some(0), Some(1)]);
}

#[test]
fn the_size_of_a_buffer_follows_the_picture_the_effect_works_on_and_is_kept_within_the_card() {
    let effects = [call("test:blur", vec![0.0; 4])];
    let steps = [first(0), later(0, 1)];
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_blur,
        base_shape(),
        (1920, 1080),
        frame_shape(),
        4096,
    );
    assert_eq!(plan[0].size, (480, 270));
    assert_eq!(plan[1].size, (1920, 1080));
    // Where the card allows less, the buffer is kept within it.
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_blur,
        base_shape(),
        (1920, 1080),
        frame_shape(),
        100,
    );
    assert_eq!(plan[0].size, (100, 100));
}

#[test]
fn a_shader_with_no_structure_to_find_is_a_step_like_any_other() {
    let effects = [call("test:unknown", vec![1.0])];
    let plan = plan_the_steps(
        &[first(0)],
        &effects,
        |_| None,
        base_shape(),
        (64, 36),
        frame_shape(),
        4096,
    );
    assert_eq!(plan[0].shader, None);
    assert_eq!(plan[0].size, (64, 36));
}

/// A transition: it reads what is behind the layer and the layer's own picture, in one pass.
fn a_transition() -> Arc<ShaderStructure> {
    Arc::new(ShaderStructure {
        images: vec![ImageSlot::Backdrop, ImageSlot::Picture],
        buffers: Vec::new(),
        passes: vec![PassSpec {
            target: None,
            width: None,
            height: None,
        }],
        pass_index_number: Some(3),
    })
}

fn only_the_transition(key: &str) -> Option<Arc<ShaderStructure>> {
    (key == "test:transition").then(a_transition)
}

fn the_backdrop(call: usize) -> EffectStep {
    EffectStep::Backdrop { call }
}

#[test]
fn the_step_that_makes_the_backdrop_draws_a_picture_the_size_of_the_frame_and_the_call_that_reads_it_says_which_step_that_is()
 {
    let effects = [call(COLOUR_MATRIX_KEY, vec![]), call("test:transition", vec![0.0; 4])];
    let steps = [first(0), the_backdrop(1), first(1)];
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_transition,
        base_shape(),
        (640, 360),
        frame_shape(),
        4096,
    );
    assert_eq!(
        plan[1],
        StepPlan {
            shape: frame_shape(),
            size: (1280, 720),
            shader: None
        },
        "the backdrop is as big as the frame, and not as big as the picture the effects work on"
    );
    assert_eq!(
        plan[2].size,
        (640, 360),
        "the effect itself draws the picture of the layer"
    );
    assert_eq!(plan[2].shape, base_shape());
    assert_eq!(
        plan[2].shader,
        Some(ShaderStep {
            pass: 0,
            first_step: 2,
            backdrop_step: Some(1),
            images: 2,
            target_format: TargetFormat::Half,
            target_buffer: None,
            reads: vec![ImageSource::Backdrop, ImageSource::Picture],
        })
    );
}

#[test]
fn every_pass_of_a_call_that_reads_the_backdrop_has_the_one_step_that_made_it_and_a_call_without_one_has_none() {
    // A shader of two passes that reads the backdrop (the blur's structure with the backdrop as its first picture), then a transition of one pass that has no such step before it.
    let two_passes = Arc::new(ShaderStructure {
        images: vec![ImageSlot::Backdrop, ImageSlot::Picture, ImageSlot::Buffer(0)],
        buffers: vec![BufferSpec {
            persistent: false,
            float: true,
        }],
        passes: vec![
            PassSpec {
                target: Some(0),
                width: None,
                height: None,
            },
            PassSpec {
                target: None,
                width: None,
                height: None,
            },
        ],
        pass_index_number: Some(3),
    });
    let structure_of = |key: &str| match key {
        "test:two" => Some(Arc::clone(&two_passes)),
        "test:transition" => Some(a_transition()),
        _ => None,
    };
    let effects = [call("test:two", vec![0.0; 4]), call("test:transition", vec![0.0; 4])];
    let steps = [the_backdrop(0), first(0), later(0, 1), first(1)];
    let plan = plan_the_steps(
        &steps,
        &effects,
        structure_of,
        base_shape(),
        (640, 360),
        frame_shape(),
        4096,
    );
    let backdrops: Vec<Option<usize>> = plan
        .iter()
        .map(|step| step.shader.as_ref().and_then(|shader| shader.backdrop_step))
        .collect();
    assert_eq!(backdrops, vec![None, Some(0), Some(0), None]);
    assert_eq!(
        plan[1].shader.as_ref().map(|shader| shader.reads.clone()),
        Some(vec![ImageSource::Backdrop, ImageSource::Picture, ImageSource::Clear])
    );
    assert_eq!(
        plan[2].shader.as_ref().map(|shader| shader.reads.clone()),
        Some(vec![
            ImageSource::Backdrop,
            ImageSource::Picture,
            ImageSource::DrawnByPass(0)
        ])
    );
}

#[test]
fn two_calls_that_read_the_backdrop_each_say_their_own_step() {
    let effects = [
        call("test:transition", vec![0.0; 4]),
        call("test:transition", vec![0.0; 4]),
    ];
    let steps = [the_backdrop(0), first(0), the_backdrop(1), first(1)];
    let plan = plan_the_steps(
        &steps,
        &effects,
        only_the_transition,
        base_shape(),
        (640, 360),
        frame_shape(),
        4096,
    );
    let backdrops: Vec<Option<usize>> = plan
        .iter()
        .map(|step| step.shader.as_ref().and_then(|shader| shader.backdrop_step))
        .collect();
    assert_eq!(backdrops, vec![None, Some(0), None, Some(2)]);
}
