// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Buffers that last from one frame to the next (phase 6.4.2), made on the software adapter: a shader that adds to what it kept makes a picture that depends on the frames before,
//! the same frame made again is the same picture, a frame that is not the next starts clear, and two states do not meet.

use std::time::Duration;

use super::effect_tests::{frame_of, render, whole_frame};
use super::foreign_effect_tests::{call, guarded};
use super::oracle_tests::TestPicture;
use super::testing::with_software_graphics;
use super::{
    BufferSpec, DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, EffectState, FrameDescription, FrameOutput, FrameRenderer,
    ImageSlot, LayerDescription, PassSpec, ShaderStructure,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

pub(super) const THE_PICTURE_AND_A_BUFFER: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@group(0) @binding(3) var buffer: texture_2d<f32>;
@group(0) @binding(4) var buffer_sampler: sampler;
";

const THE_PICTURE_AND_TWO_BUFFERS: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
@group(0) @binding(3) var first_buffer: texture_2d<f32>;
@group(0) @binding(4) var first_buffer_sampler: sampler;
@group(0) @binding(5) var second_buffer: texture_2d<f32>;
@group(0) @binding(6) var second_buffer_sampler: sampler;
";

/// The first pass adds a tenth of red to what the buffer held when the frame before ended; the second shows the buffer.
pub(super) const ADD_A_TENTH_OF_RED: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let held = textureLoad(buffer, vec2<i32>(position.xy), 0);
    if numbers[0].w < 0.5 {
        return vec4<f32>(held.r + 0.1, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(held.r, 0.0, 0.0, 1.0);
}
";

/// Two buffers that last: the first gets a tenth of red added in each frame, the second a fifth of green; the third pass shows both.
const ADD_TO_TWO_BUFFERS: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let first = textureLoad(first_buffer, vec2<i32>(position.xy), 0);
    let second = textureLoad(second_buffer, vec2<i32>(position.xy), 0);
    if numbers[0].w < 0.5 {
        return vec4<f32>(first.r + 0.1, 0.0, 0.0, 1.0);
    }
    if numbers[0].w < 1.5 {
        return vec4<f32>(0.0, second.g + 0.2, 0.0, 1.0);
    }
    return vec4<f32>(first.r, second.g, 0.0, 1.0);
}
";

fn pass(target: Option<usize>) -> PassSpec {
    PassSpec {
        target,
        width: None,
        height: None,
    }
}

/// The picture, and one buffer that lasts (half floats), drawn by the first of two passes.
pub(super) fn one_buffer_that_lasts() -> ShaderStructure {
    ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Buffer(0)],
        buffers: vec![BufferSpec {
            persistent: true,
            float: true,
        }],
        passes: vec![pass(Some(0)), pass(None)],
        pass_index_number: Some(3),
    }
}

fn two_buffers_that_last() -> ShaderStructure {
    ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Buffer(0), ImageSlot::Buffer(1)],
        buffers: vec![
            BufferSpec {
                persistent: true,
                float: true
            };
            2
        ],
        passes: vec![pass(Some(0)), pass(Some(1)), pass(None)],
        pass_index_number: Some(3),
    }
}

fn installed_as(renderer: &mut FrameRenderer, key: &str, header: &str, stage: &str, structure: &ShaderStructure) {
    renderer
        .install_effect_with_structure(
            key,
            &guarded(&format!("{header}\n{stage}"), DEFAULT_MOST_PASSES_OF_A_LOOP),
            structure,
        )
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
}

/// A call of `key` that goes on with the state `id`, at frame `frame`.
pub(super) fn stateful(key: &str, id: u64, frame: u64) -> EffectCall {
    EffectCall {
        state: Some(EffectState { id, frame }),
        ..call(key, &[0.0; 4])
    }
}

/// The red of the middle of the frame that `renderer` makes of the picture 1 with the effects `effects`.
fn red_of(renderer: &mut FrameRenderer, effects: Vec<EffectCall>) -> i32 {
    let (frame, _) = render(renderer, (8, 8), vec![whole_frame(1, effects)]);
    i32::from(frame.pixel(4, 4)[0])
}

fn renderer_with_the_picture(graphics: &super::testing::SoftwareGraphics) -> FrameRenderer {
    let mut renderer = FrameRenderer::new(graphics.device.clone());
    renderer
        .upload_picture(super::PictureId(1), &TestPicture::gradient(8, 8).as_picture_data())
        .unwrap();
    renderer
}

/// What a tenth of red added `frames` times comes to as a level of 255.
pub(super) fn tenths(frames: i32) -> i32 {
    (25.5 * frames as f32).round() as i32
}

pub(super) fn near(found: i32, wanted: i32) -> bool {
    (found - wanted).abs() <= 1
}

#[test]
fn frames_one_after_another_go_on_from_each_other_and_the_picture_depends_on_the_frames_before() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        for frame in 0..6 {
            let red = red_of(&mut renderer, vec![stateful("test:accumulate", 1, frame)]);
            assert!(
                near(red, tenths(frame as i32 + 1)),
                "frame {frame}: {red} and not {}",
                tenths(frame as i32 + 1)
            );
        }
        // Two textures for the one buffer, 8 by 8, half floats.
        assert_eq!(renderer.effect_states_kept(), (1, 2 * 8 * 8 * 8));
    });
}

#[test]
fn the_same_frame_made_again_is_the_same_picture_and_does_not_advance_the_state() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        let mut seen = Vec::new();
        for frame in [0, 1, 1, 1, 2, 2, 3] {
            seen.push(red_of(&mut renderer, vec![stateful("test:accumulate", 1, frame)]));
        }
        let wanted = [1, 2, 2, 2, 3, 3, 4].map(tenths);
        for (found, wanted) in seen.iter().zip(wanted) {
            assert!(near(*found, wanted), "{seen:?} and not {wanted:?}");
        }
    });
}

#[test]
fn a_frame_that_is_not_the_next_or_the_same_starts_clear_and_the_frames_after_it_go_on_from_there() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        let mut seen = Vec::new();
        // The first three are in order; then a frame skipped (5), the next (6), a step back to the first frame (0), and on (1).
        for frame in [0, 1, 2, 5, 6, 0, 1, 40] {
            seen.push(red_of(&mut renderer, vec![stateful("test:accumulate", 1, frame)]));
        }
        let wanted = [1, 2, 3, 1, 2, 1, 2, 1].map(tenths);
        for (found, wanted) in seen.iter().zip(wanted) {
            assert!(near(*found, wanted), "{seen:?} and not {wanted:?}");
        }
    });
}

#[test]
fn two_states_do_not_meet_and_a_call_with_no_state_keeps_nothing() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        // The frames of two effects are made in turn.
        let mut first = Vec::new();
        let mut second = Vec::new();
        for frame in 0..4 {
            first.push(red_of(&mut renderer, vec![stateful("test:accumulate", 10, frame)]));
            if frame >= 1 {
                // (The second begins a frame late: it is on frame 0 when the first is on frame 1.)
                second.push(red_of(&mut renderer, vec![stateful("test:accumulate", 20, frame - 1)]));
            }
        }
        for (frame, found) in first.iter().enumerate() {
            assert!(near(*found, tenths(frame as i32 + 1)), "first {first:?}");
        }
        for (frame, found) in second.iter().enumerate() {
            assert!(near(*found, tenths(frame as i32 + 1)), "second {second:?}");
        }
        assert_eq!(renderer.effect_states_kept().0, 2);

        // A call with no state is read as having kept nothing, in every frame.
        for _ in 0..3 {
            let red = red_of(&mut renderer, vec![call("test:accumulate", &[0.0; 4])]);
            assert!(near(red, tenths(1)), "no state: {red}");
        }
        assert_eq!(renderer.effect_states_kept().0, 2, "and it makes none");
    });
}

#[test]
fn a_shader_with_two_buffers_that_last_keeps_each_by_itself() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:two",
            THE_PICTURE_AND_TWO_BUFFERS,
            ADD_TO_TWO_BUFFERS,
            &two_buffers_that_last(),
        );
        for frame in 0..4_i32 {
            let (picture, _) = render(
                &mut renderer,
                (8, 8),
                vec![whole_frame(1, vec![stateful("test:two", 5, frame as u64)])],
            );
            let [red, green, blue, _] = picture.pixel(4, 4);
            assert!(near(i32::from(red), tenths(frame + 1)), "frame {frame}: red {red}");
            // (A fifth of green, added `frame + 1` times.)
            assert!(
                near(i32::from(green), 2 * tenths(frame + 1)),
                "frame {frame}: green {green}"
            );
            assert_eq!(blue, 0);
        }
        assert_eq!(renderer.effect_states_kept(), (1, 2 * 2 * 8 * 8 * 8));
    });
}

#[test]
fn frames_that_are_begun_before_the_one_before_them_is_finished_go_on_from_each_other_all_the_same() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        // (As the export makes them: the card is given the next while the pixels of the one before are taken.)
        let description_of = |frame: u64| {
            let mut description = FrameDescription::black((8, 8));
            description.layers = vec![whole_frame(1, vec![stateful("test:accumulate", 1, frame)])];
            description
        };
        let begun: Vec<_> = (0..4)
            .map(|frame| {
                renderer
                    .begin_frame(&description_of(frame), FrameOutput::Rgba8, A_LONG_TIME)
                    .unwrap()
            })
            .collect();
        for (frame, pending) in begun.into_iter().enumerate() {
            let (finished, _) = renderer.finish_frame(pending).unwrap();
            let red = i32::from(finished.into_rgba8().pixel(4, 4)[0]);
            assert!(near(red, tenths(frame as i32 + 1)), "frame {frame}: {red}");
        }
    });
}

#[test]
fn a_persistent_effect_on_a_picture_of_another_size_or_on_a_group_keeps_a_state_as_big_as_its_picture() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        let picture = TestPicture::gradient(16, 4);
        let layer: LayerDescription = whole_frame(7, vec![stateful("test:accumulate", 1, 0)]);
        let (frame, _) = frame_of(&mut renderer, (16, 4), &[(7, &picture)], vec![layer]);
        assert!(near(i32::from(frame.pixel(3, 1)[0]), tenths(1)));
        assert_eq!(renderer.effect_states_kept(), (1, 2 * 16 * 4 * 8));
    });
}

#[test]
fn a_state_that_two_calls_of_one_frame_have_is_kept_by_the_first_and_the_second_goes_without() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        // Two calls that go on with one state in one frame, one after the other.
        let twice = |frame: u64| {
            vec![
                stateful("test:accumulate", 1, frame),
                stateful("test:accumulate", 1, frame),
            ]
        };
        let (frame, _) = render(&mut renderer, (8, 8), vec![whole_frame(1, twice(0))]);
        assert!(
            near(i32::from(frame.pixel(4, 4)[0]), tenths(1)),
            "{:?}",
            frame.pixel(4, 4)
        );
        assert_eq!(renderer.effect_states_kept().0, 1, "one state, whoever has it");
        // (The first goes on from the frame before as it should.)
        let (next, _) = render(&mut renderer, (8, 8), vec![whole_frame(1, twice(1))]);
        assert!(
            near(i32::from(next.pixel(4, 4)[0]), tenths(1)),
            "the second call reads nothing, and is what is seen"
        );
        let (alone, _) = render(
            &mut renderer,
            (8, 8),
            vec![whole_frame(1, vec![stateful("test:accumulate", 1, 2)])],
        );
        assert!(
            near(i32::from(alone.pixel(4, 4)[0]), tenths(3)),
            "{:?}",
            alone.pixel(4, 4)
        );
    });
}

#[test]
fn a_state_that_no_frame_asks_for_in_a_long_time_is_let_go_of_and_one_that_is_asked_for_is_not() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        installed_as(
            &mut renderer,
            "test:accumulate",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &one_buffer_that_lasts(),
        );
        for id in [1, 2] {
            red_of(&mut renderer, vec![stateful("test:accumulate", id, 0)]);
        }
        assert_eq!(renderer.effect_states_kept().0, 2);
        // The frames that follow use the second state only, and then none.
        for frame in 1..=super::effect_states::FRAMES_BEFORE_A_STATE_IS_LET_GO_OF + 1 {
            red_of(&mut renderer, vec![stateful("test:accumulate", 2, frame)]);
        }
        assert_eq!(
            renderer.effect_states_kept().0,
            1,
            "the first is not used and is let go of"
        );
        let red = red_of(
            &mut renderer,
            vec![stateful(
                "test:accumulate",
                2,
                super::effect_states::FRAMES_BEFORE_A_STATE_IS_LET_GO_OF + 2,
            )],
        );
        assert!(
            red >= 250,
            "the second has gone on all the time, and its red is all there is by now: {red}"
        );
    });
}

#[test]
fn a_call_that_has_a_state_but_no_buffer_that_lasts_keeps_nothing() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_picture(&graphics);
        // (The shader of two passes with a buffer that does not last, with a state that nobody needed given to it.)
        let mut forgets = one_buffer_that_lasts();
        forgets.buffers[0].persistent = false;
        installed_as(
            &mut renderer,
            "test:forgets",
            THE_PICTURE_AND_A_BUFFER,
            ADD_A_TENTH_OF_RED,
            &forgets,
        );
        for frame in 0..3 {
            red_of(&mut renderer, vec![stateful("test:forgets", 1, frame)]);
        }
        assert_eq!(renderer.effect_states_kept(), (0, 0));
    });
}
