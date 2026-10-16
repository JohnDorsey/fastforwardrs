// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture engine is asked to draw for a frame, worked out from the same thing the old drawing of the preview is given (`PreviewContent`), so that the two agree
//! by construction about what is on the picture: the same layers, in the same order, in the same places, with the same colours and the same pictures (see
//! `docs/engine-phase-1-design.md`, section 8), and from the same thing the export is given, so that the export draws what the preview does (phase 2).
//!
//! [`describe_frame`] **sends nothing itself.** For each picture that a layer is made of it asks a [`PictureSupply`] to make sure the engine has it (the supply
//! makes the pixels and sends them if the engine has not been sent them already), and it leaves a layer out when its picture cannot be had yet, as the old
//! drawing draws nothing for a text object whose fonts are not loaded or a thumbnail that cannot be unpacked. [`SentPictures`] is the supply that remembers
//! which pictures the engine has been sent, and makes and sends the others.
//!
//! The layers that cannot show a picture (a video whose picture has not come, a picture file that is not decoded yet) are a colour, as the old drawing makes
//! them: the colour that [`describe_frame`] is given for them (the window's `TRACK_BACKGROUND`).

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Rect, pos2, vec2};

use super::{PreviewContent, PreviewLayer, PreviewLayerContent, SymbolOnThePreview, VideoInThePicture};
use crate::engine::{
    EffectCall, FrameColour, FrameDescription, LayerDescription, LayerSource, PictureColour, PictureData, PictureId,
    WorkingSpace,
};
use crate::media::{CompressedPicture, ThumbnailKey};
use crate::playback::{SuppliedFrame, SuppliedPicture, colour_for_the_engine};
use crate::project::{Corner, SpeedFactor, SpeedIndicatorLayout, TextObject};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::{
    SMALLEST_HEIGHT_IN_PIXELS, indicator_aspect_ratio, place_indicator, render_speed_indicator,
};
use crate::text_rendering::{LazyTextRenderer, TextAppearance};

/// How to make the pixels of a picture that a layer is made of, from what the content has in it.
pub enum HowToMakeIt<'c> {
    /// The picture of a video that the supplier has (in RGBA, or in planes).
    VideoPicture(&'c SuppliedPicture),
    /// A thumbnail of a video, which stands in until its picture comes.
    Thumbnail(&'c CompressedPicture),
    /// The picture of a picture file.
    Image(&'c RgbaPicture),
    /// The picture of a text object on a picture of this size, which only the text renderer can make, and only once it has the fonts.
    Text {
        object: &'c TextObject,
        picture_size: (u32, u32),
    },
    /// The fast-forward symbol of this speed, this many pixels tall.
    Symbol { speed: SpeedFactor, height_in_pixels: u32 },
}

/// A picture that a layer needs the engine to have, and how to make it if it does not.
pub struct PictureWanted<'c> {
    pub id: PictureId,
    pub how: HowToMakeIt<'c>,
    /// How the numbers of the picture are to be read (what its codes mean), which its id says too.
    pub colour: PictureColour,
    /// The working space the engine is to make it for, which its id says too.
    pub working: WorkingSpace,
}

impl PictureWanted<'_> {
    /// The pixels of the picture, with the colour it is made in, or `None` if they cannot be made now (see [`HowToMakeIt::make`]).
    pub fn make(&self, text_pictures: &mut dyn TextPictures) -> Option<PictureData> {
        self.how
            .make(text_pictures)
            .map(|data| data.in_colour(self.colour, self.working))
    }
}

impl HowToMakeIt<'_> {
    /// The pixels of the picture, or `None` if they cannot be made now (the fonts of a text are not loaded, a thumbnail does not unpack).
    pub fn make(&self, text_pictures: &mut dyn TextPictures) -> Option<PictureData> {
        let rgba = |width: u32, height: u32, pixels: &[u8]| PictureData::rgba8(width, height, pixels.to_vec()).ok();
        match self {
            Self::VideoPicture(supplied) => match &supplied.frame {
                SuppliedFrame::Rgba(frame) => rgba(frame.width_in_pixels, frame.height_in_pixels, &frame.rgba_pixels),
                SuppliedFrame::Yuv420p { frame, colour } => PictureData::yuv420p8(
                    frame.width_in_pixels,
                    frame.height_in_pixels,
                    frame.y.clone(),
                    frame.u.clone(),
                    frame.v.clone(),
                    colour_for_the_engine(*colour),
                )
                .ok()
                .map(|data| data.with_chroma_siting(supplied.chroma_siting)),
            },
            Self::Thumbnail(compressed) => {
                let frame = compressed.decompress().ok()?;
                rgba(frame.width_in_pixels, frame.height_in_pixels, &frame.rgba_pixels)
            }
            Self::Image(picture) => rgba(picture.width_in_pixels, picture.height_in_pixels, &picture.rgba_pixels),
            Self::Text { object, picture_size } => {
                let picture = text_pictures.text_picture(object, picture_size.0, picture_size.1)?;
                rgba(picture.width_in_pixels, picture.height_in_pixels, &picture.rgba_pixels)
            }
            Self::Symbol {
                speed,
                height_in_pixels,
            } => {
                let picture = render_speed_indicator(*speed, *height_in_pixels);
                rgba(picture.width_in_pixels, picture.height_in_pixels, &picture.rgba_pixels)
            }
        }
    }
}

/// What makes the picture of a text object when a frame is described: the computer's fonts (the preview's, and the export's), or, for a test that must not depend on
/// the fonts a computer has, a function of its own.
pub trait TextPictures {
    /// The picture of `object` on a finished picture that is `picture_width_in_pixels` by `picture_height_in_pixels` (the picture of the object's own area, text on
    /// transparent: see `TextRenderer::render`), or `None` if it cannot be made now (the fonts are not loaded yet).
    fn text_picture(
        &mut self,
        object: &TextObject,
        picture_width_in_pixels: u32,
        picture_height_in_pixels: u32,
    ) -> Option<RgbaPicture>;
}

impl TextPictures for LazyTextRenderer {
    fn text_picture(
        &mut self,
        object: &TextObject,
        picture_width_in_pixels: u32,
        picture_height_in_pixels: u32,
    ) -> Option<RgbaPicture> {
        Some(
            self.get_if_ready()?
                .render(object, picture_width_in_pixels, picture_height_in_pixels),
        )
    }
}

/// A function that draws a text object is a source of text pictures that is always ready.
impl<F: FnMut(&TextObject, u32, u32) -> RgbaPicture> TextPictures for F {
    fn text_picture(
        &mut self,
        object: &TextObject,
        picture_width_in_pixels: u32,
        picture_height_in_pixels: u32,
    ) -> Option<RgbaPicture> {
        Some(self(object, picture_width_in_pixels, picture_height_in_pixels))
    }
}

/// What makes sure the engine has the pictures a frame is made of.
pub trait PictureSupply {
    /// Makes sure the engine has `wanted` (sending it if it has not been sent), and says whether it has it now: `false` if the picture cannot be made yet.
    fn make_sure_the_engine_has(&mut self, wanted: &PictureWanted<'_>) -> bool;
}

/// A [`PictureSupply`] that remembers which pictures the engine has been sent, and makes and sends the others with `send`. A picture that cannot be made is not
/// counted as sent, so it is tried again next time.
pub struct SentPictures<'a, Send: FnMut(PictureId, PictureData)> {
    /// The pictures the engine has. (Whoever owns it takes an id out when the engine says it let go of the picture, or has lost it.)
    pub sent: &'a mut HashSet<PictureId>,
    pub text_renderer: &'a mut dyn TextPictures,
    pub send: Send,
}

impl<Send: FnMut(PictureId, PictureData)> PictureSupply for SentPictures<'_, Send> {
    fn make_sure_the_engine_has(&mut self, wanted: &PictureWanted<'_>) -> bool {
        if self.sent.contains(&wanted.id) {
            return true;
        }
        let Some(data) = wanted.make(self.text_renderer) else {
            return false;
        };
        (self.send)(wanted.id, data);
        self.sent.insert(wanted.id);
        true
    }
}

/// What kind of picture an id is of (other than a video's, whose id is its serial number, and has these bits 0). It is in the top four bits of the id, so that
/// the ids of two kinds of pictures are never the same.
#[derive(Clone, Copy, Hash)]
enum KindOfPicture {
    Image = 1,
    Text = 2,
    Thumbnail = 3,
    Symbol = 4,
}

/// What an id says of the colour its picture is made in: **nothing** in the compatibility mode with a picture that is sRGB (the id is what it always was), and otherwise a
/// number made of the working space and the colour, never 0, so that a picture that is made in another colour is another picture to the engine, and a setting that is
/// changed does not make a frame use a picture that was made for the one before.
fn colour_tag_of(colour: PictureColour, working: WorkingSpace) -> u64 {
    if working == WorkingSpace::DisplayReferred && colour == PictureColour::SRGB {
        return 0;
    }
    let mut hasher = DefaultHasher::new();
    (colour, working).hash(&mut hasher);
    (hasher.finish() & 0xFFFF) | 1
}

/// The id of a picture of `kind` that is told from the others of its kind by `what`, made in the colour `colour_tag` says (see [`colour_tag_of`]).
fn id_of(kind: KindOfPicture, what: impl Hash, colour_tag: u64) -> PictureId {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    what.hash(&mut hasher);
    if colour_tag != 0 {
        colour_tag.hash(&mut hasher);
    }
    PictureId((hasher.finish() >> 4) | ((kind as u64) << 60))
}

/// How many of the bits of the id of a video picture are the serial number of the picture: the ones above them (up to the four that say what kind of picture it is) are
/// the tag of its colour.
const BITS_OF_THE_SERIAL_NUMBER_IN_AN_ID: u32 = 44;

/// The id of the picture of a video that has this serial number, made in the colour `colour_tag` says.
fn id_of_a_video_picture(serial_number: u64, colour_tag: u64) -> PictureId {
    debug_assert!(
        serial_number < 1 << BITS_OF_THE_SERIAL_NUMBER_IN_AN_ID,
        "{serial_number} has no room for the colour of the picture"
    );
    PictureId(serial_number | (colour_tag << BITS_OF_THE_SERIAL_NUMBER_IN_AN_ID))
}

/// Where a layer is on the picture, as the four corners (top left, top right, bottom right, bottom left) in fractions of the picture: turned as it is, and worked
/// out as the old drawing works them out (in pixels of the finished picture, where turning is the same along and across).
fn corners_of(layer: &PreviewLayer<'_>, picture_size: (u32, u32)) -> [(f32, f32); 4] {
    let size_in_pixels = (picture_size.0.max(1) as f32, picture_size.1.max(1) as f32);
    [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomRight,
        Corner::BottomLeft,
    ]
    .map(|corner| layer.placement.corner_position(corner, size_in_pixels))
}

/// What the engine is to draw for `content` on a frame `render_size` big, the layers at the back first. `generation` is a number that the window changes
/// whenever it lets go of what it knows of the files it shows (when another project is opened or a file is replaced), so that the ids of pictures that come
/// from files are not those of the pictures that were there before. `colour_of_a_layer_without_a_picture` is what a layer is that has none yet (red, green, blue
/// and how opaque, not premultiplied). **`colour` is what the frame is put together in and encoded as** (phase 3): the pictures that are asked of the supply are made for
/// its working space, and the ids say so.
pub fn describe_frame(
    content: &PreviewContent<'_>,
    render_size: (u32, u32),
    generation: u64,
    colour_of_a_layer_without_a_picture: [u8; 4],
    colour: FrameColour,
    supply: &mut impl PictureSupply,
) -> FrameDescription {
    let mut description = FrameDescription::black(render_size);
    description.colour = colour;
    let Some(picture_size) = content.picture_size else {
        return description;
    };
    for layer in &content.layers {
        description.layers.extend(describe_layer(
            layer,
            picture_size,
            generation,
            colour_of_a_layer_without_a_picture,
            colour.working,
            supply,
        ));
        if let Some(symbol) = content.symbol
            && symbol.over == layer.clip
        {
            description
                .layers
                .extend(describe_symbol(content, symbol, render_size, colour.working, supply));
        }
    }
    description
}

/// The layer that `layer` is, if it has anything to show.
fn describe_layer(
    layer: &PreviewLayer<'_>,
    picture_size: (u32, u32),
    generation: u64,
    colour_of_a_layer_without_a_picture: [u8; 4],
    working: WorkingSpace,
    supply: &mut impl PictureSupply,
) -> Option<LayerDescription> {
    let corners = corners_of(layer, picture_size);
    // (A text, a symbol and a thumbnail are sRGB. The picture of a video or of a picture file is what its file says, or what the user says, or what is assumed of it: it
    // comes with its colour.)
    let colour = PictureColour::SRGB;
    let tag = colour_tag_of(colour, working);
    let wanted = match &layer.content {
        PreviewLayerContent::Video(VideoInThePicture::Picture(supplied)) => PictureWanted {
            id: id_of_a_video_picture(supplied.serial_number, colour_tag_of(supplied.colour, working)),
            how: HowToMakeIt::VideoPicture(supplied),
            colour: supplied.colour,
            working,
        },
        PreviewLayerContent::Video(VideoInThePicture::Thumbnail {
            source,
            second,
            library,
        }) => {
            let key = ThumbnailKey {
                level: 0,
                index: *second,
            };
            // (The finest thumbnail that is made, which is not the one that was asked for until it is: a blurrier picture of about the same moment is
            // better than none. What is drawn is nothing if there is not even that.)
            let (made_key, compressed) = library.nearest_made(key)?;
            PictureWanted {
                id: id_of(KindOfPicture::Thumbnail, (generation, source, made_key), tag),
                how: HowToMakeIt::Thumbnail(compressed),
                colour,
                working,
            }
        }
        PreviewLayerContent::Image {
            source,
            picture: Some(decoded),
            colour: image_colour,
        } => PictureWanted {
            id: id_of(
                KindOfPicture::Image,
                (generation, source, decoded.width_in_pixels, decoded.height_in_pixels),
                colour_tag_of(*image_colour, working),
            ),
            how: HowToMakeIt::Image(decoded),
            colour: *image_colour,
            working,
        },
        PreviewLayerContent::Text(object) => PictureWanted {
            id: id_of(
                KindOfPicture::Text,
                TextAppearance::of(object, picture_size.0, picture_size.1),
                tag,
            ),
            how: HowToMakeIt::Text { object, picture_size },
            colour,
            working,
        },
        PreviewLayerContent::Video(VideoInThePicture::Waiting) | PreviewLayerContent::Image { picture: None, .. } => {
            return Some(LayerDescription {
                source: LayerSource::Solid(colour_of_a_layer_without_a_picture),
                corners,
                effects: Vec::new(),
            });
        }
    };
    if !supply.make_sure_the_engine_has(&wanted) {
        return None;
    }
    // A layer's effects that only mix colours are one matrix already (the content has them folded, and the window cuts the colours off once, at the end),
    // which is one call, unless it changes nothing.
    let effects = if layer.colour.changes_nothing() {
        Vec::new()
    } else {
        vec![EffectCall::colour_matrix(layer.colour.rows())]
    };
    Some(LayerDescription {
        source: LayerSource::Picture(wanted.id),
        corners,
        effects,
    })
}

/// Where the fast-forward symbol of `speed` is on a frame `width` by `height` pixels, with `layout` (the same sum that the window does to say where the symbol is on
/// the screen: `SymbolOnScreen::of`, with the picture at the origin).
fn the_symbol_on_a_frame(layout: SpeedIndicatorLayout, speed: SpeedFactor, width: f32, height: f32) -> Rect {
    let picture = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height));
    let placement = place_indicator(layout, picture.width(), picture.height(), indicator_aspect_ratio(speed));
    Rect::from_min_size(
        picture.min + vec2(placement.left, placement.top),
        vec2(placement.width, placement.height),
    )
}

/// The fast-forward symbol, as a layer of its own right after the layer it is over, so that what is in front of that layer still covers it. It is drawn at the
/// height that it has in the pixels of the frame.
fn describe_symbol(
    content: &PreviewContent<'_>,
    symbol: SymbolOnThePreview,
    render_size: (u32, u32),
    working: WorkingSpace,
    supply: &mut impl PictureSupply,
) -> Option<LayerDescription> {
    let (width, height) = (render_size.0.max(1) as f32, render_size.1.max(1) as f32);
    let on_the_frame = the_symbol_on_a_frame(content.indicator_layout, symbol.speed, width, height);
    let height_in_pixels = (on_the_frame.height().round() as u32).max(SMALLEST_HEIGHT_IN_PIXELS);
    let colour = PictureColour::SRGB;
    let wanted = PictureWanted {
        id: id_of(
            KindOfPicture::Symbol,
            (symbol.speed.value(), height_in_pixels),
            colour_tag_of(colour, working),
        ),
        how: HowToMakeIt::Symbol {
            speed: symbol.speed,
            height_in_pixels,
        },
        colour,
        working,
    };
    if !supply.make_sure_the_engine_has(&wanted) {
        return None;
    }
    let (left, top) = (on_the_frame.left() / width, on_the_frame.top() / height);
    let (right, bottom) = (on_the_frame.right() / width, on_the_frame.bottom() / height);
    Some(LayerDescription {
        source: LayerSource::Picture(wanted.id),
        corners: [(left, top), (right, top), (right, bottom), (left, bottom)],
        effects: Vec::new(),
    })
}
