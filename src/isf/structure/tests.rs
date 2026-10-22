// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the engine is told of a shader: made from the header, in the order that the rewriting step bound the images in.

use super::*;
use crate::engine::{ImageSource, MathFunction, Operator};
use crate::isf::compile::compile;
use crate::isf::read;

const BLUR: &str = include_str!("../fixtures/gaussian_blur.fs");
const PLASMA: &str = include_str!("../fixtures/plasma.fs");
const VIGNETTE: &str = include_str!("../fixtures/vignette.fs");

/// The structure of the shader in `source`, which has to compile.
fn structure_from(source: &str) -> Result<ShaderStructure, Vec<IsfProblem>> {
    compile(&read(source).unwrap())
        .unwrap_or_else(|problems| panic!("{problems:?}"))
        .structure
}

fn good(source: &str) -> ShaderStructure {
    structure_from(source).unwrap_or_else(|problems| panic!("{problems:?}"))
}

fn why_not(source: &str) -> String {
    structure_from(source)
        .expect_err("the structure was made")
        .iter()
        .map(IsfProblem::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// A shader with an image input, the inputs `more_inputs` (JSON, each with a comma before it), and the passes `passes` (JSON).
fn shader(more_inputs: &str, passes: &str) -> String {
    format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }}{more_inputs} ], \"PASSES\": [ {passes} ] }}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage); }}\n"
    )
}

#[test]
fn a_filter_with_no_passes_listed_is_one_pass_that_reads_one_picture() {
    let structure = good(VIGNETTE);
    assert_eq!(structure.images, vec![ImageSlot::Picture]);
    assert_eq!(structure.buffers, vec![]);
    assert_eq!(
        structure.passes,
        vec![PassSpec {
            target: None,
            width: None,
            height: None
        }]
    );
    // (What differs from the structure that a shader of phase 1 has is that this one has a number for the pass.)
    assert_eq!(structure.pass_index_number, Some(3));
    assert_eq!(structure.check(), Ok(()));
}

#[test]
fn a_generator_reads_no_picture_and_is_one_pass() {
    let structure = good(PLASMA);
    assert_eq!(structure.images, vec![]);
    assert_eq!(structure.passes.len(), 1);
    assert_eq!(structure.passes[0].target, None);
    assert_eq!(structure.check(), Ok(()));
}

#[test]
fn the_two_pass_blur_has_the_picture_a_buffer_and_two_passes_the_first_of_which_draws_the_buffer() {
    let structure = good(BLUR);
    assert_eq!(structure.images, vec![ImageSlot::Picture, ImageSlot::Buffer(0)]);
    assert_eq!(
        structure.buffers,
        vec![BufferSpec {
            persistent: false,
            float: false
        }]
    );
    assert_eq!(structure.passes.len(), 2);
    assert_eq!(structure.passes[0].target, Some(0));
    assert_eq!(structure.passes[1].target, None);
    assert_eq!(structure.pass_index_number, Some(3));
    assert_eq!(structure.check(), Ok(()));
    assert_eq!(
        structure.what_each_image_reads(1),
        vec![ImageSource::Picture, ImageSource::DrawnByPass(0)]
    );
}

#[test]
fn the_flags_of_a_buffer_are_those_of_the_pass_that_draws_it() {
    let structure = good(&shader(
        "",
        "{ \"TARGET\": \"a\", \"FLOAT\": true }, { \"TARGET\": \"b\", \"PERSISTENT\": true }, {}",
    ));
    assert_eq!(
        structure.buffers,
        vec![
            BufferSpec {
                persistent: false,
                float: true
            },
            BufferSpec {
                persistent: true,
                float: false
            }
        ]
    );
}

#[test]
fn the_flags_of_a_buffer_drawn_twice_are_those_of_either_pass_whichever_comes_first() {
    let structure = good(&shader(
        "",
        "{ \"TARGET\": \"a\", \"PERSISTENT\": true, \"FLOAT\": true }, { \"TARGET\": \"a\" }, {}",
    ));
    assert_eq!(
        structure.buffers,
        vec![BufferSpec {
            persistent: true,
            float: true
        }]
    );
    let structure = good(&shader(
        "",
        "{ \"TARGET\": \"a\" }, { \"TARGET\": \"a\", \"FLOAT\": true }, {}",
    ));
    assert!(structure.buffers[0].float);
}

#[test]
fn the_buffers_are_numbered_in_the_order_they_are_first_drawn_into_and_a_buffer_drawn_twice_is_one() {
    let structure = good(&shader(
        "",
        "{ \"TARGET\": \"second\" }, { \"TARGET\": \"first\" }, { \"TARGET\": \"second\", \"PERSISTENT\": true }, {}",
    ));
    assert_eq!(structure.buffers.len(), 2);
    assert_eq!(
        structure.passes.iter().map(|pass| pass.target).collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(0), None]
    );
    assert!(
        structure.buffers[0].persistent,
        "any pass that says it makes the buffer last"
    );
    assert!(!structure.buffers[1].persistent);
    // (The engine refuses it, in words: a buffer drawn into by two passes is not run yet.)
    assert!(structure.check().unwrap_err().contains("more than one pass"));
}

#[test]
fn the_expressions_of_a_size_are_read_with_the_names_of_the_inputs_that_are_numbers() {
    // The inputs are `inputImage` (no number), then `scale` (a float: the 5th number of a call), then `choice` (a long: the 6th).
    let structure = good(&shader(
        ", { \"NAME\": \"scale\", \"TYPE\": \"float\" }, { \"NAME\": \"choice\", \"TYPE\": \"long\", \"VALUES\": [1, 2], \"LABELS\": [\"a\", \"b\"] }",
        "{ \"TARGET\": \"small\", \"WIDTH\": \"$WIDTH / scale\", \"HEIGHT\": \"max(2, choice)\" }, {}",
    ));
    let width = structure.passes[0].width.as_ref().expect("the width is there");
    assert_eq!(
        *width,
        SizeExpression::Binary(
            Operator::Divide,
            Box::new(SizeExpression::Width),
            Box::new(SizeExpression::Value(4))
        )
    );
    assert_eq!(
        *structure.passes[0].height.as_ref().expect("the height is there"),
        SizeExpression::Function(
            MathFunction::Max,
            vec![SizeExpression::Number(2.0), SizeExpression::Value(5)]
        )
    );
    // (The place of the number is where the engine gives the shader the input's value: after the four of the shader's own.)
    let numbers = [0.0, 0.0, 0.0, 0.0, 4.0, 3.0];
    assert_eq!(width.evaluate((640, 360), &numbers), 160.0);
}

#[test]
fn a_number_given_as_a_number_of_json_is_an_expression_too_and_a_pass_with_no_size_has_none() {
    let structure = good(&shader(
        "",
        "{ \"TARGET\": \"half\", \"WIDTH\": 128, \"HEIGHT\": 64 }, {}",
    ));
    assert_eq!(structure.passes[0].width, Some(SizeExpression::Number(128.0)));
    assert_eq!(structure.passes[0].height, Some(SizeExpression::Number(64.0)));
    assert_eq!(structure.passes[1].width, None);
}

#[test]
fn an_expression_that_cannot_be_read_says_which_size_of_which_pass() {
    let said = why_not(&shader(
        "",
        "{ \"TARGET\": \"a\" }, { \"TARGET\": \"b\", \"WIDTH\": \"$WIDTH / nonsense\" }, {}",
    ));
    assert!(said.contains("the WIDTH of pass number 2"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    let said = why_not(&shader("", "{ \"TARGET\": \"a\", \"HEIGHT\": \"(1\" }, {}"));
    assert!(said.contains("the HEIGHT of pass number 1"), "{said}");
}

#[test]
fn an_input_that_is_not_one_number_cannot_be_used_in_a_size() {
    let said = why_not(&shader(
        ", { \"NAME\": \"tint\", \"TYPE\": \"color\" }",
        "{ \"TARGET\": \"a\", \"WIDTH\": \"tint\" }, {}",
    ));
    assert!(
        said.contains("tint is not an input of the shader that is a number"),
        "{said}"
    );
}

#[test]
fn what_this_version_cannot_give_a_shader_is_told_as_the_reason_it_has_no_structure() {
    // (Rewritten on purpose in phase 6.4.4: the images that a shader brings are slots of its structure now: see the test of that.)
    let mask = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"maskImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage) * IMG_THIS_PIXEL(maskImage); }\n";
    assert!(why_not(mask).contains("picture called maskImage"), "{}", why_not(mask));

    let into_the_picture = shader("", "{ \"TARGET\": \"inputImage\" }, {}");
    assert!(
        why_not(&into_the_picture).contains("draws into inputImage, which is the name of a picture"),
        "{}",
        why_not(&into_the_picture)
    );
}

#[test]
fn a_shader_that_the_engine_would_refuse_still_has_the_structure_that_says_why() {
    // The last pass draws into a buffer.
    let structure = good(&shader("", "{ \"TARGET\": \"a\" }, { \"TARGET\": \"b\" }"));
    assert!(structure.check().unwrap_err().contains("last pass draws into a buffer"));
    // More passes than there may be.
    let many = vec!["{}"; 17].join(", ");
    assert!(good(&shader("", &many)).check().unwrap_err().contains("17 passes"));
}

#[test]
fn the_images_a_shader_brings_are_the_nth_that_its_header_lists_and_come_after_the_picture_and_the_buffers() {
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"held\" }, {} ], \"IMPORTED\": { \"noise\": { \"PATH\": \"n.png\" }, \"ramp\": { \"PATH\": \"r.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(noise) + IMG_THIS_PIXEL(ramp) + IMG_THIS_PIXEL(held) + IMG_THIS_PIXEL(inputImage); }\n";
    let structure = good(source);
    assert_eq!(
        structure.images,
        vec![
            ImageSlot::Picture,
            ImageSlot::Buffer(0),
            ImageSlot::Imported(0),
            ImageSlot::Imported(1)
        ]
    );
    assert_eq!(structure.imported_images(), 2);
    assert_eq!(structure.check(), Ok(()));
    assert_eq!(
        structure.what_each_image_reads(1),
        vec![
            ImageSource::Picture,
            ImageSource::DrawnByPass(0),
            ImageSource::Imported(0),
            ImageSource::Imported(1)
        ]
    );
}

#[test]
fn a_shader_that_brings_no_image_has_none_to_name() {
    assert_eq!(good(VIGNETTE).imported_images(), 0);
    assert_eq!(good(PLASMA).imported_images(), 0);
    assert_eq!(good(BLUR).imported_images(), 0);
}

const WIPE: &str = include_str!("../fixtures/wipe_transition.fs");

#[test]
fn a_transition_reads_what_is_behind_the_layer_as_start_image_and_the_layers_picture_as_end_image() {
    let structure = good(WIPE);
    assert_eq!(structure.images, vec![ImageSlot::Backdrop, ImageSlot::Picture]);
    assert!(structure.reads_the_backdrop());
    assert_eq!(structure.check(), Ok(()));
    assert_eq!(
        structure.what_each_image_reads(0),
        vec![ImageSource::Backdrop, ImageSource::Picture]
    );
    assert_eq!(structure.passes.len(), 1);
    assert_eq!(structure.pass_index_number, Some(3));
}

#[test]
fn the_pictures_of_a_transition_are_bound_in_the_order_the_header_lists_them_in() {
    let end_first = "/*{ \"INPUTS\": [ { \"NAME\": \"endImage\", \"TYPE\": \"image\" }, { \"NAME\": \"startImage\", \"TYPE\": \"image\" }, { \"NAME\": \"progress\", \"TYPE\": \"float\" } ] }*/\nvoid main() { gl_FragColor = mix(IMG_THIS_PIXEL(startImage), IMG_THIS_PIXEL(endImage), progress); }\n";
    let structure = good(end_first);
    assert_eq!(structure.images, vec![ImageSlot::Picture, ImageSlot::Backdrop]);
}

#[test]
fn a_filter_and_a_generator_do_not_read_the_backdrop() {
    assert!(!good(VIGNETTE).reads_the_backdrop());
    assert!(!good(BLUR).reads_the_backdrop());
    assert!(!good(PLASMA).reads_the_backdrop());
}
