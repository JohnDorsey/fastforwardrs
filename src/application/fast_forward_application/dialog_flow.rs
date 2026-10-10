//! The windows that need an answer or a look from the user: messages, the question about throwing
//! unsaved changes away, the progress of an export, and the warning that the source video must be
//! chosen again.

use egui::{Context, ViewportCommand};

use super::{FastForwardApplication, SourceStatus};
use crate::application::dialogs::{
    self, AfterConfirmation, Dialog, DialogAnswer, MissingSourceWarning, show_dialog, show_export_progress,
};

impl FastForwardApplication {
    /// Asks before closing the window if there are unsaved changes. (With autosave on and a file
    /// to save to, they are saved instead, and nothing is asked.)
    pub(super) fn handle_close_request(&mut self, context: &Context) {
        let close_requested = context.input(|input| input.viewport().close_requested());
        if !close_requested {
            return;
        }
        self.autosave_now();
        if self.has_unsaved_changes && !self.close_confirmed {
            context.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pause_playback_for_dialog();
            self.dialog = Some(Dialog::ConfirmDiscardChanges {
                then: AfterConfirmation::CloseWindow,
            });
        }
    }

    /// Shows the dialog and the warnings, and acts on what the user does to them.
    pub(super) fn show_dialogs(&mut self, context: &Context) {
        if let Some(export) = &self.export
            && show_export_progress(context, export.frames_written, export.total_frames)
        {
            export.export.cancel();
        }
        if let Some(dialog) = self.dialog.clone() {
            match show_dialog(context, &dialog) {
                DialogAnswer::StillOpen => {}
                DialogAnswer::Dismissed => self.dialog = None,
                DialogAnswer::Confirmed => {
                    self.dialog = None;
                    if let Dialog::ConfirmDiscardChanges { then } = dialog {
                        self.note_everything_is_saved();
                        self.carry_on_after_confirmation(context, then);
                    }
                }
            }
        }
        if let SourceStatus::Unavailable { reason } = &self.source_status {
            let warning = MissingSourceWarning {
                last_known_path: self
                    .project
                    .source_videos()
                    .first()
                    .map(|video| video.link.absolute_path.display().to_string())
                    .unwrap_or_default(),
                reason: reason.clone(),
            };
            let mut open = self.missing_source_warning_open;
            let choose = dialogs::show_missing_source_warning(context, &warning, &mut open);
            self.missing_source_warning_open = open;
            if choose {
                self.import_video();
            }
        }
    }

    fn carry_on_after_confirmation(&mut self, context: &Context, then: AfterConfirmation) {
        match then {
            AfterConfirmation::NewProject => self.start_new_project(),
            AfterConfirmation::LoadProject => self.choose_and_load_project(),
            AfterConfirmation::CloseWindow => {
                self.close_confirmed = true;
                context.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    /// Makes room for a message the user has to read, in place of any message already showing.
    pub(super) fn show_message(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.dialog = Some(Dialog::Message {
            title: title.into(),
            text: text.into(),
        });
    }

    /// Like [`Self::show_message`], but a message already showing stays, with this one added under
    /// it. (For the problems found while the program starts, of which there can be several.)
    pub(super) fn add_to_message(&mut self, title: impl Into<String>, text: impl Into<String>) {
        match &mut self.dialog {
            Some(Dialog::Message {
                title: shown_title,
                text: shown_text,
            }) => {
                shown_title.push_str(" / ");
                shown_title.push_str(&title.into());
                shown_text.push_str("\n\n");
                shown_text.push_str(&text.into());
            }
            _ => self.show_message(title, text),
        }
    }
}
