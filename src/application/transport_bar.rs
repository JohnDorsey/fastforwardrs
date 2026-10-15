// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The row along the top of the timeline panel: the stop, play and pause buttons, the current time next to them, and the
//! slider through the whole video that fills the rest of the row.

use egui::{Rect, Response, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};

use super::commands::Command;
use super::{scrub_bar, theme};
use crate::project::{FrameRate, TimelineFrameIndex, format_timecode};

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransportIcon {
    Pause,
    Play,
    Stop,
}

/// What the bar needs to know to draw itself.
pub struct TransportBarState {
    pub is_playing: bool,
    pub current_frame: TimelineFrameIndex,
    /// How many frames the timeline has, which is what the slider goes through (and there is nothing to play when it is 0).
    pub total_frames: u64,
    pub frame_rate: FrameRate,
}

/// Draws the buttons, the time and the slider, and adds a command for each button pressed and for the slider being used.
pub fn show(ui: &mut Ui, state: &TransportBarState, commands: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let enabled = state.total_frames > 0;
        if icon_button(ui, TransportIcon::Stop, "Stop", enabled, false).clicked() {
            commands.push(Command::Stop);
        }
        if icon_button(ui, TransportIcon::Play, "Play", enabled, state.is_playing).clicked() {
            commands.push(Command::Play);
        }
        if icon_button(ui, TransportIcon::Pause, "Pause", enabled, false).clicked() {
            commands.push(Command::Pause);
        }
        ui.add_space(theme::SPACING);
        ui.label(
            egui::RichText::new(format_timecode(state.current_frame, state.frame_rate))
                .monospace()
                .size(20.0)
                .color(theme::ICON),
        );
        ui.add_space(theme::SPACING);
        scrub_bar::show(ui, state.current_frame, state.total_frames, commands);
    });
}

/// A square button with a picture drawn on it (drawn with shapes, so that no font is needed).
fn icon_button(ui: &mut Ui, icon: TransportIcon, tooltip: &str, enabled: bool, active: bool) -> Response {
    let size = vec2(34.0, 30.0);
    // Not focusable: the keys of the keyboard are shortcuts of the whole window (S cuts, the arrows
    // move the playhead), and a button that had the focus would answer to some of them as well.
    let (rectangle, response) = ui.allocate_exact_size(size, if enabled { Sense::CLICK } else { Sense::hover() });
    // The button has no text on it, so screen readers (and the tests) are told what it is.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, tooltip));
    let painter = ui.painter();

    let background = if !enabled {
        None
    } else if response.is_pointer_button_down_on() {
        Some(theme::ICON_BUTTON_PRESSED)
    } else if active {
        Some(theme::ICON_BUTTON_ACTIVE)
    } else if response.hovered() {
        Some(theme::ICON_BUTTON_HOVER)
    } else {
        None
    };
    if let Some(background) = background {
        painter.rect_filled(rectangle, 4.0, background);
    }

    let colour = if enabled { theme::ICON } else { theme::ICON_DISABLED };
    let centre = rectangle.center();
    match icon {
        TransportIcon::Pause => {
            for side in [-1.0, 1.0] {
                let bar = Rect::from_center_size(pos2(centre.x + side * 4.5, centre.y), vec2(4.5, 14.0));
                painter.rect_filled(bar, 1.0, colour);
            }
        }
        TransportIcon::Play => {
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(centre.x - 5.0, centre.y - 8.0),
                    pos2(centre.x - 5.0, centre.y + 8.0),
                    pos2(centre.x + 8.0, centre.y),
                ],
                colour,
                Stroke::NONE,
            ));
        }
        TransportIcon::Stop => {
            painter.rect_filled(Rect::from_center_size(centre, vec2(13.0, 13.0)), 1.0, colour);
        }
    }

    if enabled {
        response.on_hover_text(tooltip)
    } else {
        response
    }
}
