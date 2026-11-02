// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is in the place of the slider of a number that is read from a mask (`docs/shader-masks-design.md`): the mask's **thumbnail**, which says which track the number is read from and as
//! which channel, and opens the window of the mask when it is clicked.
//!
//! The thumbnail is a small picture of what the mask does to the number, not of the track's pictures: a bar that goes from nothing to everything of the channel in the range of the
//! number, tinted as the channel is. (A picture of the track at the playhead is for later.)

use egui::{
    Color32, CornerRadius, CursorIcon, Response, Sense, Shape, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2,
    vec2,
};

use crate::application::mask_settings_window::name_of_a_channel;
use crate::project::{MaskChannel, MaskMap};

/// How tall the thumbnail is, in points.
const HEIGHT: f32 = 22.0;

/// How wide the bar that is the picture of the mask is, in points.
const BAR_WIDTH: f32 = 34.0;

/// What the thumbnail is called to a screen reader and the tests: the number it stands for.
pub fn label_of_the_thumbnail(name_of_the_number: &str) -> String {
    format!("Mask of {name_of_the_number}")
}

/// What the thumbnail says when the pointer rests on it.
pub const HINT: &str = "This number is read from the picture of a track. Click to change that.";

/// How much of its tint the bar has at the place `level` along it (0 at the left, 1 at the right): the level itself for a straight map, and the height of the graph at it for a graph, so that the
/// bar is a picture of what the mask does to the number.
pub fn level_of_the_bar_at(map: MaskMap, level: f32) -> f32 {
    match map {
        MaskMap::Straight => level,
        MaskMap::Graph(graph) => graph.height_at(level),
    }
}

/// The colour the bar of a channel is tinted with.
fn tint_of(channel: MaskChannel) -> Color32 {
    match channel {
        MaskChannel::Red => Color32::from_rgb(230, 70, 70),
        MaskChannel::Green => Color32::from_rgb(70, 190, 90),
        MaskChannel::Blue => Color32::from_rgb(80, 130, 240),
        MaskChannel::Brightness => Color32::from_gray(235),
    }
}

/// Shows the thumbnail of a mask made of the track called `track`, whose `channel`, mapped as `map` says, is the number called `name_of_the_number`, as wide as there is room. Gives its response: it is
/// clicked when the window of the mask is wanted.
pub fn show(ui: &mut Ui, name_of_the_number: &str, track: &str, channel: MaskChannel, map: MaskMap) -> Response {
    let width = ui.available_width().max(BAR_WIDTH + 40.0);
    let (rectangle, response) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::click());
    let label = label_of_the_thumbnail(name_of_the_number);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label.clone()));
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();
    painter.rect(
        rectangle,
        CornerRadius::same(4),
        if response.hovered() {
            visuals.bg_fill
        } else {
            ui.visuals().extreme_bg_color
        },
        Stroke::new(1.0, visuals.bg_stroke.color),
        StrokeKind::Inside,
    );
    // The bar: from nothing of the channel, on the left, to all of it, on the right, in steps that the eye cannot tell from a smooth ramp.
    let bar = egui::Rect::from_min_size(rectangle.min + vec2(4.0, 4.0), vec2(BAR_WIDTH, HEIGHT - 8.0));
    const STEPS: usize = 16;
    let tint = tint_of(channel);
    for step in 0..STEPS {
        let (from, to) = (step as f32 / STEPS as f32, (step + 1) as f32 / STEPS as f32);
        let level = level_of_the_bar_at(map, (from + to) / 2.0);
        let shade = Color32::from_rgb(
            (f32::from(tint.r()) * level) as u8,
            (f32::from(tint.g()) * level) as u8,
            (f32::from(tint.b()) * level) as u8,
        );
        painter.add(Shape::rect_filled(
            egui::Rect::from_min_max(
                pos2(bar.left() + bar.width() * from, bar.top()),
                pos2(bar.left() + bar.width() * to, bar.bottom()),
            ),
            CornerRadius::ZERO,
            shade,
        ));
    }
    painter.rect_stroke(
        bar,
        CornerRadius::same(2),
        Stroke::new(1.0, visuals.fg_stroke.color),
        StrokeKind::Outside,
    );
    painter.text(
        pos2(bar.right() + 8.0, rectangle.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{track} \u{b7} {}", name_of_a_channel(channel)),
        egui::TextStyle::Body.resolve(ui.style()),
        visuals.text_color(),
    );
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(HINT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Graph;

    #[test]
    fn the_bar_of_a_straight_map_goes_from_nothing_to_all_and_that_of_a_graph_follows_the_graph() {
        let upside_down = MaskMap::Graph(Graph::from_points(&[(0.0, 1.0), (1.0, 0.0)]).unwrap());
        let triangle = MaskMap::Graph(Graph::from_points(&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]).unwrap());

        for level in [0.0, 0.25, 0.5, 1.0] {
            assert_eq!(level_of_the_bar_at(MaskMap::Straight, level), level);
            assert!((level_of_the_bar_at(upside_down, level) - (1.0 - level)).abs() < 1e-6);
        }
        assert!((level_of_the_bar_at(triangle, 0.25) - 0.5).abs() < 1e-6);
        assert!((level_of_the_bar_at(triangle, 0.5) - 1.0).abs() < 1e-6);
    }
}
