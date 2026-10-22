// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's effects of exposure and of opacity (phase 4, piece 4.3), on the software adapter: each against its arithmetic, in the light of the working space where they
//! are made for, and in the plan that runs them.

use std::time::Duration;

use super::linear_light_tests::{colour_of, picture_for, the_whole_frame, upload};
use super::testing::with_software_graphics;
use super::{
    EffectCall, FrameDescription, FrameRenderer, FrameStatistics, LayerDescription, RenderedFrame, WorkingSpace,
};
use crate::colour_effects::ColourMatrix;
use crate::colour_space::{StandardPrimaries, TransferFunction};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

fn render(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, FrameStatistics) {
    let mut description = FrameDescription::black((4, 4));
    description.colour = colour_of(working);
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap()
}

/// The frame of a flat picture of `colour` with `effects` done to it, over black.
fn flat_frame(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    colour: [u8; 4],
    effects: Vec<EffectCall>,
) -> (RenderedFrame, FrameStatistics) {
    upload(renderer, 1, &picture_for(working, 4, 4, colour.repeat(16)));
    render(
        renderer,
        working,
        vec![LayerDescription {
            effects,
            ..the_whole_frame(1)
        }],
    )
}

/// The code of a sRGB colour (as a whole number of 255) after its light is multiplied by `gain`, worked out the plain way and cut off at white.
fn code_after_light_is_multiplied_by(code: u8, gain: f64) -> f64 {
    let light = TransferFunction::Srgb.decode_to_light(f64::from(code) / 255.0);
    TransferFunction::Srgb.encode_from_light((light * gain).min(1.0)) * 255.0
}

fn assert_near(made: [u8; 4], wanted: [f64; 3], opacity: u8, because: &str) {
    for place in 0..3 {
        assert!(
            (f64::from(made[place]) - wanted[place]).abs() <= 1.0,
            "{because}: {made:?} where {wanted:?} was wanted"
        );
    }
    assert_eq!(made[3], opacity, "{because}: {made:?}");
}

#[test]
fn an_exposure_multiplies_the_light_by_two_to_the_stops_and_leaves_the_opacity_alone() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        for colour in [[100, 60, 20, 255], [200, 180, 10, 255], [8, 8, 8, 255]] {
            for stops in [-3.0_f32, -1.0, -0.5, 0.5, 1.0, 2.0] {
                let (made, statistics) =
                    flat_frame(&mut renderer, LINEAR_REC709, colour, vec![EffectCall::exposure(stops)]);
                assert_eq!(statistics.effects_not_run, 0);
                assert_eq!(statistics.passes_recorded, 2 + 1, "one pass, and none to convert");
                let gain = f64::from(stops).exp2();
                assert_near(
                    made.pixel(1, 1),
                    [0, 1, 2].map(|place| code_after_light_is_multiplied_by(colour[place], gain)),
                    255,
                    &format!("{colour:?} at {stops} stops"),
                );
            }
        }
    });
}

#[test]
fn an_exposure_leaves_the_colours_the_picture_has_no_opacity_for_alone_and_keeps_what_is_see_through_see_through() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Half see-through, over black: the colour is half of what the picture's own is, in light. One stop more is twice that.
        let (plain, _) = flat_frame(&mut renderer, LINEAR_REC709, [200, 100, 50, 128], Vec::new());
        let (brighter, _) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            [200, 100, 50, 128],
            vec![EffectCall::exposure(1.0)],
        );
        for place in 0..3 {
            let before = TransferFunction::Srgb.decode_to_light(f64::from(plain.pixel(1, 1)[place]) / 255.0);
            let after = TransferFunction::Srgb.decode_to_light(f64::from(brighter.pixel(1, 1)[place]) / 255.0);
            assert!(
                (after - 2.0 * before).abs() < 0.01,
                "place {place}: light {before} became {after}"
            );
        }
        assert_eq!(brighter.pixel(1, 1)[3], plain.pixel(1, 1)[3]);
    });
}

#[test]
fn an_opacity_multiplies_the_picture_colours_and_opacity_together_so_it_shows_the_layer_below_through_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let colour = [200, 120, 40, 255];
        upload(&mut renderer, 1, &picture_for(LINEAR_REC709, 4, 4, colour.repeat(16)));
        upload(
            &mut renderer,
            2,
            &picture_for(LINEAR_REC709, 4, 4, [10, 240, 130, 255].repeat(16)),
        );
        for opacity in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let (made, statistics) = render(
                &mut renderer,
                LINEAR_REC709,
                vec![
                    the_whole_frame(2),
                    LayerDescription {
                        effects: vec![EffectCall::opacity(opacity)],
                        ..the_whole_frame(1)
                    },
                ],
            );
            assert_eq!(statistics.effects_not_run, 0);
            // The light of the layer below, and of the layer above, mixed in the proportion that the opacity says.
            let mixed = |place: usize| {
                let below = TransferFunction::Srgb.decode_to_light(f64::from([10, 240, 130][place]) / 255.0);
                let above = TransferFunction::Srgb.decode_to_light(f64::from(colour[place]) / 255.0);
                TransferFunction::Srgb
                    .encode_from_light(above * f64::from(opacity) + below * (1.0 - f64::from(opacity)))
                    * 255.0
            };
            assert_near(
                made.pixel(1, 1),
                [mixed(0), mixed(1), mixed(2)],
                255,
                &format!("opacity {opacity}"),
            );
        }
    });
}

#[test]
fn an_opacity_of_nothing_on_the_only_layer_shows_black_and_an_opacity_outside_nought_to_one_is_cut_off_to_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let (nothing, _) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            [200, 120, 40, 255],
            vec![EffectCall::opacity(0.0)],
        );
        assert!(nothing.every_pixel_is([0, 0, 0, 255]));
        let (below, _) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            [200, 120, 40, 255],
            vec![EffectCall::opacity(-3.0)],
        );
        assert!(below.every_pixel_is([0, 0, 0, 255]));
        let (plain, statistics) = flat_frame(&mut renderer, LINEAR_REC709, [200, 120, 40, 255], Vec::new());
        let (above, _) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            [200, 120, 40, 255],
            vec![EffectCall::opacity(0.5)],
        );
        let (more, more_statistics) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            [200, 120, 40, 255],
            vec![EffectCall::opacity(7.0)],
        );
        assert_eq!(more.rgba_pixels, plain.rgba_pixels, "more than all of it is all of it");
        assert_eq!(
            more_statistics.passes_recorded, statistics.passes_recorded,
            "and no pass is made for it"
        );
        assert_ne!(above.rgba_pixels, plain.rgba_pixels);
    });
}

#[test]
fn an_exposure_of_nothing_and_an_opacity_of_all_are_not_run_and_a_call_with_the_wrong_numbers_is_counted_as_not_run() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let colour = [200, 120, 40, 255];
        let (plain, plain_statistics) = flat_frame(&mut renderer, LINEAR_REC709, colour, Vec::new());
        for (name, effect) in [
            ("no stops", EffectCall::exposure(0.0)),
            ("a hundredth of a step of stops", EffectCall::exposure(1.0e-5)),
            ("all the opacity", EffectCall::opacity(1.0)),
        ] {
            let (made, statistics) = flat_frame(&mut renderer, LINEAR_REC709, colour, vec![effect]);
            assert_eq!(statistics.passes_recorded, plain_statistics.passes_recorded, "{name}");
            assert_eq!(statistics.effects_not_run, 0, "{name}: it did what it was asked to");
            assert_eq!(made.rgba_pixels, plain.rgba_pixels, "{name}");
        }
        for (name, values) in [
            ("two numbers", vec![1.0, 2.0]),
            ("none", Vec::new()),
            ("not a number", vec![f32::NAN]),
            ("infinity", vec![f32::INFINITY]),
        ] {
            for key in [super::EXPOSURE_KEY, super::OPACITY_KEY] {
                let (made, statistics) = flat_frame(
                    &mut renderer,
                    LINEAR_REC709,
                    colour,
                    vec![EffectCall {
                        key: key.to_owned(),
                        values: values.clone(),
                        table: None,
                        state: None,
                    }],
                );
                assert_eq!(statistics.effects_not_run, 1, "{key}, {name}");
                assert_eq!(made.rgba_pixels, plain.rgba_pixels, "{key}, {name}");
            }
        }
    });
}

#[test]
fn effects_of_light_and_of_codes_in_a_row_are_converted_where_the_domains_meet_and_make_the_arithmetic_of_each() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let colour = [150, 100, 50, 255];
        // A tenth of the steps brighter, in codes (the matrix), and then one stop of exposure, in light: the codes go to light after the first and the light is doubled.
        let brighter = || {
            EffectCall::colour_matrix_with_offsets([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.1, 0.1, 0.1])
        };
        let (made, statistics) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            colour,
            vec![brighter(), EffectCall::exposure(1.0)],
        );
        assert_eq!(
            statistics.passes_recorded,
            2 + 2,
            "the matrix converts out of the codes itself, and the exposure needs no conversion"
        );
        let wanted = [0, 1, 2].map(|place| {
            let code = (f64::from(colour[place]) / 255.0 + 0.1).min(1.0);
            TransferFunction::Srgb.encode_from_light((TransferFunction::Srgb.decode_to_light(code) * 2.0).min(1.0))
                * 255.0
        });
        assert_near(made.pixel(1, 1), wanted, 255, "brighter, then one stop");
        // The other way round: the light is doubled and the codes of that are made brighter. It is not the same picture.
        let (other_way, _) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            colour,
            vec![EffectCall::exposure(1.0), brighter()],
        );
        let wanted = [0, 1, 2].map(|place| {
            let code = TransferFunction::Srgb.encode_from_light(
                (TransferFunction::Srgb.decode_to_light(f64::from(colour[place]) / 255.0) * 2.0).min(1.0),
            );
            (code + 0.1).min(1.0) * 255.0
        });
        assert_near(other_way.pixel(1, 1), wanted, 255, "one stop, then brighter");
        assert_ne!(made.rgba_pixels, other_way.rgba_pixels);
        // The hue shift, an exposure and another hue shift: the first converts into the codes and out of them, so does the last, and the exposure is one pass between them.
        let turn = || EffectCall::colour_matrix(ColourMatrix::hue_rotation(60.0).rows());
        let (_, around) = flat_frame(
            &mut renderer,
            LINEAR_REC709,
            colour,
            vec![turn(), EffectCall::exposure(0.5), turn()],
        );
        assert_eq!(around.passes_recorded, 2 + 3);
    });
}

#[test]
fn in_the_compatibility_mode_an_exposure_and_an_opacity_multiply_the_numbers_that_are_there() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let (exposed, _) = flat_frame(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            [60, 80, 100, 255],
            vec![EffectCall::exposure(1.0)],
        );
        assert_near(exposed.pixel(1, 1), [120.0, 160.0, 200.0], 255, "one stop of the codes");
        let (faded, _) = flat_frame(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            [60, 80, 100, 255],
            vec![EffectCall::opacity(0.5)],
        );
        assert_near(
            faded.pixel(1, 1),
            [30.0, 40.0, 50.0],
            255,
            "half of the codes, over black",
        );
    });
}
