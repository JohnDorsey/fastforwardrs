// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture engine is asked to draw for the preview, worked out from the same thing the old drawing is given (`PreviewContent`), so that the two agree
//! by construction about what is on the picture: the same layers, in the same order, in the same places, with the same colours and the same pictures (see
//! `docs/engine-phase-1-design.md`, section 8).
//!
//! [`describe_frame`] **sends nothing itself.** For each picture that a layer is made of it asks a [`PictureSupply`] to make sure the engine has it (the supply
//! makes the pixels and sends them if the engine has not been sent them already), and it leaves a layer out when its picture cannot be had yet, as the old
//! drawing draws nothing for a text object whose fonts are not loaded or a thumbnail that cannot be unpacked. [`SentPictures`] is the supply that remembers
//! which pictures the engine has been sent, and makes and sends the others.
//!
//! The layers that cannot show a picture (a video whose picture has not come, a picture file that is not decoded yet) are a colour, as the old drawing makes
//! them: [`theme::TRACK_BACKGROUND`].

// The window uses this from piece 1.5 of phase 1 on; until then only the tests do.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Rect, pos2, vec2};

use super::speed_indicator_overlay::SymbolOnScreen;
use super::{PreviewContent, PreviewLayer, PreviewLayerContent, SymbolOnThePreview, VideoInThePicture};
use crate::application::theme;
use crate::engine::{EffectCall, FrameDescription, LayerDescription, LayerSource, PictureData, PictureId};
use crate::media::{CompressedPicture, ThumbnailKey};
use crate::playback::{SuppliedFrame, SuppliedPicture, colour_for_the_engine};
use crate::project::{Corner, SpeedFactor, TextObject};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::{SMALLEST_HEIGHT_IN_PIXELS, render_speed_indicator};
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
}

impl HowToMakeIt<'_> {
    /// The pixels of the picture, or `None` if they cannot be made now (the fonts of a text are not loaded, a thumbnail does not unpack).
    pub fn make(&self, text_renderer: &mut LazyTextRenderer) -> Option<PictureData> {
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
                .ok(),
            },
            Self::Thumbnail(compressed) => {
                let frame = compressed.decompress().ok()?;
                rgba(frame.width_in_pixels, frame.height_in_pixels, &frame.rgba_pixels)
            }
            Self::Image(picture) => rgba(picture.width_in_pixels, picture.height_in_pixels, &picture.rgba_pixels),
            Self::Text { object, picture_size } => {
                let picture = text_renderer
                    .get_if_ready()?
                    .render(object, picture_size.0, picture_size.1);
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
    pub text_renderer: &'a mut LazyTextRenderer,
    pub send: Send,
}

impl<Send: FnMut(PictureId, PictureData)> PictureSupply for SentPictures<'_, Send> {
    fn make_sure_the_engine_has(&mut self, wanted: &PictureWanted<'_>) -> bool {
        if self.sent.contains(&wanted.id) {
            return true;
        }
        let Some(data) = wanted.how.make(self.text_renderer) else {
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

/// The id of a picture of `kind` that is told from the others of its kind by `what`.
fn id_of(kind: KindOfPicture, what: impl Hash) -> PictureId {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    what.hash(&mut hasher);
    PictureId((hasher.finish() >> 4) | ((kind as u64) << 60))
}

/// The id of the picture of a video that has this serial number.
fn id_of_a_video_picture(serial_number: u64) -> PictureId {
    debug_assert!(
        serial_number < 1 << 60,
        "{serial_number} has no room for the kind of picture it is"
    );
    PictureId(serial_number)
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

/// The colour of a layer that has no picture to show yet.
fn colour_of_a_layer_without_a_picture() -> [u8; 4] {
    theme::TRACK_BACKGROUND.to_srgba_unmultiplied()
}

/// What the engine is to draw for `content` on a frame `render_size` big, the layers at the back first. `generation` is a number that the window changes
/// whenever it lets go of what it knows of the files it shows (when another project is opened or a file is replaced), so that the ids of pictures that come
/// from files are not those of the pictures that were there before.
pub fn describe_frame(
    content: &PreviewContent<'_>,
    render_size: (u32, u32),
    generation: u64,
    supply: &mut impl PictureSupply,
) -> FrameDescription {
    let mut description = FrameDescription::black(render_size);
    let Some(picture_size) = content.picture_size else {
        return description;
    };
    for layer in &content.layers {
        description
            .layers
            .extend(describe_layer(layer, picture_size, generation, supply));
        if let Some(symbol) = content.symbol
            && symbol.over == layer.clip
        {
            description
                .layers
                .extend(describe_symbol(content, symbol, render_size, supply));
        }
    }
    description
}

/// The layer that `layer` is, if it has anything to show.
fn describe_layer(
    layer: &PreviewLayer<'_>,
    picture_size: (u32, u32),
    generation: u64,
    supply: &mut impl PictureSupply,
) -> Option<LayerDescription> {
    let corners = corners_of(layer, picture_size);
    let wanted = match &layer.content {
        PreviewLayerContent::Video(VideoInThePicture::Picture(supplied)) => PictureWanted {
            id: id_of_a_video_picture(supplied.serial_number),
            how: HowToMakeIt::VideoPicture(supplied),
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
                id: id_of(KindOfPicture::Thumbnail, (generation, source, made_key)),
                how: HowToMakeIt::Thumbnail(compressed),
            }
        }
        PreviewLayerContent::Image {
            source,
            picture: Some(decoded),
        } => PictureWanted {
            id: id_of(
                KindOfPicture::Image,
                (generation, source, decoded.width_in_pixels, decoded.height_in_pixels),
            ),
            how: HowToMakeIt::Image(decoded),
        },
        PreviewLayerContent::Text(object) => PictureWanted {
            id: id_of(
                KindOfPicture::Text,
                TextAppearance::of(object, picture_size.0, picture_size.1),
            ),
            how: HowToMakeIt::Text { object, picture_size },
        },
        PreviewLayerContent::Video(VideoInThePicture::Waiting) | PreviewLayerContent::Image { picture: None, .. } => {
            return Some(LayerDescription {
                source: LayerSource::Solid(colour_of_a_layer_without_a_picture()),
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

/// The fast-forward symbol, as a layer of its own right after the layer it is over, so that what is in front of that layer still covers it. It is drawn at the
/// height that it has in the pixels of the frame.
fn describe_symbol(
    content: &PreviewContent<'_>,
    symbol: SymbolOnThePreview,
    render_size: (u32, u32),
    supply: &mut impl PictureSupply,
) -> Option<LayerDescription> {
    let (width, height) = (render_size.0.max(1) as f32, render_size.1.max(1) as f32);
    let picture = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height));
    let on_the_frame = SymbolOnScreen::of(content.indicator_layout, symbol.speed, picture).body;
    let height_in_pixels = (on_the_frame.height().round() as u32).max(SMALLEST_HEIGHT_IN_PIXELS);
    let wanted = PictureWanted {
        id: id_of(KindOfPicture::Symbol, (symbol.speed.value(), height_in_pixels)),
        how: HowToMakeIt::Symbol {
            speed: symbol.speed,
            height_in_pixels,
        },
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
