//! The small pictures that make up the video track: one frame for every second of the source
//! video, made once in the background and kept in memory as compressed JPEGs.
//!
//! Keeping them compressed is what lets a long video fit: an hour of thumbnails is a few tens of
//! megabytes as JPEG, but hundreds of megabytes as raw pixels. Only the handful of thumbnails that
//! are actually on screen are ever decompressed.

use std::io::{self, Read};
use std::path::Path;
use std::sync::mpsc::SyncSender;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageFormat};
use thiserror::Error;

use super::UserInterfaceWakeUp;
use super::child_process::{StreamingTool, background_command};
use super::ffmpeg_toolchain::FfmpegToolchain;
use super::frame_stream::DecodedFrame;
use crate::project::{FrameRate, SourceFrameIndex};

/// A good balance of size and quality for pictures this small.
const JPEG_QUALITY: u8 = 85;

const RGB_BYTES_PER_PIXEL: usize = 3;

/// How many thumbnails may wait to be collected before making them pauses. They are small, and
/// the window collects every one that is ready each time it is drawn.
const THUMBNAILS_BUFFERED_AHEAD: usize = 256;

/// The size of a picture in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureSize {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
}

impl PictureSize {
    /// The size of a thumbnail `target_height_in_pixels` tall, for a video whose picture is
    /// `display_width_in_pixels` by `display_height_in_pixels`. The shape of the video is kept.
    pub fn thumbnail_for(
        display_width_in_pixels: u32,
        display_height_in_pixels: u32,
        target_height_in_pixels: u32,
    ) -> Self {
        let display_height = display_height_in_pixels.max(1);
        let width = (f64::from(display_width_in_pixels) * f64::from(target_height_in_pixels)
            / f64::from(display_height))
        .round() as u32;
        Self {
            width_in_pixels: width.max(1),
            height_in_pixels: target_height_in_pixels.max(1),
        }
    }
}

#[derive(Debug, Error)]
pub enum ThumbnailError {
    #[error("ffmpeg could not be started to make the video track pictures: {0}")]
    CouldNotStart(#[source] io::Error),
    #[error("a thumbnail must be at least one pixel wide and tall")]
    EmptyPicture,
    #[error("a thumbnail could not be compressed: {0}")]
    CouldNotCompress(String),
    #[error("a thumbnail could not be decompressed: {0}")]
    CouldNotDecompress(String),
}

/// A thumbnail kept as a JPEG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompressedPicture {
    jpeg_bytes: Vec<u8>,
    size: PictureSize,
}

impl CompressedPicture {
    /// Compresses `rgb_pixels`: three bytes per pixel, row by row, top row first.
    pub fn from_rgb_pixels(rgb_pixels: &[u8], size: PictureSize) -> Result<Self, ThumbnailError> {
        // The JPEG encoder panics (instead of failing) on a buffer of the wrong length or size.
        let expected_byte_count = size.width_in_pixels as usize * size.height_in_pixels as usize * RGB_BYTES_PER_PIXEL;
        if expected_byte_count == 0 {
            return Err(ThumbnailError::EmptyPicture);
        }
        if rgb_pixels.len() != expected_byte_count {
            return Err(ThumbnailError::CouldNotCompress(format!(
                "a {}x{} picture needs {expected_byte_count} bytes of pixels, but {} were given",
                size.width_in_pixels,
                size.height_in_pixels,
                rgb_pixels.len()
            )));
        }
        let mut jpeg_bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg_bytes, JPEG_QUALITY)
            .encode(
                rgb_pixels,
                size.width_in_pixels,
                size.height_in_pixels,
                ExtendedColorType::Rgb8,
            )
            .map_err(|error| ThumbnailError::CouldNotCompress(error.to_string()))?;
        Ok(Self { jpeg_bytes, size })
    }

    pub fn size(&self) -> PictureSize {
        self.size
    }

    /// How much memory the compressed picture takes.
    pub fn byte_count(&self) -> usize {
        self.jpeg_bytes.len()
    }

    /// Decompresses the picture to RGBA pixels, ready to be shown.
    pub fn decompress(&self) -> Result<DecodedFrame, ThumbnailError> {
        let picture = image::load_from_memory_with_format(&self.jpeg_bytes, ImageFormat::Jpeg)
            .map_err(|error| ThumbnailError::CouldNotDecompress(error.to_string()))?
            .to_rgba8();
        Ok(DecodedFrame {
            width_in_pixels: picture.width(),
            height_in_pixels: picture.height(),
            rgba_pixels: picture.into_raw(),
        })
    }
}

/// Which second's thumbnail best stands for `source_frame`: the one whose moment is nearest.
pub fn thumbnail_second_for_source_frame(frame_rate: FrameRate, source_frame: SourceFrameIndex) -> usize {
    frame_rate.seconds_at_frame(source_frame.value()).round() as usize
}

/// All the thumbnails made so far for one source video, indexed by second.
#[derive(Clone, Debug, Default)]
pub struct ThumbnailStrip {
    pictures: Vec<Option<CompressedPicture>>,
}

impl ThumbnailStrip {
    /// A strip with room for `seconds_of_video` thumbnails, none of them made yet.
    pub fn with_room_for(seconds_of_video: usize) -> Self {
        Self {
            pictures: vec![None; seconds_of_video],
        }
    }

    /// Stores the thumbnail for `second`. A second beyond the end of the video is ignored.
    pub fn insert(&mut self, second: usize, picture: CompressedPicture) {
        if let Some(slot) = self.pictures.get_mut(second) {
            *slot = Some(picture);
        }
    }

    /// The thumbnail for `second`, if it has been made.
    pub fn picture_for_second(&self, second: usize) -> Option<&CompressedPicture> {
        self.pictures.get(second)?.as_ref()
    }

    /// How many seconds the strip has room for.
    pub fn second_count(&self) -> usize {
        self.pictures.len()
    }

    /// How many thumbnails have been made so far.
    pub fn made_count(&self) -> usize {
        self.pictures.iter().flatten().count()
    }
}

/// What a running thumbnail extraction reports.
#[derive(Debug)]
pub enum ThumbnailEvent {
    /// The thumbnail for the given whole second of the source video.
    Thumbnail { second: usize, picture: CompressedPicture },
    /// Extraction has ended. `error_output` holds ffmpeg's complaint if it ended in an error.
    Finished { error_output: Option<String> },
}

/// A running ffmpeg process that makes one thumbnail for every second of a video, in the
/// background. Dropping it stops ffmpeg.
pub struct ThumbnailExtraction {
    tool: StreamingTool<ThumbnailEvent>,
}

impl ThumbnailExtraction {
    /// Starts making thumbnails of `size` for the video at `video_path`. The `wake_up` callback is
    /// called each time something new is waiting.
    pub fn start(
        toolchain: &FfmpegToolchain,
        video_path: &Path,
        size: PictureSize,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, ThumbnailError> {
        if size.width_in_pixels == 0 || size.height_in_pixels == 0 {
            return Err(ThumbnailError::EmptyPicture);
        }
        let mut command = background_command(toolchain.ffmpeg_path());
        command.args(thumbnail_arguments(video_path, size));
        let tool = StreamingTool::start(
            command,
            THUMBNAILS_BUFFERED_AHEAD,
            move |standard_output, sender, error_output_when_finished| {
                read_thumbnails(standard_output, size, &sender, &wake_up, error_output_when_finished);
            },
        )
        .map_err(ThumbnailError::CouldNotStart)?;
        Ok(Self { tool })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<ThumbnailEvent> {
        self.tool.try_next_event()
    }
}

fn read_thumbnails(
    mut standard_output: impl Read,
    size: PictureSize,
    sender: &SyncSender<ThumbnailEvent>,
    wake_up: &UserInterfaceWakeUp,
    error_output_when_finished: impl FnOnce() -> String,
) {
    let frame_byte_count = size.width_in_pixels as usize * size.height_in_pixels as usize * RGB_BYTES_PER_PIXEL;
    let mut rgb_pixels = vec![0u8; frame_byte_count];
    let mut compression_failure: Option<String> = None;
    for second in 0.. {
        if standard_output.read_exact(&mut rgb_pixels).is_err() {
            break;
        }
        match CompressedPicture::from_rgb_pixels(&rgb_pixels, size) {
            Ok(picture) => {
                if sender.send(ThumbnailEvent::Thumbnail { second, picture }).is_err() {
                    return;
                }
                wake_up();
            }
            Err(error) => {
                compression_failure = Some(error.to_string());
                break;
            }
        }
    }
    let error_output = compression_failure.unwrap_or_else(error_output_when_finished);
    let _ = sender.send(ThumbnailEvent::Finished {
        error_output: (!error_output.is_empty()).then_some(error_output),
    });
    wake_up();
}

/// The ffmpeg command line that makes one thumbnail per second.
///
/// `fps=1:round=up` is what makes thumbnail `k` the frame that is on screen at exactly `k`
/// seconds (the default rounding picks a frame from the middle of the second instead).
pub fn thumbnail_arguments(video_path: &Path, size: PictureSize) -> Vec<std::ffi::OsString> {
    use std::ffi::OsString;
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-i"]
        .map(OsString::from)
        .to_vec();
    arguments.push(video_path.into());
    arguments.extend(["-an", "-sn", "-dn", "-vf"].map(OsString::from));
    arguments.push(
        format!(
            "fps=1:round=up,scale={}:{}:flags=area,format=rgb24",
            size.width_in_pixels, size.height_in_pixels
        )
        .into(),
    );
    arguments.extend(["-f", "rawvideo", "pipe:1"].map(OsString::from));
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: u32, height: u32) -> PictureSize {
        PictureSize {
            width_in_pixels: width,
            height_in_pixels: height,
        }
    }

    #[test]
    fn thumbnail_sizes_keep_the_shape_of_the_video() {
        assert_eq!(PictureSize::thumbnail_for(1920, 1080, 90), size(160, 90));
        assert_eq!(PictureSize::thumbnail_for(1080, 1920, 96), size(54, 96));
        assert_eq!(PictureSize::thumbnail_for(1024, 576, 90), size(160, 90));
        assert_eq!(PictureSize::thumbnail_for(100, 100, 50), size(50, 50));
    }

    #[test]
    fn thumbnail_sizes_are_never_empty() {
        assert_eq!(PictureSize::thumbnail_for(1, 1000, 10), size(1, 10));
        assert_eq!(PictureSize::thumbnail_for(0, 0, 0), size(1, 1));
    }

    #[test]
    fn a_source_frame_uses_the_thumbnail_of_the_nearest_second() {
        let frame_rate = FrameRate::new(30, 1).unwrap();
        let second_for = |frame: u64| thumbnail_second_for_source_frame(frame_rate, SourceFrameIndex::new(frame));
        assert_eq!(second_for(0), 0);
        assert_eq!(second_for(14), 0);
        assert_eq!(second_for(15), 1, "half a second rounds up");
        assert_eq!(second_for(29), 1);
        assert_eq!(second_for(30), 1);
        assert_eq!(second_for(44), 1);
        assert_eq!(second_for(45), 2);
        assert_eq!(second_for(30 * 3600), 3600);
    }

    #[test]
    fn nearest_second_follows_real_time_for_fractional_frame_rates() {
        let frame_rate = FrameRate::new(30000, 1001).unwrap();
        // Frame 30000 is at exactly 1001 s.
        assert_eq!(
            thumbnail_second_for_source_frame(frame_rate, SourceFrameIndex::new(30000)),
            1001
        );
    }

    fn solid_rgb(width: u32, height: u32, red: u8, green: u8, blue: u8) -> Vec<u8> {
        (0..width * height).flat_map(|_| [red, green, blue]).collect()
    }

    #[test]
    fn a_compressed_picture_decompresses_to_the_same_size_and_nearly_the_same_colours() {
        let picture = CompressedPicture::from_rgb_pixels(&solid_rgb(64, 36, 200, 40, 90), size(64, 36)).unwrap();
        assert_eq!(picture.size(), size(64, 36));
        assert!(
            picture.byte_count() < 64 * 36 * 3 / 4,
            "a flat picture must compress well, got {} bytes",
            picture.byte_count()
        );

        let decoded = picture.decompress().unwrap();
        assert_eq!((decoded.width_in_pixels, decoded.height_in_pixels), (64, 36));
        assert_eq!(decoded.rgba_pixels.len(), 64 * 36 * 4);
        let middle_pixel = &decoded.rgba_pixels[((18 * 64 + 32) * 4)..][..4];
        for (actual, expected) in middle_pixel.iter().zip([200u8, 40, 90, 255]) {
            assert!(
                actual.abs_diff(expected) <= 6,
                "pixel {middle_pixel:?} is too far from (200, 40, 90, 255)"
            );
        }
    }

    #[test]
    fn compressing_the_wrong_amount_of_pixel_data_is_an_error_not_a_panic() {
        let result = CompressedPicture::from_rgb_pixels(&[0u8; 10], size(64, 36));
        match result {
            Err(ThumbnailError::CouldNotCompress(reason)) => {
                assert!(reason.contains("6912") && reason.contains("10"), "{reason}");
            }
            other => panic!("expected CouldNotCompress, got {other:?}"),
        }
        // Too much data is just as wrong as too little.
        let result = CompressedPicture::from_rgb_pixels(&vec![0u8; 6913], size(64, 36));
        assert!(matches!(result, Err(ThumbnailError::CouldNotCompress(_))));
    }

    #[test]
    fn compressing_an_empty_picture_is_an_error_not_a_panic() {
        for empty_size in [size(0, 36), size(64, 0), size(0, 0)] {
            let result = CompressedPicture::from_rgb_pixels(&[], empty_size);
            assert!(matches!(result, Err(ThumbnailError::EmptyPicture)), "{empty_size:?}");
        }
    }

    #[test]
    fn a_strip_stores_and_returns_thumbnails_by_second() {
        let picture = CompressedPicture::from_rgb_pixels(&solid_rgb(8, 8, 1, 2, 3), size(8, 8)).unwrap();
        let mut strip = ThumbnailStrip::with_room_for(3);
        assert_eq!((strip.second_count(), strip.made_count()), (3, 0));
        assert_eq!(strip.picture_for_second(1), None);

        strip.insert(1, picture.clone());
        assert_eq!(strip.picture_for_second(1), Some(&picture));
        assert_eq!(strip.picture_for_second(0), None);
        assert_eq!(strip.made_count(), 1);
    }

    #[test]
    fn a_thumbnail_beyond_the_end_of_the_strip_is_ignored() {
        let picture = CompressedPicture::from_rgb_pixels(&solid_rgb(8, 8, 1, 2, 3), size(8, 8)).unwrap();
        let mut strip = ThumbnailStrip::with_room_for(2);
        strip.insert(2, picture.clone());
        strip.insert(1000, picture);
        assert_eq!(strip.made_count(), 0);
        assert_eq!(strip.picture_for_second(1000), None);
    }

    #[test]
    fn the_command_line_makes_one_exact_frame_per_second_at_the_requested_size() {
        let arguments: Vec<String> = thumbnail_arguments(Path::new("holiday.mp4"), size(160, 90))
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            arguments,
            [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-i",
                "holiday.mp4",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                "fps=1:round=up,scale=160:90:flags=area,format=rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ]
        );
    }

    #[test]
    fn whole_thumbnails_are_read_in_order_numbered_by_second() {
        let size = size(2, 1);
        let bytes: Vec<u8> = (0..18).collect(); // three 6-byte pictures
        let (sender, receiver) = std::sync::mpsc::sync_channel(16);
        read_thumbnails(
            &bytes[..],
            size,
            &sender,
            &crate::media::do_nothing_when_woken(),
            String::new,
        );
        let events: Vec<ThumbnailEvent> = receiver.try_iter().collect();
        assert_eq!(events.len(), 4);
        for (expected_second, event) in events[..3].iter().enumerate() {
            match event {
                ThumbnailEvent::Thumbnail { second, .. } => assert_eq!(*second, expected_second),
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(matches!(&events[3], ThumbnailEvent::Finished { error_output: None }));
    }
}
