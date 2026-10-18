// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The windows that appear in the middle of the screen: messages, questions, the progress of an
//! export, and the warning that source files must be chosen again.

use egui::{
    Align2, Color32, Context, Id, Key, Modal, ProgressBar, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Widget,
    WidgetInfo, WidgetType, Window, vec2,
};

use super::commands::Command;
use crate::plugin_trust::{HoldToConfirm, Question, SECONDS_TO_HOLD};
use crate::project::SourceMediaIdentifier;

/// What to do once the user has agreed to throw away unsaved changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterConfirmation {
    NewProject,
    LoadProject,
    CloseWindow,
}

/// A window that has to be dealt with before anything else can be done.
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    /// Something to read, closed with "OK".
    Message { title: String, text: String },
    /// A warning that unsaved changes would be lost.
    ConfirmDiscardChanges { then: AfterConfirmation },
    /// A warning that a change would lose `how_many` keyframes, because it makes something last so little time that there is no
    /// frame for them. The `change` is carried out if the user agrees, and not otherwise.
    ConfirmLosingKeyframes { change: Box<Command>, how_many: usize },
    /// A warning that the video that would be exported does not have what some of the effects of the project do, because the program does
    /// not have them (a project file brought them): `effects` says which, each by its name, its key and its version. If the user agrees
    /// the video is exported anyway, and otherwise it is not.
    ConfirmExportWithMissingEffects { effects: Vec<String> },
    /// Files cannot be included in a project file unless it is a `.ffrs` file, and the project is not saved as one: it is asked
    /// where to save it as one, and if the user agrees the `files` are included as it is.
    MustSaveAsAnArchiveToIncludeFiles {
        files: Vec<SourceMediaIdentifier>,
        /// Whether the project has a project file (a plain one), and has not just never been saved.
        has_a_plain_project_file: bool,
    },
    /// A table of colours has been chosen, and the settings say to ask how it is kept: embedded in the project file (`embed`, which is not offered when the project file is plain
    /// JSON and cannot have it, `can_embed`), or linked by its path (`link`). The command that is chosen is carried out.
    HowToKeepATable {
        file_name: String,
        can_embed: bool,
        embed: Box<Command>,
        link: Box<Command>,
    },
    /// Tables of colours cannot be included in a project file unless it is a `.ffrs` file, and the project is not saved as one: it is asked where to save it as one, and if the
    /// user agrees the tables called `names` are included as it is.
    MustSaveAsAnArchiveToIncludeTables {
        names: Vec<String>,
        /// Whether the project has a project file (a plain one), and has not just never been saved.
        has_a_plain_project_file: bool,
    },
    /// A plugin is a program that runs with all the rights of the person who runs the editor: it is about to be run, and the person is asked. `Confirmed` is yes, anything else
    /// is no. A plugin that is a file somewhere is asked about with a warning (once, by its path and what is in the file); one that is inside the project file with the
    /// severe question, which can only be said yes to by holding the button down for five seconds.
    AskAboutAPlugin { question: Question },
    /// A file cannot be embedded in a project file unless it is a `.ffrs` file, and the project is not saved as one: it is asked
    /// where to save it as one, and if the user agrees it is, and then the file to embed is asked for.
    MustSaveAsAnArchiveToEmbedAFile {
        /// Whether the project has a project file (a plain one), and has not just never been saved.
        has_a_plain_project_file: bool,
    },
}

/// What the user did to a dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAnswer {
    /// Nothing yet: the dialog stays.
    StillOpen,
    /// Closed without agreeing to anything.
    Dismissed,
    /// Agreed to what the dialog asked.
    Confirmed,
    /// Chose the other of the two ways that a dialog that has two offers (and a way to cancel).
    ConfirmedTheOtherWay,
}

/// What the questions about a plugin say on their buttons.
pub const PLUGIN_DONT_LOAD_LABEL: &str = "Don't load";
pub const PLUGIN_LOAD_LABEL: &str = "Load it";

/// What the question about a plugin that is a file somewhere says, for the plugin called `library` that is the file `path`.
pub fn warning_about_a_linked_plugin(library: &str, path: &std::path::Path) -> String {
    format!(
        "The project uses the plugin \u{ab}{library}\u{bb}, which is the file {}. A plugin is a program: it runs with all your rights, and nothing checks what it does. \
         Load it only if you know where it came from. If you say yes, that is remembered for this file as it is now; if the file changes you will be asked again.",
        path.display()
    )
}

/// What the severe question about a plugin that is inside the project file says (the words of SF-06 of the features list).
pub fn warning_about_an_embedded_plugin(library: &str, platform: &str) -> String {
    format!(
        "This project contains a program: \u{ab}{library}\u{bb}, for {platform}. It runs with all your rights and nothing checks what it does. \
         Load it only if you trust where this project came from."
    )
}

/// What the question about how a table is kept says on its two buttons.
pub const HOW_TO_KEEP_A_TABLE_EMBED: &str = "Embed in the project file";
pub const HOW_TO_KEEP_A_TABLE_LINK: &str = "Link to the file";

const DIALOG_WIDTH: f32 = 420.0;

/// Shows `dialog` in the middle of the window, over a dimmed backdrop that blocks everything
/// behind it.
pub fn show_dialog(context: &Context, dialog: &Dialog) -> DialogAnswer {
    let mut answer = DialogAnswer::StillOpen;
    let modal = Modal::new(Id::new("dialog")).show(context, |ui| {
        ui.set_width(DIALOG_WIDTH);
        match dialog {
            Dialog::Message { title, text } => {
                ui.heading(title);
                ui.add_space(6.0);
                ui.label(text);
                ui.add_space(12.0);
                if ui.button("OK").clicked() {
                    answer = DialogAnswer::Dismissed;
                }
            }
            Dialog::ConfirmDiscardChanges { .. } => {
                ui.heading("Discard unsaved changes?");
                ui.add_space(6.0);
                ui.label("The project has changes that have not been saved. They will be lost.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Discard changes").clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
            Dialog::MustSaveAsAnArchiveToIncludeTables {
                names,
                has_a_plain_project_file,
            } => {
                ui.heading(if names.len() == 1 {
                    "Save as a .ffrs file to include the table"
                } else {
                    "Save as a .ffrs file to include the tables"
                });
                ui.add_space(6.0);
                ui.label(format!(
                    "Tables can only be included in a project file that is a .ffrs file, and this project is {}.                      Save it as a .ffrs file to include {}?",
                    if *has_a_plain_project_file {
                        "saved as a plain .json file"
                    } else {
                        "not saved yet"
                    },
                    if names.len() == 1 { "it" } else { "them" },
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Choose where to save it").clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
            Dialog::MustSaveAsAnArchiveToIncludeFiles {
                files,
                has_a_plain_project_file,
            } => {
                ui.heading(if files.len() == 1 {
                    "Save as a .ffrs file to include the file"
                } else {
                    "Save as a .ffrs file to include the files"
                });
                ui.add_space(6.0);
                ui.label(format!(
                    "Files can only be included in a project file that is a .ffrs file, and this project is {}. \
                     Save it as a .ffrs file to include {}?",
                    if *has_a_plain_project_file {
                        "saved as a plain .json file"
                    } else {
                        "not saved yet"
                    },
                    if files.len() == 1 { "it" } else { "them" },
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Choose where to save it").clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
            Dialog::MustSaveAsAnArchiveToEmbedAFile {
                has_a_plain_project_file,
            } => {
                ui.heading("Save as a .ffrs file to embed a file");
                ui.add_space(6.0);
                ui.label(format!(
                    "A file can only be embedded in a project file that is a .ffrs file, and this project is {}. \
                     Save it as a .ffrs file first, and then choose the file to embed?",
                    if *has_a_plain_project_file {
                        "saved as a plain .json file"
                    } else {
                        "not saved yet"
                    },
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Choose where to save it").clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
            Dialog::HowToKeepATable {
                file_name,
                can_embed,
                ..
            } => {
                ui.heading("How should the table be kept?");
                ui.add_space(6.0);
                ui.label(format!(
                    "{file_name} can be put inside the project file, so that the project can be moved with everything it uses, or the project can remember where the file is, \
                     as it does for a video.{}",
                    if *can_embed {
                        ""
                    } else {
                        " (The project is saved as a plain .json file, which has no room for it: only linking is possible.)"
                    }
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if *can_embed && ui.button(HOW_TO_KEEP_A_TABLE_EMBED).clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button(HOW_TO_KEEP_A_TABLE_LINK).clicked() {
                        answer = DialogAnswer::ConfirmedTheOtherWay;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
            Dialog::ConfirmExportWithMissingEffects { effects } => {
                ui.heading("Export without some effects?");
                ui.add_space(6.0);
                ui.label(
                    "The project has effects that this program does not have, or tables of colours that it cannot use, so the video would not have what they do. \
                     Their values are kept in the project, and the video can be exported again when the program has them:",
                );
                for effect in effects {
                    ui.label(format!("  - {effect}"));
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Export anyway").clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
            Dialog::AskAboutAPlugin {
                question:
                    Question::Linked {
                        library,
                        path,
                        digest: _,
                    },
            } => {
                ui.heading("Run this plugin?");
                ui.add_space(6.0);
                ui.label(warning_about_a_linked_plugin(library, path));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(PLUGIN_DONT_LOAD_LABEL).clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                    if ui.button(PLUGIN_LOAD_LABEL).clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                });
            }
            Dialog::AskAboutAPlugin {
                question:
                    Question::Embedded {
                        library,
                        platform,
                        size,
                        digest,
                    },
            } => {
                ui.heading(RichText::new("This project contains a program").color(Color32::from_rgb(200, 40, 40)));
                ui.add_space(6.0);
                ui.label(warning_about_an_embedded_plugin(library, platform));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("({size} bytes, SHA-256 {})", digest.short_hex())).weak().small());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(PLUGIN_DONT_LOAD_LABEL).clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                    if HoldDownToConfirm::new(PLUGIN_LOAD_LABEL).ui(ui).changed() {
                        answer = DialogAnswer::Confirmed;
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "To load it, hold the button down with the mouse, or hold the Enter key, for {SECONDS_TO_HOLD:.0} seconds. Letting go starts it over."
                    ))
                    .weak()
                    .small(),
                );
            }
            Dialog::ConfirmLosingKeyframes { change, how_many } => {
                let (keyframes, them, would_be) = if *how_many == 1 {
                    ("1 keyframe".to_owned(), "it", "it would be")
                } else {
                    (format!("{how_many} keyframes"), "them", "they would be")
                };
                let what_would_be_done = change
                    .name_in_a_question_about_losing_keyframes()
                    .unwrap_or("This change");
                ui.heading(format!("Lose {keyframes}?"));
                ui.add_space(6.0);
                ui.label(format!(
                    "{what_would_be_done} leaves no room for {keyframes} in the drawers of what gets shorter, so {would_be} lost. \
                     Every other keyframe stays, closer together where it has to be."
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("Lose {them}")).clicked() {
                        answer = DialogAnswer::Confirmed;
                    }
                    if ui.button("Cancel").clicked() {
                        answer = DialogAnswer::Dismissed;
                    }
                });
            }
        }
    });
    if answer == DialogAnswer::StillOpen && modal.should_close() {
        answer = DialogAnswer::Dismissed;
    }
    answer
}

/// A button that is not clicked but **held down**: with the mouse on it or with the Enter key, for [`SECONDS_TO_HOLD`] without a break, while it fills up. Letting go starts it over.
/// **A click does nothing, and nor does the Space bar** (it presses whatever has the focus, so that it must not be able to say yes to this): only the mouse button that went
/// down on it and the Enter key are looked at. (It senses a drag as well as a click because egui stops calling a press a click after under a second, and a press that is held
/// is a drag until the button goes up.) `changed()` of its response is true in the frame in which the hold is complete.
struct HoldDownToConfirm<'label> {
    label: &'label str,
}

impl<'label> HoldDownToConfirm<'label> {
    fn new(label: &'label str) -> Self {
        Self { label }
    }
}

impl Widget for HoldDownToConfirm<'_> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, mut response) = ui.allocate_exact_size(vec2(140.0, 26.0), Sense::click_and_drag());
        let key_for_the_hold = ui.input(|input| input.key_down(Key::Enter));
        let held = response.is_pointer_button_down_on() || key_for_the_hold;
        let now = ui.input(|input| input.time);
        let mut hold: HoldToConfirm = ui.data(|data| data.get_temp(response.id)).unwrap_or_default();
        let was_confirmed = hold.is_confirmed();
        let progress = hold.update(now, held);
        ui.data_mut(|data| data.insert_temp(response.id, hold));
        if held && !hold.is_confirmed() {
            // (The fill is drawn every frame while it is held, and the time is read again then.)
            ui.ctx().request_repaint();
        }
        if hold.is_confirmed() && !was_confirmed {
            response.mark_changed();
        }
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), self.label));
        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);
            let painter = ui.painter();
            painter.rect(
                rect,
                visuals.corner_radius,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                StrokeKind::Inside,
            );
            let filled = Rect::from_min_size(rect.min, vec2(rect.width() * progress as f32, rect.height()));
            painter.rect(
                filled,
                visuals.corner_radius,
                Color32::from_rgb(200, 40, 40),
                Stroke::NONE,
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                self.label,
                egui::FontId::proportional(14.0),
                visuals.text_color(),
            );
        }
        response
    }
}

/// Shows how far an export has got, with a button to stop it. Returns whether the button was pressed.
pub fn show_export_progress(context: &Context, frames_written: u64, total_frames: u64) -> bool {
    let mut cancel_pressed = false;
    Modal::new(Id::new("export progress")).show(context, |ui| {
        ui.set_width(DIALOG_WIDTH);
        ui.heading("Exporting");
        ui.add_space(8.0);
        let fraction = if total_frames == 0 {
            0.0
        } else {
            (frames_written as f32 / total_frames as f32).clamp(0.0, 1.0)
        };
        ui.add(ProgressBar::new(fraction).show_percentage());
        ui.add_space(12.0);
        if ui.button("Cancel").clicked() {
            cancel_pressed = true;
        }
    });
    cancel_pressed
}

/// Shows how far the copying of files into a project file has got (`copied` of `to_copy` bytes), with a button to stop it. Returns
/// whether the button was pressed.
pub fn show_copy_progress(context: &Context, heading: &str, copied: u64, to_copy: u64) -> bool {
    let mut cancel_pressed = false;
    Modal::new(Id::new("copy progress")).show(context, |ui| {
        ui.set_width(DIALOG_WIDTH);
        ui.heading(heading);
        ui.add_space(8.0);
        let fraction = if to_copy == 0 {
            0.0
        } else {
            (copied as f32 / to_copy as f32).clamp(0.0, 1.0)
        };
        ui.add(ProgressBar::new(fraction).show_percentage());
        ui.add_space(12.0);
        if ui.button("Cancel").clicked() {
            cancel_pressed = true;
        }
    });
    cancel_pressed
}

/// A source file that cannot be used, as the warning about missing files shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingSource {
    pub identifier: SourceMediaIdentifier,
    /// Where the project last had the file.
    pub last_known_path: String,
    /// Why it could not be used.
    pub reason: String,
}

/// Shows, in the middle of the window, that source files must be chosen again. Unlike a dialog it
/// does not block the rest of the program: edits can still be made. Closing it with its close
/// button clears `open`. Returns the file whose "Choose the file again" was pressed, if one was.
pub fn show_missing_sources_warning(
    context: &Context,
    missing: &[MissingSource],
    open: &mut bool,
) -> Option<SourceMediaIdentifier> {
    let mut chosen = None;
    Window::new("Source files not found")
        .id(Id::new("missing source warning"))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .open(open)
        .show(context, |ui| {
            ui.set_width(DIALOG_WIDTH);
            ui.label(if missing.len() == 1 {
                "A file this project is made from can't be found or can't be opened, so it must be chosen again. \
                 Your edits are all still here."
            } else {
                "Some of the files this project is made from can't be found or can't be opened, so they must be \
                 chosen again. Your edits are all still here."
            });
            for source in missing {
                ui.add_space(8.0);
                ui.label(RichText::new(&source.last_known_path).monospace().weak());
                ui.label(RichText::new(&source.reason).weak());
                if ui.button("Choose the file again").clicked() {
                    chosen = Some(source.identifier);
                }
            }
        });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogs_compare_by_what_they_say() {
        let message = Dialog::Message {
            title: "A".into(),
            text: "B".into(),
        };
        assert_eq!(
            message,
            Dialog::Message {
                title: "A".into(),
                text: "B".into()
            }
        );
        assert_ne!(
            message,
            Dialog::ConfirmDiscardChanges {
                then: AfterConfirmation::NewProject
            }
        );
        assert_ne!(
            Dialog::ConfirmDiscardChanges {
                then: AfterConfirmation::NewProject
            },
            Dialog::ConfirmDiscardChanges {
                then: AfterConfirmation::CloseWindow
            }
        );
    }
}
