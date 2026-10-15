// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The state of the editor, in six groups by what it is about (see [`super::FastForwardApplication`], which has one of each).
//!
//! A group is a place where what belongs together lives and where what is added later has somewhere to go: the sound of the preview, the
//! pictures of the preview and whatever renders them are in [`Playback`]; the project and what is known of it as a file are in
//! [`Document`]; the files it is made from are in [`SourceFiles`]; the work that goes on in the background in [`BackgroundJobs`]; what
//! the window shows and what is going on in it in [`Interface`]; and what the editor is given by the computer it runs on (ffmpeg, the file
//! dialogs, the settings kept in a file, the fonts) in [`Environment`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[cfg(test)]
use super::OpeningPurpose;
use super::saving::RunningSave;
use super::{OpeningTask, RunningExport, SourceFileState};
use crate::application::archive_cache::ArchiveCache;
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::FileChooser;
use crate::application::gesture_frame::GestureFrame;
use crate::application::history::History;
use crate::application::preview_panel::PreviewView;
use crate::application::selection::Selection;
use crate::application::settings_windows::SettingsWindowsOpen;
use crate::application::textures::Textures;
use crate::application::timeline_panel::TimelineViewState;
use crate::media::{FfmpegToolchain, MediaLocation, UserInterfaceWakeUp, VideoStreamFormat};
use crate::playback::{
    AudioSupplier, ClockReading, FfmpegAudioStreamStarter, FfmpegPictureStreamStarter, PlaybackClock, PreviewPictures,
    SoundEffects,
};
use crate::project::{Project, Settings, SourceMediaIdentifier, TimelineFrameIndex};
use crate::text_rendering::LazyTextRenderer;

/// The project that is being edited, and what is known of it as a file: where it is kept, what has been done to it (to undo), and whether
/// what is on the screen has been saved.
pub(super) struct Document {
    pub(super) project: Project,
    /// Where the project was last saved or loaded from.
    pub(super) project_file_path: Option<PathBuf>,
    /// The edits that can be undone, and the ones that were undone.
    pub(super) history: History,
    /// Whether something that is not in the history was changed since the project was saved: a setting of the project, or the place a file
    /// was found.
    pub(super) unsaved_change_outside_history: bool,
    /// When the project last changed in a way that has not been saved; see [`super::autosave`]. `None` when nothing is waiting to be
    /// autosaved.
    pub(super) last_unsaved_change_at: Option<Instant>,
    /// Where the files that are taken out of a project file that has files included in it are kept while the project is open. `None` keeps
    /// none: such a project's included files cannot be opened (a test that wants them gives it a place, see
    /// [`super::FastForwardApplication::keep_archive_files_in`]).
    pub(super) archive_cache: Option<ArchiveCache>,
}

impl Document {
    pub(super) fn new(project: Project) -> Self {
        Self {
            project,
            project_file_path: None,
            history: History::new(),
            unsaved_change_outside_history: false,
            last_unsaved_change_at: None,
            archive_cache: None,
        }
    }
}

/// What makes the preview: the clock, the pictures of the videos that are seen, the sound, and the editor's own little sounds.
pub(super) struct Playback {
    pub(super) clock: PlaybackClock,
    /// Gets the pictures of the videos that are seen, a stream of them for each track.
    pub(super) preview_pictures: PreviewPictures<FfmpegPictureStreamStarter>,
    /// Plays the sound of the preview. It is silent until given speakers to play through.
    pub(super) audio_supplier: AudioSupplier<FfmpegAudioStreamStarter>,
    /// Plays the editor's own little sounds, such as the snip of a cut. It is silent until given speakers to play through.
    pub(super) sound_effects: SoundEffects,
    /// Whether the pictures of the videos that are stored in a way the picture engine can use as it is are wanted in planes, as ffmpeg decodes them,
    /// rather than as RGBA made by ffmpeg: it is so while the picture engine is what draws the preview. (While it is not, nobody even asks ffprobe
    /// how the pictures of a video are stored, and the preview is made as it always was.)
    pub(super) planes_wanted: bool,
}

impl Playback {
    /// Where the clock is at `now`, for a timeline of `total_frames` frames.
    pub(super) fn reading(&self, now: Instant, total_frames: u64) -> ClockReading {
        ClockReading {
            frame: self.clock.position(now, total_frames),
            seconds: self.clock.position_in_seconds(now, total_frames),
            is_playing: self.clock.is_playing(),
        }
    }
}

/// What is known, while the program runs, about the files the project is made from, and the files that are being looked for.
#[derive(Default)]
pub(super) struct SourceFiles {
    /// What is known about each of the project's source files.
    pub(super) files: HashMap<SourceMediaIdentifier, SourceFileState>,
    /// Source files that are being looked for and opened, in the background.
    pub(super) openings: Vec<OpeningTask>,
    /// For tests: why each file that was asked to be opened was (there may be no ffmpeg to open it with, in a test).
    #[cfg(test)]
    pub(super) opening_purposes_started: Vec<OpeningPurpose>,
}

impl SourceFiles {
    /// Whether the source file `identifier` has been found and opens.
    pub(super) fn is_available(&self, identifier: SourceMediaIdentifier) -> bool {
        self.files
            .get(&identifier)
            .is_some_and(|state| state.location_if_available().is_some())
    }

    /// Where the bytes of each source file that can be used are.
    pub(super) fn available_locations(&self) -> HashMap<SourceMediaIdentifier, MediaLocation> {
        self.files
            .iter()
            .filter_map(|(identifier, state)| Some((*identifier, state.location_if_available()?.clone())))
            .collect()
    }
}

/// Work that goes on in the background and that the window waits for: an export, a save that copies a great deal, and the tidying of the
/// project file before the window closes.
pub(super) struct BackgroundJobs {
    pub(super) export: Option<RunningExport>,
    /// A save that is being written on a thread of its own, because it copies a great deal.
    pub(super) running_save: Option<RunningSave>,
    /// How many bytes of files a save copies without doing it in the background.
    pub(super) most_bytes_copied_at_once: u64,
    /// For tests: makes a copy in the background wait to be let go.
    #[cfg(test)]
    pub(super) copy_gate: Option<std::sync::Arc<super::saving::Gate>>,
    /// Whether writing the project file again without its garbage has been tried (or looked at, and was not worth it) since the window was
    /// asked to close: it is tried once.
    pub(super) tidying_before_closing_was_tried: bool,
}

impl Default for BackgroundJobs {
    fn default() -> Self {
        Self {
            export: None,
            running_save: None,
            most_bytes_copied_at_once: super::saving::MOST_BYTES_COPIED_AT_ONCE,
            #[cfg(test)]
            copy_gate: None,
            tidying_before_closing_was_tried: false,
        }
    }
}

/// What the window shows and what is going on in it: the selection, the panels' views, the dialog that is up, and the closing of the window.
pub(super) struct Interface {
    /// The clips, tracks, keyframes, racks and groups that were clicked, which Delete deletes and S cuts.
    pub(super) selection: Selection,
    /// The frame that the gesture that is going on (a value dragged, say) makes keyframes at: see [`GestureFrame`].
    pub(super) gesture_frame: GestureFrame,
    /// The frame that the commands that are being carried out make or change keyframes at (`None` when none is being carried out, or none
    /// of them is about what has keyframes): see [`super::FastForwardApplication::frame_to_make_keyframes_at`].
    pub(super) frame_keyframes_are_made_at: Option<TimelineFrameIndex>,
    pub(super) textures: Textures,
    pub(super) timeline_view: TimelineViewState,
    /// How the picture of the preview is zoomed, and where it is.
    pub(super) preview_view: PreviewView,
    /// Where the preview panel was in the last frame, for the keys that mean something else when the pointer is over it.
    pub(super) preview_panel_in_the_last_frame: Option<egui::Rect>,
    pub(super) settings_windows_open: SettingsWindowsOpen,
    pub(super) dialog: Option<Dialog>,
    /// Whether the "source files not found" warning is showing.
    pub(super) missing_sources_warning_open: bool,
    /// Whether the window is to be closed as soon as nothing is being done in the background and no message is up (it was asked to close,
    /// and was kept open for that).
    pub(super) close_when_not_busy: bool,
    /// Set once the user has agreed to close the window with unsaved changes.
    pub(super) close_confirmed: bool,
}

impl Interface {
    pub(super) fn new(startup_dialog: Option<Dialog>) -> Self {
        Self {
            selection: Selection::empty(),
            gesture_frame: GestureFrame::default(),
            frame_keyframes_are_made_at: None,
            textures: Textures::default(),
            timeline_view: TimelineViewState::default(),
            preview_view: PreviewView::default(),
            preview_panel_in_the_last_frame: None,
            settings_windows_open: SettingsWindowsOpen::default(),
            dialog: startup_dialog,
            missing_sources_warning_open: false,
            close_when_not_busy: false,
            close_confirmed: false,
        }
    }
}

/// What finds out how the pictures of the video at a place are stored and what colours they say they stand for, or cannot tell (`None`). It runs
/// on a thread of its own. (It is ffprobe; a test gives the editor another.)
pub(super) type VideoFormatProber = Arc<dyn Fn(&MediaLocation) -> Option<VideoStreamFormat> + Send + Sync>;

/// What the editor is given by the computer it runs on: ffmpeg, whoever asks which file to open, the settings that are kept in a file, the
/// fonts, and the way to wake the window up from another thread.
pub(super) struct Environment {
    pub(super) toolchain: Option<std::sync::Arc<FfmpegToolchain>>,
    /// Finds out how the pictures of a video are stored (see [`Playback::planes_wanted`]).
    pub(super) video_format_prober: VideoFormatProber,
    /// What asks the person which file to open or where to save one.
    pub(super) file_chooser: Box<dyn FileChooser>,
    pub(super) wake_up: UserInterfaceWakeUp,
    pub(super) text_renderer: LazyTextRenderer,
    /// The fonts the computer has, once they are known.
    pub(super) font_families: Option<Vec<String>>,
    /// The settings that are the same for every project.
    pub(super) global_settings: Settings,
    /// Where the global settings are kept. `None` keeps them only while the program runs.
    pub(super) global_settings_path: Option<PathBuf>,
}
