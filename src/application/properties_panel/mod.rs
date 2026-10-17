// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The properties panel, at the left above the timeline: the settings of what is selected are shown
//! here, and changed here.
//!
//! What is shown depends on what is selected:
//!
//! * nothing: a hint;
//! * one clip or one track: its properties, in a box headed with the kind of thing it is (a video clip,
//!   a sound clip, a picture clip, a text object, a track);
//! * one group: a box with a thick outline in its colour, a bar at its head with its kind, its name and its colour, and the list of what is in
//!   it (see `group_properties`);
//! * several things: their properties cannot be changed together, so they are listed, and clicking one of
//!   them selects just that one.
//!
//! Under the box of a clip, of a track or of a group are its **effects**, with the **badges** among them that say where the effects of the
//! groups it is in (and, for a clip, of its track) are done to it, and the plus button that puts another effect on (see `chain`). What a badge,
//! or an item of the list of what is in a group, stands for is selected with a **double click**, so that taking hold of a badge to move it does
//! not select anything.
//!
//! Each change is a command, as it is everywhere in the window, so that it is an edit like any other
//! (one step of the history for a slider that is dragged).

mod animatable;
mod badge;
mod chain;
mod clip_properties;
mod controls;
mod effects;
mod group_properties;
mod keyframe_buttons;
mod keyframe_properties;
mod kind_box;
mod placement_controls;
mod rack_properties;
mod selection_list;
mod text_properties;
mod track_properties;

// The names the buttons of the keyframes and of the effects are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use effects::{
    ADD_EFFECT_LABEL, CHOOSE_A_TABLE_LABEL, DELETE_EFFECT_LABEL, DUPLICATE_EFFECT_LABEL, NO_TABLE_CHOSEN_NOTE,
    SWITCH_EFFECT_OFF_LABEL, SWITCHED_OFF_NOTE, TABLE_NOT_IN_THE_PROJECT_NOTE,
};
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
            Some(SelectionItem::Group(group)) => group_properties::show(ui, content, group, commands),
            Some(SelectionItem::Keyframe(keyframe)) => keyframe_properties::show(ui, content, keyframe),
            Some(SelectionItem::Rack(rack)) => rack_properties::show(ui, content, rack),
        });
    });
}
