// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that a window that is not changing puts nothing on the graphics card. A picture that is there stays there: it is not sent
//! again and again, however it is put on the screen.
//!
//! The panels of the window share the pictures that are on the graphics card: the preview puts the picture of a file on the screen
//! the way the picture is zoomed and the way the effects change its colours, and the timeline draws the same picture, small. If
//! either put it on the screen as it liked, each would undo what the other did in every frame, and the whole picture would be sent
//! to the graphics card again in every frame, with nothing to be seen of it on the screen (the preview draws last, so what is seen
//! is as it should be: only the tests can tell).

use egui::{Modifiers, vec2};

use super::picture_tests::picture_source;
use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::textures::PictureFilter;
use crate::colour_effects::ColourMatrix;
use crate::project::{AnimationTarget, ClipId, PropertyId, PropertyValue, SourceMediaIdentifier, TimelineFrameIndex};
use crate::rgba_picture::RgbaPicture;
use crate::test_support::put_a_hue_shift_on;

/// How much the wheel is turned to zoom in by far more than there is room for, in points: what `egui` makes of it (its speed is 1/200
/// for each point, and the picture is as big as it can get long before it has doubled eight times).
const WHEEL_FOR_ZOOMING_IN_AS_FAR_AS_IT_GOES: f32 = 200.0 * std::f32::consts::LN_2 * 8.0;

const HUE_SHIFT_OF_THE_PICTURE_IN_DEGREES: f32 = 120.0;

/// A window with a video and, in front of it, a picture from a file (which is decoded) that has a hue shift, from the first frame.
fn window_with_a_picture_that_has_a_hue_shift() -> (Window, SourceMediaIdentifier, ClipId) {
    let mut project = project_of(300);
    let source = picture_source();
    let identifier = source.identifier;
    let picture = project
        .import_source(source, TimelineFrameIndex::new(0))
        .unwrap()
        .clip
        .expect("a picture makes a clip");
    put_a_hue_shift_on(
        &mut project,
        AnimationTarget::Clip(picture),
        HUE_SHIFT_OF_THE_PICTURE_IN_DEGREES,
    );
    let mut window = Window::showing(project);
    window
        .application_mut()
        .source_files
        .get_mut(&identifier)
        .expect("the window has the files of the project")
        .picture
        .picture = Some(RgbaPicture {
        width_in_pixels: 4,
        height_in_pixels: 2,
        rgba_pixels: [255, 0, 0, 255].repeat(8),
    });
    window.settle();
    (window, identifier, picture)
}

fn zoom_the_preview_in_as_far_as_it_goes(window: &mut Window) {
    let middle = window.the_preview_panel().center();
    window.scroll_the_wheel(
        middle,
        vec2(0.0, WHEEL_FOR_ZOOMING_IN_AS_FAR_AS_IT_GOES),
        Modifiers::NONE,
    );
    window.settle();
}

#[test]
fn a_window_that_is_not_changing_does_not_send_the_picture_of_a_file_to_the_graphics_card_again() {
    let (mut window, picture_file, _) = window_with_a_picture_that_has_a_hue_shift();
    zoom_the_preview_in_as_far_as_it_goes(&mut window);
    // (The picture is on the screen in the way that the preview puts it there, which is not the way that it is first sent to the
    // graphics card: it is made of blocks, and its colours are changed. That is what there is to keep as it is.)
    let textures = &window.application().textures;
    assert_eq!(
        textures.filter_of_the_picture_of_the_file(picture_file),
        Some(PictureFilter::Pixels)
    );
    assert_eq!(
        textures.colours_of_the_picture_of_the_file(picture_file),
        Some(ColourMatrix::hue_rotation(HUE_SHIFT_OF_THE_PICTURE_IN_DEGREES))
    );

    let sent = window.textures_uploaded_while(|window| window.harness.run_steps(5));

    assert_eq!(
        sent, 0,
        "pictures were sent to the graphics card in 5 frames of a window that is not changing"
    );
}

#[test]
fn what_is_sent_to_the_graphics_card_is_counted_when_the_picture_is_changed() {
    let (mut window, picture_file, picture) = window_with_a_picture_that_has_a_hue_shift();

    let sent = window.textures_uploaded_while(|window| {
        window.do_this(Command::SetEffectValue {
            target: AnimationTarget::Clip(picture),
            property: PropertyId::HueShift,
            value: PropertyValue::Number(60.0),
        });
    });

    assert!(
        sent >= 1,
        "the picture was made again with its colours turned another way, and sent"
    );
    assert_eq!(
        window
            .application()
            .textures
            .colours_of_the_picture_of_the_file(picture_file),
        Some(ColourMatrix::hue_rotation(60.0))
    );
}
