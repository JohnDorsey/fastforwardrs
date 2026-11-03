// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawing shaders to pictures: the values that are given as text, the time, a filter's picture, the frames that a shader remembers, the files that are written and what is counted of the
//! ones that cannot be drawn. Drawn on the software adapter, like every test that draws.

use super::*;
use crate::engine::PicturePixels;
use crate::engine::testing::with_software_graphics;

/// A generator whose colour says its inputs: red is `level`, green is a tenth of the time, blue is the blue of `tint` where `on` is on.
const SAYS_ITS_INPUTS: &str = "/*{ \"INPUTS\": [
    { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.25 },
    { \"NAME\": \"on\", \"TYPE\": \"bool\", \"DEFAULT\": false },
    { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [0.0, 0.0, 1.0, 1.0] },
    { \"NAME\": \"spot\", \"TYPE\": \"point2D\", \"DEFAULT\": [0.0, 0.0] }
] }*/
void main() { gl_FragColor = vec4(level, TIME / 10.0, on ? tint.b : 0.0, 1.0); }
";

const PASSES_THE_PICTURE_ON: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }
";

/// A filter that adds a sixteenth to the red that it kept from the frame before, so that the red of its picture counts the frames that were made.
const COUNTS_THE_FRAMES: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true, \"FLOAT\": true }, {} ] }*/
void main() {
    if (PASSINDEX == 0) {
        vec4 held = IMG_THIS_PIXEL(memory);
        gl_FragColor = vec4(held.r + 0.0625, 0.0, 0.0, 1.0);
    } else {
        gl_FragColor = IMG_THIS_PIXEL(memory);
    }
}
";

fn a_bad_shader() -> &'static str {
    include_str!("../../isf/fixtures/bad_syntax.fs")
}

fn near(got: [u8; 4], wanted: [u8; 4]) -> bool {
    got.iter().zip(wanted).all(|(got, wanted)| got.abs_diff(wanted) <= 1)
}

fn the_scene(size: (u32, u32)) -> PictureData {
    PictureData::rgba8(size.0, size.1, a_scene_to_look_at(size.0, size.1)).unwrap()
}

#[test]
fn a_value_is_read_as_the_kind_of_value_that_the_input_begins_as() {
    let number = PropertyValue::Number(0.0);
    assert_eq!(value_of_the_text(&number, " 1.5 "), Ok(PropertyValue::Number(1.5)));
    assert!(value_of_the_text(&number, "one").is_err());
    assert_eq!(
        value_of_the_text(&PropertyValue::Flag(false), "true"),
        Ok(PropertyValue::Flag(true))
    );
    assert!(value_of_the_text(&PropertyValue::Flag(false), "yes").is_err());
    assert_eq!(
        value_of_the_text(&PropertyValue::Words("0".to_owned()), " 3 "),
        Ok(PropertyValue::Words("3".to_owned())),
        "the option of a choice is its number, as the value is kept"
    );
    assert_eq!(
        value_of_the_text(&PropertyValue::Pair(0.0, 0.0), "0.25/ 0.75"),
        Ok(PropertyValue::Pair(0.25, 0.75))
    );
    assert!(value_of_the_text(&PropertyValue::Pair(0.0, 0.0), "0.25").is_err());
    let colour = PropertyValue::Colour(TextColour::new(0, 0, 0, 255));
    assert_eq!(
        value_of_the_text(&colour, "#ff8000"),
        Ok(PropertyValue::Colour(TextColour::new(255, 128, 0, 255))),
        "no opacity is all of it"
    );
    assert_eq!(
        value_of_the_text(&colour, "#ff800040"),
        Ok(PropertyValue::Colour(TextColour::new(255, 128, 0, 64)))
    );
    assert!(value_of_the_text(&colour, "#ff80").is_err());
    assert!(value_of_the_text(&colour, "#gg0000").is_err());
}

#[test]
fn a_generator_is_drawn_at_its_time_with_the_values_it_begins_as_and_with_the_values_that_are_given() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let installed = ShaderOnARenderer::install(&mut renderer, 1, "Says Its Inputs.fs", SAYS_ITS_INPUTS).unwrap();
        let picture = the_scene((16, 10));

        let as_it_begins = installed.values_with(&[]).unwrap();
        let frame = installed
            .draw(&mut renderer, (16, 10), 2.5, &as_it_begins, &picture)
            .unwrap();
        assert!(
            near(frame.pixel(7, 4), [64, 64, 0, 255]),
            "a quarter, a quarter of ten seconds, and no blue while it is off: {:?}",
            frame.pixel(7, 4)
        );
        assert!(
            frame
                .rgba_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| near(*pixel, frame.pixel(0, 0))),
            "every pixel is the same"
        );

        let changed = installed
            .values_with(&[("level", "0.5"), ("on", "true"), ("tint", "#0000ff")])
            .unwrap();
        let frame = installed
            .draw(&mut renderer, (16, 10), 5.0, &changed, &picture)
            .unwrap();
        assert!(near(frame.pixel(7, 4), [128, 128, 255, 255]), "{:?}", frame.pixel(7, 4));
    });
}

#[test]
fn a_value_that_names_no_input_or_cannot_be_read_is_refused_in_words() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let installed = ShaderOnARenderer::install(&mut renderer, 1, "Says Its Inputs.fs", SAYS_ITS_INPUTS).unwrap();
        let no_such = installed.values_with(&[("brightness", "1")]).unwrap_err();
        assert!(
            no_such.contains("brightness") && no_such.contains("no input"),
            "{no_such}"
        );
        let unreadable = installed.values_with(&[("level", "lots")]).unwrap_err();
        assert!(
            unreadable.contains("level") && unreadable.contains("lots"),
            "{unreadable}"
        );
    });
}

#[test]
fn a_filter_is_drawn_on_the_picture_it_is_given() {
    with_software_graphics!(graphics, {
        let size = (96, 54);
        let scene = the_scene(size);
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let frame = picture_of_a_shader(
            &mut renderer,
            1,
            &ShaderToDraw {
                file_name: "Passes The Picture On.fs",
                text: PASSES_THE_PICTURE_ON,
                size,
                time: 0.0,
                values: &[],
                picture: &scene,
            },
        )
        .unwrap();
        let PicturePixels::Rgba8 { straight_alpha } = &scene.pixels else {
            panic!("the scene is 8 bits of each colour");
        };
        let wrong = frame
            .rgba_pixels
            .iter()
            .zip(straight_alpha)
            .filter(|(got, wanted)| got.abs_diff(**wanted) > 2)
            .count();
        assert_eq!(
            wrong, 0,
            "every number of every pixel is the scene's, to within the rounding"
        );
    });
}

#[test]
fn a_shader_that_remembers_the_frames_before_it_is_drawn_from_its_first_frame_to_the_one_at_the_time() {
    with_software_graphics!(graphics, {
        let size = (8, 8);
        let scene = the_scene(size);
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let installed =
            ShaderOnARenderer::install(&mut renderer, 1, "Counts The Frames.fs", COUNTS_THE_FRAMES).unwrap();
        let values = installed.values_with(&[]).unwrap();
        // (At thirty frames a second a tenth of a second is the frame numbered 3: four frames were made, each adding a sixteenth.)
        let at_a_tenth = installed.draw(&mut renderer, size, 0.1, &values, &scene).unwrap();
        assert!(
            near(at_a_tenth.pixel(3, 3), [64, 0, 0, 255]),
            "{:?}",
            at_a_tenth.pixel(3, 3)
        );
        let at_the_start = installed.draw(&mut renderer, size, 0.0, &values, &scene).unwrap();
        assert!(
            near(at_the_start.pixel(3, 3), [16, 0, 0, 255]),
            "{:?}",
            at_the_start.pixel(3, 3)
        );
    });
}

#[test]
fn a_sheet_has_the_frames_in_rows_and_the_files_that_are_written_can_be_read_back() {
    let folder = tempfile::tempdir().unwrap();
    let flat = |colour: [u8; 4]| RenderedFrame {
        width_in_pixels: 4,
        height_in_pixels: 2,
        rgba_pixels: colour.repeat(8),
    };
    let frames = [
        flat([255, 0, 0, 255]),
        flat([0, 255, 0, 255]),
        flat([0, 0, 255, 255]),
        flat([255, 255, 0, 255]),
        flat([0, 255, 255, 255]),
    ];
    let path = folder.path().join("sheet.png");
    write_a_sheet(&frames, 2, &path).unwrap();
    let sheet = image::open(&path).unwrap().to_rgba8();
    assert_eq!(
        sheet.dimensions(),
        (8, 6),
        "two across and three rows, the last row not full"
    );
    assert_eq!(sheet.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(sheet.get_pixel(4, 1).0, [0, 255, 0, 255]);
    assert_eq!(sheet.get_pixel(3, 2).0, [0, 0, 255, 255]);
    assert_eq!(sheet.get_pixel(7, 3).0, [255, 255, 0, 255]);
    assert_eq!(sheet.get_pixel(1, 5).0, [0, 255, 255, 255]);
    assert_eq!(sheet.get_pixel(5, 5).0, [0, 0, 0, 0], "what no frame is in is nothing");

    let single = folder.path().join("one.png");
    write_as_png(&frames[3], &single).unwrap();
    assert_eq!(
        image::open(&single).unwrap().to_rgba8().into_raw(),
        frames[3].rgba_pixels
    );
    assert!(write_a_sheet(&[], 3, &folder.path().join("nothing.png")).is_ok());
    assert!(!folder.path().join("nothing.png").exists(), "no frames, no file");
}

#[test]
fn a_folder_of_shaders_is_its_shader_files_by_name_and_a_file_is_itself() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("b.fs"), "b").unwrap();
    std::fs::write(folder.path().join("a.ISF"), "a").unwrap();
    std::fs::write(folder.path().join("notes.txt"), "not a shader").unwrap();
    let alone = folder.path().join("notes.txt");
    let found = shader_files_named(&[folder.path().to_owned(), alone.clone()]).unwrap();
    assert_eq!(
        found,
        [folder.path().join("a.ISF"), folder.path().join("b.fs"), alone],
        "a folder gives the shader files of it, by name; a file is whatever it is"
    );
    assert!(
        shader_files_named(&[folder.path().join("missing")]).is_ok(),
        "a path that is no folder is left for whoever reads it to refuse"
    );
}

#[test]
fn the_pictures_asked_for_are_written_and_what_cannot_be_drawn_is_counted_and_told() {
    with_software_graphics!(graphics, {
        let shaders = tempfile::tempdir().unwrap();
        std::fs::write(shaders.path().join("Says Its Inputs.fs"), SAYS_ITS_INPUTS).unwrap();
        std::fs::write(shaders.path().join("Broken.fs"), a_bad_shader()).unwrap();
        let out = tempfile::tempdir().unwrap();
        let asked = PicturesAsked {
            shaders: vec![shaders.path().to_owned()],
            folder: out.path().join("pictures"),
            size: (16, 9),
            times: vec![0.0, 2.5],
            sets_of_values: vec![vec![], vec![("level".to_owned(), "1".to_owned())]],
            picture: None,
            write_the_wgsl: true,
            more_times_to_measure: 2,
            sheet_columns: None,
        };
        let mut said = Vec::new();
        let failed =
            write_the_pictures_asked_for(&graphics.device, &asked, &mut |line| said.push(line.to_owned())).unwrap();

        assert_eq!(
            failed, 4,
            "the broken shader could not be drawn at either time with either set of values"
        );
        assert!(
            said.iter()
                .any(|line| line.starts_with("FAIL Broken.fs") && line.contains("compile")),
            "{said:?}"
        );
        for set in 0..2 {
            for time in ["0", "2.5"] {
                let name = format!("Says_Its_Inputs_v{set}_t{time}.png");
                assert!(
                    out.path().join("pictures").join(&name).is_file(),
                    "{name} was written: {said:?}"
                );
            }
        }
        let line = said
            .iter()
            .find(|line| line.starts_with("ok") && line.contains("_v1_t2.5"))
            .unwrap();
        assert!(line.contains("a frame (the mean of 2)"), "{line}");
        let first = image::open(out.path().join("pictures").join("Says_Its_Inputs_v1_t2.5.png"))
            .unwrap()
            .to_rgba8();
        assert!(
            near(first.get_pixel(3, 3).0, [255, 64, 0, 255]),
            "{:?}",
            first.get_pixel(3, 3)
        );
        let sheet = image::open(out.path().join("pictures").join("sheet.png"))
            .unwrap()
            .to_rgba8();
        assert_eq!(
            sheet.dimensions(),
            (32, 18),
            "four pictures, the times side by side and the sets of values one under the other"
        );
        assert!(out.path().join("pictures").join("Says_Its_Inputs.wgsl").is_file());
        assert!(
            out.path()
                .join("pictures")
                .join("Says_Its_Inputs_guarded.wgsl")
                .is_file()
        );
    });
}

#[test]
fn a_picture_file_is_what_a_filter_works_on() {
    with_software_graphics!(graphics, {
        let shaders = tempfile::tempdir().unwrap();
        std::fs::write(shaders.path().join("Passes The Picture On.fs"), PASSES_THE_PICTURE_ON).unwrap();
        let out = tempfile::tempdir().unwrap();
        let red = out.path().join("red.png");
        image::save_buffer(
            &red,
            &[200, 10, 20, 255].repeat(9),
            3,
            3,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        let asked = PicturesAsked {
            shaders: vec![shaders.path().to_owned()],
            folder: out.path().join("pictures"),
            size: (12, 12),
            times: vec![0.0],
            sets_of_values: Vec::new(),
            picture: Some(red),
            write_the_wgsl: false,
            more_times_to_measure: 0,
            sheet_columns: None,
        };
        let failed = write_the_pictures_asked_for(&graphics.device, &asked, &mut |_| {}).unwrap();
        assert_eq!(failed, 0);
        let drawn = image::open(out.path().join("pictures").join("Passes_The_Picture_On_v0_t0.png"))
            .unwrap()
            .to_rgba8();
        assert!(
            near(drawn.get_pixel(6, 6).0, [200, 10, 20, 255]),
            "{:?}",
            drawn.get_pixel(6, 6)
        );
        assert!(!out.path().join("pictures").join("Passes_The_Picture_On.wgsl").exists());
    });
}
