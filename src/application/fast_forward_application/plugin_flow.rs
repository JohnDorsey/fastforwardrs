// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asking the person about the plugins of a project (phase 7, piece 7.3; the rules are in `plugin_trust`).
//!
//! When a project is opened, the libraries of native code it names or carries are looked at: **each one that has not been answered for is asked about**, one after another, in
//! a window that blocks everything else (UI-12: nothing about plugins is an always-visible control). A linked plugin gets the warning and is remembered by its path and what is
//! in the file; an embedded one gets the severe question every time the project is opened, and is never remembered. A library that cannot be used at all (an embedded one with no
//! build for this computer, a linked one that is not where it was said to be) is said so, once, in a message. **Nothing is loaded here:** the answers are kept for what loads
//! (phase 7.4 and after) to ask; a plugin that was not said yes to is a plugin that is not there.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use super::FastForwardApplication;
use crate::application::dialogs::Dialog;
use crate::plugin_host::{Wanted, folders_the_sound_of_a_project_needs};
use crate::plugin_trust::{
    Answer, ApprovalStore, FolderStanding, Question, TrustSession, Unavailable, folder_for_a_person, this_platform,
    what_is_needed,
};

/// What the editor knows of who may run which plugin.
#[derive(Default)]
pub(super) struct PluginTrust {
    /// The linked plugins that were approved, by path and by what is in the file; kept in a file.
    pub(super) approvals: ApprovalStore,
    /// What was answered for the project that is open (an embedded plugin's *yes* is only here).
    pub(super) session: TrustSession,
    /// What is still to be asked, in the order of the libraries.
    pub(super) waiting: VecDeque<Question>,
    /// An export was asked for and is waiting for the questions about the folders its sound needs to be answered (it goes on when they are).
    pub(super) export_is_waiting: bool,
    /// What the plugins said that a person was told since the sound last stopped, so that it is told once (a sound that is made again says it again).
    pub(super) notes_told: HashSet<String>,
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
        self.environment
            .plugin_host
            .set_approved_folders(self.environment.plugin_trust.approvals.approved_folders());
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
        self.environment.plugin_trust.export_is_waiting = false;
        self.environment.plugin_host.forget_what_was_declined();
        self.environment.plugin_trust.notes_told.clear();
        // (The plugins that the sound of the project uses are asked about when it is opened, and not when the sound first reaches them.)
        self.ask_about_the_folders_the_sound_needs();
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
        // (What runs the plugins is told: the sound is made again with them, or without.)
        if let Question::Linked { folder, .. } = question {
            match answer {
                Answer::Load => self.environment.plugin_host.approve_folder(folder),
                Answer::DontLoad => self.environment.plugin_host.decline_folder(folder),
            }
        }
        self.show_the_next_plugin_question();
        self.go_on_with_the_export_that_was_waiting();
    }

    /// Puts the question about the folder `wanted` in the queue, if nobody has answered for the folder and it is not in the queue or on the screen. Whether nobody has
    /// answered for the folder (whether the question is in the queue or on the screen now, or was put in just now).
    fn ask_about_a_folder(&mut self, wanted: &Wanted) -> bool {
        let trust = &self.environment.plugin_trust;
        if trust.session.standing_of_folder(&trust.approvals, &wanted.folder) != FolderStanding::NotAsked {
            return false;
        }
        let question = Question::linked(&wanted.folder, wanted.libraries.clone());
        let on_the_screen =
            matches!(&self.interface.dialog, Some(Dialog::AskAboutAPlugin { question: shown }) if shown == &question);
        if !on_the_screen && !trust.waiting.contains(&question) {
            self.environment.plugin_trust.waiting.push_back(question);
        }
        true
    }

    /// Asks about the folders that the plugins on the clips of the project are in and nobody has answered for. Whether there are any such folders (whose questions are
    /// in the queue or on the screen).
    pub(super) fn ask_about_the_folders_the_sound_needs(&mut self) -> bool {
        let mut unanswered = false;
        for wanted in folders_the_sound_of_a_project_needs(&self.document.project) {
            unanswered |= self.ask_about_a_folder(&wanted);
        }
        self.show_the_next_plugin_question();
        unanswered
    }

    /// The plugins on the clips of the project that will not be run in the sound of an export now, one line for each folder: it was said no to, or nobody has said yes.
    pub(super) fn plugins_that_an_export_would_not_run(&self) -> Vec<String> {
        let trust = &self.environment.plugin_trust;
        folders_the_sound_of_a_project_needs(&self.document.project)
            .into_iter()
            .filter_map(|wanted| {
                let why = match trust.session.standing_of_folder(&trust.approvals, &wanted.folder) {
                    FolderStanding::Approved => return None,
                    FolderStanding::Declined => "you said no",
                    FolderStanding::NotAsked => "nobody has said yes",
                };
                Some(format!(
                    "The plugins {} in the folder {} were not run, because {why}: the sound goes through them unchanged.",
                    wanted
                        .plugins
                        .iter()
                        .map(|name| format!("\u{ab}{name}\u{bb}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    folder_for_a_person(&wanted.folder),
                ))
            })
            .collect()
    }

    /// Asks about the folders that the sound, as it is made, has come to plugins in that nobody has said anything about (a plugin was put on a clip after the project was
    /// opened, say): it went through them unchanged, and is made again with them if the answer is yes.
    pub(super) fn ask_about_the_folders_that_plugins_are_wanted_from(&mut self) {
        for wanted in self.environment.plugin_host.take_wanted() {
            self.ask_about_a_folder(&wanted);
        }
        self.show_the_next_plugin_question();
    }

    /// Tells the person what the plugins said about the sound that is playing (a plugin that could not be loaded, one that was too slow and was left out), each thing once
    /// while the sound plays.
    pub(super) fn tell_what_the_sound_plugins_said(&mut self) {
        let is_playing = self.playback.clock.is_playing();
        let notes = if is_playing {
            self.playback.audio_supplier.take_notes()
        } else {
            Vec::new()
        };
        self.tell_these_notes(notes, is_playing);
    }

    /// Tells the person `notes`, each once for as long as the sound goes on playing (`is_playing`): a note told is not told again until the sound has stopped.
    pub(super) fn tell_these_notes(&mut self, notes: Vec<String>, is_playing: bool) {
        if !is_playing {
            self.environment.plugin_trust.notes_told.clear();
            return;
        }
        for note in notes {
            if self.environment.plugin_trust.notes_told.insert(note.clone()) {
                self.add_to_message("A sound plugin", note);
            }
        }
    }

    /// An export that was waiting for the answers about plugin folders goes on when they are all given.
    fn go_on_with_the_export_that_was_waiting(&mut self) {
        let trust = &self.environment.plugin_trust;
        if trust.export_is_waiting && trust.waiting.is_empty() && self.interface.dialog.is_none() {
            self.environment.plugin_trust.export_is_waiting = false;
            self.ask_where_to_export_and_export();
        }
    }

    /// The decisions about plugins. (What loads them is phase 7.4 and after; the tests look at them now.)
    #[cfg(test)]
    pub(super) fn plugin_trust(&self) -> &PluginTrust {
        &self.environment.plugin_trust
    }

    /// What runs the plugins in the sound.
    #[cfg(test)]
    pub(super) fn plugin_host(&self) -> &std::sync::Arc<crate::plugin_host::PluginHost> {
        &self.environment.plugin_host
    }
}
