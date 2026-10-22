// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the shaders in folders: which files are looked at, what is listed of each, the order, the limits, and what a duplicate is.

use std::path::{Path, PathBuf};

use super::*;

const VIGNETTE: &str = include_str!("../../isf/fixtures/vignette.fs");
const PLASMA: &str = include_str!("../../isf/fixtures/plasma.fs");
const WIPE: &str = include_str!("../../isf/fixtures/wipe_transition.fs");
const NEEDS_A_MASK: &str = include_str!("../../isf/fixtures/needs_a_mask.fs");
const BAD: &str = include_str!("../../isf/fixtures/bad_syntax.fs");

fn write(folder: &Path, name: &str, text: &str) -> PathBuf {
    let path = folder.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn names(scan: &ShaderScan) -> Vec<&str> {
    scan.shaders.iter().map(|shader| shader.file_name.as_str()).collect()
}

#[test]
fn the_shaders_of_a_folder_are_listed_in_the_order_of_their_names_each_with_what_it_is_and_whether_it_can_be_used() {
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "b plasma.fs", PLASMA);
    write(folder.path(), "a vignette.fs", VIGNETTE);
    write(folder.path(), "c wipe.isf", WIPE);
    write(folder.path(), "d broken.fs", BAD);
    write(folder.path(), "e mask.fs", NEEDS_A_MASK);

    let scan = scan_folders(&[folder.path().to_owned()]);

    assert_eq!(
        names(&scan),
        ["a vignette.fs", "b plasma.fs", "c wipe.isf", "d broken.fs", "e mask.fs"]
    );
    let shapes: Vec<Option<Shape>> = scan.shaders.iter().map(|shader| shader.shape).collect();
    assert_eq!(
        shapes,
        [
            Some(Shape::Filter),
            Some(Shape::Generator),
            Some(Shape::Transition),
            Some(Shape::Filter),
            Some(Shape::Filter)
        ]
    );
    let usable: Vec<bool> = scan.shaders.iter().map(FoundShader::can_be_used).collect();
    // (Rewritten on purpose in phase 6.6: a transition is run now, so it can be used.)
    assert_eq!(usable, [true, true, true, false, false]);
    // (Each that cannot be used says why: the one that has a mistake has a place in it, the other wants a picture that this version cannot give it.)
    assert!(scan.shaders[3].why_not_usable.as_deref().unwrap().contains("line"));
    assert!(scan.shaders[4].why_not_usable.as_deref().unwrap().contains("maskImage"));
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    assert_eq!((scan.usable_count(), scan.unusable_count()), (3, 2));
    assert_eq!(scan.shaders[0].path, folder.path().join("a vignette.fs"));
}

#[test]
fn files_that_do_not_begin_like_an_isf_file_are_not_shaders_and_are_left_out_without_a_word() {
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "program.fs", "module Program\nlet main = 1\n");
    write(folder.path(), "plain.fs", "void main() { gl_FragColor = vec4(1.0); }\n");
    write(
        folder.path(),
        "comment.fs",
        "/* only a comment, with no JSON in it */\nvoid main() {}\n",
    );
    write(folder.path(), "notes.txt", VIGNETTE);
    write(folder.path(), "real.fs", VIGNETTE);

    let scan = scan_folders(&[folder.path().to_owned()]);

    assert_eq!(names(&scan), ["real.fs"]);
    assert!(scan.problems.is_empty());
}

#[test]
fn a_file_that_begins_like_an_isf_file_and_is_wrong_is_listed_with_the_reason_and_the_ending_may_be_in_capitals() {
    let folder = tempfile::tempdir().unwrap();
    write(
        folder.path(),
        "wrong.FS",
        "/*{ \"INPUTS\": [ { \"NAME\": \"a\", \"TYPE\": \"nonsense\" } ] }*/\nvoid main() {}\n",
    );
    std::fs::write(folder.path().join("bytes.fs"), b"/*{ \"x\": 1 }*/ \xff\xfe\n").unwrap();

    let scan = scan_folders(&[folder.path().to_owned()]);

    assert_eq!(names(&scan), ["bytes.fs", "wrong.FS"]);
    for shader in &scan.shaders {
        assert!(!shader.can_be_used(), "{}", shader.file_name);
        assert!(shader.why_not_usable.is_some(), "{}", shader.file_name);
        assert!(
            shader.kind.is_none() && shader.shape.is_none(),
            "a file that cannot be read as ISF has no kind and no shape: {}",
            shader.file_name
        );
    }
    assert_eq!(scan.shaders[0].why_not_usable.as_deref(), Some("the file is not text"));
    assert!(scan.shaders[1].why_not_usable.as_deref().unwrap().contains("nonsense"));
}

#[test]
fn the_folders_below_are_looked_in_to_a_depth_and_those_that_begin_with_a_dot_are_not() {
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "top.fs", VIGNETTE);
    write(
        folder.path(),
        "one/two/three/four/deep.fs",
        &VIGNETTE.replace("0.6", "0.61"),
    );
    write(
        folder.path(),
        "one/two/three/four/five/too deep.fs",
        &VIGNETTE.replace("0.6", "0.62"),
    );
    write(folder.path(), "one/found.fs", &VIGNETTE.replace("0.6", "0.63"));
    write(folder.path(), ".building/hidden.fs", &VIGNETTE.replace("0.6", "0.64"));
    write(folder.path(), ".dot file.fs", &VIGNETTE.replace("0.6", "0.65"));

    let scan = scan_folders(&[folder.path().to_owned()]);

    assert_eq!(names(&scan), ["found.fs", "deep.fs", "top.fs"], "{:?}", names(&scan));
    let shallow = scan_folders_within(&[folder.path().to_owned()], 1, 100);
    assert_eq!(names(&shallow), ["found.fs", "top.fs"], "{:?}", names(&shallow));
    let flat = scan_folders_within(&[folder.path().to_owned()], 0, 100);
    assert_eq!(names(&flat), ["top.fs"]);
}

#[test]
fn only_so_many_files_are_looked_at_and_the_scan_says_that_it_stopped() {
    let folder = tempfile::tempdir().unwrap();
    for number in 0..5 {
        write(
            folder.path(),
            &format!("shader {number}.fs"),
            &VIGNETTE.replace("0.6", &format!("0.{number}1")),
        );
    }
    let scan = scan_folders_within(&[folder.path().to_owned()], 4, 3);
    assert_eq!(names(&scan), ["shader 0.fs", "shader 1.fs", "shader 2.fs"]);
    assert_eq!(scan.problems.len(), 1, "{:?}", scan.problems);
    assert!(
        scan.problems[0].contains("3 shader files were looked at"),
        "{:?}",
        scan.problems
    );

    // Exactly as many as the limit is not stopped.
    let exactly = scan_folders_within(&[folder.path().to_owned()], 4, 5);
    assert_eq!(exactly.shaders.len(), 5);
    assert!(exactly.problems.is_empty());
    // Files that are no shaders count too (they were looked at).
    let other = tempfile::tempdir().unwrap();
    for number in 0..4 {
        write(other.path(), &format!("a{number}.fs"), "module A\n");
    }
    write(other.path(), "z.fs", VIGNETTE);
    let scan = scan_folders_within(&[other.path().to_owned()], 4, 4);
    assert!(scan.shaders.is_empty(), "the shader came after the limit");
    assert_eq!(scan.problems.len(), 1);
}

#[test]
fn the_folders_are_looked_in_in_the_order_they_are_given_and_one_that_is_not_there_is_no_problem() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    write(first.path(), "z.fs", VIGNETTE);
    write(second.path(), "a.fs", PLASMA);

    let scan = scan_folders(&[
        first.path().to_owned(),
        first.path().join("not there"),
        second.path().to_owned(),
    ]);

    assert_eq!(
        names(&scan),
        ["z.fs", "a.fs"],
        "the order of the folders, not of the names"
    );
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    assert!(scan_folders(&[]).shaders.is_empty());
}

#[test]
fn a_shader_in_two_folders_counts_once() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    write(first.path(), "Vignette.fs", VIGNETTE);
    write(second.path(), "Vignette.fs", VIGNETTE);
    write(second.path(), "Plasma.fs", PLASMA);

    let scan = scan_folders(&[first.path().to_owned(), second.path().to_owned()]);

    assert_eq!(scan.shaders.len(), 3, "all three files are listed");
    assert_eq!(scan.usable_count(), 2, "and the same shader twice is one");
    let kinds = scan.usable_kinds();
    assert_eq!(kinds[0].name(), "vignette");
    assert_eq!(kinds[1].name(), "plasma");
    // (The files are in the order of the folders, and in the second folder in the order of the names: the first folder's Vignette, then Plasma, then the second's Vignette.)
    assert_eq!(
        scan.shaders[0].kind, scan.shaders[2].kind,
        "two files with one text are one kind"
    );
    assert_ne!(scan.shaders[0].kind, scan.shaders[1].kind);
    assert_eq!(scan.shaders[0].path, first.path().join("Vignette.fs"));
}

#[test]
fn a_file_that_is_too_long_to_be_a_shader_is_listed_if_it_begins_like_one_and_not_if_it_does_not() {
    let folder = tempfile::tempdir().unwrap();
    let padding = " ".repeat(MOST_BYTES_OF_A_SHADER);
    write(folder.path(), "long.fs", &format!("{VIGNETTE}{padding}"));
    write(folder.path(), "long program.fs", &format!("module Program\n{padding}"));

    let scan = scan_folders(&[folder.path().to_owned()]);

    assert_eq!(names(&scan), ["long.fs"]);
    let why = scan.shaders[0].why_not_usable.as_deref().unwrap();
    assert!(why.contains("are the most a shader can be"), "{why}");
    assert!(scan.shaders[0].kind.is_none());
}

#[test]
fn the_text_that_begins_like_an_isf_file_has_a_comment_that_begins_with_a_brace_wherever_it_is() {
    assert!(begins_like_an_isf_file("/*{ }*/"));
    assert!(begins_like_an_isf_file("/*   \n\t{ \"A\": 1 }*/"));
    assert!(begins_like_an_isf_file(
        "/* a licence */\n/*{ \"INPUTS\": [] }*/\nvoid main() {}"
    ));
    assert!(!begins_like_an_isf_file("/* a licence */\nvoid main() {}"));
    assert!(!begins_like_an_isf_file("{ not a comment }"));
    assert!(!begins_like_an_isf_file(""));
    assert!(!begins_like_an_isf_file("/*"));
}

#[test]
fn the_usual_folders_are_those_of_the_systems_that_have_them_and_none_is_made_up() {
    let folders = usual_folders();
    if cfg!(target_os = "macos") {
        assert_eq!(folders[0], PathBuf::from("/Library/Graphics/ISF"));
        assert!(folders.iter().any(|folder| folder.ends_with("Library/Graphics/ISF")));
    } else if cfg!(windows) {
        assert!(folders.iter().all(|folder| folder.ends_with("ISF")), "{folders:?}");
    } else {
        assert!(folders.is_empty());
    }
}

#[test]
fn a_file_of_exactly_the_most_bytes_is_read_as_a_shader_and_one_byte_more_is_too_long() {
    let folder = tempfile::tempdir().unwrap();
    let mut at_most = VIGNETTE.to_owned();
    at_most.push_str(&" ".repeat(MOST_BYTES_OF_A_SHADER - at_most.len()));
    write(folder.path(), "at most.fs", &at_most);
    at_most.push(' ');
    write(folder.path(), "one more.fs", &at_most);

    let scan = scan_folders(&[folder.path().to_owned()]);

    assert_eq!(names(&scan), ["at most.fs", "one more.fs"]);
    assert!(scan.shaders[0].can_be_used(), "{:?}", scan.shaders[0].why_not_usable);
    assert!(
        scan.shaders[1]
            .why_not_usable
            .as_deref()
            .unwrap()
            .contains("are the most a shader can be")
    );
}
