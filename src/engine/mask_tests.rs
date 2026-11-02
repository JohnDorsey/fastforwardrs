// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A float input of a shader read from a mask (`docs/shader-masks-design.md`), made on the software adapter with the real rewriter and compiler: the value at each pixel is the channel of
//! the mask's colour there, spread over the range of the input; where the mask has nothing it is the number the call has for the input; the mask is read as the codes of a screen in light
//! as it is in the compatibility mode; and a mask is a group like any other (its layers have effects, it holds groups, it is a picture or a colour).

use std::time::Duration;

use super::effect_tests::{frame_of, whole_frame};
use super::foreign_effect_tests::call;
use super::linear_light_tests::{colour_of, picture_for, the_whole_frame, upload};
use super::oracle_tests::TestPicture;
use super::testing::with_software_graphics;
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, FrameDescription, FrameRenderer,
    FrameStatistics, GroupDescription, LayerDescription, LayerSource, RenderedFrame, ShaderStructure, WorkingSpace,
    guard_wgsl,
};
use crate::colour_space::StandardPrimaries;
use crate::isf::{DrivenInput, MaskChannel, compile_with_driven_inputs, read};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

/// A filter that shows the number `level` (0 to 1, 0.5 to begin with) as a grey, and ignores its picture.
const SHOW_THE_LEVEL: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5 } ] }*/\nvoid main() { gl_FragColor = vec4(vec3(level), 1.0); }\n";

/// A filter that shows `amount` (2 to 6) over 8 as a grey.
const SHOW_THE_AMOUNT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 6.0, \"DEFAULT\": 4.0 } ] }*/\nvoid main() { gl_FragColor = vec4(vec3(amount / 8.0), 1.0); }\n";

/// A filter that shows `red` and `green` (0 to 1) as the red and green of the picture.
const SHOW_TWO_LEVELS: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"red\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0 }, { \"NAME\": \"green\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0 } ] }*/\nvoid main() { gl_FragColor = vec4(red, green, 0.0, 1.0); }\n";

/// A filter whose picture is the average of eight reads of its own picture, spaced by `level`, and which shows `level` in the blue: the input is used in a loop.
const USE_THE_LEVEL_IN_A_LOOP: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5 } ] }*/\nvoid main() {\n\tfloat sum = 0.0;\n\tfor (int i = 0; i < 8; i++) { sum += level; }\n\tgl_FragColor = vec4(vec3(sum / 8.0), 1.0);\n}\n";

fn driven(name: &str, mask: usize, channel: MaskChannel) -> DrivenInput {
    DrivenInput {
        name: name.to_owned(),
        mask,
        channel,
    }
}

/// Compiles `text` with the inputs `driven` read from masks and gives it to the renderer under `key`.
fn installed_with_masks(
    renderer: &mut FrameRenderer,
    key: &str,
    text: &str,
    driven: &[DrivenInput],
) -> ShaderStructure {
    let file = read(text).unwrap();
    let compiled = compile_with_driven_inputs(&file, driven).unwrap_or_else(|problems| panic!("{problems:?}"));
    let structure = compiled
        .structure
        .clone()
        .unwrap_or_else(|problems| panic!("{problems:?}"));
    structure.check().unwrap();
    let guarded = guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP)
        .unwrap_or_else(|problem| panic!("{problem:?}\n{}", compiled.wgsl));
    renderer
        .install_effect_with_structure(key, &guarded, &structure)
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
    structure
}

/// A call of `key` whose inputs have these numbers (what a call has for an input that is read from a mask is the value where the mask has nothing) and which has these masks.
fn reading_masks(key: &str, inputs: &[f32], masks: Vec<GroupDescription>) -> EffectCall {
    let mut numbers = vec![0.0, 0.0, 0.0, 0.0];
    numbers.extend_from_slice(inputs);
    EffectCall {
        masks,
        ..call(key, &numbers)
    }
}

/// A colour (red, green, blue and opacity, straight) over `corners` of the mask.
fn a_colour_over(corners: [(f32, f32); 4], colour: [u8; 4]) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Solid(colour),
        corners,
        effects: Vec::new(),
    }
}

fn all_of_it(colour: [u8; 4]) -> LayerDescription {
    a_colour_over(CORNERS_OF_THE_WHOLE_PICTURE, colour)
}

fn the_left_half_of_it(colour: [u8; 4]) -> LayerDescription {
    a_colour_over([(0.0, 0.0), (0.5, 0.0), (0.5, 1.0), (0.0, 1.0)], colour)
}

fn mask_of(layers: Vec<LayerDescription>) -> GroupDescription {
    GroupDescription { layers }
}

/// A frame of 8 by 8 pixels made of the single layer `layer`, in the compatibility mode (the numbers are codes).
fn frame_of_one_layer(renderer: &mut FrameRenderer, layer: LayerDescription) -> (RenderedFrame, FrameStatistics) {
    let picture = TestPicture::gradient(8, 8);
    frame_of(renderer, (8, 8), &[(1, &picture)], vec![layer])
}

/// The frame of the layer that shows what is read from `masks`, with the call `key` that has `inputs` as its numbers.
fn shown(
    renderer: &mut FrameRenderer,
    key: &str,
    inputs: &[f32],
    masks: Vec<GroupDescription>,
) -> (RenderedFrame, FrameStatistics) {
    frame_of_one_layer(renderer, whole_frame(1, vec![reading_masks(key, inputs, masks)]))
}

/// Whether `pixel` is `expected` to within `by` in each colour, and opaque.
fn is_near(pixel: [u8; 4], expected: [u8; 3], by: u8) -> bool {
    pixel[3] == 255 && (0..3).all(|colour| pixel[colour].abs_diff(expected[colour]) <= by)
}

#[test]
fn the_value_of_an_input_is_the_channel_of_the_colour_of_its_mask_spread_over_the_range_of_the_input() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 200, 100 and 50 of 255; the brightness is 0.2126 * 200 + 0.7152 * 100 + 0.0722 * 50, which is 117.65.
        for (channel, wanted) in [
            (MaskChannel::Red, 200),
            (MaskChannel::Green, 100),
            (MaskChannel::Blue, 50),
            (MaskChannel::Brightness, 118),
        ] {
            let key = format!("test:level:{channel:?}");
            installed_with_masks(&mut renderer, &key, SHOW_THE_LEVEL, &[driven("level", 0, channel)]);
            let (frame, statistics) = shown(
                &mut renderer,
                &key,
                &[0.5],
                vec![mask_of(vec![all_of_it([200, 100, 50, 255])])],
            );
            assert_eq!(statistics.effects_not_run, 0, "{channel:?}");
            for (across, down) in [(0, 0), (7, 7), (3, 4)] {
                assert!(
                    is_near(frame.pixel(across, down), [wanted; 3], 1),
                    "{channel:?} at {across}, {down}: {:?}, wanted {wanted}",
                    frame.pixel(across, down)
                );
            }
        }
    });
}

#[test]
fn the_range_of_the_input_is_what_the_channel_is_spread_over() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:amount",
            SHOW_THE_AMOUNT,
            &[driven("amount", 0, MaskChannel::Red)],
        );
        // The amount is 2 where the red is 0, 6 where it is 255, and in between in proportion; the shader shows it over 8: 64, 191 and (the half, 4) 128.
        for (red, wanted) in [(0, 64), (255, 191), (128, 128)] {
            let (frame, _) = shown(
                &mut renderer,
                "test:amount",
                &[4.0],
                vec![mask_of(vec![all_of_it([red, 0, 0, 255])])],
            );
            assert!(
                is_near(frame.pixel(2, 2), [wanted; 3], 1),
                "red {red}: {:?}",
                frame.pixel(2, 2)
            );
        }
    });
}

#[test]
fn where_the_mask_has_nothing_the_value_is_the_number_the_call_has_for_the_input() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Brightness)],
        );
        // A white on the left half; the right half has nothing, and the call says 0.25 for the input: 64 of 255.
        let (frame, statistics) = shown(
            &mut renderer,
            "test:level",
            &[0.25],
            vec![mask_of(vec![the_left_half_of_it([255, 255, 255, 255])])],
        );
        assert_eq!(statistics.effects_not_run, 0);
        assert!(
            is_near(frame.pixel(1, 3), [255; 3], 1),
            "on the colour: {:?}",
            frame.pixel(1, 3)
        );
        assert!(
            is_near(frame.pixel(6, 3), [64; 3], 1),
            "where there is nothing: {:?}",
            frame.pixel(6, 3)
        );
        // The number the call has is what shows, whatever it is.
        let (other, _) = shown(
            &mut renderer,
            "test:level",
            &[0.75],
            vec![mask_of(vec![the_left_half_of_it([255, 255, 255, 255])])],
        );
        assert!(is_near(other.pixel(6, 3), [191; 3], 1), "{:?}", other.pixel(6, 3));
        assert!(is_near(other.pixel(1, 3), [255; 3], 1), "{:?}", other.pixel(1, 3));
    });
}

#[test]
fn a_mask_with_no_layer_at_all_has_nothing_everywhere() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );

        let (frame, statistics) = shown(&mut renderer, "test:level", &[0.5], vec![mask_of(Vec::new())]);

        assert_eq!(statistics.effects_not_run, 0);
        assert!(frame.every_pixel_is([128, 128, 128, 255]), "{:?}", frame.pixel(0, 0));
    });
}

#[test]
fn a_mask_that_is_partly_see_through_gives_a_mixture_of_the_number_of_the_call_and_what_the_colour_says() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );
        // Half as opaque as it can be (128 of 255 is 0.502), and as red as it can be: the unit is 1 whatever the opacity (the colour of the mask is premultiplied, and the opacity is
        // divided out), so the value is the mixture of the call's 0.2 and 1.0 by 0.502: 0.2 * 0.498 + 1.0 * 0.502, which is 0.6016, which is 153.4 of 255.
        let (frame, _) = shown(
            &mut renderer,
            "test:level",
            &[0.2],
            vec![mask_of(vec![all_of_it([255, 0, 0, 128])])],
        );
        assert!(is_near(frame.pixel(4, 4), [153; 3], 2), "{:?}", frame.pixel(4, 4));
        // The colour is not what makes it less: a mask of half of the red (128) as opaque as it can be is the mixture of 0.2 and 0.502 by 1: 128.
        let (dark, _) = shown(
            &mut renderer,
            "test:level",
            &[0.2],
            vec![mask_of(vec![all_of_it([128, 0, 0, 255])])],
        );
        assert!(is_near(dark.pixel(4, 4), [128; 3], 1), "{:?}", dark.pixel(4, 4));
    });
}

#[test]
fn two_masks_give_two_inputs_their_values() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:two",
            SHOW_TWO_LEVELS,
            &[
                driven("red", 0, MaskChannel::Red),
                driven("green", 1, MaskChannel::Green),
            ],
        );

        let (frame, statistics) = shown(
            &mut renderer,
            "test:two",
            &[0.0, 0.0],
            vec![
                mask_of(vec![all_of_it([100, 0, 0, 255])]),
                mask_of(vec![all_of_it([0, 200, 0, 255])]),
            ],
        );

        assert_eq!(statistics.effects_not_run, 0);
        assert!(is_near(frame.pixel(3, 3), [100, 200, 0], 1), "{:?}", frame.pixel(3, 3));
    });
}

#[test]
fn two_inputs_of_one_mask_read_two_channels_of_the_same_picture() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:two",
            SHOW_TWO_LEVELS,
            &[
                driven("red", 0, MaskChannel::Blue),
                driven("green", 0, MaskChannel::Red),
            ],
        );

        let (frame, _) = shown(
            &mut renderer,
            "test:two",
            &[0.0, 0.0],
            vec![mask_of(vec![all_of_it([30, 60, 90, 255])])],
        );

        assert!(is_near(frame.pixel(3, 3), [90, 30, 0], 1), "{:?}", frame.pixel(3, 3));
    });
}

#[test]
fn the_value_changes_across_the_picture_when_the_mask_does() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );
        // A mask that is a picture: its red goes from 0 on the left to 255 on the right (and is 3 * 255 / 7, which is 109, at the fourth pixel).
        let picture = TestPicture::gradient(8, 8);
        let (frame, _) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture), (2, &picture)],
            vec![whole_frame(
                1,
                vec![reading_masks(
                    "test:level",
                    &[0.5],
                    vec![mask_of(vec![whole_frame(2, Vec::new())])],
                )],
            )],
        );

        for (across, wanted) in [(0, 0), (3, 109), (7, 255)] {
            for down in [0, 5] {
                assert!(
                    is_near(frame.pixel(across, down), [wanted; 3], 2),
                    "at {across}, {down}: {:?}, wanted {wanted}",
                    frame.pixel(across, down)
                );
            }
        }
    });
}

#[test]
fn the_layers_of_a_mask_have_their_effects_done_before_the_mask_is_put_together() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );
        let picture = TestPicture::gradient(8, 8);
        // The picture of the mask is made see-through by an effect of its layer: nothing is left of it, and the value is the call's 0.25 everywhere.
        let (frame, statistics) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture), (2, &picture)],
            vec![whole_frame(
                1,
                vec![reading_masks(
                    "test:level",
                    &[0.25],
                    vec![mask_of(vec![whole_frame(2, vec![EffectCall::opacity(0.0)])])],
                )],
            )],
        );

        assert_eq!(statistics.effects_not_run, 0);
        assert!(is_near(frame.pixel(7, 3), [64; 3], 1), "{:?}", frame.pixel(7, 3));
        assert!(is_near(frame.pixel(0, 3), [64; 3], 1), "{:?}", frame.pixel(0, 3));
    });
}

#[test]
fn a_mask_is_a_group_and_may_hold_groups_and_layers_over_each_other() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Brightness)],
        );
        // A white on the left half, in a group; over it, a black on the whole of it that is not opaque. The brightness is what is left of white at the left and of nothing at the right.
        // At the left: white 255 under black of opacity 0.5: 127.5 premultiplied over opacity 1: brightness 127.5. At the right: black 0.5 over nothing: the colour is 0, the cover 0.5:
        // the value is the mixture of the call's 1.0 and 0 by 0.5.
        let mask = mask_of(vec![
            LayerDescription {
                source: LayerSource::Group(mask_of(vec![the_left_half_of_it([255, 255, 255, 255])])),
                corners: CORNERS_OF_THE_WHOLE_PICTURE,
                effects: Vec::new(),
            },
            all_of_it([0, 0, 0, 128]),
        ]);

        let (frame, _) = shown(&mut renderer, "test:level", &[1.0], vec![mask]);

        assert!(is_near(frame.pixel(1, 3), [127; 3], 2), "left: {:?}", frame.pixel(1, 3));
        assert!(
            is_near(frame.pixel(6, 3), [127; 3], 2),
            "right: {:?}",
            frame.pixel(6, 3)
        );
    });
}

#[test]
fn the_layer_that_reads_a_mask_may_be_in_a_group() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Green)],
        );
        let picture = TestPicture::gradient(8, 8);
        let layer = LayerDescription {
            source: LayerSource::Group(mask_of(vec![whole_frame(
                1,
                vec![reading_masks(
                    "test:level",
                    &[0.5],
                    vec![mask_of(vec![all_of_it([0, 150, 0, 255])])],
                )],
            )])),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        };

        let (frame, statistics) = frame_of(&mut renderer, (8, 8), &[(1, &picture)], vec![layer]);

        assert_eq!(statistics.effects_not_run, 0);
        assert!(is_near(frame.pixel(4, 4), [150; 3], 1), "{:?}", frame.pixel(4, 4));
    });
}

#[test]
fn an_input_used_in_a_loop_is_the_value_of_the_mask_in_every_round() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:loop",
            USE_THE_LEVEL_IN_A_LOOP,
            &[driven("level", 0, MaskChannel::Red)],
        );
        let picture = TestPicture::gradient(8, 8);
        let (frame, _) = frame_of(
            &mut renderer,
            (8, 8),
            &[(1, &picture), (2, &picture)],
            vec![whole_frame(
                1,
                vec![reading_masks(
                    "test:loop",
                    &[0.5],
                    vec![mask_of(vec![whole_frame(2, Vec::new())])],
                )],
            )],
        );

        for (across, wanted) in [(0, 0), (3, 109), (7, 255)] {
            assert!(
                is_near(frame.pixel(across, 2), [wanted; 3], 2),
                "at {across}: {:?}, wanted {wanted}",
                frame.pixel(across, 2)
            );
        }
    });
}

#[test]
fn a_call_that_does_not_have_the_masks_its_shader_reads_is_not_run() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );

        let (without, statistics) = shown(&mut renderer, "test:level", &[0.5], Vec::new());
        let (too_many, too_many_statistics) = shown(
            &mut renderer,
            "test:level",
            &[0.5],
            vec![mask_of(Vec::new()), mask_of(Vec::new())],
        );

        // (The effect is left out and counted, and the layer is its own picture.)
        assert_eq!(statistics.effects_not_run, 1);
        assert_eq!(too_many_statistics.effects_not_run, 1);
        let gradient = TestPicture::gradient(8, 8);
        let first = (2 * 8 + 5) * 4;
        let its_own = [gradient.rgba[first], gradient.rgba[first + 1], gradient.rgba[first + 2]];
        assert!(
            is_near(without.pixel(5, 2), its_own, 1),
            "{:?}, wanted {its_own:?}",
            without.pixel(5, 2)
        );
        assert_eq!(too_many.pixel(5, 2), without.pixel(5, 2));
    });
}

#[test]
fn a_shader_given_without_its_masks_in_the_structure_is_not_run_by_a_call_that_has_them() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The shader of the same text, read from no mask: a call that names a mask for it is not what it reads.
        installed_with_masks(&mut renderer, "test:plain", SHOW_THE_LEVEL, &[]);

        let (_, statistics) = shown(
            &mut renderer,
            "test:plain",
            &[0.5],
            vec![mask_of(vec![all_of_it([255, 0, 0, 255])])],
        );

        assert_eq!(statistics.effects_not_run, 1);
    });
}

#[test]
fn a_frame_with_a_mask_is_the_same_frame_when_it_is_made_again() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Brightness)],
        );
        let masks = || vec![mask_of(vec![the_left_half_of_it([200, 180, 40, 255])])];

        let (first, _) = shown(&mut renderer, "test:level", &[0.3], masks());
        let (second, _) = shown(&mut renderer, "test:level", &[0.3], masks());

        assert_eq!(first, second);
    });
}

// --- in light ---

fn render_in_light(renderer: &mut FrameRenderer, layers: Vec<LayerDescription>) -> (RenderedFrame, FrameStatistics) {
    let mut description = FrameDescription::black((8, 8));
    description.colour = colour_of(LINEAR_REC709);
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap()
}

#[test]
fn in_light_the_mask_is_read_as_the_codes_of_a_screen_and_not_as_light() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );
        upload(
            &mut renderer,
            1,
            &picture_for(LINEAR_REC709, 8, 8, [10, 20, 30, 255].repeat(64)),
        );

        let (frame, statistics) = render_in_light(
            &mut renderer,
            vec![LayerDescription {
                effects: vec![reading_masks(
                    "test:level",
                    &[0.5],
                    vec![mask_of(vec![all_of_it([200, 100, 50, 255])])],
                )],
                ..the_whole_frame(1)
            }],
        );

        // The mask is a colour of 200 in the codes of a screen, which is a light of 0.578. The shader is given 200 of 255, and not 0.578, and the frame comes out as the codes it was given
        // (give or take the rounding of the conversions).
        assert_eq!(statistics.effects_not_run, 0);
        assert!(is_near(frame.pixel(4, 4), [200; 3], 2), "{:?}", frame.pixel(4, 4));
    });
}

#[test]
fn in_light_a_mask_that_is_partly_see_through_is_converted_without_its_opacity_in_the_way() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Red)],
        );
        upload(
            &mut renderer,
            1,
            &picture_for(LINEAR_REC709, 8, 8, [10, 20, 30, 255].repeat(64)),
        );

        // A red of 128 in the codes that is 128 of 255 opaque. Its light is 0.2158 (premultiplied: 0.1083), its codes are 128 straight, and the shader divides by the cover (0.502) to
        // find the unit 0.502 and mixes it with the call's 0 by the cover: 0.252, which is 64 of 255. (A conversion that took the codes of the premultiplied light would find 0.359 and
        // make 91: this tells the two apart.)
        let (frame, _) = render_in_light(
            &mut renderer,
            vec![LayerDescription {
                effects: vec![reading_masks(
                    "test:level",
                    &[0.0],
                    vec![mask_of(vec![all_of_it([128, 0, 0, 128])])],
                )],
                ..the_whole_frame(1)
            }],
        );

        assert!(is_near(frame.pixel(4, 4), [64; 3], 2), "{:?}", frame.pixel(4, 4));
    });
}

#[test]
fn what_a_mask_makes_does_not_depend_on_the_mode_the_frame_is_made_in() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:level",
            SHOW_THE_LEVEL,
            &[driven("level", 0, MaskChannel::Brightness)],
        );
        upload(
            &mut renderer,
            1,
            &picture_for(LINEAR_REC709, 8, 8, [10, 20, 30, 255].repeat(64)),
        );
        // (A picture that is sent is made for a working space, and is kept as it was sent: the frame in codes is given a picture of its own.)
        upload(
            &mut renderer,
            2,
            &picture_for(WorkingSpace::DisplayReferred, 8, 8, [10, 20, 30, 255].repeat(64)),
        );
        let layers = |picture: u64| {
            vec![LayerDescription {
                effects: vec![reading_masks(
                    "test:level",
                    &[0.25],
                    vec![mask_of(vec![the_left_half_of_it([200, 120, 60, 255])])],
                )],
                ..the_whole_frame(picture)
            }]
        };

        let (in_light, _) = render_in_light(&mut renderer, layers(1));
        let in_codes = {
            let mut description = FrameDescription::black((8, 8));
            description.layers = layers(2);
            renderer.render(&description, A_LONG_TIME).unwrap().0
        };

        // (Brightness of 200, 120 and 60 in codes is 0.2126 * 200 + 0.7152 * 120 + 0.0722 * 60 = 132.7 on the left; the call's 0.25, which is 64, on the right.)
        for (across, down, wanted) in [(1, 1, 133), (6, 6, 64)] {
            let (a, b) = (in_light.pixel(across, down), in_codes.pixel(across, down));
            assert!(
                is_near(a, [wanted; 3], 2),
                "in light at {across}, {down}: {a:?}, wanted {wanted}"
            );
            assert!(
                is_near(b, [wanted; 3], 1),
                "in codes at {across}, {down}: {b:?}, wanted {wanted}"
            );
        }
    });
}
