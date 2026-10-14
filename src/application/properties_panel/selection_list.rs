//! What the panel shows when more than one thing is selected: their properties cannot be changed together,
//! so they are listed, in a yellow outline like the one round a selected clip. Clicking one of them selects
//! just that one, and its properties are shown.

use egui::{Button, CornerRadius, Frame, Margin, Response, Stroke, Ui};

use super::{PropertiesContent, keyframe_properties, rack_properties};
use crate::application::commands::Command;
use crate::application::naming::describe_a_member;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::theme;

/// What the list says about a selected thing: its kind, and then what tells it from the others of its kind
/// (the name of its file, the beginning of its words, the title of the track), and for a clip where it
/// starts, which tells apart the pieces of one video. `None` for a thing that is not in the project.
pub(super) fn describe(content: &PropertiesContent<'_>, item: SelectionItem) -> Option<String> {
    match item {
        SelectionItem::Track(_) | SelectionItem::Group(_) | SelectionItem::Clip(_) => {
            describe_a_member(content.project, content.source_looks, item.as_a_member()?)
        }
        SelectionItem::Keyframe(keyframe) => {
            let (what, when) = keyframe_properties::description(content, keyframe)?;
            Some(format!("{}: {what} (at {when})", keyframe_properties::KIND_NAME))
        }
        SelectionItem::Rack(rack) => {
            let (what, how_many) = rack_properties::description(content, rack)?;
            Some(format!("{}: {what} ({how_many})", rack_properties::KIND_NAME))
        }
    }
}

/// The command that makes just `item` the selection. (A click on an item of this list asks for it, and so does a double click on a badge
/// or on an item of the list of what is in a group.)
fn select_only(item: SelectionItem) -> Command {
    Command::SelectItems {
        items: vec![item],
        change: SelectionChange::Replace,
    }
}

/// Adds the command that makes just `item` the selection if `response`, which is what stands for `item` in the panel (a badge, an item of the
/// list of what is in a group), was double-clicked. A click does not select it, so that taking hold of a badge to move it, or clicking by
/// mistake, does not leave what was being looked at.
///
/// (egui calls a click a *triple* click when it is the second of two near each other and another click was made less than twice the time
/// of a double click before it, wherever that one was: so a double click soon after a click somewhere else is reported as a triple click,
/// and is a double click here.)
pub(super) fn select_when_double_clicked(response: &Response, item: SelectionItem, commands: &mut Vec<Command>) {
    if response.double_clicked() || response.triple_clicked() {
        commands.push(select_only(item));
    }
}

/// Shows the list of what is selected, and adds a command when one of them is clicked.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, commands: &mut Vec<Command>) {
    Frame::new()
        .stroke(Stroke::new(2.0, theme::SELECTION_OUTLINE))
        .corner_radius(CornerRadius::same(theme::CLIP_CORNER_RADIUS))
        .inner_margin(Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.weak("Select just one of these to change its properties.");
            ui.add_space(6.0);
            for item in content.selection.items() {
                let Some(description) = describe(content, item) else {
                    continue;
                };
                if ui.add(Button::new(description).frame(false)).clicked() {
                    commands.push(select_only(item));
                }
            }
        });
}
