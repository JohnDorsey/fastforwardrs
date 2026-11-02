// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Masks in a frame that is described for the engine (`docs/shader-masks-design.md`): the effect that reads a mask is given the mask as a group of the layers of its track, the layers of a
//! mask have their own effects and masks, a mask that is not among the masks of the frame is nothing, and the fast-forward symbol is only ever over the frame's own layers. Nothing here needs a
//! graphics card.

use super::*;
use crate::engine::{CORNERS_OF_THE_WHOLE_PICTURE, FrameColour, GroupDescription, LayerSource};
use crate::project::{CanvasArea, SpeedIndicatorLayout};

/// A supply that has every picture.
struct HasEverything;

impl PictureSupply for HasEverything {
    fn make_sure_the_engine_has(&mut self, _: &PictureWanted<'_>) -> bool {
        true
    }
}

fn a_picture() -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: vec![200; 16],
    }
}

fn the_shader(key: &str, masks: Vec<TrackId>) -> PictureOperation {
    PictureOperation::Shader {
        key: key.to_owned(),
        numbers: vec![0.0, 0.0, 0.0, 0.0, 0.25],
        state: None,
        images: Vec::new(),
        reads_the_backdrop: false,
        masks,
    }
}

/// A layer that is the picture file number `source`, of the clip `clip` on the track `track`, over the whole picture, with these operations.
fn a_layer<'a>(
    clip: u32,
    track: u32,
    source: u32,
    picture: &'a RgbaPicture,
    operations: Vec<PictureOperation>,
) -> PreviewLayer<'a> {
    PreviewLayer {
        clip: ClipId::new(TrackId::new(track), clip),
        track: TrackId::new(track),
        placement: Placement::upright(CanvasArea::WHOLE_PICTURE),
        operations,
        content: PreviewLayerContent::Image {
            source: SourceMediaIdentifier::new(source),
            picture: Some(picture),
            colour: PictureColour::SRGB,
        },
    }
}

fn a_mask<'a>(track: u32, layers: Vec<PreviewLayer<'a>>, masks: Vec<PreviewMask<'a>>) -> PreviewMask<'a> {
    PreviewMask {
        track: TrackId::new(track),
        layers,
        groups: Vec::new(),
        masks,
    }
}

fn the_frame<'a>(
    layers: Vec<PreviewLayer<'a>>,
    masks: Vec<PreviewMask<'a>>,
    symbol: Option<SymbolOnThePreview>,
) -> PreviewContent<'a> {
    PreviewContent {
        picture_size: Some((1600, 900)),
        layers,
        groups: Vec::new(),
        masks,
        symbol,
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
        selected_clip: None,
    }
}

fn describe(content: &PreviewContent<'_>) -> crate::engine::FrameDescription {
    describe_frame(
        content,
        (800, 450),
        0,
        [0, 0, 0, 0],
        FrameColour::default(),
        &mut HasEverything,
    )
}

#[test]
fn the_effect_that_reads_a_mask_is_given_the_layers_of_the_track_as_a_group_on_its_call() {
    let (picture, in_the_mask) = (a_picture(), a_picture());
    let host = a_layer(
        1,
        1,
        1,
        &picture,
        vec![the_shader("isf:masked+masks[amount=0:Red]", vec![TrackId::new(7)])],
    );
    let mask_layer = a_layer(2, 7, 2, &in_the_mask, Vec::new());
    let content = the_frame(vec![host], vec![a_mask(7, vec![mask_layer], Vec::new())], None);

    let described = describe(&content);

    assert_eq!(described.layers.len(), 1, "the mask is not a layer of the frame");
    let call = &described.layers[0].effects[0];
    assert_eq!(call.key, "isf:masked+masks[amount=0:Red]");
    assert_eq!(call.values, [0.0, 0.0, 0.0, 0.0, 0.25]);
    assert_eq!(call.masks.len(), 1);
    assert_eq!(call.masks[0].layers.len(), 1, "the one layer of the track");
    assert!(matches!(call.masks[0].layers[0].source, LayerSource::Picture(_)));
    assert_eq!(call.masks[0].layers[0].corners, CORNERS_OF_THE_WHOLE_PICTURE);
    assert!(
        described.pictures_used().len() >= 2,
        "the picture of the mask is a picture of the frame, so that it is sent"
    );
}

#[test]
fn two_masks_are_in_the_call_in_the_order_the_operation_names_them_and_a_mask_that_is_read_twice_is_in_each_call() {
    let (picture, first, second) = (
        a_picture(),
        a_picture(),
        RgbaPicture {
            rgba_pixels: vec![9; 16],
            ..a_picture()
        },
    );
    let host = a_layer(
        1,
        1,
        1,
        &picture,
        vec![
            the_shader("isf:two", vec![TrackId::new(8), TrackId::new(7)]),
            the_shader("isf:one", vec![TrackId::new(7)]),
        ],
    );
    let content = the_frame(
        vec![host],
        vec![
            a_mask(7, vec![a_layer(2, 7, 2, &first, Vec::new())], Vec::new()),
            a_mask(
                8,
                vec![
                    a_layer(3, 8, 3, &second, Vec::new()),
                    a_layer(4, 8, 4, &second, Vec::new()),
                ],
                Vec::new(),
            ),
        ],
        None,
    );

    let described = describe(&content);

    let calls = &described.layers[0].effects;
    assert_eq!(
        calls[0].masks.iter().map(|mask| mask.layers.len()).collect::<Vec<_>>(),
        [2, 1],
        "the track 8 first, as the operation says, and then the track 7"
    );
    assert_eq!(
        calls[1].masks.iter().map(|mask| mask.layers.len()).collect::<Vec<_>>(),
        [1]
    );
}

#[test]
fn a_mask_that_is_not_among_the_masks_of_the_frame_is_a_group_with_nothing_in_it() {
    let picture = a_picture();
    let host = a_layer(1, 1, 1, &picture, vec![the_shader("isf:masked", vec![TrackId::new(7)])]);
    let content = the_frame(vec![host], Vec::new(), None);

    let described = describe(&content);

    assert_eq!(
        described.layers[0].effects[0].masks,
        [GroupDescription { layers: Vec::new() }],
        "the number of the call is what it is everywhere"
    );
}

#[test]
fn the_layers_of_a_mask_have_the_effects_they_have_and_the_masks_those_read() {
    let (picture, middle, inner) = (a_picture(), a_picture(), a_picture());
    let host = a_layer(1, 1, 1, &picture, vec![the_shader("isf:outer", vec![TrackId::new(7)])]);
    let in_the_middle = a_layer(2, 7, 2, &middle, vec![the_shader("isf:inner", vec![TrackId::new(9)])]);
    let at_the_bottom = a_layer(3, 9, 3, &inner, Vec::new());
    let content = the_frame(
        vec![host],
        vec![a_mask(
            7,
            vec![in_the_middle],
            vec![a_mask(9, vec![at_the_bottom], Vec::new())],
        )],
        None,
    );

    let described = describe(&content);

    let outer = &described.layers[0].effects[0];
    let layer_of_the_mask = &outer.masks[0].layers[0];
    assert_eq!(layer_of_the_mask.effects[0].key, "isf:inner");
    assert_eq!(layer_of_the_mask.effects[0].masks.len(), 1, "the mask of the mask");
    assert_eq!(layer_of_the_mask.effects[0].masks[0].layers.len(), 1);
}

#[test]
fn the_fast_forward_symbol_is_over_the_layers_of_the_frame_and_never_in_a_mask() {
    let (picture, in_the_mask) = (a_picture(), a_picture());
    // The same clip is a layer of the frame and a layer of the mask (its track is seen and is read as a mask): the symbol is over the clip in the frame, once.
    let host = a_layer(1, 1, 1, &picture, vec![the_shader("isf:masked", vec![TrackId::new(7)])]);
    let seen_and_a_mask = a_layer(2, 7, 2, &picture, Vec::new());
    let same_in_the_mask = a_layer(2, 7, 2, &in_the_mask, Vec::new());
    let symbol = SymbolOnThePreview {
        over: ClipId::new(TrackId::new(7), 2),
        speed: SpeedFactor::new(4).unwrap(),
    };
    let content = the_frame(
        vec![host, seen_and_a_mask],
        vec![a_mask(7, vec![same_in_the_mask], Vec::new())],
        Some(symbol),
    );

    let described = describe(&content);

    assert_eq!(
        described.layers.len(),
        3,
        "the host, the seen track, and the symbol over it"
    );
    assert_eq!(
        described.layers[0].effects[0].masks[0].layers.len(),
        1,
        "and nothing but the track's layer in the mask: no symbol"
    );
}

#[test]
fn a_frame_with_no_masks_is_described_as_it_always_was() {
    let picture = a_picture();
    let layers = || {
        vec![
            a_layer(1, 1, 1, &picture, Vec::new()),
            a_layer(2, 2, 2, &picture, Vec::new()),
        ]
    };

    let with_none = describe(&the_frame(layers(), Vec::new(), None));
    let with_a_mask_nothing_reads = describe(&the_frame(
        layers(),
        vec![a_mask(5, vec![a_layer(3, 5, 3, &picture, Vec::new())], Vec::new())],
        None,
    ));

    assert_eq!(with_none.layers.len(), 2);
    assert_eq!(
        with_none.layers, with_a_mask_nothing_reads.layers,
        "a mask that no effect reads changes nothing about the layers"
    );
}
