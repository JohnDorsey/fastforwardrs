//! The pictures kept on the graphics card for drawing: the preview picture, the thumbnails that
//! are on screen, the fast-forward symbol and the text objects.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

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

/// The picture of the video of a track on the graphics card, and which picture it is.
struct PreviewTexture {
    handle: TextureHandle,
    serial_number: u64,
}

/// A picture on the graphics card, with what it is a picture of.
#[derive(Default)]
pub struct Textures {
    preview_pictures: HashMap<TrackId, PreviewTexture>,
    thumbnails: HashMap<ThumbnailTextureKey, TextureHandle>,
    thumbnails_used_this_frame: HashSet<ThumbnailTextureKey>,
    /// The finer pictures that were wanted since they were last taken, and were not there.
    finer_thumbnails_wanted: BTreeMap<SourceMediaIdentifier, BTreeSet<ThumbnailKey>>,
    symbols: HashMap<(u32, u32), TextureHandle>,
    text_pictures: HashMap<TextAppearance, TextureHandle>,
    image_pictures: HashMap<SourceMediaIdentifier, TextureHandle>,
}

impl Textures {
    /// The texture of the preview picture of the video on `track`, uploading it first if it is a new one.
    pub fn preview_picture(&mut self, context: &Context, track: TrackId, picture: &SuppliedPicture) -> &TextureHandle {
        match self.preview_pictures.entry(track) {
            Entry::Occupied(mut occupied) => {
                let texture = occupied.get_mut();
                if texture.serial_number != picture.serial_number {
                    texture
                        .handle
                        .set(color_image_of(&picture.frame), TextureOptions::LINEAR);
                    texture.serial_number = picture.serial_number;
                }
            }
            Entry::Vacant(vacancy) => {
                vacancy.insert(PreviewTexture {
                    handle: context.load_texture(
                        format!("preview picture of {track}"),
                        color_image_of(&picture.frame),
                        TextureOptions::LINEAR,
                    ),
                    serial_number: picture.serial_number,
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
        if key.level > 0 && library.get(key).is_none() {
            self.finer_thumbnails_wanted.entry(source).or_default().insert(key);
        }
        let (made_key, picture) = library.nearest_made(key)?;
        let texture_key = (source, made_key);
        self.thumbnails_used_this_frame.insert(texture_key);
        if let Entry::Vacant(vacancy) = self.thumbnails.entry(texture_key) {
            let frame = picture.decompress().ok()?;
            vacancy.insert(context.load_texture(
                format!(
                    "thumbnail {} of level {} of file {source}",
                    made_key.index, made_key.level
                ),
                color_image_of(&frame),
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

    /// The texture of a text object's picture. `make_picture` is only called (and may take a
    /// while) when there is no picture for `key` yet, and may decline to make one (`None`), for
    /// example while the fonts are still loading.
    pub fn text_picture(
        &mut self,
        context: &Context,
        key: TextAppearance,
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
            self.text_pictures.insert(
                key.clone(),
                context.load_texture("text object", image, TextureOptions::LINEAR),
            );
        }
        self.text_pictures.get(&key)
    }

    /// The texture of the picture of an image file, uploading it first if it has not been.
    pub fn image_picture(
        &mut self,
        context: &Context,
        source: SourceMediaIdentifier,
        picture: &RgbaPicture,
    ) -> &TextureHandle {
        self.image_pictures.entry(source).or_insert_with(|| {
            let image = ColorImage::from_rgba_unmultiplied(
                [picture.width_in_pixels as usize, picture.height_in_pixels as usize],
                &picture.rgba_pixels,
            );
            context.load_texture(format!("picture of file {source}"), image, TextureOptions::LINEAR)
        })
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
    use crate::project::{Angle, CanvasArea, FontSize, TextColour, TextObject};

    fn object() -> TextObject {
        TextObject {
            text: "Hello".to_owned(),
            font_family: String::new(),
            font_size: FontSize::DEFAULT,
            colour: TextColour::WHITE,
            area: CanvasArea::new(0.1, 0.2, 0.5, 0.25),
            rotation: Angle::ZERO,
        }
    }

    fn key(object: &TextObject) -> TextAppearance {
        TextAppearance::of(object, 1920, 1080)
    }

    fn picture_of(shade: u8) -> crate::media::CompressedPicture {
        crate::media::CompressedPicture::from_rgb_pixels(
            &[shade; 8 * 8 * 3],
            crate::media::PictureSize {
                width_in_pixels: 8,
                height_in_pixels: 8,
            },
        )
        .unwrap()
    }

    const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

    fn thumbnail_key(level: u8, index: usize) -> ThumbnailKey {
        ThumbnailKey { level, index }
    }

    #[test]
    fn a_thumbnail_that_has_been_made_is_given_and_nothing_is_wanted() {
        let context = Context::default();
        let mut textures = Textures::default();
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(thumbnail_key(2, 9), picture_of(40));

        assert!(
            textures
                .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 9))
                .is_some()
        );

        assert!(textures.take_finer_thumbnails_wanted().is_empty());
    }

    #[test]
    fn a_finer_thumbnail_that_is_not_made_gives_the_coarser_one_of_about_the_same_moment_and_is_wanted() {
        let context = Context::default();
        let mut textures = Textures::default();
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(thumbnail_key(0, 2), picture_of(40)); // 2.0 s

        // 2.25 s at four a second: there is the picture of 2 s, which is shown until the finer one is made.
        assert!(
            textures
                .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 9))
                .is_some()
        );

        let wanted = textures.take_finer_thumbnails_wanted();
        assert_eq!(wanted.len(), 1);
        assert_eq!(wanted[&VIDEO], BTreeSet::from([thumbnail_key(2, 9)]));
        assert!(
            textures.take_finer_thumbnails_wanted().is_empty(),
            "taken: it is not handed over twice"
        );
    }

    #[test]
    fn a_thumbnail_of_a_second_that_is_not_made_yet_is_not_wanted_because_all_of_them_are_being_made() {
        let context = Context::default();
        let mut textures = Textures::default();
        let library = ThumbnailLibrary::with_room_for(10);

        assert!(
            textures
                .thumbnail(&context, VIDEO, &library, thumbnail_key(0, 3))
                .is_none()
        );

        assert!(textures.take_finer_thumbnails_wanted().is_empty());
    }

    #[test]
    fn nothing_at_all_made_gives_no_texture_but_the_finer_picture_is_wanted() {
        let context = Context::default();
        let mut textures = Textures::default();
        let library = ThumbnailLibrary::with_room_for(10);

        assert!(
            textures
                .thumbnail(&context, VIDEO, &library, thumbnail_key(3, 17))
                .is_none()
        );

        assert_eq!(
            textures.take_finer_thumbnails_wanted()[&VIDEO],
            BTreeSet::from([thumbnail_key(3, 17)])
        );
    }

    #[test]
    fn the_same_coarse_picture_is_one_texture_however_many_finer_ones_stand_in_for_it() {
        let context = Context::default();
        let mut textures = Textures::default();
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(thumbnail_key(0, 2), picture_of(40));

        let first = textures
            .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 8))
            .unwrap()
            .id();
        let second = textures
            .thumbnail(&context, VIDEO, &library, thumbnail_key(2, 9))
            .unwrap()
            .id();

        assert_eq!(first, second);
    }

    fn supplied_picture(serial_number: u64, shade: u8) -> SuppliedPicture {
        SuppliedPicture {
            source: VIDEO,
            source_frame: crate::project::SourceFrameIndex::new(0),
            frame: DecodedFrame {
                width_in_pixels: 1,
                height_in_pixels: 1,
                rgba_pixels: vec![shade, 0, 0, 255],
            },
            serial_number,
        }
    }

    #[test]
    fn the_pictures_of_different_tracks_are_different_textures_and_a_track_keeps_its_own_as_it_changes() {
        let context = Context::default();
        let mut textures = Textures::default();
        let (track_a, track_b) = (TrackId::new(0), TrackId::new(1));

        let a = textures
            .preview_picture(&context, track_a, &supplied_picture(1, 10))
            .id();
        let b = textures
            .preview_picture(&context, track_b, &supplied_picture(2, 20))
            .id();
        assert_ne!(a, b);

        let a_again = textures
            .preview_picture(&context, track_a, &supplied_picture(1, 10))
            .id();
        assert_eq!(a, a_again, "the same picture is not uploaded again");
        let a_next = textures
            .preview_picture(&context, track_a, &supplied_picture(3, 30))
            .id();
        assert_eq!(a, a_next, "a new picture of the track is put in the track's texture");
        assert_eq!(
            textures
                .preview_picture(&context, track_b, &supplied_picture(2, 20))
                .id(),
            b,
            "and the other track's is not touched"
        );
    }

    #[test]
    fn moving_a_text_object_does_not_change_its_picture() {
        let mut moved = object();
        moved.area = moved.area.moved_by(0.1, 0.2);
        assert_eq!(key(&object()), key(&moved));
    }

    /// The picture is of the text upright in its area; turning it is done when it is drawn, so
    /// turning a text object round and round makes no new pictures.
    #[test]
    fn the_picture_of_an_image_file_is_uploaded_once_and_each_file_has_its_own() {
        let context = Context::default();
        let mut textures = Textures::default();
        let red = RgbaPicture {
            width_in_pixels: 1,
            height_in_pixels: 1,
            rgba_pixels: vec![255, 0, 0, 255],
        };
        let blue = RgbaPicture {
            rgba_pixels: vec![0, 0, 255, 255],
            ..red.clone()
        };
        let first = textures
            .image_picture(&context, SourceMediaIdentifier::new(1), &red)
            .id();
        let second = textures
            .image_picture(&context, SourceMediaIdentifier::new(2), &blue)
            .id();
        assert_ne!(first, second);
        assert_eq!(
            textures
                .image_picture(&context, SourceMediaIdentifier::new(1), &red)
                .id(),
            first,
            "the same file is the same texture"
        );
        textures.clear();
        assert_ne!(
            textures
                .image_picture(&context, SourceMediaIdentifier::new(1), &red)
                .id(),
            first,
            "forgotten when everything is"
        );
    }

    #[test]
    fn turning_a_text_object_does_not_change_its_picture() {
        let mut turned = object();
        turned.rotation = Angle::from_degrees(37.0);
        assert_eq!(key(&object()), key(&turned));
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
            TextAppearance::of(&object(), 1280, 720),
            "how big the finished picture is"
        );
    }
}
