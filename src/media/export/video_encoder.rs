// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the finished video is compressed.

use crate::media::child_process::background_command;
use crate::media::ffmpeg_toolchain::FfmpegToolchain;

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

    /// The options that make ffmpeg use this encoder, and at what quality.
    pub(crate) fn arguments(self) -> &'static [&'static str] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoders_are_asked_for_by_name_and_with_a_quality() {
        assert_eq!(
            VideoEncoder::Libx264.arguments(),
            ["-c:v", "libx264", "-preset", "medium", "-crf", "18"]
        );
        assert_eq!(VideoEncoder::Mpeg4.arguments(), ["-c:v", "mpeg4", "-q:v", "2"]);
    }

    #[test]
    fn an_ffmpeg_that_cannot_be_run_has_no_encoder_and_the_one_built_in_is_the_choice() {
        let nothing =
            FfmpegToolchain::unchecked_for_tests("/nowhere/ffmpeg".into(), "/nowhere/ffprobe".into(), Some(9));
        assert_eq!(VideoEncoder::choose(&nothing), VideoEncoder::Mpeg4);
    }
}
