//! The box the properties of one thing are in: a rounded box, a little lighter than the panel, with the name
//! of the kind of thing it is at the top. (Under it are the boxes of its effects, and the button that adds one: see `chain`.)

use egui::{CornerRadius, Frame, Label, Margin, Rect, Response, RichText, Sense, Ui, UiBuilder, vec2};

use crate::application::theme;

/// How far the contents of the box are from its edges, across and up and down, in points. (The corners are
/// as round as those of the clips on the timeline, which takes room.)
const MARGIN_ACROSS: i8 = 18;
const MARGIN_UP_AND_DOWN: i8 = 14;

/// How tall the row of the heading is, in points.
const HEADING_HEIGHT: f32 = 24.0;

/// Draws the box as wide as there is room for, with `kind` at the top of it ("Video clip", "Track"...) and
/// then what `add_properties` adds.
pub(super) fn show<R>(ui: &mut Ui, kind: &str, add_properties: impl FnOnce(&mut Ui) -> R) -> R {
    show_the_box(ui, kind, None::<fn(&mut Ui)>, add_properties).inner
}

/// A box that has been drawn.
pub(super) struct DrawnBox<R> {
    /// What was made of what the box holds.
    pub inner: R,
    /// The heading, which can be taken hold of (to move the box, see `chain`), and that a menu opens from when it is right-clicked.
    pub heading: Response,
    /// Where the whole box is.
    pub rectangle: Rect,
}

/// Like [`show`], with a menu that opens when the heading is right-clicked, which `add_menu_items` makes the items of, and a heading that
/// can be taken hold of: what is drawn says where the heading and the box are, and what the pointer did to the heading.
pub(super) fn show_with_a_menu<R>(
    ui: &mut Ui,
    kind: &str,
    add_menu_items: impl FnOnce(&mut Ui),
    add_properties: impl FnOnce(&mut Ui) -> R,
) -> DrawnBox<R> {
    show_the_box(ui, kind, Some(add_menu_items), add_properties)
}

fn show_the_box<R>(
    ui: &mut Ui,
    kind: &str,
    add_menu_items: Option<impl FnOnce(&mut Ui)>,
    add_properties: impl FnOnce(&mut Ui) -> R,
) -> DrawnBox<R> {
    let shown = Frame::new()
        .fill(theme::PROPERTIES_BOX_BACKGROUND)
        .corner_radius(CornerRadius::same(theme::CLIP_CORNER_RADIUS))
        .inner_margin(Margin::symmetric(MARGIN_ACROSS, MARGIN_UP_AND_DOWN))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let heading = heading_row(ui, kind, add_menu_items.is_some());
            if let Some(add_menu_items) = add_menu_items {
                heading.context_menu(add_menu_items);
            }
            ui.add_space(6.0);
            (add_properties(ui), heading)
        });
    let (inner, heading) = shown.inner;
    DrawnBox {
        inner,
        heading,
        rectangle: shown.response.rect,
    }
}

/// The row with `kind` in it, as wide as the box: all of it is what the pointer can take hold of when `takes_hold` (and the name of the kind
/// is what a screen reader reads).
fn heading_row(ui: &mut Ui, kind: &str, takes_hold: bool) -> Response {
    let heading = RichText::new(kind).strong().size(17.0);
    if !takes_hold {
        return ui.label(heading);
    }
    // (The constants, and not `Sense::click_and_drag()`, which can take the keyboard focus: then Tab would stop at a heading.)
    let (rectangle, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEADING_HEIGHT), Sense::CLICK | Sense::DRAG);
    let mut row = ui.new_child(UiBuilder::new().max_rect(rectangle));
    row.add(Label::new(heading).selectable(false));
    response
}
