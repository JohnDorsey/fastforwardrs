// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound engine (phase 7; the design is `docs/engine-phase-7-design.md`): **a block graph** that makes the sound of a project, for the live preview and for the export alike.
//!
//! It is given what the filter graph of the sound is given today (a [`SoundGraphSpecification`](crate::media::export::SoundGraphSpecification): for each track a strip of
//! clips and silences, with how loud each clip is) and **the sound of each clip as it comes out of its file** (cut out and sped up: ffmpeg does that, see
//! [`clip_feed_filters`](crate::media::export::clip_feed_filters)), and makes the finished sound a block at a time:
//!
//! * a clip's sound is changed by the **insertions** it has (its sound effects: plugins, made by a [`PluginRunner`]; see [`Insertions`]), then made as loud as the clip says, **at every sample** (the filter graph
//!   changes the volume once for every chunk of sound it handles, which is a staircase; this is the curve it means);
//! * a clip is made **exactly as long as its place on the timeline**: its sound is cut off if it is longer, and silence follows it if it is shorter;
//! * the clips and silences of a track are put one after another, where each item begins at **the sample that its frame is at, worked out from the frame and not by
//!   adding up the lengths of the items before** (see [`sample_at_frame`]), so that a frame rate that is not a whole number of samples to a frame does not drift;
//! * the insertions of the track are applied, and the tracks are **added** (a plain sum, as the filter graph does it; nothing limits it: the speakers and the encoder do).
//!
//! Nothing here uses ffmpeg, a thread, a device or `unsafe`: it is arithmetic on buffers, so that all of it can be tested exactly. The same graph is called by the preview
//! (a thread that keeps the speakers' queue full ahead of the clock) and by the export (a loop that runs as fast as it can).
//!
//! **Sound is stereo, 32-bit floating point, [`SAMPLE_RATE`] samples a second**, in two separate buffers (left and right), which is also how plugins want it.

mod feed;
mod gain;
mod graph;
mod insertions;
mod lookahead;
mod plugins;
mod position;

pub use crate::media::export::SAMPLE_RATE;
pub use feed::{ClipFeed, ClipFeeds, ClipRequest, SoundInMemory};
pub use gain::GainCurve;
pub use graph::SoundGraph;
pub use insertions::{BlockProcessor, Insertions, NoInsertions};
pub use lookahead::{ClipSpawner, LookAheadFeeds, MOST_FEEDS_OPEN, SECONDS_OF_LOOK_AHEAD};
pub use plugins::{Notes, PluginInsertions, PluginRunner};
pub use position::{pre_roll_samples_of, sample_at_frame};

/// How many samples the preview and the export ask the graph for at a time. (The graph gives the same sound for any number: this is a size, not a rule.)
pub const BLOCK_SAMPLES: usize = 512;

#[cfg(test)]
mod tests;
