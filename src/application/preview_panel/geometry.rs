//! Where things on the preview are on the screen, and what a point on the screen is on.
//!
//! Everything about a layer's place is kept as fractions of the finished picture (see
//! [`crate::project::Placement`]). The picture is drawn on the screen as a rectangle of the same
//! shape, so turning (which is worked out in pixels, where it is the same along and across) comes out
//! the same on the screen.

use egui::{CursorIcon, Pos2, Rect, pos2, vec2};

use crate::application::textures::PictureFilter;
use crate::project::{Corner, PictureDimensions, Placement};

/// How big the squares at the corners of the selected layer are, in points.
pub(super) const HANDLE_SIZE: f32 = 12.0;

/// How far from a corner of the selected layer, in points, a point just outside it still turns it.
pub(super) const TURN_ZONE_RADIUS: f32 = 34.0;

/// The finished picture, as it is drawn on the screen.
#[derive(Clone, Copy, Debug)]
pub(super) struct PictureOnScreen {
    pub rectangle: Rect,
    /// How big the finished picture is, in pixels. It is only used for its shape and for the sizes
    /// that turning is worked out in.
    pub size_in_pixels: PictureDimensions,
    /// How the pictures that are drawn on it are put on the screen: blended, or, when the picture is zoomed in so far that the pixels
    /// can be seen, as blocks.
    pub filter: PictureFilter,
}

impl PictureOnScreen {
    /// The picture of this many pixels drawn in `rectangle`, which blends its pixels.
    pub fn new(rectangle: Rect, picture_width_in_pixels: u32, picture_height_in_pixels: u32) -> Self {
        Self {
            rectangle,
            size_in_pixels: (
                picture_width_in_pixels.max(1) as f32,
                picture_height_in_pixels.max(1) as f32,
            ),
            filter: PictureFilter::Smooth,
        }
    }

    /// The same picture, with the pictures that are drawn on it put on the screen the way `filter` says.
    #[must_use]
    pub fn shown_as(self, filter: PictureFilter) -> Self {
        Self { filter, ..self }
    }

    /// How many pixels wide and high the finished picture is.
    pub fn size_in_whole_pixels(&self) -> (u32, u32) {
        (
            self.size_in_pixels.0.round() as u32,
            self.size_in_pixels.1.round() as u32,
        )
    }

    /// Where the point `(x, y)` of the picture, as fractions of its width and height, is on the screen.
    pub fn point_at(&self, (x, y): (f32, f32)) -> Pos2 {
        pos2(
            self.rectangle.left() + x * self.rectangle.width(),
            self.rectangle.top() + y * self.rectangle.height(),
        )
    }

    /// How far across and down the picture, as fractions of its size, `point` on the screen is.
    pub fn fractions_at(&self, point: Pos2) -> (f32, f32) {
        (
            (point.x - self.rectangle.left()) / self.rectangle.width().max(1.0),
            (point.y - self.rectangle.top()) / self.rectangle.height().max(1.0),
        )
    }

    /// Where `corner` of the layer with this `placement` is on the screen, turned as it is.
    pub fn corner_of(&self, placement: &Placement, corner: Corner) -> Pos2 {
        self.point_at(placement.corner_position(corner, self.size_in_pixels))
    }

    /// The four corners of the layer on the screen, in the order the outline goes round: top left,
    /// top right, bottom right, bottom left.
    pub fn outline_of(&self, placement: &Placement) -> [Pos2; 4] {
        [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomRight,
            Corner::BottomLeft,
        ]
        .map(|corner| self.corner_of(placement, corner))
    }

    /// The smallest rectangle that has all of the layer in it.
    pub fn bounding_rectangle_of(&self, placement: &Placement) -> Rect {
        Rect::from_points(&self.outline_of(placement))
    }

    /// Whether `point` on the screen is on the layer, which is the rectangle turned as it is.
    pub fn contains(&self, placement: &Placement, point: Pos2) -> bool {
        placement.contains(self.fractions_at(point), self.size_in_pixels)
    }

    /// The point of the screen the layer is turned about.
    pub fn middle_of(&self, placement: &Placement) -> Pos2 {
        self.point_at(placement.centre())
    }
}

/// The corner of the layer whose square handle `point` is on, if it is on one.
pub(super) fn corner_whose_handle_is_at(
    picture: &PictureOnScreen,
    placement: &Placement,
    point: Pos2,
) -> Option<Corner> {
    Corner::ALL.into_iter().find(|&corner| {
        Rect::from_center_size(picture.corner_of(placement, corner), vec2(HANDLE_SIZE, HANDLE_SIZE)).contains(point)
    })
}

/// The corner of the layer that `point` is just outside of, so that grabbing the layer there turns it,
/// if it is: not on the layer, not on a handle, and not too far from a corner.
pub(super) fn corner_to_turn_by(picture: &PictureOnScreen, placement: &Placement, point: Pos2) -> Option<Corner> {
    if picture.contains(placement, point) || corner_whose_handle_is_at(picture, placement, point).is_some() {
        return None;
    }
    let (corner, distance) = Corner::ALL
        .into_iter()
        .map(|corner| (corner, picture.corner_of(placement, corner).distance(point)))
        .min_by(|first, second| first.1.total_cmp(&second.1))?;
    (distance <= TURN_ZONE_RADIUS).then_some(corner)
}

/// The mouse pointer that shows a corner handle can be dragged to resize: the one that points along
/// the line from the middle of the layer through the corner (as near as the pointers go: across,
/// down, or either way along a diagonal).
pub(super) fn cursor_for_resizing_at(picture: &PictureOnScreen, placement: &Placement, corner: Corner) -> CursorIcon {
    let outwards = picture.corner_of(placement, corner) - picture.middle_of(placement);
    let eighths_of_a_turn = (outwards.y.atan2(outwards.x).to_degrees() / 45.0).round() as i32;
    match eighths_of_a_turn.rem_euclid(4) {
        0 => CursorIcon::ResizeHorizontal,
        1 => CursorIcon::ResizeNwSe,
        2 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeNeSw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Angle, CanvasArea};

    /// A picture of 1600 by 900 pixels drawn 800 by 450 points, with its top left corner at (100, 50).
    fn picture() -> PictureOnScreen {
        PictureOnScreen::new(Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 450.0)), 1600, 900)
    }

    fn close(actual: Pos2, expected: Pos2) {
        assert!(actual.distance(expected) < 1e-3, "{actual:?} is not {expected:?}");
    }

    #[test]
    fn a_point_of_the_picture_is_found_on_the_screen_and_back() {
        close(picture().point_at((0.25, 0.5)), pos2(300.0, 275.0));
        let (x, y) = picture().fractions_at(pos2(300.0, 275.0));
        assert_eq!((x, y), (0.25, 0.5));
        // Outside the picture is outside the fractions 0 to 1.
        let (x, y) = picture().fractions_at(pos2(0.0, 0.0));
        assert!(x < 0.0 && y < 0.0);
    }

    #[test]
    fn the_corners_of_an_upright_layer_are_those_of_its_rectangle_on_the_screen() {
        let placement = Placement::upright(CanvasArea::new(0.25, 0.5, 0.5, 0.2));
        let outline = picture().outline_of(&placement);
        close(outline[0], pos2(300.0, 275.0));
        close(outline[1], pos2(700.0, 275.0));
        close(outline[2], pos2(700.0, 365.0));
        close(outline[3], pos2(300.0, 365.0));
        assert_eq!(
            picture().bounding_rectangle_of(&placement),
            Rect::from_min_max(pos2(300.0, 275.0), pos2(700.0, 365.0))
        );
    }

    #[test]
    fn a_layer_turned_a_quarter_has_its_corners_a_quarter_round_its_middle_in_screen_points_too() {
        // 400 by 90 points, middle at (500, 320): turned a quarter clockwise it is 90 across and 400 down.
        let placement = Placement::new(CanvasArea::new(0.25, 0.5, 0.5, 0.2), Angle::from_degrees(90.0));
        let outline = picture().outline_of(&placement);
        close(outline[0], pos2(545.0, 120.0));
        close(outline[1], pos2(545.0, 520.0));
        close(outline[2], pos2(455.0, 520.0));
        close(outline[3], pos2(455.0, 120.0));
        close(picture().middle_of(&placement), pos2(500.0, 320.0));
    }

    #[test]
    fn what_is_on_a_turned_layer_is_what_is_inside_the_turned_rectangle_not_its_bounding_one() {
        let placement = Placement::new(CanvasArea::new(0.25, 0.5, 0.5, 0.2), Angle::from_degrees(45.0));
        let picture = picture();
        assert!(picture.contains(&placement, pos2(500.0, 320.0)), "the middle");
        // The corner of the bounding rectangle is far from the turned rectangle, which only reaches
        // 45 points either way across.
        let bounds = picture.bounding_rectangle_of(&placement);
        assert!(!picture.contains(&placement, bounds.left_top() + vec2(2.0, 2.0)));
        assert!(!picture.contains(&placement, bounds.right_bottom() - vec2(2.0, 2.0)));
        // Along the way it is turned it reaches nearly 200 points from the middle.
        assert!(picture.contains(&placement, pos2(500.0, 320.0) + vec2(130.0, 130.0)));
        assert!(!picture.contains(&placement, pos2(500.0, 320.0) + vec2(130.0, -130.0)));
    }

    #[test]
    fn a_handle_is_where_a_point_is_within_half_its_size_of_a_corner() {
        let placement = Placement::upright(CanvasArea::new(0.25, 0.5, 0.5, 0.2));
        let picture = picture();
        assert_eq!(
            corner_whose_handle_is_at(&picture, &placement, pos2(303.0, 278.0)),
            Some(Corner::TopLeft)
        );
        assert_eq!(
            corner_whose_handle_is_at(&picture, &placement, pos2(697.0, 362.0)),
            Some(Corner::BottomRight)
        );
        assert_eq!(
            corner_whose_handle_is_at(&picture, &placement, pos2(500.0, 320.0)),
            None
        );
        assert_eq!(
            corner_whose_handle_is_at(&picture, &placement, pos2(320.0, 295.0)),
            None
        );
    }

    #[test]
    fn a_point_just_outside_a_corner_turns_by_that_corner_and_not_inside_or_on_the_handle_or_far_away() {
        let placement = Placement::upright(CanvasArea::new(0.25, 0.5, 0.5, 0.2));
        let picture = picture();
        // The top left corner is at (300, 275).
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(285.0, 260.0)),
            Some(Corner::TopLeft)
        );
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(715.0, 380.0)),
            Some(Corner::BottomRight)
        );
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(310.0, 285.0)),
            None,
            "inside"
        );
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(296.0, 271.0)),
            None,
            "on the handle, which resizes"
        );
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(200.0, 200.0)),
            None,
            "far away"
        );
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(500.0, 250.0)),
            None,
            "beside the middle of a side is not near a corner"
        );
    }

    #[test]
    fn the_corner_to_turn_by_follows_the_corner_where_it_is_after_turning() {
        // Turned a quarter clockwise, the top left corner of the rectangle is at the top right.
        let placement = Placement::new(CanvasArea::new(0.25, 0.5, 0.5, 0.2), Angle::from_degrees(90.0));
        let picture = picture();
        // Just up and to the right of (545, 120).
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(560.0, 105.0)),
            Some(Corner::TopLeft)
        );
        assert_eq!(
            corner_to_turn_by(&picture, &placement, pos2(440.0, 105.0)),
            Some(Corner::BottomLeft)
        );
    }

    #[test]
    fn the_pointer_over_a_resize_handle_points_the_way_the_corner_is_from_the_middle() {
        let picture = picture();
        let upright = Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5));
        assert_eq!(
            cursor_for_resizing_at(&picture, &upright, Corner::TopLeft),
            CursorIcon::ResizeNwSe
        );
        assert_eq!(
            cursor_for_resizing_at(&picture, &upright, Corner::BottomRight),
            CursorIcon::ResizeNwSe
        );
        assert_eq!(
            cursor_for_resizing_at(&picture, &upright, Corner::TopRight),
            CursorIcon::ResizeNeSw
        );
        assert_eq!(
            cursor_for_resizing_at(&picture, &upright, Corner::BottomLeft),
            CursorIcon::ResizeNeSw
        );
        // A box (320 by 180 pixels) turned an eighth of a turn has its corners nearest up, across, across and down.
        let turned = Placement::new(CanvasArea::new(0.4, 0.4, 0.2, 0.2), Angle::from_degrees(45.0));
        let cursors: Vec<CursorIcon> = Corner::ALL
            .map(|corner| cursor_for_resizing_at(&picture, &turned, corner))
            .to_vec();
        assert_eq!(
            cursors,
            vec![
                CursorIcon::ResizeVertical,   // top left, now at the top
                CursorIcon::ResizeHorizontal, // top right, now at the right
                CursorIcon::ResizeHorizontal, // bottom left, now at the left
                CursorIcon::ResizeVertical,   // bottom right, now at the bottom
            ]
        );
    }
}
