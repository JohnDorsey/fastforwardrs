// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything that talks to the ffmpeg tools: finding them, asking a video what it contains,
//! decoding frames for the live preview, making the timeline thumbnails and exporting.
//!
//! ffmpeg and ffprobe are run as separate programs rather than linked in as libraries. That keeps
//! building the editor simple on every operating system, and lets users update ffmpeg on their
//! own. All of the code that depends on that choice lives in this module, so it could be replaced
//! by a different video backend without touching the rest of the editor.

mod audio_stream;
mod child_process;
pub mod export;
mod ffmpeg_toolchain;
mod finer_thumbnails;
mod frame_stream;
mod location;
mod picture_decoder;
mod source_colour;
mod source_probe;
mod thumbnail_library;
mod thumbnails;
mod waveform;

use std::sync::Arc;

pub use audio_stream::{
    AudioFormat, AudioStream, AudioStreamError, AudioStreamEvent, AudioStreamRequest, audio_stream_arguments,
};
pub(crate) use child_process::{background_command, collect_error_output_in_background};
pub use ffmpeg_toolchain::{FFMPEG_DIRECTORY_ENVIRONMENT_VARIABLE, FfmpegToolchain, FfmpegVersion, ToolchainError};
pub use finer_thumbnails::FinerThumbnails;
pub use frame_stream::{
    DecodedFrame, DecodedYuvFrame, FrameDelivery, FrameStream, FrameStreamError, FrameStreamEvent, FrameStreamRequest,
    WaitedFor, frame_stream_arguments, frame_stream_arguments_delivering,
};
pub use location::{MediaLocation, Stretch};
pub use picture_decoder::{PictureDecodeError, decode_picture, picture_decoding_arguments, size_to_decode_at};
pub use source_colour::{Assumption, VideoColour, matrix_assumed_for, matrix_name};
pub use source_probe::{
    NativeYuv420p, ProbeError, VideoStreamFormat, probe_source_media, probe_video, probe_video_stream_format,
    properties_from_probe_output, source_properties_from_probe_output, video_stream_format_from_probe_output,
};
pub use thumbnail_library::{ThumbnailLibrary, plan_finer_thumbnails};
pub use thumbnails::{
    CompressedPicture, MOST_FINER_THUMBNAILS_AT_ONCE, PictureSize, ThumbnailError, ThumbnailEvent, ThumbnailExtraction,
    ThumbnailKey, ThumbnailRange, ThumbnailStrip, finest_thumbnail_level, thumbnail_arguments,
    thumbnail_level_for_spacing, thumbnail_range_arguments, thumbnail_second_for_source_frame,
};
pub use waveform::{PEAKS_PER_SECOND, Waveform, WaveformError, WaveformEvent, WaveformExtraction, waveform_arguments};

/// A function that background work calls whenever it has something new for the user interface,
/// so that the interface can wake up and show it. (The interface passes in a function that asks
/// its window to redraw; this module knows nothing about the interface itself.)
pub type UserInterfaceWakeUp = Arc<dyn Fn() + Send + Sync>;

/// A wake-up callback for unit tests that do not care about being woken.
#[cfg(test)]
pub(crate) fn do_nothing_when_woken() -> UserInterfaceWakeUp {
    Arc::new(|| {})
}
