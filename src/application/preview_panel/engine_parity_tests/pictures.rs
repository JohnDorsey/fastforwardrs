// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures that show the maintainer what the engine's preview looks like beside the old drawing's: for each scene a picture of three panels, **the old
//! drawing, the engine, and what is different between them** (the biggest difference of any of the numbers of a pixel, made sixteen times as big, in grey: black is
//! the same, and a pixel that is one level off is a sixteenth of the way to white).
//!
//! They are written by a test that is marked to be ignored (it writes files into `docs/engine-comparison`):
//!
//! ```text
//! cargo test --lib write_the_pictures_that_show_what_differs -- --ignored
//! ```
//!
//! The same scenes are also checked in the ordinary tests above, which is what says that the two drawings agree; this is for looking at.

use std::path::PathBuf;

use super::*;

/// How many times bigger than they are the differences are made in the third panel.
const HOW_MUCH_THE_DIFFERENCES_ARE_MADE_BIGGER: u8 = 16;

/// The space between the panels, in pixels.
const SPACE_BETWEEN_THE_PANELS: u32 = 6;

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("engine-comparison")
}

/// A picture of RGBA bytes made from `colour_at`, which is given the place in the picture as fractions of its width and height.
fn a_picture_from(width: u32, height: u32, colour_at: impl Fn(f32, f32) -> [u8; 4]) -> RgbaPicture {
    let mut rgba_pixels = Vec::with_capacity((width * height * 4) as usize);
    for down in 0..height {
        for across in 0..width {
            rgba_pixels.extend(colour_at(
                (across as f32 + 0.5) / width as f32,
                (down as f32 + 0.5) / height as f32,
            ));
        }
    }
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels,
    }
}

fn a_video_picture_from(
    width: u32,
    height: u32,
    serial_number: u64,
    colour_at: impl Fn(f32, f32) -> [u8; 3],
) -> SuppliedPicture {
    let picture = a_picture_from(width, height, |across, down| {
        let [red, green, blue] = colour_at(across, down);
        [red, green, blue, 255]
    });
    SuppliedPicture {
        source: SourceMediaIdentifier::new(1),
        source_frame: SourceFrameIndex::new(0),
        frame: SuppliedFrame::Rgba(DecodedFrame {
            width_in_pixels: width,
            height_in_pixels: height,
            rgba_pixels: picture.rgba_pixels,
        }),
        serial_number,
    }
}

fn a_scene_of(
    video_pictures: Vec<SuppliedPicture>,
    images: Vec<RgbaPicture>,
    recipes: Vec<LayerRecipe>,
    symbol: Option<SymbolOnThePreview>,
) -> Scene {
    Scene {
        video_pictures,
        images,
        libraries: Vec::new(),
        recipes,
        symbol,
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
    }
}

fn recipe(kind: Kind, placement: Placement, colour: ColourMatrix) -> LayerRecipe {
    LayerRecipe {
        kind,
        placement,
        colour,
    }
}

/// A sky of colours that change smoothly, as a picture of a video does.
fn the_sky(across: f32, down: f32) -> [u8; 3] {
    [
        (60.0 + 150.0 * across) as u8,
        (90.0 + 110.0 * (1.0 - down)) as u8,
        (200.0 - 120.0 * across * down) as u8,
    ]
}

/// The scenes: what each is for is in its name.
fn the_scenes() -> Vec<(&'static str, Scene)> {
    let mut scenes = Vec::new();

    // Pictures that are turned and made bigger and smaller, some of them see-through: where the edges of layers are, which is where the two drawings differ
    // most (the edge of a picture that goes through the middle of a pixel, to a thousandth, is in or out of it as the last bit of a number says).
    let gradient = a_picture_from(48, 32, |across, down| {
        let [red, green, blue] = the_sky(across, down);
        [red, green, blue, 255]
    });
    let soft_disc = a_picture_from(40, 40, |across, down| {
        let distance = ((across - 0.5).powi(2) + (down - 0.5).powi(2)).sqrt() * 2.0;
        [230, 90, 40, ((1.0 - distance).clamp(0.0, 1.0) * 255.0) as u8]
    });
    let half_clear_checkers = a_picture_from(50, 30, |across, down| {
        if ((across * 10.0) as u32 + (down * 6.0) as u32).is_multiple_of(2) {
            [20, 40, 200, 128]
        } else {
            [240, 220, 40, 128]
        }
    });
    scenes.push((
        "01-pictures-turned-and-see-through",
        a_scene_of(
            Vec::new(),
            vec![gradient, soft_disc, half_clear_checkers],
            vec![
                recipe(
                    Kind::Image(0),
                    Placement::new(CanvasArea::new(0.05, 0.1, 0.55, 0.7), Angle::from_degrees(17.0)),
                    ColourMatrix::NOTHING_CHANGES,
                ),
                recipe(
                    Kind::Image(1),
                    Placement::new(CanvasArea::new(0.35, 0.15, 0.55, 0.75), Angle::from_degrees(-35.0)),
                    ColourMatrix::NOTHING_CHANGES,
                ),
                recipe(
                    Kind::Image(2),
                    Placement::new(CanvasArea::new(0.55, 0.05, 0.4, 0.45), Angle::from_degrees(10.0)),
                    ColourMatrix::NOTHING_CHANGES,
                ),
            ],
            None,
        ),
    ));

    // Colours: the colour effects, which the engine works in half-float numbers and the old drawing in 8 bits.
    scenes.push((
        "02-colour-effects",
        a_scene_of(
            vec![
                a_video_picture_from(64, 36, 1, the_sky),
                a_video_picture_from(64, 36, 2, |across, down| the_sky(1.0 - across, down)),
            ],
            Vec::new(),
            vec![
                recipe(
                    Kind::VideoInRgba(0),
                    Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 1.0)),
                    ColourMatrix::hue_rotation(120.0),
                ),
                recipe(
                    Kind::VideoInRgba(1),
                    Placement::upright(CanvasArea::new(0.5, 0.0, 0.5, 1.0)),
                    ColourMatrix::from_rows([[1.3, -0.2, 0.1], [-0.1, 1.1, 0.0], [0.0, 0.3, 0.7]]),
                ),
            ],
            None,
        ),
    ));

    // A video that is stored in planes, as the engine takes it: the colours are made on the card from the planes, and not by ffmpeg, which the old drawing's picture
    // is (here the old drawing makes them on the processor, by the same arithmetic: the difference with ffmpeg's own is in `docs/engine-phase-1-design.md`).
    let (width, height) = (64u32, 36u32);
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let planes = SuppliedPicture {
        source: SourceMediaIdentifier::new(1),
        source_frame: SourceFrameIndex::new(0),
        frame: SuppliedFrame::Yuv420p {
            frame: DecodedYuvFrame {
                width_in_pixels: width,
                height_in_pixels: height,
                y: (0..width * height)
                    .map(|place| (40.0 + 180.0 * ((place % width) as f32 / width as f32)) as u8)
                    .collect(),
                u: (0..chroma_width * chroma_height)
                    .map(|place| (90.0 + 80.0 * ((place / chroma_width) as f32 / chroma_height as f32)) as u8)
                    .collect(),
                v: (0..chroma_width * chroma_height)
                    .map(|place| (170.0 - 90.0 * ((place % chroma_width) as f32 / chroma_width as f32)) as u8)
                    .collect(),
            },
            colour: NativeYuv420p {
                matrix: NativeMatrix::Bt709,
                full_range: false,
            },
        },
        serial_number: 3,
    };
    scenes.push((
        "03-a-video-in-planes",
        a_scene_of(
            vec![planes],
            Vec::new(),
            vec![recipe(
                Kind::VideoInPlanes(0),
                Placement::new(CanvasArea::new(0.1, 0.1, 0.8, 0.8), Angle::from_degrees(-8.0)),
                ColourMatrix::NOTHING_CHANGES,
            )],
            None,
        ),
    ));

    // The fast-forward symbol, which the engine draws at the resolution of the frame, and the old drawing at the resolution of the screen.
    scenes.push((
        "04-the-symbol-over-a-video",
        a_scene_of(
            vec![a_video_picture_from(64, 36, 4, the_sky)],
            Vec::new(),
            vec![recipe(
                Kind::VideoInRgba(0),
                Placement::upright(CanvasArea::WHOLE_PICTURE),
                ColourMatrix::NOTHING_CHANGES,
            )],
            Some(SymbolOnThePreview {
                over: clip(0),
                speed: SpeedFactor::new(8).unwrap(),
            }),
        ),
    ));
    scenes
}

/// Three panels side by side: `old`, `engine` and the difference between them made bigger. Both are `RENDER_SIZE` big. Also says how big the difference is.
fn three_panels(old: &[u8], engine: &[u8]) -> (image::RgbaImage, u8, usize) {
    let (width, height) = RENDER_SIZE;
    let mut panels = image::RgbaImage::from_pixel(
        3 * width + 2 * SPACE_BETWEEN_THE_PANELS,
        height,
        image::Rgba([90, 90, 90, 255]),
    );
    let (mut largest, mut more_than_one) = (0u8, 0usize);
    for down in 0..height {
        for across in 0..width {
            let place = ((down * width + across) * 4) as usize;
            let (old_pixel, engine_pixel) = (&old[place..place + 4], &engine[place..place + 4]);
            let difference = (0..4)
                .map(|channel| old_pixel[channel].abs_diff(engine_pixel[channel]))
                .max()
                .unwrap();
            largest = largest.max(difference);
            more_than_one += usize::from(difference > 1);
            let shown = difference.saturating_mul(HOW_MUCH_THE_DIFFERENCES_ARE_MADE_BIGGER);
            panels.put_pixel(
                across,
                down,
                image::Rgba([old_pixel[0], old_pixel[1], old_pixel[2], 255]),
            );
            panels.put_pixel(
                width + SPACE_BETWEEN_THE_PANELS + across,
                down,
                image::Rgba([engine_pixel[0], engine_pixel[1], engine_pixel[2], 255]),
            );
            panels.put_pixel(
                2 * (width + SPACE_BETWEEN_THE_PANELS) + across,
                down,
                image::Rgba([shown, shown, shown, 255]),
            );
        }
    }
    (panels, largest, more_than_one)
}

#[test]
#[ignore = "writes the pictures into docs/engine-comparison: run it when the engine or the old drawing changes what it makes"]
fn write_the_pictures_that_show_what_differs() {
    with_software_graphics!(graphics, {
        let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
        text_renderer.get_waiting_if_needed().expect("the fonts load");
        std::fs::create_dir_all(folder()).unwrap();
        let mut lines = Vec::new();

        // The scenes that are made of numbers.
        let mut all: Vec<(String, Scene)> = the_scenes()
            .into_iter()
            .map(|(name, scene)| (name.to_owned(), scene))
            .collect();
        // Text, over a picture, turned: the edges of letters are every opacity there is.
        let background = a_video_picture_from(64, 36, 5, the_sky);
        all.push((
            "05-text-over-a-picture".to_owned(),
            a_scene_of(
                vec![background],
                Vec::new(),
                vec![recipe(
                    Kind::VideoInRgba(0),
                    Placement::upright(CanvasArea::WHOLE_PICTURE),
                    ColourMatrix::NOTHING_CHANGES,
                )],
                None,
            ),
        ));

        for (name, scene) in &all {
            let mut content = scene.content();
            if name == "05-text-over-a-picture" {
                content.layers.push(PreviewLayer {
                    clip: clip(1),
                    track: TrackId::new(1),
                    placement: Placement::new(CanvasArea::new(0.1, 0.25, 0.8, 0.5), Angle::from_degrees(-12.0)),
                    colour: ColourMatrix::NOTHING_CHANGES,
                    content: PreviewLayerContent::Text(std::borrow::Cow::Owned(TextObject {
                        text: "Hello, preview".to_owned(),
                        ..TextObject::default()
                    })),
                });
            }
            assert_the_drawings_agree(name, &graphics, &content, &mut text_renderer);
            let old = the_old_drawing(&graphics, &content, &mut text_renderer);
            let (engine, _, _) = the_engine_drawing(&graphics, &content, &mut text_renderer);
            let (panels, largest, more_than_one) = three_panels(&old, &engine);
            panels.save(folder().join(format!("{name}.png"))).unwrap();
            lines.push(format!(
                "{name}: the largest difference of any number of a pixel is {largest} (of 255), and {more_than_one} of the {} pixels differ by more than 1",
                RENDER_SIZE.0 * RENDER_SIZE.1
            ));
        }
        for line in &lines {
            println!("{line}");
        }
    });
}
