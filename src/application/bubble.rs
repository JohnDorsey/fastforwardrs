//! The floating settings boxes: a box that floats above the thing it is about, with a little
//! pointer coming out of its bottom edge, like the tail of a speech bubble, whose tip touches that
//! thing.
//!
//! The box is an egui [`Area`], so that it floats above everything and takes the clicks that are
//! on it. Whoever opens one keeps it open until a press lands outside it, or the Escape key is
//! pressed, or the thing it points at is gone.

use egui::{
    Align2, Area, Context, Frame, Id, Key, Margin, Modifiers, Order, Painter, Pos2, Rect, Shadow, Shape, Stroke, Ui,
    pos2, vec2,
};

use super::theme;

/// How tall the pointer is, in points: how far the bottom of the box is above what it points at.
pub const TAIL_HEIGHT: f32 = 12.0;

/// How wide the pointer is where it joins the box, in points.
const TAIL_WIDTH: f32 = 20.0;

/// How far the pointer keeps from the corners of the box, whatever it points at, in points. (The
/// corners are rounded by this much.)
const CORNER_RADIUS: u8 = 10;

/// What the box takes up on screen, and what its contents answered.
pub struct BubbleResponse<R> {
    pub rect: Rect,
    pub inner: R,
}

/// The three corners of the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tail {
    /// The point at the end of the pointer, which touches what is pointed at.
    pub tip: Pos2,
    pub base_left: Pos2,
    pub base_right: Pos2,
}

/// The pointer of a box whose body is `body`, so that its tip is at `target`: it comes out of the
/// bottom edge of the box, as near to straight above the target as the corners of the box allow.
pub fn tail_pointing_at(body: Rect, target: Pos2) -> Tail {
    let half_width = TAIL_WIDTH / 2.0;
    let room_each_side = f32::from(CORNER_RADIUS) + half_width;
    let (leftmost, rightmost) = (body.left() + room_each_side, body.right() - room_each_side);
    // A box too narrow to have room for the pointer anywhere has it in the middle.
    let centre_x = if leftmost > rightmost {
        body.center().x
    } else {
        target.x.clamp(leftmost, rightmost)
    };
    Tail {
        tip: target,
        base_left: pos2(centre_x - half_width, body.bottom()),
        base_right: pos2(centre_x + half_width, body.bottom()),
    }
}

/// Shows a box above `target` (so that the tip of its pointer is at `target`) with what
/// `add_contents` puts in it.
pub fn show<R>(context: &Context, id: Id, target: Pos2, add_contents: impl FnOnce(&mut Ui) -> R) -> BubbleResponse<R> {
    let area = Area::new(id)
        .order(Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(target.x, target.y - TAIL_HEIGHT))
        .constrain(true)
        .show(context, |ui| {
            let body = Frame::new()
                .fill(theme::BUBBLE_BACKGROUND)
                .stroke(Stroke::new(1.0, theme::BUBBLE_BORDER))
                .corner_radius(CORNER_RADIUS)
                .inner_margin(Margin::same(12))
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 14,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(110),
                })
                .show(ui, add_contents);
            paint_tail(ui.painter(), tail_pointing_at(body.response.rect, target));
            BubbleResponse {
                rect: body.response.rect,
                inner: body.inner,
            }
        });
    area.inner
}

fn paint_tail(painter: &Painter, tail: Tail) {
    // The box's own border runs across the place where the pointer comes out; paint over it.
    painter.line_segment(
        [tail.base_left + vec2(1.0, 0.0), tail.base_right - vec2(1.0, 0.0)],
        Stroke::new(2.0, theme::BUBBLE_BACKGROUND),
    );
    painter.add(Shape::convex_polygon(
        vec![tail.base_left, tail.tip, tail.base_right],
        theme::BUBBLE_BACKGROUND,
        Stroke::NONE,
    ));
    let border = Stroke::new(1.0, theme::BUBBLE_BORDER);
    painter.line_segment([tail.base_left, tail.tip], border);
    painter.line_segment([tail.tip, tail.base_right], border);
}

/// Whether the box should be closed because a mouse button was pressed somewhere that is not on
/// it. `bubble_rect` is where the box was drawn, if it was.
pub fn pressed_outside(context: &Context, bubble_rect: Option<Rect>) -> bool {
    context.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|position| bubble_rect.is_none_or(|rect| !rect.expand(2.0).contains(position)))
    })
}

/// Whether the Escape key was pressed, which closes the box. (The key is used up.)
pub fn escape_pressed(context: &Context) -> bool {
    context.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> Rect {
        Rect::from_min_max(pos2(100.0, 200.0), pos2(300.0, 320.0))
    }

    #[test]
    fn the_pointer_comes_out_of_the_bottom_edge_straight_above_what_it_points_at() {
        let tail = tail_pointing_at(body(), pos2(180.0, 345.0));
        assert_eq!(tail.tip, pos2(180.0, 345.0));
        assert_eq!(tail.base_left, pos2(170.0, 320.0));
        assert_eq!(tail.base_right, pos2(190.0, 320.0));
    }

    #[test]
    fn the_pointer_keeps_clear_of_the_rounded_corners_and_leans_towards_what_it_points_at() {
        let far_left = tail_pointing_at(body(), pos2(-500.0, 345.0));
        assert_eq!(far_left.base_left.x, 100.0 + 10.0, "starts after the rounded corner");
        assert_eq!(far_left.base_right.x, 100.0 + 10.0 + 20.0);
        assert_eq!(far_left.tip, pos2(-500.0, 345.0), "but still touches what it points at");

        let far_right = tail_pointing_at(body(), pos2(9000.0, 345.0));
        assert_eq!(far_right.base_right.x, 300.0 - 10.0);
        assert_eq!(far_right.base_left.x, 300.0 - 10.0 - 20.0);
    }

    #[test]
    fn a_box_too_narrow_for_the_pointer_has_it_in_the_middle() {
        let narrow = Rect::from_min_max(pos2(100.0, 200.0), pos2(120.0, 260.0));
        let tail = tail_pointing_at(narrow, pos2(500.0, 290.0));
        assert_eq!((tail.base_left.x + tail.base_right.x) / 2.0, 110.0);
    }

    #[test]
    fn the_base_of_the_pointer_is_always_on_the_bottom_edge() {
        for target_x in [-10.0, 110.0, 200.0, 290.0, 1000.0] {
            let tail = tail_pointing_at(body(), pos2(target_x, 400.0));
            assert_eq!(tail.base_left.y, 320.0);
            assert_eq!(tail.base_right.y, 320.0);
            assert_eq!(tail.base_right.x - tail.base_left.x, 20.0);
        }
    }
}
