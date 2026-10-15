// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The controls for where a video, a picture or a text object is on the picture: where its middle is, how wide and tall it is,
//! and how far it is turned. (They can be changed on the preview, too, by dragging the thing and its corners, which is
//! quicker; these are for the exact numbers, and for the buttons that make keyframes of them.)

use egui::{DragValue, Ui};

use super::PropertiesContent;
use super::animatable::{self, Animatable};
use crate::application::commands::Command;
use crate::project::{CanvasArea, ClipId, PropertyId, PropertyValue, with_placement_value};

/// A number of percent of the picture, that is changed by dragging it or by typing.
fn percent<'a>(value: &'a mut f32, smallest: f32) -> DragValue<'a> {
    DragValue::new(value)
        .range(smallest..=100.0)
        .speed(0.1)
        .max_decimals(1)
        .suffix(" %")
}

/// Shows the rows for where the clip `clip` (which is on the picture) is at the playhead, with `size_label` for the row of how
/// big its box is, and adds a command if one of them is changed.
pub(super) fn show(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    clip: ClipId,
    size_label: &str,
    commands: &mut Vec<Command>,
) {
    let Some(placement) = content.project.placement_at(clip, content.current_frame) else {
        return;
    };
    let mut changed = placement;
    let smallest_side = CanvasArea::SMALLEST_SIDE_FRACTION * 100.0;

    let position = animatable::row(
        ui,
        content,
        Animatable::of_a_clip(clip, PropertyId::Position),
        commands,
        |ui, _| {
            ui.label("Position");
            let (across, down) = placement.centre();
            let (mut x, mut y) = (across * 100.0, down * 100.0);
            let x_label = ui.label("x");
            let x_changed = ui.add(percent(&mut x, 0.0)).labelled_by(x_label.id).changed();
            let y_label = ui.label("y");
            let y_changed = ui.add(percent(&mut y, 0.0)).labelled_by(y_label.id).changed();
            (x_changed || y_changed).then_some(PropertyValue::Pair(x / 100.0, y / 100.0))
        },
    );
    if let Some(value) = position {
        changed = with_placement_value(changed, PropertyId::Position, &value);
    }

    let size = animatable::row(
        ui,
        content,
        Animatable::of_a_clip(clip, PropertyId::Size),
        commands,
        |ui, _| {
            ui.label(size_label);
            let (mut wide, mut tall) = (placement.area.width() * 100.0, placement.area.height() * 100.0);
            let wide_label = ui.label("w");
            let wide_changed = ui
                .add(percent(&mut wide, smallest_side))
                .labelled_by(wide_label.id)
                .changed();
            let tall_label = ui.label("h");
            let tall_changed = ui
                .add(percent(&mut tall, smallest_side))
                .labelled_by(tall_label.id)
                .changed();
            (wide_changed || tall_changed).then_some(PropertyValue::Pair(wide / 100.0, tall / 100.0))
        },
    );
    if let Some(value) = size {
        changed = with_placement_value(changed, PropertyId::Size, &value);
    }

    let rotation = animatable::row(
        ui,
        content,
        Animatable::of_a_clip(clip, PropertyId::Rotation),
        commands,
        |ui, _| {
            let label = ui.label("Rotation");
            let mut degrees = placement.rotation.degrees();
            // (No limit: the turn is typed, or dragged, as far as it is wanted, which is as many whole turns as a clip is to
            // go round between two keyframes. A turn of a degree for each point of the way the pointer is dragged is
            // a whole turn in 360 points, and Shift held makes it a tenth of that, for exactness.)
            ui.add(DragValue::new(&mut degrees).speed(1.0).max_decimals(1).suffix("°"))
                .labelled_by(label.id)
                .changed()
                .then_some(PropertyValue::Number(degrees))
        },
    );
    if let Some(value) = rotation {
        changed = with_placement_value(changed, PropertyId::Rotation, &value);
    }

    if changed != placement {
        commands.push(Command::SetClipPlacement {
            clip,
            placement: changed,
        });
    }
}
