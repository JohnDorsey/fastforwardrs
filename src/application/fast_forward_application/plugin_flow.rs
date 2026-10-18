// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asking the person about the plugins of a project (phase 7, piece 7.3; the rules are in `plugin_trust`).
//!
//! When a project is opened, the libraries of native code it names or carries are looked at: **each one that has not been answered for is asked about**, one after another, in
//! a window that blocks everything else (UI-12: nothing about plugins is an always-visible control). A linked plugin gets the warning and is remembered by its path and what is
//! in the file; an embedded one gets the severe question every time the project is opened, and is never remembered. A library that cannot be used at all (an embedded one with no
//! build for this computer, a linked one that is not where it was said to be) is said so, once, in a message. **Nothing is loaded here:** the answers are kept for what loads
//! (phase 7.4 and after) to ask; a plugin that was not said yes to is a plugin that is not there.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use super::FastForwardApplication;
use crate::application::dialogs::Dialog;
use crate::plugin_trust::{Answer, ApprovalStore, Question, TrustSession, Unavailable, this_platform, what_is_needed};

/// What the editor knows of who may run which plugin.
#[derive(Default)]
pub(super) struct PluginTrust {
    /// The linked plugins that were approved, by path and by what is in the file; kept in a file.
    pub(super) approvals: ApprovalStore,
    /// What was answered for the project that is open (an embedded plugin's *yes* is only here).
    pub(super) session: TrustSession,
    /// What is still to be asked, in the order of the libraries.
    pub(super) waiting: VecDeque<Question>,
}

/// How a library that cannot be used is said in the message about it.
fn why_it_cannot_be_used(library: &str, why: &Unavailable) -> String {
    match why {
        Unavailable::NoBuildForThisPlatform { platforms } => format!(
            "\u{ab}{library}\u{bb} is inside the project file, with builds for {} only, and this computer is {}.",
            platforms.join(" and "),
            this_platform()
        ),
        Unavailable::FileNotFound { tried } => format!(
            "\u{ab}{library}\u{bb} is not where the project says it is: {}.",
            tried
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" or ")
        ),
        Unavailable::FileUnreadable { path, reason } => {
            format!("\u{ab}{library}\u{bb}, {}, cannot be read: {reason}.", path.display())
        }
        Unavailable::FileTooBig { path, bytes } => {
            format!(
                "\u{ab}{library}\u{bb}, {}, has {bytes} bytes, and a plugin of more than 256 MiB is not used.",
                path.display()
            )
        }
    }
}

impl FastForwardApplication {
    /// Keeps the approvals of linked plugins in the file at `file` (and reads what is in it). Without this they are kept only while the program runs.
    pub fn keep_plugin_approvals_in(&mut self, file: PathBuf) {
        self.environment.plugin_trust.approvals = ApprovalStore::at(file);
        if let Some(problem) = self
            .environment
            .plugin_trust
            .approvals
            .last_problem()
            .map(str::to_owned)
        {
            self.add_to_message("The approved plugins could not be read", problem);
        }
    }

    /// Looks at the plugin libraries of the project that has just been opened, forgets what was answered for the one before, and puts what has to be asked in the queue (the first
    /// window is shown when no other is in the way). What cannot be used at all is said in a message.
    pub(super) fn ask_about_the_plugins_of_the_project(&mut self) {
        self.environment.plugin_trust.session.forget_everything();
        self.environment.plugin_trust.waiting.clear();
        if self.document.project.plugin_libraries().is_empty() {
            return;
        }
        let project_directory: Option<PathBuf> = self
            .document
            .project_file_path
            .as_deref()
            .and_then(Path::parent)
            .and_then(|folder| std::path::absolute(folder).ok());
        let needed = what_is_needed(
            self.document.project.plugin_libraries(),
            project_directory.as_deref(),
            &this_platform(),
            &self.environment.plugin_trust.approvals,
            &self.environment.plugin_trust.session,
        );
        self.environment.plugin_trust.waiting = needed.questions.into();
        if !needed.unavailable.is_empty() {
            let lines: Vec<String> = needed
                .unavailable
                .iter()
                .map(|(library, why)| why_it_cannot_be_used(library, why))
                .collect();
            self.add_to_message(
                if lines.len() == 1 {
                    "A plugin cannot be used"
                } else {
                    "Some plugins cannot be used"
                },
                format!(
                    "{}\n\nWhat uses them passes the sound through, and keeps its settings.",
                    lines.join("\n")
                ),
            );
        }
        self.show_the_next_plugin_question();
    }

    /// Shows the next question about a plugin if there is one and nothing else is being asked.
    pub(super) fn show_the_next_plugin_question(&mut self) {
        if self.interface.dialog.is_some() {
            return;
        }
        if let Some(question) = self.environment.plugin_trust.waiting.pop_front() {
            self.pause_playback_for_dialog();
            self.interface.dialog = Some(Dialog::AskAboutAPlugin { question });
        }
    }

    /// Takes the person's answer to a question about a plugin, and goes on to the next.
    pub(super) fn answer_the_plugin_question(&mut self, question: &Question, answer: Answer) {
        let trust = &mut self.environment.plugin_trust;
        trust.session.answer(question, answer, &mut trust.approvals);
        if let Some(problem) = trust.approvals.last_problem().map(str::to_owned) {
            self.add_to_message("The approved plugins could not be kept", problem);
        }
        self.show_the_next_plugin_question();
    }

    /// The decisions about plugins. (What loads them is phase 7.4 and after; the tests look at them now.)
    #[cfg(test)]
    pub(super) fn plugin_trust(&self) -> &PluginTrust {
        &self.environment.plugin_trust
    }
}
