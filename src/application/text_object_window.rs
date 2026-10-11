//! The pop-up window in which a text object is edited: what it says, its font, the size and colour
//! of its letters, and how long it is shown for. (Where it is on the picture is changed on the
//! preview, by dragging it and its corners.)

use egui::{ComboBox, Context, DragValue, Id, TextEdit, Window, pos2};

use super::commands::Command;
use super::duration_control::shown_for;
use crate::project::{ClipLocation, FontSize, FrameRate, TextColour, TextObject};

/// How wide the box for the words is, in points.
const WINDOW_WIDTH: f32 = 340.0;

/// How many lines of words show without scrolling.
const LINES_OF_WORDS_SHOWN: usize = 4;

/// What the window shows.
pub struct TextWindowContent<'a> {
    pub clip: ClipLocation,
    pub object: &'a TextObject,
    /// How long the text is shown for, in frames.
    pub duration_in_frames: u64,
    pub frame_rate: FrameRate,
    /// The fonts the computer has, once they are known (they take a moment to find).
    pub font_families: Option<&'a [String]>,
}

/// The label of the font that is chosen when no font is: the editor's own choice.
const DEFAULT_FONT_LABEL: &str = "Default font";

/// Shows the window, and adds a command for each change made in it. Pressing its close button
/// clears `open`.
pub fn show(context: &Context, content: &TextWindowContent<'_>, open: &mut bool, commands: &mut Vec<Command>) {
    let start = context.content_rect().right_top() + egui::vec2(-WINDOW_WIDTH - 40.0, 56.0);
    Window::new("Text object")
        .id(Id::new("text object window"))
        .default_pos(pos2(start.x, start.y))
        .default_width(WINDOW_WIDTH)
        .resizable(false)
        .collapsible(false)
        .open(open)
        .show(context, |ui| {
            let mut object = content.object.clone();
            let text_label = ui.label("Text");
            ui.add(
                TextEdit::multiline(&mut object.text)
                    .desired_rows(LINES_OF_WORDS_SHOWN)
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(text_label.id);
            ui.add_space(6.0);

            ui.horizontal(|ui| {
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
                            .width(WINDOW_WIDTH - 90.0)
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
            });

            ui.horizontal(|ui| {
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
            });

            ui.horizontal(|ui| {
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
            });

            if let Some(frames) = shown_for(ui, content.frame_rate, content.duration_in_frames) {
                commands.push(Command::SetClipDuration {
                    clip: content.clip,
                    frames,
                });
            }

            if &object != content.object {
                commands.push(Command::SetTextObject {
                    clip: content.clip,
                    object,
                });
            }
        });
}
