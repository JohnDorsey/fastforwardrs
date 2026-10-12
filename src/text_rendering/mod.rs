//! Making the picture of a text object, for the preview and the export alike.
//!
//! The text is laid out and drawn by the `cosmic-text` library, which knows the fonts of the
//! computer, falls back to another font for the letters a font does not have (accents, other
//! alphabets, emoji), and wraps lines at the edge of the box. The picture is made at the size the
//! finished video's picture has, so that the preview (which only scales it down) and the export
//! (which lays it over the video as it is) show exactly the same thing.
//!
//! Loading the fonts of the computer takes a good fraction of a second, so it is done once, in the
//! background, by [`LazyTextRenderer`].

use std::thread::{self, JoinHandle};

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};

use crate::media::UserInterfaceWakeUp;
use crate::project::{TextColour, TextObject};
use crate::rgba_picture::RgbaPicture;

/// How tall a line of text is, as a multiple of the size of the letters.
const LINE_HEIGHT_PER_FONT_SIZE: f32 = 1.2;

/// Draws text objects. It owns the computer's fonts, which makes it slow to make and not to be
/// shared between threads; see [`LazyTextRenderer`].
pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl TextRenderer {
    /// Loads the fonts of the computer. This takes a good fraction of a second.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    /// The names of the font families the computer has, in alphabetical order, each once.
    pub fn font_family_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .font_system
            .db()
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _language)| name.clone()))
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        names
    }

    /// The picture of `object`'s own area, on a finished picture that is `picture_width_in_pixels`
    /// by `picture_height_in_pixels`: the text, wrapped inside the area and starting at its top left
    /// corner, on a transparent background.
    pub fn render(
        &mut self,
        object: &TextObject,
        picture_width_in_pixels: u32,
        picture_height_in_pixels: u32,
    ) -> RgbaPicture {
        let (width_in_pixels, height_in_pixels) =
            area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels);
        let mut picture = RgbaPicture {
            width_in_pixels,
            height_in_pixels,
            rgba_pixels: vec![0; width_in_pixels as usize * height_in_pixels as usize * 4],
        };
        if object.text.is_empty() {
            return picture;
        }

        let letter_size = object.font_size.in_pixels(picture_height_in_pixels).max(1.0);
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(letter_size, letter_size * LINE_HEIGHT_PER_FONT_SIZE),
        );
        buffer.set_size(Some(width_in_pixels as f32), Some(height_in_pixels as f32));
        buffer.set_wrap(Wrap::WordOrGlyph);
        let family = if object.font_family.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(&object.font_family)
        };
        buffer.set_text(&object.text, &Attrs::new().family(family), Shaping::Advanced, None);

        let colour = object.colour;
        let text_colour = Color::rgba(colour.red, colour.green, colour.blue, colour.alpha);
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_colour,
            |left, top, width, height, covering_colour| {
                // cosmic-text gives how much of the pixel a letter covers as the opacity of the
                // colour, and leaves out the opacity of the text's own colour, so it is put in here.
                let opacity = opacity_of_covering(covering_colour.a(), colour.alpha);
                let rows = top.max(0)..(top + height as i32).min(height_in_pixels as i32);
                let columns = left.max(0)..(left + width as i32).min(width_in_pixels as i32);
                for row in rows {
                    for column in columns.clone() {
                        let first_byte = (row as usize * width_in_pixels as usize + column as usize) * 4;
                        let pixel: &mut [u8; 4] = (&mut picture.rgba_pixels[first_byte..first_byte + 4])
                            .try_into()
                            .expect("a pixel is four bytes");
                        lay_over(
                            pixel,
                            [covering_colour.r(), covering_colour.g(), covering_colour.b()],
                            opacity,
                        );
                    }
                }
            },
        );
        picture
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything that the picture [`TextRenderer::render`] makes of a text object depends on: two text objects on finished
/// pictures of the same size have the same appearance exactly when their pictures are the same. Where the object is on the
/// picture and how far it is turned are not part of it (the picture is of the text upright in its own area, and is
/// moved and turned when it is laid over the picture), only how big its area is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextAppearance {
    text: String,
    font_family: String,
    /// The bits of the size of the letters as a fraction of the picture's height: a number with a fraction cannot be hashed.
    font_size_bits: u32,
    colour: TextColour,
    area_size_in_pixels: (u32, u32),
    picture_height_in_pixels: u32,
}

impl TextAppearance {
    /// The appearance of `object` on a finished picture that is `picture_width_in_pixels` by `picture_height_in_pixels`.
    pub fn of(object: &TextObject, picture_width_in_pixels: u32, picture_height_in_pixels: u32) -> Self {
        Self {
            text: object.text.clone(),
            font_family: object.font_family.clone(),
            font_size_bits: object.font_size.fraction_of_picture_height().to_bits(),
            colour: object.colour,
            area_size_in_pixels: area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels),
            picture_height_in_pixels,
        }
    }
}

/// How big the picture of `object`'s area is, in pixels, on a finished picture of the given size.
/// (Never empty.)
pub fn area_size_in_pixels(
    object: &TextObject,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> (u32, u32) {
    let pixels = object
        .area
        .in_pixels((picture_width_in_pixels, picture_height_in_pixels));
    (pixels.width, pixels.height)
}

/// How opaque a bit of a letter is when `covered` of the pixel is covered by the letter (0 to 255)
/// and the text's own colour is `colour_opacity` opaque (0 to 255).
fn opacity_of_covering(covered: u8, colour_opacity: u8) -> u8 {
    u8::try_from((u32::from(covered) * u32::from(colour_opacity) + 127) / 255).unwrap_or(u8::MAX)
}

/// Lays a colour with some opacity over a pixel whose colour is not premultiplied by its opacity
/// (so that a pixel that nothing was drawn on stays transparent, and a pixel drawn on with a
/// colour is that colour, however faintly).
fn lay_over(pixel: &mut [u8; 4], colour: [u8; 3], opacity: u8) {
    if opacity == 0 {
        return;
    }
    let over = f32::from(opacity) / 255.0;
    let under = f32::from(pixel[3]) / 255.0 * (1.0 - over);
    let combined = over + under;
    for (channel, new_value) in pixel.iter_mut().zip(colour) {
        let mixed = (f32::from(new_value) * over + f32::from(*channel) * under) / combined;
        *channel = mixed.round().clamp(0.0, 255.0) as u8;
    }
    pixel[3] = (combined * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// A [`TextRenderer`] that is made in the background the first time it is wanted.
pub struct LazyTextRenderer {
    wake_up: UserInterfaceWakeUp,
    state: LoadingState,
}

enum LoadingState {
    NotStarted,
    Loading(JoinHandle<TextRenderer>),
    Ready(Box<TextRenderer>),
    /// The thread that loads the fonts stopped unexpectedly. There is no text, then.
    Failed,
}

impl LazyTextRenderer {
    /// A renderer that has not started loading. `wake_up` is called when it has finished, so that
    /// the window draws the text then.
    pub fn new(wake_up: UserInterfaceWakeUp) -> Self {
        Self {
            wake_up,
            state: LoadingState::NotStarted,
        }
    }

    /// The renderer, if it is ready. The first call starts loading it.
    pub fn get_if_ready(&mut self) -> Option<&mut TextRenderer> {
        if matches!(self.state, LoadingState::NotStarted) {
            let wake_up = std::sync::Arc::clone(&self.wake_up);
            self.state = LoadingState::Loading(thread::spawn(move || {
                let renderer = TextRenderer::new();
                wake_up();
                renderer
            }));
        }
        if matches!(&self.state, LoadingState::Loading(handle) if handle.is_finished()) {
            self.finish_loading();
        }
        match &mut self.state {
            LoadingState::Ready(renderer) => Some(renderer),
            _ => None,
        }
    }

    /// The renderer, waiting for it to be loaded if need be. `None` only if loading failed.
    pub fn get_waiting_if_needed(&mut self) -> Option<&mut TextRenderer> {
        self.get_if_ready();
        if matches!(self.state, LoadingState::Loading(_)) {
            self.finish_loading();
        }
        match &mut self.state {
            LoadingState::Ready(renderer) => Some(renderer),
            _ => None,
        }
    }

    /// Whether the renderer is ready, without starting to load it.
    pub fn is_ready(&self) -> bool {
        matches!(self.state, LoadingState::Ready(_))
    }

    /// Whether the fonts are being loaded now: asked for, and not ready yet.
    pub fn is_loading(&self) -> bool {
        matches!(self.state, LoadingState::Loading(_))
    }

    fn finish_loading(&mut self) {
        let LoadingState::Loading(handle) = std::mem::replace(&mut self.state, LoadingState::Failed) else {
            return;
        };
        if let Ok(renderer) = handle.join() {
            self.state = LoadingState::Ready(Box::new(renderer));
        }
    }
}

#[cfg(test)]
mod tests;
