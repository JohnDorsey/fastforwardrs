// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where something that is seen (a video, a picture, a piece of text) is on the picture, and how it
//! is turned.
//!
//! Like the fast-forward symbol, everything about where something is and how big it is is a
//! fraction of the picture rather than a number of pixels, so that it lands in the same place
//! whatever size the picture is shown or exported at. A [`CanvasArea`] is the rectangle, and a
//! [`Placement`] is that rectangle and an [`Angle`] it is turned by about its middle. Something may be
//! put partly past the edge of the picture (what is past it is not seen), as long as its middle is on the
//! picture, so that it can always be found and taken hold of again.
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

/// A rectangle of whole pixels on a picture: what an area comes to when the picture is a given size. Its
/// left and top are below zero when it starts before the picture does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelBox {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// A rectangle on the picture, as fractions of the picture's width and height: `(0, 0)` is the
/// top left corner of the picture and `(1, 1)` the bottom right. It is never smaller than
/// [`Self::SMALLEST_SIDE_FRACTION`] each way and never bigger than the picture, and its middle is always on
/// the picture, but it can reach out past the edges of it.
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
    /// until its middle is on the picture. (A value that is not a number counts as the default one.)
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        let width = sanitised(width, Self::SMALLEST_SIDE_FRACTION, 1.0, Self::DEFAULT.width);
        let height = sanitised(height, Self::SMALLEST_SIDE_FRACTION, 1.0, Self::DEFAULT.height);
        let (least_left, most_left) = starts_with_the_middle_on_the_picture(width);
        let (least_top, most_top) = starts_with_the_middle_on_the_picture(height);
        let left = sanitised(
            left,
            least_left,
            most_left,
            Self::DEFAULT.left.clamp(least_left, most_left),
        );
        let top = sanitised(top, least_top, most_top, Self::DEFAULT.top.clamp(least_top, most_top));
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
    /// never empty and never bigger than the picture. An area that is inside the picture is inside it in
    /// pixels too, whatever the rounding; one that reaches out of it does so by about as much in pixels.
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
    /// it keeps its size, and is stopped when its middle is at an edge of the picture (so that half of
    /// it is past the edge).
    #[must_use]
    pub fn moved_by(&self, across: f32, down: f32) -> Self {
        let across = if across.is_nan() { 0.0 } else { across };
        let down = if down.is_nan() { 0.0 } else { down };
        let (least_left, most_left) = starts_with_the_middle_on_the_picture(self.width);
        let (least_top, most_top) = starts_with_the_middle_on_the_picture(self.height);
        Self {
            left: (self.left + across).clamp(least_left, most_left),
            top: (self.top + down).clamp(least_top, most_top),
            ..*self
        }
    }

    /// The area after `corner` is dragged to the place `(x, y)` (as fractions of the picture, which can be
    /// outside it): the corner diagonally across stays where it is. The corner cannot go past the corner
    /// that is staying, the area cannot become smaller than [`Self::SMALLEST_SIDE_FRACTION`] or bigger than
    /// the picture, and the corner stops where the middle of the area would leave the picture.
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
/// can go past the picture's edge, but the side cannot be shorter than the smallest side length or
/// longer than the picture, and its middle cannot be off the picture.
fn ordered_edges(fixed: f32, dragged: f32, moving_end_is_the_smaller: bool) -> (f32, f32) {
    let smallest = CanvasArea::SMALLEST_SIDE_FRACTION;
    if moving_end_is_the_smaller {
        // The side runs from the moving end to `fixed`. Its middle is on the picture when the sum of
        // its ends is from 0 to 2.
        let least = (fixed - 1.0).max(-fixed);
        let most = (fixed - smallest).min(2.0 - fixed).max(least);
        (dragged.clamp(least, most), fixed)
    } else {
        // (`CanvasArea::new` would also stop the side at the length of the picture, as it keeps its start: but that
        // is not for this function to lean on, and the other branch has to do it itself, as it keeps its end.)
        let most = (fixed + 1.0).min(2.0 - fixed);
        let least = (fixed + smallest).max(-fixed).min(most);
        (fixed, dragged.clamp(least, most))
    }
}

/// The least and the most a side that is `length` long may start at (as fractions of the picture's side)
/// for its middle to be on the picture: from half its length before the picture to half its length before
/// the picture's end.
fn starts_with_the_middle_on_the_picture(length: f32) -> (f32, f32) {
    (-length / 2.0, 1.0 - length / 2.0)
}

/// How far past the end of the picture, as a fraction of it, a side can end and still be taken to be inside
/// the picture: rounding errors of the sums that its end comes from are far below this.
const ROUNDING_ERROR_OF_AN_EDGE: f32 = 1e-6;

/// Where a side of an area starts and how long it is, in the whole pixels of a picture side that is
/// `picture_side` pixels long: the length is rounded, at least a pixel and at most the picture, and
/// the start is rounded. A side that does not reach past the end of the picture is kept so that it does not
/// after rounding either; one that does reach past it does, as far as it does.
fn side_in_pixels(start: f32, length: f32, picture_side: u32) -> (i32, u32) {
    let picture_side = picture_side.max(1);
    let length_in_pixels = ((length * picture_side as f32).round() as u32).clamp(1, picture_side);
    let start_in_pixels = (start * picture_side as f32).round() as i32;
    let reaches_past_the_end = start + length > 1.0 + ROUNDING_ERROR_OF_AN_EDGE;
    if reaches_past_the_end {
        (start_in_pixels, length_in_pixels)
    } else {
        let last_start = i32::try_from(picture_side - length_in_pixels).unwrap_or(i32::MAX);
        (start_in_pixels.min(last_start), length_in_pixels)
    }
}

fn sanitised(value: f32, smallest: f32, largest: f32, fallback: f32) -> f32 {
    if value.is_nan() {
        fallback
    } else {
        value.clamp(smallest, largest.max(smallest))
    }
}

/// How far something is turned, in degrees, clockwise as it is seen on the screen. It may be any number of turns, and
/// is kept as it was made: 720 degrees is two whole turns, which looks the same as none but is not the same thing between
/// two keyframes (the picture goes round twice on the way). What it looks like, as an angle between -180 and 180, is
/// [`Self::within_a_turn`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Angle {
    degrees: f32,
}

impl Angle {
    pub const ZERO: Angle = Angle { degrees: 0.0 };

    /// The angle of `degrees`, which may be any number of turns: what is not a number is no angle at all.
    pub fn from_degrees(degrees: f32) -> Self {
        if degrees.is_finite() {
            Self { degrees }
        } else {
            Self::ZERO
        }
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

    /// This angle as the one between -180 (not included) and 180 that points the same way.
    #[must_use]
    pub fn within_a_turn(self) -> Self {
        let mut within_a_turn = self.degrees.rem_euclid(360.0);
        if within_a_turn > 180.0 {
            within_a_turn -= 360.0;
        }
        Self { degrees: within_a_turn }
    }

    /// The turn, the short way round, that takes `other` to this angle: between -180 (not included) and 180.
    #[must_use]
    pub fn shortest_turn_from(self, other: Angle) -> Self {
        self.minus(other).within_a_turn()
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

    /// The same place, moved so that it is held to the edges of the picture: where the rectangle, as it is turned, has an
    /// edge that is within `within` of an edge of the picture (`within` is how far, as fractions of the picture's width
    /// and of its height), it is moved so that the two are exactly together. It is the farthest left, right, top and bottom of
    /// the rectangle that count, so a turned rectangle is held by the corner that is nearest to the edge. Along each way, if
    /// both of its edges are near the picture's (it is nearly as wide as the picture), the one that is nearer is held.
    /// Nothing else about it changes, and it is not moved farther than [`Self::moved_by`] lets it be.
    #[must_use]
    pub fn snapped_to_the_edges_of_the_picture(&self, within: (f32, f32), picture_size: PictureDimensions) -> Self {
        let corners = Corner::ALL.map(|corner| self.corner_position(corner, picture_size));
        let across_the_corners = corners.map(|(x, _)| x);
        let down_the_corners = corners.map(|(_, y)| y);
        let across = move_that_holds_to_an_edge(
            smallest_of(across_the_corners),
            largest_of(across_the_corners),
            within.0,
        );
        let down = move_that_holds_to_an_edge(smallest_of(down_the_corners), largest_of(down_the_corners), within.1);
        self.moved_by(across, down)
    }

    /// The place after `corner` is dragged to `point` (which can be outside the picture). The corner
    /// diagonally across stays where it is on the screen (unless that would take the middle of the
    /// rectangle off the picture), and the rectangle stays turned as it was. With `keep_shape` the
    /// rectangle keeps the shape it has, and gets as big as it must for the corner to be as near to
    /// `point` as it can be.
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

/// How far something that goes from `near` to `far` along a line that the picture goes from 0 to 1 along has to move to have
/// `near` at 0 or `far` at 1, when that is no further than `within`: the shorter of the two moves when both are, and none
/// when neither is.
fn move_that_holds_to_an_edge(near: f32, far: f32, within: f32) -> f32 {
    [-near, 1.0 - far]
        .into_iter()
        .filter(|distance| distance.abs() <= within)
        .min_by(|first, second| first.abs().total_cmp(&second.abs()))
        .unwrap_or(0.0)
}

fn smallest_of(values: [f32; 4]) -> f32 {
    values.into_iter().fold(f32::INFINITY, f32::min)
}

fn largest_of(values: [f32; 4]) -> f32 {
    values.into_iter().fold(f32::NEG_INFINITY, f32::max)
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
mod tests;
