// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading ISF files: the files of the spike (written for it, in the style of the Vidvox examples), and headers that are odd, wrong or both.

use super::*;

const COLOR_CONTROLS: &str = include_str!("../fixtures/color_controls.fs");
const PLASMA: &str = include_str!("../fixtures/plasma.fs");
const WIPE: &str = include_str!("../fixtures/wipe_transition.fs");
const BLUR: &str = include_str!("../fixtures/gaussian_blur.fs");
const OLD_STYLE: &str = include_str!("../fixtures/old_style.fs");
const POSTERIZE: &str = include_str!("../fixtures/posterize.fs");
const VIGNETTE: &str = include_str!("../fixtures/vignette.fs");
const KALEIDOSCOPE: &str = include_str!("../fixtures/kaleidoscope.fs");
const BAD_SYNTAX: &str = include_str!("../fixtures/bad_syntax.fs");

/// A filter whose inputs are `inputImage` and `inputs` (the text of more inputs, with a comma between).
fn filter_with(inputs: &str) -> String {
    format!(
        "/*{{\n\t\"INPUTS\": [\n\t\t{{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }},\n\t\t{inputs}\n\t]\n}}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage); }}\n"
    )
}

fn refused(source: &str) -> IsfProblem {
    read(source).expect_err("the file was read")
}

#[test]
fn a_filter_with_numbers_a_switch_and_a_colour_is_read_with_what_its_header_says_of_each_input() {
    let file = read(COLOR_CONTROLS).unwrap();

    assert_eq!(file.shape(), Shape::Filter);
    assert_eq!(file.categories, ["Color Adjustment"]);
    assert_eq!(file.credit, "spike");
    assert!(file.description.starts_with("brightness, contrast"));
    let names: Vec<&str> = file.inputs.iter().map(|input| input.name.as_str()).collect();
    assert_eq!(
        names,
        ["inputImage", "brightness", "contrast", "saturation", "invert", "tint"]
    );
    assert_eq!(file.inputs[0].kind, IsfInputKind::Image);
    assert_eq!(
        file.inputs[1].kind,
        IsfInputKind::Float {
            minimum: -1.0,
            maximum: 1.0,
            default: 0.0
        }
    );
    assert_eq!(file.inputs[4].kind, IsfInputKind::Bool { default: false });
    assert_eq!(
        file.inputs[5].kind,
        IsfInputKind::Color {
            default: [1.0, 1.0, 1.0, 1.0]
        }
    );
    assert_eq!(file.numbers_of_the_inputs(), 1 + 1 + 1 + 1 + 4);
    assert!(file.passes.is_empty() && file.imported.is_empty());
}

#[test]
fn a_shader_that_reads_no_picture_is_a_generator_one_that_reads_input_image_a_filter_and_start_and_end_images_a_transition()
 {
    assert_eq!(read(PLASMA).unwrap().shape(), Shape::Generator);
    assert_eq!(read(VIGNETTE).unwrap().shape(), Shape::Filter);
    assert_eq!(read(WIPE).unwrap().shape(), Shape::Transition);
    assert_eq!(read(WIPE).unwrap().categories, ["Transition"]);
    // Only one of the two images of a transition is not one.
    let half = "/*{ \"INPUTS\": [ { \"NAME\": \"startImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() {}\n";
    assert_eq!(read(half).unwrap().shape(), Shape::Generator);
}

#[test]
fn a_choice_has_its_values_labels_and_default_and_the_default_is_a_value_it_has() {
    let file = read(POSTERIZE).unwrap();
    assert_eq!(
        file.inputs[1].kind,
        IsfInputKind::Long {
            values: vec![2, 4, 8, 16],
            labels: vec!["2".into(), "4".into(), "8".into(), "16".into()],
            default: 4
        }
    );
    assert_eq!(file.inputs[2].kind, IsfInputKind::Bool { default: true });

    // A default that is none of the values is the first; labels that are not there are the values; values that are not there are 0, 1, 2...
    let odd = read(&filter_with(
        "{ \"NAME\": \"a\", \"TYPE\": \"long\", \"VALUES\": [5, 6], \"DEFAULT\": 99 },\n\t\t{ \"NAME\": \"b\", \"TYPE\": \"long\", \"LABELS\": [\"x\", \"y\", \"z\"], \"DEFAULT\": 2 }",
    ))
    .unwrap();
    assert_eq!(
        odd.inputs[1].kind,
        IsfInputKind::Long {
            values: vec![5, 6],
            labels: vec!["5".into(), "6".into()],
            default: 5
        }
    );
    assert_eq!(
        odd.inputs[2].kind,
        IsfInputKind::Long {
            values: vec![0, 1, 2],
            labels: vec!["x".into(), "y".into(), "z".into()],
            default: 2
        }
    );
}

#[test]
fn inputs_that_say_less_than_all_of_it_have_the_defaults_isf_has() {
    let file = read(&filter_with(
        "{ \"NAME\": \"p\", \"TYPE\": \"point2D\" },\n\t\t{ \"NAME\": \"c\", \"TYPE\": \"color\" },\n\t\t{ \"NAME\": \"c3\", \"TYPE\": \"color\", \"DEFAULT\": [0.1, 0.2, 0.3] },\n\t\t{ \"NAME\": \"f\", \"TYPE\": \"float\" },\n\t\t{ \"NAME\": \"g\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 5.0 },\n\t\t{ \"NAME\": \"b\", \"TYPE\": \"bool\", \"DEFAULT\": 1 },\n\t\t{ \"NAME\": \"e\", \"TYPE\": \"event\" }",
    ))
    .unwrap();
    let kinds: Vec<&IsfInputKind> = file.inputs.iter().skip(1).map(|input| &input.kind).collect();
    assert_eq!(*kinds[0], IsfInputKind::Point2D { default: [0.0, 0.0] });
    assert_eq!(
        *kinds[1],
        IsfInputKind::Color {
            default: [0.0, 0.0, 0.0, 0.0]
        }
    );
    assert_eq!(
        *kinds[2],
        IsfInputKind::Color {
            default: [0.1, 0.2, 0.3, 1.0]
        },
        "three numbers are opaque"
    );
    assert_eq!(
        *kinds[3],
        IsfInputKind::Float {
            minimum: 0.0,
            maximum: 1.0,
            default: 0.0
        }
    );
    assert_eq!(
        *kinds[4],
        IsfInputKind::Float {
            minimum: 2.0,
            maximum: 5.0,
            default: 2.0
        },
        "the least, if nothing else is said"
    );
    assert_eq!(*kinds[5], IsfInputKind::Bool { default: true }, "a number is a switch");
    assert_eq!(*kinds[6], IsfInputKind::Event);
}

#[test]
fn a_label_is_what_is_shown_and_the_name_is_it_if_there_is_none() {
    let file = read(&filter_with(
        "{ \"NAME\": \"amount\", \"LABEL\": \"How much\", \"TYPE\": \"float\" },\n\t\t{ \"NAME\": \"other\", \"TYPE\": \"float\" }",
    ))
    .unwrap();
    assert_eq!(
        (file.inputs[1].name.as_str(), file.inputs[1].label.as_str()),
        ("amount", "How much")
    );
    assert_eq!(file.inputs[2].label, "other");
}

#[test]
fn passes_and_imported_images_are_read_in_both_forms() {
    let file = read(BLUR).unwrap();
    assert_eq!(
        file.passes,
        [
            IsfPass {
                target: Some("bufferHorizontal".into()),
                ..IsfPass::default()
            },
            IsfPass::default()
        ]
    );

    let source = "/*{ \"INPUTS\": [], \"PASSES\": [ { \"TARGET\": \"keep\", \"PERSISTENT\": true, \"FLOAT\": 1, \"WIDTH\": \"$WIDTH/2\", \"HEIGHT\": 64 } ],\n\"IMPORTED\": { \"tex\": { \"PATH\": \"a.png\" }, \"other\": \"b.jpg\" } }*/\nvoid main() {}\n";
    let file = read(source).unwrap();
    assert_eq!(
        file.passes,
        [IsfPass {
            target: Some("keep".into()),
            persistent: true,
            float: true,
            width: Some("$WIDTH/2".into()),
            height: Some("64".into()),
        }]
    );
    let names: Vec<(&str, &str)> = file
        .imported
        .iter()
        .map(|image| (image.name.as_str(), image.path.as_str()))
        .collect();
    assert_eq!(
        names,
        [("other", "b.jpg"), ("tex", "a.png")],
        "in the order of the map, which is the order of the names"
    );

    let array = "/*{ \"IMPORTED\": [ { \"NAME\": \"one\", \"PATH\": \"x.png\" } ] }*/\nvoid main() {}\n";
    assert_eq!(
        read(array).unwrap().imported[0],
        IsfImportedImage {
            name: "one".into(),
            path: "x.png".into()
        }
    );
}

#[test]
fn an_image_input_with_another_name_is_listed_as_one_that_cannot_be_filled_yet() {
    let file = read(&filter_with("{ \"NAME\": \"maskImage\", \"TYPE\": \"image\" }")).unwrap();
    assert_eq!(file.other_image_inputs(), ["maskImage"]);
    assert!(read(COLOR_CONTROLS).unwrap().other_image_inputs().is_empty());
}

// --- the header is read leniently ---

#[test]
fn a_header_with_a_trailing_comma_a_comment_a_repeated_key_and_a_number_for_the_version_is_read() {
    let source = "/*{\n\t// this is not JSON, and it is in the header\n\t\"ISFVSN\": 2,\n\t\"ISFVSN\": 2.5,\n\t\"INPUTS\": [\n\t\t{ \"NAME\": \"inputImage\", \"TYPE\": \"image\", },\n\t\t{ \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25, }, // a trailing comma\n\t],\n}*/\nvoid main() {}\n";

    let file = read(source).unwrap();

    assert_eq!(
        file.version, "2.5",
        "the last of two keys wins, and a number is a version"
    );
    assert_eq!(file.inputs.len(), 2);
    assert_eq!(
        file.inputs[1].kind,
        IsfInputKind::Float {
            minimum: 0.0,
            maximum: 1.0,
            default: 0.25
        }
    );
    assert_eq!(
        read(OLD_STYLE).unwrap().version,
        "2.0",
        "the file of the spike has it twice"
    );
}

#[test]
fn what_looks_like_a_comment_or_a_comma_inside_a_string_of_the_header_is_left_alone() {
    let source =
        "/*{ \"DESCRIPTION\": \"a // b, ] and a , } too\", \"CREDIT\": \"c:\\\\path // x\" }*/\nvoid main() {}\n";
    let file = read(source).unwrap();
    assert_eq!(file.description, "a // b, ] and a , } too");
    assert_eq!(file.credit, "c:\\path // x");
}

#[test]
fn a_comment_before_the_header_that_is_not_the_header_is_passed_over() {
    let source =
        "/* Copyright someone. Free to use. */\n// and a line\n/*{ \"DESCRIPTION\": \"the one\" }*/\nvoid main() {}\n";
    assert_eq!(read(source).unwrap().description, "the one");
}

#[test]
fn the_body_has_the_lines_it_had_and_the_glsl_is_where_it_was() {
    for source in [COLOR_CONTROLS, PLASMA, WIPE, OLD_STYLE, BAD_SYNTAX, POSTERIZE] {
        let file = read(source).unwrap();
        assert_eq!(file.body.lines().count(), source.lines().count());
        assert_eq!(
            file.body.len(),
            source.len(),
            "every character of the header became a space or stayed a line end"
        );
        let line_of = |text: &str| text.lines().position(|line| line.contains("void main")).unwrap();
        assert_eq!(line_of(&file.body), line_of(source));
        assert!(!file.body.contains("INPUTS"), "the header is gone");
    }
    // And a header that begins in the middle of a line keeps what is before it.
    let source = "// first\nfloat k = 1.0; /*{ \"INPUTS\": [] }*/ float j = 2.0;\n";
    let file = read(source).unwrap();
    assert!(file.body.starts_with("// first\nfloat k = 1.0; "));
    assert!(file.body.ends_with(" float j = 2.0;\n"));
}

// --- what is refused, and where ---

#[test]
fn a_file_with_no_header_is_refused_and_says_what_a_header_is() {
    for source in ["void main() {}\n", "/* just a comment */\nvoid main() {}\n", ""] {
        let problem = refused(source);
        assert!(
            problem.message.contains("no header") && problem.message.contains("/*{"),
            "{problem}"
        );
        assert_eq!(problem.line, None);
    }
}

#[test]
fn a_header_that_is_never_closed_is_refused_at_the_place_it_begins() {
    let problem = refused("\n\n   /*{ \"INPUTS\": [] }\nvoid main() {}\n");
    assert_eq!((problem.line, problem.column), (Some(3), Some(4)));
    assert!(problem.message.contains("never closed"), "{problem}");
}

#[test]
fn json_that_is_wrong_is_refused_at_the_line_and_the_column_of_the_file() {
    // The mistake is on line 4: a value that is not there, after the colon.
    let source =
        "// licence\n/*{\n\t\"DESCRIPTION\": \"ok\",\n\t\"INPUTS\": [ { \"NAME\": }\n\t]\n}*/\nvoid main() {}\n";
    let problem = refused(source);
    assert_eq!(problem.line, Some(4), "{problem}");
    let wrong_place = source.lines().nth(3).unwrap().find('}').unwrap() as u32 + 1;
    assert_eq!(problem.column, Some(wrong_place), "{problem}");
    assert!(problem.message.starts_with("the header is not JSON"), "{problem}");

    // On the first line of the header the column counts from the beginning of the line of the file.
    let problem = refused("   /*{ \"A\" 1 }*/\n");
    assert_eq!(problem.line, Some(1));
    assert!(problem.column.unwrap() > 8, "{problem}");
    assert!(
        refused("/*{ [ ] }*/\n").message.contains("JSON"),
        "an array is not a header"
    );
}

#[test]
fn an_input_that_cannot_be_is_refused_with_its_name() {
    let cases = [
        ("{ \"NAME\": \"2fast\", \"TYPE\": \"float\" }", "2fast", "name"),
        ("{ \"NAME\": \"has space\", \"TYPE\": \"float\" }", "has space", "name"),
        ("{ \"NAME\": \"TIME\", \"TYPE\": \"float\" }", "TIME", "of its own"),
        (
            "{ \"NAME\": \"RENDERSIZE\", \"TYPE\": \"point2D\" }",
            "RENDERSIZE",
            "of its own",
        ),
        (
            "{ \"NAME\": \"gl_thing\", \"TYPE\": \"float\" }",
            "gl_thing",
            "of its own",
        ),
        ("{ \"NAME\": \"_isf_x\", \"TYPE\": \"float\" }", "_isf_x", "of its own"),
        ("{ \"NAME\": \"IMG_X\", \"TYPE\": \"float\" }", "IMG_X", "of its own"),
        (
            "{ \"NAME\": \"inputImage\", \"TYPE\": \"float\" }",
            "inputImage",
            "another input",
        ),
        (
            "{ \"NAME\": \"a\", \"TYPE\": \"float\", \"MIN\": 3, \"MAX\": 1 }",
            "a",
            "MIN",
        ),
        ("{ \"NAME\": \"a\", \"TYPE\": \"long\" }", "a", "VALUES"),
        (
            "{ \"NAME\": \"a\", \"TYPE\": \"long\", \"VALUES\": [1, 2], \"LABELS\": [\"x\"] }",
            "a",
            "as many",
        ),
        (
            "{ \"NAME\": \"a\", \"TYPE\": \"color\", \"DEFAULT\": [1, 2] }",
            "a",
            "three or four",
        ),
        (
            "{ \"NAME\": \"a\", \"TYPE\": \"point2D\", \"DEFAULT\": [1] }",
            "a",
            "two numbers",
        ),
        (
            "{ \"NAME\": \"a\", \"TYPE\": \"point2D\", \"DEFAULT\": \"x\" }",
            "a",
            "two numbers",
        ),
        ("{ \"NAME\": \"a\", \"TYPE\": \"matrix\" }", "a", "matrix"),
        ("{ \"NAME\": \"a\" }", "a", "TYPE"),
    ];
    for (input, name, expected) in cases {
        let problem = refused(&filter_with(input));
        assert!(problem.message.contains(expected), "{input}: {problem}");
        assert!(problem.message.contains(name), "{input}: {problem}");
        assert!(problem.line.is_some(), "the input is in a place: {input}: {problem}");
    }
    let no_name = refused("/*{ \"INPUTS\": [ { \"TYPE\": \"float\" } ] }*/\n");
    assert!(
        no_name.message.contains("number 1") && no_name.message.contains("NAME"),
        "{no_name}"
    );
    let not_an_object = refused("/*{ \"INPUTS\": [ 3 ] }*/\n");
    assert!(not_an_object.message.contains("number 1"), "{not_an_object}");
}

#[test]
fn the_place_of_a_problem_with_an_input_is_where_its_name_is_written_in_the_header() {
    let source = filter_with("{ \"NAME\": \"TIME\", \"TYPE\": \"float\" }");
    let problem = refused(&source);
    let line = source.lines().position(|line| line.contains("\"TIME\"")).unwrap() as u32 + 1;
    assert_eq!(problem.line, Some(line));
    let column = source.lines().nth(line as usize - 1).unwrap().find("\"TIME\"").unwrap() as u32 + 1;
    assert_eq!(problem.column, Some(column));
}

#[test]
fn a_header_that_asks_for_more_numbers_than_a_call_has_is_refused_and_one_that_asks_for_exactly_that_many_is_read() {
    let colours = |count: usize| {
        (0..count)
            .map(|number| format!("{{ \"NAME\": \"c{number}\", \"TYPE\": \"color\" }}"))
            .collect::<Vec<_>>()
            .join(",\n")
    };
    assert_eq!(MOST_NUMBERS_OF_THE_INPUTS, 60);
    let all = read(&filter_with(&colours(15))).unwrap();
    assert_eq!(all.numbers_of_the_inputs(), MOST_NUMBERS_OF_THE_INPUTS);

    let problem = refused(&filter_with(&colours(16)));
    assert!(
        problem.message.contains("64 numbers") && problem.message.contains("60 are the most"),
        "{problem}"
    );
}

#[test]
fn a_pass_that_cannot_be_is_refused() {
    assert!(refused("/*{ \"PASSES\": [ 1 ] }*/\n").message.contains("pass number 1"));
    let problem = refused("/*{ \"PASSES\": [ { \"TARGET\": \"not a name\" } ] }*/\n");
    assert!(
        problem.message.contains("TARGET") && problem.message.contains("not a name"),
        "{problem}"
    );
}

#[test]
fn an_imported_image_that_cannot_be_is_refused() {
    assert!(
        refused("/*{ \"IMPORTED\": { \"a\": {} } }*/\n")
            .message
            .contains("no PATH")
    );
    assert!(
        refused("/*{ \"IMPORTED\": [ { \"NAME\": \"a\" } ] }*/\n")
            .message
            .contains("NAME or no PATH")
    );
    assert!(
        refused("/*{ \"IMPORTED\": { \"a b\": { \"PATH\": \"x\" } } }*/\n")
            .message
            .contains("a b")
    );
}

#[test]
fn the_problem_says_its_place_in_words() {
    assert_eq!(IsfProblem::here(3, 7, "wrong").to_string(), "line 3, column 7: wrong");
    assert_eq!(IsfProblem::nowhere_in_particular("wrong").to_string(), "wrong");
}

#[test]
fn every_file_of_the_spike_is_read() {
    for (name, source) in [
        ("color_controls", COLOR_CONTROLS),
        ("plasma", PLASMA),
        ("wipe_transition", WIPE),
        ("gaussian_blur", BLUR),
        ("old_style", OLD_STYLE),
        ("posterize", POSTERIZE),
        ("vignette", VIGNETTE),
        ("kaleidoscope", KALEIDOSCOPE),
        ("bad_syntax", BAD_SYNTAX),
    ] {
        read(source).unwrap_or_else(|problem| panic!("{name}: {problem}"));
    }
}

#[test]
fn one_number_more_than_a_call_has_room_for_is_refused_and_a_float_after_fifteen_colours_is_that_one_more() {
    let colours: Vec<String> = (0..15)
        .map(|number| format!("{{ \"NAME\": \"c{number}\", \"TYPE\": \"color\" }}"))
        .collect();
    let with_a_float = format!("{}, {{ \"NAME\": \"f\", \"TYPE\": \"float\" }}", colours.join(", "));

    let problem = refused(&filter_with(&with_a_float));

    assert!(problem.message.contains("61 numbers"), "{problem}");
}

#[test]
fn the_column_of_a_mistake_on_the_first_line_of_the_header_is_the_column_of_the_file() {
    // The header begins after three spaces and the comment mark, so that its first character is at column 6 of the file; serde_json says the column of the mistake in the JSON.
    let source = "   /*{ \"A\" 1 }*/\n";
    let in_the_json = serde_json::from_str::<Value>(&made_strict(&source[5..source.len() - 4]))
        .unwrap_err()
        .column() as u32;

    let problem = refused(source);

    assert_eq!(problem.line, Some(1));
    assert_eq!(problem.column, Some(5 + in_the_json), "{problem}");
}
