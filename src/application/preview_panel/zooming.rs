// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the wheel and the mouse do to how the preview is zoomed and where its picture is: the wheel zooms about the pointer (and so
//! does a pinch, or the wheel with Ctrl held), the wheel with Shift held moves the picture across and with Alt held up and down,
//! and the middle button, held down and dragged, takes hold of the picture and moves it. And the button that brings the picture back to
//! where it fits, which is only there while it is not.
//!
//! How the view comes to be what it is, from these, is worked out in [`super::view`].

use egui::{Button, CursorIcon, Id, PointerButton, Rect, Ui, Vec2, vec2};

use super::view::PreviewView;

/// How far from the edges of the panel the button that makes the picture fit is, in points.
const MARGIN_OF_THE_FIT_BUTTON: f32 = 10.0;

/// How big the button that makes the picture fit is, in points.
const SIZE_OF_THE_FIT_BUTTON: Vec2 = vec2(52.0, 26.0);

/// The name the button that makes the picture fit is known by, to the people who press it and to screen readers.
pub const FIT_BUTTON_LABEL: &str = "Fit";

/// Zooms and moves the view as the wheel and the mouse ask, while the pointer is over the panel (and nothing is over the panel
/// there: a menu, or a window that has to be dealt with first), and keeps it where it can be when the panel has changed its size.
pub(super) fn handle_the_wheel_and_the_mouse(ui: &Ui, view: &mut PreviewView, panel: Rect, picture_size: (u32, u32)) {
    *view = view.kept_where_it_can_be(panel, picture_size);
    let moving_id = Id::new("the picture of the preview is being moved by the middle button");
    let (pointer, zoom_by_pinch_or_control_wheel, scrolled, alt_is_held) = ui.input(|input| {
        (
            input.pointer.hover_pos(),
            input.zoom_delta(),
            input.smooth_scroll_delta,
            input.modifiers.alt,
        )
    });
    let pointer_is_over_the_panel = ui.rect_contains_pointer(panel);

    if let Some(pointer) = pointer.filter(|_| pointer_is_over_the_panel) {
        let zoom_speed = ui.ctx().options(|options| options.input_options.scroll_zoom_speed);
        // The wheel on its own zooms: Ctrl and the wheel is a zoom already, which is in `zoom_delta`; Shift turns what the wheel
        // does into going across, and Alt into going up and down.
        let zoom_by_wheel = if alt_is_held {
            1.0
        } else {
            (scrolled.y * zoom_speed).exp()
        };
        let factor = zoom_by_pinch_or_control_wheel * zoom_by_wheel;
        if factor != 1.0 {
            *view = view.zoomed_by(factor, pointer, panel, picture_size);
        }
        let moved_by_the_wheel = if alt_is_held { scrolled } else { vec2(scrolled.x, 0.0) };
        if moved_by_the_wheel != Vec2::ZERO {
            *view = view.shifted_by(moved_by_the_wheel, panel, picture_size);
        }
    }

    // Taking hold of the picture with the middle button is for as long as the button is held, wherever the pointer goes.
    let (middle_button_went_down, middle_button_is_down, pointer_went_by) = ui.input(|input| {
        (
            input.pointer.button_pressed(PointerButton::Middle),
            input.pointer.button_down(PointerButton::Middle),
            input.pointer.delta(),
        )
    });
    if middle_button_went_down && pointer_is_over_the_panel {
        ui.data_mut(|data| data.insert_temp(moving_id, true));
    }
    if !middle_button_is_down {
        ui.data_mut(|data| data.remove_temp::<bool>(moving_id));
    }
    if middle_button_is_down && ui.data(|data| data.get_temp::<bool>(moving_id)).unwrap_or(false) {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        *view = view.shifted_by(pointer_went_by, panel, picture_size);
    }
}

/// Shows the button that makes the picture fit the panel again, in the bottom right corner of it, if the picture does not fit it
/// now (it is not there when it does, as there is nothing for it to do). Pressing it puts the view back to where the picture fits.
///
/// It is made after everything else on the preview, so that it is over what is under it, and takes the press.
pub(super) fn show_the_button_that_makes_the_picture_fit(ui: &mut Ui, view: &mut PreviewView, panel: Rect) {
    if view.is_fitting() {
        return;
    }
    let button = Rect::from_min_size(
        panel.right_bottom() - SIZE_OF_THE_FIT_BUTTON - vec2(MARGIN_OF_THE_FIT_BUTTON, MARGIN_OF_THE_FIT_BUTTON),
        SIZE_OF_THE_FIT_BUTTON,
    );
    if ui
        .put(button, Button::new(FIT_BUTTON_LABEL))
        .on_hover_text("Show all of the picture, as big as it can be here")
        .clicked()
    {
        *view = PreviewView::FITTING;
    }
}
