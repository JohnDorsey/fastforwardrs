// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What text objects are, as the program had them before phase 9 made them generators (`docs/engine-phase-9-design.md`, T9): **the gate of that phase, as a test that does not change with it.**
//!
//! The project files in `tests/data/text` were written by the program before phase 9 (one with a plain text, one whose words, font, size, colour, place and turning change, one with effects on it,
//! one whose keyframes are in racks that were taken out onto crates and merged, one that was cut in two, one in groups). Each is read, and the frames of it that are listed below are described as
//! the preview and the export describe them: by the very functions that do (`video_export::FrameDescriber`), with a text drawer that draws no letters (the fonts of a computer are not what a test
//! can count on) but a picture that is made of the text, the font, the size and the colour it is given and of the size of its area, so that any of them that came out otherwise would show. What
//! is written down for each frame, in `tests/data/text/*.expected.txt`, is **everything that a person or the engine could tell**: the text object that each text clip is at the frame (the
//! values of the properties at it, keyframes and all), what the text drawer was asked to draw, and each layer of the frame: where its corners are, what picture it has, and every effect
//! that is done to it with the numbers it is given. There are no ids in it (they come from hashes), only what is in the pictures.
//!
//! **After the migration to format 18 the same files must give the same text, byte for byte.** If one does not, either the migration or what the describer reads is not what it was, which is what
//! this is for; the expected files are not to be changed to make it pass. (They are written, once, by `WRITE_THE_TEXT_EXPECTATIONS=1 cargo test --test text_objects_as_they_are`.)

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use fastforwardrs::engine::{LayerDescription, LayerSource, PictureData, PictureId};
use fastforwardrs::lead_up::PreviewRunStreamStarter;
use fastforwardrs::project::{Project, Settings, TextObject, TimelineFrameIndex};
use fastforwardrs::project_file::project_from_json_text;
use fastforwardrs::rgba_picture::RgbaPicture;
use fastforwardrs::video_export::{FrameDescriber, PicturesOfTheRuns, plan_export};

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("text")
}

/// The frames that are looked at: the first ones, either side of every place where something is likely to change (a keyframe, a cut, the end of a clip), and the last.
const FRAMES: &[u64] = &[
    0, 1, 2, 5, 9, 10, 11, 19, 20, 21, 29, 30, 31, 39, 40, 41, 44, 45, 46, 49, 50, 51, 54, 55, 56, 59, 60, 61, 69, 70,
    71, 79, 80, 81, 88, 89, 90, 91, 99,
];

/// A number made of bytes, which says whether two lists of bytes are the same (FNV-1a).
fn digest_of(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The picture that stands in for the letters of `object` on a picture of this size: the area of the text, in the colour of the text, with the first pixel made of what the text says, the
/// font and the size, so that a change of any of them is a different picture.
fn picture_of_the_text(object: &TextObject, picture_width: u32, picture_height: u32) -> RgbaPicture {
    let (width, height) = fastforwardrs::text_rendering::area_size_in_pixels(object, picture_width, picture_height);
    let colour = object.colour;
    let mut pixels = [colour.red, colour.green, colour.blue, colour.alpha].repeat((width * height) as usize);
    let mut look = Vec::new();
    look.extend_from_slice(object.text.as_bytes());
    look.push(0);
    look.extend_from_slice(object.font_family.as_bytes());
    look.push(0);
    look.extend_from_slice(&object.font_size.fraction_of_picture_height().to_bits().to_le_bytes());
    pixels[..4].copy_from_slice(&digest_of(&look).to_le_bytes()[..4]);
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: pixels,
    }
}

/// Numbers as the lines of the expected files have them for what is made with sines and cosines (where a layer's corners are once it is turned, the matrix of a turn of the hues): to five
/// decimals, because the last bits of a sine are not the same on every computer, and what is made without them (the text objects, the keyframes) is written in full.
fn to_five_decimals(numbers: impl IntoIterator<Item = f32>) -> String {
    let numbers: Vec<String> = numbers.into_iter().map(|number| format!("{number:.5}")).collect();
    format!("[{}]", numbers.join(", "))
}

fn describe_the_layers(
    layers: &[LayerDescription],
    pictures: &HashMap<PictureId, String>,
    indent: usize,
    into: &mut String,
) {
    let margin = " ".repeat(indent);
    for (number, layer) in layers.iter().enumerate() {
        let source = match &layer.source {
            LayerSource::Picture(id) => format!("picture {}", pictures.get(id).map_or("(not sent)", String::as_str)),
            LayerSource::Solid(colour) => format!("solid {colour:?}"),
            LayerSource::Group(_) => "group".to_owned(),
        };
        writeln!(into, "{margin}layer {number}: {source}").unwrap();
        writeln!(
            into,
            "{margin}  corners {}",
            to_five_decimals(layer.corners.iter().flat_map(|(across, down)| [*across, *down]))
        )
        .unwrap();
        for effect in &layer.effects {
            writeln!(
                into,
                "{margin}  effect {} {} table {:?} state {:?} images {}",
                effect.key,
                to_five_decimals(effect.values.iter().copied()),
                effect.table,
                effect.state,
                effect.images.len()
            )
            .unwrap();
        }
        if let LayerSource::Group(group) = &layer.source {
            describe_the_layers(&group.layers, pictures, indent + 4, into);
        }
    }
}

/// What the project of the file `name` is, frame after frame, as the text that is compared with the expected one.
fn what_the_text_of(name: &str) -> String {
    let file = folder().join(format!("{name}.json"));
    let json = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    let project: Project = project_from_json_text(&json).unwrap_or_else(|error| panic!("{name}: {error}"));
    let plan = plan_export(&project, Settings::DEFAULT).expect("the project can be exported");
    let total = project.total_frame_count();
    let mut out = String::new();
    writeln!(
        out,
        "{name}: {total} frames, the picture is {:?}, {} track(s)",
        project.picture_size(),
        project.tracks().len()
    )
    .unwrap();

    let drawn: Rc<RefCell<Vec<String>>> = Rc::default();
    let drawer = {
        let drawn = Rc::clone(&drawn);
        move |object: &TextObject, width: u32, height: u32| {
            let picture = picture_of_the_text(object, width, height);
            drawn.borrow_mut().push(format!(
                "{object:?} on {width}x{height}, made {}x{} {:016x}",
                picture.width_in_pixels,
                picture.height_in_pixels,
                digest_of(&picture.rgba_pixels)
            ));
            picture
        }
    };
    let starter = PreviewRunStreamStarter::new(None);
    let pictures_of_the_runs = PicturesOfTheRuns::new(starter, &plan, &HashMap::new(), &HashMap::new())
        .expect("a project with no video has no run to start");
    let mut describer = FrameDescriber::new(&plan, pictures_of_the_runs, HashMap::new(), Box::new(drawer));

    let mut pictures: HashMap<PictureId, String> = HashMap::new();
    let never = AtomicBool::new(false);
    for &frame in FRAMES.iter().filter(|&&frame| frame < total) {
        writeln!(out, "frame {frame}").unwrap();
        // The text objects that are there, as the properties say they are at the frame.
        for track in project.tracks() {
            for placement in track.placements() {
                let first = placement.first_timeline_frame.value();
                if frame < first || frame >= first + placement.timeline_frame_count() {
                    continue;
                }
                if placement.clip.as_text().is_some() {
                    let object = project
                        .text_object_at(placement.id, TimelineFrameIndex::new(frame))
                        .expect("a text clip has a text object at every frame it is shown at");
                    writeln!(out, "  text clip of track {:?} at {first}: {object:?}", track.id()).unwrap();
                }
            }
        }
        drawn.borrow_mut().clear();
        let mut sent: Vec<(PictureId, PictureData)> = Vec::new();
        let described = describer
            .describe(frame, &never, &mut |id, data| sent.push((id, data)))
            .expect("the frame can be described");
        for (id, data) in sent {
            pictures.insert(
                id,
                format!(
                    "{}x{} {:016x}",
                    data.width_in_pixels,
                    data.height_in_pixels,
                    digest_of(format!("{:?}", data.pixels).as_bytes())
                ),
            );
        }
        for call in drawn.borrow().iter() {
            writeln!(out, "  the drawer was asked for {call}").unwrap();
        }
        describe_the_layers(&described.description.layers, &pictures, 2, &mut out);
    }
    out
}

fn same_lines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn check(name: &str) {
    let found = what_the_text_of(name);
    let expected_file = folder().join(format!("{name}.expected.txt"));
    if std::env::var_os("WRITE_THE_TEXT_EXPECTATIONS").is_some() {
        std::fs::write(&expected_file, &found).unwrap();
        return;
    }
    let expected =
        std::fs::read_to_string(&expected_file).unwrap_or_else(|error| panic!("{}: {error}", expected_file.display()));
    let (expected, found) = (same_lines(&expected), same_lines(&found));
    if expected != found {
        let line = expected
            .lines()
            .zip(found.lines())
            .position(|(before, now)| before != now)
            .unwrap_or_else(|| expected.lines().count().min(found.lines().count()));
        panic!(
            "{name} is not what it was before phase 9; the first line that differs is line {}:\n  was: {}\n  now: {}",
            line + 1,
            expected.lines().nth(line).unwrap_or("(nothing)"),
            found.lines().nth(line).unwrap_or("(nothing)")
        );
    }
}

#[test]
fn a_text_that_is_not_animated_is_what_it_was() {
    check("plain");
}

#[test]
fn a_text_whose_words_font_size_colour_place_and_turning_change_is_what_it_was() {
    check("animated");
}

#[test]
fn a_text_with_effects_on_it_and_on_its_track_is_what_it_was() {
    check("with_effects");
}

#[test]
fn a_text_whose_keyframes_are_in_racks_that_are_in_crates_or_merged_is_what_it_was() {
    check("racks_and_crates");
}

#[test]
fn a_text_that_was_cut_in_two_is_what_it_was() {
    check("cut_in_two");
}

#[test]
fn texts_in_groups_are_what_they_were() {
    check("in_groups");
}
