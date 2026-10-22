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

use super::{PreviewContent, PreviewGroup, PreviewLayer, PreviewLayerContent, SymbolOnThePreview, VideoInThePicture};
use crate::cube_table::Interpolation;
use crate::engine::{
    CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, EffectState, FrameColour, FrameDescription, GroupDescription,
    LayerDescription, LayerSource, LutInterpolation, LutSpace, PictureColour, PictureData, PictureId, WorkingSpace,
    publish_table,
};
use crate::media::{CompressedPicture, ThumbnailKey};
use crate::playback::{SuppliedFrame, SuppliedPicture, colour_for_the_engine};
use crate::project::{Corner, PictureOperation, SpeedFactor, SpeedIndicatorLayout, TextObject};
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
    let mut describer = Describer {
        content,
        picture_size,
        render_size,
        generation,
        colour_of_a_layer_without_a_picture,
        working: colour.working,
        supply,
    };
    description.layers = describer.describe_layers(0, content.layers.len(), &content.groups);
    description
}

/// What describing the layers of a frame needs, which is the same for every layer of it and every group.
struct Describer<'c, 'a, Supply: PictureSupply> {
    content: &'c PreviewContent<'a>,
    picture_size: (u32, u32),
    render_size: (u32, u32),
    generation: u64,
    colour_of_a_layer_without_a_picture: [u8; 4],
    working: WorkingSpace,
    supply: &'c mut Supply,
}

impl<Supply: PictureSupply> Describer<'_, '_, Supply> {
    /// The layers `first..end` of the content, the one at the back first, where `groups` are the groups that make a picture of their own inside them (the one that holds another
    /// first): **a group is a layer made of the layers in it**, with the effects of the group, put over the whole frame. A group that has nothing to show is not there.
    fn describe_layers(&mut self, first: usize, end: usize, groups: &[PreviewGroup]) -> Vec<LayerDescription> {
        let mut layers = Vec::new();
        let mut at = first;
        while at < end {
            if let Some(place) = groups
                .iter()
                .position(|group| group.first_layer == at && group.end_layer <= end)
            {
                let group = &groups[place];
                // (What is inside it: the groups that come after it and begin before it ends.)
                let inside: Vec<PreviewGroup> = groups[place + 1..]
                    .iter()
                    .take_while(|inner| inner.first_layer < group.end_layer)
                    .cloned()
                    .collect();
                let mut children = self.describe_layers(group.first_layer, group.end_layer, &inside);
                let effects: Vec<EffectCall> = group.operations.iter().filter_map(call_of_an_operation).collect();
                if effects.is_empty() {
                    // (A group whose effects do nothing at this frame is pass-through: its layers are drawn into what it is in. What is behind it is black already, if it
                    // is at the frame, so no black is put there.)
                    layers.extend(children);
                } else if !children.is_empty() {
                    if group.over_black {
                        children.insert(
                            0,
                            LayerDescription {
                                source: LayerSource::Solid([0, 0, 0, 255]),
                                corners: CORNERS_OF_THE_WHOLE_PICTURE,
                                effects: Vec::new(),
                            },
                        );
                    }
                    layers.push(LayerDescription {
                        source: LayerSource::Group(GroupDescription { layers: children }),
                        corners: CORNERS_OF_THE_WHOLE_PICTURE,
                        effects,
                    });
                }
                at = group.end_layer.max(at + 1);
                continue;
            }
            let layer = &self.content.layers[at];
            layers.extend(describe_layer(
                layer,
                self.picture_size,
                self.generation,
                self.colour_of_a_layer_without_a_picture,
                self.working,
                self.supply,
            ));
            if let Some(symbol) = self.content.symbol
                && symbol.over == layer.clip
            {
                layers.extend(describe_symbol(
                    self.content,
                    symbol,
                    self.render_size,
                    self.working,
                    self.supply,
                ));
            }
            at += 1;
        }
        layers
    }
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
        PreviewLayerContent::Generator => {
            // (Its shader reads no picture and makes one, so what the effects work on is a picture that has nothing in it, the size of the frame, and what they make of it is put where
            // the clip is. A generator whose shader cannot be run, which has no effect that does anything, is nothing.)
            let effects: Vec<EffectCall> = layer.operations.iter().filter_map(call_of_an_operation).collect();
            if effects.is_empty() {
                return None;
            }
            return Some(LayerDescription {
                source: LayerSource::Group(GroupDescription {
                    layers: vec![LayerDescription {
                        source: LayerSource::Solid([0, 0, 0, 0]),
                        corners: CORNERS_OF_THE_WHOLE_PICTURE,
                        effects: Vec::new(),
                    }],
                }),
                corners,
                effects,
            });
        }
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
    // A layer's effects are its operations, one call each (the colours that are mixed one after another are one operation already, and the window cuts the
    // colours off once, at the end), unless one changes nothing.
    let effects = layer.operations.iter().filter_map(call_of_an_operation).collect();
    Some(LayerDescription {
        source: LayerSource::Picture(wanted.id),
        corners,
        effects,
    })
}

/// The call of the engine that does an operation, or none if the operation changes nothing. (The call of a colour mix has nine numbers, or twelve if it has offsets.)
fn call_of_an_operation(operation: &PictureOperation) -> Option<EffectCall> {
    if operation.changes_nothing() {
        return None;
    }
    Some(match operation {
        PictureOperation::Mix(matrix) if matrix.offsets() == [0.0; 3] => EffectCall::colour_matrix(matrix.rows()),
        PictureOperation::Mix(matrix) => EffectCall::colour_matrix_with_offsets(matrix.rows(), matrix.offsets()),
        PictureOperation::Exposure { stops } => EffectCall::exposure(*stops),
        PictureOperation::Opacity { opacity } => EffectCall::opacity(*opacity),
        PictureOperation::Lut(look) => EffectCall::lut(
            publish_table(&look.table),
            look.intensity,
            engine_space_of(look.input),
            engine_space_of(look.output),
            match look.interpolation {
                Interpolation::Tetrahedral => LutInterpolation::Tetrahedral,
                Interpolation::Trilinear => LutInterpolation::Trilinear,
            },
        ),
        PictureOperation::Blur { radius } => EffectCall::blur(*radius),
        PictureOperation::Shader { key, numbers, state } => EffectCall {
            key: key.clone(),
            values: numbers.clone(),
            table: None,
            state: state.map(|(id, frame)| EffectState { id, frame }),
        },
        PictureOperation::Crop {
            left,
            top,
            right,
            bottom,
            feather,
        } => EffectCall::crop(*left, *top, *right, *bottom, *feather),
    })
}

/// The space of the engine that a space of a table of the project is.
fn engine_space_of(space: crate::project::LutSpace) -> LutSpace {
    match space {
        crate::project::LutSpace::Screen => LutSpace::Screen,
        crate::project::LutSpace::Video => LutSpace::Video,
        crate::project::LutSpace::Linear => LutSpace::Linear,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_that_is_srgb_in_the_compatibility_mode_has_no_colour_tag_and_every_other_has_one_that_is_not_nought() {
        assert_eq!(colour_tag_of(PictureColour::SRGB, WorkingSpace::DisplayReferred), 0);
        let other_working = WorkingSpace::Linear(crate::colour_space::StandardPrimaries::Rec709);
        assert_ne!(colour_tag_of(PictureColour::SRGB, other_working), 0);
        assert!(colour_tag_of(PictureColour::SRGB, other_working) <= 0xFFFF);
        let acescg = WorkingSpace::Linear(crate::colour_space::StandardPrimaries::AcesCg);
        assert_ne!(
            colour_tag_of(PictureColour::SRGB, other_working),
            colour_tag_of(PictureColour::SRGB, acescg),
            "(another working space is another picture to the engine)"
        );
    }

    #[test]
    fn the_id_of_a_video_picture_is_its_serial_number_with_the_colour_tag_above_the_bits_of_the_number() {
        assert_eq!(id_of_a_video_picture(5, 0), PictureId(5));
        assert_eq!(id_of_a_video_picture(5, 3), PictureId(5 | (3 << 44)));
        assert_eq!(
            id_of_a_video_picture((1 << 44) - 1, 1),
            PictureId(((1 << 44) - 1) | (1 << 44)),
            "(the most the serial number can be)"
        );
    }

    #[test]
    #[should_panic(expected = "has no room for the colour")]
    fn a_serial_number_that_has_no_room_for_the_colour_is_found_out() {
        let _ = id_of_a_video_picture(1 << 44, 1);
    }
}
