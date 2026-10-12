//! The application itself: the state of the editor, and what happens on each frame of the window.
//!
//! The work is divided between files by what it is about, all adding to the one
//! [`FastForwardApplication`]:
//!
//! * this file: the state, starting up, and the loop that runs once for each frame drawn;
//! * `drawing`: laying out the window (the menu bar and the three panels: the properties, the preview, the
//!   timeline);
//! * `command_execution`: what each [`Command`] does;
//! * `dialog_flow`: messages, questions, the export's progress and the missing-files warning;
//! * `source_media_flow`: finding, opening and relinking the source files (videos and sounds),
//!   and making the thumbnails and waveforms for the timeline;
//! * `project_file_flow`: new, load and save;
//! * `settings_flow`: the settings in effect, and keeping the global ones in their file;
//! * `autosave`: noting unsaved changes, and saving them by itself when the setting is on;
//! * `export_flow`: exporting and its progress;
//! * `programmatic_interface`: what the window does, without its file dialogs, and a look at the
//!   state afterwards (for tests, and for scripting).

mod animating;
mod autosave;
#[cfg(test)]
mod clip_drag_tests;
#[cfg(test)]
mod clip_look_tests;
mod command_execution;
#[cfg(test)]
mod cut_feedback_tests;
mod dialog_flow;
#[cfg(test)]
mod drawer_tests;
mod drawing;
mod export_flow;
#[cfg(test)]
mod file_question_tests;
#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod keyframe_panel_tests;
#[cfg(test)]
mod keyframe_tests;
#[cfg(test)]
mod picture_tests;
#[cfg(test)]
mod preview_layer_tests;
mod programmatic_interface;
mod project_file_flow;
#[cfg(test)]
mod properties_panel_tests;
#[cfg(test)]
mod selection_tests;
mod settings_flow;
#[cfg(test)]
mod settings_tests;
mod source_media_flow;
#[cfg(test)]
mod test_window;
#[cfg(test)]
mod text_object_tests;
#[cfg(test)]
mod track_height_tests;
#[cfg(test)]
mod track_reorder_tests;
#[cfg(test)]
mod track_selection_tests;
#[cfg(test)]
mod track_tests;
#[cfg(test)]
mod undo_tests;
#[cfg(test)]
mod zoom_tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::Context;

use super::background::BackgroundTask;
use super::commands::Command;
use super::dialogs::Dialog;
use super::file_chooser::{FileChooser, NativeFileChooser, RefusingFileChooser};
use super::history::History;
use super::keyboard::{self, KeyboardState};
use super::opening::OpeningOutcome;
use super::selection::Selection;
use super::settings_windows::{self, SettingsWindowsOpen};
use super::textures::Textures;
use super::theme;
use super::timeline_panel::TimelineViewState;
use crate::composition::{PicturePlan, plan_pictures};
use crate::media::export::VideoExport;
use crate::media::{
    FfmpegToolchain, FinerThumbnails, ThumbnailExtraction, ThumbnailLibrary, ToolchainError, UserInterfaceWakeUp,
    Waveform, WaveformExtraction,
};
use crate::playback::{
    AudioContent, AudioSupplier, ClockReading, FfmpegAudioStreamStarter, FfmpegPictureStreamStarter,
    PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY, PicturesReport, PlaybackClock, PreviewPictures, PreviewSetup,
    PreviewVideo, SoundEffects, opener_of_the_default_effect_speakers, opener_of_the_default_speakers,
    preview_picture_size, wanted_pictures,
};
use crate::project::{FrameRate, Project, Settings, SourceMediaIdentifier, TimelineFrameIndex};
use crate::rgba_picture::RgbaPicture;
use crate::settings_file::default_settings_file_path;
use crate::text_rendering::LazyTextRenderer;

/// Whether a source file of the project can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceStatus {
    /// The file is being looked for and opened.
    Opening,
    /// The file is found and opens.
    Available { path: PathBuf },
    /// The file cannot be found or opened.
    Unavailable { reason: String },
}

/// Why a file is being looked for, which decides what happens when it is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpeningPurpose {
    /// A file was chosen to be put on the timeline, at the current frame.
    ImportNewSource,
    /// A file was chosen to replace a source file of the project that cannot be found. The edits
    /// are kept.
    RelinkMissingSource(SourceMediaIdentifier),
    /// A project was just loaded and the places one of its files might be are being tried.
    LocateForLoadedProject(SourceMediaIdentifier),
}

struct OpeningTask {
    purpose: OpeningPurpose,
    task: BackgroundTask<OpeningOutcome>,
}

struct RunningExport {
    export: VideoExport,
    frames_written: u64,
    total_frames: u64,
    /// Frames of the finished video that are missing the picture of a video only because a video the
    /// project is made from has fewer pictures than its clips need.
    frames_missing_a_picture: u64,
    destination: PathBuf,
}

/// The thumbnail pictures of a source video, and the work of making them.
#[derive(Default)]
struct ThumbnailState {
    library: ThumbnailLibrary,
    /// Making the picture of every second of the whole video, which is done once, when it is opened.
    extraction: Option<ThumbnailExtraction>,
    /// Making finer pictures of the stretch of the video that the timeline shows.
    finer: FinerThumbnails,
}

/// The shape of the sound of a source sound file, and the work of making it.
#[derive(Default)]
struct WaveformState {
    waveform: Waveform,
    extraction: Option<WaveformExtraction>,
}

/// The decoded picture of an image file, and the work of decoding it.
#[derive(Default)]
struct PictureState {
    /// Decoding the picture, which is done once, when the file is opened.
    load: Option<BackgroundTask<Result<RgbaPicture, String>>>,
    /// The picture, once it is decoded.
    picture: Option<RgbaPicture>,
}

/// What the editor knows while it runs about one source file of the project.
struct SourceFileState {
    status: SourceStatus,
    /// The pictures for the timeline, for a video.
    thumbnails: ThumbnailState,
    /// The waveform for the timeline, for a sound file.
    waveform: WaveformState,
    /// The picture, for an image file.
    picture: PictureState,
}

impl SourceFileState {
    fn new(status: SourceStatus) -> Self {
        Self {
            status,
            thumbnails: ThumbnailState::default(),
            waveform: WaveformState::default(),
            picture: PictureState::default(),
        }
    }

    fn path_if_available(&self) -> Option<&std::path::Path> {
        match &self.status {
            SourceStatus::Available { path } => Some(path),
            _ => None,
        }
    }

    /// Where the file is, if it can be used.
    fn status_path(&self) -> Option<PathBuf> {
        self.path_if_available().map(std::path::Path::to_owned)
    }
}

/// What was worked out at the start of a frame, which the drawing then uses.
struct FrameState {
    current_frame: TimelineFrameIndex,
    total_frames: u64,
    frame_rate: FrameRate,
    is_playing: bool,
    /// What the picture is made of, at every frame of the timeline.
    picture_plan: PicturePlan,
    /// Which of the pictures of videos that the preview needs for the current frame it has exactly.
    pictures_report: PicturesReport,
}

pub struct FastForwardApplication {
    toolchain: Option<Arc<FfmpegToolchain>>,
    project: Project,
    /// Where the project was last saved or loaded from.
    project_file_path: Option<PathBuf>,
    /// The edits that can be undone, and the ones that were undone.
    history: History,
    /// Whether something that is not in the history was changed since the project was saved: a
    /// setting of the project, or the place a file was found.
    unsaved_change_outside_history: bool,
    /// When the project last changed in a way that has not been saved; see [`autosave`]. `None`
    /// when nothing is waiting to be autosaved.
    last_unsaved_change_at: Option<Instant>,
    /// The settings that are the same for every project.
    global_settings: Settings,
    /// Where the global settings are kept. `None` keeps them only while the program runs.
    global_settings_path: Option<PathBuf>,
    settings_windows_open: SettingsWindowsOpen,
    /// What is known about each of the project's source files while the program runs.
    source_files: HashMap<SourceMediaIdentifier, SourceFileState>,
    /// Source files that are being looked for and opened, in the background.
    openings: Vec<OpeningTask>,
    /// The clips that were clicked, which Delete deletes and S cuts.
    selection: Selection,
    text_renderer: LazyTextRenderer,
    /// The fonts the computer has, once they are known.
    font_families: Option<Vec<String>>,
    clock: PlaybackClock,
    /// Gets the pictures of the videos that are seen, a stream of them for each track.
    preview_pictures: PreviewPictures<FfmpegPictureStreamStarter>,
    /// Plays the sound of the preview. It is silent until given speakers to play through.
    audio_supplier: AudioSupplier<FfmpegAudioStreamStarter>,
    /// Plays the editor's own little sounds, such as the snip of a cut. It is silent until given speakers to
    /// play through.
    sound_effects: SoundEffects,
    textures: Textures,
    timeline_view: TimelineViewState,
    dialog: Option<Dialog>,
    /// Whether the "source files not found" warning is showing.
    missing_sources_warning_open: bool,
    export: Option<RunningExport>,
    /// Set once the user has agreed to close the window with unsaved changes.
    close_confirmed: bool,
    /// What asks the person which file to open or where to save one.
    file_chooser: Box<dyn FileChooser>,
    wake_up: UserInterfaceWakeUp,
}

impl FastForwardApplication {
    /// Starts the editor in the window `creation_context` belongs to, with the global settings
    /// kept in their usual file. If `project_to_open` is given (the path of a project file, as when
    /// one is opened from the command line) it is loaded.
    pub fn new(creation_context: &eframe::CreationContext<'_>, project_to_open: Option<PathBuf>) -> Self {
        let mut application = Self::with_context(&creation_context.egui_ctx, FfmpegToolchain::locate());
        application.file_chooser = Box::new(NativeFileChooser);
        if let Some(settings_path) = default_settings_file_path() {
            application.keep_global_settings_in(settings_path);
        }
        application.use_the_default_speakers();
        if let Some(path) = project_to_open {
            application.load_project(path);
        }
        application
    }

    /// Has the editor play the sound of the preview, and its own little sounds, through the computer's default speakers
    /// (which are opened when the first sound is made). Without this it is silent, which is what the tests want.
    fn use_the_default_speakers(&mut self) {
        self.audio_supplier.play_through(opener_of_the_default_speakers());
        self.sound_effects.play_through(opener_of_the_default_effect_speakers());
    }

    /// An editor drawing into `context`, using `toolchain` (or reporting that ffmpeg is missing).
    /// The global settings are the defaults and are not kept in any file; see
    /// [`Self::keep_global_settings_in`]. It has no one to ask for files: reaching a file dialog
    /// is a failure (the editor is given a file chooser that panics when asked), and what a dialog
    /// would have chosen is given to the editor directly (see the programmatic interface).
    pub fn with_context(context: &Context, toolchain: Result<FfmpegToolchain, ToolchainError>) -> Self {
        context.set_visuals(egui::Visuals::dark());
        let wake_up: UserInterfaceWakeUp = {
            let context = context.clone();
            Arc::new(move || context.request_repaint())
        };

        let (toolchain, startup_dialog) = match toolchain {
            Ok(toolchain) => (Some(Arc::new(toolchain)), None),
            Err(error) => (
                None,
                Some(Dialog::Message {
                    title: "ffmpeg was not found".to_owned(),
                    text: error.to_string(),
                }),
            ),
        };

        let project = Project::empty();
        Self {
            preview_pictures: PreviewPictures::new(FfmpegPictureStreamStarter::new(
                toolchain.clone(),
                Arc::clone(&wake_up),
            )),
            audio_supplier: AudioSupplier::new(FfmpegAudioStreamStarter::new(toolchain.clone(), Arc::clone(&wake_up))),
            sound_effects: SoundEffects::default(),
            toolchain,
            clock: PlaybackClock::new(project.frame_rate()),
            project,
            project_file_path: None,
            history: History::new(),
            unsaved_change_outside_history: false,
            last_unsaved_change_at: None,
            global_settings: Settings::DEFAULT,
            global_settings_path: None,
            settings_windows_open: SettingsWindowsOpen::default(),
            source_files: HashMap::new(),
            openings: Vec::new(),
            selection: Selection::empty(),
            text_renderer: LazyTextRenderer::new(Arc::clone(&wake_up)),
            font_families: None,
            textures: Textures::default(),
            timeline_view: TimelineViewState::default(),
            dialog: startup_dialog,
            missing_sources_warning_open: false,
            export: None,
            close_confirmed: false,
            file_chooser: Box::new(RefusingFileChooser),
            wake_up,
        }
    }

    /// Whether the source file `identifier` has been found and opens.
    fn source_is_available(&self, identifier: SourceMediaIdentifier) -> bool {
        self.source_files
            .get(&identifier)
            .is_some_and(|state| state.path_if_available().is_some())
    }

    /// Where each source file that can be used is.
    fn available_source_paths(&self) -> HashMap<SourceMediaIdentifier, PathBuf> {
        self.source_files
            .iter()
            .filter_map(|(identifier, state)| Some((*identifier, state.path_if_available()?.to_owned())))
            .collect()
    }

    /// Stops playback before a window opens that waits for an answer (a file dialog, say), so
    /// that the video does not carry on playing unseen behind it.
    fn pause_playback_for_dialog(&mut self) {
        let total_frames = self.project.total_frame_count();
        self.clock.pause(Instant::now(), total_frames);
    }

    /// Whether a dialog or an export is in the way of editing.
    fn is_blocked(&self) -> bool {
        self.dialog.is_some() || self.export.is_some()
    }

    /// Everything that has to be done at the start of a frame: collecting what background work has
    /// finished, moving the clock on, and getting the preview picture.
    fn prepare_frame(&mut self, context: &Context, now: Instant) -> FrameState {
        self.collect_finished_background_work();
        self.forget_what_is_gone_from_the_project();
        self.synchronise_the_preview_with_the_project();

        let total_frames = self.project.total_frame_count();
        self.clock.stop_at_the_end(now, total_frames);
        let current_frame = self.clock.position(now, total_frames);
        let is_playing = self.clock.is_playing();

        let picture_plan = plan_pictures(&self.project, |source| self.source_is_available(source));
        let look_ahead_frames = if is_playing {
            PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY
        } else {
            0
        };
        let wanted = wanted_pictures(&picture_plan, current_frame, look_ahead_frames);
        let report = self.preview_pictures.supply(&wanted, is_playing, now);
        if let Some(wait) = report.ask_again_in {
            context.request_repaint_after(wait);
        }
        if is_playing {
            context.request_repaint();
        }
        self.update_the_sound(now, total_frames);

        FrameState {
            current_frame,
            total_frames,
            frame_rate: self.project.frame_rate(),
            is_playing,
            picture_plan,
            pictures_report: report,
        }
    }

    /// Lets go of a selection that is about a clip or a track that is not there any more.
    fn forget_what_is_gone_from_the_project(&mut self) {
        self.selection.forget_what_is_gone(&self.project);
    }

    /// Tells the picture supplier which videos it can get pictures from, and how big the pictures
    /// are, whenever that has changed (a file was found, or the project was replaced).
    fn synchronise_the_preview_with_the_project(&mut self) {
        let videos: Vec<PreviewVideo> = self
            .project
            .sources()
            .iter()
            .filter_map(|source| {
                let video = source.properties.video()?;
                Some(PreviewVideo {
                    source: source.identifier,
                    video_path: self
                        .source_files
                        .get(&source.identifier)?
                        .path_if_available()?
                        .to_owned(),
                    display_size_in_pixels: (video.display_width_in_pixels, video.display_height_in_pixels),
                    frame_rate: video.frame_rate,
                })
            })
            .collect();
        let wanted_setup = (!videos.is_empty()).then(|| {
            let (picture_width, picture_height) = self.project.picture_size();
            let (picture_width_in_pixels, picture_height_in_pixels) =
                preview_picture_size(picture_width, picture_height);
            PreviewSetup {
                frame_rate: self.project.frame_rate(),
                picture_width_in_pixels,
                picture_height_in_pixels,
                videos,
            }
        });
        if self.preview_pictures.setup() != wanted_setup.as_ref() {
            self.preview_pictures.set_setup(wanted_setup);
        }
    }

    /// Starts, keeps in step, or stops the sound of the preview: it plays while the video plays.
    fn update_the_sound(&mut self, now: Instant, total_frames: u64) {
        let clock = ClockReading {
            frame: self.clock.position(now, total_frames),
            seconds: self.clock.position_in_seconds(now, total_frames),
            is_playing: self.clock.is_playing(),
        };
        let sped_up_clip_sound = self.effective_settings().sped_up_clip_sound;
        let source_paths = self.available_source_paths();
        // There is sound to play when some file that can be read is there to make it from.
        let content = (!source_paths.is_empty()).then_some(AudioContent {
            project: &self.project,
            source_paths: &source_paths,
            sped_up_clip_sound,
        });
        self.audio_supplier.update(content, clock, now);
    }

    /// Turns the keyboard into commands (see [`keyboard`]), unless something is in the way.
    fn collect_keyboard_commands(&self, context: &Context, frame_state: &FrameState, commands: &mut Vec<Command>) {
        if self.is_blocked() {
            return;
        }
        let selected_clips_at_the_current_frame = self
            .selection
            .iter()
            .filter(|clip| {
                self.project
                    .placement_of(*clip)
                    .is_some_and(|placement| placement.contains(frame_state.current_frame))
            })
            .collect();
        commands.extend(keyboard::commands_for_the_keys_pressed(
            context,
            KeyboardState {
                current_frame: frame_state.current_frame,
                is_playing: frame_state.is_playing,
                timeline_is_empty: frame_state.total_frames == 0,
                selected_clips: self.selection.to_vec(),
                selected_clips_at_the_current_frame,
                can_undo: self.history.can_undo(),
                can_redo: self.history.can_redo(),
            },
        ));
    }
}

impl eframe::App for FastForwardApplication {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl FastForwardApplication {
    /// Draws one frame of the window into `ui`, and carries out what the user asked for in it.
    /// This is all of the editor's work for a frame: the window system calls it for every frame
    /// it draws (through [`eframe::App::ui`]), and the tests call it directly.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let now = Instant::now();
        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::GAP_BACKGROUND);

        self.handle_close_request(&context);
        let frame_state = self.prepare_frame(&context, now);

        let mut commands = Vec::new();
        self.collect_keyboard_commands(&context, &frame_state, &mut commands);
        self.draw(ui, &frame_state, &mut commands);
        self.ask_for_finer_thumbnails();
        settings_windows::show(
            &context,
            &mut self.settings_windows_open,
            (self.project.settings(), self.project.frame_rate()),
            self.global_settings,
            &mut commands,
        );
        self.show_dialogs(&context);
        self.execute_all(commands, now, &context);
        self.autosave_when_quiet_for_long_enough(&context, now);

        self.textures.forget_thumbnails_not_used_this_frame();
    }
}
