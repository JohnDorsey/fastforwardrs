// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exporting the finished video, and keeping track of how it is going.

use std::path::PathBuf;
use std::sync::Arc;

use super::{FastForwardApplication, RunningExport};
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::media::MediaLocation;
use crate::project::{EffectKind, Project, SourceMediaIdentifier};
use crate::video_export::{
    ExportEvent, ExportOptions, ExportResult, VideoExport, plan_export, words_about_held_pictures,
};

/// The file types the finished video can be saved as. All of them hold the same H.264 and AAC.
const EXPORT_FILE_EXTENSIONS: [&str; 3] = ["mp4", "mkv", "mov"];

const DEFAULT_EXPORT_EXTENSION: &str = "mp4";

/// What the message at the end of an export says. It says where the video went and, if a file the
/// project is made from is shorter than the one it was edited on, what that cost, and if a video gave fewer pictures than the project was told it has, how many
/// pictures were held in their place.
fn export_finished_text(destination: &std::path::Path, frames_missing_a_picture: u64, pictures_held: u64) -> String {
    let mut text = format!("The video was saved to {}", destination.display());
    if frames_missing_a_picture != 0 {
        text.push_str(&format!(
            "\n\n{frames_missing_a_picture} frames of the project are missing the picture of a video, because a video file \
             the project is made from is not long enough for the clips that reach past its end."
        ));
    }
    if let Some(words) = words_about_held_pictures(pictures_held) {
        text.push_str("\n\n");
        text.push_str(&words);
    }
    text
}

/// The effects that the program does not have (a project file brought them) that are applied to the clips of `project`, so that the video would not have what
/// they do: each once, by its name, its key and its version, in the order of their keys.
pub(crate) fn effects_the_export_would_leave_out(project: &Project) -> Vec<String> {
    let mut missing: Vec<EffectKind> = project
        .effects_applied_to_every_clip()
        .values()
        .flatten()
        .map(|applied| applied.effect.kind())
        .filter(|kind| !kind.is_installed())
        .collect();
    missing.sort();
    missing.dedup();
    let mut left_out: Vec<String> = missing
        .into_iter()
        .map(|kind| {
            let (major, minor) = kind.version();
            format!("{} ({}, version {major}.{minor})", kind.name(), kind.key())
        })
        .collect();
    // (A look-up table effect whose table the project does not have, or whose file is not a table or is missing, does nothing to the picture.)
    left_out.extend(
        project
            .resources_in_use_that_are_missing()
            .into_iter()
            .map(|name| format!("the table {name} (the project does not have it, or it cannot be read)")),
    );
    left_out
}

impl FastForwardApplication {
    /// The source files that the clips of the project are made from and that cannot be used right
    /// now (they are missing, or still being opened).
    pub(super) fn sources_in_use_that_are_not_available(&self) -> Vec<SourceMediaIdentifier> {
        let mut in_use: Vec<SourceMediaIdentifier> = self
            .document
            .project
            .tracks()
            .iter()
            .flat_map(|track| track.entries())
            .filter_map(|entry| entry.clip().source())
            .collect();
        in_use.sort();
        in_use.dedup();
        in_use.retain(|identifier| !self.sources.is_available(*identifier));
        in_use
    }

    /// The effects that the program does not have (a project file brought them) that are applied to the clips of the project, so that the
    /// video would not have what they do: each once, by its name, its key and its version, in the order of their keys.
    pub(super) fn effects_that_the_export_would_leave_out(&self) -> Vec<String> {
        effects_the_export_would_leave_out(&self.document.project)
    }

    /// Asks where to save the finished video and starts exporting it, if the video would have all that the project says; and if it would
    /// not, because it has effects that the program does not have, asks about that first (see [`Dialog::ConfirmExportWithMissingEffects`]).
    pub(super) fn export_video(&mut self) {
        if !self.sources_in_use_that_are_not_available().is_empty() {
            return;
        }
        self.pause_playback_for_dialog();
        let left_out = self.effects_that_the_export_would_leave_out();
        if !left_out.is_empty() {
            self.interface.dialog = Some(Dialog::ConfirmExportWithMissingEffects { effects: left_out });
            return;
        }
        self.ask_where_to_export_and_export();
    }

    /// Asks where to save the finished video and starts exporting it.
    pub(super) fn ask_where_to_export_and_export(&mut self) {
        if let Some(destination) = self.ask_where_to_export() {
            self.export_video_to(destination);
        }
    }

    /// Starts exporting the finished video to `destination`, in the background. A message
    /// reports how it ended; see [`Self::is_working_in_the_background`].
    pub fn export_video_to(&mut self, destination: PathBuf) {
        let Some(toolchain) = self.environment.toolchain.clone() else {
            return;
        };
        if !self.sources_in_use_that_are_not_available().is_empty() {
            return;
        }
        let Some(adapter) = self.environment.export_adapter.clone() else {
            self.show_message(
                "The export could not be started",
                "The export is made by the picture engine, which needs the graphics adapter that the window is drawn with, and there is none.",
            );
            return;
        };
        let settings = self.effective_settings();
        let plan = plan_export(&self.document.project, settings);
        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => {
                self.show_message(error.heading(), error.to_string());
                return;
            }
        };
        let source_file_locations: Vec<MediaLocation> = plan
            .source_files
            .iter()
            .filter_map(|identifier| Some(self.sources.files.get(identifier)?.location_if_available()?.clone()))
            .collect();
        match VideoExport::start(
            &toolchain,
            &plan,
            &source_file_locations,
            &destination,
            ExportOptions::new(adapter),
            Arc::clone(&self.environment.wake_up),
        ) {
            Ok(export) => {
                self.jobs.export = Some(RunningExport {
                    export,
                    frames_written: 0,
                    total_frames: plan.total_frame_count,
                    frames_missing_a_picture: plan.frames_missing_a_picture,
                    destination,
                });
            }
            Err(error) => self.show_message("The export could not be started", error.to_string()),
        }
    }

    fn ask_where_to_export(&self) -> Option<PathBuf> {
        let first_file_name = self
            .document
            .project
            .sources()
            .first()
            .and_then(|source| source.link.absolute_path.file_stem())
            .map_or_else(|| "video".to_owned(), |stem| stem.to_string_lossy().into_owned());
        // Not the name of a source file, which exporting must never overwrite.
        let suggested_file_name = format!("{first_file_name}_fastforward.{DEFAULT_EXPORT_EXTENSION}");
        let path = self.environment.file_chooser.choose_where_to_save(&FileQuestion {
            title: "Export the video",
            suggested_file_name: Some(&suggested_file_name),
            starting_directory: None,
            kinds: &[FileKind {
                name: "Video files",
                extensions: &EXPORT_FILE_EXTENSIONS,
            }],
        })?;
        Some(if path.extension().is_none() {
            path.with_extension(DEFAULT_EXPORT_EXTENSION)
        } else {
            path
        })
    }

    /// Picks up the progress of a running export, and reports how it ended.
    pub(super) fn collect_export_events(&mut self) {
        let Some(running) = &mut self.jobs.export else { return };
        let mut result = None;
        while let Some(event) = running.export.try_next_event() {
            match event {
                ExportEvent::Progress { frames_written } => running.frames_written = frames_written,
                ExportEvent::Finished(finished) => {
                    result = Some(finished);
                    break;
                }
            }
        }
        let Some(result) = result else { return };
        let destination = running.destination.clone();
        let frames_missing_a_picture = running.frames_missing_a_picture;
        let pictures_held = running.export.pictures_held();
        self.jobs.export = None;
        match result {
            ExportResult::Completed => {
                self.show_message(
                    "Export finished",
                    export_finished_text(&destination, frames_missing_a_picture, pictures_held),
                );
            }
            ExportResult::Cancelled => {}
            ExportResult::Failed { reason } => self.show_message("The export failed", reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_message_says_where_the_video_went() {
        let text = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 0, 0);
        assert!(text.starts_with("The video was saved to "), "{text}");
        assert!(text.contains("holiday_fastforward.mp4"), "{text}");
        assert!(!text.contains("missing"), "nothing was left out: {text}");
    }

    #[test]
    fn the_message_says_how_many_frames_are_missing_the_picture_of_a_video_when_there_are_some() {
        let text = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 417, 0);
        assert!(text.contains("holiday_fastforward.mp4"), "{text}");
        assert!(
            text.contains("417 frames of the project are missing the picture of a video"),
            "{text}"
        );
    }

    #[test]
    fn the_message_says_how_many_pictures_were_held_when_some_were_and_nothing_of_it_when_none_were() {
        let none = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 0, 0);
        assert!(!none.contains("not there"), "{none}");
        let some = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 0, 210);
        assert!(some.starts_with("The video was saved to "), "{some}");
        assert!(some.contains("210 pictures of videos were not there"), "{some}");
    }

    #[test]
    fn the_message_says_both_when_frames_are_missing_a_picture_and_pictures_were_held() {
        let both = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 417, 3);
        assert!(
            both.contains("417 frames of the project are missing the picture of a video"),
            "{both}"
        );
        assert!(both.contains("3 pictures of videos were not there"), "{both}");
    }
}
