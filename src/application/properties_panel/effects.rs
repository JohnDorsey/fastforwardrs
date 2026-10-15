// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The controls of the values of the effects of a clip, of a track or of a group in the properties panel: the box of each effect (see
//! `chain`, which lays out the boxes with the badges among them and the plus button) has these in it.
//!
//! Every effect is a row of one table (see [`crate::project::EffectKind`]): its box is made from what the row says (its name,
//! its values), and the menu of the plus button lists the effects the thing can have (also those that it has: it may have several of a
//! kind, each with values of its own). What each value is
//! changed with is in [`control_for`], which has a control for each property that is a value of an effect. A change is a command
//! like any other, so it is an edit that is undone with one step, and a value that has keyframes is changed at the playhead. (The values of
//! the effect of a group have no keyframes: they are not animated.)

use egui::{Slider, SliderClamping, Ui};

use super::PropertiesContent;
use super::animatable::{self, Animatable};
use super::controls::labelled_row;
use crate::application::commands::Command;
use crate::project::{AnimationTarget, Effect, EffectKind, GroupId, PropertyId, PropertyValue};

/// What the plus button that puts an effect on is called, which it says when the pointer rests on it, and to screen readers.
pub const ADD_EFFECT_LABEL: &str = "Add an effect";

/// What the item of the menu of a box of an effect that deletes the effect is called.
pub const DELETE_EFFECT_LABEL: &str = "Delete effect";

/// The effects among `kinds` that the menu of the plus button offers: those that `can_have` says the thing can have, in the order they are
/// in `kinds`. (What it has already is offered again: it may have several effects of one kind.)
pub(super) fn effects_to_offer(
    kinds: impl IntoIterator<Item = EffectKind>,
    can_have: impl Fn(EffectKind) -> bool,
) -> Vec<EffectKind> {
    kinds.into_iter().filter(|kind| can_have(*kind)).collect()
}

/// Shows the row of the control of the value of an effect of a clip or a track that `property` is, with the buttons of its keyframes, and
/// adds a command if it is changed.
pub(super) fn show_the_control_of(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    target: AnimationTarget,
    kind: EffectKind,
    property: PropertyId,
    commands: &mut Vec<Command>,
) {
    let Some(shown) = content
        .project
        .property_value_at(target, property, content.current_frame)
    else {
        return;
    };
    let changed = animatable::row(ui, content, Animatable { target, property }, commands, |ui, _| {
        labelled_row(ui, |ui| control_for(ui, kind, property, &shown))
    });
    if let Some(value) = changed {
        commands.push(Command::SetEffectValue {
            target,
            property,
            value,
        });
    }
}

/// Shows the row of the control of the value of the effect `effect` of the group `group` that `property` is, which has no buttons of
/// keyframes (its values are not animated), and adds a command if it is changed.
pub(super) fn show_the_control_of_a_group_effect(
    ui: &mut Ui,
    group: GroupId,
    effect: &Effect,
    property: PropertyId,
    commands: &mut Vec<Command>,
) {
    let Some(shown) = effect.value_of(property) else {
        return;
    };
    if let Some(value) = labelled_row(ui, |ui| control_for(ui, effect.kind(), property, shown)) {
        commands.push(Command::SetGroupEffectValue { group, property, value });
    }
}

/// The control that changes the value of an effect that `property` is, which shows `shown`: the new value if it was changed. A
/// property that is not a value of an effect has none.
fn control_for(ui: &mut Ui, kind: EffectKind, property: PropertyId, shown: &PropertyValue) -> Option<PropertyValue> {
    let PropertyId::EffectValue { slot, .. } = property else {
        return None;
    };
    match (kind, slot, shown) {
        (EffectKind::HueShift, 0, PropertyValue::Number(degrees)) => {
            let mut degrees = *degrees;
            let label = ui.label("Turn");
            // (From half a turn back to half a turn on: that is all the colours there are to turn to. The value can be typed to
            // go further, as a turn is typed, which is how many times the colours go round between two keyframes.)
            let changed = ui
                .add(
                    Slider::new(&mut degrees, -180.0..=180.0)
                        .clamping(SliderClamping::Never)
                        .suffix("°")
                        .max_decimals(1),
                )
                .labelled_by(label.id)
                .changed();
            changed.then_some(PropertyValue::Number(degrees))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_offers_the_effects_the_thing_can_have_in_the_order_they_are_in_and_what_it_has_already_too() {
        // (A thing may have several effects of one kind, so what it has is offered as much as what it has not: the menu is not told what it has.)
        let kinds = EffectKind::ALL;

        assert_eq!(effects_to_offer(kinds, |_| true), kinds.to_vec(), "all of them");
        assert_eq!(
            effects_to_offer(kinds, |_| false),
            Vec::<EffectKind>::new(),
            "what it cannot have is not offered"
        );
        assert_eq!(
            effects_to_offer(kinds, |kind| kind != EffectKind::HueShift),
            Vec::<EffectKind>::new(),
            "whichever it is that it cannot have"
        );
        assert_eq!(
            effects_to_offer([EffectKind::HueShift, EffectKind::HueShift], |_| true).len(),
            2,
            "(it offers what it is given: the table has each kind once)"
        );
    }
}
