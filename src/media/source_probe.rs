// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asking ffprobe what a file contains: a video, a sound, or a picture.

use std::io;

use serde::Deserialize;
use thiserror::Error;

use super::child_process::{background_command, end_of_error_output};
use super::ffmpeg_toolchain::FfmpegToolchain;
use super::location::MediaLocation;
use crate::colour_space::{ChromaSiting, VideoMatrix};
use crate::project::{
    FrameRate, InputColourOverride, SourceAudioProperties, SourceImageProperties, SourceMediaProperties,
    SourceVideoProperties,
};

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("ffprobe could not be started: {0}")]
    CouldNotRunFfprobe(#[source] io::Error),
    #[error("{location} could not be opened as a video: {message}")]
    CouldNotOpen { location: MediaLocation, message: String },
    #[error("ffprobe's answer could not be understood: {0}")]
    UnreadableAnswer(String),
    #[error("the file contains no video picture")]
    NoVideoStream,
    #[error("the file contains neither a video picture nor any sound")]
    NoVideoPictureOrSound,
    #[error("the video's frame rate could not be determined")]
    UnknownFrameRate,
    #[error("the video's length could not be determined")]
    UnknownLength,
}

/// Opens the video at `video` with ffprobe and reports what the project needs to know about it.
pub fn probe_video(
    toolchain: &FfmpegToolchain,
    video: impl Into<MediaLocation>,
) -> Result<SourceVideoProperties, ProbeError> {
    let video = video.into();
    let probe_output_json = run_ffprobe(toolchain, &video)?;
    properties_from_probe_output(&probe_output_json, |video_stream_index| {
        count_video_packets(toolchain, &video, video_stream_index)
    })
}

/// Opens the media file at `location` with ffprobe and reports what the project needs to know about it, whether it is a
/// picture (a still image), a video (a file with moving pictures) or a sound file (a file with only sound).
pub fn probe_source_media(
    toolchain: &FfmpegToolchain,
    location: impl Into<MediaLocation>,
) -> Result<SourceMediaProperties, ProbeError> {
    let location = location.into();
    let probe_output_json = run_ffprobe(toolchain, &location)?;
    source_properties_from_probe_output(&probe_output_json, |video_stream_index| {
        count_video_packets(toolchain, &location, video_stream_index)
    })
}

/// Opens the video at `video` with ffprobe and reports how its pictures are stored and what they say about their colours: what the picture engine
/// needs to know to make the pictures itself from the planes ffmpeg decodes, instead of asking ffmpeg to make them RGBA.
pub fn probe_video_stream_format(
    toolchain: &FfmpegToolchain,
    video: impl Into<MediaLocation>,
) -> Result<VideoStreamFormat, ProbeError> {
    let video = video.into();
    let probe_output_json = run_ffprobe(toolchain, &video)?;
    video_stream_format_from_probe_output(&probe_output_json)
}

/// What ffprobe's JSON says about how the pictures of the first real video stream are stored and what colours they stand for.
pub fn video_stream_format_from_probe_output(probe_output_json: &str) -> Result<VideoStreamFormat, ProbeError> {
    let probe_output = parse_probe_output(probe_output_json)?;
    let video_stream = probe_output
        .streams
        .iter()
        .find(|stream| stream.is_video() && !stream.is_cover_art())
        .ok_or(ProbeError::NoVideoStream)?;
    let stated = |text: &Option<String>| {
        text.as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty() && !matches!(*text, "unknown" | "unspecified" | "N/A"))
            .map(str::to_owned)
    };
    Ok(VideoStreamFormat {
        pixel_format: stated(&video_stream.pix_fmt),
        colour_space: stated(&video_stream.color_space),
        colour_range: stated(&video_stream.color_range),
        colour_primaries: stated(&video_stream.color_primaries),
        colour_transfer: stated(&video_stream.color_transfer),
        chroma_location: stated(&video_stream.chroma_location),
    })
}

/// How the pictures of a video are stored and what colours they say they stand for, as ffprobe prints it (the names it uses: `yuv420p`, `bt709`, `tv`,
/// and so on). What the file does not say is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct VideoStreamFormat {
    pub pixel_format: Option<String>,
    /// The matrix that turns brightness and colour differences into red, green and blue.
    pub colour_space: Option<String>,
    /// `tv` (limited: 16 to 235) or `pc` (full: 0 to 255).
    pub colour_range: Option<String>,
    pub colour_primaries: Option<String>,
    pub colour_transfer: Option<String>,
    /// Where the colour differences sit among the pixels (`left`, `center`, `topleft`...), for a video that stores one for every square of four pixels.
    pub chroma_location: Option<String>,
}

impl VideoStreamFormat {
    /// Where the colour differences of the pictures sit: what the file says, and the usual place where it does not.
    pub fn chroma_siting(&self) -> ChromaSiting {
        ChromaSiting::from_ffprobe(self.chroma_location.as_deref())
    }
}

/// A video whose pictures the preview can take as ffmpeg decodes them, in planes, and turn into colours itself: 8 bits to a number, 4:2:0, with a
/// matrix it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeYuv420p {
    pub matrix: VideoMatrix,
    pub full_range: bool,
}

impl VideoStreamFormat {
    /// Whether the pictures of the video, whose picture is `width` by `height` pixels, can be had as planes and turned into colours by the preview, and how. `None` for
    /// every video that is stored another way (10 bits to a number, 4:2:2 or 4:4:4, RGB, a pixel format that is only for full range, and so on) or that says it uses a
    /// matrix that is not known: those are decoded by ffmpeg into RGBA, as they always were.
    ///
    /// **Which matrix and which range is what [`VideoStreamFormat::colour_of_a_picture_of_size`] says**: what the file says, and where it says nothing the matrix that
    /// goes with the size of the picture (BT.709 for high definition, BT.601 for standard definition; it was BT.601 for every size until phase 3) and the limited range.
    pub fn as_native_yuv420p(&self, width: u32, height: u32) -> Option<NativeYuv420p> {
        self.as_native_yuv420p_with(width, height, &InputColourOverride::AUTOMATIC)
    }

    /// [`Self::as_native_yuv420p`] when the user has said some of the colours of the video (see [`Self::colour_of_a_picture_of_size_with`]). A matrix that the user
    /// chooses is one that the program has, and makes planes of a video whose own matrix it has not.
    pub fn as_native_yuv420p_with(&self, width: u32, height: u32, said: &InputColourOverride) -> Option<NativeYuv420p> {
        if self.pixel_format.as_deref() != Some("yuv420p") {
            return None;
        }
        let colour = self.colour_of_a_picture_of_size_with(width, height, said);
        Some(NativeYuv420p {
            matrix: colour.matrix?,
            full_range: colour.full_range?,
        })
    }
}

/// Runs ffprobe on `location` and returns the JSON it prints.
fn run_ffprobe(toolchain: &FfmpegToolchain, location: &MediaLocation) -> Result<String, ProbeError> {
    let output = background_command(toolchain.ffprobe_path())
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-i",
        ])
        .arg(location.ffmpeg_input())
        .output()
        .map_err(ProbeError::CouldNotRunFfprobe)?;
    if !output.status.success() {
        return Err(ProbeError::CouldNotOpen {
            location: location.clone(),
            message: end_of_error_output(&output.stderr),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Counts the packets (which is to say the frames) of one stream by reading through the file
/// without decoding it. It takes well under a tenth of a second even for a file of hundreds of
/// megabytes.
fn count_video_packets(
    toolchain: &FfmpegToolchain,
    video: &MediaLocation,
    stream_index: u32,
) -> Result<u64, ProbeError> {
    let output = background_command(toolchain.ffprobe_path())
        .args([
            "-v",
            "error",
            "-count_packets",
            "-show_entries",
            "stream=nb_read_packets",
            "-of",
            "csv=p=0",
            "-select_streams",
        ])
        .arg(stream_index.to_string())
        .arg("-i")
        .arg(video.ffmpeg_input())
        .output()
        .map_err(ProbeError::CouldNotRunFfprobe)?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_end_matches(',')
        .parse()
        .map_err(|_| ProbeError::UnknownLength)
}

/// Turns the JSON that `ffprobe -print_format json -show_format -show_streams` prints into the
/// properties of the first real video stream.
///
/// Most files state how many frames the video has. Some, such as Matroska files, do not, and
/// their duration is not a trustworthy way to work it out (it can be a frame too long). For those
/// `count_packets` is called, with the number of the video stream, to count the frames properly.
/// If counting fails the length is estimated from the duration instead.
pub fn properties_from_probe_output(
    probe_output_json: &str,
    count_packets: impl FnOnce(u32) -> Result<u64, ProbeError>,
) -> Result<SourceVideoProperties, ProbeError> {
    video_properties(&parse_probe_output(probe_output_json)?, count_packets)
}

/// Like [`properties_from_probe_output`], but a file that is a still image has the properties of a
/// picture, and a file that has no video picture is a sound file if it has sound, and its
/// properties are those of a sound.
pub fn source_properties_from_probe_output(
    probe_output_json: &str,
    count_packets: impl FnOnce(u32) -> Result<u64, ProbeError>,
) -> Result<SourceMediaProperties, ProbeError> {
    let probe_output = parse_probe_output(probe_output_json)?;
    if probe_output.is_a_still_image() {
        return image_properties(&probe_output).map(SourceMediaProperties::Image);
    }
    match video_properties(&probe_output, count_packets) {
        Ok(video) => Ok(SourceMediaProperties::Video(video)),
        Err(ProbeError::NoVideoStream) => audio_properties(&probe_output).map(SourceMediaProperties::Audio),
        Err(other) => Err(other),
    }
}

fn parse_probe_output(probe_output_json: &str) -> Result<ProbeOutput, ProbeError> {
    serde_json::from_str(probe_output_json).map_err(|error| ProbeError::UnreadableAnswer(error.to_string()))
}

/// The size a picture is shown at, from the first of its streams that is a picture.
fn image_properties(probe_output: &ProbeOutput) -> Result<SourceImageProperties, ProbeError> {
    let picture_stream = probe_output
        .streams
        .iter()
        .find(|stream| stream.is_video())
        .ok_or(ProbeError::NoVideoStream)?;
    let (display_width_in_pixels, display_height_in_pixels) = picture_stream.display_size()?;
    Ok(SourceImageProperties {
        display_width_in_pixels,
        display_height_in_pixels,
    })
}

/// How long a sound file lasts, from the first of its sound streams that says, or else from the
/// file as a whole.
fn audio_properties(probe_output: &ProbeOutput) -> Result<SourceAudioProperties, ProbeError> {
    let sound_streams: Vec<&ProbeStream> = probe_output.streams.iter().filter(|stream| stream.is_audio()).collect();
    if sound_streams.is_empty() {
        return Err(ProbeError::NoVideoPictureOrSound);
    }
    let duration_in_seconds = sound_streams
        .iter()
        .filter_map(|stream| stream.duration.as_deref())
        .chain(
            probe_output
                .format
                .as_ref()
                .and_then(|format| format.duration.as_deref()),
        )
        .find_map(|text| text.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .ok_or(ProbeError::UnknownLength)?;
    Ok(SourceAudioProperties {
        duration_in_microseconds: (duration_in_seconds * 1_000_000.0).round() as u64,
    })
}

fn video_properties(
    probe_output: &ProbeOutput,
    count_packets: impl FnOnce(u32) -> Result<u64, ProbeError>,
) -> Result<SourceVideoProperties, ProbeError> {
    let video_stream = probe_output
        .streams
        .iter()
        .find(|stream| stream.is_video() && !stream.is_cover_art())
        .ok_or(ProbeError::NoVideoStream)?;
    let has_audio = probe_output
        .streams
        .iter()
        .any(|stream| stream.codec_type.as_deref() == Some("audio"));

    let frame_rate = [&video_stream.avg_frame_rate, &video_stream.r_frame_rate]
        .into_iter()
        .flatten()
        .find_map(|text| text.parse::<FrameRate>().ok())
        .ok_or(ProbeError::UnknownFrameRate)?;

    let total_frame_count = total_frame_count(video_stream, probe_output.format.as_ref(), frame_rate, count_packets)?;
    let (display_width_in_pixels, display_height_in_pixels) = video_stream.display_size()?;

    Ok(SourceVideoProperties {
        frame_rate,
        total_frame_count,
        display_width_in_pixels,
        display_height_in_pixels,
        has_audio,
    })
}

fn total_frame_count(
    video_stream: &ProbeStream,
    format: Option<&ProbeFormat>,
    frame_rate: FrameRate,
    count_packets: impl FnOnce(u32) -> Result<u64, ProbeError>,
) -> Result<u64, ProbeError> {
    let stated_frames = video_stream
        .nb_frames
        .as_deref()
        .and_then(|text| text.parse::<u64>().ok())
        .filter(|&count| count > 0);
    if let Some(stated_frames) = stated_frames {
        return Ok(stated_frames);
    }
    if let Some(counted_frames) = video_stream
        .index
        .and_then(|index| count_packets(index).ok())
        .filter(|&count| count > 0)
    {
        return Ok(counted_frames);
    }
    let duration_in_seconds = [
        video_stream.duration.as_deref(),
        format.and_then(|format| format.duration.as_deref()),
    ]
    .into_iter()
    .flatten()
    .find_map(|text| text.parse::<f64>().ok())
    .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
    .ok_or(ProbeError::UnknownLength)?;
    Ok(frame_rate.nearest_frame_to_seconds(duration_in_seconds).max(1))
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

impl ProbeOutput {
    /// Whether the file is a still image (a PNG, a JPEG...) and not a video, which ffprobe says by
    /// the kind of file it reads it as: `image2` for a file it knows by its extension, and a name
    /// ending in `_pipe` (`png_pipe`, `webp_pipe`) for one it knows by what is in it. (A video
    /// file of a single picture is still a video, and so is an animated GIF, whose format is `gif`.)
    fn is_a_still_image(&self) -> bool {
        self.format
            .as_ref()
            .and_then(|format| format.format_name.as_deref())
            .is_some_and(|names| {
                names
                    .split(',')
                    .any(|name| name == "image2" || name == "image2pipe" || name.ends_with("_pipe"))
            })
    }
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    /// The number of the stream within the file, which is how ffprobe can be told to look at it.
    index: Option<u32>,
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_aspect_ratio: Option<String>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    duration: Option<String>,
    nb_frames: Option<String>,
    #[serde(default)]
    side_data_list: Vec<ProbeSideData>,
    #[serde(default)]
    tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    disposition: ProbeDisposition,
    pix_fmt: Option<String>,
    color_space: Option<String>,
    color_range: Option<String>,
    color_primaries: Option<String>,
    color_transfer: Option<String>,
    chroma_location: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeSideData {
    rotation: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct ProbeDisposition {
    attached_pic: Option<u8>,
}

impl ProbeStream {
    fn is_video(&self) -> bool {
        self.codec_type.as_deref() == Some("video")
    }

    fn is_audio(&self) -> bool {
        self.codec_type.as_deref() == Some("audio")
    }

    /// An album-art picture attached to an audio file shows up as a one-frame "video" stream.
    fn is_cover_art(&self) -> bool {
        self.disposition.attached_pic == Some(1)
    }

    /// How many degrees the picture must be turned to be shown upright, as 0, 90, 180 or 270.
    fn rotation_in_degrees(&self) -> i64 {
        let from_display_matrix = self.side_data_list.iter().find_map(|side_data| side_data.rotation);
        let from_old_style_tag = || self.tags.get("rotate").and_then(|text| text.trim().parse::<f64>().ok());
        let degrees = from_display_matrix.or_else(from_old_style_tag).unwrap_or(0.0);
        (degrees.round() as i64).rem_euclid(360)
    }

    /// The size of the picture as it is shown: the stored size, widened or narrowed for non-square
    /// pixels, and with width and height swapped if the picture is turned on its side.
    fn display_size(&self) -> Result<(u32, u32), ProbeError> {
        let (stored_width, stored_height) = match (self.width, self.height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => (width, height),
            _ => return Err(ProbeError::NoVideoStream),
        };
        let (pixel_width_part, pixel_height_part) = self.pixel_shape();
        let shown_width = ((f64::from(stored_width) * pixel_width_part / pixel_height_part).round() as u32).max(1);
        if matches!(self.rotation_in_degrees(), 90 | 270) {
            Ok((stored_height, shown_width))
        } else {
            Ok((shown_width, stored_height))
        }
    }

    /// The shape of one pixel as width and height parts, such as `(1.0, 1.0)` for square pixels.
    fn pixel_shape(&self) -> (f64, f64) {
        let parts = self.sample_aspect_ratio.as_deref().and_then(|text| {
            let (width_text, height_text) = text.split_once(':')?;
            let width: f64 = width_text.trim().parse().ok()?;
            let height: f64 = height_text.trim().parse().ok()?;
            (width > 0.0 && height > 0.0).then_some((width, height))
        });
        parts.unwrap_or((1.0, 1.0))
    }
}

#[cfg(test)]
mod tests;
