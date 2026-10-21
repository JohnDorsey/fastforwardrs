// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how the pictures on the preview are put on the screen: blended, until the picture is zoomed so far that the pixels of it
//! can be seen, and then as blocks; and with the colours that the effects of each layer make. The pictures are small ones, drawn on
//! a picture that has a lot more pixels, so that this can be seen with a few: what is looked at is what the graphics card has been
//! given.

use std::borrow::Cow;
use std::sync::Arc;

use egui::{Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::view::PreviewView;
use super::zooming::FIT_BUTTON_LABEL;
use super::{PreviewContent, PreviewLayer, PreviewLayerContent, VideoInThePicture, show};
use crate::application::commands::Command;
use crate::application::textures::{PictureFilter, Textures};
use crate::colour_effects::ColourMatrix;
use crate::engine::PictureColour;
use crate::media::{CompressedPicture, DecodedFrame, PictureSize, ThumbnailKey, ThumbnailLibrary};
use crate::playback::SuppliedPicture;
use crate::project::{
    CanvasArea, ClipId, Placement, SourceFrameIndex, SourceMediaIdentifier, SpeedIndicatorLayout, TextObject, TrackId,
};
use crate::rgba_picture::RgbaPicture;
use crate::text_rendering::LazyTextRenderer;

const THE_VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);
const THE_PICTURE_FILE: SourceMediaIdentifier = SourceMediaIdentifier::new(2);
const THE_VIDEO_TRACK: TrackId = TrackId::new(0);
const THE_PICTURE_SIZE: (u32, u32) = (1280, 720);

/// How the colours of each of the layers of the preview in the tests are changed: the video, the picture from a file, and the text.
#[derive(Clone, Copy)]
struct Colours {
    video: ColourMatrix,
    picture_file: ColourMatrix,
    text: ColourMatrix,
}

impl Colours {
    const UNCHANGED: Self = Self {
        video: ColourMatrix::NOTHING_CHANGES,
        picture_file: ColourMatrix::NOTHING_CHANGES,
        text: ColourMatrix::NOTHING_CHANGES,
    };
}

/// What the preview in the tests is made of and kept in: a picture of 1280 by 720 pixels with a video and a picture file on it, each
/// of which is a picture of a few pixels, the picture of the video as one that was supplied; and, when it is asked for, a text object
/// and a thumbnail that stands in for the video.
struct Preview {
    view: PreviewView,
    textures: Textures,
    text_renderer: LazyTextRenderer,
    commands: Vec<Command>,
    supplied: SuppliedPicture,
    from_a_file: RgbaPicture,
    colours: Colours,
    /// The text object that is shown too, if there is one. (Its picture needs the fonts, which are slow to load: only the tests
    /// that look at text ask for it.)
    text: Option<TextObject>,
    /// The thumbnails that stand in for the picture of the video in place of it, if there are some.
    thumbnails: Option<ThumbnailLibrary>,
}

impl Preview {
    fn new(view: PreviewView) -> Self {
        Self {
            view,
            textures: Textures::default(),
            text_renderer: LazyTextRenderer::new(Arc::new(|| {})),
            commands: Vec::new(),
            supplied: SuppliedPicture {
                chroma_siting: crate::colour_space::ChromaSiting::Left,
                colour: crate::engine::PictureColour::SRGB,
                source: THE_VIDEO,
                source_frame: SourceFrameIndex::new(0),
                frame: DecodedFrame {
                    width_in_pixels: 2,
                    height_in_pixels: 2,
                    rgba_pixels: vec![255; 16],
                }
                .into(),
                serial_number: 1,
            },
            from_a_file: RgbaPicture {
                width_in_pixels: 2,
                height_in_pixels: 2,
                rgba_pixels: vec![200; 16],
            },
            colours: Colours::UNCHANGED,
            text: None,
            thumbnails: None,
        }
    }
}

fn show_the_preview(ui: &mut egui::Ui, preview: &mut Preview) {
    let Preview {
        view,
        textures,
        text_renderer,
        commands,
        supplied,
        from_a_file,
        colours,
        text,
        thumbnails,
    } = preview;
    let layer = |number: u32, colour, content| PreviewLayer {
        clip: ClipId::new(TrackId::new(number), 0),
        track: TrackId::new(number),
        placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
        operations: PreviewLayer::operations_of_a_colour(colour),
        content,
    };
    let video = match thumbnails {
        Some(library) => VideoInThePicture::Thumbnail {
            source: THE_VIDEO,
            second: 0,
            library,
        },
        None => VideoInThePicture::Picture(supplied),
    };
    let mut layers = vec![
        layer(0, colours.video, PreviewLayerContent::Video(video)),
        layer(
            1,
            colours.picture_file,
            PreviewLayerContent::Image {
                source: THE_PICTURE_FILE,
                picture: Some(from_a_file),
                colour: PictureColour::SRGB,
            },
        ),
    ];
    if let Some(object) = text {
        layers.push(layer(2, colours.text, PreviewLayerContent::Text(Cow::Borrowed(object))));
    }
    let content = PreviewContent {
        picture_size: Some(THE_PICTURE_SIZE),
        layers,
        symbol: None,
        groups: Vec::new(),
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
        selected_clip: None,
    };
    show(ui, &content, view, textures, text_renderer, commands);
}

fn the_preview_of(preview: Preview) -> Harness<'static, Preview> {
    let mut harness = Harness::builder()
        .with_size(vec2(1000.0, 600.0))
        .build_ui_state(|ui, preview: &mut Preview| show_the_preview(ui, preview), preview);
    harness.run();
    harness
}

fn the_preview_with(view: PreviewView) -> Harness<'static, Preview> {
    the_preview_of(Preview::new(view))
}

/// The room the preview has, which is all of the window of the test.
fn the_panel() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0))
}

fn zoomed_by(factor: f32) -> PreviewView {
    PreviewView::FITTING.zoomed_by(factor, the_panel().center(), the_panel(), THE_PICTURE_SIZE)
}

/// How the picture of the video and the picture of the file are put on the screen.
fn how_the_pictures_are_shown(harness: &Harness<'static, Preview>) -> (Option<PictureFilter>, Option<PictureFilter>) {
    let textures = &harness.state().textures;
    (
        textures.filter_of_the_preview_picture(THE_VIDEO_TRACK),
        textures.filter_of_the_picture_of_the_file(THE_PICTURE_FILE),
    )
}

fn turned_by(degrees: f32) -> ColourMatrix {
    ColourMatrix::hue_rotation(degrees)
}

// --- how the pixels are put on the screen ---

#[test]
fn the_pictures_are_blended_when_the_picture_fits_and_when_it_is_zoomed_a_little() {
    for view in [PreviewView::FITTING, zoomed_by(2.0)] {
        let harness = the_preview_with(view);
        assert_eq!(
            how_the_pictures_are_shown(&harness),
            (Some(PictureFilter::Smooth), Some(PictureFilter::Smooth)),
            "{view:?}"
        );
    }
}

#[test]
fn the_pictures_are_put_on_the_screen_as_blocks_when_the_picture_is_zoomed_in_so_far_that_its_pixels_can_be_seen() {
    let far_in = zoomed_by(100.0);
    assert_eq!(far_in.filter(the_panel(), THE_PICTURE_SIZE, 1.0), PictureFilter::Pixels);

    let harness = the_preview_with(far_in);

    assert_eq!(
        how_the_pictures_are_shown(&harness),
        (Some(PictureFilter::Pixels), Some(PictureFilter::Pixels))
    );
}

#[test]
fn the_pictures_are_blended_again_when_the_picture_is_zoomed_out_again() {
    let mut harness = the_preview_with(zoomed_by(100.0));
    assert_eq!(how_the_pictures_are_shown(&harness).0, Some(PictureFilter::Pixels));

    harness.state_mut().view = PreviewView::FITTING;
    harness.run();

    assert_eq!(
        how_the_pictures_are_shown(&harness),
        (Some(PictureFilter::Smooth), Some(PictureFilter::Smooth))
    );
}

#[test]
fn the_picture_of_a_text_object_is_put_on_the_screen_as_blocks_when_the_picture_is_zoomed_in_so_far_that_pixels_can_be_seen()
 {
    let mut preview = Preview::new(PreviewView::FITTING);
    preview.text = Some(TextObject::default());
    preview.text_renderer.get_waiting_if_needed().expect("the fonts load");
    let mut harness = the_preview_of(preview);
    let pattern = ColourMatrix::NOTHING_CHANGES.bit_pattern();
    assert_eq!(
        harness.state().textures.ways_the_pictures_of_text_were_made(),
        vec![(PictureFilter::Smooth, pattern)],
        "blended when it fits"
    );

    harness.state_mut().view = zoomed_by(100.0);
    harness.run();

    let mut ways = harness.state().textures.ways_the_pictures_of_text_were_made();
    ways.sort_by_key(|(filter, _)| matches!(filter, PictureFilter::Pixels));
    assert_eq!(
        ways,
        vec![(PictureFilter::Smooth, pattern), (PictureFilter::Pixels, pattern)],
        "and as blocks when the pixels can be seen: each is kept"
    );
}

#[test]
fn the_button_that_makes_the_picture_fit_is_only_there_when_it_does_not_and_puts_the_view_back() {
    let mut harness = the_preview_with(PreviewView::FITTING);
    assert!(harness.query_by_label(FIT_BUTTON_LABEL).is_none());

    harness.state_mut().view = zoomed_by(3.0);
    harness.run();
    assert!(!harness.state().view.is_fitting());
    harness.get_by_label(FIT_BUTTON_LABEL).click();
    harness.run();

    assert!(harness.state().view.is_fitting());
    assert!(harness.query_by_label(FIT_BUTTON_LABEL).is_none());
}

// --- the colours that effects make ---

#[test]
fn the_picture_of_each_layer_is_made_with_the_colours_that_its_effects_make_and_the_picture_of_a_layer_with_none_is_as_it_is()
 {
    let mut preview = Preview::new(PreviewView::FITTING);
    preview.colours = Colours {
        video: turned_by(120.0),
        picture_file: turned_by(-90.0),
        ..Colours::UNCHANGED
    };

    let harness = the_preview_of(preview);

    let textures = &harness.state().textures;
    assert_eq!(
        textures.colours_of_the_preview_picture(THE_VIDEO_TRACK),
        Some(turned_by(120.0))
    );
    assert_eq!(
        textures.colours_of_the_picture_of_the_file(THE_PICTURE_FILE),
        Some(turned_by(-90.0))
    );

    let harness = the_preview_with(PreviewView::FITTING);
    let textures = &harness.state().textures;
    assert_eq!(
        textures.colours_of_the_preview_picture(THE_VIDEO_TRACK),
        Some(ColourMatrix::NOTHING_CHANGES)
    );
    assert_eq!(
        textures.colours_of_the_picture_of_the_file(THE_PICTURE_FILE),
        Some(ColourMatrix::NOTHING_CHANGES)
    );
}

#[test]
fn the_colours_of_a_picture_are_changed_again_when_the_effects_change_them_another_way_at_another_frame() {
    let mut harness = the_preview_with(PreviewView::FITTING);

    for degrees in [30.0, 60.0, 0.0] {
        harness.state_mut().colours = Colours {
            video: turned_by(degrees),
            picture_file: turned_by(degrees + 10.0),
            ..Colours::UNCHANGED
        };
        harness.run();

        let textures = &harness.state().textures;
        assert_eq!(
            textures.colours_of_the_preview_picture(THE_VIDEO_TRACK),
            Some(turned_by(degrees)),
            "{degrees} degrees"
        );
        assert_eq!(
            textures.colours_of_the_picture_of_the_file(THE_PICTURE_FILE),
            Some(turned_by(degrees + 10.0)),
            "{degrees} degrees"
        );
    }
}

#[test]
fn the_picture_of_a_text_object_is_made_with_the_colours_that_its_effects_make() {
    let mut preview = Preview::new(PreviewView::FITTING);
    preview.text = Some(TextObject::default());
    preview.colours.text = turned_by(200.0);
    preview.text_renderer.get_waiting_if_needed().expect("the fonts load");

    let harness = the_preview_of(preview);

    assert_eq!(
        harness.state().textures.ways_the_pictures_of_text_were_made(),
        vec![(PictureFilter::Smooth, turned_by(200.0).bit_pattern())]
    );
}

#[test]
fn the_thumbnail_that_stands_in_for_the_picture_of_a_video_has_the_colours_that_the_effects_of_its_layer_make() {
    let mut thumbnails = ThumbnailLibrary::with_room_for(10);
    thumbnails.insert(
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
    let mut preview = Preview::new(PreviewView::FITTING);
    preview.thumbnails = Some(thumbnails);
    preview.colours.video = turned_by(120.0);

    let mut harness = the_preview_of(preview);

    assert_eq!(
        harness
            .state()
            .textures
            .ways_the_colours_of_the_thumbnails_were_changed(),
        vec![turned_by(120.0).bit_pattern()]
    );
    assert_eq!(
        harness.state().textures.colours_of_the_preview_picture(THE_VIDEO_TRACK),
        None,
        "the picture of the video itself is not what is shown"
    );

    // (Changing the colours the other way again makes the thumbnail as it is.)
    harness.state_mut().colours.video = ColourMatrix::NOTHING_CHANGES;
    harness.run();
    let ways = harness
        .state()
        .textures
        .ways_the_colours_of_the_thumbnails_were_changed();
    assert!(ways.contains(&ColourMatrix::NOTHING_CHANGES.bit_pattern()), "{ways:?}");
}
