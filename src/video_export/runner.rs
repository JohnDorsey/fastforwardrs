// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Running an export: the picture engine draws each frame, ffmpeg decodes the files and encodes the frames, and this brings them together, reports the progress, and
//! finishes or cancels cleanly.
//!
//! The work is done by a thread of its own ([`VideoExport::start`] returns at once): it probes the videos, decodes the picture files, starts the engine and the
//! encoder, and then goes frame by frame: describe the frame ([`FrameDescriber`]), ask the engine for it, and give the encoder what the engine has made, **keeping a
//! few frames asked for and not yet written** so that the engine, the decoders and the encoder all have work at the same time, and never more than that so that
//! nothing piles up in memory. The video is written to a temporary file beside the destination and moved into place only when it is complete.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use thiserror::Error;

use super::encoder::{
    Encoder, EncoderEnd, EncoderKiller, EncoderSpecification, SoundForTheEncoder, sound_graph_for_the_encoder,
};
use super::frames::{FrameDescriber, FrameProblem};
use super::output_colour::output_colour_for;
use super::pictures::{FfmpegRunStreamStarter, PicturesOfTheRuns, RunPictureProblem};
use super::plan::ExportPlan;
use crate::engine::{AdapterChoice, Engine, EngineEvent, EngineSettings, PictureData, PictureId, RenderedPlanes};
use crate::frame_scene::TextPictures;
use crate::media::export::VideoEncoder;
use crate::media::{
    FfmpegToolchain, MediaLocation, NativeYuv420p, UserInterfaceWakeUp, VideoStreamFormat, decode_picture,
    probe_video_stream_format, size_to_decode_at,
};
use crate::project::{SourceMediaIdentifier, TextObject};
use crate::rgba_picture::RgbaPicture;
use crate::text_rendering::LazyTextRenderer;

/// How many frames are asked of the engine and not yet written to the encoder, at most. The engine works on two at once (`EngineSettings::export_frames_in_flight`);
/// the others wait their turn, and are the room that the decoders and the encoder are given to be a little late and a little early without stopping the engine.
const MOST_FRAMES_ASKED_AHEAD: usize = 4;

/// How long the export waits for the engine to say something before it looks at everything else again.
const HOW_LONG_TO_WAIT_FOR_THE_ENGINE: Duration = Duration::from_millis(20);

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(
        "the export would overwrite one of the files the project is made from ({path}); choose a different name for the exported video"
    )]
    DestinationIsASourceFile { path: PathBuf },
    #[error("the export needs {needed} source files, but {given} were given")]
    WrongNumberOfSourceFiles { needed: usize, given: usize },
    #[error("the export could not be started: {0}")]
    CouldNotStart(#[source] io::Error),
}

/// How an export ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportResult {
    /// The finished video is at the destination.
    Completed,
    /// The export was cancelled. Nothing was left at the destination; a file that was already there is untouched.
    Cancelled,
    /// The export failed; `reason` says why. Nothing was left at the destination; a file that was already there is untouched.
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportEvent {
    /// How many frames of the finished video have been written so far.
    Progress {
        frames_written: u64,
    },
    Finished(ExportResult),
}

/// What an export is run with, other than the project.
pub struct ExportOptions {
    /// The graphics adapter the picture engine of the export is made on.
    pub adapter: AdapterChoice,
    /// What draws the text objects. `None`: the fonts of the computer, as the preview draws them.
    pub text_pictures: Option<Box<dyn TextPictures + Send>>,
}

impl ExportOptions {
    /// An export on `adapter`, with the text drawn in the fonts of the computer.
    pub fn new(adapter: AdapterChoice) -> Self {
        Self {
            adapter,
            text_pictures: None,
        }
    }

    /// The same, with the text objects drawn by `text_pictures` (for a test that must not depend on the fonts of a computer).
    #[must_use]
    pub fn with_text_pictures(mut self, text_pictures: impl TextPictures + Send + 'static) -> Self {
        self.text_pictures = Some(Box::new(text_pictures));
        self
    }
}

/// What the thread of the export and the one that asked for it both reach.
struct Shared {
    cancel_requested: AtomicBool,
    /// What stops the encoder, once there is one: a write to an encoder that is busy waits, and cancelling must end the wait.
    encoder: Mutex<Option<EncoderKiller>>,
}

impl Shared {
    fn is_cancelled(&self) -> bool {
        self.cancel_requested.load(Ordering::SeqCst)
    }

    fn encoder(&self) -> std::sync::MutexGuard<'_, Option<EncoderKiller>> {
        self.encoder.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A running export. Dropping it cancels the export, and waits for the clean-up (which takes a moment) so that no half-written file is left behind, even if the
/// program is about to exit.
pub struct VideoExport {
    shared: Arc<Shared>,
    events: Receiver<ExportEvent>,
    thread: Option<JoinHandle<()>>,
}

impl VideoExport {
    /// Starts exporting. `source_file_locations` are where the plan's source files are, in the order the plan lists them ([`ExportPlan::source_files`]). The video is
    /// written to a temporary file beside `destination` and moved into place only when it is complete, so a cancelled or failed export never damages a file that is
    /// already at `destination`. Everything else that can go wrong (a file that cannot be read, no graphics adapter, ffmpeg that cannot be started) is told by the
    /// [`ExportResult::Failed`] that ends the export.
    pub fn start(
        toolchain: &FfmpegToolchain,
        plan: &ExportPlan,
        source_file_locations: &[MediaLocation],
        destination: &Path,
        options: ExportOptions,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, ExportError> {
        if source_file_locations.len() != plan.source_files.len() {
            return Err(ExportError::WrongNumberOfSourceFiles {
                needed: plan.source_files.len(),
                given: source_file_locations.len(),
            });
        }
        // (A source file that is a stretch of another file, such as a project file, is the file that it is in, for this: writing over that would destroy it as
        // much.)
        if let Some(location) = source_file_locations
            .iter()
            .find(|location| is_the_same_file(location.file(), destination))
        {
            return Err(ExportError::DestinationIsASourceFile {
                path: location.file().to_owned(),
            });
        }

        let shared = Arc::new(Shared {
            cancel_requested: AtomicBool::new(false),
            encoder: Mutex::new(None),
        });
        let (sender, events) = mpsc::channel();
        let job = Job {
            toolchain: toolchain.clone(),
            plan: plan.clone(),
            locations: plan
                .source_files
                .iter()
                .copied()
                .zip(source_file_locations.iter().cloned())
                .collect(),
            destination: destination.to_owned(),
            temporary_output: temporary_output_path(destination),
            options,
            shared: Arc::clone(&shared),
            sender,
            wake_up,
        };
        let thread = thread::Builder::new()
            .name("export".to_owned())
            .spawn(move || run(job))
            .map_err(ExportError::CouldNotStart)?;
        Ok(Self {
            shared,
            events,
            thread: Some(thread),
        })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<ExportEvent> {
        self.events.try_recv().ok()
    }

    /// Stops the export. An [`ExportResult::Cancelled`] event follows once everything has stopped and the temporary file has been removed.
    pub fn cancel(&self) {
        self.shared.cancel_requested.store(true, Ordering::SeqCst);
        if let Some(encoder) = self.shared.encoder().as_ref() {
            encoder.kill();
        }
    }
}

impl Drop for VideoExport {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Everything the thread of an export needs.
struct Job {
    toolchain: FfmpegToolchain,
    plan: ExportPlan,
    locations: HashMap<SourceMediaIdentifier, MediaLocation>,
    destination: PathBuf,
    temporary_output: PathBuf,
    options: ExportOptions,
    shared: Arc<Shared>,
    sender: Sender<ExportEvent>,
    wake_up: UserInterfaceWakeUp,
}

/// Why an export did not go to its end.
#[derive(Debug, PartialEq, Eq)]
enum Ending {
    Cancelled,
    Failed(String),
}

impl Ending {
    fn failed(reason: impl Into<String>) -> Self {
        Self::Failed(reason.into())
    }
}

impl From<RunPictureProblem> for Ending {
    fn from(problem: RunPictureProblem) -> Self {
        match problem {
            RunPictureProblem::Cancelled => Self::Cancelled,
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<FrameProblem> for Ending {
    fn from(problem: FrameProblem) -> Self {
        match problem {
            FrameProblem::Picture(picture) => picture.into(),
            other => Self::Failed(other.to_string()),
        }
    }
}

/// The body of the thread of an export: does the export, and tells how it ended.
fn run(job: Job) {
    let sender = job.sender.clone();
    let wake_up = Arc::clone(&job.wake_up);
    let temporary_output = job.temporary_output.clone();
    let destination = job.destination.clone();
    // (A mistake of the program, a panic, ends the export with a message like any other ending: the window is waiting for the end, and would wait for ever. What the
    // export holds is let go of as the panic goes up: ffmpeg is stopped by the drop of its encoder and of its decoders.)
    let produced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| produce(job))).unwrap_or_else(|panic| {
        Err(Ending::Failed(format!(
            "the export stopped because of a mistake in the program: {}",
            what_a_panic_said(panic.as_ref())
        )))
    });
    let result = match produced {
        Ok(()) => match fs::rename(&temporary_output, &destination) {
            Ok(()) => ExportResult::Completed,
            Err(error) => ExportResult::Failed {
                reason: format!(
                    "the finished video could not be moved to {}: {error}",
                    destination.display()
                ),
            },
        },
        Err(Ending::Cancelled) => ExportResult::Cancelled,
        Err(Ending::Failed(reason)) => ExportResult::Failed { reason },
    };
    if result != ExportResult::Completed {
        let _ = fs::remove_file(&temporary_output);
    }
    let _ = sender.send(ExportEvent::Finished(result));
    wake_up();
}

/// What a panic said, if it said it in words.
fn what_a_panic_said(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(words) = panic.downcast_ref::<&str>() {
        (*words).to_owned()
    } else if let Some(words) = panic.downcast_ref::<String>() {
        words.clone()
    } else {
        "it did not say what".to_owned()
    }
}

/// Makes the video, in the temporary file.
fn produce(job: Job) -> Result<(), Ending> {
    let Job {
        toolchain,
        plan,
        locations,
        temporary_output,
        options,
        shared,
        sender,
        wake_up,
        ..
    } = job;
    let ExportOptions { adapter, text_pictures } = options;
    let stop_if_cancelled = || {
        if shared.is_cancelled() {
            Err(Ending::Cancelled)
        } else {
            Ok(())
        }
    };

    // The engine is started first: making its device takes a good fraction of a second, which goes by while the files are looked at.
    let (wake_sender, wake_receiver) = mpsc::channel::<()>();
    let mut engine = Engine::start(
        EngineSettings {
            most_cached_frames: 0,
            most_cached_frame_bytes: 0,
            ..EngineSettings::new(adapter)
        },
        Arc::new(move || {
            let _ = wake_sender.send(());
        }),
    );

    // How the videos are stored: which can be given to the engine as the planes ffmpeg decodes, and how the finished video gets its colours.
    let mut formats: HashMap<SourceMediaIdentifier, VideoStreamFormat> = HashMap::new();
    for &source in plan.videos.keys() {
        stop_if_cancelled()?;
        let location = &locations[&source];
        let format = probe_video_stream_format(&toolchain, location.clone())
            .map_err(|error| Ending::failed(format!("{} cannot be read: {error}", name_of(location))))?;
        formats.insert(source, format);
    }
    let planes: HashMap<SourceMediaIdentifier, NativeYuv420p> = formats
        .iter()
        .filter_map(|(&source, format)| Some((source, format.as_native_yuv420p()?)))
        .collect();
    let colour = output_colour_for(plan.first_video().and_then(|source| formats.get(&source)));

    // The picture files, decoded once.
    let mut images = HashMap::new();
    for (&source, image) in &plan.images {
        stop_if_cancelled()?;
        let location = &locations[&source];
        let size = size_to_decode_at(image.display_size_in_pixels, image.largest_box_in_pixels);
        let picture = decode_picture(&toolchain, location.clone(), size)
            .map_err(|error| Ending::failed(format!("{} cannot be read: {error}", name_of(location))))?;
        images.insert(source, picture);
    }

    // The sound: its graph is saved in a file, which is far longer than a command line allows for a project with many clips.
    let working_directory = tempfile::Builder::new()
        .prefix("fastforwardrs-export-")
        .tempdir()
        .map_err(|error| Ending::failed(format!("the export's working files could not be prepared: {error}")))?;
    let sound = match &plan.sound {
        None => None,
        Some(sound) => {
            let graph = sound_graph_for_the_encoder(sound);
            let graph_file = working_directory.path().join("sound_graph.txt");
            fs::write(&graph_file, &graph.text).map_err(|error| {
                Ending::failed(format!("the export's working files could not be prepared: {error}"))
            })?;
            Some(SoundForTheEncoder {
                files: sound
                    .source_files
                    .iter()
                    .map(|source| locations[source].clone())
                    .collect(),
                graph_file,
                output_label: graph.output_label,
            })
        }
    };

    // The pictures of the videos, and what draws the text.
    let streams = FfmpegRunStreamStarter {
        toolchain: Arc::new(toolchain.clone()),
    };
    let pictures_of_the_runs = PicturesOfTheRuns::new(streams, &plan, &locations, &planes)?;
    let text: Box<dyn TextPictures> = match text_pictures {
        Some(text_pictures) => text_pictures,
        None => Box::new(FontsOfTheComputer(LazyTextRenderer::new(Arc::new(|| {})))),
    };
    let mut describer = FrameDescriber::new(&plan, pictures_of_the_runs, images, text);

    // The encoder.
    stop_if_cancelled()?;
    let progress_sender = sender.clone();
    let progress_wake_up = Arc::clone(&wake_up);
    let specification = EncoderSpecification {
        picture_size: plan.picture_size(),
        frame_rate: plan.frame_rate,
        sound,
        tags: &colour.tags,
        video_encoder: VideoEncoder::choose(&toolchain),
        output: &temporary_output,
    };
    let mut encoder = Encoder::start(
        &toolchain,
        &specification,
        Box::new(move |frames_written| {
            let _ = progress_sender.send(ExportEvent::Progress { frames_written });
            progress_wake_up();
        }),
    )
    .map_err(|error| Ending::failed(format!("ffmpeg could not be started for the export: {error}")))?;
    *shared.encoder() = Some(encoder.killer());
    if let Err(ending) = stop_if_cancelled() {
        return Err(stop_the_encoder(encoder, &shared, ending));
    }

    // The frames.
    let total = plan.total_frame_count;
    let mut asked: VecDeque<AskedFrame> = VecDeque::new();
    let mut next_to_ask = 0;
    let mut written = 0;
    let outcome = loop {
        if shared.is_cancelled() {
            break Err(Ending::Cancelled);
        }
        // What the engine has made goes to the encoder first, so that the encoder is never kept waiting for a picture to be asked for.
        let mut something_happened = false;
        while let Some(event) = engine.try_next_event() {
            something_happened = true;
            match what_to_do_with(event, &mut asked) {
                Ok(Some(planes)) => {
                    if (planes.width_in_pixels, planes.height_in_pixels) != plan.picture_size() {
                        let ending = Ending::failed(format!(
                            "the picture engine made a frame of {} by {} pixels and the video is {} by {}",
                            planes.width_in_pixels,
                            planes.height_in_pixels,
                            plan.picture_width_in_pixels,
                            plan.picture_height_in_pixels
                        ));
                        return Err(stop_the_encoder(encoder, &shared, ending));
                    }
                    if let Err(error) = encoder.write_frame(&planes.planes) {
                        return Err(stop_the_encoder_that_a_frame_could_not_be_given_to(
                            encoder, &shared, &error,
                        ));
                    }
                    written += 1;
                }
                Ok(None) => {}
                Err(ending) => return Err(stop_the_encoder(encoder, &shared, ending)),
            }
        }
        if written == total {
            break Ok(());
        }
        if next_to_ask < total && asked.len() < MOST_FRAMES_ASKED_AHEAD {
            let described = describer.describe(
                next_to_ask,
                &shared.cancel_requested,
                &mut |id: PictureId, data: PictureData| {
                    engine.upload_picture(id, Arc::new(data));
                },
            );
            let described = match described {
                Ok(described) => described,
                Err(problem) => return Err(stop_the_encoder(encoder, &shared, problem.into())),
            };
            if !described.forget.is_empty() {
                engine.forget_pictures(described.forget);
            }
            let serial = engine.request_export_frame(described.description, colour.planes);
            asked.push_back(AskedFrame {
                serial,
                timeline_frame: next_to_ask,
            });
            next_to_ask += 1;
            continue;
        }
        if !something_happened {
            while wake_receiver.try_recv().is_ok() {}
            let _ = wake_receiver.recv_timeout(HOW_LONG_TO_WAIT_FOR_THE_ENGINE);
        }
    };
    if let Err(ending) = outcome {
        return Err(stop_the_encoder(encoder, &shared, ending));
    }

    // The end of the video.
    drop(engine);
    drop(describer);
    let end = encoder.finish();
    *shared.encoder() = None;
    if shared.is_cancelled() {
        return Err(Ending::Cancelled);
    }
    the_ending_of_the_file(end)
}

/// How an export ends when ffmpeg has finished the file, or has not: well if it says it did, and otherwise with its own words, which are the reason.
fn the_ending_of_the_file(end: EncoderEnd) -> Result<(), Ending> {
    match end.status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(Ending::failed(if end.error_output.is_empty() {
            format!("ffmpeg stopped with {status}")
        } else {
            end.error_output
        })),
        Err(error) => Err(Ending::failed(format!("ffmpeg could not be waited for: {error}"))),
    }
}

/// Stops the encoder, and gives the ending of the export: `ending`, with what ffmpeg said added to the reason if it failed (ffmpeg's own complaint is often the
/// reason: it stopped by itself).
fn stop_the_encoder(encoder: Encoder, shared: &Shared, ending: Ending) -> Ending {
    let end = encoder.stop();
    *shared.encoder() = None;
    the_ending_with_what_ffmpeg_said(ending, &end.error_output)
}

/// `ending`, with what ffmpeg said added to the reason if it failed.
fn the_ending_with_what_ffmpeg_said(ending: Ending, ffmpeg_said: &str) -> Ending {
    match ending {
        // An export that was asked to stop ends as cancelled, whatever went wrong while it was stopping.
        Ending::Cancelled => Ending::Cancelled,
        Ending::Failed(reason) if ffmpeg_said.is_empty() => Ending::Failed(reason),
        Ending::Failed(reason) => Ending::Failed(format!("{reason}: {ffmpeg_said}")),
    }
}

/// Stops the encoder that a frame could not be given to, which means that ffmpeg has stopped (or was stopped by a cancel), and gives the ending of the export: cancelled if
/// it was asked to stop, and otherwise a failure whose reason is what ffmpeg said (its own complaint is the reason, and the broken pipe was only the way the program found
/// out) or, if it said nothing, the words of the broken pipe.
fn stop_the_encoder_that_a_frame_could_not_be_given_to(encoder: Encoder, shared: &Shared, error: &io::Error) -> Ending {
    if shared.is_cancelled() {
        return stop_the_encoder(encoder, shared, Ending::Cancelled);
    }
    let end = encoder.stop();
    *shared.encoder() = None;
    the_ending_when_a_frame_could_not_be_given(error, end.error_output)
}

/// The ending of an export whose encoder had stopped when a frame was given to it: what ffmpeg said, if it said anything, and otherwise the words of the broken pipe.
fn the_ending_when_a_frame_could_not_be_given(error: &io::Error, ffmpeg_said: String) -> Ending {
    if ffmpeg_said.is_empty() {
        Ending::Failed(format!("the video could not be written: {error}"))
    } else {
        Ending::Failed(ffmpeg_said)
    }
}

/// A frame the engine was asked for and has not answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AskedFrame {
    serial: u64,
    timeline_frame: u64,
}

/// What the export does with one thing the engine says: gives the planes of a frame (the engine answers in the order it was asked), or goes on, or ends the
/// export. The frames the engine answers are taken off the front of `asked`.
fn what_to_do_with(event: EngineEvent, asked: &mut VecDeque<AskedFrame>) -> Result<Option<RenderedPlanes>, Ending> {
    let frame_of = |serial: u64, asked: &VecDeque<AskedFrame>| {
        asked.iter().find(|frame| frame.serial == serial).map_or_else(
            || "a frame".to_owned(),
            |frame| format!("frame {}", frame.timeline_frame),
        )
    };
    match event {
        EngineEvent::ExportFrame { serial, planes, .. } => match asked.front() {
            Some(front) if front.serial == serial => {
                asked.pop_front();
                Ok(Some(planes))
            }
            _ => Err(Ending::failed(format!(
                "the picture engine answered out of order (the answer for request {serial} came when {:?} was next)",
                asked.front().map(|front| front.serial)
            ))),
        },
        EngineEvent::FrameFailed { serial, problem } => Err(Ending::failed(format!(
            "the picture engine could not make {}: {problem}",
            frame_of(serial, asked)
        ))),
        EngineEvent::PicturesMissing { serial, ids } => Err(Ending::failed(format!(
            "the picture engine did not have {} of the pictures that {} is made of",
            ids.len(),
            frame_of(serial, asked)
        ))),
        EngineEvent::PictureRejected { problem, .. } => Err(Ending::failed(format!(
            "a picture could not be used by the picture engine: {problem}"
        ))),
        EngineEvent::DeviceReplaced { why, .. } => Err(Ending::failed(format!(
            "the graphics device was lost during the export ({why})"
        ))),
        EngineEvent::Stopped { problem } => Err(Ending::failed(format!("the picture engine stopped: {problem}"))),
        EngineEvent::Frame { .. } => Err(Ending::failed("the picture engine answered a request for the preview")),
        // (The export installs no effects of its own.)
        EngineEvent::EffectRejected { .. } => Ok(None),
    }
}

/// The text pictures of the export when nothing else is asked for: drawn in the fonts of the computer, waiting for them to be loaded when the first text is wanted.
struct FontsOfTheComputer(LazyTextRenderer);

impl TextPictures for FontsOfTheComputer {
    fn text_picture(
        &mut self,
        object: &TextObject,
        picture_width_in_pixels: u32,
        picture_height_in_pixels: u32,
    ) -> Option<RgbaPicture> {
        Some(
            self.0
                .get_waiting_if_needed()?
                .render(object, picture_width_in_pixels, picture_height_in_pixels),
        )
    }
}

/// What a file is called in a message.
fn name_of(location: &MediaLocation) -> String {
    location
        .file()
        .file_name()
        .map_or_else(|| location.to_string(), |name| name.to_string_lossy().into_owned())
}

/// Whether `destination` is the very file `source_file_path` (so that exporting would destroy the original). A destination that is not there yet is no file
/// that is there, and a source that is not there is not destroyed by anything. (The operating system makes the paths whole, from the folder the program is in if they
/// are relative, and follows links.)
fn is_the_same_file(source_file_path: &Path, destination: &Path) -> bool {
    match (fs::canonicalize(source_file_path), fs::canonicalize(destination)) {
        (Ok(source), Ok(destination)) => source == destination,
        _ => false,
    }
}

/// Where the video is written until it is complete: beside the destination, with `.exporting` before the extension so that ffmpeg still recognises the file type.
fn temporary_output_path(destination: &Path) -> PathBuf {
    let stem = destination
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match destination.extension() {
        Some(extension) => format!("{stem}.exporting.{}", extension.to_string_lossy()),
        None => format!("{stem}.exporting"),
    };
    destination.with_file_name(name)
}

#[cfg(test)]
mod tests;
