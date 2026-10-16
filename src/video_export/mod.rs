// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exporting a project as a finished video file, made by the picture engine (phase 2 of `docs/plugin-system-plan-of-action.md`; the design is in
//! `docs/engine-phase-2-design.md`).
//!
//! ffmpeg only **decodes** the files the project is made from (one process for each *run* of a video clip, see [`VideoRun`]) and **encodes** the finished picture
//! (given as raw video on its standard input, with the sound made by the same filter graph as the live preview's). Between them, for each frame of the project, the
//! picture engine draws the picture from the very description that the preview is drawn from (`frame_scene`), makes the planes the encoder takes, and hands them back.
//!
//! * [`plan_export`] works out what has to happen, from a project (pure logic, using what [`crate::composition`] says the picture and the sound are);
//! * [`find_the_runs`] cuts the video of the plan into the stretches that a decode process each is run for.

mod encoder;
mod frames;
mod output_colour;
mod pictures;
mod plan;
mod runner;
mod runs;
#[cfg(test)]
mod test_projects;
#[cfg(test)]
mod test_streams;

pub use encoder::{
    Encoder, EncoderEnd, EncoderKiller, EncoderSpecification, SoundForTheEncoder, encoder_arguments,
    sound_graph_for_the_encoder,
};
pub use frames::{DescribedFrame, FrameDescriber, FrameProblem};
pub use output_colour::{OutputColour, OutputTags, output_colour_for};
pub use pictures::{FfmpegRunStreamStarter, PicturesOfTheRuns, RunPictureProblem, RunStream, RunStreamStarter};
pub use plan::{ExportPlan, ExportPlanError, ImageToExport, VideoToExport, plan_export};
pub use runner::{ExportError, ExportEvent, ExportOptions, ExportResult, VideoExport};
pub use runs::{VideoRun, find_the_runs};
