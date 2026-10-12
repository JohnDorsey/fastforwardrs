//! The box the properties of one thing are in: a rounded box, a little lighter than the panel, with the name
//! of the kind of thing it is at the top. (Under it there will be the button that adds filters, one day.)

use egui::{CornerRadius, Frame, Margin, RichText, Ui};

use crate::application::theme;

/// How far the contents of the box are from its edges, across and up and down, in points. (The corners are
/// as round as those of the clips on the timeline, which takes room.)
const MARGIN_ACROSS: i8 = 18;
const MARGIN_UP_AND_DOWN: i8 = 14;

/// Draws the box as wide as there is room for, with `kind` at the top of it ("Video clip", "Track"...) and
/// then what `add_properties` adds.
pub(super) fn show<R>(ui: &mut Ui, kind: &str, add_properties: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(theme::PROPERTIES_BOX_BACKGROUND)
        .corner_radius(CornerRadius::same(theme::CLIP_CORNER_RADIUS))
        .inner_margin(Margin::symmetric(MARGIN_ACROSS, MARGIN_UP_AND_DOWN))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(kind).strong().size(17.0));
            ui.add_space(6.0);
            add_properties(ui)
        })
        .inner
}
