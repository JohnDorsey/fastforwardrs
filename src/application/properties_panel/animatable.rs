//! A row of the properties panel that has a control for a property that can be animated: the control, and at the right of it
//! (while the pointer is over the row) the buttons that make keyframes and go to them.
//!
//! What the control shows is what the property is at the playhead, which is what its keyframes say if it has any, and the
//! control changes it there: the change is a command like any other, and it is [`FastForwardApplication`]'s to make it a
//! keyframe at the playhead if the property is animated (see `animating` there).
//!
//! [`FastForwardApplication`]: crate::application::FastForwardApplication

use egui::{Rect, Ui, pos2};

use super::PropertiesContent;
use super::keyframe_buttons::{self, KeyframesHere, ROOM_FOR_THE_BUTTONS};
use crate::application::commands::Command;
use crate::project::{AnimationTarget, PropertyId, PropertyValue};

/// The narrowest a control is made to make room for the buttons, in points.
const NARROWEST_CONTROL: f32 = 80.0;

/// Which property of what a row is for.
#[derive(Clone, Copy)]
pub(super) struct Animatable {
    pub target: AnimationTarget,
    pub property: PropertyId,
}

impl Animatable {
    pub fn of_a_clip(clip: crate::project::ClipId, property: PropertyId) -> Self {
        Self {
            target: AnimationTarget::Clip(clip),
            property,
        }
    }

    pub fn of_a_track(track: crate::project::TrackId, property: PropertyId) -> Self {
        Self {
            target: AnimationTarget::Track(track),
            property,
        }
    }
}

/// The number that `property` of `target` is at the playhead, if it has one.
pub(super) fn number_at_the_playhead(
    content: &PropertiesContent<'_>,
    target: AnimationTarget,
    property: PropertyId,
) -> Option<f32> {
    match content
        .project
        .property_value_at(target, property, content.current_frame)
    {
        Some(PropertyValue::Number(number)) => Some(number),
        _ => None,
    }
}

/// The yes or no that `property` of `target` is at the playhead, if it has one.
pub(super) fn flag_at_the_playhead(
    content: &PropertiesContent<'_>,
    target: AnimationTarget,
    property: PropertyId,
) -> Option<bool> {
    match content
        .project
        .property_value_at(target, property, content.current_frame)
    {
        Some(PropertyValue::Flag(flag)) => Some(flag),
        _ => None,
    }
}

/// Lays out the row: `add_control` makes the control, in the width that is left of the row's when the room for the buttons is
/// taken away, and gives what it makes (and gets the list of commands to add to, which the buttons add to as well).
///
/// A property that is animated is not changed where it has no keyframes to put a change in, which is where the playhead is
/// not on its clip: there the control is dimmed.
pub(super) fn row<R>(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    animatable: Animatable,
    commands: &mut Vec<Command>,
    add_control: impl FnOnce(&mut Ui, &mut Vec<Command>) -> R,
) -> R {
    let Animatable { target, property } = animatable;
    let here = KeyframesHere::of(content.project, target, property, content.current_frame);
    let can_be_changed_here = here.can_make_one_here || !content.project.is_animated(target, property);

    let row_left = ui.available_rect_before_wrap().left();
    let row_width = ui.available_width();
    let control = ui.horizontal(|ui| {
        ui.scope(|ui| {
            ui.set_max_width((row_width - ROOM_FOR_THE_BUTTONS).max(NARROWEST_CONTROL));
            ui.add_enabled_ui(can_be_changed_here, |ui| add_control(ui, commands))
                .inner
        })
        .inner
    });

    let row = Rect::from_min_max(
        pos2(row_left, control.response.rect.top()),
        pos2(row_left + row_width, control.response.rect.bottom()),
    );
    let is_hovered = ui.rect_contains_pointer(row);
    keyframe_buttons::show(ui, row, &here, is_hovered, commands);
    control.inner
}
