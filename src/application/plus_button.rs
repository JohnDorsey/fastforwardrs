// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The round button with a plus on it: the one in the timeline that adds a track, and the one in the properties panel that adds an
//! effect. It is drawn with shapes, so that no font is needed.

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, WidgetInfo, WidgetType, vec2};

use super::theme;

/// How wide across the button is, in points.
pub const DIAMETER: f32 = 34.0;

/// Makes the button with its middle at `centre`, called `label` (which is also what is shown when the pointer rests on it, while
/// it can be used). It is dimmed, and does nothing, if it cannot be used (`can_edit` is false). `resting_fill` is the colour it is
/// when nothing is being done to it, which is the colour of what it is on, near enough. `id_salt` tells it from other buttons.
///
/// It is not made so that the keyboard can focus it, so that the keys of the keyboard keep their meanings as shortcuts of the
/// window. Gives what the pointer did to it: `clicked()` on it is what the caller acts on.
pub fn show(
    ui: &mut Ui,
    centre: Pos2,
    id_salt: impl Hash + Debug,
    label: &str,
    can_edit: bool,
    resting_fill: Color32,
) -> Response {
    let rectangle = Rect::from_center_size(centre, vec2(DIAMETER, DIAMETER));
    let response = ui.interact(
        rectangle,
        ui.id().with(id_salt),
        if can_edit { Sense::CLICK } else { Sense::hover() },
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can_edit, label));
    let painter = ui.painter();
    let fill = if !can_edit {
        resting_fill
    } else if response.is_pointer_button_down_on() {
        theme::ICON_BUTTON_PRESSED
    } else if response.hovered() {
        theme::ICON_BUTTON_HOVER
    } else {
        resting_fill
    };
    painter.circle_filled(centre, DIAMETER / 2.0, fill);
    painter.circle_stroke(centre, DIAMETER / 2.0, Stroke::new(1.5, theme::TRACK_BORDER));
    draw_plus(
        painter,
        centre,
        DIAMETER * 0.24,
        3.0,
        if can_edit { theme::ICON } else { theme::ICON_DISABLED },
    );
    if can_edit {
        response.on_hover_text(label)
    } else {
        response
    }
}

/// Draws a plus sign whose arms reach `arm_length` from `centre`.
fn draw_plus(painter: &Painter, centre: Pos2, arm_length: f32, thickness: f32, colour: Color32) {
    let stroke = Stroke::new(thickness, colour);
    painter.line_segment(
        [
            Pos2::new(centre.x - arm_length, centre.y),
            Pos2::new(centre.x + arm_length, centre.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(centre.x, centre.y - arm_length),
            Pos2::new(centre.x, centre.y + arm_length),
        ],
        stroke,
    );
}
