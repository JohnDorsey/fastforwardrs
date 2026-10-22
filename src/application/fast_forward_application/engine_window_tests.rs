// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window with its preview made by the picture engine (on the software graphics adapter), against the window as it always was: the picture that is
//! shown is the engine's, everything the window draws to show where things are and what the mouse can take hold of is as it was, a window that is not changing
//! is not asking for frames, and a window whose engine stops makes its preview the old way.

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
