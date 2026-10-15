// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview's side of the engine, against a real engine on the software adapter: frames are asked for when the content changes and not when it does not,
//! pictures are sent once, the engine's complaints are answered, and old pictures are let go of.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Context, vec2};

use super::*;
use crate::application::preview_panel::test_content::{clip, in_the_middle, layer, whole_picture};
use crate::application::preview_panel::{PreviewLayer, PreviewLayerContent, VideoInThePicture};
use crate::application::theme;
use crate::colour_effects::ColourMatrix;
use crate::engine::testing::software_graphics;
use crate::media::DecodedFrame;
use crate::playback::{SuppliedFrame, SuppliedPicture};
use crate::project::{SourceFrameIndex, SourceMediaIdentifier, SpeedIndicatorLayout};

const RENDER_SIZE: (u32, u32) = (64, 36);
const A_LONG_TIME: Duration = Duration::from_secs(30);

/// A preview of these layers on a picture of 1600 by 900 pixels.
fn content<'a>(layers: Vec<PreviewLayer<'a>>) -> PreviewContent<'a> {
    PreviewContent {
        picture_size: Some((1600, 900)),
        layers,
        symbol: None,
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
        selected_clip: None,
    }
}

/// A picture of one flat colour, as the supplier has it, with this serial number.
fn flat_picture(serial_number: u64, colour: [u8; 4]) -> SuppliedPicture {
    SuppliedPicture {
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
}

fn setup() -> Option<Setup> {
    let graphics = software_graphics()?;
    Some(Setup {
        _graphics: graphics,
        context: Context::default(),
        preview: EnginePreview::start(AdapterChoice::SoftwareOnly, Arc::new(|| {})),
        text_renderer: LazyTextRenderer::new(Arc::new(|| {})),
    })
}

impl Setup {
    fn update(&mut self, content: &PreviewContent<'_>) {
        self.update_with(content, PictureFilter::Smooth);
    }

    fn update_with(&mut self, content: &PreviewContent<'_>, filter: PictureFilter) {
        self.preview
            .update(&self.context, content, RENDER_SIZE, filter, &mut self.text_renderer);
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

    /// Updates with `content` until the frame that is shown is not the one that was before (by its serial number), and gives it.
    fn show_a_new_frame_of(&mut self, content: &PreviewContent<'_>) -> RenderedFrame {
        let before = self.preview.shown.as_ref().map(|shown| shown.serial);
        self.update_until(content, "a new frame is shown", |preview| {
            preview.shown.as_ref().map(|shown| shown.serial) != before
        });
        self.preview.the_frame_shown().unwrap().clone()
    }
}

fn pixel_of(frame: &RenderedFrame, across: u32, down: u32) -> [u8; 4] {
    frame.pixel(across, down)
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
    turned.colour = ColourMatrix::from_rows([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]);
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
