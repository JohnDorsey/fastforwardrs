//! What the panel shows when more than one thing is selected: their properties cannot be changed together,
//! so they are listed, in a yellow outline like the one round a selected clip. Clicking one of them selects
//! just that one, and its properties are shown.

use egui::{Button, CornerRadius, Frame, Margin, Stroke, Ui};

use super::{PropertiesContent, keyframe_properties, track_properties};
use crate::application::commands::Command;
use crate::application::naming::{detail_of_a_clip, kind_name};
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::theme;
use crate::project::format_timecode;

/// What the list says about a selected thing: its kind, and then what tells it from the others of its kind
/// (the name of its file, the beginning of its words, the title of the track), and for a clip where it
/// starts, which tells apart the pieces of one video. `None` for a thing that is not in the project.
pub(super) fn describe(content: &PropertiesContent<'_>, item: SelectionItem) -> Option<String> {
    match item {
        SelectionItem::Track(track) => {
            let track = content.project.track(track)?;
            Some(format!("{}: {}", track_properties::KIND_NAME, track.title()))
        }
        SelectionItem::Keyframe(keyframe) => {
            let (what, when) = keyframe_properties::description(content, keyframe)?;
            Some(format!("{}: {what} (at {when})", keyframe_properties::KIND_NAME))
        }
        SelectionItem::Clip(clip) => {
            let placement = content.project.placement_of(clip)?;
            let detail = detail_of_a_clip(placement.clip, content.source_looks);
            let kind = kind_name(placement.clip.kind());
            let starts = format_timecode(placement.first_timeline_frame, content.project.frame_rate());
            Some(if detail.is_empty() {
                format!("{kind} (at {starts})")
            } else {
                format!("{kind}: {detail} (at {starts})")
            })
        }
    }
}

/// The command that makes just `item` the selection.
fn select_only(item: SelectionItem) -> Command {
    match item {
        SelectionItem::Clip(clip) => Command::select_clips(vec![clip], SelectionChange::Replace),
        SelectionItem::Track(track) => Command::select_track(track, SelectionChange::Replace),
        SelectionItem::Keyframe(_) => Command::SelectItems {
            items: vec![item],
            change: SelectionChange::Replace,
        },
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
