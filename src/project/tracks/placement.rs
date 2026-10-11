//! Where something that is seen (a video, a picture, a piece of text) is on the picture, and how it
//! is turned.
//!
//! Like the fast-forward symbol, everything about where something is and how big it is is a
//! fraction of the picture rather than a number of pixels, so that it lands in the same place
//! whatever size the picture is shown or exported at. A [`CanvasArea`] is the rectangle, and a
//! [`Placement`] is that rectangle and an [`Angle`] it is turned by about its middle.
//!
//! Turning makes a difference to how a corner is dragged, and to what is under a point, which are
//! worked out here, in the pixels of a picture of a given size (the fractions of the width and of
//! the height are not the same length, so nothing about angles can be done in them).

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

    /// Which way the corner is from the middle of the area, along and across it: -1 for the left
    /// or the top, 1 for the right or the bottom.
    fn signs(self) -> (f32, f32) {
        match self {
            Self::TopLeft => (-1.0, -1.0),
            Self::TopRight => (1.0, -1.0),
            Self::BottomLeft => (-1.0, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }
}

/// A rectangle of whole pixels on a picture: what an area comes to when the picture is a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelBox {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
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

    /// All of the picture.
    pub const WHOLE_PICTURE: CanvasArea = CanvasArea {
        left: 0.0,
        top: 0.0,
        width: 1.0,
        height: 1.0,
    };

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

    /// Where the area is on a picture `picture_size` pixels across and down, in whole pixels. It is
    /// never empty and never reaches outside the picture.
    pub fn in_pixels(&self, (picture_width, picture_height): (u32, u32)) -> PixelBox {
        let (left, width) = side_in_pixels(self.left, self.width, picture_width);
        let (top, height) = side_in_pixels(self.top, self.height, picture_height);
        PixelBox {
            left,
            top,
            width,
            height,
        }
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

/// Where a side of an area starts and how long it is, in the whole pixels of a picture side that is
/// `picture_side` pixels long: the length is rounded, at least a pixel and at most the picture, and
/// the start is rounded and kept so that the side does not reach past the end of the picture.
fn side_in_pixels(start: f32, length: f32, picture_side: u32) -> (u32, u32) {
    let picture_side = picture_side.max(1);
    let length_in_pixels = ((length * picture_side as f32).round() as u32).clamp(1, picture_side);
    let start_in_pixels = ((start * picture_side as f32).round() as u32).min(picture_side - length_in_pixels);
    (start_in_pixels, length_in_pixels)
}

fn sanitised(value: f32, smallest: f32, largest: f32, fallback: f32) -> f32 {
    if value.is_nan() {
        fallback
    } else {
        value.clamp(smallest, largest.max(smallest))
    }
}

/// How far something is turned, in degrees, clockwise as it is seen on the screen. It is always
/// more than -180 and at most 180: a turn of 270 degrees is a turn of -90.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Angle {
    degrees: f32,
}

impl Angle {
    pub const ZERO: Angle = Angle { degrees: 0.0 };

    /// The angle of `degrees` (which may be any number of turns): what is not a number is no angle at all.
    pub fn from_degrees(degrees: f32) -> Self {
        if !degrees.is_finite() {
            return Self::ZERO;
        }
        let mut within_a_turn = degrees.rem_euclid(360.0);
        if within_a_turn > 180.0 {
            within_a_turn -= 360.0;
        }
        Self { degrees: within_a_turn }
    }

    pub fn degrees(self) -> f32 {
        self.degrees
    }

    pub fn radians(self) -> f32 {
        self.degrees.to_radians()
    }

    pub fn is_zero(self) -> bool {
        self.degrees == 0.0
    }

    /// This angle and `other` one after the other.
    #[must_use]
    pub fn plus(self, other: Angle) -> Self {
        Self::from_degrees(self.degrees + other.degrees)
    }

    /// How far on from `other` this angle is: the turn that takes `other` to this.
    #[must_use]
    pub fn minus(self, other: Angle) -> Self {
        Self::from_degrees(self.degrees - other.degrees)
    }

    /// The nearest angle that is a whole number of `step_degrees`.
    #[must_use]
    pub fn snapped_to(self, step_degrees: f32) -> Self {
        if step_degrees.is_nan() || step_degrees <= 0.0 {
            return self;
        }
        Self::from_degrees((self.degrees / step_degrees).round() * step_degrees)
    }
}

/// Where something is on the picture: the rectangle it fills, and how far that is turned about its middle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub area: CanvasArea,
    pub rotation: Angle,
}

/// A point on the picture, as fractions of its width and of its height.
pub type PointOnThePicture = (f32, f32);

/// The size of the picture, in pixels or any other unit that is the same along it and down it.
pub type PictureDimensions = (f32, f32);

impl Placement {
    pub fn new(area: CanvasArea, rotation: Angle) -> Self {
        Self { area, rotation }
    }

    /// The place not turned at all.
    pub fn upright(area: CanvasArea) -> Self {
        Self::new(area, Angle::ZERO)
    }

    /// The middle of the rectangle, which it is turned about.
    pub fn centre(&self) -> PointOnThePicture {
        (
            self.area.left() + self.area.width() / 2.0,
            self.area.top() + self.area.height() / 2.0,
        )
    }

    /// Where `corner` of the rectangle is once it is turned.
    pub fn corner_position(&self, corner: Corner, picture_size: PictureDimensions) -> PointOnThePicture {
        let (half_width, half_height) = self.half_sides_in_pixels(picture_size);
        let (sign_x, sign_y) = corner.signs();
        let (centre_x, centre_y) = self.centre_in_pixels(picture_size);
        let (offset_x, offset_y) = turned(sign_x * half_width, sign_y * half_height, self.rotation);
        in_fractions((centre_x + offset_x, centre_y + offset_y), picture_size)
    }

    /// The direction from the middle of the rectangle to `point`: 0 is straight to the right and it
    /// goes clockwise. (A point right in the middle has no direction: zero.)
    pub fn direction_to(&self, point: PointOnThePicture, picture_size: PictureDimensions) -> Angle {
        let (centre_x, centre_y) = self.centre_in_pixels(picture_size);
        let (x, y) = in_pixels(point, picture_size);
        let (across, down) = (x - centre_x, y - centre_y);
        if across == 0.0 && down == 0.0 {
            return Angle::ZERO;
        }
        Angle::from_degrees(down.atan2(across).to_degrees())
    }

    /// Whether `point` is inside the rectangle, turned as it is.
    pub fn contains(&self, point: PointOnThePicture, picture_size: PictureDimensions) -> bool {
        let (half_width, half_height) = self.half_sides_in_pixels(picture_size);
        let (centre_x, centre_y) = self.centre_in_pixels(picture_size);
        let (x, y) = in_pixels(point, picture_size);
        let (along, across) = turned(
            x - centre_x,
            y - centre_y,
            Angle::from_degrees(-self.rotation.degrees()),
        );
        along.abs() <= half_width && across.abs() <= half_height
    }

    /// The same place moved by this many picture widths and heights, as far as the picture allows.
    #[must_use]
    pub fn moved_by(&self, across: f32, down: f32) -> Self {
        Self {
            area: self.area.moved_by(across, down),
            ..*self
        }
    }

    /// The place after `corner` is dragged to `point`. The corner diagonally across stays where it is
    /// on the screen (unless that would take the rectangle out of the picture), and the rectangle
    /// stays turned as it was. With `keep_shape` the rectangle keeps the shape it has, and gets as big
    /// as it must for the corner to be as near to `point` as it can be.
    ///
    /// An upright rectangle that may change shape is dragged the way [`CanvasArea::with_corner_dragged_to`] says.
    #[must_use]
    pub fn with_corner_dragged_to(
        &self,
        corner: Corner,
        point: PointOnThePicture,
        picture_size: PictureDimensions,
        keep_shape: bool,
    ) -> Self {
        let point = (
            if point.0.is_nan() {
                self.corner_position(corner, picture_size).0
            } else {
                point.0
            },
            if point.1.is_nan() {
                self.corner_position(corner, picture_size).1
            } else {
                point.1
            },
        );
        if self.rotation.is_zero() && !keep_shape {
            return Self {
                area: self.area.with_corner_dragged_to(corner, point.0, point.1),
                ..*self
            };
        }
        let (picture_width, picture_height) = picture_size;
        let (half_width, half_height) = self.half_sides_in_pixels(picture_size);
        let (sign_x, sign_y) = corner.signs();
        let (centre_x, centre_y) = self.centre_in_pixels(picture_size);

        // The corner that stays, and where the pointer is in relation to it along and across the rectangle.
        let (stay_offset_x, stay_offset_y) = turned(sign_x * half_width, sign_y * half_height, self.rotation);
        let (fixed_x, fixed_y) = (centre_x - stay_offset_x, centre_y - stay_offset_y);
        let (pointer_x, pointer_y) = in_pixels(point, picture_size);
        let (along, across) = turned(
            pointer_x - fixed_x,
            pointer_y - fixed_y,
            Angle::from_degrees(-self.rotation.degrees()),
        );

        let smallest = (
            CanvasArea::SMALLEST_SIDE_FRACTION * picture_width,
            CanvasArea::SMALLEST_SIDE_FRACTION * picture_height,
        );
        let (mut width, mut height) = ((sign_x * along).max(smallest.0), (sign_y * across).max(smallest.1));
        if keep_shape {
            // The scale at which the rectangle just reaches the pointer along both sides, then the
            // one that is not too small nor too big for the picture.
            let (own_width, own_height) = (2.0 * half_width, 2.0 * half_height);
            let least_scale = (smallest.0 / own_width).max(smallest.1 / own_height);
            let most_scale = (picture_width / own_width).min(picture_height / own_height);
            let scale = (width / own_width)
                .max(height / own_height)
                .clamp(least_scale, most_scale.max(least_scale));
            (width, height) = (scale * own_width, scale * own_height);
        } else {
            (width, height) = (width.min(picture_width), height.min(picture_height));
        }

        let (move_x, move_y) = turned(sign_x * width / 2.0, sign_y * height / 2.0, self.rotation);
        let (new_centre_x, new_centre_y) = (fixed_x + move_x, fixed_y + move_y);
        Self {
            area: CanvasArea::new(
                (new_centre_x - width / 2.0) / picture_width,
                (new_centre_y - height / 2.0) / picture_height,
                width / picture_width,
                height / picture_height,
            ),
            ..*self
        }
    }

    fn half_sides_in_pixels(&self, (picture_width, picture_height): PictureDimensions) -> (f32, f32) {
        (
            self.area.width() * picture_width / 2.0,
            self.area.height() * picture_height / 2.0,
        )
    }

    fn centre_in_pixels(&self, picture_size: PictureDimensions) -> (f32, f32) {
        in_pixels(self.centre(), picture_size)
    }
}

fn in_pixels((x, y): PointOnThePicture, (picture_width, picture_height): PictureDimensions) -> (f32, f32) {
    (x * picture_width, y * picture_height)
}

fn in_fractions((x, y): (f32, f32), (picture_width, picture_height): PictureDimensions) -> PointOnThePicture {
    (x / picture_width, y / picture_height)
}

/// The vector `(across, down)` turned clockwise (as it is seen on the screen) by `angle`.
fn turned(across: f32, down: f32, angle: Angle) -> (f32, f32) {
    let (sine, cosine) = angle.radians().sin_cos();
    (across * cosine - down * sine, across * sine + down * cosine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::DeterministicRandomNumbers;

    const SQUARE_PICTURE: PictureDimensions = (1000.0, 1000.0);
    const WIDE_PICTURE: PictureDimensions = (1920.0, 1080.0);

    fn assert_area_is_valid(area: CanvasArea) {
        assert!(area.left() >= 0.0 && area.top() >= 0.0, "{area:?}");
        assert!(area.right() <= 1.0 + 1e-6 && area.bottom() <= 1.0 + 1e-6, "{area:?}");
        assert!(area.width() >= CanvasArea::SMALLEST_SIDE_FRACTION - 1e-6, "{area:?}");
        assert!(area.height() >= CanvasArea::SMALLEST_SIDE_FRACTION - 1e-6, "{area:?}");
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-4, "{actual} is not {expected}");
    }

    fn assert_points_close(actual: PointOnThePicture, expected: PointOnThePicture) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-4 && (actual.1 - expected.1).abs() < 1e-4,
            "{actual:?} is not {expected:?}"
        );
    }

    // --- the area ---

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
    fn the_area_of_the_whole_picture_is_the_one_that_is_made_from_the_whole_picture() {
        assert_eq!(CanvasArea::WHOLE_PICTURE, CanvasArea::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(
            CanvasArea::WHOLE_PICTURE,
            CanvasArea::new(-3.0, -3.0, 9.0, 9.0),
            "what is too big is made as big as the picture"
        );
        assert_area_is_valid(CanvasArea::WHOLE_PICTURE);
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
    fn an_area_is_the_pixels_its_fractions_come_to_rounded() {
        let area = CanvasArea::new(0.25, 0.5, 0.5, 0.25);
        assert_eq!(
            area.in_pixels((1280, 720)),
            PixelBox {
                left: 320,
                top: 360,
                width: 640,
                height: 180
            }
        );
        assert_eq!(
            CanvasArea::WHOLE_PICTURE.in_pixels((1280, 720)),
            PixelBox {
                left: 0,
                top: 0,
                width: 1280,
                height: 720
            }
        );
        // 0.1 of 1920 is 192, 0.74 of 1080 is 799.2, 0.8 of 1920 is 1536 and 0.16 of 1080 is 172.8.
        assert_eq!(
            CanvasArea::DEFAULT.in_pixels((1920, 1080)),
            PixelBox {
                left: 192,
                top: 799,
                width: 1536,
                height: 173
            }
        );
    }

    #[test]
    fn an_area_in_pixels_is_never_empty_and_never_reaches_outside_the_picture() {
        let mut random = DeterministicRandomNumbers::with_seed(5);
        for _ in 0..3000 {
            let unit = |random: &mut DeterministicRandomNumbers| random.next_below(10_001) as f32 / 10_000.0;
            let area = CanvasArea::new(
                unit(&mut random),
                unit(&mut random),
                unit(&mut random),
                unit(&mut random),
            );
            let picture = (1 + random.next_below(4000) as u32, 1 + random.next_below(4000) as u32);
            let pixels = area.in_pixels(picture);
            assert!(
                pixels.width >= 1 && pixels.height >= 1,
                "{area:?} on {picture:?}: {pixels:?}"
            );
            assert!(
                pixels.left + pixels.width <= picture.0,
                "{area:?} on {picture:?}: {pixels:?}"
            );
            assert!(
                pixels.top + pixels.height <= picture.1,
                "{area:?} on {picture:?}: {pixels:?}"
            );
        }
        // Even a picture with nothing in it (which is not made) gives a box that is a pixel.
        let pixels = CanvasArea::WHOLE_PICTURE.in_pixels((0, 0));
        assert_eq!((pixels.width, pixels.height), (1, 1));
    }

    #[test]
    fn an_area_that_reaches_the_edge_of_the_picture_reaches_it_in_pixels_too_whatever_the_rounding() {
        // 0.3333 + 0.6667 reaches the edge, and 3.333 and 6.667 rounded are 3 and 7: 10 pixels.
        let area = CanvasArea::new(0.3333, 0.0, 0.6667, 1.0);
        let pixels = area.in_pixels((10, 10));
        assert_eq!(pixels.left + pixels.width, 10);
        // A half across a picture of an odd width: 3.5 and 3.5 both round up, which would be 8.
        let area = CanvasArea::new(0.5, 0.0, 0.5, 1.0);
        let pixels = area.in_pixels((7, 7));
        assert_eq!(pixels.left + pixels.width, 7, "{pixels:?}");
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

    // --- angles ---

    #[test]
    fn an_angle_is_kept_between_minus_180_and_180_whatever_it_was_made_from() {
        for (made_from, is) in [
            (0.0, 0.0),
            (90.0, 90.0),
            (180.0, 180.0),
            (-180.0, 180.0),
            (181.0, -179.0),
            (270.0, -90.0),
            (-270.0, 90.0),
            (360.0, 0.0),
            (725.0, 5.0),
            (-725.0, -5.0),
        ] {
            assert_close(Angle::from_degrees(made_from).degrees(), is);
        }
        for odd in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(Angle::from_degrees(odd), Angle::ZERO, "{odd}");
        }
        assert!(Angle::ZERO.is_zero() && !Angle::from_degrees(0.5).is_zero());
        assert_close(Angle::from_degrees(180.0).radians(), std::f32::consts::PI);
    }

    #[test]
    fn angles_add_and_subtract_going_round_the_circle() {
        let a = Angle::from_degrees(170.0);
        let b = Angle::from_degrees(30.0);
        assert_close(a.plus(b).degrees(), -160.0);
        assert_close(b.minus(a).degrees(), -140.0);
        assert_close(a.minus(b).degrees(), 140.0);
        assert_close(a.plus(Angle::from_degrees(-170.0)).degrees(), 0.0);
    }

    #[test]
    fn an_angle_snaps_to_the_nearest_multiple_of_a_step() {
        let snapped = |degrees: f32| Angle::from_degrees(degrees).snapped_to(15.0).degrees();
        assert_close(snapped(7.0), 0.0);
        assert_close(snapped(8.0), 15.0);
        assert_close(snapped(-8.0), -15.0);
        assert_close(snapped(100.0), 105.0);
        assert_close(snapped(179.0), 180.0);
        assert_close(snapped(-179.0), 180.0);
        assert_close(Angle::from_degrees(33.3).snapped_to(0.0).degrees(), 33.3);
        assert_close(Angle::from_degrees(33.3).snapped_to(f32::NAN).degrees(), 33.3);
    }

    // --- turning ---

    #[test]
    fn the_corners_of_an_upright_place_are_those_of_its_area() {
        let place = Placement::upright(CanvasArea::new(0.2, 0.3, 0.4, 0.3));
        for corner in Corner::ALL {
            assert_points_close(
                place.corner_position(corner, WIDE_PICTURE),
                place.area.corner_position(corner),
            );
        }
        assert_points_close(place.centre(), (0.4, 0.45));
    }

    #[test]
    fn a_quarter_turn_takes_each_corner_clockwise_to_the_next_one_about_the_middle() {
        // 400 by 200 pixels, in the middle of a square picture: turned a quarter clockwise it is 200 by 400.
        let place = Placement::new(CanvasArea::new(0.3, 0.4, 0.4, 0.2), Angle::from_degrees(90.0));
        assert_points_close(place.corner_position(Corner::TopLeft, SQUARE_PICTURE), (0.6, 0.3));
        assert_points_close(place.corner_position(Corner::TopRight, SQUARE_PICTURE), (0.6, 0.7));
        assert_points_close(place.corner_position(Corner::BottomRight, SQUARE_PICTURE), (0.4, 0.7));
        assert_points_close(place.corner_position(Corner::BottomLeft, SQUARE_PICTURE), (0.4, 0.3));
    }

    #[test]
    fn turning_does_not_move_the_middle() {
        let place = Placement::new(CanvasArea::new(0.1, 0.2, 0.5, 0.3), Angle::from_degrees(37.0));
        let (x, y) = (
            Corner::ALL
                .map(|corner| place.corner_position(corner, WIDE_PICTURE).0)
                .iter()
                .sum::<f32>()
                / 4.0,
            Corner::ALL
                .map(|corner| place.corner_position(corner, WIDE_PICTURE).1)
                .iter()
                .sum::<f32>()
                / 4.0,
        );
        assert_points_close((x, y), place.centre());
    }

    #[test]
    fn the_direction_to_a_point_is_measured_clockwise_from_straight_to_the_right() {
        let place = Placement::upright(CanvasArea::new(0.4, 0.4, 0.2, 0.2)); // the middle is (0.5, 0.5)
        let direction = |point: PointOnThePicture| place.direction_to(point, SQUARE_PICTURE).degrees();
        assert_close(direction((0.8, 0.5)), 0.0);
        assert_close(direction((0.5, 0.8)), 90.0);
        assert_close(direction((0.2, 0.5)), 180.0);
        assert_close(direction((0.5, 0.2)), -90.0);
        assert_close(direction((0.8, 0.8)), 45.0);
        assert_eq!(place.direction_to((0.5, 0.5), SQUARE_PICTURE), Angle::ZERO);
    }

    #[test]
    fn the_direction_is_true_on_a_picture_that_is_wider_than_it_is_tall() {
        let place = Placement::upright(CanvasArea::new(0.4, 0.4, 0.2, 0.2));
        // A tenth of the width (192 pixels) across and 192 pixels down is 45 degrees, which is 0.1778 of the height.
        let direction = place.direction_to((0.6, 0.5 + 192.0 / 1080.0), WIDE_PICTURE).degrees();
        assert_close(direction, 45.0);
    }

    #[test]
    fn a_point_is_inside_a_turned_place_when_it_is_inside_the_rectangle_that_is_turned() {
        // 400 by 100 pixels in the middle of a square picture, turned a quarter: it is upright and 100 by 400.
        let place = Placement::new(CanvasArea::new(0.3, 0.45, 0.4, 0.1), Angle::from_degrees(90.0));
        assert!(place.contains((0.5, 0.5), SQUARE_PICTURE));
        assert!(
            place.contains((0.5, 0.69), SQUARE_PICTURE),
            "where the rectangle now reaches up and down"
        );
        assert!(place.contains((0.54, 0.31), SQUARE_PICTURE));
        assert!(
            !place.contains((0.69, 0.5), SQUARE_PICTURE),
            "where it reached across before it was turned"
        );
        assert!(!place.contains((0.5, 0.71), SQUARE_PICTURE));
        let upright = Placement::upright(place.area);
        assert!(upright.contains((0.69, 0.5), SQUARE_PICTURE));
        assert!(!upright.contains((0.5, 0.69), SQUARE_PICTURE));
    }

    // --- dragging a corner of a turned place ---

    #[test]
    fn dragging_a_corner_of_an_upright_place_is_what_the_area_alone_does() {
        let place = Placement::upright(CanvasArea::new(0.2, 0.3, 0.4, 0.3));
        for corner in Corner::ALL {
            let dragged = place.with_corner_dragged_to(corner, (0.45, 0.5), WIDE_PICTURE, false);
            assert_eq!(
                dragged.area,
                place.area.with_corner_dragged_to(corner, 0.45, 0.5),
                "{corner:?}"
            );
            assert_eq!(dragged.rotation, Angle::ZERO);
        }
    }

    #[test]
    fn dragging_a_corner_of_a_turned_place_moves_that_corner_to_the_pointer_and_leaves_the_opposite_corner_where_it_is_on_the_screen()
     {
        let place = Placement::new(CanvasArea::new(0.3, 0.3, 0.3, 0.2), Angle::from_degrees(30.0));
        for corner in Corner::ALL {
            let fixed = place.corner_position(corner.opposite(), WIDE_PICTURE);
            // A place beyond the corner, along the way the rectangle is turned.
            let direction = (
                place.corner_position(corner, WIDE_PICTURE).0 - fixed.0,
                place.corner_position(corner, WIDE_PICTURE).1 - fixed.1,
            );
            let pointer = (fixed.0 + direction.0 * 1.2, fixed.1 + direction.1 * 1.2);

            let dragged = place.with_corner_dragged_to(corner, pointer, WIDE_PICTURE, false);

            assert_eq!(dragged.rotation, place.rotation, "{corner:?}");
            assert_points_close(dragged.corner_position(corner.opposite(), WIDE_PICTURE), fixed);
            assert_points_close(dragged.corner_position(corner, WIDE_PICTURE), pointer);
            assert_area_is_valid(dragged.area);
        }
    }

    #[test]
    fn dragging_a_corner_with_the_shape_kept_scales_the_rectangle_about_the_opposite_corner() {
        let place = Placement::new(CanvasArea::new(0.3, 0.3, 0.3, 0.3), Angle::from_degrees(-20.0));
        let fixed = place.corner_position(Corner::TopLeft, SQUARE_PICTURE);
        let corner_now = place.corner_position(Corner::BottomRight, SQUARE_PICTURE);
        let pointer = (
            fixed.0 + (corner_now.0 - fixed.0) * 1.5,
            fixed.1 + (corner_now.1 - fixed.1) * 1.5,
        );

        let scaled = place.with_corner_dragged_to(Corner::BottomRight, pointer, SQUARE_PICTURE, true);

        assert_close(scaled.area.width(), 0.45);
        assert_close(scaled.area.height(), 0.45);
        assert_points_close(scaled.corner_position(Corner::TopLeft, SQUARE_PICTURE), fixed);
        assert_points_close(scaled.corner_position(Corner::BottomRight, SQUARE_PICTURE), pointer);
    }

    #[test]
    fn keeping_the_shape_the_rectangle_stays_the_shape_it_was_in_pixels_not_in_fractions() {
        let place = Placement::upright(CanvasArea::new(0.3, 0.3, 0.2, 0.3)); // 384 by 324 pixels
        let scaled = place.with_corner_dragged_to(Corner::BottomRight, (0.7, 0.9), WIDE_PICTURE, true);
        let shape = |area: CanvasArea| area.width() * WIDE_PICTURE.0 / (area.height() * WIDE_PICTURE.1);
        assert_close(shape(scaled.area), shape(place.area));
        assert!(scaled.area.width() > place.area.width());
    }

    #[test]
    fn dragging_a_corner_of_a_turned_place_is_never_a_change_when_it_has_not_moved() {
        let place = Placement::new(CanvasArea::new(0.2, 0.3, 0.4, 0.3), Angle::from_degrees(45.0));
        for corner in Corner::ALL {
            for keep_shape in [false, true] {
                let here = place.corner_position(corner, WIDE_PICTURE);
                let dragged = place.with_corner_dragged_to(corner, here, WIDE_PICTURE, keep_shape);
                assert_close(dragged.area.left(), place.area.left());
                assert_close(dragged.area.top(), place.area.top());
                assert_close(dragged.area.width(), place.area.width());
                assert_close(dragged.area.height(), place.area.height());
            }
        }
    }

    #[test]
    fn whatever_is_dragged_wherever_a_turned_place_stays_valid_turned_the_same_way_and_never_too_small_or_too_big() {
        let mut random = DeterministicRandomNumbers::with_seed(23);
        for round in 0..3000 {
            let unit = |random: &mut DeterministicRandomNumbers| random.next_below(10_001) as f32 / 10_000.0;
            let area = CanvasArea::new(
                unit(&mut random),
                unit(&mut random),
                unit(&mut random),
                unit(&mut random),
            );
            let place = Placement::new(area, Angle::from_degrees(unit(&mut random) * 360.0 - 180.0));
            let corner = Corner::ALL[random.next_below(4) as usize];
            let pointer = (unit(&mut random) * 3.0 - 1.0, unit(&mut random) * 3.0 - 1.0);
            let keep_shape = random.next_below(2) == 0;
            let picture = [WIDE_PICTURE, SQUARE_PICTURE, (1080.0, 1920.0)][random.next_below(3) as usize];

            let dragged = place.with_corner_dragged_to(corner, pointer, picture, keep_shape);

            let context = format!(
                "round {round}: {place:?} dragged at {corner:?} to {pointer:?} on {picture:?} (shape kept: {keep_shape})"
            );
            assert_eq!(dragged.rotation, place.rotation, "{context}");
            assert_area_is_valid(dragged.area);
            if keep_shape {
                let shape = |area: CanvasArea| (area.width() * picture.0) / (area.height() * picture.1);
                let at_a_limit = dragged.area.width() <= CanvasArea::SMALLEST_SIDE_FRACTION + 1e-4
                    || dragged.area.height() <= CanvasArea::SMALLEST_SIDE_FRACTION + 1e-4
                    || dragged.area.width() >= 1.0 - 1e-4
                    || dragged.area.height() >= 1.0 - 1e-4;
                if !at_a_limit && place.area.width() > 0.05 && place.area.height() > 0.05 {
                    let relative_difference = (shape(dragged.area) / shape(place.area) - 1.0).abs();
                    assert!(
                        relative_difference < 1e-3,
                        "{context}: the shape changed by {relative_difference}"
                    );
                }
            }
        }
    }
}
