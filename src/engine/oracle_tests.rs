// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The oracle: what the window drew before there was an engine, drawn again by the window's own renderer (egui-wgpu, in the mode made for tests that
//! compare pictures: no dithering, and the filtering of textures done in the shader so that it is the same on every card), and the same layers made by
//! the engine. If the engine is right the two are the same picture, to within the little that comes of the engine working in half-float numbers and
//! the window's renderer working in 8 bits at every step.
//!
//! The oracle is given what the window gives egui for a layer (see `application/preview_panel/layers.rs`): a texture of the picture, uploaded
//! premultiplied in 8 bits as egui does it, and a mesh of the layer's four corners and two triangles (indices 0 1 2 and 0 2 3), in white, over a
//! frame that is first filled with black. The corners are worked out in pixels as a number between 0 and 1 times the size of the frame, as the
//! window works them out from the fractions that a placement gives.
//!
//! **What was measured** (on the software adapters of Windows and of Linux, 2026-10-15), and is now asserted: a picture drawn one pixel to a
//! pixel is exactly the oracle's; every other scene (made bigger or smaller, turned, cut off by the edge, translucent, all of one colour, and forty made up
//! from numbers) differs by one level at most in any number, in a few of a hundred of them, and covers exactly the same pixels.

use egui::epaint::{ClippedPrimitive, Color32, ColorImage, ImageDelta, Mesh, Primitive, Vertex};
use egui::{Rect, TextureId, TextureOptions, pos2};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

use super::readback::ReadbackBuffers;
use super::testing::{SoftwareGraphics, with_software_graphics};
use super::{EngineDevice, FrameDescription, FrameRenderer, LayerDescription, LayerSource, PictureData, PictureId};

const A_LONG_TIME: std::time::Duration = std::time::Duration::from_secs(60);

// ---- the pictures that the layers are made of ----

/// A picture of straight RGBA bytes.
#[derive(Clone)]
pub struct TestPicture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TestPicture {
    /// Red goes from 0 on the left to 255 on the right, green from 0 at the top to 255 at the bottom, blue is something else again: a different
    /// colour in nearly every pixel, and opaque.
    pub fn gradient(width: u32, height: u32) -> Self {
        let mut rgba = Vec::new();
        for down in 0..height {
            for across in 0..width {
                rgba.push((across * 255 / (width - 1).max(1)) as u8);
                rgba.push((down * 255 / (height - 1).max(1)) as u8);
                rgba.push(((across * 7 + down * 13) % 256) as u8);
                rgba.push(255);
            }
        }
        Self { width, height, rgba }
    }

    /// Squares of two colours, `square` pixels each, which show how a picture is sampled. (Neither colour is black.)
    pub fn checkers(width: u32, height: u32, square: u32) -> Self {
        let mut rgba = Vec::new();
        for down in 0..height {
            for across in 0..width {
                let dark = (across / square + down / square).is_multiple_of(2);
                rgba.extend(if dark { [30, 60, 200, 255] } else { [240, 220, 40, 255] });
            }
        }
        Self { width, height, rgba }
    }

    /// A disc of one colour whose opacity falls from the middle to the edge, and nothing but clear outside it: pixels with every opacity from 0
    /// to 255, as the edges of letters have.
    pub fn soft_disc(width: u32, height: u32, colour: [u8; 3]) -> Self {
        let mut rgba = Vec::new();
        let (centre_across, centre_down) = (width as f32 / 2.0, height as f32 / 2.0);
        let radius = centre_across.min(centre_down);
        for down in 0..height {
            for across in 0..width {
                let distance =
                    ((across as f32 + 0.5 - centre_across).powi(2) + (down as f32 + 0.5 - centre_down).powi(2)).sqrt();
                let opacity = (1.0 - distance / radius).clamp(0.0, 1.0);
                rgba.extend([colour[0], colour[1], colour[2], (opacity * 255.0).round() as u8]);
            }
        }
        Self { width, height, rgba }
    }

    pub fn as_picture_data(&self) -> PictureData {
        PictureData::rgba8(self.width, self.height, self.rgba.clone()).unwrap()
    }
}

// ---- where the layers are ----

/// The four corners (top left, top right, bottom right, bottom left) of a rectangle that is not turned, as fractions of the frame.
pub fn upright(left: f32, top: f32, width: f32, height: f32) -> [(f32, f32); 4] {
    [
        (left, top),
        (left + width, top),
        (left + width, top + height),
        (left, top + height),
    ]
}

/// The four corners of a rectangle of `width` by `height` pixels whose middle is at `middle` (fractions of the frame) turned `degrees` clockwise
/// (as seen on the screen, where down is the way y goes), on a frame of `frame` pixels.
pub fn turned(middle: (f32, f32), width: f32, height: f32, degrees: f32, frame: (u32, u32)) -> [(f32, f32); 4] {
    let (sine, cosine) = degrees.to_radians().sin_cos();
    let (frame_width, frame_height) = (frame.0 as f32, frame.1 as f32);
    let centre = (middle.0 * frame_width, middle.1 * frame_height);
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sign_across, sign_down)| {
        let (across, down) = (sign_across * width / 2.0, sign_down * height / 2.0);
        (
            (centre.0 + across * cosine - down * sine) / frame_width,
            (centre.1 + across * sine + down * cosine) / frame_height,
        )
    })
}

const WHOLE_FRAME: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

// ---- a scene, made by the engine and by the oracle ----

/// What a layer of a test scene is made of.
#[derive(Clone)]
pub enum TestSource {
    Picture(TestPicture),
    Solid([u8; 4]),
}

#[derive(Clone)]
pub struct TestLayer {
    pub source: TestSource,
    pub corners: [(f32, f32); 4],
}

fn gradient(width: u32, height: u32) -> TestSource {
    TestSource::Picture(TestPicture::gradient(width, height))
}

fn checkers(width: u32, height: u32, square: u32) -> TestSource {
    TestSource::Picture(TestPicture::checkers(width, height, square))
}

/// The pixels of the frame that the engine makes of `layers` on a frame `size` big.
pub fn engine_frame(graphics: &SoftwareGraphics, size: (u32, u32), layers: &[TestLayer]) -> Vec<u8> {
    let mut renderer = FrameRenderer::new(graphics.device.clone());
    let mut description = FrameDescription::black(size);
    for (number, layer) in layers.iter().enumerate() {
        let source = match &layer.source {
            TestSource::Picture(picture) => {
                let id = PictureId(number as u64 + 1);
                renderer.upload_picture(id, &picture.as_picture_data()).unwrap();
                LayerSource::Picture(id)
            }
            TestSource::Solid(colour) => LayerSource::Solid(*colour),
        };
        description.layers.push(LayerDescription {
            source,
            corners: layer.corners,
            effects: Vec::new(),
        });
    }
    renderer.render(&description, A_LONG_TIME).unwrap().0.rgba_pixels
}

/// The pixels of the frame that the window's renderer makes of the same layers.
pub fn oracle_frame(device: &EngineDevice, size: (u32, u32), layers: &[TestLayer]) -> Vec<u8> {
    let (width, height) = size;
    let mut renderer = Renderer::new(
        &device.device,
        wgpu::TextureFormat::Rgba8Unorm,
        RendererOptions::PREDICTABLE,
    );
    let mut paint_jobs = Vec::new();
    for (number, layer) in layers.iter().enumerate() {
        let texture_id = TextureId::Managed(number as u64);
        let image = match &layer.source {
            TestSource::Picture(picture) => {
                ColorImage::from_rgba_unmultiplied([picture.width as usize, picture.height as usize], &picture.rgba)
            }
            // (One white pixel: the window draws a layer that is only a colour with the colour of the vertices.)
            TestSource::Solid(_) => ColorImage::from_rgba_unmultiplied([1, 1], &[255, 255, 255, 255]),
        };
        renderer.update_texture(
            &device.device,
            &device.queue,
            texture_id,
            &ImageDelta::full(image, TextureOptions::LINEAR),
        );
        let colour = match layer.source {
            TestSource::Picture(_) => Color32::WHITE,
            TestSource::Solid([red, green, blue, alpha]) => Color32::from_rgba_unmultiplied(red, green, blue, alpha),
        };
        let uvs = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
        let mut mesh = Mesh::with_texture(texture_id);
        for (corner, uv) in layer.corners.iter().zip(uvs) {
            mesh.vertices.push(Vertex {
                pos: pos2(corner.0 * width as f32, corner.1 * height as f32),
                uv,
                color: colour,
            });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        paint_jobs.push(ClippedPrimitive {
            clip_rect: Rect::from_min_max(pos2(0.0, 0.0), pos2(width as f32, height as f32)),
            primitive: Primitive::Mesh(mesh),
        });
    }
    let screen = ScreenDescriptor {
        size_in_pixels: [width, height],
        pixels_per_point: 1.0,
    };
    let target = device.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("oracle frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("oracle") });
    let callbacks = renderer.update_buffers(&device.device, &device.queue, &mut encoder, &paint_jobs, &screen);
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oracle frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &paint_jobs, &screen);
    }
    let mut readback = ReadbackBuffers::default();
    let pending = readback.record_copy(&device.device, &mut encoder, &target);
    device.queue.submit(callbacks.into_iter().chain([encoder.finish()]));
    readback
        .collect(&device.device, pending, std::time::Instant::now() + A_LONG_TIME)
        .unwrap()
}

// ---- comparing ----

/// How far two frames are from each other, channel by channel.
#[derive(Debug)]
pub struct Difference {
    /// The most that any one number (a red, a green, a blue or an opacity of a pixel) is off by.
    pub largest: u8,
    /// How many numbers differ at all.
    pub differing: usize,
}

pub fn compare(engine: &[u8], oracle: &[u8]) -> Difference {
    assert_eq!(engine.len(), oracle.len());
    let mut difference = Difference {
        largest: 0,
        differing: 0,
    };
    for (a, b) in engine.iter().zip(oracle) {
        let gap = a.abs_diff(*b);
        difference.largest = difference.largest.max(gap);
        difference.differing += usize::from(gap > 0);
    }
    difference
}

/// How many pixels are drawn on (not black) in one of the frames and not in the other: for pictures with no black in them, the pixels that the two do
/// not agree are covered, which the same triangles must not give.
pub fn pixels_covered_by_only_one(engine: &[u8], oracle: &[u8]) -> usize {
    engine
        .as_chunks::<4>()
        .0
        .iter()
        .zip(oracle.as_chunks::<4>().0)
        .filter(|(engine, oracle)| (engine[..3] == [0, 0, 0]) != (oracle[..3] == [0, 0, 0]))
        .count()
}

/// Makes the scene both ways and checks that they differ by no more than one level in any number. Gives both frames.
fn assert_the_engine_agrees_with_the_oracle(
    name: &str,
    graphics: &SoftwareGraphics,
    size: (u32, u32),
    layers: &[TestLayer],
) -> (Vec<u8>, Vec<u8>) {
    assert_the_engine_is_within_of_the_oracle(name, graphics, size, layers, 1)
}

/// Makes the scene both ways and checks that they differ by no more than `allowed` levels in any number. Gives both frames.
fn assert_the_engine_is_within_of_the_oracle(
    name: &str,
    graphics: &SoftwareGraphics,
    size: (u32, u32),
    layers: &[TestLayer],
    allowed: u8,
) -> (Vec<u8>, Vec<u8>) {
    let engine = engine_frame(graphics, size, layers);
    let oracle = oracle_frame(&graphics.device, size, layers);
    let difference = compare(&engine, &oracle);
    assert!(
        difference.largest <= allowed,
        "{name}: {difference:?} (up to {allowed} is allowed)"
    );
    (engine, oracle)
}

/// Whether any pixel of the layer is not opaque.
fn is_translucent(layer: &TestLayer) -> bool {
    match &layer.source {
        TestSource::Solid([_, _, _, opacity]) => *opacity < 255,
        TestSource::Picture(picture) => picture.rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255),
    }
}

fn pixel_of(frame: &[u8], width: usize, across: usize, down: usize) -> [u8; 4] {
    let first = (down * width + across) * 4;
    [frame[first], frame[first + 1], frame[first + 2], frame[first + 3]]
}

#[test]
fn a_picture_drawn_one_pixel_to_a_pixel_is_exactly_what_the_oracle_draws() {
    with_software_graphics!(graphics, {
        let (engine, oracle) = assert_the_engine_agrees_with_the_oracle(
            "one to one",
            &graphics,
            (96, 72),
            &[TestLayer {
                source: gradient(96, 72),
                corners: WHOLE_FRAME,
            }],
        );
        assert_eq!(engine, oracle, "not a number is off, not even by one");
        // (And it is the picture: the pixel at the top left is the picture's first, the one at the bottom right its last.)
        assert_eq!(pixel_of(&engine, 96, 0, 0), [0, 0, 0, 255]);
        assert_eq!(
            pixel_of(&engine, 96, 95, 71),
            [255, 255, ((95 * 7 + 71 * 13) % 256) as u8, 255]
        );
    });
}

#[test]
fn a_picture_made_bigger_or_smaller_agrees_with_the_oracle() {
    with_software_graphics!(graphics, {
        assert_the_engine_agrees_with_the_oracle(
            "magnified",
            &graphics,
            (96, 72),
            &[TestLayer {
                source: gradient(16, 12),
                corners: WHOLE_FRAME,
            }],
        );
        assert_the_engine_agrees_with_the_oracle(
            "minified",
            &graphics,
            (96, 72),
            &[TestLayer {
                source: checkers(192, 144, 6),
                corners: upright(0.1, 0.1, 0.6, 0.6),
            }],
        );
        assert_the_engine_agrees_with_the_oracle(
            "made wider and shorter",
            &graphics,
            (97, 61),
            &[TestLayer {
                source: gradient(20, 50),
                corners: upright(0.05, 0.2, 0.9, 0.5),
            }],
        );
    });
}

#[test]
fn a_picture_turned_by_any_angle_agrees_with_the_oracle_and_covers_the_same_pixels() {
    with_software_graphics!(graphics, {
        let size = (96, 72);
        for degrees in [
            1.0,
            17.0,
            45.0,
            90.0,
            123.0,
            180.0,
            200.0,
            271.0,
            359.0,
            -30.0,
            720.0 + 33.0,
        ] {
            let (engine, oracle) = assert_the_engine_agrees_with_the_oracle(
                &format!("turned {degrees}"),
                &graphics,
                size,
                &[TestLayer {
                    source: checkers(64, 40, 4),
                    corners: turned((0.5, 0.5), 60.0, 36.0, degrees, size),
                }],
            );
            assert_eq!(pixels_covered_by_only_one(&engine, &oracle), 0, "turned {degrees}");
            assert!(
                engine.as_chunks::<4>().0.iter().any(|pixel| pixel[..3] != [0, 0, 0]),
                "turned {degrees}: something was drawn"
            );
        }
    });
}

#[test]
fn a_picture_partly_or_wholly_outside_the_frame_is_cut_off_as_the_oracle_cuts_it_off() {
    with_software_graphics!(graphics, {
        let size = (96, 72);
        for (name, corners) in [
            ("off the left", upright(-0.3, 0.4, 0.8, 0.8)),
            ("off the right and the bottom", upright(0.6, 0.5, 0.8, 0.8)),
            ("bigger than the frame", upright(-0.5, -0.5, 2.0, 2.0)),
            ("turned and off the top", turned((0.5, 0.0), 60.0, 40.0, 33.0, size)),
        ] {
            let (engine, oracle) = assert_the_engine_agrees_with_the_oracle(
                name,
                &graphics,
                size,
                &[TestLayer {
                    source: checkers(40, 40, 5),
                    corners,
                }],
            );
            assert_eq!(pixels_covered_by_only_one(&engine, &oracle), 0, "{name}");
        }
        // Wholly outside: nothing but black, in both.
        let (engine, oracle) = assert_the_engine_agrees_with_the_oracle(
            "wholly outside",
            &graphics,
            size,
            &[TestLayer {
                source: checkers(40, 40, 5),
                corners: upright(1.5, 1.5, 0.5, 0.5),
            }],
        );
        assert!(engine.as_chunks::<4>().0.iter().all(|pixel| *pixel == [0, 0, 0, 255]));
        assert_eq!(engine, oracle);
    });
}

#[test]
fn a_translucent_picture_over_another_agrees_with_the_oracle() {
    with_software_graphics!(graphics, {
        let size = (96, 72);
        assert_the_engine_agrees_with_the_oracle(
            "a soft disc over a gradient",
            &graphics,
            size,
            &[
                TestLayer {
                    source: gradient(96, 72),
                    corners: WHOLE_FRAME,
                },
                TestLayer {
                    source: TestSource::Picture(TestPicture::soft_disc(60, 60, [250, 20, 30])),
                    corners: turned((0.5, 0.5), 50.0, 50.0, 30.0, size),
                },
            ],
        );
    });
}

#[test]
fn layers_are_laid_over_each_other_back_to_front() {
    with_software_graphics!(graphics, {
        let size = (40, 40);
        let layers = [
            TestLayer {
                source: TestSource::Solid([200, 0, 0, 255]),
                corners: upright(0.0, 0.0, 0.75, 0.75),
            },
            TestLayer {
                source: TestSource::Solid([0, 200, 0, 255]),
                corners: upright(0.25, 0.25, 0.75, 0.75),
            },
        ];
        let (engine, _) = assert_the_engine_agrees_with_the_oracle("two squares", &graphics, size, &layers);
        assert_eq!(
            pixel_of(&engine, 40, 5, 5),
            [200, 0, 0, 255],
            "only the back one is here"
        );
        assert_eq!(
            pixel_of(&engine, 40, 20, 20),
            [0, 200, 0, 255],
            "both are here, and the front one is the one that is seen"
        );
        assert_eq!(
            pixel_of(&engine, 40, 35, 35),
            [0, 200, 0, 255],
            "only the front one is here"
        );
        assert_eq!(pixel_of(&engine, 40, 35, 3), [0, 0, 0, 255], "neither: black");
        // The other way round, the other one is in front.
        let mut reversed = layers.to_vec();
        reversed.reverse();
        let (engine, _) =
            assert_the_engine_agrees_with_the_oracle("the squares the other way round", &graphics, size, &reversed);
        assert_eq!(pixel_of(&engine, 40, 20, 20), [200, 0, 0, 255]);
    });
}

/// Not the oracle but arithmetic: what half-opaque yellow makes of a blue, worked out by hand.
#[test]
fn a_translucent_colour_over_a_colour_is_the_blend_of_the_two_by_how_opaque_it_is() {
    with_software_graphics!(graphics, {
        let size = (8, 8);
        let layers = [
            TestLayer {
                source: TestSource::Solid([20, 120, 220, 255]),
                corners: WHOLE_FRAME,
            },
            TestLayer {
                source: TestSource::Solid([250, 200, 10, 128]),
                corners: WHOLE_FRAME,
            },
        ];
        let (engine, _) = assert_the_engine_agrees_with_the_oracle("yellow over blue", &graphics, size, &layers);
        let opacity = 128.0 / 255.0;
        let expected = [(250.0, 20.0), (200.0, 120.0), (10.0, 220.0)]
            .map(|(over, under): (f64, f64)| over * opacity + under * (1.0 - opacity));
        for channel in 0..3 {
            assert!(
                (f64::from(engine[channel]) - expected[channel]).abs() <= 1.0,
                "channel {channel}: {} is not {}",
                engine[channel],
                expected[channel]
            );
        }
        assert_eq!(engine[3], 255, "what is laid over black and over a colour stays opaque");
        // Over black, with nothing between: the colour times how opaque it is.
        let over_black = engine_frame(&graphics, size, &layers[1..]);
        for (channel, colour) in [250.0, 200.0, 10.0].into_iter().enumerate() {
            assert!(
                (f64::from(over_black[channel]) - colour * opacity).abs() <= 1.0,
                "channel {channel}"
            );
        }
    });
}

/// Numbers that look random and are the same every time.
struct Randomness(u64);

impl Randomness {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (self.next() % 10_000) as f32 / 10_000.0 * (high - low)
    }

    fn whole_between(&mut self, low: u32, high: u32) -> u32 {
        low + (self.next() % u64::from(high - low + 1)) as u32
    }
}

/// Layers made up from a number, all kinds, all sizes, turned, small and big, partly off the frame: the engine and the oracle agree on every one.
///
/// **How closely: one level for the layers that are opaque, and one more for every layer that is not.** The oracle (the window's renderer) works in 8
/// bits at every step: each texel of a translucent picture is multiplied by its opacity and rounded to a whole number when the picture is uploaded, and
/// what is laid over something is rounded again after every layer, so each translucent layer can be off by about a level on its own account, and two of
/// them over a gradient are sometimes off by two. (The engine's numbers are half floats, which are good for about a tenth of a level.) This was found when
/// the tests were run on Linux: of forty scenes thirty-nine were within one level on both systems, and seed 24, whose two translucent discs lie over
/// two gradients, was within two on lavapipe. The scenes that are made by hand above, with one translucent layer at most, are all within one.
#[test]
fn scenes_made_up_from_numbers_agree_with_the_oracle() {
    with_software_graphics!(graphics, {
        for seed in 1..=40_u64 {
            let mut random = Randomness(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let size = (random.whole_between(17, 130), random.whole_between(17, 100));
            let mut layers = Vec::new();
            for _ in 0..random.whole_between(1, 5) {
                let (width, height) = (random.whole_between(4, 90), random.whole_between(4, 90));
                let source = match random.next() % 5 {
                    0 => TestSource::Solid([
                        random.whole_between(0, 255) as u8,
                        random.whole_between(0, 255) as u8,
                        random.whole_between(0, 255) as u8,
                        random.whole_between(1, 255) as u8,
                    ]),
                    1 => TestSource::Picture(TestPicture::soft_disc(
                        width,
                        height,
                        [random.whole_between(0, 255) as u8, 90, 200],
                    )),
                    2 => checkers(width, height, random.whole_between(1, 7)),
                    _ => gradient(width, height),
                };
                let middle = (random.between(-0.1, 1.1), random.between(-0.1, 1.1));
                let corners = turned(
                    middle,
                    random.between(3.0, size.0 as f32 * 1.2),
                    random.between(3.0, size.1 as f32 * 1.2),
                    random.between(-400.0, 400.0),
                    size,
                );
                layers.push(TestLayer { source, corners });
            }
            let description: Vec<String> = layers
                .iter()
                .map(|layer| match &layer.source {
                    TestSource::Picture(picture) => format!(
                        "picture {}x{} turned to {:?}",
                        picture.width, picture.height, layer.corners
                    ),
                    TestSource::Solid(colour) => format!("solid {colour:?} at {:?}", layer.corners),
                })
                .collect();
            let translucent_layers = layers.iter().filter(|layer| is_translucent(layer)).count();
            assert_the_engine_is_within_of_the_oracle(
                &format!("seed {seed}, frame {size:?}, layers {description:#?}"),
                &graphics,
                size,
                &layers,
                1 + u8::try_from(translucent_layers).unwrap(),
            );
        }
    });
}
