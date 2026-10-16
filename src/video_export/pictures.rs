// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures of the runs of video of an export: one decode process for each run, started a little before its first frame and let go of with its last, and the
//! picture of a run at a frame of the timeline taken from it, in order.
//!
//! The decode is the preview's (`FrameStream`: the very same choice of pictures, so that the export has the frames the preview shows), except for how big the
//! pictures are made and how well (see [`VideoRun::delivered_size_in_pixels`]). What a stream delivers is turned into a [`SuppliedPicture`], which is what
//! the preview is given too, so that the picture of a frame is described to the engine by the same code.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use thiserror::Error;

use super::plan::ExportPlan;
use super::runs::VideoRun;
use crate::media::{
    FfmpegToolchain, FrameDelivery, FrameStream, FrameStreamError, FrameStreamEvent, FrameStreamRequest, MediaLocation,
    NativeYuv420p, WaitedFor,
};
use crate::playback::{SuppliedFrame, SuppliedPicture};
use crate::project::{SourceFrameIndex, SourceMediaIdentifier};

/// How long the export waits for a picture before it gives up (ffmpeg has to start and seek, which takes a moment, and a file on a network drive can take longer).
const HOW_LONG_A_PICTURE_MAY_TAKE: Duration = Duration::from_secs(120);

/// How long a wait for a picture lasts before the export looks at whether it was cancelled.
const HOW_LONG_BETWEEN_LOOKS_AT_THE_CANCEL: Duration = Duration::from_millis(100);

/// How many frames before its first frame a run is started, so that ffmpeg has started and seeked when the picture is wanted.
pub const FRAMES_AHEAD_THAT_A_RUN_IS_STARTED: u64 = 45;

/// A running source of the pictures of one run. ffmpeg is one; the tests have their own.
pub trait RunStream {
    /// The next thing the stream has to report, waiting for it for as long as `how_long` if it is not ready.
    fn wait_for_an_event(&mut self, how_long: Duration) -> WaitedFor<FrameStreamEvent>;
}

impl RunStream for FrameStream {
    fn wait_for_an_event(&mut self, how_long: Duration) -> WaitedFor<FrameStreamEvent> {
        FrameStream::wait_for_an_event(self, how_long)
    }
}

/// Starts the streams of runs.
pub trait RunStreamStarter {
    type Stream: RunStream;

    /// Starts a stream that delivers the pictures of `request` `delivered_size` pixels big.
    fn start(&self, request: &FrameStreamRequest, delivered_size: (u32, u32))
    -> Result<Self::Stream, FrameStreamError>;
}

/// The starter that runs ffmpeg, scaling as ffmpeg does by default (bicubic), which is what the export has always done.
#[derive(Clone)]
pub struct FfmpegRunStreamStarter {
    pub toolchain: Arc<FfmpegToolchain>,
}

impl RunStreamStarter for FfmpegRunStreamStarter {
    type Stream = FrameStream;

    fn start(&self, request: &FrameStreamRequest, delivered_size: (u32, u32)) -> Result<FrameStream, FrameStreamError> {
        FrameStream::start_delivering(&self.toolchain, request, delivered_size, "bicubic", Arc::new(|| {}))
    }
}

#[derive(Debug, Error)]
pub enum RunPictureProblem {
    #[error("{video} cannot be read: {why}")]
    CouldNotStart { video: String, why: String },
    #[error("ffmpeg gave no picture of {video}{}", ffmpeg_said.as_ref().map(|words| format!(": {words}")).unwrap_or_default())]
    NoPicture { video: String, ffmpeg_said: Option<String> },
    #[error("a picture of {video} did not come in {limit:?}")]
    TookTooLong { video: String, limit: Duration },
    #[error("the export was cancelled")]
    Cancelled,
}

/// What the export needs to read a run.
struct RunToRead {
    run: VideoRun,
    request: FrameStreamRequest,
    /// What the planes stand for, if the pictures are delivered as planes.
    colour: Option<NativeYuv420p>,
    /// What the video is called in a message.
    name: String,
}

/// How far a run has been read.
struct Reading<Stream> {
    /// The stream, from when the run is started until its last picture has been taken.
    stream: Option<Stream>,
    started: bool,
    /// How many pictures of the run have been taken.
    delivered: u64,
    /// The last picture taken, which stands in for the ones that the stream did not have if it ends too soon, and is the one given again if a frame is asked for again.
    last: Option<Arc<SuppliedPicture>>,
    /// What ffmpeg said when the stream ended, once it has (`Some(None)` is an end with nothing said).
    ended: Option<Option<String>>,
}

impl<Stream> Reading<Stream> {
    fn new() -> Self {
        Self {
            stream: None,
            started: false,
            delivered: 0,
            last: None,
            ended: None,
        }
    }
}

/// The pictures of the runs of an export. See the module documentation.
pub struct PicturesOfTheRuns<Starter: RunStreamStarter> {
    starter: Starter,
    runs: Vec<RunToRead>,
    reading: Vec<Reading<Starter::Stream>>,
    next_serial_number: u64,
    how_long_a_picture_may_take: Duration,
    /// How many pictures were made by repeating the last because a stream had ended before the run did.
    pictures_held: u64,
}

impl<Starter: RunStreamStarter> PicturesOfTheRuns<Starter> {
    /// The pictures of the runs of `plan`. `locations` are where the video files are, and `planes` says, for each video that is stored in a way that the engine can take
    /// as ffmpeg decodes it, what the planes stand for (the others are delivered as RGBA, made by ffmpeg).
    pub fn new(
        starter: Starter,
        plan: &ExportPlan,
        locations: &HashMap<SourceMediaIdentifier, MediaLocation>,
        planes: &HashMap<SourceMediaIdentifier, NativeYuv420p>,
    ) -> Result<Self, RunPictureProblem> {
        let mut runs = Vec::with_capacity(plan.runs.len());
        for run in &plan.runs {
            let (Some(video), Some(location)) = (plan.videos.get(&run.source), locations.get(&run.source)) else {
                return Err(RunPictureProblem::CouldNotStart {
                    video: format!("the video {}", run.source),
                    why: "the project does not know where it is".to_owned(),
                });
            };
            let colour = planes.get(&run.source).copied();
            runs.push(RunToRead {
                request: FrameStreamRequest {
                    video: location.clone(),
                    project_frame_rate: plan.frame_rate,
                    video_frame_rate: video.frame_rate,
                    first_source_frame: SourceFrameIndex::new(run.first_source_frame),
                    source_frame_count: run.source_frame_count,
                    speed_factor: run.speed,
                    video_display_size_in_pixels: video.display_size_in_pixels,
                    picture_width_in_pixels: plan.picture_width_in_pixels,
                    picture_height_in_pixels: plan.picture_height_in_pixels,
                    delivery: if colour.is_some() {
                        FrameDelivery::Yuv420p
                    } else {
                        FrameDelivery::Rgba
                    },
                },
                colour,
                name: location.file().file_name().map_or_else(
                    || format!("the video {}", run.source),
                    |name| name.to_string_lossy().into_owned(),
                ),
                run: run.clone(),
            });
        }
        let reading = runs.iter().map(|_| Reading::new()).collect();
        Ok(Self {
            starter,
            runs,
            reading,
            next_serial_number: 1,
            how_long_a_picture_may_take: HOW_LONG_A_PICTURE_MAY_TAKE,
            pictures_held: 0,
        })
    }

    /// For the tests: the time that a picture is waited for.
    #[cfg(test)]
    pub fn wait_for_a_picture_no_longer_than(&mut self, how_long: Duration) {
        self.how_long_a_picture_may_take = how_long;
    }

    /// How many pictures were repeated because a stream ended before its run did.
    pub fn pictures_held(&self) -> u64 {
        self.pictures_held
    }

    /// Starts the runs that are to begin within a few frames of `timeline_frame` (or have begun and were not started), so that their pictures are coming when they are
    /// wanted.
    pub fn start_the_runs_that_begin_soon(&mut self, timeline_frame: u64) -> Result<(), RunPictureProblem> {
        for number in 0..self.runs.len() {
            let run = &self.runs[number].run;
            let is_wanted_soon = run.first_timeline_frame <= timeline_frame + FRAMES_AHEAD_THAT_A_RUN_IS_STARTED
                && timeline_frame < run.end_timeline_frame;
            if is_wanted_soon && !self.reading[number].started {
                self.start(number)?;
            }
        }
        Ok(())
    }

    fn start(&mut self, number: usize) -> Result<(), RunPictureProblem> {
        let to_read = &self.runs[number];
        let stream = self
            .starter
            .start(&to_read.request, to_read.run.delivered_size_in_pixels)
            .map_err(|error| RunPictureProblem::CouldNotStart {
                video: to_read.name.clone(),
                why: error.to_string(),
            })?;
        self.reading[number].stream = Some(stream);
        self.reading[number].started = true;
        Ok(())
    }

    /// The picture of run `number` at `timeline_frame` (a frame it is seen at). The pictures of a run are asked for in order, a frame after a frame, and the same frame
    /// may be asked for again. Waits for the picture if it is not here yet, looking at `cancelled` every now and then. If the stream ends before the run does, the last
    /// picture it had stands in for the pictures it did not have (and is counted: [`Self::pictures_held`]).
    pub fn picture(
        &mut self,
        number: usize,
        timeline_frame: u64,
        cancelled: &AtomicBool,
    ) -> Result<Arc<SuppliedPicture>, RunPictureProblem> {
        let first_timeline_frame = self.runs[number].run.first_timeline_frame;
        debug_assert!(
            first_timeline_frame <= timeline_frame && timeline_frame < self.runs[number].run.end_timeline_frame,
            "frame {timeline_frame} is not a frame of run {number}"
        );
        let wanted = timeline_frame - first_timeline_frame;
        if !self.reading[number].started {
            self.start(number)?;
        }
        let Self {
            runs,
            reading,
            next_serial_number,
            how_long_a_picture_may_take,
            pictures_held,
            ..
        } = self;
        let to_read = &runs[number];
        let reading = &mut reading[number];
        debug_assert!(
            reading.delivered <= wanted + 1,
            "a picture of the run before the last one taken was asked for"
        );
        let started_waiting = Instant::now();
        while reading.delivered <= wanted {
            if cancelled.load(Ordering::SeqCst) {
                return Err(RunPictureProblem::Cancelled);
            }
            if let Some(ended) = &reading.ended {
                // The stream has no more pictures: the last stands in for the rest, and a stream that had none at all is a mistake.
                if reading.last.is_none() {
                    return Err(RunPictureProblem::NoPicture {
                        video: to_read.name.clone(),
                        ffmpeg_said: ended.clone(),
                    });
                }
                *pictures_held += wanted + 1 - reading.delivered;
                reading.delivered = wanted + 1;
                break;
            }
            let stream = reading
                .stream
                .as_mut()
                .expect("a run that has not ended and has pictures to give has its stream");
            let frame = match stream.wait_for_an_event(HOW_LONG_BETWEEN_LOOKS_AT_THE_CANCEL) {
                WaitedFor::Event(FrameStreamEvent::Frame(frame)) => SuppliedFrame::Rgba(frame),
                WaitedFor::Event(FrameStreamEvent::YuvFrame(frame)) => SuppliedFrame::Yuv420p {
                    frame,
                    colour: to_read
                        .colour
                        .expect("pictures in planes are asked for only of a video that can be had so"),
                },
                WaitedFor::Event(FrameStreamEvent::Finished { error_output }) => {
                    reading.ended = Some(error_output);
                    continue;
                }
                WaitedFor::NothingYet => {
                    if started_waiting.elapsed() > *how_long_a_picture_may_take {
                        return Err(RunPictureProblem::TookTooLong {
                            video: to_read.name.clone(),
                            limit: *how_long_a_picture_may_take,
                        });
                    }
                    continue;
                }
                WaitedFor::Ended => {
                    reading.ended = Some(Some("the reading of the pictures stopped".to_owned()));
                    continue;
                }
            };
            let serial_number = *next_serial_number;
            *next_serial_number += 1;
            reading.last = Some(Arc::new(SuppliedPicture {
                source: to_read.run.source,
                source_frame: SourceFrameIndex::new(to_read.run.source_frame_of_picture(reading.delivered)),
                frame,
                serial_number,
            }));
            reading.delivered += 1;
        }
        let picture = Arc::clone(reading.last.as_ref().expect("a picture was delivered"));
        if wanted + 1 >= to_read.run.picture_count() {
            // That was the last picture of the run: the stream is let go of, and ffmpeg is told to stop.
            reading.stream = None;
        }
        Ok(picture)
    }
}

#[cfg(test)]
mod tests;
