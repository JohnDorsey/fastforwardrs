// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders that come with the program: that the list is the files that are there, that each one reads, compiles and can be used, that each has what a person needs of it (a name, a
//! description, a label on every input, a range that holds its first value), that each draws a picture that is not blank, and **that every input of each does something** (a slider that
//! changes nothing is a mistake that nobody sees until they use it).

use std::collections::BTreeSet;
use std::path::Path;

use super::*;
use crate::engine::testing::with_software_graphics;
use crate::engine::{FrameRenderer, PictureData, RenderedFrame};
use crate::isf::{IsfInputKind, Shape, read};
use crate::project::{MOST_BYTES_OF_A_SHADER, ParameterControl, PropertyValue};
use crate::shader_files::{ShaderOnARenderer, a_scene_to_look_at};

/// How big the pictures are that the shaders are drawn at in these tests: small, because a software adapter does the work, and enough that a slider that does something is seen.
const SIZE: (u32, u32) = (48, 27);

/// The first two lines of every file: who made it and under which licence. (The names of the tags are cut from what follows them in the source: a file that has the text of a tag in it is
/// read by `reuse lint` as having that tag, and what follows it here is not a licence.)
const THE_HEADER_OF_EVERY_FILE: &str = concat!(
    "// SPDX-FileCopyrightText",
    ": 2026 John Dorsey\n// SPDX-License-Identifier",
    ": CC0-1.0\n"
);

/// `folder/file name` of every shader file under `assets/shaders`, by name.
fn files_in_the_source() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let root = Path::new(FOLDER_OF_THE_FILES_IN_THE_SOURCE);
    for folder in std::fs::read_dir(root)
        .expect("the folder of the shaders is there")
        .flatten()
    {
        if !folder.path().is_dir() {
            continue;
        }
        for file in std::fs::read_dir(folder.path())
            .expect("a folder can be read")
            .flatten()
        {
            let name = file.file_name().to_string_lossy().into_owned();
            if file.path().is_file() && (name.ends_with(".fs") || name.ends_with(".isf")) {
                found.insert(format!("{}/{name}", folder.file_name().to_string_lossy()));
            }
        }
    }
    found
}

#[test]
fn the_list_of_the_shaders_of_the_program_is_the_files_that_are_in_the_folder_of_the_shaders() {
    let listed: BTreeSet<String> = SHADERS_OF_THE_PROGRAM
        .iter()
        .map(|shader| format!("{}/{}", shader.folder, shader.file_name))
        .collect();
    assert_eq!(
        listed.len(),
        SHADERS_OF_THE_PROGRAM.len(),
        "no shader is in the list twice"
    );
    assert_eq!(
        listed,
        files_in_the_source(),
        "a file that is in assets/shaders and not in the list is not in the program; a name that is in the list and has no file would not build"
    );
}

#[test]
fn every_shader_of_the_program_has_the_header_of_a_file_made_for_the_program() {
    for shader in SHADERS_OF_THE_PROGRAM {
        assert!(
            shader.text.starts_with(THE_HEADER_OF_EVERY_FILE),
            "{} does not begin with the two lines of SPDX that say who made it and under which licence (the licence of the lines is what `reuse lint` reads)",
            shader.file_name
        );
        assert!(
            shader.text.len() <= MOST_BYTES_OF_A_SHADER,
            "{} is {} bytes, and {MOST_BYTES_OF_A_SHADER} are the most a shader can be",
            shader.file_name,
            shader.text.len()
        );
        assert!(
            !shader.text.contains('\r'),
            "{} has the line ends of Windows in it",
            shader.file_name
        );
    }
}

#[test]
fn every_shader_of_the_program_is_read_and_compiled_and_can_be_used_and_is_said_to_be_built_into_the_program() {
    let scan = scan_of_the_shaders_of_the_program();
    assert_eq!(scan.shaders.len(), SHADERS_OF_THE_PROGRAM.len());
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    for (shader, found) in SHADERS_OF_THE_PROGRAM.iter().zip(&scan.shaders) {
        assert!(
            found.can_be_used(),
            "{} cannot be used: {:?}",
            shader.file_name,
            found.why_not_usable
        );
        assert!(found.built_into_the_program, "{}", shader.file_name);
        assert_eq!(found.place(), "built into the program");
        assert_eq!(found.file_name, shader.file_name);
    }
    assert_eq!(
        scan.usable_count(),
        SHADERS_OF_THE_PROGRAM.len(),
        "no two of them are one kind of effect"
    );
}

#[test]
fn every_shader_of_the_program_has_a_name_a_description_a_category_and_a_label_for_every_input() {
    let mut names = BTreeSet::new();
    for shader in SHADERS_OF_THE_PROGRAM {
        let file = read(shader.text).unwrap_or_else(|problem| panic!("{}: {problem}", shader.file_name));
        assert!(
            file.description.trim().len() >= 40,
            "{}: the description says what it is, in a sentence or two",
            shader.file_name
        );
        assert!(!file.categories.is_empty(), "{}: it has a category", shader.file_name);
        assert!(
            !file.credit.trim().is_empty(),
            "{}: it says who made it (CREDIT)",
            shader.file_name
        );
        assert!(
            names.insert(shader.file_name.to_lowercase()),
            "{}: another shader has the same name, apart from the case",
            shader.file_name
        );
        for input in &file.inputs {
            if input.kind == IsfInputKind::Image {
                continue;
            }
            assert_ne!(
                input.label, input.name,
                "{}: the input {} has no LABEL, which is what a person is shown",
                shader.file_name, input.name
            );
            match &input.kind {
                IsfInputKind::Float {
                    minimum,
                    maximum,
                    default,
                } => assert!(
                    minimum < maximum && (minimum..=maximum).contains(&default),
                    "{}: the range of {} is {minimum} to {maximum}, and it begins as {default}",
                    shader.file_name,
                    input.name
                ),
                IsfInputKind::Long {
                    values,
                    labels,
                    default,
                } => assert!(
                    values.contains(default) && labels.len() == values.len() && values.len() >= 2,
                    "{}: the choice {} has its labels and its first value among its values",
                    shader.file_name,
                    input.name
                ),
                _ => {}
            }
        }
        assert!(
            file.numbers_of_the_inputs() <= crate::isf::MOST_NUMBERS_OF_THE_INPUTS,
            "{}: its inputs are {} numbers, and {} are the most",
            shader.file_name,
            file.numbers_of_the_inputs(),
            crate::isf::MOST_NUMBERS_OF_THE_INPUTS
        );
    }
}

#[test]
fn the_shaders_of_the_fractals_folder_are_generators() {
    for shader in SHADERS_OF_THE_PROGRAM
        .iter()
        .filter(|shader| shader.folder == "fractals")
    {
        let file = read(shader.text).unwrap();
        assert_eq!(
            file.shape(),
            Shape::Generator,
            "{} is in the folder of the fractals, which are generators: they make a picture of their own",
            shader.file_name
        );
    }
}

#[test]
fn a_scan_that_is_followed_by_another_keeps_the_first_of_two_files_that_are_one_kind() {
    let ours = scan_of_the_shaders_of_the_program();
    let mut theirs = scan_of_the_shaders_of_the_program();
    for shader in &mut theirs.shaders {
        shader.built_into_the_program = false;
        shader.path = std::path::PathBuf::from("/a/folder").join(&shader.file_name);
    }
    theirs.problems.push("a folder could not be looked in".to_owned());
    let both = ours.followed_by(theirs);
    assert_eq!(both.shaders.len(), 2 * SHADERS_OF_THE_PROGRAM.len());
    assert_eq!(both.problems, ["a folder could not be looked in"]);
    assert_eq!(
        both.usable_count(),
        SHADERS_OF_THE_PROGRAM.len(),
        "a shader that is in a folder too is the same kind of effect, and is listed once"
    );
    assert!(
        both.shaders[..SHADERS_OF_THE_PROGRAM.len()]
            .iter()
            .all(|shader| shader.built_into_the_program),
        "the ones that come with the program are first"
    );
    assert!(
        both.shaders[SHADERS_OF_THE_PROGRAM.len()..]
            .iter()
            .all(|shader| shader.place().starts_with("/a/folder")),
        "the others are where they were found"
    );
}

// ---- the code that shaders share ----

/// What a line begins with where a copy of a shared block begins (see `tools/shaders/sync_shared_blocks.py`).
const BEGINNING_OF_A_SHARED_BLOCK: &str = "// --- shared: ";

/// The shared blocks that `text` has, each as its name, the line that begins it and the code between that line and the line that ends it.
fn shared_blocks_in(text: &str) -> Vec<(String, String, String)> {
    let mut blocks = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix(BEGINNING_OF_A_SHARED_BLOCK) else {
            continue;
        };
        let name = rest.split(' ').next().unwrap().to_owned();
        let end = format!("// --- shared: {name} (end) ---");
        let mut code = String::new();
        let mut ended = false;
        for inside in lines.by_ref() {
            if inside == end {
                ended = true;
                break;
            }
            code.push_str(inside);
            code.push('\n');
        }
        assert!(ended, "the block {name} is never ended: {end} is missing");
        blocks.push((name, line.to_owned(), code));
    }
    blocks
}

/// The code of the shared block `name`, as the file of it in `assets/shaders/shared` has it (after its two lines of SPDX).
fn true_text_of_the_shared_block(name: &str) -> String {
    let file = Path::new(FOLDER_OF_THE_FILES_IN_THE_SOURCE)
        .join("shared")
        .join(format!("{name}.glsl"));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(concat!("// SPDX-FileCopyrightText", ": 2026 John Dorsey")),
        "{name}.glsl"
    );
    assert_eq!(
        lines.next(),
        Some(concat!("// SPDX-License-Identifier", ": CC0-1.0")),
        "{name}.glsl"
    );
    let code: Vec<&str> = lines.collect();
    format!("{}\n", code.join("\n").trim_matches('\n'))
}

/// The names that the code of the shared block `name` reads from the inputs of the shader that has it (it is the shader that has them).
fn names_that_a_shared_block_takes_from_the_inputs(name: &str) -> &'static [&'static str] {
    match name {
        "palettes" => &["colourA", "colourB"],
        "raymarch" => &[
            "cameraTurn",
            "cameraTilt",
            "cameraDistance",
            "cameraSpin",
            "fieldOfView",
            "maxSteps",
            "detail",
            "lightTurn",
            "glowAmount",
            "fogAmount",
            "backgroundTop",
            "backgroundBottom",
            "colourScale",
            "colourOffset",
            "colourCycle",
            "palette",
            "samples",
        ],
        _ => &[],
    }
}

/// The words of the code of `text`, whole (a word is letters, digits and underscores), with what the comments say left out.
fn words_of_the_code_in(text: &str) -> BTreeSet<String> {
    let mut words = BTreeSet::new();
    for line in text.lines() {
        let code = line.split("//").next().unwrap_or("");
        for word in code.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_')) {
            if !word.is_empty() {
                words.insert(word.to_owned());
            }
        }
    }
    words
}

#[test]
fn no_input_of_a_shader_has_the_name_of_something_in_the_shared_blocks_it_has() {
    // (The name of an input is a macro in the shader, and **the macro is put in wherever the word is**: a parameter of a function of a block that is called `reach`, in a shader with an
    // input `reach`, becomes `vec3 _isf_u[1].y`, and the compiler says only that it expected a parenthesis.)
    for shader in SHADERS_OF_THE_PROGRAM {
        let file = read(shader.text).unwrap();
        for (block, _, code) in shared_blocks_in(shader.text) {
            let words = words_of_the_code_in(&code);
            let taken = names_that_a_shared_block_takes_from_the_inputs(&block);
            for input in &file.inputs {
                assert!(
                    !words.contains(&input.name) || taken.contains(&input.name.as_str()),
                    "{}: the input {} has the name of a word in the shared block {block}, which would be put in its place there: give the input another name",
                    shader.file_name,
                    input.name
                );
            }
            for needed in taken {
                assert!(
                    file.inputs.iter().any(|input| input.name == *needed),
                    "{}: the shared block {block} reads the input {needed}, which the shader does not have",
                    shader.file_name
                );
            }
        }
    }
}

#[test]
fn the_copies_of_a_shared_block_are_all_the_true_text_of_it() {
    let mut blocks_used = BTreeSet::new();
    for shader in SHADERS_OF_THE_PROGRAM {
        for (name, beginning, code) in shared_blocks_in(shader.text) {
            assert_eq!(
                beginning,
                format!(
                    "// --- shared: {name} (begin: a copy of assets/shaders/shared/{name}.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---"
                ),
                "{}: the line that begins the block {name}",
                shader.file_name
            );
            assert_eq!(
                code,
                true_text_of_the_shared_block(&name),
                "{}: its copy of the block {name} is not the text in assets/shaders/shared/{name}.glsl (run tools/shaders/sync_shared_blocks.py)",
                shader.file_name
            );
            blocks_used.insert(name);
        }
    }
    let files: BTreeSet<String> = std::fs::read_dir(Path::new(FOLDER_OF_THE_FILES_IN_THE_SOURCE).join("shared"))
        .expect("the folder of the shared blocks is there")
        .flatten()
        .map(|file| file.file_name().to_string_lossy().trim_end_matches(".glsl").to_owned())
        .collect();
    assert_eq!(
        blocks_used, files,
        "every shared block is used by a shader, and every block that is used has its file"
    );
}

// ---- drawing ----

/// How many pixels of `second` are not what they are in `first`, by three levels of a colour or more in some number.
fn pixels_that_differ(first: &RenderedFrame, second: &RenderedFrame) -> usize {
    first
        .rgba_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(second.rgba_pixels.as_chunks::<4>().0)
        .filter(|(one, other)| {
            one.iter()
                .zip(other.iter())
                .any(|(one, other)| one.abs_diff(*other) >= 3)
        })
        .count()
}

/// How many different colours the frame has (up to `limit`).
fn colours_in(frame: &RenderedFrame, limit: usize) -> usize {
    let mut colours = BTreeSet::new();
    for pixel in frame.rgba_pixels.as_chunks::<4>().0 {
        colours.insert(*pixel);
        if colours.len() >= limit {
            break;
        }
    }
    colours.len()
}

fn the_scene() -> PictureData {
    PictureData::rgba8(SIZE.0, SIZE.1, a_scene_to_look_at(SIZE.0, SIZE.1)).unwrap()
}

#[test]
fn every_shader_of_the_program_draws_a_picture_with_colours_in_it_at_its_first_moment_and_at_another() {
    with_software_graphics!(graphics, {
        let scene = the_scene();
        for (number, shader) in SHADERS_OF_THE_PROGRAM.iter().enumerate() {
            let mut renderer = FrameRenderer::new(graphics.device.clone());
            let installed = ShaderOnARenderer::install(&mut renderer, number as u64, shader.file_name, shader.text)
                .unwrap_or_else(|why| panic!("{}: {why}", shader.file_name));
            let values = installed.values_with(&[]).unwrap();
            for time in [0.0, 2.5] {
                let frame = installed
                    .draw(&mut renderer, SIZE, time, &values, &scene)
                    .unwrap_or_else(|why| panic!("{} at {time}: {why}", shader.file_name));
                assert!(
                    colours_in(&frame, 12) >= 12,
                    "{} at {time} s draws a picture of fewer than twelve colours: it is as good as blank",
                    shader.file_name
                );
                assert!(
                    frame.rgba_pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255),
                    "{} at {time} s draws something that is not opaque",
                    shader.file_name
                );
            }
        }
    });
}

/// The changes to the values of a shader, besides what it begins as, that make each of its inputs do something: **a set of values for each way that the shader can be set up** (an input that
/// is only used while another is on does nothing until it is on). Every input must change the picture in at least one of them.
fn ways_to_set_up(file_name: &str) -> Vec<Vec<(&'static str, &'static str)>> {
    let mut ways = vec![Vec::new()];
    match file_name {
        "Mandelbrot Explorer.fs" => {
            // (The Julia set of a point, a point that circles, orbit traps of each kind, the two-colour palette, a view that stays where it is and does not glide, and a zoom that is
            // fast enough to reach its limit and turn round before the time of the picture.)
            ways.push(vec![("julia", "true"), ("juliaOrbit", "0.3"), ("zoomSpeed", "0")]);
            ways.push(vec![("colourMode", "1")]);
            ways.push(vec![("colourMode", "3")]);
            ways.push(vec![("palette", "7")]);
            ways.push(vec![("wholeSetFirst", "false"), ("zoom", "2")]);
            ways.push(vec![("zoomSpeed", "3")]);
        }
        "Julia Dance.fs" => {
            // (The inside in one colour, an orbit trap of the circle, and the two-colour palette.)
            ways.push(vec![("insideMode", "0")]);
            ways.push(vec![("colourMode", "1")]);
            ways.push(vec![("palette", "7")]);
        }
        "Newton Basins.fs" => {
            // (Walks that are too short for most points to arrive, so that the colour of the lost is seen; and the two-colour palette.)
            ways.push(vec![("iterations", "8")]);
            ways.push(vec![("palette", "7")]);
        }
        "Apollonian Gasket.fs" => {
            ways.push(vec![("palette", "7")]);
            ways.push(vec![("colourMode", "0")]);
        }
        // (The two-colour palette; and a shape that moves fast with the time, so that what the movement is for is seen in a picture of 48 pixels across.)
        "Mandelbulb.fs" => {
            ways.push(vec![("palette", "7")]);
            ways.push(vec![("wobbleSpeed", "0.4")]);
        }
        "Folded Sponge.fs" => {
            ways.push(vec![("palette", "7")]);
            ways.push(vec![("twistSpeed", "9.0")]);
        }
        "Mandelbox.fs" => {
            ways.push(vec![("palette", "7")]);
            ways.push(vec![("driftSpeed", "0.4")]);
        }
        // (A half turn of eight wedges is a whole number of wedges, which changes nothing: seven wedges have no such turn among the ends of the slider.)
        "Kaleidoscope Mandala.fs" => {
            ways.push(vec![("wedges", "7")]);
        }
        // (The two-colour palette: the two colours are only used while it is the palette.)
        "Acid Palette.fs"
        | "Neon Edges.fs"
        | "Plasma Storm.fs"
        | "Quasicrystal.fs"
        | "Liquid Marble.fs"
        | "Hyperbolic Tiling.fs" => {
            ways.push(vec![("palette", "7")]);
        }
        _ => {}
    }
    ways
}

/// The other values that input `input` of `kind` could be given, from what it is in `way`: the ends of its range and the middle for a number, every other option for a choice, the other
/// state for a switch, the opposite for a colour, and a move for a pair.
fn other_values_for(kind: crate::project::EffectKind, slot: usize, now: &PropertyValue) -> Vec<String> {
    let parameter = &kind.parameters()[slot];
    match now {
        PropertyValue::Number(number) => {
            let (low, high) = parameter.range.expect("a number of a shader has a range");
            [low, (low + high) / 2.0, high]
                .into_iter()
                .filter(|other| (other - number).abs() > (high - low) * 0.05)
                .map(|other| other.to_string())
                .collect()
        }
        PropertyValue::Flag(on) => vec![(!on).to_string()],
        PropertyValue::Words(words) => match &parameter.control {
            ParameterControl::Choice(options) => options
                .iter()
                .filter(|option| option.id != *words)
                .map(|option| option.id.to_string())
                .collect(),
            _ => Vec::new(),
        },
        PropertyValue::Colour(colour) => vec![format!(
            "#{:02x}{:02x}{:02x}",
            255 - colour.red,
            255 - colour.green,
            255 - colour.blue
        )],
        PropertyValue::Pair(across, down) => vec![format!("{}/{}", across + 0.2, down + 0.2)],
    }
}

#[test]
fn every_input_of_every_shader_of_the_program_changes_the_picture_in_one_way_of_setting_it_up_or_another() {
    with_software_graphics!(graphics, {
        let scene = the_scene();
        for (number, shader) in SHADERS_OF_THE_PROGRAM.iter().enumerate() {
            let mut renderer = FrameRenderer::new(graphics.device.clone());
            let installed = ShaderOnARenderer::install(&mut renderer, number as u64, shader.file_name, shader.text)
                .unwrap_or_else(|why| panic!("{}: {why}", shader.file_name));
            let kind = installed.kind();
            let mut moved: BTreeSet<usize> = BTreeSet::new();
            for way in ways_to_set_up(shader.file_name) {
                let begin = installed.values_with(&way).unwrap();
                let baseline = installed
                    .draw(&mut renderer, SIZE, 1.3, &begin, &scene)
                    .unwrap_or_else(|why| panic!("{} set up as {way:?}: {why}", shader.file_name));
                for (slot, begins_as) in begin.iter().enumerate() {
                    if moved.contains(&slot) {
                        continue;
                    }
                    for other in other_values_for(kind, slot, begins_as) {
                        let id = kind.parameters()[slot].id.to_string();
                        let mut changed = way.clone();
                        changed.push((id.as_str(), other.as_str()));
                        let values = installed.values_with(&changed).unwrap();
                        let frame = installed
                            .draw(&mut renderer, SIZE, 1.3, &values, &scene)
                            .unwrap_or_else(|why| panic!("{} with {changed:?}: {why}", shader.file_name));
                        if pixels_that_differ(&baseline, &frame) > 0 {
                            moved.insert(slot);
                            break;
                        }
                    }
                }
            }
            let dead: Vec<String> = (0..kind.value_count())
                .filter(|slot| !moved.contains(slot))
                .map(|slot| kind.parameters()[slot].id.to_string())
                .collect();
            assert!(
                dead.is_empty(),
                "{}: these inputs changed nothing in any way of setting the shader up that the test knows of: {dead:?}",
                shader.file_name
            );
        }
    });
}
