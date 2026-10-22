// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a shader file: what it becomes, what is refused and why, and the images that lie beside it.

use std::path::{Path, PathBuf};

use super::*;

const VIGNETTE: &str = include_str!("../../isf/fixtures/vignette.fs");
const PLASMA: &str = include_str!("../../isf/fixtures/plasma.fs");
const WIPE: &str = include_str!("../../isf/fixtures/wipe_transition.fs");
const NEEDS_A_MASK: &str = include_str!("../../isf/fixtures/needs_a_mask.fs");
const BAD: &str = include_str!("../../isf/fixtures/bad_syntax.fs");

fn written(folder: &Path, name: &str, text: &str) -> PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn a_png() -> Vec<u8> {
    crate::rgba_picture::RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: vec![200; 16],
    }
    .to_png_bytes()
    .unwrap()
}

fn a_shader_that_brings(path: &str) -> String {
    format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }} ], \"IMPORTED\": {{ \"map\": {{ \"PATH\": {path:?} }} }} }}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage) + IMG_THIS_PIXEL(map); }}\n"
    )
}

#[test]
fn a_filter_a_generator_and_a_transition_are_read_each_with_what_it_is_for() {
    let folder = tempfile::tempdir().unwrap();
    let filter = read_a_shader_file(&written(folder.path(), "Vignette.fs", VIGNETTE)).unwrap();
    assert_eq!(filter.shape, Shape::Filter);
    assert_eq!(filter.file_name, "Vignette.fs");
    assert!(filter.why_not_usable.is_none(), "{:?}", filter.why_not_usable);
    assert!(filter.kind.key().starts_with("isf:vignette-"), "{}", filter.kind.key());
    assert_eq!(filter.kind.name(), "vignette");
    assert!(filter.images.is_empty());
    assert_eq!(filter.text, VIGNETTE);

    let generator = read_a_shader_file(&written(folder.path(), "plasma.fs", PLASMA)).unwrap();
    assert_eq!(generator.shape, Shape::Generator);
    assert!(generator.why_not_usable.is_none(), "{:?}", generator.why_not_usable);
    assert!(generator.kind.is_a_generator());

    // (Rewritten on purpose in phase 6.6: a transition is run now.)
    let transition = read_a_shader_file(&written(folder.path(), "Wipe.fs", WIPE)).unwrap();
    assert_eq!(transition.shape, Shape::Transition);
    assert!(transition.why_not_usable.is_none(), "{:?}", transition.why_not_usable);
    assert!(transition.kind.is_a_transition());

    // A shader that wants a picture that this version cannot give it is read, and is kept with the reason that it is not run: it is still a shader.
    let masked = read_a_shader_file(&written(folder.path(), "Mask.fs", NEEDS_A_MASK)).unwrap();
    assert_eq!(masked.shape, Shape::Filter);
    assert!(masked.why_not_usable.as_deref().unwrap().contains("maskImage"));
}

#[test]
fn one_text_is_one_kind_wherever_it_is_read_and_another_text_is_another() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = read_a_shader_file(&written(first.path(), "Vignette.fs", VIGNETTE)).unwrap();
    let b = read_a_shader_file(&written(second.path(), "Vignette.fs", VIGNETTE)).unwrap();
    assert_eq!(a.kind, b.kind, "the same file in two folders is one kind of effect");
    let changed = read_a_shader_file(&written(second.path(), "Vignette.fs", &VIGNETTE.replace("0.6", "0.7"))).unwrap();
    assert_ne!(a.kind, changed.kind);
}

#[test]
fn a_file_that_is_not_a_shader_is_refused_and_says_what_is_wrong_with_the_line_and_the_column() {
    let folder = tempfile::tempdir().unwrap();
    let problem = read_a_shader_file(&written(folder.path(), "Broken.fs", BAD));
    // (The mistakes of that file are the compiler's, found when it is made into a kind: it reads, and cannot be run.)
    let broken = problem.unwrap();
    let why = broken.why_not_usable.expect("it cannot be run");
    assert!(why.contains("line"), "{why}");

    let no_header = read_a_shader_file(&written(
        folder.path(),
        "Plain.fs",
        "void main() { gl_FragColor = vec4(1.0); }\n",
    ))
    .unwrap_err();
    assert!(
        matches!(&no_header, ShaderFileProblem::NotAShader(why) if why.contains("no header")),
        "{no_header:?}"
    );

    let wrong_header = read_a_shader_file(&written(
        folder.path(),
        "Wrong.fs",
        "/*{ \"INPUTS\": [ { \"NAME\": \"a\", \"TYPE\": \"nonsense\" } ] }*/\nvoid main() {}\n",
    ))
    .unwrap_err();
    assert!(
        matches!(wrong_header, ShaderFileProblem::NotAShader(_)),
        "{wrong_header:?}"
    );
}

#[test]
fn a_file_that_is_not_text_a_file_that_is_missing_and_a_file_that_is_too_long_are_each_refused_in_their_own_way() {
    let folder = tempfile::tempdir().unwrap();
    let bytes = folder.path().join("Bytes.fs");
    std::fs::write(&bytes, [0xff, 0xfe, 0x00, 0x80]).unwrap();
    assert_eq!(
        read_a_shader_file(&bytes).unwrap_err(),
        ShaderFileProblem::NotAShader("the file is not text".to_owned())
    );
    let missing = read_a_shader_file(&folder.path().join("Nothing.fs")).unwrap_err();
    assert!(matches!(missing, ShaderFileProblem::CouldNotBeRead(_)), "{missing:?}");
    let long = format!("{VIGNETTE}{}", " ".repeat(MOST_BYTES_OF_A_SHADER));
    let too_long = read_a_shader_file(&written(folder.path(), "Long.fs", &long)).unwrap_err();
    assert!(
        matches!(&too_long, ShaderFileProblem::NotAShader(why) if why.contains("are the most a shader can be")),
        "{too_long:?}"
    );
}

#[test]
fn the_most_bytes_there_can_be_are_read_and_one_more_is_not() {
    let folder = tempfile::tempdir().unwrap();
    let mut text = VIGNETTE.to_owned();
    text.push_str(&" ".repeat(MOST_BYTES_OF_A_SHADER - text.len()));
    assert_eq!(text.len(), MOST_BYTES_OF_A_SHADER);
    assert!(read_a_shader_file(&written(folder.path(), "Exactly.fs", &text)).is_ok());
    text.push(' ');
    assert!(read_a_shader_file(&written(folder.path(), "OneMore.fs", &text)).is_err());
}

#[test]
fn the_name_of_the_kind_is_made_from_the_name_of_the_file() {
    let made = kind_of_a_shader("Random Shape-2.fs", VIGNETTE, Vec::new()).unwrap();
    assert_eq!(made.kind.name(), "random shape 2");
    assert!(
        made.kind.key().starts_with("isf:random_shape_2-"),
        "{}",
        made.kind.key()
    );
    assert_eq!(made.shape, Shape::Filter);
}

// --- the images that lie beside a shader ---

#[test]
fn an_image_beside_the_shader_and_one_in_a_folder_below_it_are_read_in_the_order_the_header_lists_them() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("maps")).unwrap();
    std::fs::write(folder.path().join("maps").join("map.png"), a_png()).unwrap();
    let path = written(folder.path(), "Map.fs", &a_shader_that_brings("maps/map.png"));
    let images = images_beside(&path, &a_shader_that_brings("maps/map.png")).unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].name(), "map");

    let file = read_a_shader_file(&path).unwrap();
    assert_eq!(file.images.len(), 1);
    assert_eq!(
        file.kind.shader_source().unwrap().images.len(),
        1,
        "the kind keeps the image inside the shader"
    );
}

#[test]
fn a_path_that_leaves_the_folder_or_says_nothing_is_refused_and_so_is_one_that_is_not_there() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("map.png"), a_png()).unwrap();
    let absolute = folder.path().join("map.png").to_string_lossy().replace('\\', "/");
    for (said, wanted_in_the_words) in [
        (absolute.as_str(), "not a file in the folder of the shader or below it"),
        ("../map.png", "not a file in the folder of the shader or below it"),
        ("  ", "not a file in the folder of the shader or below it"),
        ("missing.png", "the image map (missing.png)"),
    ] {
        let text = a_shader_that_brings(said);
        let path = written(folder.path(), "Map.fs", &text);
        let why = images_beside(&path, &text).unwrap_err();
        assert!(why.contains(wanted_in_the_words), "{said:?}: {why}");
        assert!(
            matches!(read_a_shader_file(&path), Err(ShaderFileProblem::NotAShader(_))),
            "{said:?}"
        );
    }
}

#[test]
fn an_image_that_is_no_picture_is_refused_with_its_name_and_a_file_with_no_header_has_no_images_to_read() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("map.png"), b"not a picture").unwrap();
    let text = a_shader_that_brings("map.png");
    let path = written(folder.path(), "Map.fs", &text);
    let why = images_beside(&path, &text).unwrap_err();
    assert!(
        why.contains("the image map (map.png)") && why.contains("not a PNG or a JPEG"),
        "{why}"
    );
    assert_eq!(images_beside(&path, "void main() {}").unwrap(), Vec::new());
}

#[test]
fn as_many_images_as_a_shader_may_bring_are_read_and_one_more_is_not() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("map.png"), a_png()).unwrap();
    let with = |count: usize| {
        let images: Vec<String> = (0..count)
            .map(|number| format!("\"map{number}\": {{ \"PATH\": \"map.png\" }}"))
            .collect();
        format!(
            "/*{{ \"IMPORTED\": {{ {} }} }}*/\nvoid main() {{}}\n",
            images.join(", ")
        )
    };
    let at_most = with(MOST_IMAGES_A_SHADER_BRINGS);
    let path = written(folder.path(), "Many.fs", &at_most);
    assert_eq!(
        images_beside(&path, &at_most).unwrap().len(),
        MOST_IMAGES_A_SHADER_BRINGS
    );
    let too_many = with(MOST_IMAGES_A_SHADER_BRINGS + 1);
    let why = images_beside(&path, &too_many).unwrap_err();
    assert!(why.contains("are the most"), "{why}");
}
