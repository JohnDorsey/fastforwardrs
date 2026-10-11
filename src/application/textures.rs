//! The pictures kept on the graphics card for drawing: the preview picture, the thumbnails that
//! are on screen, the fast-forward symbol and the text objects.

use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::media::{DecodedFrame, ThumbnailStrip};
use crate::playback::SuppliedPicture;
use crate::project::{SourceMediaIdentifier, SpeedFactor, TextObject};
use crate::rgba_picture::RgbaPicture;
use crate::speed_indicator::render_speed_indicator;

/// How many symbol pictures (each speed at each size is a different one) are kept before the
/// ones no longer needed are thrown away. Dragging the symbol's size makes many of them.
const MOST_SYMBOL_PICTURES_KEPT: usize = 24;

/// How many pictures of text are kept before the ones no longer needed are thrown away. (Resizing a
/// text object makes a new picture for every size it passes through.)
const MOST_TEXT_PICTURES_KEPT: usize = 24;

/// Which thumbnail: the picture for a second of a video.
type ThumbnailKey = (SourceMediaIdentifier, usize);

/// What a picture of a text object depends on, so that it is only made again when one of these changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextPictureKey(u64);

impl TextPictureKey {
    /// The key of the picture of `object` on a finished picture that is `picture_width_in_pixels`
    /// by `picture_height_in_pixels`. Where the object is on the picture does not matter, only how
    /// big its area is, so moving it does not make the picture again.
    pub fn of(object: &TextObject, picture_width_in_pixels: u32, picture_height_in_pixels: u32) -> Self {
        let mut hasher = DefaultHasher::new();
        object.text.hash(&mut hasher);
        object.font_family.hash(&mut hasher);
        object
            .font_size
            .fraction_of_picture_height()
            .to_bits()
            .hash(&mut hasher);
        object.colour.hash(&mut hasher);
        crate::text_rendering::area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels)
            .hash(&mut hasher);
        picture_height_in_pixels.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// A picture on the graphics card, with what it is a picture of.
#[derive(Default)]
pub struct Textures {
    preview_picture: Option<TextureHandle>,
    preview_picture_serial_number: u64,
    thumbnails: HashMap<ThumbnailKey, TextureHandle>,
    thumbnails_used_this_frame: HashSet<ThumbnailKey>,
    symbols: HashMap<(u32, u32), TextureHandle>,
    text_pictures: HashMap<TextPictureKey, TextureHandle>,
}

impl Textures {
    /// The texture of the preview picture, uploading it first if it is a new one.
    pub fn preview_picture(&mut self, context: &Context, picture: &SuppliedPicture) -> &TextureHandle {
        let up_to_date = self.preview_picture.is_some() && self.preview_picture_serial_number == picture.serial_number;
        if !up_to_date {
            let image = color_image_of(&picture.frame);
            match self.preview_picture.as_mut() {
                Some(handle) => handle.set(image, TextureOptions::LINEAR),
                None => {
                    self.preview_picture = Some(context.load_texture("preview picture", image, TextureOptions::LINEAR))
                }
            }
            self.preview_picture_serial_number = picture.serial_number;
        }
        self.preview_picture
            .as_ref()
            .expect("the preview picture was just made")
    }

    /// The texture of the thumbnail of the video `source` for `second`, if it has been made.
    /// Unpacks it on first use.
    pub fn thumbnail(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        strip: &ThumbnailStrip,
        second: usize,
    ) -> Option<&TextureHandle> {
        let key = (source, second);
        self.thumbnails_used_this_frame.insert(key);
        if let Entry::Vacant(vacancy) = self.thumbnails.entry(key) {
            let frame = strip.picture_for_second(second)?.decompress().ok()?;
            vacancy.insert(context.load_texture(
                format!("thumbnail {second} of file {source}"),
                color_image_of(&frame),
                TextureOptions::LINEAR,
            ));
        }
        self.thumbnails.get(&key)
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

    /// The texture of a text object's picture. `make_picture` is only called (and may take a
    /// while) when there is no picture for `key` yet, and may decline to make one (`None`), for
    /// example while the fonts are still loading.
    pub fn text_picture(
        &mut self,
        context: &Context,
        key: TextPictureKey,
        make_picture: impl FnOnce() -> Option<RgbaPicture>,
    ) -> Option<&TextureHandle> {
        if !self.text_pictures.contains_key(&key) {
            let picture = make_picture()?;
            if self.text_pictures.len() >= MOST_TEXT_PICTURES_KEPT {
                self.text_pictures.clear();
            }
            let image = ColorImage::from_rgba_unmultiplied(
                [picture.width_in_pixels as usize, picture.height_in_pixels as usize],
                &picture.rgba_pixels,
            );
            self.text_pictures
                .insert(key, context.load_texture("text object", image, TextureOptions::LINEAR));
        }
        self.text_pictures.get(&key)
    }

    /// Forgets everything (when a different project is chosen).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn color_image_of(frame: &DecodedFrame) -> ColorImage {
    ColorImage::from_rgba_unmultiplied(
        [frame.width_in_pixels as usize, frame.height_in_pixels as usize],
        &frame.rgba_pixels,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{CanvasArea, FontSize, TextColour};

    fn object() -> TextObject {
        TextObject {
            text: "Hello".to_owned(),
            font_family: String::new(),
            font_size: FontSize::DEFAULT,
            colour: TextColour::WHITE,
            area: CanvasArea::new(0.1, 0.2, 0.5, 0.25),
        }
    }

    fn key(object: &TextObject) -> TextPictureKey {
        TextPictureKey::of(object, 1920, 1080)
    }

    #[test]
    fn moving_a_text_object_does_not_change_its_picture() {
        let mut moved = object();
        moved.area = moved.area.moved_by(0.1, 0.2);
        assert_eq!(key(&object()), key(&moved));
    }

    #[test]
    fn anything_that_changes_how_the_text_looks_changes_its_picture() {
        let base = key(&object());
        let mut changed = object();
        changed.text = "Hello!".to_owned();
        assert_ne!(base, key(&changed), "the words");
        let mut changed = object();
        changed.font_family = "Arial".to_owned();
        assert_ne!(base, key(&changed), "the font");
        let mut changed = object();
        changed.font_size = FontSize::from_points(80.0);
        assert_ne!(base, key(&changed), "the size of the letters");
        let mut changed = object();
        changed.colour = TextColour::new(1, 2, 3, 4);
        assert_ne!(base, key(&changed), "the colour");
        let mut changed = object();
        changed.area = changed
            .area
            .with_corner_dragged_to(crate::project::Corner::BottomRight, 0.9, 0.9);
        assert_ne!(base, key(&changed), "how big the area is");
        assert_ne!(
            base,
            TextPictureKey::of(&object(), 1280, 720),
            "how big the finished picture is"
        );
    }
}
