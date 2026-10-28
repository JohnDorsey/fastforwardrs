// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window with its preview made by the picture engine (on the software graphics adapter), against the window as it always was: the picture that is
//! shown is the engine's, everything the window draws to show where things are and what the mouse can take hold of is as it was, a window that is not changing
//! is not asking for frames, and a window whose engine stops makes its preview the old way.

use std::sync::{Mutex, MutexGuard, PoisonError};

use egui::{Modifiers, Rect, vec2};
use egui_kittest::kittest::Queryable;

use super::picture_tests::picture_source;
use super::test_window::{VIDEO, Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::preview_panel::{
    FIT_BUTTON_LABEL, corner_handle_label, picture_label, render_size_for, video_label,
};
use crate::application::selection::SelectionChange;
use crate::application::textures::PictureFilter;
use crate::application::theme;
use crate::colour_effects::ColourMatrix;
use crate::engine::testing::DevicesMade;
use crate::engine::{AdapterChoice, EngineSettings};
use crate::media::{CompressedPicture, PictureSize, ThumbnailKey, ThumbnailLibrary};
use crate::project::{
    AnimationTarget, ClipId, Corner, Project, PropertyValue, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex,
};
use crate::rgba_picture::RgbaPicture;
use crate::test_support::{HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE, put_a_hue_shift_on};

/// The size of the picture of the projects of the tests.
const PICTURE_SIZE: (u32, u32) = (1280, 720);

/// How much the wheel is turned to zoom in by far more than there is room for, in points (see `still_window_tests`).
const WHEEL_FOR_ZOOMING_IN_AS_FAR_AS_IT_GOES: f32 = 200.0 * std::f32::consts::LN_2 * 8.0;

/// A project of a video and, in front of it, a picture file, which is where it is put: `(project, its source, its clip)`.
fn project_with_a_picture_file(hue_shift_in_degrees: Option<f32>) -> (Project, SourceMediaIdentifier, ClipId) {
    let mut project = project_of(300);
    let source = picture_source();
    let identifier = source.identifier;
    let clip = project
        .import_source(source, TimelineFrameIndex::new(0))
        .unwrap()
        .clip
        .expect("a picture makes a clip");
    if let Some(degrees) = hue_shift_in_degrees {
        put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), degrees);
    }
    (project, identifier, clip)
}

/// Makes the picture file of `source` decoded: all of one colour.
fn decode_the_picture_file(window: &mut Window, source: SourceMediaIdentifier, colour: [u8; 4]) {
    window
        .application_mut()
        .sources
        .files
        .get_mut(&source)
        .expect("the window has the files of the project")
        .picture
        .picture = Some(RgbaPicture {
        width_in_pixels: 4,
        height_in_pixels: 2,
        rgba_pixels: colour.repeat(8),
    });
    window.settle();
}

/// The video clip of a window made from a project made by [`project_of`]. (It is on the back track: a picture file is put on a track in front of it.)
fn the_video(window: &Window) -> ClipId {
    window
        .application()
        .project()
        .tracks()
        .last()
        .expect("the project has a track")
        .entries()[0]
        .id()
}

/// Where the picture of the preview is on the screen, in points. (The video of the project is all of the picture.)
fn the_picture_on_the_screen(window: &Window) -> Rect {
    window.rectangle_of(&video_label(the_video(window)))
}

/// Where the middle of the layer of `clip` is on the picture, as fractions of its width and height. (The window knows where it is: it says so for the mouse.
/// A clip that has not been placed is fitted inside the picture, which is not to be read from the clip.)
fn middle_of_the_layer(window: &Window, label: &str) -> (f32, f32) {
    let picture = the_picture_on_the_screen(window);
    let middle = window.rectangle_of(label).center();
    (
        (middle.x - picture.left()) / picture.width(),
        (middle.y - picture.top()) / picture.height(),
    )
}

/// The pixel of the frame that the engine made that is where the point `across`, `down` is of the picture (fractions of its width and height).
fn the_pixel_at(window: &Window, across: f32, down: f32) -> [u8; 4] {
    let frame = window
        .engine_preview()
        .expect("the window has an engine")
        .the_frame_shown()
        .expect("a frame is shown");
    frame.pixel(
        (across * frame.width_in_pixels as f32) as u32,
        (down * frame.height_in_pixels as f32) as u32,
    )
}

fn differ_by_at_most(actual: [u8; 4], expected: [u8; 4], levels: u8) -> bool {
    actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= levels)
}

/// What the old drawing makes of a colour with the hue turned by `degrees`.
fn turned(colour: [u8; 4], degrees: f32) -> [u8; 4] {
    let mut pixel = colour;
    ColourMatrix::hue_rotation(degrees).apply_to_rgba_pixels(&mut pixel);
    pixel
}

fn select(window: &mut Window, clip: ClipId) {
    window.do_this(Command::select_clips(vec![clip], SelectionChange::Replace));
}

#[test]
fn the_preview_of_a_window_with_the_engine_is_the_picture_the_engine_made_where_the_layer_is_and_with_its_colours() {
    let (project, source, picture) = project_with_a_picture_file(Some(120.0));
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();

    let middle = middle_of_the_layer(&window, &picture_label(picture));
    let colour_of_the_layer = the_pixel_at(&window, middle.0, middle.1);
    assert!(
        differ_by_at_most(colour_of_the_layer, turned([255, 0, 0, 255], 120.0), 1),
        "the red picture file with its hue turned by 120 degrees is {:?}, not {colour_of_the_layer:?}",
        turned([255, 0, 0, 255], 120.0)
    );

    // The frame is made at the size the picture is shown at.
    let shown = the_picture_on_the_screen(&window);
    let frame = window.engine_preview().unwrap().the_frame_shown().unwrap();
    assert_eq!(
        (frame.width_in_pixels, frame.height_in_pixels),
        render_size_for(shown.size(), 1.0, PICTURE_SIZE)
    );
}

/// The code that a colour of this sRGB code comes out as on a screen that does `display` (what the arithmetic of the standards says), rounded.
fn as_shown_on(display: crate::project::DisplayTransform, code: u8) -> u8 {
    use crate::colour_space::TransferFunction;
    let light = TransferFunction::Srgb.decode_to_light(f64::from(code) / 255.0);
    (display.transfer_function().encode_from_light(light) * 255.0).round() as u8
}

#[test]
fn the_preview_is_made_in_linear_light_and_a_colour_comes_out_as_the_screen_shows_it_which_depends_on_the_display_setting()
 {
    use crate::project::DisplayTransform;
    let (project, source, picture) = project_with_a_picture_file(None);
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [200, 100, 50, 255]);
    window.wait_for_the_engine();
    let middle = middle_of_the_layer(&window, &picture_label(picture));

    // The default display is sRGB, which a picture file already is: a colour that is only shown comes out the same (the pipeline is the identity for it).
    let shown = the_pixel_at(&window, middle.0, middle.1);
    assert!(differ_by_at_most(shown, [200, 100, 50, 255], 1), "{shown:?}");

    // A screen that does the power 2.4 shows the same light with other codes: the preview is made of light, and the screen's curve is what it ends with.
    window.application_mut().environment.global_settings.display_transform = DisplayTransform::Gamma24;
    // (The window is drawn once, so that it asks for the frame of the new colour, and then the engine is waited for.)
    window.harness.step();
    window.wait_for_the_engine();
    let shown = the_pixel_at(&window, middle.0, middle.1);
    let expected = [200, 100, 50].map(|code| as_shown_on(DisplayTransform::Gamma24, code));
    assert!(
        differ_by_at_most(shown, [expected[0], expected[1], expected[2], 255], 1),
        "{shown:?} where the light of [200, 100, 50] on a 2.4 screen is {expected:?}"
    );
    assert_ne!(
        expected,
        [200, 100, 50],
        "a premise: the two screens differ for this colour"
    );
}

#[test]
fn the_preview_is_the_same_picture_when_the_project_is_put_together_in_another_working_space_and_the_pictures_are_made_again_for_it()
 {
    use crate::project::{ProjectSettings, WorkingColourSpace};
    let (project, source, picture) = project_with_a_picture_file(Some(40.0));
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [180, 60, 20, 255]);
    window.wait_for_the_engine();
    let middle = middle_of_the_layer(&window, &picture_label(picture));
    let in_rec709 = the_pixel_at(&window, middle.0, middle.1);
    let asked_before = window.engine_preview().unwrap().frames_asked_for();

    window.application_mut().document.project.set_settings(ProjectSettings {
        working_colour_space: Some(WorkingColourSpace::AcesCg),
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    });
    window.harness.step();
    window.wait_for_the_engine();

    assert!(
        window.engine_preview().unwrap().frames_asked_for() > asked_before,
        "a frame of another colour is asked for"
    );
    assert_eq!(window.engine_preview().unwrap().why_it_stopped(), None);
    let in_acescg = the_pixel_at(&window, middle.0, middle.1);
    assert!(
        differ_by_at_most(in_acescg, in_rec709, 1),
        "{in_acescg:?} in ACEScg and {in_rec709:?} in Rec.709: the same picture, put together in other primaries"
    );
}

#[test]
fn what_the_window_draws_to_show_where_things_are_and_what_the_mouse_takes_hold_of_is_where_it_was() {
    let (project, source, picture) = project_with_a_picture_file(None);
    let mut with_the_old_drawing = Window::showing(project.clone());
    decode_the_picture_file(&mut with_the_old_drawing, source, [255, 0, 0, 255]);
    select(&mut with_the_old_drawing, picture);
    let Some(mut with_the_engine) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut with_the_engine, source, [255, 0, 0, 255]);
    select(&mut with_the_engine, picture);
    with_the_engine.wait_for_the_engine();

    let mut labels = vec![picture_label(picture)];
    labels.extend(Corner::ALL.map(corner_handle_label));
    labels.push(video_label(the_video(&with_the_old_drawing)));
    for label in &labels {
        let old = with_the_old_drawing
            .harness
            .query_by_label(label)
            .map(|node| node.rect());
        let new = with_the_engine.harness.query_by_label(label).map(|node| node.rect());
        assert!(old.is_some(), "{label}: the old drawing does not have it either");
        assert_eq!(new, old, "{label}");
    }
    assert_eq!(
        the_picture_on_the_screen(&with_the_engine),
        the_picture_on_the_screen(&with_the_old_drawing)
    );
}

#[test]
fn while_the_engine_draws_the_window_does_not_make_the_pictures_it_makes_when_it_draws_them_itself() {
    // (The timeline draws the picture file too, small, and as it is, so there is a texture of it either way: what tells the preview's own is that it has the
    // colours of the layer.)
    let (project, source, _) = project_with_a_picture_file(Some(60.0));
    let mut with_the_old_drawing = Window::showing(project.clone());
    decode_the_picture_file(&mut with_the_old_drawing, source, [255, 0, 0, 255]);
    assert_eq!(
        with_the_old_drawing
            .application()
            .interface
            .textures
            .colours_of_the_picture_of_the_file(source),
        Some(ColourMatrix::hue_rotation(60.0)),
        "the old drawing makes the picture of the file with the colours of the layer"
    );

    let Some(mut with_the_engine) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut with_the_engine, source, [255, 0, 0, 255]);
    with_the_engine.wait_for_the_engine();
    assert_eq!(
        with_the_engine
            .application()
            .interface
            .textures
            .colours_of_the_picture_of_the_file(source),
        Some(ColourMatrix::NOTHING_CHANGES),
        "the window leaves the colours to the engine"
    );
}

#[test]
fn a_window_that_is_not_changing_asks_for_no_more_frames_and_puts_nothing_on_the_graphics_card() {
    let (project, source, _) = project_with_a_picture_file(Some(60.0));
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    let asked = window.engine_preview().unwrap().frames_asked_for();

    let sent = window.textures_uploaded_while(|window| window.harness.run_steps(8));

    assert_eq!(
        sent, 0,
        "pictures were put on the graphics card in 8 frames of a window that is not changing"
    );
    assert_eq!(window.engine_preview().unwrap().frames_asked_for(), asked);
}

#[test]
fn a_change_of_what_is_on_the_picture_is_a_new_frame_from_the_engine() {
    let (project, source, picture) = project_with_a_picture_file(Some(120.0));
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    let asked = window.engine_preview().unwrap().frames_asked_for();
    let (across, down) = middle_of_the_layer(&window, &picture_label(picture));

    window.do_this(Command::SetEffectValue {
        target: AnimationTarget::Clip(picture),
        property: HUE,
        value: PropertyValue::Number(240.0),
    });
    window.wait_for_the_engine();

    assert!(window.engine_preview().unwrap().frames_asked_for() > asked);
    assert!(differ_by_at_most(
        the_pixel_at(&window, across, down),
        turned([255, 0, 0, 255], 240.0),
        1
    ));
}

#[test]
fn zooming_in_shows_the_pixels_of_the_frame_as_blocks_and_does_not_make_the_frame_bigger_than_the_picture_is() {
    let (project, source, _) = project_with_a_picture_file(None);
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    let fitting = {
        let frame = window.engine_preview().unwrap().the_frame_shown().unwrap();
        (frame.width_in_pixels, frame.height_in_pixels)
    };
    assert!(
        fitting.0 < PICTURE_SIZE.0,
        "the picture is shown smaller than it is in the window of a test: {fitting:?}"
    );

    let middle = window.the_preview_panel().center();
    window.scroll_the_wheel(
        middle,
        vec2(0.0, WHEEL_FOR_ZOOMING_IN_AS_FAR_AS_IT_GOES),
        Modifiers::NONE,
    );
    window.wait_for_the_engine();

    let frame = window.engine_preview().unwrap().the_frame_shown().unwrap();
    assert_eq!(
        (frame.width_in_pixels, frame.height_in_pixels),
        PICTURE_SIZE,
        "the whole of the picture, and no more of it"
    );
    assert_eq!(
        window.engine_preview().unwrap().filter_it_is_shown_with(),
        Some(PictureFilter::Pixels),
        "blocks, when the pixels can be seen"
    );

    // Zoomed out, the engine has less to do.
    window.harness.get_by_label(FIT_BUTTON_LABEL).click();
    window.settle();
    window.scroll_the_wheel(
        middle,
        vec2(0.0, -200.0 * 2.0 * std::f32::consts::LN_2),
        Modifiers::NONE,
    );
    window.wait_for_the_engine();
    let frame = window.engine_preview().unwrap().the_frame_shown().unwrap();
    assert!(
        frame.width_in_pixels < fitting.0,
        "{} is not less than {}",
        frame.width_in_pixels,
        fitting.0
    );
}

/// Whether the frame has a pixel that is neither black nor the colour that stands for a video whose picture has not come.
fn the_frame_has_more_on_it_than_the_picture_that_has_not_come(window: &Window) -> bool {
    let placeholder = theme::TRACK_BACKGROUND.to_srgba_unmultiplied();
    window
        .engine_preview()
        .and_then(|preview| preview.the_frame_shown())
        .is_some_and(|frame| {
            frame
                .rgba_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[..3] != placeholder[..3] && pixel[..3] != [0, 0, 0])
        })
}

#[test]
fn the_thumbnail_that_stands_in_for_a_video_whose_picture_has_not_come_is_in_the_frame_with_the_colours_of_the_layer() {
    let mut project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(video), 90.0);
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    // A thumbnail of the whole second the playhead is in: all of one colour.
    let library = &mut window
        .application_mut()
        .sources
        .files
        .get_mut(&VIDEO)
        .unwrap()
        .thumbnails
        .library;
    *library = ThumbnailLibrary::with_room_for(10);
    library.insert(
        ThumbnailKey { level: 0, index: 0 },
        CompressedPicture::from_rgb_pixels(
            &[[200u8, 100, 50]; 8 * 8].concat(),
            PictureSize {
                width_in_pixels: 8,
                height_in_pixels: 8,
            },
        )
        .unwrap(),
    );
    window.draw_frames_until(
        "the thumbnail is in the frame",
        the_frame_has_more_on_it_than_the_picture_that_has_not_come,
    );
    window.wait_for_the_engine();

    let colour = the_pixel_at(&window, 0.5, 0.5);
    assert!(
        differ_by_at_most(colour, turned([200, 100, 50, 255], 90.0), 6),
        "{colour:?} is not the thumbnail (200, 100, 50), with its hue turned by 90 degrees: {:?}",
        turned([200, 100, 50, 255], 90.0)
    );
    // (The timeline draws thumbnails too, as they are: what the preview makes of one has the colours of the layer.)
    let ways = window
        .application()
        .interface
        .textures
        .ways_the_colours_of_the_thumbnails_were_changed();
    assert!(
        ways.iter()
            .all(|pattern| *pattern == ColourMatrix::NOTHING_CHANGES.bit_pattern()),
        "the window made a thumbnail with the colours of the layer itself: {ways:?}"
    );
}

#[test]
fn the_fast_forward_symbol_is_in_the_frame_where_the_window_says_it_is() {
    let mut project = project_of(300);
    let video = project.tracks()[0].entries()[0].id();
    project.set_clip_speed(video, SpeedFactor::new(4).unwrap()).unwrap();
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    window.wait_for_the_engine();

    // The place the window says the symbol is in (where the mouse can take hold of it), as fractions of the picture.
    let picture = the_picture_on_the_screen(&window);
    let symbol = window.rectangle_of("Fast-forward symbol");
    let (left, top) = (
        (symbol.left() - picture.left()) / picture.width(),
        (symbol.top() - picture.top()) / picture.height(),
    );
    let (right, bottom) = (
        (symbol.right() - picture.left()) / picture.width(),
        (symbol.bottom() - picture.top()) / picture.height(),
    );
    let frame = window.engine_preview().unwrap().the_frame_shown().unwrap();
    let placeholder = theme::TRACK_BACKGROUND.to_srgba_unmultiplied();
    let (mut inside, mut outside) = (0, 0);
    for down in 0..frame.height_in_pixels {
        for across in 0..frame.width_in_pixels {
            let (x, y) = (
                (across as f32 + 0.5) / frame.width_in_pixels as f32,
                (down as f32 + 0.5) / frame.height_in_pixels as f32,
            );
            let pixel = frame.pixel(across, down);
            let is_the_symbol = pixel[..3] != placeholder[..3];
            let in_its_place = x >= left - 0.01 && x <= right + 0.01 && y >= top - 0.01 && y <= bottom + 0.01;
            match (is_the_symbol, in_its_place) {
                (true, true) => inside += 1,
                (true, false) => outside += 1,
                _ => {}
            }
        }
    }
    assert!(inside > 50, "only {inside} pixels of the symbol are in its place");
    assert_eq!(
        outside, 0,
        "pixels that are not the colour of the layer are outside the place of the symbol"
    );
}

#[test]
fn when_the_engine_stops_the_window_says_so_and_makes_the_preview_the_old_way_from_then_on() {
    let (project, source, _) = project_with_a_picture_file(Some(60.0));
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    assert!(
        window.application().playback.planes_wanted,
        "while the engine draws, the pictures of videos are asked for in planes"
    );

    window.engine_preview().unwrap().stop();
    window.draw_frames_until("the old drawing is back", |window| window.engine_preview().is_none());

    let application = window.application();
    assert!(!application.playback.planes_wanted);
    match &application.interface.dialog {
        Some(Dialog::Message { title, text }) => {
            assert_eq!(title, "The picture engine stopped");
            assert!(text.contains("old way"), "{text}");
            // (Since phase 2 of the plugin plan the export is made by a picture engine too: it was said that it was not.)
            assert!(!text.contains("does not use the picture engine"), "{text}");
            assert!(text.contains("an export starts a picture engine of its own"), "{text}");
        }
        other => panic!("a message was expected: {other:?}"),
    }
    // The picture of the file is drawn by the window itself now, with the colours of the layer.
    window.settle();
    assert_eq!(
        window
            .application()
            .interface
            .textures
            .colours_of_the_picture_of_the_file(source),
        Some(ColourMatrix::hue_rotation(60.0))
    );
}

#[test]
fn the_preview_is_made_by_the_engine_unless_the_old_drawing_is_asked_for() {
    use super::{WhichDrawing, which_drawing_is_asked_for};
    // (It was the other way round until 2026-10-16: the engine was started by `FASTFORWARDRS_PREVIEW=engine` and by nothing else, until the maintainer had
    // looked at it on their graphics cards and said that it is to be the default.)
    assert_eq!(which_drawing_is_asked_for(None), WhichDrawing::EngineByDefault);
    assert_eq!(which_drawing_is_asked_for(Some("engine")), WhichDrawing::EngineByName);
    assert_eq!(which_drawing_is_asked_for(Some("old")), WhichDrawing::OldDrawing);
    assert_eq!(
        which_drawing_is_asked_for(Some("")),
        WhichDrawing::EngineByDefault,
        "a variable that says nothing says nothing"
    );
    assert_eq!(
        which_drawing_is_asked_for(Some("something that is not a name")),
        WhichDrawing::EngineByDefault,
        "a name that is not one is not the old drawing"
    );
}

#[test]
fn what_is_done_about_the_engine_follows_what_is_asked_for_and_whether_the_window_is_drawn_by_wgpu() {
    use super::{
        WhatToDoAboutTheEngine::{LeaveItAlone, LeaveItAloneAndSayWhy, Start},
        WhichDrawing, what_to_do_about_the_engine,
    };
    // The engine is started whenever the window can have one, unless the old drawing is asked for.
    assert_eq!(what_to_do_about_the_engine(WhichDrawing::EngineByDefault, true), Start);
    assert_eq!(what_to_do_about_the_engine(WhichDrawing::EngineByName, true), Start);
    assert_eq!(
        what_to_do_about_the_engine(WhichDrawing::OldDrawing, true),
        LeaveItAlone
    );
    assert_eq!(
        what_to_do_about_the_engine(WhichDrawing::OldDrawing, false),
        LeaveItAlone
    );
    // A window that is not drawn by wgpu cannot have it; only someone who asked for it by name is told so.
    assert_eq!(
        what_to_do_about_the_engine(WhichDrawing::EngineByDefault, false),
        LeaveItAlone
    );
    assert_eq!(
        what_to_do_about_the_engine(WhichDrawing::EngineByName, false),
        LeaveItAloneAndSayWhy
    );
}

/// Whether the window is showing the message that says that the graphics device was replaced.
fn is_announcing_a_replaced_device(window: &Window) -> bool {
    matches!(
        &window.application().interface.dialog,
        Some(Dialog::Message { title, .. }) if title == "The graphics device was replaced"
    )
}

#[test]
fn when_the_graphics_device_is_replaced_the_window_says_so_and_goes_on_with_the_engine() {
    let devices = DevicesMade::default();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.watch_the_devices = Some(devices.watcher());
    let (project, source, _) = project_with_a_picture_file(None);
    let Some(mut window) = Window::showing_with_the_engine_set_up(project, settings) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    assert_eq!(window.application().interface.dialog, None, "nothing has gone wrong");

    // The card is reset. The window does not know until it asks for another frame, and the engine finds out.
    devices.nth(1, std::time::Duration::from_secs(30)).destroy();
    window.show_another_project(project_of(100));
    window.draw_frames_until("the replaced device is announced", is_announcing_a_replaced_device);

    let Some(Dialog::Message { text, .. }) = &window.application().interface.dialog else {
        panic!("a message was expected");
    };
    assert!(text.contains("lost"), "it says why: {text}");
    assert!(text.contains("goes on"), "it says that the engine goes on: {text}");
    assert!(text.contains("Nothing of the project is changed"), "{text}");
    assert_eq!(devices.count(), 2);
    assert!(window.engine_preview().is_some(), "the engine is not given up");
    assert_eq!(window.engine_preview().unwrap().why_it_stopped(), None);
    window.wait_for_the_engine();
    assert!(window.application().playback.planes_wanted, "and it still draws");
}

#[test]
fn the_announcement_of_a_replaced_device_waits_for_a_question_that_is_being_asked_and_does_not_take_its_place() {
    let devices = DevicesMade::default();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.watch_the_devices = Some(devices.watcher());
    let (project, source, _) = project_with_a_picture_file(None);
    let Some(mut window) = Window::showing_with_the_engine_set_up(project, settings) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    let question = Dialog::ConfirmExportWithMissingEffects {
        effects: vec!["an effect that is not here".to_owned()],
    };
    window.application_mut().interface.dialog = Some(question.clone());

    devices.nth(1, std::time::Duration::from_secs(30)).destroy();
    window.show_another_project(project_of(100));
    window.draw_frames_until("the engine has replaced its device", |_| devices.count() == 2);
    for _ in 0..20 {
        window.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        window.application().interface.dialog,
        Some(question),
        "the question is still the one that is asked"
    );

    window.application_mut().interface.dialog = None;
    window.draw_frames_until(
        "the replaced device is announced when nothing is asked",
        is_announcing_a_replaced_device,
    );
}

#[test]
fn a_replaced_device_is_announced_under_a_message_that_is_showing_and_not_in_place_of_it() {
    let devices = DevicesMade::default();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.watch_the_devices = Some(devices.watcher());
    let (project, source, _) = project_with_a_picture_file(None);
    let Some(mut window) = Window::showing_with_the_engine_set_up(project, settings) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    window.application_mut().interface.dialog = Some(Dialog::Message {
        title: "Something else".to_owned(),
        text: "that was said before".to_owned(),
    });

    devices.nth(1, std::time::Duration::from_secs(30)).destroy();
    window.show_another_project(project_of(100));
    window.draw_frames_until("the replaced device is announced", |window| {
        matches!(&window.application().interface.dialog, Some(Dialog::Message { text, .. }) if text.contains("lost"))
    });

    let Some(Dialog::Message { title, text }) = &window.application().interface.dialog else {
        panic!("a message was expected");
    };
    assert!(title.starts_with("Something else"), "{title}");
    assert!(text.starts_with("that was said before"), "{text}");
}

#[test]
fn another_project_is_asked_for_as_a_frame_of_its_own_and_what_is_shown_is_not_the_old_ones() {
    let (project, source, _) = project_with_a_picture_file(None);
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [255, 0, 0, 255]);
    window.wait_for_the_engine();
    let asked = window.engine_preview().unwrap().frames_asked_for();
    let generation = window.engine_preview().unwrap().generation();

    window.show_another_project(project_of(100));
    window.wait_for_the_engine();

    assert!(
        window.engine_preview().unwrap().generation() > generation,
        "the window let go of what it knew of its files, and the engine's side was told"
    );
    assert!(window.engine_preview().unwrap().frames_asked_for() > asked);
    // The video of the other project has no picture: the layer that stands for it, and nothing of the picture file that the old project had.
    let placeholder = theme::TRACK_BACKGROUND.to_srgba_unmultiplied();
    assert_eq!(the_pixel_at(&window, 0.5, 0.5)[..3], placeholder[..3]);
}

#[test]
fn a_window_that_is_made_with_no_engine_has_none_and_does_not_ask_for_pictures_in_planes() {
    let window = Window::showing(project_of(300));
    assert!(window.engine_preview().is_none());
    assert!(!window.application().playback.planes_wanted);
}

// ---- an effect that depends on earlier frames, and a jump (EN-14, option 3) ----

/// A shader that keeps a buffer from one frame to the next: it depends on earlier frames.
const REMEMBERS: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true }, {} ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(memory); }\n";

fn go_to_the_frame(window: &mut Window, frame: u64) {
    window.do_this(Command::SeekTo(TimelineFrameIndex::new(frame)));
    window.wait_for_the_engine();
}

fn says_the_picture_is_not_exact(window: &Window) -> bool {
    window
        .harness
        .query_by_label(crate::application::preview_panel::THE_PICTURE_IS_NOT_EXACT_NOTE)
        .is_some()
}

#[test]
fn a_jump_in_a_project_with_an_effect_that_depends_on_earlier_frames_says_over_the_picture_that_it_is_not_exact_and_an_edit_makes_every_jump_say_so()
 {
    let (mut project, source, clip) = project_with_a_picture_file(None);
    let file = crate::isf::read(REMEMBERS).unwrap();
    let kind =
        crate::project::EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "remembers.fs", REMEMBERS)).unwrap();
    assert!(kind.is_installed() && kind.depends_on_earlier_frames(), "a premise");
    project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    let Some(mut window) = Window::showing_with_the_engine(project) else {
        return;
    };
    decode_the_picture_file(&mut window, source, [200, 100, 50, 255]);
    // A checkpoint every 30 frames of the effect, which begins with the clip (frame 0).
    let global = window.application().global_settings();
    window.do_this(Command::SetGlobalSettings(crate::project::Settings {
        checkpoint_interval: crate::project::CheckpointInterval::Every30Frames,
        ..global
    }));

    for frame in 0..=40 {
        go_to_the_frame(&mut window, frame);
    }
    assert!(
        !says_the_picture_is_not_exact(&window),
        "it was played in order from the first frame"
    );
    assert!(!window.engine_preview().unwrap().the_picture_shown_is_not_exact());

    // A jump to frame 35: the checkpoint is of frame 30, and frame 34 is the one before it. Not exact, and the window says so, over the picture.
    go_to_the_frame(&mut window, 35);
    assert!(window.engine_preview().unwrap().the_picture_shown_is_not_exact());
    assert!(says_the_picture_is_not_exact(&window));

    // A jump to frame 31: the checkpoint is of the frame before it: exact, and nothing is said.
    go_to_the_frame(&mut window, 31);
    assert!(!window.engine_preview().unwrap().the_picture_shown_is_not_exact());
    assert!(!says_the_picture_is_not_exact(&window));

    // An edit of the project makes the checkpoints go: what they are copies of is not the project that there is. (The title of a track changes nothing that is seen.) The same jump
    // that was exact is not now: the effect starts clear.
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackTitle {
        track,
        title: "an edit".to_owned(),
    });
    go_to_the_frame(&mut window, 38);
    go_to_the_frame(&mut window, 31);
    assert!(
        says_the_picture_is_not_exact(&window),
        "no checkpoint is left to go on from"
    );
}

// ---- the lead-up on a seek (EN-14, option 1) ----

/// A shader that adds a tenth of red to what it kept in every frame and shows what it kept: the red of the picture is how many frames of the effect have gone into it.
const COUNTS_THE_FRAMES: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true }, {} ] }*/
void main() {
    if (PASSINDEX == 0) {
        vec4 held = IMG_THIS_PIXEL(memory);
        gl_FragColor = vec4(held.r + 0.1, 0.0, 0.0, 1.0);
    } else {
        gl_FragColor = IMG_THIS_PIXEL(memory);
    }
}
";

/// A window with a picture file that has the effect that counts the frames on it (and nothing else: **no video, which would need ffmpeg to be decoded for the lead-up**, and a window of a test has
/// none; the lead-up of a video has tests with the real one), a checkpoint every 30 frames, and the lead-up on a seek as `lead_up_on_seek` says.
fn a_window_whose_picture_counts_its_frames(lead_up_on_seek: bool) -> Option<Window> {
    a_window_whose_picture_has_the_effect(COUNTS_THE_FRAMES, lead_up_on_seek)
}

/// The same, with the effect that `shader` is (an ISF shader that depends on earlier frames).
fn a_window_whose_picture_has_the_effect(shader: &str, lead_up_on_seek: bool) -> Option<Window> {
    let mut project = Project::empty();
    let picture_file = picture_source();
    let source = picture_file.identifier;
    let clip = project
        .import_source(picture_file, TimelineFrameIndex::new(0))
        .unwrap()
        .clip
        .expect("a picture makes a clip");
    let file = crate::isf::read(shader).unwrap();
    let kind =
        crate::project::EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "counts.fs", shader)).unwrap();
    assert!(kind.is_installed() && kind.depends_on_earlier_frames(), "a premise");
    project.add_effect(AnimationTarget::Clip(clip), kind).unwrap();
    let mut window = Window::showing_with_the_engine(project)?;
    decode_the_picture_file(&mut window, source, [200, 100, 50, 255]);
    let global = window.application().global_settings();
    window.do_this(Command::SetGlobalSettings(crate::project::Settings {
        checkpoint_interval: crate::project::CheckpointInterval::Every30Frames,
        ..global
    }));
    window.do_this(Command::SetGenerateLeadUpOnSeek(lead_up_on_seek));
    Some(window)
}

fn the_red_of_the_picture(window: &Window) -> u8 {
    the_pixel_at(window, 0.5, 0.5)[0]
}

fn is_not_exact(window: &Window) -> bool {
    window.engine_preview().unwrap().the_picture_shown_is_not_exact()
}

/// Draws the window for `how_long`, as a window that is looked at is.
fn draw_the_window_for(window: &mut Window, how_long: std::time::Duration) {
    let started = std::time::Instant::now();
    while started.elapsed() < how_long {
        window.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_seek_that_leaves_the_picture_not_exact_is_made_exact_by_the_lead_up_when_the_picture_stays_and_it_is_the_picture_that_playing_from_the_start_makes()
 {
    let Some(mut window) = a_window_whose_picture_counts_its_frames(true) else {
        return;
    };
    // Played in order from the first frame: nine frames of the effect, nine tenths of red.
    for frame in 0..=8 {
        go_to_the_frame(&mut window, frame);
    }
    assert!(!is_not_exact(&window));
    let red_made_by_playing = the_red_of_the_picture(&window);
    assert!(red_made_by_playing > 200, "nine tenths of 255: {red_made_by_playing}");

    // A jump away and back to frame 8: the state goes on from the checkpoint of frame 0, and the picture is not exact and shows two tenths.
    go_to_the_frame(&mut window, 20);
    go_to_the_frame(&mut window, 8);
    assert!(is_not_exact(&window), "it went on from the checkpoint");
    let red_after_the_jump = the_red_of_the_picture(&window);
    assert!(
        red_after_the_jump + 100 < red_made_by_playing,
        "a picture of two tenths and not nine: {red_after_the_jump} and {red_made_by_playing}"
    );

    // It stays: the lead-up is wanted, made, and the picture that is made again is exact, and is what playing made.
    window.draw_frames_until("the lead-up has made the picture exact", |window| !is_not_exact(window));
    let red_after_the_lead_up = the_red_of_the_picture(&window);
    assert!(
        red_after_the_lead_up.abs_diff(red_made_by_playing) <= 1,
        "the lead-up made the state that playing makes: {red_after_the_lead_up} and {red_made_by_playing}"
    );
    assert!(!window.engine_preview().unwrap().a_lead_up_is_being_made());
    assert_eq!(window.engine_preview().unwrap().why_it_stopped(), None);

    // The frame after it goes on from the state that the lead-up made: ten tenths, which is all the red there is.
    go_to_the_frame(&mut window, 9);
    assert!(!is_not_exact(&window), "and it is exact");
    assert!(the_red_of_the_picture(&window) > red_after_the_lead_up);
}

#[test]
fn with_the_checkbox_off_the_picture_stays_not_exact_and_no_lead_up_is_made_and_turning_it_on_makes_one() {
    let Some(mut window) = a_window_whose_picture_counts_its_frames(false) else {
        return;
    };
    for frame in 0..=8 {
        go_to_the_frame(&mut window, frame);
    }
    let red_made_by_playing = the_red_of_the_picture(&window);
    go_to_the_frame(&mut window, 20);
    go_to_the_frame(&mut window, 8);
    assert!(is_not_exact(&window));

    // It stays for longer than the time that a lead-up waits for a picture that stays: none is made.
    draw_the_window_for(&mut window, std::time::Duration::from_millis(700));
    assert!(is_not_exact(&window), "no lead-up is made with the checkbox off");
    assert!(!window.engine_preview().unwrap().a_lead_up_is_being_made());

    // The checkbox is turned on, and the picture that stays has a lead-up made for it.
    window.do_this(Command::SetGenerateLeadUpOnSeek(true));
    window.draw_frames_until("the lead-up has made the picture exact", |window| !is_not_exact(window));
    assert!(the_red_of_the_picture(&window).abs_diff(red_made_by_playing) <= 1);
}

#[test]
fn a_seek_before_the_picture_has_stayed_has_no_lead_up_made_for_the_frame_that_is_left_and_the_next_stay_is_made_exact()
{
    let Some(mut window) = a_window_whose_picture_counts_its_frames(true) else {
        return;
    };
    for frame in 0..=8 {
        go_to_the_frame(&mut window, frame);
    }
    go_to_the_frame(&mut window, 20);
    go_to_the_frame(&mut window, 8);
    // Another seek at once, before the picture has stayed for a while: nothing is made for frame 8.
    go_to_the_frame(&mut window, 9);
    assert!(!window.engine_preview().unwrap().a_lead_up_is_being_made());
    window.draw_frames_until("the lead-up has made the picture of frame 9 exact", |window| {
        !is_not_exact(window)
    });
    assert!(the_red_of_the_picture(&window) > 200);
}

/// The requests that a scripted engine has been given and the window has not looked at yet, gathered over the frames drawn (so that none is lost).
#[derive(Default)]
struct WhatTheWindowAsked {
    pictures: Vec<(crate::engine::PictureId, (u32, u32))>,
    frames: Vec<(u64, crate::engine::FrameDescription)>,
    lead_ups: Vec<(u64, std::sync::mpsc::Receiver<crate::engine::LeadUpItem>)>,
    /// The chases that the engine was asked for (they go on to the end of the project, and not up to a frame).
    chases: Vec<(u64, std::sync::mpsc::Receiver<crate::engine::LeadUpItem>)>,
    cancelled_lead_ups: Vec<u64>,
}

impl WhatTheWindowAsked {
    fn take_from(&mut self, requests: &std::sync::mpsc::Receiver<crate::engine::EngineRequest>) {
        use crate::engine::EngineRequest;
        while let Ok(request) = requests.try_recv() {
            match request {
                EngineRequest::Upload { id, data } => {
                    self.pictures.push((id, (data.width_in_pixels, data.height_in_pixels)))
                }
                EngineRequest::Render { serial, description } => self.frames.push((serial, description)),
                EngineRequest::StartTheLeadUp { generation, items } => self.lead_ups.push((generation, items)),
                EngineRequest::StartTheChase { generation, items } => self.chases.push((generation, items)),
                EngineRequest::CancelTheLeadUp { generation } => self.cancelled_lead_ups.push(generation),
                _ => {}
            }
        }
    }
}

/// Draws the window until `done`, gathering what it asks of the scripted engine.
fn draw_until_asked(
    window: &mut Window,
    requests: &std::sync::mpsc::Receiver<crate::engine::EngineRequest>,
    asked: &mut WhatTheWindowAsked,
    what: &str,
    done: impl Fn(&WhatTheWindowAsked, &Window) -> bool,
) {
    let started = std::time::Instant::now();
    loop {
        window.harness.step();
        asked.take_from(requests);
        if done(asked, window) {
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "it did not happen: {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
}

fn a_frame_that_the_scripted_engine_made(serial: u64, approximate: bool) -> crate::engine::EngineEvent {
    crate::engine::EngineEvent::Frame {
        serial,
        frame: std::sync::Arc::new(crate::engine::RenderedFrame {
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

/// A window of the picture that counts its frames, with a preview that is made with an engine that is scripted (it makes no frame, and what the window asks of it is read), with the playhead
/// at frame 8, the frame asked for, and the engine saying that it is not exact: and the lead-up that the window makes of that, started and waiting for the engine to take what it describes.
struct WindowWithALeadUpThatIsBeingMade {
    window: Window,
    events: std::sync::mpsc::Sender<crate::engine::EngineEvent>,
    requests: std::sync::mpsc::Receiver<crate::engine::EngineRequest>,
    asked: WhatTheWindowAsked,
    /// The frame that the preview asked for, as the window described it, and the number of the request.
    the_frame_on_the_screen: (u64, crate::engine::FrameDescription),
    /// What the lead-up gives the engine.
    items: std::sync::mpsc::Receiver<crate::engine::LeadUpItem>,
    generation: u64,
}

fn a_window_that_is_making_a_lead_up() -> Option<WindowWithALeadUpThatIsBeingMade> {
    use crate::application::preview_panel::EnginePreview;
    use crate::engine::Engine;
    let mut window = a_window_whose_picture_counts_its_frames(true)?;
    let (engine, events, requests) = Engine::scripted_for_tests();
    window.application_mut().interface.engine_preview = Some(EnginePreview::with_engine(engine));
    let mut asked = WhatTheWindowAsked::default();
    window.do_this(Command::SeekTo(TimelineFrameIndex::new(8)));
    draw_until_asked(
        &mut window,
        &requests,
        &mut asked,
        "the frame is asked for",
        |asked, _| !asked.frames.is_empty(),
    );
    let the_frame_on_the_screen = asked.frames.last().unwrap().clone();
    events
        .send(a_frame_that_the_scripted_engine_made(the_frame_on_the_screen.0, true))
        .unwrap();
    draw_until_asked(
        &mut window,
        &requests,
        &mut asked,
        "the lead-up is started",
        |asked, _| !asked.lead_ups.is_empty(),
    );
    let (generation, items) = asked.lead_ups.remove(0);
    Some(WindowWithALeadUpThatIsBeingMade {
        window,
        events,
        requests,
        asked,
        the_frame_on_the_screen,
        items,
        generation,
    })
}

#[test]
fn the_lead_up_that_the_window_makes_is_of_the_frames_before_the_one_on_the_screen_made_with_the_states_the_size_and_the_colour_of_the_preview()
 {
    use crate::application::preview_panel::{
        THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE, THE_PICTURE_IS_NOT_EXACT_NOTE,
    };
    use crate::engine::{EngineEvent, LeadUpItem};
    let Some(mut made) = a_window_that_is_making_a_lead_up() else {
        return;
    };
    assert_eq!(made.generation, 1);
    assert!(made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    // The line over the picture says what it waits for.
    made.window.harness.step();
    assert!(
        made.window
            .harness
            .query_by_label(THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE)
            .is_some()
    );
    assert!(
        made.window
            .harness
            .query_by_label(THE_PICTURE_IS_NOT_EXACT_NOTE)
            .is_none()
    );

    // What it gives the engine, until its end.
    let started = std::time::Instant::now();
    let mut given: Vec<LeadUpItem> = Vec::new();
    loop {
        match made.items.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(item) => given.push(item),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(30),
                    "the lead-up did not end"
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(matches!(given.last(), Some(LeadUpItem::End)), "it ends with its end");
    let frames: Vec<&crate::engine::FrameDescription> = given
        .iter()
        .filter_map(|item| match item {
            LeadUpItem::Frame(description) => Some(description),
            _ => None,
        })
        .collect();
    // The effect has gone on for 8 frames at frame 8, so the lead-up is those 8: frames 0 to 7 of the effect's own clock.
    assert_eq!(frames.len(), 8);
    let state_of = |description: &crate::engine::FrameDescription| {
        let states: Vec<crate::engine::EffectState> = description
            .flatten()
            .layers
            .iter()
            .flat_map(|flat| flat.layer.effects.iter().filter_map(|effect| effect.state))
            .collect();
        assert_eq!(states.len(), 1, "one effect with a state");
        states[0]
    };
    let state_of_the_preview = state_of(&made.the_frame_on_the_screen.1);
    assert_eq!(state_of_the_preview.frame, 8, "the playhead is at frame 8");
    for (number, description) in frames.iter().enumerate() {
        let state = state_of(description);
        assert_eq!(state.frame, number as u64, "the frames of the effect's clock, in order");
        assert_eq!(
            state.id, state_of_the_preview.id,
            "**the same state**: the frame after the lead-up goes on from the state that it made"
        );
        assert_eq!(
            description.render_size, made.the_frame_on_the_screen.1.render_size,
            "made at the size of the picture on the screen"
        );
        assert_eq!(
            description.colour, made.the_frame_on_the_screen.1.colour,
            "and in its colour"
        );
    }
    // The picture of the layer is the picture file that the window has decoded, as big as the preview made it (which is what a state is the size of), under an id that is not the preview's.
    let pictures_of_the_lead_up: Vec<(crate::engine::PictureId, (u32, u32))> = given
        .iter()
        .filter_map(|item| match item {
            LeadUpItem::Picture { id, data } => Some((*id, (data.width_in_pixels, data.height_in_pixels))),
            _ => None,
        })
        .collect();
    assert!(!pictures_of_the_lead_up.is_empty());
    for (id, size) in &pictures_of_the_lead_up {
        assert!(id.is_in_the_lead_up());
        assert!(
            made.asked.pictures.iter().any(|(_, preview_size)| preview_size == size),
            "the preview sent a picture of {size:?}: {:?}",
            made.asked.pictures
        );
        assert!(made.asked.pictures.iter().all(|(preview_id, _)| preview_id != id));
    }

    // The engine says that it is done: the frame on the screen is asked for again, and the line over the picture is gone when the frame is exact.
    made.events
        .send(EngineEvent::LeadUpDone {
            generation: made.generation,
            frames_made: 8,
        })
        .unwrap();
    let asked_before = made.asked.frames.len();
    draw_until_asked(
        &mut made.window,
        &made.requests,
        &mut made.asked,
        "the frame is asked for again",
        |asked, _| asked.frames.len() > asked_before,
    );
    let (serial, again) = made.asked.frames.last().unwrap().clone();
    assert_eq!(again, made.the_frame_on_the_screen.1, "the same frame");
    assert!(!made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    made.events
        .send(a_frame_that_the_scripted_engine_made(serial, false))
        .unwrap();
    draw_until_asked(
        &mut made.window,
        &made.requests,
        &mut made.asked,
        "the frame is shown",
        |_, window| !is_not_exact(window),
    );
    assert!(
        made.window
            .harness
            .query_by_label(THE_PICTURE_IS_NOT_EXACT_NOTE)
            .is_none()
    );
    assert!(
        made.window
            .harness
            .query_by_label(THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE)
            .is_none()
    );
}

#[test]
fn turning_the_checkbox_off_while_the_lead_up_is_made_cancels_it_and_the_engine_is_told() {
    let Some(mut made) = a_window_that_is_making_a_lead_up() else {
        return;
    };
    assert!(made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    made.window.do_this(Command::SetGenerateLeadUpOnSeek(false));
    draw_until_asked(
        &mut made.window,
        &made.requests,
        &mut made.asked,
        "the lead-up is cancelled",
        |asked, _| !asked.cancelled_lead_ups.is_empty(),
    );
    assert_eq!(made.asked.cancelled_lead_ups, [made.generation]);
    assert!(!made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    // The thread that describes the frames finds that nobody is listening once the engine lets go (here: when the test does, which is what the engine does).
    drop(made.items);
}

// ---- the chase: the lead-up of a jump while the project plays (EN-14) ----

/// A shader that adds a sixteenth of red to what it kept in every frame, in a buffer of half floats, and starts again at nothing when there is a whole: the red of the picture is how many frames of the effect
/// have gone into it **modulo 16**, so it does not stop at the most there is, as the red of the shader that counts tenths does (which is why playing for longer than ten frames cannot tell an exact picture
/// from one that is not).
const COUNTS_THE_FRAMES_IN_A_CYCLE: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true, \"FLOAT\": true }, {} ] }*/
void main() {
    if (PASSINDEX == 0) {
        vec4 held = IMG_THIS_PIXEL(memory);
        gl_FragColor = vec4(fract(held.r + 0.0625), 0.0, 0.0, 1.0);
    } else {
        gl_FragColor = IMG_THIS_PIXEL(memory);
    }
}
";

/// How many frames the picture of the window of [`a_window_whose_picture_counts_in_a_cycle_at_one_frame_a_second`] lasts: forty seconds.
const FRAMES_OF_THE_SLOW_PROJECT: u64 = 40;

/// The same window as [`a_window_whose_picture_counts_its_frames`], with the shader that counts in a cycle, and a project of **one frame a second**, which lasts forty frames. A frame lasts a second,
/// which the software adapter makes a frame in with a great deal of time to spare even when the computer is busy: these tests play in real time, and the machines that run them are not all
/// as fast as the one they were written on (at ten frames a second a frame took about the time that a frame lasts, and a chase caught up with the playhead only after the project had stopped). A chase that
/// has to catch up with a playhead needs the engine to make more than one frame for each that the playhead moves, and a second is a long time for it.
fn a_window_whose_picture_counts_in_a_cycle_at_one_frame_a_second() -> Option<(Window, MutexGuard<'static, ()>)> {
    // (The tests that play this window in real time take turns: each needs the engine to make more frames a second than the playhead moves, and three of them at once, with the
    // other tests of the program, were too many for the runners of the CI, which failed about one run in three with a chase that did not catch up in time.)
    static TURNS: Mutex<()> = Mutex::new(());
    let turn = TURNS.lock().unwrap_or_else(PoisonError::into_inner);
    let mut window = a_window_whose_picture_has_the_effect(COUNTS_THE_FRAMES_IN_A_CYCLE, true)?;
    window.do_this(Command::SetProjectFrameRate(
        crate::project::FrameRate::from_whole_frames_per_second(1).unwrap(),
    ));
    let clip = window.clip(0, 0);
    window.do_this(Command::SetClipDuration {
        clip,
        frames: FRAMES_OF_THE_SLOW_PROJECT,
    });
    assert_eq!(
        window.application().project().total_frame_count(),
        FRAMES_OF_THE_SLOW_PROJECT,
        "a premise: forty seconds of picture at one frame a second"
    );
    Some((window, turn))
}

/// The red of the picture of each of the frames 0 to 15 of the effect when it is made in order, which is what the red of the picture of any frame is, modulo 16, when the picture is exact.
/// The window is paused, and ends up at frame 15.
fn the_reds_of_the_picture_by_the_frame_of_the_effect_modulo_16(window: &mut Window) -> [u8; 16] {
    let mut reds = [0; 16];
    for frame in 0..16 {
        go_to_the_frame(window, frame);
        assert!(!is_not_exact(window), "frame {frame} was made in order from the first");
        reds[frame as usize] = the_red_of_the_picture(window);
    }
    let mut different = reds.to_vec();
    different.sort_unstable();
    different.dedup();
    assert_eq!(
        different.len(),
        16,
        "a premise: a picture that is exact can be told from one that is a few frames short: {reds:?}"
    );
    reds
}

/// Plays the project from where it is until it has gone on to `frame`, and then jumps to `to`: **a jump while the project plays**, which leaves the picture not exact.
fn play_to_and_then_jump_while_playing(window: &mut Window, frame: u64, to: u64) {
    window.do_this(Command::Play);
    window.draw_frames_until("the project has played on", |window| {
        window.application().current_frame().value() >= frame
    });
    assert!(window.application().is_playing());
    assert!(!is_not_exact(window), "it was played in order from frame 15");
    window.do_this(Command::SeekTo(TimelineFrameIndex::new(to)));
    window.draw_frames_until("the picture of the jump is not exact", is_not_exact);
}

#[test]
fn a_jump_while_the_project_plays_is_made_exact_by_a_chase_that_catches_up_with_the_playhead_and_the_pictures_after_it_are_those_that_playing_makes()
 {
    let Some((mut window, _turn)) = a_window_whose_picture_counts_in_a_cycle_at_one_frame_a_second() else {
        return;
    };
    let began = std::time::Instant::now();
    let reds = the_reds_of_the_picture_by_the_frame_of_the_effect_modulo_16(&mut window);
    // (A chase that is to catch up with a playhead that moves a frame a second needs the engine to make several frames a second: more than one for each that the playhead moves, and ten
    // more for the frames the shader has to be run through before the one that is shown. Measured: where this test passes a frame takes 70 to 100 milliseconds here, and where six of it
    // run at once, or a runner of the CI has other tests running on it, it takes 170 to 190 and the chase does not catch up in time (it is given up after twenty seconds). So a computer
    // that takes more than 120 milliseconds for a frame is one that cannot show it, and this test says so and stops instead of failing on the speed of the machine.)
    let a_frame_took = began.elapsed() / 16;
    eprintln!("the engine took {a_frame_took:?} for a frame in this window");
    if a_frame_took > std::time::Duration::from_millis(120) {
        eprintln!(
            "SKIPPED: the engine took {a_frame_took:?} for a frame here, which is too slow for a chase to catch up with a project that plays"
        );
        return;
    }
    play_to_and_then_jump_while_playing(&mut window, 17, 10);
    assert!(window.application().is_playing(), "the project plays on");

    // The picture keeps changing for a quarter of a second, and then the chase is made, and the engine hands over its states when it has caught up with the playhead.
    let started = std::time::Instant::now();
    let mut the_chase_was_made = false;
    loop {
        window.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(3));
        let preview = window.engine_preview().unwrap();
        the_chase_was_made |= preview.a_lead_up_is_being_made();
        if the_chase_was_made && !preview.a_lead_up_is_being_made() && !preview.the_picture_shown_is_not_exact() {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the picture did not become exact: the chase was made: {the_chase_was_made}, playing: {}",
            window.application().is_playing()
        );
    }
    assert_eq!(window.engine_preview().unwrap().why_it_stopped(), None);
    assert!(!says_the_picture_is_not_exact(&window));
    assert!(
        window.application().is_playing(),
        "the chase caught up with the playhead while the project played, and not when it had stopped at the end"
    );

    // Looked at where it stands: the picture of a frame that is asked for again from the states that the chase handed over is the picture that playing from the first frame makes.
    window.do_this(Command::Pause);
    window.wait_for_the_engine();
    let frame = window.application().current_frame().value();
    if is_not_exact(&window) {
        // (A frame of the preview was passed over after the chase, as a card that is late passes frames over, and goes on from the state it has: not exact, as it always was. Nothing is wrong with the chase.)
        return;
    }
    assert!(
        the_red_of_the_picture(&window).abs_diff(reds[(frame % 16) as usize]) <= 1,
        "frame {frame}: {} and the exact {}",
        the_red_of_the_picture(&window),
        reds[(frame % 16) as usize]
    );
}

#[test]
fn a_chase_that_the_project_is_paused_during_catches_up_with_the_frame_that_stays_and_the_picture_is_exact_when_it_does()
 {
    let Some((mut window, _turn)) = a_window_whose_picture_counts_in_a_cycle_at_one_frame_a_second() else {
        return;
    };
    let reds = the_reds_of_the_picture_by_the_frame_of_the_effect_modulo_16(&mut window);
    play_to_and_then_jump_while_playing(&mut window, 17, 10);
    window.draw_frames_until("the chase is made", |window| {
        window.engine_preview().unwrap().a_lead_up_is_being_made()
    });
    // The project is paused while the chase goes on: the frame stays, and the chase has a frame to catch up with.
    window.do_this(Command::Pause);
    let frame = window.application().current_frame().value();
    window.draw_frames_until("the picture is exact", |window| {
        !window.engine_preview().unwrap().a_lead_up_is_being_made() && !is_not_exact(window)
    });
    assert_eq!(window.engine_preview().unwrap().why_it_stopped(), None);
    assert_eq!(
        window.application().current_frame().value(),
        frame,
        "the project is paused: the frame that stayed is the frame of the picture"
    );
    assert!(
        the_red_of_the_picture(&window).abs_diff(reds[(frame % 16) as usize]) <= 1,
        "frame {frame}: {} and the exact {}",
        the_red_of_the_picture(&window),
        reds[(frame % 16) as usize]
    );
}

#[test]
fn with_the_checkbox_off_a_jump_while_the_project_plays_makes_no_chase() {
    let Some((mut window, _turn)) = a_window_whose_picture_counts_in_a_cycle_at_one_frame_a_second() else {
        return;
    };
    the_reds_of_the_picture_by_the_frame_of_the_effect_modulo_16(&mut window);
    window.do_this(Command::SetGenerateLeadUpOnSeek(false));
    play_to_and_then_jump_while_playing(&mut window, 17, 10);
    draw_the_window_for(&mut window, std::time::Duration::from_millis(700));
    assert!(!window.engine_preview().unwrap().a_lead_up_is_being_made());
    assert!(
        is_not_exact(&window),
        "it goes on as it did, from the nearest state, and says so"
    );
}

/// A window of the picture that counts its frames (at thirty frames a second, which is a project of 150 frames) with a preview made with a scripted engine (it makes no frame, and what the window asks of it is read), **while the project
/// plays after a jump to frame 20**, with the engine saying that the picture is not exact, and the chase that the window makes of that, started and waiting for the engine to take what it describes.
struct WindowWithAChaseThatIsBeingMade {
    window: Window,
    events: std::sync::mpsc::Sender<crate::engine::EngineEvent>,
    requests: std::sync::mpsc::Receiver<crate::engine::EngineRequest>,
    asked: WhatTheWindowAsked,
    generation: u64,
    items: std::sync::mpsc::Receiver<crate::engine::LeadUpItem>,
}

fn a_window_that_is_making_a_chase() -> Option<WindowWithAChaseThatIsBeingMade> {
    use crate::application::preview_panel::EnginePreview;
    use crate::engine::Engine;
    let mut window = a_window_whose_picture_counts_its_frames(true)?;
    let (engine, events, requests) = Engine::scripted_for_tests();
    window.application_mut().interface.engine_preview = Some(EnginePreview::with_engine(engine));
    let mut asked = WhatTheWindowAsked::default();
    window.do_this(Command::Play);
    window.do_this(Command::SeekTo(TimelineFrameIndex::new(20)));
    draw_until_asked(
        &mut window,
        &requests,
        &mut asked,
        "a frame is asked for after the jump",
        |asked, _| !asked.frames.is_empty(),
    );
    let serial = asked.frames.last().unwrap().0;
    events
        .send(a_frame_that_the_scripted_engine_made(serial, true))
        .unwrap();
    draw_until_asked(
        &mut window,
        &requests,
        &mut asked,
        "the chase is started",
        |asked, _| !asked.chases.is_empty(),
    );
    let (generation, items) = asked.chases.remove(0);
    Some(WindowWithAChaseThatIsBeingMade {
        window,
        events,
        requests,
        asked,
        generation,
        items,
    })
}

#[test]
fn the_chase_that_the_window_makes_is_of_the_frames_from_before_the_frame_on_the_screen_to_the_end_of_the_project_and_is_asked_of_the_engine_as_a_chase()
 {
    use crate::engine::LeadUpItem;
    let Some(mut made) = a_window_that_is_making_a_chase() else {
        return;
    };
    assert_eq!(made.generation, 1);
    assert!(made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    assert!(
        made.asked.lead_ups.is_empty(),
        "the engine was not asked for a lead-up for a frame: the project plays"
    );
    let the_frame_on_the_screen = made.asked.frames.last().unwrap().1.clone();

    let started = std::time::Instant::now();
    let mut given: Vec<LeadUpItem> = Vec::new();
    loop {
        match made.items.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(item) => given.push(item),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(30),
                    "the chase did not end"
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(matches!(given.last(), Some(LeadUpItem::End)));
    let frames: Vec<&crate::engine::FrameDescription> = given
        .iter()
        .filter_map(|item| match item {
            LeadUpItem::Frame(description) => Some(description),
            _ => None,
        })
        .collect();
    let state_of = |description: &crate::engine::FrameDescription| {
        let states: Vec<crate::engine::EffectState> = description
            .flatten()
            .layers
            .iter()
            .flat_map(|flat| flat.layer.effects.iter().filter_map(|effect| effect.state))
            .collect();
        assert_eq!(states.len(), 1, "one effect with a state");
        states[0]
    };
    // The effect began with the clip, so the chase begins there, whatever frame the playhead was at when it was wanted (it has gone on from 20), and goes on to the last frame of the project (150 frames).
    assert_eq!(frames.len(), 150, "every frame of the project");
    for (number, description) in frames.iter().enumerate() {
        let state = state_of(description);
        assert_eq!(
            state.frame, number as u64,
            "the frames of the effect's clock, in order, from the first"
        );
        assert_eq!(state.id, state_of(&the_frame_on_the_screen).id, "the same state");
        assert_eq!(description.render_size, the_frame_on_the_screen.render_size);
        assert_eq!(description.colour, the_frame_on_the_screen.colour);
    }
    assert!(
        state_of(&the_frame_on_the_screen).frame >= 20,
        "the playhead went on from the jump to frame 20, and the chase is for the frames before it, wherever it is"
    );
    // (The line over the picture says what it waits for, as it does for a lead-up.)
    made.window.harness.step();
    assert!(
        made.window
            .harness
            .query_by_label(
                crate::application::preview_panel::THE_PICTURE_IS_NOT_EXACT_AND_THE_LEAD_UP_IS_BEING_MADE_NOTE
            )
            .is_some()
    );
}

#[test]
fn another_jump_while_the_chase_is_made_cancels_it_and_the_engine_is_told() {
    let Some(mut made) = a_window_that_is_making_a_chase() else {
        return;
    };
    let frames_asked_before = made.asked.frames.len();
    made.window.do_this(Command::SeekTo(TimelineFrameIndex::new(60)));
    draw_until_asked(
        &mut made.window,
        &made.requests,
        &mut made.asked,
        "the chase is cancelled",
        |asked, _| !asked.cancelled_lead_ups.is_empty() && asked.frames.len() > frames_asked_before,
    );
    assert_eq!(made.asked.cancelled_lead_ups, [made.generation]);
    assert!(!made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    // (The window goes on: the frame that is asked for after the jump is the frame at 60 or after.)
    assert!(made.window.application().current_frame().value() >= 60);
    drop(made.items);
}

#[test]
fn a_frame_that_follows_the_one_before_does_not_cancel_the_chase_that_the_window_makes() {
    let Some(mut made) = a_window_that_is_making_a_chase() else {
        return;
    };
    let frames_asked_before = made.asked.frames.len();
    draw_until_asked(
        &mut made.window,
        &made.requests,
        &mut made.asked,
        "more frames are asked for as the project plays",
        |asked, _| asked.frames.len() >= frames_asked_before + 3,
    );
    assert!(
        made.asked.cancelled_lead_ups.is_empty(),
        "the project plays on, and the chase with it"
    );
    assert!(made.window.engine_preview().unwrap().a_lead_up_is_being_made());
    made.events
        .send(crate::engine::EngineEvent::LeadUpDone {
            generation: made.generation,
            frames_made: 100,
        })
        .unwrap();
    let frames_asked_before = made.asked.frames.len();
    draw_until_asked(
        &mut made.window,
        &made.requests,
        &mut made.asked,
        "the frame is asked for again, and the chase is over",
        |asked, window| {
            asked.frames.len() > frames_asked_before && !window.engine_preview().unwrap().a_lead_up_is_being_made()
        },
    );
}
