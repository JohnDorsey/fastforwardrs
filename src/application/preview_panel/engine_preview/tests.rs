// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview's side of the engine, against a real engine on the software adapter: frames are asked for when the content changes and not when it does not,
//! pictures are sent once, the engine's complaints are answered, and old pictures are let go of.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Context, vec2};

use super::*;
use crate::application::preview_panel::test_content::{clip, in_the_middle, layer, whole_picture};
use crate::application::preview_panel::{PreviewLayer, PreviewLayerContent, VideoInThePicture};
use crate::application::theme;
use crate::colour_effects::ColourMatrix;
use crate::engine::EngineRequest;
use crate::engine::testing::{DevicesMade, software_graphics};
use crate::media::DecodedFrame;
use crate::playback::{SuppliedFrame, SuppliedPicture};
use crate::project::{FrameRate, SourceFrameIndex, SourceMediaIdentifier, SpeedIndicatorLayout};

const RENDER_SIZE: (u32, u32) = (64, 36);
const A_LONG_TIME: Duration = Duration::from_secs(30);

/// The clock of a project that is not playing (which the quality of the preview leaves alone, except that a picture that has been still for a while is made whole),
/// at this moment.
fn the_clock_is_stopped() -> HowTheClockGoes {
    HowTheClockGoes {
        is_playing: false,
        how_long_a_frame_lasts: Duration::from_millis(33),
        now: Instant::now(),
    }
}

/// A preview of these layers on a picture of 1600 by 900 pixels.
fn content<'a>(layers: Vec<PreviewLayer<'a>>) -> PreviewContent<'a> {
    PreviewContent {
        picture_size: Some((1600, 900)),
        layers,
        symbol: None,
        groups: Vec::new(),
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
        selected_clip: None,
    }
}

/// A picture of one flat colour, as the supplier has it, with this serial number.
fn flat_picture(serial_number: u64, colour: [u8; 4]) -> SuppliedPicture {
    SuppliedPicture {
        chroma_siting: crate::colour_space::ChromaSiting::Left,
        colour: crate::engine::PictureColour::SRGB,
        source: SourceMediaIdentifier::new(1),
        source_frame: SourceFrameIndex::new(0),
        frame: SuppliedFrame::Rgba(DecodedFrame {
            width_in_pixels: 4,
            height_in_pixels: 4,
            rgba_pixels: colour.repeat(16),
        }),
        serial_number,
    }
}

fn showing<'a>(number: u32, picture: &'a SuppliedPicture) -> PreviewLayer<'a> {
    PreviewLayer {
        content: PreviewLayerContent::Video(VideoInThePicture::Picture(picture)),
        ..layer(number, whole_picture())
    }
}

/// An engine on the software adapter and what is needed to use it, or nothing, on a computer that has no software adapter.
struct Setup {
    _graphics: crate::engine::testing::SoftwareGraphics,
    context: Context,
    preview: EnginePreview,
    text_renderer: LazyTextRenderer,
    /// Whether the project is playing, and how long a frame of it lasts, as the preview is told when it is drawn.
    is_playing: bool,
    how_long_a_frame_lasts: Duration,
}

fn setup() -> Option<Setup> {
    setup_with(EngineSettings::patient(AdapterChoice::SoftwareOnly))
}

/// The same, with an engine that has these settings.
fn setup_with(settings: EngineSettings) -> Option<Setup> {
    let graphics = software_graphics()?;
    Some(Setup {
        _graphics: graphics,
        context: Context::default(),
        preview: EnginePreview::with_engine(Engine::start(settings, Arc::new(|| {}))),
        text_renderer: LazyTextRenderer::new(Arc::new(|| {})),
        is_playing: false,
        how_long_a_frame_lasts: Duration::from_millis(33),
    })
}

impl Setup {
    fn update(&mut self, content: &PreviewContent<'_>) {
        self.update_with(content, PictureFilter::Smooth);
    }

    fn update_with(&mut self, content: &PreviewContent<'_>, filter: PictureFilter) {
        let clock = HowTheClockGoes {
            is_playing: self.is_playing,
            how_long_a_frame_lasts: self.how_long_a_frame_lasts,
            now: Instant::now(),
        };
        self.preview.update(
            &self.context,
            content,
            RENDER_SIZE,
            filter,
            &mut self.text_renderer,
            clock,
        );
    }

    /// Updates with `content` until `done`, or fails after a while.
    fn update_until(&mut self, content: &PreviewContent<'_>, what: &str, done: impl Fn(&EnginePreview) -> bool) {
        let started = Instant::now();
        loop {
            self.update(content);
            if done(&self.preview) {
                return;
            }
            assert!(started.elapsed() < A_LONG_TIME, "it did not happen: {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn show_until_a_frame_is_there(&mut self, content: &PreviewContent<'_>) {
        self.update_until(content, "a frame is shown", EnginePreview::has_a_frame_to_show);
    }

    /// Updates with `content` until the frame that is shown is not the one that was before (by its serial number) and is the answer to the last frame that was
    /// asked for, and gives it. (The last one: the preview may have asked for a frame more than once, for instance when the engine says the device was replaced,
    /// and an answer to an earlier request that comes late is not the answer to what is asked for now.)
    fn show_a_new_frame_of(&mut self, content: &PreviewContent<'_>) -> RenderedFrame {
        let before = self.preview.shown.as_ref().map(|shown| shown.serial);
        self.update_until(content, "a new frame is shown", |preview| {
            preview.shown.as_ref().map(|shown| shown.serial) != before && preview.has_shown_the_last_frame_asked_for()
        });
        self.preview.the_frame_shown().unwrap().clone()
    }
}

fn pixel_of(frame: &RenderedFrame, across: u32, down: u32) -> [u8; 4] {
    frame.pixel(across, down)
}

#[test]
fn the_clock_of_a_project_lasts_a_frame_by_its_frame_rate_and_says_whether_it_plays_and_when() {
    let now = Instant::now();
    let at_sixty = HowTheClockGoes::new(true, FrameRate::from_whole_frames_per_second(60).unwrap(), now);
    assert!(at_sixty.is_playing);
    assert_eq!(at_sixty.now, now);
    assert!(
        at_sixty
            .how_long_a_frame_lasts
            .abs_diff(Duration::from_secs_f64(1.0 / 60.0))
            < Duration::from_micros(1),
        "{:?}",
        at_sixty.how_long_a_frame_lasts
    );
    let at_ntsc = HowTheClockGoes::new(false, FrameRate::new(30000, 1001).unwrap(), now);
    assert!(!at_ntsc.is_playing);
    assert!(
        at_ntsc
            .how_long_a_frame_lasts
            .abs_diff(Duration::from_secs_f64(1001.0 / 30000.0))
            < Duration::from_micros(1),
        "a frame of 29.97 to the second lasts 33.37 milliseconds: {:?}",
        at_ntsc.how_long_a_frame_lasts
    );
    // A million frames to the second is a frame of a millionth of a second, and a frame rate so high that a frame is less than a billionth of one lasts no
    // time that can be told, which is what the quality of the preview leaves alone.
    let a_million = HowTheClockGoes::new(true, FrameRate::new(1_000_000, 1).unwrap(), now);
    assert_eq!(a_million.how_long_a_frame_lasts, Duration::from_micros(1));
    let as_fast_as_can_be = HowTheClockGoes::new(true, FrameRate::new(u32::MAX, 1).unwrap(), now);
    assert_eq!(as_fast_as_can_be.how_long_a_frame_lasts, Duration::ZERO);
}

#[test]
fn the_size_of_the_frame_is_the_size_the_picture_is_shown_at_but_not_more_than_the_picture_has() {
    // 1600 by 900 pixels, shown on a screen of 1 pixel to the point.
    assert_eq!(render_size_for(vec2(800.0, 450.0), 1.0, (1600, 900)), (800, 450));
    assert_eq!(render_size_for(vec2(1600.0, 900.0), 1.0, (1600, 900)), (1600, 900));
    assert_eq!(
        render_size_for(vec2(3200.0, 1800.0), 1.0, (1600, 900)),
        (1600, 900),
        "shown bigger than it is: the window makes the pixels bigger"
    );
    // On a screen of 2 pixels to the point, the same 800 by 450 points are 1600 by 900 pixels.
    assert_eq!(render_size_for(vec2(800.0, 450.0), 2.0, (1600, 900)), (1600, 900));
    assert_eq!(render_size_for(vec2(400.0, 225.0), 1.25, (1600, 900)), (500, 281));
    // The shape of the picture is kept, whatever is shown, and the frame is never smaller than two pixels.
    assert_eq!(render_size_for(vec2(100.0, 9999.0), 1.0, (1000, 500)), (100, 50));
    assert_eq!(render_size_for(vec2(0.0, 0.0), 1.0, (1600, 900)), (2, 2));
    assert_eq!(
        render_size_for(vec2(0.0, 0.0), 1.0, (100, 1000)),
        (2, 10),
        "a shown width of nothing counts as one pixel, which is a hundredth of this picture, and its height is a hundredth too"
    );
    assert_eq!(
        render_size_for(vec2(10.0, 10.0), 1.0, (0, 0)),
        (2, 2),
        "a picture of no size does not break it"
    );
}

#[test]
fn a_frame_is_asked_for_made_and_shown_and_it_is_the_layers_over_black() {
    let Some(mut setup) = setup() else { return };
    let waiting = content(vec![layer(0, in_the_middle())]);
    setup.show_until_a_frame_is_there(&waiting);

    let frame = setup.preview.the_frame_shown().unwrap();
    assert_eq!((frame.width_in_pixels, frame.height_in_pixels), RENDER_SIZE);
    let colour = theme::TRACK_BACKGROUND.to_srgba_unmultiplied();
    assert_eq!(
        pixel_of(frame, 32, 18),
        [colour[0], colour[1], colour[2], 255],
        "the middle: the layer"
    );
    assert_eq!(
        pixel_of(frame, 2, 2),
        [0, 0, 0, 255],
        "a corner: the picture, which is black"
    );
    assert_eq!(setup.preview.frames_asked_for(), 1);
    assert_eq!(setup.preview.why_it_stopped(), None);
}

#[test]
fn the_same_content_is_not_asked_for_again_however_often_the_window_is_drawn() {
    let Some(mut setup) = setup() else { return };
    let picture = flat_picture(1, [200, 100, 50, 255]);
    let shown = content(vec![showing(0, &picture)]);
    setup.show_until_a_frame_is_there(&shown);
    for _ in 0..50 {
        setup.update(&shown);
    }
    assert_eq!(setup.preview.frames_asked_for(), 1);
    assert_eq!(
        setup.preview.pictures_the_engine_has(),
        1,
        "and the picture was sent once"
    );
    assert_eq!(
        pixel_of(setup.preview.the_frame_shown().unwrap(), 10, 10),
        [200, 100, 50, 255]
    );
}

#[test]
fn other_content_is_asked_for_and_the_frame_that_is_shown_becomes_the_new_one() {
    let Some(mut setup) = setup() else { return };
    let (first, second) = (
        flat_picture(1, [200, 100, 50, 255]),
        flat_picture(2, [10, 220, 90, 255]),
    );
    setup.show_until_a_frame_is_there(&content(vec![showing(0, &first)]));
    assert_eq!(
        pixel_of(setup.preview.the_frame_shown().unwrap(), 10, 10),
        [200, 100, 50, 255]
    );

    let frame = setup.show_a_new_frame_of(&content(vec![showing(0, &second)]));
    assert_eq!(pixel_of(&frame, 10, 10), [10, 220, 90, 255]);
    assert_eq!(setup.preview.frames_asked_for(), 2);

    // The colours of the layer are part of what is asked for.
    let mut turned = showing(0, &second);
    turned.operations = PreviewLayer::operations_of_a_colour(ColourMatrix::from_rows([
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ]));
    let frame = setup.show_a_new_frame_of(&content(vec![turned]));
    assert_eq!(pixel_of(&frame, 10, 10), [90, 220, 10, 255]);
    assert_eq!(setup.preview.frames_asked_for(), 3);
}

#[test]
fn the_pictures_of_a_video_that_are_not_used_any_more_are_let_go_of_so_that_they_do_not_pile_up() {
    let Some(mut setup) = setup() else { return };
    // A new picture of a video for every window frame, as when it plays.
    for serial in 1..=120u64 {
        let picture = flat_picture(serial, [(serial * 2) as u8, 100, 50, 255]);
        setup.update(&content(vec![showing(0, &picture)]));
    }
    assert!(
        setup.preview.pictures_the_engine_has() <= MOST_PICTURES_KEPT_WITHOUT_USE + 1,
        "{} pictures are kept",
        setup.preview.pictures_the_engine_has()
    );
    // The engine is still showing what it was last asked for: the picture of the last serial number.
    let last = flat_picture(121, [240, 100, 50, 255]);
    let frame = setup.show_a_new_frame_of(&content(vec![showing(0, &last)]));
    assert_eq!(pixel_of(&frame, 10, 10), [240, 100, 50, 255]);
}

#[test]
fn a_picture_that_the_engine_lost_is_sent_again_and_the_frame_comes_all_the_same() {
    let Some(mut setup) = setup() else { return };
    let picture = flat_picture(7, [200, 100, 50, 255]);
    setup.show_until_a_frame_is_there(&content(vec![showing(0, &picture)]));

    // The engine lets go of the picture (as it does when it has too many), and the window does not know.
    setup.preview.engine.forget_pictures(vec![PictureId(7)]);
    // Something else changes, so that the frame is asked for again, and it uses the picture the engine has not got.
    let with_a_second_layer = content(vec![showing(0, &picture), layer(1, in_the_middle())]);
    let frame = setup.show_a_new_frame_of(&with_a_second_layer);

    let colour = theme::TRACK_BACKGROUND.to_srgba_unmultiplied();
    assert_eq!(
        pixel_of(&frame, 2, 2),
        [200, 100, 50, 255],
        "the picture is there again"
    );
    assert_eq!(pixel_of(&frame, 32, 18), [colour[0], colour[1], colour[2], 255]);
    assert!(setup.preview.sent.contains(&PictureId(7)));
}

#[test]
fn a_picture_that_the_engine_cannot_use_is_left_out_and_not_sent_again_and_the_other_layers_are_shown() {
    let Some(mut setup) = setup() else { return };
    // Too wide for any graphics card.
    let too_wide = SuppliedPicture {
        frame: SuppliedFrame::Rgba(DecodedFrame {
            width_in_pixels: 40_000,
            height_in_pixels: 1,
            rgba_pixels: vec![255; 40_000 * 4],
        }),
        ..flat_picture(3, [0, 0, 0, 255])
    };
    let fine = flat_picture(4, [10, 200, 30, 255]);
    let shown = content(vec![showing(0, &too_wide), showing(1, &fine)]);
    setup.update_until(&shown, "the engine says it cannot use the picture", |preview| {
        preview.last_problem().is_some()
    });
    setup.update_until(
        &shown,
        "the frame without it is shown",
        EnginePreview::has_a_frame_to_show,
    );

    assert_eq!(
        pixel_of(setup.preview.the_frame_shown().unwrap(), 10, 10),
        [10, 200, 30, 255],
        "the one that can be used is"
    );
    assert!(setup.preview.rejected.contains(&PictureId(3)));
    assert!(!setup.preview.sent.contains(&PictureId(3)));
    // It is not asked for again and again.
    let asked = setup.preview.frames_asked_for();
    for _ in 0..30 {
        setup.update(&shown);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(setup.preview.frames_asked_for(), asked);
}

#[test]
fn after_the_window_lets_go_of_its_files_the_frame_is_asked_for_again_and_frames_of_before_are_not_shown() {
    let Some(mut setup) = setup() else { return };
    let picture = flat_picture(1, [200, 100, 50, 255]);
    let shown = content(vec![showing(0, &picture)]);
    setup.show_until_a_frame_is_there(&shown);
    assert_eq!(setup.preview.frames_asked_for(), 1);

    setup.preview.forget_what_is_known_of_the_files();
    setup.update(&shown);
    assert_eq!(
        setup.preview.frames_asked_for(),
        2,
        "the same content is asked for again"
    );
    assert!(
        setup.preview.has_a_frame_to_show(),
        "and what is shown stays until the new frame comes, so that nothing flashes"
    );

    // A frame of a request from before is not shown, even if it comes later than one that is: the window has let go of what it was made from.
    let old_serial = 1;
    let before = setup.preview.shown.as_ref().unwrap().serial;
    setup.preview.show(
        &setup.context.clone(),
        old_serial,
        Arc::new(RenderedFrame {
            width_in_pixels: 2,
            height_in_pixels: 2,
            rgba_pixels: [9, 9, 9, 255].repeat(4),
        }),
        PictureFilter::Smooth,
        false,
    );
    assert_eq!(setup.preview.shown.as_ref().unwrap().serial, before);
    let frame = setup.preview.the_frame_shown().unwrap();
    assert_eq!(
        (frame.width_in_pixels, frame.height_in_pixels),
        RENDER_SIZE,
        "the frame that is shown is not the old one that came late"
    );
    assert_eq!(pixel_of(frame, 10, 10), [200, 100, 50, 255]);
}

#[test]
fn the_way_the_frame_is_put_on_the_screen_follows_the_zoom_without_asking_for_the_frame_again() {
    let Some(mut setup) = setup() else { return };
    let picture = flat_picture(1, [200, 100, 50, 255]);
    let shown = content(vec![showing(0, &picture)]);
    setup.show_until_a_frame_is_there(&shown);
    assert_eq!(setup.preview.shown.as_ref().unwrap().filter, PictureFilter::Smooth);

    // What is given to the graphics card, which is what decides how the picture is put on the screen: the changes of the textures since the last time.
    let last_options_given_to_the_graphics_card = |setup: &Setup| {
        let mut changes = setup.context.tex_manager().write().take_delta();
        let options = changes
            .set
            .values()
            .flat_map(|deltas| deltas.iter().map(|delta| delta.options))
            .last();
        changes.clear();
        options
    };
    assert_eq!(
        last_options_given_to_the_graphics_card(&setup),
        Some(PictureFilter::Smooth.options()),
        "the first frame"
    );

    setup.update_with(&shown, PictureFilter::Pixels);
    assert_eq!(setup.preview.shown.as_ref().unwrap().filter, PictureFilter::Pixels);
    assert_eq!(
        last_options_given_to_the_graphics_card(&setup),
        Some(PictureFilter::Pixels.options()),
        "put on the graphics card again the other way"
    );
    setup.update_with(&shown, PictureFilter::Pixels);
    assert_eq!(
        last_options_given_to_the_graphics_card(&setup),
        None,
        "and not again while it stays that way"
    );
    setup.update_with(&shown, PictureFilter::Smooth);
    assert_eq!(setup.preview.shown.as_ref().unwrap().filter, PictureFilter::Smooth);
    assert_eq!(
        last_options_given_to_the_graphics_card(&setup),
        Some(PictureFilter::Smooth.options())
    );
    assert_eq!(setup.preview.frames_asked_for(), 1);
}

#[test]
fn an_engine_that_stops_is_noticed_and_asks_for_nothing_more() {
    let Some(mut setup) = setup() else { return };
    let waiting = content(vec![layer(0, whole_picture())]);
    setup.show_until_a_frame_is_there(&waiting);
    setup.preview.stop();
    setup.update_until(&waiting, "the stop is noticed", |preview| {
        preview.why_it_stopped().is_some()
    });
    assert!(
        setup.preview.why_it_stopped().unwrap().contains("stop"),
        "{:?}",
        setup.preview.why_it_stopped()
    );

    let asked = setup.preview.frames_asked_for();
    let other = content(vec![layer(0, in_the_middle())]);
    for _ in 0..5 {
        setup.update(&other);
    }
    assert_eq!(setup.preview.frames_asked_for(), asked, "a stopped engine is not asked");
}

#[test]
fn an_engine_that_cannot_start_says_so() {
    let Some(graphics) = software_graphics() else { return };
    let mut preview = EnginePreview::start(AdapterChoice::NoAdapterAtAll, Arc::new(|| {}));
    let context = Context::default();
    let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
    let started = Instant::now();
    while preview.why_it_stopped().is_none() {
        preview.update(
            &context,
            &content(vec![layer(0, whole_picture())]),
            RENDER_SIZE,
            PictureFilter::Smooth,
            &mut text_renderer,
            the_clock_is_stopped(),
        );
        assert!(
            started.elapsed() < A_LONG_TIME,
            "the engine never said it could not start"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    drop(graphics);
    assert!(!preview.has_a_frame_to_show());
    let _ = clip(0);
}

#[test]
fn when_the_graphics_device_is_replaced_the_preview_sends_its_pictures_again_without_being_asked_to_and_shows_what_it_asked_for()
 {
    let devices = DevicesMade::default();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.watch_the_devices = Some(devices.watcher());
    let Some(mut setup) = setup_with(settings) else { return };

    let red = flat_picture(1, [200, 20, 20, 255]);
    let first = content(vec![showing(1, &red)]);
    setup.show_until_a_frame_is_there(&first);
    assert_eq!(
        pixel_of(setup.preview.the_frame_shown().unwrap(), 10, 10),
        [200, 20, 20, 255]
    );

    // The card is reset. Nothing has changed in what is shown, so the preview asks for nothing, and does not know. When something does change, the engine finds
    // out when it is asked, replaces the device, and says so.
    devices.nth(1, A_LONG_TIME).destroy();
    let green = flat_picture(2, [20, 200, 20, 255]);
    let second = content(vec![showing(1, &green)]);
    let frame = setup.show_a_new_frame_of(&second);
    assert_eq!(
        pixel_of(&frame, 10, 10),
        [20, 200, 20, 255],
        "the frame is the one that was asked for"
    );
    assert_eq!(devices.count(), 2);
    assert!(
        setup.preview.last_problem().is_some_and(|words| words.contains("lost")),
        "the preview was told why: {:?}",
        setup.preview.last_problem()
    );
    assert_eq!(
        setup.preview.why_it_stopped(),
        None,
        "a device that is replaced is not an engine that stopped"
    );
    let to_tell = setup.preview.take_what_the_user_is_to_be_told();
    assert_eq!(to_tell.len(), 1, "the user is to be told once: {to_tell:?}");
    assert!(to_tell[0].contains("lost"), "and why: {}", to_tell[0]);
    assert!(
        setup.preview.take_what_the_user_is_to_be_told().is_empty(),
        "taking what there is to tell empties it"
    );

    // The red picture was on the card that was lost. The preview was told, so it does not think the engine has it: it sends it when a frame uses it, and the
    // frame is made at once (and not first said to be missing a picture that the preview thought the engine had, which would be a frame asked for twice).
    let asked_before = setup.preview.frames_asked_for();
    let frame = setup.show_a_new_frame_of(&first);
    assert_eq!(pixel_of(&frame, 10, 10), [200, 20, 20, 255]);
    assert_eq!(setup.preview.frames_asked_for(), asked_before + 1);
}

#[test]
fn an_effect_that_the_graphics_library_will_not_take_is_noted_as_the_last_problem_and_the_preview_goes_on() {
    let Some(mut setup) = setup() else { return };
    // (The guard takes it: it is a shader that reads, and means something. It is the library that will not take a list of numbers that is bigger than an effect
    // is given: 72 numbers, one more than the 68 of a pass since phase 6 (it asked for 68 before, and was rewritten on purpose).)
    setup
        .preview
        .engine
        .compile_effect_shader(
            "test:too_many_numbers",
            "@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 18>;
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(numbers[17].x, 0.0, 0.0, 1.0);
}
",
        )
        .expect("the guard takes it");
    let waiting = content(vec![layer(0, whole_picture())]);
    setup.update_until(&waiting, "the engine's complaint is noted", |preview| {
        preview
            .last_problem()
            .is_some_and(|words| words.contains("would not take it"))
    });
    assert_eq!(
        setup.preview.why_it_stopped(),
        None,
        "a shader that is refused is not an engine that stopped"
    );
    setup.show_until_a_frame_is_there(&waiting);
}

/// What the preview asked a scripted engine for since this was last looked at: the ids of the pictures it sent, and the frames it asked for with their serial
/// numbers.
fn what_was_asked(requests: &Receiver<EngineRequest>) -> (Vec<PictureId>, Vec<(u64, FrameDescription)>) {
    let (pictures, frames, forgotten) = what_was_asked_and_what_was_forgotten(requests);
    assert_eq!(forgotten, Vec::new(), "the engine was told to let go of pictures");
    (pictures, frames)
}

/// Like [`what_was_asked`], and also the ids of the pictures that the engine was told to let go of.
fn what_was_asked_and_what_was_forgotten(
    requests: &Receiver<EngineRequest>,
) -> (Vec<PictureId>, Vec<(u64, FrameDescription)>, Vec<PictureId>) {
    let (mut pictures, mut frames, mut forgotten) = (Vec::new(), Vec::new(), Vec::new());
    while let Ok(request) = requests.try_recv() {
        match request {
            EngineRequest::Upload { id, .. } => pictures.push(id),
            EngineRequest::Render { serial, description } => frames.push((serial, description)),
            EngineRequest::Forget(ids) => forgotten.extend(ids),
            // (Rewritten on purpose in EN-14: what is said about the checkpoints has tests of its own, which read it with `what_was_said_of_the_checkpoints`; here it is not what is asked
            // for or let go of.)
            EngineRequest::SetTheRulesOfCheckpoints(_) | EngineRequest::ForgetTheCheckpoints => {}
            other => panic!("it asked for something else: {other:?}"),
        }
    }
    (pictures, frames, forgotten)
}

#[test]
fn a_frame_that_failed_is_not_asked_for_again_but_when_the_device_was_replaced_it_is_and_the_pictures_are_sent_again() {
    let (engine, events, requests) = Engine::scripted_for_tests();
    let mut preview = EnginePreview::with_engine(engine);
    let context = Context::default();
    let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
    let picture = flat_picture(1, [10, 20, 30, 255]);
    let shown = content(vec![showing(1, &picture)]);
    let mut draw = |preview: &mut EnginePreview| {
        preview.update(
            &context,
            &shown,
            RENDER_SIZE,
            PictureFilter::Smooth,
            &mut text_renderer,
            the_clock_is_stopped(),
        );
    };

    draw(&mut preview);
    let (pictures, frames) = what_was_asked(&requests);
    assert_eq!(pictures.len(), 1, "the picture is sent");
    let [(first_serial, description)] = frames.as_slice() else {
        panic!("{frames:?}")
    };

    // The frame failed (the card did not finish it in time, say). The same frame would fail again, so it is not asked for again, however often the window is drawn.
    events
        .send(EngineEvent::FrameFailed {
            serial: *first_serial,
            problem: "the graphics card did not finish the frame in time".to_owned(),
        })
        .unwrap();
    draw(&mut preview);
    draw(&mut preview);
    assert_eq!(what_was_asked(&requests), (Vec::new(), Vec::new()));
    assert!(
        preview
            .last_problem()
            .is_some_and(|words| words.contains("did not finish")),
        "{:?}",
        preview.last_problem()
    );

    // Then the engine says that it made a new device, and switched an effect off: the pictures it had are gone with the old device, and the frame (which may
    // be made now, without the effect) is asked for again.
    events
        .send(EngineEvent::DeviceReplaced {
            why: "the graphics card did not finish a frame in 2.0 seconds".to_owned(),
            effects_switched_off: vec!["test:slow".to_owned()],
        })
        .unwrap();
    draw(&mut preview);
    let (pictures_again, frames_again) = what_was_asked(&requests);
    assert_eq!(pictures_again, pictures, "the pictures the engine had are sent again");
    assert_eq!(frames_again.len(), 1, "{frames_again:?}");
    assert_eq!(&frames_again[0].1, description, "the same frame is asked for");
    assert!(frames_again[0].0 > *first_serial, "as a new request");
    assert_eq!(preview.why_it_stopped(), None);
    // And it is asked for once, not again at every drawing of the window.
    draw(&mut preview);
    assert_eq!(what_was_asked(&requests), (Vec::new(), Vec::new()));
}

#[test]
fn a_frame_that_was_made_before_comes_from_the_engines_cache_when_the_content_comes_back_to_what_it_was() {
    let Some(mut setup) = setup() else { return };
    let red = flat_picture(1, [200, 20, 20, 255]);
    let green = flat_picture(2, [20, 200, 20, 255]);
    let red_shown = content(vec![showing(1, &red)]);
    let green_shown = content(vec![showing(1, &green)]);
    let first_red = setup.show_a_new_frame_of(&red_shown);
    setup.show_a_new_frame_of(&green_shown);
    assert_eq!(setup.preview.frames_from_the_cache(), 0, "two frames that are new");
    // The playhead goes back and forth over the two: each is made again, and the engine has it.
    let second_red = setup.show_a_new_frame_of(&red_shown);
    assert_eq!(setup.preview.frames_from_the_cache(), 1);
    let second_green = setup.show_a_new_frame_of(&green_shown);
    assert_eq!(setup.preview.frames_from_the_cache(), 2);
    assert_eq!(second_red, first_red, "the same frame");
    assert_eq!(pixel_of(&second_green, 10, 10), [20, 200, 20, 255]);
    assert_eq!(
        setup.preview.frames_asked_for(),
        4,
        "all four were asked for: the window does not remember frames, the engine does"
    );
}

// ---- the size the frames are made at: the quality of the preview ----

/// A frame that the engine made, as an event says it, of this size.
fn a_frame_of_size(serial: u64, size: (u32, u32), came_from_the_cache: bool) -> EngineEvent {
    EngineEvent::Frame {
        serial,
        frame: Arc::new(RenderedFrame {
            width_in_pixels: size.0,
            height_in_pixels: size.1,
            rgba_pixels: [0, 0, 0, 255].repeat((size.0 * size.1) as usize),
        }),
        statistics: crate::engine::FrameStatistics {
            came_from_the_cache,
            ..Default::default()
        },
    }
}

/// What a drawing of the preview did, as far as the engine and the window are concerned.
#[derive(Debug, PartialEq, Eq)]
struct Drawn {
    /// The size of the frame that was asked for, if one was.
    asked_for_a_frame_of: Option<(u32, u32)>,
    /// How long from now the window was asked to be drawn again, each time it was asked to.
    asked_to_be_drawn_again_after: Vec<Duration>,
    /// The pictures that the engine was told to let go of.
    told_to_let_go_of: Vec<PictureId>,
}

/// A preview and a scripted engine, and what it takes to draw it: a picture for each number (each is another picture, so that the content changes), and a way
/// to draw it at a time that is made up, with the clock going or not.
struct Scripted {
    preview: EnginePreview,
    events: std::sync::mpsc::Sender<EngineEvent>,
    requests: Receiver<EngineRequest>,
    /// The window's own context, the same for every drawing (the textures of the frames are made in it and have numbers that go on from one drawing to the next).
    context: Context,
    text_renderer: LazyTextRenderer,
    pictures: Vec<SuppliedPicture>,
    start: Instant,
}

impl Scripted {
    fn new() -> Self {
        let (engine, events, requests) = Engine::scripted_for_tests();
        Self {
            preview: EnginePreview::with_engine(engine),
            events,
            requests,
            context: Context::default(),
            text_renderer: LazyTextRenderer::new(Arc::new(|| {})),
            pictures: (1..=80)
                .map(|serial| flat_picture(serial, [(serial % 40) as u8 * 5, 0, 0, 255]))
                .collect(),
            start: Instant::now(),
        }
    }

    /// Draws the preview of picture `number` `milliseconds` after the start, with the clock going or not, in a window that has not been asked to be drawn
    /// again. A frame of the project lasts 100 milliseconds.
    fn draw(&mut self, number: usize, is_playing: bool, milliseconds: u64) -> Drawn {
        let delays = Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let delays = Arc::clone(&delays);
            self.context
                .set_request_repaint_callback(move |information| delays.lock().unwrap().push(information.delay));
        }
        let shown = content(vec![showing(1, &self.pictures[number])]);
        self.preview.update(
            &self.context,
            &shown,
            RENDER_SIZE,
            PictureFilter::Smooth,
            &mut self.text_renderer,
            HowTheClockGoes {
                is_playing,
                how_long_a_frame_lasts: Duration::from_millis(100),
                now: self.start + Duration::from_millis(milliseconds),
            },
        );
        let (_, frames, mut told_to_let_go_of) = what_was_asked_and_what_was_forgotten(&self.requests);
        told_to_let_go_of.sort();
        assert!(frames.len() <= 1, "{frames:?}");
        let asked_to_be_drawn_again_after = delays.lock().unwrap().clone();
        Drawn {
            asked_for_a_frame_of: frames.first().map(|(_, description)| description.render_size),
            asked_to_be_drawn_again_after,
            told_to_let_go_of,
        }
    }

    /// The size of the frame asked for by a drawing (which must have asked for one).
    fn size_asked_for(&mut self, number: usize, is_playing: bool, milliseconds: u64) -> (u32, u32) {
        self.draw(number, is_playing, milliseconds)
            .asked_for_a_frame_of
            .expect("a frame was asked for")
    }

    /// The engine says that it made the frame it was last asked for, at the size it was asked for.
    fn the_frame_comes(&self, serial: u64, size: (u32, u32), came_from_the_cache: bool) {
        self.events
            .send(a_frame_of_size(serial, size, came_from_the_cache))
            .unwrap();
    }
}

const WHOLE: (u32, u32) = (64, 36);
const HALF: (u32, u32) = (32, 18);
const QUARTER: (u32, u32) = (16, 9);

#[test]
fn while_the_clock_goes_two_frames_that_come_late_make_the_next_ones_asked_for_smaller() {
    let mut scripted = Scripted::new();
    // (Each frame is asked for at the time the one before came, and comes 95 milliseconds after that, which is late for a frame of 100.)
    assert_eq!(scripted.size_asked_for(0, true, 0), WHOLE);
    scripted.the_frame_comes(1, WHOLE, false);
    assert_eq!(scripted.size_asked_for(1, true, 95), WHOLE, "one late frame is not two");
    scripted.the_frame_comes(2, WHOLE, false);
    assert_eq!(scripted.size_asked_for(2, true, 190), HALF, "two are");
    scripted.the_frame_comes(3, HALF, false);
    assert_eq!(
        scripted.size_asked_for(3, true, 285),
        HALF,
        "and the count begins again"
    );
    scripted.the_frame_comes(4, HALF, false);
    assert_eq!(scripted.size_asked_for(4, true, 380), QUARTER);
    scripted.the_frame_comes(5, QUARTER, false);
    assert_eq!(scripted.size_asked_for(5, true, 475), QUARTER);
    scripted.the_frame_comes(6, QUARTER, false);
    assert_eq!(
        scripted.size_asked_for(6, true, 570),
        QUARTER,
        "it does not go below a quarter"
    );
}

#[test]
fn frames_that_came_comfortably_make_the_size_go_up_again_ten_of_them_for_each_step() {
    let mut scripted = Scripted::new();
    // Down to a quarter first.
    let (mut serial, mut at, mut number) = (0, 0, 0);
    let mut size = scripted.size_asked_for(number, true, at);
    let mut sizes = vec![size];
    for _ in 0..4 {
        serial += 1;
        scripted.the_frame_comes(serial, size, false);
        at += 95;
        number += 1;
        size = scripted.size_asked_for(number, true, at);
        sizes.push(size);
    }
    assert_eq!(sizes, vec![WHOLE, WHOLE, HALF, HALF, QUARTER]);
    // Then frames that come at once: ten of them make it half, and ten more make it whole.
    let mut sizes_asked = Vec::new();
    for _ in 0..20 {
        serial += 1;
        scripted.the_frame_comes(serial, size, false);
        at += 10;
        number += 1;
        size = scripted.size_asked_for(number, true, at);
        sizes_asked.push(size);
    }
    let mut expected = vec![QUARTER; 9];
    expected.push(HALF);
    expected.extend([HALF; 9]);
    expected.push(WHOLE);
    assert_eq!(sizes_asked, expected);
}

#[test]
fn only_the_last_sixty_four_frames_that_were_asked_for_and_have_not_come_are_remembered_for_their_timing() {
    let mut scripted = Scripted::new();
    for number in 0..70 {
        scripted.draw(number, false, number as u64);
    }
    assert_eq!(scripted.preview.frames_asked_for(), 70);
    let remembered: Vec<u64> = scripted.preview.asked_at.iter().map(|(serial, _)| *serial).collect();
    assert_eq!(
        remembered,
        (7..=70).collect::<Vec<u64>>(),
        "the oldest six are forgotten"
    );
}

#[test]
fn pictures_that_the_last_frames_did_not_use_are_let_go_of_when_there_are_more_than_forty_eight_and_not_before() {
    let mut scripted = Scripted::new();
    // Each frame is of a picture of its own: the first of them is picture 1, and so on.
    for number in 0..48 {
        let drawn = scripted.draw(number, false, number as u64);
        assert_eq!(
            drawn.told_to_let_go_of,
            Vec::<PictureId>::new(),
            "with {} pictures sent, nothing is let go of",
            number + 1
        );
    }
    // With the 49th picture there are more than forty-eight: those that none of the last four frames used (and this is the 49th frame, so the pictures of the
    // 45th, 46th, 47th and 48th and this one are in use) are let go of: the first forty-four.
    let drawn = scripted.draw(48, false, 48);
    assert_eq!(drawn.told_to_let_go_of, (1..=44).map(PictureId).collect::<Vec<_>>());
    // Five pictures are left, and a sixth comes with the next frame: nothing more is let go of until there are more than forty-eight again.
    let drawn = scripted.draw(49, false, 49);
    assert_eq!(
        drawn.told_to_let_go_of,
        Vec::<PictureId>::new(),
        "there are only six pictures sent now"
    );
}

#[test]
fn the_texture_of_the_shown_frame_is_kept_for_a_frame_of_the_same_size_and_made_again_when_either_side_is_another() {
    let mut scripted = Scripted::new();
    scripted.draw(0, false, 0);
    scripted.the_frame_comes(1, WHOLE, false);
    scripted.draw(1, false, 10);
    let texture = |scripted: &Scripted| scripted.preview.shown.as_ref().unwrap().texture.id();
    let first = texture(&scripted);
    scripted.the_frame_comes(2, WHOLE, false);
    scripted.draw(2, false, 20);
    assert_eq!(
        texture(&scripted),
        first,
        "the same size: the texture is set again, and is the same"
    );
    scripted.the_frame_comes(3, (64, 18), false);
    scripted.draw(3, false, 30);
    let third = texture(&scripted);
    assert_ne!(third, first, "the same width and another height is another texture");
    scripted.the_frame_comes(4, (32, 18), false);
    scripted.draw(4, false, 40);
    let fourth = texture(&scripted);
    assert_ne!(fourth, third, "another width and the same height is another texture");
    scripted.the_frame_comes(5, (32, 18), false);
    scripted.draw(5, false, 50);
    assert_eq!(texture(&scripted), fourth);
}

#[test]
fn a_frame_that_was_asked_for_while_an_older_one_was_still_on_its_way_is_timed_from_when_it_was_asked_for() {
    // The card is slower than what is shown changes: the second frame is asked for before the first has come.
    let mut scripted = Scripted::new();
    assert_eq!(scripted.size_asked_for(0, true, 0), WHOLE);
    assert_eq!(scripted.size_asked_for(1, true, 10), WHOLE);
    // The first comes at 95: 95 milliseconds from when it was asked for, which is late. (The third is asked for as that is taken.)
    scripted.the_frame_comes(1, WHOLE, false);
    assert_eq!(scripted.size_asked_for(2, true, 95), WHOLE);
    // The second comes at 190: 180 milliseconds from when it was asked for (at 10), which is late again, and that is two in a row.
    scripted.the_frame_comes(2, WHOLE, false);
    assert_eq!(scripted.size_asked_for(3, true, 190), HALF);
}

#[test]
fn a_frame_that_came_from_the_cache_says_nothing_about_how_fast_the_card_is() {
    let mut scripted = Scripted::new();
    let mut size = scripted.size_asked_for(0, true, 0);
    for number in 1..=8 {
        scripted.the_frame_comes(number as u64, size, true);
        size = scripted.size_asked_for(number, true, number as u64 * 95);
        assert_eq!(
            size, WHOLE,
            "frame {number}: frames of the cache are not late, and are not comfortable either"
        );
    }
}

#[test]
fn the_size_is_kept_while_the_picture_is_changed_with_the_clock_stopped_and_a_picture_that_is_still_is_asked_for_whole_after_120_milliseconds()
 {
    let mut scripted = Scripted::new();
    // Down to half while playing.
    assert_eq!(scripted.size_asked_for(0, true, 0), WHOLE);
    scripted.the_frame_comes(1, WHOLE, false);
    assert_eq!(scripted.size_asked_for(1, true, 95), WHOLE);
    scripted.the_frame_comes(2, WHOLE, false);
    assert_eq!(scripted.size_asked_for(2, true, 190), HALF);
    // The clock stops and the picture is changed (the playhead is dragged over frames): each is asked for at the size it was.
    for (step, number) in (3..9).enumerate() {
        assert_eq!(
            scripted.size_asked_for(number, false, 200 + step as u64 * 40),
            HALF,
            "frame {number}"
        );
    }
    // It stays as it is, and 119 milliseconds of that are not enough while 120 are. (It is seen to be still at the first drawing that has nothing new.)
    let still_since = 200 + 5 * 40 + 1;
    assert_eq!(scripted.draw(8, false, still_since).asked_for_a_frame_of, None);
    assert_eq!(scripted.draw(8, false, still_since + 100).asked_for_a_frame_of, None);
    assert_eq!(scripted.draw(8, false, still_since + 119).asked_for_a_frame_of, None);
    assert_eq!(
        scripted.draw(8, false, still_since + 120).asked_for_a_frame_of,
        Some(WHOLE)
    );
    // And it is asked for once.
    assert_eq!(scripted.draw(8, false, still_since + 300).asked_for_a_frame_of, None);
}

#[test]
fn a_clock_that_goes_again_keeps_the_picture_from_being_made_whole() {
    let mut scripted = Scripted::new();
    scripted.draw(0, true, 0);
    scripted.the_frame_comes(1, WHOLE, false);
    scripted.draw(1, true, 95);
    scripted.the_frame_comes(2, WHOLE, false);
    assert_eq!(scripted.size_asked_for(2, true, 190), HALF);
    // Still for 100 milliseconds with the clock stopped, then it goes again on the same picture (the clock has not moved a frame yet).
    assert_eq!(scripted.draw(2, false, 200).asked_for_a_frame_of, None);
    assert_eq!(scripted.draw(2, false, 300).asked_for_a_frame_of, None);
    assert_eq!(scripted.draw(2, true, 310).asked_for_a_frame_of, None);
    assert_eq!(
        scripted.draw(2, true, 1000).asked_for_a_frame_of,
        None,
        "playing is not still: it is not made whole"
    );
    // Stopped again: 120 milliseconds from here.
    assert_eq!(scripted.draw(2, false, 1010).asked_for_a_frame_of, None);
    assert_eq!(scripted.draw(2, false, 1129).asked_for_a_frame_of, None);
    assert_eq!(scripted.draw(2, false, 1130).asked_for_a_frame_of, Some(WHOLE));
}

#[test]
fn the_window_is_asked_to_be_drawn_again_for_a_still_picture_to_be_made_whole_and_not_otherwise() {
    let mut scripted = Scripted::new();
    // (egui asks for the window to be drawn a little before the time it is given: a frame of a screen of 60 to the second sooner, so as not to be late.)
    let is_asked_for_in_about = |delays: Vec<Duration>, milliseconds: u64| {
        let asked = Duration::from_millis(milliseconds);
        matches!(delays.as_slice(), [delay] if *delay <= asked && *delay + Duration::from_millis(20) >= asked)
    };
    // A whole picture waits for nothing, still or not.
    assert_eq!(
        scripted.draw(0, false, 0).asked_to_be_drawn_again_after,
        Vec::<Duration>::new()
    );
    assert_eq!(
        scripted.draw(0, false, 500).asked_to_be_drawn_again_after,
        Vec::<Duration>::new()
    );
    scripted.the_frame_comes(1, WHOLE, false);
    scripted.draw(1, true, 600);
    scripted.the_frame_comes(2, WHOLE, false);
    scripted.draw(2, true, 695);
    scripted.the_frame_comes(3, WHOLE, false);
    assert_eq!(scripted.size_asked_for(3, true, 790), HALF);
    // A smaller picture that is playing, or is being changed, does not wait for a moment of stillness. One that is still does, for as long as is left of the time.
    assert_eq!(
        scripted.draw(3, true, 800).asked_to_be_drawn_again_after,
        Vec::<Duration>::new()
    );
    assert_eq!(
        scripted.draw(4, false, 810).asked_to_be_drawn_again_after,
        Vec::<Duration>::new()
    );
    assert!(is_asked_for_in_about(
        scripted.draw(4, false, 820).asked_to_be_drawn_again_after,
        120
    ));
    assert!(is_asked_for_in_about(
        scripted.draw(4, false, 870).asked_to_be_drawn_again_after,
        70
    ));
    // When it has been made whole it waits for nothing.
    let drawn = scripted.draw(4, false, 940);
    assert_eq!(drawn.asked_for_a_frame_of, Some(WHOLE));
    assert_eq!(drawn.asked_to_be_drawn_again_after, Vec::<Duration>::new());
    assert_eq!(
        scripted.draw(4, false, 950).asked_to_be_drawn_again_after,
        Vec::<Duration>::new()
    );
}

#[test]
fn with_a_real_engine_frames_that_come_late_are_made_smaller_and_frames_that_come_at_once_are_made_bigger_again_and_what_is_shown_is_the_frame_of_that_size()
 {
    let Some(mut setup) = setup() else { return };
    let pictures: Vec<SuppliedPicture> = (1..=100)
        .map(|serial| flat_picture(serial, [(serial % 200) as u8 + 10, 40, 40, 255]))
        .collect();
    let mut number = 0;
    let mut next_frame = |setup: &mut Setup| {
        let shown = content(vec![showing(1, &pictures[number])]);
        number += 1;
        let frame = setup.show_a_new_frame_of(&shown);
        (frame.width_in_pixels, frame.height_in_pixels)
    };

    // A frame of the project that lasts a nanosecond: nothing is in time for it.
    setup.is_playing = true;
    setup.how_long_a_frame_lasts = Duration::from_nanos(1);
    let mut sizes = Vec::new();
    for _ in 0..10 {
        sizes.push(next_frame(&mut setup));
    }
    assert_eq!(sizes[0], RENDER_SIZE, "the first frames are whole");
    assert_eq!(
        *sizes.last().unwrap(),
        (16, 9),
        "and the frames that came late made them smaller, down to a quarter: {sizes:?}"
    );
    assert!(
        sizes.windows(2).all(|pair| pair[1].0 <= pair[0].0),
        "and they never went up: {sizes:?}"
    );

    // A frame of the project that lasts an hour: everything is in time for it.
    setup.how_long_a_frame_lasts = Duration::from_secs(3600);
    let mut sizes = Vec::new();
    for _ in 0..60 {
        sizes.push(next_frame(&mut setup));
        if *sizes.last().unwrap() == RENDER_SIZE {
            break;
        }
    }
    assert_eq!(*sizes.last().unwrap(), RENDER_SIZE, "back to whole: {sizes:?}");
    assert!(
        sizes.windows(2).all(|pair| pair[1].0 >= pair[0].0),
        "and they never went down: {sizes:?}"
    );
    assert!(sizes.contains(&(32, 18)), "by way of half: {sizes:?}");
}

#[test]
fn every_replacement_of_the_device_is_to_be_told_not_only_the_last_and_in_the_order_they_came() {
    // (An engine that cannot start still has a preview to tell things to: no card is used.)
    let mut preview = EnginePreview::start(AdapterChoice::NoAdapterAtAll, Arc::new(|| {}));
    preview.note_that_the_device_was_replaced("the first card was reset".to_owned(), &[]);
    preview.note_that_the_device_was_replaced("the second card was reset".to_owned(), &["file:one".to_owned()]);

    let to_tell = preview.take_what_the_user_is_to_be_told();

    assert_eq!(to_tell.len(), 2, "{to_tell:?}");
    assert!(to_tell[0].contains("the first card was reset"), "{to_tell:?}");
    assert!(to_tell[1].contains("the second card was reset"), "{to_tell:?}");
    assert!(to_tell[1].contains("file:one"), "{to_tell:?}");
    assert_eq!(preview.last_problem(), Some("the second card was reset"));
}

#[test]
fn the_words_about_a_replaced_device_say_why_that_the_engine_goes_on_and_which_effects_are_not_drawn_any_more() {
    let without_effects = super::the_words_about_a_replaced_device("the card was reset", &[]);
    assert!(without_effects.contains("the card was reset"), "{without_effects}");
    assert!(without_effects.contains("goes on"), "{without_effects}");
    assert!(
        without_effects.contains("Nothing of the project is changed"),
        "{without_effects}"
    );
    assert!(
        !without_effects.contains("not drawn again"),
        "no effect is switched off: {without_effects}"
    );

    let with_effects = super::the_words_about_a_replaced_device(
        "a frame did not finish",
        &["file:one".to_owned(), "file:two".to_owned()],
    );
    assert!(with_effects.contains("a frame did not finish"), "{with_effects}");
    assert!(with_effects.contains("not drawn again"), "{with_effects}");
    assert!(with_effects.contains("file:one, file:two"), "{with_effects}");
}

// ---- the checkpoints of the states of effects, and the picture that is not exact (EN-14) ----

/// What a scripted engine was told about the checkpoints since this was last looked at: `Ok(rules)` for each time it was given rules, and `Err(())` for each time it was told to forget them.
fn what_was_said_of_the_checkpoints(requests: &Receiver<EngineRequest>) -> Vec<Result<RulesOfCheckpoints, ()>> {
    let mut said = Vec::new();
    while let Ok(request) = requests.try_recv() {
        match request {
            EngineRequest::SetTheRulesOfCheckpoints(rules) => said.push(Ok(rules)),
            EngineRequest::ForgetTheCheckpoints => said.push(Err(())),
            _ => {}
        }
    }
    said
}

const EVERY_SIXTY_FRAMES_AND_A_GIB: RulesOfCheckpoints = RulesOfCheckpoints {
    interval_in_frames: 60,
    memory_budget_in_bytes: 1 << 30,
};

#[test]
fn the_rules_of_checkpoints_are_sent_when_they_are_not_what_the_engine_was_told_last_and_only_then() {
    let (engine, _events, requests) = Engine::scripted_for_tests();
    let mut preview = EnginePreview::with_engine(engine);
    // (The engine takes none until it is told, so telling it none is nothing to say.)
    preview.set_the_rules_of_checkpoints(RulesOfCheckpoints::NONE);
    assert_eq!(what_was_said_of_the_checkpoints(&requests), []);
    preview.set_the_rules_of_checkpoints(EVERY_SIXTY_FRAMES_AND_A_GIB);
    preview.set_the_rules_of_checkpoints(EVERY_SIXTY_FRAMES_AND_A_GIB);
    assert_eq!(
        what_was_said_of_the_checkpoints(&requests),
        [Ok(EVERY_SIXTY_FRAMES_AND_A_GIB)]
    );
    let more = RulesOfCheckpoints {
        interval_in_frames: 30,
        ..EVERY_SIXTY_FRAMES_AND_A_GIB
    };
    preview.set_the_rules_of_checkpoints(more);
    preview.set_the_rules_of_checkpoints(RulesOfCheckpoints::NONE);
    assert_eq!(
        what_was_said_of_the_checkpoints(&requests),
        [Ok(more), Ok(RulesOfCheckpoints::NONE)]
    );
}

#[test]
fn the_checkpoints_are_forgotten_when_the_colours_of_the_frames_change_or_the_files_are_let_go_of_or_the_project_is_not_what_it_was()
 {
    let (engine, _events, requests) = Engine::scripted_for_tests();
    let mut preview = EnginePreview::with_engine(engine);
    // The colour it has, again: nothing.
    preview.set_the_colour(FrameColour::default());
    assert_eq!(what_was_said_of_the_checkpoints(&requests), []);
    // Another colour: what the states hold is in the other colour.
    let other = FrameColour {
        working: crate::engine::WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709),
        ..FrameColour::default()
    };
    preview.set_the_colour(other);
    preview.set_the_colour(other);
    assert_eq!(what_was_said_of_the_checkpoints(&requests), [Err(())], "once");
    // The window lets go of its files: another project, or a file that was replaced.
    preview.forget_what_is_known_of_the_files();
    assert_eq!(what_was_said_of_the_checkpoints(&requests), [Err(())]);
    // The project: the first state is not a change, and the same state again is not, and another is.
    preview.the_project_is_now((1, 1));
    preview.the_project_is_now((1, 1));
    assert_eq!(what_was_said_of_the_checkpoints(&requests), []);
    preview.the_project_is_now((1, 2));
    assert_eq!(what_was_said_of_the_checkpoints(&requests), [Err(())]);
    // (Undo goes to a state it was in before: that is another state too, and so is another project that has the same number of edits.)
    preview.the_project_is_now((1, 1));
    preview.the_project_is_now((2, 1));
    assert_eq!(what_was_said_of_the_checkpoints(&requests), [Err(()), Err(())]);
}

/// A frame that the engine made, as an event says it, that is exact or not.
fn a_frame_that_is_approximate(serial: u64, approximate: bool) -> EngineEvent {
    EngineEvent::Frame {
        serial,
        frame: Arc::new(RenderedFrame {
            width_in_pixels: 2,
            height_in_pixels: 2,
            rgba_pixels: [0, 0, 0, 255].repeat(4),
        }),
        statistics: crate::engine::FrameStatistics {
            approximate,
            ..Default::default()
        },
    }
}

#[test]
fn the_picture_that_is_shown_says_whether_the_engine_said_it_is_exact_and_a_picture_that_is_not_there_is_not_said_to_be_inexact()
 {
    let mut scripted = Scripted::new();
    assert!(!scripted.preview.the_picture_shown_is_not_exact(), "nothing is shown");
    scripted.draw(0, false, 0);
    scripted.events.send(a_frame_that_is_approximate(1, true)).unwrap();
    scripted.draw(0, false, 1);
    assert!(scripted.preview.has_a_frame_to_show());
    assert!(scripted.preview.the_picture_shown_is_not_exact());
    // The next frame is exact, and the picture is.
    scripted.draw(1, false, 2);
    scripted.events.send(a_frame_that_is_approximate(2, false)).unwrap();
    scripted.draw(1, false, 3);
    assert!(!scripted.preview.the_picture_shown_is_not_exact());
}

// ---- the lead-up on a seek (EN-14, option 1) ----

/// The layer `layer` with a shader that has a state (the state number 9) at this frame of its own clock.
fn with_a_state_at(layer: PreviewLayer<'_>, frame_of_the_state: u64) -> PreviewLayer<'_> {
    PreviewLayer {
        operations: vec![crate::project::PictureOperation::Shader {
            key: "file:test".to_owned(),
            numbers: vec![0.0; 4],
            state: Some((9, frame_of_the_state)),
            images: Vec::new(),
            reads_the_backdrop: false,
        }],
        ..layer
    }
}

impl Scripted {
    /// Draws picture `number` with a shader on it that has a state at `frame_of_the_state`, `milliseconds` after the start, with the clock stopped, and gives everything that the
    /// preview asked of the engine while it did.
    fn draw_with_a_state(&mut self, number: usize, frame_of_the_state: u64, milliseconds: u64) -> Vec<EngineRequest> {
        let shown = content(vec![with_a_state_at(
            showing(1, &self.pictures[number]),
            frame_of_the_state,
        )]);
        self.preview.update(
            &self.context,
            &shown,
            RENDER_SIZE,
            PictureFilter::Smooth,
            &mut self.text_renderer,
            HowTheClockGoes {
                is_playing: false,
                how_long_a_frame_lasts: Duration::from_millis(100),
                now: self.start + Duration::from_millis(milliseconds),
            },
        );
        self.requests.try_iter().collect()
    }

    /// What the engine was asked since, other than the pictures it was sent.
    fn the_requests_of_the_lead_up(requests: &[EngineRequest]) -> Vec<String> {
        requests
            .iter()
            .filter_map(|request| match request {
                EngineRequest::Render { serial, .. } => Some(format!("render {serial}")),
                EngineRequest::StartTheLeadUp { generation, .. } => Some(format!("start {generation}")),
                EngineRequest::CancelTheLeadUp { generation } => Some(format!("cancel {generation}")),
                _ => None,
            })
            .collect()
    }

    /// A preview that has been shown picture `number` with a state at `frame_of_the_state` for as long as it takes (`WAIT` milliseconds) with the engine saying that it
    /// is not exact, and a lead-up is wanted of.
    fn that_wants_a_lead_up(number: usize, frame_of_the_state: u64) -> (Self, LeadUpWanted) {
        let mut scripted = Self::new();
        scripted.preview.set_a_lead_up_is_wanted(true);
        let asked = scripted.draw_with_a_state(number, frame_of_the_state, 0);
        assert_eq!(Self::the_requests_of_the_lead_up(&asked), ["render 1"]);
        scripted.events.send(a_frame_that_is_approximate(1, true)).unwrap();
        scripted.draw_with_a_state(number, frame_of_the_state, WAIT);
        let wanted = scripted
            .preview
            .the_lead_up_that_is_wanted()
            .expect("a lead-up is wanted of a picture that is not exact and has stayed as it is");
        (scripted, wanted)
    }
}

/// A little more than the time that a picture stays as it is before a lead-up is wanted of it.
const WAIT: u64 = 300;

/// What the window gives the preview when it starts the lead-up that is wanted: the end of the channel that the engine takes it from, and what keeps the thread that makes it alive.
fn a_lead_up_that_is_started() -> (std::sync::mpsc::Sender<LeadUpItem>, Receiver<LeadUpItem>) {
    std::sync::mpsc::channel()
}

#[test]
fn a_lead_up_is_wanted_of_a_picture_that_is_not_exact_has_a_state_and_has_stayed_as_it_is_for_a_quarter_of_a_second() {
    let mut scripted = Scripted::new();
    scripted.preview.set_a_lead_up_is_wanted(true);
    scripted.preview.set_the_colour(another_colour());
    scripted.draw_with_a_state(0, 400, 0);
    scripted.events.send(a_frame_that_is_approximate(1, true)).unwrap();
    scripted.draw_with_a_state(0, 400, 100);
    assert!(scripted.preview.the_picture_shown_is_not_exact());
    assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), None, "not yet");
    scripted.draw_with_a_state(0, 400, 249);
    assert_eq!(
        scripted.preview.the_lead_up_that_is_wanted(),
        None,
        "not before a quarter of a second"
    );
    scripted.draw_with_a_state(0, 400, 250);
    let wanted = scripted.preview.the_lead_up_that_is_wanted().expect("now it is");
    assert_eq!(
        wanted.render_size, RENDER_SIZE,
        "the size that the picture on the screen was made at"
    );
    assert_eq!(
        wanted.colour,
        another_colour(),
        "the colour that the picture on the screen was made in"
    );
    assert_eq!(
        wanted.most_frames_a_state_has_gone_on_for, 400,
        "how far back the states of the frame on the screen go"
    );
    assert_eq!(wanted.generation, 1);
    // (Asked again, it is the same: nothing has been done about it.)
    scripted.draw_with_a_state(0, 400, 400);
    assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), Some(wanted));
}

#[test]
fn the_preview_asks_to_be_drawn_again_when_the_quarter_of_a_second_is_over_so_that_a_picture_that_is_still_is_looked_at_then()
 {
    let mut scripted = Scripted::new();
    scripted.preview.set_a_lead_up_is_wanted(true);
    scripted.draw_with_a_state(0, 400, 0);
    scripted.events.send(a_frame_that_is_approximate(1, true)).unwrap();
    let delays = Arc::new(std::sync::Mutex::new(Vec::new()));
    {
        let delays = Arc::clone(&delays);
        scripted
            .context
            .set_request_repaint_callback(move |information| delays.lock().unwrap().push(information.delay));
    }
    scripted.draw_with_a_state(0, 400, 100);
    // (The 150 milliseconds that are left, less what egui takes off for the frame that it is in the middle of.)
    assert!(
        delays
            .lock()
            .unwrap()
            .iter()
            .any(|delay| *delay > Duration::from_millis(100) && *delay <= Duration::from_millis(150)),
        "{:?}",
        delays.lock().unwrap()
    );
}

#[test]
fn no_lead_up_is_wanted_unless_all_of_it_is_so() {
    // The setting is off (the preview is not told it is on).
    let mut scripted = Scripted::new();
    scripted.draw_with_a_state(0, 400, 0);
    scripted.events.send(a_frame_that_is_approximate(1, true)).unwrap();
    scripted.draw_with_a_state(0, 400, WAIT);
    assert!(scripted.preview.the_picture_shown_is_not_exact());
    assert_eq!(
        scripted.preview.the_lead_up_that_is_wanted(),
        None,
        "the setting is off"
    );
    scripted.preview.set_a_lead_up_is_wanted(true);
    scripted.draw_with_a_state(0, 400, WAIT + 1);
    assert!(
        scripted.preview.the_lead_up_that_is_wanted().is_some(),
        "and now it is on"
    );

    // The picture is exact.
    let mut exact = Scripted::new();
    exact.preview.set_a_lead_up_is_wanted(true);
    exact.draw_with_a_state(0, 400, 0);
    exact.events.send(a_frame_that_is_approximate(1, false)).unwrap();
    exact.draw_with_a_state(0, 400, WAIT);
    assert_eq!(exact.preview.the_lead_up_that_is_wanted(), None, "the picture is exact");

    // The picture on the screen is not the one that was asked for last: the engine has not answered the last.
    let mut late = Scripted::new();
    late.preview.set_a_lead_up_is_wanted(true);
    late.draw_with_a_state(0, 400, 0);
    late.events.send(a_frame_that_is_approximate(1, true)).unwrap();
    late.draw_with_a_state(0, 400, 1);
    late.draw_with_a_state(1, 400, WAIT);
    assert_eq!(
        late.preview.the_lead_up_that_is_wanted(),
        None,
        "the frame that is asked for has not come"
    );
    late.draw_with_a_state(1, 400, 2 * WAIT);
    assert_eq!(
        late.preview.the_lead_up_that_is_wanted(),
        None,
        "and the picture on the screen is of the one before it, which is not what the lead-up would be for"
    );

    // Nothing in the picture depends on earlier frames.
    let mut stateless = Scripted::new();
    stateless.preview.set_a_lead_up_is_wanted(true);
    stateless.draw(0, false, 0);
    stateless.events.send(a_frame_that_is_approximate(1, true)).unwrap();
    stateless.draw(0, false, WAIT);
    assert_eq!(stateless.preview.the_lead_up_that_is_wanted(), None, "no state");

    // The state is at the first frame of its effect: the picture is exact and there is nothing before it.
    let mut at_the_first_frame = Scripted::new();
    at_the_first_frame.preview.set_a_lead_up_is_wanted(true);
    at_the_first_frame.draw_with_a_state(0, 0, 0);
    at_the_first_frame
        .events
        .send(a_frame_that_is_approximate(1, true))
        .unwrap();
    at_the_first_frame.draw_with_a_state(0, 0, WAIT);
    assert_eq!(
        at_the_first_frame.preview.the_lead_up_that_is_wanted(),
        None,
        "no frame before it"
    );
}

#[test]
fn a_picture_that_keeps_changing_has_no_lead_up_wanted_until_it_stays_as_it_is() {
    let mut scripted = Scripted::new();
    scripted.preview.set_a_lead_up_is_wanted(true);
    // A playhead that is dragged: another frame every 100 milliseconds, each of them said to be not exact.
    for step in 0..6_u64 {
        scripted.draw_with_a_state(step as usize, 400 + step, step * 100);
        scripted
            .events
            .send(a_frame_that_is_approximate(step + 1, true))
            .unwrap();
        scripted.draw_with_a_state(step as usize, 400 + step, step * 100 + 50);
        assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), None, "step {step}");
    }
    // And then it stays.
    scripted.draw_with_a_state(5, 405, 600 + WAIT);
    assert!(scripted.preview.the_lead_up_that_is_wanted().is_some());
}

#[test]
fn a_lead_up_that_the_window_starts_is_given_to_the_engine_and_is_being_made_until_it_is_over() {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    assert!(!scripted.preview.a_lead_up_is_being_made());
    let (_sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(&wanted, items, ());
    assert!(scripted.preview.a_lead_up_is_being_made());
    assert_eq!(
        scripted.preview.the_lead_up_that_is_wanted(),
        None,
        "it is not wanted when it is going on"
    );
    let asked: Vec<EngineRequest> = scripted.requests.try_iter().collect();
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), ["start 1"]);
    // While it goes on the same frame is not asked for again, and no other lead-up is wanted.
    let asked = scripted.draw_with_a_state(0, 400, 2 * WAIT);
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), Vec::<String>::new());
    assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), None);
    assert!(
        scripted.preview.the_picture_shown_is_not_exact(),
        "the picture is as it was until it is over"
    );
}

#[test]
fn a_lead_up_that_was_not_asked_for_or_is_not_the_one_that_was_is_not_started() {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let (sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(
        &LeadUpWanted {
            generation: 99,
            ..wanted
        },
        items,
        (),
    );
    assert!(!scripted.preview.a_lead_up_is_being_made());
    assert!(
        scripted.requests.try_iter().next().is_none(),
        "the engine is told nothing"
    );
    assert!(
        sender.send(LeadUpItem::End).is_err(),
        "and the thread that made it finds that it is let go of"
    );
    // One that is not wanted at all.
    let mut nothing_wanted = Scripted::new();
    let (sender, items) = a_lead_up_that_is_started();
    nothing_wanted.preview.start_the_lead_up(&wanted, items, ());
    assert!(!nothing_wanted.preview.a_lead_up_is_being_made());
    assert!(sender.send(LeadUpItem::End).is_err());
}

#[test]
fn another_frame_that_is_asked_for_cancels_the_lead_up_before_it_is_asked_for_and_the_same_frame_asked_for_again_does_not()
 {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let (_sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(&wanted, items, ());
    scripted.requests.try_iter().for_each(drop);

    // The engine says that it has not got a picture of the frame: the same frame is asked for again, and the lead-up goes on.
    scripted
        .events
        .send(EngineEvent::PicturesMissing {
            serial: 1,
            ids: Vec::new(),
        })
        .unwrap();
    let asked = scripted.draw_with_a_state(0, 400, 2 * WAIT);
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), ["render 2"]);
    assert!(scripted.preview.a_lead_up_is_being_made());

    // Another frame: the lead-up is cancelled, and the engine is told so before it is asked for the other frame (so that it does not make the frame in the middle of it).
    let asked = scripted.draw_with_a_state(1, 400, 3 * WAIT);
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), ["cancel 1", "render 3"]);
    assert!(!scripted.preview.a_lead_up_is_being_made());
}

#[test]
fn when_the_lead_up_is_done_the_frame_is_asked_for_again_and_no_other_lead_up_is_wanted_for_it() {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let (_sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(&wanted, items, ());
    scripted.requests.try_iter().for_each(drop);
    scripted
        .events
        .send(EngineEvent::LeadUpDone {
            generation: 1,
            frames_made: 300,
        })
        .unwrap();
    let asked = scripted.draw_with_a_state(0, 400, 2 * WAIT);
    assert!(!scripted.preview.a_lead_up_is_being_made());
    assert_eq!(
        Scripted::the_requests_of_the_lead_up(&asked),
        ["render 2"],
        "the same frame, now that the states are what the lead-up made them"
    );
    // Said to be exact, the picture is. And if it were said not to be, there would be no lead-up again for the same frame: that is a loop.
    scripted.events.send(a_frame_that_is_approximate(2, true)).unwrap();
    scripted.draw_with_a_state(0, 400, 3 * WAIT);
    assert!(scripted.preview.the_picture_shown_is_not_exact());
    scripted.draw_with_a_state(0, 400, 10 * WAIT);
    assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), None);
    // A lead-up is wanted again when the picture has been another and comes back to this one, and is not exact.
    scripted.draw_with_a_state(1, 401, 11 * WAIT);
    scripted.events.send(a_frame_that_is_approximate(3, false)).unwrap();
    scripted.draw_with_a_state(1, 401, 12 * WAIT);
    scripted.draw_with_a_state(0, 400, 13 * WAIT);
    scripted.events.send(a_frame_that_is_approximate(4, true)).unwrap();
    scripted.draw_with_a_state(0, 400, 14 * WAIT);
    scripted.draw_with_a_state(0, 400, 15 * WAIT);
    let wanted_again = scripted
        .preview
        .the_lead_up_that_is_wanted()
        .expect("it came back to the frame");
    assert_eq!(wanted_again.generation, 2, "another lead-up, another number");
}

#[test]
fn a_lead_up_that_failed_is_not_made_again_for_the_same_frame_and_the_problem_is_kept_and_the_frame_is_not_asked_for_again()
 {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let (_sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(&wanted, items, ());
    scripted.requests.try_iter().for_each(drop);
    scripted
        .events
        .send(EngineEvent::LeadUpFailed {
            generation: 1,
            problem: "the video cannot be read".to_owned(),
        })
        .unwrap();
    let asked = scripted.draw_with_a_state(0, 400, 2 * WAIT);
    assert!(!scripted.preview.a_lead_up_is_being_made());
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), Vec::<String>::new());
    assert!(
        scripted
            .preview
            .last_problem()
            .is_some_and(|problem| problem.contains("the video cannot be read")),
        "{:?}",
        scripted.preview.last_problem()
    );
    scripted.draw_with_a_state(0, 400, 10 * WAIT);
    assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), None);
}

#[test]
fn what_the_engine_says_of_another_lead_up_than_the_one_going_on_is_not_heeded() {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let (_sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(&wanted, items, ());
    scripted.requests.try_iter().for_each(drop);
    for event in [
        EngineEvent::LeadUpDone {
            generation: 7,
            frames_made: 1,
        },
        EngineEvent::LeadUpFailed {
            generation: 8,
            problem: "not this one".to_owned(),
        },
    ] {
        scripted.events.send(event).unwrap();
    }
    let asked = scripted.draw_with_a_state(0, 400, 2 * WAIT);
    assert!(scripted.preview.a_lead_up_is_being_made());
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), Vec::<String>::new());
    assert_eq!(scripted.preview.last_problem(), None);
}

#[test]
fn turning_the_setting_off_cancels_the_lead_up_that_is_going_on_and_one_that_is_wanted_is_not() {
    let (mut scripted, _wanted) = Scripted::that_wants_a_lead_up(0, 400);
    scripted.preview.set_a_lead_up_is_wanted(false);
    assert_eq!(
        scripted.preview.the_lead_up_that_is_wanted(),
        None,
        "it is not wanted any more"
    );
    let (mut going, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let (_sender, items) = a_lead_up_that_is_started();
    going.preview.start_the_lead_up(&wanted, items, ());
    going.requests.try_iter().for_each(drop);
    going.preview.set_a_lead_up_is_wanted(true);
    assert!(going.preview.a_lead_up_is_being_made(), "the setting is still on");
    going.preview.set_a_lead_up_is_wanted(false);
    assert!(!going.preview.a_lead_up_is_being_made());
    let asked: Vec<EngineRequest> = going.requests.try_iter().collect();
    assert_eq!(Scripted::the_requests_of_the_lead_up(&asked), ["cancel 1"]);
}

/// Something done to the preview (or the engine it is told by) that cancels a lead-up, which gives what the preview asked of the engine in the drawing that it needed, if it needed one.
type WhatChangesTheProject = fn(&mut Scripted) -> Vec<EngineRequest>;

fn another_colour() -> FrameColour {
    FrameColour {
        working: crate::engine::WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709),
        ..FrameColour::default()
    }
}

#[test]
fn a_lead_up_is_cancelled_when_the_colour_changes_the_files_are_let_go_of_the_project_is_not_what_it_was_and_the_device_is_replaced()
 {
    // Each change gives what the preview asked of the engine in the drawing that it needed, if it needed one.
    let changes: [(&str, WhatChangesTheProject); 4] = [
        ("the colour", |scripted| {
            scripted.preview.set_the_colour(another_colour());
            Vec::new()
        }),
        ("the files", |scripted| {
            scripted.preview.forget_what_is_known_of_the_files();
            Vec::new()
        }),
        ("the project", |scripted| {
            scripted.preview.the_project_is_now((1, 1));
            scripted.preview.the_project_is_now((1, 2));
            Vec::new()
        }),
        ("the device", |scripted| {
            scripted
                .events
                .send(EngineEvent::DeviceReplaced {
                    why: "it was lost".to_owned(),
                    effects_switched_off: Vec::new(),
                })
                .unwrap();
            scripted.draw_with_a_state(0, 400, 2 * WAIT)
        }),
    ];
    for (what, change) in changes {
        let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
        let (_sender, items) = a_lead_up_that_is_started();
        scripted.preview.start_the_lead_up(&wanted, items, ());
        scripted.requests.try_iter().for_each(drop);
        let mut asked = change(&mut scripted);
        assert!(!scripted.preview.a_lead_up_is_being_made(), "{what}");
        asked.extend(scripted.requests.try_iter());
        assert!(
            Scripted::the_requests_of_the_lead_up(&asked).contains(&"cancel 1".to_owned()),
            "{what}: {:?}",
            Scripted::the_requests_of_the_lead_up(&asked)
        );
    }
}

#[test]
fn a_lead_up_that_the_window_could_not_make_is_not_wanted_again_for_the_frame_and_the_reason_is_kept() {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    scripted
        .preview
        .the_lead_up_cannot_be_made(&wanted, "the project cannot be planned".to_owned());
    assert_eq!(scripted.preview.the_lead_up_that_is_wanted(), None);
    assert!(
        scripted
            .preview
            .last_problem()
            .is_some_and(|problem| problem.contains("the project cannot be planned"))
    );
    scripted.draw_with_a_state(0, 400, 10 * WAIT);
    assert_eq!(
        scripted.preview.the_lead_up_that_is_wanted(),
        None,
        "not again for this frame"
    );
    // (A call about a lead-up that is not the one that is wanted is nothing.)
    let (mut other, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    other.preview.the_lead_up_cannot_be_made(
        &LeadUpWanted {
            generation: 99,
            ..wanted
        },
        "not this one".to_owned(),
    );
    assert!(other.preview.the_lead_up_that_is_wanted().is_some());
}

#[test]
fn dropping_what_keeps_the_thread_of_a_lead_up_alive_is_what_cancels_it_and_a_cancel_lets_go_of_it() {
    let (mut scripted, wanted) = Scripted::that_wants_a_lead_up(0, 400);
    let alive = Arc::new(());
    let (_sender, items) = a_lead_up_that_is_started();
    scripted.preview.start_the_lead_up(&wanted, items, Arc::clone(&alive));
    assert_eq!(Arc::strong_count(&alive), 2, "the preview keeps it");
    scripted.draw_with_a_state(1, 400, 2 * WAIT);
    assert_eq!(
        Arc::strong_count(&alive),
        1,
        "and lets go of it when it cancels the lead-up"
    );
}
