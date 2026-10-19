// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The application itself: the state of the editor, and what happens on each frame of the window.
//!
//! The work is divided between files by what it is about, all adding to the one
//! [`FastForwardApplication`]:
//!
//! * this file: starting up, and the loop that runs once for each frame drawn;
//! * `state`: the state, in six groups by what it is about (the document, the playback, the source files, the background jobs, the
//!   interface, and what the computer gives the editor);
//! * `drawing`: laying out the window (the menu bar and the three panels: the properties, the preview, the
//!   timeline);
//! * `command_execution`: what each [`Command`] does;
//! * `dialog_flow`: messages, questions, the export's progress and the missing-files warning;
//! * `losing_keyframes`: the question that is asked before a change that would lose keyframes is made;
//! * `source_media_flow`: finding, opening and relinking the source files (videos and sounds),
//!   and making the thumbnails and waveforms for the timeline;
//! * `project_file_flow`: new, load and save;
//! * `settings_flow`: the settings in effect, and keeping the global ones in their file;
//! * `autosave`: noting unsaved changes, and saving them by itself when the setting is on;
//! * `included_files_flow`: including files in the project file (which is then an archive) and taking them out again;
//! * `export_flow`: exporting and its progress;
//! * `programmatic_interface`: what the window does, without its file dialogs, and a look at the
//!   state afterwards (for tests, and for scripting).

mod animating;
mod autosave;
#[cfg(test)]
mod chain_tests;
#[cfg(test)]
mod clip_drag_tests;
#[cfg(test)]
mod clip_look_tests;
mod command_execution;
#[cfg(test)]
mod cut_feedback_tests;
mod dialog_flow;
#[cfg(test)]
mod drawer_motion_tests;
#[cfg(test)]
mod drawer_sound_tests;
#[cfg(test)]
mod drawer_tests;
mod drawing;
#[cfg(test)]
mod duplicate_tests;
#[cfg(test)]
mod effect_menu_tests;
#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod embed_and_import_tests;
#[cfg(test)]
mod engine_window_tests;
mod export_flow;
pub(crate) use export_flow::effects_the_export_would_leave_out;
#[cfg(test)]
mod file_colour_tests;
#[cfg(test)]
mod file_question_tests;
#[cfg(test)]
mod grip_dot_tests;
#[cfg(test)]
mod group_box_tests;
#[cfg(test)]
mod group_command_fuzz_tests;
#[cfg(test)]
mod group_drag_tests;
#[cfg(test)]
mod group_edge_tests;
mod group_flow;
#[cfg(test)]
mod group_menu_tests;
#[cfg(test)]
mod group_outline_tests;
#[cfg(test)]
mod group_refusal_tests;
#[cfg(test)]
mod group_render_tests;
#[cfg(test)]
mod group_title_bar_tests;
mod included_files_flow;
#[cfg(test)]
mod included_files_tests;
#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod keyframe_drag_tests;
#[cfg(test)]
mod keyframe_loss_tests;
#[cfg(test)]
mod keyframe_panel_tests;
#[cfg(test)]
mod keyframe_tests;
mod losing_keyframes;
#[cfg(test)]
mod navigation_key_tests;
#[cfg(test)]
mod picture_tests;
#[cfg(test)]
mod playing_edit_tests;
mod plugin_flow;
#[cfg(test)]
mod plugin_tests;
#[cfg(test)]
mod preview_layer_tests;
#[cfg(test)]
mod preview_zoom_tests;
mod programmatic_interface;
mod project_file_flow;
#[cfg(test)]
mod properties_panel_tests;
#[cfg(test)]
mod rack_command_tests;
#[cfg(test)]
mod rack_drag_tests;
#[cfg(test)]
mod rack_group_drag_tests;
#[cfg(test)]
mod rack_mouse_tests;
#[cfg(test)]
mod rack_selection_tests;
#[cfg(test)]
mod rack_tests;
mod saving;
#[cfg(test)]
mod saving_tests;
#[cfg(test)]
mod selection_tests;
mod settings_flow;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod sound_plugin_folder_tests;
mod sound_plugins_flow;
#[cfg(test)]
mod sound_plugins_tests;
mod source_media_flow;
mod state;
#[cfg(test)]
mod still_window_tests;
mod table_flow;
mod table_keeping_flow;
#[cfg(test)]
mod table_keeping_tests;
#[cfg(test)]
mod table_tests;
#[cfg(test)]
mod test_window;
#[cfg(test)]
mod text_object_tests;
#[cfg(test)]
mod tidying_tests;
#[cfg(test)]
mod track_height_tests;
#[cfg(test)]
mod track_reorder_tests;
#[cfg(test)]
mod track_selection_tests;
#[cfg(test)]
mod track_tests;
#[cfg(test)]
mod transport_row_tests;
#[cfg(test)]
mod undo_tests;
#[cfg(test)]
mod vertical_zoom_tests;
#[cfg(test)]
mod video_format_tests;
#[cfg(test)]
mod view_history_tests;
#[cfg(test)]
mod zoom_tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::Context;

use super::archive_cache::ArchiveCache;
use super::background::BackgroundTask;
use super::colour_of_a_file_window;
use super::commands::Command;
use super::dialogs::Dialog;
use super::file_chooser::{NativeFileChooser, RefusingFileChooser};
use super::keyboard::{self, KeyboardState};
use super::mouse_buttons;
use super::opening::OpeningOutcome;
use super::preview_panel::EnginePreview;
use super::settings_windows;
use super::theme;
use super::timeline_panel::DrawerSound;
use crate::composition::{PicturePlan, plan_pictures};
use crate::engine::AdapterChoice;
use crate::media::{
    FfmpegToolchain, FinerThumbnails, MediaLocation, ThumbnailExtraction, ThumbnailLibrary, ToolchainError,
    UserInterfaceWakeUp, VideoStreamFormat, Waveform, WaveformExtraction,
};
use crate::playback::{
    AudioContent, AudioSupplier, FfmpegAudioStreamStarter, FfmpegPictureStreamStarter,
    PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY, PicturesReport, PlaybackClock, PreviewPictures, PreviewSetup,
    PreviewVideo, SoundEffect, SoundEffects, opener_of_the_default_effect_speakers, opener_of_the_default_speakers,
    preview_picture_size, wanted_pictures,
};
use crate::project::{FrameRate, Project, Settings, SourceMediaIdentifier, TimelineFrameIndex};
use crate::rgba_picture::RgbaPicture;
use crate::settings_file::default_settings_file_path;
use crate::text_rendering::LazyTextRenderer;
use crate::video_export::VideoExport;

use source_media_flow::ffprobe_as_the_video_format_prober;
use state::{BackgroundJobs, Document, Environment, Interface, Playback, SourceFiles};

/// Whether a source file of the project can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceStatus {
    /// The file is being looked for and opened.
    Opening,
    /// The file is found and opens, and its bytes are at `location`.
    Available { location: MediaLocation },
    /// The file cannot be found or opened.
    Unavailable { reason: String },
}

/// Why a file is being looked for, which decides what happens when it is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpeningPurpose {
    /// A file was chosen to be put on the timeline, at the current frame, where it stays the file that it is.
    ImportNewSource,
    /// A file was chosen to be put on the timeline, at the current frame, and in the project file too.
    ImportNewSourceAndEmbedIt,
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
    /// What a person said no to that the sound of the video would have had: the plugins that were not run because of it (one line each).
    plugins_not_run: Vec<String>,
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

/// How the pictures of a video are stored and what colours they say they stand for, which the preview needs to know to take them in planes, and the
/// work of finding that out. (It is not kept in the project file: it is asked of ffprobe again each time the file is found.)
#[derive(Default)]
struct VideoFormatState {
    /// Asking ffprobe, which is done once for each time the file is found, and only while the preview wants pictures in planes.
    probe: Option<BackgroundTask<Option<VideoStreamFormat>>>,
    /// Whether ffprobe was asked.
    was_asked: bool,
    /// What ffprobe said. `None` until it has, and for good if it could not say: the pictures of the video are then RGBA made by ffmpeg.
    known: Option<VideoStreamFormat>,
}

/// What the editor knows while it runs about one source file of the project.
struct SourceFileState {
    status: SourceStatus,
    /// The pictures for the timeline, for a video.
    thumbnails: ThumbnailState,
    /// How the pictures are stored, for a video.
    video_format: VideoFormatState,
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
            video_format: VideoFormatState::default(),
            waveform: WaveformState::default(),
            picture: PictureState::default(),
        }
    }

    /// Where the bytes of the file are, if it can be used.
    fn location_if_available(&self) -> Option<&MediaLocation> {
        match &self.status {
            SourceStatus::Available { location } => Some(location),
            _ => None,
        }
    }
}

/// Which drawing of the preview the environment asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WhichDrawing {
    /// Nothing was said about it: the engine, which is what the preview is made by.
    EngineByDefault,
    /// `FASTFORWARDRS_PREVIEW=engine`: the engine, and a message if it cannot be had.
    EngineByName,
    /// `FASTFORWARDRS_PREVIEW=old`: the window's own drawing, which the preview was made by before the engine, and is made by if the engine cannot start or stops.
    OldDrawing,
}

/// What is to be done about the picture engine when the program starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WhatToDoAboutTheEngine {
    Start,
    /// The preview is made the old way, and nothing is said.
    LeaveItAlone,
    /// The preview is made the old way, and a message says that the engine was asked for and cannot be had.
    LeaveItAloneAndSayWhy,
}

/// What is to be done about the engine, when `asked_for` is what the environment asks for and `window_is_drawn_by_wgpu` says whether the window is drawn by wgpu, which
/// the engine's device is made from the adapter of. Only someone who asked for the engine by name is told that it cannot be had: the others have not asked for anything.
fn what_to_do_about_the_engine(asked_for: WhichDrawing, window_is_drawn_by_wgpu: bool) -> WhatToDoAboutTheEngine {
    match (asked_for, window_is_drawn_by_wgpu) {
        (WhichDrawing::OldDrawing, _) | (WhichDrawing::EngineByDefault, false) => WhatToDoAboutTheEngine::LeaveItAlone,
        (WhichDrawing::EngineByName, false) => WhatToDoAboutTheEngine::LeaveItAloneAndSayWhy,
        (WhichDrawing::EngineByDefault | WhichDrawing::EngineByName, true) => WhatToDoAboutTheEngine::Start,
    }
}

/// What `FASTFORWARDRS_PREVIEW` (its value, if it is set) asks for. **The engine is the default** since 2026-10-16 (the maintainer looked at it on their graphics
/// cards and said so); until then it was started only by `engine`.
fn which_drawing_is_asked_for(value: Option<&str>) -> WhichDrawing {
    match value {
        Some("old") => WhichDrawing::OldDrawing,
        Some("engine") => WhichDrawing::EngineByName,
        _ => WhichDrawing::EngineByDefault,
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

/// The editor. Its state is in six groups (see `state.rs`) so that what belongs together is in one place.
pub struct FastForwardApplication {
    /// The project, and what is known of it as a file.
    document: Document,
    /// The clock, and what makes the pictures and the sound of the preview.
    playback: Playback,
    /// What is known about the files the project is made from.
    sources: SourceFiles,
    /// An export, a save, and other work that goes on in the background.
    jobs: BackgroundJobs,
    /// What the window shows and what is going on in it.
    interface: Interface,
    /// What the computer the editor runs on gives it.
    environment: Environment,
}

impl FastForwardApplication {
    /// Starts the editor in the window `creation_context` belongs to, with the global settings
    /// kept in their usual file. If `project_to_open` is given (the path of a project file, as when
    /// one is opened from the command line) it is loaded.
    pub fn new(creation_context: &eframe::CreationContext<'_>, project_to_open: Option<PathBuf>) -> Self {
        let mut application = Self::with_context(&creation_context.egui_ctx, FfmpegToolchain::locate());
        application.environment.file_chooser = Box::new(NativeFileChooser);
        if let Some(settings_path) = default_settings_file_path() {
            application.keep_global_settings_in(settings_path);
        }
        if let Some(approvals_file) = crate::plugin_trust::ApprovalStore::default_file() {
            application.keep_plugin_approvals_in(approvals_file);
        }
        application.find_plugins_the_way_a_person_uses_the_editor();
        application.use_the_default_speakers();
        application.environment.export_adapter = creation_context
            .wgpu_render_state
            .as_ref()
            .map(|render_state| AdapterChoice::Window(Arc::new(render_state.adapter.clone())));
        application.start_the_picture_engine_unless_the_old_drawing_is_asked_for(creation_context);
        application.document.archive_cache = ArchiveCache::in_the_usual_place();
        if let Some(path) = project_to_open {
            application.load_project(path);
        }
        application
    }

    /// Makes the preview with the picture engine, which is how it is made unless `FASTFORWARDRS_PREVIEW` is `old` (see [`which_drawing_is_asked_for`]), and
    /// when the window is drawn by wgpu, which the engine's device is made from the adapter of. If the engine cannot start or stops, the preview is made the old
    /// way from then on (see [`Self::go_back_to_the_old_drawing_if_the_engine_has_stopped`]).
    fn start_the_picture_engine_unless_the_old_drawing_is_asked_for(
        &mut self,
        creation_context: &eframe::CreationContext<'_>,
    ) {
        let asked_for = which_drawing_is_asked_for(std::env::var("FASTFORWARDRS_PREVIEW").ok().as_deref());
        let render_state = creation_context.wgpu_render_state.as_ref();
        match what_to_do_about_the_engine(asked_for, render_state.is_some()) {
            WhatToDoAboutTheEngine::LeaveItAlone => {}
            WhatToDoAboutTheEngine::LeaveItAloneAndSayWhy => self.show_message(
                "The picture engine cannot be used",
                "FASTFORWARDRS_PREVIEW=engine asks for the preview to be made by the picture engine, but the window is not drawn by wgpu, which the engine needs. The preview is made the old way.",
            ),
            WhatToDoAboutTheEngine::Start => {
                let render_state = render_state.expect("the engine is started only when the window is drawn by wgpu");
                self.use_the_picture_engine(EnginePreview::start(
                    AdapterChoice::Window(Arc::new(render_state.adapter.clone())),
                    Arc::clone(&self.environment.wake_up),
                ));
            }
        }
    }

    /// Makes the preview with this engine from now on. (The pictures of videos that are stored in a way that the engine can use as it is are then given to
    /// it in planes: ffmpeg does not make them RGBA.)
    fn use_the_picture_engine(&mut self, engine_preview: EnginePreview) {
        self.interface.engine_preview = Some(engine_preview);
        self.playback.planes_wanted = true;
    }

    /// If the picture engine has stopped (it could not start, or lost its device, or its thread ended), says so and makes the preview the old way from now on.
    fn go_back_to_the_old_drawing_if_the_engine_has_stopped(&mut self) {
        let Some(why) = self
            .interface
            .engine_preview
            .as_ref()
            .and_then(|engine_preview| engine_preview.why_it_stopped())
            .map(str::to_owned)
        else {
            return;
        };
        self.interface.engine_preview = None;
        self.playback.planes_wanted = false;
        self.add_to_message(
            "The picture engine stopped",
            format!("{why}\n\nThe preview is made the old way from now on. Nothing of the project is changed, and an export starts a picture engine of its own."),
        );
    }

    /// Tells the user what the picture engine has done and gone on after (it replaced its graphics device), now if no question is being asked, and otherwise when
    /// it is not: a message does not take the place of a question, and is put under a message that is showing.
    fn say_what_the_picture_engine_has_done_and_gone_on_after(&mut self) {
        if !matches!(self.interface.dialog, None | Some(Dialog::Message { .. })) {
            return;
        }
        let Some(engine_preview) = self.interface.engine_preview.as_mut() else {
            return;
        };
        for words in engine_preview.take_what_the_user_is_to_be_told() {
            self.add_to_message("The graphics device was replaced", words);
        }
    }

    /// Has the editor play the sound of the preview, and its own little sounds, through the computer's default speakers
    /// (which are opened when the first sound is made). Without this it is silent, which is what the tests want.
    fn use_the_default_speakers(&mut self) {
        self.playback
            .audio_supplier
            .play_through(opener_of_the_default_speakers());
        self.playback
            .sound_effects
            .play_through(opener_of_the_default_effect_speakers());
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
        let video_format_prober = ffprobe_as_the_video_format_prober(toolchain.clone());
        let plugin_host = Arc::new(crate::plugin_host::PluginHost::new());
        Self {
            playback: Playback {
                preview_pictures: PreviewPictures::new(FfmpegPictureStreamStarter::new(
                    toolchain.clone(),
                    Arc::clone(&wake_up),
                )),
                audio_supplier: AudioSupplier::new(
                    FfmpegAudioStreamStarter::new(toolchain.clone(), Arc::clone(&wake_up))
                        .with_plugins(Arc::clone(&plugin_host) as Arc<dyn crate::sound_engine::PluginRunner>),
                ),
                sound_effects: SoundEffects::default(),
                clock: PlaybackClock::new(project.frame_rate()),
                planes_wanted: false,
            },
            document: Document::new(project),
            sources: SourceFiles::default(),
            jobs: BackgroundJobs::default(),
            interface: Interface::new(startup_dialog),
            environment: Environment {
                toolchain,
                export_adapter: None,
                video_format_prober,
                file_chooser: Box::new(RefusingFileChooser),
                text_renderer: LazyTextRenderer::new(Arc::clone(&wake_up)),
                font_families: None,
                global_settings: Settings::DEFAULT,
                global_settings_path: None,
                plugin_trust: plugin_flow::PluginTrust::default(),
                plugin_host,
                plugin_search: sound_plugins_flow::PluginSearch::default(),
                wake_up,
            },
        }
    }

    /// Stops playback before a window opens that waits for an answer (a file dialog, say), so
    /// that the video does not carry on playing unseen behind it.
    fn pause_playback_for_dialog(&mut self) {
        let total_frames = self.document.project.total_frame_count();
        self.playback.clock.pause(Instant::now(), total_frames);
    }

    /// Whether a dialog or an export is in the way of editing.
    fn is_blocked(&self) -> bool {
        self.interface.dialog.is_some() || self.jobs.export.is_some() || self.jobs.running_save.is_some()
    }

    /// Everything that has to be done at the start of a frame: collecting what background work has
    /// finished, moving the clock on, and getting the preview picture.
    fn prepare_frame(&mut self, context: &Context, now: Instant) -> FrameState {
        self.go_back_to_the_old_drawing_if_the_engine_has_stopped();
        self.say_what_the_picture_engine_has_done_and_gone_on_after();
        self.collect_finished_background_work();
        self.take_what_the_plugin_scan_found();
        self.forget_what_is_gone_from_the_project();
        self.synchronise_the_preview_with_the_project();

        let total_frames = self.document.project.total_frame_count();
        self.playback.clock.stop_at_the_end(now, total_frames);
        let current_frame = self.playback.clock.position(now, total_frames);
        let is_playing = self.playback.clock.is_playing();

        let picture_plan = plan_pictures(&self.document.project, |source| self.sources.is_available(source));
        let look_ahead_frames = if is_playing {
            PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY
        } else {
            0
        };
        let wanted = wanted_pictures(&picture_plan, current_frame, look_ahead_frames);
        let report = self.playback.preview_pictures.supply(&wanted, is_playing, now);
        if let Some(wait) = report.ask_again_in {
            context.request_repaint_after(wait);
        }
        if is_playing {
            context.request_repaint();
        }
        self.update_the_sound(now, total_frames);
        self.ask_about_the_folders_that_plugins_are_wanted_from();
        self.tell_what_the_sound_plugins_said();

        FrameState {
            current_frame,
            total_frames,
            frame_rate: self.document.project.frame_rate(),
            is_playing,
            picture_plan,
            pictures_report: report,
        }
    }

    /// Plays the sounds that drawers began to make in this frame of the window: those of the drawers that a click opened or shut, and
    /// those that opened because a keyframe was put in them.
    fn play_the_sounds_of_the_drawers(&mut self, now: Instant) {
        for sound in self.interface.timeline_view.take_the_drawer_sounds() {
            self.playback.sound_effects.play(
                match sound {
                    DrawerSound::Opening => SoundEffect::DrawerOpening,
                    DrawerSound::Closing => SoundEffect::DrawerClosing,
                },
                now,
            );
        }
    }

    /// Lets go of a selection that is about a clip or a track that is not there any more.
    fn forget_what_is_gone_from_the_project(&mut self) {
        self.interface.selection.forget_what_is_gone(&self.document.project);
    }

    /// Tells the picture supplier which videos it can get pictures from, and how big the pictures
    /// are, whenever that has changed (a file was found, or the project was replaced).
    fn synchronise_the_preview_with_the_project(&mut self) {
        let videos: Vec<PreviewVideo> = self
            .document
            .project
            .sources()
            .iter()
            .filter_map(|source| {
                let video = source.properties.video()?;
                let state = self.sources.files.get(&source.identifier)?;
                Some(PreviewVideo {
                    source: source.identifier,
                    location: state.location_if_available()?.clone(),
                    display_size_in_pixels: (video.display_width_in_pixels, video.display_height_in_pixels),
                    frame_rate: video.frame_rate,
                    format: state.video_format.known.clone(),
                    colour: source.colour,
                })
            })
            .collect();
        let wanted_setup = (!videos.is_empty()).then(|| {
            let (picture_width, picture_height) = self.document.project.picture_size();
            let (picture_width_in_pixels, picture_height_in_pixels) =
                preview_picture_size(picture_width, picture_height);
            PreviewSetup {
                frame_rate: self.document.project.frame_rate(),
                picture_width_in_pixels,
                picture_height_in_pixels,
                videos,
                planes_wanted: self.playback.planes_wanted,
            }
        });
        if self.playback.preview_pictures.setup() != wanted_setup.as_ref() {
            self.playback.preview_pictures.set_setup(wanted_setup);
        }
    }

    /// Starts, keeps in step, or stops the sound of the preview: it plays while the video plays.
    fn update_the_sound(&mut self, now: Instant, total_frames: u64) {
        let clock = self.playback.reading(now, total_frames);
        let sped_up_clip_sound = self.effective_settings().sped_up_clip_sound;
        let source_locations = self.sources.available_locations();
        // There is sound to play when some file that can be read is there to make it from.
        let content = (!source_locations.is_empty()).then_some(AudioContent {
            project: &self.document.project,
            source_locations: &source_locations,
            sped_up_clip_sound,
        });
        self.playback.audio_supplier.update(content, clock, now);
    }

    /// Turns the keyboard (see [`keyboard`]) and the extra buttons of the mouse (see [`mouse_buttons`]) into commands, unless
    /// something is in the way.
    fn collect_shortcut_commands(&self, context: &Context, frame_state: &FrameState, commands: &mut Vec<Command>) {
        if self.is_blocked() {
            return;
        }
        let selected_clips_at_the_current_frame = self
            .interface
            .selection
            .iter()
            .filter(|clip| {
                self.document
                    .project
                    .placement_of(*clip)
                    .is_some_and(|placement| placement.contains(frame_state.current_frame))
            })
            .collect();
        let pointer_is_over_the_preview = self
            .interface
            .preview_panel_in_the_last_frame
            .zip(context.input(|input| input.pointer.hover_pos()))
            .is_some_and(|(panel, pointer)| panel.contains(pointer));
        commands.extend(keyboard::commands_for_the_keys_pressed(
            context,
            KeyboardState {
                current_frame: frame_state.current_frame,
                is_playing: frame_state.is_playing,
                timeline_is_empty: frame_state.total_frames == 0,
                selected_clips: self.interface.selection.to_vec(),
                selected_keyframes: self.interface.selection.keyframes_to_vec(),
                selected_racks: self.interface.selection.racks_to_vec(),
                selected_groups: self.interface.selection.groups().collect(),
                selected_clips_at_the_current_frame,
                pointer_is_over_the_preview,
                clip_ends_to_go_between: self.clip_ends_to_go_between(frame_state.total_frames),
                can_undo: self.document.history.can_undo(),
                can_redo: self.document.history.can_redo(),
            },
        ));
        commands.extend(mouse_buttons::commands_for_the_buttons_pressed(context));
    }

    /// The frames where the clips begin and end that the keys that go to the end of a clip go between: those of the selected
    /// clips if some are selected, and otherwise of every clip there is.
    fn clip_ends_to_go_between(&self, total_frames: u64) -> Vec<TimelineFrameIndex> {
        let some_clips_are_selected = self.interface.selection.iter().next().is_some();
        let clips = self
            .document
            .project
            .tracks()
            .iter()
            .flat_map(|track| track.placements())
            .filter(|placement| !some_clips_are_selected || self.interface.selection.contains(placement.id))
            .map(|placement| (placement.first_timeline_frame, placement.end_timeline_frame));
        keyboard::clip_ends_to_go_between(clips, total_frames)
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
        self.collect_shortcut_commands(&context, &frame_state, &mut commands);
        self.draw(ui, &frame_state, &mut commands);
        self.ask_for_finer_thumbnails();
        settings_windows::show(
            &context,
            &mut self.interface.settings_windows_open,
            (self.document.project.settings(), self.document.project.frame_rate()),
            self.environment.global_settings,
            &mut commands,
        );
        self.show_the_window_of_sound_plugins(&context, &mut commands);
        colour_of_a_file_window::show(
            &context,
            &mut self.interface.colours_of_a_file_window_open_for,
            &self.document.project,
            |source| {
                self.sources
                    .files
                    .get(&source)
                    .and_then(|state| state.video_format.known.as_ref())
            },
            &mut commands,
        );
        let agreed_to = self.show_dialogs(&context);
        self.execute_all(commands, now, &context);
        // (What the user has just agreed to was asked about already: it is carried out without being asked about again.)
        self.carry_out_all(agreed_to, now, &context);
        self.play_the_sounds_of_the_drawers(now);
        self.autosave_when_quiet_for_long_enough(&context, now);

        self.interface.textures.forget_thumbnails_not_used_this_frame();
    }
}
