// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoding the frames of the live preview, and of the export.
//!
//! The preview must show exactly the frames the export will contain. So a stream is told, in the
//! editor's own terms, which source frame to start at, how many source frames to read and at what
//! speed, and **the export reads its video through the very same streams** (one for each run of a
//! clip's video: see `video_export`), asking for the pictures at the size the layer needs and with ffmpeg's
//! own scaling. Frame number `k` that comes out of a stream is the clip's frame
//! number `k`: for a video that does not run at the frame rate of the project too, which then has,
//! in each frame, the picture that is nearest in time (see [`frame_stream_arguments`]).

use std::ffi::OsString;
use std::io::{self, Read};
use std::sync::mpsc::SyncSender;

use thiserror::Error;

use super::UserInterfaceWakeUp;
pub use super::child_process::WaitedFor;
use super::child_process::{StreamingTool, background_command};
use super::ffmpeg_toolchain::FfmpegToolchain;
use super::location::MediaLocation;
use crate::colour_space::{VideoMatrix, VideoRange};
use crate::project::{FrameRate, SourceFrameIndex, SpeedFactor, fit_inside_picture};

/// How many decoded pictures may wait to be shown before decoding pauses. Small, because each
/// picture is several megabytes and the decoder is much faster than playback.
const FRAMES_BUFFERED_AHEAD: usize = 6;

const BYTES_PER_PIXEL: usize = 4;

/// What the pictures of a stream are delivered as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameDelivery {
    /// Red, green, blue and how opaque, four bytes to a pixel, made by ffmpeg (which does what it does with the colours of the video).
    Rgba,
    /// As ffmpeg decodes them, in `yuv420p`: a plane of brightness, and two planes of colour differences with a pixel for every square of four, each plane
    /// after the one before. What the numbers stand for is not said by the bytes: whoever asked for them knows it from the video's tags. Only for a
    /// video that is stored that way (see `VideoStreamFormat::as_native_yuv420p`).
    Yuv420p,
}

/// What ffmpeg is **told** about the planes of a video that it turns into red, green and blue itself (a stream that delivers [`FrameDelivery::Rgba`]), so that it does not
/// guess: its guess for a file that says nothing is BT.601 whatever the size of the picture, where the program reads such a file by the size (BT.709 for high definition).
/// A part that is `None` is left to ffmpeg.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HowToReadColours {
    pub matrix: Option<VideoMatrix>,
    pub range: Option<VideoRange>,
}

impl HowToReadColours {
    /// The options of ffmpeg's `scale` filter that say it (each with a colon in front), or nothing for a part that is left to ffmpeg.
    fn as_scale_options(self) -> String {
        let mut options = String::new();
        if let Some(matrix) = self.matrix {
            options.push_str(match matrix {
                VideoMatrix::Bt601 => ":in_color_matrix=bt601",
                VideoMatrix::Bt709 => ":in_color_matrix=bt709",
                VideoMatrix::Bt2020 => ":in_color_matrix=bt2020",
            });
        }
        if let Some(range) = self.range {
            options.push_str(match range {
                VideoRange::Limited => ":in_range=limited",
                VideoRange::Full => ":in_range=full",
            });
        }
        options
    }
}

/// What to decode.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameStreamRequest {
    /// Where the bytes of the video are.
    pub video: MediaLocation,
    /// The frame rate of the project: the rate at which the pictures are delivered, and the one the
    /// numbers of the frames in this request are at.
    pub project_frame_rate: FrameRate,
    /// The frame rate of the video itself. It says how long each of the video's pictures lasts, and so
    /// how far back in the file the reading has to begin to have the picture that is on screen at the
    /// first frame wanted, when that picture began before it.
    pub video_frame_rate: FrameRate,
    /// The first source frame to show.
    pub first_source_frame: SourceFrameIndex,
    /// How many source frames to read, starting at `first_source_frame`. With a speed factor of
    /// `N` only every `N`th of them is delivered.
    pub source_frame_count: u64,
    pub speed_factor: SpeedFactor,
    /// How big the video's picture is as it is shown.
    pub video_display_size_in_pixels: (u32, u32),
    /// The size of the finished picture of the preview. The video is delivered as big as it can be
    /// inside it, in its own shape: see [`Self::delivered_size`].
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    /// What the pictures are delivered as.
    pub delivery: FrameDelivery,
    /// What ffmpeg is told about the planes it turns into colours, for a stream that delivers RGBA (`None` leaves everything to ffmpeg). A stream that delivers planes
    /// turns nothing into colours, and this is not used.
    pub ffmpeg_reads_planes_as: Option<HowToReadColours>,
}

impl FrameStreamRequest {
    /// The size every delivered picture is: the video as big as it can be inside the finished
    /// picture, in its own shape. There is no black round it: where it goes on the finished picture
    /// is for whoever draws it to say (it can be turned, made smaller, moved), so it is delivered as
    /// the picture it is.
    pub fn delivered_size(&self) -> (u32, u32) {
        let fit = fit_inside_picture(
            self.video_display_size_in_pixels.0,
            self.video_display_size_in_pixels.1,
            self.picture_width_in_pixels,
            self.picture_height_in_pixels,
        );
        (fit.width_in_pixels, fit.height_in_pixels)
    }
}

/// One decoded picture, as RGBA bytes (four per pixel, row by row, top row first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedFrame {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub rgba_pixels: Vec<u8>,
}

/// One decoded picture, as ffmpeg decodes it, in planes (see [`FrameDelivery::Yuv420p`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedYuvFrame {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    /// The brightness of every pixel, row by row, the top row first.
    pub y: Vec<u8>,
    /// The colour difference toward blue of every square of four pixels, and `v` the one toward red. (A picture that is not an even number of pixels across
    /// or down has one for the last single column or row.)
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

/// What a running stream reports.
#[derive(Debug)]
pub enum FrameStreamEvent {
    /// The next frame of the clip. Frames arrive in order, starting from the clip's frame 0.
    Frame(DecodedFrame),
    /// The next frame of the clip, for a stream that delivers [`FrameDelivery::Yuv420p`].
    YuvFrame(DecodedYuvFrame),
    /// The stream has ended: it reached the end of the clip, or ffmpeg stopped. `error_output`
    /// holds ffmpeg's complaint if it stopped because of an error.
    Finished { error_output: Option<String> },
}

#[derive(Debug, Error)]
pub enum FrameStreamError {
    #[error("ffmpeg could not be started to decode the preview: {0}")]
    CouldNotStart(#[source] io::Error),
    #[error("the preview picture must be at least one pixel wide and tall")]
    EmptyPicture,
}

/// A running ffmpeg process that delivers decoded frames in the background. Dropping the stream
/// stops ffmpeg.
pub struct FrameStream {
    tool: StreamingTool<FrameStreamEvent>,
}

impl FrameStream {
    /// Starts decoding. Frames are decoded a few at a time, ahead of when they are asked for; the
    /// `wake_up` callback is called each time something new is waiting.
    pub fn start(
        toolchain: &FfmpegToolchain,
        request: &FrameStreamRequest,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, FrameStreamError> {
        Self::start_delivering(
            toolchain,
            request,
            request.delivered_size(),
            QUICK_SCALING_OF_THE_PREVIEW,
            wake_up,
        )
    }

    /// Like [`Self::start`], but the pictures are delivered `delivered_size` pixels big (not as big as can be inside the picture of the request) and scaled by ffmpeg
    /// with `scaling` (such as `bicubic`, ffmpeg's own and best default, which the export has always used). For the export, which decodes a video as big as the layer
    /// that shows it needs it.
    pub fn start_delivering(
        toolchain: &FfmpegToolchain,
        request: &FrameStreamRequest,
        delivered_size: (u32, u32),
        scaling: &str,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, FrameStreamError> {
        if request.picture_width_in_pixels == 0
            || request.picture_height_in_pixels == 0
            || delivered_size.0 == 0
            || delivered_size.1 == 0
        {
            return Err(FrameStreamError::EmptyPicture);
        }
        let mut command = background_command(toolchain.ffmpeg_path());
        command.args(frame_stream_arguments_delivering(request, delivered_size, scaling));
        let (delivered_width, delivered_height) = delivered_size;
        let frame_size = FrameSize {
            width_in_pixels: delivered_width,
            height_in_pixels: delivered_height,
            delivery: request.delivery,
        };
        let tool = StreamingTool::start(
            command,
            FRAMES_BUFFERED_AHEAD,
            move |standard_output, sender, error_output_when_finished| {
                read_frames(
                    standard_output,
                    frame_size,
                    &sender,
                    &wake_up,
                    error_output_when_finished,
                );
            },
        )
        .map_err(FrameStreamError::CouldNotStart)?;
        Ok(Self { tool })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<FrameStreamEvent> {
        self.tool.try_next_event()
    }

    /// The next event, waiting for it for as long as `how_long` if it is not ready.
    pub fn wait_for_an_event(&self, how_long: std::time::Duration) -> WaitedFor<FrameStreamEvent> {
        self.tool.wait_for_an_event(how_long)
    }
}

/// How the preview has ffmpeg scale the pictures it decodes: quickly (the preview wants them in time, and is not looked at closely).
pub const QUICK_SCALING_OF_THE_PREVIEW: &str = "fast_bilinear";

#[derive(Clone, Copy)]
struct FrameSize {
    width_in_pixels: u32,
    height_in_pixels: u32,
    delivery: FrameDelivery,
}

impl FrameSize {
    /// How many pixels across and down each of the two planes of colour differences is, in a picture of this size.
    fn chroma_size(self) -> (usize, usize) {
        (
            (self.width_in_pixels as usize).div_ceil(2),
            (self.height_in_pixels as usize).div_ceil(2),
        )
    }

    /// How many bytes one picture takes up in the pipe.
    fn byte_count(self) -> usize {
        let pixels = self.width_in_pixels as usize * self.height_in_pixels as usize;
        match self.delivery {
            FrameDelivery::Rgba => pixels * BYTES_PER_PIXEL,
            FrameDelivery::Yuv420p => {
                let (chroma_width, chroma_height) = self.chroma_size();
                pixels + 2 * chroma_width * chroma_height
            }
        }
    }

    /// The picture that these bytes (as many as [`Self::byte_count`]) are.
    fn into_event(self, bytes: Vec<u8>) -> FrameStreamEvent {
        match self.delivery {
            FrameDelivery::Rgba => FrameStreamEvent::Frame(DecodedFrame {
                width_in_pixels: self.width_in_pixels,
                height_in_pixels: self.height_in_pixels,
                rgba_pixels: bytes,
            }),
            FrameDelivery::Yuv420p => {
                let brightness_bytes = self.width_in_pixels as usize * self.height_in_pixels as usize;
                let (chroma_width, chroma_height) = self.chroma_size();
                let chroma_bytes = chroma_width * chroma_height;
                let mut planes = bytes;
                let v = planes.split_off(brightness_bytes + chroma_bytes);
                let u = planes.split_off(brightness_bytes);
                FrameStreamEvent::YuvFrame(DecodedYuvFrame {
                    width_in_pixels: self.width_in_pixels,
                    height_in_pixels: self.height_in_pixels,
                    y: planes,
                    u,
                    v,
                })
            }
        }
    }
}

/// Reads whole pictures from `standard_output` and sends them on, until the data ends. Then
/// sends [`FrameStreamEvent::Finished`] with whatever `error_output_when_finished` returns.
fn read_frames(
    mut standard_output: impl Read,
    frame_size: FrameSize,
    sender: &SyncSender<FrameStreamEvent>,
    wake_up: &UserInterfaceWakeUp,
    error_output_when_finished: impl FnOnce() -> String,
) {
    loop {
        let mut bytes = vec![0u8; frame_size.byte_count()];
        if standard_output.read_exact(&mut bytes).is_err() {
            break;
        }
        // A full buffer makes this wait, which is what keeps the decoder from racing ahead. An
        // error means the stream was dropped, so nobody wants more frames.
        if sender.send(frame_size.into_event(bytes)).is_err() {
            return;
        }
        wake_up();
    }
    let error_output = error_output_when_finished();
    let finished = FrameStreamEvent::Finished {
        error_output: (!error_output.is_empty()).then_some(error_output),
    };
    let _ = sender.send(finished);
    wake_up();
}

/// How much later than the exact middle between two frames of the project a picture of the video has to
/// be taken to be, in microseconds.
///
/// A picture that is exactly half way between two frames of the project goes to the later of them: that
/// is how ffmpeg rounds a half, when it does its sums on the exact times, as the export does. The times
/// a stream does its sums on are not exact: ffmpeg moves them by the time it seeked to, to a whole number
/// of the file's own ticks, so they are up to half a tick out, and a picture that is exactly half way
/// would go to either frame by chance, and the preview would not agree with the export about it. Taking
/// every picture to be this much later than it is settles those the way the export does. It is more than
/// half a tick of any file that is made for editing (a tick is a ten thousandth of a second or finer),
/// and a three-hundredth of a frame at 30 frames a second, which no picture is likely to be that near to
/// the middle of two frames except by being exactly in it.
const BIAS_TOWARDS_THE_LATER_FRAME_IN_MICROSECONDS: i64 = 100;

/// Where a stream reads its file from and to, and where its first frame is in what is read, all in whole
/// microseconds (the unit ffmpeg is told them in, so that nothing is lost to rounding on the way).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StreamTiming {
    /// Where in the file the reading starts.
    start_in_file: u64,
    /// How long is read from there.
    duration_read: u64,
    /// How long after the start of what is read the first frame wanted is.
    first_frame_after_the_start: u64,
}

impl StreamTiming {
    /// The reading starts before the first frame wanted by half a frame of the project, so that rounding
    /// in the file's timestamps can never cut a wanted picture off, and by as much more as a picture of
    /// the video lasts longer than a frame of the project: a video of 25 pictures a second has, in a frame
    /// of a project of 30, the picture that began up to 40 ms before, and that picture is before the
    /// first frame wanted. It ends half a frame before the frame that follows the last wanted (measured
    /// with videos of 5 to 60 pictures a second in projects of 12 to 30 frames a second: that is enough
    /// for every frame wanted to come out, however slow the pictures are, and no more is read).
    fn of(request: &FrameStreamRequest) -> Self {
        let project_frame_rate = request.project_frame_rate;
        let first_frame_at = project_frame_rate.microseconds_at_frame(request.first_source_frame.value());
        let end_at = project_frame_rate.microseconds_at_frame(
            request
                .first_source_frame
                .value()
                .saturating_add(request.source_frame_count),
        );

        let frame = 1_000_000.0 / project_frame_rate.frames_per_second();
        let half_a_frame = frame / 2.0;
        let picture_of_the_video = 1_000_000.0 / request.video_frame_rate.frames_per_second();
        let longer_than_a_frame = (picture_of_the_video - frame).max(0.0);

        let lead_in = (half_a_frame + longer_than_a_frame).ceil() as u64;
        let start_in_file = first_frame_at.saturating_sub(lead_in);
        let stop_in_file = end_at as f64 - half_a_frame;
        Self {
            start_in_file,
            duration_read: (stop_in_file - start_in_file as f64).max(0.0).round() as u64,
            first_frame_after_the_start: first_frame_at - start_in_file,
        }
    }
}

/// Microseconds as ffmpeg is given them as seconds: "1.500000".
fn microseconds_as_seconds(microseconds: u64) -> String {
    format!("{}.{:06}", microseconds / 1_000_000, microseconds % 1_000_000)
}

/// The ffmpeg command line that decodes `request`.
///
/// The pictures are chosen the way the export chooses them (it reads its pictures through this very command line, see `video_export`): resample
/// to the project's constant frame rate with `round=near` so that there is one picture for every
/// frame, whatever the file's frame rate and timestamps look like, then keep every `speed_factor`th
/// picture of the clip, starting with its first.
///
/// What comes out has to be what the export has at the same frames, wherever the stream starts, even
/// though it reads only a stretch of the file:
///
/// * it begins reading early enough (see `StreamTiming::of`) to have the picture that is on screen at
///   the first frame wanted, which may have begun before it;
/// * the timestamps are made microseconds (`settb=AVTB`) and moved by a whole number of them, so that the
///   first frame wanted is at the time 0 (a little later, see
///   `BIAS_TOWARDS_THE_LATER_FRAME_IN_MICROSECONDS`), and `fps` is told to start at 0 too
///   (`start_time=0`). Without that it would begin with the first picture it is given, wherever that is,
///   and every picture would be a frame too early whenever no picture was in the frame wanted first;
/// * `-frames:v` ends the stream after the pictures that are wanted, however many the reading brought.
pub fn frame_stream_arguments(request: &FrameStreamRequest) -> Vec<OsString> {
    frame_stream_arguments_delivering(request, request.delivered_size(), QUICK_SCALING_OF_THE_PREVIEW)
}

/// Like [`frame_stream_arguments`], for pictures delivered `delivered_size` pixels big and scaled with `scaling`.
pub fn frame_stream_arguments_delivering(
    request: &FrameStreamRequest,
    delivered_size: (u32, u32),
    scaling: &str,
) -> Vec<OsString> {
    let timing = StreamTiming::of(request);
    let first_frame_moved_to_time_zero =
        timing.first_frame_after_the_start as i64 - BIAS_TOWARDS_THE_LATER_FRAME_IN_MICROSECONDS;
    let project_frame_rate = request.project_frame_rate;

    let mut filters = format!(
        "settb=AVTB,setpts=PTS-({first_frame_moved_to_time_zero}),fps={project_frame_rate}:round=near:start_time=0"
    );
    if request.speed_factor.is_accelerated() {
        filters.push_str(&format!(",select='not(mod(n,{}))'", request.speed_factor.value()));
    }
    let (delivered_width, delivered_height) = delivered_size;
    let delivered_format = match request.delivery {
        FrameDelivery::Rgba => "rgba",
        FrameDelivery::Yuv420p => "yuv420p",
    };
    let told_how_to_read_colours = match request.delivery {
        FrameDelivery::Rgba => request
            .ffmpeg_reads_planes_as
            .map(HowToReadColours::as_scale_options)
            .unwrap_or_default(),
        FrameDelivery::Yuv420p => String::new(),
    };
    filters.push_str(&format!(
        ",setpts=N*{}/({}*TB),scale={delivered_width}:{delivered_height}{told_how_to_read_colours}:flags={scaling},format={delivered_format}",
        project_frame_rate.denominator(),
        project_frame_rate.numerator(),
    ));

    let mut arguments: Vec<OsString> = Vec::new();
    arguments.extend(["-hide_banner", "-loglevel", "error", "-nostdin", "-ss"].map(OsString::from));
    arguments.push(microseconds_as_seconds(timing.start_in_file).into());
    arguments.push("-t".into());
    arguments.push(microseconds_as_seconds(timing.duration_read).into());
    arguments.push("-i".into());
    arguments.push(request.video.ffmpeg_input());
    arguments.extend(["-an", "-sn", "-dn", "-vf"].map(OsString::from));
    arguments.push(filters.into());
    let pictures_wanted = request
        .source_frame_count
        .div_ceil(u64::from(request.speed_factor.value()));
    if pictures_wanted > 0 {
        arguments.extend(["-frames:v".into(), pictures_wanted.to_string().into()]);
    }
    arguments.extend(["-f", "rawvideo", "pipe:1"].map(OsString::from));
    arguments
}

#[cfg(test)]
mod tests;
