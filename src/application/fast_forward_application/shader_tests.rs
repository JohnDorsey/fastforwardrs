// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shaders (ISF files) as effects in the window (phase 6, piece 6.3): *Add effect from file...* with a `.fs` or `.isf` file, what is refused and why, the values in the box of
//! the effect, that it is one step of the history, and what a kind that cannot be run says in its box.

use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::shader_flow::NOT_A_SHADER_TITLE;
use super::table_flow::NOT_A_FILE_TITLE;
use super::test_window::Window;
use super::undo_tests::{redo, undo};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::ScriptedFileChooser;
use crate::project::{AnimationTarget, ClipId, EffectKind, GroupKind, MOST_BYTES_OF_A_SHADER, Member, PropertyValue};

const TINT: &str = r#"/*{
	"DESCRIPTION": "mixes the picture with a colour",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "tint", "LABEL": "Tint colour", "TYPE": "color", "DEFAULT": [1.0, 0.0, 0.0, 1.0] },
		{ "NAME": "amount", "LABEL": "How much", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "invert", "LABEL": "Invert it", "TYPE": "bool", "DEFAULT": false }
	]
}*/
void main() {
	vec4 c = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = vec4(mix(c.rgb, tint.rgb, amount), c.a);
}
"#;

// (A transition: the kind of shader that this version cannot run.)
const A_TRANSITION: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"startImage\", \"TYPE\": \"image\" }, { \"NAME\": \"endImage\", \"TYPE\": \"image\" }, { \"NAME\": \"progress\", \"TYPE\": \"float\" } ] }*/\nvoid main() { gl_FragColor = mix(IMG_THIS_PIXEL(startImage), IMG_THIS_PIXEL(endImage), progress); }\n";

const A_GENERATOR: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"speed\", \"TYPE\": \"float\" } ] }*/\nvoid main() { gl_FragColor = vec4(speed); }\n";

/// A window with a video that is selected, nothing unsaved, a chooser that will be asked for files, and the clip.
fn window_that_asks() -> (Window, ClipId, ScriptedFileChooser) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    (window, clip, chooser)
}

fn a_file_in(folder: &Path, name: &str, text: &str) -> PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn add_from_a_file(window: &mut Window, chooser: &ScriptedFileChooser, clip: ClipId, path: &Path) {
    chooser.will_answer(Some(path.to_owned()));
    window.do_this(Command::AddEffectFromFile {
        owner: Member::Clip(clip),
    });
}

fn the_kinds_of_the_effects_of(window: &Window, clip: ClipId) -> Vec<EffectKind> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect()
}

fn the_message(window: &Window) -> Option<(String, String)> {
    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => Some((title.clone(), text.clone())),
        _ => None,
    }
}

#[test]
fn a_shader_file_is_put_on_the_clip_as_an_effect_of_the_kind_it_is_with_the_values_its_header_says() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "tint colour.fs", TINT);
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &file);

    assert_eq!(the_message(&window), None, "nothing was said against it");
    let kinds = the_kinds_of_the_effects_of(&window, clip);
    assert_eq!(kinds.len(), 1);
    let kind = kinds[0];
    assert!(kind.key().starts_with("isf:tint_colour-"), "{}", kind.key());
    assert_eq!(kind.name(), "tint colour");
    assert_eq!(kind.shader_source().unwrap().text, TINT, "the project carries the file");
    assert!(kind.is_installed());
    let effect = window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .values()
        .to_vec();
    assert_eq!(
        effect,
        [
            PropertyValue::Colour(crate::project::TextColour::new(255, 0, 0, 255)),
            PropertyValue::Number(0.5),
            PropertyValue::Flag(false),
        ]
    );
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn the_file_is_asked_for_as_one_of_tables_and_shaders_and_a_table_is_still_a_table() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "look.isf", TINT);
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &file);

    let asked = chooser.questions_asked();
    assert_eq!(asked[0].title, "Choose a file for an effect");
    assert!(
        asked[0]
            .kinds
            .iter()
            .any(|(name, extensions)| name.contains("Shaders") && extensions == &["fs".to_owned(), "isf".to_owned()])
    );
    assert_eq!(
        the_kinds_of_the_effects_of(&window, clip).len(),
        1,
        "an .isf file is a shader too"
    );

    // A table is read as a table, in whatever letters its ending is written.
    let table = a_file_in(
        folder.path(),
        "GRADE.CUBE",
        "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n",
    );
    add_from_a_file(&mut window, &chooser, clip, &table);
    assert_eq!(the_kinds_of_the_effects_of(&window, clip)[1], EffectKind::LUT);
}

#[test]
fn putting_a_shader_on_a_clip_is_one_step_of_the_history_and_is_taken_back_and_done_again_whole() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "tint.fs", TINT);
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &file);

    undo(&mut window);
    assert!(the_kinds_of_the_effects_of(&window, clip).is_empty());
    redo(&mut window);
    let kinds = the_kinds_of_the_effects_of(&window, clip);
    assert_eq!(kinds.len(), 1);
    assert!(kinds[0].shader_source().is_some());
}

#[test]
fn a_shader_can_be_put_on_a_track_and_on_a_group_too() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "tint.fs", TINT);
    let (mut window, clip, chooser) = window_that_asks();
    let track = window.application().project().tracks()[0].id();
    chooser.will_answer(Some(file.clone()));
    window.do_this(Command::AddEffectFromFile {
        owner: Member::Track(track),
    });
    let track_effects = window
        .application()
        .project()
        .effects_of(AnimationTarget::Track(track))
        .unwrap();
    assert_eq!(track_effects.len(), 1);

    let group = window
        .application_mut()
        .document
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clip)])
        .unwrap();
    chooser.will_answer(Some(file));
    window.do_this(Command::AddEffectFromFile {
        owner: Member::Group(group),
    });
    assert_eq!(
        window.application().project().group(group).unwrap().effects().len(),
        1,
        "the effect of a tag"
    );
}

#[test]
fn a_file_that_is_not_a_shader_is_refused_with_the_line_that_is_wrong_and_the_project_is_left_as_it_was() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, clip, chooser) = window_that_asks();
    let bad = a_file_in(
        folder.path(),
        "bad.fs",
        include_str!("../../isf/fixtures/bad_syntax.fs"),
    );
    window.do_this(Command::Deselect);
    select(&mut window, clip);
    let before = window.application().project().clone();

    add_from_a_file(&mut window, &chooser, clip, &bad);

    let (title, text) = the_message(&window).expect("it says why");
    assert_eq!(title, NOT_A_SHADER_TITLE);
    assert!(text.starts_with("bad.fs: line 12, column"), "{text}");
    assert_eq!(window.application().project(), &before, "nothing was changed");
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn what_is_refused_says_so_in_its_words() {
    let folder = tempfile::tempdir().unwrap();
    let cases = [
        ("none.fs", "void main() {}\n".to_owned(), "no header"),
        ("generator.fs", A_GENERATOR.to_owned(), "generator"),
        (
            "huge.fs",
            format!("{TINT}// {}\n", "x".repeat(MOST_BYTES_OF_A_SHADER)),
            "the most a shader can be",
        ),
        (
            "odd.fs",
            "/*{ \"INPUTS\": [ { \"NAME\": \"TIME\", \"TYPE\": \"float\" } ] }*/\n".to_owned(),
            "of its own",
        ),
    ];
    for (name, text, expected) in cases {
        let (mut window, clip, chooser) = window_that_asks();
        let file = a_file_in(folder.path(), name, &text);

        add_from_a_file(&mut window, &chooser, clip, &file);

        let (title, said) = the_message(&window).unwrap_or_else(|| panic!("{name} says nothing"));
        assert!(
            title == NOT_A_SHADER_TITLE || title == NOT_A_FILE_TITLE,
            "{name}: {title}"
        );
        assert!(said.contains(expected), "{name}: {said}");
        assert!(the_kinds_of_the_effects_of(&window, clip).is_empty(), "{name}");
    }
}

#[test]
fn a_shader_of_exactly_the_most_bytes_there_can_be_is_taken_and_one_byte_more_is_not() {
    let folder = tempfile::tempdir().unwrap();
    let padding = |bytes: usize| format!("{TINT}// {}", "x".repeat(bytes - TINT.len() - 3));
    let (mut window, clip, chooser) = window_that_asks();

    let at_the_limit = padding(MOST_BYTES_OF_A_SHADER);
    assert_eq!(at_the_limit.len(), MOST_BYTES_OF_A_SHADER);
    add_from_a_file(
        &mut window,
        &chooser,
        clip,
        &a_file_in(folder.path(), "at_the_limit.fs", &at_the_limit),
    );
    assert_eq!(the_message(&window), None);
    assert_eq!(the_kinds_of_the_effects_of(&window, clip).len(), 1);

    add_from_a_file(
        &mut window,
        &chooser,
        clip,
        &a_file_in(folder.path(), "over_it.fs", &padding(MOST_BYTES_OF_A_SHADER + 1)),
    );
    assert!(the_message(&window).unwrap().1.contains("the most a shader can be"));
    assert_eq!(the_kinds_of_the_effects_of(&window, clip).len(), 1);
}

#[test]
fn a_file_that_is_not_text_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("binary.fs");
    std::fs::write(&path, [0xff, 0xfe, 0x00, 0x80]).unwrap();
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &path);

    let (title, text) = the_message(&window).unwrap();
    assert_eq!(title, NOT_A_SHADER_TITLE);
    assert!(text.contains("not text"), "{text}");
    assert!(the_kinds_of_the_effects_of(&window, clip).is_empty());
}

#[test]
fn a_file_that_cannot_be_read_says_so() {
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, Path::new("/nowhere/at/all.fs"));
    let (title, _) = the_message(&window).unwrap();
    assert_eq!(title, NOT_A_FILE_TITLE);
}

#[test]
fn cancelling_the_question_changes_nothing() {
    let (mut window, clip, chooser) = window_that_asks();
    chooser.will_answer(None);
    window.do_this(Command::AddEffectFromFile {
        owner: Member::Clip(clip),
    });
    assert!(the_kinds_of_the_effects_of(&window, clip).is_empty());
    assert_eq!(the_message(&window), None);
}

#[test]
fn the_box_of_a_shaders_effect_has_a_control_for_each_input_called_by_its_label() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "mixer.fs", TINT);
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &file);
    window.settle();

    assert!(
        window.harness.query_by_label_contains("Mixer").is_some(),
        "the box of the effect is called by the name of the file"
    );
    for label in ["Tint colour", "How much", "Invert it"] {
        assert!(window.harness.query_by_label(label).is_some(), "{label}");
    }
}

#[test]
fn a_kind_that_this_version_cannot_run_says_why_in_its_box_and_keeps_its_values() {
    // A project that was made with a shader that this version cannot run (a transition, which it cannot yet): the effect stays, with its value, and says why it does nothing.
    // (Rewritten on purpose in phase 6: a generator is run now, so the kind that cannot be is a transition.)
    let file = crate::isf::read(A_TRANSITION).unwrap();
    let kind = EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "fade.fs", A_TRANSITION)).unwrap();
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window
        .application_mut()
        .document
        .project
        .add_effect(AnimationTarget::Clip(clip), kind)
        .unwrap();
    window.settle();

    assert!(!kind.is_installed());
    assert!(
        window.harness.query_by_label_contains("transition").is_some(),
        "the box says why: it is a transition, which this version cannot run yet"
    );
    assert!(
        window.harness.query_by_label("progress").is_some(),
        "its value is still there"
    );
}

#[test]
fn a_colour_and_a_point_of_a_shader_have_controls_and_not_only_words() {
    let source = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"centre\", \"LABEL\": \"Centre\", \"TYPE\": \"point2D\", \"DEFAULT\": [100, 50] }, { \"NAME\": \"glow\", \"LABEL\": \"Glow colour\", \"TYPE\": \"color\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage) + glow + vec4(centre, 0.0, 0.0); }\n";
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "glow.fs", source);
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &file);
    window.settle();

    assert_eq!(
        window.harness.query_all_by_label("Centre").count(),
        2,
        "two boxes to drag in, across and down"
    );
    assert_eq!(
        window.harness.query_all_by_label("Glow colour").count(),
        1,
        "a colour button"
    );
    assert!(
        window.harness.query_by_label_contains("Glow colour:").is_none(),
        "not the words that said what it was"
    );
    // What changes evenly between two keyframes, a point and a colour, has the stopwatch (it shows when the pointer is on the row).
    for (row, name) in [("Centre", "centre"), ("Glow colour", "glow colour")] {
        let middle = window.rectangle_of_the_nth(row, 0).center();
        window.move_pointer_to(middle);
        assert_eq!(
            window.how_many_are_called(&format!("Make a keyframe of {name} here")),
            1,
            "{row}"
        );
    }
}

const REMEMBERS: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true }, {} ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(memory); }\n";

#[test]
fn the_box_of_a_shader_that_keeps_something_from_one_frame_to_the_next_says_so_and_no_other_does() {
    let folder = tempfile::tempdir().unwrap();
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(
        &mut window,
        &chooser,
        clip,
        &a_file_in(folder.path(), "remembers.fs", REMEMBERS),
    );
    window.settle();
    assert!(
        window
            .harness
            .query_by_label_contains("Depends on earlier frames")
            .is_some()
    );
    assert!(
        window.harness.query_by_label_contains("The export is exact").is_some(),
        "and that the export is not touched by it"
    );

    let (mut other, other_clip, other_chooser) = window_that_asks();
    add_from_a_file(
        &mut other,
        &other_chooser,
        other_clip,
        &a_file_in(folder.path(), "mixer.fs", TINT),
    );
    other.settle();
    assert!(
        other
            .harness
            .query_by_label_contains("Depends on earlier frames")
            .is_none(),
        "a shader that keeps nothing says nothing of it"
    );
}

// --- the images that a shader brings with it (phase 6.4.4) ---

/// A shader that reads the image `tex`, which is the file `tex.png` beside it (or where `path` says).
fn a_shader_that_brings_an_image(path: &str) -> String {
    format!(
        "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }} ], \"IMPORTED\": {{ \"tex\": {{ \"PATH\": {path:?} }} }} }}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage) * IMG_THIS_PIXEL(tex); }}\n"
    )
}

fn a_png_file_in(folder: &Path, name: &str) -> Vec<u8> {
    let bytes = crate::rgba_picture::RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: [255, 128, 0, 255].repeat(4),
    }
    .to_png_bytes()
    .unwrap();
    if let Some(parent) = folder.join(name).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(folder.join(name), &bytes).unwrap();
    bytes
}

#[test]
fn a_shader_that_brings_an_image_is_put_on_the_clip_with_the_image_that_is_beside_it() {
    let folder = tempfile::tempdir().unwrap();
    let png = a_png_file_in(folder.path(), "tex.png");
    let file = a_file_in(folder.path(), "textured.fs", &a_shader_that_brings_an_image("tex.png"));
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &file);

    assert_eq!(the_message(&window), None);
    let kinds = the_kinds_of_the_effects_of(&window, clip);
    assert_eq!(kinds.len(), 1);
    let images = &kinds[0].shader_source().unwrap().images;
    assert_eq!(images.len(), 1);
    assert_eq!(
        (images[0].name(), &**images[0].bytes()),
        ("tex", png.as_slice()),
        "the project carries the file of the image"
    );
    assert!(kinds[0].is_installed());
}

#[test]
fn the_image_may_be_in_a_folder_below_the_shader_and_a_current_folder_in_the_path_is_nothing() {
    let folder = tempfile::tempdir().unwrap();
    a_png_file_in(folder.path(), "textures/inner/tex.png");
    let file = a_file_in(
        folder.path(),
        "textured.fs",
        &a_shader_that_brings_an_image("./textures/inner/tex.png"),
    );
    let (mut window, clip, chooser) = window_that_asks();

    add_from_a_file(&mut window, &chooser, clip, &file);

    assert_eq!(the_message(&window), None);
    assert!(the_kinds_of_the_effects_of(&window, clip)[0].is_installed());
}

#[test]
fn an_image_that_cannot_be_used_refuses_the_shader_and_says_which_and_why_and_changes_nothing() {
    let folder = tempfile::tempdir().unwrap();
    a_png_file_in(folder.path(), "tex.png");
    std::fs::write(folder.path().join("text.png"), b"this is text").unwrap();
    let absolute = folder.path().join("tex.png").to_string_lossy().into_owned();
    for (path, expected) in [
        ("missing.png", "the image tex (missing.png)"),
        ("text.png", "is not a PNG or a JPEG"),
        ("../tex.png", "not a file in the folder of the shader or below it"),
        (
            "textures/../../tex.png",
            "not a file in the folder of the shader or below it",
        ),
        (absolute.as_str(), "not a file in the folder of the shader or below it"),
        ("", "not a file in the folder of the shader or below it"),
    ] {
        let file = a_file_in(folder.path(), "textured.fs", &a_shader_that_brings_an_image(path));
        let (mut window, clip, chooser) = window_that_asks();
        window.application_mut().note_everything_is_saved();

        add_from_a_file(&mut window, &chooser, clip, &file);

        let (title, text) = the_message(&window).unwrap_or_else(|| panic!("{path:?}: nothing was said"));
        assert_eq!(title, NOT_A_SHADER_TITLE, "{path:?}");
        assert!(text.contains(expected), "{path:?}: {text}");
        assert!(text.starts_with("textured.fs: "), "{path:?}: {text}");
        assert!(the_kinds_of_the_effects_of(&window, clip).is_empty(), "{path:?}");
        assert!(!window.application().has_unsaved_changes(), "{path:?}");
    }
}

#[test]
fn an_image_of_too_many_bytes_or_pixels_and_a_shader_with_too_many_images_are_refused() {
    use crate::project::{
        MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER, MOST_IMAGES_A_SHADER_BRINGS, MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER,
    };
    let folder = tempfile::tempdir().unwrap();
    // More bytes than an image may have.
    let mut big = crate::rgba_picture::RgbaPicture {
        width_in_pixels: 1,
        height_in_pixels: 1,
        rgba_pixels: vec![0, 0, 0, 255],
    }
    .to_png_bytes()
    .unwrap();
    big.resize(MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER + 1, 0);
    std::fs::write(folder.path().join("big.png"), &big).unwrap();
    // More pixels across than an image may have.
    let wide = crate::rgba_picture::RgbaPicture {
        width_in_pixels: MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER + 1,
        height_in_pixels: 1,
        rgba_pixels: vec![0; 4 * (MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER as usize + 1)],
    }
    .to_png_bytes()
    .unwrap();
    std::fs::write(folder.path().join("wide.png"), &wide).unwrap();
    for (path, expected) in [("big.png", "are the most"), ("wide.png", "across or down are the most")] {
        let file = a_file_in(folder.path(), "textured.fs", &a_shader_that_brings_an_image(path));
        let (mut window, clip, chooser) = window_that_asks();
        add_from_a_file(&mut window, &chooser, clip, &file);
        let (_, text) = the_message(&window).unwrap_or_else(|| panic!("{path}: nothing was said"));
        assert!(text.contains(expected), "{path}: {text}");
        assert!(the_kinds_of_the_effects_of(&window, clip).is_empty());
    }

    // As many images as there may be are taken, and one more is not.
    a_png_file_in(folder.path(), "tex.png");
    let shader_with = |count: usize| {
        let imported: Vec<String> = (0..count)
            .map(|number| format!("\"tex{number}\": {{ \"PATH\": \"tex.png\" }}"))
            .collect();
        let uses: Vec<String> = (0..count)
            .map(|number| format!("IMG_THIS_PIXEL(tex{number})"))
            .collect();
        format!(
            "/*{{ \"INPUTS\": [ {{ \"NAME\": \"inputImage\", \"TYPE\": \"image\" }} ], \"IMPORTED\": {{ {} }} }}*/\nvoid main() {{ gl_FragColor = IMG_THIS_PIXEL(inputImage){}; }}\n",
            imported.join(", "),
            uses.iter().map(|call| format!(" + {call}")).collect::<String>()
        )
    };
    let file = a_file_in(folder.path(), "many.fs", &shader_with(MOST_IMAGES_A_SHADER_BRINGS));
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &file);
    // (Six images and the picture itself are seven pictures, which is as many as a shader may read.)
    assert_eq!(the_message(&window), None);
    assert_eq!(
        the_kinds_of_the_effects_of(&window, clip)[0]
            .shader_source()
            .unwrap()
            .images
            .len(),
        MOST_IMAGES_A_SHADER_BRINGS
    );
    let file = a_file_in(
        folder.path(),
        "too_many.fs",
        &shader_with(MOST_IMAGES_A_SHADER_BRINGS + 1),
    );
    let (mut window, clip, chooser) = window_that_asks();
    add_from_a_file(&mut window, &chooser, clip, &file);
    let (_, text) = the_message(&window).expect("it was refused");
    assert!(text.contains("brings 7 images, and 6 are the most"), "{text}");
}
