// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effect that mixes the colours of a layer's picture (`engine:colour_matrix`), made by the engine on the software adapter and held to what the
//! window does: it mixes the colours of the 8-bit pixels on the processor (`ColourMatrix::apply_to_rgba_pixels`) and gives egui the result, which
//! draws it. The engine does the same in half-float numbers with no 8-bit step in the middle, so the two are the same picture to within the rounding.
//!
//! **What was measured** (on the software adapter of Windows, 2026-10-15), and is asserted below: for nine matrices (turns of the hues, mixings, ones that
//! are cut off at both ends, a swap of colours, a darkening, two turns folded into one) and for opaque and see-through pictures, the engine differs from
//! the window by one level at most in any number (and not at all for a swap of colours).

use std::time::Duration;

use super::oracle_tests::{TestLayer, TestPicture, TestSource, compare, oracle_frame, turned, upright};
use super::testing::with_software_graphics;
use super::{
    COLOUR_MATRIX_KEY, CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, FrameRenderer, FrameStatistics,
    LayerDescription, LayerSource, PictureId, RenderedFrame,
};
use crate::colour_effects::ColourMatrix;

const A_LONG_TIME: Duration = Duration::from_secs(60);

pub(super) fn matrix_effect(matrix: ColourMatrix) -> EffectCall {
    EffectCall::colour_matrix(matrix.rows())
}

/// A layer of `picture` over the whole frame, with `effects` done to it first.
pub(super) fn whole_frame(picture: u64, effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects,
    }
}

/// The frame of `layers` over black, with the pictures `(id, picture)` sent first, and what making it came to.
pub(super) fn frame_of(
    renderer: &mut FrameRenderer,
    size: (u32, u32),
    pictures: &[(u64, &TestPicture)],
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, FrameStatistics) {
    for (id, picture) in pictures {
        renderer
            .upload_picture(PictureId(*id), &picture.as_picture_data())
            .unwrap();
    }
    render(renderer, size, layers)
}

/// The frame of `layers` over black, from the pictures the renderer has.
pub(super) fn render(
    renderer: &mut FrameRenderer,
    size: (u32, u32),
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, FrameStatistics) {
    let mut description = FrameDescription::black(size);
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap()
}

/// What the window makes of a picture that has a colour matrix: the matrix applied to its 8-bit pixels on the processor.
fn as_the_window_changes_it(picture: &TestPicture, matrix: ColourMatrix) -> TestPicture {
    let mut rgba = picture.rgba.clone();
    matrix.apply_to_rgba_pixels(&mut rgba);
    TestPicture {
        width: picture.width,
        height: picture.height,
        rgba,
    }
}

/// The window's own picture of `picture` with `matrix` done to it, over the whole frame.
fn the_window_draws(
    graphics: &super::testing::SoftwareGraphics,
    size: (u32, u32),
    picture: &TestPicture,
    matrix: ColourMatrix,
) -> Vec<u8> {
    oracle_frame(
        &graphics.device,
        size,
        &[TestLayer {
            source: TestSource::Picture(as_the_window_changes_it(picture, matrix)),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
        }],
    )
}

/// Numbers that look random and are the same every time.
fn noise(count: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// A picture of random colours and, if `see_through`, random opacities (some of them 0 and some 255).
pub(super) fn noisy(width: u32, height: u32, seed: u64, see_through: bool) -> TestPicture {
    let mut rgba = noise((width * height * 4) as usize, seed);
    for (number, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel[3] = if see_through {
            match number % 7 {
                0 => 0,
                1 => 255,
                _ => pixel[3],
            }
        } else {
            255
        };
    }
    TestPicture { width, height, rgba }
}

/// A picture all of one colour.
pub(super) fn flat(width: u32, height: u32, colour: [u8; 4]) -> TestPicture {
    TestPicture {
        width,
        height,
        rgba: colour.repeat((width * height) as usize),
    }
}

/// Matrices of every kind: turns of the hues, a mixing of everything with everything, ones that make numbers too big or too small (which are cut off),
/// ones that swap colours and ones that darken.
pub(super) fn matrices() -> Vec<(&'static str, ColourMatrix)> {
    vec![
        ("hue 30", ColourMatrix::hue_rotation(30.0)),
        ("hue 120", ColourMatrix::hue_rotation(120.0)),
        ("hue 200", ColourMatrix::hue_rotation(200.0)),
        ("hue 300", ColourMatrix::hue_rotation(300.0)),
        (
            "greyer",
            ColourMatrix::from_rows([[0.5, 0.25, 0.25], [0.25, 0.5, 0.25], [0.25, 0.25, 0.5]]),
        ),
        (
            "cut off at both ends",
            ColourMatrix::from_rows([[1.5, 0.5, 0.0], [-0.5, 1.2, 0.3], [0.2, -0.4, 1.8]]),
        ),
        (
            "colours swapped",
            ColourMatrix::from_rows([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]),
        ),
        (
            "darker",
            ColourMatrix::from_rows([[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]]),
        ),
        (
            "two turns of the hues folded into one",
            ColourMatrix::hue_rotation(70.0).then(ColourMatrix::hue_rotation(110.0)),
        ),
    ]
}

#[test]
fn a_picture_with_a_colour_matrix_is_what_the_window_makes_of_it_to_within_a_level() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = [
            ("gradient", TestPicture::gradient(64, 48)),
            ("random colours", noisy(40, 30, 3, false)),
            ("checkers", TestPicture::checkers(64, 48, 4)),
        ];
        for (picture_name, picture) in &pictures {
            let size = (picture.width, picture.height);
            for (matrix_name, matrix) in matrices() {
                let (frame, statistics) = frame_of(
                    &mut renderer,
                    size,
                    &[(1, picture)],
                    vec![whole_frame(1, vec![matrix_effect(matrix)])],
                );
                assert_eq!(statistics.effects_not_run, 0);
                let difference = compare(&frame.rgba_pixels, &the_window_draws(&graphics, size, picture, matrix));
                assert!(
                    difference.largest <= 1,
                    "{picture_name} [{matrix_name}]: {difference:?}"
                );
            }
        }
    });
}

#[test]
fn a_picture_that_is_see_through_is_mixed_in_its_straight_colours_and_stays_as_clear_as_it_was() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = [
            ("soft disc", TestPicture::soft_disc(64, 48, [220, 90, 40])),
            ("random colours and opacities", noisy(40, 30, 5, true)),
        ];
        for (picture_name, picture) in &pictures {
            let size = (picture.width, picture.height);
            for (matrix_name, matrix) in matrices() {
                let (frame, _) = frame_of(
                    &mut renderer,
                    size,
                    &[(1, picture)],
                    vec![whole_frame(1, vec![matrix_effect(matrix)])],
                );
                let difference = compare(&frame.rgba_pixels, &the_window_draws(&graphics, size, picture, matrix));
                assert!(
                    difference.largest <= 1,
                    "{picture_name} [{matrix_name}]: {difference:?}"
                );
            }
        }
    });
}

#[test]
fn a_pixel_that_is_all_clear_stays_clear_whatever_colour_it_has_and_whatever_the_matrix_makes_of_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Bright colours, none of them showing: an opacity of nothing.
        let clear = flat(8, 8, [255, 40, 200, 0]);
        for (name, matrix) in matrices() {
            let (frame, _) = frame_of(
                &mut renderer,
                (8, 8),
                &[(1, &clear)],
                vec![whole_frame(1, vec![matrix_effect(matrix)])],
            );
            assert!(
                frame.every_pixel_is([0, 0, 0, 255]),
                "[{name}]: {:?}",
                frame.pixel(3, 3)
            );
        }
    });
}

#[test]
fn colours_that_are_mixed_come_out_as_the_arithmetic_says_exactly() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = flat(4, 4, [100, 150, 200, 255]);
        let swap = ColourMatrix::from_rows([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]);
        let half = ColourMatrix::from_rows([[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]]);
        let all_red = ColourMatrix::from_rows([[1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]]);
        let (swapped, _) = frame_of(
            &mut renderer,
            (4, 4),
            &[(1, &picture)],
            vec![whole_frame(1, vec![matrix_effect(swap)])],
        );
        assert!(swapped.every_pixel_is([200, 150, 100, 255]));
        let (halved, _) = render(&mut renderer, (4, 4), vec![whole_frame(1, vec![matrix_effect(half)])]);
        // (50, 75 and 100: a half of 100, 150 and 200, which a half-float number keeps exactly.)
        assert!(halved.every_pixel_is([50, 75, 100, 255]), "{:?}", halved.pixel(0, 0));
        let (red, _) = render(
            &mut renderer,
            (4, 4),
            vec![whole_frame(1, vec![matrix_effect(all_red)])],
        );
        // 100 + 150 + 200 is far more than there is room for: red is as red as it gets, the rest is nothing.
        assert!(red.every_pixel_is([255, 0, 0, 255]));
    });
}

#[test]
fn effects_are_run_in_the_order_they_were_asked_for_each_on_what_the_one_before_made() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = TestPicture::gradient(64, 48);
        let size = (64, 48);
        // The first cuts colours off at both ends, so doing it first or second makes a difference (matrices on their own would only commute if they
        // were turns of the same wheel).
        let cut_off = ColourMatrix::from_rows([[1.5, 0.5, 0.0], [-0.5, 1.2, 0.3], [0.2, -0.4, 1.8]]);
        let greyer = ColourMatrix::from_rows([[0.5, 0.25, 0.25], [0.25, 0.5, 0.25], [0.25, 0.25, 0.5]]);

        let (first_then_second, statistics) = frame_of(
            &mut renderer,
            size,
            &[(1, &picture)],
            vec![whole_frame(1, vec![matrix_effect(cut_off), matrix_effect(greyer)])],
        );
        let (second_then_first, _) = render(
            &mut renderer,
            size,
            vec![whole_frame(1, vec![matrix_effect(greyer), matrix_effect(cut_off)])],
        );
        assert_eq!(statistics.passes_recorded, 2 + 2, "two effects, the layers, the output");
        assert!(
            compare(&first_then_second.rgba_pixels, &second_then_first.rgba_pixels).largest > 8,
            "the order does not matter, so this tests nothing"
        );

        // What the window does for two effects, if each were done to the 8-bit pixels in turn (it does one matrix, of the two folded into one, which is
        // not the same: the engine runs what it is told, and it is the description of a frame that folds them).
        let window_way = as_the_window_changes_it(&as_the_window_changes_it(&picture, cut_off), greyer);
        let oracle = oracle_frame(
            &graphics.device,
            size,
            &[TestLayer {
                source: TestSource::Picture(window_way),
                corners: CORNERS_OF_THE_WHOLE_PICTURE,
            }],
        );
        let difference = compare(&first_then_second.rgba_pixels, &oracle);
        assert!(
            difference.largest <= 2,
            "two steps, each rounded to 8 bits in the window's way: {difference:?}"
        );
    });
}

#[test]
fn a_turned_and_resized_picture_with_a_colour_matrix_is_what_the_window_makes_of_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let size = (96, 72);
        let picture = TestPicture::gradient(40, 30);
        let see_through = TestPicture::soft_disc(48, 48, [30, 200, 120]);
        for (name, matrix) in matrices() {
            for (placement_name, corners) in [
                ("upright and smaller", upright(0.1, 0.15, 0.5, 0.6)),
                ("turned 33 degrees", turned((0.5, 0.5), 50.0, 36.0, 33.0, size)),
                (
                    "turned 200 degrees and cut off by the edge",
                    turned((0.9, 0.6), 70.0, 50.0, 200.0, size),
                ),
            ] {
                for (picture_name, picture) in [("opaque", &picture), ("see-through", &see_through)] {
                    let (frame, _) = frame_of(
                        &mut renderer,
                        size,
                        &[(1, picture)],
                        vec![LayerDescription {
                            source: LayerSource::Picture(PictureId(1)),
                            corners,
                            effects: vec![matrix_effect(matrix)],
                        }],
                    );
                    let oracle = oracle_frame(
                        &graphics.device,
                        size,
                        &[TestLayer {
                            source: TestSource::Picture(as_the_window_changes_it(picture, matrix)),
                            corners,
                        }],
                    );
                    let difference = compare(&frame.rgba_pixels, &oracle);
                    assert!(
                        difference.largest <= 1,
                        "{picture_name} picture, {placement_name} [{name}]: {difference:?}"
                    );
                }
            }
        }
    });
}

#[test]
fn an_effect_that_changes_nothing_is_not_run_and_an_effect_the_engine_does_not_have_is_left_out_and_counted() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = TestPicture::gradient(32, 24);
        let size = (32, 24);
        let (plain, plain_statistics) =
            frame_of(&mut renderer, size, &[(1, &picture)], vec![whole_frame(1, Vec::new())]);
        assert_eq!(plain_statistics.passes_recorded, 2);
        assert_eq!(plain_statistics.effects_not_run, 0);

        let (with_nothing_changing, statistics) = render(
            &mut renderer,
            size,
            vec![whole_frame(1, vec![matrix_effect(ColourMatrix::NOTHING_CHANGES)])],
        );
        assert_eq!(with_nothing_changing, plain);
        assert_eq!(statistics.passes_recorded, 2, "no pass for it");
        assert_eq!(statistics.effects_not_run, 0, "it did what it was asked to");

        let someone_elses = EffectCall {
            key: "someone:else".to_owned(),
            values: vec![1.0, 2.0, 3.0],
            table: None,
            state: None,
        };
        let too_few_numbers = EffectCall {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: vec![1.0; 4],
            table: None,
            state: None,
        };
        let (with_the_others, statistics) = render(
            &mut renderer,
            size,
            vec![whole_frame(1, vec![someone_elses, too_few_numbers])],
        );
        assert_eq!(with_the_others, plain, "left out, as if they were not there");
        assert_eq!(statistics.effects_not_run, 2);
        assert_eq!(statistics.passes_recorded, 2);

        // The ones that can be run are, and the others do not stop them.
        let redder = ColourMatrix::hue_rotation(120.0);
        let (mixed, statistics) = render(
            &mut renderer,
            size,
            vec![whole_frame(
                1,
                vec![
                    EffectCall {
                        key: "someone:else".to_owned(),
                        values: Vec::new(),
                        table: None,
                        state: None,
                    },
                    matrix_effect(redder),
                ],
            )],
        );
        assert_eq!(statistics.effects_not_run, 1);
        let difference = compare(&mixed.rgba_pixels, &the_window_draws(&graphics, size, &picture, redder));
        assert!(difference.largest <= 1, "{difference:?}");
    });
}

#[test]
fn the_effects_of_a_layer_that_is_only_a_colour_are_left_out_and_counted() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut description = FrameDescription::black((16, 16));
        description.layers.push(LayerDescription {
            source: LayerSource::Solid([200, 100, 50, 255]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: vec![matrix_effect(ColourMatrix::hue_rotation(120.0))],
        });
        let (frame, statistics) = renderer.render(&description, A_LONG_TIME).unwrap();
        assert!(frame.every_pixel_is([200, 100, 50, 255]));
        assert_eq!(statistics.effects_not_run, 1);
    });
}

#[test]
fn an_effect_does_not_change_the_picture_the_engine_keeps_and_pictures_shared_by_layers_have_their_own_effects() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = TestPicture::gradient(32, 24);
        let size = (64, 24);
        let turned_hues = ColourMatrix::hue_rotation(120.0);
        let left = upright(0.0, 0.0, 0.5, 1.0);
        let right = upright(0.5, 0.0, 0.5, 1.0);
        let layer = |corners, effects| LayerDescription {
            source: LayerSource::Picture(PictureId(1)),
            corners,
            effects,
        };

        // One picture on both halves of the frame, with the effect on the right half only.
        let (both, _) = frame_of(
            &mut renderer,
            size,
            &[(1, &picture)],
            vec![layer(left, Vec::new()), layer(right, vec![matrix_effect(turned_hues)])],
        );
        let oracle = oracle_frame(
            &graphics.device,
            size,
            &[
                TestLayer {
                    source: TestSource::Picture(picture.clone()),
                    corners: left,
                },
                TestLayer {
                    source: TestSource::Picture(as_the_window_changes_it(&picture, turned_hues)),
                    corners: right,
                },
            ],
        );
        assert!(compare(&both.rgba_pixels, &oracle).largest <= 1);

        // The picture itself was not touched: asked for again with nothing done to it, it is as it was; and with the effect again, the same as before.
        let (plain, _) = render(
            &mut renderer,
            size,
            vec![layer(left, Vec::new()), layer(right, Vec::new())],
        );
        let plain_oracle = oracle_frame(
            &graphics.device,
            size,
            &[
                TestLayer {
                    source: TestSource::Picture(picture.clone()),
                    corners: left,
                },
                TestLayer {
                    source: TestSource::Picture(picture.clone()),
                    corners: right,
                },
            ],
        );
        assert_eq!(
            plain.rgba_pixels, plain_oracle,
            "the same pixels as the window's, not a number off"
        );
        let (again, _) = render(
            &mut renderer,
            size,
            vec![layer(left, Vec::new()), layer(right, vec![matrix_effect(turned_hues)])],
        );
        assert_eq!(again, both);
    });
}

#[test]
fn the_pictures_that_effects_make_come_from_the_pool_and_go_back_to_it_so_that_frames_after_the_first_make_none() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = TestPicture::gradient(48, 36);
        let layers = || {
            vec![
                whole_frame(
                    1,
                    vec![
                        matrix_effect(ColourMatrix::hue_rotation(30.0)),
                        matrix_effect(ColourMatrix::hue_rotation(100.0).then(ColourMatrix::from_rows([
                            [0.5, 0.25, 0.25],
                            [0.25, 0.5, 0.25],
                            [0.25, 0.25, 0.5],
                        ]))),
                    ],
                ),
                whole_frame(1, vec![matrix_effect(ColourMatrix::hue_rotation(250.0))]),
            ]
        };
        frame_of(&mut renderer, (48, 36), &[(1, &picture)], layers());
        let made_after_the_first = renderer.pool_statistics().made;
        for _ in 0..5 {
            render(&mut renderer, (48, 36), layers());
        }
        assert_eq!(
            renderer.pool_statistics().made,
            made_after_the_first,
            "a texture was made for a frame after the first"
        );
        assert!(renderer.pool_statistics().reused > 0);
    });
}

#[test]
fn the_shader_has_its_matrix_where_the_writer_puts_the_numbers_and_the_struct_is_as_big_as_the_pipeline_says() {
    use super::pipelines::COLOUR_MATRIX_UNIFORM_BYTES;
    use super::{UniformBytes, shaders};
    let module = naga::front::wgsl::parse_str(shaders::COLOUR_MATRIX).unwrap();
    let (_, global) = module
        .global_variables
        .iter()
        .find(|(_, global)| global.name.as_deref() == Some("mixing"))
        .unwrap();
    let naga::TypeInner::Struct { members, span } = &module.types[global.ty].inner else {
        panic!("not a struct")
    };
    let members: Vec<(String, u32)> = members
        .iter()
        .map(|member| (member.name.clone().unwrap(), member.offset))
        .collect();
    // (Rewritten on purpose in phase 3: after the matrix that mixes there are the two that take the light of the working space to the display-referred domain and back, and
    // the number of the curve of the domain. And again in phase 4, piece 4.2: after those, the three offsets that are added to the mixed colour, and the number that says
    // which parts of the pass are left out. The first four are where they were.)
    assert_eq!(
        members,
        vec![
            ("to_new".to_owned(), 0),
            ("into_domain".to_owned(), 48),
            ("out_of_domain".to_owned(), 96),
            ("curve".to_owned(), 144),
            ("offsets".to_owned(), 160),
            ("parts".to_owned(), 172),
        ]
    );
    assert_eq!(u64::from(*span), COLOUR_MATRIX_UNIFORM_BYTES);

    let domain = super::DisplayDomainConversion {
        into_domain: [[10.0, 11.0, 12.0], [13.0, 14.0, 15.0], [16.0, 17.0, 18.0]],
        out_of_domain: [[20.0, 21.0, 22.0], [23.0, 24.0, 25.0], [26.0, 27.0, 28.0]],
        curve: 5,
    };
    let mut bytes = UniformBytes::new(256);
    let parts = super::MixingParts {
        offsets: [0.25, 0.5, 0.75],
        converts_into_the_domain: false,
        converts_out_of_the_domain: true,
        mixes: false,
    };
    let slot = bytes.add_slot(|writer| {
        writer.mat3x3_from_rows([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        domain.write(writer);
        parts.write(writer);
    });
    assert_eq!(u64::from(slot.size), COLOUR_MATRIX_UNIFORM_BYTES);
    let number = |offset: usize| f32::from_le_bytes(bytes.as_bytes()[offset..offset + 4].try_into().unwrap());
    // Column by column, 16 bytes each: the first column is the first number of each row.
    assert_eq!([number(0), number(4), number(8)], [1.0, 4.0, 7.0]);
    assert_eq!([number(16), number(20), number(24)], [2.0, 5.0, 8.0]);
    assert_eq!([number(32), number(36), number(40)], [3.0, 6.0, 9.0]);
    assert_eq!(
        [number(48), number(52), number(56)],
        [10.0, 13.0, 16.0],
        "the matrix into the domain"
    );
    assert_eq!(
        [number(96), number(100), number(104)],
        [20.0, 23.0, 26.0],
        "and out of it"
    );
    assert_eq!(
        u32::from_le_bytes(bytes.as_bytes()[144..148].try_into().unwrap()),
        5,
        "the curve of the domain, after them"
    );
    assert_eq!(
        [number(160), number(164), number(168)],
        [0.25, 0.5, 0.75],
        "the offsets"
    );
    assert_eq!(
        u32::from_le_bytes(bytes.as_bytes()[172..176].try_into().unwrap()),
        0b101,
        "the conversion into the domain and the mixing are left out, and the conversion out of it is not"
    );
}
