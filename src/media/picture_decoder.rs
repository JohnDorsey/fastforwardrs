// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoding a still image file (a photograph, a logo) into the pixels the preview draws.
//!
//! ffmpeg does it, as it does for the export, so that the preview and the finished video are made
//! from the very same decoded picture, whatever kind of image file it is (PNG, JPEG, WebP, BMP...).

use std::ffi::OsString;
use std::io;

use thiserror::Error;

use super::child_process::{background_command, end_of_error_output};
use super::ffmpeg_toolchain::FfmpegToolchain;
use super::location::MediaLocation;
use crate::rgba_picture::RgbaPicture;

#[derive(Debug, Error)]
pub enum PictureDecodeError {
    #[error("ffmpeg could not be started: {0}")]
    CouldNotRunFfmpeg(#[source] io::Error),
    #[error("the picture could not be decoded: {message}")]
    DecodingFailed { message: String },
    #[error("ffmpeg gave {found} bytes of picture where a picture of {width} by {height} pixels is {expected}")]
    WrongAmountOfPicture {
        found: usize,
        expected: usize,
        width: u32,
        height: u32,
    },
}

/// How big to decode a picture that is shown `display_size` pixels (width, height) so that it is no
/// bigger than needed to draw it where it can be at most `largest` pixels: its own size if that
/// fits, and otherwise the biggest size inside `largest` that has its shape. Never empty.
pub fn size_to_decode_at(display_size: (u32, u32), largest: (u32, u32)) -> (u32, u32) {
    let (width, height) = (display_size.0.max(1), display_size.1.max(1));
    let (largest_width, largest_height) = (largest.0.max(1), largest.1.max(1));
    if width <= largest_width && height <= largest_height {
        return (width, height);
    }
    // The scale at which it just fits, as a fraction (so that nothing is rounded until the end).
    let (scaled_width, scaled_height) =
        if u64::from(width) * u64::from(largest_height) > u64::from(height) * u64::from(largest_width) {
            // Wider than `largest`: as wide as it.
            (
                largest_width,
                (u64::from(height) * u64::from(largest_width) / u64::from(width)) as u32,
            )
        } else {
            (
                (u64::from(width) * u64::from(largest_height) / u64::from(height)) as u32,
                largest_height,
            )
        };
    (scaled_width.max(1), scaled_height.max(1))
}

/// The arguments that make ffmpeg print the first picture of the file at `location`, scaled to `size`
/// (width, height) pixels, as raw RGBA pixels with straight alpha.
pub fn picture_decoding_arguments(location: impl Into<MediaLocation>, size: (u32, u32)) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-i"]
        .into_iter()
        .map(OsString::from)
        .collect();
    arguments.push(location.into().ffmpeg_input());
    arguments.extend(
        [
            "-frames:v".to_owned(),
            "1".to_owned(),
            "-vf".to_owned(),
            format!("scale={}:{},setsar=1", size.0, size.1),
            "-f".to_owned(),
            "rawvideo".to_owned(),
            "-pix_fmt".to_owned(),
            "rgba".to_owned(),
            "pipe:1".to_owned(),
        ]
        .map(OsString::from),
    );
    arguments
}

/// Decodes the picture in the file at `location` at `size` (width, height) pixels, which takes as
/// long as ffmpeg does (about a tenth of a second for a photograph).
pub fn decode_picture(
    toolchain: &FfmpegToolchain,
    location: impl Into<MediaLocation>,
    size: (u32, u32),
) -> Result<RgbaPicture, PictureDecodeError> {
    let output = background_command(toolchain.ffmpeg_path())
        .args(picture_decoding_arguments(location, size))
        .output()
        .map_err(PictureDecodeError::CouldNotRunFfmpeg)?;
    if !output.status.success() {
        return Err(PictureDecodeError::DecodingFailed {
            message: end_of_error_output(&output.stderr),
        });
    }
    let expected = size.0 as usize * size.1 as usize * 4;
    if output.stdout.len() != expected {
        return Err(PictureDecodeError::WrongAmountOfPicture {
            found: output.stdout.len(),
            expected,
            width: size.0,
            height: size.1,
        });
    }
    Ok(RgbaPicture {
        width_in_pixels: size.0,
        height_in_pixels: size.1,
        rgba_pixels: output.stdout,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_picture_that_fits_is_decoded_at_its_own_size_and_never_made_bigger() {
        assert_eq!(size_to_decode_at((100, 50), (1280, 720)), (100, 50));
        assert_eq!(size_to_decode_at((1280, 720), (1280, 720)), (1280, 720));
    }

    #[test]
    fn a_picture_that_is_too_big_is_made_the_biggest_that_fits_and_keeps_its_shape() {
        assert_eq!(
            size_to_decode_at((4000, 3000), (1280, 720)),
            (960, 720),
            "too tall for it"
        );
        assert_eq!(
            size_to_decode_at((4000, 1000), (1280, 720)),
            (1280, 320),
            "too wide for it"
        );
        assert_eq!(size_to_decode_at((2560, 1440), (1280, 720)), (1280, 720));
        assert_eq!(size_to_decode_at((720, 5000), (1280, 720)), (103, 720));
    }

    #[test]
    fn nothing_is_ever_decoded_at_no_size() {
        assert_eq!(size_to_decode_at((0, 0), (1280, 720)), (1, 1));
        assert_eq!(size_to_decode_at((10_000, 1), (100, 100)), (100, 1));
        assert_eq!(size_to_decode_at((100, 100), (0, 0)), (1, 1));
    }

    #[test]
    fn the_size_is_never_more_than_the_largest_in_either_way() {
        for width in [1u32, 7, 640, 1281, 4000, 100_000] {
            for height in [1u32, 5, 360, 721, 3000, 100_000] {
                let (decoded_width, decoded_height) = size_to_decode_at((width, height), (1280, 720));
                assert!(decoded_width >= 1 && decoded_height >= 1);
                assert!(decoded_width <= 1280 && decoded_height <= 720, "{width}x{height}");
                assert!(
                    decoded_width <= width.max(1) && decoded_height <= height.max(1),
                    "{width}x{height}"
                );
            }
        }
    }

    #[test]
    fn the_arguments_read_one_picture_of_the_file_and_give_raw_rgba_pixels_of_the_size() {
        let arguments: Vec<String> = picture_decoding_arguments(PathBuf::from("logo.png"), (640, 360))
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
                "logo.png",
                "-frames:v",
                "1",
                "-vf",
                "scale=640:360,setsar=1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ]
        );
    }
}
