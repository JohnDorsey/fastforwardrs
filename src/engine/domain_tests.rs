// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour domains of the effects (phase 4, piece 4.2; the design is `docs/engine-phase-4-design.md`, F3 and F4), on the software adapter: the colour matrix with
//! offsets, and the effects that someone else wrote being given the codes of a screen when the engine works in light, with the conversions made once between neighbours.
//!
//! The compatibility mode (`WorkingSpace::DisplayReferred`), where the numbers already are codes, is what the effects of the display-referred domain are held to: they have to
//! make the same pictures, to within the rounding of half floats, in light.

use std::time::Duration;

use super::effect_tests::matrix_effect;
use super::foreign_effect_tests::{call, effect_source, installed};
use super::linear_light_tests::{colour_of, picture_for, the_whole_frame, upload};
use super::oracle_tests::compare;
use super::testing::with_software_graphics;
use super::{
    EffectCall, FrameDescription, FrameRenderer, FrameStatistics, LayerDescription, RenderedFrame, WorkingSpace,
};
use crate::colour_effects::ColourMatrix;
use crate::colour_space::StandardPrimaries;

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

/// Makes every colour of the picture half of what it is *in the numbers it is given*, and leaves how opaque it is.
const HALVE: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let premultiplied = textureLoad(picture, vec2<i32>(position.xy), 0);
    let opacity = premultiplied.a;
    var colour = vec3<f32>(0.0);
    if opacity > 0.0 {
        colour = premultiplied.rgb / opacity;
    }
    return vec4<f32>(colour * 0.5 * opacity, opacity);
}
";

fn render(
    renderer: &mut FrameRenderer,
    size: (u32, u32),
    working: WorkingSpace,
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, FrameStatistics) {
    let mut description = FrameDescription::black(size);
    description.colour = colour_of(working);
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap()
}

/// The frame of `layer` over a flat picture of `colour`, made in `working`.
fn flat_frame(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    colour: [u8; 4],
    effects: Vec<EffectCall>,
) -> (RenderedFrame, FrameStatistics) {
    upload(renderer, 1, &picture_for(working, 4, 4, colour.repeat(16)));
    render(
        renderer,
        (4, 4),
        working,
        vec![LayerDescription {
            effects,
            ..the_whole_frame(1)
        }],
    )
}

fn with_offsets(matrix: ColourMatrix, offsets: [f32; 3]) -> EffectCall {
    let mut call = matrix_effect(matrix);
    call.values.extend(offsets);
    call
}

fn the_largest_difference(a: &RenderedFrame, b: &RenderedFrame) -> u8 {
    compare(&a.rgba_pixels, &b.rgba_pixels).largest
}

#[test]
fn a_colour_matrix_with_offsets_adds_them_to_the_mixed_colours_in_codes_and_cuts_the_sum_off() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 100, 150 and 200 of 255, with a tenth of a step more red, nothing more green and a tenth less blue: 100 + 25.5, 150 and 200 - 25.5.
        let (moved, statistics) = flat_frame(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            [100, 150, 200, 255],
            vec![with_offsets(ColourMatrix::NOTHING_CHANGES, [0.1, 0.0, -0.1])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        let [red, green, blue, opacity] = moved.pixel(1, 1);
        assert!(
            red.abs_diff(126) <= 1 && green == 150 && blue.abs_diff(174) <= 1 && opacity == 255,
            "{:?}",
            moved.pixel(1, 1)
        );
        // The sum is cut off at both ends, after the mixing: 250 and 10 move by 51 either way, to 255 and 0.
        let (cut, _) = flat_frame(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            [250, 10, 100, 255],
            vec![with_offsets(ColourMatrix::NOTHING_CHANGES, [0.2, -0.2, 0.0])],
        );
        assert_eq!(cut.pixel(0, 0), [255, 0, 100, 255]);
        // The offsets come after the matrix: half of 200 is 100, and 0.2 of a step more is 151.
        let halved = ColourMatrix::from_rows([[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]]);
        let (mixed_then_moved, _) = flat_frame(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            [200, 200, 200, 255],
            vec![with_offsets(halved, [0.2, 0.2, 0.2])],
        );
        assert!(
            mixed_then_moved.pixel(0, 0)[..3]
                .iter()
                .all(|colour| colour.abs_diff(151) <= 1),
            "{:?}",
            mixed_then_moved.pixel(0, 0)
        );
    });
}

#[test]
fn a_colour_matrix_with_offsets_makes_the_same_colours_in_light_as_in_codes_and_one_with_nine_numbers_is_what_it_was() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for colour in [[180, 90, 45, 255], [20, 200, 240, 255], [128, 128, 128, 255]] {
            let moved = with_offsets(ColourMatrix::hue_rotation(70.0), [0.08, -0.05, 0.02]);
            let nine = matrix_effect(ColourMatrix::hue_rotation(70.0));
            for effect in [moved, nine] {
                let (in_codes, _) = flat_frame(
                    &mut renderer,
                    WorkingSpace::DisplayReferred,
                    colour,
                    vec![effect.clone()],
                );
                let (in_light, _) = flat_frame(&mut renderer, LINEAR_REC709, colour, vec![effect.clone()]);
                assert!(
                    the_largest_difference(&in_light, &in_codes) <= 1,
                    "{colour:?}, {} numbers: {:?} where it was {:?}",
                    effect.values.len(),
                    in_light.pixel(0, 0),
                    in_codes.pixel(0, 0)
                );
            }
        }
    });
}

#[test]
fn an_effect_that_a_shader_runs_is_given_the_codes_of_a_screen_in_light_and_makes_what_it_makes_in_the_compatibility_mode()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:halve", &effect_source(HALVE));
        for colour in [[200, 120, 60, 255], [255, 255, 255, 255], [30, 90, 250, 255]] {
            let (in_codes, in_codes_statistics) = flat_frame(
                &mut renderer,
                WorkingSpace::DisplayReferred,
                colour,
                vec![call("test:halve", &[])],
            );
            let (in_light, in_light_statistics) =
                flat_frame(&mut renderer, LINEAR_REC709, colour, vec![call("test:halve", &[])]);
            // (Half of the codes. Half of the light would be much brighter: a code of 200 is 58 of 100 of the light, and half of that is the code 146.)
            let halved = |code: u8| code.div_ceil(2);
            assert!(
                (0..3).all(|number| in_codes.pixel(0, 0)[number].abs_diff(halved(colour[number])) <= 1),
                "{colour:?}: {:?}",
                in_codes.pixel(0, 0)
            );
            assert!(
                the_largest_difference(&in_light, &in_codes) <= 1,
                "{colour:?}: {:?} where the codes made {:?}",
                in_light.pixel(0, 0),
                in_codes.pixel(0, 0)
            );
            assert_eq!(
                in_codes_statistics.passes_recorded,
                2 + 1,
                "nothing is converted in the compatibility mode"
            );
            assert_eq!(
                in_light_statistics.passes_recorded,
                2 + 3,
                "the pass that converts into the codes, the effect, and the pass that converts out of them"
            );
        }
    });
}

#[test]
fn neighbours_of_the_display_referred_domain_are_converted_into_it_once_and_out_of_it_once() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:halve", &effect_source(HALVE));
        let turn = || matrix_effect(ColourMatrix::hue_rotation(40.0));
        let darker = || {
            with_offsets(
                ColourMatrix::from_rows([[0.9, 0.0, 0.0], [0.0, 0.9, 0.0], [0.0, 0.0, 0.9]]),
                [0.0, 0.0, 0.0],
            )
        };
        let colour = [180, 90, 45, 255];
        // A matrix, a shader and a matrix: the first matrix converts into the codes, the last converts out of them, and nobody converts between.
        let effects = || vec![turn(), call("test:halve", &[]), darker()];
        let (in_codes, in_codes_statistics) =
            flat_frame(&mut renderer, WorkingSpace::DisplayReferred, colour, effects());
        let (in_light, in_light_statistics) = flat_frame(&mut renderer, LINEAR_REC709, colour, effects());
        assert_eq!(in_codes_statistics.passes_recorded, 2 + 3);
        assert_eq!(
            in_light_statistics.passes_recorded,
            2 + 3,
            "three effects, and no pass for a conversion: the matrices do it"
        );
        assert!(
            the_largest_difference(&in_light, &in_codes) <= 2,
            "{:?} where the codes made {:?}",
            in_light.pixel(0, 0),
            in_codes.pixel(0, 0)
        );
        // A shader on its own needs the two passes; a matrix and a shader need one; and the same two the other way round.
        let (_, shader_alone) = flat_frame(&mut renderer, LINEAR_REC709, colour, vec![call("test:halve", &[])]);
        assert_eq!(shader_alone.passes_recorded, 2 + 1 + 2);
        let (_, matrix_then_shader) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            colour,
            vec![turn(), call("test:halve", &[])],
        );
        assert_eq!(matrix_then_shader.passes_recorded, 2 + 2 + 1);
        let (_, shader_then_matrix) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            colour,
            vec![call("test:halve", &[]), turn()],
        );
        assert_eq!(shader_then_matrix.passes_recorded, 2 + 2 + 1);
        // The two kinds of neighbour make the same pictures in light as in codes, in any order.
        for effects in [
            vec![call("test:halve", &[]), turn()],
            vec![turn(), call("test:halve", &[])],
            vec![call("test:halve", &[]), call("test:halve", &[])],
        ] {
            let (in_codes, _) = flat_frame(&mut renderer, WorkingSpace::DisplayReferred, colour, effects.clone());
            let (in_light, _) = flat_frame(&mut renderer, LINEAR_REC709, colour, effects.clone());
            assert!(
                the_largest_difference(&in_light, &in_codes) <= 2,
                "{effects:?}: {:?} where the codes made {:?}",
                in_light.pixel(0, 0),
                in_codes.pixel(0, 0)
            );
        }
    });
}

#[test]
fn the_conversions_round_an_effect_that_changes_nothing_change_nothing_either() {
    with_software_graphics!(graphics, {
        // The light goes into the codes and out of them again by the passes that only convert, and comes back as it was (to within the half floats the codes are kept in).
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed(&mut renderer, "test:identity", &effect_source(IDENTITY));
        for colour in [
            [100, 150, 200, 255],
            [1, 2, 3, 255],
            [254, 128, 7, 255],
            [90, 60, 30, 128],
        ] {
            let (plain, _) = flat_frame(&mut renderer, LINEAR_REC709, colour, Vec::new());
            let (converted, _) = flat_frame(&mut renderer, LINEAR_REC709, colour, vec![call("test:identity", &[])]);
            assert!(
                the_largest_difference(&plain, &converted) <= 1,
                "{colour:?}: {:?} where it was {:?}",
                converted.pixel(0, 0),
                plain.pixel(0, 0)
            );
        }
    });
}

/// Gives the picture back as it is.
const IDENTITY: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(picture, vec2<i32>(position.xy), 0);
}
";
