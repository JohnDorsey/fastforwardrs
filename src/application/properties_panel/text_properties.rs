// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The properties of a text object: what it says, its font, the size and colour of its letters, how long
//! it is shown for, and where it is on the picture (see `placement_controls`; it can be moved and resized on the preview too, by
//! dragging it and its corners). What is shown of what it says and how it looks, when there are keyframes for them, is what
//! they are at the playhead.

use egui::{ComboBox, DragValue, TextEdit, Ui};

use super::animatable::{self, Animatable};
use super::{PropertiesContent, kind_box, placement_controls};
use crate::application::commands::Command;
use crate::application::duration_control::shown_for;
use crate::application::naming::kind_name;
use crate::project::{ClipId, ClipKind, FontSize, PropertyId, TextClip, TextColour};

/// How many lines of words show without scrolling.
const LINES_OF_WORDS_SHOWN: usize = 4;

/// The label of the font that is chosen when no font is: the editor's own choice.
const DEFAULT_FONT_LABEL: &str = "Default font";

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
        let at_the_playhead = content
            .project
            .text_object_at(clip, content.current_frame)
            .unwrap_or_else(|| text.object.clone());
        let mut object = at_the_playhead.clone();

        let text_label = ui.label("Text");
        animatable::row(
            ui,
            content,
            Animatable::of_a_clip(clip, PropertyId::Text),
            commands,
            |ui, _| {
                ui.add(
                    TextEdit::multiline(&mut object.text)
                        .desired_rows(LINES_OF_WORDS_SHOWN)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(text_label.id);
            },
        );
        ui.add_space(6.0);

        animatable::row(
            ui,
            content,
            Animatable::of_a_clip(clip, PropertyId::FontFamily),
            commands,
            |ui, _| {
                ui.label("Font");
                match content.font_families {
                    Some(families) => {
                        let shown = if object.font_family.is_empty() {
                            DEFAULT_FONT_LABEL
                        } else {
                            &object.font_family
                        };
                        ComboBox::from_id_salt("text object font")
                            .selected_text(shown)
                            .width((ui.available_width() - 4.0).max(60.0))
                            .height(320.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut object.font_family, String::new(), DEFAULT_FONT_LABEL);
                                for family in families {
                                    ui.selectable_value(&mut object.font_family, family.clone(), family);
                                }
                            });
                    }
                    None => {
                        ui.weak("Looking for the fonts...");
                    }
                }
            },
        );

        animatable::row(
            ui,
            content,
            Animatable::of_a_clip(clip, PropertyId::FontSize),
            commands,
            |ui, _| {
                let label = ui.label("Size");
                let mut points = object.font_size.points();
                let smallest = FontSize::SMALLEST_FRACTION * FontSize::REFERENCE_PICTURE_HEIGHT;
                let largest = FontSize::LARGEST_FRACTION * FontSize::REFERENCE_PICTURE_HEIGHT;
                if ui
                    .add(
                        DragValue::new(&mut points)
                            .range(smallest..=largest)
                            .speed(0.5)
                            .max_decimals(1)
                            .suffix(" pt"),
                    )
                    .labelled_by(label.id)
                    .changed()
                {
                    object.font_size = FontSize::from_points(points);
                }
            },
        );

        animatable::row(
            ui,
            content,
            Animatable::of_a_clip(clip, PropertyId::Colour),
            commands,
            |ui, _| {
                let label = ui.label("Colour");
                let mut rgba = [
                    object.colour.red,
                    object.colour.green,
                    object.colour.blue,
                    object.colour.alpha,
                ];
                if ui
                    .color_edit_button_srgba_unmultiplied(&mut rgba)
                    .labelled_by(label.id)
                    .changed()
                {
                    object.colour = TextColour::new(rgba[0], rgba[1], rgba[2], rgba[3]);
                }
            },
        );

        if let Some(frames) = shown_for(ui, content.project.frame_rate(), text.duration_in_frames()) {
            commands.push(Command::SetClipDuration { clip, frames });
        }

        if object != at_the_playhead {
            commands.push(Command::SetTextObject { clip, object });
        }
        placement_controls::show(ui, content, clip, BOX_SIZE_LABEL, commands);
    });
}
