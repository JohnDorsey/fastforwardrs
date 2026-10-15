// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exporting the finished video, and keeping track of how it is going.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use super::{FastForwardApplication, RunningExport};
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::media::MediaLocation;
use crate::media::export::{ExportEvent, ExportResult, VideoExport, plan_export};
use crate::project::{SourceMediaIdentifier, TextObject};
use crate::rgba_picture::RgbaPicture;
use crate::text_rendering::area_size_in_pixels;

/// The file types the finished video can be saved as. All of them hold the same H.264 and AAC.
const EXPORT_FILE_EXTENSIONS: [&str; 3] = ["mp4", "mkv", "mov"];

const DEFAULT_EXPORT_EXTENSION: &str = "mp4";

/// What the message at the end of an export says. It says where the video went and, if a file the
/// project is made from is shorter than the one it was edited on, what that cost.
fn export_finished_text(destination: &std::path::Path, frames_missing_a_picture: u64) -> String {
    let saved = format!("The video was saved to {}", destination.display());
    if frames_missing_a_picture == 0 {
        saved
    } else {
        format!(
            "{saved}\n\n{frames_missing_a_picture} frames of the project are missing the picture of a video, because a video file \
             the project is made from is not long enough for the clips that reach past its end."
        )
    }
}

/// The picture a text object gets when the fonts could not be loaded: a transparent one, so that
/// the export still finishes (without the text).
fn blank_picture_of_text_area(object: &TextObject, picture_width: u32, picture_height: u32) -> RgbaPicture {
    let (width_in_pixels, height_in_pixels) = area_size_in_pixels(object, picture_width, picture_height);
    RgbaPicture {
        width_in_pixels,
        height_in_pixels,
        rgba_pixels: vec![0; width_in_pixels as usize * height_in_pixels as usize * 4],
    }
}

impl FastForwardApplication {
    /// The source files that the clips of the project are made from and that cannot be used right
    /// now (they are missing, or still being opened).
    pub(super) fn sources_in_use_that_are_not_available(&self) -> Vec<SourceMediaIdentifier> {
        let mut in_use: Vec<SourceMediaIdentifier> = self
            .project
            .tracks()
            .iter()
            .flat_map(|track| track.entries())
            .filter_map(|entry| entry.clip().source())
            .collect();
        in_use.sort();
        in_use.dedup();
        in_use.retain(|identifier| !self.source_is_available(*identifier));
        in_use
    }

    /// Asks where to save the finished video and starts exporting it.
    pub(super) fn export_video(&mut self) {
        if !self.sources_in_use_that_are_not_available().is_empty() {
            return;
        }
        self.pause_playback_for_dialog();
        if let Some(destination) = self.ask_where_to_export() {
            self.export_video_to(destination);
        }
    }

    /// Starts exporting the finished video to `destination`, in the background. A message
    /// reports how it ended; see [`Self::is_working_in_the_background`].
    pub fn export_video_to(&mut self, destination: PathBuf) {
        let Some(toolchain) = self.toolchain.clone() else {
            return;
        };
        if !self.sources_in_use_that_are_not_available().is_empty() {
            return;
        }
        let settings = self.effective_settings();
        let has_text = self
            .project
            .tracks()
            .iter()
            .flat_map(|track| track.entries())
            .any(|entry| entry.clip().as_text().is_some());
        // The fonts are wanted now, if any text is to be drawn: wait for them if they are not ready.
        let text_renderer = has_text
            .then(|| self.text_renderer.get_waiting_if_needed())
            .flatten()
            .map(RefCell::new);
        let plan = plan_export(
            &self.project,
            settings,
            &|object, picture_width, picture_height| match &text_renderer {
                Some(renderer) => renderer.borrow_mut().render(object, picture_width, picture_height),
                None => blank_picture_of_text_area(object, picture_width, picture_height),
            },
        );
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
            .filter_map(|identifier| Some(self.source_files.get(identifier)?.location_if_available()?.clone()))
            .collect();
        match VideoExport::start(
            &toolchain,
            &plan,
            &source_file_locations,
            &destination,
            Arc::clone(&self.wake_up),
        ) {
            Ok(export) => {
                self.export = Some(RunningExport {
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
            .project
            .sources()
            .first()
            .and_then(|source| source.link.absolute_path.file_stem())
            .map_or_else(|| "video".to_owned(), |stem| stem.to_string_lossy().into_owned());
        // Not the name of a source file, which exporting must never overwrite.
        let suggested_file_name = format!("{first_file_name}_fastforward.{DEFAULT_EXPORT_EXTENSION}");
        let path = self.file_chooser.choose_where_to_save(&FileQuestion {
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
        let Some(running) = &mut self.export else { return };
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
        self.export = None;
        match result {
            ExportResult::Completed => {
                self.show_message(
                    "Export finished",
                    export_finished_text(&destination, frames_missing_a_picture),
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
        let text = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 0);
        assert!(text.starts_with("The video was saved to "), "{text}");
        assert!(text.contains("holiday_fastforward.mp4"), "{text}");
        assert!(!text.contains("missing"), "nothing was left out: {text}");
    }

    #[test]
    fn the_message_says_how_many_frames_are_missing_the_picture_of_a_video_when_there_are_some() {
        let text = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 417);
        assert!(text.contains("holiday_fastforward.mp4"), "{text}");
        assert!(
            text.contains("417 frames of the project are missing the picture of a video"),
            "{text}"
        );
    }

    #[test]
    fn a_text_whose_fonts_could_not_be_loaded_is_a_transparent_picture_the_size_of_its_area() {
        let object = TextObject::default();
        let picture = blank_picture_of_text_area(&object, 1000, 500);
        assert_eq!(
            (picture.width_in_pixels, picture.height_in_pixels),
            area_size_in_pixels(&object, 1000, 500)
        );
        assert!(picture.rgba_pixels.iter().all(|byte| *byte == 0));
    }
}
