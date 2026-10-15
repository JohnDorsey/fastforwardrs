// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving the project to a file, and putting files in the project file: where a long copy is done in the background.
//!
//! Writing a project file that is an archive copies the files that are put in it, and a video is gigabytes: a project that is saved as
//! another file, or has a video included in it, copies all of that, which takes as long as copying a file does, and the window must not
//! stop for it. So what is to be written is worked out first (see [`crate::project_file::WritePlan`]), and if it is more than a little
//! to copy it is copied on a thread of its own, with a window that says how far it has got and has a button to stop it (the project
//! file is as it was then). Nothing else can be done while that goes on: it is the project that is being written.
//!
//! What a save that is added to an archive copies is only what is new, so that an autosave of a project with a great deal of video
//! included in it copies nothing (see [`crate::project_file::archive`]). What such saves leave in the file that is no use any more
//! (the text of the project as it was, and the files that were taken out) is taken away by writing the project file again, which is
//! done when it is worth it (see [`crate::project_file::archive::ArchiveState::is_worth_compacting`]), and at moments when that can be:
//!
//! * after the user has saved the project, when no file that is included is read in place (the files that are in the archive are in
//!   other places afterwards, and on Windows a file that is open cannot be replaced), which takes no time then, for what is left in
//!   the archive is only text;
//! * when the window is closed, when every file that is read in place is let go of first (the window stays open for as long as it
//!   takes, with a window that says how far it has got and has a button to give up), and if it is given up, or does not work, the
//!   project file is as it was.
//!
//! Never while the project is being exported, and never by an autosave, which would make every pause in the editing as long as a copy.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{FastForwardApplication, SourceStatus};
use crate::project::Project;
use crate::project_file::archive::EntryPlace;
use crate::project_file::{
    HowToWrite, IncludedFiles, ProjectFileError, WritePlan, Written, plan_the_compaction, plan_the_write,
};

/// How many bytes of files are copied without the window waiting on a thread (and without a window that says how it goes): a few
/// seconds of a video, which a disk copies in a moment.
pub(super) const MOST_BYTES_COPIED_AT_ONCE: u64 = 8 << 20;

/// How often the window is asked to show how far a copy has got.
const TIME_BETWEEN_REPORTS: Duration = Duration::from_millis(50);

/// How far a copy has got, and whether it is to stop. (Shared between the thread that copies and the window.)
struct CopyProgress {
    copied: AtomicU64,
    to_copy: u64,
    stop: AtomicBool,
}

/// For tests: what makes a copy wait until it is let go, so that what the window does while a copy goes on can be looked at (and
/// not only before it has begun, or after it is done).
#[cfg(test)]
pub(super) struct Gate {
    open: AtomicBool,
}

#[cfg(test)]
impl Gate {
    pub(super) fn closed() -> Arc<Self> {
        Arc::new(Self {
            open: AtomicBool::new(false),
        })
    }

    pub(super) fn let_through(&self) {
        self.open.store(true, Ordering::Relaxed);
    }

    /// Waits until the gate is open, or the copy is to stop.
    fn wait(&self, stop: &AtomicBool) {
        while !self.open.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

/// What a job that is done on a thread of its own is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    /// Writing the project to the file that it is to be saved as.
    Saving,
    /// Writing the project file again without the garbage in it, because the window is being closed.
    TidyingBeforeClosing,
}

/// What the window is to do when it has been asked to close, once the project file has been looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WhatToDoAboutClosing {
    /// Close.
    Close,
    /// The project file is being written again, or the user is being told that it could not be: the window closes when that is
    /// done.
    KeepTheWindowOpen,
}

/// A save that is being written on a thread of its own.
pub(super) struct RunningSave {
    progress: Arc<CopyProgress>,
    thread: Option<JoinHandle<()>>,
    outcome: Receiver<Result<Written, ProjectFileError>>,
    destination: PathBuf,
    /// What to put the project back to if the save does not work (for what was done to it so that it would be saved: it was said to
    /// have files included).
    project_to_go_back_to: Option<Project>,
    heading: &'static str,
    purpose: Purpose,
}

impl Drop for RunningSave {
    /// A save that is dropped (the window is closed) stops, and is waited for, which takes a moment, so that the project file is
    /// left as it was.
    fn drop(&mut self) {
        self.progress.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl FastForwardApplication {
    /// Saves the project to `path`, which from then on is the project's file, if that can be done: the project file is an archive
    /// if the name ends in `.ffrs`, with the files that are included in it (a project that has some cannot be saved as anything
    /// else). A project that is saved to the file that it has is added to, which copies none of the files that are in it already.
    /// Says whether it was saved, or is being (a copy of a great deal is done in the background, and says so when it is done); if
    /// it was not, the user is told why, and nothing has changed.
    pub fn save_project_to(&mut self, path: PathBuf) -> bool {
        self.save_the_project(path, None)
    }

    /// [`Self::save_project_to`], and if it does not work the project is put back to `project_to_go_back_to`, if there is one.
    pub(super) fn save_the_project(&mut self, path: PathBuf, project_to_go_back_to: Option<Project>) -> bool {
        if self.jobs.running_save.is_some() {
            self.go_back_to(project_to_go_back_to);
            self.show_message(
                "The project could not be saved",
                "It is being saved already.".to_owned(),
            );
            return false;
        }
        let included_files = match self.included_files_to_save() {
            Ok(files) => files,
            Err(problem) => {
                self.go_back_to(project_to_go_back_to);
                self.show_message("The project could not be saved", problem);
                return false;
            }
        };
        // A project that is saved to the file that it has is added to, which writes none of the files that are included in it again;
        // saved to another file, it is written anew, with all that is included in it.
        let how = if self.document.project_file_path.as_deref() == Some(path.as_path()) {
            HowToWrite::AddingToWhatIsThere
        } else {
            HowToWrite::Anew
        };
        let plan = match plan_the_write(&self.document.project, &path, &included_files, how) {
            Ok(plan) => plan,
            Err(error) => {
                self.go_back_to(project_to_go_back_to);
                self.show_message("The project could not be saved", error.to_string());
                return false;
            }
        };
        if plan.bytes_to_copy() <= self.jobs.most_bytes_copied_at_once {
            match plan.carry_out(&mut |_| Ok(())) {
                Ok(written) => {
                    self.finish_a_save(path, &written);
                    true
                }
                Err(error) => {
                    self.go_back_to(project_to_go_back_to);
                    self.say_that_a_save_did_not_work(&error);
                    false
                }
            }
        } else {
            let heading = if how == HowToWrite::Anew {
                "Saving the project with its files"
            } else {
                "Putting files in the project file"
            };
            self.start_a_job_in_the_background(plan, path, project_to_go_back_to, heading, Purpose::Saving);
            true
        }
    }

    /// Carries out `plan` on a thread of its own, with a window that says how far it has got (see [`Self::progress_of_the_running_save`]).
    fn start_a_job_in_the_background(
        &mut self,
        plan: WritePlan,
        destination: PathBuf,
        project_to_go_back_to: Option<Project>,
        heading: &'static str,
        purpose: Purpose,
    ) {
        let progress = Arc::new(CopyProgress {
            copied: AtomicU64::new(0),
            to_copy: plan.bytes_to_copy(),
            stop: AtomicBool::new(false),
        });
        let (sender, outcome) = mpsc::channel();
        let for_the_thread = Arc::clone(&progress);
        let wake_up = Arc::clone(&self.environment.wake_up);
        #[cfg(test)]
        let gate = self.jobs.copy_gate.clone();
        let thread = thread::spawn(move || {
            let mut last_report = Instant::now();
            let result = plan.carry_out(&mut |copied| {
                for_the_thread.copied.store(copied, Ordering::Relaxed);
                // (For tests: the copy waits once it has copied something, and not at the text of the project that comes first.)
                #[cfg(test)]
                if copied > 0
                    && let Some(gate) = &gate
                {
                    gate.wait(&for_the_thread.stop);
                }
                if for_the_thread.stop.load(Ordering::Relaxed) {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
                }
                if last_report.elapsed() >= TIME_BETWEEN_REPORTS {
                    last_report = Instant::now();
                    wake_up();
                }
                Ok(())
            });
            let _ = sender.send(result);
            wake_up();
        });
        self.pause_playback_for_dialog();
        self.jobs.running_save = Some(RunningSave {
            progress,
            thread: Some(thread),
            outcome,
            destination,
            project_to_go_back_to,
            heading,
            purpose,
        });
    }

    /// Picks up a save that was being written in the background, if it is done.
    pub(super) fn collect_the_running_save(&mut self) {
        let Some(save) = &self.jobs.running_save else {
            return;
        };
        let result = match save.outcome.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(ProjectFileError::CouldNotWrite {
                path: save.destination.clone(),
                source: io::Error::other("the program that writes the project stopped unexpectedly"),
            }),
        };
        let Some(mut save) = self.jobs.running_save.take() else {
            return;
        };
        match (save.purpose, result) {
            (Purpose::Saving, Ok(written)) => {
                let destination = save.destination.clone();
                self.finish_a_save(destination, &written);
            }
            (Purpose::Saving, Err(error)) => {
                let project_to_go_back_to = save.project_to_go_back_to.take();
                self.go_back_to(project_to_go_back_to);
                self.say_that_a_save_did_not_work(&error);
            }
            // (The project file is as it was, or tidier, and the window closes when no message is up.)
            (Purpose::TidyingBeforeClosing, Ok(_)) => {}
            (Purpose::TidyingBeforeClosing, Err(error)) => self.say_that_the_project_file_could_not_be_tidied(&error),
        }
    }

    /// The heading of the save that is being written in the background, and how many bytes it has copied and has to copy.
    pub(super) fn progress_of_the_running_save(&self) -> Option<(&'static str, u64, u64)> {
        let save = self.jobs.running_save.as_ref()?;
        Some((
            save.heading,
            save.progress.copied.load(Ordering::Relaxed),
            save.progress.to_copy,
        ))
    }

    /// Asks the save that is being written in the background to stop. (It does, and says so, in a moment.)
    pub(super) fn stop_the_running_save(&self) {
        if let Some(save) = &self.jobs.running_save {
            save.progress.stop.store(true, Ordering::Relaxed);
        }
    }

    fn go_back_to(&mut self, project: Option<Project>) {
        if let Some(project) = project {
            self.document.project = project;
        }
    }

    fn say_that_the_project_file_could_not_be_tidied(&mut self, error: &ProjectFileError) {
        // (Giving it up is not a failure either.)
        if !matches!(error, ProjectFileError::Stopped) {
            self.show_message(
                "The project file could not be tidied up",
                format!("Nothing is lost: the project file is as it was. {error}"),
            );
        }
    }

    /// What can be done to write the project file again without the garbage in it, if the project file is an archive that has enough
    /// of it to be worth that, and nothing is being done that would be hurt by it.
    fn plan_to_tidy_the_project_file(&self) -> Option<WritePlan> {
        if self.jobs.export.is_some() || self.jobs.running_save.is_some() || !self.project_file_is_an_archive() {
            return None;
        }
        plan_the_compaction(self.document.project_file_path.as_deref()?)
    }

    /// After the user saved the project: writes the project file again without the garbage in it, if that is worth doing, and can be
    /// done at once and with nothing in the way: no file that is included is being read in place (so the files that are in the
    /// archive can be in other places, and the archive be replaced), which also means that there is little to copy.
    pub(super) fn tidy_the_project_file_after_saving(&mut self) {
        if self
            .document
            .project
            .sources()
            .iter()
            .any(|source| source.link.is_included())
        {
            return;
        }
        let Some(plan) = self.plan_to_tidy_the_project_file() else {
            return;
        };
        if let Err(error) = plan.carry_out(&mut |_| Ok(())) {
            self.show_message(
                "The project was saved, but its file could not be tidied up",
                format!("Nothing is lost: the project file is as it was. {error}"),
            );
        }
    }

    /// When the window is to close and nothing is left to ask: writes the project file again without the garbage in it, if that is
    /// worth doing (once). Every file that is read in place is let go of first, as the project file is in other places afterwards (and
    /// on Windows cannot be replaced while one is open). If it is more than a little to copy, it is done in the background, with a
    /// window that says how far it has got and a button to give it up (and the window closes when it is done).
    pub(super) fn tidy_the_project_file_before_closing(&mut self) -> WhatToDoAboutClosing {
        if self.jobs.tidying_before_closing_was_tried {
            return WhatToDoAboutClosing::Close;
        }
        self.jobs.tidying_before_closing_was_tried = true;
        let (Some(plan), Some(archive)) = (
            self.plan_to_tidy_the_project_file(),
            self.document.project_file_path.clone(),
        ) else {
            return WhatToDoAboutClosing::Close;
        };
        self.let_go_of_every_file_that_is_read();
        self.interface.close_when_not_busy = true;
        if plan.bytes_to_copy() > self.jobs.most_bytes_copied_at_once {
            self.start_a_job_in_the_background(
                plan,
                archive,
                None,
                "Tidying up the project file",
                Purpose::TidyingBeforeClosing,
            );
            return WhatToDoAboutClosing::KeepTheWindowOpen;
        }
        match plan.carry_out(&mut |_| Ok(())) {
            Ok(_) => {
                self.interface.close_when_not_busy = false;
                WhatToDoAboutClosing::Close
            }
            Err(error) => {
                // (The window stays open until it has been read.)
                self.say_that_the_project_file_could_not_be_tidied(&error);
                WhatToDoAboutClosing::KeepTheWindowOpen
            }
        }
    }

    /// Stops everything that reads the files of the project, and forgets what was made from them: the pictures and the sound that are
    /// supplied, the thumbnails and waveforms. (For the project file to be written again; the project itself is not changed. What
    /// was forgotten is made again when the project is loaded.)
    fn let_go_of_every_file_that_is_read(&mut self) {
        self.sources.openings.clear();
        self.sources.files.clear();
        self.playback.preview_pictures.set_setup(None);
        self.interface.let_go_of_the_pictures_of_files();
        self.interface.missing_sources_warning_open = false;
        let now = Instant::now();
        let total_frames = self.document.project.total_frame_count();
        let clock = self.playback.reading(now, total_frames);
        self.playback.audio_supplier.update(None, clock, now);
    }

    fn say_that_a_save_did_not_work(&mut self, error: &ProjectFileError) {
        // (A save that was stopped was stopped on purpose: it is not a failure to be told of.)
        if !matches!(error, ProjectFileError::Stopped) {
            self.show_message("The project could not be saved", error.to_string());
        }
    }

    /// The project was written to `path`, which is its file from now on.
    fn finish_a_save(&mut self, path: PathBuf, written: &Written) {
        self.document.project_file_path = Some(path.clone());
        self.note_everything_is_saved();
        self.read_the_included_files_where_they_are_in(&path, written);
    }

    /// Where the bytes of each source file that is included in the project file are, to be put in it again when the project is saved:
    /// where they are in the archive that the project was loaded from or last saved as, or where the file is if it was included just
    /// now. The files that are included must all be found and opened for that.
    fn included_files_to_save(&self) -> Result<IncludedFiles, String> {
        let mut files = IncludedFiles::new();
        for source in self
            .document
            .project
            .sources()
            .iter()
            .filter(|source| source.link.is_included())
        {
            let name = source.link.absolute_path.file_name().map_or_else(
                || source.identifier.to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            match self.sources.files.get(&source.identifier).map(|state| &state.status) {
                Some(SourceStatus::Available { location }) => {
                    files.insert(source.identifier, location.clone());
                }
                Some(SourceStatus::Opening) => {
                    return Err(format!(
                        "The file {name} is included in the project file, and is still being opened. Save the project again in a moment."
                    ));
                }
                Some(SourceStatus::Unavailable { reason }) => {
                    return Err(format!(
                        "The file {name} is included in the project file, and cannot be used just now ({reason}), so it cannot be written to the project file again."
                    ));
                }
                None => {
                    return Err(format!(
                        "The file {name} is included in the project file, and has not been opened yet."
                    ));
                }
            }
        }
        Ok(files)
    }

    /// After the project was written to the archive `path`: the files that are included are read from where they are in it, which for
    /// those that were written just now is not where they were read from (the files that were there already, in an archive that was
    /// added to, have not moved, and are not touched).
    fn read_the_included_files_where_they_are_in(&mut self, path: &Path, written: &Written) {
        let Some(archive) = &written.archive else {
            return;
        };
        let archive_path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        for source in self.document.project.sources() {
            let Some(entry_name) = &source.link.included_as else {
                continue;
            };
            let Some(location) = archive
                .entries
                .get(entry_name)
                .and_then(|place: &EntryPlace| place.as_a_location_in(&archive_path))
            else {
                continue;
            };
            if let Some(state) = self.sources.files.get_mut(&source.identifier)
                && matches!(state.status, SourceStatus::Available { .. })
                && state.location_if_available() != Some(&location)
            {
                state.status = SourceStatus::Available { location };
            }
        }
    }
}
