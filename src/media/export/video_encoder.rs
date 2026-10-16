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
        Self::choose_from_the_list(&list_of_the_encoders(toolchain))
    }

    /// The best encoder that `list` (what `ffmpeg -encoders` prints) has.
    fn choose_from_the_list(list: &str) -> Self {
        if lists_the_encoder(list, "libx264") {
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

/// What this ffmpeg prints for `-encoders` (nothing, if it cannot be run).
fn list_of_the_encoders(toolchain: &FfmpegToolchain) -> String {
    match background_command(toolchain.ffmpeg_path())
        .args(["-hide_banner", "-encoders"])
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

/// Whether `list` (what `ffmpeg -encoders` prints) has the encoder called `encoder_name`: in the column that has the names, and not in the words that say what each is.
fn lists_the_encoder(list: &str, encoder_name: &str) -> bool {
    // Each line looks like " V....D libx264              libx264 H.264 / AVC ...".
    list.lines()
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

    const WHAT_FFMPEG_PRINTS: &str = "Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V....D libx265              libx265 H.265 / HEVC (codec hevc)
 V....D mpeg4                MPEG-4 part 2
 A....D aac                  AAC (Advanced Audio Coding)
";

    #[test]
    fn the_encoder_that_is_chosen_is_x264_when_the_list_has_it_and_the_one_built_in_when_it_does_not() {
        assert_eq!(
            VideoEncoder::choose_from_the_list(WHAT_FFMPEG_PRINTS),
            VideoEncoder::Libx264
        );
        let without = WHAT_FFMPEG_PRINTS.replace(
            "libx264              libx264 H.264",
            "libopenh264          OpenH264 H.264",
        );
        assert_eq!(VideoEncoder::choose_from_the_list(&without), VideoEncoder::Mpeg4);
        assert_eq!(VideoEncoder::choose_from_the_list(""), VideoEncoder::Mpeg4);
    }

    #[test]
    fn an_encoder_is_listed_by_its_name_and_not_by_a_word_in_what_is_said_of_another() {
        assert!(lists_the_encoder(WHAT_FFMPEG_PRINTS, "libx264"));
        assert!(lists_the_encoder(WHAT_FFMPEG_PRINTS, "aac"));
        assert!(!lists_the_encoder(WHAT_FFMPEG_PRINTS, "libx266"));
        assert!(
            !lists_the_encoder(
                " V....D libopenh264          OpenH264 (a libx264 look-alike)\n",
                "libx264"
            ),
            "a name in the words that follow is not a name"
        );
        assert!(
            !lists_the_encoder(" V....D libx264\n", "V....D"),
            "the column of the flags is not a name"
        );
    }

    #[test]
    fn an_ffmpeg_that_cannot_be_run_has_no_encoder_and_the_one_built_in_is_the_choice() {
        let nothing =
            FfmpegToolchain::unchecked_for_tests("/nowhere/ffmpeg".into(), "/nowhere/ffprobe".into(), Some(9));
        assert_eq!(VideoEncoder::choose(&nothing), VideoEncoder::Mpeg4);
    }
}
