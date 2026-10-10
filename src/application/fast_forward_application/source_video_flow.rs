//! Finding, opening and replacing the project's source video, and making its thumbnails.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{FastForwardApplication, OpeningPurpose, OpeningTask, SourceStatus, ThumbnailState};
use crate::application::background::{BackgroundTask, TaskStatus};
use crate::application::opening::{OpeningOutcome, open_first_that_works};
use crate::application::theme;
use crate::media::{PictureSize, ThumbnailEvent, ThumbnailExtraction, ThumbnailStrip, probe_video};
use crate::playback::{PreviewSource, preview_picture_size};
use crate::project::{SourceVideo, SourceVideoLink, SourceVideoProperties};
use crate::project_file::relative_path_between;

/// The kinds of file offered first in the dialog that asks for a video.
const VIDEO_FILE_EXTENSIONS: [&str; 12] = [
    "mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "ts", "mts",
];

impl FastForwardApplication {
    /// Asks the user for a video file and uses it as the project's video: it is imported when the
    /// project has none, or replaces the missing one (keeping all the edits).
    pub(super) fn import_video(&mut self) {
        let Some(purpose) = self.purpose_of_importing() else {
            return;
        };
        self.pause_playback_for_dialog();
        if let Some(path) = ask_for_a_video_file() {
            self.start_opening(purpose, vec![path]);
        }
    }

    /// Starts trying `candidates`, in order, in the background.
    pub(super) fn start_opening(&mut self, purpose: OpeningPurpose, candidates: Vec<PathBuf>) {
        let Some(toolchain) = self.toolchain.clone() else {
            self.source_status = match self.source_status {
                SourceStatus::NoVideo => SourceStatus::NoVideo,
                _ => SourceStatus::Unavailable {
                    reason: "ffmpeg could not be found".to_owned(),
                },
            };
            self.show_message(
                "ffmpeg was not found",
                "Videos cannot be opened without ffmpeg. Install it so that both ffmpeg and ffprobe are on your PATH, then start fastforwardrs again.",
            );
            return;
        };
        if matches!(
            purpose,
            OpeningPurpose::ImportIntoEmptyProject | OpeningPurpose::LocateForLoadedProject
        ) {
            self.source_status = SourceStatus::Opening;
        }
        let task = BackgroundTask::start(Arc::clone(&self.wake_up), move || {
            open_first_that_works(&candidates, |path| probe_video(&toolchain, path))
        });
        self.opening = Some(OpeningTask { purpose, task });
    }

    /// Picks up whatever the background work has finished since the last frame.
    pub(super) fn collect_finished_background_work(&mut self) {
        self.collect_opening_result();
        self.collect_thumbnails();
        self.collect_export_events();
    }

    fn collect_opening_result(&mut self) {
        let Some(opening) = &self.opening else { return };
        let purpose = opening.purpose;
        match opening.task.status() {
            TaskStatus::Pending => {}
            TaskStatus::Finished(outcome) => {
                self.opening = None;
                self.finish_opening(purpose, outcome);
            }
            TaskStatus::Lost => {
                self.opening = None;
                self.finish_opening(
                    purpose,
                    OpeningOutcome::Failed {
                        attempts: vec![(
                            PathBuf::new(),
                            "the program that checks videos stopped unexpectedly".to_owned(),
                        )],
                    },
                );
            }
        }
    }

    fn finish_opening(&mut self, purpose: OpeningPurpose, outcome: OpeningOutcome) {
        match (purpose, outcome) {
            (OpeningPurpose::ImportIntoEmptyProject, OpeningOutcome::Opened { path, properties }) => {
                let video = SourceVideo {
                    identifier: self.project.next_source_video_identifier(),
                    link: self.link_to(&path),
                    properties,
                };
                match self.project.import_source_video(video) {
                    Ok(()) => {
                        self.has_unsaved_changes = true;
                        self.clock.reset_for_frame_rate(properties.frame_rate);
                        self.timeline_view.reset();
                        self.source_is_now_available(path, properties);
                    }
                    Err(error) => {
                        self.source_status = SourceStatus::NoVideo;
                        self.show_message("The video could not be imported", error.to_string());
                    }
                }
            }
            (OpeningPurpose::ImportIntoEmptyProject, failure @ OpeningOutcome::Failed { .. }) => {
                self.source_status = SourceStatus::NoVideo;
                self.show_message("The video could not be opened", failure.describe_failure());
            }

            (OpeningPurpose::RelinkMissingVideo, OpeningOutcome::Opened { path, properties }) => {
                let Some(identifier) = self.project.source_videos().first().map(|video| video.identifier) else {
                    return;
                };
                let link = self.link_to(&path);
                match self.project.relink_source_video(identifier, link, properties) {
                    Ok(relinked) => {
                        self.has_unsaved_changes = true;
                        self.source_is_now_available(path, properties);
                        if relinked.total_frame_count_before != relinked.total_frame_count_after {
                            self.show_message(
                                "The video is not the same length",
                                format!(
                                    "The project was edited on a video of {} frames, and this one has {}. All your edits are kept.",
                                    relinked.total_frame_count_before, relinked.total_frame_count_after
                                ),
                            );
                        }
                    }
                    Err(error) => self.show_message("That video cannot be used", error.to_string()),
                }
            }
            (OpeningPurpose::RelinkMissingVideo, failure @ OpeningOutcome::Failed { .. }) => {
                self.show_message("The video could not be opened", failure.describe_failure());
            }

            (OpeningPurpose::LocateForLoadedProject, OpeningOutcome::Opened { path, properties }) => {
                self.finish_locating_for_loaded_project(path, properties);
            }
            (OpeningPurpose::LocateForLoadedProject, failure @ OpeningOutcome::Failed { .. }) => {
                // The warning shows where the video was last known to be, so that is not repeated.
                self.source_status = SourceStatus::Unavailable {
                    reason: failure.describe_failure_for_a_known_place(),
                };
                self.missing_source_warning_open = true;
            }
        }
    }

    /// A video was found for a project that was just loaded. It must run at the frame rate the
    /// edits were made at; if it does, the project keeps everything it knew and only the place
    /// the video was found is remembered.
    fn finish_locating_for_loaded_project(&mut self, path: PathBuf, properties: SourceVideoProperties) {
        let Some(project_video) = self.project.source_videos().first().cloned() else {
            return;
        };
        if project_video.properties.frame_rate != properties.frame_rate {
            self.source_status = SourceStatus::Unavailable {
                reason: format!(
                    "{} runs at {} frames per second, but the project was edited on a video running at {}",
                    path.display(),
                    properties.frame_rate,
                    project_video.properties.frame_rate
                ),
            };
            self.missing_source_warning_open = true;
            return;
        }
        let link = self.link_to(&path);
        // Only the link changes: the project keeps the properties it was edited with.
        let _ = self
            .project
            .relink_source_video(project_video.identifier, link, project_video.properties);
        self.source_is_now_available(path, properties);
    }

    /// The video is usable: show its pictures, and make the thumbnails for the video track.
    fn source_is_now_available(&mut self, path: PathBuf, properties: SourceVideoProperties) {
        self.missing_source_warning_open = false;
        self.textures.clear();
        self.picture_supplier
            .set_source(self.project.source_videos().first().map(|video| {
                let (picture_width_in_pixels, picture_height_in_pixels) =
                    preview_picture_size(properties.display_width_in_pixels, properties.display_height_in_pixels);
                PreviewSource {
                    source_video_identifier: video.identifier,
                    video_path: path.clone(),
                    frame_rate: video.properties.frame_rate,
                    picture_width_in_pixels,
                    picture_height_in_pixels,
                }
            }));
        self.start_thumbnails(&path, properties);
        self.source_status = SourceStatus::Available { path };
    }

    fn start_thumbnails(&mut self, path: &Path, properties: SourceVideoProperties) {
        let seconds_of_video = properties
            .frame_rate
            .seconds_at_frame(properties.total_frame_count)
            .ceil() as usize;
        self.thumbnails = ThumbnailState {
            strip: ThumbnailStrip::with_room_for(seconds_of_video),
            extraction: None,
        };
        let Some(toolchain) = &self.toolchain else { return };
        let size = PictureSize::thumbnail_for(
            properties.display_width_in_pixels,
            properties.display_height_in_pixels,
            theme::THUMBNAIL_HEIGHT_IN_PIXELS,
        );
        match ThumbnailExtraction::start(toolchain, path, size, Arc::clone(&self.wake_up)) {
            Ok(extraction) => self.thumbnails.extraction = Some(extraction),
            Err(error) => self.show_message("The pictures for the video track could not be made", error.to_string()),
        }
    }

    fn collect_thumbnails(&mut self) {
        let Some(extraction) = &self.thumbnails.extraction else {
            return;
        };
        let mut problem = None;
        let mut finished = false;
        while let Some(event) = extraction.try_next_event() {
            match event {
                ThumbnailEvent::Thumbnail { second, picture } => self.thumbnails.strip.insert(second, picture),
                ThumbnailEvent::Finished { error_output } => {
                    problem = error_output;
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.thumbnails.extraction = None;
        }
        if let Some(problem) = problem {
            self.show_message("The pictures for the video track could not all be made", problem);
        }
    }

    /// How a video at `path` is linked to the project: by absolute path, and (once the project has
    /// a file) by a path relative to the project's folder.
    fn link_to(&self, path: &Path) -> SourceVideoLink {
        let absolute_path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let relative_path = self
            .project_file_path
            .as_deref()
            .and_then(Path::parent)
            .and_then(|project_directory| std::path::absolute(project_directory).ok())
            .and_then(|project_directory| relative_path_between(&project_directory, &absolute_path));
        SourceVideoLink {
            absolute_path,
            relative_path,
        }
    }
}

fn ask_for_a_video_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a video")
        .add_filter("Video files", &VIDEO_FILE_EXTENSIONS)
        .add_filter("All files", &["*"])
        .pick_file()
}
