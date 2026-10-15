// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Describing the preview for the engine: which layers, in which order, where, with which colours and which pictures, and what is left out. Nothing here
//! needs a graphics card: the engine is not given anything, a supply that only writes down what it was asked for stands in for it.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use egui::{Rect, pos2, vec2};

use super::engine_view::{HowToMakeIt, PictureSupply, PictureWanted, SentPictures, describe_frame};
use super::geometry::PictureOnScreen;
use super::test_content::{clip, in_the_middle, layer, whole_picture};
use super::{PreviewContent, PreviewLayer, PreviewLayerContent, SymbolOnThePreview, VideoInThePicture};
use crate::application::theme;
use crate::colour_effects::ColourMatrix;
use crate::engine::{
    COLOUR_MATRIX_KEY, CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, LayerSource, PictureData, PictureId,
    PicturePixels,
};
use crate::media::{
    CompressedPicture, DecodedFrame, DecodedYuvFrame, NativeMatrix, NativeYuv420p, PictureSize, ThumbnailKey,
    ThumbnailLibrary,
};
use crate::playback::{SuppliedFrame, SuppliedPicture};
use crate::project::{
    Angle, CanvasArea, Placement, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout,
    TextObject,
};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::indicator_aspect_ratio;
use crate::text_rendering::LazyTextRenderer;

const RENDER_SIZE: (u32, u32) = (800, 450);

/// A preview of these layers, the one at the back first, with no symbol, and `selected` selected, on a picture of 1600 by 900 pixels (like
/// `test_content::content`, but the layers may show pictures that are borrowed).
fn content<'a>(layers: Vec<PreviewLayer<'a>>, selected: Option<u32>) -> PreviewContent<'a> {
    PreviewContent {
        picture_size: Some((1600, 900)),
        layers,
        symbol: None,
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
        selected_clip: selected.map(clip),
    }
}

/// A supply that writes down what it is asked for and says yes to everything but what it is told to refuse.
#[derive(Default)]
struct WritesItDown {
    asked: Vec<PictureId>,
    refuses: HashSet<PictureId>,
    kinds: Vec<&'static str>,
}

impl PictureSupply for WritesItDown {
    fn make_sure_the_engine_has(&mut self, wanted: &PictureWanted<'_>) -> bool {
        self.asked.push(wanted.id);
        self.kinds.push(match wanted.how {
            HowToMakeIt::VideoPicture(_) => "video picture",
            HowToMakeIt::Thumbnail(_) => "thumbnail",
            HowToMakeIt::Image(_) => "image",
            HowToMakeIt::Text { .. } => "text",
            HowToMakeIt::Symbol { .. } => "symbol",
        });
        !self.refuses.contains(&wanted.id)
    }
}

fn describe(content: &PreviewContent<'_>) -> (FrameDescription, WritesItDown) {
    describe_with(content, 0, WritesItDown::default())
}

fn describe_with(
    content: &PreviewContent<'_>,
    generation: u64,
    mut supply: WritesItDown,
) -> (FrameDescription, WritesItDown) {
    let description = super::engine_view::describe_frame(content, RENDER_SIZE, generation, &mut supply);
    (description, supply)
}

fn video_picture(serial_number: u64) -> SuppliedPicture {
    SuppliedPicture {
        source: SourceMediaIdentifier::new(1),
        source_frame: SourceFrameIndex::new(0),
        frame: SuppliedFrame::Rgba(DecodedFrame {
            width_in_pixels: 2,
            height_in_pixels: 2,
            rgba_pixels: vec![10; 16],
        }),
        serial_number,
    }
}

fn showing<'a>(number: u32, picture: &'a SuppliedPicture, placement: Placement) -> PreviewLayer<'a> {
    PreviewLayer {
        content: PreviewLayerContent::Video(VideoInThePicture::Picture(picture)),
        ..layer(number, placement)
    }
}

fn image_layer<'a>(
    number: u32,
    source: u32,
    picture: Option<&'a RgbaPicture>,
    placement: Placement,
) -> PreviewLayer<'a> {
    PreviewLayer {
        content: PreviewLayerContent::Image {
            source: SourceMediaIdentifier::new(source),
            picture,
        },
        ..layer(number, placement)
    }
}

fn an_image() -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: 3,
        height_in_pixels: 2,
        rgba_pixels: (0..24).collect(),
    }
}

fn the_corners_on(frame: (u32, u32), corners: [(f32, f32); 4]) -> [(f32, f32); 4] {
    corners.map(|(across, down)| (across * frame.0 as f32, down * frame.1 as f32))
}

#[test]
fn a_preview_with_no_picture_to_show_is_a_black_frame_and_nothing_is_asked_for() {
    let mut nothing = content(vec![layer(0, whole_picture())], None);
    nothing.picture_size = None;
    let (description, supply) = describe(&nothing);
    assert_eq!(description, FrameDescription::black(RENDER_SIZE));
    assert!(supply.asked.is_empty());

    let (description, _) = describe(&content(Vec::new(), None));
    assert_eq!(
        description,
        FrameDescription::black(RENDER_SIZE),
        "a picture with nothing on it is black"
    );
}

#[test]
fn a_video_with_its_picture_is_a_layer_of_that_picture_over_all_of_the_frame_and_the_supply_is_asked_for_it_once() {
    let picture = video_picture(17);
    let (description, supply) = describe(&content(vec![showing(0, &picture, whole_picture())], None));
    assert_eq!(description.render_size, RENDER_SIZE);
    assert_eq!(description.layers.len(), 1);
    assert_eq!(
        description.layers[0].source,
        LayerSource::Picture(PictureId(17)),
        "its serial number"
    );
    assert_eq!(description.layers[0].corners, CORNERS_OF_THE_WHOLE_PICTURE);
    assert!(description.layers[0].effects.is_empty());
    assert_eq!(supply.asked, vec![PictureId(17)]);
    assert_eq!(supply.kinds, vec!["video picture"]);
}

#[test]
fn the_layers_are_in_the_order_the_content_has_them_the_one_at_the_back_first() {
    let (first, second, third) = (video_picture(1), video_picture(2), video_picture(3));
    let (description, _) = describe(&content(
        vec![
            showing(0, &first, whole_picture()),
            showing(1, &second, in_the_middle()),
            showing(2, &third, whole_picture()),
        ],
        None,
    ));
    let ids: Vec<LayerSource> = description.layers.iter().map(|layer| layer.source).collect();
    assert_eq!(
        ids,
        vec![
            LayerSource::Picture(PictureId(1)),
            LayerSource::Picture(PictureId(2)),
            LayerSource::Picture(PictureId(3))
        ]
    );
}

/// The corners of a layer are the corners that the old drawing puts it at: the same arithmetic (`Placement::corner_position` in the pixels of the finished
/// picture, where turning is the same along and across), so a layer turned or partly past the edge is where it was.
#[test]
fn a_layer_is_where_the_old_drawing_puts_it_turned_or_cut_off_by_the_edge_or_not() {
    let picture = video_picture(1);
    let picture_rectangle = Rect::from_min_size(pos2(0.0, 0.0), vec2(RENDER_SIZE.0 as f32, RENDER_SIZE.1 as f32));
    let on_the_screen = PictureOnScreen::new(picture_rectangle, 1600, 900);
    for placement in [
        whole_picture(),
        in_the_middle(),
        Placement::new(CanvasArea::new(0.25, 0.25, 0.5, 0.5), Angle::from_degrees(30.0)),
        Placement::new(CanvasArea::new(0.1, 0.6, 0.7, 0.3), Angle::from_degrees(-117.0)),
        Placement::new(CanvasArea::new(-0.2, -0.1, 0.6, 0.5), Angle::from_degrees(720.0 + 45.0)),
        Placement::new(CanvasArea::new(0.7, 0.8, 0.6, 0.5), Angle::from_degrees(12.0)),
    ] {
        let (description, _) = describe(&content(vec![showing(0, &picture, placement)], None));
        let ours = the_corners_on(RENDER_SIZE, description.layers[0].corners);
        let theirs = on_the_screen.outline_of(&placement);
        for (ours, theirs) in ours.iter().zip(theirs) {
            assert!(
                (ours.0 - theirs.x).abs() < 1e-2 && (ours.1 - theirs.y).abs() < 1e-2,
                "{placement:?}: {ours:?} is not {theirs:?}"
            );
        }
    }
}

#[test]
fn the_colours_of_a_layer_are_one_matrix_when_it_changes_something_and_nothing_when_it_does_not() {
    let picture = video_picture(1);
    let hue = ColourMatrix::hue_rotation(120.0);
    let folded = ColourMatrix::hue_rotation(70.0).then(ColourMatrix::hue_rotation(110.0));
    for (colour, expected) in [
        (hue, Some(hue)),
        (folded, Some(folded)),
        (ColourMatrix::NOTHING_CHANGES, None),
        (ColourMatrix::hue_rotation(360.0), None),
        (ColourMatrix::hue_rotation(0.001), None),
    ] {
        let mut shown = showing(0, &picture, whole_picture());
        shown.colour = colour;
        let (description, _) = describe(&content(vec![shown], None));
        match expected {
            Some(matrix) => assert_eq!(
                description.layers[0].effects,
                vec![EffectCall::colour_matrix(matrix.rows())],
                "{colour:?}"
            ),
            None => assert!(description.layers[0].effects.is_empty(), "{colour:?}"),
        }
    }
    let mut shown = showing(0, &picture, whole_picture());
    shown.colour = hue;
    let (description, _) = describe(&content(vec![shown], None));
    assert_eq!(description.layers[0].effects[0].key, COLOUR_MATRIX_KEY);
    assert_eq!(
        description.layers[0].effects[0].values.len(),
        9,
        "the nine numbers, row by row"
    );
}

#[test]
fn a_matrix_that_the_engine_would_not_run_is_not_described_and_one_that_it_would_is() {
    // The describing and the engine agree on which matrices change nothing: a described matrix is one the engine runs (and the other way round).
    let picture = video_picture(1);
    for degrees in [0.0, 0.001, 0.005, 0.01, 0.05, 0.1, 1.0, 45.0, 359.0, 360.0, 360.01] {
        let mut shown = showing(0, &picture, whole_picture());
        shown.colour = ColourMatrix::hue_rotation(degrees);
        let (description, _) = describe(&content(vec![shown], None));
        let plan = crate::engine::plan_frame(&description, &crate::engine::EffectsThatRun::built_in_only());
        assert_eq!(
            description.layers[0].effects.is_empty(),
            plan.effects_not_run == 0 && plan.passes_recorded() == 2,
            "{degrees} degrees: described {:?}, planned {:?}",
            description.layers[0].effects,
            plan.passes
        );
    }
}

#[test]
fn a_video_that_has_no_picture_yet_and_a_picture_file_that_is_not_decoded_are_each_a_colour_and_have_no_effects() {
    let mut waiting = layer(0, in_the_middle());
    waiting.colour = ColourMatrix::hue_rotation(120.0);
    let mut not_decoded = image_layer(1, 7, None, whole_picture());
    not_decoded.colour = ColourMatrix::hue_rotation(120.0);
    let (description, supply) = describe(&content(vec![waiting, not_decoded], None));
    let colour = theme::TRACK_BACKGROUND.to_srgba_unmultiplied();
    assert_eq!(description.layers.len(), 2);
    for described in &description.layers {
        assert_eq!(described.source, LayerSource::Solid(colour));
        assert!(described.effects.is_empty());
    }
    assert_eq!(
        description.layers[0].corners,
        [(0.25, 0.25), (0.75, 0.25), (0.75, 0.75), (0.25, 0.75)]
    );
    assert_eq!(description.layers[1].corners, CORNERS_OF_THE_WHOLE_PICTURE);
    assert!(supply.asked.is_empty(), "there is nothing to make a picture of");
}

#[test]
fn a_picture_file_is_a_picture_with_the_id_of_the_file_and_its_size() {
    let decoded = an_image();
    let (description, supply) = describe(&content(vec![image_layer(0, 7, Some(&decoded), whole_picture())], None));
    assert_eq!(supply.kinds, vec!["image"]);
    let LayerSource::Picture(id) = description.layers[0].source else {
        panic!("a picture was expected");
    };
    // Another file, another size and another generation are each another picture; the same ones, the same.
    let id_of = |source: u32, picture: &RgbaPicture, generation: u64| {
        let (description, _) = describe_with(
            &content(vec![image_layer(0, source, Some(picture), whole_picture())], None),
            generation,
            WritesItDown::default(),
        );
        description.layers[0].source
    };
    assert_eq!(id_of(7, &decoded, 0), LayerSource::Picture(id));
    assert_ne!(id_of(8, &decoded, 0), LayerSource::Picture(id), "another file");
    let bigger = RgbaPicture {
        width_in_pixels: 4,
        height_in_pixels: 2,
        rgba_pixels: vec![0; 32],
    };
    assert_ne!(id_of(7, &bigger, 0), LayerSource::Picture(id), "another size");
    assert_ne!(id_of(7, &decoded, 1), LayerSource::Picture(id), "another generation");
}

fn a_thumbnail_library() -> ThumbnailLibrary {
    let mut library = ThumbnailLibrary::with_room_for(10);
    library.insert(
        ThumbnailKey { level: 0, index: 3 },
        CompressedPicture::from_rgb_pixels(
            &[[200u8, 100, 50]; 8 * 8].concat(),
            PictureSize {
                width_in_pixels: 8,
                height_in_pixels: 8,
            },
        )
        .unwrap(),
    );
    library
}

fn thumbnail_layer(second: usize, library: &ThumbnailLibrary) -> PreviewLayer<'_> {
    PreviewLayer {
        content: PreviewLayerContent::Video(VideoInThePicture::Thumbnail {
            source: SourceMediaIdentifier::new(1),
            second,
            library,
        }),
        ..layer(0, whole_picture())
    }
}

#[test]
fn a_thumbnail_that_stands_in_for_a_video_is_a_picture_of_its_own_with_the_colours_of_the_layer_and_nothing_if_there_is_none()
 {
    let library = a_thumbnail_library();
    let mut standing_in = thumbnail_layer(3, &library);
    standing_in.colour = ColourMatrix::hue_rotation(200.0);
    let (description, supply) = describe(&content(vec![standing_in], None));
    assert_eq!(supply.kinds, vec!["thumbnail"]);
    assert_eq!(description.layers.len(), 1);
    assert!(matches!(description.layers[0].source, LayerSource::Picture(_)));
    assert_eq!(
        description.layers[0].effects,
        vec![EffectCall::colour_matrix(ColourMatrix::hue_rotation(200.0).rows())]
    );

    // The second that has no thumbnail: the old drawing draws nothing for it.
    let (description, supply) = describe(&content(vec![thumbnail_layer(4, &library)], None));
    assert!(description.layers.is_empty());
    assert!(supply.asked.is_empty());

    // The id changes with the generation (a file that is replaced has other thumbnails), and not with anything else.
    let id_of = |generation| {
        describe_with(
            &content(vec![thumbnail_layer(3, &library)], None),
            generation,
            WritesItDown::default(),
        )
        .0
        .layers[0]
            .source
    };
    assert_eq!(id_of(0), id_of(0));
    assert_ne!(id_of(0), id_of(1));
}

#[test]
fn a_text_object_is_a_picture_whose_id_is_what_it_looks_like_and_it_is_left_out_while_it_cannot_be_made() {
    let text = |words: &str| TextObject {
        text: words.to_owned(),
        ..TextObject::default()
    };
    let text_layer = |number: u32, object: &TextObject| PreviewLayer {
        content: PreviewLayerContent::Text(Cow::Owned(object.clone())),
        ..layer(number, whole_picture())
    };
    let (hello, goodbye) = (text("hello"), text("goodbye"));
    let (description, supply) = describe(&content(
        vec![text_layer(0, &hello), text_layer(1, &goodbye), text_layer(2, &hello)],
        None,
    ));
    assert_eq!(supply.kinds, vec!["text", "text", "text"]);
    let ids: Vec<LayerSource> = description.layers.iter().map(|layer| layer.source).collect();
    assert_eq!(ids[0], ids[2], "the same words look the same");
    assert_ne!(ids[0], ids[1], "other words do not");

    // While the picture cannot be made the layer is not in the frame, and the others are, in their order.
    let LayerSource::Picture(refused) = ids[1] else {
        panic!("a picture was expected");
    };
    let supply = WritesItDown {
        refuses: HashSet::from([refused]),
        ..WritesItDown::default()
    };
    let (description, _) = describe_with(
        &content(
            vec![text_layer(0, &hello), text_layer(1, &goodbye), text_layer(2, &hello)],
            None,
        ),
        0,
        supply,
    );
    let kept: Vec<LayerSource> = description.layers.iter().map(|layer| layer.source).collect();
    assert_eq!(kept, vec![ids[0], ids[2]]);
}

fn the_symbol_over(clip_number: u32, speed: u32) -> SymbolOnThePreview {
    SymbolOnThePreview {
        over: clip(clip_number),
        speed: SpeedFactor::new(speed).unwrap(),
    }
}

#[test]
fn the_symbol_is_a_layer_of_its_own_right_after_the_layer_it_is_over_so_that_what_is_in_front_covers_it() {
    let (back, front) = (video_picture(1), video_picture(2));
    let mut shown = content(
        vec![showing(0, &back, whole_picture()), showing(1, &front, whole_picture())],
        None,
    );
    shown.symbol = Some(the_symbol_over(0, 4));
    shown.indicator_layout = SpeedIndicatorLayout::new(0.1, 0.2, 0.15);
    let (description, supply) = describe(&shown);
    assert_eq!(supply.kinds, vec!["video picture", "symbol", "video picture"]);
    assert_eq!(description.layers.len(), 3);
    assert_eq!(description.layers[0].source, LayerSource::Picture(PictureId(1)));
    assert_eq!(description.layers[2].source, LayerSource::Picture(PictureId(2)));
    let symbol = &description.layers[1];
    assert!(matches!(symbol.source, LayerSource::Picture(_)));
    assert!(symbol.effects.is_empty());

    // It is where the layout puts it: its top left corner at a tenth across and a fifth down, a sixth of the height tall (a hundredth either way), as wide as its
    // shape makes it (in pixels), and it is not turned.
    let [top_left, top_right, bottom_right, bottom_left] = symbol.corners;
    assert!(
        (top_left.0 - 0.1).abs() < 0.01 && (top_left.1 - 0.2).abs() < 0.01,
        "{top_left:?}"
    );
    assert_eq!(top_left.1, top_right.1);
    assert_eq!(bottom_left.1, bottom_right.1);
    assert_eq!(top_left.0, bottom_left.0);
    assert_eq!(top_right.0, bottom_right.0);
    let (width, height) = (
        (top_right.0 - top_left.0) * RENDER_SIZE.0 as f32,
        (bottom_left.1 - top_left.1) * RENDER_SIZE.1 as f32,
    );
    assert!((height - 0.15 * RENDER_SIZE.1 as f32).abs() < 1.0, "{height}");
    assert!(
        (width / height - indicator_aspect_ratio(SpeedFactor::new(4).unwrap())).abs() < 0.05,
        "{width} by {height}"
    );
}

#[test]
fn the_symbol_is_a_picture_of_its_speed_and_its_height_in_pixels_and_nothing_else_makes_it_another() {
    let picture = video_picture(1);
    let the_symbol_id = |speed: u32, layout: SpeedIndicatorLayout, generation: u64| {
        let mut shown = content(vec![showing(0, &picture, whole_picture())], None);
        shown.symbol = Some(the_symbol_over(0, speed));
        shown.indicator_layout = layout;
        let (description, _) = describe_with(&shown, generation, WritesItDown::default());
        description.layers[1].source
    };
    let layout = SpeedIndicatorLayout::new(0.1, 0.2, 0.15);
    let id = the_symbol_id(4, layout, 0);
    assert_eq!(the_symbol_id(4, layout, 0), id);
    assert_eq!(
        the_symbol_id(4, SpeedIndicatorLayout::new(0.5, 0.5, 0.15), 0),
        id,
        "where it is does not change what it looks like"
    );
    assert_eq!(the_symbol_id(4, layout, 9), id, "the files have nothing to do with it");
    assert_ne!(the_symbol_id(8, layout, 0), id, "another speed");
    assert_ne!(
        the_symbol_id(4, SpeedIndicatorLayout::new(0.1, 0.2, 0.3), 0),
        id,
        "another height"
    );
}

#[test]
fn a_symbol_over_a_layer_that_is_not_there_is_not_drawn() {
    let picture = video_picture(1);
    let mut shown = content(vec![showing(0, &picture, whole_picture())], None);
    shown.symbol = Some(the_symbol_over(5, 4));
    let (description, supply) = describe(&shown);
    assert_eq!(description.layers.len(), 1);
    assert_eq!(supply.kinds, vec!["video picture"]);
}

#[test]
fn a_picture_that_the_supply_cannot_make_leaves_its_layer_out_and_not_the_others() {
    let (first, second, third) = (video_picture(1), video_picture(2), video_picture(3));
    let supply = WritesItDown {
        refuses: HashSet::from([PictureId(2)]),
        ..WritesItDown::default()
    };
    let (description, supply) = describe_with(
        &content(
            vec![
                showing(0, &first, whole_picture()),
                showing(1, &second, whole_picture()),
                showing(2, &third, whole_picture()),
            ],
            None,
        ),
        0,
        supply,
    );
    let sources: Vec<LayerSource> = description.layers.iter().map(|layer| layer.source).collect();
    assert_eq!(
        sources,
        vec![LayerSource::Picture(PictureId(1)), LayerSource::Picture(PictureId(3))]
    );
    assert_eq!(supply.asked.len(), 3, "it was asked about all three");
}

#[test]
fn the_ids_of_different_kinds_of_picture_are_never_the_same_and_a_video_picture_is_its_serial_number() {
    let video = video_picture(5);
    let image = an_image();
    let library = a_thumbnail_library();
    let mut shown = content(
        vec![
            showing(0, &video, whole_picture()),
            image_layer(1, 5, Some(&image), whole_picture()),
            thumbnail_layer(3, &library),
            PreviewLayer {
                content: PreviewLayerContent::Text(Cow::Owned(TextObject::default())),
                ..layer(3, whole_picture())
            },
        ],
        None,
    );
    shown.layers[2].clip = clip(2);
    shown.symbol = Some(the_symbol_over(0, 4));
    let (description, _) = describe(&shown);
    let ids: Vec<u64> = description
        .layers
        .iter()
        .filter_map(|layer| match layer.source {
            LayerSource::Picture(PictureId(id)) => Some(id),
            LayerSource::Solid(_) => None,
        })
        .collect();
    assert_eq!(
        ids.len(),
        5,
        "the video, its symbol, the picture file, the thumbnail and the text"
    );
    assert_eq!(ids[0], 5, "a video picture's serial number");
    let kinds: Vec<u64> = ids.iter().map(|id| id >> 60).collect();
    assert_eq!(kinds, vec![0, 4, 1, 3, 2], "the kind is in the top bits of the id");
    let mut all = ids.clone();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 5);
}

#[test]
fn pictures_are_sent_once_each_and_one_that_cannot_be_made_is_tried_again_and_one_the_engine_lost_is_sent_again() {
    let picture = video_picture(9);
    let image = an_image();
    let mut sent = HashSet::new();
    let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));
    let mut arrivals: Vec<(PictureId, (u32, u32))> = Vec::new();
    let shown = content(
        vec![
            showing(0, &picture, whole_picture()),
            image_layer(1, 7, Some(&image), whole_picture()),
        ],
        None,
    );
    for _ in 0..3 {
        let mut supply = SentPictures {
            sent: &mut sent,
            text_renderer: &mut text_renderer,
            send: |id, data: PictureData| arrivals.push((id, (data.width_in_pixels, data.height_in_pixels))),
        };
        let description = describe_frame(&shown, RENDER_SIZE, 0, &mut supply);
        assert_eq!(description.layers.len(), 2);
    }
    assert_eq!(
        arrivals.len(),
        2,
        "each was sent once, however often it was drawn: {arrivals:?}"
    );
    assert_eq!(arrivals[0], (PictureId(9), (2, 2)));
    assert_eq!(arrivals[1].1, (3, 2));

    // The engine says it lost a picture: it is sent again, and the other is not.
    sent.remove(&PictureId(9));
    let mut supply = SentPictures {
        sent: &mut sent,
        text_renderer: &mut text_renderer,
        send: |id, data: PictureData| arrivals.push((id, (data.width_in_pixels, data.height_in_pixels))),
    };
    describe_frame(&shown, RENDER_SIZE, 0, &mut supply);
    assert_eq!(arrivals.len(), 3);
    assert_eq!(arrivals[2].0, PictureId(9));

    // A picture that cannot be made is asked for again next time, and is not counted as sent. (A thumbnail whose picture is not a picture.)
    let library = a_thumbnail_library();
    let mut broken_library = ThumbnailLibrary::with_room_for(10);
    broken_library.insert(
        ThumbnailKey { level: 0, index: 3 },
        CompressedPicture::that_cannot_be_unpacked(PictureSize {
            width_in_pixels: 8,
            height_in_pixels: 8,
        }),
    );
    let broken = content(vec![thumbnail_layer(3, &broken_library)], None);
    let sent_before = sent.len();
    let mut supply = SentPictures {
        sent: &mut sent,
        text_renderer: &mut text_renderer,
        send: |_, _| panic!("a picture that cannot be made must not be sent"),
    };
    let description = describe_frame(&broken, RENDER_SIZE, 0, &mut supply);
    assert!(description.layers.is_empty());
    assert_eq!(sent.len(), sent_before);
    drop(library);
}

#[test]
fn what_a_picture_is_made_of_is_what_the_engine_is_sent() {
    let mut text_renderer = LazyTextRenderer::new(Arc::new(|| {}));

    // A video picture in RGBA, as ffmpeg made it.
    let rgba = video_picture(1);
    let data = HowToMakeIt::VideoPicture(&rgba).make(&mut text_renderer).unwrap();
    assert_eq!((data.width_in_pixels, data.height_in_pixels), (2, 2));
    assert!(matches!(&data.pixels, PicturePixels::Rgba8 { straight_alpha } if straight_alpha == &vec![10; 16]));

    // A video picture in planes: the planes and what the numbers stand for.
    let planes = SuppliedPicture {
        frame: SuppliedFrame::Yuv420p {
            frame: DecodedYuvFrame {
                width_in_pixels: 3,
                height_in_pixels: 3,
                y: (0..9).collect(),
                u: vec![1, 2, 3, 4],
                v: vec![5, 6, 7, 8],
            },
            colour: NativeYuv420p {
                matrix: NativeMatrix::Bt709,
                full_range: true,
            },
        },
        ..video_picture(2)
    };
    let data = HowToMakeIt::VideoPicture(&planes).make(&mut text_renderer).unwrap();
    assert_eq!((data.width_in_pixels, data.height_in_pixels), (3, 3));
    let PicturePixels::Yuv420p8 { y, u, v, colour } = &data.pixels else {
        panic!("planes were expected");
    };
    assert_eq!((y.len(), u.len(), v.len()), (9, 4, 4));
    assert_eq!(
        *colour,
        crate::playback::colour_for_the_engine(NativeYuv420p {
            matrix: NativeMatrix::Bt709,
            full_range: true,
        })
    );

    // A picture file, a thumbnail, the symbol.
    let image = an_image();
    let data = HowToMakeIt::Image(&image).make(&mut text_renderer).unwrap();
    assert_eq!((data.width_in_pixels, data.height_in_pixels), (3, 2));
    let library = a_thumbnail_library();
    let (_, compressed) = library.nearest_made(ThumbnailKey { level: 0, index: 3 }).unwrap();
    let data = HowToMakeIt::Thumbnail(compressed).make(&mut text_renderer).unwrap();
    assert_eq!((data.width_in_pixels, data.height_in_pixels), (8, 8));
    let data = HowToMakeIt::Symbol {
        speed: SpeedFactor::new(4).unwrap(),
        height_in_pixels: 60,
    }
    .make(&mut text_renderer)
    .unwrap();
    assert_eq!(data.height_in_pixels, 60);

    // Text, once the fonts are there.
    text_renderer.get_waiting_if_needed().expect("the fonts load");
    let object = TextObject::default();
    let data = HowToMakeIt::Text {
        object: &object,
        picture_size: (1600, 900),
    }
    .make(&mut text_renderer)
    .unwrap();
    assert!(data.width_in_pixels > 0 && data.height_in_pixels > 0);
}
