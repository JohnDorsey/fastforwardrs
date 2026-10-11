//! The ffmpeg command line for an export.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::filter_graph::FilterGraph;
use crate::media::child_process::background_command;
use crate::media::ffmpeg_toolchain::FfmpegToolchain;

/// The files an export hands to ffmpeg.
#[derive(Clone, Debug)]
pub struct ExportFiles {
    /// The files ffmpeg reads, in the order they are its inputs 0, 1, ...: the source files, then
    /// the still pictures (the fast-forward symbols and the text).
    pub inputs: Vec<PathBuf>,
    /// The filter graph, saved in a file: for a project with many clips it is far longer than the
    /// command line allows.
    pub filter_graph_file: PathBuf,
    pub output: PathBuf,
}

/// How the finished video is compressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoEncoder {
    /// H.264 through x264: what nearly everything can play. Used whenever ffmpeg has it.
    Libx264,
    /// MPEG-4 part 2, built into every ffmpeg. A fallback for an ffmpeg without x264.
    Mpeg4,
}

impl VideoEncoder {
    /// The best encoder this ffmpeg has.
    pub fn choose(toolchain: &FfmpegToolchain) -> Self {
        if supports_encoder(toolchain, "libx264") {
            Self::Libx264
        } else {
            Self::Mpeg4
        }
    }

    fn arguments(self) -> &'static [&'static str] {
        match self {
            // Visually lossless quality, at a sensible size.
            Self::Libx264 => &["-c:v", "libx264", "-preset", "medium", "-crf", "18"],
            Self::Mpeg4 => &["-c:v", "mpeg4", "-q:v", "2"],
        }
    }
}

/// Whether this ffmpeg can encode with the encoder called `encoder_name`.
fn supports_encoder(toolchain: &FfmpegToolchain, encoder_name: &str) -> bool {
    let Ok(output) = background_command(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-encoders"])
        .output()
    else {
        return false;
    };
    // Each line looks like " V....D libx264              libx264 H.264 / AVC ...".
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(encoder_name))
}

/// The ffmpeg option that reads a filter graph from the file named after it. A graph for a project
/// with many clips is far longer than a command line allows, so it is always given in a file.
/// (ffmpeg 7 replaced `-filter_complex_script <file>` with `-/filter_complex <file>`.)
pub(crate) fn filter_graph_file_option(toolchain: &FfmpegToolchain) -> &'static str {
    if toolchain.reads_filter_graph_files_with_slash_prefix() {
        "-/filter_complex"
    } else {
        "-filter_complex_script"
    }
}

/// The complete ffmpeg command line for an export.
pub fn export_arguments(
    toolchain: &FfmpegToolchain,
    files: &ExportFiles,
    graph: &FilterGraph,
    encoder: VideoEncoder,
) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]
        .map(OsString::from)
        .to_vec();
    for input in &files.inputs {
        arguments.push("-i".into());
        arguments.push(input.clone().into());
    }

    arguments.push(filter_graph_file_option(toolchain).into());
    arguments.push(files.filter_graph_file.clone().into());

    arguments.push("-map".into());
    arguments.push(format!("[{}]", graph.video_output_label).into());
    if let Some(audio_label) = graph.audio_output_label {
        arguments.push("-map".into());
        arguments.push(format!("[{audio_label}]").into());
    }

    arguments.extend(encoder.arguments().iter().map(OsString::from));
    arguments.extend(["-pix_fmt", "yuv420p"].map(OsString::from));
    if graph.audio_output_label.is_some() {
        arguments.extend(["-c:a", "aac", "-b:a", "192k"].map(OsString::from));
    }
    if prefers_index_at_start(&files.output) {
        arguments.extend(["-movflags", "+faststart"].map(OsString::from));
    }
    // Progress goes to standard output as simple "key=value" lines.
    arguments.extend(["-progress", "pipe:1", "-nostats"].map(OsString::from));
    arguments.push(files.output.clone().into());
    arguments
}

/// MP4 and MOV files can have their index put at the start, so that they can be played while
/// they are still downloading.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn toolchain(major_version: Option<u32>) -> FfmpegToolchain {
        FfmpegToolchain::unchecked_for_tests(PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"), major_version)
    }

    /// Files for an export that reads `holiday.mp4` and then `picture_count` still pictures.
    fn files(picture_count: usize, output: &str) -> ExportFiles {
        ExportFiles {
            inputs: std::iter::once(PathBuf::from("holiday.mp4"))
                .chain((0..picture_count).map(|number| PathBuf::from(format!("picture_{number}.png"))))
                .collect(),
            filter_graph_file: PathBuf::from("graph.txt"),
            output: PathBuf::from(output),
        }
    }

    fn graph(with_audio: bool) -> FilterGraph {
        FilterGraph {
            text: String::new(),
            video_output_label: "finished_video",
            audio_output_label: with_audio.then_some("finished_audio"),
        }
    }

    fn as_text(arguments: Vec<OsString>) -> Vec<String> {
        arguments
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_complete_export_command_with_sound_and_still_pictures() {
        let arguments = as_text(export_arguments(
            &toolchain(Some(9)),
            &files(2, "out.mp4"),
            &graph(true),
            VideoEncoder::Libx264,
        ));
        assert_eq!(
            arguments,
            [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-i",
                "holiday.mp4",
                "-i",
                "picture_0.png",
                "-i",
                "picture_1.png",
                "-/filter_complex",
                "graph.txt",
                "-map",
                "[finished_video]",
                "-map",
                "[finished_audio]",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
                "-progress",
                "pipe:1",
                "-nostats",
                "out.mp4",
            ]
        );
    }

    #[test]
    fn every_input_is_read_with_its_own_dash_i_in_the_order_given() {
        let files = ExportFiles {
            inputs: ["a.mp4", "b.mp3", "c.png"].map(PathBuf::from).to_vec(),
            filter_graph_file: PathBuf::from("graph.txt"),
            output: PathBuf::from("out.mp4"),
        };
        let arguments = as_text(export_arguments(
            &toolchain(Some(9)),
            &files,
            &graph(true),
            VideoEncoder::Libx264,
        ));
        let inputs: Vec<&String> = arguments
            .windows(2)
            .filter(|pair| pair[0] == "-i")
            .map(|pair| &pair[1])
            .collect();
        assert_eq!(inputs, ["a.mp4", "b.mp3", "c.png"]);
    }

    #[test]
    fn without_sound_no_audio_is_mapped_or_encoded() {
        let arguments = as_text(export_arguments(
            &toolchain(Some(9)),
            &files(0, "out.mp4"),
            &graph(false),
            VideoEncoder::Libx264,
        ));
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.contains("audio") || argument == "aac"),
            "{arguments:?}"
        );
        assert_eq!(arguments.iter().filter(|argument| *argument == "-map").count(), 1);
    }

    #[test]
    fn the_filter_graph_file_option_follows_the_ffmpeg_version() {
        for (major_version, expected) in [
            (Some(4), "-filter_complex_script"),
            (Some(6), "-filter_complex_script"),
            (Some(7), "-/filter_complex"),
            (Some(9), "-/filter_complex"),
            (None, "-/filter_complex"),
        ] {
            let arguments = as_text(export_arguments(
                &toolchain(major_version),
                &files(0, "out.mp4"),
                &graph(false),
                VideoEncoder::Libx264,
            ));
            let position = arguments
                .iter()
                .position(|argument| argument.contains("filter_complex"))
                .unwrap();
            assert_eq!(arguments[position], expected, "ffmpeg {major_version:?}");
            assert_eq!(arguments[position + 1], "graph.txt");
        }
    }

    #[test]
    fn the_fallback_encoder_is_used_when_asked_for() {
        let arguments = as_text(export_arguments(
            &toolchain(Some(9)),
            &files(0, "out.mp4"),
            &graph(false),
            VideoEncoder::Mpeg4,
        ));
        assert!(
            arguments.windows(2).any(|pair| pair == ["-c:v", "mpeg4"]),
            "{arguments:?}"
        );
        assert!(!arguments.contains(&"libx264".to_owned()));
    }

    #[test]
    fn the_fast_start_option_is_only_for_mp4_like_files() {
        for (output, expected) in [
            ("a.mp4", true),
            ("a.MP4", true),
            ("a.mov", true),
            ("a.m4v", true),
            ("a.mkv", false),
            ("a.webm", false),
            ("a", false),
        ] {
            let arguments = as_text(export_arguments(
                &toolchain(Some(9)),
                &files(0, output),
                &graph(false),
                VideoEncoder::Libx264,
            ));
            assert_eq!(arguments.contains(&"+faststart".to_owned()), expected, "{output}");
        }
    }

    #[test]
    fn the_output_file_is_the_last_argument() {
        let arguments = as_text(export_arguments(
            &toolchain(Some(9)),
            &files(1, "folder/out.mkv"),
            &graph(true),
            VideoEncoder::Libx264,
        ));
        assert_eq!(arguments.last().unwrap(), "folder/out.mkv");
    }
}
