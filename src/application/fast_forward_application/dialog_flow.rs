// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The windows that need an answer or a look from the user: messages, the question about throwing
//! unsaved changes away, the progress of an export, the warning that source files must be chosen
//! again, and the window a text object is edited in.

use egui::{Context, ViewportCommand};

use super::FastForwardApplication;
use super::saving::WhatToDoAboutClosing;
use crate::application::commands::Command;
use crate::application::dialogs::{
    self, AfterConfirmation, Dialog, DialogAnswer, show_copy_progress, show_dialog, show_export_progress,
};

impl FastForwardApplication {
    /// Asks before closing the window if there are unsaved changes. (With autosave on and a file
    /// to save to, they are saved instead, and nothing is asked.)
    pub(super) fn handle_close_request(&mut self, context: &Context) {
        if self.interface.close_when_not_busy && self.jobs.running_save.is_none() && self.interface.dialog.is_none() {
            // (The window was kept open to write the project file again, which is done, or was stopped, or went wrong.)
            self.interface.close_when_not_busy = false;
            context.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        let close_requested = context.input(|input| input.viewport().close_requested());
        if !close_requested {
            return;
        }
        self.autosave_now();
        if self.has_unsaved_changes() && !self.interface.close_confirmed {
            context.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pause_playback_for_dialog();
            self.interface.dialog = Some(Dialog::ConfirmDiscardChanges {
                then: AfterConfirmation::CloseWindow,
            });
        } else if self.tidy_the_project_file_before_closing() == WhatToDoAboutClosing::KeepTheWindowOpen {
            context.send_viewport_cmd(ViewportCommand::CancelClose);
        }
    }

    /// Shows the dialog and the warnings, and acts on what the user does to them. Gives the commands that the user has just
    /// agreed to have carried out, which they were asked about first (see [`Self::carry_out_all`]).
    pub(super) fn show_dialogs(&mut self, context: &Context) -> Vec<Command> {
        let mut agreed_to = Vec::new();
        if let Some(export) = &self.jobs.export
            && show_export_progress(context, export.frames_written, export.total_frames)
        {
            export.export.cancel();
        }
        if let Some((heading, copied, to_copy)) = self.progress_of_the_running_save()
            && show_copy_progress(context, heading, copied, to_copy)
        {
            self.stop_the_running_save();
        }
        if let Some(dialog) = self.interface.dialog.clone() {
            match show_dialog(context, &dialog) {
                DialogAnswer::StillOpen => {}
                DialogAnswer::Dismissed => self.interface.dialog = None,
                DialogAnswer::Confirmed => {
                    self.interface.dialog = None;
                    match dialog {
                        Dialog::ConfirmDiscardChanges { then } => {
                            self.note_everything_is_saved();
                            self.carry_on_after_confirmation(context, then);
                        }
                        Dialog::ConfirmLosingKeyframes { change, .. } => agreed_to.push(*change),
                        Dialog::ConfirmExportWithMissingEffects { .. } => self.ask_where_to_export_and_export(),
                        Dialog::MustSaveAsAnArchiveToIncludeFiles { files, .. } => {
                            self.save_as_an_archive_with_files_included(&files);
                        }
                        Dialog::MustSaveAsAnArchiveToIncludeTables { names, .. } => {
                            self.save_as_an_archive_with_tables_included(&names);
                        }
                        Dialog::MustSaveAsAnArchiveToEmbedAFile { .. } => {
                            self.save_as_an_archive_and_ask_for_a_file_to_embed();
                        }
                        Dialog::HowToKeepATable { embed, .. } => agreed_to.push(*embed),
                        Dialog::Message { .. } => {}
                    }
                }
                DialogAnswer::ConfirmedTheOtherWay => {
                    self.interface.dialog = None;
                    if let Dialog::HowToKeepATable { link, .. } = dialog {
                        agreed_to.push(*link);
                    }
                }
            }
        }
        let missing = self.missing_sources();
        if !missing.is_empty() && self.interface.missing_sources_warning_open {
            let mut open = self.interface.missing_sources_warning_open;
            let chosen = dialogs::show_missing_sources_warning(context, &missing, &mut open);
            self.interface.missing_sources_warning_open = open;
            if let Some(identifier) = chosen {
                self.relink_source(identifier);
            }
        }
        agreed_to
    }

    fn carry_on_after_confirmation(&mut self, context: &Context, then: AfterConfirmation) {
        match then {
            AfterConfirmation::NewProject => self.start_new_project(),
            AfterConfirmation::LoadProject => self.choose_and_load_project(),
            AfterConfirmation::CloseWindow => {
                self.interface.close_confirmed = true;
                context.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    /// Makes room for a message the user has to read, in place of any message already showing.
    pub(super) fn show_message(&mut self, title: impl Into<String>, text: impl Into<String>) {
        self.interface.dialog = Some(Dialog::Message {
            title: title.into(),
            text: text.into(),
        });
    }

    /// Like [`Self::show_message`], but a message already showing stays, with this one added under
    /// it. (For the problems found while the program starts, of which there can be several.)
    pub(super) fn add_to_message(&mut self, title: impl Into<String>, text: impl Into<String>) {
        match &mut self.interface.dialog {
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
