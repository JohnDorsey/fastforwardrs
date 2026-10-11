//! The windows that appear in the middle of the screen: messages, questions, the progress of an
//! export, and the warning that source files must be chosen again.

use egui::{Align2, Context, Id, Modal, ProgressBar, RichText, Window, vec2};

use crate::project::SourceMediaIdentifier;

/// What to do once the user has agreed to throw away unsaved changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterConfirmation {
    NewProject,
    LoadProject,
    CloseWindow,
}

/// A window that has to be dealt with before anything else can be done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// Something to read, closed with "OK".
    Message { title: String, text: String },
    /// A warning that unsaved changes would be lost.
    ConfirmDiscardChanges { then: AfterConfirmation },
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
}

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
        }
    });
    if answer == DialogAnswer::StillOpen && modal.should_close() {
        answer = DialogAnswer::Dismissed;
    }
    answer
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
