//! The effects of a clip or of a track in the properties panel: a box for each effect it has, under the box of the thing itself, with
//! the controls of its values, and the plus button that puts another effect on.
//!
//! Every effect is a row of one table (see [`crate::project::EffectKind`]): its box is made from what the row says (its name,
//! its values), and the menu of the plus button lists the effects the thing can have and does not have yet. What each value is
//! changed with is in [`control_for`], which has a control for each property that is a value of an effect. A change is a command
//! like any other, so it is an edit that is undone with one step, and a value that has keyframes is changed at the playhead.

use egui::{Popup, Slider, SliderClamping, Ui, pos2, vec2};

use super::PropertiesContent;
use super::animatable::{self, Animatable};
use super::controls::labelled_row;
use super::kind_box;
use crate::application::commands::Command;
use crate::application::naming::what_an_effect_is_called;
use crate::application::plus_button;
use crate::application::theme;
use crate::project::{AnimationTarget, ClipKind, EffectKind, Effects, PropertyId, PropertyValue};

/// What the plus button that puts an effect on is called, which it says when the pointer rests on it, and to screen readers.
pub const ADD_EFFECT_LABEL: &str = "Add an effect";

/// What the item of the menu of a box of an effect that deletes the effect is called.
pub const DELETE_EFFECT_LABEL: &str = "Delete effect";

/// How much room there is round the plus button, above and below it, in points.
const SPACE_ROUND_THE_PLUS_BUTTON: f32 = 12.0;

/// Shows the boxes of the effects that `target` has, and the plus button under them, and adds a command for each change made in
/// them. `clip_kind` is the kind of the clip `target` is (`None` for a track). A thing that no effect can be put on is not
/// given a plus button, as it would have nothing to put on.
pub(super) fn show(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    target: AnimationTarget,
    clip_kind: Option<ClipKind>,
    commands: &mut Vec<Command>,
) {
    let Some(has) = content.project.effects_of(target) else {
        return;
    };
    for effect in has.iter() {
        let kind = effect.kind();
        ui.add_space(8.0);
        let mut to_be_deleted = false;
        kind_box::show_with_a_menu(
            ui,
            &what_an_effect_is_called(kind),
            |ui| {
                if ui.button(DELETE_EFFECT_LABEL).clicked() {
                    to_be_deleted = true;
                    ui.close();
                }
            },
            |ui| {
                for property in kind.properties() {
                    show_the_control_of(ui, content, target, *property, commands);
                }
            },
        );
        if to_be_deleted {
            commands.push(Command::RemoveEffect { target, kind });
        }
    }

    let can_have = |kind: EffectKind| match clip_kind {
        Some(clip_kind) => kind.applies_to_a_clip_of_kind(clip_kind),
        None => kind.applies_to_a_track(),
    };
    if !EffectKind::ALL.into_iter().any(can_have) {
        return;
    }
    let to_offer = effects_to_offer(EffectKind::ALL, can_have, has);
    let room = vec2(
        ui.available_width(),
        plus_button::DIAMETER + 2.0 * SPACE_ROUND_THE_PLUS_BUTTON,
    );
    let (row, _) = ui.allocate_exact_size(room, egui::Sense::hover());
    let button = plus_button::show(
        ui,
        pos2(row.center().x, row.center().y),
        ("add an effect to", target),
        ADD_EFFECT_LABEL,
        ui.is_enabled() && !to_offer.is_empty(),
        theme::PROPERTIES_BOX_BACKGROUND,
    );
    Popup::menu(&button).show(|ui| {
        for kind in &to_offer {
            if ui
                .button(what_an_effect_is_called(*kind))
                .on_hover_text(kind.description())
                .clicked()
            {
                commands.push(Command::AddEffect { target, kind: *kind });
                ui.close();
            }
        }
    });
}

/// The effects among `kinds` that the menu of the plus button offers: those that `can_have` says the thing can have and that it
/// does not have yet, in the order they are in `kinds`.
fn effects_to_offer(
    kinds: impl IntoIterator<Item = EffectKind>,
    can_have: impl Fn(EffectKind) -> bool,
    has: &Effects,
) -> Vec<EffectKind> {
    kinds
        .into_iter()
        .filter(|kind| can_have(*kind) && !has.contains(*kind))
        .collect()
}

/// Shows the row of the control of the value of an effect that `property` is, with the buttons of its keyframes, and adds a command
/// if it is changed.
fn show_the_control_of(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    target: AnimationTarget,
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
        labelled_row(ui, |ui| control_for(ui, property, &shown))
    });
    if let Some(value) = changed {
        commands.push(Command::SetEffectValue {
            target,
            property,
            value,
        });
    }
}

/// The control that changes the value of an effect that `property` is, which shows `shown`: the new value if it was changed. A
/// property that is not a value of an effect has none.
fn control_for(ui: &mut Ui, property: PropertyId, shown: &PropertyValue) -> Option<PropertyValue> {
    match (property, shown) {
        (PropertyId::HueShift, PropertyValue::Number(degrees)) => {
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
    fn the_menu_offers_the_effects_the_thing_can_have_and_has_not_yet_in_the_order_they_are_in() {
        let kinds = EffectKind::ALL;
        let none = Effects::none();
        let mut hue_shifted = Effects::none();
        hue_shifted.add(EffectKind::HueShift);

        assert_eq!(effects_to_offer(kinds, |_| true, &none), kinds.to_vec(), "all of them");
        assert_eq!(
            effects_to_offer(kinds, |_| true, &hue_shifted),
            Vec::<EffectKind>::new(),
            "what it has is not offered again"
        );
        assert_eq!(
            effects_to_offer(kinds, |_| false, &none),
            Vec::<EffectKind>::new(),
            "what it cannot have is not offered"
        );
        assert_eq!(
            effects_to_offer(kinds, |kind| kind != EffectKind::HueShift, &none),
            Vec::<EffectKind>::new(),
            "whichever it is that it cannot have"
        );
        assert_eq!(
            effects_to_offer([EffectKind::HueShift, EffectKind::HueShift], |_| true, &none).len(),
            2,
            "(it offers what it is given: the table has each kind once)"
        );
    }
}
