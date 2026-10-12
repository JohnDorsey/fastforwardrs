//! The box the properties of one thing are in: a rounded box, a little lighter than the panel, with the name
//! of the kind of thing it is at the top. (Under it are the boxes of its effects, and the button that adds one: see `effects`.)

use egui::{CornerRadius, Frame, Label, Margin, RichText, Sense, Ui};

use crate::application::theme;

/// How far the contents of the box are from its edges, across and up and down, in points. (The corners are
/// as round as those of the clips on the timeline, which takes room.)
const MARGIN_ACROSS: i8 = 18;
const MARGIN_UP_AND_DOWN: i8 = 14;

/// Draws the box as wide as there is room for, with `kind` at the top of it ("Video clip", "Track"...) and
/// then what `add_properties` adds.
pub(super) fn show<R>(ui: &mut Ui, kind: &str, add_properties: impl FnOnce(&mut Ui) -> R) -> R {
    show_the_box(ui, kind, None::<fn(&mut Ui)>, add_properties)
}

/// Like [`show`], with a menu that opens when the heading is right-clicked, which `add_menu_items` makes the items of.
pub(super) fn show_with_a_menu<R>(
    ui: &mut Ui,
    kind: &str,
    add_menu_items: impl FnOnce(&mut Ui),
    add_properties: impl FnOnce(&mut Ui) -> R,
) -> R {
    show_the_box(ui, kind, Some(add_menu_items), add_properties)
}

fn show_the_box<R>(
    ui: &mut Ui,
    kind: &str,
    add_menu_items: Option<impl FnOnce(&mut Ui)>,
    add_properties: impl FnOnce(&mut Ui) -> R,
) -> R {
    Frame::new()
        .fill(theme::PROPERTIES_BOX_BACKGROUND)
        .corner_radius(CornerRadius::same(theme::CLIP_CORNER_RADIUS))
        .inner_margin(Margin::symmetric(MARGIN_ACROSS, MARGIN_UP_AND_DOWN))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let heading = RichText::new(kind).strong().size(17.0);
            match add_menu_items {
                Some(add_menu_items) => {
                    ui.add(Label::new(heading).sense(Sense::click()))
                        .context_menu(add_menu_items);
                }
                None => {
                    ui.label(heading);
                }
            }
            ui.add_space(6.0);
            add_properties(ui)
        })
        .inner
}
