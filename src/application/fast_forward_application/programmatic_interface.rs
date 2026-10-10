//! What the editor can be asked to do, and asked about, without anyone using its window: the work
//! of the File menu without its file dialogs, and a look at the state afterwards.
//!
//! The window itself uses the same work (the dialogs just choose the file first), so this is also
//! how the tests run whole journeys through the program, and how a command line or scripting
//! feature could later drive it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{FastForwardApplication, OpeningPurpose, SourceStatus};
use crate::playback::SuppliedPicture;
use crate::project::{Project, TimelineFrameIndex};

impl FastForwardApplication {
    /// Uses the video at `path` as the project's video, as choosing it in the Import dialog does:
    /// it is imported when the project has none, or replaces the missing one (keeping all the
    /// edits). Does nothing while the project's video is working.
    ///
    /// The video is opened in the background; see [`Self::is_working_in_the_background`].
    pub fn import_video_from(&mut self, path: PathBuf) {
        if let Some(purpose) = self.purpose_of_importing() {
            self.start_opening(purpose, vec![path]);
        }
    }

    /// Loads the project file at `path`, as choosing it in the Load project dialog does.
    pub fn load_project_from(&mut self, path: PathBuf) {
        self.load_project(path);
    }

    /// Starts a new, empty project, as the New project menu item does before it asks for a video.
    pub fn start_empty_project(&mut self) {
        self.replace_project(Project::empty(), None);
    }

    /// The project being edited.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Whether the project has changes that have not been saved (there is no autosave).
    pub fn has_unsaved_changes(&self) -> bool {
        self.has_unsaved_changes
    }

    /// The file the project was last saved to or loaded from.
    pub fn project_file_path(&self) -> Option<&Path> {
        self.project_file_path.as_deref()
    }

    /// Whether the project's video has been found and opens.
    pub fn source_video_is_available(&self) -> bool {
        matches!(self.source_status, SourceStatus::Available { .. })
    }

    /// Whether the warning that the source video must be chosen again is showing.
    pub fn missing_source_warning_is_showing(&self) -> bool {
        matches!(self.source_status, SourceStatus::Unavailable { .. }) && self.missing_source_warning_open
    }

    /// Whether a video is being opened, thumbnails are being made, or an export is running.
    pub fn is_working_in_the_background(&self) -> bool {
        self.opening.is_some() || self.thumbnails.extraction.is_some() || self.export.is_some()
    }

    /// The title and text of the message the user has to read, if one is showing.
    pub fn message_showing(&self) -> Option<(&str, &str)> {
        match &self.dialog {
            Some(super::Dialog::Message { title, text }) => Some((title, text)),
            _ => None,
        }
    }

    /// The current frame of the timeline.
    pub fn current_frame(&self) -> TimelineFrameIndex {
        let total_frames = self.project.video_track().total_frame_count();
        self.clock.position(Instant::now(), total_frames)
    }

    pub fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }

    /// The latest picture the preview has, which may be a moment older than the current frame.
    pub fn preview_picture(&self) -> Option<&SuppliedPicture> {
        self.picture_supplier.picture()
    }

    /// How many thumbnails have been made, and how many there will be.
    pub fn thumbnail_progress(&self) -> (usize, usize) {
        (self.thumbnails.strip.made_count(), self.thumbnails.strip.second_count())
    }

    /// How far from the left edge of the video track, as it is drawn now, timeline frame `frame`
    /// begins, in points. This is negative or beyond the track's width for a frame that has been
    /// scrolled out of view. `None` while the project has no video.
    pub fn video_track_offset_of_frame(&self, frame: TimelineFrameIndex) -> Option<f32> {
        let geometry = self.timeline_geometry()?;
        Some((geometry.x_of_frame(frame.value()) - self.timeline_view.scroll_offset) as f32)
    }

    /// Which purpose a video chosen for import would serve, or `None` if the project's video is
    /// working and none is wanted.
    pub(super) fn purpose_of_importing(&self) -> Option<OpeningPurpose> {
        match self.source_status {
            SourceStatus::NoVideo => Some(OpeningPurpose::ImportIntoEmptyProject),
            SourceStatus::Unavailable { .. } => Some(OpeningPurpose::RelinkMissingVideo),
            // Only one video per project in this version, and it is working.
            SourceStatus::Opening | SourceStatus::Available { .. } => None,
        }
    }
}
