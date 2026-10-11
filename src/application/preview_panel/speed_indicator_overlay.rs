//! The fast-forward symbol on the preview, which can be dragged to move it and resized by its corner.

use egui::{Color32, CursorIcon, Id, Rect, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2};

use crate::application::commands::Command;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::{SpeedFactor, SpeedIndicatorLayout};
use crate::speed_indicator::{SMALLEST_HEIGHT_IN_PIXELS, indicator_aspect_ratio, place_indicator};

/// The size of the corner that is grabbed to resize the symbol, in points.
const RESIZE_CORNER_SIZE: f32 = 14.0;

/// Draws the symbol for `speed_factor` on the picture, and adds a command when it is dragged.
pub(super) fn show(
    ui: &mut Ui,
    layout: SpeedIndicatorLayout,
    speed_factor: SpeedFactor,
    picture_rectangle: Rect,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let placement = place_indicator(
        layout,
        picture_rectangle.width(),
        picture_rectangle.height(),
        indicator_aspect_ratio(speed_factor),
    );
    let symbol_rectangle = Rect::from_min_size(
        picture_rectangle.min + vec2(placement.left, placement.top),
        vec2(placement.width, placement.height),
    );

    let height_in_pixels =
        ((placement.height * ui.ctx().pixels_per_point()).round() as u32).max(SMALLEST_HEIGHT_IN_PIXELS);
    let texture = textures.symbol(ui.ctx(), speed_factor, height_in_pixels);
    ui.painter().image(
        texture.id(),
        symbol_rectangle,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    // Dragging the symbol moves it; dragging its corner resizes it. Neither can take the keyboard
    // focus, so the keys of the keyboard keep their meanings.
    let drag_sense = Sense::CLICK | Sense::DRAG;
    let corner_rectangle = Rect::from_min_size(
        symbol_rectangle.right_bottom() - vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
        vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
    );
    let body = ui.interact(symbol_rectangle, Id::new("speed indicator body"), drag_sense);
    let corner = ui.interact(corner_rectangle, Id::new("speed indicator corner"), drag_sense);
    body.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Fast-forward symbol"));
    corner.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Fast-forward symbol size handle"));

    if corner.dragged() {
        // The corner follows the pointer: the symbol is as tall as the distance down to it.
        if let Some(pointer) = corner.interact_pointer_pos() {
            let new_height = (pointer.y - symbol_rectangle.top()).max(1.0);
            commands.push(Command::SetSpeedIndicatorLayout(SpeedIndicatorLayout::new(
                placement.left / picture_rectangle.width(),
                placement.top / picture_rectangle.height(),
                new_height / picture_rectangle.height(),
            )));
        }
    } else if body.dragged() {
        let delta = body.drag_delta();
        commands.push(Command::SetSpeedIndicatorLayout(SpeedIndicatorLayout::new(
            (placement.left + delta.x) / picture_rectangle.width(),
            (placement.top + delta.y) / picture_rectangle.height(),
            layout.height_fraction_of_frame_height(),
        )));
    }

    let in_use = body.dragged() || corner.dragged();
    if body.hovered() || corner.hovered() || in_use {
        ui.painter().rect_stroke(
            symbol_rectangle,
            2.0,
            Stroke::new(1.5, theme::INDICATOR_OUTLINE),
            StrokeKind::Outside,
        );
        ui.painter()
            .rect_filled(corner_rectangle.shrink(3.0), 1.0, theme::INDICATOR_OUTLINE);
        let cursor = if corner.hovered() || corner.dragged() {
            CursorIcon::ResizeNwSe
        } else if body.dragged() {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        };
        ui.ctx().set_cursor_icon(cursor);
    }
}
