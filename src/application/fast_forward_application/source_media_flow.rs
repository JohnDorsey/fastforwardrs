//! Finding, opening and replacing the project's source files (videos, sounds and pictures), and
//! making the pictures and waveforms the timeline shows for them.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::{
    FastForwardApplication, OpeningPurpose, OpeningTask, PictureState, SourceFileState, SourceStatus, ThumbnailState,
    WaveformState,
};
use crate::application::background::{BackgroundTask, TaskStatus};
use crate::application::dialogs::MissingSource;
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::application::opening::{OpeningOutcome, open_first_that_works};
use crate::application::theme;
use crate::media::{
    MediaLocation, PictureSize, ThumbnailEvent, ThumbnailExtraction, ThumbnailKey, ThumbnailLibrary, WaveformEvent,
    WaveformExtraction, decode_picture, probe_source_media, size_to_decode_at,
};
use crate::playback::preview_picture_size;
use crate::project::{
    SourceImageProperties, SourceMedia, SourceMediaIdentifier, SourceMediaLink, SourceMediaProperties,
    SourceVideoProperties,
};
use crate::project_file::archive::entry_place;
use crate::project_file::relative_path_between;

/// The kinds of file offered first in the dialog that asks for a video.
const VIDEO_FILE_EXTENSIONS: [&str; 12] = [
    "mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "ts", "mts",
];

/// How many finer pictures of one video are kept before the ones that are not on screen are let go of.
const MOST_FINER_THUMBNAILS_KEPT: usize = 4000;

/// ...and for a sound.
const SOUND_FILE_EXTENSIONS: [&str; 10] = ["mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "wma", "aiff"];

/// ...and for a picture. (A GIF is not one: ffprobe says it is a video, and so does the program.)
const PICTURE_FILE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff"];

impl FastForwardApplication {
    /// Asks the user for a video, a sound or a picture and puts all of it on the timeline, at the
    /// current frame: on the front-most track that has nothing on it, or on a new track in front.
    /// The project remembers where the file is, and does not hold it.
    pub(super) fn link_and_import_file(&mut self) {
        self.pause_playback_for_dialog();
        if let Some(path) = self.ask_for_a_media_file("Choose a video, a sound or a picture") {
            self.start_opening(OpeningPurpose::ImportNewSource, vec![path]);
        }
    }

    /// Asks the user for the file to use in place of the source file `identifier`, which cannot be
    /// found. All the edits are kept.
    pub(super) fn relink_source(&mut self, identifier: SourceMediaIdentifier) {
        if self.project.source(identifier).is_none() {
            return;
        }
        self.pause_playback_for_dialog();
        if let Some(path) = self.ask_for_a_media_file("Choose the file again") {
            self.start_opening(OpeningPurpose::RelinkMissingSource(identifier), vec![path]);
        }
    }

    /// Starts opening the file `entry_name` of the archive `archive` (the project file, which has the file of the source file
    /// `identifier` included in it), in the background. It is read where it is in the archive, as a stretch of it; only a file that
    /// another program compressed cannot be, and that is taken out into the cache first.
    pub(super) fn start_opening_an_included_file(
        &mut self,
        identifier: SourceMediaIdentifier,
        archive: PathBuf,
        entry_name: String,
    ) {
        let Some(toolchain) = self.toolchain.clone() else {
            self.source_files.insert(
                identifier,
                SourceFileState::new(SourceStatus::Unavailable {
                    reason: "ffmpeg could not be found".to_owned(),
                }),
            );
            self.missing_sources_warning_open = true;
            return;
        };
        let cache = self.archive_cache.clone();
        self.source_files
            .insert(identifier, SourceFileState::new(SourceStatus::Opening));
        let archive = std::path::absolute(&archive).unwrap_or(archive);
        let task = BackgroundTask::start(Arc::clone(&self.wake_up), move || {
            let failed = |reason: String| OpeningOutcome::Failed {
                attempts: vec![(MediaLocation::of_a_file(&entry_name), reason)],
            };
            let place = match entry_place(&archive, &entry_name) {
                Ok(place) => place,
                Err(error) => return failed(error.to_string()),
            };
            let location = match place.as_a_location_in(&archive) {
                Some(location) => location,
                None => match &cache {
                    Some(cache) => match cache.take_out(&archive, &entry_name) {
                        Ok(path) => MediaLocation::of_a_file(path),
                        Err(error) => return failed(error.to_string()),
                    },
                    None => {
                        return failed(
                            "it is compressed in the project file, and there is nowhere to keep the files that are taken out of it"
                                .to_owned(),
                        );
                    }
                },
            };
            open_first_that_works(&[location], |location| probe_source_media(&toolchain, location))
        });
        self.openings.push(OpeningTask {
            purpose: OpeningPurpose::LocateForLoadedProject(identifier),
            task,
        });
    }

    /// Starts trying `candidates`, in order, in the background.
    pub(super) fn start_opening(&mut self, purpose: OpeningPurpose, candidates: Vec<PathBuf>) {
        #[cfg(test)]
        self.opening_purposes_started.push(purpose);
        let Some(toolchain) = self.toolchain.clone() else {
            if let OpeningPurpose::LocateForLoadedProject(identifier) = purpose {
                self.source_files.insert(
                    identifier,
                    SourceFileState::new(SourceStatus::Unavailable {
                        reason: "ffmpeg could not be found".to_owned(),
                    }),
                );
            }
            self.show_message(
                "ffmpeg was not found",
                "Videos and sounds cannot be opened without ffmpeg. Install it so that both ffmpeg and ffprobe are on your PATH, then start fastforwardrs again.",
            );
            return;
        };
        if let OpeningPurpose::LocateForLoadedProject(identifier) = purpose {
            self.source_files
                .insert(identifier, SourceFileState::new(SourceStatus::Opening));
        }
        let candidates: Vec<MediaLocation> = candidates.into_iter().map(MediaLocation::of_a_file).collect();
        let task = BackgroundTask::start(Arc::clone(&self.wake_up), move || {
            open_first_that_works(&candidates, |location| probe_source_media(&toolchain, location))
        });
        self.openings.push(OpeningTask { purpose, task });
    }

    /// Picks up whatever the background work has finished since the last frame.
    pub(super) fn collect_finished_background_work(&mut self) {
        self.collect_opening_results();
        self.collect_thumbnails();
        self.collect_finer_thumbnails();
        self.collect_waveforms();
        self.collect_pictures();
        self.collect_export_events();
        self.collect_the_running_save();
    }

    fn collect_opening_results(&mut self) {
        let mut finished = Vec::new();
        let mut index = 0;
        while index < self.openings.len() {
            let outcome = match self.openings[index].task.status() {
                TaskStatus::Pending => {
                    index += 1;
                    continue;
                }
                TaskStatus::Finished(outcome) => outcome,
                TaskStatus::Lost => OpeningOutcome::Failed {
                    attempts: vec![(
                        MediaLocation::of_a_file(PathBuf::new()),
                        "the program that checks files stopped unexpectedly".to_owned(),
                    )],
                },
            };
            let task = self.openings.remove(index);
            finished.push((task.purpose, outcome));
        }
        for (purpose, outcome) in finished {
            self.finish_opening(purpose, outcome);
        }
    }

    pub(super) fn finish_opening(&mut self, purpose: OpeningPurpose, outcome: OpeningOutcome) {
        match (purpose, outcome) {
            (OpeningPurpose::ImportNewSource, OpeningOutcome::Opened { location, properties }) => {
                self.put_opened_file_on_the_timeline(location, properties);
            }
            (OpeningPurpose::ImportNewSource, failure @ OpeningOutcome::Failed { .. }) => {
                self.show_message("The file could not be opened", failure.describe_failure());
            }

            (OpeningPurpose::ImportNewSourceAndEmbedIt, OpeningOutcome::Opened { location, properties }) => {
                // The file is on the timeline whatever happens next: if it cannot be put in the project file, it stays where it is, and
                // the user is told why (as for any file that is included).
                if let Some(identifier) = self.put_opened_file_on_the_timeline(location, properties) {
                    self.include_files_in_the_project_file(&[identifier]);
                }
            }
            (OpeningPurpose::ImportNewSourceAndEmbedIt, failure @ OpeningOutcome::Failed { .. }) => {
                self.show_message("The file could not be opened", failure.describe_failure());
            }

            (OpeningPurpose::RelinkMissingSource(identifier), OpeningOutcome::Opened { location, properties }) => {
                let link = self.link_to(location.file());
                match self.project.relink_source(identifier, link, properties) {
                    Ok(relinked) => {
                        self.note_unsaved_change();
                        self.source_is_now_available(identifier, location, properties);
                        if relinked.frame_count_before != relinked.frame_count_after {
                            self.show_message(
                                "The file is not the same length",
                                format!(
                                    "The project was edited on a file of {} frames, and this one has {}. All your edits are kept; \
                                     where the file has nothing for a clip, the picture is black and the sound is silent.",
                                    relinked.frame_count_before, relinked.frame_count_after
                                ),
                            );
                        }
                    }
                    Err(error) => self.show_message("That file cannot be used", error.to_string()),
                }
            }
            (OpeningPurpose::RelinkMissingSource(_), failure @ OpeningOutcome::Failed { .. }) => {
                self.show_message("The file could not be opened", failure.describe_failure());
            }

            (OpeningPurpose::LocateForLoadedProject(identifier), OpeningOutcome::Opened { location, properties }) => {
                self.finish_locating_a_file_of_the_loaded_project(identifier, location, properties);
            }
            (OpeningPurpose::LocateForLoadedProject(identifier), failure @ OpeningOutcome::Failed { .. }) => {
                // The warning shows where the file was last known to be, so that is not repeated.
                self.mark_source_unavailable(identifier, failure.describe_failure_for_a_known_place());
            }
        }
    }

    /// A file the user chose was opened: it becomes a source file of the project, with a clip of
    /// all of it at the current frame. Gives the source file, if it could be put on the timeline.
    fn put_opened_file_on_the_timeline(
        &mut self,
        location: MediaLocation,
        properties: SourceMediaProperties,
    ) -> Option<SourceMediaIdentifier> {
        let identifier = self.project.next_source_identifier();
        let source = SourceMedia {
            identifier,
            link: self.link_to(location.file()),
            properties,
        };
        let frame_rate_before = self.project.frame_rate();
        let total_frames = self.project.total_frame_count();
        let now = Instant::now();
        let current_frame = self.clock.position(now, total_frames);
        let project_before = self.project.clone();
        match self.project.import_source(source, current_frame) {
            Ok(_) => {
                // Importing is an edit like any other: undoing it takes the file off the timeline again.
                self.history
                    .record_change(project_before, self.selection.to_items(), None, now, false);
                self.note_the_project_was_edited(now);
                // The first video decides the frame rate of the whole project.
                if self.project.frame_rate() != frame_rate_before {
                    self.clock.reset_for_frame_rate(self.project.frame_rate());
                    self.timeline_view.reset();
                }
                self.source_is_now_available(identifier, location, properties);
                Some(identifier)
            }
            Err(error) => {
                self.show_message("The file could not be imported", error.to_string());
                None
            }
        }
    }

    /// A file was found for a source file of a project that was just loaded. It must be the same
    /// kind of file; if it is, the project keeps everything it knew and only the place the file was
    /// found is remembered.
    fn finish_locating_a_file_of_the_loaded_project(
        &mut self,
        identifier: SourceMediaIdentifier,
        location: MediaLocation,
        properties: SourceMediaProperties,
    ) {
        let Some(source) = self.project.source(identifier).cloned() else {
            return;
        };
        if source.properties.kind() != properties.kind() {
            let (expected, found) = (source.properties.kind().description(), properties.kind().description());
            self.mark_source_unavailable(
                identifier,
                format!("{location} is {found}, but the project was edited on {expected}"),
            );
            return;
        }
        // A file that is included in the project file was taken out of it, and is where it was put: that is not where it is linked
        // to, which is where it was and is what it is put back at.
        if !source.link.is_included() {
            let link = self.link_to(location.file());
            // Only the link changes: the project keeps the properties it was edited with.
            let _ = self.project.relink_source(identifier, link, source.properties);
        }
        self.source_is_now_available(identifier, location, properties);
    }

    /// The source file cannot be used: say so, and show the warning.
    fn mark_source_unavailable(&mut self, identifier: SourceMediaIdentifier, reason: String) {
        self.source_files
            .insert(identifier, SourceFileState::new(SourceStatus::Unavailable { reason }));
        self.missing_sources_warning_open = true;
    }

    /// The file is usable: show its pictures, and make the thumbnails or the waveform its clips show.
    fn source_is_now_available(
        &mut self,
        identifier: SourceMediaIdentifier,
        location: MediaLocation,
        properties: SourceMediaProperties,
    ) {
        self.textures.clear();
        self.source_files.insert(
            identifier,
            SourceFileState::new(SourceStatus::Available {
                location: location.clone(),
            }),
        );
        if self.missing_sources().is_empty() {
            self.missing_sources_warning_open = false;
        }
        match properties {
            SourceMediaProperties::Video(video) => self.start_thumbnails(identifier, &location, video),
            SourceMediaProperties::Audio(_) => self.start_waveform(identifier, &location),
            SourceMediaProperties::Image(image) => self.start_loading_the_picture(identifier, &location, image),
        }
    }

    /// Starts decoding the picture of an image file, at the size it is drawn at most: no bigger than
    /// the picture of the preview.
    fn start_loading_the_picture(
        &mut self,
        identifier: SourceMediaIdentifier,
        location: &MediaLocation,
        properties: SourceImageProperties,
    ) {
        let (picture_width, picture_height) = self.project.picture_size();
        let largest = preview_picture_size(picture_width, picture_height);
        let size = size_to_decode_at(
            (properties.display_width_in_pixels, properties.display_height_in_pixels),
            largest,
        );
        let Some(state) = self.source_files.get_mut(&identifier) else {
            return;
        };
        state.picture = PictureState::default();
        let Some(toolchain) = self.toolchain.clone() else {
            return;
        };
        let location = location.clone();
        state.picture.load = Some(BackgroundTask::start(Arc::clone(&self.wake_up), move || {
            decode_picture(&toolchain, &location, size).map_err(|error| error.to_string())
        }));
    }

    /// Picks up the pictures that have been decoded since the last frame.
    fn collect_pictures(&mut self) {
        let mut problems = Vec::new();
        for state in self.source_files.values_mut() {
            let Some(load) = &state.picture.load else {
                continue;
            };
            match load.status() {
                TaskStatus::Pending => {}
                TaskStatus::Finished(Ok(picture)) => {
                    state.picture.picture = Some(picture);
                    state.picture.load = None;
                }
                TaskStatus::Finished(Err(problem)) => {
                    problems.push(problem);
                    state.picture.load = None;
                }
                TaskStatus::Lost => {
                    problems.push("the program that decodes pictures stopped unexpectedly".to_owned());
                    state.picture.load = None;
                }
            }
        }
        if let Some(problem) = problems.into_iter().next() {
            self.show_message("The picture could not be drawn", problem);
        }
    }

    fn start_thumbnails(
        &mut self,
        identifier: SourceMediaIdentifier,
        location: &MediaLocation,
        properties: SourceVideoProperties,
    ) {
        let seconds_of_video = properties
            .frame_rate
            .seconds_at_frame(properties.total_frame_count)
            .ceil() as usize;
        let Some(state) = self.source_files.get_mut(&identifier) else {
            return;
        };
        state.thumbnails = ThumbnailState {
            library: ThumbnailLibrary::with_room_for(seconds_of_video),
            ..ThumbnailState::default()
        };
        let Some(toolchain) = &self.toolchain else { return };
        let size = PictureSize::thumbnail_for(
            properties.display_width_in_pixels,
            properties.display_height_in_pixels,
            theme::THUMBNAIL_HEIGHT_IN_PIXELS,
        );
        match ThumbnailExtraction::start(toolchain, location, size, Arc::clone(&self.wake_up)) {
            Ok(extraction) => state.thumbnails.extraction = Some(extraction),
            Err(error) => {
                let problem = error.to_string();
                self.show_message("The pictures for the timeline could not be made", problem);
            }
        }
    }

    fn start_waveform(&mut self, identifier: SourceMediaIdentifier, location: &MediaLocation) {
        let Some(state) = self.source_files.get_mut(&identifier) else {
            return;
        };
        state.waveform = WaveformState::default();
        let Some(toolchain) = &self.toolchain else { return };
        match WaveformExtraction::start(toolchain, location, Arc::clone(&self.wake_up)) {
            Ok(extraction) => state.waveform.extraction = Some(extraction),
            Err(error) => {
                let problem = error.to_string();
                self.show_message("The waveform of the sound could not be made", problem);
            }
        }
    }

    fn collect_thumbnails(&mut self) {
        let mut problems = Vec::new();
        for state in self.source_files.values_mut() {
            let Some(extraction) = &state.thumbnails.extraction else {
                continue;
            };
            let mut finished = false;
            while let Some(event) = extraction.try_next_event() {
                match event {
                    ThumbnailEvent::Thumbnail { index, picture } => state
                        .thumbnails
                        .library
                        .insert(ThumbnailKey { level: 0, index }, picture),
                    ThumbnailEvent::Finished { error_output } => {
                        problems.extend(error_output);
                        finished = true;
                        break;
                    }
                }
            }
            if finished {
                state.thumbnails.extraction = None;
            }
        }
        if let Some(problem) = problems.into_iter().next() {
            self.show_message("The pictures for the timeline could not all be made", problem);
        }
    }

    /// Tells each video which of its finer pictures the timeline wanted, and did not have, when it
    /// was drawn (see [`crate::application::textures::Textures::take_finer_thumbnails_wanted`]).
    pub(super) fn ask_for_finer_thumbnails(&mut self) {
        let mut wanted = self.textures.take_finer_thumbnails_wanted();
        for (identifier, state) in &mut self.source_files {
            state
                .thumbnails
                .finer
                .set_wanted(wanted.remove(identifier).unwrap_or_default());
        }
    }

    /// Takes in the finer pictures that have been made, and starts making the next ones that are
    /// wanted: one run of pictures at a time, and not while the pictures of every second are still
    /// being made, which come first.
    fn collect_finer_thumbnails(&mut self) {
        for (identifier, state) in &mut self.source_files {
            let thumbnails = &mut state.thumbnails;
            thumbnails.finer.collect_into(&mut thumbnails.library);
            thumbnails
                .finer
                .keep_within(&mut thumbnails.library, MOST_FINER_THUMBNAILS_KEPT);
            if thumbnails.finer.is_running() || thumbnails.extraction.is_some() {
                continue;
            }
            let (Some(toolchain), Some(location), Some(video)) = (
                &self.toolchain,
                state.location_if_available().cloned(),
                self.project
                    .source(*identifier)
                    .and_then(|source| source.properties.video()),
            ) else {
                continue;
            };
            let thumbnails = &mut state.thumbnails;
            let Some(range) = thumbnails.finer.next_run(&thumbnails.library) else {
                continue;
            };
            let size = PictureSize::thumbnail_for(
                video.display_width_in_pixels,
                video.display_height_in_pixels,
                theme::THUMBNAIL_HEIGHT_IN_PIXELS,
            );
            // (If ffmpeg cannot be started the pictures are simply not made: they were asked for, so
            // they are not asked for again, and the coarser ones stay on show.)
            if let Ok(extraction) =
                ThumbnailExtraction::start_range(toolchain, &location, size, range, Arc::clone(&self.wake_up))
            {
                thumbnails.finer.start(range, extraction);
            }
        }
    }

    fn collect_waveforms(&mut self) {
        let mut problems = Vec::new();
        for state in self.source_files.values_mut() {
            let Some(extraction) = &state.waveform.extraction else {
                continue;
            };
            let mut finished = false;
            while let Some(event) = extraction.try_next_event() {
                match event {
                    WaveformEvent::Peaks(peaks) => state.waveform.waveform.extend(&peaks),
                    WaveformEvent::Finished { error_output } => {
                        problems.extend(error_output);
                        finished = true;
                        break;
                    }
                }
            }
            if finished {
                state.waveform.extraction = None;
            }
        }
        if let Some(problem) = problems.into_iter().next() {
            self.show_message("The waveform of the sound could not be made", problem);
        }
    }

    /// The source files that cannot be found or opened, in the order the project has them.
    pub(super) fn missing_sources(&self) -> Vec<MissingSource> {
        self.project
            .sources()
            .iter()
            .filter_map(|source| {
                let SourceStatus::Unavailable { reason } = &self.source_files.get(&source.identifier)?.status else {
                    return None;
                };
                Some(MissingSource {
                    identifier: source.identifier,
                    last_known_path: if source.link.is_included() {
                        format!("{} (included in the project file)", source.link.absolute_path.display())
                    } else {
                        source.link.absolute_path.display().to_string()
                    },
                    reason: reason.clone(),
                })
            })
            .collect()
    }

    /// How a file at `path` is linked to the project: by absolute path, and (once the project has
    /// a file) by a path relative to the project's folder.
    fn link_to(&self, path: &Path) -> SourceMediaLink {
        let absolute_path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let relative_path = self
            .project_file_path
            .as_deref()
            .and_then(Path::parent)
            .and_then(|project_directory| std::path::absolute(project_directory).ok())
            .and_then(|project_directory| relative_path_between(&project_directory, &absolute_path));
        SourceMediaLink {
            absolute_path,
            relative_path,
            included_as: None,
        }
    }

    /// Asks which video, sound or picture file to use, offering all the kinds there are first.
    /// `None` if none was chosen.
    pub(super) fn ask_for_a_media_file(&self, title: &str) -> Option<PathBuf> {
        let everything_we_know: Vec<&str> = VIDEO_FILE_EXTENSIONS
            .iter()
            .chain(SOUND_FILE_EXTENSIONS.iter())
            .chain(PICTURE_FILE_EXTENSIONS.iter())
            .copied()
            .collect();
        self.file_chooser.choose_a_file_to_open(&FileQuestion {
            title,
            suggested_file_name: None,
            starting_directory: None,
            kinds: &[
                FileKind {
                    name: "Video, sound and picture files",
                    extensions: &everything_we_know,
                },
                FileKind {
                    name: "Video files",
                    extensions: &VIDEO_FILE_EXTENSIONS,
                },
                FileKind {
                    name: "Sound files",
                    extensions: &SOUND_FILE_EXTENSIONS,
                },
                FileKind {
                    name: "Picture files",
                    extensions: &PICTURE_FILE_EXTENSIONS,
                },
                FileKind {
                    name: "All files",
                    extensions: &["*"],
                },
            ],
        })
    }
}
