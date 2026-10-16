// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The visible differences of phase 3 (linear light and colour management), as pictures and numbers: the same scene made twice by the picture engine on a software
//! adapter, **before** (the engine in its compatibility mode, which is what the preview and the export did until phase 3, and which the tests of the engine against the
//! old drawing hold to those numbers) and **after** (put together in linear light, with the colour of a video read as phase 3 reads it). Three panels of each scene, made
//! three times as big as they are so that a pixel can be seen: before, after, and what is different between them (the biggest difference of any number of a pixel, made
//! eight times as strong, in grey).
//!
//! It is written for the review that decision CM-20 asks for (`docs/engine-phase-3-design.md`, sections 10 and 12), and marked to be ignored: it writes files into
//! `docs/engine-comparison-phase-3` and prints the table of numbers that the report quotes.
//!
//! ```text
//! cargo test --test linear_light_comparison -- --ignored --nocapture
//! ```
//!
//! Everything here runs on a **software adapter** (SF-01): nothing is made on a graphics card.

use std::path::PathBuf;
use std::time::Duration;

use fastforwardrs::colour_space::{ChromaSiting, StandardPrimaries};
use fastforwardrs::engine::{
    AdapterChoice, EngineDevice, FrameColour, FrameDescription, FrameRenderer, LayerDescription, LayerSource,
    OutputEncoding, PictureColour, PictureData, PictureId, WorkingSpace, YuvColour, YuvMatrix, YuvRange,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);
const MADE_BIGGER: u32 = 3;
const DIFFERENCE_MADE_BIGGER: u8 = 8;
const SPACE_BETWEEN_THE_PANELS: u32 = 6;

const LINEAR: WorkingSpace = WorkingSpace::Linear(StandardPrimaries::Rec709);

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("engine-comparison-phase-3")
}

/// A frame of the engine and how it is made: in the compatibility mode (before) or in linear light (after).
#[derive(Clone, Copy, Debug)]
enum Mode {
    Before,
    After,
}

impl Mode {
    fn colour(self) -> FrameColour {
        match self {
            Self::Before => FrameColour::default(),
            Self::After => FrameColour {
                working: LINEAR,
                output: OutputEncoding::SRGB,
            },
        }
    }

    fn working(self) -> WorkingSpace {
        self.colour().working
    }
}

type MakesAPicture = Box<dyn Fn(Mode) -> PictureData>;
type MakesLayers = Box<dyn Fn(Mode) -> Vec<LayerDescription>>;

/// A scene: what is put on the frame (`pictures` first, and then the `layers` that refer to them by the number they are given), made for either mode.
struct Scene {
    name: &'static str,
    what: &'static str,
    size: (u32, u32),
    /// The pictures, by id; each is made for the mode it is asked for.
    pictures: Vec<(u64, MakesAPicture)>,
    /// The layers, from the back; each is made for the mode it is asked for.
    layers: MakesLayers,
    /// Whether both of the two are put together in the compatibility mode (for the scene that is about the matrix of a video alone).
    both_in_the_compatibility_mode: bool,
}

fn whole(picture: u64) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        effects: Vec::new(),
    }
}

fn solid(rgba: [u8; 4]) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Solid(rgba),
        corners: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        effects: Vec::new(),
    }
}

/// A picture put on the frame turned by `degrees` about the middle of the frame, `scale` times as big as it is in pixels, in a frame of `frame` pixels.
fn turned_and_scaled(
    picture: u64,
    picture_size: (u32, u32),
    frame: (u32, u32),
    degrees: f32,
    scale: f32,
) -> LayerDescription {
    let (width, height) = (picture_size.0 as f32 * scale, picture_size.1 as f32 * scale);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let corner = |across: f32, down: f32| -> (f32, f32) {
        let (x, y) = (across * width / 2.0, down * height / 2.0);
        (
            (frame.0 as f32 / 2.0 + x * cos - y * sin) / frame.0 as f32,
            (frame.1 as f32 / 2.0 + x * sin + y * cos) / frame.1 as f32,
        )
    };
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: [
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ],
        effects: Vec::new(),
    }
}

fn rgba_picture(width: u32, height: u32, pixels: Vec<u8>, mode: Mode) -> PictureData {
    PictureData::rgba8(width, height, pixels)
        .unwrap()
        .in_colour(PictureColour::SRGB, mode.working())
}

/// Black with white lines one pixel wide every `every` pixels, in both directions.
fn white_lines_on_black(width: u32, height: u32, every: u32) -> Vec<u8> {
    let mut pixels = Vec::new();
    for down in 0..height {
        for across in 0..width {
            let on = across % every == 0 || down % every == 0;
            let value = if on { 255 } else { 0 };
            pixels.extend([value, value, value, 255]);
        }
    }
    pixels
}

/// A checkerboard of squares of `size` pixels, black and white.
fn checkerboard(width: u32, height: u32, size: u32) -> Vec<u8> {
    let mut pixels = Vec::new();
    for down in 0..height {
        for across in 0..width {
            let value = if ((across / size) + (down / size)).is_multiple_of(2) {
                255
            } else {
                0
            };
            pixels.extend([value, value, value, 255]);
        }
    }
    pixels
}

/// A soft-edged spot of `colour`, as an antialiased letter's edge is: the colour is the same all over, and how opaque it is goes from 0 to 255 over a few pixels.
fn soft_spot(width: u32, height: u32, colour: [u8; 3]) -> Vec<u8> {
    let (middle_across, middle_down) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = width.min(height) as f32 * 0.38;
    let mut pixels = Vec::new();
    for down in 0..height {
        for across in 0..width {
            let distance =
                ((across as f32 + 0.5 - middle_across).powi(2) + (down as f32 + 0.5 - middle_down).powi(2)).sqrt();
            let coverage = ((radius - distance) / 3.0 + 0.5).clamp(0.0, 1.0);
            pixels.extend([colour[0], colour[1], colour[2], (coverage * 255.0).round() as u8]);
        }
    }
    pixels
}

/// A gradient across and down, as a picture file would be.
fn smooth_gradient(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::new();
    for down in 0..height {
        for across in 0..width {
            pixels.extend([
                (across * 255 / (width - 1)) as u8,
                (down * 255 / (height - 1)) as u8,
                ((across + down) * 255 / (width + height - 2)) as u8,
                255,
            ]);
        }
    }
    pixels
}

/// Planes of four flat colours, one in each quarter (strong colour differences, with hard edges between the quarters).
fn four_colours_in_planes(width: u32, height: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let y = vec![150_u8; (width * height) as usize];
    let mut u = Vec::new();
    let mut v = Vec::new();
    for down in 0..chroma_height {
        for across in 0..chroma_width {
            let (left, top) = (across < chroma_width / 2, down < chroma_height / 2);
            let (blue, red) = match (left, top) {
                (true, true) => (90, 180),
                (false, true) => (128, 128),
                (true, false) => (180, 90),
                (false, false) => (110, 150),
            };
            u.push(blue);
            v.push(red);
        }
    }
    (y, u, v)
}

fn video_in_planes(width: u32, height: u32, colour: YuvColour, mode: Mode, siting: ChromaSiting) -> PictureData {
    let (y, u, v) = four_colours_in_planes(width, height);
    PictureData::yuv420p8(width, height, y, u, v, colour)
        .unwrap()
        .in_colour(PictureColour::SRGB, mode.working())
        .with_chroma_siting(siting)
}

const BT601: YuvColour = YuvColour {
    matrix: YuvMatrix::Bt601,
    range: YuvRange::Limited,
};
const BT709: YuvColour = YuvColour {
    matrix: YuvMatrix::Bt709,
    range: YuvRange::Limited,
};

/// A scene whose pictures and layers are made for either mode (and which is put together in the mode, unless it is about something that the mode must not change).
fn scene(
    name: &'static str,
    what: &'static str,
    size: (u32, u32),
    pictures: Vec<(u64, MakesAPicture)>,
    layers: MakesLayers,
) -> Scene {
    Scene {
        name,
        what,
        size,
        pictures,
        layers,
        both_in_the_compatibility_mode: false,
    }
}

fn scenes() -> Vec<Scene> {
    let picture_of = |id: u64, make: MakesAPicture| (id, make);
    let band = |from: f32, to: f32, rgba: [u8; 4]| LayerDescription {
        source: LayerSource::Solid(rgba),
        corners: [(from, 0.0), (to, 0.0), (to, 1.0), (from, 1.0)],
        effects: Vec::new(),
    };
    vec![
        scene(
            "01-thin-lines-turned-and-made-smaller",
            "A grid of white lines one pixel wide on black, turned 20 degrees and made 0.6 times as big.",
            (160, 90),
            vec![picture_of(
                1,
                Box::new(|mode| rgba_picture(96, 96, white_lines_on_black(96, 96, 6), mode)),
            )],
            Box::new(|_| vec![turned_and_scaled(1, (96, 96), (160, 90), 20.0, 0.6)]),
        ),
        scene(
            "02-a-checkerboard-made-smaller",
            "A black and white checkerboard of squares of two pixels, made 0.7 times as big as it is.",
            (134, 76),
            vec![picture_of(
                1,
                Box::new(|mode| rgba_picture(192, 108, checkerboard(192, 108, 2), mode)),
            )],
            Box::new(|_| vec![whole(1)]),
        ),
        scene(
            "03-see-through-colours",
            "Half-clear white over black, half-clear red over blue, and half-clear grey over white: the three bands of the picture.",
            (150, 60),
            Vec::new(),
            Box::new(move |_| {
                vec![
                    solid([0, 0, 0, 255]),
                    band(0.0, 1.0 / 3.0, [255, 255, 255, 128]),
                    band(1.0 / 3.0, 2.0 / 3.0, [0, 0, 255, 255]),
                    band(1.0 / 3.0, 2.0 / 3.0, [255, 0, 0, 128]),
                    band(2.0 / 3.0, 1.0, [255, 255, 255, 255]),
                    band(2.0 / 3.0, 1.0, [128, 128, 128, 128]),
                ]
            }),
        ),
        scene(
            "04-light-on-dark-the-soft-edge-of-a-letter",
            "A white spot with a soft edge, as the edge of a letter is, over a dark picture.",
            (96, 96),
            vec![picture_of(
                1,
                Box::new(|mode| rgba_picture(96, 96, soft_spot(96, 96, [255, 255, 255]), mode)),
            )],
            Box::new(|_| vec![solid([25, 25, 35, 255]), whole(1)]),
        ),
        scene(
            "05-dark-on-light-the-soft-edge-of-a-letter",
            "A black spot with a soft edge over a light picture.",
            (96, 96),
            vec![picture_of(
                1,
                Box::new(|mode| rgba_picture(96, 96, soft_spot(96, 96, [0, 0, 0]), mode)),
            )],
            Box::new(|_| vec![solid([230, 230, 220, 255]), whole(1)]),
        ),
        Scene {
            both_in_the_compatibility_mode: true,
            ..scene(
                "06-a-high-definition-video-that-says-nothing-bt601-then-bt709",
                "The planes of a video of four flat colours, read as ffmpeg guessed (BT.601, before) and as phase 3 reads a picture of more than 1024 across (BT.709, after); both put together the old way and with the colour differences copied, to show the matrix alone.",
                (160, 90),
                vec![picture_of(
                    1,
                    Box::new(|mode| {
                        let colour = match mode {
                            Mode::Before => BT601,
                            Mode::After => BT709,
                        };
                        let (y, u, v) = four_colours_in_planes(160, 90);
                        PictureData::yuv420p8(160, 90, y, u, v, colour).unwrap()
                    }),
                )],
                Box::new(|_| vec![whole(1)]),
            )
        },
        scene(
            "07-the-edge-between-two-colours-of-a-video",
            "The planes of a video of four flat colours: each colour difference copied to its four pixels (before), and blended from the squares round each pixel (after); the matrix is the same.",
            (160, 90),
            vec![picture_of(
                1,
                Box::new(|mode| video_in_planes(160, 90, BT709, mode, ChromaSiting::Left)),
            )],
            Box::new(|_| vec![whole(1)]),
        ),
        scene(
            "08-a-gradient-shown-as-it-is",
            "A smooth gradient shown at its own size: nothing mixes, so nothing changes (decision C12).",
            (128, 72),
            vec![picture_of(
                1,
                Box::new(|mode| rgba_picture(128, 72, smooth_gradient(128, 72), mode)),
            )],
            Box::new(|_| vec![whole(1)]),
        ),
        scene(
            "09-a-gradient-made-half-the-size",
            "The same gradient made half as big: the filter mixes neighbours that are nearly alike, so the change is small.",
            (64, 36),
            vec![picture_of(
                1,
                Box::new(|mode| rgba_picture(128, 72, smooth_gradient(128, 72), mode)),
            )],
            Box::new(|_| vec![whole(1)]),
        ),
    ]
}

/// Renders `scene` in `mode`.
fn render(renderer: &mut FrameRenderer, scene: &Scene, mode: Mode) -> Vec<u8> {
    for (id, make) in &scene.pictures {
        renderer.upload_picture(PictureId(*id), &make(mode)).unwrap();
    }
    let mut description = FrameDescription::black(scene.size);
    description.colour = if scene.both_in_the_compatibility_mode {
        FrameColour::default()
    } else {
        mode.colour()
    };
    description.layers = (scene.layers)(mode);
    renderer.render(&description, A_LONG_TIME).unwrap().0.rgba_pixels
}

#[test]
#[ignore = "writes the pictures into docs/engine-comparison-phase-3, and needs a software adapter"]
fn write_the_pictures_of_the_visible_differences_of_phase_3() {
    let device = match EngineDevice::create(&AdapterChoice::SoftwareOnly) {
        Ok(device) => device,
        Err(error) => {
            eprintln!("SKIPPED: no software adapter ({error:?})");
            return;
        }
    };
    let mut renderer = FrameRenderer::new(device);
    std::fs::create_dir_all(folder()).unwrap();
    println!("| scene | largest difference | mean difference | values changed |");
    println!("|---|---|---|---|");
    for scene in scenes() {
        let before = render(&mut renderer, &scene, Mode::Before);
        let after = render(&mut renderer, &scene, Mode::After);
        let (width, height) = scene.size;
        let panel_width = width * MADE_BIGGER;
        let mut panels = image::RgbaImage::from_pixel(
            3 * panel_width + 2 * SPACE_BETWEEN_THE_PANELS,
            height * MADE_BIGGER,
            image::Rgba([90, 90, 90, 255]),
        );
        let (mut largest, mut sum, mut changed, mut values) = (0u8, 0u64, 0usize, 0usize);
        for down in 0..height {
            for across in 0..width {
                let place = ((down * width + across) * 4) as usize;
                let (old, new) = (&before[place..place + 4], &after[place..place + 4]);
                let mut pixel_difference = 0u8;
                for channel in 0..3 {
                    let difference = old[channel].abs_diff(new[channel]);
                    largest = largest.max(difference);
                    pixel_difference = pixel_difference.max(difference);
                    sum += u64::from(difference);
                    changed += usize::from(difference > 0);
                    values += 1;
                }
                let grey = pixel_difference.saturating_mul(DIFFERENCE_MADE_BIGGER);
                for (panel, colour) in [
                    (0, image::Rgba([old[0], old[1], old[2], 255])),
                    (1, image::Rgba([new[0], new[1], new[2], 255])),
                    (2, image::Rgba([grey, grey, grey, 255])),
                ] {
                    for step_down in 0..MADE_BIGGER {
                        for step_across in 0..MADE_BIGGER {
                            panels.put_pixel(
                                panel * (panel_width + SPACE_BETWEEN_THE_PANELS) + across * MADE_BIGGER + step_across,
                                down * MADE_BIGGER + step_down,
                                colour,
                            );
                        }
                    }
                }
            }
        }
        panels.save(folder().join(format!("{}.png", scene.name))).unwrap();
        println!(
            "| {} | {largest} | {:.3} | {:.1} % |",
            scene.name,
            sum as f64 / values as f64,
            100.0 * changed as f64 / values as f64
        );
        eprintln!("{}: {}", scene.name, scene.what);
    }
}
