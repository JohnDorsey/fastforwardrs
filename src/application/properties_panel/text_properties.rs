// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The properties of a text object: what it says, its font, the size and colour of its letters, how long it is shown for, and where it is on the picture (see `placement_controls`; it can be
//! moved and resized on the preview too, by dragging it and its corners). What is shown of what it says and how it looks, when there are keyframes for them, is what they are at the playhead.
//!
//! **What it says, its font, and the size and colour of its letters are the values of its text effect**, the content of the clip, and their rows are the rows that any effect's values have
//! (see `effects`, which draws them with the controls that the text effect says its values have, and keyframes of their own): they are shown here, in the box of the text, where they
//! always were, and not in a box of the chain of the clip (see `chain`, which leaves that one out).

use egui::Ui;

use super::effects::show_the_control_of;
use super::{PropertiesContent, kind_box, placement_controls};
use crate::application::commands::Command;
use crate::application::duration_control::shown_for;
use crate::application::naming::kind_name;
use crate::project::{AnimationTarget, ClipId, ClipKind, EffectKind, TextClip, properties_of_the_text_effect};

/// What the row of how big a text object's box is is called (its letters have a size of their own).
const BOX_SIZE_LABEL: &str = "Box size";

/// Shows the properties of the text object of `clip`, and adds a command for each change made in them.
pub(super) fn show(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    clip: ClipId,
    text: &TextClip,
    commands: &mut Vec<Command>,
) {
    kind_box::show(ui, kind_name(ClipKind::Text), |ui| {
        let [text_property, font_property, size_property, colour_property] =
            properties_of_the_text_effect(text.content());
        let target = AnimationTarget::Clip(clip);
        show_the_control_of(ui, content, target, EffectKind::TEXT, text_property, commands);
        ui.add_space(6.0);
        for property in [font_property, size_property, colour_property] {
            show_the_control_of(ui, content, target, EffectKind::TEXT, property, commands);
        }
        if let Some(frames) = shown_for(ui, content.project.frame_rate(), text.duration_in_frames()) {
            commands.push(Command::SetClipDuration { clip, frames });
        }
        placement_controls::show(ui, content, clip, BOX_SIZE_LABEL, commands);
    });
}
