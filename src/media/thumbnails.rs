// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The small pictures that make up the video track: one frame for every second of the source
//! video, made once in the background and kept in memory as compressed JPEGs, and finer ones (two
//! or four or more for every second) made later for the stretch of the video that is on screen
//! when the timeline is zoomed in or a track is short, so that the pictures on a clip are never
//! stretched or cropped to make up for their number.
//!
//! Keeping them compressed is what lets a long video fit: an hour of thumbnails is a few tens of
//! megabytes as JPEG, but hundreds of megabytes as raw pixels. Only the handful of thumbnails that
//! are actually on screen are ever decompressed.

use std::io::{self, Read};
use std::sync::mpsc::SyncSender;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageFormat};
use thiserror::Error;

use super::UserInterfaceWakeUp;
use super::child_process::{StreamingTool, background_command};
use super::ffmpeg_toolchain::FfmpegToolchain;
use super::frame_stream::DecodedFrame;
use super::location::MediaLocation;
use crate::project::{FrameRate, SourceFrameIndex};

/// The most pictures that one run of ffmpeg is asked for when finer ones are made for the part of the
/// video that is on screen.
pub const MOST_FINER_THUMBNAILS_AT_ONCE: usize = 48;

/// How much earlier than its first moment a range of pictures is started from, in seconds. The
/// frame that is on screen at a moment began before it, and ffmpeg throws away what comes before
/// the place it was asked to start at, so without this the first picture would be of the next frame.
const LEAD_IN_IN_SECONDS: f64 = 0.25;

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

    /// A compressed picture that cannot be decompressed, for the tests of what is done with a thumbnail that cannot be unpacked.
    #[cfg(test)]
    pub fn that_cannot_be_unpacked(size: PictureSize) -> Self {
        Self {
            jpeg_bytes: vec![1, 2, 3],
            size,
        }
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

/// Which picture of a video: the `index`th of those that are made `2^level` for every second of it.
/// Level 0 is the picture of every whole second, so index 3 of level 0 is at 3 seconds; index 3 of
/// level 2 (four to a second) is at three quarters of a second.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThumbnailKey {
    pub level: u8,
    pub index: usize,
}

impl ThumbnailKey {
    /// How many pictures of level `level` are made for each second of the video.
    pub fn pictures_per_second(level: u8) -> f64 {
        f64::from(1u32 << u32::from(level.min(30)))
    }

    /// The picture of level `level` whose moment is nearest `seconds` into the video.
    pub fn nearest(level: u8, seconds: f64) -> Self {
        Self {
            level,
            index: (seconds.max(0.0) * Self::pictures_per_second(level)).round() as usize,
        }
    }

    /// The moment of the video this picture stands for, in seconds.
    pub fn seconds(self) -> f64 {
        self.index as f64 / Self::pictures_per_second(self.level)
    }
}

/// The finest level of pictures that is any use at `frame_rate`: the one with at most one picture
/// for every frame, so that no two of them are of the same frame.
pub fn finest_thumbnail_level(frame_rate: FrameRate) -> u8 {
    frame_rate.frames_per_second().max(1.0).log2().floor() as u8
}

/// The coarsest level whose pictures are at least as close together as `seconds_between_pictures`
/// (never finer than [`finest_thumbnail_level`]). Pictures that are shown this far apart on the
/// timeline need pictures of this level to be told apart.
pub fn thumbnail_level_for_spacing(seconds_between_pictures: f64, frame_rate: FrameRate) -> u8 {
    let finest = finest_thumbnail_level(frame_rate);
    if seconds_between_pictures.is_nan() || seconds_between_pictures <= 0.0 {
        return finest;
    }
    let level = (-seconds_between_pictures.log2())
        .ceil()
        .max(0.0)
        .min(f64::from(finest));
    level as u8
}

/// Which pictures of a video to make: `count` of those of level `level` from the `first_index`th on,
/// or all of them from there to the end of the video if `count` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbnailRange {
    pub level: u8,
    pub first_index: usize,
    pub count: Option<usize>,
}

impl ThumbnailRange {
    /// One picture for every second of the whole video.
    pub const EVERY_SECOND: Self = Self {
        level: 0,
        first_index: 0,
        count: None,
    };

    /// How many pictures before the first one ffmpeg is asked to make as well, and that are thrown
    /// away, so that the first one is right (see [`LEAD_IN_IN_SECONDS`]).
    fn lead_in(self) -> usize {
        let pictures = (LEAD_IN_IN_SECONDS * ThumbnailKey::pictures_per_second(self.level)).ceil() as usize;
        pictures.min(self.first_index)
    }

    /// The keys of the pictures in the range, if it has a count.
    pub fn keys(self) -> impl Iterator<Item = ThumbnailKey> {
        (self.first_index..self.first_index.saturating_add(self.count.unwrap_or(0))).map(move |index| ThumbnailKey {
            level: self.level,
            index,
        })
    }
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
    /// The thumbnail with this number among those of its level, which for the pictures of every
    /// second is the second of the source video it is at.
    Thumbnail { index: usize, picture: CompressedPicture },
    /// Extraction has ended. `error_output` holds ffmpeg's complaint if it ended in an error.
    Finished { error_output: Option<String> },
}

/// A running ffmpeg process that makes thumbnails of a video, in the background: one for every
/// second of it, or finer ones for a part. Dropping it stops ffmpeg.
pub struct ThumbnailExtraction {
    tool: StreamingTool<ThumbnailEvent>,
}

impl ThumbnailExtraction {
    /// Starts making a thumbnail of `size` for every second of the video at `video`. The
    /// `wake_up` callback is called each time something new is waiting.
    pub fn start(
        toolchain: &FfmpegToolchain,
        video: impl Into<MediaLocation>,
        size: PictureSize,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, ThumbnailError> {
        Self::start_range(toolchain, video, size, ThumbnailRange::EVERY_SECOND, wake_up)
    }

    /// Starts making the thumbnails of `range` (see [`ThumbnailRange`]).
    pub fn start_range(
        toolchain: &FfmpegToolchain,
        video: impl Into<MediaLocation>,
        size: PictureSize,
        range: ThumbnailRange,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, ThumbnailError> {
        if size.width_in_pixels == 0 || size.height_in_pixels == 0 {
            return Err(ThumbnailError::EmptyPicture);
        }
        let lead_in = range.lead_in();
        let mut command = background_command(toolchain.ffmpeg_path());
        command.args(thumbnail_range_arguments(video, size, range));
        let tool = StreamingTool::start(
            command,
            THUMBNAILS_BUFFERED_AHEAD,
            move |standard_output, sender, error_output_when_finished| {
                read_thumbnails(
                    standard_output,
                    size,
                    (range.first_index - lead_in, lead_in),
                    &sender,
                    &wake_up,
                    error_output_when_finished,
                );
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

/// Reads the pictures ffmpeg makes and sends them on. The first picture is number `first_index`, and
/// the first `skipped` of them are not sent.
fn read_thumbnails(
    mut standard_output: impl Read,
    size: PictureSize,
    (first_index, skipped): (usize, usize),
    sender: &SyncSender<ThumbnailEvent>,
    wake_up: &UserInterfaceWakeUp,
    error_output_when_finished: impl FnOnce() -> String,
) {
    let frame_byte_count = size.width_in_pixels as usize * size.height_in_pixels as usize * RGB_BYTES_PER_PIXEL;
    let mut rgb_pixels = vec![0u8; frame_byte_count];
    let mut compression_failure: Option<String> = None;
    for index in first_index.. {
        if standard_output.read_exact(&mut rgb_pixels).is_err() {
            break;
        }
        if index - first_index < skipped {
            continue;
        }
        match CompressedPicture::from_rgb_pixels(&rgb_pixels, size) {
            Ok(picture) => {
                if sender.send(ThumbnailEvent::Thumbnail { index, picture }).is_err() {
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
pub fn thumbnail_arguments(video: impl Into<MediaLocation>, size: PictureSize) -> Vec<std::ffi::OsString> {
    thumbnail_range_arguments(video, size, ThumbnailRange::EVERY_SECOND)
}

/// The ffmpeg command line that makes the thumbnails of `range`.
///
/// `fps=...:round=up` is what makes each thumbnail the frame that is on screen at exactly its
/// moment (the default rounding picks a frame from the middle of the interval instead). A range
/// that does not start at the start of the video begins by seeking to a little before its first
/// moment: ffmpeg throws away what comes before the place it seeks to, and when that is in the
/// middle of a frame the first picture it makes is of the frame after the one on screen (measured,
/// with a duplicate of it in front). So the pictures from there to the first moment are made too, and
/// thrown away by the one that reads them.
pub fn thumbnail_range_arguments(
    video: impl Into<MediaLocation>,
    size: PictureSize,
    range: ThumbnailRange,
) -> Vec<std::ffi::OsString> {
    use std::ffi::OsString;
    let pictures_per_second = ThumbnailKey::pictures_per_second(range.level);
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .map(OsString::from)
        .to_vec();
    let lead_in = range.lead_in();
    let first_made = range.first_index - lead_in;
    if first_made > 0 {
        // (A whole number divided by a power of two is exact as a decimal number.)
        arguments.extend([
            OsString::from("-ss"),
            OsString::from((first_made as f64 / pictures_per_second).to_string()),
        ]);
    }
    arguments.push(OsString::from("-i"));
    arguments.push(video.into().ffmpeg_input());
    arguments.extend(["-an", "-sn", "-dn", "-vf"].map(OsString::from));
    arguments.push(
        format!(
            "fps={pictures_per_second}:round=up,scale={}:{}:flags=area,format=rgb24",
            size.width_in_pixels, size.height_in_pixels
        )
        .into(),
    );
    if let Some(count) = range.count {
        arguments.extend([
            OsString::from("-frames:v"),
            OsString::from((count + lead_in).to_string()),
        ]);
    }
    arguments.extend(["-f", "rawvideo", "pipe:1"].map(OsString::from));
    arguments
}

#[cfg(test)]
mod tests {
    use std::path::Path;

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

    // --- the finer pictures ---

    fn frame_rate(frames_per_second: u32) -> FrameRate {
        FrameRate::new(frames_per_second, 1).unwrap()
    }

    #[test]
    fn a_picture_is_at_its_index_divided_by_the_pictures_there_are_for_each_second() {
        for (level, index, seconds) in [(0, 3, 3.0), (1, 3, 1.5), (2, 3, 0.75), (3, 12, 1.5), (4, 0, 0.0)] {
            assert_eq!(ThumbnailKey { level, index }.seconds(), seconds, "{level} {index}");
        }
        assert_eq!(ThumbnailKey::pictures_per_second(0), 1.0);
        assert_eq!(ThumbnailKey::pictures_per_second(5), 32.0);
    }

    #[test]
    fn the_nearest_picture_to_a_moment_is_the_one_whose_moment_is_nearest() {
        assert_eq!(ThumbnailKey::nearest(0, 2.4), ThumbnailKey { level: 0, index: 2 });
        assert_eq!(
            ThumbnailKey::nearest(0, 2.5),
            ThumbnailKey { level: 0, index: 3 },
            "halves round up"
        );
        assert_eq!(ThumbnailKey::nearest(2, 2.6), ThumbnailKey { level: 2, index: 10 });
        assert_eq!(ThumbnailKey::nearest(2, 2.63), ThumbnailKey { level: 2, index: 11 });
        assert_eq!(
            ThumbnailKey::nearest(3, -4.0),
            ThumbnailKey { level: 3, index: 0 },
            "not before the start"
        );
    }

    #[test]
    fn the_finest_pictures_worth_making_are_not_more_than_one_for_every_frame() {
        for (frames_per_second, level) in [(1, 0), (2, 1), (24, 4), (25, 4), (30, 4), (50, 5), (60, 5), (120, 6)] {
            assert_eq!(
                finest_thumbnail_level(frame_rate(frames_per_second)),
                level,
                "{frames_per_second}"
            );
        }
        assert_eq!(finest_thumbnail_level(FrameRate::new(30000, 1001).unwrap()), 4);
    }

    #[test]
    fn pictures_are_as_fine_as_the_spacing_of_the_pictures_on_the_timeline_asks_for() {
        let level_for = |seconds: f64| thumbnail_level_for_spacing(seconds, frame_rate(30));
        assert_eq!(level_for(60.0), 0, "far apart: the pictures of every second do");
        assert_eq!(level_for(1.0), 0);
        assert_eq!(level_for(0.99), 1);
        assert_eq!(level_for(0.5), 1);
        assert_eq!(level_for(0.49), 2);
        assert_eq!(level_for(0.25), 2);
        assert_eq!(level_for(0.154), 3);
        assert_eq!(level_for(0.125), 3);
        assert_eq!(level_for(0.12), 4);
        assert_eq!(level_for(0.0001), 4, "never finer than one for every frame");
        assert_eq!(level_for(0.0), 4);
        assert_eq!(level_for(f64::NAN), 4);
        assert_eq!(thumbnail_level_for_spacing(0.0001, frame_rate(60)), 5);
    }

    #[test]
    fn a_range_is_made_from_a_little_before_its_first_moment_and_the_pictures_before_it_are_thrown_away() {
        let range = |level, first_index, count| ThumbnailRange {
            level,
            first_index,
            count,
        };
        assert_eq!(
            range(2, 10, Some(4)).lead_in(),
            1,
            "a quarter of a second is one picture of four a second"
        );
        assert_eq!(range(4, 40, Some(4)).lead_in(), 4, "and four of sixteen");
        assert_eq!(range(0, 7, None).lead_in(), 1);
        assert_eq!(
            range(3, 1, Some(2)).lead_in(),
            1,
            "no earlier than the start of the video"
        );
        assert_eq!(range(3, 0, Some(2)).lead_in(), 0);
        assert_eq!(
            range(2, 10, Some(4)).keys().collect::<Vec<_>>(),
            (10..14)
                .map(|index| ThumbnailKey { level: 2, index })
                .collect::<Vec<_>>()
        );
    }

    fn arguments_for(range: ThumbnailRange) -> Vec<String> {
        thumbnail_range_arguments(Path::new("holiday.mp4"), size(160, 90), range)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_command_line_of_a_range_in_the_middle_seeks_to_its_lead_in_and_asks_for_the_lead_in_as_well() {
        // Four a second, from the 10th, 4 of them: from 2.25 s (the 9th), 5 pictures.
        assert_eq!(
            arguments_for(ThumbnailRange {
                level: 2,
                first_index: 10,
                count: Some(4)
            }),
            [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-ss",
                "2.25",
                "-i",
                "holiday.mp4",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                "fps=4:round=up,scale=160:90:flags=area,format=rgb24",
                "-frames:v",
                "5",
                "-f",
                "rawvideo",
                "pipe:1",
            ]
        );
    }

    #[test]
    fn a_range_that_starts_at_the_start_has_no_seeking_and_no_padding() {
        let arguments = arguments_for(ThumbnailRange {
            level: 3,
            first_index: 0,
            count: Some(5),
        });
        assert!(!arguments.contains(&"-ss".to_owned()), "{arguments:?}");
        assert!(arguments.contains(&"fps=8:round=up,scale=160:90:flags=area,format=rgb24".to_owned()));
        assert_eq!(
            arguments[arguments.iter().position(|argument| argument == "-frames:v").unwrap() + 1],
            "5"
        );
    }

    #[test]
    fn a_range_without_a_count_goes_to_the_end_of_the_video() {
        let arguments = arguments_for(ThumbnailRange {
            level: 0,
            first_index: 5,
            count: None,
        });
        assert!(!arguments.contains(&"-frames:v".to_owned()), "{arguments:?}");
        assert!(
            arguments.contains(&"-ss".to_owned()) && arguments.contains(&"4".to_owned()),
            "{arguments:?}"
        );
    }

    #[test]
    fn the_pictures_before_the_first_wanted_are_read_and_not_sent() {
        let size = size(2, 1);
        let bytes: Vec<u8> = (0..24).collect(); // four 6-byte pictures, the first two of which are not wanted
        let (sender, receiver) = std::sync::mpsc::sync_channel(16);
        read_thumbnails(
            &bytes[..],
            size,
            (8, 2),
            &sender,
            &crate::media::do_nothing_when_woken(),
            String::new,
        );
        let indexes: Vec<usize> = receiver
            .try_iter()
            .filter_map(|event| match event {
                ThumbnailEvent::Thumbnail { index, .. } => Some(index),
                ThumbnailEvent::Finished { .. } => None,
            })
            .collect();
        assert_eq!(
            indexes,
            vec![10, 11],
            "the pictures are numbered from the first one made"
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
            (0, 0),
            &sender,
            &crate::media::do_nothing_when_woken(),
            String::new,
        );
        let events: Vec<ThumbnailEvent> = receiver.try_iter().collect();
        assert_eq!(events.len(), 4);
        for (expected_second, event) in events[..3].iter().enumerate() {
            match event {
                ThumbnailEvent::Thumbnail { index, .. } => assert_eq!(*index, expected_second),
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(matches!(&events[3], ThumbnailEvent::Finished { error_output: None }));
    }
}
