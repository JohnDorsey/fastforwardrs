//! Text that is shown over the picture for a while: what it says, how it looks, and where it is.
//!
//! Like the fast-forward symbol, everything about where a text object is and how big it is is a
//! fraction of the picture rather than a number of pixels, so that it lands in the same place
//! whatever size the picture is shown or exported at.

use thiserror::Error;

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

/// One of the four corners of a [`CanvasArea`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Self::TopLeft, Self::TopRight, Self::BottomLeft, Self::BottomRight];

    /// The corner diagonally across.
    pub fn opposite(self) -> Self {
        match self {
            Self::TopLeft => Self::BottomRight,
            Self::TopRight => Self::BottomLeft,
            Self::BottomLeft => Self::TopRight,
            Self::BottomRight => Self::TopLeft,
        }
    }
}

/// A rectangle on the picture, as fractions of the picture's width and height: `(0, 0)` is the
/// top left corner of the picture and `(1, 1)` the bottom right. It is always inside the picture
/// and never smaller than [`Self::SMALLEST_SIDE_FRACTION`] each way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasArea {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl CanvasArea {
    pub const SMALLEST_SIDE_FRACTION: f32 = 0.02;

    /// Along the bottom of the picture, where captions go.
    pub const DEFAULT: CanvasArea = CanvasArea {
        left: 0.1,
        top: 0.74,
        width: 0.8,
        height: 0.16,
    };

    /// The area with this left edge, top edge, width and height. What does not fit is corrected:
    /// the area is made at least as big as it may be, then not bigger than the picture, then moved
    /// to be inside it. (A value that is not a number counts as the default one.)
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        let width = sanitised(width, Self::SMALLEST_SIDE_FRACTION, 1.0, Self::DEFAULT.width);
        let height = sanitised(height, Self::SMALLEST_SIDE_FRACTION, 1.0, Self::DEFAULT.height);
        let left = sanitised(left, 0.0, 1.0 - width, Self::DEFAULT.left.min(1.0 - width));
        let top = sanitised(top, 0.0, 1.0 - height, Self::DEFAULT.top.min(1.0 - height));
        Self {
            left,
            top,
            width,
            height,
        }
    }

    pub fn left(&self) -> f32 {
        self.left
    }

    pub fn top(&self) -> f32 {
        self.top
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn right(&self) -> f32 {
        self.left + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }

    /// Where `corner` is, as fractions of the picture's width and height.
    pub fn corner_position(&self, corner: Corner) -> (f32, f32) {
        match corner {
            Corner::TopLeft => (self.left, self.top),
            Corner::TopRight => (self.right(), self.top),
            Corner::BottomLeft => (self.left, self.bottom()),
            Corner::BottomRight => (self.right(), self.bottom()),
        }
    }

    /// The same area moved by this many picture widths and heights, as far as the picture allows:
    /// it is stopped at the edges, and keeps its size.
    #[must_use]
    pub fn moved_by(&self, across: f32, down: f32) -> Self {
        let across = if across.is_nan() { 0.0 } else { across };
        let down = if down.is_nan() { 0.0 } else { down };
        Self {
            left: (self.left + across).clamp(0.0, 1.0 - self.width),
            top: (self.top + down).clamp(0.0, 1.0 - self.height),
            ..*self
        }
    }

    /// The area after `corner` is dragged to the place `(x, y)` (as fractions of the picture): the
    /// corner diagonally across stays where it is. The corner cannot go outside the picture or
    /// past the corner that is staying, and the area cannot become smaller than
    /// [`Self::SMALLEST_SIDE_FRACTION`].
    #[must_use]
    pub fn with_corner_dragged_to(&self, corner: Corner, x: f32, y: f32) -> Self {
        let (current_x, current_y) = self.corner_position(corner);
        let x = if x.is_nan() { current_x } else { x };
        let y = if y.is_nan() { current_y } else { y };
        if x == current_x && y == current_y {
            // Not a drag at all. Working the area out again could change it by a rounding error,
            // which would count as an edit every time the window is drawn during a drag that stands still.
            return *self;
        }
        let (fixed_x, fixed_y) = self.corner_position(corner.opposite());
        let (new_left, new_right) = ordered_edges(fixed_x, x, corner_is_on_the_left(corner));
        let (new_top, new_bottom) = ordered_edges(fixed_y, y, corner_is_at_the_top(corner));
        Self::new(new_left, new_top, new_right - new_left, new_bottom - new_top)
    }
}

impl Default for CanvasArea {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn corner_is_on_the_left(corner: Corner) -> bool {
    matches!(corner, Corner::TopLeft | Corner::BottomLeft)
}

fn corner_is_at_the_top(corner: Corner) -> bool {
    matches!(corner, Corner::TopLeft | Corner::TopRight)
}

/// The two edges (smaller first) of a side whose fixed end is at `fixed` and whose moving end has
/// been dragged to `dragged`. The moving end is the smaller one if `moving_end_is_the_smaller`. It
/// stops at the picture's edge, and at the smallest side length from the fixed end.
fn ordered_edges(fixed: f32, dragged: f32, moving_end_is_the_smaller: bool) -> (f32, f32) {
    let smallest = CanvasArea::SMALLEST_SIDE_FRACTION;
    if moving_end_is_the_smaller {
        let moving = dragged.clamp(0.0, (fixed - smallest).max(0.0));
        (moving, fixed)
    } else {
        let moving = dragged.clamp((fixed + smallest).min(1.0), 1.0);
        (fixed, moving)
    }
}

fn sanitised(value: f32, smallest: f32, largest: f32, fallback: f32) -> f32 {
    if value.is_nan() {
        fallback
    } else {
        value.clamp(smallest, largest.max(smallest))
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
    /// Where on the picture the text goes. What does not fit in it is cut off.
    pub area: CanvasArea,
}

impl TextObject {
    /// What a new text object says, until it is changed.
    pub const NEW_TEXT: &'static str = "Text";
}

impl Default for TextObject {
    /// A new text object: a short placeholder, white, in the default font, along the bottom.
    fn default() -> Self {
        Self {
            text: Self::NEW_TEXT.to_owned(),
            font_family: String::new(),
            font_size: FontSize::DEFAULT,
            colour: TextColour::WHITE,
            area: CanvasArea::DEFAULT,
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
    use crate::test_support::DeterministicRandomNumbers;

    fn assert_area_is_valid(area: CanvasArea) {
        assert!(area.left() >= 0.0 && area.top() >= 0.0, "{area:?}");
        assert!(area.right() <= 1.0 + 1e-6 && area.bottom() <= 1.0 + 1e-6, "{area:?}");
        assert!(area.width() >= CanvasArea::SMALLEST_SIDE_FRACTION - 1e-6, "{area:?}");
        assert!(area.height() >= CanvasArea::SMALLEST_SIDE_FRACTION - 1e-6, "{area:?}");
    }

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
    fn an_area_that_fits_is_kept_exactly() {
        let area = CanvasArea::new(0.25, 0.5, 0.5, 0.25);
        assert_eq!(
            (area.left(), area.top(), area.width(), area.height()),
            (0.25, 0.5, 0.5, 0.25)
        );
        assert_eq!((area.right(), area.bottom()), (0.75, 0.75));
    }

    #[test]
    fn an_area_that_does_not_fit_is_corrected_rather_than_refused() {
        // Too big: made as big as the picture.
        assert_eq!(CanvasArea::new(0.3, 0.3, 5.0, 5.0), CanvasArea::new(0.0, 0.0, 1.0, 1.0));
        // Sticking out: moved back in, keeping its size.
        let moved_in = CanvasArea::new(0.9, 0.9, 0.3, 0.2);
        assert_close(moved_in.left(), 0.7);
        assert_close(moved_in.top(), 0.8);
        // Too small: made the smallest.
        let tiny = CanvasArea::new(0.0, 0.0, 0.0, -1.0);
        assert_eq!(
            (tiny.width(), tiny.height()),
            (CanvasArea::SMALLEST_SIDE_FRACTION, CanvasArea::SMALLEST_SIDE_FRACTION)
        );
        // Not numbers: the default area.
        assert_eq!(
            CanvasArea::new(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            CanvasArea::DEFAULT
        );
        for odd in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            assert_area_is_valid(CanvasArea::new(odd, odd, odd, odd));
        }
    }

    #[test]
    fn the_default_area_is_itself_valid_and_survives_being_made_again() {
        let default = CanvasArea::DEFAULT;
        assert_area_is_valid(default);
        assert_eq!(
            CanvasArea::new(default.left(), default.top(), default.width(), default.height()),
            default
        );
    }

    #[test]
    fn moving_an_area_keeps_its_size_and_stops_at_the_edges_of_the_picture() {
        let area = CanvasArea::new(0.2, 0.2, 0.4, 0.3);
        let moved = area.moved_by(0.1, -0.05);
        assert_close(moved.left(), 0.3);
        assert_close(moved.top(), 0.15);
        assert_eq!((moved.width(), moved.height()), (area.width(), area.height()));

        let pushed_into_the_corner = area.moved_by(5.0, 5.0);
        assert_close(pushed_into_the_corner.right(), 1.0);
        assert_close(pushed_into_the_corner.bottom(), 1.0);
        let pushed_the_other_way = area.moved_by(-5.0, -5.0);
        assert_eq!((pushed_the_other_way.left(), pushed_the_other_way.top()), (0.0, 0.0));
        assert_eq!(area.moved_by(f32::NAN, f32::NAN), area, "not a number is no movement");
    }

    #[test]
    fn dragging_a_corner_moves_that_corner_and_leaves_the_opposite_one_where_it_was() {
        let area = CanvasArea::new(0.2, 0.3, 0.4, 0.3); // right = 0.6, bottom = 0.6
        for corner in Corner::ALL {
            let (fixed_x, fixed_y) = area.corner_position(corner.opposite());
            let dragged = area.with_corner_dragged_to(corner, 0.45, 0.5);
            assert_area_is_valid(dragged);
            let (still_x, still_y) = dragged.corner_position(corner.opposite());
            assert_close(still_x, fixed_x);
            assert_close(still_y, fixed_y);
        }
        let bottom_right = area.with_corner_dragged_to(Corner::BottomRight, 0.9, 0.8);
        assert_eq!(
            (bottom_right.left(), bottom_right.top()),
            (area.left(), area.top()),
            "the top left corner stays"
        );
        assert_close(bottom_right.right(), 0.9);
        assert_close(bottom_right.bottom(), 0.8);
        let top_left = area.with_corner_dragged_to(Corner::TopLeft, 0.1, 0.05);
        assert_close(top_left.left(), 0.1);
        assert_close(top_left.top(), 0.05);
        assert_close(top_left.right(), area.right());
        assert_close(top_left.bottom(), area.bottom());
    }

    #[test]
    fn dragging_a_corner_cannot_leave_the_picture_or_cross_the_opposite_corner() {
        let area = CanvasArea::new(0.2, 0.3, 0.4, 0.3);
        let past_the_picture = area.with_corner_dragged_to(Corner::BottomRight, 9.0, 9.0);
        assert_close(past_the_picture.right(), 1.0);
        assert_close(past_the_picture.bottom(), 1.0);
        // Dragging the bottom right corner up and to the left of the top left one makes it as
        // small as it can be, still with the top left corner where it was.
        let crossed = area.with_corner_dragged_to(Corner::BottomRight, 0.0, 0.0);
        assert_close(crossed.left(), area.left());
        assert_close(crossed.top(), area.top());
        assert_close(crossed.width(), CanvasArea::SMALLEST_SIDE_FRACTION);
        assert_close(crossed.height(), CanvasArea::SMALLEST_SIDE_FRACTION);
        assert_eq!(
            area.with_corner_dragged_to(Corner::TopLeft, f32::NAN, f32::NAN),
            area,
            "not a number: the corner stays"
        );
    }

    /// Regression test: a corner that was "dragged" to where it already was came back a rounding
    /// error different, so a drag that stood still looked like an edit on every frame.
    #[test]
    fn dragging_a_corner_to_where_it_is_changes_nothing_not_even_by_a_rounding_error() {
        for area in [
            CanvasArea::new(0.2, 0.3, 0.4, 0.3),
            CanvasArea::DEFAULT,
            CanvasArea::new(0.123_456, 0.654_321, 0.111_111, 0.222_222),
        ] {
            for corner in Corner::ALL {
                let (x, y) = area.corner_position(corner);
                assert_eq!(
                    area.with_corner_dragged_to(corner, x, y),
                    area,
                    "{corner:?} of {area:?}"
                );
            }
        }
    }

    #[test]
    fn whatever_is_dragged_wherever_the_area_stays_valid_and_the_opposite_corner_stays() {
        let mut random = DeterministicRandomNumbers::with_seed(11);
        for _ in 0..2000 {
            let unit = |random: &mut DeterministicRandomNumbers| random.next_below(10_001) as f32 / 10_000.0;
            let area = CanvasArea::new(
                unit(&mut random),
                unit(&mut random),
                unit(&mut random),
                unit(&mut random),
            );
            assert_area_is_valid(area);
            let corner = Corner::ALL[random.next_below(4) as usize];
            let x = unit(&mut random) * 3.0 - 1.0;
            let y = unit(&mut random) * 3.0 - 1.0;
            let dragged = area.with_corner_dragged_to(corner, x, y);
            assert_area_is_valid(dragged);
            let before = area.corner_position(corner.opposite());
            let after = dragged.corner_position(corner.opposite());
            assert!(
                (before.0 - after.0).abs() < 1e-4 && (before.1 - after.1).abs() < 1e-4,
                "{area:?} dragged at {corner:?} to ({x}, {y}) became {dragged:?}"
            );
        }
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
