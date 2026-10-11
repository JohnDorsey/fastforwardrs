//! What the editor can be asked to do, and asked about, without anyone using its window: the work
//! of the File menu without its file dialogs, and a look at the state afterwards.
//!
//! The window itself uses the same work (the dialogs just choose the file first), so this is also
//! how the tests run whole journeys through the program, and how a command line or scripting
//! feature could later drive it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{FastForwardApplication, OpeningPurpose};
use crate::playback::{AudioSinkOpener, SuppliedPicture};
use crate::project::{ClipLocation, Project, Settings, SourceMediaIdentifier, TimelineFrameIndex};

impl FastForwardApplication {
    /// Puts the video or sound file at `path` on the timeline at the current frame, as choosing it
    /// in the Import dialog does.
    ///
    /// The file is opened in the background; see [`Self::is_working_in_the_background`].
    pub fn import_file_from(&mut self, path: PathBuf) {
        self.start_opening(OpeningPurpose::ImportNewSource, vec![path]);
    }

    /// Uses the file at `path` in place of the source file `identifier` of the project, as
    /// choosing it after "Choose the file again" does. All the edits are kept.
    ///
    /// The file is opened in the background; see [`Self::is_working_in_the_background`].
    pub fn relink_source_from(&mut self, identifier: SourceMediaIdentifier, path: PathBuf) {
        self.start_opening(OpeningPurpose::RelinkMissingSource(identifier), vec![path]);
    }

    /// Loads the project file at `path`, as choosing it in the Load project dialog does.
    pub fn load_project_from(&mut self, path: PathBuf) {
        self.load_project(path);
    }

    /// Starts a new, empty project, as the New project menu item does before it asks for a file.
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

    /// The clip that is selected, if one is.
    pub fn selected_clip(&self) -> Option<ClipLocation> {
        self.selected_clip
    }

    /// Whether the source file `identifier` has been found and opens.
    pub fn source_file_is_available(&self, identifier: SourceMediaIdentifier) -> bool {
        self.source_is_available(identifier)
    }

    /// Whether every source file of the project has been found and opens. (A project with no
    /// source files has every one of them.)
    pub fn every_source_file_is_available(&self) -> bool {
        self.project
            .sources()
            .iter()
            .all(|source| self.source_is_available(source.identifier))
    }

    /// Whether the warning that source files must be chosen again is showing.
    pub fn missing_sources_warning_is_showing(&self) -> bool {
        self.missing_sources_warning_open && !self.missing_sources().is_empty()
    }

    /// Whether a file is being opened, thumbnails or waveforms are being made, the fonts for the text
    /// are being loaded, or an export is running.
    pub fn is_working_in_the_background(&self) -> bool {
        !self.openings.is_empty()
            || self.export.is_some()
            || self.text_renderer.is_loading()
            || self
                .source_files
                .values()
                .any(|state| state.thumbnails.extraction.is_some() || state.waveform.extraction.is_some())
    }

    /// The title and text of the message the user has to read, if one is showing.
    pub fn message_showing(&self) -> Option<(&str, &str)> {
        match &self.dialog {
            Some(super::Dialog::Message { title, text }) => Some((title, text)),
            _ => None,
        }
    }

    /// The settings that are the same for every project.
    pub fn global_settings(&self) -> Settings {
        self.global_settings
    }

    /// The settings in effect for the project being edited: its own where it has set them, the
    /// global ones everywhere else.
    pub fn settings_in_effect(&self) -> Settings {
        self.effective_settings()
    }

    /// The current frame of the timeline.
    pub fn current_frame(&self) -> TimelineFrameIndex {
        let total_frames = self.project.total_frame_count();
        self.clock.position(Instant::now(), total_frames)
    }

    pub fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }

    /// Plays the preview's sound through the speakers `open_speakers` makes (when sound is first
    /// needed). Until this is called there is no sound: a program that is only being tested, or
    /// driven by a script, must not make noise.
    pub fn play_sound_through(&mut self, open_speakers: AudioSinkOpener) {
        self.audio_supplier.play_through(open_speakers);
    }

    /// Whether the preview's sound is being decoded and played right now.
    pub fn sound_is_running(&self) -> bool {
        self.audio_supplier.is_running()
    }

    /// What last went wrong with the preview's sound, if anything has.
    pub fn sound_problem(&self) -> Option<&str> {
        self.audio_supplier.last_problem()
    }

    /// The latest picture the preview has, which may be a moment older than the current frame.
    pub fn preview_picture(&self) -> Option<&SuppliedPicture> {
        self.picture_supplier.picture()
    }

    /// How many thumbnails of the video `identifier` have been made, and how many there will be.
    pub fn thumbnail_progress(&self, identifier: SourceMediaIdentifier) -> (usize, usize) {
        self.source_files.get(&identifier).map_or((0, 0), |state| {
            (
                state.thumbnails.strip.made_count(),
                state.thumbnails.strip.second_count(),
            )
        })
    }

    /// How many peaks of the waveform of the sound file `identifier` have been made.
    pub fn waveform_peak_count(&self, identifier: SourceMediaIdentifier) -> usize {
        self.source_files
            .get(&identifier)
            .map_or(0, |state| state.waveform.waveform.peak_count())
    }

    /// How far from the left edge of the strips, as they are drawn now, timeline frame `frame`
    /// begins, in points. This is negative or beyond the strips' width for a frame that has been
    /// scrolled out of view.
    pub fn strip_offset_of_frame(&self, frame: TimelineFrameIndex) -> f32 {
        (self.timeline_geometry().x_of_frame(frame.value()) - self.timeline_view.scroll_offset) as f32
    }
}
