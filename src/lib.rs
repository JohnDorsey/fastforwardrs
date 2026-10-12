//! fastforwardrs: a small video editor that does one thing well. It lets you fast-forward chosen
//! parts of a video.
//!
//! The crate is organised in layers that only depend "downwards", so each layer can be understood
//! (and tested) without the layers above it:
//!
//! * [`project`] describes what a project *is*: the source video, the clips the video track has
//!   been split into, the speed of each clip, and all of the timeline arithmetic. It is pure
//!   logic with no file, process or user-interface code in it.
//! * [`project_file`] saves and loads a project as pretty, human-readable JSON.
//! * [`settings_file`] does the same for the global settings, in the operating system's folder
//!   for programs' settings.
//! * [`media`] runs the ffmpeg tools: probing videos, decoding preview frames, making timeline
//!   thumbnails and exporting.
//! * [`colour_effects`] is the arithmetic of the effects that change the colours of a picture, and
//!   what asks ffmpeg to do the same, for the preview and the export alike.
//! * [`speed_indicator`] draws and positions the fast-forward symbol, for the preview and the
//!   export alike.
//! * [`playback`] runs the live preview: the clock, and getting the right picture for each frame.

#![forbid(unsafe_code)]

pub mod application;
mod atomic_file;
pub mod colour_effects;
pub mod composition;
pub mod media;
pub mod playback;
pub mod project;
pub mod project_file;
pub mod rgba_picture;
pub mod settings_file;
pub mod speed_indicator;
pub mod text_rendering;

#[cfg(test)]
mod test_support;
