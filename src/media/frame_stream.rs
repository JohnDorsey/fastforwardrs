//! Decoding the frames of the live preview.
//!
//! The preview must show exactly the frames the export will contain. So a stream is told, in the
//! editor's own terms, which source frame to start at, how many source frames to read and at what
//! speed, and it runs the very same trimming, retiming and frame-rate steps that the export uses
//! (see [`crate::media::export`]). Frame number `k` that comes out of a stream is the clip's frame
//! number `k`.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;

use thiserror::Error;

use super::UserInterfaceWakeUp;
use super::child_process::{StreamingTool, background_command};
use super::ffmpeg_toolchain::FfmpegToolchain;
use crate::project::{FrameRate, SourceFrameIndex, SpeedFactor, fit_inside_picture};

/// How many decoded pictures may wait to be shown before decoding pauses. Small, because each
/// picture is several megabytes and the decoder is much faster than playback.
const FRAMES_BUFFERED_AHEAD: usize = 6;

const BYTES_PER_PIXEL: usize = 4;

/// What to decode.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameStreamRequest {
    pub video_path: PathBuf,
    pub frame_rate: FrameRate,
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

/// What a running stream reports.
#[derive(Debug)]
pub enum FrameStreamEvent {
    /// The next frame of the clip. Frames arrive in order, starting from the clip's frame 0.
    Frame(DecodedFrame),
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
        if request.picture_width_in_pixels == 0 || request.picture_height_in_pixels == 0 {
            return Err(FrameStreamError::EmptyPicture);
        }
        let mut command = background_command(toolchain.ffmpeg_path());
        command.args(frame_stream_arguments(request));
        let (delivered_width, delivered_height) = request.delivered_size();
        let frame_size = FrameSize {
            width_in_pixels: delivered_width,
            height_in_pixels: delivered_height,
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
}

#[derive(Clone, Copy)]
struct FrameSize {
    width_in_pixels: u32,
    height_in_pixels: u32,
}

impl FrameSize {
    fn byte_count(self) -> usize {
        self.width_in_pixels as usize * self.height_in_pixels as usize * BYTES_PER_PIXEL
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
        let mut rgba_pixels = vec![0u8; frame_size.byte_count()];
        if standard_output.read_exact(&mut rgba_pixels).is_err() {
            break;
        }
        let frame = DecodedFrame {
            width_in_pixels: frame_size.width_in_pixels,
            height_in_pixels: frame_size.height_in_pixels,
            rgba_pixels,
        };
        // A full buffer makes this wait, which is what keeps the decoder from racing ahead. An
        // error means the stream was dropped, so nobody wants more frames.
        if sender.send(FrameStreamEvent::Frame(frame)).is_err() {
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

/// The ffmpeg command line that decodes `request`.
///
/// The pictures are chosen the way the export chooses them (see `crate::media::export`): resample
/// to the project's constant frame rate with `round=near` so that there is one picture for every
/// frame, whatever the file's timestamps look like, then keep every `speed_factor`th picture of
/// the clip, starting with its first.
///
/// To start at the clip's first picture the file is read from half a frame before it, and to stop
/// after its last the reading ends half a frame before the picture that follows. That half frame
/// of slack means that rounding in the file's timestamps can never cut a wanted picture off or
/// let an unwanted one in. The timestamps are then moved back by exactly that half frame, so
/// that every picture sits right on a frame slot, which is where resampling to the nearest slot
/// is most dependable.
pub fn frame_stream_arguments(request: &FrameStreamRequest) -> Vec<OsString> {
    let frame_rate = request.frame_rate;
    let first_picture_seconds = frame_rate.seconds_at_frame(request.first_source_frame.value());
    let seconds_to_start = (first_picture_seconds - half_a_frame_in_seconds(frame_rate)).max(0.0);
    let seconds_to_stop = frame_rate.seconds_at_frame(request.first_source_frame.value() + request.source_frame_count)
        - half_a_frame_in_seconds(frame_rate);
    let seconds_to_read = (seconds_to_stop - seconds_to_start).max(0.0);
    // After seeking, the first wanted picture is this far from time zero.
    let seconds_the_first_picture_is_late = first_picture_seconds - seconds_to_start;

    let mut filters = format!("setpts=PTS-{seconds_the_first_picture_is_late:.9}/TB,fps={frame_rate}:round=near");
    if request.speed_factor.is_accelerated() {
        filters.push_str(&format!(",select='not(mod(n,{}))'", request.speed_factor.value()));
    }
    let (delivered_width, delivered_height) = request.delivered_size();
    filters.push_str(&format!(
        ",setpts=N*{}/({}*TB),scale={delivered_width}:{delivered_height}:flags=fast_bilinear,format=rgba",
        frame_rate.denominator(),
        frame_rate.numerator(),
    ));

    let mut arguments: Vec<OsString> = Vec::new();
    arguments.extend(["-hide_banner", "-loglevel", "error", "-nostdin", "-ss"].map(OsString::from));
    arguments.push(format!("{seconds_to_start:.6}").into());
    arguments.push("-t".into());
    arguments.push(format!("{seconds_to_read:.6}").into());
    arguments.push("-i".into());
    arguments.push(request.video_path.clone().into());
    arguments.extend(["-an", "-sn", "-dn", "-vf"].map(OsString::from));
    arguments.push(filters.into());
    arguments.extend(["-f", "rawvideo", "pipe:1"].map(OsString::from));
    arguments
}

fn half_a_frame_in_seconds(frame_rate: FrameRate) -> f64 {
    0.5 / frame_rate.frames_per_second()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn request(first_source_frame: u64, source_frame_count: u64, speed_factor: u32) -> FrameStreamRequest {
        FrameStreamRequest {
            video_path: PathBuf::from("holiday.mp4"),
            frame_rate: FrameRate::new(30, 1).unwrap(),
            first_source_frame: SourceFrameIndex::new(first_source_frame),
            source_frame_count,
            speed_factor: SpeedFactor::new(speed_factor).unwrap(),
            video_display_size_in_pixels: (1280, 720),
            picture_width_in_pixels: 640,
            picture_height_in_pixels: 360,
        }
    }

    fn arguments_as_text(request: &FrameStreamRequest) -> Vec<String> {
        frame_stream_arguments(request)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    fn filters_of(request: &FrameStreamRequest) -> String {
        let arguments = arguments_as_text(request);
        arguments[arguments.iter().position(|argument| argument == "-vf").unwrap() + 1].clone()
    }

    #[test]
    fn the_command_line_seeks_half_a_frame_early_and_reads_exactly_the_clip() {
        // Frames 300 up to (not including) 600 at 30 fps: start at 9.983333 s (half a frame
        // before frame 300 at 10 s), read until half a frame before frame 600 at 20 s. The first
        // picture is then half a frame (0.016666667 s) after the start of what is read.
        let arguments = arguments_as_text(&request(300, 300, 1));
        assert_eq!(
            arguments,
            [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-ss",
                "9.983333",
                "-t",
                "10.000000",
                "-i",
                "holiday.mp4",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                "setpts=PTS-0.016666667/TB,fps=30/1:round=near,setpts=N*1/(30*TB),scale=640:360:flags=fast_bilinear,format=rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ]
        );
    }

    #[test]
    fn a_clip_at_the_very_start_cannot_seek_before_zero_and_stops_half_a_frame_before_its_end() {
        let arguments = arguments_as_text(&request(0, 90, 1));
        assert_eq!(arguments[5], "0.000000");
        // Frames 0..90 end half a frame before 3 s.
        assert_eq!(arguments[7], "2.983333");
        // Reading starts at the very first picture, so there is no half frame to take back.
        assert!(
            filters_of(&request(0, 90, 1)).starts_with("setpts=PTS-0.000000000/TB,"),
            "{}",
            filters_of(&request(0, 90, 1))
        );
    }

    #[test]
    fn an_accelerated_clip_keeps_every_nth_picture_after_resampling_and_a_normal_one_keeps_all() {
        let accelerated = filters_of(&request(300, 300, 8));
        assert!(
            accelerated.contains("fps=30/1:round=near,select='not(mod(n,8))',setpts=N*1/(30*TB),"),
            "{accelerated}"
        );
        assert!(!filters_of(&request(300, 300, 1)).contains("select"));
    }

    #[test]
    fn resampling_uses_the_nearest_slot_never_round_up() {
        for speed in [1, 4] {
            let filters = filters_of(&request(300, 300, speed));
            assert!(
                filters.contains("round=near") && !filters.contains("round=up"),
                "{filters}"
            );
        }
    }

    #[test]
    fn fractional_frame_rates_are_passed_as_exact_fractions() {
        let mut request = request(0, 100, 1);
        request.frame_rate = FrameRate::new(30000, 1001).unwrap();
        let filters = filters_of(&request);
        assert!(filters.contains("fps=30000/1001:round=near"), "{filters}");
        assert!(filters.contains("setpts=N*1001/(30000*TB)"), "{filters}");
    }

    #[test]
    fn a_video_of_the_shape_of_the_picture_is_scaled_straight_to_it() {
        let filters = filters_of(&request(0, 100, 1));
        assert!(
            filters.ends_with(",scale=640:360:flags=fast_bilinear,format=rgba"),
            "{filters}"
        );
        assert!(!filters.contains("pad"), "{filters}");
    }

    #[test]
    fn a_video_of_another_shape_is_delivered_as_big_as_fits_inside_the_picture_in_its_own_shape_with_no_black_round_it()
    {
        let mut upright = request(0, 100, 1);
        upright.video_display_size_in_pixels = (720, 1280);
        // 9:16 in 640x360: as tall as the picture, 202.5 wide, which is made 202 (even).
        let filters = filters_of(&upright);
        assert!(
            filters.ends_with(",scale=202:360:flags=fast_bilinear,format=rgba"),
            "{filters}"
        );
        assert!(
            !filters.contains("pad"),
            "where it goes on the picture is for the drawing to say"
        );
        assert_eq!(upright.delivered_size(), (202, 360));
    }

    #[test]
    fn what_is_delivered_is_the_fit_of_the_video_in_the_picture_whatever_the_shapes() {
        for video in [(1280, 720), (720, 1280), (1000, 1000), (3840, 1080), (320, 180)] {
            for picture in [(640, 360), (360, 640), (500, 500), (64, 36)] {
                let mut request = request(0, 100, 1);
                request.video_display_size_in_pixels = video;
                request.picture_width_in_pixels = picture.0;
                request.picture_height_in_pixels = picture.1;
                let (width, height) = request.delivered_size();
                let fit = fit_inside_picture(video.0, video.1, picture.0, picture.1);
                assert_eq!(
                    (width, height),
                    (fit.width_in_pixels, fit.height_in_pixels),
                    "{video:?} in {picture:?}"
                );
                assert!(
                    width <= picture.0 && height <= picture.1,
                    "{video:?} in {picture:?}: {width}x{height}"
                );
                assert!(
                    filters_of(&request).contains(&format!("scale={width}:{height}:")),
                    "{video:?} in {picture:?}"
                );
            }
        }
    }

    use crate::media::do_nothing_when_woken;

    #[test]
    fn a_zero_sized_picture_is_refused_before_ffmpeg_is_started() {
        let toolchain = FfmpegToolchain::unchecked_for_tests(
            PathBuf::from("no-such-ffmpeg"),
            PathBuf::from("no-such-ffprobe"),
            Some(9),
        );
        for (width, height) in [(0, 360), (640, 0), (0, 0)] {
            let mut request = request(0, 10, 1);
            request.picture_width_in_pixels = width;
            request.picture_height_in_pixels = height;
            let result = FrameStream::start(&toolchain, &request, do_nothing_when_woken());
            assert!(
                matches!(result, Err(FrameStreamError::EmptyPicture)),
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn a_missing_ffmpeg_is_reported_as_not_startable() {
        let toolchain = FfmpegToolchain::unchecked_for_tests(
            PathBuf::from("no-such-ffmpeg"),
            PathBuf::from("no-such-ffprobe"),
            Some(9),
        );
        let result = FrameStream::start(&toolchain, &request(0, 10, 1), do_nothing_when_woken());
        assert!(matches!(result, Err(FrameStreamError::CouldNotStart(_))));
    }

    #[test]
    fn frames_are_read_whole_in_order_and_the_end_is_reported() {
        let frame_size = FrameSize {
            width_in_pixels: 2,
            height_in_pixels: 1,
        };
        let bytes: Vec<u8> = (0..16).collect(); // two 8-byte frames
        let (sender, receiver) = mpsc::sync_channel(10);
        read_frames(&bytes[..], frame_size, &sender, &do_nothing_when_woken(), || {
            "decoder said no".to_owned()
        });
        let events: Vec<FrameStreamEvent> = receiver.try_iter().collect();
        assert_eq!(events.len(), 3);
        match (&events[0], &events[1], &events[2]) {
            (
                FrameStreamEvent::Frame(first),
                FrameStreamEvent::Frame(second),
                FrameStreamEvent::Finished { error_output },
            ) => {
                assert_eq!(first.rgba_pixels, (0..8).collect::<Vec<u8>>());
                assert_eq!(second.rgba_pixels, (8..16).collect::<Vec<u8>>());
                assert_eq!(error_output.as_deref(), Some("decoder said no"));
            }
            other => panic!("unexpected events {other:?}"),
        }
    }

    #[test]
    fn a_partial_frame_at_the_end_of_the_data_is_dropped() {
        let frame_size = FrameSize {
            width_in_pixels: 2,
            height_in_pixels: 1,
        };
        let bytes: Vec<u8> = (0..12).collect(); // one whole frame and half of another
        let (sender, receiver) = mpsc::sync_channel(10);
        read_frames(&bytes[..], frame_size, &sender, &do_nothing_when_woken(), String::new);
        let events: Vec<FrameStreamEvent> = receiver.try_iter().collect();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[1], FrameStreamEvent::Finished { error_output: None }));
    }

    #[test]
    fn the_reader_stops_quietly_when_nobody_is_listening_any_more() {
        let frame_size = FrameSize {
            width_in_pixels: 2,
            height_in_pixels: 1,
        };
        let bytes: Vec<u8> = (0..64).collect();
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        // Must return rather than block or panic.
        read_frames(&bytes[..], frame_size, &sender, &do_nothing_when_woken(), String::new);
    }
}
