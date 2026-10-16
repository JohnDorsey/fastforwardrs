// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The ffmpeg that makes the finished file: it is given the frames of the picture as raw video on its standard input, makes the sound from the source files with the
//! same filter graph the live preview's sound is made with, and writes the video and the sound into one file.
//!
//! ffmpeg is the only thing here that knows what kind of file comes out: the engine gives it planes of `yuv420p`, and it compresses them (`libx264` when it has it).

use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::output_colour::OutputTags;
use super::plan::SoundToExport;
use crate::media::export::{
    AudioGraph, FILTER_GRAPH_FILE_OPTION, SoundGraphSpecification, SoundItemToExport, VideoEncoder, build_audio_graph,
    frames_written_in_progress_line,
};
use crate::media::{FfmpegToolchain, MediaLocation, background_command, collect_error_output_in_background};
use crate::project::FrameRate;

/// How often a finishing export looks at whether ffmpeg has stopped.
const HOW_OFTEN_TO_LOOK_AT_THE_END: Duration = Duration::from_millis(20);

/// The sound as the encoder is given it.
#[derive(Clone, Debug)]
pub struct SoundForTheEncoder {
    /// The files the sound is made from, which are the encoder's inputs 1, 2, ... (input 0 is the picture), in the order the graph refers to them.
    pub files: Vec<MediaLocation>,
    /// The file that has the text of the filter graph (it is far longer than a command line allows for a project with many clips).
    pub graph_file: PathBuf,
    /// What the graph's output is called.
    pub output_label: &'static str,
}

/// What an encoder is started with.
#[derive(Clone, Debug)]
pub struct EncoderSpecification<'a> {
    /// How many pixels across and down the frames are.
    pub picture_size: (u32, u32),
    pub frame_rate: FrameRate,
    pub sound: Option<SoundForTheEncoder>,
    /// What the file says about the colours of the picture.
    pub tags: &'a OutputTags,
    pub video_encoder: VideoEncoder,
    pub output: &'a Path,
}

/// The sound graph of the export, for an encoder whose first input is the picture: the sound files are the inputs after it, so each input number of the graph is one
/// more than the position of its file in the plan.
pub fn sound_graph_for_the_encoder(sound: &SoundToExport) -> AudioGraph {
    build_audio_graph(&with_inputs_moved_up_by_one(&sound.specification))
}

fn with_inputs_moved_up_by_one(specification: &SoundGraphSpecification) -> SoundGraphSpecification {
    let mut moved = specification.clone();
    for strip in &mut moved.strips {
        for item in &mut strip.items {
            if let SoundItemToExport::Clip(clip) = item {
                clip.source_input += 1;
            }
        }
    }
    moved
}

/// The complete command line of the encoder.
pub fn encoder_arguments(specification: &EncoderSpecification<'_>) -> Vec<OsString> {
    let (width, height) = specification.picture_size;
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-y"]
        .map(OsString::from)
        .to_vec();

    // Input 0: the picture, as raw video, from the standard input. (ffmpeg does not read the keyboard from an input that is the standard input.)
    arguments.extend(
        [
            "-f".to_owned(),
            "rawvideo".to_owned(),
            "-pixel_format".to_owned(),
            "yuv420p".to_owned(),
            "-video_size".to_owned(),
            format!("{width}x{height}"),
            "-framerate".to_owned(),
            format!(
                "{}/{}",
                specification.frame_rate.numerator(),
                specification.frame_rate.denominator()
            ),
            "-i".to_owned(),
            "pipe:0".to_owned(),
        ]
        .map(OsString::from),
    );
    if let Some(sound) = &specification.sound {
        for file in &sound.files {
            arguments.push("-i".into());
            arguments.push(file.ffmpeg_input());
        }
        arguments.push(FILTER_GRAPH_FILE_OPTION.into());
        arguments.push(sound.graph_file.clone().into());
    }

    arguments.extend(["-map", "0:v"].map(OsString::from));
    if let Some(sound) = &specification.sound {
        arguments.push("-map".into());
        arguments.push(format!("[{}]", sound.output_label).into());
    }
    if let Some(filter) = specification.tags.setparams_filter() {
        arguments.push("-vf".into());
        arguments.push(filter.into());
    }

    arguments.extend(specification.video_encoder.arguments().iter().map(OsString::from));
    arguments.extend(["-pix_fmt", "yuv420p"].map(OsString::from));
    if specification.sound.is_some() {
        arguments.extend(["-c:a", "aac", "-b:a", "192k"].map(OsString::from));
    }
    if prefers_index_at_start(specification.output) {
        arguments.extend(["-movflags", "+faststart"].map(OsString::from));
    }
    // Progress goes to standard output as simple "key=value" lines.
    arguments.extend(["-progress", "pipe:1", "-nostats"].map(OsString::from));
    arguments.push(specification.output.into());
    arguments
}

/// MP4 and MOV files can have their index put at the start, so that they can be played while they are still downloading.
fn prefers_index_at_start(output: &Path) -> bool {
    matches!(
        output
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp4" | "m4v" | "mov")
    )
}

fn lock(child: &Mutex<Child>) -> MutexGuard<'_, Child> {
    child.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What can stop the encoder from another thread.
#[derive(Clone)]
pub struct EncoderKiller {
    child: Arc<Mutex<Child>>,
}

impl EncoderKiller {
    /// Stops ffmpeg, so that a write to it that is waiting (it is not reading as fast as the frames come) fails, and so that it is gone.
    pub fn kill(&self) {
        let _ = lock(&self.child).kill();
    }
}

/// How the encoder ended.
#[derive(Debug)]
pub struct EncoderEnd {
    pub status: io::Result<ExitStatus>,
    /// The end of what ffmpeg said on its error stream.
    pub error_output: String,
}

/// A running encoder. See the module documentation.
pub struct Encoder {
    child: Arc<Mutex<Child>>,
    /// The standard input, until the end of the video is told by closing it.
    stdin: Option<ChildStdin>,
    progress_reader: Option<JoinHandle<()>>,
    error_output: Option<JoinHandle<String>>,
}

impl Encoder {
    /// Starts ffmpeg. `on_progress` is called, on a thread of its own, with how many frames ffmpeg says it has written, as it says so.
    pub fn start(
        toolchain: &FfmpegToolchain,
        specification: &EncoderSpecification<'_>,
        mut on_progress: Box<dyn FnMut(u64) + Send>,
    ) -> io::Result<Self> {
        let mut child = background_command(toolchain.ffmpeg_path())
            .args(encoder_arguments(specification))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (Some(stdin), Some(standard_output), Some(standard_error)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("the encoder's pipes were not created"));
        };
        let progress_reader = thread::spawn(move || {
            for line in BufReader::new(standard_output).lines() {
                let Ok(line) = line else { break };
                if let Some(frames_written) = frames_written_in_progress_line(&line) {
                    on_progress(frames_written);
                }
            }
        });
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin: Some(stdin),
            progress_reader: Some(progress_reader),
            error_output: Some(collect_error_output_in_background(standard_error)),
        })
    }

    /// What another thread can stop the encoder with.
    pub fn killer(&self) -> EncoderKiller {
        EncoderKiller {
            child: Arc::clone(&self.child),
        }
    }

    /// Gives the encoder the next frame: the planes of a `yuv420p` picture, one after the other. Waits while ffmpeg is busy. An error means ffmpeg has stopped
    /// (or was stopped); [`Self::finish`] says why.
    pub fn write_frame(&mut self, planes: &[u8]) -> io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "the video was already ended"))?;
        stdin.write_all(planes)
    }

    /// Tells ffmpeg that there are no more frames, waits for it to finish the file, and says how it went.
    pub fn finish(mut self) -> EncoderEnd {
        drop(self.stdin.take());
        let status = loop {
            match lock(&self.child).try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(HOW_OFTEN_TO_LOOK_AT_THE_END),
                Err(error) => break Err(error),
            }
        };
        self.join_the_readers_and_collect_the_error_output(status)
    }

    /// Stops ffmpeg and waits for it to be gone, and says what it had said.
    pub fn stop(mut self) -> EncoderEnd {
        drop(self.stdin.take());
        let _ = lock(&self.child).kill();
        let status = lock(&self.child).wait();
        self.join_the_readers_and_collect_the_error_output(status)
    }

    fn join_the_readers_and_collect_the_error_output(&mut self, status: io::Result<ExitStatus>) -> EncoderEnd {
        if let Some(reader) = self.progress_reader.take() {
            let _ = reader.join();
        }
        let error_output = self
            .error_output
            .take()
            .map(|collector| collector.join().unwrap_or_default())
            .unwrap_or_default();
        EncoderEnd { status, error_output }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // (An encoder that is dropped without being finished or stopped must not leave ffmpeg running.)
        if self.stdin.is_some() || self.error_output.is_some() {
            drop(self.stdin.take());
            let mut child = lock(&self.child);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests;
