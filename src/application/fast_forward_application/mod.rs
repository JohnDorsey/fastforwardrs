//! The application itself: the state of the editor, and what happens on each frame of the window.
//!
//! The work is divided between files by what it is about, all adding to the one
//! [`FastForwardApplication`]:
//!
//! * this file: the state, starting up, and the loop that runs once for each frame drawn;
//! * `drawing`: laying out the window (the menu bar, the preview, the timeline);
//! * `command_execution`: what each [`Command`] does;
//! * `dialog_flow`: messages, questions, the export's progress and the missing-video warning;
//! * `source_video_flow`: finding, opening and relinking the source video;
//! * `project_file_flow`: new, load and save;
//! * `settings_flow`: the settings in effect, and keeping the global ones in their file;
//! * `autosave`: noting unsaved changes, and saving them by itself when the setting is on;
//! * `export_flow`: exporting and its progress;
//! * `programmatic_interface`: what the window does, without its file dialogs, and a look at the
//!   state afterwards (for tests, and for scripting).

mod autosave;
mod command_execution;
mod dialog_flow;
mod drawing;
mod export_flow;
#[cfg(test)]
mod interaction_tests;
mod programmatic_interface;
mod project_file_flow;
mod settings_flow;
#[cfg(test)]
mod settings_tests;
mod source_video_flow;
#[cfg(test)]
mod test_window;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::Context;

use super::background::BackgroundTask;
use super::commands::Command;
use super::dialogs::Dialog;
use super::keyboard;
use super::opening::OpeningOutcome;
use super::settings_windows::{self, SettingsWindowsOpen};
use super::textures::Textures;
use super::theme;
use super::timeline_panel::TimelineViewState;
use crate::media::export::VideoExport;
use crate::media::{FfmpegToolchain, ThumbnailExtraction, ThumbnailStrip, ToolchainError, UserInterfaceWakeUp};
use crate::playback::{
    AudioContent, AudioSupplier, ClockReading, FfmpegAudioStreamStarter, FfmpegPictureStreamStarter, PictureSupplier,
    PlaybackClock, opener_of_the_default_speakers, wanted_picture,
};
use crate::project::{FrameRate, Project, Settings, TimelineFrameIndex};
use crate::settings_file::default_settings_file_path;

/// The frame rate used (for the timecode and the clock) while the project has no video to take one from.
fn frame_rate_without_a_video() -> FrameRate {
    FrameRate::from_whole_frames_per_second(30).expect("30 frames per second is a frame rate")
}

/// Whether the project's source video can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceStatus {
    /// The project has no video.
    NoVideo,
    /// The video is being looked for and opened.
    Opening,
    /// The video is found and opens.
    Available { path: PathBuf },
    /// The video cannot be found or opened.
    Unavailable { reason: String },
}

/// Why a video is being looked for, which decides what happens when it is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpeningPurpose {
    /// A video was chosen for a project that has none: it becomes the project's video.
    ImportIntoEmptyProject,
    /// A video was chosen to replace the project's missing one. The edits are kept.
    RelinkMissingVideo,
    /// A project was just loaded and the places its video might be are being tried.
    LocateForLoadedProject,
}

struct OpeningTask {
    purpose: OpeningPurpose,
    task: BackgroundTask<OpeningOutcome>,
}

struct RunningExport {
    export: VideoExport,
    frames_written: u64,
    total_frames: u64,
    /// Frames of the timeline the export leaves out, for clips past the end of the source video.
    frames_left_out: u64,
    destination: PathBuf,
}

/// The thumbnail pictures of the source video, and the work of making them.
struct ThumbnailState {
    strip: ThumbnailStrip,
    extraction: Option<ThumbnailExtraction>,
}

/// What was worked out at the start of a frame, which the drawing then uses.
struct FrameState {
    current_frame: TimelineFrameIndex,
    total_frames: u64,
    frame_rate: FrameRate,
    is_playing: bool,
    /// Whether the preview has exactly the picture for the current frame.
    preview_is_up_to_date: bool,
}

pub struct FastForwardApplication {
    toolchain: Option<Arc<FfmpegToolchain>>,
    project: Project,
    /// Where the project was last saved or loaded from.
    project_file_path: Option<PathBuf>,
    has_unsaved_changes: bool,
    /// When the project last changed in a way that has not been saved; see [`autosave`]. `None`
    /// when nothing is waiting to be autosaved.
    last_unsaved_change_at: Option<Instant>,
    /// The settings that are the same for every project.
    global_settings: Settings,
    /// Where the global settings are kept. `None` keeps them only while the program runs.
    global_settings_path: Option<PathBuf>,
    settings_windows_open: SettingsWindowsOpen,
    source_status: SourceStatus,
    opening: Option<OpeningTask>,
    clock: PlaybackClock,
    picture_supplier: PictureSupplier<FfmpegPictureStreamStarter>,
    /// Plays the sound of the preview. It is silent until given speakers to play through.
    audio_supplier: AudioSupplier<FfmpegAudioStreamStarter>,
    thumbnails: ThumbnailState,
    textures: Textures,
    timeline_view: TimelineViewState,
    dialog: Option<Dialog>,
    /// Whether the "source video not found" warning is showing.
    missing_source_warning_open: bool,
    export: Option<RunningExport>,
    /// Set once the user has agreed to close the window with unsaved changes.
    close_confirmed: bool,
    wake_up: UserInterfaceWakeUp,
}

impl FastForwardApplication {
    /// Starts the editor in the window `creation_context` belongs to, with the global settings
    /// kept in their usual file. If `project_to_open` is given (the path of a project file, as when
    /// one is opened from the command line) it is loaded.
    pub fn new(creation_context: &eframe::CreationContext<'_>, project_to_open: Option<PathBuf>) -> Self {
        let mut application = Self::with_context(&creation_context.egui_ctx, FfmpegToolchain::locate());
        if let Some(settings_path) = default_settings_file_path() {
            application.keep_global_settings_in(settings_path);
        }
        application
            .audio_supplier
            .play_through(opener_of_the_default_speakers());
        if let Some(path) = project_to_open {
            application.load_project(path);
        }
        application
    }

    /// An editor drawing into `context`, using `toolchain` (or reporting that ffmpeg is missing).
    /// The global settings are the defaults and are not kept in any file; see
    /// [`Self::keep_global_settings_in`].
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

        Self {
            picture_supplier: PictureSupplier::new(FfmpegPictureStreamStarter::new(
                toolchain.clone(),
                Arc::clone(&wake_up),
            )),
            audio_supplier: AudioSupplier::new(FfmpegAudioStreamStarter::new(toolchain.clone(), Arc::clone(&wake_up))),
            toolchain,
            project: Project::empty(),
            project_file_path: None,
            has_unsaved_changes: false,
            last_unsaved_change_at: None,
            global_settings: Settings::DEFAULT,
            global_settings_path: None,
            settings_windows_open: SettingsWindowsOpen::default(),
            source_status: SourceStatus::NoVideo,
            opening: None,
            clock: PlaybackClock::new(frame_rate_without_a_video()),
            thumbnails: ThumbnailState {
                strip: ThumbnailStrip::default(),
                extraction: None,
            },
            textures: Textures::default(),
            timeline_view: TimelineViewState::default(),
            dialog: startup_dialog,
            missing_source_warning_open: false,
            export: None,
            close_confirmed: false,
            wake_up,
        }
    }

    /// The frame rate of the project's video, or a stand-in while there is no video.
    fn frame_rate(&self) -> FrameRate {
        self.project
            .timeline_frame_rate()
            .unwrap_or_else(frame_rate_without_a_video)
    }

    /// Stops playback before a window opens that waits for an answer (a file dialog, say), so
    /// that the video does not carry on playing unseen behind it.
    fn pause_playback_for_dialog(&mut self) {
        let total_frames = self.project.video_track().total_frame_count();
        self.clock.pause(Instant::now(), total_frames);
    }

    /// Whether a dialog, an export or the opening of a video is in the way of editing.
    fn is_blocked(&self) -> bool {
        self.dialog.is_some() || self.export.is_some()
    }

    /// Everything that has to be done at the start of a frame: collecting what background work has
    /// finished, moving the clock on, and getting the preview picture.
    fn prepare_frame(&mut self, context: &Context, now: Instant) -> FrameState {
        self.collect_finished_background_work();

        let total_frames = self.project.video_track().total_frame_count();
        self.clock.stop_at_the_end(now, total_frames);
        let current_frame = self.clock.position(now, total_frames);
        let is_playing = self.clock.is_playing();

        let wanted = wanted_picture(self.project.video_track(), current_frame);
        let report = self.picture_supplier.supply(wanted, is_playing, now);
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
            frame_rate: self.frame_rate(),
            is_playing,
            preview_is_up_to_date: report.shows_the_wanted_picture,
        }
    }

    /// Starts, keeps in step, or stops the sound of the preview: it plays while the video plays.
    fn update_the_sound(&mut self, now: Instant, total_frames: u64) {
        let clock = ClockReading {
            frame: self.clock.position(now, total_frames),
            seconds: self.clock.position_in_seconds(now, total_frames),
            is_playing: self.clock.is_playing(),
        };
        let mute_sped_up_clips = self.effective_settings().mute_sped_up_clips;
        // There is sound to play when the video can be read and has some.
        let content = match (&self.source_status, self.project.source_videos().first()) {
            (SourceStatus::Available { path }, Some(video)) if video.properties.has_audio => Some(AudioContent {
                video_path: path,
                frame_rate: video.properties.frame_rate,
                source_frame_count: video.properties.total_frame_count,
                track: self.project.video_track(),
                mute_sped_up_clips,
            }),
            _ => None,
        };
        self.audio_supplier.update(content, clock, now);
    }

    /// Turns the keyboard into commands (see [`keyboard`]), unless something is in the way.
    fn collect_keyboard_commands(&self, context: &Context, frame_state: &FrameState, commands: &mut Vec<Command>) {
        if self.is_blocked() || frame_state.total_frames == 0 {
            return;
        }
        commands.extend(keyboard::commands_for_the_keys_pressed(
            context,
            frame_state.current_frame,
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
        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::WINDOW_BACKGROUND);

        self.handle_close_request(&context);
        let frame_state = self.prepare_frame(&context, now);

        let mut commands = Vec::new();
        self.collect_keyboard_commands(&context, &frame_state, &mut commands);
        self.draw(ui, &frame_state, &mut commands);
        settings_windows::show(
            &context,
            &mut self.settings_windows_open,
            self.project.settings(),
            self.global_settings,
            &mut commands,
        );
        self.show_dialogs(&context);
        for command in commands {
            self.execute(command, now, &context);
        }
        self.autosave_when_quiet_for_long_enough(&context, now);

        self.textures.forget_thumbnails_not_used_this_frame();
    }
}
