// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeping the live preview supplied with the right picture.
//!
//! The preview shows one picture at a time: the one for the current frame of the timeline. Getting
//! that picture means running ffmpeg, which takes a moment to start and then delivers pictures one
//! after another much faster than they are needed. The [`PictureSupplier`] decides, every time the
//! screen is drawn, what to do about it:
//!
//! * keep reading pictures from the running stream while playback moves forward;
//! * start a new stream where it is needed when the user jumps somewhere else, never more often
//!   than every few dozen milliseconds, so that dragging the slider cannot start a hundred ffmpeg
//!   processes a second;
//! * skip ahead (by starting a new stream) when the decoder cannot keep up;
//! * get the next clip's stream ready before playback reaches it, so that crossing from one clip to
//!   the next does not stall.
//!
//! Streams are started through the [`PictureStreamStarter`] trait. The real one runs ffmpeg; the
//! tests use a scripted one, which is what lets all of the decisions above be tested exactly.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::wanted::{UpcomingVideo, WantedPicture};
use crate::engine::{PictureColour, YuvColour, YuvMatrix, YuvRange, rgba_of_yuv420p_planes};
use crate::media::{
    DecodedFrame, DecodedYuvFrame, FfmpegToolchain, FrameDelivery, FrameStream, FrameStreamError, FrameStreamEvent,
    FrameStreamRequest, MediaLocation, NativeMatrix, NativeYuv420p, UserInterfaceWakeUp, VideoStreamFormat,
};
use crate::project::{FrameRate, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor};

/// New streams are not started more often than this. (Starting ffmpeg takes tens of milliseconds
/// and a lot of processor time, and a person dragging a slider asks for a new picture on every
/// screen refresh.)
pub const MINIMUM_TIME_BETWEEN_STREAM_STARTS: Duration = Duration::from_millis(40);

/// If playback has gone this many pictures past what the decoder has delivered, the decoder is
/// too slow to catch up by itself: a new stream is started further on instead.
pub const MOST_PICTURES_TO_WAIT_FOR: u64 = 12;

/// When playback is this close to the end of a clip (in pictures), the next clip's stream is
/// started so that its first pictures are ready by the time they are needed.
pub const PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY: u64 = 20;

/// A running source of pictures. ffmpeg is one; the tests have their own.
pub trait PictureStream {
    /// The next thing the stream has to report, if anything is ready. Never waits.
    fn try_next_event(&self) -> Option<FrameStreamEvent>;
}

impl PictureStream for FrameStream {
    fn try_next_event(&self) -> Option<FrameStreamEvent> {
        FrameStream::try_next_event(self)
    }
}

/// Starts streams of pictures.
pub trait PictureStreamStarter {
    type Stream: PictureStream;

    fn start(&self, request: &FrameStreamRequest) -> Result<Self::Stream, FrameStreamError>;
}

/// The starter that runs ffmpeg. Without ffmpeg (`None`) every start fails with a clear message,
/// so that the rest of the editor keeps working without a preview.
#[derive(Clone)]
pub struct FfmpegPictureStreamStarter {
    toolchain: Option<Arc<FfmpegToolchain>>,
    wake_up: UserInterfaceWakeUp,
}

impl FfmpegPictureStreamStarter {
    pub fn new(toolchain: Option<Arc<FfmpegToolchain>>, wake_up: UserInterfaceWakeUp) -> Self {
        Self { toolchain, wake_up }
    }
}

impl PictureStreamStarter for FfmpegPictureStreamStarter {
    type Stream = FrameStream;

    fn start(&self, request: &FrameStreamRequest) -> Result<FrameStream, FrameStreamError> {
        match &self.toolchain {
            Some(toolchain) => FrameStream::start(toolchain, request, Arc::clone(&self.wake_up)),
            None => Err(FrameStreamError::CouldNotStart(std::io::Error::other(
                "ffmpeg could not be found",
            ))),
        }
    }
}

/// The serial number the next picture supplied will get. It is counted once for all suppliers, so
/// that no two pictures ever have the same, though they were supplied by different suppliers or by
/// one that was made again since (a track's picture is kept on the graphics card by its serial number).
static NEXT_SERIAL_NUMBER: AtomicU64 = AtomicU64::new(1);

/// A video the preview can show pictures of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewVideo {
    pub source: SourceMediaIdentifier,
    /// Where the bytes of the video are.
    pub location: MediaLocation,
    /// How big the video's picture is as it is shown, so that a video of another shape than the
    /// preview's picture can be fitted inside it.
    pub display_size_in_pixels: (u32, u32),
    /// The frame rate of the video itself, which may be another than the project's.
    pub frame_rate: FrameRate,
    /// How the video's pictures are stored and what colours they say they stand for, once that is known (it takes a moment to find out, after the file is
    /// found). Only used when the setup asks for planes.
    pub format: Option<VideoStreamFormat>,
}

/// The videos the preview shows pictures of, and how big the pictures are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewSetup {
    pub frame_rate: FrameRate,
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    pub videos: Vec<PreviewVideo>,
    /// Whether the pictures of a video that is stored in a way that the picture engine can use directly (8 bits to a number, 4:2:0, and a matrix that is
    /// known: see [`VideoStreamFormat::as_native_yuv420p`]) are wanted as ffmpeg decodes them, in planes. If not, or for any other video, they are RGBA,
    /// made by ffmpeg.
    pub planes_wanted: bool,
}

/// What the colour of a video in planes is, for the picture engine.
pub fn colour_for_the_engine(native: NativeYuv420p) -> YuvColour {
    YuvColour {
        matrix: match native.matrix {
            NativeMatrix::Bt601 => YuvMatrix::Bt601,
            NativeMatrix::Bt709 => YuvMatrix::Bt709,
            NativeMatrix::Bt2020 => YuvMatrix::Bt2020,
        },
        range: if native.full_range {
            YuvRange::Full
        } else {
            YuvRange::Limited
        },
    }
}

/// The pixels of a picture the preview can show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SuppliedFrame {
    /// RGBA, as ffmpeg made it.
    Rgba(DecodedFrame),
    /// In planes, as ffmpeg decoded it, with what the numbers stand for.
    Yuv420p {
        frame: DecodedYuvFrame,
        colour: NativeYuv420p,
    },
}

impl From<DecodedFrame> for SuppliedFrame {
    fn from(frame: DecodedFrame) -> Self {
        Self::Rgba(frame)
    }
}

impl SuppliedFrame {
    /// How many pixels across and down the picture is.
    pub fn size_in_pixels(&self) -> (u32, u32) {
        match self {
            Self::Rgba(frame) => (frame.width_in_pixels, frame.height_in_pixels),
            Self::Yuv420p { frame, .. } => (frame.width_in_pixels, frame.height_in_pixels),
        }
    }

    /// The picture as RGBA: as it is, or, for one in planes, made on the processor the way the graphics card makes it. (The window draws pictures it
    /// has as RGBA; a picture in planes only comes when the picture engine is the one drawing, so this is for when it has stopped and the
    /// window must draw for itself.)
    pub fn rgba(&self) -> std::borrow::Cow<'_, DecodedFrame> {
        match self {
            Self::Rgba(frame) => std::borrow::Cow::Borrowed(frame),
            Self::Yuv420p { frame, colour } => std::borrow::Cow::Owned(DecodedFrame {
                width_in_pixels: frame.width_in_pixels,
                height_in_pixels: frame.height_in_pixels,
                rgba_pixels: rgba_of_yuv420p_planes(
                    frame.width_in_pixels,
                    frame.height_in_pixels,
                    &frame.y,
                    &frame.u,
                    &frame.v,
                    colour_for_the_engine(*colour),
                ),
            }),
        }
    }
}

/// The picture the preview can show right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppliedPicture {
    pub source: SourceMediaIdentifier,
    /// Which source frame this is a picture of.
    pub source_frame: SourceFrameIndex,
    pub frame: SuppliedFrame,
    /// What the codes of its red, green and blue stand for (for a picture in planes, the ones that the matrix makes of them): the primaries and the curve that the
    /// file says, or the ones assumed for it (see `VideoStreamFormat::colour_of_a_picture_of_size`). sRGB where nothing is known of the file.
    pub colour: PictureColour,
    /// Different for every picture ever supplied (by any supplier), so the screen code can tell
    /// cheaply whether there is a new picture to upload.
    pub serial_number: u64,
}

/// What the supplier says about the picture it can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplyReport {
    /// Whether [`PictureSupplier::picture`] is the picture that was wanted (rather than an older
    /// one, or none, while the wanted one is still being got ready).
    pub shows_the_wanted_picture: bool,
    /// If set, the supplier needs to be asked again after this long even if nothing else happens
    /// (because it put off starting a stream).
    pub ask_again_in: Option<Duration>,
}

/// A stream that has been started, with what is needed to know which pictures it will deliver.
struct RunningStream<Stream> {
    stream: Stream,
    source: SourceMediaIdentifier,
    speed_factor: SpeedFactor,
    first_source_frame: u64,
    source_frame_count: u64,
    /// What the numbers of the pictures stand for, for a stream that delivers planes.
    native_colour: Option<NativeYuv420p>,
    /// What the codes of the red, green and blue of the pictures stand for.
    picture_colour: PictureColour,
    /// How many pictures have been taken from the stream so far.
    pictures_taken: u64,
    /// Set once the stream has reported its end. A stream that has ended delivers nothing more.
    ended: bool,
}

impl<Stream> RunningStream<Stream> {
    fn new(
        stream: Stream,
        source: SourceMediaIdentifier,
        request: &FrameStreamRequest,
        native_colour: Option<NativeYuv420p>,
        picture_colour: PictureColour,
    ) -> Self {
        Self {
            stream,
            source,
            speed_factor: request.speed_factor,
            first_source_frame: request.first_source_frame.value(),
            source_frame_count: request.source_frame_count,
            native_colour,
            picture_colour,
            pictures_taken: 0,
            ended: false,
        }
    }

    /// Which of the stream's pictures (counting from 0) is `source_frame`, if it is one of them.
    fn picture_number_of(
        &self,
        source: SourceMediaIdentifier,
        speed_factor: SpeedFactor,
        source_frame: SourceFrameIndex,
    ) -> Option<u64> {
        if source != self.source || speed_factor != self.speed_factor {
            return None;
        }
        let frame = source_frame.value();
        if frame < self.first_source_frame || frame >= self.first_source_frame + self.source_frame_count {
            return None;
        }
        let frames_in = frame - self.first_source_frame;
        let step = u64::from(self.speed_factor.value());
        frames_in.is_multiple_of(step).then_some(frames_in / step)
    }

    fn source_frame_of_picture(&self, picture_number: u64) -> SourceFrameIndex {
        SourceFrameIndex::new(self.first_source_frame + picture_number * u64::from(self.speed_factor.value()))
    }

    /// The number of the picture that is `wanted`, if this stream can still deliver it: it is one
    /// of the stream's pictures, and has not been taken yet.
    fn picture_number_still_to_come(&self, wanted: &WantedPicture) -> Option<u64> {
        if self.ended {
            return None;
        }
        self.picture_number_of(wanted.source, wanted.speed_factor, wanted.source_frame)
            .filter(|&picture_number| picture_number >= self.pictures_taken)
    }
}

/// Decides, every time the screen is drawn, which stream of pictures to read, start or drop, and
/// remembers the latest picture. See the module documentation.
pub struct PictureSupplier<Starter: PictureStreamStarter> {
    starter: Starter,
    setup: Option<PreviewSetup>,
    current_stream: Option<RunningStream<Starter::Stream>>,
    upcoming_stream: Option<RunningStream<Starter::Stream>>,
    picture: Option<SuppliedPicture>,
    last_stream_start: Option<Instant>,
    /// The picture that a stream ended without delivering, so that it is not asked for again and
    /// again forever.
    could_not_deliver: Option<(SourceMediaIdentifier, SourceFrameIndex)>,
    last_problem: Option<String>,
}

impl<Starter: PictureStreamStarter> PictureSupplier<Starter> {
    pub fn new(starter: Starter) -> Self {
        Self {
            starter,
            setup: None,
            current_stream: None,
            upcoming_stream: None,
            picture: None,
            last_stream_start: None,
            could_not_deliver: None,
            last_problem: None,
        }
    }

    /// Chooses the videos pictures are made from (or none). Everything in progress is dropped.
    pub fn set_setup(&mut self, setup: Option<PreviewSetup>) {
        self.current_stream = None;
        self.upcoming_stream = None;
        self.picture = None;
        self.last_stream_start = None;
        self.could_not_deliver = None;
        self.last_problem = None;
        self.setup = setup;
    }

    pub fn setup(&self) -> Option<&PreviewSetup> {
        self.setup.as_ref()
    }

    /// The latest picture got, which may be older than the one wanted.
    pub fn picture(&self) -> Option<&SuppliedPicture> {
        self.picture.as_ref()
    }

    /// What went wrong the last time something did (ffmpeg could not be started, or complained).
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    /// Does whatever is needed to get the `wanted` picture, and says how that is going. Call it
    /// each time the screen is drawn. While `is_playing` it also gets the next clip ready ahead of
    /// time.
    pub fn supply(&mut self, wanted: Option<WantedPicture>, is_playing: bool, now: Instant) -> SupplyReport {
        let nothing_to_wait_for = SupplyReport {
            shows_the_wanted_picture: false,
            ask_again_in: None,
        };
        let Some(wanted) = wanted else {
            self.current_stream = None;
            self.upcoming_stream = None;
            return nothing_to_wait_for;
        };
        let source_matches = self
            .setup
            .as_ref()
            .is_some_and(|setup| setup.videos.iter().any(|video| video.source == wanted.source));
        if !source_matches {
            return nothing_to_wait_for;
        }

        let ask_again_in = if self.shows(&wanted) {
            None
        } else {
            self.get_the_wanted_picture(&wanted, now)
        };
        if is_playing {
            self.get_the_next_video_ready(&wanted);
        } else {
            // Nothing is about to need it, so do not keep ffmpeg running for it.
            self.upcoming_stream = None;
        }
        SupplyReport {
            shows_the_wanted_picture: self.shows(&wanted),
            ask_again_in,
        }
    }

    fn shows(&self, wanted: &WantedPicture) -> bool {
        self.picture
            .as_ref()
            .is_some_and(|picture| picture.source == wanted.source && picture.source_frame == wanted.source_frame)
    }

    /// Reads pictures from the right stream, starting a new one if none will do. Returns how long
    /// to wait before trying again if starting a stream had to be put off.
    fn get_the_wanted_picture(&mut self, wanted: &WantedPicture, now: Instant) -> Option<Duration> {
        if self.is_known_to_be_undeliverable(wanted) {
            return None;
        }

        self.switch_to_the_stream_that_can_deliver(wanted);
        if let Some(picture_number) = self.picture_number_in_current_stream(wanted) {
            self.take_pictures_up_to(picture_number, wanted);
            if self.shows(wanted) || self.is_known_to_be_undeliverable(wanted) {
                return None;
            }
            // The decoder has not caught up yet. Wait, unless it is hopelessly far behind.
            let pictures_taken = self.current_stream.as_ref().map_or(0, |stream| stream.pictures_taken);
            if picture_number.saturating_sub(pictures_taken) <= MOST_PICTURES_TO_WAIT_FOR {
                return None;
            }
        }

        self.start_stream_at(wanted, now)
    }

    /// Whether a stream has already been tried for this picture and ended (or could not start)
    /// without delivering it, so that trying again would only repeat the failure.
    fn is_known_to_be_undeliverable(&self, wanted: &WantedPicture) -> bool {
        self.could_not_deliver == Some((wanted.source, wanted.source_frame))
    }

    /// Makes the stream that was got ready in advance the current one, if the current stream will
    /// not deliver the wanted picture but that one will.
    fn switch_to_the_stream_that_can_deliver(&mut self, wanted: &WantedPicture) {
        if self.picture_number_in_current_stream(wanted).is_some() {
            return;
        }
        // (An upcoming stream that cannot deliver it is of no use any more, and is dropped.)
        if let Some(upcoming) = self.upcoming_stream.take()
            && upcoming.picture_number_still_to_come(wanted).is_some()
        {
            self.current_stream = Some(upcoming);
        }
    }

    fn picture_number_in_current_stream(&self, wanted: &WantedPicture) -> Option<u64> {
        self.current_stream.as_ref()?.picture_number_still_to_come(wanted)
    }

    /// Takes pictures from the current stream up to number `picture_number`, keeping only the
    /// last one taken: the ones before it are already out of date.
    fn take_pictures_up_to(&mut self, picture_number: u64, wanted: &WantedPicture) {
        let Some(stream) = self.current_stream.as_mut() else {
            return;
        };
        let mut latest_picture: Option<(u64, SuppliedFrame)> = None;
        while stream.pictures_taken <= picture_number {
            match stream.stream.try_next_event() {
                Some(FrameStreamEvent::Frame(frame)) => {
                    latest_picture = Some((stream.pictures_taken, SuppliedFrame::Rgba(frame)));
                    stream.pictures_taken += 1;
                }
                Some(FrameStreamEvent::YuvFrame(frame)) => {
                    // (A stream only delivers planes if it was asked to, and was then given the colour to say they stand for.)
                    let colour = stream.native_colour.unwrap_or(NativeYuv420p {
                        matrix: NativeMatrix::Bt601,
                        full_range: false,
                    });
                    latest_picture = Some((stream.pictures_taken, SuppliedFrame::Yuv420p { frame, colour }));
                    stream.pictures_taken += 1;
                }
                Some(FrameStreamEvent::Finished { error_output }) => {
                    stream.ended = true;
                    // The stream ended before it reached the picture that was wanted.
                    self.could_not_deliver = Some((wanted.source, wanted.source_frame));
                    if let Some(error_output) = error_output {
                        self.last_problem = Some(error_output);
                    }
                    break;
                }
                None => break,
            }
        }
        if let Some((taken_number, frame)) = latest_picture {
            self.picture = Some(SuppliedPicture {
                source: stream.source,
                source_frame: stream.source_frame_of_picture(taken_number),
                frame,
                colour: stream.picture_colour,
                serial_number: NEXT_SERIAL_NUMBER.fetch_add(1, Ordering::Relaxed),
            });
        }
    }

    /// Starts a stream whose first picture is the wanted one, replacing the current stream. Returns
    /// how long to wait if one was started too recently.
    fn start_stream_at(&mut self, wanted: &WantedPicture, now: Instant) -> Option<Duration> {
        if let Some(last_start) = self.last_stream_start {
            let since = now.saturating_duration_since(last_start);
            if since < MINIMUM_TIME_BETWEEN_STREAM_STARTS {
                return Some(MINIMUM_TIME_BETWEEN_STREAM_STARTS - since);
            }
        }
        let (request, native_colour, picture_colour) = self.request_for(
            wanted.source,
            wanted.source_frame,
            wanted.range_end_source_frame,
            wanted.speed_factor,
        )?;
        self.last_stream_start = Some(now);
        self.upcoming_stream = None;
        match self.starter.start(&request) {
            Ok(stream) => {
                self.current_stream = Some(RunningStream::new(
                    stream,
                    wanted.source,
                    &request,
                    native_colour,
                    picture_colour,
                ));
                if let Some(picture_number) = self.picture_number_in_current_stream(wanted) {
                    self.take_pictures_up_to(picture_number, wanted);
                }
            }
            Err(error) => {
                self.current_stream = None;
                self.could_not_deliver = Some((wanted.source, wanted.source_frame));
                self.last_problem = Some(error.to_string());
            }
        }
        None
    }

    /// While near the end of a clip, starts the stream for the clip that follows.
    fn get_the_next_video_ready(&mut self, wanted: &WantedPicture) {
        if self.upcoming_stream.is_some() || wanted.pictures_left > PICTURES_BEFORE_THE_END_TO_GET_THE_NEXT_CLIP_READY {
            return;
        }
        let Some(next_video) = wanted.next_video else { return };
        // The stream being read may already run on into the next clip (when both have the same
        // speed and one clip was split in two after the stream started).
        let already_covered = self.current_stream.as_ref().is_some_and(|stream| {
            stream
                .picture_number_of(
                    next_video.source,
                    next_video.speed_factor,
                    next_video.first_source_frame,
                )
                .is_some()
        });
        if !already_covered {
            self.start_upcoming_stream(&next_video);
        }
    }

    fn start_upcoming_stream(&mut self, next_video: &UpcomingVideo) {
        let Some((request, native_colour, picture_colour)) = self.request_for(
            next_video.source,
            next_video.first_source_frame,
            next_video.end_source_frame,
            next_video.speed_factor,
        ) else {
            return;
        };
        if let Ok(stream) = self.starter.start(&request) {
            self.upcoming_stream = Some(RunningStream::new(
                stream,
                next_video.source,
                &request,
                native_colour,
                picture_colour,
            ));
        }
    }

    /// The request for a stream of the pictures of one stretch of video, from `first_source_frame`
    /// to the end of the stretch, and what the numbers of its pictures stand for if it delivers planes, or `None` if the video is not one this
    /// supplier shows.
    fn request_for(
        &self,
        source: SourceMediaIdentifier,
        first_source_frame: SourceFrameIndex,
        range_end_source_frame: SourceFrameIndex,
        speed_factor: SpeedFactor,
    ) -> Option<(FrameStreamRequest, Option<NativeYuv420p>, PictureColour)> {
        let setup = self.setup.as_ref()?;
        let video = setup.videos.iter().find(|video| video.source == source)?;
        let (width, height) = video.display_size_in_pixels;
        let native_colour = video
            .format
            .as_ref()
            .and_then(|format| format.as_native_yuv420p(width, height))
            .filter(|_| setup.planes_wanted);
        // What the codes stand for is what the file says of its primaries and its curve, or what is assumed (sRGB where the file's format is not known yet).
        let picture_colour = video.format.as_ref().map_or(PictureColour::SRGB, |format| {
            let colour = format.colour_of_a_picture_of_size(width, height);
            PictureColour {
                primaries: colour.primaries,
                curve: colour.curve,
            }
        });
        let request = FrameStreamRequest {
            video: video.location.clone(),
            project_frame_rate: setup.frame_rate,
            video_frame_rate: video.frame_rate,
            first_source_frame,
            source_frame_count: range_end_source_frame
                .value()
                .saturating_sub(first_source_frame.value()),
            speed_factor,
            video_display_size_in_pixels: video.display_size_in_pixels,
            picture_width_in_pixels: setup.picture_width_in_pixels,
            picture_height_in_pixels: setup.picture_height_in_pixels,
            delivery: if native_colour.is_some() {
                FrameDelivery::Yuv420p
            } else {
                FrameDelivery::Rgba
            },
        };
        Some((request, native_colour, picture_colour))
    }
}

#[cfg(test)]
mod tests;
