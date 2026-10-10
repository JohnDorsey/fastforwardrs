//! The pictures kept on the graphics card for drawing: the preview picture, the thumbnails that
//! are on screen, and the fast-forward symbol.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::media::{DecodedFrame, ThumbnailStrip};
use crate::playback::SuppliedPicture;
use crate::project::SpeedFactor;
use crate::speed_indicator::render_speed_indicator;

/// How many symbol pictures (each speed at each size is a different one) are kept before the
/// ones no longer needed are thrown away. Dragging the symbol's size makes many of them.
const MOST_SYMBOL_PICTURES_KEPT: usize = 24;

/// A picture on the graphics card, with what it is a picture of.
#[derive(Default)]
pub struct Textures {
    preview_picture: Option<TextureHandle>,
    preview_picture_serial_number: u64,
    thumbnails: HashMap<usize, TextureHandle>,
    thumbnails_used_this_frame: HashSet<usize>,
    symbols: HashMap<(u32, u32), TextureHandle>,
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

    /// The texture of the thumbnail for `second`, if it has been made. Unpacks it on first use.
    pub fn thumbnail(&mut self, context: &Context, strip: &ThumbnailStrip, second: usize) -> Option<&TextureHandle> {
        self.thumbnails_used_this_frame.insert(second);
        if let Entry::Vacant(vacancy) = self.thumbnails.entry(second) {
            let frame = strip.picture_for_second(second)?.decompress().ok()?;
            vacancy.insert(context.load_texture(
                format!("thumbnail {second}"),
                color_image_of(&frame),
                TextureOptions::LINEAR,
            ));
        }
        self.thumbnails.get(&second)
    }

    /// Lets go of the thumbnails that were not used since the last call, so that only the ones
    /// on screen take up space on the graphics card. Call once at the end of drawing.
    pub fn forget_thumbnails_not_used_this_frame(&mut self) {
        let used = std::mem::take(&mut self.thumbnails_used_this_frame);
        self.thumbnails.retain(|second, _| used.contains(second));
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

    /// Forgets everything (when a different video is chosen).
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
