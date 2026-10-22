// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A shader that reads what is behind its layer (a transition: `startImage` is the backdrop, `endImage` the layer's own picture; phase 6.6, the design is
//! `docs/engine-phase-6-6-design.md`, T1 to T5), made on the software adapter.
//!
//! The shader here is the mix of its two pictures by its first number, which is what a transition at its simplest is. Two things are held to oracles that do not depend on the
//! colours of the numbers: **at progress 0 the frame is what the layers behind make, and at progress 1 it is the layer over them** (the frame of the same layers with no effect),
//! which is the arithmetic of "over" (T2), in the compatibility mode and in linear light. In between, the mix is done on the codes of a screen, as every shader's is.

use super::effect_tests::{matrix_effect, noisy, whole_frame};
use super::foreign_effect_tests::{call, guarded};
use super::linear_light_tests::{colour_of, picture_for, upload};
use super::oracle_tests::{TestPicture, upright};
use super::testing::with_software_graphics;
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, FrameDescription, FrameRenderer,
    GroupDescription, ImageSlot, LayerDescription, LayerSource, PassSpec, PictureId, RenderedFrame, ShaderStructure,
    WorkingSpace,
};
use crate::colour_effects::ColourMatrix;

const THE_WIPE: &str = "test:wipe";

/// What a transition is given: the numbers of its call, what is behind the layer (and its sampler) and the layer's own picture (and its sampler).
const THE_BINDINGS: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 17>;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var backdrop_sampler: sampler;
@group(0) @binding(3) var picture: texture_2d<f32>;
@group(0) @binding(4) var picture_sampler: sampler;
";

/// The mix of what is behind and the picture by the first number of the call, each read where the pixel is, as a fraction of the picture it is read from.
const MIX_BY_THE_FIRST_NUMBER: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / vec2<f32>(textureDimensions(picture));
    let start = textureSample(backdrop, backdrop_sampler, uv);
    let end = textureSample(picture, picture_sampler, uv);
    return mix(start, end, numbers[0].x);
}
";

fn the_structure_of_a_transition() -> ShaderStructure {
    ShaderStructure {
        images: vec![ImageSlot::Backdrop, ImageSlot::Picture],
        buffers: Vec::new(),
        passes: vec![PassSpec {
            target: None,
            width: None,
            height: None,
        }],
        pass_index_number: None,
    }
}

fn installed_wipe(renderer: &mut FrameRenderer) {
    renderer
        .install_effect_with_structure(
            THE_WIPE,
            &guarded(
                &format!("{THE_BINDINGS}\n{MIX_BY_THE_FIRST_NUMBER}"),
                DEFAULT_MOST_PASSES_OF_A_LOOP,
            ),
            &the_structure_of_a_transition(),
        )
        .unwrap();
}

fn wipe(progress: f32) -> EffectCall {
    call(THE_WIPE, &[progress])
}

const SIZE: (u32, u32) = (8, 8);
const RED: [u8; 4] = [200, 40, 40, 255];
const GREEN: [u8; 4] = [40, 200, 40, 255];
const BLUE: [u8; 4] = [40, 40, 200, 255];

fn flat_picture(size: (u32, u32), colour: [u8; 4]) -> TestPicture {
    TestPicture {
        width: size.0,
        height: size.1,
        rgba: colour.repeat((size.0 * size.1) as usize),
    }
}

/// Red on the left half, blue on the right half.
fn red_and_blue(size: (u32, u32)) -> TestPicture {
    let mut rgba = Vec::new();
    for _ in 0..size.1 {
        for across in 0..size.0 {
            rgba.extend(if across < size.0 / 2 { RED } else { BLUE });
        }
    }
    TestPicture {
        width: size.0,
        height: size.1,
        rgba,
    }
}

/// How many levels (of 8 bits) the two frames differ by at most, in any number.
fn the_largest_difference(a: &RenderedFrame, b: &RenderedFrame) -> u8 {
    assert_eq!(a.rgba_pixels.len(), b.rgba_pixels.len());
    a.rgba_pixels
        .iter()
        .zip(&b.rgba_pixels)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
}

/// How far the compatibility mode and linear light may be from the oracle: a picture that is made into codes and light and back through half floats comes out within a level or two.
fn most_levels_off(working: WorkingSpace) -> u8 {
    match working {
        WorkingSpace::DisplayReferred => 1,
        WorkingSpace::Linear(_) => 2,
    }
}

const LIGHT: WorkingSpace = WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709);

/// Both working spaces, each with a renderer of its own that has the wipe and the pictures `(id, picture)` (made for the space).
fn each_working_space(pictures: &[(u64, &TestPicture)], check: impl FnMut(WorkingSpace, &mut FrameRenderer)) {
    each_working_space_with(installed_wipe, pictures, check);
}

/// The same, with the shaders that `install` puts in each renderer.
fn each_working_space_with(
    install: impl Fn(&mut FrameRenderer),
    pictures: &[(u64, &TestPicture)],
    mut check: impl FnMut(WorkingSpace, &mut FrameRenderer),
) {
    for working in [WorkingSpace::DisplayReferred, LIGHT] {
        with_software_graphics!(graphics, {
            let mut renderer = FrameRenderer::new(graphics.device.clone());
            install(&mut renderer);
            for (id, picture) in pictures {
                upload(
                    &mut renderer,
                    *id,
                    &picture_for(working, picture.width, picture.height, picture.rgba.clone()),
                );
            }
            check(working, &mut renderer);
        });
    }
}

fn frame_in(
    renderer: &mut FrameRenderer,
    working: WorkingSpace,
    layers: Vec<LayerDescription>,
) -> (RenderedFrame, usize, usize) {
    let mut description = FrameDescription::black(SIZE);
    description.colour = colour_of(working);
    description.layers = layers;
    let (frame, statistics) = renderer
        .render(&description, std::time::Duration::from_secs(60))
        .unwrap();
    (frame, statistics.effects_not_run, statistics.passes_recorded)
}

fn a_group(layers: Vec<LayerDescription>, effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Group(GroupDescription { layers }),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects,
    }
}

fn the_top_right_quarter(picture: u64, effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: upright(0.5, 0.0, 0.5, 0.5),
        effects,
    }
}

#[test]
fn at_progress_nought_the_frame_is_what_the_layers_behind_make_and_at_progress_one_it_is_the_layer_over_them() {
    let backdrop = TestPicture::gradient(SIZE.0, SIZE.1);
    // (The clip has opaque pixels, clear ones, and ones that are partly clear: what "over" is made of.)
    let clip = noisy(SIZE.0, SIZE.1, 5, true);
    each_working_space(&[(1, &backdrop), (2, &clip)], |working, renderer| {
        let behind = frame_in(renderer, working, vec![whole_frame(1, Vec::new())]).0;
        let over = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), whole_frame(2, Vec::new())],
        )
        .0;
        assert!(
            the_largest_difference(&behind, &over) > 40,
            "{working:?}: (the oracles differ, or the test would not tell one from the other)"
        );
        let (at_nought, not_run, _) = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe(0.0)])],
        );
        assert_eq!(not_run, 0);
        let off = the_largest_difference(&at_nought, &behind);
        assert!(
            off <= most_levels_off(working),
            "{working:?}: at progress 0 the frame is {off} levels from what is behind the layer"
        );
        let at_one = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe(1.0)])],
        )
        .0;
        let off = the_largest_difference(&at_one, &over);
        assert!(
            off <= most_levels_off(working),
            "{working:?}: at progress 1 the frame is {off} levels from the layer over what is behind it"
        );
    });
}

#[test]
fn between_the_two_the_codes_of_what_is_behind_and_of_the_layer_are_mixed_by_the_progress() {
    let (red, green) = (flat_picture(SIZE, RED), flat_picture(SIZE, GREEN));
    each_working_space(&[(1, &red), (2, &green)], |working, renderer| {
        for (progress, expected) in [
            (0.25, [160, 80, 40, 255]),
            (0.5, [120, 120, 40, 255]),
            (0.75, [80, 160, 40, 255]),
        ] {
            let (frame, _, _) = frame_in(
                renderer,
                working,
                vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe(progress)])],
            );
            let off = (0..4)
                .map(|number| frame.pixel(3, 3)[number].abs_diff(expected[number]))
                .max()
                .unwrap();
            assert!(
                off <= most_levels_off(working),
                "{working:?}, progress {progress}: {:?} where {expected:?} is the mix of the codes",
                frame.pixel(3, 3)
            );
            assert!(
                frame
                    .rgba_pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == frame.pixel(3, 3)),
                "{working:?}, progress {progress}: every pixel is the same, as the pictures are flat"
            );
        }
    });
}

/// What the frame is where a layer of `clip` (straight colour, opaque or not) is partly see-through, with the transition at `progress` over `behind` (opaque), by the arithmetic of T2:
/// `(1 - progress) * behind + progress * (clip over behind)`, in the numbers of the codes (the compatibility mode).
fn the_mix_of_what_is_behind_and_the_clip_over_it(behind: [u8; 4], clip: [u8; 4], progress: f32) -> [u8; 3] {
    let opacity = f32::from(clip[3]) / 255.0;
    let mut mixed = [0; 3];
    for colour in 0..3 {
        let (behind, clip) = (f32::from(behind[colour]), f32::from(clip[colour]));
        let clip_over_behind = clip * opacity + behind * (1.0 - opacity);
        mixed[colour] = ((1.0 - progress) * behind + progress * clip_over_behind).round() as u8;
    }
    mixed
}

#[test]
fn where_the_layer_is_partly_clear_the_frame_is_the_mix_of_what_is_behind_and_the_layer_over_it() {
    let backdrop = flat_picture(SIZE, RED);
    let clip = flat_picture(SIZE, [40, 200, 40, 128]);
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_wipe(&mut renderer);
        upload(
            &mut renderer,
            1,
            &picture_for(WorkingSpace::DisplayReferred, SIZE.0, SIZE.1, backdrop.rgba.clone()),
        );
        upload(
            &mut renderer,
            2,
            &picture_for(WorkingSpace::DisplayReferred, SIZE.0, SIZE.1, clip.rgba.clone()),
        );
        for progress in [0.0, 0.25, 0.5, 1.0] {
            let (frame, not_run, _) = frame_in(
                &mut renderer,
                WorkingSpace::DisplayReferred,
                vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe(progress)])],
            );
            assert_eq!(not_run, 0);
            let expected = the_mix_of_what_is_behind_and_the_clip_over_it(RED, [40, 200, 40, 128], progress);
            let [red, green, blue, opacity] = frame.pixel(2, 2);
            for (got, wanted, which) in [
                (red, expected[0], "red"),
                (green, expected[1], "green"),
                (blue, expected[2], "blue"),
            ] {
                assert!(
                    got.abs_diff(wanted) <= 1,
                    "progress {progress}: {which} is {got} where the arithmetic of over says {wanted}"
                );
            }
            assert_eq!(opacity, 255);
        }
    });
}

#[test]
fn the_effects_before_the_transition_are_done_to_the_picture_of_the_layer_and_the_effects_after_it_to_what_it_made() {
    let (red, green) = (flat_picture(SIZE, RED), flat_picture(SIZE, GREEN));
    // A matrix that swaps the red and the green.
    let swap_red_and_green = || {
        matrix_effect(ColourMatrix::from_rows([
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        ]))
    };
    each_working_space(&[(1, &red), (2, &green)], |working, renderer| {
        // Before: the layer is green, swapped it is red; at progress 1 the frame is that.
        let (before, not_run, _) = frame_in(
            renderer,
            working,
            vec![
                whole_frame(1, Vec::new()),
                whole_frame(2, vec![swap_red_and_green(), wipe(1.0)]),
            ],
        );
        assert_eq!(not_run, 0);
        let off = the_largest_difference(
            &before,
            &frame_in(renderer, working, vec![whole_frame(1, Vec::new())]).0,
        );
        assert!(
            off <= most_levels_off(working),
            "{working:?}: the swapped green layer is red, and the frame (all red behind it) is as it was: {off} levels off"
        );
        // After: at progress 0 the transition makes what is behind (red), and the swap is done to that, so the layer is green over the red, and the frame is green.
        let (after, _, _) = frame_in(
            renderer,
            working,
            vec![
                whole_frame(1, Vec::new()),
                whole_frame(2, vec![wipe(0.0), swap_red_and_green()]),
            ],
        );
        let green_frame = frame_in(renderer, working, vec![whole_frame(2, Vec::new())]).0;
        let off = the_largest_difference(&after, &green_frame);
        assert!(
            off <= most_levels_off(working),
            "{working:?}: the effects after the transition are done to what the transition made (the backdrop, red, swapped to green): {off} levels off"
        );
    });
}

#[test]
fn a_transition_on_a_layer_that_is_a_group_of_the_clip_reads_the_frame_where_the_clip_is_and_changes_nothing_where_it_is_not()
 {
    // The way that the describer makes a layer that has a transition: the clip, in the top right quarter of the frame, is the one layer of a group that is as big as the frame,
    // and the transition is an effect of the group's layer. What is behind is red on the left and blue on the right.
    let backdrop = red_and_blue(SIZE);
    let clip = flat_picture((4, 4), GREEN);
    each_working_space(&[(1, &backdrop), (2, &clip)], |working, renderer| {
        let a_clip_with_a_transition =
            |progress: f32| a_group(vec![the_top_right_quarter(2, Vec::new())], vec![wipe(progress)]);
        let behind = frame_in(renderer, working, vec![whole_frame(1, Vec::new())]).0;
        let over = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), the_top_right_quarter(2, Vec::new())],
        )
        .0;
        let at_nought = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), a_clip_with_a_transition(0.0)],
        )
        .0;
        assert!(
            the_largest_difference(&at_nought, &behind) <= most_levels_off(working),
            "{working:?}: at progress 0 the frame is what is behind, wherever the clip is"
        );
        let at_one = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), a_clip_with_a_transition(1.0)],
        )
        .0;
        assert!(
            the_largest_difference(&at_one, &over) <= most_levels_off(working),
            "{working:?}: at progress 1 the frame is the clip, where it is, over what is behind"
        );
        // Half way: in the quarter the clip (green) is mixed with what is behind it there (blue: the right half), and everywhere else the frame is what is behind it.
        let (halfway, not_run, _) = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), a_clip_with_a_transition(0.5)],
        );
        assert_eq!(not_run, 0);
        let near = |got: [u8; 4], wanted: [u8; 4], what: &str| {
            let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
            assert!(
                off <= most_levels_off(working),
                "{working:?}: {what}: {got:?} where {wanted:?} is wanted"
            );
        };
        near(
            halfway.pixel(6, 1),
            [40, 120, 120, 255],
            "in the quarter, the clip mixed with the blue that is behind it",
        );
        near(
            halfway.pixel(1, 1),
            RED,
            "in the top left, where the clip is not: what is behind (red)",
        );
        near(
            halfway.pixel(6, 6),
            BLUE,
            "in the bottom right, where the clip is not: what is behind (blue)",
        );
        near(
            halfway.pixel(1, 6),
            RED,
            "in the bottom left, where the clip is not: what is behind (red)",
        );
    });
}

#[test]
fn a_transition_on_a_layer_that_is_not_the_frame_reads_what_is_behind_by_the_coordinates_of_its_own_picture() {
    // What the engine does with a layer that is smaller than the frame and has the transition itself (which is why the describer makes the layer a group): its picture is 4 by 4,
    // placed in the top right quarter, and the shader reads the backdrop by the coordinates of that picture, so the quarter shows the whole of what is behind squeezed into it.
    let backdrop = red_and_blue(SIZE);
    let clip = flat_picture((4, 4), GREEN);
    each_working_space(&[(1, &backdrop), (2, &clip)], |working, renderer| {
        let (frame, not_run, _) = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), the_top_right_quarter(2, vec![wipe(0.0)])],
        );
        assert_eq!(not_run, 0);
        let near = |got: [u8; 4], wanted: [u8; 4], what: &str| {
            let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
            assert!(
                off <= 2 * most_levels_off(working),
                "{working:?}: {what}: {got:?} where {wanted:?} is wanted"
            );
        };
        near(
            frame.pixel(4, 1),
            RED,
            "the left of the quarter shows the left of the frame squeezed (red)",
        );
        near(
            frame.pixel(7, 1),
            BLUE,
            "the right of the quarter shows the right of the frame squeezed (blue)",
        );
        near(frame.pixel(1, 1), RED, "the top left is what it was");
        near(frame.pixel(6, 6), BLUE, "the bottom right is what it was");
    });
}

#[test]
fn a_layer_that_reads_the_backdrop_reads_the_layers_behind_it_and_the_layers_in_front_of_it_are_put_over_what_it_made()
{
    // Red behind, then the transition layer (green) at progress 0.5, then a blue square in the top left corner that is in front of it.
    let (red, green) = (flat_picture(SIZE, RED), flat_picture(SIZE, GREEN));
    let blue = flat_picture((2, 2), BLUE);
    each_working_space(&[(1, &red), (2, &green), (3, &blue)], |working, renderer| {
        let square = LayerDescription {
            source: LayerSource::Picture(PictureId(3)),
            corners: upright(0.0, 0.0, 0.25, 0.25),
            effects: Vec::new(),
        };
        let (frame, not_run, passes) = frame_in(
            renderer,
            working,
            vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe(0.5)]), square],
        );
        assert_eq!(not_run, 0);
        let near = |got: [u8; 4], wanted: [u8; 4], what: &str| {
            let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
            assert!(
                off <= most_levels_off(working),
                "{working:?}: {what}: {got:?} where {wanted:?} is wanted"
            );
        };
        near(
            frame.pixel(5, 5),
            [120, 120, 40, 255],
            "the mix of red and green, where nothing is in front",
        );
        near(frame.pixel(0, 0), BLUE, "the square in front of the transition's layer");
        // (The layer behind and the layers in front: two put in their places, the backdrop and the call, and the output; light has the two conversions of the picture besides.)
        let in_light = matches!(working, WorkingSpace::Linear(_));
        assert_eq!(passes, 1 + 2 + 2 * usize::from(in_light) + 1 + 1, "{working:?}");
    });
}

#[test]
fn several_layers_that_read_the_backdrop_each_read_what_the_layers_up_to_it_made() {
    let (red, green, blue) = (
        flat_picture(SIZE, RED),
        flat_picture(SIZE, GREEN),
        flat_picture(SIZE, BLUE),
    );
    each_working_space(&[(1, &red), (2, &green), (3, &blue)], |working, renderer| {
        let (frame, not_run, _) = frame_in(
            renderer,
            working,
            vec![
                whole_frame(1, Vec::new()),
                whole_frame(2, vec![wipe(0.5)]),
                whole_frame(3, vec![wipe(0.5)]),
            ],
        );
        assert_eq!(not_run, 0);
        // The first mixes red and green half and half: (120, 120, 40); the second mixes that and blue (40, 40, 200): (80, 80, 120).
        let wanted = [80, 80, 120, 255];
        let off = (0..4)
            .map(|number| frame.pixel(4, 4)[number].abs_diff(wanted[number]))
            .max()
            .unwrap();
        assert!(
            off <= 2 * most_levels_off(working),
            "{working:?}: {:?} where {wanted:?} is wanted",
            frame.pixel(4, 4)
        );
    });
}

#[test]
fn a_layer_at_the_very_back_that_reads_the_backdrop_reads_the_opaque_black_that_every_frame_begins_as() {
    let green = flat_picture(SIZE, GREEN);
    each_working_space(&[(2, &green)], |working, renderer| {
        let (frame, not_run, _) = frame_in(renderer, working, vec![whole_frame(2, vec![wipe(0.5)])]);
        assert_eq!(not_run, 0);
        // (Half of the green and half of black: it is mixed in codes, and the green is 40, 200, 40.)
        let wanted = [20, 100, 20, 255];
        let off = (0..4)
            .map(|number| frame.pixel(4, 4)[number].abs_diff(wanted[number]))
            .max()
            .unwrap();
        assert!(
            off <= 2 * most_levels_off(working),
            "{working:?}: {:?} where {wanted:?} is wanted",
            frame.pixel(4, 4)
        );
    });
}

#[test]
fn a_shader_that_reads_the_backdrop_is_not_run_in_a_group_and_the_group_is_drawn_without_it() {
    let (red, green) = (flat_picture(SIZE, RED), flat_picture(SIZE, GREEN));
    each_working_space(&[(1, &red), (2, &green)], |working, renderer| {
        let (with_the_wipe, not_run, _) = frame_in(
            renderer,
            working,
            vec![
                whole_frame(1, Vec::new()),
                a_group(vec![whole_frame(2, vec![wipe(0.5)])], Vec::new()),
            ],
        );
        assert_eq!(
            not_run, 1,
            "{working:?}: the wipe is counted as an effect that did not run"
        );
        let without = frame_in(
            renderer,
            working,
            vec![
                whole_frame(1, Vec::new()),
                a_group(vec![whole_frame(2, Vec::new())], Vec::new()),
            ],
        )
        .0;
        assert_eq!(with_the_wipe.rgba_pixels, without.rgba_pixels, "{working:?}");
    });
}

#[test]
fn a_frame_with_a_transition_is_the_same_frame_every_time_it_is_made_and_leaves_no_layer_without_its_picture() {
    // (The pictures of the pool are reused from one frame to the next: the backdrop that one frame made is not what the next one reads.)
    let (backdrop, clip) = (
        TestPicture::gradient(SIZE.0, SIZE.1),
        TestPicture::checkers(SIZE.0, SIZE.1, 2),
    );
    each_working_space(&[(1, &backdrop), (2, &clip)], |working, renderer| {
        let layers = || vec![whole_frame(1, Vec::new()), whole_frame(2, vec![wipe(0.3)])];
        let first = frame_in(renderer, working, layers()).0;
        // Another frame in between, with another backdrop and clip.
        let _ = frame_in(
            renderer,
            working,
            vec![whole_frame(2, Vec::new()), whole_frame(1, vec![wipe(0.9)])],
        );
        let again = frame_in(renderer, working, layers()).0;
        assert_eq!(first.rgba_pixels, again.rgba_pixels, "{working:?}");
    });
}

// --- from the ISF file to the picture ---

/// A transition that is the mix of its two pictures by its progress, as the files of the collections that are the plainest are.
const MIX_TRANSITION: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"startImage\", \"TYPE\": \"image\" }, { \"NAME\": \"endImage\", \"TYPE\": \"image\" }, { \"NAME\": \"progress\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.0 } ] }*/
void main() { gl_FragColor = mix(IMG_THIS_PIXEL(startImage), IMG_THIS_PIXEL(endImage), progress); }
";

/// The soft diagonal wipe of the fixtures of the ISF host, whose picture at progress 0 is not all of `startImage` (its soft edge reaches into the bottom right corner).
const SOFT_WIPE: &str = include_str!("../isf/fixtures/wipe_transition.fs");

/// The numbers of a call of an ISF shader: the four of the clock and the pass, and then its inputs (an image takes none).
fn isf_call(key: &str, progress: f32) -> EffectCall {
    call(key, &[0.0, 0.0, 0.0, 0.0, progress])
}

#[test]
fn a_transition_that_is_an_isf_file_is_given_to_the_engine_as_one_that_reads_the_backdrop_and_draws_the_two_oracles() {
    let backdrop = TestPicture::gradient(SIZE.0, SIZE.1);
    let clip = noisy(SIZE.0, SIZE.1, 9, true);
    each_working_space_with(
        |renderer| super::isf_pass_tests::installed_from_isf(renderer, "isf:mix", MIX_TRANSITION),
        &[(1, &backdrop), (2, &clip)],
        |working, renderer| {
            let behind = frame_in(renderer, working, vec![whole_frame(1, Vec::new())]).0;
            let over = frame_in(
                renderer,
                working,
                vec![whole_frame(1, Vec::new()), whole_frame(2, Vec::new())],
            )
            .0;
            let (at_nought, not_run, _) = frame_in(
                renderer,
                working,
                vec![
                    whole_frame(1, Vec::new()),
                    whole_frame(2, vec![isf_call("isf:mix", 0.0)]),
                ],
            );
            assert_eq!(not_run, 0, "{working:?}");
            assert!(
                the_largest_difference(&at_nought, &behind) <= most_levels_off(working),
                "{working:?}: at progress 0 the frame is what is behind"
            );
            let at_one = frame_in(
                renderer,
                working,
                vec![
                    whole_frame(1, Vec::new()),
                    whole_frame(2, vec![isf_call("isf:mix", 1.0)]),
                ],
            )
            .0;
            assert!(
                the_largest_difference(&at_one, &over) <= most_levels_off(working),
                "{working:?}: at progress 1 the frame is the layer over what is behind"
            );
        },
    );
}

#[test]
fn the_soft_wipe_of_the_fixtures_shows_what_is_behind_before_it_begins_and_the_layer_after_it_ends() {
    let (backdrop, clip) = (flat_picture(SIZE, RED), flat_picture(SIZE, GREEN));
    each_working_space_with(
        |renderer| super::isf_pass_tests::installed_from_isf(renderer, "isf:soft_wipe", SOFT_WIPE),
        &[(1, &backdrop), (2, &clip)],
        |working, renderer| {
            // (Its progress is not held between 0 and 1 by the shader: before 0 and after 1 the soft edge is off the picture.)
            for (progress, wanted, what) in [(-0.2, RED, "before it begins"), (1.3, GREEN, "after it ends")] {
                let (frame, not_run, _) = frame_in(
                    renderer,
                    working,
                    vec![
                        whole_frame(1, Vec::new()),
                        whole_frame(2, vec![isf_call("isf:soft_wipe", progress)]),
                    ],
                );
                assert_eq!(not_run, 0, "{working:?}");
                for (across, down) in [(0, 0), (4, 4), (7, 7), (0, 7), (7, 0)] {
                    let got = frame.pixel(across, down);
                    let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
                    assert!(
                        off <= most_levels_off(working),
                        "{working:?}, {what}: the pixel ({across}, {down}) is {got:?} where {wanted:?} is wanted"
                    );
                }
            }
            // In between it is both: red where the wipe has not got to and green where it has, with a soft edge between them. (The normalised coordinates of an ISF file begin at the
            // bottom left, so the wipe goes from the bottom left to the top right, and the top left and the bottom right are on its edge.)
            let (halfway, _, _) = frame_in(
                renderer,
                working,
                vec![
                    whole_frame(1, Vec::new()),
                    whole_frame(2, vec![isf_call("isf:soft_wipe", 0.5)]),
                ],
            );
            let bottom_left = halfway.pixel(0, 7);
            let top_right = halfway.pixel(7, 0);
            assert!(
                bottom_left[0] > bottom_left[1],
                "{working:?}: the bottom left is still red: {bottom_left:?}"
            );
            assert!(
                top_right[1] > top_right[0],
                "{working:?}: the top right is green already: {top_right:?}"
            );
        },
    );
}
