//! Text that is shown over the picture for a while: what it says, how it looks, and where it is
//! (an area of the picture, and how far that is turned: see [`super::placement`]).
//!
//! Like the fast-forward symbol, everything about where a text object is and how big it is is a
//! fraction of the picture rather than a number of pixels, so that it lands in the same place
//! whatever size the picture is shown or exported at.

use thiserror::Error;

use super::placement::{Angle, CanvasArea, Placement};

/// A colour with transparency, as the picture's pixels have them: straight (not premultiplied).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextColour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl TextColour {
    pub const WHITE: TextColour = TextColour {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    };

    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

/// How big the letters are, as a fraction of the height of the picture. It is shown (and chosen) as
/// the number of points it would be on a picture that is [`FontSize::REFERENCE_PICTURE_HEIGHT`]
/// tall, which is what people expect a font size to look like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSize {
    fraction_of_picture_height: f32,
}

impl FontSize {
    /// The height of the picture that a font size in points is measured on.
    pub const REFERENCE_PICTURE_HEIGHT: f32 = 1080.0;

    pub const SMALLEST_FRACTION: f32 = 0.01;
    pub const LARGEST_FRACTION: f32 = 0.5;

    /// A size that is `fraction_of_picture_height` of the picture's height. Out-of-range values
    /// (or values that are not numbers) are corrected rather than rejected, so that a hand-edited
    /// project file can never break the editor.
    pub fn from_fraction_of_picture_height(fraction_of_picture_height: f32) -> Self {
        let fraction = if fraction_of_picture_height.is_nan() {
            Self::DEFAULT.fraction_of_picture_height
        } else {
            fraction_of_picture_height.clamp(Self::SMALLEST_FRACTION, Self::LARGEST_FRACTION)
        };
        Self {
            fraction_of_picture_height: fraction,
        }
    }

    /// The size a font of `points` points has on a picture [`Self::REFERENCE_PICTURE_HEIGHT`] tall.
    pub fn from_points(points: f32) -> Self {
        Self::from_fraction_of_picture_height(points / Self::REFERENCE_PICTURE_HEIGHT)
    }

    /// A little under a twelfth of the picture's height: easy to read as a caption.
    pub const DEFAULT: FontSize = FontSize {
        fraction_of_picture_height: 0.06,
    };

    pub fn fraction_of_picture_height(self) -> f32 {
        self.fraction_of_picture_height
    }

    /// The size in points, as it is shown to the user.
    pub fn points(self) -> f32 {
        self.fraction_of_picture_height * Self::REFERENCE_PICTURE_HEIGHT
    }

    /// How tall the letters are, in pixels, on a picture `picture_height_in_pixels` tall.
    pub fn in_pixels(self, picture_height_in_pixels: u32) -> f32 {
        self.fraction_of_picture_height * picture_height_in_pixels as f32
    }
}

impl Default for FontSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Text, shown over the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct TextObject {
    /// What it says. It is wrapped to fit the width of [`Self::area`].
    pub text: String,
    /// The name of the typeface, such as `Arial`. Empty for the program's own default.
    pub font_family: String,
    pub font_size: FontSize,
    pub colour: TextColour,
    /// Where on the picture the text goes, before it is turned. What does not fit in it is cut off.
    pub area: CanvasArea,
    /// How far the area, and the text in it, is turned about the middle of the area.
    pub rotation: Angle,
}

impl TextObject {
    /// What a new text object says, until it is changed.
    pub const NEW_TEXT: &'static str = "Text";

    /// Where the text is on the picture: its area, and how far that is turned.
    pub fn placement(&self) -> Placement {
        Placement::new(self.area, self.rotation)
    }

    /// The same text, looking the same, put at `placement`.
    #[must_use]
    pub fn with_placement(&self, placement: Placement) -> Self {
        Self {
            area: placement.area,
            rotation: placement.rotation,
            ..self.clone()
        }
    }
}

impl Default for TextObject {
    /// A new text object: a short placeholder, white, in the default font, along the bottom, upright.
    fn default() -> Self {
        Self {
            text: Self::NEW_TEXT.to_owned(),
            font_family: String::new(),
            font_size: FontSize::DEFAULT,
            colour: TextColour::WHITE,
            area: CanvasArea::DEFAULT,
            rotation: Angle::ZERO,
        }
    }
}

/// Returned when a clip would last no time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a clip must last at least one frame")]
pub struct EmptyTextClip;

/// A text object on a track, and for how long it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct TextClip {
    pub object: TextObject,
    duration_in_frames: u64,
}

impl TextClip {
    pub fn new(object: TextObject, duration_in_frames: u64) -> Result<Self, EmptyTextClip> {
        if duration_in_frames == 0 {
            return Err(EmptyTextClip);
        }
        Ok(Self {
            object,
            duration_in_frames,
        })
    }

    /// How many frames of the timeline the text is shown for. A text object has no speed: this is
    /// exactly how long it lasts whatever the speed of its track.
    pub fn duration_in_frames(&self) -> u64 {
        self.duration_in_frames
    }

    /// The same text for a different number of frames, or `None` if that is none at all.
    #[must_use]
    pub fn with_duration_in_frames(&self, duration_in_frames: u64) -> Option<Self> {
        Self::new(self.object.clone(), duration_in_frames).ok()
    }

    /// Cuts the clip in two after its first `frames_in` frames, or `None` if that is not strictly
    /// inside it. Both halves say and look the same.
    pub fn split_after(&self, frames_in: u64) -> Option<(Self, Self)> {
        if frames_in == 0 || frames_in >= self.duration_in_frames {
            return None;
        }
        Some((
            Self {
                object: self.object.clone(),
                duration_in_frames: frames_in,
            },
            Self {
                object: self.object.clone(),
                duration_in_frames: self.duration_in_frames - frames_in,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} is not {expected}");
    }

    #[test]
    fn the_font_size_is_shown_in_points_on_a_picture_1080_tall() {
        let size = FontSize::from_points(54.0);
        assert_close(size.fraction_of_picture_height(), 0.05);
        assert_close(size.points(), 54.0);
        assert_close(size.in_pixels(1080), 54.0);
        assert_close(size.in_pixels(720), 36.0);
        assert_close(size.in_pixels(2160), 108.0);
    }

    #[test]
    fn a_font_size_outside_what_is_allowed_is_brought_back_inside_and_nothing_is_the_default() {
        assert_eq!(
            FontSize::from_points(100_000.0).fraction_of_picture_height(),
            FontSize::LARGEST_FRACTION
        );
        assert_eq!(
            FontSize::from_points(0.0).fraction_of_picture_height(),
            FontSize::SMALLEST_FRACTION
        );
        assert_eq!(
            FontSize::from_points(-5.0).fraction_of_picture_height(),
            FontSize::SMALLEST_FRACTION
        );
        assert_eq!(FontSize::from_points(f32::NAN), FontSize::DEFAULT);
        assert_eq!(FontSize::default(), FontSize::DEFAULT);
    }

    #[test]
    fn a_new_text_object_says_text_in_white_along_the_bottom_in_the_default_font() {
        let object = TextObject::default();
        assert_eq!(object.text, "Text");
        assert_eq!(object.font_family, "");
        assert_eq!(object.colour, TextColour::WHITE);
        assert_eq!(object.area, CanvasArea::DEFAULT);
        assert_eq!(object.font_size, FontSize::DEFAULT);
        assert!(object.area.top() > 0.5, "along the bottom half");
    }

    #[test]
    fn a_new_text_object_is_not_turned() {
        assert_eq!(TextObject::default().rotation, Angle::ZERO);
    }

    #[test]
    fn the_place_of_a_text_object_is_its_area_and_how_far_that_is_turned_and_it_can_be_put_somewhere_else() {
        let object = TextObject {
            text: "Hello".to_owned(),
            area: CanvasArea::new(0.2, 0.3, 0.4, 0.2),
            rotation: Angle::from_degrees(30.0),
            ..TextObject::default()
        };
        assert_eq!(
            object.placement(),
            Placement::new(CanvasArea::new(0.2, 0.3, 0.4, 0.2), Angle::from_degrees(30.0))
        );

        let elsewhere = Placement::new(CanvasArea::new(0.5, 0.5, 0.1, 0.1), Angle::from_degrees(-45.0));
        let moved = object.with_placement(elsewhere);
        assert_eq!(moved.placement(), elsewhere);
        assert_eq!(
            TextObject {
                area: object.area,
                rotation: object.rotation,
                ..moved
            },
            object,
            "only where it is changed, not what it says or how it looks"
        );
    }

    #[test]
    fn a_text_clip_lasts_at_least_one_frame_and_ignores_speed_so_its_duration_is_what_it_says() {
        assert_eq!(TextClip::new(TextObject::default(), 0), Err(EmptyTextClip));
        let clip = TextClip::new(TextObject::default(), 90).unwrap();
        assert_eq!(clip.duration_in_frames(), 90);
        assert_eq!(clip.with_duration_in_frames(10).unwrap().duration_in_frames(), 10);
        assert_eq!(clip.with_duration_in_frames(0), None);
    }

    #[test]
    fn cutting_a_text_clip_makes_two_that_say_the_same_and_last_as_long_together() {
        let clip = TextClip::new(
            TextObject {
                text: "Hello".to_owned(),
                ..TextObject::default()
            },
            90,
        )
        .unwrap();
        let (first, second) = clip.split_after(30).unwrap();
        assert_eq!((first.duration_in_frames(), second.duration_in_frames()), (30, 60));
        assert_eq!(first.object, clip.object);
        assert_eq!(second.object, clip.object);
        for outside in [0, 90, 91] {
            assert_eq!(clip.split_after(outside), None, "after {outside} frames");
        }
    }
}
