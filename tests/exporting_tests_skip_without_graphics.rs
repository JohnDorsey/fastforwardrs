// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A check of the other test programs, which reads their text: **every test that makes an export must skip itself where there is no software graphics adapter** (a Mac of
//! the CI has none), because the export is made by the picture engine, which the tests run on a software adapter and nothing else (SF-01). A test that forgot to says
//! nothing on a computer that has an adapter, and fails on one that has none, which only the CI's Mac tells (it did, once: a journey of the window's export).
//!
//! It is a text check: a test is the text from one `#[test]` to the next, and it makes an export if the text has one of the calls that start one.

use std::fs;
use std::path::Path;

/// What in the text of a test means that it makes an export.
const CALLS_THAT_MAKE_AN_EXPORT: [&str; 6] = [
    "export_to(",
    "export_video_to(",
    "VideoExport::start(",
    "export_and_wait(",
    "run_on_a_software_adapter(",
    "export_with_the_window(",
];

/// What in the text of a test means that it skips itself without a graphics adapter.
const WAYS_OF_SKIPPING: [&str; 4] = [
    "toolchain_and_graphics_or_skip!",
    "ready_or_skip!",
    "there_is_graphics_to_export_with",
    "software_graphics()",
];

/// The tests of a program that make an export and do not skip: `(the file, the name of the test)`. A function is from the line that begins with `fn` (at the left edge,
/// as rustfmt writes them) to the line that is a `}` and nothing else, and it is a test if the lines before it that are attributes (or comments on it) have `#[test]`:
/// what a helper after a test does is not what the test does.
fn the_tests_that_do_not_skip(text: &str, file: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut number = 0;
    while number < lines.len() {
        let line = lines[number];
        if !(line.starts_with("fn ") || line.starts_with("pub fn ")) {
            number += 1;
            continue;
        }
        let is_a_test = lines[..number]
            .iter()
            .rev()
            .take_while(|before| before.starts_with("#[") || before.starts_with("///"))
            .any(|before| before.starts_with("#[test]"));
        let end = lines[number..]
            .iter()
            .position(|candidate| *candidate == "}")
            .map_or(lines.len(), |relative| number + relative);
        if is_a_test {
            let body = lines[number..=end.min(lines.len() - 1)].join(
                "
",
            );
            let name = line
                .split("fn ")
                .nth(1)
                .and_then(|after| after.split('(').next())
                .unwrap_or("?")
                .trim()
                .to_owned();
            let makes_an_export = CALLS_THAT_MAKE_AN_EXPORT.iter().any(|call| body.contains(call));
            let skips = WAYS_OF_SKIPPING.iter().any(|way| body.contains(way));
            if makes_an_export && !skips {
                found.push((file.to_owned(), name));
            }
        }
        number = end + 1;
    }
    found
}

#[test]
fn every_test_that_makes_an_export_skips_itself_where_there_is_no_graphics_adapter() {
    let mut not_skipping = Vec::new();
    let mut files_read = 0;
    for entry in fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // (Not this file: its own text has every call in it.)
        if path.extension().is_none_or(|extension| extension != "rs")
            || name == "exporting_tests_skip_without_graphics.rs"
        {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        not_skipping.extend(the_tests_that_do_not_skip(&text, &name));
        files_read += 1;
    }
    assert!(files_read > 10, "the test programs were found ({files_read})");
    assert_eq!(
        not_skipping,
        Vec::<(String, String)>::new(),
        "these tests make an export and do not skip themselves where there is no software graphics adapter: use `toolchain_and_graphics_or_skip!`"
    );
}

#[test]
fn the_check_finds_a_test_that_makes_an_export_and_does_not_skip_and_lets_one_that_skips_and_one_that_makes_none_go() {
    let text = "
#[test]
fn one_that_forgot() {
    let toolchain = toolchain_or_skip!();
    export_to(&mut journey, &destination);
}

#[test]
fn one_that_skips() {
    let toolchain = toolchain_and_graphics_or_skip!();
    export_to(&mut journey, &destination);
}

#[test]
fn one_that_makes_none() {
    let toolchain = toolchain_or_skip!();
    play(&mut journey);
}

fn a_helper_after_it_that_makes_an_export() {
    VideoExport::start(&toolchain);
}
";
    assert_eq!(
        the_tests_that_do_not_skip(text, "a.rs"),
        vec![("a.rs".to_owned(), "one_that_forgot".to_owned())]
    );
}
