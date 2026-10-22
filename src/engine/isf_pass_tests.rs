// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! ISF shaders with passes of their own, from the file to the picture, made on the software adapter and compared with the same arithmetic done on the processor (phase 6.4.1): the
//! spike's two-pass blur is a blur.

use super::effect_tests::{flat, frame_of, noisy, whole_frame};
use super::foreign_effect_tests::call;
use super::oracle_tests::{TestPicture, compare};
use super::testing::with_software_graphics;
use super::{DEFAULT_MOST_PASSES_OF_A_LOOP, FrameRenderer, guard_wgsl};
use crate::isf::{compile, read};

const BLUR: &str = include_str!("../isf/fixtures/gaussian_blur.fs");

/// The weights of the kernel of the blur of the spike (`gaussian_blur.fs`), for the centre and for each of the four taps on either side.
const WEIGHTS: [f32; 5] = [0.227_027_03, 0.194_594_6, 0.121_621_62, 0.054_054_054, 0.016_216_217];

/// Installs the ISF shader `source` in the renderer under `key`, the way the program does: compiled, with the structure that its header says.
pub(super) fn installed_from_isf(renderer: &mut FrameRenderer, key: &str, source: &str) {
    let compiled = compile(&read(source).unwrap()).unwrap_or_else(|problems| panic!("{problems:?}"));
    let structure = compiled
        .structure
        .clone()
        .unwrap_or_else(|problems| panic!("{problems:?}"));
    renderer
        .install_effect_with_structure(
            key,
            &guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap_or_else(|problem| panic!("{problem}")),
            &structure,
        )
        .unwrap_or_else(|problem| panic!("{key}: {problem}"));
}

/// One pass of the blur on the processor: every pixel is the weighted sum of the pixels `step` apart along a row (`along_rows`) or a column, the pixel at the edge standing for
/// what is beyond it, and then rounded to a step of 8 bits (the buffer between the passes holds 8 bits).
fn blurred_one_way(picture: &TestPicture, step: usize, along_rows: bool) -> TestPicture {
    let (width, height) = (picture.width as usize, picture.height as usize);
    let mut rgba = picture.rgba.clone();
    for y in 0..height {
        for x in 0..width {
            for channel in 0..4 {
                let at = |dx: isize, dy: isize| {
                    let x = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    let y = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                    f32::from(picture.rgba[(y * width + x) * 4 + channel])
                };
                let mut sum = at(0, 0) * WEIGHTS[0];
                for (tap, weight) in WEIGHTS.iter().enumerate().skip(1) {
                    let offset = (tap * step) as isize;
                    let (dx, dy) = if along_rows { (offset, 0) } else { (0, offset) };
                    sum += (at(dx, dy) + at(-dx, -dy)) * weight;
                }
                rgba[(y * width + x) * 4 + channel] = sum.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    TestPicture {
        width: picture.width,
        height: picture.height,
        rgba,
    }
}

/// The blur of `gaussian_blur.fs` for the `radius` that makes its taps whole pixels apart (`radius / 4` of them): along the rows, then along the columns.
fn the_blur_on_the_processor(picture: &TestPicture, radius: usize) -> TestPicture {
    let across = blurred_one_way(picture, radius / 4, true);
    blurred_one_way(&across, radius / 4, false)
}

#[test]
fn the_two_pass_blur_of_the_spike_blurs_a_picture_as_the_arithmetic_on_the_processor_does() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_from_isf(&mut renderer, "isf:blur", BLUR);
        for (name, picture) in [
            ("noise", noisy(24, 16, 7, false)),
            ("a gradient", TestPicture::gradient(32, 20)),
        ] {
            // The call's numbers are the four of the shader's own, and then its one input: the radius.
            for radius in [4_usize, 8, 12] {
                let (frame, statistics) = frame_of(
                    &mut renderer,
                    (picture.width, picture.height),
                    &[(1, &picture)],
                    vec![whole_frame(
                        1,
                        vec![call("isf:blur", &[0.0, 0.0, 0.0, 0.0, radius as f32])],
                    )],
                );
                assert_eq!(statistics.effects_not_run, 0);
                assert_eq!(statistics.passes_recorded, 2 + 1 + 1);
                let wanted = the_blur_on_the_processor(&picture, radius);
                let difference = compare(&frame.rgba_pixels, &wanted.rgba);
                assert!(
                    difference.largest <= 2,
                    "{name}, radius {radius}: the blur of the engine and of the processor differ by {} of 255",
                    difference.largest
                );
            }
        }
    });
}

#[test]
fn the_blur_changes_a_picture_that_is_not_flat_and_leaves_one_that_is_flat_as_it_is() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_from_isf(&mut renderer, "isf:blur", BLUR);
        let noise = noisy(24, 16, 3, false);
        let (blurred, _) = frame_of(
            &mut renderer,
            (24, 16),
            &[(1, &noise)],
            vec![whole_frame(1, vec![call("isf:blur", &[0.0, 0.0, 0.0, 0.0, 8.0])])],
        );
        assert!(
            compare(&blurred.rgba_pixels, &noise.rgba).largest > 40,
            "noise is changed by a blur (or the test would not tell a blur from nothing)"
        );
        let colour = flat(24, 16, [200, 100, 50, 255]);
        let (still, _) = frame_of(
            &mut renderer,
            (24, 16),
            &[(2, &colour)],
            vec![whole_frame(2, vec![call("isf:blur", &[0.0, 0.0, 0.0, 0.0, 8.0])])],
        );
        assert!(
            compare(&still.rgba_pixels, &colour.rgba).largest <= 1,
            "the weights add up to one: {:?}",
            still.pixel(5, 5)
        );
    });
}

/// A shader with a point of its own, normalised as nearly all that are in the wild are (`MAX` 1 and a centre of 0.5): it shows the point as a colour.
const A_POINT: &str = "/*{ \"ISFVSN\": \"2\", \"INPUTS\": [ { \"NAME\": \"spot\", \"TYPE\": \"point2D\", \"DEFAULT\": [0.5, 0.25], \"MIN\": [0, 0], \"MAX\": [1, 1] } ] }*/
void main() { gl_FragColor = vec4(spot.x, spot.y, 0.0, 1.0); }
";

/// Regression test for what the corpus of the Vidvox shaders found (phase 6.7): a point was multiplied by the height of the picture over 1080, which is right for a point in pixels
/// and wrong for the 79 of 90 points of those shaders that are fractions of the picture (`MAX` [1, 1]), whose pictures came out all black. A point reaches the shader as the file says it,
/// whatever the size of the picture.
#[test]
fn a_point_reaches_the_shader_as_the_file_wrote_it_at_every_size_of_the_picture() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_from_isf(&mut renderer, "isf:spot", A_POINT);
        for (width, height) in [(16, 9), (64, 36), (96, 54)] {
            let picture = flat(width, height, [10, 20, 30, 255]);
            let (frame, _) = frame_of(
                &mut renderer,
                (width, height),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call("isf:spot", &[0.0, 0.0, 0.0, 0.0, 0.5, 0.25])])],
            );
            assert!(
                frame.every_pixel_is([128, 64, 0, 255]),
                "{width} by {height}: {:?}",
                frame.pixel(0, 0)
            );
        }
    });
}

/// A shader that has, at the top of its code, variables whose initialisers need what the call gives (an input, the size of the picture): **naga drops such initialisers
/// without a word** (a global is given only an initialiser that is a constant), so the variables were zero, and every transition that begins with `float ratio = RENDERSIZE.x / RENDERSIZE.y;`
/// drew nothing right. The corpus of the Vidvox shaders found it (phase 6.6: Circle Crop was black at every progress).
const GLOBALS_THAT_NEED_THE_CALL: &str =
    "/*{ \"INPUTS\": [ { \"NAME\": \"gain\", \"TYPE\": \"float\", \"DEFAULT\": 0.5, \"MIN\": 0.0, \"MAX\": 1.0 } ] }*/
float half_gain = gain * 0.5;
float aspect = RENDERSIZE.x / RENDERSIZE.y;
vec2 both = vec2(half_gain, aspect / 4.0);
void main() { gl_FragColor = vec4(both, 0.0, 1.0); }
";

/// Regression test for what the corpus found: a global whose initialiser needs the numbers of the call is given it, at every size of the picture and for every value of the input.
#[test]
fn a_global_whose_initialiser_needs_the_numbers_of_the_call_has_it_when_the_shader_runs() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_from_isf(&mut renderer, "isf:globals", GLOBALS_THAT_NEED_THE_CALL);
        for ((width, height), gain) in [((48, 30), 0.5_f32), ((64, 36), 0.5), ((48, 30), 1.0)] {
            let picture = flat(width, height, [10, 20, 30, 255]);
            let (frame, _) = frame_of(
                &mut renderer,
                (width, height),
                &[(1, &picture)],
                vec![whole_frame(1, vec![call("isf:globals", &[0.0, 0.0, 0.0, 0.0, gain])])],
            );
            let aspect = width as f32 / height as f32;
            let wanted = [
                (gain * 0.5 * 255.0).round() as u8,
                (aspect / 4.0 * 255.0).round() as u8,
                0,
                255,
            ];
            let got = frame.pixel(width / 2, height / 2);
            let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
            assert!(
                off <= 1,
                "{width} by {height}, gain {gain}: the pixel is {got:?} where {wanted:?} is wanted"
            );
            assert!(frame.every_pixel_is(got), "one colour all over: the picture is flat");
        }
    });
}
