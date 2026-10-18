// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's look-up table effect (phase 4, piece 4.6; the design is `docs/engine-phase-4-design.md`, F9 and F10), on the software adapter: the pass held to the look-up on
//! the processor (`CubeTable::look_up`, which is held to ffmpeg's `lut3d` in `tests/cube_tables_against_ffmpeg.rs`), the spaces a table is taken to and from, the intensity, the
//! tables on the card, and the plan that runs it.

use std::sync::Arc;
use std::time::Duration;

use super::linear_light_tests::{colour_of, picture_for, the_whole_frame, upload};
use super::oracle_tests::TestPicture;
use super::testing::with_software_graphics;
use super::{
    EffectCall, FrameDescription, FrameRenderer, FrameStatistics, LayerDescription, LutInterpolation, LutSpace,
    RenderedFrame, TableId, WorkingSpace, publish_table,
};
use crate::colour_effects::ColourMatrix;
use crate::colour_space::{StandardPrimaries, TransferFunction};
use crate::cube_table::{CubeShape, CubeTable, Interpolation, parse_cube};

const A_LONG_TIME: Duration = Duration::from_secs(60);

const LINEAR_REC709: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

const SIDE: u32 = 64;

/// The text of a 3-D table by formula.
fn text_of_a_3d_table(size: usize, header: &str, make: impl Fn(f32, f32, f32) -> [f32; 3]) -> String {
    let mut text = format!("{header}LUT_3D_SIZE {size}\n");
    let top = (size - 1) as f32;
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let [r, g, b] = make(red as f32 / top, green as f32 / top, blue as f32 / top);
                text.push_str(&format!("{r:.6} {g:.6} {b:.6}\n"));
            }
        }
    }
    text
}

fn the_identity_3d(size: usize) -> Arc<CubeTable> {
    Arc::new(parse_cube(&text_of_a_3d_table(size, "", |r, g, b| [r, g, b])).unwrap())
}

/// A table, published (so that the engine can take it), and the id to call it by. (The test keeps the `Arc`: the shelf holds only a weak reference.)
fn published(table: &Arc<CubeTable>) -> TableId {
    publish_table(table)
}

/// Tables of the kinds the engine has to look up in.
fn tables() -> Vec<(&'static str, Arc<CubeTable>)> {
    let three = |size, make: &dyn Fn(f32, f32, f32) -> [f32; 3]| {
        Arc::new(parse_cube(&text_of_a_3d_table(size, "", make)).unwrap())
    };
    let mut one_d = String::from("LUT_1D_SIZE 1000\n");
    for step in 0..1000 {
        let x = step as f32 / 999.0;
        one_d.push_str(&format!("{:.6} {:.6} {:.6}\n", x.powf(0.8), x * x, 1.0 - 0.9 * x));
    }
    vec![
        (
            "a warm curved grade, 17 points",
            three(17, &|r, g, b| {
                [
                    r.powf(0.8) * 1.05,
                    g.powf(1.1) * 0.95 + 0.02 * r,
                    (b * 0.9 + 0.1 * g).powf(1.3),
                ]
            }),
        ),
        (
            "channels crossed over, 9 points",
            three(9, &|r, g, b| [g * 0.7 + b * 0.3, b * 0.5 + r * 0.5, r * 0.2 + g * 0.8]),
        ),
        (
            "the corners of the cube, 2 points",
            three(2, &|r, g, b| [r * 0.9 + 0.05, g * 0.2 + b * 0.7, (1.0 - r) * b]),
        ),
        (
            "values outside 0 to 1, 5 points",
            three(5, &|r, g, b| [r * 1.3 - 0.1, g, (b * b) * 1.2]),
        ),
        (
            "a 1-D table of 1000 points (four rows of the strip)",
            Arc::new(parse_cube(&one_d).unwrap()),
        ),
    ]
}

/// Every colour of a coarse grid of the cube of colours, then noise: `SIDE * SIDE` pixels, opaque.
fn the_colours() -> TestPicture {
    let mut rgba = Vec::new();
    let levels = [0_u8, 1, 17, 64, 100, 127, 128, 190, 254, 255];
    for red in levels {
        for green in levels {
            for blue in levels {
                rgba.extend([red, green, blue, 255]);
            }
        }
    }
    let mut state = 0x2545_F491_u32;
    while rgba.len() < (SIDE * SIDE * 4) as usize {
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 8) as u8
        };
        rgba.extend([next(), next(), next(), 255]);
    }
    rgba.truncate((SIDE * SIDE * 4) as usize);
    TestPicture {
        width: SIDE,
        height: SIDE,
        rgba,
    }
}

fn render(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, FrameStatistics) {
    let mut description = FrameDescription::black((SIDE, SIDE));
    description.colour = colour_of(working);
    description.layers = layers;
    renderer.render(&description, A_LONG_TIME).unwrap()
}

/// A layer of the picture 1 with `effects` done to it.
fn with(effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        effects,
        ..the_whole_frame(1)
    }
}

fn the_lut(table: TableId, intensity: f32, input: LutSpace, output: LutSpace, way: LutInterpolation) -> EffectCall {
    EffectCall::lut(table, intensity, input, output, way)
}

fn the_codes_screen(table: TableId) -> EffectCall {
    the_lut(
        table,
        1.0,
        LutSpace::Screen,
        LutSpace::Screen,
        LutInterpolation::Tetrahedral,
    )
}

/// The most that any number of any pixel of `frame` differs from `wanted`, as whole numbers of 255.
fn the_largest_difference(frame: &RenderedFrame, wanted: &[[u8; 3]]) -> (u8, usize) {
    let mut largest = (0, 0);
    for (number, (pixel, wanted)) in frame.rgba_pixels.as_chunks::<4>().0.iter().zip(wanted).enumerate() {
        for place in 0..3 {
            let difference = pixel[place].abs_diff(wanted[place]);
            if difference > largest.0 {
                largest = (difference, number);
            }
        }
    }
    largest
}

/// What the look-up on the processor makes of every colour of `picture`, as 8-bit colours.
fn what_the_reference_makes_of(table: &CubeTable, picture: &TestPicture, interpolation: Interpolation) -> Vec<[u8; 3]> {
    picture
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            let looked_up = table.look_up([0, 1, 2].map(|place| f32::from(pixel[place]) / 255.0), interpolation);
            looked_up.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
        })
        .collect()
}

// ---- the pass against the look-up on the processor ----

#[test]
fn a_table_through_the_pass_makes_the_colours_that_the_look_up_on_the_processor_makes_to_within_a_level() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = the_colours();
        upload(&mut renderer, 1, &picture.as_picture_data());
        for (name, table) in tables() {
            let id = published(&table);
            let ways: &[(LutInterpolation, Interpolation)] =
                if matches!(table.shape(), CubeShape::OneDimensional { .. }) {
                    &[(LutInterpolation::Tetrahedral, Interpolation::Tetrahedral)]
                } else {
                    &[
                        (LutInterpolation::Tetrahedral, Interpolation::Tetrahedral),
                        (LutInterpolation::Trilinear, Interpolation::Trilinear),
                    ]
                };
            for (way, reference_way) in ways {
                let (frame, statistics) = render(
                    &mut renderer,
                    WorkingSpace::DisplayReferred,
                    vec![with(vec![the_lut(id, 1.0, LutSpace::Screen, LutSpace::Screen, *way)])],
                );
                assert_eq!(statistics.effects_not_run, 0, "{name}");
                assert_eq!(statistics.passes_recorded, 2 + 1, "{name}: one pass");
                let wanted = what_the_reference_makes_of(&table, &picture, *reference_way);
                let (largest, at) = the_largest_difference(&frame, &wanted);
                assert!(
                    largest <= 1,
                    "{name} {way:?}: {largest} levels at the colour {:?}, where the pass makes {:?} and the look-up makes {:?}",
                    &picture.rgba[at * 4..at * 4 + 3],
                    frame.pixel((at as u32) % SIDE, (at as u32) / SIDE),
                    wanted[at]
                );
            }
        }
    });
}

#[test]
fn a_table_with_a_domain_maps_the_colours_from_the_domain_to_the_table_in_the_pass_as_it_does_on_the_processor() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let picture = the_colours();
        upload(&mut renderer, 1, &picture.as_picture_data());
        let text = text_of_a_3d_table(5, "DOMAIN_MIN 0.1 0.0 0.2\nDOMAIN_MAX 0.9 1.0 0.8\n", |r, g, b| {
            [r * 1.3 - 0.1, g, (b * b) * 1.2]
        });
        let table = Arc::new(parse_cube(&text).unwrap());
        let id = published(&table);

        let (frame, _) = render(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            vec![with(vec![the_codes_screen(id)])],
        );

        let wanted = what_the_reference_makes_of(&table, &picture, Interpolation::Tetrahedral);
        let (largest, _) = the_largest_difference(&frame, &wanted);
        assert!(largest <= 1, "{largest} levels");
    });
}

#[test]
fn a_1d_table_that_is_longer_than_a_row_of_the_strip_is_read_in_all_its_rows() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 700 points: the strip is 256 wide, so this is three rows; the table is the identity, so a mistake in the rows shows at once.
        let mut text = String::from("LUT_1D_SIZE 700\n");
        for step in 0..700 {
            let x = step as f32 / 699.0;
            text.push_str(&format!("{x:.7} {x:.7} {x:.7}\n"));
        }
        let table = Arc::new(parse_cube(&text).unwrap());
        let id = published(&table);
        let picture = the_colours();
        upload(&mut renderer, 1, &picture.as_picture_data());

        let (frame, _) = render(
            &mut renderer,
            WorkingSpace::DisplayReferred,
            vec![with(vec![the_codes_screen(id)])],
        );

        let same: Vec<[u8; 3]> = picture
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]])
            .collect();
        let (largest, _) = the_largest_difference(&frame, &same);
        assert!(largest <= 1, "{largest} levels");
    });
}

// ---- the spaces ----

fn a_flat_picture_frame(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    code: u8,
    effects: Vec<EffectCall>,
) -> (RenderedFrame, FrameStatistics) {
    upload(
        renderer,
        1,
        &picture_for(
            working,
            SIDE,
            SIDE,
            [code, code, code, 255].repeat((SIDE * SIDE) as usize),
        ),
    );
    render(renderer, working, vec![with(effects)])
}

fn code_of_light(light: f64) -> f64 {
    TransferFunction::Srgb.encode_from_light(light.clamp(0.0, 1.0)) * 255.0
}

fn light_of_code(code: u8) -> f64 {
    TransferFunction::Srgb.decode_to_light(f64::from(code) / 255.0)
}

#[test]
fn the_identity_table_in_the_same_space_on_both_sides_changes_nothing_in_any_space() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let table = the_identity_3d(17);
        let id = published(&table);
        for working in [
            WorkingSpace::DisplayReferred,
            LINEAR_REC709,
            WorkingSpace::Linear(StandardPrimaries::AcesCg),
        ] {
            for space in LutSpace::ALL {
                for code in [0_u8, 7, 100, 200, 255] {
                    let (looked_up, _) = a_flat_picture_frame(
                        &mut renderer,
                        working,
                        code,
                        vec![the_lut(id, 1.0, space, space, LutInterpolation::Tetrahedral)],
                    );
                    let (plain, _) = a_flat_picture_frame(&mut renderer, working, code, Vec::new());
                    let (a, b) = (looked_up.pixel(10, 10), plain.pixel(10, 10));
                    assert!(
                        (0..3).all(|place| a[place].abs_diff(b[place]) <= 2),
                        "{working:?}, {space:?}, code {code}: {a:?} where it was {b:?}"
                    );
                }
            }
        }
    });
}

#[test]
fn a_table_is_taken_the_colours_in_the_space_that_the_call_says_and_gives_them_back_in_the_space_that_the_call_says() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The identity table, so that what the table does to the colour is nothing, and only the two spaces make a difference: the colour goes in as one space's numbers and
        // comes out as the same numbers read as another space's.
        let table = the_identity_3d(33);
        let id = published(&table);
        for code in [30_u8, 100, 180, 250] {
            let light = light_of_code(code);
            let video_code = light.powf(1.0 / 2.4);
            for (input, output, shown_as_light) in [
                // screen codes in, linear light out: the codes are read as light, so the picture is the light that the codes are (darker)
                (LutSpace::Screen, LutSpace::Linear, f64::from(code) / 255.0),
                // linear light in, screen codes out: the light is read as codes, so it is brighter than it was
                (
                    LutSpace::Linear,
                    LutSpace::Screen,
                    TransferFunction::Srgb.decode_to_light(light),
                ),
                // screen codes in, video codes out: the codes are read as gamma 2.4 codes
                (LutSpace::Screen, LutSpace::Video, (f64::from(code) / 255.0).powf(2.4)),
                // video codes in, screen codes out
                (
                    LutSpace::Video,
                    LutSpace::Screen,
                    TransferFunction::Srgb.decode_to_light(video_code),
                ),
            ] {
                let (frame, _) = a_flat_picture_frame(
                    &mut renderer,
                    LINEAR_REC709,
                    code,
                    vec![the_lut(id, 1.0, input, output, LutInterpolation::Tetrahedral)],
                );
                let wanted = code_of_light(shown_as_light);
                let found = f64::from(frame.pixel(10, 10)[0]);
                assert!(
                    (found - wanted).abs() <= 2.0,
                    "{input:?} to {output:?}, code {code}: {found} where {wanted:.1} was wanted"
                );
            }
        }
    });
}

#[test]
fn a_table_of_linear_light_gives_back_light_above_one_and_a_table_of_codes_is_cut_off_at_one() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A table that makes every colour 2.0, and then an exposure of minus one stop, which halves the light. If the table's 2.0 was cut off at 1 the light would be 0.5 and
        // the picture is 188 of 255; kept, it is 1.0 and the picture is 255.
        let twice = Arc::new(parse_cube("LUT_1D_SIZE 2\n2 2 2\n2 2 2\n").unwrap());
        let id = published(&twice);
        let (kept, _) = a_flat_picture_frame(
            &mut renderer,
            LINEAR_REC709,
            100,
            vec![
                the_lut(
                    id,
                    1.0,
                    LutSpace::Linear,
                    LutSpace::Linear,
                    LutInterpolation::Tetrahedral,
                ),
                EffectCall::exposure(-1.0),
            ],
        );
        assert_eq!(kept.pixel(3, 3), [255, 255, 255, 255]);
        let (cut, _) = a_flat_picture_frame(
            &mut renderer,
            LINEAR_REC709,
            100,
            vec![
                the_lut(
                    id,
                    1.0,
                    LutSpace::Linear,
                    LutSpace::Screen,
                    LutInterpolation::Tetrahedral,
                ),
                EffectCall::exposure(-1.0),
            ],
        );
        let wanted = code_of_light(TransferFunction::Srgb.decode_to_light(1.0) * 0.5);
        assert!(
            (f64::from(cut.pixel(3, 3)[0]) - wanted).abs() <= 2.0,
            "{:?} where {wanted:.1}",
            cut.pixel(3, 3)
        );
        // And a table of codes that gives less than 0 gives black, not a negative light.
        let below = Arc::new(parse_cube("LUT_1D_SIZE 2\n-1 -1 -1\n-1 -1 -1\n").unwrap());
        let below_id = published(&below);
        let (black, _) = a_flat_picture_frame(&mut renderer, LINEAR_REC709, 100, vec![the_codes_screen(below_id)]);
        assert_eq!(black.pixel(3, 3), [0, 0, 0, 255]);
    });
}

// ---- the intensity ----

#[test]
fn an_intensity_mixes_the_light_that_went_in_and_the_light_that_came_out() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A table that makes every colour white.
        let white = Arc::new(parse_cube("LUT_1D_SIZE 2\n1 1 1\n1 1 1\n").unwrap());
        let id = published(&white);
        let code = 60;
        for intensity in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let (frame, statistics) = a_flat_picture_frame(
                &mut renderer,
                LINEAR_REC709,
                code,
                vec![the_lut(
                    id,
                    intensity,
                    LutSpace::Screen,
                    LutSpace::Screen,
                    LutInterpolation::Tetrahedral,
                )],
            );
            let light = light_of_code(code) * f64::from(1.0 - intensity) + f64::from(intensity);
            let wanted = code_of_light(light);
            assert!(
                (f64::from(frame.pixel(5, 5)[0]) - wanted).abs() <= 2.0,
                "intensity {intensity}: {:?} where {wanted:.1} was wanted",
                frame.pixel(5, 5)
            );
            if intensity == 0.0 {
                assert_eq!(
                    statistics.passes_recorded, 2,
                    "none of the table is wanted: nothing is run"
                );
                assert_eq!(statistics.effects_not_run, 0);
            }
        }
    });
}

// ---- the plan ----

#[test]
fn a_table_of_the_codes_of_a_screen_shares_the_conversions_with_the_colour_matrices_next_to_it_and_any_other_table_does_not()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let table = the_identity_3d(9);
        let id = published(&table);
        let turn = || EffectCall::colour_matrix(ColourMatrix::hue_rotation(50.0).rows());
        let code = 140;
        // A matrix, the table and a matrix: all three are in the codes of a screen, so the first converts in and the last converts out and nobody converts between.
        let (_, around) = a_flat_picture_frame(
            &mut renderer,
            LINEAR_REC709,
            code,
            vec![turn(), the_codes_screen(id), turn()],
        );
        assert_eq!(around.passes_recorded, 2 + 3);
        // A table that wants less than all of it is in light: it converts for itself, so the matrices each convert for themselves.
        let (_, half) = a_flat_picture_frame(
            &mut renderer,
            LINEAR_REC709,
            code,
            vec![
                turn(),
                the_lut(
                    id,
                    0.5,
                    LutSpace::Screen,
                    LutSpace::Screen,
                    LutInterpolation::Tetrahedral,
                ),
                turn(),
            ],
        );
        assert_eq!(half.passes_recorded, 2 + 3);
        // The same when the table is of linear light.
        let (_, linear) = a_flat_picture_frame(
            &mut renderer,
            LINEAR_REC709,
            code,
            vec![
                turn(),
                the_lut(
                    id,
                    1.0,
                    LutSpace::Linear,
                    LutSpace::Linear,
                    LutInterpolation::Tetrahedral,
                ),
                turn(),
            ],
        );
        assert_eq!(linear.passes_recorded, 2 + 3);
        // And what comes out is the same picture, whichever way: the identity table changes nothing, so the picture is that of the two turns, to within the rounding of
        // the half floats the codes are kept in.
        let (with_the_table, _) = a_flat_picture_frame(
            &mut renderer,
            LINEAR_REC709,
            code,
            vec![turn(), the_codes_screen(id), turn()],
        );
        let (without, _) = a_flat_picture_frame(&mut renderer, LINEAR_REC709, code, vec![turn(), turn()]);
        let (a, b) = (with_the_table.pixel(5, 5), without.pixel(5, 5));
        assert!(
            (0..3).all(|place| a[place].abs_diff(b[place]) <= 2),
            "{a:?} where {b:?}"
        );
    });
}

#[test]
fn a_call_with_a_table_the_engine_does_not_have_or_with_numbers_that_mean_nothing_is_not_run_and_is_counted() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let table = the_identity_3d(5);
        let id = published(&table);
        let code = 90;
        let (plain, _) = a_flat_picture_frame(&mut renderer, LINEAR_REC709, code, Vec::new());
        let nobody_has_it = {
            let gone = the_identity_3d(2);
            let gone_id = published(&gone);
            drop(gone);
            gone_id
        };
        let mut call = |effect: EffectCall| a_flat_picture_frame(&mut renderer, LINEAR_REC709, code, vec![effect]);
        let mut no_table = the_codes_screen(id);
        no_table.table = None;
        let wrong_numbers = |values: Vec<f32>| EffectCall {
            values,
            ..the_codes_screen(id)
        };
        for (name, effect) in [
            ("a table nobody has", the_codes_screen(nobody_has_it)),
            (
                "a table that was never published",
                the_codes_screen(TableId(u64::MAX - 1)),
            ),
            ("no table", no_table),
            ("too few numbers", wrong_numbers(vec![1.0, 0.0, 0.0])),
            ("too many numbers", wrong_numbers(vec![1.0, 0.0, 0.0, 0.0, 0.0])),
            ("a space that is none", wrong_numbers(vec![1.0, 7.0, 0.0, 0.0])),
            ("a way that is none", wrong_numbers(vec![1.0, 0.0, 0.0, 3.0])),
            ("a number that is not one", wrong_numbers(vec![f32::NAN, 0.0, 0.0, 0.0])),
        ] {
            let (frame, statistics) = call(effect);
            assert_eq!(statistics.effects_not_run, 1, "{name}");
            assert_eq!(frame.rgba_pixels, plain.rgba_pixels, "{name}: the picture is as it was");
        }
    });
}

#[test]
fn the_card_keeps_the_tables_of_the_frames_in_use_and_lets_go_of_the_least_recently_used_when_there_are_too_many() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // More tables than the card keeps, each used in a frame of its own: every frame is made, and the first table is found again when it is asked for again.
        let tables: Vec<Arc<CubeTable>> = (0..super::MOST_TABLES_ON_THE_CARD + 4)
            .map(|number| {
                Arc::new(
                    parse_cube(&text_of_a_3d_table(2, "", |r, g, b| {
                        [r, g, (b + number as f32 / 100.0).min(1.0)]
                    }))
                    .unwrap(),
                )
            })
            .collect();
        let ids: Vec<TableId> = tables.iter().map(published).collect();
        let picture = the_colours();
        upload(&mut renderer, 1, &picture.as_picture_data());
        for round in 0..2 {
            for (number, id) in ids.iter().enumerate() {
                let (frame, statistics) = render(
                    &mut renderer,
                    WorkingSpace::DisplayReferred,
                    vec![with(vec![the_codes_screen(*id)])],
                );
                assert_eq!(statistics.effects_not_run, 0, "round {round}, table {number}");
                let wanted = what_the_reference_makes_of(&tables[number], &picture, Interpolation::Tetrahedral);
                let (largest, _) = the_largest_difference(&frame, &wanted);
                assert!(largest <= 1, "round {round}, table {number}: {largest} levels");
                assert!(
                    renderer.tables_on_the_card() <= super::MOST_TABLES_ON_THE_CARD,
                    "round {round}, table {number}"
                );
            }
        }
        assert_eq!(
            renderer.tables_on_the_card(),
            super::MOST_TABLES_ON_THE_CARD,
            "(as many as it keeps, not one fewer)"
        );
    });
}

#[test]
fn a_frame_that_asks_for_a_table_is_another_frame_than_one_that_asks_for_another_table() {
    let (first, second) = (the_identity_3d(2), the_identity_3d(2));
    let (a, b) = (published(&first), published(&second));
    assert_ne!(a, b);
    let frame = |id| {
        let mut description = FrameDescription::black((8, 8));
        description.layers = vec![with(vec![the_codes_screen(id)])];
        description
    };
    assert_ne!(frame(a).fingerprint(), frame(b).fingerprint());
    assert_eq!(frame(a).fingerprint(), frame(a).fingerprint());
}

/// A picture of 3 by 3 blocks of colours, some of them see-through, as straight (not premultiplied) 8-bit colours.
fn a_picture_of_colours_some_see_through() -> Vec<u8> {
    let colours: [[u8; 4]; 9] = [
        [200, 60, 30, 255],
        [10, 220, 90, 255],
        [40, 40, 250, 255],
        [200, 100, 50, 128],
        [30, 160, 220, 64],
        [255, 255, 255, 200],
        [200, 100, 50, 0],
        [0, 0, 0, 0],
        [250, 20, 140, 255],
    ];
    let block = (SIDE / 3) as usize;
    let mut rgba = Vec::new();
    for row in 0..SIDE as usize {
        for column in 0..SIDE as usize {
            rgba.extend(colours[(row / block).min(2) * 3 + (column / block).min(2)]);
        }
    }
    rgba
}

#[test]
fn the_identity_table_changes_no_colour_that_is_not_grey_in_any_space_or_working_space_and_none_that_is_see_through() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let table = the_identity_3d(17);
        let id = published(&table);
        for working in [
            WorkingSpace::DisplayReferred,
            LINEAR_REC709,
            WorkingSpace::Linear(StandardPrimaries::AcesCg),
        ] {
            upload(
                &mut renderer,
                1,
                &picture_for(working, SIDE, SIDE, a_picture_of_colours_some_see_through()),
            );
            let (plain, _) = render(&mut renderer, working, vec![with(Vec::new())]);
            for space in LutSpace::ALL {
                let (looked_up, _) = render(
                    &mut renderer,
                    working,
                    vec![with(vec![the_lut(
                        id,
                        1.0,
                        space,
                        space,
                        LutInterpolation::Tetrahedral,
                    )])],
                );
                for (x, y) in [
                    (5, 5),
                    (30, 5),
                    (50, 5),
                    (5, 30),
                    (30, 30),
                    (50, 30),
                    (5, 50),
                    (30, 50),
                    (50, 50),
                ] {
                    let (a, b) = (looked_up.pixel(x, y), plain.pixel(x, y));
                    assert!(
                        (0..3).all(|place| a[place].abs_diff(b[place]) <= 3),
                        "{working:?}, {space:?}, at {x},{y}: {a:?} where it was {b:?}"
                    );
                }
            }
        }
    });
}

#[test]
fn the_spaces_of_a_table_are_converted_with_the_matrices_that_go_to_the_space_and_come_back_from_it() {
    let working = WorkingSpace::Linear(StandardPrimaries::AcesCg);
    let conversion = super::colour::LutSpaceConversion::of(working, LutSpace::Screen, LutSpace::Screen);
    let to_rec709 = working.matrix_to(StandardPrimaries::Rec709);
    let from_rec709 = working.matrix_from(StandardPrimaries::Rec709);
    assert_eq!(conversion.to_input, crate::colour_space::matrix::to_f32(&to_rec709));
    assert_eq!(
        conversion.from_output,
        crate::colour_space::matrix::to_f32(&from_rec709)
    );
    assert_ne!(conversion.to_input, conversion.from_output);
    // (A table of linear light is in the working space: nothing to convert.)
    let identity = crate::colour_space::matrix::to_f32(&crate::colour_space::matrix::IDENTITY);
    let linear = super::colour::LutSpaceConversion::of(working, LutSpace::Linear, LutSpace::Linear);
    assert_eq!((linear.to_input, linear.from_output), (identity, identity));
}

#[test]
fn the_output_of_a_table_is_cut_off_where_it_is_codes_and_not_where_it_is_light_in_the_working_space() {
    use super::colour::LutSpaceConversion;
    let linear_working = LINEAR_REC709;
    let clamps = |working, input, output| LutSpaceConversion::of(working, input, output).clamps_the_output;
    assert!(clamps(linear_working, LutSpace::Screen, LutSpace::Screen));
    assert!(clamps(linear_working, LutSpace::Linear, LutSpace::Video));
    assert!(!clamps(linear_working, LutSpace::Screen, LutSpace::Linear));
    assert!(
        clamps(WorkingSpace::DisplayReferred, LutSpace::Screen, LutSpace::Linear),
        "(the compatibility mode has only codes)"
    );
}

#[test]
fn only_a_whole_table_of_screen_codes_into_screen_codes_works_in_the_domain_of_the_screen() {
    use super::plan::{CallDomain, KindOfEffect};
    let id = published(&the_identity_3d(2));
    let domain_of = |intensity: f32, input: LutSpace, output: LutSpace| {
        let call = the_lut(id, intensity, input, output, LutInterpolation::Tetrahedral);
        (
            KindOfEffect::Lut.domain_of_a_call(&call),
            KindOfEffect::Lut.converts_itself_for_a_call(&call),
        )
    };
    assert_eq!(
        domain_of(1.0, LutSpace::Screen, LutSpace::Screen),
        (CallDomain::DisplayReferred, true)
    );
    for (intensity, input, output) in [
        (0.99, LutSpace::Screen, LutSpace::Screen),
        (1.0, LutSpace::Video, LutSpace::Screen),
        (1.0, LutSpace::Screen, LutSpace::Linear),
    ] {
        assert_eq!(
            domain_of(intensity, input, output),
            (CallDomain::SceneLinear, false),
            "{intensity} {input:?} {output:?}"
        );
    }
}

#[test]
fn the_number_of_passes_of_each_kind_of_effect_is_what_each_takes() {
    use super::plan::KindOfEffect;
    for (kind, passes) in [
        (KindOfEffect::Blur, 2),
        (KindOfEffect::ColourMatrix, 1),
        (KindOfEffect::Shader, 1),
        (KindOfEffect::Exposure, 1),
        (KindOfEffect::Opacity, 1),
        (KindOfEffect::Crop, 1),
        (KindOfEffect::Lut, 1),
    ] {
        assert_eq!(kind.passes(), passes, "{kind:?}");
    }
}
