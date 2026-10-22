// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn number(value: f64) -> Box<SizeExpression> {
    Box::new(SizeExpression::Number(value))
}

fn buffer(persistent: bool, float: bool) -> BufferSpec {
    BufferSpec { persistent, float }
}

fn pass(target: Option<usize>) -> PassSpec {
    PassSpec {
        target,
        width: None,
        height: None,
    }
}

/// A filter with the shape of the two-pass blur: the picture, one buffer, two passes (the first draws the buffer).
fn two_pass_blur() -> ShaderStructure {
    ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Buffer(0)],
        buffers: vec![buffer(false, false)],
        passes: vec![pass(Some(0)), pass(None)],
        pass_index_number: Some(3),
    }
}

#[test]
fn an_expression_is_worked_out_from_the_size_of_the_picture_and_the_numbers_of_the_call() {
    use Operator::*;
    // $WIDTH / 2 + radius * 4, where radius is the 5th number of the call.
    let expression = SizeExpression::Binary(
        Add,
        Box::new(SizeExpression::Binary(
            Divide,
            Box::new(SizeExpression::Width),
            number(2.0),
        )),
        Box::new(SizeExpression::Binary(
            Multiply,
            Box::new(SizeExpression::Value(4)),
            number(4.0),
        )),
    );
    assert_eq!(expression.evaluate((640, 360), &[0.0, 0.0, 0.0, 0.0, 3.0]), 332.0);
    assert_eq!(SizeExpression::Height.evaluate((640, 360), &[]), 360.0);
    // A number the call does not have is zero.
    assert_eq!(SizeExpression::Value(9).evaluate((1, 1), &[1.0]), 0.0);
    for (operator, expected) in [
        (Subtract, 7.0),
        (Remainder, 1.0),
        (Multiply, 30.0),
        (Divide, 3.3333333333333335),
    ] {
        let both = SizeExpression::Binary(operator, number(10.0), number(3.0));
        assert_eq!(both.evaluate((1, 1), &[]), expected);
    }
    assert_eq!(SizeExpression::Negate(number(5.0)).evaluate((1, 1), &[]), -5.0);
}

#[test]
fn every_function_of_an_expression_is_the_function_of_that_name() {
    use MathFunction::*;
    let one = |function, argument: f64| {
        SizeExpression::Function(function, vec![SizeExpression::Number(argument)]).evaluate((1, 1), &[])
    };
    let two = |function, a: f64, b: f64| {
        SizeExpression::Function(function, vec![SizeExpression::Number(a), SizeExpression::Number(b)])
            .evaluate((1, 1), &[])
    };
    assert_eq!(one(Floor, 2.7), 2.0);
    assert_eq!(one(Ceil, 2.2), 3.0);
    assert_eq!(one(Round, 2.5), 3.0);
    assert_eq!(one(Abs, -4.0), 4.0);
    assert_eq!(two(Min, 3.0, 8.0), 3.0);
    assert_eq!(two(Max, 3.0, 8.0), 8.0);
    assert_eq!(two(Pow, 2.0, 10.0), 1024.0);
    assert_eq!(one(Sqrt, 81.0), 9.0);
    assert!((one(Sin, std::f64::consts::FRAC_PI_2) - 1.0).abs() < 1e-12);
    assert!((one(Cos, 0.0) - 1.0).abs() < 1e-12);
    assert!((one(Exp, 1.0) - std::f64::consts::E).abs() < 1e-12);
    assert!((one(Log, std::f64::consts::E) - 1.0).abs() < 1e-12);
    assert_eq!(one(Log2, 8.0), 3.0);
    assert_eq!((one(Sign, -9.0), one(Sign, 9.0), one(Sign, 0.0)), (-1.0, 1.0, 0.0));
    assert!(one(Sign, f64::NAN).is_nan());
    // A function that is short of an argument is not a number.
    assert!(
        SizeExpression::Function(Max, vec![SizeExpression::Number(1.0)])
            .evaluate((1, 1), &[])
            .is_nan()
    );
    assert_eq!(
        (
            Min.arguments(),
            Max.arguments(),
            Pow.arguments(),
            Floor.arguments(),
            Log2.arguments()
        ),
        (2, 2, 2, 1, 1)
    );
}

#[test]
fn a_size_is_the_whole_number_under_the_value_at_least_one_and_at_most_the_card_and_the_limit() {
    assert_eq!(size_from(333.9, 4096), 333);
    assert_eq!(size_from(0.4, 4096), 1);
    assert_eq!(size_from(-50.0, 4096), 1);
    assert_eq!(size_from(f64::NAN, 4096), 1, "not a number is one");
    assert_eq!(size_from(f64::INFINITY, 4096), 4096);
    assert_eq!(size_from(f64::NEG_INFINITY, 4096), 1);
    assert_eq!(size_from(5000.0, 4096), 4096, "the card's side is the limit");
    assert_eq!(
        size_from(20000.0, 16384),
        LARGEST_SIDE_OF_A_BUFFER,
        "the program's limit is the limit"
    );
    assert_eq!(size_from(8192.0, 16384), 8192);
}

#[test]
fn a_shader_given_with_nothing_said_of_it_is_one_pass_that_reads_one_picture() {
    let default = ShaderStructure::one_pass_of_one_picture();
    assert!(default.is_one_pass_of_one_picture());
    assert_eq!(default.check(), Ok(()));
    assert_eq!(default.images, vec![ImageSlot::Picture]);
    assert_eq!(default.passes.len(), 1);
    assert_eq!(
        default.pass_index_number, None,
        "a shader of phase 1 owns all its numbers"
    );
    assert!(!two_pass_blur().is_one_pass_of_one_picture());
}

#[test]
fn the_structure_of_a_two_pass_blur_is_well_formed() {
    assert_eq!(two_pass_blur().check(), Ok(()));
}

#[test]
fn a_structure_that_is_not_well_formed_says_what_is_wrong_with_it() {
    let said = |structure: ShaderStructure| structure.check().unwrap_err();

    let mut too_many_images = two_pass_blur();
    too_many_images.images = vec![ImageSlot::Picture; MOST_IMAGES_OF_A_SHADER + 1];
    assert!(said(too_many_images).contains("reads 8 pictures"));
    let mut most_images = two_pass_blur();
    most_images.images = vec![ImageSlot::Picture; MOST_IMAGES_OF_A_SHADER];
    assert_eq!(most_images.check(), Ok(()));

    let mut no_pass = two_pass_blur();
    no_pass.passes.clear();
    assert!(said(no_pass).contains("no pass"));

    let mut too_many_passes = ShaderStructure::one_pass_of_one_picture();
    too_many_passes.passes = vec![pass(None); MOST_PASSES_OF_A_SHADER + 1];
    assert!(said(too_many_passes).contains("17 passes"));
    let mut most_passes = ShaderStructure::one_pass_of_one_picture();
    most_passes.passes = vec![pass(None); MOST_PASSES_OF_A_SHADER];
    assert_eq!(most_passes.check(), Ok(()));

    let mut image_of_a_missing_buffer = two_pass_blur();
    image_of_a_missing_buffer.images.push(ImageSlot::Buffer(1));
    assert!(said(image_of_a_missing_buffer).contains("buffer 1, which there is not"));

    let mut pass_into_a_missing_buffer = two_pass_blur();
    pass_into_a_missing_buffer.passes[0].target = Some(3);
    assert!(said(pass_into_a_missing_buffer).contains("pass 0 draws into buffer 3, which there is not"));
    // (The first number that is not a buffer is the number of the buffers.)
    let mut pass_into_the_buffer_after_the_last = two_pass_blur();
    pass_into_the_buffer_after_the_last.passes[0].target = Some(1);
    assert!(said(pass_into_the_buffer_after_the_last).contains("pass 0 draws into buffer 1, which there is not"));

    let mut two_passes_one_buffer = two_pass_blur();
    two_passes_one_buffer.passes = vec![pass(Some(0)), pass(Some(0)), pass(None)];
    assert!(said(two_passes_one_buffer).contains("buffer 0 is drawn into by more than one pass"));

    let mut last_pass_into_a_buffer = two_pass_blur();
    last_pass_into_a_buffer.passes = vec![pass(None), pass(Some(0))];
    assert!(said(last_pass_into_a_buffer).contains("the last pass draws into a buffer"));

    let mut lasting = two_pass_blur();
    lasting.buffers = vec![buffer(true, false)];
    assert!(said(lasting).contains("lasts from one frame to the next is not run yet"));
}

#[test]
fn a_pass_with_no_target_is_as_big_as_the_picture_and_in_the_format_of_pictures() {
    let facts = two_pass_blur().facts_of_the_passes((640, 360), &[], 4096);
    assert_eq!(
        facts[1],
        PassFacts {
            target: None,
            size: (640, 360),
            format: TargetFormat::Half
        }
    );
}

#[test]
fn a_buffer_with_no_size_said_is_as_big_as_the_picture_and_eight_bit_unless_it_is_float() {
    let facts = two_pass_blur().facts_of_the_passes((640, 360), &[], 4096);
    assert_eq!(
        facts[0],
        PassFacts {
            target: Some(0),
            size: (640, 360),
            format: TargetFormat::EightBit
        }
    );
    let mut float = two_pass_blur();
    float.buffers = vec![buffer(false, true)];
    assert_eq!(
        float.facts_of_the_passes((640, 360), &[], 4096)[0].format,
        TargetFormat::Half
    );
}

#[test]
fn the_size_of_a_buffer_is_its_expressions_worked_out_for_the_call() {
    let mut structure = two_pass_blur();
    // WIDTH: $WIDTH / 4, HEIGHT: the 5th number of the call (an input) times 10.
    structure.passes[0].width = Some(SizeExpression::Binary(
        Operator::Divide,
        Box::new(SizeExpression::Width),
        number(4.0),
    ));
    structure.passes[0].height = Some(SizeExpression::Binary(
        Operator::Multiply,
        Box::new(SizeExpression::Value(4)),
        number(10.0),
    ));
    let numbers = [0.0, 0.0, 0.0, 0.0, 3.5];
    assert_eq!(
        structure.facts_of_the_passes((640, 360), &numbers, 4096)[0].size,
        (160, 35)
    );
    // The same shader for another picture and another input.
    let numbers = [0.0, 0.0, 0.0, 0.0, 9.0];
    assert_eq!(
        structure.facts_of_the_passes((1920, 1080), &numbers, 4096)[0].size,
        (480, 90)
    );
    // A size is kept within the card's side, and within one.
    assert_eq!(
        structure.facts_of_the_passes((1920, 1080), &numbers, 100)[0].size,
        (100, 90)
    );
    let numbers = [0.0, 0.0, 0.0, 0.0, 0.0];
    assert_eq!(
        structure.facts_of_the_passes((640, 360), &numbers, 4096)[0].size,
        (160, 1)
    );
    // Only one of the two said: the other is the picture's.
    structure.passes[0].height = None;
    assert_eq!(
        structure.facts_of_the_passes((640, 360), &numbers, 4096)[0].size,
        (160, 360)
    );
}

#[test]
fn the_size_of_a_pass_with_no_target_is_the_pictures_whatever_its_expressions_say() {
    let mut structure = two_pass_blur();
    structure.passes[1].width = Some(*number(10.0));
    structure.passes[1].height = Some(*number(10.0));
    assert_eq!(structure.facts_of_the_passes((640, 360), &[], 4096)[1].size, (640, 360));
}

#[test]
fn a_pass_reads_a_buffer_as_the_pass_that_drew_it_left_it_and_a_buffer_not_drawn_yet_is_clear() {
    let structure = two_pass_blur();
    assert_eq!(
        structure.what_each_image_reads(0),
        vec![ImageSource::Picture, ImageSource::Clear],
        "the first pass reads the buffer it is drawing: nothing"
    );
    assert_eq!(
        structure.what_each_image_reads(1),
        vec![ImageSource::Picture, ImageSource::DrawnByPass(0)]
    );
}

#[test]
fn a_buffer_drawn_in_the_middle_is_read_by_the_passes_after_it_and_not_by_those_before() {
    // The picture, buffers 0 and 1; passes: draw 0, draw 1, the picture.
    let structure = ShaderStructure {
        images: vec![ImageSlot::Picture, ImageSlot::Buffer(0), ImageSlot::Buffer(1)],
        buffers: vec![buffer(false, false), buffer(false, true)],
        passes: vec![pass(Some(0)), pass(Some(1)), pass(None)],
        pass_index_number: Some(3),
    };
    assert_eq!(structure.check(), Ok(()));
    assert_eq!(
        structure.what_each_image_reads(0),
        vec![ImageSource::Picture, ImageSource::Clear, ImageSource::Clear]
    );
    assert_eq!(
        structure.what_each_image_reads(1),
        vec![ImageSource::Picture, ImageSource::DrawnByPass(0), ImageSource::Clear]
    );
    assert_eq!(
        structure.what_each_image_reads(2),
        vec![
            ImageSource::Picture,
            ImageSource::DrawnByPass(0),
            ImageSource::DrawnByPass(1)
        ]
    );
}

#[test]
fn a_generator_has_no_picture_and_starts_at_its_buffers() {
    let structure = ShaderStructure {
        images: vec![ImageSlot::Buffer(0)],
        buffers: vec![buffer(false, false)],
        passes: vec![pass(Some(0)), pass(None)],
        pass_index_number: Some(3),
    };
    assert_eq!(structure.check(), Ok(()));
    assert_eq!(structure.what_each_image_reads(1), vec![ImageSource::DrawnByPass(0)]);
}
