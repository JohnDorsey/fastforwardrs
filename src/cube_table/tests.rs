// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The text of a 3-D table of `size` points a side whose point `(red, green, blue)` becomes `make(red, green, blue)` (each number from 0 to 1).
fn text_of_a_3d_table(size: usize, header: &str, make: impl Fn(f32, f32, f32) -> [f32; 3]) -> String {
    let mut text = format!("{header}LUT_3D_SIZE {size}\n");
    let top = (size - 1) as f32;
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let [r, g, b] = make(red as f32 / top, green as f32 / top, blue as f32 / top);
                text.push_str(&format!("{r} {g} {b}\n"));
            }
        }
    }
    text
}

fn the_identity(red: f32, green: f32, blue: f32) -> [f32; 3] {
    [red, green, blue]
}

fn problem_of(text: &str) -> CubeProblem {
    parse_cube(text).expect_err(text)
}

#[test]
fn a_3d_table_is_read_with_red_changing_fastest_and_its_title_and_the_default_domain() {
    let text = text_of_a_3d_table(3, "TITLE \"A grade\"\n", |r, g, b| [r, g * 0.5, b * 0.25]);

    let table = parse_cube(&text).unwrap();

    assert_eq!(table.title(), "A grade");
    assert_eq!(table.shape(), CubeShape::ThreeDimensional { points_a_side: 3 });
    assert_eq!(table.rows().len(), 27);
    assert_eq!(table.domain_min(), [0.0; 3]);
    assert_eq!(table.domain_max(), [1.0; 3]);
    // Point (red 1, green 2, blue 0) is row 1 + 2 * 3 + 0 * 9 = 7.
    assert_eq!(table.rows()[7], [0.5, 0.5, 0.0]);
    // Point (red 2, green 0, blue 1) is row 2 + 0 + 9 = 11.
    assert_eq!(table.rows()[11], [1.0, 0.0, 0.125]);
}

#[test]
fn a_1d_table_is_read_and_so_are_comments_blank_lines_windows_line_ends_and_a_mark_of_the_byte_order() {
    let text = "\u{feff}# made by hand\r\n\r\nTITLE \"Curve\"\r\n  LUT_1D_SIZE 4  \r\n# the rows\r\n0 0 0\r\n0.2 0.3 0.4\r\n  0.6 0.7 0.8  \r\n1 1 1\r\n";

    let table = parse_cube(text).unwrap();

    assert_eq!(table.shape(), CubeShape::OneDimensional { points: 4 });
    assert_eq!(table.title(), "Curve");
    assert_eq!(table.rows()[1], [0.2, 0.3, 0.4]);
    assert_eq!(table.rows()[2], [0.6, 0.7, 0.8]);
}

#[test]
fn numbers_may_be_written_every_way_a_program_writes_them() {
    let text = "LUT_1D_SIZE 3\n+0.5 -0.25 .75\n1e-3 2.5E+1 -1.\n0 0 0\n";

    let table = parse_cube(text).unwrap();

    assert_eq!(table.rows()[0], [0.5, -0.25, 0.75]);
    assert_eq!(table.rows()[1], [0.001, 25.0, -1.0]);
}

#[test]
fn a_domain_and_the_resolve_variants_of_the_input_range_are_read() {
    let with_domain = parse_cube("DOMAIN_MIN -0.5 -0.25 0\nDOMAIN_MAX 2 3 4\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
    assert_eq!(with_domain.domain_min(), [-0.5, -0.25, 0.0]);
    assert_eq!(with_domain.domain_max(), [2.0, 3.0, 4.0]);

    let one = parse_cube("LUT_1D_INPUT_RANGE -0.1 1.5\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
    assert_eq!((one.domain_min(), one.domain_max()), ([-0.1; 3], [1.5; 3]));
    let three = parse_cube(&text_of_a_3d_table(2, "LUT_3D_INPUT_RANGE 0.25 0.75\n", the_identity)).unwrap();
    assert_eq!((three.domain_min(), three.domain_max()), ([0.25; 3], [0.75; 3]));
}

#[test]
fn the_most_points_a_side_there_is_for_now_is_read_and_one_more_is_refused_without_reading_the_rows() {
    let biggest = parse_cube(&text_of_a_3d_table(MOST_POINTS_A_SIDE_FOR_NOW, "", the_identity)).unwrap();
    assert_eq!(biggest.rows().len(), 65 * 65 * 65);

    let refused = problem_of("LUT_3D_SIZE 66\n0 0 0\n");
    assert_eq!(refused.line, 1);
    assert!(
        refused.message.contains("66 points a side is not used yet") && refused.message.contains("the most is 65"),
        "{refused}"
    );
    assert!(problem_of("LUT_3D_SIZE 256\n").message.contains("not used yet"));
    assert!(problem_of("LUT_3D_SIZE 257\n").message.contains("from 2 to 256"));
}

#[test]
fn a_1d_table_may_have_as_many_points_as_there_can_be_and_no_more() {
    let mut text = format!("LUT_1D_SIZE {MOST_POINTS_OF_A_1D_TABLE}\n");
    for _ in 0..MOST_POINTS_OF_A_1D_TABLE {
        text.push_str("0.5 0.5 0.5\n");
    }
    assert_eq!(parse_cube(&text).unwrap().rows().len(), MOST_POINTS_OF_A_1D_TABLE);
    assert!(problem_of("LUT_1D_SIZE 65537\n").message.contains("from 2 to 65536"));
}

#[test]
fn every_way_a_file_can_be_wrong_is_refused_with_the_line_and_the_words_for_it() {
    let rows = |count: usize| "0.5 0.5 0.5\n".repeat(count);
    let cases: Vec<(String, usize, &str)> = vec![
        (String::new(), 0, "does not say how big"),
        ("TITLE \"x\"\n".to_owned(), 0, "does not say how big"),
        (
            format!("LUT_1D_SIZE 4\n{}", rows(3)),
            0,
            "LUT_1D_SIZE 4 needs 4 rows, and the file has 3",
        ),
        (format!("LUT_1D_SIZE 4\n{}", rows(5)), 6, "more rows than the 4"),
        (
            format!("LUT_3D_SIZE 2\n{}", rows(7)),
            0,
            "LUT_3D_SIZE 2 needs 8 rows, and the file has 7",
        ),
        ("0 0 0\nLUT_1D_SIZE 2\n".to_owned(), 1, "comes before the size"),
        (
            format!("LUT_1D_SIZE 2\n{}TITLE \"late\"\n", rows(1)),
            3,
            "comes after the table has begun (at line 2)",
        ),
        (
            format!("LUT_1D_SIZE 2\n{}LUT_1D_SIZE 2\n", rows(2)),
            4,
            "comes after the table has begun",
        ),
        ("LUT_1D_SIZE 2\nLUT_1D_SIZE 3\n".to_owned(), 2, "said twice"),
        ("LUT_3D_SIZE 2\nLUT_3D_SIZE 2\n".to_owned(), 2, "said twice"),
        (
            format!("LUT_1D_SIZE 2\nLUT_3D_SIZE 2\n{}", rows(8)),
            0,
            "a 1-D table and a 3-D table",
        ),
        ("LUT_1D_SIZE 1\n".to_owned(), 1, "from 2 to 65536"),
        ("LUT_3D_SIZE 1\n".to_owned(), 1, "from 2 to 256"),
        (
            "LUT_3D_SIZE two\n".to_owned(),
            1,
            "needs a whole number, and \"two\" is not one",
        ),
        ("LUT_1D_SIZE 2.5\n".to_owned(), 1, "needs a whole number"),
        ("LUT_1D_SIZE -2\n".to_owned(), 1, "needs a whole number"),
        ("LUT_1D_SIZE\n".to_owned(), 1, "needs a whole number"),
        (
            format!("LUT_1D_SIZE 2\n0 0\n{}", rows(1)),
            2,
            "a row of the table needs three numbers",
        ),
        (
            format!("LUT_1D_SIZE 2\n0 0 0 0\n{}", rows(1)),
            2,
            "needs three numbers, and there is more: \"0\"",
        ),
        (format!("LUT_1D_SIZE 2\n0 0 x\n{}", rows(1)), 2, "\"x\" is not a number"),
        (
            format!("LUT_1D_SIZE 2\n0 0 1,5\n{}", rows(1)),
            2,
            "\"1,5\" is not a number",
        ),
        (
            format!("LUT_1D_SIZE 2\n0 0 1e999\n{}", rows(1)),
            2,
            "not a number that a table can have",
        ),
        (format!("LUT_1D_SIZE 2\n0 0 -.\n{}", rows(1)), 2, "is not a number"),
        (
            format!("DOMAIN_MIN 0 0\nLUT_1D_SIZE 2\n{}", rows(2)),
            1,
            "DOMAIN_MIN needs three numbers",
        ),
        (
            format!("DOMAIN_MAX 1 1 1 1\nLUT_1D_SIZE 2\n{}", rows(2)),
            1,
            "DOMAIN_MAX needs three numbers, and there is more",
        ),
        (
            format!("DOMAIN_MIN 0 0 q\nLUT_1D_SIZE 2\n{}", rows(2)),
            1,
            "\"q\" is not a number",
        ),
        (
            format!("DOMAIN_MIN 1 0 0\nDOMAIN_MAX 1 1 1\nLUT_1D_SIZE 2\n{}", rows(2)),
            0,
            "the domain of the red is from 1 to 1, which is not a range",
        ),
        (
            format!("DOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 -0.5 1\nLUT_1D_SIZE 2\n{}", rows(2)),
            0,
            "the domain of the green",
        ),
        (
            format!("LUT_1D_INPUT_RANGE 1\nLUT_1D_SIZE 2\n{}", rows(2)),
            1,
            "LUT_1D_INPUT_RANGE needs two numbers",
        ),
        (
            format!("LUT_3D_INPUT_RANGE 0.5 0.5\nLUT_1D_SIZE 2\n{}", rows(2)),
            0,
            "which is not a range",
        ),
        (
            format!("LUT_1D_SIZE 2\nSHAPER_LUT 3\n{}", rows(2)),
            2,
            "\"SHAPER_LUT\" is not something that a .cube file says",
        ),
        ("hello\n".to_owned(), 1, "\"hello\" is not something"),
        ("LUT_3D_SIZE 2 3\n".to_owned(), 1, "needs a whole number"),
    ];
    for (text, line, wanted) in cases {
        let found = problem_of(&text);
        assert_eq!(found.line, line, "{wanted}: {found} in\\n{text}");
        assert!(found.message.contains(wanted), "{wanted}: {found}");
    }
}

#[test]
fn a_problem_is_told_with_its_line_or_without_one_for_the_whole_file() {
    assert_eq!(
        problem_of("LUT_1D_SIZE two\n").to_string(),
        "line 1: LUT_1D_SIZE needs a whole number, and \"two\" is not one"
    );
    assert!(!problem_of("TITLE x\n").to_string().starts_with("line"));
}

#[test]
fn tables_are_equal_by_what_they_say_and_are_told_apart_by_their_serial_numbers() {
    let text = text_of_a_3d_table(2, "", the_identity);
    let (first, second) = (parse_cube(&text).unwrap(), parse_cube(&text).unwrap());
    assert_eq!(first, second);
    assert_ne!(first.serial_number(), second.serial_number());
    let other = parse_cube(&text_of_a_3d_table(2, "", |r, g, b| [g, r, b])).unwrap();
    assert_ne!(first, other);
}

#[test]
fn a_table_made_by_hand_must_have_the_rows_its_shape_needs_and_a_domain_that_is_a_range() {
    let shape = CubeShape::OneDimensional { points: 2 };
    assert!(CubeTable::new("x", [0.0; 3], [1.0; 3], shape, vec![[0.0; 3]; 2]).is_ok());
    assert!(CubeTable::new("x", [0.0; 3], [1.0; 3], shape, vec![[0.0; 3]; 3]).is_err());
    assert!(CubeTable::new("x", [0.0; 3], [0.0, 1.0, 1.0], shape, vec![[0.0; 3]; 2]).is_err());
    assert!(CubeTable::new("x", [0.0; 3], [f32::NAN, 1.0, 1.0], shape, vec![[0.0; 3]; 2]).is_err());
}

// --- looking a colour up ---

fn near(found: [f32; 3], wanted: [f32; 3], because: &str) {
    for place in 0..3 {
        assert!(
            (found[place] - wanted[place]).abs() < 1.0e-5,
            "{because}: {found:?} where {wanted:?}"
        );
    }
}

#[test]
fn the_identity_table_gives_every_colour_back_by_every_way_of_looking_up() {
    let table = parse_cube(&text_of_a_3d_table(5, "", the_identity)).unwrap();
    let one_d = parse_cube("LUT_1D_SIZE 3\n0 0 0\n0.5 0.5 0.5\n1 1 1\n").unwrap();
    for colour in [
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [0.3, 0.6, 0.9],
        [0.99, 0.01, 0.5],
        [0.126, 0.874, 0.375],
    ] {
        for interpolation in [Interpolation::Tetrahedral, Interpolation::Trilinear] {
            near(
                table.look_up(colour, interpolation),
                colour,
                &format!("3-D {interpolation:?}"),
            );
        }
        near(one_d.look_up(colour, Interpolation::Tetrahedral), colour, "1-D");
    }
}

#[test]
fn a_table_gives_the_colour_of_its_point_exactly_at_its_points_and_a_colour_outside_the_domain_is_the_nearest_inside_it()
 {
    let table = parse_cube(&text_of_a_3d_table(3, "", |r, g, b| [1.0 - r, g * g, b * 0.5 + 0.25])).unwrap();
    for interpolation in [Interpolation::Tetrahedral, Interpolation::Trilinear] {
        near(
            table.look_up([0.5, 1.0, 0.0], interpolation),
            [0.5, 1.0, 0.25],
            "at a point",
        );
        near(
            table.look_up([0.0, 0.5, 1.0], interpolation),
            [1.0, 0.25, 0.75],
            "at another",
        );
        near(
            table.look_up([-3.0, 7.0, 0.0], interpolation),
            table.look_up([0.0, 1.0, 0.0], interpolation),
            "outside",
        );
    }
    let shifted = parse_cube("LUT_1D_INPUT_RANGE 0.25 0.75\nLUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
    near(
        shifted.look_up([0.25; 3], Interpolation::Tetrahedral),
        [0.0; 3],
        "the bottom of the domain",
    );
    near(
        shifted.look_up([0.5; 3], Interpolation::Tetrahedral),
        [0.5; 3],
        "the middle of it",
    );
    near(
        shifted.look_up([0.75; 3], Interpolation::Tetrahedral),
        [1.0; 3],
        "the top of it",
    );
    near(
        shifted.look_up([0.0; 3], Interpolation::Tetrahedral),
        [0.0; 3],
        "below it",
    );
    near(
        shifted.look_up([9.0; 3], Interpolation::Tetrahedral),
        [1.0; 3],
        "above it",
    );
}

#[test]
fn between_the_points_both_ways_agree_on_what_is_linear_and_differ_on_what_is_not() {
    // A table that is a straight line in every direction (the identity with a tilt): the two ways of looking up give the same colours everywhere.
    let straight = parse_cube(&text_of_a_3d_table(4, "", |r, g, b| {
        [0.5 * r + 0.25 * g, g, 0.5 * b + 0.5 * r]
    }))
    .unwrap();
    let curved = parse_cube(&text_of_a_3d_table(3, "", |r, g, b| [r * g, g * b, b * r])).unwrap();
    let mut differ = false;
    for red in [0.07, 0.31, 0.5, 0.77, 0.93] {
        for green in [0.02, 0.4, 0.61, 0.88] {
            for blue in [0.11, 0.35, 0.58, 0.99] {
                let colour = [red, green, blue];
                near(
                    straight.look_up(colour, Interpolation::Tetrahedral),
                    straight.look_up(colour, Interpolation::Trilinear),
                    "a table that is straight",
                );
                let (tetra, tri) = (
                    curved.look_up(colour, Interpolation::Tetrahedral),
                    curved.look_up(colour, Interpolation::Trilinear),
                );
                differ |= (0..3).any(|place| (tetra[place] - tri[place]).abs() > 1.0e-3);
            }
        }
    }
    assert!(
        differ,
        "a table that is curved is not looked up the same by the two ways"
    );
}

#[test]
fn the_tetrahedral_look_up_is_the_same_whichever_of_the_six_tetrahedra_the_colour_is_in_by_the_arithmetic_of_its_four_corners()
 {
    // A table of 2 points a side has one little cube. The fractions in the six orders each pick a tetrahedron, and the result is a mix of four of the eight corners.
    let table = parse_cube(&text_of_a_3d_table(2, "", |r, g, b| {
        [r * 0.9 + g * 0.05, g * 0.8 + b * 0.1, b * 0.7 + r * 0.2]
    }))
    .unwrap();
    let corner = |r: usize, g: usize, b: usize| table.rows()[(b * 2 + g) * 2 + r];
    let by_hand = |d: [f32; 3]| -> [f32; 3] {
        // Sort the fractions from the largest to the smallest, and walk from the corner 000 to the corner 111 one step at a time, in the order of the sort.
        let mut order = [0usize, 1, 2];
        order.sort_by(|a, b| d[*b].total_cmp(&d[*a]));
        let mut at = [0usize; 3];
        let mut weights = vec![(corner(0, 0, 0), 1.0 - d[order[0]])];
        for (step, axis) in order.iter().enumerate() {
            at[*axis] = 1;
            let next = if step < 2 { d[order[step + 1]] } else { 0.0 };
            weights.push((corner(at[0], at[1], at[2]), d[*axis] - next));
        }
        let mut sum = [0.0_f32; 3];
        for (colour, weight) in weights {
            for place in 0..3 {
                sum[place] += colour[place] * weight;
            }
        }
        sum
    };
    for d in [
        [0.9, 0.5, 0.1],
        [0.9, 0.1, 0.5],
        [0.5, 0.9, 0.1],
        [0.1, 0.9, 0.5],
        [0.5, 0.1, 0.9],
        [0.1, 0.5, 0.9],
        [0.3, 0.3, 0.8],
        [0.0, 1.0, 0.0],
    ] {
        near(
            table.look_up(d, Interpolation::Tetrahedral),
            by_hand(d),
            &format!("{d:?}"),
        );
    }
}

#[test]
fn a_1d_table_is_looked_up_in_each_channels_own_column_by_linear_interpolation() {
    let table = parse_cube("LUT_1D_SIZE 3\n0 1 0.5\n0.5 0.5 0.5\n1 0 0.5\n").unwrap();
    near(
        table.look_up([0.0, 0.0, 0.0], Interpolation::Tetrahedral),
        [0.0, 1.0, 0.5],
        "first row",
    );
    near(
        table.look_up([0.25, 0.25, 0.25], Interpolation::Tetrahedral),
        [0.25, 0.75, 0.5],
        "between the first two",
    );
    near(
        table.look_up([0.5, 0.75, 1.0], Interpolation::Tetrahedral),
        [0.5, 0.25, 0.5],
        "each channel its own place",
    );
    near(
        table.look_up([1.0, 1.0, 1.0], Interpolation::Tetrahedral),
        [1.0, 0.0, 0.5],
        "last row",
    );
}
