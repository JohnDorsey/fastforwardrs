// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const NAMES: NamesOfNumbers<'static> = &[("scale", 4), ("count", 5)];

fn value_of(text: &str, picture: (u32, u32), numbers: &[f32]) -> f64 {
    read_size_expression(text, NAMES)
        .unwrap_or_else(|why| panic!("{text}: {why}"))
        .evaluate(picture, numbers)
}

fn said_of(text: &str) -> String {
    read_size_expression(text, NAMES).unwrap_err()
}

#[test]
fn numbers_and_the_size_of_the_picture_are_read() {
    assert_eq!(value_of("16", (1, 1), &[]), 16.0);
    assert_eq!(value_of("0.5", (1, 1), &[]), 0.5);
    assert_eq!(value_of(".25", (1, 1), &[]), 0.25);
    assert_eq!(value_of("2e2", (1, 1), &[]), 200.0);
    assert_eq!(value_of("5E-1", (1, 1), &[]), 0.5);
    assert_eq!(value_of("$WIDTH", (640, 360), &[]), 640.0);
    assert_eq!(value_of("$HEIGHT", (640, 360), &[]), 360.0);
    assert_eq!(value_of("  $WIDTH  ", (640, 360), &[]), 640.0, "spaces are nothing");
}

#[test]
fn the_names_of_inputs_that_are_numbers_are_their_values_in_the_call() {
    let numbers = [0.0, 0.0, 0.0, 0.0, 3.0, 7.0];
    assert_eq!(value_of("scale", (1, 1), &numbers), 3.0);
    assert_eq!(value_of("count", (1, 1), &numbers), 7.0);
    assert_eq!(value_of("$WIDTH / scale + count", (12, 1), &numbers), 11.0);
}

#[test]
fn the_operators_have_the_order_that_arithmetic_has() {
    assert_eq!(value_of("2 + 3 * 4", (1, 1), &[]), 14.0);
    assert_eq!(value_of("(2 + 3) * 4", (1, 1), &[]), 20.0);
    assert_eq!(value_of("10 - 4 - 3", (1, 1), &[]), 3.0, "from the left");
    assert_eq!(value_of("100 / 10 / 5", (1, 1), &[]), 2.0, "from the left");
    assert_eq!(value_of("17 % 5 * 2", (1, 1), &[]), 4.0, "% is a product");
    assert_eq!(value_of("-3 + 5", (1, 1), &[]), 2.0);
    assert_eq!(value_of("2 * -3", (1, 1), &[]), -6.0);
    assert_eq!(value_of("--4", (1, 1), &[]), 4.0);
    assert_eq!(value_of("+4", (1, 1), &[]), 4.0);
    assert_eq!(value_of("$WIDTH/4", (1000, 1), &[]), 250.0);
}

#[test]
fn the_functions_are_called_with_their_arguments() {
    assert_eq!(value_of("floor($WIDTH / 3)", (10, 1), &[]), 3.0);
    assert_eq!(value_of("ceil($WIDTH / 3)", (10, 1), &[]), 4.0);
    assert_eq!(value_of("max(16, $WIDTH / 8)", (64, 1), &[]), 16.0);
    assert_eq!(value_of("min(16, $WIDTH / 8)", (64, 1), &[]), 8.0);
    assert_eq!(value_of("pow(2, 3)", (1, 1), &[]), 8.0);
    assert_eq!(value_of("sqrt(16) + abs(-2)", (1, 1), &[]), 6.0);
    assert_eq!(value_of("round(2.5)", (1, 1), &[]), 3.0);
    assert_eq!(value_of("log2(8)", (1, 1), &[]), 3.0);
    assert_eq!(
        value_of("max(1, min(count, 4))", (1, 1), &[0.0, 0.0, 0.0, 0.0, 0.0, 9.0]),
        4.0
    );
    assert_eq!(value_of("sign(-7)", (1, 1), &[]), -1.0);
    for name in ["sin", "cos", "exp", "log"] {
        assert!(read_size_expression(&format!("{name}(1)"), NAMES).is_ok(), "{name}");
    }
}

#[test]
fn what_cannot_be_read_says_what_and_where() {
    assert!(said_of("").contains("ends where something was wanted"));
    assert!(said_of("$WIDTH +").contains("ends where something was wanted"));
    assert!(said_of("(1 + 2").contains("bracket is not closed"));
    assert!(said_of("1 + 2)").contains(") cannot follow the end"));
    assert!(said_of("$DEPTH").contains("$DEPTH is not something that a size can use"));
    assert!(said_of("banana").contains("banana is not an input of the shader that is a number"));
    assert!(said_of("banana(1)").contains("banana is not a function"));
    assert!(said_of("max(1)").contains("max takes 2 arguments and is given 1"));
    assert!(said_of("floor(1, 2)").contains("floor takes 1 argument and is given 2"));
    assert!(said_of("floor(1").contains("bracket of floor is not closed"));
    assert!(said_of("1 # 2").contains("# cannot follow the end"));
    assert!(said_of("1..2").contains("cannot follow"), "{}", said_of("1..2"));
    assert!(said_of("#").contains("# cannot be in the expression of a size"));
    // The place is given.
    for (text, character) in [
        ("1 + banana", 5),
        ("2 * max(1)", 5),
        ("$DEPTH", 1),
        ("1 + nope(2)", 5),
        ("3 + 1e999", 5),
        ("(1 + 2", 7),
    ] {
        let said = said_of(text);
        assert!(
            said.ends_with(&format!("(at character {character} of the expression)")),
            "{text}: {said}"
        );
    }
}

#[test]
fn an_expression_that_is_too_long_or_nested_too_deeply_is_refused_and_one_at_the_limit_is_read() {
    let too_long = "1+".repeat(LONGEST_EXPRESSION) + "1";
    assert!(said_of(&too_long).contains("longer than"));
    let at_the_longest = "1+".repeat(LONGEST_EXPRESSION / 2 - 1) + "11";
    assert_eq!(at_the_longest.chars().count(), LONGEST_EXPRESSION);
    assert!(read_size_expression(&at_the_longest, NAMES).is_ok());

    let brackets = |depth: usize| format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
    assert!(read_size_expression(&brackets(MOST_NESTING_OF_AN_EXPRESSION), NAMES).is_ok());
    assert!(said_of(&brackets(MOST_NESTING_OF_AN_EXPRESSION + 1)).contains("nested more than"));
    let minuses = format!("{}1", "-".repeat(MOST_NESTING_OF_AN_EXPRESSION + 1));
    assert!(said_of(&minuses).contains("nested more than"));
    let functions = format!(
        "{}1{}",
        "abs(".repeat(MOST_NESTING_OF_AN_EXPRESSION + 1),
        ")".repeat(MOST_NESTING_OF_AN_EXPRESSION + 1)
    );
    assert!(said_of(&functions).contains("nested more than"));
}

#[test]
fn a_number_that_is_not_finite_is_not_read() {
    assert!(said_of("1e999").contains("not a number"), "{}", said_of("1e999"));
}

#[test]
fn an_e_that_is_not_an_exponent_is_not_taken_for_one() {
    // `2e` is a 2 and then an `e`, which cannot follow.
    assert!(said_of("2e").contains("e cannot follow the end"), "{}", said_of("2e"));
}
