// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets that take the mouse on the preview.
//!
//! One of them is over all of the panel the preview is in, and takes every press on it. What a press is
//! about is worked out from where it began (see [`super::grab`]), and not from which widget it came to, so
//! that is all the panel needs.
//!
//! The box of the selected layer (its outline, and the handles and the places to turn it from at its corners)
//! is another matter: it can reach out of the picture, and what is drawn on top of the picture is what a person
//! expects to take hold of. So each corner has a widget of its own, over the handle and the place that turns the
//! layer from it, registered after the panel's, which makes it the one that is on top where it is. (It stops at
//! the edge of the panel, as what is drawn of it does: over the other panels of the window, what is in them is
//! what is taken hold of.) These widgets only take the press to the same place as the panel's: what it is about is
//! worked out from where it began.

use egui::{PointerButton, Pos2, Rect, Response, Sense, Ui, Vec2, WidgetInfo, WidgetType, vec2};

use super::PreviewLayer;
use super::geometry::{HANDLE_SIZE, PictureOnScreen, TURN_ZONE_RADIUS};
use super::names::{catcher_id, corner_handle_id, corner_handle_label, turn_handle_label, turn_place_id};
use crate::project::Corner;

/// The widgets that take the mouse on the preview in one frame, and what they were done to.
pub(super) struct Catchers {
    responses: Vec<Response>,
}

impl Catchers {
    /// Makes the widgets of the frame: the one over `panel_rectangle`, and, if a layer is selected, one over
    /// each of its corners.
    pub fn register(
        ui: &mut Ui,
        panel_rectangle: Rect,
        selected_layer: Option<(&PreviewLayer<'_>, &PictureOnScreen)>,
    ) -> Self {
        // (The constants and not `Sense::click_and_drag()`, which can take the keyboard focus: then Tab would
        // stop at these, which have nothing to do for the keyboard.)
        let sense = Sense::CLICK | Sense::DRAG;
        let mut responses = vec![ui.interact(panel_rectangle, catcher_id(), sense)];
        if let Some((layer, picture)) = selected_layer {
            for corner in Corner::ALL {
                // The handle that resizes the layer is in the place just outside the corner that turns it, so the
                // one widget takes the press for both, and which of them it is about is worked out from where it began.
                let corner_on_the_screen = picture.corner_of(&layer.placement, corner);
                let place_to_turn_from = Rect::from_center_size(
                    corner_on_the_screen,
                    vec2(2.0 * TURN_ZONE_RADIUS, 2.0 * TURN_ZONE_RADIUS),
                );
                let response = ui.interact(place_to_turn_from, turn_place_id(layer.clip, corner), sense);
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, turn_handle_label(corner)));
                responses.push(response);
                // The handle is only named, for screen readers and the tests: the place it is in takes the press.
                let handle = Rect::from_center_size(corner_on_the_screen, vec2(HANDLE_SIZE, HANDLE_SIZE));
                ui.interact(handle, corner_handle_id(layer.clip, corner), Sense::hover())
                    .widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, corner_handle_label(corner)));
            }
        }
        Self { responses }
    }

    /// Whether a drag with `button` began on one of them this frame.
    pub fn drag_started_by(&self, button: PointerButton) -> bool {
        self.responses.iter().any(|response| response.drag_started_by(button))
    }

    /// Whether one of them is being dragged with `button`.
    pub fn dragged_by(&self, button: PointerButton) -> bool {
        self.responses.iter().any(|response| response.dragged_by(button))
    }

    /// Whether one of them was clicked with `button` this frame.
    pub fn clicked_by(&self, button: PointerButton) -> bool {
        self.responses.iter().any(|response| response.clicked_by(button))
    }

    /// Whether the pointer is over one of them, with nothing else on top of it.
    pub fn hovered(&self) -> bool {
        self.responses.iter().any(Response::hovered)
    }

    /// Where the pointer is, if one of them is being clicked or dragged.
    pub fn interact_pointer_pos(&self) -> Option<Pos2> {
        self.responses.iter().find_map(Response::interact_pointer_pos)
    }

    /// How far the pointer went since the last frame, if one of them is being dragged.
    pub fn drag_delta(&self) -> Vec2 {
        self.responses
            .iter()
            .fold(Vec2::ZERO, |total, response| total + response.drag_delta())
    }
}
