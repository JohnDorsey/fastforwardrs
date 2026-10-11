//! Finding, opening and replacing the project's source files (videos and sounds), and making the
//! pictures and waveforms the timeline shows for them.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::{
    FastForwardApplication, OpeningPurpose, OpeningTask, SourceFileState, SourceStatus, ThumbnailState, WaveformState,
};
use crate::application::background::{BackgroundTask, TaskStatus};
use crate::application::dialogs::MissingSource;
use crate::application::opening::{OpeningOutcome, open_first_that_works};
use crate::application::theme;
use crate::media::{
    PictureSize, ThumbnailEvent, ThumbnailExtraction, ThumbnailStrip, WaveformEvent, WaveformExtraction,
    probe_source_media,
};
use crate::project::{
    SourceMedia, SourceMediaIdentifier, SourceMediaKind, SourceMediaLink, SourceMediaProperties, SourceVideoProperties,
};
use crate::project_file::relative_path_between;

/// The kinds of file offered first in the dialog that asks for a video.
const VIDEO_FILE_EXTENSIONS: [&str; 12] = [
    "mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "ts", "mts",
];

/// ...and for a sound.
const SOUND_FILE_EXTENSIONS: [&str; 10] = ["mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "wma", "aiff"];

impl FastForwardApplication {
    /// Asks the user for a video or a sound file and puts all of it on the timeline, at the current
    /// frame: on the front-most track that has nothing on it, or on a new track in front.
    pub(super) fn import_file(&mut self) {
        self.pause_playback_for_dialog();
        if let Some(path) = ask_for_a_media_file("Choose a video or a sound") {
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
        if let Some(path) = ask_for_a_media_file("Choose the file again") {
            self.start_opening(OpeningPurpose::RelinkMissingSource(identifier), vec![path]);
        }
    }

    /// Starts trying `candidates`, in order, in the background.
    pub(super) fn start_opening(&mut self, purpose: OpeningPurpose, candidates: Vec<PathBuf>) {
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
        let task = BackgroundTask::start(Arc::clone(&self.wake_up), move || {
            open_first_that_works(&candidates, |path| probe_source_media(&toolchain, path))
        });
        self.openings.push(OpeningTask { purpose, task });
    }

    /// Picks up whatever the background work has finished since the last frame.
    pub(super) fn collect_finished_background_work(&mut self) {
        self.collect_opening_results();
        self.collect_thumbnails();
        self.collect_waveforms();
        self.collect_export_events();
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
                        PathBuf::new(),
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

    fn finish_opening(&mut self, purpose: OpeningPurpose, outcome: OpeningOutcome) {
        match (purpose, outcome) {
            (OpeningPurpose::ImportNewSource, OpeningOutcome::Opened { path, properties }) => {
                self.put_opened_file_on_the_timeline(path, properties);
            }
            (OpeningPurpose::ImportNewSource, failure @ OpeningOutcome::Failed { .. }) => {
                self.show_message("The file could not be opened", failure.describe_failure());
            }

            (OpeningPurpose::RelinkMissingSource(identifier), OpeningOutcome::Opened { path, properties }) => {
                let link = self.link_to(&path);
                match self.project.relink_source(identifier, link, properties) {
                    Ok(relinked) => {
                        self.note_unsaved_change();
                        self.source_is_now_available(identifier, path, properties);
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

            (OpeningPurpose::LocateForLoadedProject(identifier), OpeningOutcome::Opened { path, properties }) => {
                self.finish_locating_a_file_of_the_loaded_project(identifier, path, properties);
            }
            (OpeningPurpose::LocateForLoadedProject(identifier), failure @ OpeningOutcome::Failed { .. }) => {
                // The warning shows where the file was last known to be, so that is not repeated.
                self.mark_source_unavailable(identifier, failure.describe_failure_for_a_known_place());
            }
        }
    }

    /// A file the user chose was opened: it becomes a source file of the project, with a clip of
    /// all of it at the current frame.
    fn put_opened_file_on_the_timeline(&mut self, path: PathBuf, properties: SourceMediaProperties) {
        let identifier = self.project.next_source_identifier();
        let source = SourceMedia {
            identifier,
            link: self.link_to(&path),
            properties,
        };
        let frame_rate_before = self.project.frame_rate();
        let total_frames = self.project.total_frame_count();
        let current_frame = self.clock.position(Instant::now(), total_frames);
        match self.project.import_source(source, current_frame) {
            Ok(_) => {
                self.note_unsaved_change();
                // The first video decides the frame rate of the whole project.
                if self.project.frame_rate() != frame_rate_before {
                    self.clock.reset_for_frame_rate(self.project.frame_rate());
                    self.timeline_view.reset();
                }
                self.source_is_now_available(identifier, path, properties);
            }
            Err(error) => self.show_message("The file could not be imported", error.to_string()),
        }
    }

    /// A file was found for a source file of a project that was just loaded. It must be the same
    /// kind of file, and a video must run at the frame rate the edits were made at; if it does,
    /// the project keeps everything it knew and only the place the file was found is remembered.
    fn finish_locating_a_file_of_the_loaded_project(
        &mut self,
        identifier: SourceMediaIdentifier,
        path: PathBuf,
        properties: SourceMediaProperties,
    ) {
        let Some(source) = self.project.source(identifier).cloned() else {
            return;
        };
        if source.properties.kind() != properties.kind() {
            let (expected, found) = match source.properties.kind() {
                SourceMediaKind::Video => ("a video", "a sound file"),
                SourceMediaKind::Audio => ("a sound file", "a video"),
            };
            self.mark_source_unavailable(
                identifier,
                format!(
                    "{} is {found}, but the project was edited on {expected}",
                    path.display()
                ),
            );
            return;
        }
        if let (Some(project_video), Some(found_video)) = (source.properties.video(), properties.video())
            && project_video.frame_rate != found_video.frame_rate
        {
            self.mark_source_unavailable(
                identifier,
                format!(
                    "{} runs at {} frames per second, but the project was edited on a video running at {}",
                    path.display(),
                    found_video.frame_rate,
                    project_video.frame_rate
                ),
            );
            return;
        }
        let link = self.link_to(&path);
        // Only the link changes: the project keeps the properties it was edited with.
        let _ = self.project.relink_source(identifier, link, source.properties);
        self.source_is_now_available(identifier, path, properties);
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
        path: PathBuf,
        properties: SourceMediaProperties,
    ) {
        self.textures.clear();
        self.source_files.insert(
            identifier,
            SourceFileState::new(SourceStatus::Available { path: path.clone() }),
        );
        if self.missing_sources().is_empty() {
            self.missing_sources_warning_open = false;
        }
        match properties {
            SourceMediaProperties::Video(video) => self.start_thumbnails(identifier, &path, video),
            SourceMediaProperties::Audio(_) => self.start_waveform(identifier, &path),
        }
    }

    fn start_thumbnails(&mut self, identifier: SourceMediaIdentifier, path: &Path, properties: SourceVideoProperties) {
        let seconds_of_video = properties
            .frame_rate
            .seconds_at_frame(properties.total_frame_count)
            .ceil() as usize;
        let Some(state) = self.source_files.get_mut(&identifier) else {
            return;
        };
        state.thumbnails = ThumbnailState {
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
            Ok(extraction) => state.thumbnails.extraction = Some(extraction),
            Err(error) => {
                let problem = error.to_string();
                self.show_message("The pictures for the timeline could not be made", problem);
            }
        }
    }

    fn start_waveform(&mut self, identifier: SourceMediaIdentifier, path: &Path) {
        let Some(state) = self.source_files.get_mut(&identifier) else {
            return;
        };
        state.waveform = WaveformState::default();
        let Some(toolchain) = &self.toolchain else { return };
        match WaveformExtraction::start(toolchain, path, Arc::clone(&self.wake_up)) {
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
                    ThumbnailEvent::Thumbnail { second, picture } => state.thumbnails.strip.insert(second, picture),
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
                    last_known_path: source.link.absolute_path.display().to_string(),
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
        }
    }
}

fn ask_for_a_media_file(title: &str) -> Option<PathBuf> {
    let everything_we_know: Vec<&str> = VIDEO_FILE_EXTENSIONS
        .iter()
        .chain(SOUND_FILE_EXTENSIONS.iter())
        .copied()
        .collect();
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("Video and sound files", &everything_we_know)
        .add_filter("Video files", &VIDEO_FILE_EXTENSIONS)
        .add_filter("Sound files", &SOUND_FILE_EXTENSIONS)
        .add_filter("All files", &["*"])
        .pick_file()
}
