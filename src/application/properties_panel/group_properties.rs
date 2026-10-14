//! The properties of a group: a box with a thick outline in the colour of the group (the colour of its outline in the timeline), which has
//! at its head a bar in that colour that says what kind of group it is and has its name, which is typed there, and the control of its colour;
//! and under it the list of what is in the group. Under the box is the chain of the effects of the group, as under the box of a clip.
//!
//! The list of what is in the group cannot be put in another order. A member is selected with a double click (as is what a badge stands for,
//! so that nothing is selected by taking hold of a badge), and a right click opens a menu that takes it out of the group, which is refused with
//! a message if that would leave something that is not in a clip group inside its rectangle.
//!
//! The menu of the bar (a right click on it) has *Disband*, which deletes the group and keeps what is in it, and *Delete all*, which deletes
//! the group and everything in it.

use egui::{
    Align, Button, CornerRadius, Frame, Label, Layout, Margin, RichText, Sense, Stroke, TextEdit, Ui, UiBuilder,
    WidgetInfo, WidgetType, vec2,
};

use super::badge::BadgeLook;
use super::selection_list::select_when_double_clicked;
use super::{PropertiesContent, chain};
use crate::application::commands::Command;
use crate::application::naming::describe_a_member;
use crate::application::selection::SelectionItem;
use crate::application::theme;
use crate::project::{Group, GroupColour, GroupId, Member};

/// What the item of the menu of the bar that takes the group apart and keeps what was in it is called.
pub const DISBAND_LABEL: &str = "Disband";

/// What the item of the menu of the bar that deletes the group and what is in it is called.
pub const DELETE_ALL_LABEL: &str = "Delete all";

/// What the item of the menu of a member that takes it out of the group is called.
pub const REMOVE_FROM_GROUP_LABEL: &str = "Remove from group";

/// What the box for the name of the group is said to be to screen readers, and what the control of its colour is called there and when the
/// pointer rests on it.
const NAME_LABEL: &str = "The name of the group";
const COLOUR_LABEL: &str = "The colour of the group";

/// What resting the pointer on a member of the list says.
const SELECTS_THE_MEMBER: &str = "Double-click to select it";

/// How thick the outline of the box is, in points.
const OUTLINE_THICKNESS: f32 = 4.0;

/// How tall the bar at the head of the box is, and how far what is in it is from its edges, in points.
const BAR_HEIGHT: f32 = 44.0;
const BAR_MARGIN_ACROSS: f32 = 18.0;

/// How far the list is from the edges of the box, across and up and down, in points.
const LIST_MARGIN_ACROSS: i8 = 14;
const LIST_MARGIN_UP_AND_DOWN: i8 = 10;

/// How much of the colour of the writing is mixed into the colour of the bar to make the colour of the box for the name, from 0 (none) to 1.
const SHARE_OF_THE_WRITING_IN_THE_NAME_BOX: f32 = 0.2;

/// Shows the properties of `group`, and adds a command for each change made in them.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, group: GroupId, commands: &mut Vec<Command>) {
    let Some(shown) = content.project.group(group) else {
        return;
    };
    let look = BadgeLook::of_a_group(shown);
    Frame::new()
        .fill(theme::PROPERTIES_BOX_BACKGROUND)
        .stroke(Stroke::new(OUTLINE_THICKNESS, look.fill))
        .corner_radius(CornerRadius::same(theme::CLIP_CORNER_RADIUS))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            show_the_bar(ui, shown, look, commands);
            Frame::new()
                .inner_margin(Margin::symmetric(LIST_MARGIN_ACROSS, LIST_MARGIN_UP_AND_DOWN))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    show_the_members(ui, content, shown, commands);
                });
        });
    chain::show(ui, content, Member::Group(group), commands);
}

/// Shows the bar at the head of the box: the kind of the group, its name, which can be typed, and its colour, which can be chosen. A right
/// click on it opens its menu.
fn show_the_bar(ui: &mut Ui, group: &Group, look: BadgeLook, commands: &mut Vec<Command>) {
    // (`Sense::CLICK`, and not `Sense::click()`, which can take the keyboard focus: then Tab would stop at the bar.)
    let (bar, background) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_HEIGHT), Sense::CLICK);
    // (The outline is round, so the bar is round where it meets it: as round as the inside of the outline is.)
    let corner = CornerRadius::same(theme::CLIP_CORNER_RADIUS.saturating_sub(OUTLINE_THICKNESS as u8));
    ui.painter()
        .rect_filled(bar, CornerRadius { sw: 0, se: 0, ..corner }, look.fill);
    background.context_menu(|ui| show_the_menu_of_the_bar(ui, group.id(), commands));

    let mut inside = ui.new_child(
        UiBuilder::new()
            .id_salt(("the bar of group", group.id()))
            .max_rect(bar.shrink2(vec2(BAR_MARGIN_ACROSS, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    // (A label whose words can be selected takes the clicks: this one is not, so that the bar under it gets its menu.)
    inside.add(
        Label::new(
            RichText::new(group.kind().name_with_a_capital())
                .strong()
                .size(17.0)
                .color(look.writing),
        )
        .selectable(false),
    );

    let room_for_the_name =
        inside.available_width() - inside.spacing().interact_size.x - inside.spacing().item_spacing.x;
    let mut name = group.name().to_owned();
    let name_response = inside.add(
        TextEdit::singleline(&mut name)
            .id_salt(("the name box of group", group.id()))
            .desired_width(room_for_the_name.max(0.0))
            .text_color(look.writing)
            .background_color(
                look.fill
                    .lerp_to_gamma(look.writing, SHARE_OF_THE_WRITING_IN_THE_NAME_BOX),
            ),
    );
    name_response.widget_info(|| WidgetInfo::text_edit(true, group.name(), name.clone(), NAME_LABEL));
    name_response.context_menu(|ui| show_the_menu_of_the_bar(ui, group.id(), commands));
    if name != group.name() {
        commands.push(Command::RenameGroup {
            group: group.id(),
            name,
        });
    }

    let colour = group.colour();
    let mut rgb = [colour.red, colour.green, colour.blue];
    let colour_response = inside.color_edit_button_srgb(&mut rgb);
    colour_response.widget_info(|| WidgetInfo::labeled(WidgetType::ColorButton, true, COLOUR_LABEL));
    if colour_response.on_hover_text(COLOUR_LABEL).changed() {
        commands.push(Command::SetGroupColour {
            group: group.id(),
            colour: GroupColour::new(rgb[0], rgb[1], rgb[2]),
        });
    }
}

/// The menu of the bar: take the group apart, or delete it with what is in it.
fn show_the_menu_of_the_bar(ui: &mut Ui, group: GroupId, commands: &mut Vec<Command>) {
    if ui
        .button(DISBAND_LABEL)
        .on_hover_text("Delete the group and keep what is in it")
        .clicked()
    {
        commands.push(Command::DisbandGroup(group));
        ui.close();
    }
    if ui
        .button(DELETE_ALL_LABEL)
        .on_hover_text("Delete the group and everything that is in it")
        .clicked()
    {
        commands.push(Command::DeleteGroupAndEverythingInIt(group));
        ui.close();
    }
}

/// Shows what is in the group, in the order it was put in. Resting on a member says how to select it, and a right click opens the menu that
/// takes it out.
fn show_the_members(ui: &mut Ui, content: &PropertiesContent<'_>, group: &Group, commands: &mut Vec<Command>) {
    for member in group.members() {
        let Some(description) = describe_a_member(content.project, content.source_looks, *member) else {
            continue;
        };
        let row = ui
            .add(Button::new(description).frame(false))
            .on_hover_text(SELECTS_THE_MEMBER);
        select_when_double_clicked(&row, SelectionItem::from(*member), commands);
        row.context_menu(|ui| {
            if ui.button(REMOVE_FROM_GROUP_LABEL).clicked() {
                commands.push(Command::RemoveFromGroup {
                    group: group.id(),
                    member: *member,
                });
                ui.close();
            }
        });
    }
}
