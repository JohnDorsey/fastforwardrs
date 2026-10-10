//! Exporting the finished video, and keeping track of how it is going.

use std::path::PathBuf;
use std::sync::Arc;

use super::{FastForwardApplication, RunningExport, SourceStatus};
use crate::media::export::{ExportEvent, ExportResult, VideoExport, plan_export};

/// The file types the finished video can be saved as. All of them hold the same H.264 and AAC.
const EXPORT_FILE_EXTENSIONS: [&str; 3] = ["mp4", "mkv", "mov"];

const DEFAULT_EXPORT_EXTENSION: &str = "mp4";

/// What the message at the end of an export says. It says where the video went and, if the video
/// the project is pointed at is shorter than the one it was edited on, what that cost.
fn export_finished_text(destination: &std::path::Path, frames_left_out: u64) -> String {
    let saved = format!("The video was saved to {}", destination.display());
    if frames_left_out == 0 {
        saved
    } else {
        format!(
            "{saved}\n\n{frames_left_out} frames of the project are not in it: the source video is not long enough \
             for the clips that reach past its end."
        )
    }
}

impl FastForwardApplication {
    /// Asks where to save the finished video and starts exporting it.
    pub(super) fn export_video(&mut self) {
        let SourceStatus::Available {
            path: source_video_path,
        } = &self.source_status
        else {
            return;
        };
        let source_video_path = source_video_path.clone();
        self.pause_playback_for_dialog();
        if let Some(destination) = self.ask_where_to_export(&source_video_path) {
            self.export_video_to(destination);
        }
    }

    /// Starts exporting the finished video to `destination`, in the background. A message
    /// reports how it ended; see [`Self::is_working_in_the_background`].
    pub fn export_video_to(&mut self, destination: PathBuf) {
        let SourceStatus::Available {
            path: source_video_path,
        } = &self.source_status
        else {
            return;
        };
        let source_video_path = source_video_path.clone();
        let Some(toolchain) = self.toolchain.clone() else {
            return;
        };
        let plan = match plan_export(&self.project, self.effective_settings()) {
            Ok(plan) => plan,
            Err(error) => {
                self.show_message("There is nothing to export", error.to_string());
                return;
            }
        };
        match VideoExport::start(
            &toolchain,
            &plan,
            &source_video_path,
            &destination,
            Arc::clone(&self.wake_up),
        ) {
            Ok(export) => {
                self.export = Some(RunningExport {
                    export,
                    frames_written: 0,
                    total_frames: plan.total_frame_count,
                    frames_left_out: plan.frames_left_out_past_the_end_of_the_source,
                    destination,
                });
            }
            Err(error) => self.show_message("The export could not be started", error.to_string()),
        }
    }

    fn ask_where_to_export(&self, source_video_path: &std::path::Path) -> Option<PathBuf> {
        let video_name = source_video_path
            .file_stem()
            .map_or_else(|| "video".to_owned(), |stem| stem.to_string_lossy().into_owned());
        let path = rfd::FileDialog::new()
            .set_title("Export the video")
            // Not the name of the source video, which exporting must never overwrite.
            .set_file_name(format!("{video_name}_fastforward.{DEFAULT_EXPORT_EXTENSION}"))
            .add_filter("Video files", &EXPORT_FILE_EXTENSIONS)
            .save_file()?;
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
        let frames_left_out = running.frames_left_out;
        self.export = None;
        match result {
            ExportResult::Completed => {
                self.show_message("Export finished", export_finished_text(&destination, frames_left_out));
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
        assert!(!text.contains("not in it"), "nothing was left out: {text}");
    }

    #[test]
    fn the_message_says_how_many_frames_were_left_out_when_there_were_some() {
        let text = export_finished_text(Path::new("/videos/holiday_fastforward.mp4"), 417);
        assert!(text.contains("holiday_fastforward.mp4"), "{text}");
        assert!(text.contains("417 frames of the project are not in it"), "{text}");
    }
}
