//! What the panel shows when more than one thing is selected: their properties cannot be changed together,
//! so they are listed, in a yellow outline like the one round a selected clip. Clicking one of them selects
//! just that one, and its properties are shown.

use egui::{Button, CornerRadius, Frame, Margin, Stroke, Ui};

use super::PropertiesContent;
use super::clip_properties::kind_name;
use super::track_properties;
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::theme;
use crate::project::{Clip, format_timecode};

/// How many letters of the words of a text object are shown to tell it from the others.
const MOST_LETTERS_OF_WORDS: usize = 24;

/// What the list says about a selected thing: its kind, and then what tells it from the others of its kind
/// (the name of its file, the beginning of its words, the title of the track), and for a clip where it
/// starts, which tells apart the pieces of one video. `None` for a thing that is not in the project.
pub(super) fn describe(content: &PropertiesContent<'_>, item: SelectionItem) -> Option<String> {
    match item {
        SelectionItem::Track(track) => {
            let track = content.project.track(track)?;
            Some(format!("{}: {}", track_properties::KIND_NAME, track.title()))
        }
        SelectionItem::Clip(clip) => {
            let placement = content.project.placement_of(clip)?;
            let detail = match placement.clip {
                Clip::Text(text) => beginning_of(&text.object.text),
                other => other
                    .source()
                    .and_then(|source| content.source_looks.get(&source))
                    .map(|look| look.file_name.clone())
                    .unwrap_or_default(),
            };
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

/// The first line of `words`, cut short if it is long.
fn beginning_of(words: &str) -> String {
    let first_line = words.lines().next().unwrap_or("").trim();
    if first_line.chars().count() > MOST_LETTERS_OF_WORDS {
        let cut: String = first_line.chars().take(MOST_LETTERS_OF_WORDS).collect();
        format!("{}...", cut.trim_end())
    } else {
        first_line.to_owned()
    }
}

/// The command that makes just `item` the selection.
fn select_only(item: SelectionItem) -> Command {
    match item {
        SelectionItem::Clip(clip) => Command::Select {
            clips: vec![clip],
            change: SelectionChange::Replace,
        },
        SelectionItem::Track(track) => Command::SelectTrack {
            track,
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
