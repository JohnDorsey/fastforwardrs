// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor can be asked to do, and asked about, without anyone using its window: the work
//! of the File menu without its file dialogs, and a look at the state afterwards.
//!
//! The window itself uses the same work (the dialogs just choose the file first), so this is also
//! how the tests run whole journeys through the program, and how a command line or scripting
//! feature could later drive it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{FastForwardApplication, OpeningPurpose};
use crate::application::selection::SelectionItem;
use crate::media::MediaLocation;
use crate::playback::{AudioSinkOpener, EffectSpeakersOpener, SuppliedPicture};
use crate::project::{ClipId, Project, Settings, SourceMediaIdentifier, TimelineFrameIndex};

impl FastForwardApplication {
    /// Puts the video or sound file at `path` on the timeline at the current frame, as choosing it
    /// after "Link and import" does.
    ///
    /// The file is opened in the background; see [`Self::is_working_in_the_background`].
    pub fn link_and_import_file_from(&mut self, path: PathBuf) {
        self.start_opening(OpeningPurpose::ImportNewSource, vec![path]);
    }

    /// Puts the video or sound file at `path` on the timeline at the current frame, and in the project file, as choosing it
    /// after "Embed and import" does. (Unlike the menu item, this does not first ask that the project file be a `.ffrs` file: if it
    /// is not one the file is put on the timeline, and the user is asked as for any file that is included.)
    ///
    /// The file is opened in the background; see [`Self::is_working_in_the_background`].
    pub fn embed_and_import_file_from(&mut self, path: PathBuf) {
        self.start_opening(OpeningPurpose::ImportNewSourceAndEmbedIt, vec![path]);
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

    /// The file the project was last saved to or loaded from.
    pub fn project_file_path(&self) -> Option<&Path> {
        self.project_file_path.as_deref()
    }

    /// The clip that is selected, if exactly one is.
    pub fn selected_clip(&self) -> Option<ClipId> {
        self.selection.single()
    }

    /// The clips that are selected.
    pub fn selected_clips(&self) -> Vec<ClipId> {
        self.selection.to_vec()
    }

    /// Everything that is selected, clips and tracks.
    pub fn selected_items(&self) -> Vec<SelectionItem> {
        self.selection.to_items()
    }

    /// Whether there is an edit that undoing would take back.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether there is an edit that was undone, which redoing would do again.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
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

    /// Where the bytes of the source file `identifier` are, if it has been found and opens: a file of its own, or (for one that is
    /// included in the project file) the stretch of the project file that it is.
    pub fn location_of_the_source(&self, identifier: SourceMediaIdentifier) -> Option<MediaLocation> {
        self.source_files.get(&identifier)?.location_if_available().cloned()
    }

    /// Sets how many bytes of files a save copies without doing it in the background (where there is a window that says how it goes,
    /// and a button to stop it). For a test that makes a save a small one cannot be.
    pub fn copy_in_the_background_from(&mut self, bytes: u64) {
        self.most_bytes_copied_at_once = bytes;
    }

    /// Whether the warning that source files must be chosen again is showing.
    pub fn missing_sources_warning_is_showing(&self) -> bool {
        self.missing_sources_warning_open && !self.missing_sources().is_empty()
    }

    /// Whether a file is being opened, thumbnails, waveforms or pictures are being made, the fonts for
    /// the text are being loaded, or an export is running.
    pub fn is_working_in_the_background(&self) -> bool {
        !self.openings.is_empty()
            || self.export.is_some()
            || self.running_save.is_some()
            || self.text_renderer.is_loading()
            || self.source_files.values().any(|state| {
                state.thumbnails.extraction.is_some()
                    || state.thumbnails.finer.is_running()
                    || state.waveform.extraction.is_some()
                    || state.picture.load.is_some()
            })
    }

    /// Whether the picture of the image file `identifier` has been decoded for drawing, which takes a
    /// moment after the file is opened.
    pub fn image_file_is_decoded(&self, identifier: SourceMediaIdentifier) -> bool {
        self.source_files
            .get(&identifier)
            .is_some_and(|state| state.picture.picture.is_some())
    }

    /// Keeps the files that are taken out of project files that have files included in them in the folder `folder` (which is made
    /// when the first is put there). The program does this in the folder of the computer's cache; a test gives it one of its own.
    pub fn keep_archive_files_in(&mut self, folder: PathBuf) {
        self.archive_cache = Some(super::ArchiveCache::at(folder));
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

    /// Plays the editor's own little sounds (the snip of a cut, say) through the speakers `open_speakers`
    /// makes (when the first of them is played). Until this is called they are silent, like the preview's sound.
    pub fn play_effects_through(&mut self, open_speakers: EffectSpeakersOpener) {
        self.sound_effects.play_through(open_speakers);
    }

    /// Whether the preview's sound is being decoded and played right now.
    pub fn sound_is_running(&self) -> bool {
        self.audio_supplier.is_running()
    }

    /// What last went wrong with the preview's sound, if anything has.
    pub fn sound_problem(&self) -> Option<&str> {
        self.audio_supplier.last_problem()
    }

    /// The latest picture the preview has of the video that is in front (of the front-most track that
    /// has one), which may be a moment older than the current frame.
    pub fn preview_picture(&self) -> Option<&SuppliedPicture> {
        self.project
            .tracks()
            .iter()
            .find_map(|track| self.preview_pictures.picture_of(track.id()))
    }

    /// How many thumbnails of the video `identifier` have been made, and how many there will be.
    pub fn thumbnail_progress(&self, identifier: SourceMediaIdentifier) -> (usize, usize) {
        self.source_files.get(&identifier).map_or((0, 0), |state| {
            (
                state.thumbnails.library.every_second().made_count(),
                state.thumbnails.library.every_second().second_count(),
            )
        })
    }

    /// How many finer thumbnails (more than one for every second) of the video `identifier` have
    /// been made so far.
    pub fn finer_thumbnails_made(&self, identifier: SourceMediaIdentifier) -> usize {
        self.source_files
            .get(&identifier)
            .map_or(0, |state| state.thumbnails.library.finer_count())
    }

    /// How many finer thumbnails of the video `identifier` the timeline wanted, and did not have,
    /// when it was last drawn.
    pub fn finer_thumbnails_still_wanted(&self, identifier: SourceMediaIdentifier) -> usize {
        self.source_files
            .get(&identifier)
            .map_or(0, |state| state.thumbnails.finer.wanted_count())
    }

    /// How many runs of ffmpeg have been started to make finer thumbnails of the video `identifier`.
    pub fn finer_thumbnail_runs_started(&self, identifier: SourceMediaIdentifier) -> usize {
        self.source_files
            .get(&identifier)
            .map_or(0, |state| state.thumbnails.finer.runs_started())
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
        (self
            .timeline_view
            .geometry(self.timeline_geometry())
            .x_of_frame(frame.value())
            - self.timeline_view.scroll_offset) as f32
    }
}
