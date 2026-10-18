// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

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
//! * [`colour_space`] has the colour spaces as numbers: the sets of primaries, the curves between codes and light, and the matrices between them (phase 3).
//! * [`speed_indicator`] draws and positions the fast-forward symbol, for the preview and the
//!   export alike.
//! * [`playback`] runs the live preview: the clock, and getting the right picture for each frame.
//! * [`frame_scene`] says what a frame of the finished picture shows and what the engine is asked to draw for it, for the preview and the export alike.
//! * [`video_export`] makes the finished video: ffmpeg decodes and encodes, and the engine draws every frame from the very description the preview is drawn from.
//! * [`sound_engine`] makes the sound of a project, a block at a time, for the preview and the export alike (phase 7).
//! * [`engine`] makes the finished picture of the preview on the graphics card, on a thread and a device of its own, from messages
//!   (what is on a frame) and gives it back as pixels.

#![forbid(unsafe_code)]

pub mod application;
mod atomic_file;
pub mod colour_effects;
pub mod colour_space;
pub mod composition;
pub mod cube_table;
pub mod digest;
pub mod engine;
pub mod frame_scene;
pub mod media;
pub mod playback;
pub mod plugin_trust;
pub mod project;
pub mod project_file;
pub mod rgba_picture;
pub mod settings_file;
pub mod sound_engine;
pub mod speed_indicator;
pub mod text_rendering;
pub mod video_export;

#[cfg(test)]
mod test_support;
