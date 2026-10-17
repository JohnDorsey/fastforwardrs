// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures kept on the graphics card for drawing: the preview picture, the thumbnails that
//! are on screen, the fast-forward symbol and the text objects.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use egui::{ColorImage, Context, TextureFilter, TextureHandle, TextureOptions, TextureWrapMode};

use crate::application::timeline_panel::grain_of_the_racks as grain_image;
use crate::colour_effects::ColourMatrix;
use crate::media::{DecodedFrame, ThumbnailKey, ThumbnailLibrary};
use crate::playback::SuppliedPicture;
use crate::project::{SourceMediaIdentifier, SpeedFactor, TrackId};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::render_speed_indicator;
use crate::text_rendering::TextAppearance;

/// How many symbol pictures (each speed at each size is a different one) are kept before the
/// ones no longer needed are thrown away. Dragging the symbol's size makes many of them.
const MOST_SYMBOL_PICTURES_KEPT: usize = 24;

/// How many pictures of text are kept before the ones no longer needed are thrown away. (Resizing a
/// text object makes a new picture for every size it passes through.)
const MOST_TEXT_PICTURES_KEPT: usize = 24;

/// Which thumbnail: a picture of a video (see [`ThumbnailKey`]).
type ThumbnailTextureKey = (SourceMediaIdentifier, ThumbnailKey);

/// Which texture of a thumbnail: which thumbnail, and how its colours were changed before it was uploaded (what tells one way of
/// changing colours from another is [`ColourMatrix::bit_pattern`]). The timeline's thumbnails are as they are; those that stand in
/// for the picture of a video in the preview have the colours of the effects of the clip.
type ThumbnailTextureWithColoursKey = (ThumbnailTextureKey, [u32; 12]);

/// How a picture is put on the screen when it is drawn bigger than it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PictureFilter {
    /// Each pixel is blended into those next to it, so that a picture that is drawn bigger than it is looks smooth.
    #[default]
    Smooth,
    /// Each pixel is a block of its own, so that the pixels of a picture that is drawn a lot bigger than it is can be seen.
    Pixels,
}

impl PictureFilter {
    pub(crate) fn options(self) -> TextureOptions {
        match self {
            Self::Smooth => TextureOptions::LINEAR,
            // (When a picture is made smaller it is blended all the same.)
            Self::Pixels => TextureOptions {
                magnification: TextureFilter::Nearest,
                minification: TextureFilter::Linear,
                ..TextureOptions::LINEAR
            },
        }
    }
}

/// The picture of the video of a track on the graphics card, and which picture it is, and how it is put on the screen, and how its
/// colours were changed before it was put there.
struct PreviewTexture {
    handle: TextureHandle,
    serial_number: u64,
    filter: PictureFilter,
    colour: ColourMatrix,
}

/// The picture of an image file on the graphics card, and how it is put on the screen, and how its colours were changed before it
/// was put there.
struct ImageTexture {
    handle: TextureHandle,
    filter: PictureFilter,
    colour: ColourMatrix,
}

/// A picture on the graphics card, with what it is a picture of.
#[derive(Default)]
pub struct Textures {
    preview_pictures: HashMap<TrackId, PreviewTexture>,
    thumbnails: HashMap<ThumbnailTextureWithColoursKey, TextureHandle>,
    thumbnails_used_this_frame: HashSet<ThumbnailTextureWithColoursKey>,
    /// The finer pictures that were wanted since they were last taken, and were not there.
    finer_thumbnails_wanted: BTreeMap<SourceMediaIdentifier, BTreeSet<ThumbnailKey>>,
    symbols: HashMap<(u32, u32), TextureHandle>,
    /// The pictures of text, for each way it looks, each way it is put on the screen and each way its colours are changed
    /// (what tells one way of changing colours from another is [`ColourMatrix::bit_pattern`]).
    text_pictures: HashMap<(TextAppearance, PictureFilter, [u32; 12]), TextureHandle>,
    image_pictures: HashMap<SourceMediaIdentifier, ImageTexture>,
    /// The grain that the racks of keyframes are textured with, which is repeated across them.
    rack_grain: Option<TextureHandle>,
}

impl Textures {
    /// The texture of the preview picture of the video on `track`, with its colours changed as `colour` says, uploading it first if
    /// it is a new one or if it is to be put on the screen another way than it was.
    pub fn preview_picture(
        &mut self,
        context: &Context,
        track: TrackId,
        picture: &SuppliedPicture,
        filter: PictureFilter,
        colour: ColourMatrix,
    ) -> &TextureHandle {
        match self.preview_pictures.entry(track) {
            Entry::Occupied(mut occupied) => {
                let texture = occupied.get_mut();
                if texture.serial_number != picture.serial_number
                    || texture.filter != filter
                    || texture.colour != colour
                {
                    texture
                        .handle
                        .set(color_image_of(&picture.frame.rgba(), colour), filter.options());
                    texture.serial_number = picture.serial_number;
                    texture.filter = filter;
                    texture.colour = colour;
                }
            }
            Entry::Vacant(vacancy) => {
                vacancy.insert(PreviewTexture {
                    handle: context.load_texture(
                        format!("preview picture of {track}"),
                        color_image_of(&picture.frame.rgba(), colour),
                        filter.options(),
                    ),
                    serial_number: picture.serial_number,
                    filter,
                    colour,
                });
            }
        }
        &self.preview_pictures[&track].handle
    }

    /// The texture of the thumbnail `key` of the video `source`, or, if that one has not been made,
    /// of the nearest coarser one that has (a blurrier picture of about the same moment is better
    /// than none). `None` if there is not even that. Unpacks it on first use.
    ///
    /// A finer picture that has not been made is remembered as wanted, for
    /// [`Self::take_finer_thumbnails_wanted`] to hand over.
    pub fn thumbnail(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        library: &ThumbnailLibrary,
        key: ThumbnailKey,
    ) -> Option<&TextureHandle> {
        self.thumbnail_with_colours_changed(context, source, library, key, ColourMatrix::NOTHING_CHANGES)
    }

    /// Like [`Self::thumbnail`], with the colours of the picture changed as `colour` says. (A thumbnail with its colours changed
    /// one way is a texture of its own, apart from the same thumbnail as it is or changed another way.)
    pub fn thumbnail_with_colours_changed(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        library: &ThumbnailLibrary,
        key: ThumbnailKey,
        colour: ColourMatrix,
    ) -> Option<&TextureHandle> {
        if key.level > 0 && library.get(key).is_none() {
            self.finer_thumbnails_wanted.entry(source).or_default().insert(key);
        }
        let (made_key, picture) = library.nearest_made(key)?;
        let texture_key = ((source, made_key), colour.bit_pattern());
        self.thumbnails_used_this_frame.insert(texture_key);
        if let Entry::Vacant(vacancy) = self.thumbnails.entry(texture_key) {
            let frame = picture.decompress().ok()?;
            vacancy.insert(context.load_texture(
                format!(
                    "thumbnail {} of level {} of file {source}",
                    made_key.index, made_key.level
                ),
                color_image_of(&frame, colour),
                TextureOptions::LINEAR,
            ));
        }
        self.thumbnails.get(&texture_key)
    }

    /// The finer thumbnails that were wanted since the last call and had not been made, for each video.
    pub fn take_finer_thumbnails_wanted(&mut self) -> BTreeMap<SourceMediaIdentifier, BTreeSet<ThumbnailKey>> {
        std::mem::take(&mut self.finer_thumbnails_wanted)
    }

    /// Lets go of the thumbnails that were not used since the last call, so that only the ones
    /// on screen take up space on the graphics card. Call once at the end of drawing.
    pub fn forget_thumbnails_not_used_this_frame(&mut self) {
        let used = std::mem::take(&mut self.thumbnails_used_this_frame);
        self.thumbnails.retain(|key, _| used.contains(key));
    }

    /// The texture of the fast-forward symbol for `speed_factor`, drawn `height_in_pixels` tall.
    pub fn symbol(&mut self, context: &Context, speed_factor: SpeedFactor, height_in_pixels: u32) -> &TextureHandle {
        let key = (speed_factor.value(), height_in_pixels);
        if self.symbols.len() >= MOST_SYMBOL_PICTURES_KEPT && !self.symbols.contains_key(&key) {
            self.symbols.clear();
        }
        self.symbols.entry(key).or_insert_with(|| {
            let picture = render_speed_indicator(speed_factor, height_in_pixels);
            let image = ColorImage::from_rgba_unmultiplied(
                [picture.width_in_pixels as usize, picture.height_in_pixels as usize],
                &picture.rgba_pixels,
            );
            let name = format!("fast-forward symbol {}x", speed_factor.value());
            context.load_texture(name, image, TextureOptions::LINEAR)
        })
    }

    /// The texture of a text object's picture, with its colours changed as `colour` says and put on the screen the way `filter`
    /// says. `make_picture` is only called (and may take a while) when there is no picture for `key` yet, and may decline to make
    /// one (`None`), for example while the fonts are still loading.
    pub fn text_picture(
        &mut self,
        context: &Context,
        key: TextAppearance,
        filter: PictureFilter,
        colour: ColourMatrix,
        make_picture: impl FnOnce() -> Option<RgbaPicture>,
    ) -> Option<&TextureHandle> {
        let key = (key, filter, colour.bit_pattern());
        if !self.text_pictures.contains_key(&key) {
            let picture = make_picture()?;
            if self.text_pictures.len() >= MOST_TEXT_PICTURES_KEPT {
                self.text_pictures.clear();
            }
            let image = color_image_of_pixels(
                [picture.width_in_pixels as usize, picture.height_in_pixels as usize],
                &picture.rgba_pixels,
                colour,
            );
            self.text_pictures.insert(
                key.clone(),
                context.load_texture("text object", image, filter.options()),
            );
        }
        self.text_pictures.get(&key)
    }

    /// The texture of the picture of an image file for the preview, with its colours changed as `colour` says, uploading it first
    /// if it has not been, or if it is to be put on the screen another way than it was.
    pub fn image_picture(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        picture: &RgbaPicture,
        filter: PictureFilter,
        colour: ColourMatrix,
    ) -> &TextureHandle {
        self.image_texture(context, source, picture, Some((filter, colour)))
    }

    /// The texture of the picture of an image file for the timeline, which is the one of the preview, whichever way that is put on
    /// the screen and whatever its colours were changed to (it is drawn a lot smaller than it is, which looks the same either way,
    /// and the clip is seen on the timeline with the colours that its own picture has), uploading it first if it has not been.
    pub fn image_picture_for_the_timeline(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        picture: &RgbaPicture,
    ) -> &TextureHandle {
        self.image_texture(context, source, picture, None)
    }

    /// The texture of the picture of an image file, put on the screen the way `how` says (how it is put on the screen, and how its
    /// colours are changed), or the way it is already if that is `None` (and blended, with its colours as they are, if it is not
    /// made yet).
    fn image_texture(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        picture: &RgbaPicture,
        how: Option<(PictureFilter, ColourMatrix)>,
    ) -> &TextureHandle {
        let image = |colour: ColourMatrix| {
            color_image_of_pixels(
                [picture.width_in_pixels as usize, picture.height_in_pixels as usize],
                &picture.rgba_pixels,
                colour,
            )
        };
        let (filter, colour) = how.unwrap_or((PictureFilter::Smooth, ColourMatrix::NOTHING_CHANGES));
        let texture = self.image_pictures.entry(source).or_insert_with(|| ImageTexture {
            handle: context.load_texture(format!("picture of file {source}"), image(colour), filter.options()),
            filter,
            colour,
        });
        if let Some((filter, colour)) = how
            && (texture.filter != filter || texture.colour != colour)
        {
            texture.handle.set(image(colour), filter.options());
            texture.filter = filter;
            texture.colour = colour;
        }
        &texture.handle
    }

    /// The texture of the grain of the racks of keyframes, uploading it first if it has not been. It is repeated across what it
    /// is put on, as small as it is, point for point.
    pub fn rack_grain(&mut self, context: &Context) -> &TextureHandle {
        self.rack_grain.get_or_insert_with(|| {
            context.load_texture(
                "grain of the racks of keyframes",
                grain_image(),
                TextureOptions {
                    wrap_mode: TextureWrapMode::Repeat,
                    ..TextureOptions::NEAREST
                },
            )
        })
    }

    /// Forgets everything (when a different project is chosen).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// How the preview picture of the video on `track` is put on the screen, if it has been uploaded.
    #[cfg(test)]
    pub fn filter_of_the_preview_picture(&self, track: TrackId) -> Option<PictureFilter> {
        self.preview_pictures.get(&track).map(|texture| texture.filter)
    }

    /// How the picture of the image file `source` is put on the screen, if it has been uploaded.
    #[cfg(test)]
    pub fn filter_of_the_picture_of_the_file(&self, source: SourceMediaIdentifier) -> Option<PictureFilter> {
        self.image_pictures.get(&source).map(|texture| texture.filter)
    }

    /// How the colours of the preview picture of the video on `track` were changed before it was uploaded, if it has been.
    #[cfg(test)]
    pub fn colours_of_the_preview_picture(&self, track: TrackId) -> Option<ColourMatrix> {
        self.preview_pictures.get(&track).map(|texture| texture.colour)
    }

    /// How the colours of the picture of the image file `source` were changed before it was uploaded, if it has been.
    #[cfg(test)]
    pub fn colours_of_the_picture_of_the_file(&self, source: SourceMediaIdentifier) -> Option<ColourMatrix> {
        self.image_pictures.get(&source).map(|texture| texture.colour)
    }

    /// How each of the pictures of text was put on the screen and had its colours changed before it was uploaded (as
    /// [`ColourMatrix::bit_pattern`]), in no order.
    #[cfg(test)]
    pub fn ways_the_pictures_of_text_were_made(&self) -> Vec<(PictureFilter, [u32; 12])> {
        self.text_pictures
            .keys()
            .map(|(_, filter, colour)| (*filter, *colour))
            .collect()
    }

    /// How the colours of each of the thumbnails on the graphics card were changed before it was uploaded (as
    /// [`ColourMatrix::bit_pattern`]), in no order.
    #[cfg(test)]
    pub fn ways_the_colours_of_the_thumbnails_were_changed(&self) -> Vec<[u32; 12]> {
        self.thumbnails.keys().map(|(_, colour)| *colour).collect()
    }
}

/// The picture of `frame` with its colours changed as `colour` says.
fn color_image_of(frame: &DecodedFrame, colour: ColourMatrix) -> ColorImage {
    color_image_of_pixels(
        [frame.width_in_pixels as usize, frame.height_in_pixels as usize],
        &frame.rgba_pixels,
        colour,
    )
}

/// The picture of `size` (across and down) pixels that `rgba_pixels` is, with its colours changed as `colour` says.
fn color_image_of_pixels(size: [usize; 2], rgba_pixels: &[u8], colour: ColourMatrix) -> ColorImage {
    if colour.changes_nothing() {
        return ColorImage::from_rgba_unmultiplied(size, rgba_pixels);
    }
    let mut changed = rgba_pixels.to_vec();
    colour.apply_to_rgba_pixels(&mut changed);
    ColorImage::from_rgba_unmultiplied(size, &changed)
}

#[cfg(test)]
mod tests;
