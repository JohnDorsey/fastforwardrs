// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A badge in a chain of effects: a rounded bar, in the colour of a group (grey for the track of a clip), that says of what it is the badge
//! and where in the chain the effects of that are applied.
//!
//! A badge that can be moved has grip dots at its left, and is taken hold of anywhere to be moved: so that it can be, **what it stands for is
//! selected by a double click and not by a click**, which does not select anything by itself.

use std::fmt::Debug;
use std::hash::Hash;

use egui::{
    Align, Color32, CornerRadius, Label, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    pos2, vec2,
};

use crate::application::timeline_panel::{GripDotArrangement, draw_grip_dots};
use crate::project::Group;

/// How tall a badge is, in points.
pub(super) const HEIGHT: f32 = 30.0;

/// How round its corners are, in points: all the way, as a bar.
pub(super) const CORNER_RADIUS: u8 = 15;

/// How far the writing is from the left edge of a badge that cannot be moved, and from that of one that can, which has the dots in the room
/// that is left, and from the right edge, in points.
const WRITING_FROM_THE_LEFT: f32 = 16.0;
const WRITING_FROM_THE_LEFT_WHEN_THERE_ARE_DOTS: f32 = 30.0;
const WRITING_FROM_THE_RIGHT: f32 = 12.0;

/// What a badge looks like.
#[derive(Clone, Copy, Debug)]
pub(super) struct BadgeLook {
    pub fill: Color32,
    pub writing: Color32,
}

impl BadgeLook {
    /// How what stands for `group` looks (its badge, and the bar at the head of its box): its colour, with white or black writing on it,
    /// whichever can be read better.
    pub(super) fn of_a_group(group: &Group) -> Self {
        let colour = group.colour();
        Self {
            fill: Color32::from_rgb(colour.red, colour.green, colour.blue),
            writing: if colour.writing_on_it_is_white() {
                Color32::WHITE
            } else {
                Color32::BLACK
            },
        }
    }
}

/// Draws a badge as wide as there is room for, which says `label`. A badge `can_be_moved` is drawn with dots, and taken hold of with the
/// pointer (the response says when it is); one that cannot only senses clicks. Hovering over it says `what_a_double_click_does`.
/// `id_salt` tells it from the other badges and boxes of the chain, so that taking hold of it goes on while the rows move.
pub(super) fn show(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    label: &str,
    look: BadgeLook,
    can_be_moved: bool,
    what_a_double_click_does: &str,
) -> Response {
    let (_, rectangle) = ui.allocate_space(vec2(ui.available_width(), HEIGHT));
    let response = ui.interact(
        rectangle,
        ui.id().with(id_salt),
        // (The constants, and not `Sense::click_and_drag()` and `Sense::click()`, which can take the keyboard focus: then Tab would stop at
        // something that cannot be seen to have it, and has nothing to do for the keyboard.)
        if can_be_moved {
            Sense::CLICK | Sense::DRAG
        } else {
            Sense::CLICK
        },
    );
    let painter = ui.painter();
    painter.rect_filled(rectangle, CornerRadius::same(CORNER_RADIUS), look.fill);
    if response.hovered() || response.dragged() {
        painter.rect_stroke(
            rectangle,
            CornerRadius::same(CORNER_RADIUS),
            Stroke::new(1.5, look.writing.gamma_multiply(0.7)),
            StrokeKind::Inside,
        );
    }
    let writing_starts = if can_be_moved {
        draw_grip_dots(
            painter,
            GripDotArrangement::FOR_A_RACK,
            pos2(rectangle.left() + 16.0, rectangle.center().y),
        );
        WRITING_FROM_THE_LEFT_WHEN_THERE_ARE_DOTS
    } else {
        WRITING_FROM_THE_LEFT
    };
    let room_for_the_writing = Rect::from_min_max(
        pos2(rectangle.left() + writing_starts, rectangle.top()),
        pos2(rectangle.right() - WRITING_FROM_THE_RIGHT, rectangle.bottom()),
    );
    let mut writing = ui.new_child(
        UiBuilder::new()
            .max_rect(room_for_the_writing)
            .layout(Layout::left_to_right(Align::Center)),
    );
    writing.add(
        Label::new(RichText::new(label).color(look.writing).size(14.0))
            .truncate()
            .selectable(false),
    );
    response.on_hover_text(what_a_double_click_does)
}
