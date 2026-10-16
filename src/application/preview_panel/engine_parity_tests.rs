// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview made by the engine against the preview made by the old drawing, on the same content: the gate of EN-02 (see `docs/engine-phase-1-design.md`).
//!
//! **The old drawing is the real one**: `preview_panel::show` is run inside an egui context, what it draws is tessellated, and egui's own renderer (egui-wgpu, in
//! the mode made for tests that compare pictures, on the software adapter) makes the pixels of it. The engine is given what `describe_frame` makes of the same
//! content. If the engine is right the two are the same picture, to within the little that comes of the engine working in half-float numbers and the old
//! drawing working in 8 bits at every step.

use std::collections::HashSet;
use std::sync::Arc;

use egui::epaint::ImageDelta;
use egui::{Context, Frame, RawInput, Rect, TextureId, pos2, vec2};

use super::engine_view::{SentPictures, describe_frame};
use super::test_content::clip;
use super::{
    PreviewContent, PreviewLayer, PreviewLayerContent, PreviewView, SymbolOnThePreview, VideoInThePicture, show,
};
use crate::application::textures::Textures;
use crate::application::theme;
use crate::colour_effects::ColourMatrix;
use crate::colour_space::VideoMatrix;
use crate::engine::oracle_tests::render_with_egui;
use crate::engine::testing::{SoftwareGraphics, with_software_graphics};
use crate::engine::{FrameDescription, FrameRenderer, LayerSource, PictureColour};
use crate::media::{
    CompressedPicture, DecodedFrame, DecodedYuvFrame, NativeYuv420p, PictureSize, ThumbnailKey, ThumbnailLibrary,
};
use crate::playback::{SuppliedFrame, SuppliedPicture};
use crate::project::{
    Angle, CanvasArea, Placement, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout,
    TextObject, TrackId,
};
use crate::rgba_picture::RgbaPicture;
use crate::text_rendering::LazyTextRenderer;

/// The size of the frames, in pixels, which is the size the preview is drawn at (one pixel to the point).
const RENDER_SIZE: (u32, u32) = (320, 180);

/// How much room the preview leaves round the picture, in points (and, at one pixel to the point, pixels), which is the same on all four sides.
const MARGIN: u32 = theme::SPACING as u32;

/// The size of the project's picture: what the corners of a layer are worked out on. It has the shape of the frames.
const PROJECT_PICTURE: (u32, u32) = (1280, 720);

const A_LONG_TIME: std::time::Duration = std::time::Duration::from_secs(60);

// ---- the two drawings ----

/// What the old drawing makes of `content`, as pixels.
fn the_old_drawing(
    graphics: &SoftwareGraphics,
    content: &PreviewContent<'_>,
    text_renderer: &mut LazyTextRenderer,
) -> Vec<u8> {
    let context = Context::default();
    let mut textures = Textures::default();
    let mut view = PreviewView::FITTING;
    let mut commands = Vec::new();
    // A panel that leaves the room round the picture that the preview leaves, so that the picture is exactly `RENDER_SIZE` big, and in it.
    let panel_size = (RENDER_SIZE.0 + 2 * MARGIN, RENDER_SIZE.1 + 2 * MARGIN);
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(panel_size.0 as f32, panel_size.1 as f32),
        )),
        ..RawInput::default()
    };
    let mut output = context.run_ui(input, |ui| {
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            show(ui, content, &mut view, &mut textures, text_renderer, &mut commands);
        });
    });
    assert!(commands.is_empty(), "nobody touched the preview: {commands:?}");
    let paint_jobs = context.tessellate(output.shapes, output.pixels_per_point);
    // (Each texture's changes in the order they were made: the picture is set first, and may be changed after.)
    let textures: Vec<(TextureId, ImageDelta)> = output
        .textures_delta
        .set
        .iter()
        .flat_map(|(id, deltas)| deltas.iter().map(move |delta| (*id, delta.clone())))
        .collect();
    // (The changes of the textures are handled: egui makes sure of that, and a test that only draws once has no other use for them.)
    output.textures_delta.clear();
    let panel = render_with_egui(&graphics.device, panel_size, &textures, &paint_jobs);
    // The picture, cut out of the panel.
    let mut picture = Vec::with_capacity(RENDER_SIZE.0 as usize * RENDER_SIZE.1 as usize * 4);
    for down in 0..RENDER_SIZE.1 {
        let first = (((down + MARGIN) * panel_size.0 + MARGIN) * 4) as usize;
        picture.extend_from_slice(&panel[first..first + RENDER_SIZE.0 as usize * 4]);
    }
    picture
}

/// What the engine makes of `content`, as pixels, what it was asked to draw, and how many effects it could not run.
fn the_engine_drawing(
    graphics: &SoftwareGraphics,
    content: &PreviewContent<'_>,
    text_renderer: &mut LazyTextRenderer,
) -> (Vec<u8>, FrameDescription, usize) {
    let mut renderer = FrameRenderer::new(graphics.device.clone());
    let mut sent = HashSet::new();
    let description = {
        let mut supply = SentPictures {
            sent: &mut sent,
            text_renderer,
            send: |id, data| renderer.upload_picture(id, &data).unwrap(),
        };
        describe_frame(content, RENDER_SIZE, 0, &mut supply)
    };
    let (frame, statistics) = renderer.render(&description, A_LONG_TIME).unwrap();
    (frame.rgba_pixels, description, statistics.effects_not_run)
}

// ---- making content from numbers ----

/// Numbers that look random and are the same every time.
struct Random(u64);

impl Random {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 >> 11
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (self.next() % 1_000_000) as f32 / 1_000_000.0 * (high - low)
    }

    fn one_in(&mut self, chance: u64) -> bool {
        self.below(chance) == 0
    }

    fn bytes(&mut self, count: usize) -> Vec<u8> {
        (0..count).map(|_| (self.next() >> 8) as u8).collect()
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    VideoInRgba(usize),
    VideoInPlanes(usize),
    VideoWaiting,
    Image(usize),
    ImageNotDecoded,
    Thumbnail(usize),
}

#[derive(Clone, Copy, Debug)]
struct LayerRecipe {
    kind: Kind,
    placement: Placement,
    colour: ColourMatrix,
}

/// Everything a random preview is made of, which the `PreviewContent` borrows.
struct Scene {
    video_pictures: Vec<SuppliedPicture>,
    images: Vec<RgbaPicture>,
    libraries: Vec<ThumbnailLibrary>,
    recipes: Vec<LayerRecipe>,
    symbol: Option<SymbolOnThePreview>,
    indicator_layout: SpeedIndicatorLayout,
}

impl Scene {
    fn content(&self) -> PreviewContent<'_> {
        let layers = self
            .recipes
            .iter()
            .enumerate()
            .map(|(number, recipe)| PreviewLayer {
                clip: clip(number as u32),
                track: TrackId::new(number as u32),
                placement: recipe.placement,
                colour: recipe.colour,
                content: match recipe.kind {
                    Kind::VideoInRgba(index) | Kind::VideoInPlanes(index) => {
                        PreviewLayerContent::Video(VideoInThePicture::Picture(&self.video_pictures[index]))
                    }
                    Kind::VideoWaiting => PreviewLayerContent::Video(VideoInThePicture::Waiting),
                    Kind::Image(index) => PreviewLayerContent::Image {
                        source: SourceMediaIdentifier::new(index as u32 + 10),
                        picture: Some(&self.images[index]),
                        colour: PictureColour::SRGB,
                    },
                    Kind::ImageNotDecoded => PreviewLayerContent::Image {
                        source: SourceMediaIdentifier::new(99),
                        picture: None,
                        colour: PictureColour::SRGB,
                    },
                    Kind::Thumbnail(index) => PreviewLayerContent::Video(VideoInThePicture::Thumbnail {
                        source: SourceMediaIdentifier::new(index as u32 + 50),
                        second: 0,
                        library: &self.libraries[index],
                    }),
                },
            })
            .collect();
        PreviewContent {
            picture_size: Some(PROJECT_PICTURE),
            layers,
            symbol: self.symbol,
            indicator_layout: self.indicator_layout,
            selected_clip: None,
        }
    }
}

fn a_random_placement(random: &mut Random) -> Placement {
    let area = CanvasArea::new(
        random.between(-0.3, 0.9),
        random.between(-0.3, 0.9),
        random.between(0.1, 1.0),
        random.between(0.1, 1.0),
    );
    let rotation = match random.below(5) {
        0 | 1 => 0.0,
        2 => random.between(-180.0, 180.0),
        3 => 90.0 * random.below(4) as f32,
        _ => random.between(-720.0, 720.0),
    };
    Placement::new(area, Angle::from_degrees(rotation))
}

fn a_random_colour(random: &mut Random) -> ColourMatrix {
    match random.below(5) {
        0 | 1 => ColourMatrix::NOTHING_CHANGES,
        2 => ColourMatrix::hue_rotation(random.between(0.0, 360.0)),
        3 => ColourMatrix::hue_rotation(random.between(0.0, 360.0))
            .then(ColourMatrix::hue_rotation(random.between(0.0, 360.0))),
        _ => ColourMatrix::from_rows([
            [
                random.between(0.0, 1.5),
                random.between(-0.3, 0.5),
                random.between(-0.3, 0.5),
            ],
            [
                random.between(-0.3, 0.5),
                random.between(0.0, 1.5),
                random.between(-0.3, 0.5),
            ],
            [
                random.between(-0.3, 0.5),
                random.between(-0.3, 0.5),
                random.between(0.0, 1.5),
            ],
        ]),
    }
}

/// A picture of noise `width` by `height`, made smooth so that a different way of putting it on the screen does not make a different picture of every pixel.
fn a_random_video_picture(random: &mut Random, serial_number: u64, in_planes: bool) -> SuppliedPicture {
    let (width, height) = (8 + random.below(40) as u32, 8 + random.below(30) as u32);
    let frame = if in_planes {
        let (chroma_width, chroma_height) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
        SuppliedFrame::Yuv420p {
            frame: DecodedYuvFrame {
                width_in_pixels: width,
                height_in_pixels: height,
                y: random.bytes((width * height) as usize),
                u: random.bytes(chroma_width * chroma_height),
                v: random.bytes(chroma_width * chroma_height),
            },
            colour: NativeYuv420p {
                matrix: [VideoMatrix::Bt601, VideoMatrix::Bt709, VideoMatrix::Bt2020][random.below(3) as usize],
                full_range: random.one_in(2),
            },
        }
    } else {
        let mut rgba_pixels = random.bytes((width * height * 4) as usize);
        for pixel in rgba_pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        SuppliedFrame::Rgba(DecodedFrame {
            width_in_pixels: width,
            height_in_pixels: height,
            rgba_pixels,
        })
    };
    SuppliedPicture {
        chroma_siting: crate::colour_space::ChromaSiting::Left,
        colour: crate::engine::PictureColour::SRGB,
        source: SourceMediaIdentifier::new(1),
        source_frame: SourceFrameIndex::new(0),
        frame,
        serial_number,
    }
}

fn a_random_image(random: &mut Random) -> RgbaPicture {
    let (width, height) = (6 + random.below(30) as u32, 6 + random.below(24) as u32);
    let mut rgba_pixels = random.bytes((width * height * 4) as usize);
    // Some of the pictures are see-through, with every opacity there is; some are opaque; some of the pixels are clear.
    match random.below(3) {
        0 => {
            for pixel in rgba_pixels.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        1 => {
            for (number, pixel) in rgba_pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                if number % 5 == 0 {
                    pixel[3] = 0;
                }
            }
        }
        _ => {}
    }
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels,
    }
}

fn a_random_thumbnail_library(random: &mut Random) -> ThumbnailLibrary {
    let (width, height) = (16 + random.below(16) as u32, 9 + random.below(9) as u32);
    let mut rgb = random.bytes((width * height * 3) as usize);
    // (Smooth, so that the compression does not make a picture of noise of it: it is the same picture on both sides either way.)
    for byte in rgb.iter_mut() {
        *byte = 40 + *byte / 2;
    }
    let mut library = ThumbnailLibrary::with_room_for(4);
    library.insert(
        ThumbnailKey { level: 0, index: 0 },
        CompressedPicture::from_rgb_pixels(
            &rgb,
            PictureSize {
                width_in_pixels: width,
                height_in_pixels: height,
            },
        )
        .unwrap(),
    );
    library
}

/// A scene made from `seed`: one to five layers of every kind there is, in places and colours of every kind, and sometimes the symbol over a video.
fn a_random_scene(seed: u64) -> Scene {
    let mut random = Random::new(seed);
    let mut scene = Scene {
        video_pictures: Vec::new(),
        images: Vec::new(),
        libraries: Vec::new(),
        recipes: Vec::new(),
        symbol: None,
        indicator_layout: SpeedIndicatorLayout::new(
            random.between(0.0, 0.7),
            random.between(0.0, 0.7),
            random.between(0.08, 0.3),
        ),
    };
    let mut videos = Vec::new();
    for number in 0..1 + random.below(5) as usize {
        let kind = match random.below(10) {
            0..=2 => {
                scene.video_pictures.push(a_random_video_picture(
                    &mut random,
                    seed * 100 + number as u64 + 1,
                    false,
                ));
                Kind::VideoInRgba(scene.video_pictures.len() - 1)
            }
            3 | 4 => {
                scene.video_pictures.push(a_random_video_picture(
                    &mut random,
                    seed * 100 + number as u64 + 1,
                    true,
                ));
                Kind::VideoInPlanes(scene.video_pictures.len() - 1)
            }
            5 => Kind::VideoWaiting,
            6 | 7 => {
                scene.images.push(a_random_image(&mut random));
                Kind::Image(scene.images.len() - 1)
            }
            8 => Kind::ImageNotDecoded,
            _ => {
                scene.libraries.push(a_random_thumbnail_library(&mut random));
                Kind::Thumbnail(scene.libraries.len() - 1)
            }
        };
        let colour = match kind {
            // (A layer that is only a colour has nothing to mix the colours of.)
            Kind::VideoWaiting | Kind::ImageNotDecoded => ColourMatrix::NOTHING_CHANGES,
            _ => a_random_colour(&mut random),
        };
        if matches!(
            kind,
            Kind::VideoInRgba(_) | Kind::VideoInPlanes(_) | Kind::Thumbnail(_) | Kind::VideoWaiting
        ) {
            videos.push(number);
        }
        scene.recipes.push(LayerRecipe {
            kind,
            placement: a_random_placement(&mut random),
            colour,
        });
    }
    if !videos.is_empty() && random.one_in(3) {
        scene.symbol = Some(SymbolOnThePreview {
            over: clip(videos[random.below(videos.len() as u64) as usize] as u32),
            speed: SpeedFactor::new([2, 4, 8, 16][random.below(4) as usize]).unwrap(),
        });
    }
    scene
}

// ---- comparing ----

/// How near to the edge of a layer a pixel has to be, in pixels, for the two drawings to be allowed to disagree about it by more than the rounding when
/// the layer is a picture: the drawings draw the same triangles, but where the middle of a pixel is on the edge of one (to a thousandth of a pixel)
/// whether it is in is a matter of the last bit of a number, which the engine and egui work out in another order.
const HOW_NEAR_THE_EDGE_OF_A_PICTURE: f32 = 0.05;

/// ...and when the layer is only a colour (which egui feathers: it draws the edge softly, over about a pixel, where the engine's is hard).
const HOW_NEAR_THE_EDGE_OF_A_COLOUR: f32 = 1.0;

/// How far a point is from the segment `from` to `to`.
fn distance_from_a_segment(point: egui::Pos2, from: egui::Pos2, to: egui::Pos2) -> f32 {
    let along = to - from;
    let length_squared = along.length_sq();
    let how_far = if length_squared == 0.0 {
        0.0
    } else {
        ((point - from).dot(along) / length_squared).clamp(0.0, 1.0)
    };
    (point - (from + along * how_far)).length()
}

/// How near the middle of the pixel `across`, `down` is to the edge of any layer of `description` that is of the kind `colours` says (a colour, or a picture).
fn nearest_edge(description: &FrameDescription, across: u32, down: u32, colours: bool) -> f32 {
    let middle = pos2(across as f32 + 0.5, down as f32 + 0.5);
    description
        .layers
        .iter()
        .filter(|layer| matches!(layer.source, LayerSource::Solid(_)) == colours)
        .flat_map(|layer| {
            let corners = layer
                .corners
                .map(|(x, y)| pos2(x * RENDER_SIZE.0 as f32, y * RENDER_SIZE.1 as f32));
            (0..4).map(move |edge| distance_from_a_segment(middle, corners[edge], corners[(edge + 1) % 4]))
        })
        .fold(f32::MAX, f32::min)
}

/// How far the largest of the red, green, blue and opacity of two pixels are from each other.
fn gap_between_pixels(engine: &[u8], old: &[u8]) -> u8 {
    engine.iter().zip(old).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0)
}

/// What the two drawings differ by.
#[derive(Debug, Default)]
struct Agreement {
    /// The most that a number of a pixel that is not on an edge is off by.
    largest_away_from_edges: u8,
    /// How many pixels are off by more than the rounding and are on the edge of a picture (to a twentieth of a pixel).
    pixels_on_the_edge_of_a_picture: usize,
    /// How many are off by more than the rounding and are on the soft edge of a colour.
    pixels_on_the_edge_of_a_colour: usize,
    /// The first pixel that is off by more than the rounding and is on no edge, if there is one: where it is, and what each drawing made of it.
    pixel_that_is_wrong: Option<(u32, u32, [u8; 4], [u8; 4])>,
}

/// What the two drawings differ by, where `rounding` is how much of a difference there is allowed to be anywhere.
fn agreement_between(engine: &[u8], old: &[u8], description: &FrameDescription, rounding: u8) -> Agreement {
    let mut agreement = Agreement::default();
    for (number, (engine, old)) in engine.as_chunks::<4>().0.iter().zip(old.as_chunks::<4>().0).enumerate() {
        let gap = gap_between_pixels(engine, old);
        let (across, down) = (number as u32 % RENDER_SIZE.0, number as u32 / RENDER_SIZE.0);
        let on_a_picture = nearest_edge(description, across, down, false) < HOW_NEAR_THE_EDGE_OF_A_PICTURE;
        let on_a_colour = nearest_edge(description, across, down, true) < HOW_NEAR_THE_EDGE_OF_A_COLOUR;
        if !on_a_picture && !on_a_colour {
            agreement.largest_away_from_edges = agreement.largest_away_from_edges.max(gap);
        }
        if gap <= rounding {
            continue;
        }
        if on_a_picture {
            agreement.pixels_on_the_edge_of_a_picture += 1;
        } else if on_a_colour {
            agreement.pixels_on_the_edge_of_a_colour += 1;
        } else if agreement.pixel_that_is_wrong.is_none() {
            agreement.pixel_that_is_wrong = Some((across, down, *engine, *old));
        }
    }
    agreement
}

// ---- the gate ----

/// How much two pixels may be off from each other by anywhere that is not on an edge, in any of the red, green, blue and opacity. **Measured** (Windows,
/// 2026-10-15), over 400 random scenes of one to five layers of every kind (videos in RGBA and in planes, picture files that are opaque and that are see-through,
/// thumbnails, colours, the symbol, effects of every kind, upright and turned, whole and cut off by the edge), with up to four layers that are see-through:
/// **2**. What is left is the old drawing's rounding to 8 bits at every step (the colours of a picture changed in 8 bits, premultiplied in 8 bits, blended in 8
/// bits) against the engine's half-float numbers.
const ROUNDING: u8 = 2;

/// How many pixels of a frame may be off by more than that on the edge of a picture. Measured: 2.
const MOST_PIXELS_ON_THE_EDGES_OF_PICTURES: usize = 8;

/// How many pixels may be off on the soft edge of a colour, for each pixel of how long the edges of the colours are (and eight more): a colour is drawn by egui
/// with an edge that is soft over about a pixel, and by the engine with one that is hard, so the pixels along the edge are not the same.
const PIXELS_OF_A_SOFT_EDGE_FOR_EACH_PIXEL_OF_ITS_LENGTH: f32 = 2.0;

/// How long the edges of all the colours of the frame are, in pixels.
fn length_of_the_edges_of_the_colours(description: &FrameDescription) -> f32 {
    description
        .layers
        .iter()
        .filter(|layer| matches!(layer.source, LayerSource::Solid(_)))
        .map(|layer| {
            let corners = layer
                .corners
                .map(|(x, y)| pos2(x * RENDER_SIZE.0 as f32, y * RENDER_SIZE.1 as f32));
            (0..4)
                .map(|edge| (corners[(edge + 1) % 4] - corners[edge]).length())
                .sum::<f32>()
        })
        .sum()
}

/// Makes the scene both ways and checks the two are the same picture: every pixel is the same to within the rounding, but the pixels on the edge of a layer.
fn assert_the_drawings_agree(
    name: &str,
    graphics: &SoftwareGraphics,
    content: &PreviewContent<'_>,
    text_renderer: &mut LazyTextRenderer,
) {
    let old = the_old_drawing(graphics, content, text_renderer);
    let (engine, description, effects_not_run) = the_engine_drawing(graphics, content, text_renderer);
    assert_eq!(effects_not_run, 0, "{name}");
    let agreement = agreement_between(&engine, &old, &description, ROUNDING);
    assert_eq!(
        agreement.pixel_that_is_wrong, None,
        "{name}: a pixel that is off by more than {ROUNDING} and is on no edge (across, down, the engine's, the old drawing's)"
    );
    assert!(agreement.largest_away_from_edges <= ROUNDING, "{name}: {agreement:?}");
    assert!(
        agreement.pixels_on_the_edge_of_a_picture <= MOST_PIXELS_ON_THE_EDGES_OF_PICTURES,
        "{name}: {agreement:?}"
    );
    let allowed_on_the_edges_of_colours = (length_of_the_edges_of_the_colours(&description)
        * PIXELS_OF_A_SOFT_EDGE_FOR_EACH_PIXEL_OF_ITS_LENGTH) as usize
        + 8;
    assert!(
        agreement.pixels_on_the_edge_of_a_colour <= allowed_on_the_edges_of_colours,
        "{name}: {agreement:?} (allowed {allowed_on_the_edges_of_colours})"
    );
}

#[test]
fn random_scenes_of_every_kind_are_the_same_picture_in_the_engine_and_in_the_old_drawing() {
    with_software_graphics!(graphics, {
        let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
        // (400 were measured; this many are run every time, which is a few seconds.)
        for seed in 1..=120u64 {
            let scene = a_random_scene(seed);
            assert_the_drawings_agree(
                &format!("scene {seed}"),
                &graphics,
                &scene.content(),
                &mut text_renderer,
            );
        }
    });
}

#[test]
#[ignore = "the longer run that the numbers above were measured with"]
fn four_hundred_random_scenes_are_the_same_picture_in_the_engine_and_in_the_old_drawing() {
    with_software_graphics!(graphics, {
        let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
        for seed in 1..=400u64 {
            let scene = a_random_scene(seed);
            assert_the_drawings_agree(
                &format!("scene {seed}"),
                &graphics,
                &scene.content(),
                &mut text_renderer,
            );
        }
    });
}

#[test]
fn a_video_picture_that_is_the_size_of_the_frame_is_exactly_what_the_old_drawing_makes_of_it() {
    with_software_graphics!(graphics, {
        let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
        let mut random = Random::new(7);
        let mut rgba_pixels = random.bytes((RENDER_SIZE.0 * RENDER_SIZE.1 * 4) as usize);
        for pixel in rgba_pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        let picture = SuppliedPicture {
            chroma_siting: crate::colour_space::ChromaSiting::Left,
            colour: crate::engine::PictureColour::SRGB,
            source: SourceMediaIdentifier::new(1),
            source_frame: SourceFrameIndex::new(0),
            frame: SuppliedFrame::Rgba(DecodedFrame {
                width_in_pixels: RENDER_SIZE.0,
                height_in_pixels: RENDER_SIZE.1,
                rgba_pixels,
            }),
            serial_number: 1,
        };
        let content = PreviewContent {
            picture_size: Some(PROJECT_PICTURE),
            layers: vec![PreviewLayer {
                clip: clip(0),
                track: TrackId::new(0),
                placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
                colour: ColourMatrix::NOTHING_CHANGES,
                content: PreviewLayerContent::Video(VideoInThePicture::Picture(&picture)),
            }],
            symbol: None,
            indicator_layout: SpeedIndicatorLayout::DEFAULT,
            selected_clip: None,
        };
        let old = the_old_drawing(&graphics, &content, &mut text_renderer);
        let (engine, _, _) = the_engine_drawing(&graphics, &content, &mut text_renderer);
        assert_eq!(engine, old, "not a number is off, not even by one");
    });
}

#[test]
fn a_text_object_is_the_same_picture_in_the_engine_and_in_the_old_drawing_whatever_its_colours_and_place() {
    with_software_graphics!(graphics, {
        let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
        text_renderer.get_waiting_if_needed().expect("the fonts load");
        let object = |words: &str, colour: ColourMatrix, placement: Placement| PreviewLayer {
            clip: clip(0),
            track: TrackId::new(0),
            placement,
            colour,
            content: PreviewLayerContent::Text(std::borrow::Cow::Owned(TextObject {
                text: words.to_owned(),
                ..TextObject::default()
            })),
        };
        for (name, colour, placement) in [
            (
                "as it is",
                ColourMatrix::NOTHING_CHANGES,
                Placement::upright(CanvasArea::WHOLE_PICTURE),
            ),
            (
                "turned hues",
                ColourMatrix::hue_rotation(150.0),
                Placement::upright(CanvasArea::new(0.1, 0.2, 0.7, 0.5)),
            ),
            (
                "turned and smaller",
                ColourMatrix::NOTHING_CHANGES,
                Placement::new(CanvasArea::new(0.2, 0.3, 0.5, 0.3), Angle::from_degrees(23.0)),
            ),
        ] {
            let content = PreviewContent {
                picture_size: Some(PROJECT_PICTURE),
                layers: vec![object("Hello, preview", colour, placement)],
                symbol: None,
                indicator_layout: SpeedIndicatorLayout::DEFAULT,
                selected_clip: None,
            };
            assert_the_drawings_agree(name, &graphics, &content, &mut text_renderer);
            // (And the picture really is there: the text is on it.)
            let (engine, _, _) = the_engine_drawing(&graphics, &content, &mut text_renderer);
            assert!(
                engine.as_chunks::<4>().0.iter().any(|pixel| pixel[..3] != [0, 0, 0]),
                "{name}: nothing is drawn"
            );
        }
    });
}

#[test]
fn the_symbol_over_a_video_is_the_same_picture_in_the_engine_and_in_the_old_drawing_at_every_size_and_speed() {
    with_software_graphics!(graphics, {
        let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
        let mut random = Random::new(3);
        let picture = a_random_video_picture(&mut random, 1, false);
        for speed in [2, 4, 8, 16] {
            for layout in [
                SpeedIndicatorLayout::DEFAULT,
                SpeedIndicatorLayout::new(0.05, 0.05, 0.1),
                SpeedIndicatorLayout::new(0.6, 0.5, 0.3),
            ] {
                let content = PreviewContent {
                    picture_size: Some(PROJECT_PICTURE),
                    layers: vec![PreviewLayer {
                        clip: clip(0),
                        track: TrackId::new(0),
                        placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
                        colour: ColourMatrix::NOTHING_CHANGES,
                        content: PreviewLayerContent::Video(VideoInThePicture::Picture(&picture)),
                    }],
                    symbol: Some(SymbolOnThePreview {
                        over: clip(0),
                        speed: SpeedFactor::new(speed).unwrap(),
                    }),
                    indicator_layout: layout,
                    selected_clip: None,
                };
                assert_the_drawings_agree(
                    &format!("{speed}x with {layout:?}"),
                    &graphics,
                    &content,
                    &mut text_renderer,
                );
            }
        }
    });
}

// The pictures of what differs, for looking at (an ignored test that writes them).
mod pictures;
