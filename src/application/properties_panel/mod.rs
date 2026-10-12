//! The properties panel, at the left above the timeline: the settings of what is selected are shown
//! here, and changed here.
//!
//! What is shown depends on what is selected:
//!
//! * nothing: a hint;
//! * one clip or one track: its properties, in a box headed with the kind of thing it is (a video clip,
//!   a sound clip, a picture clip, a text object, a track);
//! * several things: their properties cannot be changed together, so they are listed, and clicking one of
//!   them selects just that one.
//!
//! Each change is a command, as it is everywhere in the window, so that it is an edit like any other
//! (one step of the history for a slider that is dragged).

mod animatable;
mod clip_properties;
mod controls;
mod keyframe_buttons;
mod kind_box;
mod placement_controls;
mod selection_list;
mod text_properties;
mod track_properties;

// The names the buttons of the keyframes are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use keyframe_buttons::{KeyframeButton, keyframe_button_label};

use std::collections::HashMap;

use egui::{ScrollArea, Ui};

use super::commands::Command;
use super::selection::{Selection, SelectionItem};
use super::timeline_panel::SourceLook;
use crate::project::{Project, SourceMediaIdentifier, TimelineFrameIndex};

/// What the panel says when nothing is selected.
const HINT_WHEN_NOTHING_IS_SELECTED: &str = "Select a clip or a track to see its properties.";

/// What the panel shows.
pub struct PropertiesContent<'a> {
    pub project: &'a Project,
    pub selection: &'a Selection,
    /// The frame the playhead is at: what is shown of a property that has keyframes is what it is there, and changing it is
    /// changing it there.
    pub current_frame: TimelineFrameIndex,
    /// What is known of each of the project's source files: the name of its file, and whether it can be used.
    pub source_looks: &'a HashMap<SourceMediaIdentifier, SourceLook<'a>>,
    /// The fonts the computer has, once they are known (they take a moment to find).
    pub font_families: Option<&'a [String]>,
    /// Whether the properties may be changed now: not while a message is up, or an export is running.
    pub can_edit: bool,
}

/// Shows the panel's contents, and adds a command for each change made in it.
pub fn show(ui: &mut Ui, content: &PropertiesContent<'_>, commands: &mut Vec<Command>) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_enabled_ui(content.can_edit, |ui| match content.selection.single_item() {
            None if content.selection.is_empty() => {
                ui.weak(HINT_WHEN_NOTHING_IS_SELECTED);
            }
            None => selection_list::show(ui, content, commands),
            Some(SelectionItem::Clip(clip)) => clip_properties::show(ui, content, clip, commands),
            Some(SelectionItem::Track(track)) => track_properties::show(ui, content, track, commands),
        });
    });
}
