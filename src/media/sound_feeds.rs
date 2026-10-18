// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound of the clips for the sound engine (phase 7, piece 7.2): **an ffmpeg that runs for each clip**, and the two things that use them: the stream of sound of the live
//! preview ([`EngineAudioStream`]) and the sound of the export, made in a file before the video is encoded ([`render_sound_to_file`]).
//!
//! The ffmpeg of a clip cuts the clip out of its file and sets its speed with the very filters that the filter graph of the sound has for it
//! ([`clip_feed_filters`]), and writes raw samples (32-bit floating point, 48000 a second, stereo) on its standard output. What is done to them after that is the block graph's
//! ([`crate::sound_engine`]): the length of the clip, its volume, the joining of the clips and the mixing of the tracks.

use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use thiserror::Error;

use super::UserInterfaceWakeUp;
use super::audio_stream::{AudioStreamError, AudioStreamEvent, AudioStreamRequest, read_samples};
use super::child_process::{StreamingTool, WaitedFor, background_command};
use super::export::{SoundGraphSpecification, clip_feed_filters};
use super::ffmpeg_toolchain::FfmpegToolchain;
use super::location::MediaLocation;
use crate::project::{FrameRate, SpedUpClipSound};
use crate::sound_engine::{
    ClipFeed, ClipRequest, ClipSpawner, LookAheadFeeds, NoInsertions, SAMPLE_RATE, SoundGraph, SoundInMemory,
};

/// How many sets of samples a clip's ffmpeg may have waiting before it pauses. (A clip's sound is read at the pace of the graph; the rest waits in the pipe.)
const CHUNKS_BUFFERED_AHEAD_OF_A_CLIP: usize = 8;

/// How often a feed that is waiting for sound looks at whether it has been told to stop.
const HOW_OFTEN_TO_LOOK_AT_STOPPING: Duration = Duration::from_millis(50);

/// How many moments of sound the graph makes at a time for the preview (about 43 milliseconds), and how many sets wait to be played before the graph pauses.
const MOMENTS_PER_CHUNK: usize = 2048;
const CHUNKS_BUFFERED_AHEAD: usize = 24;

/// What the ffmpeg of a clips said that was not sound.
type Problems = Arc<Mutex<Vec<String>>>;

/// The feed of one clip: the sound its ffmpeg writes.
struct FfmpegClipFeed {
    tool: StreamingTool<AudioStreamEvent>,
    /// Sound that has come and has not been given yet (interleaved), and how much of it has.
    waiting: Vec<f32>,
    given: usize,
    has_ended: bool,
    stop: Arc<AtomicBool>,
    problems: Problems,
}

impl FfmpegClipFeed {
    fn next_event(&mut self) -> Option<AudioStreamEvent> {
        loop {
            match self.tool.wait_for_an_event(HOW_OFTEN_TO_LOOK_AT_STOPPING) {
                WaitedFor::Event(event) => return Some(event),
                WaitedFor::Ended => return None,
                WaitedFor::NothingYet => {
                    if self.stop.load(Ordering::Relaxed) {
                        return None;
                    }
                }
            }
        }
    }
}

impl ClipFeed for FfmpegClipFeed {
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        let wanted = left.len().min(right.len());
        let mut count = 0;
        while count < wanted {
            if self.given >= self.waiting.len() {
                if self.has_ended {
                    break;
                }
                match self.next_event() {
                    Some(AudioStreamEvent::Samples(samples)) => {
                        self.waiting = samples;
                        self.given = 0;
                    }
                    Some(AudioStreamEvent::Finished { error_output }) => {
                        self.has_ended = true;
                        if let Some(problem) = error_output {
                            self.problems.lock().unwrap_or_else(|p| p.into_inner()).push(problem);
                        }
                        break;
                    }
                    None => {
                        self.has_ended = true;
                        break;
                    }
                }
            }
            let moments = ((self.waiting.len() - self.given) / 2).min(wanted - count);
            for place in 0..moments {
                left[count + place] = self.waiting[self.given + place * 2];
                right[count + place] = self.waiting[self.given + place * 2 + 1];
            }
            self.given += moments * 2;
            count += moments;
        }
        count
    }
}

/// Starts the ffmpeg of a clip.
pub struct FfmpegClipSpawner {
    ffmpeg: PathBuf,
    sources: Vec<MediaLocation>,
    frame_rate: FrameRate,
    sped_up_clip_sound: SpedUpClipSound,
    stop: Arc<AtomicBool>,
    problems: Problems,
}

impl FfmpegClipSpawner {
    /// `stop` makes every feed that is waiting for sound give up (the sound is no longer wanted).
    pub fn new(
        toolchain: &FfmpegToolchain,
        sources: Vec<MediaLocation>,
        specification: &SoundGraphSpecification,
        stop: Arc<AtomicBool>,
    ) -> Self {
        Self {
            ffmpeg: toolchain.ffmpeg_path().to_owned(),
            sources,
            frame_rate: specification.frame_rate,
            sped_up_clip_sound: specification.sped_up_clip_sound,
            stop,
            problems: Arc::default(),
        }
    }

    /// What the ffmpeg of the clips have complained of, so far.
    pub fn problems(&self) -> Problems {
        Arc::clone(&self.problems)
    }
}

impl ClipSpawner for FfmpegClipSpawner {
    fn spawn(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        let Some(source) = self.sources.get(request.clip.source_input) else {
            self.problems.lock().unwrap_or_else(|p| p.into_inner()).push(format!(
                "the sound of a clip names file number {}, which there is not",
                request.clip.source_input
            ));
            return Box::new(SoundInMemory::new(Vec::new(), Vec::new()));
        };
        let filters = clip_feed_filters(self.frame_rate, request.clip, self.sped_up_clip_sound);
        let mut command = background_command(&self.ffmpeg);
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(source.ffmpeg_input())
            .args(["-map", "0:a:0", "-af", &filters, "-f", "f32le", "-ar"])
            .arg(SAMPLE_RATE.to_string())
            .args(["-ac", "2", "pipe:1"]);
        let tool = StreamingTool::start(
            command,
            CHUNKS_BUFFERED_AHEAD_OF_A_CLIP,
            |standard_output, sender, error_output_when_finished| {
                read_samples(
                    standard_output,
                    2,
                    &sender,
                    &(Arc::new(|| {}) as UserInterfaceWakeUp),
                    error_output_when_finished,
                );
            },
        );
        match tool {
            Ok(tool) => Box::new(FfmpegClipFeed {
                tool,
                waiting: Vec::new(),
                given: 0,
                has_ended: false,
                stop: Arc::clone(&self.stop),
                problems: Arc::clone(&self.problems),
            }),
            Err(error) => {
                self.problems
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push(format!("ffmpeg could not be started for the sound of a clip: {error}"));
                Box::new(SoundInMemory::new(Vec::new(), Vec::new()))
            }
        }
    }
}

/// The text that says what went wrong, if anything did.
fn what_the_clips_complained_of(problems: &Problems) -> Option<String> {
    let problems = problems.lock().unwrap_or_else(|p| p.into_inner());
    (!problems.is_empty()).then(|| problems.join("\n"))
}

/// Writes the interleaved samples of two channels.
fn interleave_into(left: &[f32], right: &[f32], into: &mut Vec<f32>) {
    into.clear();
    for (left, right) in left.iter().zip(right) {
        into.push(*left);
        into.push(*right);
    }
}

/// The sound of the live preview, made by the sound engine: it runs the block graph on a thread of its own, with the ffmpeg of each clip, and delivers the samples as
/// [`AudioStream`](super::AudioStream) does (the same events, from the playhead on), so that whatever plays one plays the other. Dropping it stops everything it started.
///
/// The engine makes stereo sound at 48000 moments a second. **If the speakers want another kind** (a device that takes 44100, or one channel, or eight) the sound is passed through an
/// ffmpeg that changes it to what they want, which is how the filter graph did it at its end too.
pub struct EngineAudioStream {
    source: Source,
    stop: Arc<AtomicBool>,
    problems: Problems,
}

enum Source {
    /// The graph's thread sends the sound itself.
    Direct(Receiver<AudioStreamEvent>),
    /// The graph's thread writes the sound into an ffmpeg that changes its kind, and the sound comes from that.
    Converted(StreamingTool<AudioStreamEvent>),
}

impl EngineAudioStream {
    pub fn start(
        toolchain: &FfmpegToolchain,
        request: &AudioStreamRequest,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, AudioStreamError> {
        if request.sound.strips.is_empty() {
            return Err(AudioStreamError::NoSound);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let spawner = FfmpegClipSpawner::new(
            toolchain,
            request.source_files.clone(),
            &request.sound,
            Arc::clone(&stop),
        );
        let problems = spawner.problems();
        let specification = request.sound.clone();
        let stop_for_the_thread = Arc::clone(&stop);
        let is_what_the_engine_makes = request.format.sample_rate == SAMPLE_RATE && request.format.channel_count == 2;
        if is_what_the_engine_makes {
            let (sender, events) = mpsc::sync_channel(CHUNKS_BUFFERED_AHEAD);
            thread::spawn(move || {
                let mut first_chunk = true;
                make_the_sound(specification, spawner, &stop_for_the_thread, |samples| {
                    // A full channel makes this wait, which keeps the graph from racing ahead of the speakers; an error means nobody wants the sound.
                    let sent = sender.send(AudioStreamEvent::Samples(samples)).is_ok();
                    if sent && std::mem::take(&mut first_chunk) {
                        wake_up();
                    }
                    sent
                });
                let _ = sender.send(AudioStreamEvent::Finished { error_output: None });
                wake_up();
            });
            return Ok(Self {
                source: Source::Direct(events),
                stop,
                problems,
            });
        }
        // The sound goes through an ffmpeg that changes its kind: it reads what the engine makes on its standard input and writes what the speakers want.
        let mut command = background_command(toolchain.ffmpeg_path());
        command
            .args(["-hide_banner", "-loglevel", "error", "-f", "f32le", "-ar"])
            .arg(SAMPLE_RATE.to_string())
            .args(["-ac", "2", "-i", "pipe:0", "-f", "f32le", "-ar"])
            .arg(request.format.sample_rate.to_string())
            .arg("-ac")
            .arg(request.format.channel_count.to_string())
            .arg("pipe:1")
            .stdin(std::process::Stdio::piped());
        let channel_count = usize::from(request.format.channel_count);
        let mut converter = StreamingTool::start(
            command,
            CHUNKS_BUFFERED_AHEAD,
            move |standard_output, sender, error_output_when_finished| {
                read_samples(
                    standard_output,
                    channel_count,
                    &sender,
                    &wake_up,
                    error_output_when_finished,
                );
            },
        )
        .map_err(AudioStreamError::CouldNotStart)?;
        let Some(mut into_the_converter) = converter.take_standard_input() else {
            return Err(AudioStreamError::CouldNotStart(io::Error::other(
                "the program's pipes were not created",
            )));
        };
        thread::spawn(move || {
            make_the_sound(specification, spawner, &stop_for_the_thread, |samples| {
                // (If the converter is gone, nobody wants the sound: the write fails and the graph stops.)
                let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
                into_the_converter.write_all(&bytes).is_ok()
            });
            // Closing the converter's input is what tells it the sound has ended.
            drop(into_the_converter);
        });
        Ok(Self {
            source: Source::Converted(converter),
            stop,
            problems,
        })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<AudioStreamEvent> {
        let event = match &self.source {
            Source::Direct(events) => events.try_recv().ok()?,
            Source::Converted(tool) => tool.try_next_event()?,
        };
        Some(match event {
            // (What the ffmpeg of the clips complained of is said when the sound ends, together with what the converter said.)
            AudioStreamEvent::Finished { error_output } => {
                let from_the_clips = what_the_clips_complained_of(&self.problems);
                AudioStreamEvent::Finished {
                    error_output: match (from_the_clips, error_output) {
                        (Some(clips), Some(converter)) => Some(format!("{clips}\n{converter}")),
                        (clips, converter) => clips.or(converter),
                    },
                }
            }
            samples => samples,
        })
    }
}

impl Drop for EngineAudioStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Runs the block graph for `specification` with an ffmpeg for each clip, and gives `deliver` the sound in sets of [`MOMENTS_PER_CHUNK`] moments (interleaved) until it ends, or
/// until `deliver` says (by giving `false`) that nobody wants any more, or `stop` is set.
fn make_the_sound(
    specification: SoundGraphSpecification,
    spawner: FfmpegClipSpawner,
    stop: &AtomicBool,
    mut deliver: impl FnMut(Vec<f32>) -> bool,
) {
    let feeds = LookAheadFeeds::new(specification.clone(), spawner);
    let mut graph = SoundGraph::new(specification, Box::new(feeds), Box::new(NoInsertions));
    let (mut left, mut right) = (vec![0.0; MOMENTS_PER_CHUNK], vec![0.0; MOMENTS_PER_CHUNK]);
    let mut interleaved = Vec::with_capacity(MOMENTS_PER_CHUNK * 2);
    while !stop.load(Ordering::Relaxed) {
        let count = graph.render(&mut left, &mut right);
        if count == 0 {
            return;
        }
        interleave_into(&left[..count], &right[..count], &mut interleaved);
        if !deliver(interleaved.clone()) {
            return;
        }
    }
}

/// Why the sound of an export could not be made.
#[derive(Debug, Error)]
pub enum SoundRenderError {
    #[error("the sound was being made when the export was stopped")]
    Cancelled,
    #[error("the file for the sound could not be written: {0}")]
    CouldNotWrite(#[source] io::Error),
    #[error("{0}")]
    FfmpegSaid(String),
}

/// Makes the sound of `specification` (from `sources`, which its clips name by their place in the list) and writes it into the file at `path` as raw samples, 32-bit floating
/// point, 48000 a second, stereo, which is how the encoder reads it. `is_cancelled` is asked between blocks.
pub fn render_sound_to_file(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: Vec<MediaLocation>,
    path: &Path,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), SoundRenderError> {
    let stop = Arc::new(AtomicBool::new(false));
    let spawner = FfmpegClipSpawner::new(toolchain, sources, specification, Arc::clone(&stop));
    let problems = spawner.problems();
    let feeds = LookAheadFeeds::new(specification.clone(), spawner);
    let mut graph = SoundGraph::new(specification.clone(), Box::new(feeds), Box::new(NoInsertions));
    let mut file = BufWriter::new(std::fs::File::create(path).map_err(SoundRenderError::CouldNotWrite)?);
    let (mut left, mut right) = (vec![0.0; MOMENTS_PER_CHUNK], vec![0.0; MOMENTS_PER_CHUNK]);
    let mut interleaved = Vec::with_capacity(MOMENTS_PER_CHUNK * 2);
    let mut bytes = Vec::with_capacity(MOMENTS_PER_CHUNK * 8);
    loop {
        if is_cancelled() {
            stop.store(true, Ordering::Relaxed);
            return Err(SoundRenderError::Cancelled);
        }
        let count = graph.render(&mut left, &mut right);
        if count == 0 {
            break;
        }
        interleave_into(&left[..count], &right[..count], &mut interleaved);
        bytes.clear();
        bytes.extend(interleaved.iter().flat_map(|sample| sample.to_le_bytes()));
        file.write_all(&bytes).map_err(SoundRenderError::CouldNotWrite)?;
    }
    file.flush().map_err(SoundRenderError::CouldNotWrite)?;
    match what_the_clips_complained_of(&problems) {
        Some(problem) => Err(SoundRenderError::FfmpegSaid(problem)),
        None => Ok(()),
    }
}
