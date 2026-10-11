//! Asking ffprobe what a file contains: a video, a sound, or a picture.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use super::child_process::{background_command, end_of_error_output};
use super::ffmpeg_toolchain::FfmpegToolchain;
use crate::project::{
    FrameRate, SourceAudioProperties, SourceImageProperties, SourceMediaProperties, SourceVideoProperties,
};

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("ffprobe could not be started: {0}")]
    CouldNotRunFfprobe(#[source] io::Error),
    #[error("{path} could not be opened as a video: {message}")]
    CouldNotOpen { path: PathBuf, message: String },
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

/// Opens `video_path` with ffprobe and reports what the project needs to know about it.
pub fn probe_video(toolchain: &FfmpegToolchain, video_path: &Path) -> Result<SourceVideoProperties, ProbeError> {
    let probe_output_json = run_ffprobe(toolchain, video_path)?;
    properties_from_probe_output(&probe_output_json, |video_stream_index| {
        count_video_packets(toolchain, video_path, video_stream_index)
    })
}

/// Opens `path` with ffprobe and reports what the project needs to know about it, whether it is a
/// picture (a still image), a video (a file with moving pictures) or a sound file (a file with only sound).
pub fn probe_source_media(toolchain: &FfmpegToolchain, path: &Path) -> Result<SourceMediaProperties, ProbeError> {
    let probe_output_json = run_ffprobe(toolchain, path)?;
    source_properties_from_probe_output(&probe_output_json, |video_stream_index| {
        count_video_packets(toolchain, path, video_stream_index)
    })
}

/// Runs ffprobe on `path` and returns the JSON it prints.
fn run_ffprobe(toolchain: &FfmpegToolchain, path: &Path) -> Result<String, ProbeError> {
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
        .arg(path)
        .output()
        .map_err(ProbeError::CouldNotRunFfprobe)?;
    if !output.status.success() {
        return Err(ProbeError::CouldNotOpen {
            path: path.to_owned(),
            message: end_of_error_output(&output.stderr),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Counts the packets (which is to say the frames) of one stream by reading through the file
/// without decoding it. It takes well under a tenth of a second even for a file of hundreds of
/// megabytes.
fn count_video_packets(toolchain: &FfmpegToolchain, video_path: &Path, stream_index: u32) -> Result<u64, ProbeError> {
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
        .arg(video_path)
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
mod tests {
    use super::*;

    /// Most tests below do not care about counting packets, so this stand-in (which shadows the
    /// real function) makes counting always fail: the length then comes from the file's own
    /// statement of its frame count, or from its duration.
    fn properties_from_probe_output(probe_output_json: &str) -> Result<SourceVideoProperties, ProbeError> {
        super::properties_from_probe_output(probe_output_json, |_| Err(ProbeError::UnknownLength))
    }

    fn frame_rate(numerator: u32, denominator: u32) -> FrameRate {
        FrameRate::new(numerator, denominator).unwrap()
    }

    /// What ffprobe prints for a 20 second, 30 fps Matroska file: no frame count, and a format
    /// duration that is a fraction of a frame too long (20.021 s would be 600.6 frames).
    const MATROSKA_WITHOUT_A_FRAME_COUNT: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "video", "width": 320, "height": 180,
              "avg_frame_rate": "30/1", "r_frame_rate": "30/1", "duration": "N/A", "nb_frames": "N/A" }
        ],
        "format": { "duration": "20.021000" }
    }"#;

    /// Regression test: the duration of a Matroska file made the video seem to have 601 frames
    /// when it has 600, so the timeline ended one frame after the real video did.
    #[test]
    fn a_file_that_does_not_state_its_frame_count_has_its_packets_counted() {
        let estimated_from_duration = properties_from_probe_output(MATROSKA_WITHOUT_A_FRAME_COUNT).unwrap();
        assert_eq!(
            estimated_from_duration.total_frame_count, 601,
            "the duration alone is misleading"
        );

        let counted = super::properties_from_probe_output(MATROSKA_WITHOUT_A_FRAME_COUNT, |_| Ok(600)).unwrap();
        assert_eq!(counted.total_frame_count, 600);
    }

    #[test]
    fn the_packets_of_the_video_stream_are_counted_not_those_of_another_stream() {
        let json = r#"{ "streams": [
            { "index": 0, "codec_type": "audio", "sample_rate": "48000" },
            { "index": 1, "codec_type": "video", "width": 320, "height": 180, "avg_frame_rate": "30/1" } ],
            "format": { "duration": "2.0" } }"#;
        let mut counted_stream = None;
        let properties = super::properties_from_probe_output(json, |stream_index| {
            counted_stream = Some(stream_index);
            Ok(60)
        })
        .unwrap();
        assert_eq!(counted_stream, Some(1));
        assert_eq!(properties.total_frame_count, 60);
    }

    #[test]
    fn a_stated_frame_count_is_trusted_and_nothing_is_counted() {
        let properties = super::properties_from_probe_output(ORDINARY_VIDEO_WITH_AUDIO, |_| {
            panic!("the packets must not be counted when the file states its frame count")
        })
        .unwrap();
        assert_eq!(properties.total_frame_count, 600);
    }

    #[test]
    fn a_failed_or_empty_count_falls_back_to_the_duration() {
        for count in [Err(ProbeError::UnknownLength), Ok(0)] {
            let properties = super::properties_from_probe_output(MATROSKA_WITHOUT_A_FRAME_COUNT, |_| count).unwrap();
            assert_eq!(properties.total_frame_count, 601);
        }
    }

    /// What ffprobe prints (trimmed to the fields we read) for a plain 320x180, 30 fps, 20 second
    /// video with a mono audio track, such as the test video the integration tests generate.
    const ORDINARY_VIDEO_WITH_AUDIO: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "video", "width": 320, "height": 180,
              "sample_aspect_ratio": "1:1", "r_frame_rate": "30/1", "avg_frame_rate": "30/1",
              "duration": "20.000000", "nb_frames": "600",
              "disposition": { "default": 1, "attached_pic": 0 } },
            { "index": 1, "codec_type": "audio", "sample_rate": "48000", "channels": 1,
              "disposition": { "default": 1, "attached_pic": 0 } }
        ],
        "format": { "duration": "20.000000" }
    }"#;

    #[test]
    fn reads_an_ordinary_video() {
        let properties = properties_from_probe_output(ORDINARY_VIDEO_WITH_AUDIO).unwrap();
        assert_eq!(
            properties,
            SourceVideoProperties {
                frame_rate: frame_rate(30, 1),
                total_frame_count: 600,
                display_width_in_pixels: 320,
                display_height_in_pixels: 180,
                has_audio: true,
            }
        );
    }

    #[test]
    fn a_video_without_an_audio_stream_reports_no_audio() {
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 64, "height": 48,
            "avg_frame_rate": "25/1", "nb_frames": "50" } ], "format": {} }"#;
        let properties = properties_from_probe_output(json).unwrap();
        assert!(!properties.has_audio);
        assert_eq!(properties.total_frame_count, 50);
    }

    #[test]
    fn a_video_turned_on_its_side_swaps_its_width_and_height() {
        for rotation in ["-90", "90", "270", "-270"] {
            let json = format!(
                r#"{{ "streams": [ {{ "codec_type": "video", "width": 1920, "height": 1080, "avg_frame_rate": "30/1",
                    "nb_frames": "30", "side_data_list": [ {{ "side_data_type": "Display Matrix", "rotation": {rotation} }} ] }} ] }}"#
            );
            let properties = properties_from_probe_output(&json).unwrap();
            assert_eq!(
                (properties.display_width_in_pixels, properties.display_height_in_pixels),
                (1080, 1920),
                "rotation {rotation}"
            );
        }
    }

    #[test]
    fn a_video_turned_upside_down_or_not_at_all_keeps_its_width_and_height() {
        for rotation in ["0", "180", "-180", "360"] {
            let json = format!(
                r#"{{ "streams": [ {{ "codec_type": "video", "width": 1920, "height": 1080, "avg_frame_rate": "30/1",
                    "nb_frames": "30", "side_data_list": [ {{ "rotation": {rotation} }} ] }} ] }}"#
            );
            let properties = properties_from_probe_output(&json).unwrap();
            assert_eq!(
                (properties.display_width_in_pixels, properties.display_height_in_pixels),
                (1920, 1080),
                "rotation {rotation}"
            );
        }
    }

    #[test]
    fn old_style_rotate_tags_are_understood_too() {
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 1280, "height": 720, "avg_frame_rate": "30/1",
            "nb_frames": "30", "tags": { "rotate": "90" } } ] }"#;
        let properties = properties_from_probe_output(json).unwrap();
        assert_eq!(
            (properties.display_width_in_pixels, properties.display_height_in_pixels),
            (720, 1280)
        );
    }

    #[test]
    fn non_square_pixels_widen_or_narrow_the_shown_picture() {
        // A PAL widescreen DVD frame: 720x576 stored, pixels 64:45 wide, shown 1024x576.
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 720, "height": 576,
            "sample_aspect_ratio": "64:45", "avg_frame_rate": "25/1", "nb_frames": "25" } ] }"#;
        let properties = properties_from_probe_output(json).unwrap();
        assert_eq!(
            (properties.display_width_in_pixels, properties.display_height_in_pixels),
            (1024, 576)
        );
    }

    #[test]
    fn unknown_or_nonsensical_pixel_shapes_count_as_square() {
        for shape in ["N/A", "0:1", "1:0", "abc", "", "-4:3"] {
            let json = format!(
                r#"{{ "streams": [ {{ "codec_type": "video", "width": 640, "height": 360, "sample_aspect_ratio": "{shape}",
                    "avg_frame_rate": "25/1", "nb_frames": "25" }} ] }}"#
            );
            let properties = properties_from_probe_output(&json).unwrap();
            assert_eq!(properties.display_width_in_pixels, 640, "shape {shape:?}");
        }
    }

    #[test]
    fn without_a_counted_number_of_frames_the_length_is_worked_out_from_the_duration() {
        // Matroska files usually have no frame count. 12.345 s at 29.97 fps is about 370 frames.
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "30000/1001", "duration": "12.345000" } ] }"#;
        assert_eq!(properties_from_probe_output(json).unwrap().total_frame_count, 370);
    }

    #[test]
    fn the_container_duration_is_used_when_the_stream_has_none() {
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "25/1", "nb_frames": "N/A", "duration": "N/A" } ],
            "format": { "duration": "4.000000" } }"#;
        assert_eq!(properties_from_probe_output(json).unwrap().total_frame_count, 100);
    }

    #[test]
    fn a_video_with_neither_frame_count_nor_duration_is_an_error() {
        let json =
            r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "25/1" } ] }"#;
        assert!(matches!(
            properties_from_probe_output(json),
            Err(ProbeError::UnknownLength)
        ));
    }

    #[test]
    fn the_real_frame_rate_is_used_when_the_average_is_unusable() {
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "0/0", "r_frame_rate": "24000/1001", "nb_frames": "100" } ] }"#;
        assert_eq!(
            properties_from_probe_output(json).unwrap().frame_rate,
            frame_rate(24000, 1001)
        );
    }

    #[test]
    fn a_video_with_no_usable_frame_rate_is_an_error() {
        let json = r#"{ "streams": [ { "codec_type": "video", "width": 640, "height": 360,
            "avg_frame_rate": "0/0", "r_frame_rate": "0/0", "nb_frames": "100" } ] }"#;
        assert!(matches!(
            properties_from_probe_output(json),
            Err(ProbeError::UnknownFrameRate)
        ));
    }

    #[test]
    fn an_audio_only_file_has_no_video_stream() {
        let json = r#"{ "streams": [ { "codec_type": "audio", "sample_rate": "44100" } ] }"#;
        assert!(matches!(
            properties_from_probe_output(json),
            Err(ProbeError::NoVideoStream)
        ));
    }

    #[test]
    fn album_art_attached_to_an_audio_file_is_not_a_video() {
        let json = r#"{ "streams": [
            { "codec_type": "audio", "sample_rate": "44100" },
            { "codec_type": "video", "width": 500, "height": 500, "avg_frame_rate": "90000/1",
              "nb_frames": "1", "disposition": { "attached_pic": 1 } } ] }"#;
        assert!(matches!(
            properties_from_probe_output(json),
            Err(ProbeError::NoVideoStream)
        ));
    }

    #[test]
    fn the_first_real_video_stream_is_used_when_there_are_several() {
        let json = r#"{ "streams": [
            { "codec_type": "video", "width": 100, "height": 100, "avg_frame_rate": "90000/1",
              "nb_frames": "1", "disposition": { "attached_pic": 1 } },
            { "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "25/1", "nb_frames": "25" },
            { "codec_type": "video", "width": 320, "height": 180, "avg_frame_rate": "30/1", "nb_frames": "30" } ] }"#;
        let properties = properties_from_probe_output(json).unwrap();
        assert_eq!(properties.display_width_in_pixels, 640);
    }

    #[test]
    fn a_video_stream_without_a_size_is_not_usable() {
        let json = r#"{ "streams": [ { "codec_type": "video", "avg_frame_rate": "25/1", "nb_frames": "25" } ] }"#;
        assert!(matches!(
            properties_from_probe_output(json),
            Err(ProbeError::NoVideoStream)
        ));
    }

    #[test]
    fn text_that_is_not_probe_output_is_reported_as_unreadable() {
        for bad_text in ["", "not json", "[1, 2]"] {
            assert!(
                matches!(
                    properties_from_probe_output(bad_text),
                    Err(ProbeError::UnreadableAnswer(_))
                ),
                "{bad_text:?}"
            );
        }
        // Valid JSON without any streams is simply a file with no video.
        assert!(matches!(
            properties_from_probe_output("{}"),
            Err(ProbeError::NoVideoStream)
        ));
    }

    // --- pictures ---

    /// What ffprobe prints (trimmed) for a PNG file of 800 by 600 pixels.
    const PNG_FILE: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "video", "codec_name": "png", "width": 800, "height": 600,
              "sample_aspect_ratio": "1:1", "r_frame_rate": "25/1", "avg_frame_rate": "0/0",
              "disposition": { "default": 0, "attached_pic": 0 } }
        ],
        "format": { "format_name": "png_pipe", "duration": "N/A" }
    }"#;

    #[test]
    fn a_png_file_is_a_picture_of_the_size_it_says() {
        assert_eq!(
            source_properties(PNG_FILE).unwrap(),
            SourceMediaProperties::Image(SourceImageProperties {
                display_width_in_pixels: 800,
                display_height_in_pixels: 600
            })
        );
    }

    #[test]
    fn a_jpeg_file_which_ffprobe_knows_by_its_extension_is_a_picture_too() {
        let json = PNG_FILE.replace("png_pipe", "image2").replace("\"png\"", "\"mjpeg\"");
        assert!(matches!(
            source_properties(&json).unwrap(),
            SourceMediaProperties::Image(_)
        ));
        for kind in ["jpeg_pipe", "webp_pipe", "bmp_pipe", "tiff_pipe", "image2pipe"] {
            let json = PNG_FILE.replace("png_pipe", kind);
            assert!(
                matches!(source_properties(&json).unwrap(), SourceMediaProperties::Image(_)),
                "{kind}"
            );
        }
    }

    #[test]
    fn a_video_of_a_single_picture_or_an_animated_gif_is_a_video_and_not_a_picture() {
        let one_frame_video = r#"{ "streams": [
            { "index": 0, "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "30/1", "nb_frames": "1" } ],
            "format": { "format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "0.033" } }"#;
        assert!(matches!(
            source_properties(one_frame_video).unwrap(),
            SourceMediaProperties::Video(_)
        ));
        let gif = r#"{ "streams": [
            { "index": 0, "codec_type": "video", "width": 100, "height": 100, "avg_frame_rate": "10/1", "nb_frames": "20" } ],
            "format": { "format_name": "gif", "duration": "2.0" } }"#;
        assert!(matches!(
            source_properties(gif).unwrap(),
            SourceMediaProperties::Video(_)
        ));
    }

    #[test]
    fn the_size_of_a_picture_is_the_size_it_is_shown_at_after_its_pixel_shape_and_turning() {
        let non_square = PNG_FILE.replace("\"1:1\"", "\"2:1\"");
        let SourceMediaProperties::Image(image) = source_properties(&non_square).unwrap() else {
            panic!("a picture")
        };
        assert_eq!(
            (image.display_width_in_pixels, image.display_height_in_pixels),
            (1600, 600)
        );
        let turned = PNG_FILE.replace(
            "\"disposition\"",
            "\"side_data_list\": [ { \"rotation\": -90 } ], \"disposition\"",
        );
        let SourceMediaProperties::Image(image) = source_properties(&turned).unwrap() else {
            panic!("a picture")
        };
        assert_eq!(
            (image.display_width_in_pixels, image.display_height_in_pixels),
            (600, 800)
        );
    }

    #[test]
    fn a_picture_file_with_no_size_or_no_stream_is_an_error() {
        let no_size = r#"{ "streams": [ { "codec_type": "video" } ], "format": { "format_name": "png_pipe" } }"#;
        assert!(matches!(source_properties(no_size), Err(ProbeError::NoVideoStream)));
        let no_stream = r#"{ "streams": [], "format": { "format_name": "image2" } }"#;
        assert!(matches!(source_properties(no_stream), Err(ProbeError::NoVideoStream)));
    }

    // --- sound files ---

    fn source_properties(probe_output_json: &str) -> Result<SourceMediaProperties, ProbeError> {
        source_properties_from_probe_output(probe_output_json, |_| Err(ProbeError::UnknownLength))
    }

    /// What ffprobe prints (trimmed) for a 12.5 second mono WAV file.
    const SOUND_FILE: &str = r#"{
        "streams": [
            { "index": 0, "codec_type": "audio", "sample_rate": "48000", "channels": 1, "duration": "12.500000" }
        ],
        "format": { "duration": "12.500000" }
    }"#;

    #[test]
    fn a_file_with_only_sound_is_a_sound_file_that_lasts_as_long_as_it_says() {
        assert_eq!(
            source_properties(SOUND_FILE).unwrap(),
            SourceMediaProperties::Audio(SourceAudioProperties {
                duration_in_microseconds: 12_500_000
            })
        );
    }

    #[test]
    fn a_sound_file_that_does_not_state_the_length_of_its_sound_has_the_length_of_the_file() {
        let json = r#"{ "streams": [ { "codec_type": "audio", "duration": "N/A" } ],
            "format": { "duration": "3.250000" } }"#;
        assert_eq!(
            source_properties(json).unwrap(),
            SourceMediaProperties::Audio(SourceAudioProperties {
                duration_in_microseconds: 3_250_000
            })
        );
    }

    #[test]
    fn a_sound_file_whose_length_is_unknown_or_nothing_is_an_error() {
        for json in [
            r#"{ "streams": [ { "codec_type": "audio" } ] }"#,
            r#"{ "streams": [ { "codec_type": "audio", "duration": "0.000000" } ], "format": { "duration": "0" } }"#,
            r#"{ "streams": [ { "codec_type": "audio", "duration": "-4" } ] }"#,
        ] {
            assert!(
                matches!(source_properties(json), Err(ProbeError::UnknownLength)),
                "{json}"
            );
        }
    }

    #[test]
    fn a_sound_file_with_album_art_is_a_sound_file_and_not_a_video() {
        let json = r#"{ "streams": [
            { "codec_type": "audio", "sample_rate": "44100", "duration": "200.0" },
            { "codec_type": "video", "width": 500, "height": 500, "avg_frame_rate": "90000/1",
              "nb_frames": "1", "disposition": { "attached_pic": 1 } } ] }"#;
        assert_eq!(
            source_properties(json).unwrap(),
            SourceMediaProperties::Audio(SourceAudioProperties {
                duration_in_microseconds: 200_000_000
            })
        );
    }

    #[test]
    fn a_file_with_a_picture_is_still_a_video_whether_or_not_it_also_has_sound() {
        let properties = source_properties(ORDINARY_VIDEO_WITH_AUDIO).unwrap();
        let expected = properties_from_probe_output(ORDINARY_VIDEO_WITH_AUDIO).unwrap();
        assert_eq!(properties, SourceMediaProperties::Video(expected));
        assert!(properties.has_sound());
    }

    #[test]
    fn a_file_with_neither_picture_nor_sound_says_so() {
        for json in [
            "{}",
            r#"{ "streams": [ { "codec_type": "subtitle" }, { "codec_type": "data" } ] }"#,
        ] {
            assert!(
                matches!(source_properties(json), Err(ProbeError::NoVideoPictureOrSound)),
                "{json}"
            );
        }
    }

    /// Regression guard: only a file with *no* video picture falls back to being a sound file. A
    /// video that cannot be understood is reported as that, not quietly turned into its sound.
    #[test]
    fn a_video_that_cannot_be_understood_is_not_taken_for_a_sound_file_because_it_has_sound() {
        let json = r#"{ "streams": [
            { "codec_type": "video", "width": 640, "height": 360, "avg_frame_rate": "0/0", "r_frame_rate": "0/0",
              "nb_frames": "100" },
            { "codec_type": "audio", "duration": "5.0" } ] }"#;
        assert!(matches!(source_properties(json), Err(ProbeError::UnknownFrameRate)));
    }

    #[test]
    fn text_that_is_not_probe_output_is_unreadable_whatever_it_is_read_as() {
        assert!(matches!(
            source_properties("not json"),
            Err(ProbeError::UnreadableAnswer(_))
        ));
    }
}
